//! Configuration loading with 3-tier priority
//!
//! Priority: CLI flags > in-file directives > config file
//! See SPEC §5 for details

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
    /// Load configuration with priority: CLI > directives > config file
    pub fn load(cli: &Cli, _input: &str) -> Self {
        // TODO: Implement full config loading (Day 1, Step 2)
        // For now, just use CLI values
        Self {
            max_label_width: cli.max_label,
            strict_parsing: cli.strict,
        }
    }
}
