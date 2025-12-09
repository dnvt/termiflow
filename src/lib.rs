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
pub mod graph;
pub mod layout;
pub mod parser;
pub mod render;
pub mod style;

// ============================================================================
// Re-exports for convenient access
// ============================================================================

pub use config::{Config, ConfigBuilder};
pub use graph::{Graph, Node, Edge};
pub use parser::{parse, ParseResult, ParseConfig};
pub use layout::waterfall;
pub use render::render as render_canvas;
pub use style::{BorderStyle, CompositeStyle};

// ============================================================================
// High-Level API
// ============================================================================

use anyhow::Result;

/// Options for rendering a diagram
#[derive(Debug, Clone, Default)]
pub struct RenderOptions {
    /// Border style (default: Unicode)
    pub style: BorderStyle,
    /// Maximum label width before truncation (default: 20)
    pub max_label_width: usize,
    /// Strict mode - fail on any parse warning (default: false)
    pub strict: bool,
}

impl RenderOptions {
    pub fn new() -> Self {
        Self {
            style: BorderStyle::default(),
            max_label_width: 20,
            strict: false,
        }
    }

    pub fn with_style(mut self, style: BorderStyle) -> Self {
        self.style = style;
        self
    }

    pub fn with_max_label(mut self, width: usize) -> Self {
        self.max_label_width = width;
        self
    }

    pub fn strict(mut self) -> Self {
        self.strict = true;
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
/// use termiflow::{render, RenderOptions, BorderStyle};
///
/// let diagram = render(
///     "graph TD\n    A[Hello] --> B[World]",
///     RenderOptions::new().with_style(BorderStyle::Rounded)
/// ).unwrap();
/// ```
pub fn render(input: &str, options: RenderOptions) -> Result<String> {
    // Parse
    let parse_result = parser::parse(input, options.strict)?;

    // Layout
    let graph = layout::waterfall(parse_result.graph)?;

    // Build config from options + in-file directives
    let config = Config::builder()
        .max_label_width(options.max_label_width)
        .strict(options.strict)
        .style(CompositeStyle::from_base(options.style))
        .build(&parse_result.config);

    // Render
    render::render(&graph, &config)
}

/// Render with default options (Unicode style, 20-char labels)
pub fn render_default(input: &str) -> Result<String> {
    render(input, RenderOptions::default())
}
