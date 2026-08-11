//! Shared capability policy for the bounded sibling-subgraph fan-in scene.
//!
//! The scene is intentionally narrower than ordinary subgraph routing.  It
//! proves the exact two-edge ownership shape before measurement or rendering
//! requests any extra target capacity.  The renderer adds the geometry gate
//! after layout; this module remains coordinate-free so measurement and
//! lowering cannot disagree about whether the target is eligible.

use std::collections::HashSet;

use crate::graph::{EdgeKind, Graph, NodeShape};

pub(crate) const TARGET_PORT_COUNT: usize = 2;

pub(crate) const fn required_primary_gap(count: usize) -> usize {
    super::fan_in_identity::required_primary_gap(count)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Scene {
    pub source_subgraph_id: String,
    pub target_id: String,
    pub edge_indexes: [usize; TARGET_PORT_COUNT],
    pub source_ids: [String; TARGET_PORT_COUNT],
}

/// Return every topology-proven sibling-subgraph target.
pub(crate) fn scenes(graph: &Graph) -> Vec<Scene> {
    if graph.subgraphs.len() != 2 || graph.has_cycles() {
        return Vec::new();
    }
    if graph.subgraphs.iter().any(|subgraph| {
        subgraph.parent_id.is_some() || !subgraph.child_ids.is_empty() || subgraph.title.is_none()
    }) {
        return Vec::new();
    }

    graph
        .nodes
        .iter()
        .filter_map(|target| scene_for_target(graph, &target.id))
        .collect()
}

pub(crate) fn scene_for_target(graph: &Graph, target_id: &str) -> Option<Scene> {
    if graph.subgraphs.len() != 2 || graph.has_cycles() {
        return None;
    }
    if graph.subgraphs.iter().any(|subgraph| {
        subgraph.parent_id.is_some() || !subgraph.child_ids.is_empty() || subgraph.title.is_none()
    }) {
        return None;
    }

    let target = graph.get_node(target_id)?;
    if target.shape != NodeShape::Rectangle
        || graph.get_node_subgraph(target_id).is_some()
        || graph.edges.iter().any(|edge| edge.from == target_id)
    {
        return None;
    }

    let incoming: Vec<(usize, &crate::graph::Edge)> = graph
        .edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| edge.to == target_id)
        .collect();
    if incoming.len() != TARGET_PORT_COUNT {
        return None;
    }
    if incoming
        .iter()
        .any(|(_, edge)| edge.is_back_edge || edge.kind != EdgeKind::Arrow || edge.label.is_some())
    {
        return None;
    }

    let source_ids: Vec<&str> = incoming
        .iter()
        .map(|(_, edge)| edge.from.as_str())
        .collect();
    let unique_sources: HashSet<&str> = source_ids.iter().copied().collect();
    if unique_sources.len() != TARGET_PORT_COUNT {
        return None;
    }

    let source_subgraph_id = graph.get_node_subgraph(source_ids[0])?.to_owned();
    if source_ids
        .iter()
        .any(|source_id| graph.get_node_subgraph(source_id) != Some(source_subgraph_id.as_str()))
    {
        return None;
    }
    let source_subgraph = graph.get_subgraph(&source_subgraph_id)?;
    if source_subgraph.node_ids.len() != TARGET_PORT_COUNT
        || !source_subgraph
            .node_ids
            .iter()
            .all(|node_id| unique_sources.contains(node_id.as_str()))
    {
        return None;
    }

    for source_id in &source_ids {
        let node = graph.get_node(source_id)?;
        if !matches!(node.shape, NodeShape::Rectangle | NodeShape::Database) {
            return None;
        }
        let outgoing: Vec<_> = graph
            .edges
            .iter()
            .filter(|edge| edge.from == *source_id)
            .collect();
        if outgoing.len() != 1 || outgoing[0].to != target_id {
            return None;
        }
    }

    // Keep internal ownership unambiguous.  Context edges may enter this
    // subgraph from elsewhere, but the two selected source nodes cannot have
    // another internal route that competes for their boundary lanes.
    if graph.edges.iter().any(|edge| {
        source_subgraph.node_ids.contains(&edge.from) && source_subgraph.node_ids.contains(&edge.to)
    }) {
        return None;
    }

    let mut ordered = incoming
        .iter()
        .map(|(index, edge)| (*index, edge.from.clone()))
        .collect::<Vec<_>>();
    ordered.sort_by_key(|(_, source_id)| source_id.clone());
    Some(Scene {
        source_subgraph_id,
        target_id: target_id.to_owned(),
        edge_indexes: [ordered[0].0, ordered[1].0],
        source_ids: [ordered[0].1.clone(), ordered[1].1.clone()],
    })
}

pub(crate) fn target_port_counts(graph: &Graph) -> Vec<(String, usize)> {
    scenes(graph)
        .into_iter()
        .map(|scene| (scene.target_id, TARGET_PORT_COUNT))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{scene_for_target, target_port_counts, TARGET_PORT_COUNT};
    use crate::graph::{Direction, Edge, Graph, Node, NodeShape, Subgraph};

    fn graph() -> Graph {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;
        for (id, shape) in [("D1", NodeShape::Database), ("D2", NodeShape::Database)] {
            graph.add_node(Node::with_shape(id, id, shape));
        }
        graph.add_node(Node::new("S", "Service"));
        graph.add_node(Node::new("T", "Target"));
        let mut source = Subgraph::new("data", Some("Data".to_owned()));
        source.bounds = crate::graph::Rectangle::new(0, 0, 20, 10);
        source.add_node("D1");
        source.add_node("D2");
        let mut peer = Subgraph::new("service", Some("Service".to_owned()));
        peer.bounds = crate::graph::Rectangle::new(0, 12, 20, 10);
        graph.add_subgraph(source);
        graph.add_subgraph(peer);
        graph.associate_node_with_subgraph("D1", "data");
        graph.associate_node_with_subgraph("D2", "data");
        graph.add_edge(Edge::new("D1", "T"));
        graph.add_edge(Edge::new("D2", "T"));
        graph
    }

    #[test]
    fn selects_only_the_two_source_external_terminal_scene() {
        let graph = graph();
        let scene = scene_for_target(&graph, "T").expect("bounded sibling scene");
        assert_eq!(scene.source_subgraph_id, "data");
        assert_eq!(scene.edge_indexes.len(), TARGET_PORT_COUNT);
        assert_eq!(target_port_counts(&graph), vec![("T".to_owned(), 2)]);
    }

    #[test]
    fn rejects_context_source_edges_and_nonterminal_targets() {
        let mut context_graph = graph();
        context_graph.add_node(Node::new("Other", "Other"));
        context_graph.add_edge(Edge::new("D1", "Other"));
        assert!(scene_for_target(&context_graph, "T").is_none());

        let mut nonterminal_graph = graph();
        nonterminal_graph.add_node(Node::new("Other", "Other"));
        nonterminal_graph.add_edge(Edge::new("T", "Other"));
        assert!(scene_for_target(&nonterminal_graph, "T").is_none());
    }

    #[test]
    fn rejects_nested_or_internal_source_topologies() {
        let mut nested = graph();
        nested
            .get_subgraph_mut("data")
            .unwrap()
            .child_ids
            .push("service".to_owned());
        assert!(scene_for_target(&nested, "T").is_none());

        let mut internal = graph();
        internal.add_edge(Edge::new("D1", "D2"));
        assert!(scene_for_target(&internal, "T").is_none());
    }
}
