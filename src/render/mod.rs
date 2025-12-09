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
use canvas::is_vertical;
pub use canvas::Canvas;

use anyhow::Result;

use crate::config::Config;
use crate::graph::{Graph, Node, NodeShape};
use crate::style::{
    display_width, truncate_label, BaseStyle, BOX_HEIGHT, COL_SPACING, EDGE_JUNCTION_HEIGHT,
    EDGE_STEM_HEIGHT, MAX_CANVAS_HEIGHT, MAX_CANVAS_WIDTH, RIGHT_GUTTER, ROW_SPACING,
};

use edge::{route_back_edge, route_expanded_edge};
use std::collections::{HashMap, HashSet};

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
    let max_bottom = graph
        .nodes
        .iter()
        .map(|n| n.y + BOX_HEIGHT)
        .max()
        .unwrap_or(0);
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
    let chars = config
        .composite_style
        .to_style_chars(BaseStyle::default());

    // Get visible nodes
    let visible_nodes: Vec<&Node> = graph
        .nodes
        .iter()
        .filter(|n| canvas.is_visible(n))
        .collect();

    // Group forward edges by source node for expanded routing
    let mut edges_by_source: HashMap<&str, Vec<&Node>> = HashMap::new();
    let mut back_edges: Vec<(&Node, &Node)> = Vec::new();
    let mut sources_with_edges: HashSet<&str> = HashSet::new();
    // Track labeled edges for later rendering: (from_node, to_node, label)
    let mut labeled_edges: Vec<(&Node, &Node, &str)> = Vec::new();

    // First pass: group edges by source
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
            edges_by_source.entry(&e.from).or_default().push(to);
            sources_with_edges.insert(&e.from);

            // Track edges with labels
            if let Some(ref label) = e.label {
                labeled_edges.push((from, to, label.as_str()));
            }
        }
    }

    // Draw forward edges using expanded routing (grouped by source), deterministic order
    let mut source_ids: Vec<&str> = sources_with_edges.into_iter().collect();
    source_ids.sort();
    for source_id in source_ids {
        let Some(from) = graph.get_node(source_id) else {
            continue;
        };
        if let Some(targets) = edges_by_source.get_mut(source_id) {
            targets.sort_by_key(|n| (n.y, n.x, n.id.clone()));
            let target_refs: Vec<&Node> = targets.to_vec();
            route_expanded_edge(from, &target_refs, &mut canvas, &chars);
        }
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
        draw_node(
            &mut canvas,
            node.x,
            node.y,
            node.width,
            &label,
            node.shape,
            &chars,
        );
    }

    Ok(canvas.to_string())
}

// ============================================================================
// Drawing Primitives
// ============================================================================

use crate::style::StyleChars;

/// Draw a node at position (x, y) with the given label and shape.
fn draw_node(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label: &str,
    shape: NodeShape,
    style: &StyleChars,
) {
    match shape {
        NodeShape::Rectangle => draw_rectangle(canvas, x, y, width, label, style),
        NodeShape::Rounded => draw_rounded(canvas, x, y, width, label, style),
        NodeShape::Diamond => draw_diamond(canvas, x, y, width, label, style),
        NodeShape::Circle => draw_circle(canvas, x, y, width, label, style),
        NodeShape::Stadium => draw_stadium(canvas, x, y, width, label, style),
        NodeShape::Hexagon => draw_hexagon(canvas, x, y, width, label, style),
        NodeShape::Database => draw_database(canvas, x, y, width, label, style),
        NodeShape::Subroutine => draw_subroutine(canvas, x, y, width, label, style),
        NodeShape::Asymmetric => draw_asymmetric(canvas, x, y, width, label, style),
        // Parallelogram and trapezoid fall back to rectangle for now
        NodeShape::Parallelogram
        | NodeShape::ParallelogramAlt
        | NodeShape::Trapezoid
        | NodeShape::TrapezoidAlt => draw_rectangle(canvas, x, y, width, label, style),
    }
}

/// Draw a rectangle box at position (x, y) with the given label.
fn draw_rectangle(
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
            style.junction_down // T-junction pointing down where edges exit
        } else {
            style.h
        };
        canvas.set(pos_x, y + 2, c);
    }
    canvas.set(x + width - 1, y + 2, style.br);
}

/// Draw a rounded box (uses round corner characters).
fn draw_rounded(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label: &str,
    style: &StyleChars,
) {
    // Use round corners: ╭ ╮ ╰ ╯ for unicode, ( ) for ascii
    let (tl, tr, bl, br) = if style.tl == '┌' {
        ('╭', '╮', '╰', '╯')
    } else {
        ('(', ')', '(', ')')
    };

    canvas.set(x, y, tl);
    for i in 1..width - 1 {
        canvas.set(x + i, y, style.h);
    }
    canvas.set(x + width - 1, y, tr);

    canvas.set(x, y + 1, style.v);
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, style.v);

    canvas.set(x, y + 2, bl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        let below = canvas.get(pos_x, y + 3);
        let c = if is_vertical(below, style) {
            style.junction_down
        } else {
            style.h
        };
        canvas.set(pos_x, y + 2, c);
    }
    canvas.set(x + width - 1, y + 2, br);
}

/// Draw a diamond/rhombus shape.
fn draw_diamond(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label: &str,
    style: &StyleChars,
) {
    let center = x + width / 2;
    let is_unicode = style.tl == '┌';
    let point_char = if is_unicode { '◇' } else { 'v' };

    // Top point
    canvas.set(center, y, if is_unicode { '◇' } else { '^' });

    // Middle row with label
    canvas.set(x, y + 1, '<');
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, '>');

    // Bottom point - check for edge exits
    let below = canvas.get(center, y + 3);
    let bottom_char = if is_vertical(below, style) {
        style.junction_down
    } else {
        point_char
    };
    canvas.set(center, y + 2, bottom_char);

    // Fill sides
    for i in 1..center - x {
        canvas.set(x + i, y + 2, ' ');
        canvas.set(center + i, y + 2, ' ');
    }
}

/// Draw a circle shape (elliptical approximation).
fn draw_circle(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label: &str,
    style: &StyleChars,
) {
    let is_unicode = style.tl == '┌';
    let (tl, tr, bl, br, h) = if is_unicode {
        ('╭', '╮', '╰', '╯', '─')
    } else {
        ('/', '\\', '\\', '/', '-')
    };

    canvas.set(x, y, tl);
    for i in 1..width - 1 {
        canvas.set(x + i, y, h);
    }
    canvas.set(x + width - 1, y, tr);

    canvas.set(x, y + 1, '(');
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, ')');

    canvas.set(x, y + 2, bl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        let below = canvas.get(pos_x, y + 3);
        let c = if is_vertical(below, style) {
            style.junction_down
        } else {
            h
        };
        canvas.set(pos_x, y + 2, c);
    }
    canvas.set(x + width - 1, y + 2, br);
}

/// Draw a stadium/pill shape.
fn draw_stadium(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label: &str,
    style: &StyleChars,
) {
    canvas.set(x, y, style.tl);
    for i in 1..width - 1 {
        canvas.set(x + i, y, style.h);
    }
    canvas.set(x + width - 1, y, style.tr);

    canvas.set(x, y + 1, '(');
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, ')');

    canvas.set(x, y + 2, style.bl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        let below = canvas.get(pos_x, y + 3);
        let c = if is_vertical(below, style) {
            style.junction_down
        } else {
            style.h
        };
        canvas.set(pos_x, y + 2, c);
    }
    canvas.set(x + width - 1, y + 2, style.br);
}

/// Draw a hexagon shape.
fn draw_hexagon(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label: &str,
    style: &StyleChars,
) {
    canvas.set(x, y, '/');
    for i in 1..width - 1 {
        canvas.set(x + i, y, style.h);
    }
    canvas.set(x + width - 1, y, '\\');

    canvas.set(x, y + 1, '<');
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, '>');

    canvas.set(x, y + 2, '\\');
    for i in 1..width - 1 {
        let pos_x = x + i;
        let below = canvas.get(pos_x, y + 3);
        let c = if is_vertical(below, style) {
            style.junction_down
        } else {
            style.h
        };
        canvas.set(pos_x, y + 2, c);
    }
    canvas.set(x + width - 1, y + 2, '/');
}

/// Draw a database/cylinder shape.
fn draw_database(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label: &str,
    style: &StyleChars,
) {
    let is_unicode = style.tl == '┌';
    let h = if is_unicode { '─' } else { '-' };

    canvas.set(x, y, '/');
    for i in 1..width - 1 {
        canvas.set(x + i, y, h);
    }
    canvas.set(x + width - 1, y, '\\');

    canvas.set(x, y + 1, style.v);
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, style.v);

    canvas.set(x, y + 2, '\\');
    for i in 1..width - 1 {
        let pos_x = x + i;
        let below = canvas.get(pos_x, y + 3);
        let c = if is_vertical(below, style) {
            style.junction_down
        } else {
            h
        };
        canvas.set(pos_x, y + 2, c);
    }
    canvas.set(x + width - 1, y + 2, '/');
}

/// Draw a subroutine box (double vertical lines on sides).
fn draw_subroutine(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label: &str,
    style: &StyleChars,
) {
    let dv = if style.tl == '┌' { '║' } else { '|' };

    canvas.set(x, y, style.tl);
    for i in 1..width - 1 {
        canvas.set(x + i, y, style.h);
    }
    canvas.set(x + width - 1, y, style.tr);

    canvas.set(x, y + 1, dv);
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, dv);

    canvas.set(x, y + 2, style.bl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        let below = canvas.get(pos_x, y + 3);
        let c = if is_vertical(below, style) {
            style.junction_down
        } else {
            style.h
        };
        canvas.set(pos_x, y + 2, c);
    }
    canvas.set(x + width - 1, y + 2, style.br);
}

/// Draw an asymmetric/flag shape.
fn draw_asymmetric(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label: &str,
    style: &StyleChars,
) {
    canvas.set(x, y, '>');
    for i in 1..width - 1 {
        canvas.set(x + i, y, style.h);
    }
    canvas.set(x + width - 1, y, style.tr);

    canvas.set(x, y + 1, ' ');
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, style.v);

    canvas.set(x, y + 2, '>');
    for i in 1..width - 1 {
        let pos_x = x + i;
        let below = canvas.get(pos_x, y + 3);
        let c = if is_vertical(below, style) {
            style.junction_down
        } else {
            style.h
        };
        canvas.set(pos_x, y + 2, c);
    }
    canvas.set(x + width - 1, y + 2, style.br);
}

/// Draw an edge label on the vertical segment between two nodes.
fn draw_edge_label(canvas: &mut Canvas, from: &Node, to: &Node, label: &str) {
    use edge::center_x;

    // Calculate the vertical segment position (where the label will go)
    let src_center_x = center_x(from);
    let edge_x = center_x(to);

    // Calculate edge span
    let stem_start_y = from.y + BOX_HEIGHT;
    let arrow_y = to.y.saturating_sub(1);

    // For straight edges (aligned), place label in middle of vertical span
    // For L-shaped edges, place after junction
    let label_y = if src_center_x == edge_x {
        // Straight edge: place label in middle of vertical span
        // Leave room for arrow at bottom
        stem_start_y + (arrow_y.saturating_sub(stem_start_y)) / 2
    } else {
        // L-shaped: use junction-based positioning
        let junction_y = stem_start_y + EDGE_STEM_HEIGHT;
        junction_y + EDGE_JUNCTION_HEIGHT
    };

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
