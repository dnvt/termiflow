//! Cycle edge routing and node geometry helpers.
//!
//! This module provides:
//! - `route_cycle_edge`: Routes cycle edges through the gutter (right for TD/BT, bottom for LR/RL)
//! - `center_x`, `center_y`: Calculate visual center coordinates of nodes

use crate::graph::Node;
use crate::spacing::SpacingConfig;
use crate::style::StyleChars;

use super::canvas::Canvas;
use super::semantic::CellOwnerKind;

const CYCLE_Z_INDEX: u8 = 5;

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
    spacing: &SpacingConfig,
    direction: crate::graph::Direction,
    owner_id: Option<&str>,
) {
    use crate::graph::Direction;

    if !canvas.is_visible(from) || !canvas.is_visible(to) {
        return;
    }

    match direction {
        Direction::TD | Direction::TB | Direction::BT => {
            // Vertical layout: use right gutter
            let cycle_gutter = spacing.cycle_gutter;
            if canvas.width <= cycle_gutter {
                return;
            }

            let gutter_x = canvas.width - 2;
            let from_y = from.center_y();
            let to_y = to.center_y();

            // Horizontal line from source to gutter
            for x in (from.x + from.width)..gutter_x {
                set_cycle_edge_char(canvas, x, from_y, style.back_h, style, owner_id);
            }

            // Vertical line in gutter
            let (top, bottom) = if from_y < to_y {
                (from_y, to_y)
            } else {
                (to_y, from_y)
            };
            for y in top..=bottom {
                set_cycle_edge_char(canvas, gutter_x, y, style.back_v, style, owner_id);
            }

            // Horizontal line from gutter to target
            for x in (to.x + to.width)..gutter_x {
                set_cycle_edge_char(canvas, x, to_y, style.back_h, style, owner_id);
            }

            // Corners at source-to-gutter and gutter-to-target junctions
            // Path: horizontal right from source → vertical in gutter → horizontal left to target
            if from_y < to_y {
                // Downward: right-then-down at source, down-then-left at target
                set_cycle_char(canvas, gutter_x, from_y, style.corner_dr, owner_id); // ┐
                set_cycle_char(canvas, gutter_x, to_y, style.corner_ur, owner_id);
            // ┘
            } else {
                // Upward: right-then-up at source, up-then-left at target
                set_cycle_char(canvas, gutter_x, from_y, style.corner_ur, owner_id); // ┘
                set_cycle_char(canvas, gutter_x, to_y, style.corner_dr, owner_id);
                // ┐
            }

            // Arrow pointing into target - back-edge always enters from right gutter
            set_cycle_char(canvas, to.x + to.width, to_y, style.arrow_left, owner_id);
        }
        Direction::LR | Direction::RL => {
            // Horizontal layout: use bottom gutter for back-edge
            // Back-edge goes: down from source → horizontal in gutter → up to target

            // Self-loop detection: route around the right side of the node
            if from.id == to.id {
                let node_right = from.x + from.width;
                let node_bottom = from.bottom_y();
                let loop_offset = spacing.row_spacing.max(2);
                let gutter_y = node_bottom + loop_offset;

                if gutter_y >= canvas.height {
                    return;
                }

                let center_x = from.x + from.width / 2;
                let loop_right = node_right + 2;

                if loop_right >= canvas.width {
                    return;
                }

                // Place corners FIRST to prevent overlap resolution converting them to crosses.
                // Path: right from node -> down -> left -> up into node bottom.
                set_cycle_char(
                    canvas,
                    loop_right,
                    from.center_y(),
                    style.corner_dr,
                    owner_id,
                );
                set_cycle_char(canvas, loop_right, gutter_y, style.corner_ur, owner_id);
                set_cycle_char(canvas, center_x, gutter_y, style.corner_ul, owner_id);

                // Draw lines EXCLUDING corner positions
                // Horizontal bridge from the node exit to the right-side loop column.
                for x in node_right..loop_right {
                    set_cycle_edge_char(canvas, x, from.center_y(), style.back_h, style, owner_id);
                }
                // Vertical down from right of box (below the top corner)
                for y in (from.center_y() + 1)..gutter_y {
                    set_cycle_edge_char(canvas, loop_right, y, style.back_v, style, owner_id);
                }
                // Horizontal in gutter (between the two corners)
                for x in (center_x + 1)..loop_right {
                    set_cycle_edge_char(canvas, x, gutter_y, style.back_h, style, owner_id);
                }
                // Vertical up from gutter to box bottom (above the bottom corner)
                for y in (node_bottom + 1)..gutter_y {
                    set_cycle_edge_char(canvas, center_x, y, style.back_v, style, owner_id);
                }
                // Place the visible arrow on the outside entry row so the later
                // box redraw doesn't erase it.
                if node_bottom < canvas.height {
                    set_cycle_char(canvas, center_x, node_bottom, style.arrow_up, owner_id);
                }
                return;
            }

            // Calculate a gutter position below the nodes
            let nodes_bottom = from.bottom_y().max(to.bottom_y());
            let gutter_y = nodes_bottom + spacing.row_spacing.max(2); // Add spacing below nodes

            if gutter_y >= canvas.height {
                // Not enough space for gutter, skip
                return;
            }

            let from_x = from.x + from.width / 2;
            let to_x = to.x + to.width / 2;

            // Vertical line from source box bottom down to gutter
            for y in from.bottom_y()..=gutter_y {
                set_cycle_edge_char(canvas, from_x, y, style.back_v, style, owner_id);
            }

            // Horizontal line in gutter connecting the two vertical lines
            let (left, right) = if from_x < to_x {
                (from_x, to_x)
            } else {
                (to_x, from_x)
            };
            for x in left..=right {
                set_cycle_edge_char(canvas, x, gutter_y, style.back_h, style, owner_id);
            }

            // Vertical line from gutter up to target box bottom
            for y in to.bottom_y()..=gutter_y {
                set_cycle_edge_char(canvas, to_x, y, style.back_v, style, owner_id);
            }

            // Arrow pointing into target's bottom (where it enters from the gutter)
            // The back-edge enters from below, so we draw an up-arrow at the target's bottom
            let target_bottom_y = to.bottom_y();

            // Place the visible arrow on the outside entry row so the later box
            // redraw doesn't erase it.
            if target_bottom_y < canvas.height {
                set_cycle_char(canvas, to_x, target_bottom_y, style.arrow_up, owner_id);
            }
        }
    }
}

fn set_cycle_char(canvas: &mut Canvas, x: usize, y: usize, ch: char, owner_id: Option<&str>) {
    if let Some(owner_id) = owner_id {
        canvas.set_owned(x, y, ch, CellOwnerKind::CycleEdge, owner_id, CYCLE_Z_INDEX);
    } else {
        canvas.set(x, y, ch);
    }
}

fn set_cycle_edge_char(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    ch: char,
    style: &StyleChars,
    owner_id: Option<&str>,
) {
    if let Some(owner_id) = owner_id {
        canvas.set_edge_char_owned(
            x,
            y,
            ch,
            style,
            CellOwnerKind::CycleEdge,
            owner_id,
            CYCLE_Z_INDEX,
        );
    } else {
        canvas.set_edge_char(x, y, ch, style);
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
    use crate::spacing::SpacingConfig;
    use crate::style::{BaseStyle, CompositeStyle};

    fn make_node(id: &str, x: usize, y: usize, width: usize) -> Node {
        Node {
            id: id.into(),
            label: id.into(),
            label_lines: Vec::new(),
            shape: crate::graph::NodeShape::Rectangle,
            click_target: None,
            x,
            y,
            width,
            height: crate::style::BOX_HEIGHT,
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

        let spacing = SpacingConfig::default_config();
        route_cycle_edge(
            &src,
            &target,
            &mut canvas,
            &chars,
            &spacing,
            Direction::TD,
            None,
        );

        // Back-edge uses right gutter
        let gutter_x = canvas.width - 2;

        // Corners should exist at junction points in gutter
        let src_mid_y = src.center_y();
        let target_mid_y = target.center_y();
        // src is below target (src_mid_y > target_mid_y), so this is an "upward cycle":
        // corner_ur (┘) at source (right-then-up), corner_dr (┐) at target (up-then-left)
        assert_eq!(canvas.get(gutter_x, src_mid_y), chars.corner_ur);
        assert_eq!(canvas.get(gutter_x, target_mid_y), chars.corner_dr);

        // Vertical line should exist between the corners
        let mid_y = (src_mid_y + target_mid_y) / 2;
        assert_eq!(canvas.get(gutter_x, mid_y), chars.back_v);

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

        let spacing = SpacingConfig::default_config();
        route_cycle_edge(
            &src,
            &target,
            &mut canvas,
            &chars,
            &spacing,
            Direction::TD,
            None,
        );

        // Nothing should be drawn (gutter would be at x=18)
        assert_eq!(canvas.get(18, 10), ' ');
    }

    #[test]
    fn cycle_edge_lr_target_entry_uses_visible_arrow() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(80, 30);

        let target = make_node("A", 8, 2, 9);
        let source = make_node("B", 34, 2, 9);

        let spacing = SpacingConfig::default_config();
        route_cycle_edge(
            &source,
            &target,
            &mut canvas,
            &chars,
            &spacing,
            Direction::LR,
            None,
        );

        let target_entry_x = target.x + target.width / 2;
        let target_bottom_y = target.bottom_y();

        assert_eq!(canvas.get(target_entry_x, target_bottom_y), chars.arrow_up);
        assert_eq!(
            canvas.get(target_entry_x, target_bottom_y + 1),
            chars.back_v
        );
    }

    #[test]
    fn cycle_self_loop_lr_places_visible_arrow_below_box() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(40, 20);

        let node = make_node("Self", 8, 2, 8);

        let spacing = SpacingConfig::default_config();
        route_cycle_edge(
            &node,
            &node,
            &mut canvas,
            &chars,
            &spacing,
            Direction::LR,
            None,
        );

        let center_x = node.x + node.width / 2;
        let loop_right = node.x + node.width + 2;
        let gutter_y = node.bottom_y() + spacing.row_spacing.max(2);
        assert_eq!(
            canvas.get(node.x + node.width, node.center_y()),
            chars.back_h
        );
        assert_eq!(canvas.get(loop_right, node.center_y()), chars.corner_dr);
        assert_eq!(canvas.get(loop_right, gutter_y), chars.corner_ur);
        assert_eq!(canvas.get(center_x, gutter_y), chars.corner_ul);
        assert_eq!(canvas.get(center_x, node.bottom_y()), chars.arrow_up);
        assert_eq!(canvas.get(center_x, node.bottom_y() + 1), chars.back_v);
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
