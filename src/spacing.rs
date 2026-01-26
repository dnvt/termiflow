//! Unified Spacing Model
//!
//! This module consolidates spacing constants into a cohesive configuration system.
//! Instead of scattered constants across multiple files with implicit coupling,
//! all spacing values are now derived from a small set of base parameters.
//!
//! # Design Principles
//!
//! 1. **Single Source of Truth**: All spacing derives from `SpacingConfig`
//! 2. **Derived Values**: Complex spacing values are computed, not hardcoded
//! 3. **Presets**: Built-in configurations for common use cases (compact, default, spacious)
//! 4. **Backward Compatible**: Default preset matches existing behavior
//!
//! # Usage
//!
//! ```ignore
//! // Use default spacing
//! let spacing = SpacingConfig::default();
//!
//! // Use compact mode for dense diagrams
//! let spacing = SpacingConfig::compact();
//!
//! // Or customize
//! let spacing = SpacingConfig::builder()
//!     .base_cell_height(2)
//!     .node_margin(0)
//!     .build();
//! ```

/// Spacing mode presets
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SpacingMode {
    /// Tight spacing for dense diagrams
    Compact,
    /// Standard spacing (default)
    #[default]
    Default,
    /// Extra breathing room for readability
    Spacious,
}

impl std::str::FromStr for SpacingMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "compact" | "tight" => Ok(SpacingMode::Compact),
            "default" | "normal" | "standard" => Ok(SpacingMode::Default),
            "spacious" | "wide" | "relaxed" => Ok(SpacingMode::Spacious),
            _ => Err(()),
        }
    }
}

/// Unified spacing configuration.
///
/// All spacing values in the layout and render pipelines should reference this
/// struct rather than using scattered constants.
#[derive(Debug, Clone)]
pub struct SpacingConfig {
    // =========================================================================
    // Base parameters (user-tunable)
    // =========================================================================
    /// Height of a node box in characters (default: 3)
    pub box_height: usize,
    /// Minimum width of a node box (default: 5)
    pub box_min_width: usize,
    /// Padding inside boxes on each side (default: 2)
    pub box_padding: usize,

    /// Vertical spacing between rows/layers (default: 2)
    pub row_spacing: usize,
    /// Horizontal spacing between columns/nodes (default: 3)
    pub col_spacing: usize,

    /// Margin around nodes for edge routing (default: 1)
    pub node_margin: usize,

    // =========================================================================
    // Derived values (computed from base parameters)
    // =========================================================================
    /// Vertical stem length for TD/BT layouts
    pub stem_length_vertical: usize,
    /// Horizontal stem length for LR/RL layouts
    pub stem_length_horizontal: usize,
    /// Height of edge junction spacing
    pub edge_junction_height: usize,
    /// Height of drop spacing for multi-target edges
    pub edge_drop_height: usize,
    /// Maximum label width before truncation
    pub max_label_width: usize,

    // =========================================================================
    // Canvas limits
    // =========================================================================
    /// Maximum canvas width in characters
    pub max_canvas_width: usize,
    /// Maximum canvas height in rows
    pub max_canvas_height: usize,
    /// Gutter size for cycle edges
    pub cycle_gutter: usize,
}

impl SpacingConfig {
    /// Create a new spacing configuration builder
    pub fn builder() -> SpacingBuilder {
        SpacingBuilder::new()
    }

    /// Default spacing configuration (matches current behavior)
    pub fn default_config() -> Self {
        Self {
            box_height: 3,
            box_min_width: 5,
            box_padding: 2,
            row_spacing: 2,
            col_spacing: 3,
            node_margin: 1,
            stem_length_vertical: 1,
            stem_length_horizontal: 3,
            edge_junction_height: 1,
            edge_drop_height: 1,
            max_label_width: 20,
            max_canvas_width: 500,
            max_canvas_height: 200,
            cycle_gutter: 4,
        }
    }

    /// Compact spacing for dense diagrams
    ///
    /// Reduces spacing to fit more content in less space:
    /// - Smaller vertical gaps between layers
    /// - Shorter stem lengths
    /// - Reduced margins
    pub fn compact() -> Self {
        let mut config = Self::default_config();
        config.row_spacing = 1;
        config.col_spacing = 2;
        config.node_margin = 0;
        config.stem_length_vertical = 1;
        config.stem_length_horizontal = 2;
        config.edge_junction_height = 1;
        config.edge_drop_height = 1;
        config
    }

    /// Spacious spacing for maximum readability
    ///
    /// Increases spacing for clearer diagrams:
    /// - More vertical space between layers
    /// - Longer stem lengths for clearer edge paths
    /// - Extra margins around nodes
    pub fn spacious() -> Self {
        let mut config = Self::default_config();
        config.box_height = 3;
        config.row_spacing = 3;
        config.col_spacing = 4;
        config.node_margin = 2;
        config.stem_length_vertical = 2;
        config.stem_length_horizontal = 4;
        config.edge_junction_height = 1;
        config.edge_drop_height = 1;
        config
    }

    /// Create spacing configuration from a preset mode
    pub fn from_mode(mode: SpacingMode) -> Self {
        match mode {
            SpacingMode::Compact => Self::compact(),
            SpacingMode::Default => Self::default_config(),
            SpacingMode::Spacious => Self::spacious(),
        }
    }

    /// Calculate the effective row height (box + spacing)
    pub fn effective_row_height(&self) -> usize {
        self.box_height + self.row_spacing
    }

    /// Calculate the minimum column width (box + spacing)
    pub fn effective_col_width(&self) -> usize {
        self.box_min_width + self.col_spacing
    }

    /// Calculate box width from a label's display width
    pub fn box_width_for_label(&self, label_width: usize) -> usize {
        let clamped = label_width.min(self.max_label_width);
        (clamped + self.box_padding * 2 + 2).max(self.box_min_width)
    }
}

impl Default for SpacingConfig {
    fn default() -> Self {
        Self::default_config()
    }
}

/// Builder for SpacingConfig
#[derive(Debug, Clone, Default)]
pub struct SpacingBuilder {
    box_height: Option<usize>,
    box_min_width: Option<usize>,
    box_padding: Option<usize>,
    row_spacing: Option<usize>,
    col_spacing: Option<usize>,
    node_margin: Option<usize>,
    stem_length_vertical: Option<usize>,
    stem_length_horizontal: Option<usize>,
    max_label_width: Option<usize>,
    max_canvas_width: Option<usize>,
    max_canvas_height: Option<usize>,
    cycle_gutter: Option<usize>,
}

impl SpacingBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn box_height(mut self, height: usize) -> Self {
        self.box_height = Some(height);
        self
    }

    pub fn box_min_width(mut self, width: usize) -> Self {
        self.box_min_width = Some(width);
        self
    }

    pub fn box_padding(mut self, padding: usize) -> Self {
        self.box_padding = Some(padding);
        self
    }

    pub fn row_spacing(mut self, spacing: usize) -> Self {
        self.row_spacing = Some(spacing);
        self
    }

    pub fn col_spacing(mut self, spacing: usize) -> Self {
        self.col_spacing = Some(spacing);
        self
    }

    pub fn node_margin(mut self, margin: usize) -> Self {
        self.node_margin = Some(margin);
        self
    }

    pub fn stem_length_vertical(mut self, length: usize) -> Self {
        self.stem_length_vertical = Some(length);
        self
    }

    pub fn stem_length_horizontal(mut self, length: usize) -> Self {
        self.stem_length_horizontal = Some(length);
        self
    }

    pub fn max_label_width(mut self, width: usize) -> Self {
        self.max_label_width = Some(width);
        self
    }

    pub fn max_canvas_width(mut self, width: usize) -> Self {
        self.max_canvas_width = Some(width);
        self
    }

    pub fn max_canvas_height(mut self, height: usize) -> Self {
        self.max_canvas_height = Some(height);
        self
    }

    pub fn cycle_gutter(mut self, gutter: usize) -> Self {
        self.cycle_gutter = Some(gutter);
        self
    }

    /// Build the SpacingConfig with defaults for unspecified values
    pub fn build(self) -> SpacingConfig {
        let defaults = SpacingConfig::default_config();
        SpacingConfig {
            box_height: self.box_height.unwrap_or(defaults.box_height),
            box_min_width: self.box_min_width.unwrap_or(defaults.box_min_width),
            box_padding: self.box_padding.unwrap_or(defaults.box_padding),
            row_spacing: self.row_spacing.unwrap_or(defaults.row_spacing),
            col_spacing: self.col_spacing.unwrap_or(defaults.col_spacing),
            node_margin: self.node_margin.unwrap_or(defaults.node_margin),
            stem_length_vertical: self
                .stem_length_vertical
                .unwrap_or(defaults.stem_length_vertical),
            stem_length_horizontal: self
                .stem_length_horizontal
                .unwrap_or(defaults.stem_length_horizontal),
            edge_junction_height: defaults.edge_junction_height,
            edge_drop_height: defaults.edge_drop_height,
            max_label_width: self.max_label_width.unwrap_or(defaults.max_label_width),
            max_canvas_width: self.max_canvas_width.unwrap_or(defaults.max_canvas_width),
            max_canvas_height: self.max_canvas_height.unwrap_or(defaults.max_canvas_height),
            cycle_gutter: self.cycle_gutter.unwrap_or(defaults.cycle_gutter),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_matches_existing_constants() {
        let spacing = SpacingConfig::default();
        // These should match the existing constants in style.rs
        assert_eq!(spacing.box_height, 3);
        assert_eq!(spacing.box_min_width, 5);
        assert_eq!(spacing.box_padding, 2);
        assert_eq!(spacing.row_spacing, 2);
        assert_eq!(spacing.col_spacing, 3);
        assert_eq!(spacing.stem_length_vertical, 1);
        assert_eq!(spacing.stem_length_horizontal, 3);
        assert_eq!(spacing.max_label_width, 20);
        assert_eq!(spacing.max_canvas_width, 500);
        assert_eq!(spacing.max_canvas_height, 200);
        assert_eq!(spacing.cycle_gutter, 4);
    }

    #[test]
    fn test_compact_reduces_spacing() {
        let default = SpacingConfig::default();
        let compact = SpacingConfig::compact();

        assert!(compact.row_spacing <= default.row_spacing);
        assert!(compact.col_spacing <= default.col_spacing);
    }

    #[test]
    fn test_spacious_increases_spacing() {
        let default = SpacingConfig::default();
        let spacious = SpacingConfig::spacious();

        assert!(spacious.row_spacing >= default.row_spacing);
        assert!(spacious.col_spacing >= default.col_spacing);
    }

    #[test]
    fn test_builder() {
        let spacing = SpacingConfig::builder()
            .box_height(4)
            .row_spacing(3)
            .build();

        assert_eq!(spacing.box_height, 4);
        assert_eq!(spacing.row_spacing, 3);
        // Others should be default
        assert_eq!(spacing.box_min_width, 5);
    }

    #[test]
    fn test_spacing_mode_parse() {
        assert_eq!("compact".parse(), Ok(SpacingMode::Compact));
        assert_eq!("tight".parse(), Ok(SpacingMode::Compact));
        assert_eq!("default".parse(), Ok(SpacingMode::Default));
        assert_eq!("normal".parse(), Ok(SpacingMode::Default));
        assert_eq!("spacious".parse(), Ok(SpacingMode::Spacious));
        assert_eq!("wide".parse(), Ok(SpacingMode::Spacious));
        assert!("invalid".parse::<SpacingMode>().is_err());
    }

    #[test]
    fn test_from_mode() {
        let compact = SpacingConfig::from_mode(SpacingMode::Compact);
        let default = SpacingConfig::from_mode(SpacingMode::Default);
        let spacious = SpacingConfig::from_mode(SpacingMode::Spacious);

        assert!(compact.row_spacing < spacious.row_spacing);
        assert_eq!(default.row_spacing, SpacingConfig::default().row_spacing);
    }

    #[test]
    fn test_effective_dimensions() {
        let spacing = SpacingConfig::default();
        assert_eq!(spacing.effective_row_height(), 5); // 3 + 2
        assert_eq!(spacing.effective_col_width(), 8); // 5 + 3
    }

    #[test]
    fn test_box_width_calculation() {
        let spacing = SpacingConfig::default();

        // Short label: label_width=1 → 1 + 4 + 2 = 7, but min is 5, so 7
        assert_eq!(spacing.box_width_for_label(1), 7);

        // Normal label: label_width=7 → 7 + 4 + 2 = 13
        assert_eq!(spacing.box_width_for_label(7), 13);

        // Long label: label_width=30 → clamped to 20 → 20 + 4 + 2 = 26
        assert_eq!(spacing.box_width_for_label(30), 26);
    }
}
