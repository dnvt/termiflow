//! Configuration loading with 3-tier priority
//!
//! Priority: CLI flags > in-file directives > config file
//! See SPEC §5 for details

use crate::parser::ParseConfig;
use crate::Cli;

/// Application configuration
#[derive(Debug, Clone)]
pub struct Config {
    pub max_label_width: usize,
    pub strict_parsing: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_label_width: 20,
            strict_parsing: false,
        }
    }
}

impl Config {
    /// Load configuration with priority: CLI > in-file directives > config file
    pub fn load(cli: &Cli, parse_config: &ParseConfig) -> Self {
        // Start with defaults (could later load from ~/.config/termiflow/config.toml)
        let mut config = Self::default();

        // Apply in-file directives (middle priority)
        if let Some(max_label) = parse_config.max_label {
            config.max_label_width = max_label;
        }

        // Apply CLI flags (highest priority) - always overrides
        config.max_label_width = cli.max_label;
        config.strict_parsing = cli.strict;

        config
    }
}
