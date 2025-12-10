//! Configuration loading with 3-tier priority
//!
//! Priority: CLI flags > in-file directives > config file
//! See SPEC §5 for details

use std::fs;

use crate::parser::ParseConfig;
use crate::style::CompositeStyle;

/// Application configuration
#[derive(Debug, Clone)]
pub struct Config {
    pub max_label_width: usize,
    pub strict_parsing: bool,
    pub composite_style: CompositeStyle,
    pub enable_subgraphs: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_label_width: 20,
            strict_parsing: false,
            composite_style: CompositeStyle::default(),
            enable_subgraphs: true, // Subgraphs enabled by default
        }
    }
}

impl Config {
    /// Create a new config builder
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::new()
    }

    /// Load configuration from file config + in-file directives
    /// Used by the library API
    pub fn from_parse_config(parse_config: &ParseConfig) -> Self {
        let mut config = Self::default();

        // Config file (lowest priority)
        if let Some(file_cfg) = load_file_config() {
            if let Some(max_label) = file_cfg.max_label_width {
                config.max_label_width = max_label;
            }
            config.composite_style = file_cfg.composite_style;
        }

        // In-file directives (medium priority)
        if let Some(max_label) = parse_config.max_label {
            config.max_label_width = max_label;
        }
        if let Some(style_str) = parse_config.style.as_ref() {
            config.composite_style = CompositeStyle::parse(style_str);
        }

        config
    }
}

/// Builder for Config - allows CLI to override settings
#[derive(Debug, Clone, Default)]
pub struct ConfigBuilder {
    max_label_width: Option<usize>,
    strict_parsing: Option<bool>,
    composite_style: Option<CompositeStyle>,
    enable_subgraphs: Option<bool>,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn max_label_width(mut self, width: usize) -> Self {
        self.max_label_width = Some(width);
        self
    }

    pub fn strict(mut self, strict: bool) -> Self {
        self.strict_parsing = Some(strict);
        self
    }

    pub fn style(mut self, style: CompositeStyle) -> Self {
        self.composite_style = Some(style);
        self
    }

    pub fn enable_subgraphs(mut self, enable: bool) -> Self {
        self.enable_subgraphs = Some(enable);
        self
    }

    /// Build config, applying CLI overrides to parse_config base
    pub fn build(self, parse_config: &ParseConfig) -> Config {
        let mut config = Config::from_parse_config(parse_config);

        // CLI overrides (highest priority)
        if let Some(width) = self.max_label_width {
            config.max_label_width = width;
        }
        if let Some(strict) = self.strict_parsing {
            config.strict_parsing = strict;
        }
        if let Some(style) = self.composite_style {
            config.composite_style = style;
        }
        if let Some(enable) = self.enable_subgraphs {
            config.enable_subgraphs = enable;
        }

        config
    }
}

/// Load configuration from ~/.config/termiflow/config.toml
fn load_file_config() -> Option<FileConfig> {
    let mut path = dirs::config_dir()?;
    path.push("termiflow");
    path.push("config.toml");

    let contents = fs::read_to_string(&path).ok()?;
    match toml::from_str::<toml::Value>(&contents) {
        Ok(value) => {
            let style_str = value.get("style").and_then(|v| v.as_str());

            // Parse composite style
            let composite_style = if let Some(s) = style_str {
                CompositeStyle::parse(s)
            } else {
                CompositeStyle::default()
            };

            let max_label_width = value.get("max_label_width").and_then(|v| v.as_integer());
            Some(FileConfig {
                max_label_width: max_label_width.map(|n| n as usize),
                composite_style,
            })
        }
        Err(e) => {
            eprintln!("termiflow: warning: {}: {}", path.display(), e);
            None
        }
    }
}

#[derive(Debug)]
struct FileConfig {
    max_label_width: Option<usize>,
    composite_style: CompositeStyle,
}
