//! Unified, direction-agnostic edge routing.
//!
//! This module provides a single edge routing algorithm that works for all
//! diagram orientations (TD, LR, BT, RL) using the orientation abstraction.

mod boundary_fan_in;
mod bt_multi_entry;
mod bt_parallel;
mod bt_parallel_sibling;
mod bt_sibling_scene;
mod bt_sibling_target;
mod convergence;
mod database_fan_in;
mod dedicated_fan_in;
mod dense_crossing;
mod diamond;
mod edge_primitives;
mod fan_in_identity;
mod fanout;
mod lr_rl_sibling_chain;
mod lr_rl_sibling_target;
mod sibling_subgraph_fan_in;
mod subgraph;
mod td_sibling_target;
mod vertical_fan_in;
mod wide_terminal_fan_in;

use crate::graph::{EdgeKind, Graph};
use crate::style::StyleChars;

pub(super) use super::canvas;
use super::canvas::Canvas;
use super::provenance::edge_owner_id;
use super::semantic::{CellOwnerKind, CellRole};
pub(super) use boundary_fan_in::plan_boundary_fan_in_scene;
pub(super) use bt_multi_entry::plan_bt_multi_entry_scene;
pub(super) use bt_parallel::plan_bt_parallel_scene;
pub(super) use bt_parallel_sibling::plan_bt_parallel_sibling_scene;
pub(super) use bt_sibling_scene::{
    direct_parallel_sibling_subgraph_ids, plan_bt_sibling_scene, sibling_target_entry_subgraph_ids,
    strict_chain_subgraph_ids,
};
pub(super) use bt_sibling_target::plan_bt_sibling_target_scene;
pub use convergence::route_convergent_edges;
pub(super) use database_fan_in::{
    repair_database_source_border, route_database_intermediate_scene,
};
pub(super) use dedicated_fan_in::route_dedicated_fan_in_edges;
pub(super) use dense_crossing::plan_dense_crossing_scenes;
pub(super) use diamond::plan_diamond_scenes;
pub use edge_primitives::edge_exit_point;
pub(super) use edge_primitives::{edge_entry_candidates, is_subgraph_title_cell};
#[cfg(test)]
use edge_primitives::{edge_entry_point, hits_foreign_subgraph_border};
pub(super) use fan_in_identity::{
    route_bt_parallel_identity_edges, route_fan_in_identity_edges,
    route_vertical_branch_rejoin_identity_edges,
};
pub use fanout::route_divergent_edges;
pub(super) use lr_rl_sibling_chain::plan_lr_rl_sibling_chain_scene;
pub(super) use lr_rl_sibling_target::plan_lr_rl_sibling_target_scene;
pub(super) use sibling_subgraph_fan_in::plan_sibling_subgraph_fan_in_scene;
pub(super) use td_sibling_target::plan_td_sibling_target_scene;
pub(super) use vertical_fan_in::route_vertical_fan_in_edges;
pub(super) use wide_terminal_fan_in::route_wide_terminal_fan_in_edges;

const ROUTE_Z_INDEX: u8 = 5;

#[derive(Copy, Clone)]
struct RouteOwner<'a> {
    kind: CellOwnerKind,
    id: &'a str,
}

fn set_route_char(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    ch: char,
    owner: Option<RouteOwner<'_>>,
) {
    if canvas.fallback_route_cell_owned_by_other(x, y, owner.map(|route| route.id)) {
        return;
    }
    if let Some(owner) = owner {
        canvas.set_owned(x, y, ch, owner.kind, owner.id, ROUTE_Z_INDEX);
    } else {
        canvas.set(x, y, ch);
    }
}

fn set_route_endpoint_char(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    ch: char,
    owner: RouteOwner<'_>,
) {
    if canvas.fallback_route_cell_owned_by_other(x, y, Some(owner.id)) {
        return;
    }
    canvas.set_owned_with_role(
        x,
        y,
        ch,
        owner.kind,
        owner.id,
        CellRole::EndpointMarker,
        ROUTE_Z_INDEX,
    );
}

fn set_route_edge_char(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    ch: char,
    style: &StyleChars,
    owner: Option<RouteOwner<'_>>,
) {
    if canvas.fallback_route_cell_owned_by_other(x, y, owner.map(|route| route.id)) {
        return;
    }
    if let Some(owner) = owner {
        canvas.set_edge_char_owned(x, y, ch, style, owner.kind, owner.id, ROUTE_Z_INDEX);
    } else {
        canvas.set_edge_char(x, y, ch, style);
    }
}

fn style_for_edge_kind(style: &StyleChars, kind: EdgeKind) -> StyleChars {
    let mut branch_style = *style;
    match kind {
        EdgeKind::Thick => {
            // The precomputed route uses heavy Unicode shafts. Keep the same
            // semantic distinction in fallback routes, with readable ASCII
            // approximations when the base style is ASCII-like.
            branch_style.edge_h = if style.edge_h == '-' { '=' } else { '━' };
            branch_style.edge_v = if style.edge_v == '|' { '|' } else { '┃' };
        }
        EdgeKind::Dotted => {
            branch_style.edge_h = if style.edge_h == '-' {
                '.'
            } else {
                style.dotted_h
            };
            branch_style.edge_v = style.dotted_v;
        }
        EdgeKind::Arrow
        | EdgeKind::Open
        | EdgeKind::Bidirectional
        | EdgeKind::CircleEnd
        | EdgeKind::CrossEnd => {}
    }
    branch_style
}

fn edge_route_owner_id(graph: &Graph, from_id: &str, to_id: &str) -> String {
    graph
        .edges
        .iter()
        .enumerate()
        .find_map(|(idx, edge)| {
            (!edge.is_back_edge && edge.from == from_id && edge.to == to_id)
                .then(|| edge_owner_id(idx, edge))
        })
        .unwrap_or_else(|| format!("edge:?:{from_id}->{to_id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Direction, Graph, Node, NodeShape, Rectangle, Subgraph};
    use crate::style::{ASCII_CHARS, UNICODE_CHARS};

    #[test]
    fn fallback_branch_style_preserves_thick_and_dotted_shafts() {
        let ascii_thick = style_for_edge_kind(&ASCII_CHARS, EdgeKind::Thick);
        assert_eq!(ascii_thick.edge_h, '=');
        assert_eq!(ascii_thick.edge_v, '|');

        let ascii_dotted = style_for_edge_kind(&ASCII_CHARS, EdgeKind::Dotted);
        assert_eq!(ascii_dotted.edge_h, '.');
        assert_eq!(ascii_dotted.edge_v, ':');

        let unicode_thick = style_for_edge_kind(&UNICODE_CHARS, EdgeKind::Thick);
        assert_eq!(unicode_thick.edge_h, '━');
        assert_eq!(unicode_thick.edge_v, '┃');

        let unicode_dotted = style_for_edge_kind(&UNICODE_CHARS, EdgeKind::Dotted);
        assert_eq!(unicode_dotted.edge_h, '╌');
        assert_eq!(unicode_dotted.edge_v, '╎');
    }

    fn make_node(id: &str, x: usize, y: usize, width: usize, height: usize) -> Node {
        let mut n = Node::new(id, id);
        n.x = x;
        n.y = y;
        n.width = width;
        n.height = height;
        n
    }

    // =========================================================================
    // edge_exit_point — all 4 directions
    // =========================================================================

    #[test]
    fn exit_point_td_is_bottom_center() {
        // Node at (10, 5), width=6, height=3 → bottom_y = 5+3=8, center_x = 10+3=13
        let n = make_node("a", 10, 5, 6, 3);
        assert_eq!(edge_exit_point(&n, Direction::TD), (13, 8));
    }

    #[test]
    fn exit_point_lr_is_right_center() {
        let n = make_node("a", 10, 5, 6, 3);
        // LR: right edge = x+width = 16, center_y = y + h/2 = 5+1 = 6
        assert_eq!(edge_exit_point(&n, Direction::LR), (16, 6));
    }

    #[test]
    fn exit_point_rl_is_left_center() {
        let n = make_node("a", 10, 5, 6, 3);
        // RL: left edge = x.saturating_sub(1) = 9, center_y = 6
        assert_eq!(edge_exit_point(&n, Direction::RL), (9, 6));
    }

    #[test]
    fn exit_point_bt_is_top_center() {
        let n = make_node("a", 10, 5, 6, 3);
        // BT: y.saturating_sub(1) = 4, center_x = 13
        assert_eq!(edge_exit_point(&n, Direction::BT), (13, 4));
    }

    #[test]
    fn exit_point_rl_at_x0_saturates() {
        let n = make_node("a", 0, 0, 6, 3);
        // x.saturating_sub(1) = 0
        assert_eq!(edge_exit_point(&n, Direction::RL), (0, 1));
    }

    // =========================================================================
    // edge_entry_point — all 4 directions
    // =========================================================================

    #[test]
    fn entry_point_td_is_above_center() {
        let n = make_node("a", 10, 5, 6, 3);
        // TD: center_x=13, y.saturating_sub(1)=4
        assert_eq!(edge_entry_point(&n, Direction::TD), (13, 4));
    }

    #[test]
    fn entry_point_lr_is_left_center() {
        let n = make_node("a", 10, 5, 6, 3);
        // LR: x.saturating_sub(1)=9, center_y=6
        assert_eq!(edge_entry_point(&n, Direction::LR), (9, 6));
    }

    #[test]
    fn entry_point_rl_is_right_center() {
        let n = make_node("a", 10, 5, 6, 3);
        // RL: x+width=16, center_y=6
        assert_eq!(edge_entry_point(&n, Direction::RL), (16, 6));
    }

    #[test]
    fn entry_point_bt_is_below_center() {
        let n = make_node("a", 10, 5, 6, 3);
        // BT: center_x=13, bottom_y=8
        assert_eq!(edge_entry_point(&n, Direction::BT), (13, 8));
    }

    #[test]
    fn diamond_entry_point_has_one_cell_visual_clearance_in_all_directions() {
        let mut n = make_node("diamond", 10, 5, 6, 3);
        n.shape = NodeShape::Diamond;

        assert_eq!(edge_entry_point(&n, Direction::TD), (13, 3));
        assert_eq!(edge_entry_point(&n, Direction::BT), (13, 9));
        assert_eq!(edge_entry_point(&n, Direction::LR), (8, 6));
        assert_eq!(edge_entry_point(&n, Direction::RL), (17, 6));
    }

    #[test]
    fn database_entry_point_uses_generic_one_cell_receiver_entry() {
        let mut n = make_node("database", 10, 5, 6, 3);
        n.shape = NodeShape::Database;

        assert_eq!(edge_entry_point(&n, Direction::TD), (13, 4));
        assert_eq!(edge_entry_point(&n, Direction::TB), (13, 4));
        assert_eq!(edge_entry_point(&n, Direction::BT), (13, 8));
        assert_eq!(edge_entry_point(&n, Direction::LR), (9, 6));
        assert_eq!(edge_entry_point(&n, Direction::RL), (16, 6));
    }

    #[test]
    fn diamond_entry_point_clearance_saturates_at_canvas_origin() {
        let mut n = make_node("diamond", 0, 0, 1, 3);
        n.shape = NodeShape::Diamond;

        assert_eq!(edge_entry_point(&n, Direction::TD), (0, 0));
        assert_eq!(edge_entry_point(&n, Direction::LR), (0, 1));
    }

    #[test]
    fn asymmetric_lr_entry_point_separates_left_point_from_arrow() {
        let mut n = make_node("flag", 10, 5, 6, 3);
        n.shape = NodeShape::Asymmetric;

        assert_eq!(edge_entry_point(&n, Direction::LR), (8, 6));
        assert_eq!(edge_entry_point(&n, Direction::RL), (16, 6));
        assert_eq!(edge_entry_point(&n, Direction::TD), (13, 4));
        assert_eq!(edge_entry_point(&n, Direction::BT), (13, 8));
    }

    // exit_point and entry_point are symmetric for the same node/direction
    #[test]
    fn exit_and_entry_points_are_symmetric() {
        let n = make_node("a", 10, 5, 6, 3);
        assert_eq!(
            edge_exit_point(&n, Direction::TD),
            edge_entry_point(&n, Direction::BT)
        );
        assert_eq!(
            edge_exit_point(&n, Direction::LR),
            edge_entry_point(&n, Direction::RL)
        );
        assert_eq!(
            edge_exit_point(&n, Direction::RL),
            edge_entry_point(&n, Direction::LR)
        );
        assert_eq!(
            edge_exit_point(&n, Direction::BT),
            edge_entry_point(&n, Direction::TD)
        );
    }

    // =========================================================================
    // hits_foreign_subgraph_border
    // =========================================================================

    fn graph_with_foreign_subgraph(sg_x: usize, sg_y: usize, sg_w: usize, sg_h: usize) -> Graph {
        let mut g = Graph::new();
        let mut sg = Subgraph::new("foreign", None);
        sg.bounds = Rectangle::new(sg_x, sg_y, sg_w, sg_h);
        g.add_subgraph(sg);
        g
    }

    #[test]
    fn hits_border_on_top_edge() {
        // Subgraph at (10,5) size 8×6 → top border y=5
        let g = graph_with_foreign_subgraph(10, 5, 8, 6);
        let n = make_node("n", 0, 0, 4, 3); // not in any subgraph
        assert!(hits_foreign_subgraph_border(&n, 14, 5, &g)); // x=14 in [10..17], y=5 = min_y
    }

    #[test]
    fn hits_border_on_left_edge() {
        let g = graph_with_foreign_subgraph(10, 5, 8, 6);
        let n = make_node("n", 0, 0, 4, 3);
        assert!(hits_foreign_subgraph_border(&n, 10, 8, &g)); // x=10 = min_x
    }

    #[test]
    fn no_hit_interior_of_subgraph() {
        let g = graph_with_foreign_subgraph(10, 5, 8, 6);
        let n = make_node("n", 0, 0, 4, 3);
        // (13, 8) is strictly inside the box — not on any border
        assert!(!hits_foreign_subgraph_border(&n, 13, 8, &g));
    }

    #[test]
    fn no_hit_outside_subgraph() {
        let g = graph_with_foreign_subgraph(10, 5, 8, 6);
        let n = make_node("n", 0, 0, 4, 3);
        assert!(!hits_foreign_subgraph_border(&n, 5, 5, &g)); // left of subgraph
        assert!(!hits_foreign_subgraph_border(&n, 20, 8, &g)); // right of subgraph
    }

    #[test]
    fn no_hit_for_own_subgraph() {
        // Node is in the same subgraph — should NOT count as a hit
        let mut g = Graph::new();
        let mut sg = Subgraph::new("own", None);
        sg.bounds = Rectangle::new(10, 5, 8, 6);
        g.add_subgraph(sg);
        g.add_node(make_node("n", 12, 6, 4, 3));
        g.associate_node_with_subgraph("n", "own");
        let n = g.get_node("n").expect("node 'n' was just added");
        assert!(!hits_foreign_subgraph_border(n, 14, 5, &g));
    }

    // =========================================================================
    // edge_entry_candidates — TD/BT: center-first, expanding outward
    // =========================================================================

    #[test]
    fn entry_candidates_td_starts_at_center() {
        let n = make_node("a", 10, 5, 6, 3);
        let candidates = edge_entry_candidates(&n, Direction::TD);
        // First candidate should be center_x, y-1
        assert!(!candidates.is_empty());
        let center_x = n.center_x(); // 10 + 3 = 13
        assert_eq!(candidates[0], (center_x, n.y.saturating_sub(1)));
    }

    #[test]
    fn entry_candidates_lr_starts_at_center() {
        let n = make_node("a", 10, 5, 6, 3);
        let candidates = edge_entry_candidates(&n, Direction::LR);
        assert!(!candidates.is_empty());
        let center_y = n.center_y(); // 5 + 1 = 6
        assert_eq!(candidates[0], (n.x.saturating_sub(1), center_y));
    }

    #[test]
    fn entry_candidates_no_duplicates() {
        let n = make_node("a", 10, 5, 6, 3);
        for dir in [Direction::TD, Direction::LR, Direction::RL, Direction::BT] {
            let candidates = edge_entry_candidates(&n, dir);
            let mut seen = std::collections::HashSet::new();
            for pt in &candidates {
                assert!(
                    seen.insert(*pt),
                    "duplicate candidate {pt:?} for direction {dir:?}"
                );
            }
        }
    }
}
