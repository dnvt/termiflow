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
use canvas::is_vertical;

use anyhow::Result;

use crate::config::Config;
use crate::graph::{Graph, Node};
use crate::style::{
    display_width, truncate_label, BorderStyle, BOX_HEIGHT, COL_SPACING, EDGE_JUNCTION_HEIGHT,
    EDGE_STEM_HEIGHT, MAX_CANVAS_HEIGHT, MAX_CANVAS_WIDTH, RIGHT_GUTTER, ROW_SPACING,
};

use edge::{route_back_edge, route_expanded_edge};
use std::collections::HashMap;

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

    // Group forward edges by source node for expanded routing
    let mut edges_by_source: HashMap<&str, Vec<&Node>> = HashMap::new();
    let mut back_edges: Vec<(&Node, &Node)> = Vec::new();
    // Track labeled edges for later rendering: (from_node, to_node, label)
    let mut labeled_edges: Vec<(&Node, &Node, &str)> = Vec::new();

    for e in &graph.edges {
        let Some(from) = graph.get_node(&e.from) else {
            continue;
        };
        let Some(to) = graph.get_node(&e.to) else {
            continue;
        };

        if e.is_back_edge {
            back_edges.push((from, to));
        } else if canvas.is_visible(from) && canvas.is_visible(to) {
            edges_by_source
                .entry(&e.from)
                .or_default()
                .push(to);

            // Track edges with labels
            if let Some(ref label) = e.label {
                labeled_edges.push((from, to, label.as_str()));
            }
        }
    }

    // Draw forward edges using expanded routing (grouped by source)
    for (source_id, targets) in &edges_by_source {
        let Some(from) = graph.get_node(source_id) else {
            continue;
        };

        // Convert Vec<&Node> to Vec<&&Node> for route_expanded_edge
        let target_refs: Vec<&Node> = targets.iter().copied().collect();
        route_expanded_edge(from, &target_refs, &mut canvas, &chars);
    }

    // Draw back-edges (cycle edges)
    for (from, to) in back_edges {
        route_back_edge(from, to, &mut canvas, &chars);
    }

    // Draw edge labels on the vertical segments
    for (from, to, label) in &labeled_edges {
        draw_edge_label(&mut canvas, from, to, label);
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

    // Bottom border - check for edge exits and place junctions
    canvas.set(x, y + 2, style.bl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        // Check if there's a vertical edge below this position
        let below = canvas.get(pos_x, y + 3);
        let c = if is_vertical(below, style) {
            style.junction_down // ┬ or + where edge exits
        } else {
            style.h
        };
        canvas.set(pos_x, y + 2, c);
    }
    canvas.set(x + width - 1, y + 2, style.br);
}

/// Draw an edge label on the vertical segment between source and target.
///
/// Labels are positioned on the vertical drop segment, centered horizontally
/// around the edge path. The label appears above the target box.
fn draw_edge_label(canvas: &mut Canvas, from: &Node, to: &Node, label: &str) {
    use edge::center_x;

    // Calculate the vertical segment position (where the label will go)
    // The edge drops to the target's center_x, so that's where we place the label
    let edge_x = center_x(to);

    // Calculate label y position - on the row between junction and arrow
    // Layout: stem -> junction -> (label here) -> arrow -> target box
    let junction_y = from.y + BOX_HEIGHT + EDGE_STEM_HEIGHT;
    let label_y = junction_y + EDGE_JUNCTION_HEIGHT; // Row after junction

    // Truncate label if too long
    let max_label_len = 12; // Keep labels reasonably short
    let display_label = if display_width(label) > max_label_len {
        let mut truncated = String::new();
        let mut width = 0;
        for c in label.chars() {
            let char_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            if width + char_width > max_label_len - 1 {
                truncated.push('…');
                break;
            }
            truncated.push(c);
            width += char_width;
        }
        truncated
    } else {
        label.to_string()
    };

    let label_width = display_width(&display_label);

    // Center the label around the edge position
    let label_start_x = edge_x.saturating_sub(label_width / 2);

    // Draw the label characters
    let mut x_pos = label_start_x;
    for c in display_label.chars() {
        if x_pos < canvas.width {
            canvas.set(x_pos, label_y, c);
        }
        x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
    }
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
