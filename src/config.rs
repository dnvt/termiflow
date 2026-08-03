//! Configuration loading with 3-tier priority
//!
//! Priority: CLI flags > in-file directives > config file
//! See SPEC §5 for details

use std::fs;
use std::path::Path;

use crate::parser::ParseConfig;
use crate::spacing::{SpacingConfig, SpacingMode};
use crate::style::CompositeStyle;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const EFFECTIVE_POLICY_SCHEMA: &str = "termiflow.effective_policy.v1";

/// Top-level policy paths owned by the render-boundary contract. The
/// architecture checker reads this list so a new behavior-bearing input cannot
/// silently bypass the effective-policy matrix.
pub const EFFECTIVE_POLICY_CONTRACT_FIELDS: &[&str] = &[
    "config.max_label_width",
    "config.max_edge_label_width",
    "config.wrap_labels",
    "config.max_label_lines",
    "config.crop",
    "config.pad",
    "config.strict_parsing",
    "config.composite_style",
    "config.spacing",
    "config.optimize_render",
    "config.render_repair_passes",
    "config.layout_repair_passes",
    "config.debug_critic",
    "runtime.compatibility.optimize_render",
    "runtime.compatibility.disable_portals",
    "runtime.compatibility.render_repair_passes",
    "runtime.compatibility.layout_repair_passes",
    "runtime.diagnostics.timing",
    "runtime.diagnostics.routes",
    "runtime.diagnostics.fan_in",
    "runtime.diagnostics.fan_out",
    "runtime.diagnostics.cross",
    "runtime.diagnostics.crossing",
    "runtime.diagnostics.critic",
    "runtime.terminal.columns",
    "runtime.terminal.lines",
    "boundary.direction",
    "boundary.display_profile",
    "boundary.scaling_mode",
    "boundary.from_json",
    "boundary.fit_terminal",
    "environment.TERM",
    "environment.LANG",
    "environment.LC_ALL",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfigSource {
    File,
    Directive,
    Builder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfigField {
    MaxLabelWidth,
    MaxEdgeLabelWidth,
    WrapLabels,
    MaxLabelLines,
    Crop,
    Pad,
    StrictParsing,
    CompositeStyle,
    Spacing,
    OptimizeRender,
    RenderRepairPasses,
    LayoutRepairPasses,
    DebugCritic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EffectClass {
    LayoutAndOutput,
    Output,
    Parsing,
    Diagnostic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResolutionAction {
    Applied,
    Normalized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResolutionRecord {
    pub(crate) source: ConfigSource,
    pub(crate) field: ConfigField,
    pub(crate) action: ResolutionAction,
    pub(crate) effect: EffectClass,
}

/// Presence-aware values contributed by one configuration source.
#[derive(Debug, Clone, Default)]
pub(crate) struct ConfigPatch {
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

#[derive(Debug, Clone)]
pub(crate) struct ResolvedConfig {
    pub(crate) config: Config,
    #[allow(dead_code)]
    pub(crate) records: Vec<ResolutionRecord>,
}

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

/// Build the canonical policy record for one resolved render boundary.
///
/// The record deliberately includes both resolved configuration and the
/// process-bound compatibility inputs consumed below the public API. Callers
/// can persist the returned `sha256` alongside evidence and provenance.
pub fn effective_render_policy(
    config: &Config,
    direction: crate::graph::Direction,
    display_profile: &str,
    scaling_mode: &str,
    from_json: bool,
    fit_terminal: bool,
) -> Value {
    let runtime = crate::runtime::current();
    let fields = json!({
        "config": {
            "max_label_width": config.max_label_width,
            "max_edge_label_width": config.max_edge_label_width,
            "wrap_labels": config.wrap_labels,
            "max_label_lines": config.max_label_lines,
            "crop": config.crop,
            "pad": config.pad,
            "strict_parsing": config.strict_parsing,
            "composite_style": composite_style_value(&config.composite_style),
            "spacing": spacing_value(&config.spacing),
            "optimize_render": config.optimize_render,
            "render_repair_passes": config.render_repair_passes,
            "layout_repair_passes": config.layout_repair_passes,
            "debug_critic": config.debug_critic,
        },
        "runtime": {
            "compatibility": {
                "optimize_render": runtime.compatibility.optimize_render,
                "disable_portals": runtime.compatibility.disable_portals,
                "render_repair_passes": runtime.compatibility.render_repair_passes,
                "layout_repair_passes": runtime.compatibility.layout_repair_passes,
            },
            "diagnostics": {
                "timing": runtime.diagnostics.timing,
                "routes": runtime.diagnostics.routes,
                "fan_in": runtime.diagnostics.fan_in,
                "fan_out": runtime.diagnostics.fan_out,
                "cross": runtime.diagnostics.cross,
                "crossing": runtime.diagnostics.crossing,
                "critic": runtime.diagnostics.critic,
            },
            "terminal": {
                "columns": runtime.terminal.columns,
                "lines": runtime.terminal.lines,
            },
        },
        "boundary": {
            "direction": format!("{direction:?}"),
            "display_profile": display_profile,
            "scaling_mode": scaling_mode,
            "from_json": from_json,
            "fit_terminal": fit_terminal,
        },
        "environment": {
            "TERM": normalized_environment("TERM"),
            "LANG": normalized_environment("LANG"),
            "LC_ALL": normalized_environment("LC_ALL"),
        },
        "contract_fields": EFFECTIVE_POLICY_CONTRACT_FIELDS,
    });
    let sha256 = policy_digest(&fields);
    json!({
        "schema": EFFECTIVE_POLICY_SCHEMA,
        "version": 1,
        "fields": fields,
        "sha256": sha256,
    })
}

fn composite_style_value(style: &CompositeStyle) -> Value {
    json!({
        "corner": style.corner.map(|value| format!("{value:?}")),
        "border": style.border.map(|value| format!("{value:?}")),
        "arrow": style.arrow.map(|value| format!("{value:?}")),
        "edge": style.edge.map(|value| format!("{value:?}")),
        "junction": style.junction.map(|value| format!("{value:?}")),
        "back": style.back.map(|value| format!("{value:?}")),
        "subgraph": style.subgraph.map(|value| format!("{value:?}")),
        "fallback": "Unicode",
    })
}

fn spacing_value(spacing: &SpacingConfig) -> Value {
    json!({
        "box_height": spacing.box_height,
        "box_min_width": spacing.box_min_width,
        "box_padding": spacing.box_padding,
        "row_spacing": spacing.row_spacing,
        "col_spacing": spacing.col_spacing,
        "node_margin": spacing.node_margin,
        "subgraph_gutter": spacing.subgraph_gutter,
        "stem_length_vertical": spacing.stem_length_vertical,
        "stem_length_horizontal": spacing.stem_length_horizontal,
        "edge_junction_height": spacing.edge_junction_height,
        "edge_drop_height": spacing.edge_drop_height,
        "max_label_width": spacing.max_label_width,
        "max_canvas_width": spacing.max_canvas_width,
        "max_canvas_height": spacing.max_canvas_height,
        "cycle_gutter": spacing.cycle_gutter,
    })
}

fn sha256_json(value: &Value) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    Sha256::digest(
        serde_json::to_vec(&canonical_json(value)).expect("effective policy is serializable"),
    )
    .iter()
    .flat_map(|byte| {
        [
            HEX[(byte >> 4) as usize] as char,
            HEX[(byte & 0x0f) as usize] as char,
        ]
    })
    .collect()
}

fn normalized_environment(name: &str) -> Value {
    std::env::var_os(name)
        .and_then(|value| value.into_string().ok())
        .map(|value| Value::String(value.trim().to_owned()))
        .unwrap_or(Value::Null)
}

/// Canonicalize JSON objects recursively while preserving array order. This
/// keeps policy digests independent of map insertion order at validation time.
pub fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort();
            let mut canonical = serde_json::Map::new();
            for key in keys {
                canonical.insert(key.clone(), canonical_json(&object[key]));
            }
            Value::Object(canonical)
        }
        Value::Array(values) => Value::Array(values.iter().map(canonical_json).collect()),
        _ => value.clone(),
    }
}

pub fn policy_digest(fields: &Value) -> String {
    sha256_json(fields)
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

impl ConfigPatch {
    fn from_parse_config(parse_config: &ParseConfig) -> Self {
        Self {
            composite_style: parse_config.style.as_deref().map(CompositeStyle::parse),
            max_label_width: parse_config.max_label,
            max_edge_label_width: parse_config.max_edge_label,
            wrap_labels: parse_config.wrap_labels,
            max_label_lines: parse_config.max_label_lines,
            spacing: parse_config.spacing_mode.map(SpacingConfig::from_mode),
            optimize_render: parse_config.optimize_render,
            render_repair_passes: parse_config.render_repair_passes,
            layout_repair_passes: parse_config.layout_repair_passes,
            debug_critic: parse_config.debug_critic,
            ..Self::default()
        }
    }

    fn from_file_config(file_config: &FileConfig) -> Self {
        Self {
            max_label_width: file_config.max_label_width,
            max_edge_label_width: file_config.max_edge_label_width,
            wrap_labels: file_config.wrap_labels,
            max_label_lines: file_config.max_label_lines,
            crop: file_config.crop,
            pad: file_config.pad,
            composite_style: file_config.composite_style.clone(),
            spacing: file_config.spacing_mode.map(SpacingConfig::from_mode),
            optimize_render: file_config.optimize_render,
            render_repair_passes: file_config.render_repair_passes,
            layout_repair_passes: file_config.layout_repair_passes,
            debug_critic: file_config.debug_critic,
            ..Self::default()
        }
    }

    fn apply(self, config: &mut Config, source: ConfigSource, records: &mut Vec<ResolutionRecord>) {
        macro_rules! apply_value {
            ($field:ident, $field_kind:ident, $target:ident, $effect:expr) => {
                if let Some(value) = self.$field {
                    config.$target = value;
                    records.push(ResolutionRecord {
                        source,
                        field: ConfigField::$field_kind,
                        action: ResolutionAction::Applied,
                        effect: $effect,
                    });
                }
            };
        }

        apply_value!(
            max_label_width,
            MaxLabelWidth,
            max_label_width,
            EffectClass::LayoutAndOutput
        );
        apply_value!(
            max_edge_label_width,
            MaxEdgeLabelWidth,
            max_edge_label_width,
            EffectClass::LayoutAndOutput
        );
        apply_value!(
            wrap_labels,
            WrapLabels,
            wrap_labels,
            EffectClass::LayoutAndOutput
        );
        apply_value!(crop, Crop, crop, EffectClass::Output);
        apply_value!(pad, Pad, pad, EffectClass::Output);
        apply_value!(
            strict_parsing,
            StrictParsing,
            strict_parsing,
            EffectClass::Parsing
        );
        apply_value!(
            composite_style,
            CompositeStyle,
            composite_style,
            EffectClass::Output
        );
        apply_value!(spacing, Spacing, spacing, EffectClass::LayoutAndOutput);
        apply_value!(
            optimize_render,
            OptimizeRender,
            optimize_render,
            EffectClass::LayoutAndOutput
        );
        apply_value!(
            debug_critic,
            DebugCritic,
            debug_critic,
            EffectClass::Diagnostic
        );

        if let Some(lines) = self.max_label_lines {
            config.max_label_lines = lines;
            records.push(ResolutionRecord {
                source,
                field: ConfigField::MaxLabelLines,
                action: ResolutionAction::Applied,
                effect: EffectClass::LayoutAndOutput,
            });
        }

        if let Some(passes) = self.render_repair_passes {
            let normalized = passes.max(1);
            config.render_repair_passes = normalized;
            records.push(ResolutionRecord {
                source,
                field: ConfigField::RenderRepairPasses,
                action: if normalized == passes {
                    ResolutionAction::Applied
                } else {
                    ResolutionAction::Normalized
                },
                effect: EffectClass::LayoutAndOutput,
            });
        }

        if let Some(passes) = self.layout_repair_passes {
            let normalized = passes.max(1);
            config.layout_repair_passes = normalized;
            records.push(ResolutionRecord {
                source,
                field: ConfigField::LayoutRepairPasses,
                action: if normalized == passes {
                    ResolutionAction::Applied
                } else {
                    ResolutionAction::Normalized
                },
                effect: EffectClass::LayoutAndOutput,
            });
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
        resolve_config(load_file_config(), parse_config, ConfigPatch::default()).config
    }
}

fn resolve_config(
    file_config: Option<FileConfig>,
    parse_config: &ParseConfig,
    builder_patch: ConfigPatch,
) -> ResolvedConfig {
    let mut config = Config::default();
    let mut records = Vec::new();

    if let Some(file_config) = file_config.as_ref() {
        ConfigPatch::from_file_config(file_config).apply(
            &mut config,
            ConfigSource::File,
            &mut records,
        );
    }

    ConfigPatch::from_parse_config(parse_config).apply(
        &mut config,
        ConfigSource::Directive,
        &mut records,
    );
    builder_patch.apply(&mut config, ConfigSource::Builder, &mut records);

    // This derived spacing field has always followed the resolved label
    // width. Keep that invariant in one place after all source patches.
    config.spacing.max_label_width = config.max_label_width;

    ResolvedConfig { config, records }
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
        resolve_config(load_file_config(), parse_config, self.into_patch()).config
    }

    fn into_patch(self) -> ConfigPatch {
        ConfigPatch {
            max_label_width: self.max_label_width,
            max_edge_label_width: self.max_edge_label_width,
            wrap_labels: self.wrap_labels,
            max_label_lines: self.max_label_lines,
            crop: self.crop,
            pad: self.pad,
            strict_parsing: self.strict_parsing,
            composite_style: self.composite_style,
            spacing: self.spacing,
            optimize_render: self.optimize_render,
            render_repair_passes: self.render_repair_passes,
            layout_repair_passes: self.layout_repair_passes,
            debug_critic: self.debug_critic,
        }
    }
}

/// Load configuration from ~/.config/termiflow/config.toml
fn load_file_config() -> Option<FileConfig> {
    let mut path = dirs::config_dir()?;
    path.push("termiflow");
    path.push("config.toml");

    let contents = fs::read_to_string(&path).ok()?;
    parse_file_config(&path, &contents)
}

fn parse_file_config(path: &Path, contents: &str) -> Option<FileConfig> {
    match toml::from_str::<toml::Value>(contents) {
        Ok(value) => {
            let style_str = value.get("style").and_then(|v| v.as_str());
            let composite_style = style_str.map(CompositeStyle::parse);

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

            Some(FileConfig {
                max_label_width: integer_alias(&value, path, &["max_label_width"]),
                max_edge_label_width: integer_alias(
                    &value,
                    path,
                    &["max_edge_label_width", "max_edge_label"],
                ),
                wrap_labels: bool_alias(&value, &["wrap", "wrap_labels"]),
                max_label_lines: integer_alias(&value, path, &["max_label_lines", "max_lines"]),
                crop: bool_alias(&value, &["crop", "trim"]),
                pad: integer_alias(&value, path, &["pad"]),
                spacing_mode,
                optimize_render: bool_alias(&value, &["optimize_render", "optimize"]),
                render_repair_passes: integer_alias(
                    &value,
                    path,
                    &["render_repair_passes", "repair_passes"],
                ),
                layout_repair_passes: integer_alias(
                    &value,
                    path,
                    &["layout_repair_passes", "layout_passes"],
                ),
                debug_critic: bool_alias(&value, &["debug_critic", "critic_debug"]),
                composite_style,
            })
        }
        Err(e) => {
            eprintln!("termiflow: warning: {}: {}", path.display(), e);
            None
        }
    }
}

fn bool_alias(value: &toml::Value, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(|v| v.as_bool()))
}

fn integer_alias(value: &toml::Value, path: &Path, keys: &[&str]) -> Option<usize> {
    for key in keys {
        let Some(number) = value.get(*key).and_then(|v| v.as_integer()) else {
            continue;
        };
        if let Some(parsed) = checked_config_usize(path, key, number) {
            return Some(parsed);
        }
    }
    None
}

fn checked_config_usize(path: &Path, key: &str, number: i64) -> Option<usize> {
    match usize::try_from(number) {
        Ok(value) => Some(value),
        Err(_) => {
            eprintln!(
                "termiflow: warning: {}: {} must be a non-negative usize; ignoring {}",
                path.display(),
                key,
                number
            );
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
    composite_style: Option<CompositeStyle>,
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

    #[test]
    fn resolver_preserves_source_presence_and_precedence() {
        let file = parse_file_config(
            Path::new("injected/config.toml"),
            "wrap = true\npad = 4\nmax_label_width = 31\n",
        )
        .expect("injected config parses");
        let directives = ParseConfig {
            wrap_labels: Some(true),
            max_label: Some(27),
            ..Default::default()
        };
        let builder = Config::builder()
            .wrap_labels(false)
            .pad(0)
            .max_label_width(19);

        let resolved = resolve_config(Some(file), &directives, builder.into_patch());

        assert!(!resolved.config.wrap_labels);
        assert_eq!(resolved.config.pad, 0);
        assert_eq!(resolved.config.max_label_width, 19);
        assert!(resolved.records.iter().any(|record| {
            record.source == ConfigSource::File
                && record.field == ConfigField::WrapLabels
                && record.effect == EffectClass::LayoutAndOutput
        }));
        assert!(resolved.records.iter().any(|record| {
            record.source == ConfigSource::Builder
                && record.field == ConfigField::WrapLabels
                && record.action == ResolutionAction::Applied
        }));
    }

    #[test]
    fn resolver_normalizes_repair_passes_and_rejects_negative_file_values() {
        let file = parse_file_config(
            Path::new("injected/config.toml"),
            "max_label_width = -1\nrender_repair_passes = -2\nlayout_repair_passes = 0\n",
        )
        .expect("injected config parses");
        assert_eq!(file.max_label_width, None);
        assert_eq!(file.render_repair_passes, None);
        assert_eq!(file.layout_repair_passes, Some(0));

        let directives = ParseConfig {
            render_repair_passes: Some(0),
            layout_repair_passes: Some(3),
            ..Default::default()
        };
        let resolved = resolve_config(Some(file), &directives, ConfigPatch::default());

        assert_eq!(resolved.config.render_repair_passes, 1);
        assert_eq!(resolved.config.layout_repair_passes, 3);
        assert!(resolved.records.iter().any(|record| {
            record.source == ConfigSource::Directive
                && record.field == ConfigField::RenderRepairPasses
                && record.action == ResolutionAction::Normalized
        }));
    }

    #[test]
    fn effective_policy_digest_binds_render_fields_but_not_unrelated_inputs() {
        let base = Config::default();
        let first = effective_render_policy(
            &base,
            crate::graph::Direction::TD,
            "test-display",
            "Fixed",
            false,
            false,
        );

        let mut changed = base.clone();
        changed.max_label_width += 1;
        let second = effective_render_policy(
            &changed,
            crate::graph::Direction::TD,
            "test-display",
            "Fixed",
            false,
            false,
        );
        assert_ne!(first["sha256"], second["sha256"]);
        assert_eq!(first["schema"], EFFECTIVE_POLICY_SCHEMA);
        assert!(first["fields"]["config"]["spacing"]
            .get("edge_drop_height")
            .is_some());

        let unrelated = effective_render_policy(
            &base,
            crate::graph::Direction::TD,
            "test-display",
            "Fixed",
            false,
            false,
        );
        assert_eq!(first, unrelated);
    }
}
