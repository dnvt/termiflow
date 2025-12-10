//! Render module - 2D character grid rendering for diagrams.
//!
//! This module handles the final rendering phase:
//! - Box drawing for nodes (9 shapes supported)
//! - Direction-agnostic edge routing (TD, LR, BT, RL)
//! - Junction/crossing detection for overlapping paths
//!
//! Rendering order: edges first, then boxes (boxes overwrite edge lines).
//!
//! # Module Structure
//!
//! - `canvas` - Canvas struct and character classification
//! - `edge` - Normal edge routing (all directions)
//! - `cycle` - Cycle/loop edge routing through gutters
//! - `shapes` - Box drawing for all 9 node shapes

pub mod canvas;
pub mod edge;
pub mod cycle;
pub mod shapes;

// Re-exports
pub use canvas::Canvas;

use anyhow::Result;

use crate::config::Config;
use crate::graph::{Graph, Node, Subgraph};
use crate::style::{
    display_width, truncate_label, BaseStyle, BOX_HEIGHT, COL_SPACING, EDGE_JUNCTION_HEIGHT,
    STEM_LENGTH_VERTICAL, MAX_CANVAS_HEIGHT, MAX_CANVAS_WIDTH, CYCLE_GUTTER, ROW_SPACING,
    StyleChars,
};

use cycle::route_cycle_edge;
use edge::{route_divergent_edges, route_convergent_edges};
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
    let mut max_right = graph.nodes.iter().map(|n| n.x + n.width).max().unwrap_or(0);
    let mut max_bottom = graph
        .nodes
        .iter()
        .map(|n| n.y + BOX_HEIGHT)
        .max()
        .unwrap_or(0);

    // Expand canvas to include subgraph bounds (which include padding)
    for subgraph in &graph.subgraphs {
        if subgraph.bounds.is_valid() {
            max_right = max_right.max(subgraph.bounds.x + subgraph.bounds.width);
            max_bottom = max_bottom.max(subgraph.bounds.y + subgraph.bounds.height);
        }
    }

    // Add gutter space for back-edges:
    // - TD/BT: right gutter (add to width)
    // - LR/RL: bottom gutter (add to height)
    let is_horizontal = matches!(graph.direction, Direction::LR | Direction::RL);
    let width_gutter = if graph.has_cycles() && !is_horizontal { CYCLE_GUTTER } else { 0 };
    let height_gutter = if graph.has_cycles() && is_horizontal { CYCLE_GUTTER } else { 0 };

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

    if graph.has_cycles() && !is_horizontal && width <= CYCLE_GUTTER {
        eprintln!("termiflow: warning: Back-edges skipped (gutter clipped)");
    }
    if graph.has_cycles() && is_horizontal && height <= CYCLE_GUTTER {
        eprintln!("termiflow: warning: Back-edges skipped (gutter clipped)");
    }

    let mut canvas = Canvas::new(width, height);
    let chars = config
        .composite_style
        .to_style_chars(BaseStyle::default());

    // Draw subgraph borders FIRST (background layer)
    // Edges and nodes will draw on top of these
    let subgraph_chars = config.composite_style.to_subgraph_chars();
    for subgraph in &graph.subgraphs {
        if subgraph.bounds.is_valid() {
            draw_subgraph(&mut canvas, subgraph, subgraph_chars);
        }
    }

    // Get visible nodes
    let visible_nodes: Vec<&Node> = graph
        .nodes
        .iter()
        .filter(|n| canvas.is_visible(n))
        .collect();

    // Group forward edges by source node for expanded routing
    let mut edges_by_source: HashMap<&str, Vec<&Node>> = HashMap::new();
    let mut cycle_edges: Vec<(&Node, &Node)> = Vec::new();
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
            cycle_edges.push((from, to));
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
    for (from, to) in cycle_edges {
        route_cycle_edge(from, to, &mut canvas, &chars, graph.direction);
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
        shapes::draw_node(
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
        use cycle::center_y;
        for &source_id in edges_by_source.keys() {
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
// Edge Label Drawing
// ============================================================================

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
    use cycle::{center_x, center_y};

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
                let junction_y = stem_start_y + STEM_LENGTH_VERTICAL;
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
                stem_start_y.saturating_sub(STEM_LENGTH_VERTICAL).saturating_sub(EDGE_JUNCTION_HEIGHT)
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
    use cycle::{center_x, center_y};

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

// ============================================================================
// Subgraph Rendering
// ============================================================================

/// Draw a subgraph border on the canvas.
///
/// Border style is determined by the `subgraph` component of CompositeStyle.
/// Default is ASCII for visual distinction from node boxes:
/// - Corners: + (or ┌┐└┘ for unicode, ╭╮╰╯ for rounded, etc.)
/// - Horizontal: - (or ─ for unicode, ━ for heavy, etc.)
/// - Vertical: | (or │ for unicode, ┃ for heavy, etc.)
///
/// If the subgraph has a title, it's centered on the top border:
/// ```text
/// +-- Title --+
/// |           |
/// +-----------+
/// ```
fn draw_subgraph(canvas: &mut Canvas, subgraph: &Subgraph, chars: &StyleChars) {
    let bounds = &subgraph.bounds;
    let x = bounds.x;
    let y = bounds.y;
    let w = bounds.width;
    let h = bounds.height;

    // Ensure we have valid dimensions
    if w < 2 || h < 2 {
        return;
    }

    let right = x + w - 1;
    let bottom = y + h - 1;

    // Draw corners using styled characters
    if x < canvas.width && y < canvas.height {
        canvas.set(x, y, chars.tl);
    }
    if right < canvas.width && y < canvas.height {
        canvas.set(right, y, chars.tr);
    }
    if x < canvas.width && bottom < canvas.height {
        canvas.set(x, bottom, chars.bl);
    }
    if right < canvas.width && bottom < canvas.height {
        canvas.set(right, bottom, chars.br);
    }

    // Draw top and bottom horizontal lines
    for col in (x + 1)..right {
        if col < canvas.width {
            if y < canvas.height {
                canvas.set(col, y, chars.h);
            }
            if bottom < canvas.height {
                canvas.set(col, bottom, chars.h);
            }
        }
    }

    // Draw left and right vertical lines
    for row in (y + 1)..bottom {
        if row < canvas.height {
            if x < canvas.width {
                canvas.set(x, row, chars.v);
            }
            if right < canvas.width {
                canvas.set(right, row, chars.v);
            }
        }
    }

    // Draw title on top border if present
    if let Some(ref title) = subgraph.title {
        let title_display = format!(" {} ", title);
        let title_width = display_width(&title_display);

        // Only draw if title fits
        if title_width + 2 <= w {
            // Center the title on the top border
            let title_start = x + (w - title_width) / 2;

            let mut col = title_start;
            for c in title_display.chars() {
                if col < canvas.width && y < canvas.height {
                    canvas.set(col, y, c);
                }
                col += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }
        }
    }
}
