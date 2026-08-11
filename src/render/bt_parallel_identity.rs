//! Shared identity policy for the bounded BT parallel subgraph scene.
//!
//! The scene is deliberately narrower than generic fan-in: it has one flat,
//! titled rectangle subgraph, two internal branch paths, and one external edge
//! entering and leaving the scene.  Keeping the selector here lets measurement
//! reserve the same target capacity that the scene lowerer consumes.

use std::collections::HashSet;

use crate::graph::{Direction, EdgeKind, Graph, Node, NodeShape};

use super::fan_in_identity::target_port_columns;

pub(crate) const TARGET_PORT_COUNT: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BtParallelIdentityScene {
    pub(crate) subgraph_id: String,
    pub(crate) fanout_source_id: String,
    pub(crate) fanin_target_id: String,
    pub(crate) branch_ids: Vec<String>,
    pub(crate) incoming_index: usize,
    pub(crate) outgoing_index: usize,
    pub(crate) edge_indices: Vec<usize>,
    pub(crate) internal_edge_indices: Vec<usize>,
}

/// Select the exact bounded BT scene that owns the two target entries.
pub(crate) fn scene_for(graph: &Graph) -> Option<BtParallelIdentityScene> {
    if graph.direction != Direction::BT
        || graph.subgraphs.len() != 1
        || graph.nodes.len() != 6
        || graph.edges.len() != 6
        || graph.has_cycles()
    {
        return None;
    }

    let subgraph = graph.subgraphs.first()?;
    if subgraph.parent_id.is_some()
        || !subgraph.child_ids.is_empty()
        || subgraph.title.as_deref().is_none_or(str::is_empty)
    {
        return None;
    }

    let direct_node_ids: HashSet<&str> = subgraph
        .node_ids
        .iter()
        .filter(|id| {
            graph
                .get_node_subgraph(id)
                .is_some_and(|owner| owner == subgraph.id)
        })
        .map(String::as_str)
        .collect();
    if direct_node_ids.len() != 4
        || direct_node_ids.len() != subgraph.node_ids.len()
        || graph
            .nodes
            .iter()
            .any(|node| node.shape != NodeShape::Rectangle)
        || graph
            .edges
            .iter()
            .any(|edge| edge.is_back_edge || edge.kind != EdgeKind::Arrow || edge.label.is_some())
    {
        return None;
    }

    let internal_edges: Vec<(usize, &crate::graph::Edge)> = graph
        .edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| {
            direct_node_ids.contains(edge.from.as_str())
                && direct_node_ids.contains(edge.to.as_str())
        })
        .collect();
    if internal_edges.len() != 4 {
        return None;
    }

    let fanouts: Vec<&str> = direct_node_ids
        .iter()
        .copied()
        .filter(|source_id| {
            internal_edges
                .iter()
                .filter(|(_, edge)| edge.from == *source_id)
                .count()
                == TARGET_PORT_COUNT
        })
        .collect();
    let fanins: Vec<&str> = direct_node_ids
        .iter()
        .copied()
        .filter(|target_id| {
            internal_edges
                .iter()
                .filter(|(_, edge)| edge.to == *target_id)
                .count()
                == TARGET_PORT_COUNT
        })
        .collect();
    if fanouts.len() != 1 || fanins.len() != 1 {
        return None;
    }
    let fanout_source_id = fanouts[0];
    let fanin_target_id = fanins[0];
    if fanout_source_id == fanin_target_id {
        return None;
    }

    let mut branch_ids: Vec<String> = internal_edges
        .iter()
        .filter(|(_, edge)| edge.from == fanout_source_id && edge.to != fanin_target_id)
        .map(|(_, edge)| edge.to.clone())
        .collect();
    branch_ids.sort_unstable();
    branch_ids.dedup();
    if branch_ids.len() != TARGET_PORT_COUNT
        || branch_ids.iter().any(|branch_id| {
            !direct_node_ids.contains(branch_id.as_str())
                || !internal_edges
                    .iter()
                    .any(|(_, edge)| edge.from == *branch_id && edge.to == fanin_target_id)
        })
    {
        return None;
    }

    let incoming: Vec<(usize, &crate::graph::Edge)> = graph
        .edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| {
            edge.to == fanout_source_id && !direct_node_ids.contains(edge.from.as_str())
        })
        .collect();
    let outgoing: Vec<(usize, &crate::graph::Edge)> = graph
        .edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| {
            edge.from == fanin_target_id && !direct_node_ids.contains(edge.to.as_str())
        })
        .collect();
    if incoming.len() != 1 || outgoing.len() != 1 || incoming[0].0 == outgoing[0].0 {
        return None;
    }

    let internal_edge_indices: Vec<usize> =
        internal_edges.iter().map(|(index, _)| *index).collect();
    let mut edge_indices = internal_edge_indices.clone();
    edge_indices.push(incoming[0].0);
    edge_indices.push(outgoing[0].0);
    edge_indices.sort_unstable();
    edge_indices.dedup();
    if edge_indices.len() != graph.edges.len() {
        return None;
    }

    Some(BtParallelIdentityScene {
        subgraph_id: subgraph.id.clone(),
        fanout_source_id: fanout_source_id.to_owned(),
        fanin_target_id: fanin_target_id.to_owned(),
        branch_ids,
        incoming_index: incoming[0].0,
        outgoing_index: outgoing[0].0,
        edge_indices,
        internal_edge_indices,
    })
}

/// Return the target capacity required by the selected scene.
pub(crate) fn target_port_counts(graph: &Graph) -> Vec<(String, usize)> {
    scene_for(graph)
        .map(|scene| (scene.fanin_target_id, TARGET_PORT_COUNT))
        .into_iter()
        .collect()
}

/// Return the two physical BT target entries in stable left-to-right order.
pub(crate) fn target_entry_points(target: &Node) -> Vec<(usize, usize)> {
    target_port_columns(target.x, target.width, TARGET_PORT_COUNT)
        .into_iter()
        .map(|x| (x, target.bottom_y()))
        .collect()
}

/// Build the subgraph-free graph view used by the ordinary transactional
/// identity lowerer for the internal two-edge fan-in.  Keeping the real node
/// rectangles preserves collision checks while removing the subgraph selector
/// restriction from that lowerer.
pub(crate) fn identity_graph(graph: &Graph, scene: &BtParallelIdentityScene) -> Graph {
    let mut identity = graph.clone();
    identity.edges = scene
        .internal_edge_indices
        .iter()
        .filter_map(|index| graph.edges.get(*index).cloned())
        .collect();
    identity.subgraphs.clear();
    identity.node_subgraph.clear();
    identity.edge_routes.clear();
    identity
}

#[cfg(test)]
mod tests {
    use super::{scene_for, target_entry_points, target_port_counts};
    use crate::graph::{Direction, Edge, Graph, Node, Subgraph};

    fn scene() -> Graph {
        let mut graph = Graph::new();
        graph.direction = Direction::BT;
        for id in ["In", "A", "B", "C", "D", "Out"] {
            graph.add_node(Node::new(id, id));
        }
        let subgraph = Subgraph::new("Process", Some("Process".to_owned()));
        graph.add_subgraph(subgraph);
        for id in ["A", "B", "C", "D"] {
            graph.associate_node_with_subgraph(id, "Process");
        }
        graph.edges = vec![
            Edge::new("A", "B"),
            Edge::new("A", "C"),
            Edge::new("B", "D"),
            Edge::new("C", "D"),
            Edge::new("In", "A"),
            Edge::new("D", "Out"),
        ];
        graph
    }

    #[test]
    fn selects_exact_scene_and_two_target_ports() {
        let graph = scene();
        let selected = scene_for(&graph).expect("bounded BT scene");
        assert_eq!(selected.branch_ids, vec!["B", "C"]);
        assert_eq!(target_port_counts(&graph), vec![("D".to_owned(), 2)]);
        let target = graph.get_node("D").expect("target");
        assert_eq!(target_entry_points(target).len(), 2);
    }

    #[test]
    fn rejects_extra_edge_outside_the_scene_contract() {
        let mut graph = scene();
        graph.edges.push(Edge::new("In", "Out"));
        assert!(scene_for(&graph).is_none());
    }
}
