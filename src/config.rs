//! Configuration loading with 3-tier priority
//!
//! Priority: CLI flags > in-file directives > config file
//! See SPEC §5 for details

use std::fs;

use crate::parser::ParseConfig;
use crate::style::{BorderStyle, CompositeStyle};
use crate::Cli;

/// Application configuration
#[derive(Debug, Clone)]
pub struct Config {
    pub max_label_width: usize,
    pub strict_parsing: bool,
    pub composite_style: CompositeStyle,  // Component-specific styles
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_label_width: 20,
            strict_parsing: false,
            composite_style: CompositeStyle::default(),
        }
    }
}

impl Config {
    /// Load configuration with priority: CLI > in-file directives > config file
    pub fn load(cli: &Cli, parse_config: &ParseConfig) -> Self {
        // Start with defaults
        let mut config = Self::default();

        // Config file (lowest priority)
        if let Some(file_cfg) = load_file_config() {
            if let Some(max_label) = file_cfg.max_label_width {
                config.max_label_width = max_label;
            }
            // Apply composite styles from config file
            config.composite_style = file_cfg.composite_style;
        }

        // In-file directives (medium priority)
        if let Some(max_label) = parse_config.max_label {
            config.max_label_width = max_label;
        }
        if let Some(style_str) = parse_config.style.as_ref() {
            // Parse as composite style
            config.composite_style = CompositeStyle::parse(style_str);
        }

        // CLI flags (highest priority) - always override
        config.max_label_width = cli.max_label;
        config.strict_parsing = cli.strict;
        // Note: CLI style is handled separately in main.rs when explicitly provided

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
            eprintln!(
                "termiflow: warning: {}: {}",
                path.display(),
                e.to_string()
            );
            None
        }
    }
}

#[derive(Debug)]
struct FileConfig {
    max_label_width: Option<usize>,
    composite_style: CompositeStyle,
}
