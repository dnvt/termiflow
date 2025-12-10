//! Unified, direction-agnostic edge routing.
//!
//! This module provides a single edge routing algorithm that works for all
//! diagram orientations (TD, LR, BT, RL) using the orientation abstraction.

use crate::graph::{Direction, Node};
use crate::orientation::{OrientedCoords, is_before};
use crate::style::{StyleChars, BOX_HEIGHT, STEM_LENGTH_VERTICAL, STEM_LENGTH_HORIZONTAL};

use super::canvas::{is_junction, Canvas};

/// Route edges from a single source to multiple targets (divergence)
/// Works for all orientations using the abstraction layer
pub fn route_divergent_edges(
    from: &Node,
    to_nodes: &[&Node],
    canvas: &mut Canvas,
    style: &StyleChars,
    direction: Direction,
) {
    if to_nodes.is_empty() || !canvas.is_visible(from) {
        return;
    }

    let coords = OrientedCoords::new(direction);
    
    // Filter to visible targets only
    let visible_targets: Vec<&&Node> = to_nodes.iter().filter(|n| canvas.is_visible(n)).collect();
    if visible_targets.is_empty() {
        return;
    }

    // Calculate source center based on orientation
    let (src_x, src_y) = get_node_center(from);
    
    // Calculate stem start position (edge of source box on primary axis)
    let (stem_start_x, stem_start_y) = edge_exit_point(from, direction);
    
    // Calculate junction position (stem length away from source)
    let stem_length = match direction {
        Direction::LR | Direction::RL => STEM_LENGTH_HORIZONTAL,
        _ => STEM_LENGTH_VERTICAL,
    };
    
    let (junction_x, junction_y) = coords.advance(stem_start_x, stem_start_y, stem_length);

    // Get target centers and sort them on secondary axis
    let mut target_positions: Vec<(usize, usize, &Node)> = visible_targets
        .into_iter()
        .map(|n| {
            let (tx, ty) = get_node_center(n);
            (tx, ty, *n)
        })
        .collect();
    
    target_positions.sort_by_key(|(x, y, _)| coords.secondary_coord(*x, *y));

    // Single target: direct route
    if target_positions.len() == 1 {
        let (target_x, target_y, target) = (target_positions[0].0, target_positions[0].1, target_positions[0].2);
        let (arrow_x, arrow_y) = edge_entry_point(target, direction);
        
        let src_secondary = coords.secondary_coord(src_x, src_y);
        let target_secondary = coords.secondary_coord(target_x, target_y);
        
        if src_secondary == target_secondary {
            // Aligned: straight line on primary axis
            draw_line_primary(stem_start_x, stem_start_y, arrow_x, arrow_y, &coords, canvas, style);
        } else {
            // L-shaped route
            // 1. Stem from source
            draw_line_primary(stem_start_x, stem_start_y, junction_x, junction_y, &coords, canvas, style);
            
            // 2. Turn at junction
            let going_before = is_before(src_secondary, target_secondary);
            let corner = coords.corner_start_to_secondary(going_before, style);
            canvas.set(junction_x, junction_y, corner);
            
            // 3. Secondary span to target column
            draw_line_secondary(junction_x, junction_y, junction_x, target_y, &coords, canvas, style);
            
            // 4. Turn to target
            let corner2 = coords.corner_secondary_to_end(going_before, style);
            let (corner2_x, corner2_y) = (
                coords.secondary_coord(junction_x, arrow_x),
                coords.secondary_coord(junction_y, arrow_y)
            );
            canvas.set(corner2_x, corner2_y, corner2);
            
            // 5. Final segment to arrow
            let (seg_start_x, seg_start_y) = coords.advance(corner2_x, corner2_y, 1);
            draw_line_primary(seg_start_x, seg_start_y, arrow_x, arrow_y, &coords, canvas, style);
        }
        
        // Arrow at target
        canvas.set(arrow_x, arrow_y, coords.arrow_end(style));
        return;
    }

    // Multiple targets: draw branching structure

    // 1. Draw stem from source to junction (not including junction)
    let stem_length = match direction {
        Direction::LR | Direction::RL => STEM_LENGTH_HORIZONTAL,
        _ => STEM_LENGTH_VERTICAL,
    };
    for i in 0..stem_length {
        let (px, py) = coords.advance(stem_start_x, stem_start_y, i);
        canvas.set_edge_char(px, py, coords.primary_edge_char(style), style);
    }
    
    // 2. Calculate span on secondary axis
    let first_secondary = coords.secondary_coord(target_positions[0].0, target_positions[0].1);
    let last_secondary = coords.secondary_coord(
        target_positions[target_positions.len()-1].0,
        target_positions[target_positions.len()-1].1
    );
    let src_secondary = coords.secondary_coord(src_x, src_y);
    
    let span_start = first_secondary.min(src_secondary);
    let span_end = last_secondary.max(src_secondary);
    
    // 3. Draw horizontal junction span with corners and junction
    for pos in span_start..=span_end {
        let (span_x, span_y) = coords.with_secondary(junction_x, junction_y, pos);

        let c = if pos == src_secondary {
            // Junction at source position - stem meets vertical span
            // For LR: stem comes from LEFT, span goes UP/DOWN → ┤ (junction_left)
            // For RL: stem comes from RIGHT, span goes UP/DOWN → ├ (junction_right)
            match direction {
                Direction::TD | Direction::TB => style.junction_up,    // ┴
                Direction::LR => style.junction_left,                  // ┤
                Direction::RL => style.junction_right,                 // ├
                Direction::BT => style.junction_down,                  // ┬
            }
        } else if pos == span_start {
            // Corner at top/left end of span
            match direction {
                Direction::TD | Direction::TB => style.corner_dl,  // ┌ (opens down-right)
                Direction::LR => style.corner_dl,  // ┌ (opens down-right for vertical span)
                Direction::RL => style.corner_dr,  // ┐ (opens down-left for vertical span)
                Direction::BT => style.corner_ul,  // └ (opens up-right)
            }
        } else if pos == span_end {
            // Corner at bottom/right end of span
            match direction {
                Direction::TD | Direction::TB => style.corner_dr,  // ┐ (opens down-left)
                Direction::LR => style.corner_ul,  // └ (opens up-right for vertical span)
                Direction::RL => style.corner_ur,  // ┘ (opens up-left for vertical span)
                Direction::BT => style.corner_ur,  // ┘ (opens up-left)
            }
        } else {
            coords.secondary_edge_char(style)
        };
        canvas.set_edge_char(span_x, span_y, c, style);
    }
    
    // 4. Draw drops and arrows for each target
    for (target_x, target_y, target) in &target_positions {
        let target_secondary = coords.secondary_coord(*target_x, *target_y);
        let (arrow_x, arrow_y) = edge_entry_point(target, direction);
        
        // Draw vertical drop from junction+1 to arrow
        let (drop_x, drop_y) = coords.with_secondary(junction_x, junction_y, target_secondary);
        let (drop_start_x, drop_start_y) = coords.advance(drop_x, drop_y, 1);
        
        // Only draw if there's actually a drop to draw
        if drop_start_x != arrow_x || drop_start_y != arrow_y {
            draw_line_primary(drop_start_x, drop_start_y, arrow_x, arrow_y, &coords, canvas, style);
        }
        
        // Arrow
        canvas.set(arrow_x, arrow_y, coords.arrow_end(style));
    }
}

// Helper: Draw lines from sources to merge point (on primary axis)
fn draw_source_lines_to_merge(
    source_positions: &[(usize, usize, &Node)],
    merge_x: usize,
    merge_y: usize,
    coords: &OrientedCoords,
    canvas: &mut Canvas,
    style: &StyleChars,
    direction: Direction,
) -> (usize, usize) {
    let mut span_start = usize::MAX;
    let mut span_end = 0;

    for &(src_x, src_y, source) in source_positions {
        let (edge_x, edge_y) = edge_exit_point(source, direction);
        let src_secondary = coords.secondary_coord(src_x, src_y);

        // Update span bounds
        span_start = span_start.min(src_secondary);
        span_end = span_end.max(src_secondary);

        // Line from source to just before merge point (along primary axis)
        let (merge_col_x, merge_col_y) = coords.with_secondary(merge_x, merge_y, src_secondary);

        // Draw line from source to merge span (direction-aware)
        match direction {
            Direction::TD | Direction::TB => {
                // Vertical layout: draw vertical line
                if edge_y < merge_col_y {
                    for y in edge_y..merge_col_y {
                        canvas.set_edge_char(src_x, y, style.edge_v, style);
                    }
                }
            }
            Direction::LR => {
                // LR layout: draw horizontal line from source right edge to merge column
                if edge_x < merge_col_x {
                    for x in edge_x..merge_col_x {
                        canvas.set_edge_char(x, src_y, style.edge_h, style);
                    }
                }
            }
            Direction::RL => {
                // RL layout: draw horizontal line from source left edge to merge column
                if merge_col_x < edge_x {
                    for x in (merge_col_x + 1)..=edge_x {
                        canvas.set_edge_char(x, src_y, style.edge_h, style);
                    }
                }
            }
            Direction::BT => {
                // BT layout: draw vertical line upward
                if merge_col_y < edge_y {
                    for y in (merge_col_y + 1)..=edge_y {
                        canvas.set_edge_char(src_x, y, style.edge_v, style);
                    }
                }
            }
        }

        // Corner where source line meets merge span
        let corner_char = get_convergence_corner(src_secondary, span_start, span_end, direction, style, coords);
        canvas.set_edge_char(merge_col_x, merge_col_y, corner_char, style);
    }

    (span_start, span_end)
}

/// Get the appropriate corner character for convergence based on position on span.
fn get_convergence_corner(
    src_secondary: usize,
    span_start: usize,
    span_end: usize,
    direction: Direction,
    style: &StyleChars,
    coords: &OrientedCoords,
) -> char {
    if src_secondary == span_start {
        // Topmost/leftmost position on span - edge from source turns down/right
        match direction {
            Direction::TD | Direction::TB => style.corner_ul, // ┌ - from above, turns right
            Direction::LR => style.corner_dr,                 // ┐ - from left, turns down
            Direction::RL => style.corner_dl,                 // ┌ - from right, turns down
            Direction::BT => style.corner_dl,                 // └ - from below, turns right
        }
    } else if src_secondary == span_end {
        // Bottommost/rightmost position on span - edge from source turns up/left
        match direction {
            Direction::TD | Direction::TB => style.corner_ur, // ┐ - from above, turns left
            Direction::LR => style.corner_ur,                 // ┘ - from left, turns up
            Direction::RL => style.corner_ul,                 // └ - from right, turns up
            Direction::BT => style.corner_dr,                 // ┘ - from below, turns left
        }
    } else {
        // Middle sources get junction
        coords.junction_merge(style)
    }
}

// Helper: Draw the horizontal merge line
fn draw_merge_line(
    merge_x: usize,
    merge_y: usize,
    span_start: usize,
    span_end: usize,
    coords: &OrientedCoords,
    canvas: &mut Canvas,
    style: &StyleChars,
) {
    for pos in span_start..=span_end {
        let (span_x, span_y) = coords.with_secondary(merge_x, merge_y, pos);
        let c = canvas.get(span_x, span_y);
        
        if c == ' ' || (is_secondary_line(c, coords, style) && !is_junction(c, style)) {
            canvas.set_edge_char(span_x, span_y, coords.secondary_edge_char(style), style);
        }
    }
}

/// Route edges from multiple sources to a single target (convergence)
pub fn route_convergent_edges(
    from_nodes: &[&Node],
    to: &Node,
    canvas: &mut Canvas,
    style: &StyleChars,
    direction: Direction,
) {
    if from_nodes.is_empty() || !canvas.is_visible(to) {
        return;
    }

    let coords = OrientedCoords::new(direction);
    
    // Filter to visible sources
    let visible_sources: Vec<&Node> = from_nodes.iter()
        .filter(|n| canvas.is_visible(n))
        .copied()
        .collect();
    if visible_sources.is_empty() {
        return;
    }

    // Get target position
    let (target_x, target_y) = get_node_center(to);
    let (arrow_x, arrow_y) = edge_entry_point(to, direction);
    
    // Calculate merge point (before target on primary axis)
    let merge_distance = match direction {
        Direction::LR | Direction::RL => STEM_LENGTH_HORIZONTAL,
        _ => STEM_LENGTH_VERTICAL,
    };
    let (merge_x, merge_y) = coords.retreat(arrow_x, arrow_y, merge_distance);
    
    // Get source positions sorted on secondary axis
    let mut source_positions: Vec<(usize, usize, &Node)> = visible_sources
        .iter()
        .map(|n| {
            let (sx, sy) = get_node_center(n);
            (sx, sy, *n)
        })
        .collect();
    
    source_positions.sort_by_key(|(x, y, _)| coords.secondary_coord(*x, *y));
    
    // Calculate target position on secondary axis
    let target_secondary = coords.secondary_coord(target_x, target_y);
    
    // Draw from each source to merge point
    let (actual_span_start, actual_span_end) = draw_source_lines_to_merge(
        &source_positions,
        merge_x,
        merge_y,
        &coords,
        canvas,
        style,
        direction,
    );
    
    // Expand span to include target if needed
    let final_span_start = actual_span_start.min(target_secondary);
    let final_span_end = actual_span_end.max(target_secondary);
    
    // Draw horizontal merge line
    draw_merge_line(merge_x, merge_y, final_span_start, final_span_end, &coords, canvas, style);
    
    // Junction at merge point and line to target
    // The junction shows where edges from above/below exit toward the target
    let junction_char = match direction {
        Direction::TD | Direction::TB => style.junction_down,  // ┬ - edges from left/right, exits down
        Direction::LR => style.junction_right,                 // ├ - edges from above/below, exits right
        Direction::RL => style.junction_left,                  // ┤ - edges from above/below, exits left
        Direction::BT => style.junction_up,                    // ┴ - edges from left/right, exits up
    };
    canvas.set_edge_char(merge_x, merge_y, junction_char, style);
    let (final_start_x, final_start_y) = coords.advance(merge_x, merge_y, 1);
    draw_line_primary(final_start_x, final_start_y, arrow_x, arrow_y, &coords, canvas, style);
    
    // Arrow
    canvas.set(arrow_x, arrow_y, coords.arrow_end(style));
}

// ============================================================================
// Helper Functions
// ============================================================================

fn get_node_center(node: &Node) -> (usize, usize) {
    (node.center_x(), node.center_y())
}

/// Where an incoming edge enters a target node (arrow position).
fn edge_entry_point(node: &Node, direction: Direction) -> (usize, usize) {
    match direction {
        Direction::TD | Direction::TB => (node.center_x(), node.y.saturating_sub(1)),
        Direction::LR => (node.x.saturating_sub(1), node.center_y()),
        Direction::RL => (node.x + node.width, node.center_y()),
        Direction::BT => (node.center_x(), node.y + BOX_HEIGHT),
    }
}

/// Where an outgoing edge exits a source node (stem start position).
fn edge_exit_point(node: &Node, direction: Direction) -> (usize, usize) {
    match direction {
        Direction::TD | Direction::TB => (node.center_x(), node.y + BOX_HEIGHT),
        Direction::LR => (node.x + node.width, node.center_y()),
        Direction::RL => (node.x.saturating_sub(1), node.center_y()),
        Direction::BT => (node.center_x(), node.y.saturating_sub(1)),
    }
}

fn draw_line_primary(x1: usize, y1: usize, x2: usize, y2: usize, coords: &OrientedCoords, canvas: &mut Canvas, style: &StyleChars) {
    let char = coords.primary_edge_char(style);
    
    match coords.primary {
        crate::orientation::Axis::Horizontal => {
            let (start, end) = if x1 < x2 { (x1, x2) } else { (x2, x1) };
            for x in start..=end {
                canvas.set_edge_char(x, y1, char, style);
            }
        }
        crate::orientation::Axis::Vertical => {
            let (start, end) = if y1 < y2 { (y1, y2) } else { (y2, y1) };
            for y in start..=end {
                canvas.set_edge_char(x1, y, char, style);
            }
        }
    }
}

fn draw_line_secondary(x1: usize, y1: usize, x2: usize, y2: usize, coords: &OrientedCoords, canvas: &mut Canvas, style: &StyleChars) {
    let char = coords.secondary_edge_char(style);
    
    match coords.secondary {
        crate::orientation::Axis::Horizontal => {
            let (start, end) = if x1 < x2 { (x1, x2) } else { (x2, x1) };
            for x in start..=end {
                if x != x1 && x != x2 {  // Skip corners
                    canvas.set_edge_char(x, y1, char, style);
                }
            }
        }
        crate::orientation::Axis::Vertical => {
            let (start, end) = if y1 < y2 { (y1, y2) } else { (y2, y1) };
            for y in start..=end {
                if y != y1 && y != y2 {  // Skip corners
                    canvas.set_edge_char(x1, y, char, style);
                }
            }
        }
    }
}

fn is_secondary_line(c: char, coords: &OrientedCoords, style: &StyleChars) -> bool {
    let expected = coords.secondary_edge_char(style);
    c == expected
}