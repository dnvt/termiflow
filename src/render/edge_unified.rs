//! Unified, direction-agnostic edge routing.
//!
//! This module provides a single edge routing algorithm that works for all
//! diagram orientations (TD, LR, BT, RL) using the orientation abstraction.

use crate::graph::{Direction, Node};
use crate::orientation::{OrientedCoords, is_before};
use crate::style::{StyleChars, BOX_HEIGHT, EDGE_STEM_HEIGHT, EDGE_STEM_WIDTH_LR};

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
    let (src_x, src_y) = get_node_center(from, direction);
    
    // Calculate stem start position (edge of source box on primary axis)
    let (stem_start_x, stem_start_y) = get_box_edge_end(from, direction);
    
    // Calculate junction position (stem length away from source)
    let stem_length = match direction {
        Direction::LR => EDGE_STEM_WIDTH_LR,
        _ => EDGE_STEM_HEIGHT,
    };
    
    let (junction_x, junction_y) = advance_on_primary(
        stem_start_x, 
        stem_start_y, 
        stem_length, 
        &coords
    );

    // Get target centers and sort them on secondary axis
    let mut target_positions: Vec<(usize, usize, &Node)> = visible_targets
        .into_iter()
        .map(|n| {
            let (tx, ty) = get_node_center(n, direction);
            (tx, ty, *n)
        })
        .collect();
    
    target_positions.sort_by_key(|(x, y, _)| coords.secondary_coord(*x, *y));

    // Single target: direct route
    if target_positions.len() == 1 {
        let (target_x, target_y, target) = (target_positions[0].0, target_positions[0].1, target_positions[0].2);
        let (arrow_x, arrow_y) = get_box_edge_start(target, direction);
        
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
            let (seg_start_x, seg_start_y) = advance_on_primary(corner2_x, corner2_y, 1, &coords);
            draw_line_primary(seg_start_x, seg_start_y, arrow_x, arrow_y, &coords, canvas, style);
        }
        
        // Arrow at target
        canvas.set(arrow_x, arrow_y, coords.arrow_end(style));
        return;
    }

    // Multiple targets: draw branching structure
    
    // 1. Draw stem from source to junction (not including junction)
    let stem_length = match direction {
        Direction::LR | Direction::RL => EDGE_STEM_WIDTH_LR,
        _ => EDGE_STEM_HEIGHT,
    };
    for i in 0..stem_length {
        let (px, py) = advance_on_primary(stem_start_x, stem_start_y, i, &coords);
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
        let (span_x, span_y) = set_secondary_get_coords(junction_x, junction_y, pos, &coords);
        
        let c = if pos == src_secondary {
            // Junction at source position - stem meets horizontal span
            match direction {
                Direction::TD | Direction::TB => style.junction_up,    // ┴
                Direction::LR => style.junction_right,                 // ├
                Direction::RL => style.junction_left,                  // ┤
                Direction::BT => style.junction_down,                  // ┬
            }
        } else if pos == span_start {
            // Corner at left/top end
            match direction {
                Direction::TD | Direction::TB => style.corner_dl,  // ┌
                Direction::LR => style.corner_ul,  // ┌ for vertical span
                Direction::RL => style.corner_ur,  // ┐ for vertical span
                Direction::BT => style.corner_ul,  // ┌
            }
        } else if pos == span_end {
            // Corner at right/bottom end
            match direction {
                Direction::TD | Direction::TB => style.corner_dr,  // ┐
                Direction::LR => style.corner_dl,  // └ for vertical span
                Direction::RL => style.corner_ul,  // ┌ for vertical span
                Direction::BT => style.corner_ur,  // ┐
            }
        } else {
            coords.secondary_edge_char(style)
        };
        canvas.set_edge_char(span_x, span_y, c, style);
    }
    
    // 4. Draw drops and arrows for each target
    for (target_x, target_y, target) in &target_positions {
        let target_secondary = coords.secondary_coord(*target_x, *target_y);
        let (arrow_x, arrow_y) = get_box_edge_start(target, direction);
        
        // Draw vertical drop from junction+1 to arrow
        let (drop_x, drop_y) = set_secondary_get_coords(junction_x, junction_y, target_secondary, &coords);
        let (drop_start_x, drop_start_y) = advance_on_primary(drop_x, drop_y, 1, &coords);
        
        // Only draw if there's actually a drop to draw
        if drop_start_x != arrow_x || drop_start_y != arrow_y {
            draw_line_primary(drop_start_x, drop_start_y, arrow_x, arrow_y, &coords, canvas, style);
        }
        
        // Arrow
        canvas.set(arrow_x, arrow_y, coords.arrow_end(style));
    }
}

// Helper: Draw vertical lines from sources to merge row
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
    
    for i in 0..source_positions.len() {
        let (src_x, src_y, source) = source_positions[i];
        let (_, edge_y) = get_box_edge_end(source, direction);
        let src_secondary = coords.secondary_coord(src_x, src_y);
        
        // Update span bounds
        span_start = span_start.min(src_secondary);
        span_end = span_end.max(src_secondary);
        
        // Line from source down to just before merge row
        let (merge_col_x, merge_col_y) = set_secondary_get_coords(merge_x, merge_y, src_secondary, &coords);
        
        // Draw vertical line from source to merge row
        if edge_y < merge_col_y {
            for y in edge_y..merge_col_y {
                canvas.set_edge_char(src_x, y, style.edge_v, style);
            }
        }
        
        // Corner where source line meets horizontal merge span
        let corner_char = get_convergence_corner(i, source_positions.len(), src_secondary, span_start, span_end, direction, style, coords);
        canvas.set_edge_char(merge_col_x, merge_col_y, corner_char, style);
    }
    
    (span_start, span_end)
}

// Helper: Get the appropriate corner character for convergence
fn get_convergence_corner(
    _index: usize,
    _total: usize,
    src_secondary: usize,
    span_start: usize,
    span_end: usize,
    direction: Direction,
    style: &StyleChars,
    coords: &OrientedCoords,
) -> char {
    if src_secondary == span_start {
        // Leftmost position on span
        match direction {
            Direction::TD | Direction::TB => style.corner_ul,
            Direction::LR => style.corner_dr,
            Direction::RL => style.corner_dl,
            Direction::BT => style.corner_dl,
        }
    } else if src_secondary == span_end {
        // Rightmost position on span
        match direction {
            Direction::TD | Direction::TB => style.corner_ur,
            Direction::LR => style.corner_ur,
            Direction::RL => style.corner_ul,
            Direction::BT => style.corner_dr,
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
        let (span_x, span_y) = set_secondary_get_coords(merge_x, merge_y, pos, &coords);
        let c = canvas.get(span_x, span_y);
        
        if c == ' ' || (is_secondary_line(c, &coords, style) && !is_junction(c, style)) {
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
    let (target_x, target_y) = get_node_center(to, direction);
    let (arrow_x, arrow_y) = get_box_edge_start(to, direction);
    
    // Calculate merge point (before target on primary axis)
    let merge_distance = match direction {
        Direction::LR | Direction::RL => EDGE_STEM_WIDTH_LR,
        _ => EDGE_STEM_HEIGHT,
    };
    let (merge_x, merge_y) = retreat_on_primary(arrow_x, arrow_y, merge_distance, &coords);
    
    // Get source positions sorted on secondary axis
    let mut source_positions: Vec<(usize, usize, &Node)> = visible_sources
        .iter()
        .map(|n| {
            let (sx, sy) = get_node_center(n, direction);
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
    let junction_char = match direction {
        Direction::TD | Direction::TB => style.junction_down,  // ┬
        Direction::LR => style.junction_left,                  // ┤
        Direction::RL => style.junction_right,                 // ├
        Direction::BT => style.junction_up,                    // ┴
    };
    canvas.set_edge_char(merge_x, merge_y, junction_char, style);
    let (final_start_x, final_start_y) = advance_on_primary(merge_x, merge_y, 1, &coords);
    draw_line_primary(final_start_x, final_start_y, arrow_x, arrow_y, &coords, canvas, style);
    
    // Arrow
    canvas.set(arrow_x, arrow_y, coords.arrow_end(style));
}

// ============================================================================
// Helper Functions
// ============================================================================

fn get_node_center(node: &Node, direction: Direction) -> (usize, usize) {
    match direction {
        Direction::TD | Direction::TB | Direction::BT => {
            (node.x + node.width / 2, node.y + BOX_HEIGHT / 2)
        }
        Direction::LR | Direction::RL => {
            (node.x + node.width / 2, node.y + BOX_HEIGHT / 2)
        }
    }
}

fn get_box_edge_start(node: &Node, direction: Direction) -> (usize, usize) {
    match direction {
        Direction::TD | Direction::TB => (node.x + node.width / 2, node.y.saturating_sub(1)),
        Direction::LR => (node.x.saturating_sub(1), node.y + BOX_HEIGHT / 2),
        Direction::RL => (node.x + node.width, node.y + BOX_HEIGHT / 2),
        Direction::BT => (node.x + node.width / 2, node.y + BOX_HEIGHT),
    }
}

fn get_box_edge_end(node: &Node, direction: Direction) -> (usize, usize) {
    match direction {
        Direction::TD | Direction::TB => (node.x + node.width / 2, node.y + BOX_HEIGHT),
        Direction::LR => (node.x + node.width, node.y + BOX_HEIGHT / 2),
        Direction::RL => (node.x.saturating_sub(1), node.y + BOX_HEIGHT / 2),
        Direction::BT => (node.x + node.width / 2, node.y.saturating_sub(1)),
    }
}

fn advance_on_primary(x: usize, y: usize, distance: usize, coords: &OrientedCoords) -> (usize, usize) {
    let mut new_x = x;
    let mut new_y = y;
    
    match coords.primary {
        crate::orientation::Axis::Horizontal => {
            match coords.direction {
                Direction::RL => new_x = new_x.saturating_sub(distance),
                _ => new_x += distance,
            }
        }
        crate::orientation::Axis::Vertical => {
            match coords.direction {
                Direction::BT => new_y = new_y.saturating_sub(distance),
                _ => new_y += distance,
            }
        }
    }
    
    (new_x, new_y)
}

fn retreat_on_primary(x: usize, y: usize, distance: usize, coords: &OrientedCoords) -> (usize, usize) {
    let mut new_x = x;
    let mut new_y = y;
    
    match coords.primary {
        crate::orientation::Axis::Horizontal => {
            match coords.direction {
                Direction::RL => new_x += distance,
                _ => new_x = new_x.saturating_sub(distance),
            }
        }
        crate::orientation::Axis::Vertical => {
            match coords.direction {
                Direction::BT => new_y += distance,
                _ => new_y = new_y.saturating_sub(distance),
            }
        }
    }
    
    (new_x, new_y)
}

fn set_secondary_get_coords(x: usize, y: usize, secondary_val: usize, coords: &OrientedCoords) -> (usize, usize) {
    let mut new_x = x;
    let mut new_y = y;
    coords.set_secondary(&mut new_x, &mut new_y, secondary_val);
    (new_x, new_y)
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