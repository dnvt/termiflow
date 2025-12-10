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
use crate::graph::{Graph, Node, NodeShape, Subgraph};
use crate::style::{
    display_width, truncate_label, BaseStyle, BOX_HEIGHT, COL_SPACING, EDGE_JUNCTION_HEIGHT,
    EDGE_STEM_HEIGHT, MAX_CANVAS_HEIGHT, MAX_CANVAS_WIDTH, RIGHT_GUTTER, ROW_SPACING,
};

use edge::{center_x as edge_center_x, route_back_edge, route_expanded_edge};
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

    // Calculate canvas size from laid-out nodes AND subgraph bounds
    let node_max_right = graph.nodes.iter().map(|n| n.x + n.width).max().unwrap_or(0);
    let node_max_bottom = graph
        .nodes
        .iter()
        .map(|n| n.y + BOX_HEIGHT)
        .max()
        .unwrap_or(0);

    // Also consider subgraph bounds (they may be wider than contained nodes for titles)
    let sg_max_right = graph
        .subgraphs
        .iter()
        .map(|s| s.bounds.x + s.bounds.width)
        .max()
        .unwrap_or(0);
    let sg_max_bottom = graph
        .subgraphs
        .iter()
        .map(|s| s.bounds.y + s.bounds.height)
        .max()
        .unwrap_or(0);

    let max_right = node_max_right.max(sg_max_right);
    let max_bottom = node_max_bottom.max(sg_max_bottom);
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

    // Compute subgraph entry points: nodes that receive edges from outside their subgraph
    // For these nodes, arrows should be drawn at subgraph.bounds.y (on the border)
    let subgraph_entry_y: HashMap<String, usize> = compute_subgraph_entry_points(graph);
    // Compute subgraph exit points: nodes that send edges to outside their subgraph
    // For these nodes, stem extends to subgraph bottom so L-turn happens outside
    let subgraph_exit_y: HashMap<String, usize> = compute_subgraph_exit_points(graph);

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
    // Split targets into internal (same subgraph) and external (cross-subgraph) groups
    // to route them correctly
    let mut source_ids: Vec<&str> = sources_with_edges.into_iter().collect();
    source_ids.sort();
    for source_id in source_ids {
        let Some(from) = graph.get_node(source_id) else {
            continue;
        };
        if let Some(targets) = edges_by_source.get_mut(source_id) {
            targets.sort_by_key(|n| (n.y, n.x, n.id.clone()));

            // Split targets: internal (same subgraph) vs external (cross-subgraph)
            let source_sg = graph.node_subgraph.get(source_id);
            let mut internal_targets: Vec<&Node> = Vec::new();
            let mut external_targets: Vec<&Node> = Vec::new();

            for target in targets.iter() {
                let target_sg = graph.node_subgraph.get(&target.id);
                if source_sg == target_sg {
                    // Same subgraph (or both global)
                    internal_targets.push(target);
                } else {
                    // Different subgraphs
                    external_targets.push(target);
                }
            }

            // Route internal edges first (they stay within the subgraph)
            if !internal_targets.is_empty() {
                route_expanded_edge(from, &internal_targets, &mut canvas, &chars, &subgraph_entry_y, &subgraph_exit_y);
            }

            // Route external edges separately (they cross subgraph boundaries)
            // For external edges, we need to handle the case where the stem would pass
            // through other nodes in the same subgraph. Instead of drawing a separate
            // stem, we route the horizontal junction from the subgraph exit point directly.
            if !external_targets.is_empty() {
                // Check if this source also has internal targets
                // If so, the stem is already drawn by internal edges
                let has_internal = !internal_targets.is_empty();
                if has_internal && subgraph_exit_y.contains_key(source_id) {
                    // External edges from a node that also has internal targets
                    // Route from the subgraph exit point instead of drawing duplicate stem
                    route_external_edge_from_exit(from, &external_targets, &mut canvas, &chars, &subgraph_entry_y, &subgraph_exit_y, graph);
                } else {
                    route_expanded_edge(from, &external_targets, &mut canvas, &chars, &subgraph_entry_y, &subgraph_exit_y);
                }
            }
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

    // Draw subgraph boundaries (if enabled and present)
    if config.enable_subgraphs {
        for subgraph in &graph.subgraphs {
            draw_subgraph(&mut canvas, subgraph, &chars);
        }
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

/// Draw a subgraph boundary box with optional title.
///
/// The subgraph is rendered with:
/// - A dashed/dotted border to differentiate from node boxes
/// - Title on a dedicated line at the top (if provided)
/// - Edge-aware rendering that preserves crossing edges
fn draw_subgraph(canvas: &mut Canvas, subgraph: &Subgraph, style: &StyleChars) {
    let crate::graph::Rectangle { x, y, width, height } = subgraph.bounds;

    // CRITICAL: Validate bounds to prevent panic on empty/degenerate subgraphs
    if width < 3 || height < 3 {
        return; // Skip drawing degenerate subgraphs
    }

    // Skip if subgraph is outside visible area
    if x >= canvas.width || y >= canvas.height {
        return;
    }

    let actual_width = width.min(canvas.width.saturating_sub(x));
    let actual_height = height.min(canvas.height.saturating_sub(y));

    // Need at least 3x3 to draw anything meaningful
    if actual_width < 3 || actual_height < 3 {
        return;
    }

    // Use dashed/dotted style for subgraph borders
    // This creates visual hierarchy: solid borders for nodes, dashed for containers
    let is_unicode = style.tl == '┌' || style.tl == '╭' || style.tl == '╔' || style.tl == '┏';
    let (sg_tl, sg_tr, sg_bl, sg_br, sg_h, sg_v) = if is_unicode {
        // Unicode: use dashed box drawing characters for container distinction
        ('┌', '┐', '└', '┘', '╌', '╎')
    } else {
        // ASCII: use colon for vertical borders (dotted pattern)
        ('+', '+', '+', '+', '-', ':')
    };

    // Draw top border (edge-aware: preserve edge characters with gaps for clean crossings)
    let top_left_char = canvas.get(x, y);
    if !is_edge_char(top_left_char, style) {
        canvas.set(x, y, sg_tl);
    }
    // First pass: identify edge crossing positions
    let mut edge_positions: Vec<usize> = Vec::new();
    for i in 1..actual_width.saturating_sub(1) {
        let pos = x + i;
        let char_at = canvas.get(pos, y);
        if is_edge_char(char_at, style) {
            edge_positions.push(pos);
        }
    }
    // Second pass: draw border with gaps around edge crossings
    for i in 1..actual_width.saturating_sub(1) {
        let pos = x + i;
        let char_at = canvas.get(pos, y);
        if is_edge_char(char_at, style) {
            // Keep edge char, but ensure spaces around it for clean visual
            // (spaces are set in third pass below)
        } else {
            // Check if this position is adjacent to an edge crossing
            let near_edge = edge_positions.iter().any(|&ep| {
                pos == ep.saturating_sub(1) || pos == ep + 1
            });
            if near_edge {
                canvas.set(pos, y, ' '); // Gap around edge crossing
            } else {
                canvas.set(pos, y, sg_h);
            }
        }
    }
    if actual_width > 1 {
        let top_right_char = canvas.get(x + actual_width - 1, y);
        if !is_edge_char(top_right_char, style) {
            canvas.set(x + actual_width - 1, y, sg_tr);
        }
    }

    // Draw title on second line if present (dedicated title row)
    let content_start_y = if let Some(ref title) = subgraph.title {
        if actual_height > 3 {
            // Truncate title if too long
            let max_title_len = actual_width.saturating_sub(4);
            let display_title = if display_width(title) > max_title_len {
                let mut truncated = String::new();
                let mut w = 0;
                for c in title.chars() {
                    let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                    if w + cw > max_title_len.saturating_sub(1) {
                        truncated.push('…');
                        break;
                    }
                    truncated.push(c);
                    w += cw;
                }
                truncated
            } else {
                title.clone()
            };

            // Draw title row: | Title | (edge-aware: preserve edge crossings)
            let left_char = canvas.get(x, y + 1);
            if !is_edge_char(left_char, style) {
                canvas.set(x, y + 1, sg_v);
            }
            let title_width = display_width(&display_title);
            let padding_left = (actual_width.saturating_sub(2).saturating_sub(title_width)) / 2;

            // Fill with spaces first, but preserve edge characters
            for i in 1..actual_width.saturating_sub(1) {
                let char_at = canvas.get(x + i, y + 1);
                if !is_edge_char(char_at, style) {
                    canvas.set(x + i, y + 1, ' ');
                }
            }

            // Draw centered title, but skip positions with edge characters
            let mut pos = x + 1 + padding_left;
            for c in display_title.chars() {
                if pos < x + actual_width.saturating_sub(1) {
                    let char_at = canvas.get(pos, y + 1);
                    if !is_edge_char(char_at, style) {
                        canvas.set(pos, y + 1, c);
                    }
                }
                pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }
            let right_char = canvas.get(x + actual_width - 1, y + 1);
            if !is_edge_char(right_char, style) {
                canvas.set(x + actual_width - 1, y + 1, sg_v);
            }

            // Draw padding row (edge-aware: preserve edge crossings)
            let left_char = canvas.get(x, y + 2);
            if !is_edge_char(left_char, style) {
                canvas.set(x, y + 2, sg_v);
            }
            for i in 1..actual_width.saturating_sub(1) {
                let char_at = canvas.get(x + i, y + 2);
                if !is_edge_char(char_at, style) {
                    canvas.set(x + i, y + 2, ' ');
                }
            }
            let right_char = canvas.get(x + actual_width - 1, y + 2);
            if !is_edge_char(right_char, style) {
                canvas.set(x + actual_width - 1, y + 2, sg_v);
            }

            3 // Content starts at row 3 (after title + padding)
        } else {
            1 // Not enough space for title row
        }
    } else {
        1 // No title, content starts at row 1
    };

    // Draw vertical borders (edge-aware: preserve existing edge characters)
    for row in content_start_y..actual_height.saturating_sub(1) {
        let left_pos = (x, y + row);
        let right_pos = (x + actual_width - 1, y + row);

        // Left border: only draw if no edge crossing
        let left_char = canvas.get(left_pos.0, left_pos.1);
        if !is_edge_char(left_char, style) {
            canvas.set(left_pos.0, left_pos.1, sg_v);
        }

        // Right border: only draw if no edge crossing
        let right_char = canvas.get(right_pos.0, right_pos.1);
        if !is_edge_char(right_char, style) {
            canvas.set(right_pos.0, right_pos.1, sg_v);
        }
    }

    // Draw bottom border (edge-aware with gaps around crossings)
    let bottom_y = y + actual_height - 1;
    let bottom_left_char = canvas.get(x, bottom_y);
    if !is_edge_char(bottom_left_char, style) {
        canvas.set(x, bottom_y, sg_bl);
    }
    // First pass: identify edge crossing positions on bottom border
    let mut bottom_edge_positions: Vec<usize> = Vec::new();
    for i in 1..actual_width.saturating_sub(1) {
        let pos = x + i;
        let char_at = canvas.get(pos, bottom_y);
        if is_edge_char(char_at, style) {
            bottom_edge_positions.push(pos);
        }
    }
    // Second pass: draw border with gaps around edge crossings
    for i in 1..actual_width.saturating_sub(1) {
        let pos = x + i;
        let char_at = canvas.get(pos, bottom_y);
        if is_edge_char(char_at, style) {
            // Keep edge char
        } else {
            // Check if this position is adjacent to an edge crossing
            let near_edge = bottom_edge_positions.iter().any(|&ep| {
                pos == ep.saturating_sub(1) || pos == ep + 1
            });
            if near_edge {
                canvas.set(pos, bottom_y, ' '); // Gap around edge crossing
            } else {
                canvas.set(pos, bottom_y, sg_h);
            }
        }
    }
    if actual_width > 1 {
        let bottom_right_char = canvas.get(x + actual_width - 1, bottom_y);
        if !is_edge_char(bottom_right_char, style) {
            canvas.set(x + actual_width - 1, bottom_y, sg_br);
        }
    }
}

/// Check if a character is part of an edge (should not be overwritten by subgraph border)
fn is_edge_char(c: char, style: &StyleChars) -> bool {
    // Check against all edge-related characters
    c == style.edge_v
        || c == style.edge_h
        || c == style.arrow_down
        || c == style.arrow_up
        || c == style.arrow_left
        || c == style.arrow_right
        || c == style.corner_dr
        || c == style.corner_dl
        || c == style.corner_ur
        || c == style.corner_ul
        || c == style.cross
        || c == style.junction_down
        || c == style.junction_up
        || c == style.junction_left
        || c == style.junction_right
        || c == '│'
        || c == '─'
        || c == '|'
        || c == '-'
        || c == '↓'
        || c == '↑'
        || c == 'v'
        || c == '^'
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

/// Compute entry points for subgraphs: nodes that receive edges from outside their subgraph.
///
/// For these nodes, arrows should be drawn at subgraph.bounds.y (on the border)
/// instead of at target.y - 1 (just above the node).
fn compute_subgraph_entry_points(graph: &Graph) -> HashMap<String, usize> {
    let mut entry_points: HashMap<String, usize> = HashMap::new();

    // For each subgraph, find nodes that receive edges from outside
    for subgraph in &graph.subgraphs {
        if subgraph.node_ids.is_empty() || subgraph.bounds.height == 0 {
            continue;
        }

        // Find nodes in this subgraph that receive edges from nodes outside it
        for edge in &graph.edges {
            if edge.is_back_edge {
                continue;
            }

            let target_in_subgraph = subgraph.node_ids.contains(&edge.to);
            let source_in_subgraph = subgraph.node_ids.contains(&edge.from);

            // Edge crosses into this subgraph from outside
            if target_in_subgraph && !source_in_subgraph {
                // Arrow should be at subgraph.bounds.y (on the border, which is edge-aware)
                let arrow_y = subgraph.bounds.y;
                entry_points.insert(edge.to.clone(), arrow_y);
            }
        }
    }

    entry_points
}

/// Compute exit points for subgraphs: nodes that send edges to outside their subgraph.
///
/// For these nodes, the vertical edge should extend to the subgraph bottom border
/// so the L-turn happens OUTSIDE the subgraph.
fn compute_subgraph_exit_points(graph: &Graph) -> HashMap<String, usize> {
    let mut exit_points: HashMap<String, usize> = HashMap::new();

    for subgraph in &graph.subgraphs {
        if subgraph.node_ids.is_empty() || subgraph.bounds.height == 0 {
            continue;
        }

        for edge in &graph.edges {
            if edge.is_back_edge {
                continue;
            }

            let source_in_subgraph = subgraph.node_ids.contains(&edge.from);
            let target_in_subgraph = subgraph.node_ids.contains(&edge.to);

            // Edge exits this subgraph
            if source_in_subgraph && !target_in_subgraph {
                // Exit point is at subgraph bottom border
                let exit_y = subgraph.bounds.y + subgraph.bounds.height - 1;
                exit_points.insert(edge.from.clone(), exit_y);
            }
        }
    }

    exit_points
}

/// Route external edges from a node that also has internal targets.
/// Instead of drawing from the node center (which would pass through internal targets),
/// we route the edges along the subgraph boundary.
fn route_external_edge_from_exit(
    from: &crate::graph::Node,
    to_nodes: &[&crate::graph::Node],
    canvas: &mut Canvas,
    style: &crate::style::StyleChars,
    subgraph_entry_y: &HashMap<String, usize>,
    subgraph_exit_y: &HashMap<String, usize>,
    graph: &crate::graph::Graph,
) {
    if to_nodes.is_empty() || !canvas.is_visible(from) {
        return;
    }

    // Find the source's subgraph to get its bounds
    let source_sg = graph.node_subgraph.get(&from.id);
    let sg_bounds = source_sg.and_then(|sg_id| {
        graph.subgraphs.iter().find(|s| &s.id == sg_id).map(|s| &s.bounds)
    });

    // Get the exit Y for the source
    let exit_y = subgraph_exit_y.get(&from.id).copied();

    // If we have subgraph bounds and exit point, route external edges
    // Since this source also has internal targets, we can't draw a stem from the source
    // (it would pass through the internal targets). Instead, we only draw the drops and
    // arrows to external targets. The horizontal junction will be connected by sibling
    // node edges that also exit the subgraph.
    if let (Some(_bounds), Some(exit_y)) = (sg_bounds, exit_y) {
        let junction_y = exit_y + 1;

        // Get destination centers, sorted left to right
        let visible_targets: Vec<&&crate::graph::Node> = to_nodes.iter()
            .filter(|n| canvas.is_visible(n))
            .collect();

        if visible_targets.is_empty() {
            return;
        }

        // Only draw drops and arrows - don't draw horizontal junction here
        // The sibling nodes' edge routing will draw the horizontal junction
        // We just need to ensure drops reach the external targets
        for target in &visible_targets {
            let dest_x = edge_center_x(target);
            let target_arrow_y = subgraph_entry_y
                .get(&target.id)
                .copied()
                .unwrap_or_else(|| target.y.saturating_sub(1));

            // Place junction marker at drop point
            // This will be merged with the horizontal junction when it's drawn
            canvas.set_edge_char(dest_x, junction_y, style.junction_up, style);

            // Draw vertical drop from junction to arrow
            for y in (junction_y + 1)..target_arrow_y {
                canvas.set_edge_char(dest_x, y, style.edge_v, style);
            }
            // Arrow
            canvas.set(dest_x, target_arrow_y, style.arrow_down);
        }
    } else {
        // Fall back to normal routing
        route_expanded_edge(from, to_nodes, canvas, style, subgraph_entry_y, subgraph_exit_y);
    }
}
