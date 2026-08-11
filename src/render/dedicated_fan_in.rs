//! Topology policy for fan-in scenes that need one visible target port per edge.
//!
//! Ordinary convergence intentionally uses one shared merge arrow.  A small
//! class of diagrams becomes visually lossy under that policy: a layered
//! pipeline can have several overlapping fan-ins, and a dual-junction can
//! have a direct edge and an indirect edge arriving at the same target.  This
//! module keeps the decision structural and shared by measurement and render;
//! fixture names, labels, and coordinates are never consulted.

use std::collections::{HashMap, HashSet};

use crate::graph::{Direction, EdgeKind, Graph, NodeShape};

use super::dual_junction::target_port_count as dual_junction_target_port_count;
use super::subgraph_fan_in_identity::internal_nonterminal_target_port_count;

/// Return the number of incoming ports required by a target in a dedicated
/// horizontal fan-in scene.
///
/// The policy deliberately has four narrow families:
///
/// * a subgraph-free, all-rectangle graph with at least four fan-in targets;
///   this is the layered/dense pipeline family;
/// * a three-node database dual-junction, where one source branches to a
///   cache and directly to the database and the cache also reaches the
///   database;
/// * an exact four-node horizontal branch/rejoin, where one source splits to
///   two branch nodes and both branches enter one terminal sink.
/// * an exact five-node horizontal mixed junction, where one source splits to
///   three branch nodes and all three branches enter one terminal sink.
pub(crate) fn target_port_count(graph: &Graph, target_id: &str) -> Option<usize> {
    if !matches!(graph.direction, Direction::LR | Direction::RL) {
        return None;
    }

    if let Some(count) = internal_nonterminal_target_port_count(graph, target_id) {
        return Some(count);
    }

    if !simple_arrow_graph(graph) || !graph.subgraphs.is_empty() {
        return None;
    }

    let fan_in_counts = incoming_counts(graph);
    let target_count = fan_in_counts.get(target_id).copied().unwrap_or(0);
    if target_count < 2 {
        return None;
    }

    if branch_rejoin(graph, target_id, target_count) {
        return Some(target_count);
    }

    if mixed_branch_rejoin(graph, target_id, target_count) {
        return Some(target_count);
    }

    let fan_in_target_count = fan_in_counts.values().filter(|count| **count >= 2).count();
    if graph
        .nodes
        .iter()
        .all(|node| node.shape == NodeShape::Rectangle)
        && fan_in_target_count >= 4
    {
        return Some(target_count);
    }

    if database_dual_junction(graph, target_id, target_count) {
        return Some(target_count);
    }

    dual_junction_target_port_count(graph, target_id)
}

fn branch_rejoin(graph: &Graph, target_id: &str, target_count: usize) -> bool {
    if !matches!(graph.direction, Direction::LR | Direction::RL)
        || graph.nodes.len() != 4
        || graph.edges.len() != 4
        || target_count != 2
        || !graph.subgraphs.is_empty()
        || graph.has_cycles()
        || graph
            .nodes
            .iter()
            .any(|node| node.shape != NodeShape::Rectangle)
    {
        return false;
    }

    let node_ids: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    if node_ids.len() != graph.nodes.len() || !node_ids.contains(target_id) {
        return false;
    }

    let incoming_sources: Vec<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.to == target_id)
        .map(|edge| edge.from.as_str())
        .collect();
    if incoming_sources.len() != 2
        || incoming_sources
            .iter()
            .copied()
            .collect::<HashSet<_>>()
            .len()
            != 2
        || graph.edges.iter().any(|edge| edge.from == target_id)
    {
        return false;
    }

    let source_ids: Vec<&str> = graph
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .filter(|node_id| {
            graph
                .edges
                .iter()
                .filter(|edge| edge.from == *node_id)
                .count()
                == 2
                && graph.edges.iter().all(|edge| edge.to != *node_id)
        })
        .collect();
    if source_ids.len() != 1 {
        return false;
    }
    let source_id = source_ids[0];

    let branch_ids: HashSet<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.from == source_id)
        .map(|edge| edge.to.as_str())
        .collect();
    let incoming_ids: HashSet<&str> = incoming_sources.iter().copied().collect();
    if branch_ids != incoming_ids
        || branch_ids.contains(target_id)
        || branch_ids.contains(source_id)
        || branch_ids.len() != 2
    {
        return false;
    }

    if branch_ids.iter().any(|branch_id| {
        graph
            .edges
            .iter()
            .filter(|edge| edge.to == *branch_id)
            .count()
            != 1
            || graph
                .edges
                .iter()
                .filter(|edge| edge.from == *branch_id && edge.to == target_id)
                .count()
                != 1
    }) {
        return false;
    }

    let expected_ids: HashSet<&str> = branch_ids
        .iter()
        .copied()
        .chain(std::iter::once(source_id))
        .chain(std::iter::once(target_id))
        .collect();
    expected_ids == node_ids
}

fn mixed_branch_rejoin(graph: &Graph, target_id: &str, target_count: usize) -> bool {
    if !matches!(graph.direction, Direction::LR | Direction::RL)
        || graph.nodes.len() != 5
        || graph.edges.len() != 6
        || target_count != 3
        || !graph.subgraphs.is_empty()
        || graph.has_cycles()
        || graph
            .nodes
            .iter()
            .any(|node| node.shape != NodeShape::Rectangle)
    {
        return false;
    }

    let node_ids: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    if node_ids.len() != graph.nodes.len() || !node_ids.contains(target_id) {
        return false;
    }

    let incoming_sources: Vec<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.to == target_id)
        .map(|edge| edge.from.as_str())
        .collect();
    if incoming_sources.len() != 3
        || incoming_sources
            .iter()
            .copied()
            .collect::<HashSet<_>>()
            .len()
            != 3
        || graph.edges.iter().any(|edge| edge.from == target_id)
    {
        return false;
    }

    let source_ids: Vec<&str> = graph
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .filter(|node_id| {
            graph
                .edges
                .iter()
                .filter(|edge| edge.from == *node_id)
                .count()
                == 3
                && graph.edges.iter().all(|edge| edge.to != *node_id)
        })
        .collect();
    if source_ids.len() != 1 {
        return false;
    }
    let source_id = source_ids[0];

    let branch_ids: HashSet<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.from == source_id)
        .map(|edge| edge.to.as_str())
        .collect();
    let incoming_ids: HashSet<&str> = incoming_sources.iter().copied().collect();
    if branch_ids != incoming_ids
        || branch_ids.contains(target_id)
        || branch_ids.contains(source_id)
        || branch_ids.len() != 3
    {
        return false;
    }

    if branch_ids.iter().any(|branch_id| {
        graph
            .edges
            .iter()
            .filter(|edge| edge.to == *branch_id)
            .count()
            != 1
            || graph
                .edges
                .iter()
                .filter(|edge| edge.from == *branch_id && edge.to == target_id)
                .count()
                != 1
    }) {
        return false;
    }

    let expected_ids: HashSet<&str> = branch_ids
        .iter()
        .copied()
        .chain(std::iter::once(source_id))
        .chain(std::iter::once(target_id))
        .collect();
    expected_ids == node_ids
}

/// Return all target IDs and their required port count for one render.
pub(crate) fn target_port_counts(graph: &Graph) -> HashMap<String, usize> {
    graph
        .nodes
        .iter()
        .filter_map(|node| target_port_count(graph, &node.id).map(|count| (node.id.clone(), count)))
        .collect()
}

/// Minimum box height that exposes `count` disjoint interior side rows.
///
/// Keeping one blank row between neighboring ports makes ASCII elbows and
/// Unicode junctions remain distinguishable after the node is drawn.
pub(crate) fn minimum_port_height(count: usize) -> usize {
    count.saturating_mul(2).saturating_add(1).max(3)
}

/// Return the side rows used by a dedicated fan-in target.
pub(crate) fn target_port_rows(y: usize, height: usize, count: usize) -> Vec<usize> {
    if count == 0 {
        return Vec::new();
    }
    let height = height.max(minimum_port_height(count));
    (0..count)
        .map(|index| y.saturating_add(1 + index.saturating_mul(2)))
        .filter(|row| *row < y.saturating_add(height).saturating_sub(1))
        .collect()
}

fn simple_arrow_graph(graph: &Graph) -> bool {
    !graph.nodes.is_empty()
        && graph
            .edges
            .iter()
            .all(|edge| !edge.is_back_edge && edge.kind == EdgeKind::Arrow && edge.label.is_none())
}

fn incoming_counts(graph: &Graph) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for edge in &graph.edges {
        if !edge.is_back_edge {
            *counts.entry(edge.to.clone()).or_insert(0) += 1;
        }
    }
    counts
}

fn outgoing_count(graph: &Graph, source_id: &str) -> usize {
    graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge && edge.from == source_id)
        .count()
}

fn database_dual_junction(graph: &Graph, target_id: &str, target_count: usize) -> bool {
    if graph.nodes.len() != 3 || graph.edges.len() != 3 || target_count != 2 {
        return false;
    }
    let Some(target) = graph.get_node(target_id) else {
        return false;
    };
    if target.shape != NodeShape::Database {
        return false;
    }

    let incoming_sources: Vec<&str> = graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge && edge.to == target_id)
        .map(|edge| edge.from.as_str())
        .collect();
    if incoming_sources.len() != 2 {
        return false;
    }

    let branching_source = incoming_sources
        .iter()
        .copied()
        .find(|source_id| outgoing_count(graph, source_id) >= 2);
    let Some(branching_source) = branching_source else {
        return false;
    };
    let Some(other_source) = incoming_sources
        .iter()
        .copied()
        .find(|source_id| *source_id != branching_source)
    else {
        return false;
    };

    graph
        .edges
        .iter()
        .any(|edge| !edge.is_back_edge && edge.from == branching_source && edge.to == other_source)
}

#[cfg(test)]
mod tests {
    use super::{minimum_port_height, target_port_count, target_port_rows};
    use crate::graph::{Direction, Edge, Graph, Node, NodeShape, Subgraph};

    fn branch_rejoin(direction: Direction) -> Graph {
        let mut graph = Graph::new();
        graph.direction = direction;
        for (id, label) in [
            ("Source", "source"),
            ("BranchA", "branch a"),
            ("BranchB", "branch b"),
            ("Target", "target"),
        ] {
            graph.add_node(Node::new(id, label));
        }
        for (from, to) in [
            ("Source", "BranchA"),
            ("Source", "BranchB"),
            ("BranchA", "Target"),
            ("BranchB", "Target"),
        ] {
            graph.add_edge(Edge::new(from, to));
        }
        graph
    }

    fn mixed_branch_rejoin(direction: Direction) -> Graph {
        let mut graph = Graph::new();
        graph.direction = direction;
        for (id, label) in [
            ("Source", "renamed source"),
            ("BranchA", "renamed branch a"),
            ("BranchB", "renamed branch b"),
            ("BranchC", "renamed branch c"),
            ("Target", "renamed target"),
        ] {
            graph.add_node(Node::new(id, label));
        }
        for (from, to) in [
            ("Source", "BranchA"),
            ("Source", "BranchB"),
            ("Source", "BranchC"),
            ("BranchA", "Target"),
            ("BranchB", "Target"),
            ("BranchC", "Target"),
        ] {
            graph.add_edge(Edge::new(from, to));
        }
        graph
    }

    fn internal_nonterminal(direction: Direction) -> Graph {
        let mut graph = Graph::new();
        graph.direction = direction;
        for id in ["A", "B", "T"] {
            graph.add_node(Node::new(id, id));
        }
        graph.add_node(Node::new("In", "Input"));
        graph.add_node(Node::new("Out", "Output"));
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
    fn layered_fan_in_family_requires_one_port_per_target_edge() {
        let mut graph = Graph::new();
        graph.direction = Direction::LR;
        for id in ["A", "B", "C", "D", "M1", "M2", "M3", "M4"] {
            graph.add_node(Node::new(id, id));
        }
        for (from, to) in [
            ("A", "M1"),
            ("B", "M1"),
            ("A", "M2"),
            ("C", "M2"),
            ("B", "M3"),
            ("D", "M3"),
            ("C", "M4"),
            ("D", "M4"),
        ] {
            graph.add_edge(Edge::new(from, to));
        }
        assert_eq!(target_port_count(&graph, "M1"), Some(2));
        assert_eq!(target_port_count(&graph, "M4"), Some(2));
    }

    #[test]
    fn database_dual_junction_is_structural_not_label_based() {
        let mut graph = Graph::new();
        graph.direction = Direction::RL;
        graph.add_node(Node::new("API", "renamed source"));
        graph.add_node(Node::new("Cache", "renamed cache"));
        graph.add_node(Node::with_shape("DB", "renamed store", NodeShape::Database));
        for (from, to) in [("API", "DB"), ("API", "Cache"), ("Cache", "DB")] {
            graph.add_edge(Edge::new(from, to));
        }
        assert_eq!(target_port_count(&graph, "DB"), Some(2));
    }

    #[test]
    fn exact_horizontal_branch_rejoin_gets_one_port_per_sink_edge() {
        for direction in [Direction::LR, Direction::RL] {
            assert_eq!(
                target_port_count(&branch_rejoin(direction), "Target"),
                Some(2)
            );
        }
    }

    #[test]
    fn exact_horizontal_mixed_junction_gets_one_port_per_sink_edge() {
        for direction in [Direction::LR, Direction::RL] {
            assert_eq!(
                target_port_count(&mixed_branch_rejoin(direction), "Target"),
                Some(3)
            );
        }
    }

    #[test]
    fn branch_rejoin_rejects_near_misses_and_keeps_generic_fan_in() {
        let mut extra_node = branch_rejoin(Direction::RL);
        extra_node.add_node(Node::new("Extra", "extra"));
        assert_eq!(target_port_count(&extra_node, "Target"), None);

        let mut extra_edge = branch_rejoin(Direction::RL);
        extra_edge.add_edge(Edge::new("Source", "Target"));
        assert_eq!(target_port_count(&extra_edge, "Target"), None);

        let mut shaped_target = branch_rejoin(Direction::RL);
        shaped_target.nodes[3].shape = NodeShape::Database;
        assert_eq!(target_port_count(&shaped_target, "Target"), None);

        let mut labeled = branch_rejoin(Direction::RL);
        labeled.edges[0].label = Some("labeled".to_owned());
        assert_eq!(target_port_count(&labeled, "Target"), None);

        let mut nested = branch_rejoin(Direction::RL);
        nested.add_subgraph(Subgraph::new("group", Some("Group".to_owned())));
        assert_eq!(target_port_count(&nested, "Target"), None);

        let mut cyclic = branch_rejoin(Direction::RL);
        cyclic.edges[0].is_back_edge = true;
        assert_eq!(target_port_count(&cyclic, "Target"), None);

        let mut ordinary = Graph::new();
        ordinary.direction = Direction::RL;
        for id in ["A", "B", "Target"] {
            ordinary.add_node(Node::new(id, id));
        }
        ordinary.add_edge(Edge::new("A", "Target"));
        ordinary.add_edge(Edge::new("B", "Target"));
        assert_eq!(target_port_count(&ordinary, "Target"), None);
    }

    #[test]
    fn mixed_branch_rejoin_rejects_near_misses_and_vertical_policy() {
        let mut extra_node = mixed_branch_rejoin(Direction::LR);
        extra_node.add_node(Node::new("Extra", "extra"));
        assert_eq!(target_port_count(&extra_node, "Target"), None);

        let mut direct_edge = mixed_branch_rejoin(Direction::LR);
        direct_edge.add_edge(Edge::new("Source", "Target"));
        assert_eq!(target_port_count(&direct_edge, "Target"), None);

        let mut labeled = mixed_branch_rejoin(Direction::LR);
        labeled.edges[0].label = Some("labeled".to_owned());
        assert_eq!(target_port_count(&labeled, "Target"), None);

        let mut nested = mixed_branch_rejoin(Direction::LR);
        nested.add_subgraph(Subgraph::new("group", Some("Group".to_owned())));
        assert_eq!(target_port_count(&nested, "Target"), None);

        let mut shaped = mixed_branch_rejoin(Direction::LR);
        shaped.nodes[3].shape = NodeShape::Database;
        assert_eq!(target_port_count(&shaped, "Target"), None);

        let mut cyclic = mixed_branch_rejoin(Direction::LR);
        cyclic.edges[0].is_back_edge = true;
        assert_eq!(target_port_count(&cyclic, "Target"), None);

        let mut continuation = mixed_branch_rejoin(Direction::LR);
        continuation.add_node(Node::new("Exit", "exit"));
        continuation.add_edge(Edge::new("Target", "Exit"));
        assert_eq!(target_port_count(&continuation, "Target"), None);

        let mut branch_to_branch = mixed_branch_rejoin(Direction::LR);
        branch_to_branch.edges[3] = Edge::new("BranchA", "BranchB");
        assert_eq!(target_port_count(&branch_to_branch, "Target"), None);

        assert_eq!(
            target_port_count(&mixed_branch_rejoin(Direction::TD), "Target"),
            None
        );

        let mut pure_fan_in = Graph::new();
        pure_fan_in.direction = Direction::LR;
        for id in ["A", "B", "C", "Target"] {
            pure_fan_in.add_node(Node::new(id, id));
        }
        for source in ["A", "B", "C"] {
            pure_fan_in.add_edge(Edge::new(source, "Target"));
        }
        assert_eq!(target_port_count(&pure_fan_in, "Target"), None);
    }

    #[test]
    fn exact_rectangle_dual_junction_shares_the_measurement_policy() {
        let mut graph = Graph::new();
        graph.direction = Direction::LR;
        for (id, label) in [
            ("A", "left"),
            ("B", "root"),
            ("C", "merge"),
            ("D", "exit"),
            ("E", "side"),
        ] {
            graph.add_node(Node::new(id, label));
        }
        for (from, to) in [("A", "C"), ("B", "C"), ("C", "D"), ("C", "E")] {
            graph.add_edge(Edge::new(from, to));
        }
        assert_eq!(target_port_count(&graph, "C"), Some(2));
    }

    #[test]
    fn unrelated_small_fan_in_keeps_shared_merge_policy() {
        let mut graph = Graph::new();
        graph.direction = Direction::LR;
        for id in ["A", "B", "C"] {
            graph.add_node(Node::new(id, id));
        }
        graph.add_edge(Edge::new("A", "C"));
        graph.add_edge(Edge::new("B", "C"));
        assert_eq!(target_port_count(&graph, "C"), None);
    }

    #[test]
    fn selects_internal_subgraph_nonterminal_fan_in_for_horizontal_routes() {
        assert_eq!(
            target_port_count(&internal_nonterminal(Direction::LR), "T"),
            Some(2)
        );
        assert_eq!(
            target_port_count(&internal_nonterminal(Direction::RL), "T"),
            Some(2)
        );

        let mut labeled = internal_nonterminal(Direction::LR);
        labeled.edges[1].label = Some("labeled".to_owned());
        assert_eq!(target_port_count(&labeled, "T"), None);
    }

    #[test]
    fn ports_have_blank_separators() {
        assert_eq!(minimum_port_height(2), 5);
        assert_eq!(minimum_port_height(3), 7);
        assert_eq!(target_port_rows(10, 5, 2), vec![11, 13]);
        assert_eq!(target_port_rows(10, 7, 3), vec![11, 13, 15]);
        assert_eq!(target_port_rows(10, 9, 4), vec![11, 13, 15, 17]);
    }
}
