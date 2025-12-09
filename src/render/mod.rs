//! Render module - 2D character grid rendering for diagrams.
//!
//! This module handles the final rendering phase:
//! - Box drawing for nodes with labels
//! - Edge routing (straight, L-shaped, back-edges)
//! - Junction/crossing detection for overlapping paths
//!
//! Rendering order: edges first, then boxes (so boxes overwrite edge lines).
//!
//! # Module Structure
//!
//! - `canvas` - Core Canvas struct and character classification utilities
//! - `edge` - Edge routing algorithms (straight, L-shaped, back-edge)
//!
//! # Future Expansion
//!
//! This module is designed to support multiple diagram types:
//! - Current: Flowchart rendering (graph TD/LR)
//! - Future: Sequence diagrams, class diagrams, etc.

pub mod canvas;
pub mod edge;

// Re-exports
pub use canvas::Canvas;

use anyhow::Result;

use crate::config::Config;
use crate::graph::{Graph, Node};
use crate::style::{
    truncate_label, BorderStyle, BOX_HEIGHT, COL_SPACING, MAX_CANVAS_HEIGHT, MAX_CANVAS_WIDTH,
    RIGHT_GUTTER, ROW_SPACING,
};

use edge::{center_x, route_back_edge, route_edge};

// ============================================================================
// Main Render Function
// ============================================================================

/// Render a graph to a string.
///
/// This is the main entry point for the render module. It:
/// 1. Calculates canvas dimensions from node positions
/// 2. Draws all edges (sorted for optimal junction creation)
/// 3. Draws all boxes (overwriting any edge lines that pass through)
pub fn render(graph: &Graph, config: &Config) -> Result<String> {
    if graph.nodes.is_empty() {
        return Ok(String::new());
    }

    // Calculate canvas size from laid-out nodes
    let max_right = graph.nodes.iter().map(|n| n.x + n.width).max().unwrap_or(0);
    let max_bottom = graph.nodes.iter().map(|n| n.y + BOX_HEIGHT).max().unwrap_or(0);
    let gutter = if graph.has_cycles() { RIGHT_GUTTER } else { 0 };

    let mut width = max_right + COL_SPACING + gutter;
    if width > MAX_CANVAS_WIDTH {
        width = MAX_CANVAS_WIDTH;
        eprintln!(
            "termiflow: warning: Graph too wide ({} chars), clipping to {}",
            max_right + COL_SPACING + gutter,
            MAX_CANVAS_WIDTH
        );
    }
    width = width
        .max(max_right.saturating_add(1).min(MAX_CANVAS_WIDTH))
        .max(1);

    let mut height = max_bottom + ROW_SPACING;
    if height > MAX_CANVAS_HEIGHT {
        height = MAX_CANVAS_HEIGHT;
        eprintln!(
            "termiflow: warning: Graph too tall ({} rows), clipping to {}",
            max_bottom + ROW_SPACING,
            MAX_CANVAS_HEIGHT
        );
    }
    height = height
        .max(max_bottom.saturating_add(1).min(MAX_CANVAS_HEIGHT))
        .max(1);

    if graph.has_cycles() && width <= RIGHT_GUTTER {
        eprintln!("termiflow: warning: Back-edges skipped (gutter clipped)");
    }

    let mut canvas = Canvas::new(width, height);
    let chars = config.composite_style.to_style_chars(BorderStyle::default());

    // Get visible nodes
    let visible_nodes: Vec<&Node> = graph
        .nodes
        .iter()
        .filter(|n| canvas.is_visible(n))
        .collect();

    // Sort edges: straight edges first, then L-shaped edges
    // This allows L-shaped edges to merge with existing straight paths
    let mut edge_indices: Vec<(usize, bool)> = graph
        .edges
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            if e.is_back_edge {
                return Some((i, false)); // Back edges last
            }
            let from = graph.get_node(&e.from)?;
            let to = graph.get_node(&e.to)?;
            if !canvas.is_visible(from) || !canvas.is_visible(to) {
                return None;
            }
            // Check if edge needs L-shaped routing
            let start_x = center_x(from);
            let end_x = center_x(to);
            let start_y = from.y + BOX_HEIGHT;
            let end_y = to.y;
            let x_diff = (start_x as isize - end_x as isize).unsigned_abs();
            let needs_lshape = x_diff > 1
                || visible_nodes.iter().any(|n| {
                    if n.id == from.id || n.id == to.id {
                        return false;
                    }
                    let node_bottom = n.y + BOX_HEIGHT;
                    if node_bottom <= start_y || n.y >= end_y {
                        return false;
                    }
                    end_x >= n.x && end_x < n.x + n.width
                });
            Some((i, needs_lshape))
        })
        .collect();

    // Sort: straight edges first (false < true)
    edge_indices.sort_by_key(|(_, needs_lshape)| *needs_lshape);

    // Draw edges FIRST (so boxes can overwrite them)
    for (i, _) in edge_indices {
        let e = &graph.edges[i];
        let Some(from) = graph.get_node(&e.from) else {
            continue;
        };
        let Some(to) = graph.get_node(&e.to) else {
            continue;
        };

        if e.is_back_edge {
            route_back_edge(from, to, &mut canvas, &chars);
        } else {
            route_edge(from, to, i, &mut canvas, &chars, &visible_nodes);
        }
    }

    // Draw boxes AFTER edges (boxes overwrite any edges passing through them)
    for node in &visible_nodes {
        let label = truncate_label(
            &node.label,
            config.max_label_width.min(node.width.saturating_sub(4)),
        );
        draw_box(&mut canvas, node.x, node.y, node.width, &label, &chars);
    }

    Ok(canvas.to_string())
}

// ============================================================================
// Drawing Primitives
// ============================================================================

use crate::style::StyleChars;

/// Draw a box at position (x, y) with the given label.
fn draw_box(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label: &str,
    style: &StyleChars,
) {
    // Top border
    canvas.set(x, y, style.tl);
    for i in 1..width - 1 {
        canvas.set(x + i, y, style.h);
    }
    canvas.set(x + width - 1, y, style.tr);

    // Middle row with label
    canvas.set(x, y + 1, style.v);
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, style.v);

    // Bottom border
    canvas.set(x, y + 2, style.bl);
    for i in 1..width - 1 {
        canvas.set(x + i, y + 2, style.h);
    }
    canvas.set(x + width - 1, y + 2, style.br);
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::canvas::Canvas;

    #[test]
    fn test_canvas_new() {
        let canvas = Canvas::new(10, 5);
        assert_eq!(canvas.width, 10);
        assert_eq!(canvas.height, 5);
    }

    #[test]
    fn test_canvas_set_get() {
        let mut canvas = Canvas::new(10, 5);
        canvas.set(2, 3, 'X');
        assert_eq!(canvas.get(2, 3), 'X');
        assert_eq!(canvas.get(0, 0), ' ');
    }
}
