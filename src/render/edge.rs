//! Edge routing algorithms for connecting nodes.
//!
//! Handles three types of edge routing:
//! - **Straight**: Direct vertical line when nodes are aligned
//! - **L-shaped**: Horizontal then vertical when nodes are offset
//! - **Back-edge**: Cycle edges routed through the right gutter

use crate::graph::Node;
use crate::style::{StyleChars, BOX_HEIGHT, RIGHT_GUTTER};

use super::canvas::{is_arrow, is_vertical, Canvas};

// ============================================================================
// Public Edge Routing
// ============================================================================

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
