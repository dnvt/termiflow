//! Configuration loading with 3-tier priority
//!
//! Priority: CLI flags > in-file directives > config file
//! See SPEC §5 for details

use std::fs;

use crate::parser::ParseConfig;
use crate::spacing::{SpacingConfig, SpacingMode};
use crate::style::CompositeStyle;

/// Application configuration
#[derive(Debug, Clone)]
pub struct Config {
    pub max_label_width: usize,
    /// Maximum edge label width before truncation.
    pub max_edge_label_width: usize,
    /// Enable multiline label wrapping (experimental; default off).
    pub wrap_labels: bool,
    /// Maximum number of label lines when wrapping is enabled.
    pub max_label_lines: usize,
    /// Crop empty margins around the rendered canvas.
    pub crop: bool,
    /// Add padding (in spaces/lines) around output.
    pub pad: usize,
    pub strict_parsing: bool,
    pub composite_style: CompositeStyle,
    pub spacing: SpacingConfig,
    /// Enable the render feedback repair loop.
    pub optimize_render: bool,
    /// Maximum number of local repair passes per render.
    pub render_repair_passes: usize,
    /// Maximum number of layout candidate repair passes per render.
    pub layout_repair_passes: usize,
    /// Emit critic findings for the rendered frame.
    pub debug_critic: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_label_width: 20,
            max_edge_label_width: 20,
            wrap_labels: false,
            max_label_lines: 1,
            crop: true,
            pad: 0,
            strict_parsing: false,
            composite_style: CompositeStyle::default(),
            spacing: SpacingConfig::default_config(),
            optimize_render: false,
            render_repair_passes: 2,
            layout_repair_passes: 2,
            debug_critic: false,
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
            if let Some(max_edge_label) = file_cfg.max_edge_label_width {
                config.max_edge_label_width = max_edge_label;
            }
            if let Some(wrap_labels) = file_cfg.wrap_labels {
                config.wrap_labels = wrap_labels;
            }
            if let Some(max_label_lines) = file_cfg.max_label_lines {
                config.max_label_lines = max_label_lines;
            }
            if let Some(crop) = file_cfg.crop {
                config.crop = crop;
            }
            if let Some(pad) = file_cfg.pad {
                config.pad = pad;
            }
            if let Some(mode) = file_cfg.spacing_mode {
                config.spacing = SpacingConfig::from_mode(mode);
            }
            if let Some(optimize_render) = file_cfg.optimize_render {
                config.optimize_render = optimize_render;
            }
            if let Some(render_repair_passes) = file_cfg.render_repair_passes {
                config.render_repair_passes = render_repair_passes;
            }
            if let Some(layout_repair_passes) = file_cfg.layout_repair_passes {
                config.layout_repair_passes = layout_repair_passes;
            }
            if let Some(debug_critic) = file_cfg.debug_critic {
                config.debug_critic = debug_critic;
            }
            config.composite_style = file_cfg.composite_style;
        }

        // In-file directives (medium priority)
        if let Some(max_label) = parse_config.max_label {
            config.max_label_width = max_label;
        }
        if let Some(max_edge_label) = parse_config.max_edge_label {
            config.max_edge_label_width = max_edge_label;
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
        if let Some(mode) = parse_config.spacing_mode {
            config.spacing = SpacingConfig::from_mode(mode);
        }
        if let Some(optimize_render) = parse_config.optimize_render {
            config.optimize_render = optimize_render;
        }
        if let Some(render_repair_passes) = parse_config.render_repair_passes {
            config.render_repair_passes = render_repair_passes;
        }
        if let Some(layout_repair_passes) = parse_config.layout_repair_passes {
            config.layout_repair_passes = layout_repair_passes;
        }
        if let Some(debug_critic) = parse_config.debug_critic {
            config.debug_critic = debug_critic;
        }

        config.spacing.max_label_width = config.max_label_width;
        config
    }
}

/// Builder for Config - allows CLI to override settings
#[derive(Debug, Clone, Default)]
pub struct ConfigBuilder {
    max_label_width: Option<usize>,
    max_edge_label_width: Option<usize>,
    wrap_labels: Option<bool>,
    max_label_lines: Option<usize>,
    crop: Option<bool>,
    pad: Option<usize>,
    strict_parsing: Option<bool>,
    composite_style: Option<CompositeStyle>,
    spacing: Option<SpacingConfig>,
    optimize_render: Option<bool>,
    render_repair_passes: Option<usize>,
    layout_repair_passes: Option<usize>,
    debug_critic: Option<bool>,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn max_label_width(mut self, width: usize) -> Self {
        self.max_label_width = Some(width);
        self
    }

    pub fn max_edge_label_width(mut self, width: usize) -> Self {
        self.max_edge_label_width = Some(width);
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

    pub fn crop(mut self, crop: bool) -> Self {
        self.crop = Some(crop);
        self
    }

    pub fn pad(mut self, pad: usize) -> Self {
        self.pad = Some(pad);
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

    pub fn spacing(mut self, spacing: SpacingConfig) -> Self {
        self.spacing = Some(spacing);
        self
    }

    pub fn optimize_render(mut self, optimize_render: bool) -> Self {
        self.optimize_render = Some(optimize_render);
        self
    }

    pub fn render_repair_passes(mut self, render_repair_passes: usize) -> Self {
        self.render_repair_passes = Some(render_repair_passes.max(1));
        self
    }

    pub fn layout_repair_passes(mut self, layout_repair_passes: usize) -> Self {
        self.layout_repair_passes = Some(layout_repair_passes.max(1));
        self
    }

    pub fn debug_critic(mut self, debug_critic: bool) -> Self {
        self.debug_critic = Some(debug_critic);
        self
    }

    /// Build config, applying CLI overrides to parse_config base
    pub fn build(self, parse_config: &ParseConfig) -> Config {
        let mut config = Config::from_parse_config(parse_config);

        // CLI overrides (highest priority)
        if let Some(width) = self.max_label_width {
            config.max_label_width = width;
        }
        if let Some(width) = self.max_edge_label_width {
            config.max_edge_label_width = width;
        }
        if let Some(wrap) = self.wrap_labels {
            config.wrap_labels = wrap;
        }
        if let Some(lines) = self.max_label_lines {
            config.max_label_lines = lines;
        }
        if let Some(crop) = self.crop {
            config.crop = crop;
        }
        if let Some(pad) = self.pad {
            config.pad = pad;
        }
        if let Some(strict) = self.strict_parsing {
            config.strict_parsing = strict;
        }
        if let Some(style) = self.composite_style {
            config.composite_style = style;
        }
        if let Some(spacing) = self.spacing {
            config.spacing = spacing;
        }
        if let Some(optimize_render) = self.optimize_render {
            config.optimize_render = optimize_render;
        }
        if let Some(render_repair_passes) = self.render_repair_passes {
            config.render_repair_passes = render_repair_passes;
        }
        if let Some(layout_repair_passes) = self.layout_repair_passes {
            config.layout_repair_passes = layout_repair_passes;
        }
        if let Some(debug_critic) = self.debug_critic {
            config.debug_critic = debug_critic;
        }

        config.spacing.max_label_width = config.max_label_width;

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

            let spacing_mode = value
                .get("spacing")
                .or_else(|| value.get("spacing_mode"))
                .and_then(|v| v.as_str())
                .and_then(|s| match s.parse::<SpacingMode>() {
                    Ok(mode) => Some(mode),
                    Err(_) => {
                        eprintln!(
                            "termiflow: warning: {}: unknown spacing preset '{}'",
                            path.display(),
                            s
                        );
                        None
                    }
                });

            let max_label_width = value.get("max_label_width").and_then(|v| v.as_integer());
            let max_edge_label_width = value
                .get("max_edge_label_width")
                .and_then(|v| v.as_integer())
                .or_else(|| value.get("max_edge_label").and_then(|v| v.as_integer()));
            let wrap_labels = value
                .get("wrap")
                .and_then(|v| v.as_bool())
                .or_else(|| value.get("wrap_labels").and_then(|v| v.as_bool()));
            let max_label_lines = value
                .get("max_label_lines")
                .and_then(|v| v.as_integer())
                .or_else(|| value.get("max_lines").and_then(|v| v.as_integer()));
            let crop = value
                .get("crop")
                .and_then(|v| v.as_bool())
                .or_else(|| value.get("trim").and_then(|v| v.as_bool()));
            let pad = value.get("pad").and_then(|v| v.as_integer());
            let optimize_render = value
                .get("optimize_render")
                .and_then(|v| v.as_bool())
                .or_else(|| value.get("optimize").and_then(|v| v.as_bool()));
            let render_repair_passes = value
                .get("render_repair_passes")
                .and_then(|v| v.as_integer())
                .or_else(|| value.get("repair_passes").and_then(|v| v.as_integer()));
            let layout_repair_passes = value
                .get("layout_repair_passes")
                .and_then(|v| v.as_integer())
                .or_else(|| value.get("layout_passes").and_then(|v| v.as_integer()));
            let debug_critic = value
                .get("debug_critic")
                .and_then(|v| v.as_bool())
                .or_else(|| value.get("critic_debug").and_then(|v| v.as_bool()));
            Some(FileConfig {
                max_label_width: max_label_width.map(|n| n as usize),
                max_edge_label_width: max_edge_label_width.map(|n| n as usize),
                wrap_labels,
                max_label_lines: max_label_lines.map(|n| n as usize),
                crop,
                pad: pad.map(|n| n as usize),
                spacing_mode,
                optimize_render,
                render_repair_passes: render_repair_passes.map(|n| (n as usize).max(1)),
                layout_repair_passes: layout_repair_passes.map(|n| (n as usize).max(1)),
                debug_critic,
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
    max_edge_label_width: Option<usize>,
    wrap_labels: Option<bool>,
    max_label_lines: Option<usize>,
    crop: Option<bool>,
    pad: Option<usize>,
    spacing_mode: Option<SpacingMode>,
    optimize_render: Option<bool>,
    render_repair_passes: Option<usize>,
    layout_repair_passes: Option<usize>,
    debug_critic: Option<bool>,
    composite_style: CompositeStyle,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_config_applies_wrap_and_max_lines() {
        let pc = ParseConfig {
            wrap_labels: Some(true),
            max_label_lines: Some(3),
            ..Default::default()
        };

        let cfg = Config::from_parse_config(&pc);
        assert!(cfg.wrap_labels);
        assert_eq!(cfg.max_label_lines, 3);
    }

    #[test]
    fn parse_config_applies_spacing_mode() {
        let pc = ParseConfig {
            spacing_mode: Some(SpacingMode::Compact),
            ..Default::default()
        };

        let cfg = Config::from_parse_config(&pc);
        let compact = SpacingConfig::compact();
        assert_eq!(cfg.spacing.row_spacing, compact.row_spacing);
        assert_eq!(cfg.spacing.col_spacing, compact.col_spacing);
    }

    #[test]
    fn parse_config_applies_render_feedback_settings() {
        let pc = ParseConfig {
            optimize_render: Some(true),
            render_repair_passes: Some(4),
            layout_repair_passes: Some(3),
            debug_critic: Some(true),
            ..Default::default()
        };

        let cfg = Config::from_parse_config(&pc);
        assert!(cfg.optimize_render);
        assert_eq!(cfg.render_repair_passes, 4);
        assert_eq!(cfg.layout_repair_passes, 3);
        assert!(cfg.debug_critic);
    }
}
