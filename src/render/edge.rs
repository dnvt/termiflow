//! Edge routing algorithms for connecting nodes.
//!
//! Handles three types of edge routing:
//! - **Expanded**: Multi-target with vertical stems and junction spans (new)
//! - **Straight**: Direct vertical line when nodes are aligned
//! - **L-shaped**: Horizontal then vertical when nodes are offset
//! - **Back-edge**: Cycle edges routed through the right gutter

use crate::graph::Node;
use crate::style::{StyleChars, BOX_HEIGHT, RIGHT_GUTTER, EDGE_STEM_HEIGHT, EDGE_JUNCTION_HEIGHT, EDGE_DROP_HEIGHT};

use super::canvas::{is_arrow, is_vertical, Canvas};

// ============================================================================
// Public Edge Routing
// ============================================================================

/// Route edges from a single source to multiple targets using expanded layout.
///
/// Draws: stem → junction span → drops → arrows
/// This creates clearer visual routing than compact L-shaped edges.
pub fn route_expanded_edge(
    from: &Node,
    to_nodes: &[&Node],
    canvas: &mut Canvas,
    style: &StyleChars,
) {
    if to_nodes.is_empty() || !canvas.is_visible(from) {
        return;
    }

    // Filter to visible targets only
    let visible_targets: Vec<&&Node> = to_nodes.iter().filter(|n| canvas.is_visible(n)).collect();
    if visible_targets.is_empty() {
        return;
    }

    let src_center_x = center_x(from);
    let stem_start_y = from.y + BOX_HEIGHT;
    let junction_y = stem_start_y + EDGE_STEM_HEIGHT;
    let drop_start_y = junction_y + EDGE_JUNCTION_HEIGHT;
    let arrow_y = drop_start_y + EDGE_DROP_HEIGHT;

    // Get destination centers, sorted left to right
    let mut dest_centers: Vec<usize> = visible_targets
        .iter()
        .map(|n| center_x(n))
        .collect();
    dest_centers.sort();

    // Single target: draw stem, optional horizontal, then arrow
    // Layout: stem (row 0) → label (row 1) → arrow (row 2) → blank (row 3)
    if dest_centers.len() == 1 {
        let dest_x = dest_centers[0];
        // Single-target arrow is right after label row, not after drop row
        let single_arrow_y = junction_y + 1;

        if src_center_x == dest_x {
            // Aligned: stem only (junction row left empty for label)
            for y in stem_start_y..junction_y {
                canvas.set_edge_char(dest_x, y, style.edge_v, style);
            }
        } else {
            // Not aligned: L-shaped route
            // Stem from source
            for y in stem_start_y..junction_y {
                canvas.set_edge_char(src_center_x, y, style.edge_v, style);
            }
            // Horizontal span - skip corner positions
            let (left, right) = if src_center_x < dest_x {
                (src_center_x, dest_x)
            } else {
                (dest_x, src_center_x)
            };
            for x in left..=right {
                if x != src_center_x && x != dest_x {
                    canvas.set_edge_char(x, junction_y, style.edge_h, style);
                }
            }
            // Corners - placed separately so they can merge with other edges
            if src_center_x < dest_x {
                canvas.set_edge_char(src_center_x, junction_y, style.corner_ul, style);
                canvas.set_edge_char(dest_x, junction_y, style.corner_dr, style);
            } else {
                canvas.set_edge_char(src_center_x, junction_y, style.corner_ur, style);
                canvas.set_edge_char(dest_x, junction_y, style.corner_dl, style);
            }
        }
        // Arrow right after label row for single-target
        canvas.set(dest_x, single_arrow_y, style.arrow_down);
        return;
    }

    // Multiple targets: stem → junction → drops → arrows
    let left_x = *dest_centers.first().unwrap();
    let right_x = *dest_centers.last().unwrap();
    let span_left = left_x.min(src_center_x);
    let span_right = right_x.max(src_center_x);

    // Phase 1: Draw source stem (from source center down to junction)
    for y in stem_start_y..junction_y {
        canvas.set_edge_char(src_center_x, y, style.edge_v, style);
    }

    // Phase 2: Draw horizontal junction span with corners and junction
    for x in span_left..=span_right {
        let c = if x == src_center_x {
            style.junction_up
        } else if x == span_left {
            style.corner_dl
        } else if x == span_right {
            style.corner_dr
        } else {
            style.edge_h
        };
        canvas.set_edge_char(x, junction_y, c, style);
    }

    // Phase 3 & 4: Draw drops and arrows for each destination
    for &dest_x in &dest_centers {
        // Draw vertical drop
        for y in (junction_y + 1)..arrow_y {
            canvas.set_edge_char(dest_x, y, style.edge_v, style);
        }
        // Place arrow
        canvas.set(dest_x, arrow_y, style.arrow_down);
    }
}

/// Route an edge from source to target node.
///
/// Handles three cases:
/// 1. Straight vertical line (nodes nearly aligned)
/// 2. Reuse existing path (when blocked by intervening node)
/// 3. L-shaped routing (horizontal then vertical)
pub fn route_edge(
    from: &Node,
    to: &Node,
    edge_index: usize,
    canvas: &mut Canvas,
    style: &StyleChars,
    all_nodes: &[&Node],
) {
    if !canvas.is_visible(from) || !canvas.is_visible(to) {
        return;
    }

    let start_x = center_x(from);
    let start_y = from.y + BOX_HEIGHT;
    let end_x = center_x(to);
    let end_y = to.y.saturating_sub(1);

    // Check if vertical line at x would pass through an intervening node
    let is_blocked = |x: usize| {
        all_nodes.iter().any(|n| {
            n.id != from.id
                && n.id != to.id
                && n.y + BOX_HEIGHT > start_y
                && n.y < end_y
                && x >= n.x
                && x < n.x + n.width
        })
    };

    let x_diff = (start_x as isize - end_x as isize).unsigned_abs();

    // Case 1: Nearly aligned with clear path - draw straight vertical
    if x_diff <= 1 && !is_blocked(end_x) {
        for y in start_y..end_y {
            canvas.set_edge_char(end_x, y, style.edge_v, style);
        }
        canvas.set(end_x, end_y, style.arrow_down);
        return;
    }

    // Case 2: Blocked - check if existing path reaches target (edge sorting ensures this)
    if is_blocked(end_x) {
        let existing = canvas.get(end_x, end_y.saturating_sub(1));
        if is_vertical(existing, style) || is_arrow(existing) {
            // Path exists - just add arrow
            canvas.set(end_x, end_y, style.arrow_down);
            return;
        }
    }

    // Case 3: L-shaped routing needed
    let mid_y = calculate_mid_y(start_y, end_y, edge_index);

    // Merge with adjacent vertical line if exists
    let use_x = [start_x.saturating_sub(1), start_x + 1, start_x]
        .into_iter()
        .find(|&x| {
            let c = canvas.get(x, start_y);
            x != start_x && (is_vertical(c, style) || is_arrow(c))
        })
        .unwrap_or(start_x);

    // Draw vertical line down from source to mid_y
    for y in start_y..mid_y {
        canvas.set_edge_char(use_x, y, style.edge_v, style);
    }

    // Draw horizontal line at mid_y with corners
    let (left, right) = if use_x < end_x {
        (use_x, end_x)
    } else {
        (end_x, use_x)
    };
    for x in left..=right {
        canvas.set_edge_char(x, mid_y, style.edge_h, style);
    }

    // Place corners
    canvas.set_edge_char(use_x, mid_y, corner_char(use_x, end_x, true, style), style);
    canvas.set_edge_char(end_x, mid_y, corner_char(end_x, use_x, false, style), style);

    // Draw vertical line down to target
    for y in (mid_y + 1)..end_y {
        canvas.set_edge_char(end_x, y, style.edge_v, style);
    }

    // Place arrow at target
    if mid_y < end_y {
        canvas.set(end_x, end_y, style.arrow_down);
    }
}

/// Route a back-edge (cycle) through the right gutter.
///
/// Back-edges go: right from source -> down/up in gutter -> left to target
pub fn route_back_edge(from: &Node, to: &Node, canvas: &mut Canvas, style: &StyleChars) {
    if !canvas.is_visible(from) || !canvas.is_visible(to) {
        return;
    }

    if canvas.width <= RIGHT_GUTTER {
        return;
    }

    let gutter_x = canvas.width - 2;

    let from_y = from.y + BOX_HEIGHT / 2;
    let to_y = to.y + BOX_HEIGHT / 2;

    // Horizontal line from source to gutter
    for x in (from.x + from.width)..gutter_x {
        canvas.set_edge_char(x, from_y, style.back_h, style);
    }

    // Vertical line in gutter
    let (top, bottom) = if from_y < to_y {
        (from_y, to_y)
    } else {
        (to_y, from_y)
    };
    for y in top..=bottom {
        canvas.set_edge_char(gutter_x, y, style.back_v, style);
    }

    // Horizontal line from gutter to target
    for x in (to.x + to.width)..gutter_x {
        canvas.set_edge_char(x, to_y, style.back_h, style);
    }

    // Arrow pointing into target
    canvas.set(to.x + to.width, to_y, style.arrow_left);
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Calculate the visual center x-coordinate of a node.
/// Uses (width-1)/2 for proper centering of odd-width boxes.
#[inline]
pub fn center_x(node: &Node) -> usize {
    node.x + (node.width.saturating_sub(1)) / 2
}

/// Calculate the y-coordinate for the horizontal segment of an L-shaped edge.
/// Spreads multiple edges slightly to avoid perfect overlap.
fn calculate_mid_y(start_y: usize, end_y: usize, edge_index: usize) -> usize {
    let base_mid = start_y + (end_y.saturating_sub(start_y)) / 2;
    let offset = (edge_index % 3) as isize - 1; // -1, 0, or 1
    let mid_y = (base_mid as isize + offset).max(start_y as isize + 1) as usize;
    mid_y.min(end_y.saturating_sub(1)) // Leave room for vertical to target
}

/// Select the appropriate corner character for an L-shaped edge.
///
/// - `from_x`: position where we're placing the corner
/// - `to_x`: position we're connecting to
/// - `is_source`: true for source corner (vertical->horizontal), false for target (horizontal->vertical)
fn corner_char(from_x: usize, to_x: usize, is_source: bool, s: &StyleChars) -> char {
    match (from_x < to_x, is_source) {
        (true, true) => s.corner_ul,   // Source going right: └
        (false, true) => s.corner_ur,  // Source going left:  ┘
        (false, false) => s.corner_dr, // Target from left:   ┐
        (true, false) => s.corner_dl,  // Target from right:  ┌
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::canvas::Canvas;
    use crate::style::BorderStyle;

    #[test]
    fn expanded_edge_connects_when_targets_on_one_side() {
        let chars = BorderStyle::Unicode.chars();
        let mut canvas = Canvas::new(80, 40);

        let src = Node {
            id: "S".into(),
            label: "S".into(),
            click_target: None,
            x: 2,
            y: 2,
            width: 7,
            rank: 0,
        };
        let t1 = Node {
            id: "T1".into(),
            label: "T1".into(),
            click_target: None,
            x: 30,
            y: 12,
            width: 7,
            rank: 1,
        };
        let t2 = Node {
            id: "T2".into(),
            label: "T2".into(),
            click_target: None,
            x: 40,
            y: 12,
            width: 7,
            rank: 1,
        };

        let stem_start_y = src.y + BOX_HEIGHT;
        let junction_y = stem_start_y + EDGE_STEM_HEIGHT;

        route_expanded_edge(&src, &[&t1, &t2], &mut canvas, chars);

        // Junction row must include the source center so the stem connects
        let stem_x = center_x(&src);
        assert_eq!(
            canvas.get(stem_x, junction_y),
            chars.junction_up,
            "expected stem to connect into junction span"
        );
    }
}
