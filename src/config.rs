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
    /// Enable multiline label wrapping (experimental; default off).
    pub wrap_labels: bool,
    /// Maximum number of label lines when wrapping is enabled.
    pub max_label_lines: usize,
    pub strict_parsing: bool,
    pub composite_style: CompositeStyle,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_label_width: 20,
            wrap_labels: false,
            max_label_lines: 1,
            strict_parsing: false,
            composite_style: CompositeStyle::default(),
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
            if let Some(wrap_labels) = file_cfg.wrap_labels {
                config.wrap_labels = wrap_labels;
            }
            if let Some(max_label_lines) = file_cfg.max_label_lines {
                config.max_label_lines = max_label_lines;
            }
            config.composite_style = file_cfg.composite_style;
        }

        // In-file directives (medium priority)
        if let Some(max_label) = parse_config.max_label {
            config.max_label_width = max_label;
        }
        if let Some(wrap_labels) = parse_config.wrap_labels {
            config.wrap_labels = wrap_labels;
        }
        if let Some(max_label_lines) = parse_config.max_label_lines {
            config.max_label_lines = max_label_lines;
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
    wrap_labels: Option<bool>,
    max_label_lines: Option<usize>,
    strict_parsing: Option<bool>,
    composite_style: Option<CompositeStyle>,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn max_label_width(mut self, width: usize) -> Self {
        self.max_label_width = Some(width);
        self
    }

    pub fn wrap_labels(mut self, wrap: bool) -> Self {
        self.wrap_labels = Some(wrap);
        self
    }

    pub fn max_label_lines(mut self, lines: usize) -> Self {
        self.max_label_lines = Some(lines);
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

    /// Build config, applying CLI overrides to parse_config base
    pub fn build(self, parse_config: &ParseConfig) -> Config {
        let mut config = Config::from_parse_config(parse_config);

        // CLI overrides (highest priority)
        if let Some(width) = self.max_label_width {
            config.max_label_width = width;
        }
        if let Some(wrap) = self.wrap_labels {
            config.wrap_labels = wrap;
        }
        if let Some(lines) = self.max_label_lines {
            config.max_label_lines = lines;
        }
        if let Some(strict) = self.strict_parsing {
            config.strict_parsing = strict;
        }
        if let Some(style) = self.composite_style {
            config.composite_style = style;
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
            let wrap_labels = value
                .get("wrap")
                .and_then(|v| v.as_bool())
                .or_else(|| value.get("wrap_labels").and_then(|v| v.as_bool()));
            let max_label_lines = value
                .get("max_label_lines")
                .and_then(|v| v.as_integer())
                .or_else(|| value.get("max_lines").and_then(|v| v.as_integer()));
            Some(FileConfig {
                max_label_width: max_label_width.map(|n| n as usize),
                wrap_labels,
                max_label_lines: max_label_lines.map(|n| n as usize),
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
    wrap_labels: Option<bool>,
    max_label_lines: Option<usize>,
    composite_style: CompositeStyle,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_config_applies_wrap_and_max_lines() {
        let mut pc = ParseConfig::default();
        pc.wrap_labels = Some(true);
        pc.max_label_lines = Some(3);

        let cfg = Config::from_parse_config(&pc);
        assert!(cfg.wrap_labels);
        assert_eq!(cfg.max_label_lines, 3);
    }
}
