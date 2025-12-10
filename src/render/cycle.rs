//! Cycle edge routing and node geometry helpers.
//!
//! This module provides:
//! - `route_cycle_edge`: Routes cycle edges through the gutter (right for TD/BT, bottom for LR/RL)
//! - `center_x`, `center_y`: Calculate visual center coordinates of nodes

use crate::graph::Node;
use crate::style::{StyleChars, BOX_HEIGHT, CYCLE_GUTTER};

use super::canvas::Canvas;

// ============================================================================
// Cycle Edge Routing
// ============================================================================

/// Route a cycle edge through the appropriate gutter.
///
/// For TD/BT: Routes through right gutter (horizontal → vertical → horizontal)
/// For LR/RL: Routes through bottom gutter (vertical → horizontal → vertical)
pub fn route_cycle_edge(
    from: &Node,
    to: &Node,
    canvas: &mut Canvas,
    style: &StyleChars,
    direction: crate::graph::Direction,
) {
    use crate::graph::Direction;

    if !canvas.is_visible(from) || !canvas.is_visible(to) {
        return;
    }

    match direction {
        Direction::TD | Direction::TB | Direction::BT => {
            // Vertical layout: use right gutter
            if canvas.width <= CYCLE_GUTTER {
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
            let arrow_char = if direction == Direction::BT {
                style.arrow_down // BT: arrow points down to re-enter the flow
            } else {
                style.arrow_left // TD/TB: arrow points left
            };
            canvas.set(to.x + to.width, to_y, arrow_char);
        }
        Direction::LR | Direction::RL => {
            // Horizontal layout: use bottom gutter for back-edge
            // Back-edge goes: down from source → horizontal in gutter → up to target

            // Calculate a gutter position below the nodes
            let nodes_bottom = from.y.max(to.y) + BOX_HEIGHT;
            let gutter_y = nodes_bottom + 2; // Add some spacing below nodes

            if gutter_y >= canvas.height {
                // Not enough space for gutter, skip
                return;
            }

            let from_x = from.x + from.width / 2;
            let to_x = to.x + to.width / 2;

            // Vertical line from source box bottom down to gutter
            for y in (from.y + BOX_HEIGHT)..=gutter_y {
                canvas.set_edge_char(from_x, y, style.back_v, style);
            }

            // Horizontal line in gutter connecting the two vertical lines
            let (left, right) = if from_x < to_x {
                (from_x, to_x)
            } else {
                (to_x, from_x)
            };
            for x in left..=right {
                canvas.set_edge_char(x, gutter_y, style.back_h, style);
            }

            // Vertical line from gutter up to target box bottom
            for y in (to.y + BOX_HEIGHT)..=gutter_y {
                canvas.set_edge_char(to_x, y, style.back_v, style);
            }

            // Arrow pointing into target's bottom (where it enters from the gutter)
            // The back-edge enters from below, so we draw an up-arrow at the target's bottom
            let target_bottom_y = to.y + BOX_HEIGHT;

            // Draw corner at target's bottom where vertical meets
            canvas.set_edge_char(to_x, target_bottom_y, style.corner_ul, style);

            // For visibility, place an up-arrow indicator near the target
            if target_bottom_y > 0 {
                canvas.set(to_x, target_bottom_y.saturating_sub(1), style.arrow_up);
            }
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Calculate the visual center x-coordinate of a node.
#[inline]
pub fn center_x(node: &Node) -> usize {
    node.center_x()
}

/// Calculate the visual center y-coordinate of a node.
#[inline]
pub fn center_y(node: &Node) -> usize {
    node.center_y()
}

#[cfg(test)]
mod tests {
    use super::super::canvas::Canvas;
    use super::*;
    use crate::graph::Direction;
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
    // Cycle Edge Routing Tests
    // ==========================================================================

    #[test]
    fn cycle_edge_routes_through_gutter_td() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(80, 40);

        let src = make_node("S", 10, 15, 7);
        let target = make_node("T", 10, 2, 7); // Target is ABOVE source (back-edge)

        route_cycle_edge(&src, &target, &mut canvas, &chars, Direction::TD);

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
    fn cycle_edge_invisible_nodes_is_noop() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(20, 20); // Small canvas

        // Nodes outside canvas bounds
        let src = make_node("S", 100, 100, 7);
        let target = make_node("T", 100, 50, 7);

        route_cycle_edge(&src, &target, &mut canvas, &chars, Direction::TD);

        // Nothing should be drawn (gutter would be at x=18)
        assert_eq!(canvas.get(18, 10), ' ');
    }

    // ==========================================================================
    // Helper Function Tests
    // ==========================================================================

    #[test]
    fn center_x_calculates_correctly() {
        // Odd width: 7/2 = 3, so center is at x+3
        let node_odd = make_node("A", 10, 0, 7);
        assert_eq!(center_x(&node_odd), 13); // 10 + 3

        // Even width: 8/2 = 4, so center is at x+4
        let node_even = make_node("B", 10, 0, 8);
        assert_eq!(center_x(&node_even), 14); // 10 + 4

        // Width 1: 1/2 = 0, center is at x
        let node_min = make_node("C", 10, 0, 1);
        assert_eq!(center_x(&node_min), 10);
    }
}
