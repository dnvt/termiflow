//! Structural policy for the bounded vertical fan-in scene.
//!
//! Ordinary convergence intentionally uses one shared merge arrow.  A small
//! terminal fan-in is easier to read when each incoming edge remains visible
//! at its own target-side port.  This policy is intentionally narrower than
//! the generic convergence family so the experiment cannot change unrelated
//! diagrams while it is being evaluated.

use std::collections::{HashMap, HashSet};

use crate::graph::{Direction, EdgeKind, Graph, NodeShape};

use super::subgraph_fan_in_identity::internal_nonterminal_target_port_count;

/// Return the incoming-edge count for the bounded vertical fan-in scene.
///
/// The match is structural rather than fixture- or label-based: two or three
/// rectangle sources, one rectangle terminal target, no subgraphs, and no
/// other edges or nodes.  TD and BT are the only directions in this first
/// experiment; horizontal fan-in has its own established policy.
pub(crate) fn target_port_count(graph: &Graph, target_id: &str) -> Option<usize> {
    if !matches!(graph.direction, Direction::TD | Direction::BT)
        || graph.nodes.len() < 3
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

    let incoming: Vec<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.to == target_id)
        .map(|edge| edge.from.as_str())
        .collect();
    let count = incoming.len();
    if !(2..=3).contains(&count)
        || graph.edges.len() != count
        || graph.nodes.len() != count + 1
        || incoming.contains(&target_id)
    {
        return None;
    }

    let source_ids: HashSet<&str> = incoming.iter().copied().collect();
    if source_ids.len() != count {
        return None;
    }

    let target = graph.get_node(target_id)?;
    if graph
        .edges
        .iter()
        .any(|edge| edge.from == target_id || edge.to != target_id)
    {
        return None;
    }

    // The node count and unique source count together prove that this graph
    // contains exactly the selected target and its incoming sources.
    let node_ids: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    if node_ids.len() != graph.nodes.len()
        || !node_ids.contains(target_id)
        || source_ids.iter().any(|source| !node_ids.contains(source))
        || target.shape != NodeShape::Rectangle
    {
        return None;
    }

    Some(count)
}

/// Return the incoming-edge count for the bounded nonterminal vertical
/// fan-in scene.
///
/// This is deliberately a separate policy from terminal fan-in.  The target
/// has exactly two incoming sources and one downstream continuation; that
/// small graph shape lets the existing per-edge target-port lowerer preserve
/// incoming identity without making cascades, branch diamonds, or subgraph
/// routes part of the same experiment.
pub(crate) fn nonterminal_target_port_count(graph: &Graph, target_id: &str) -> Option<usize> {
    if !matches!(graph.direction, Direction::TD | Direction::BT) {
        return None;
    }

    if let Some(count) = internal_nonterminal_target_port_count(graph, target_id) {
        return Some(count);
    }

    if graph.nodes.len() != 4
        || graph.edges.len() != 3
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
    if incoming.len() != 2
        || incoming.contains(&target_id)
        || incoming.iter().copied().collect::<HashSet<_>>().len() != incoming.len()
    {
        return None;
    }

    let outgoing: Vec<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.from == target_id)
        .map(|edge| edge.to.as_str())
        .collect();
    if outgoing.len() != 1
        || outgoing[0] == target_id
        || incoming.iter().any(|source| *source == outgoing[0])
        || !node_ids.contains(outgoing[0])
    {
        return None;
    }

    let target = graph.get_node(target_id)?;
    if target.shape != NodeShape::Rectangle {
        return None;
    }

    // The exact four-node/three-edge cardinality plus the disjoint source and
    // downstream IDs proves that the graph has no extra branch or target.
    let expected_ids: HashSet<&str> = incoming
        .iter()
        .copied()
        .chain(std::iter::once(target_id))
        .chain(std::iter::once(outgoing[0]))
        .collect();
    if expected_ids.len() != graph.nodes.len() || expected_ids != node_ids {
        return None;
    }

    Some(incoming.len())
}

/// Return all target IDs and their required target-side port count.
pub(crate) fn target_port_counts(graph: &Graph) -> HashMap<String, usize> {
    graph
        .nodes
        .iter()
        .filter_map(|node| target_port_count(graph, &node.id).map(|count| (node.id.clone(), count)))
        .collect()
}

/// Return all nonterminal target IDs and their required target-side port
/// count.  Kept separate from [`target_port_counts`] so callers can preserve
/// the terminal/nonterminal policy boundary explicitly.
pub(crate) fn nonterminal_target_port_counts(graph: &Graph) -> HashMap<String, usize> {
    graph
        .nodes
        .iter()
        .filter_map(|node| {
            nonterminal_target_port_count(graph, &node.id).map(|count| (node.id.clone(), count))
        })
        .collect()
}

/// Minimum width that can expose `count` interior target ports with one blank
/// column between neighboring arrow attachments.
pub(crate) fn minimum_port_width(count: usize) -> usize {
    count.saturating_mul(2).saturating_add(1).max(5)
}

/// Return centered, separated target-side columns for a vertical fan-in.
pub(crate) fn target_port_columns(x: usize, width: usize, count: usize) -> Vec<usize> {
    if count == 0 || width < minimum_port_width(count) {
        return Vec::new();
    }

    let center = x + width / 2;
    let start = center.saturating_sub(count.saturating_sub(1));
    let columns: Vec<usize> = (0..count)
        .map(|index| start.saturating_add(index.saturating_mul(2)))
        .collect();
    if columns
        .iter()
        .any(|column| *column < x.saturating_add(1) || *column >= x + width.saturating_sub(1))
    {
        Vec::new()
    } else {
        columns
    }
}

#[cfg(test)]
mod tests {
    use super::{
        minimum_port_width, nonterminal_target_port_count, target_port_columns, target_port_count,
    };
    use crate::graph::{Direction, Edge, Graph, Node, NodeShape};

    fn terminal_fan_in(direction: Direction, count: usize) -> Graph {
        let mut graph = Graph::new();
        graph.direction = direction;
        for index in 0..count {
            let source = format!("S{index}");
            graph.add_node(Node::new(&source, &source));
            graph.add_edge(Edge::new(&source, "T"));
        }
        graph.add_node(Node::new("T", "Target"));
        graph
    }

    #[test]
    fn selects_only_bounded_terminal_vertical_fan_in() {
        assert_eq!(
            target_port_count(&terminal_fan_in(Direction::BT, 2), "T"),
            Some(2)
        );
        assert_eq!(
            target_port_count(&terminal_fan_in(Direction::TD, 3), "T"),
            Some(3)
        );

        let mut cascade = terminal_fan_in(Direction::BT, 2);
        cascade.add_node(Node::new("O", "Other"));
        cascade.add_edge(Edge::new("T", "O"));
        assert_eq!(target_port_count(&cascade, "T"), None);
    }

    #[test]
    fn rejects_non_terminal_and_non_simple_edges() {
        let mut graph = terminal_fan_in(Direction::BT, 2);
        graph.edges[0].label = Some("labeled".to_string());
        assert_eq!(target_port_count(&graph, "T"), None);
    }

    fn nonterminal_fan_in(direction: Direction) -> Graph {
        let mut graph = Graph::new();
        graph.direction = direction;
        for (id, label) in [("A", "A"), ("B", "B"), ("C", "C"), ("D", "D")] {
            graph.add_node(Node::new(id, label));
        }
        graph.add_edge(Edge::new("A", "B"));
        graph.add_edge(Edge::new("D", "B"));
        graph.add_edge(Edge::new("B", "C"));
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
        let mut subgraph = crate::graph::Subgraph::new("sg", Some("Process".to_owned()));
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
    fn selects_only_the_exact_nonterminal_two_in_one_out_shape() {
        assert_eq!(
            nonterminal_target_port_count(&nonterminal_fan_in(Direction::TD), "B"),
            Some(2)
        );
        assert_eq!(
            nonterminal_target_port_count(&nonterminal_fan_in(Direction::BT), "B"),
            Some(2)
        );

        let mut terminal = nonterminal_fan_in(Direction::TD);
        terminal.edges.pop();
        assert_eq!(
            nonterminal_target_port_count(&terminal, "B"),
            None,
            "terminal fan-in must stay on the terminal policy"
        );

        let mut multiple_outputs = nonterminal_fan_in(Direction::TD);
        multiple_outputs.add_node(Node::new("E", "E"));
        multiple_outputs.add_edge(Edge::new("B", "E"));
        assert_eq!(
            nonterminal_target_port_count(&multiple_outputs, "B"),
            None,
            "a second downstream edge must remain outside this slice"
        );
    }

    #[test]
    fn rejects_horizontal_labels_shapes_and_subgraphs() {
        let horizontal = nonterminal_fan_in(Direction::LR);
        assert_eq!(nonterminal_target_port_count(&horizontal, "B"), None);

        let mut labeled = nonterminal_fan_in(Direction::TD);
        labeled.edges[0].label = Some("labeled".to_string());
        assert_eq!(nonterminal_target_port_count(&labeled, "B"), None);

        let mut shaped = nonterminal_fan_in(Direction::TD);
        shaped.nodes[1].shape = NodeShape::Diamond;
        assert_eq!(nonterminal_target_port_count(&shaped, "B"), None);

        let mut subgraph = nonterminal_fan_in(Direction::TD);
        subgraph.add_subgraph(crate::graph::Subgraph::new(
            "group",
            Some("Group".to_string()),
        ));
        assert_eq!(nonterminal_target_port_count(&subgraph, "B"), None);
    }

    #[test]
    fn selects_internal_subgraph_nonterminal_fan_in_for_vertical_routes() {
        assert_eq!(
            nonterminal_target_port_count(&internal_nonterminal(Direction::TD), "T"),
            Some(2)
        );
        assert_eq!(
            nonterminal_target_port_count(&internal_nonterminal(Direction::BT), "T"),
            Some(2)
        );

        let mut labeled = internal_nonterminal(Direction::TD);
        labeled.edges[1].label = Some("labeled".to_owned());
        assert_eq!(nonterminal_target_port_count(&labeled, "T"), None);
    }

    #[test]
    fn target_ports_are_centered_and_separated() {
        assert_eq!(minimum_port_width(2), 5);
        assert_eq!(minimum_port_width(3), 7);
        assert_eq!(target_port_columns(10, 12, 2), vec![15, 17]);
        assert_eq!(target_port_columns(10, 12, 3), vec![14, 16, 18]);
        assert!(target_port_columns(10, 5, 3).is_empty());
    }
}
