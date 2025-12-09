//! Edge routing algorithms for connecting nodes.
//!
//! Handles three types of edge routing:
//! - **Expanded**: Multi-target with vertical stems and junction spans (new)
//! - **Straight**: Direct vertical line when nodes are aligned
//! - **L-shaped**: Horizontal then vertical when nodes are offset
//! - **Back-edge**: Cycle edges routed through the right gutter

use crate::graph::Node;
use crate::style::{StyleChars, BOX_HEIGHT, EDGE_STEM_HEIGHT, EDGE_STEM_WIDTH_LR, RIGHT_GUTTER};

use super::canvas::{is_arrow, is_vertical, Canvas};

// ============================================================================
// Public Edge Routing
// ============================================================================

/// Route edges from a single source to multiple targets using expanded layout (horizontal).
///
/// For LR (left-to-right) diagrams.
/// Draws: horizontal stem → vertical junction → horizontal lines → arrows
pub fn route_expanded_edge_horizontal(
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

    let src_center_y = center_y(from);
    let stem_start_x = from.x + from.width;
    let junction_x = stem_start_x + EDGE_STEM_WIDTH_LR;

    // Get destination centers, sorted top to bottom
    let mut dest_centers: Vec<usize> = visible_targets.iter().map(|n| center_y(n)).collect();
    dest_centers.sort();

    // Single target: draw horizontal stem, optional vertical, then arrow
    if dest_centers.len() == 1 {
        let target = visible_targets[0];
        let dest_y = dest_centers[0];
        let target_arrow_x = target.x.saturating_sub(1);

        if src_center_y == dest_y {
            // Aligned: draw horizontal stem to arrow
            for x in stem_start_x..target_arrow_x {
                canvas.set_edge_char(x, dest_y, style.edge_h, style);
            }
        } else {
            // Not aligned: L-shaped route
            // Horizontal stem from source
            for x in stem_start_x..junction_x {
                canvas.set_edge_char(x, src_center_y, style.edge_h, style);
            }
            // Vertical span - skip corner positions
            let (top, bottom) = if src_center_y < dest_y {
                (src_center_y, dest_y)
            } else {
                (dest_y, src_center_y)
            };
            for y in top..=bottom {
                if y != src_center_y && y != dest_y {
                    canvas.set_edge_char(junction_x, y, style.edge_v, style);
                }
            }
            // Corners - horizontal line turns to vertical
            if src_center_y < dest_y {
                canvas.set_edge_char(junction_x, src_center_y, style.corner_dl, style); // └ (turn down)
                canvas.set_edge_char(junction_x, dest_y, style.corner_ur, style);      // ┐ (turn right)
            } else {
                canvas.set_edge_char(junction_x, src_center_y, style.corner_ul, style); // ┌ (turn up)  
                canvas.set_edge_char(junction_x, dest_y, style.corner_dr, style);      // ┘ (turn right)
            }
            // Horizontal line from corner to arrow
            for x in (junction_x + 1)..target_arrow_x {
                canvas.set_edge_char(x, dest_y, style.edge_h, style);
            }
        }
        // Arrow right before target box
        canvas.set(target_arrow_x, dest_y, style.arrow_right);
        return;
    }

    // Multiple targets: horizontal stem → vertical junction → horizontal lines → arrows
    let top_y = *dest_centers.first().unwrap();
    let bottom_y = *dest_centers.last().unwrap();
    let span_top = top_y.min(src_center_y);
    let span_bottom = bottom_y.max(src_center_y);

    // Phase 1: Draw source stem (from source right edge to junction)
    for x in stem_start_x..junction_x {
        canvas.set_edge_char(x, src_center_y, style.edge_h, style);
    }

    // Phase 2: Draw junction at stem/span intersection
    let junction_char = if src_center_y < top_y {
        style.junction_right  // ├ (stem enters from left, continues down)
    } else if src_center_y > bottom_y {
        style.junction_right  // ├ (stem enters from left, continues up)
    } else {
        style.junction_right  // ├ (stem enters from left, splits both ways)
    };
    canvas.set_edge_char(junction_x, src_center_y, junction_char, style);

    // Phase 3: Draw vertical span (connecting all targets)
    for y in span_top..=span_bottom {
        let c = canvas.get(junction_x, y);
        // Skip if we already placed a junction or corner
        if c == ' ' || is_vertical(c, style) {
            canvas.set_edge_char(junction_x, y, style.edge_v, style);
        }
    }

    // Phase 4: Draw horizontal lines and arrows to each target
    for target in visible_targets {
        let dest_y = center_y(target);
        let arrow_x = target.x.saturating_sub(1);

        // Place T-junction at the vertical span for this target (├ = stem goes right from vertical)
        canvas.set_edge_char(junction_x, dest_y, style.junction_right, style);

        // Horizontal line from junction to arrow
        for x in (junction_x + 1)..arrow_x {
            canvas.set_edge_char(x, dest_y, style.edge_h, style);
        }

        // Arrow pointing right into target
        canvas.set(arrow_x, dest_y, style.arrow_right);
    }
}

/// Route edges from a single source to multiple targets using expanded layout.
///
/// For TD (top-down) diagrams.
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

    // Get destination centers, sorted left to right
    let mut dest_centers: Vec<usize> = visible_targets.iter().map(|n| center_x(n)).collect();
    dest_centers.sort();

    // Single target: draw stem, optional horizontal, then arrow
    // Arrow is always right above target box (target.y - 1)
    if dest_centers.len() == 1 {
        let target = visible_targets[0];
        let dest_x = dest_centers[0];
        let target_arrow_y = target.y.saturating_sub(1);

        if src_center_x == dest_x {
            // Aligned: draw vertical stem down to arrow
            for y in stem_start_y..target_arrow_y {
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
            // Corners
            if src_center_x < dest_x {
                canvas.set_edge_char(src_center_x, junction_y, style.corner_ul, style);
                canvas.set_edge_char(dest_x, junction_y, style.corner_dr, style);
            } else {
                canvas.set_edge_char(src_center_x, junction_y, style.corner_ur, style);
                canvas.set_edge_char(dest_x, junction_y, style.corner_dl, style);
            }
            // Vertical drop from corner to arrow
            for y in (junction_y + 1)..target_arrow_y {
                canvas.set_edge_char(dest_x, y, style.edge_v, style);
            }
        }
        // Arrow right above target box
        canvas.set(dest_x, target_arrow_y, style.arrow_down);
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
    // Arrow positioned right above each target box
    for target in &visible_targets {
        let dest_x = center_x(target);
        let target_arrow_y = target.y.saturating_sub(1);
        // Draw vertical drop from junction to arrow
        for y in (junction_y + 1)..target_arrow_y {
            canvas.set_edge_char(dest_x, y, style.edge_v, style);
        }
        // Place arrow right above target box
        canvas.set(dest_x, target_arrow_y, style.arrow_down);
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

pub fn center_y(node: &Node) -> usize {
    node.y + BOX_HEIGHT / 2
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
    use super::super::canvas::Canvas;
    use super::*;
    use crate::style::{BaseStyle, CompositeStyle};

    fn make_node(id: &str, x: usize, y: usize, width: usize) -> Node {
        Node {
            id: id.into(),
            label: id.into(),
            shape: crate::graph::NodeShape::Rectangle,
            click_target: None,
            x,
            y,
            width,
            rank: 0,
        }
    }

    fn unicode_chars() -> StyleChars {
        CompositeStyle::default().to_style_chars(BaseStyle::Unicode)
    }

    // ==========================================================================
    // Expanded Edge Routing Tests
    // ==========================================================================

    #[test]
    fn expanded_edge_connects_when_targets_on_one_side() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(80, 40);

        let src = make_node("S", 2, 2, 7);
        let t1 = make_node("T1", 30, 12, 7);
        let t2 = make_node("T2", 40, 12, 7);

        let stem_start_y = src.y + BOX_HEIGHT;
        let junction_y = stem_start_y + EDGE_STEM_HEIGHT;

        route_expanded_edge(&src, &[&t1, &t2], &mut canvas, &chars);

        // Junction row must include the source center so the stem connects
        let stem_x = center_x(&src);
        assert_eq!(
            canvas.get(stem_x, junction_y),
            chars.junction_up,
            "expected stem to connect into junction span"
        );
    }

    #[test]
    fn expanded_edge_single_target_aligned() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(80, 40);

        // Source and target aligned vertically
        let src = make_node("S", 10, 2, 7);
        let target = make_node("T", 10, 12, 7);

        route_expanded_edge(&src, &[&target], &mut canvas, &chars);

        // Arrow should be right above target box
        let arrow_y = target.y.saturating_sub(1);
        let edge_x = center_x(&target);
        assert_eq!(canvas.get(edge_x, arrow_y), chars.arrow_down);

        // Vertical line should connect source to arrow
        let stem_start_y = src.y + BOX_HEIGHT;
        assert_eq!(canvas.get(edge_x, stem_start_y), chars.edge_v);
    }

    #[test]
    fn expanded_edge_single_target_offset() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(80, 40);

        // Source and target NOT aligned (L-shaped routing)
        let src = make_node("S", 5, 2, 7);
        let target = make_node("T", 30, 12, 7);

        route_expanded_edge(&src, &[&target], &mut canvas, &chars);

        // Arrow should be right above target box
        let arrow_y = target.y.saturating_sub(1);
        let target_x = center_x(&target);
        assert_eq!(canvas.get(target_x, arrow_y), chars.arrow_down);

        // Junction should have corner characters
        let junction_y = src.y + BOX_HEIGHT + EDGE_STEM_HEIGHT;
        let src_x = center_x(&src);

        // Source corner (going right)
        assert_eq!(canvas.get(src_x, junction_y), chars.corner_ul);
        // Target corner (coming from left)
        assert_eq!(canvas.get(target_x, junction_y), chars.corner_dr);
    }

    #[test]
    fn expanded_edge_empty_targets_is_noop() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(80, 40);
        let src = make_node("S", 10, 2, 7);

        route_expanded_edge(&src, &[], &mut canvas, &chars);

        // Canvas should still be empty (only spaces)
        assert_eq!(canvas.get(center_x(&src), src.y + BOX_HEIGHT), ' ');
    }

    // ==========================================================================
    // Back-Edge Routing Tests
    // ==========================================================================

    #[test]
    fn back_edge_routes_through_gutter() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(80, 40);

        let src = make_node("S", 10, 15, 7);
        let target = make_node("T", 10, 2, 7); // Target is ABOVE source (back-edge)

        route_back_edge(&src, &target, &mut canvas, &chars);

        // Back-edge uses right gutter
        let gutter_x = canvas.width - 2;

        // Vertical line should exist in gutter
        let src_mid_y = src.y + BOX_HEIGHT / 2;
        let target_mid_y = target.y + BOX_HEIGHT / 2;
        assert_eq!(canvas.get(gutter_x, src_mid_y), chars.back_v);
        assert_eq!(canvas.get(gutter_x, target_mid_y), chars.back_v);

        // Arrow should point into target
        assert_eq!(
            canvas.get(target.x + target.width, target_mid_y),
            chars.arrow_left
        );
    }

    #[test]
    fn back_edge_invisible_nodes_is_noop() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(20, 20); // Small canvas

        // Nodes outside canvas bounds
        let src = make_node("S", 100, 100, 7);
        let target = make_node("T", 100, 50, 7);

        route_back_edge(&src, &target, &mut canvas, &chars);

        // Nothing should be drawn (gutter would be at x=18)
        assert_eq!(canvas.get(18, 10), ' ');
    }

    // ==========================================================================
    // Helper Function Tests
    // ==========================================================================

    #[test]
    fn center_x_calculates_correctly() {
        // Odd width: (7-1)/2 = 3, so center is at x+3
        let node_odd = make_node("A", 10, 0, 7);
        assert_eq!(center_x(&node_odd), 13); // 10 + 3

        // Even width: (8-1)/2 = 3, so center is at x+3
        let node_even = make_node("B", 10, 0, 8);
        assert_eq!(center_x(&node_even), 13); // 10 + 3

        // Width 1: (1-1)/2 = 0, center is at x
        let node_min = make_node("C", 10, 0, 1);
        assert_eq!(center_x(&node_min), 10);
    }

    #[test]
    fn corner_char_selects_correct_direction() {
        let s = unicode_chars();

        // Source going right: └
        assert_eq!(corner_char(5, 10, true, &s), s.corner_ul);
        // Source going left: ┘
        assert_eq!(corner_char(10, 5, true, &s), s.corner_ur);
        // Target from left: ┐
        assert_eq!(corner_char(10, 5, false, &s), s.corner_dr);
        // Target from right: ┌
        assert_eq!(corner_char(5, 10, false, &s), s.corner_dl);
    }

    // ==========================================================================
    // Route Edge Tests (L-shaped routing)
    // ==========================================================================

    #[test]
    fn route_edge_straight_vertical() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(80, 40);

        let src = make_node("S", 10, 2, 7);
        let target = make_node("T", 10, 12, 7);
        let all_nodes = [&src, &target];

        route_edge(&src, &target, 0, &mut canvas, &chars, &all_nodes);

        // Arrow at target
        let arrow_y = target.y.saturating_sub(1);
        let edge_x = center_x(&target);
        assert_eq!(canvas.get(edge_x, arrow_y), chars.arrow_down);

        // Vertical line connecting
        let start_y = src.y + BOX_HEIGHT;
        assert_eq!(canvas.get(edge_x, start_y), chars.edge_v);
    }

    #[test]
    fn route_edge_l_shaped_horizontal_then_vertical() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(80, 40);

        let src = make_node("S", 5, 2, 7);
        let target = make_node("T", 30, 12, 7);
        let all_nodes = [&src, &target];

        route_edge(&src, &target, 0, &mut canvas, &chars, &all_nodes);

        // Arrow at target
        let arrow_y = target.y.saturating_sub(1);
        let target_x = center_x(&target);
        assert_eq!(canvas.get(target_x, arrow_y), chars.arrow_down);

        // Source should have vertical stem
        let src_x = center_x(&src);
        let start_y = src.y + BOX_HEIGHT;
        assert_eq!(canvas.get(src_x, start_y), chars.edge_v);
    }
}
