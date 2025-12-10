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
pub mod edge_unified;

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

use edge::route_back_edge;
use edge_unified::{route_divergent_edges, route_convergent_edges};
use std::collections::{HashMap, HashSet};
use crate::graph::Direction;

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

    // Add gutter space for back-edges:
    // - TD/BT: right gutter (add to width)
    // - LR/RL: bottom gutter (add to height)
    let is_horizontal = matches!(graph.direction, Direction::LR | Direction::RL);
    let width_gutter = if graph.has_cycles() && !is_horizontal { RIGHT_GUTTER } else { 0 };
    let height_gutter = if graph.has_cycles() && is_horizontal { RIGHT_GUTTER } else { 0 };

    let mut width = max_right + COL_SPACING + width_gutter;
    if width > MAX_CANVAS_WIDTH {
        width = MAX_CANVAS_WIDTH;
        eprintln!(
            "termiflow: warning: Graph too wide ({} chars), clipping to {}",
            max_right + COL_SPACING + width_gutter,
            MAX_CANVAS_WIDTH
        );
    }
    width = width
        .max(max_right.saturating_add(1).min(MAX_CANVAS_WIDTH))
        .max(1);

    let mut height = max_bottom + ROW_SPACING + height_gutter;
    if height > MAX_CANVAS_HEIGHT {
        height = MAX_CANVAS_HEIGHT;
        eprintln!(
            "termiflow: warning: Graph too tall ({} rows), clipping to {}",
            max_bottom + ROW_SPACING + height_gutter,
            MAX_CANVAS_HEIGHT
        );
    }
    height = height
        .max(max_bottom.saturating_add(1).min(MAX_CANVAS_HEIGHT))
        .max(1);

    if graph.has_cycles() && !is_horizontal && width <= RIGHT_GUTTER {
        eprintln!("termiflow: warning: Back-edges skipped (gutter clipped)");
    }
    if graph.has_cycles() && is_horizontal && height <= RIGHT_GUTTER {
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

    // Group edges by target for convergence handling
    let mut edges_by_target: HashMap<&str, Vec<&Node>> = HashMap::new();
    for e in &graph.edges {
        if e.is_back_edge { continue; }
        let Some(from) = graph.get_node(&e.from) else { continue; };
        let Some(to) = graph.get_node(&e.to) else { continue; };
        if canvas.is_visible(from) && canvas.is_visible(to) {
            edges_by_target.entry(&e.to).or_default().push(from);
        }
    }

    // Identify convergent labeled edges (those going to targets with multiple sources)
    let convergent_targets: HashSet<&str> = edges_by_target
        .iter()
        .filter(|(_, sources)| sources.len() > 1)
        .map(|(target, _)| *target)
        .collect();

    // Separate labeled edges into convergent and regular
    let (convergent_labels, regular_labels): (Vec<_>, Vec<_>) = labeled_edges
        .iter()
        .partition(|(_, to, _)| convergent_targets.contains(to.id.as_str()));
    
    // Process edges: prioritize convergence (multiple sources to one target)
    let mut processed_edges: HashSet<(&str, &str)> = HashSet::new();
    
    // First, handle convergence cases (multiple sources → one target)
    for (target_id, sources) in &edges_by_target {
        if sources.len() > 1 {
            let Some(target) = graph.get_node(target_id) else { continue; };
            let mut source_refs: Vec<&Node> = sources.clone();
            source_refs.sort_by_key(|n| (n.y, n.x, n.id.clone()));
            route_convergent_edges(&source_refs, target, &mut canvas, &chars, graph.direction);
            for source in sources {
                processed_edges.insert((&source.id, target_id));
            }
        }
    }
    
    // Then, handle remaining divergence cases (one source → multiple targets)
    let mut source_ids: Vec<&str> = sources_with_edges.into_iter().collect();
    source_ids.sort();
    for source_id in source_ids {
        let Some(from) = graph.get_node(source_id) else { continue; };
        if let Some(targets) = edges_by_source.get_mut(source_id) {
            // Filter out already processed edges
            let unprocessed: Vec<&Node> = targets.iter()
                .filter(|t| !processed_edges.contains(&(source_id, t.id.as_str())))
                .copied()
                .collect();
            
            if !unprocessed.is_empty() {
                let mut target_refs: Vec<&Node> = unprocessed;
                target_refs.sort_by_key(|n| (n.y, n.x, n.id.clone()));
                route_divergent_edges(from, &target_refs, &mut canvas, &chars, graph.direction);
            }
        }
    }

    // Draw back-edges (cycle edges)
    for (from, to) in back_edges {
        route_back_edge(from, to, &mut canvas, &chars, graph.direction);
    }

    // Draw edge labels on the appropriate segments (vertical for TD/BT, horizontal for LR/RL)
    // Regular (non-convergent) labels
    for (from, to, label) in regular_labels {
        draw_edge_label(&mut canvas, from, to, label, graph.direction, &chars);
    }
    // Convergent edge labels - draw on the vertical drop from each source
    for (from, to, label) in convergent_labels {
        draw_convergent_edge_label(&mut canvas, from, to, label, graph.direction);
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
            graph.direction,
        );
    }

    // Draw junction characters AFTER boxes (for LR/RL edge connections)
    // Shows where edges exit source boxes
    if matches!(graph.direction, Direction::LR | Direction::RL) {
        use edge::center_y;
        for (&source_id, _targets) in &edges_by_source {
            if let Some(from) = graph.get_node(source_id) {
                if canvas.is_visible(from) {
                    let junction_y = center_y(from);
                    // For LR: junction on right side of source box
                    // For RL: junction on left side of source box
                    let junction_x = if graph.direction == Direction::LR {
                        from.x + from.width - 1 // Right edge of box
                    } else {
                        from.x // Left edge of box
                    };
                    let junction_char = if graph.direction == Direction::LR {
                        chars.junction_right // ├
                    } else {
                        chars.junction_left // ┤
                    };
                    if junction_x < canvas.width && junction_y < canvas.height {
                        canvas.set(junction_x, junction_y, junction_char);
                    }
                }
            }
        }
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
    direction: Direction,
) {
    match shape {
        NodeShape::Rectangle => draw_rectangle(canvas, x, y, width, label, style, direction),
        NodeShape::Rounded => draw_rounded(canvas, x, y, width, label, style, direction),
        NodeShape::Diamond => draw_diamond(canvas, x, y, width, label, style),
        NodeShape::Circle => draw_circle(canvas, x, y, width, label, style),
        NodeShape::Stadium => draw_stadium(canvas, x, y, width, label, style, direction),
        NodeShape::Hexagon => draw_hexagon(canvas, x, y, width, label, style, direction),
        NodeShape::Database => draw_database(canvas, x, y, width, label, style, direction),
        NodeShape::Subroutine => draw_subroutine(canvas, x, y, width, label, style, direction),
        NodeShape::Asymmetric => draw_asymmetric(canvas, x, y, width, label, style, direction),
        // Parallelogram and trapezoid fall back to rectangle for now
        NodeShape::Parallelogram
        | NodeShape::ParallelogramAlt
        | NodeShape::Trapezoid
        | NodeShape::TrapezoidAlt => draw_rectangle(canvas, x, y, width, label, style, direction),
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
    direction: Direction,
) {
    // Top border - check for edge exits above (BT direction only)
    canvas.set(x, y, style.tl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        // Only check above in BT direction (edges exit upward)
        let c = if direction == Direction::BT {
            let above = if y > 0 { canvas.get(pos_x, y - 1) } else { ' ' };
            if is_vertical(above, style) {
                style.junction_up // T-junction pointing up where edges exit (BT)
            } else {
                style.h
            }
        } else {
            style.h
        };
        canvas.set(pos_x, y, c);
    }
    canvas.set(x + width - 1, y, style.tr);

    // Middle row with label
    canvas.set(x, y + 1, style.v);
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, style.v);

    // Bottom border - check for edge exits below (TD/TB direction only)
    canvas.set(x, y + 2, style.bl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        // Only check below in TD/TB direction (edges exit downward)
        let c = if matches!(direction, Direction::TD | Direction::TB) {
            let below = canvas.get(pos_x, y + 3);
            if is_vertical(below, style) {
                style.junction_down // T-junction pointing down where edges exit
            } else {
                style.h
            }
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
    direction: Direction,
) {
    // Use round corners: ╭ ╮ ╰ ╯ for unicode, ( ) for ascii
    let (tl, tr, bl, br) = if style.tl == '┌' {
        ('╭', '╮', '╰', '╯')
    } else {
        ('(', ')', '(', ')')
    };

    // Top border - check for edge exits above (BT direction only)
    canvas.set(x, y, tl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if direction == Direction::BT {
            let above = if y > 0 { canvas.get(pos_x, y - 1) } else { ' ' };
            if is_vertical(above, style) {
                style.junction_up // T-junction pointing up where edges exit (BT)
            } else {
                style.h
            }
        } else {
            style.h
        };
        canvas.set(pos_x, y, c);
    }
    canvas.set(x + width - 1, y, tr);

    canvas.set(x, y + 1, style.v);
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, style.v);

    // Bottom border - check for edge exits below (TD/TB direction only)
    canvas.set(x, y + 2, bl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if matches!(direction, Direction::TD | Direction::TB) {
            let below = canvas.get(pos_x, y + 3);
            if is_vertical(below, style) {
                style.junction_down
            } else {
                style.h
            }
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
    direction: Direction,
) {
    // Top border
    canvas.set(x, y, style.tl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if direction == Direction::BT {
            let above = if y > 0 { canvas.get(pos_x, y - 1) } else { ' ' };
            if is_vertical(above, style) { style.junction_up } else { style.h }
        } else {
            style.h
        };
        canvas.set(pos_x, y, c);
    }
    canvas.set(x + width - 1, y, style.tr);

    canvas.set(x, y + 1, '(');
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, ')');

    // Bottom border
    canvas.set(x, y + 2, style.bl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if matches!(direction, Direction::TD | Direction::TB) {
            let below = canvas.get(pos_x, y + 3);
            if is_vertical(below, style) { style.junction_down } else { style.h }
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
    direction: Direction,
) {
    // Top border
    canvas.set(x, y, '/');
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if direction == Direction::BT {
            let above = if y > 0 { canvas.get(pos_x, y - 1) } else { ' ' };
            if is_vertical(above, style) { style.junction_up } else { style.h }
        } else {
            style.h
        };
        canvas.set(pos_x, y, c);
    }
    canvas.set(x + width - 1, y, '\\');

    canvas.set(x, y + 1, '<');
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, '>');

    // Bottom border
    canvas.set(x, y + 2, '\\');
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if matches!(direction, Direction::TD | Direction::TB) {
            let below = canvas.get(pos_x, y + 3);
            if is_vertical(below, style) { style.junction_down } else { style.h }
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
    direction: Direction,
) {
    let is_unicode = style.tl == '┌';
    let h = if is_unicode { '─' } else { '-' };

    // Top border
    canvas.set(x, y, '/');
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if direction == Direction::BT {
            let above = if y > 0 { canvas.get(pos_x, y - 1) } else { ' ' };
            if is_vertical(above, style) { style.junction_up } else { h }
        } else {
            h
        };
        canvas.set(pos_x, y, c);
    }
    canvas.set(x + width - 1, y, '\\');

    canvas.set(x, y + 1, style.v);
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, style.v);

    // Bottom border
    canvas.set(x, y + 2, '\\');
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if matches!(direction, Direction::TD | Direction::TB) {
            let below = canvas.get(pos_x, y + 3);
            if is_vertical(below, style) { style.junction_down } else { h }
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
    direction: Direction,
) {
    let dv = if style.tl == '┌' { '║' } else { '|' };

    // Top border
    canvas.set(x, y, style.tl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if direction == Direction::BT {
            let above = if y > 0 { canvas.get(pos_x, y - 1) } else { ' ' };
            if is_vertical(above, style) { style.junction_up } else { style.h }
        } else {
            style.h
        };
        canvas.set(pos_x, y, c);
    }
    canvas.set(x + width - 1, y, style.tr);

    canvas.set(x, y + 1, dv);
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, dv);

    // Bottom border
    canvas.set(x, y + 2, style.bl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if matches!(direction, Direction::TD | Direction::TB) {
            let below = canvas.get(pos_x, y + 3);
            if is_vertical(below, style) { style.junction_down } else { style.h }
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
    direction: Direction,
) {
    // Top border
    canvas.set(x, y, '>');
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if direction == Direction::BT {
            let above = if y > 0 { canvas.get(pos_x, y - 1) } else { ' ' };
            if is_vertical(above, style) { style.junction_up } else { style.h }
        } else {
            style.h
        };
        canvas.set(pos_x, y, c);
    }
    canvas.set(x + width - 1, y, style.tr);

    canvas.set(x, y + 1, ' ');
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, style.v);

    // Bottom border
    canvas.set(x, y + 2, '>');
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if matches!(direction, Direction::TD | Direction::TB) {
            let below = canvas.get(pos_x, y + 3);
            if is_vertical(below, style) { style.junction_down } else { style.h }
        } else {
            style.h
        };
        canvas.set(pos_x, y + 2, c);
    }
    canvas.set(x + width - 1, y + 2, style.br);
}

/// Draw an edge label on the appropriate segment between two nodes.
/// For TD/BT: labels go on vertical segments
/// For LR/RL: labels go on horizontal segments
fn draw_edge_label(
    canvas: &mut Canvas,
    from: &Node,
    to: &Node,
    label: &str,
    direction: Direction,
    style: &StyleChars,
) {
    use edge::{center_x, center_y};

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

    match direction {
        Direction::TD | Direction::TB => {
            // Vertical layout: place label on vertical segment
            let src_center_x = center_x(from);
            let edge_x = center_x(to);
            let stem_start_y = from.y + BOX_HEIGHT;
            let arrow_y = to.y.saturating_sub(1);

            // For straight edges (aligned), place label in middle of vertical span
            // For L-shaped edges, place after junction
            let label_y = if src_center_x == edge_x {
                // Straight edge: place label in middle of vertical span
                stem_start_y + (arrow_y.saturating_sub(stem_start_y)) / 2
            } else {
                // L-shaped: use junction-based positioning
                let junction_y = stem_start_y + EDGE_STEM_HEIGHT;
                junction_y + EDGE_JUNCTION_HEIGHT
            };

            // Center the label around the edge position
            let label_start_x = edge_x.saturating_sub(label_width / 2);

            // Draw the label characters
            let mut x_pos = label_start_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height {
                    canvas.set(x_pos, label_y, c);
                }
                x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }
        }
        Direction::BT => {
            // Bottom-to-top: similar to TD but arrows point up
            let src_center_x = center_x(from);
            let edge_x = center_x(to);
            let stem_start_y = from.y.saturating_sub(1);
            let arrow_y = to.y + BOX_HEIGHT;

            let label_y = if src_center_x == edge_x {
                // Straight edge: place label in middle of vertical span
                let (top, bottom) = if stem_start_y < arrow_y { (stem_start_y, arrow_y) } else { (arrow_y, stem_start_y) };
                top + (bottom - top) / 2
            } else {
                // L-shaped: use junction-based positioning
                stem_start_y.saturating_sub(EDGE_STEM_HEIGHT).saturating_sub(EDGE_JUNCTION_HEIGHT)
            };

            let label_start_x = edge_x.saturating_sub(label_width / 2);
            let mut x_pos = label_start_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height {
                    canvas.set(x_pos, label_y, c);
                }
                x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }
        }
        Direction::LR => {
            // Left-to-right: place label INLINE with edge (on the connection line)
            // Format: ├─ label ─→
            let edge_y = center_y(to);
            let stem_start_x = from.x + from.width;
            let arrow_x = to.x.saturating_sub(1);

            // Calculate the middle of the horizontal span for label placement
            let span_width = arrow_x.saturating_sub(stem_start_x);
            let label_with_padding = label_width + 2; // " label "

            if span_width >= label_with_padding + 2 {
                // Enough room for: ─ label ─
                let label_start_x = stem_start_x + (span_width - label_with_padding) / 2;

                // Draw leading edge segment (from box to label)
                for x in stem_start_x..label_start_x {
                    canvas.set(x, edge_y, style.edge_h);
                }

                // Draw space before label
                canvas.set(label_start_x, edge_y, ' ');

                // Draw label characters
                let mut x_pos = label_start_x + 1;
                for c in display_label.chars() {
                    if x_pos < canvas.width {
                        canvas.set(x_pos, edge_y, c);
                    }
                    x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                }

                // Draw space after label
                if x_pos < canvas.width {
                    canvas.set(x_pos, edge_y, ' ');
                }
                x_pos += 1;

                // Draw trailing edge segment (from label to arrow)
                for x in x_pos..arrow_x {
                    if x < canvas.width {
                        canvas.set(x, edge_y, style.edge_h);
                    }
                }
            } else {
                // Not enough room - place label above the edge
                let label_x = stem_start_x + span_width / 2;
                let label_start_x = label_x.saturating_sub(label_width / 2);
                let label_row = edge_y.saturating_sub(1);

                let mut x_pos = label_start_x;
                for c in display_label.chars() {
                    if x_pos < canvas.width && label_row < canvas.height {
                        canvas.set(x_pos, label_row, c);
                    }
                    x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                }
            }
        }
        Direction::RL => {
            // Right-to-left: place label INLINE with edge (on the connection line)
            // Format: ←─ label ─┤ (arrow on left, junction on right)
            let edge_y = center_y(to);
            let arrow_x = to.x + to.width; // Arrow is after target box
            let stem_end_x = from.x; // Edge ends at left side of source box

            // Calculate the span between arrow and source box
            let span_width = stem_end_x.saturating_sub(arrow_x);
            let label_with_padding = label_width + 2; // " label "

            if span_width >= label_with_padding + 2 {
                // Enough room for: ─ label ─
                let label_start_x = arrow_x + (span_width - label_with_padding) / 2;

                // Draw leading edge segment (from arrow to label)
                for x in (arrow_x + 1)..label_start_x {
                    if x < canvas.width {
                        canvas.set(x, edge_y, style.edge_h);
                    }
                }

                // Draw space before label
                if label_start_x < canvas.width {
                    canvas.set(label_start_x, edge_y, ' ');
                }

                // Draw label characters
                let mut x_pos = label_start_x + 1;
                for c in display_label.chars() {
                    if x_pos < canvas.width {
                        canvas.set(x_pos, edge_y, c);
                    }
                    x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                }

                // Draw space after label
                if x_pos < canvas.width {
                    canvas.set(x_pos, edge_y, ' ');
                }
                x_pos += 1;

                // Draw trailing edge segment (from label to source box)
                for x in x_pos..stem_end_x {
                    if x < canvas.width {
                        canvas.set(x, edge_y, style.edge_h);
                    }
                }
            } else {
                // Not enough room - place label above the edge
                let label_x = arrow_x + span_width / 2;
                let label_start_x = label_x.saturating_sub(label_width / 2);
                let label_row = edge_y.saturating_sub(1);

                let mut x_pos = label_start_x;
                for c in display_label.chars() {
                    if x_pos < canvas.width && label_row < canvas.height {
                        canvas.set(x_pos, label_row, c);
                    }
                    x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                }
            }
        }
    }

    // Suppress unused warning for style parameter (reserved for future use)
    let _ = style;
}

/// Draw an edge label for convergent edges (multiple sources to one target).
/// Labels are placed on the vertical segment from the source before the merge point.
fn draw_convergent_edge_label(
    canvas: &mut Canvas,
    from: &Node,
    _to: &Node,
    label: &str,
    direction: Direction,
) {
    use edge::{center_x, center_y};

    // Truncate label if too long
    let max_label_len = 10; // Slightly shorter for convergent labels
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

    match direction {
        Direction::TD | Direction::TB => {
            // Place label on vertical line from source, before merge point
            let src_x = center_x(from);
            let stem_start_y = from.y + BOX_HEIGHT;
            // Place label just below the source box on the vertical stem
            let label_y = stem_start_y + 1;

            // Center label horizontally around source's edge position
            let label_start_x = src_x.saturating_sub(label_width / 2);

            let mut x_pos = label_start_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height {
                    canvas.set(x_pos, label_y, c);
                }
                x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }
        }
        Direction::BT => {
            let src_x = center_x(from);
            let stem_start_y = from.y.saturating_sub(1);
            let label_y = stem_start_y.saturating_sub(1);

            let label_start_x = src_x.saturating_sub(label_width / 2);
            let mut x_pos = label_start_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height {
                    canvas.set(x_pos, label_y, c);
                }
                x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }
        }
        Direction::LR => {
            // Place label on horizontal line from source, before merge
            let src_y = center_y(from);
            let stem_start_x = from.x + from.width;
            let label_x = stem_start_x + 1;
            // Place label above the edge line
            let label_y = src_y.saturating_sub(1);

            let mut x_pos = label_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height {
                    canvas.set(x_pos, label_y, c);
                }
                x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }
        }
        Direction::RL => {
            let src_y = center_y(from);
            let stem_start_x = from.x.saturating_sub(1);
            let label_x = stem_start_x.saturating_sub(label_width);
            let label_y = src_y.saturating_sub(1);

            let mut x_pos = label_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height {
                    canvas.set(x_pos, label_y, c);
                }
                x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }
        }
    }
}
