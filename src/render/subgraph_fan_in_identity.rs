//! Shared capacity policy for the strict boundary-owned subgraph fan-in scene.
//!
//! This is deliberately separate from ordinary subgraph-free fan-in. A
//! subgraph boundary and its title are owned visual resources, so the policy
//! proves the narrow topology first and only then reuses the centered target
//! port geometry.

use std::collections::HashSet;

use crate::graph::{Direction, EdgeKind, Graph, Node, NodeShape};

use super::fan_in_identity::{minimum_port_span, target_port_columns, target_port_rows};

pub(crate) const MIN_TARGET_PORTS: usize = 3;
pub(crate) const MAX_TARGET_PORTS: usize = 4;

/// Return the incoming-edge count for the exact internal subgraph
/// nonterminal fan-in.
///
/// The boundary-owned fan-in policy below handles an external target.  This
/// selector is intentionally separate: the two incoming edges terminate on a
/// rectangle inside one titled, non-nested subgraph, and that target has one
/// downstream continuation.  The route lowerers can therefore preserve the
/// two local target entries without claiming a subgraph portal or a generic
/// cross-boundary collector.
pub(crate) fn internal_nonterminal_target_port_count(
    graph: &Graph,
    target_id: &str,
) -> Option<usize> {
    if !matches!(
        graph.direction,
        Direction::TD | Direction::TB | Direction::BT | Direction::LR | Direction::RL
    ) || graph.subgraphs.len() != 1
        || graph.has_cycles()
    {
        return None;
    }

    let subgraph = graph.subgraphs.first()?;
    if subgraph.title.is_none() || subgraph.parent_id.is_some() || !subgraph.child_ids.is_empty() {
        return None;
    }

    let target = graph.get_node(target_id)?;
    if target.shape != NodeShape::Rectangle
        || graph.get_node_subgraph(target_id) != Some(subgraph.id.as_str())
    {
        return None;
    }

    let incoming: Vec<_> = graph
        .edges
        .iter()
        .filter(|edge| edge.to == target_id)
        .collect();
    if incoming.len() != 2 {
        return None;
    }

    let source_ids: HashSet<&str> = incoming.iter().map(|edge| edge.from.as_str()).collect();
    if source_ids.len() != incoming.len() || source_ids.contains(target_id) {
        return None;
    }
    if incoming.iter().any(|edge| {
        edge.is_back_edge
            || edge.kind != EdgeKind::Arrow
            || edge.label.is_some()
            || graph.get_node_subgraph(&edge.from) != Some(subgraph.id.as_str())
            || graph
                .get_node(&edge.from)
                .is_none_or(|source| source.shape != NodeShape::Rectangle)
    }) {
        return None;
    }

    let outgoing: Vec<_> = graph
        .edges
        .iter()
        .filter(|edge| edge.from == target_id)
        .collect();
    if outgoing.len() != 1 {
        return None;
    }
    let downstream = outgoing[0].to.as_str();
    if downstream == target_id
        || source_ids.contains(downstream)
        || graph.get_node(downstream).is_none_or(|node| {
            node.shape != NodeShape::Rectangle || node.id == target_id || node.id.is_empty()
        })
        || outgoing[0].is_back_edge
        || outgoing[0].kind != EdgeKind::Arrow
        || outgoing[0].label.is_some()
    {
        return None;
    }

    Some(incoming.len())
}

/// Return the target's bounded port count only for the strict simple scene.
pub(crate) fn target_port_count(graph: &Graph, target_id: &str) -> Option<usize> {
    if graph.subgraphs.len() != 1 || graph.edges.len() < MIN_TARGET_PORTS {
        return None;
    }

    let target = graph.get_node(target_id)?;
    if target.shape != NodeShape::Rectangle {
        return None;
    }

    let subgraph_id = graph.subgraphs.first()?.id.as_str();
    if graph.get_node_subgraph(target_id) == Some(subgraph_id) {
        return None;
    }

    let incoming: Vec<_> = graph
        .edges
        .iter()
        .filter(|edge| edge.to == target_id)
        .collect();
    if !(MIN_TARGET_PORTS..=MAX_TARGET_PORTS).contains(&incoming.len())
        || incoming.len() != graph.edges.len()
    {
        return None;
    }

    let source_ids: HashSet<&str> = incoming.iter().map(|edge| edge.from.as_str()).collect();
    if source_ids.len() != incoming.len() || source_ids.contains(target_id) {
        return None;
    }
    if incoming.iter().any(|edge| {
        edge.is_back_edge
            || edge.label.is_some()
            || edge.kind != EdgeKind::Arrow
            || graph.get_node_subgraph(&edge.from) != Some(subgraph_id)
            || graph
                .get_node(&edge.from)
                .is_none_or(|source| source.shape != NodeShape::Rectangle)
    }) {
        return None;
    }

    Some(incoming.len())
}

pub(crate) fn target_port_counts(graph: &Graph) -> Vec<(String, usize)> {
    graph
        .nodes
        .iter()
        .filter_map(|node| target_port_count(graph, &node.id).map(|count| (node.id.clone(), count)))
        .collect()
}

pub(crate) fn minimum_target_span(count: usize) -> usize {
    minimum_port_span(count)
}

pub(crate) fn required_primary_gap(count: usize) -> usize {
    super::fan_in_identity::required_primary_gap(count)
}

/// Return physical target-entry points in secondary-axis source order.
pub(crate) fn target_entry_points(
    target: &Node,
    direction: Direction,
    count: usize,
) -> Vec<(usize, usize)> {
    match direction {
        Direction::TD | Direction::TB => target_port_columns(target.x, target.width, count)
            .into_iter()
            .map(|x| (x, target.y.saturating_sub(1)))
            .collect(),
        Direction::BT => target_port_columns(target.x, target.width, count)
            .into_iter()
            .map(|x| (x, target.bottom_y()))
            .collect(),
        Direction::LR => target_port_rows(target.y, target.height, count)
            .into_iter()
            .map(|y| (target.x.saturating_sub(1), y))
            .collect(),
        Direction::RL => target_port_rows(target.y, target.height, count)
            .into_iter()
            .map(|y| (target.x.saturating_add(target.width), y))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::{internal_nonterminal_target_port_count, target_entry_points, target_port_count};
    use crate::graph::{Direction, Edge, Graph, Node, Subgraph};

    fn graph(direction: Direction) -> Graph {
        let mut graph = Graph::new();
        graph.direction = direction;
        let mut subgraph = crate::graph::Subgraph::new("sg", Some("Sources".to_owned()));
        for id in ["A", "B", "C"] {
            graph.add_node(Node::new(id, id));
            subgraph.add_node(id);
        }
        graph.add_node(Node::new("T", "Target"));
        for id in ["A", "B", "C"] {
            graph.add_edge(Edge::new(id, "T"));
        }
        graph.add_subgraph(subgraph);
        for id in ["A", "B", "C"] {
            graph.associate_node_with_subgraph(id, "sg");
        }
        graph
    }

    fn internal_nonterminal(direction: Direction) -> Graph {
        let mut graph = Graph::new();
        graph.direction = direction;
        for id in ["A", "B", "T"] {
            graph.add_node(Node::new(id, id));
        }
        graph.add_node(Node::new("Out", "Output"));
        graph.add_node(Node::new("In", "Input"));
        let mut subgraph = Subgraph::new("sg", Some("Process".to_owned()));
        for id in ["A", "B", "T"] {
            subgraph.add_node(id);
        }
        graph.add_subgraph(subgraph);
        for id in ["A", "B", "T"] {
            graph.associate_node_with_subgraph(id, "sg");
        }
        graph.add_edge(Edge::new("In", "A"));
        graph.add_edge(Edge::new("A", "T"));
        graph.add_edge(Edge::new("B", "T"));
        graph.add_edge(Edge::new("T", "Out"));
        graph
    }

    #[test]
    fn selects_only_the_strict_three_port_scene() {
        let graph = graph(Direction::TD);
        assert_eq!(target_port_count(&graph, "T"), Some(3));
        let target = graph.get_node("T").expect("target");
        assert_eq!(target_entry_points(target, Direction::TD, 3).len(), 3);
    }

    #[test]
    fn selects_only_the_exact_internal_nonterminal_scene() {
        for direction in [
            Direction::TD,
            Direction::TB,
            Direction::BT,
            Direction::LR,
            Direction::RL,
        ] {
            assert_eq!(
                internal_nonterminal_target_port_count(&internal_nonterminal(direction), "T"),
                Some(2),
                "internal nonterminal selector should cover {direction:?}"
            );
        }

        let mut labeled = internal_nonterminal(Direction::LR);
        labeled.edges[2].label = Some("labeled".to_owned());
        assert_eq!(internal_nonterminal_target_port_count(&labeled, "T"), None);

        let mut extra_downstream = internal_nonterminal(Direction::TD);
        extra_downstream.add_node(Node::new("Other", "Other"));
        extra_downstream.add_edge(Edge::new("T", "Other"));
        assert_eq!(
            internal_nonterminal_target_port_count(&extra_downstream, "T"),
            None
        );
    }
}
