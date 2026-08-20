//! Graph-aware policies for normal edge routing.
//!
//! Low-level route primitives remain in [`super::edge`]. This module owns
//! entry-column decisions that depend on graph structure and title spans.

use crate::graph::{Direction, EdgeKind, Graph, Node, NodeShape};

use crate::portals::{
    td_single_external_entry_uses_literal_gutter_lane, title_margin_for_direction,
    title_safe_portal_x, PortalColumnPreference,
};

use super::edge::{edge_entry_candidates, is_subgraph_title_cell};

/// Return whether an edge belongs to the deliberately narrow H9 route
/// transaction. Unsupported scenes retain their existing route owner.
fn td_single_route_eligible(from: &Node, to: &Node, graph: &Graph) -> bool {
    if !matches!(graph.direction, Direction::TD | Direction::TB)
        || graph.has_cycles()
        || graph.subgraphs.len() != 1
        || graph.edges.iter().filter(|edge| !edge.is_back_edge).count() != 2
        || graph
            .nodes
            .iter()
            .any(|node| !matches!(node.shape, NodeShape::Rectangle))
    {
        return false;
    }

    let Some(subgraph) = graph.subgraphs.first() else {
        return false;
    };
    if subgraph.parent_id.is_some()
        || !subgraph.child_ids.is_empty()
        || !subgraph.has_title()
        || subgraph.node_ids.len() != 1
        || !subgraph.bounds.is_valid()
    {
        return false;
    }
    let Some(internal_id) = subgraph.node_ids.iter().next() else {
        return false;
    };
    let from_inside = graph.get_node_subgraph(&from.id) == Some(subgraph.id.as_str());
    let to_inside = graph.get_node_subgraph(&to.id) == Some(subgraph.id.as_str());
    if from_inside == to_inside
        || (from_inside && from.id != *internal_id)
        || (to_inside && to.id != *internal_id)
    {
        return false;
    }

    let edges: Vec<_> = graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge)
        .collect();
    if edges.iter().any(|edge| {
        edge.label.is_some()
            || edge.kind != EdgeKind::Arrow
            || !graph.nodes.iter().any(|node| node.id == edge.from)
            || !graph.nodes.iter().any(|node| node.id == edge.to)
    }) {
        return false;
    }

    let has_edge = edges
        .iter()
        .any(|edge| edge.from == from.id && edge.to == to.id);
    let has_incoming = edges.iter().any(|edge| {
        graph.get_node_subgraph(&edge.from).is_none()
            && graph.get_node_subgraph(&edge.to) == Some(subgraph.id.as_str())
            && edge.to == *internal_id
    });
    let has_outgoing = edges.iter().any(|edge| {
        graph.get_node_subgraph(&edge.from) == Some(subgraph.id.as_str())
            && graph.get_node_subgraph(&edge.to).is_none()
            && edge.from == *internal_id
    });
    has_edge && has_incoming && has_outgoing
}

/// Select the one incoming portal/node x for the strict H9 route transaction.
/// The returned x is both a title-safe portal lane and a legal target-node
/// entry candidate. `None` leaves the caller on its existing route policy.
pub(super) fn td_single_incoming_route_x(
    from: &Node,
    to: &Node,
    desired_arrow_x: usize,
    arrow_y: usize,
    graph: &Graph,
) -> Option<usize> {
    if !td_single_route_eligible(from, to, graph)
        || graph.get_node_subgraph(&from.id).is_some()
        || graph.get_node_subgraph(&to.id).is_none()
    {
        return None;
    }
    let subgraph_id = graph.get_node_subgraph(&to.id)?;
    let subgraph = graph.get_subgraph(subgraph_id)?;
    let title_margin = if td_single_external_entry_uses_literal_gutter_lane(
        graph,
        &from.id,
        &to.id,
        subgraph_id,
    ) {
        0
    } else {
        title_margin_for_direction(graph.direction)
    };
    let portal_x = title_safe_portal_x(
        subgraph.bounds.x,
        subgraph.bounds.width,
        subgraph.title.as_deref(),
        desired_arrow_x,
        graph.direction,
        title_margin,
        PortalColumnPreference::Directional,
    );
    edge_entry_candidates(to, graph.direction)
        .into_iter()
        .find(|(candidate_x, candidate_y)| {
            *candidate_x == portal_x
                && *candidate_y == arrow_y
                && (arrow_y == 0 || !is_subgraph_title_cell(graph, *candidate_x, arrow_y - 1))
        })
        .map(|(candidate_x, _)| candidate_x)
}

/// Select the outgoing target entry x that continues the internal source exit
/// lane. This is intentionally independent of titled-subgraph portal logic;
/// the strict eligibility guard keeps ordinary target shapes and other scenes
/// on their existing route policy.
pub(super) fn td_single_outgoing_route_x(
    from: &Node,
    to: &Node,
    desired_arrow_x: usize,
    arrow_y: usize,
    graph: &Graph,
) -> Option<usize> {
    if !td_single_route_eligible(from, to, graph)
        || graph.get_node_subgraph(&from.id).is_none()
        || graph.get_node_subgraph(&to.id).is_some()
    {
        return None;
    }
    edge_entry_candidates(to, graph.direction)
        .into_iter()
        .find(|(candidate_x, candidate_y)| {
            *candidate_x == desired_arrow_x && *candidate_y == arrow_y
        })
        .map(|(candidate_x, _)| candidate_x)
}

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
