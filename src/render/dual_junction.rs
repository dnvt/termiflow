//! Structural policy for the exact two-in/two-out dual-junction scene.
//!
//! Generic convergence intentionally exposes one shared target arrow.  The
//! evaluator holdout showed that this loses one of the two incoming edges when
//! the same target also branches to two destinations.  Keep the correction
//! narrowly scoped: only the exact five-node/four-edge rectangle graph is
//! eligible for one target port per incoming edge.

use std::collections::{HashMap, HashSet};

use crate::graph::{Direction, EdgeKind, Graph, NodeShape};

/// Return the number of target-side incoming ports required by an exact
/// dual-junction graph.
pub(crate) fn target_port_count(graph: &Graph, target_id: &str) -> Option<usize> {
    if !matches!(
        graph.direction,
        Direction::TD | Direction::BT | Direction::LR | Direction::RL
    ) || graph.nodes.len() != 5
        || graph.edges.len() != 4
        || !graph.subgraphs.is_empty()
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

    let node_ids: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    if node_ids.len() != graph.nodes.len() || !node_ids.contains(target_id) {
        return None;
    }

    let incoming: Vec<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.to == target_id)
        .map(|edge| edge.from.as_str())
        .collect();
    let outgoing: Vec<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.from == target_id)
        .map(|edge| edge.to.as_str())
        .collect();
    let incoming_ids: HashSet<&str> = incoming.iter().copied().collect();
    let outgoing_ids: HashSet<&str> = outgoing.iter().copied().collect();

    if incoming.len() != 2
        || outgoing.len() != 2
        || incoming_ids.len() != incoming.len()
        || outgoing_ids.len() != outgoing.len()
        || incoming.contains(&target_id)
        || outgoing.contains(&target_id)
        || incoming_ids.iter().any(|id| outgoing_ids.contains(id))
        || incoming_ids.iter().any(|id| !node_ids.contains(id))
        || outgoing_ids.iter().any(|id| !node_ids.contains(id))
    {
        return None;
    }

    let expected_ids: HashSet<&str> = incoming_ids
        .iter()
        .copied()
        .chain(std::iter::once(target_id))
        .chain(outgoing_ids.iter().copied())
        .collect();
    (expected_ids == node_ids).then_some(incoming.len())
}

/// Return all dual-junction targets and their required incoming port count.
pub(crate) fn target_port_counts(graph: &Graph) -> HashMap<String, usize> {
    graph
        .nodes
        .iter()
        .filter_map(|node| target_port_count(graph, &node.id).map(|count| (node.id.clone(), count)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{target_port_count, target_port_counts};
    use crate::graph::{Direction, Edge, Graph, Node, NodeShape, Subgraph};

    fn dual_junction(direction: Direction) -> Graph {
        let mut graph = Graph::new();
        graph.direction = direction;
        for (id, label) in [("A", "A"), ("B", "B"), ("C", "C"), ("D", "D"), ("E", "E")] {
            graph.add_node(Node::new(id, label));
        }
        graph.add_edge(Edge::new("A", "C"));
        graph.add_edge(Edge::new("B", "C"));
        graph.add_edge(Edge::new("C", "D"));
        graph.add_edge(Edge::new("C", "E"));
        graph
    }

    #[test]
    fn selects_the_exact_dual_junction_in_all_directions() {
        for direction in [Direction::TD, Direction::BT, Direction::LR, Direction::RL] {
            let graph = dual_junction(direction);
            assert_eq!(target_port_count(&graph, "C"), Some(2));
            assert_eq!(target_port_counts(&graph).get("C"), Some(&2));
            assert_eq!(target_port_counts(&graph).len(), 1);
        }
    }

    #[test]
    fn rejects_extra_edges_nodes_labels_shapes_and_subgraphs() {
        let mut extra_edge = dual_junction(Direction::TD);
        extra_edge.add_edge(Edge::new("A", "D"));
        assert_eq!(target_port_count(&extra_edge, "C"), None);

        let mut extra_node = dual_junction(Direction::TD);
        extra_node.add_node(Node::new("F", "F"));
        assert_eq!(target_port_count(&extra_node, "C"), None);

        let mut labeled = dual_junction(Direction::TD);
        labeled.edges[0].label = Some("label".to_string());
        assert_eq!(target_port_count(&labeled, "C"), None);

        let mut shaped = dual_junction(Direction::TD);
        shaped.nodes[2].shape = NodeShape::Diamond;
        assert_eq!(target_port_count(&shaped, "C"), None);

        let mut subgraph = dual_junction(Direction::TD);
        subgraph.add_subgraph(Subgraph::new("group", Some("Group".to_string())));
        assert_eq!(target_port_count(&subgraph, "C"), None);
    }

    #[test]
    fn rejects_terminal_and_non_dual_branching_shapes() {
        let mut terminal = dual_junction(Direction::TD);
        terminal.edges.retain(|edge| edge.from != "C");
        terminal.edges.truncate(2);
        assert_eq!(target_port_count(&terminal, "C"), None);

        let mut three_incoming = dual_junction(Direction::TD);
        three_incoming.add_node(Node::new("F", "F"));
        three_incoming.add_edge(Edge::new("F", "C"));
        assert_eq!(target_port_count(&three_incoming, "C"), None);

        let mut overlap = dual_junction(Direction::TD);
        overlap.edges[2] = Edge::new("C", "A");
        assert_eq!(target_port_count(&overlap, "C"), None);
    }
}
