//! TermiFlow - Terminal Mermaid Diagram Renderer
//!
//! A library for rendering Mermaid flowchart diagrams as ASCII/Unicode art.
//!
//! # Quick Start
//!
//! ```rust
//! use termiflow::{render, RenderOptions};
//!
//! let input = "graph TD\n    A[Start] --> B[End]";
//! let output = render(input, RenderOptions::default()).unwrap();
//! println!("{}", output);
//! ```
//!
//! # Pipeline
//!
//! The rendering pipeline has three stages:
//! 1. **Parse** - Convert Mermaid syntax to a graph structure
//! 2. **Layout** - Assign coordinates using the waterfall algorithm
//! 3. **Render** - Draw boxes and edges on a 2D canvas

// ============================================================================
// Modules
// ============================================================================

pub mod config;
pub mod geom;
pub mod graph;
pub mod layout;
pub mod measure;
pub mod orientation;
pub mod portals;
pub mod parser;
pub mod render;
pub mod style;

// ============================================================================
// Re-exports for convenient access
// ============================================================================

pub use config::{Config, ConfigBuilder};
pub use graph::{Edge, Graph, Node};
pub use layout::coarse_waterfall;
pub use parser::{parse, ParseConfig, ParseResult};
pub use render::render as render_canvas;
pub use style::{BaseStyle, CompositeStyle};

// ============================================================================
// High-Level API
// ============================================================================

use anyhow::Result;
/// Options for rendering a diagram
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// Border style (default: Unicode)
    pub style: BaseStyle,
    /// Maximum label width before truncation (default: 20)
    pub max_label_width: usize,
    /// Enable multiline label wrapping (default: false)
    pub wrap_labels: bool,
    /// Maximum number of label lines when wrapping is enabled (default: 1)
    pub max_label_lines: usize,
    /// Strict mode - fail on any parse warning (default: false)
    pub strict: bool,
    /// Crop empty margins around output (default: true)
    pub crop: bool,
    /// Add padding around output (default: 0)
    pub pad: usize,
    /// Use a tighter layout spacing (default: false)
    pub compact: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderOptions {
    pub fn new() -> Self {
        Self {
            style: BaseStyle::default(),
            max_label_width: 20,
            wrap_labels: false,
            max_label_lines: 1,
            strict: false,
            crop: true,
            pad: 0,
            compact: false,
        }
    }

    pub fn with_style(mut self, style: BaseStyle) -> Self {
        self.style = style;
        self
    }

    pub fn with_max_label(mut self, width: usize) -> Self {
        self.max_label_width = width;
        self
    }

    pub fn with_wrap_labels(mut self, wrap: bool) -> Self {
        self.wrap_labels = wrap;
        self
    }

    pub fn with_max_label_lines(mut self, lines: usize) -> Self {
        self.max_label_lines = lines;
        self
    }

    pub fn strict(mut self) -> Self {
        self.strict = true;
        self
    }

    pub fn with_crop(mut self, crop: bool) -> Self {
        self.crop = crop;
        self
    }

    pub fn with_pad(mut self, pad: usize) -> Self {
        self.pad = pad;
        self
    }

    pub fn with_compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }
}

/// Render a Mermaid diagram to ASCII/Unicode art.
///
/// This is the main entry point for library usage. It combines parsing,
/// layout, and rendering into a single function.
///
/// # Arguments
/// * `input` - Mermaid flowchart source (e.g., "graph TD\nA --> B")
/// * `options` - Rendering options (style, label width, etc.)
///
/// # Returns
/// * `Ok(String)` - The rendered diagram as a string
/// * `Err` - Parse or layout error
///
/// # Example
/// ```rust
/// use termiflow::{render, RenderOptions, BaseStyle};
///
/// let diagram = render(
///     "graph TD\n    A[Hello] --> B[World]",
///     RenderOptions::new().with_style(BaseStyle::Rounded)
/// ).unwrap();
/// ```
pub fn render(input: &str, options: RenderOptions) -> Result<String> {
    // Parse
    let parse_result = parser::parse(input, options.strict)?;

    // Build config from options + in-file directives
    let config = Config::builder()
        .max_label_width(options.max_label_width)
        .wrap_labels(options.wrap_labels)
        .max_label_lines(options.max_label_lines)
        .crop(options.crop)
        .pad(options.pad)
        .strict(options.strict)
        .style(CompositeStyle::from_base(options.style))
        .build(&parse_result.config);

    // Measure labels + node height (opt-in via config)
    let mut graph = parse_result.graph;
    measure::measure_graph(&mut graph, &config);

    // Layout (default coarse waterfall)
    let layout_config = if options.compact {
        layout::CoarseLayoutConfig::compact()
    } else {
        layout::CoarseLayoutConfig::default()
    };
    let graph = layout::coarse_waterfall_with_config(graph, layout_config)?;

    // Render
    render::render(&graph, &config)
}

/// Render with default options (Unicode style, 20-char labels)
pub fn render_default(input: &str) -> Result<String> {
    render(input, RenderOptions::default())
}
