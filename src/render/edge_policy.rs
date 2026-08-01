//! Graph-aware policies for normal edge routing.
//!
//! Low-level route primitives remain in [`super::edge`]. This module owns
//! entry-column decisions that depend on graph structure and title spans.

use crate::graph::{Graph, Node};

use super::edge::{edge_entry_candidates, is_subgraph_title_cell};

/// Select a legal target entry column that leaves a visible shaft below a
/// titled TD/TB subgraph title.
pub(super) fn title_safe_td_entry_x(
    target: &Node,
    arrow_x: usize,
    arrow_y: usize,
    stem_start_y: usize,
    graph: &Graph,
) -> usize {
    let Some(target_sg_id) = graph.get_node_subgraph(&target.id) else {
        return arrow_x;
    };
    let Some(target_sg) = graph.get_subgraph(target_sg_id) else {
        return arrow_x;
    };
    let entering_from_above =
        stem_start_y < target_sg.bounds.y && arrow_y >= target_sg.bounds.y.saturating_add(1);
    if !entering_from_above
        || arrow_y == 0
        || !target_sg.has_title()
        || !is_subgraph_title_cell(graph, arrow_x, arrow_y - 1)
    {
        return arrow_x;
    }

    edge_entry_candidates(target, graph.direction)
        .into_iter()
        .filter(|(candidate_x, candidate_y)| {
            *candidate_y == arrow_y && !is_subgraph_title_cell(graph, *candidate_x, arrow_y - 1)
        })
        .min_by_key(|(candidate_x, _)| (candidate_x.abs_diff(arrow_x), *candidate_x))
        .map(|(candidate_x, _)| candidate_x)
        .unwrap_or(arrow_x)
}
