//! Shared policy for ordinary fan-in scenes that preserve target identity.
//!
//! The generic convergence lowerer intentionally uses one shared target arrow.
//! This policy selects the narrower, measurable family where every incoming
//! edge must remain individually visible.  Routing and measurement consume
//! this module so a route cannot request capacity the layout did not reserve.

use std::collections::{HashMap, HashSet};

use crate::graph::{Direction, EdgeKind, Graph, NodeShape};

pub(crate) const MIN_TARGET_PORTS: usize = 2;
pub(crate) const MAX_TARGET_PORTS: usize = 4;
pub(crate) const PORT_PITCH: usize = 2;

/// Return the target-side port count for the exact labeled terminal fan-in.
///
/// Labeled edges are normally kept off the ordinary identity policy because
/// label placement owns additional cells near a route.  This three-node,
/// two-edge terminal scene is the bounded exception: its labels are drawn
/// after the routes and the target has enough measured width/height for two
/// distinct entries.  Keeping the exception exact prevents a generic labeled
/// convergence from being widened accidentally.
pub(crate) fn labeled_terminal_fan_in_target_count(
    graph: &Graph,
    target_id: &str,
) -> Option<usize> {
    if !matches!(
        graph.direction,
        Direction::TD | Direction::TB | Direction::BT | Direction::LR | Direction::RL
    ) || graph.nodes.len() != 3
        || graph.edges.len() != 2
        || !graph.subgraphs.is_empty()
        || graph.has_cycles()
        || graph
            .nodes
            .iter()
            .any(|node| node.shape != NodeShape::Rectangle)
    {
        return None;
    }

    let target = graph.get_node(target_id)?;
    if target.shape != NodeShape::Rectangle
        || graph.edges.iter().any(|edge| {
            edge.to != target_id
                || edge.from == target_id
                || edge.is_back_edge
                || edge.kind != EdgeKind::Arrow
                || edge.label.is_none()
        })
    {
        return None;
    }

    let incoming: Vec<&str> = graph.edges.iter().map(|edge| edge.from.as_str()).collect();
    let source_ids: HashSet<&str> = incoming.iter().copied().collect();
    let node_ids: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    if incoming.len() != MIN_TARGET_PORTS
        || source_ids.len() != incoming.len()
        || source_ids.contains(target_id)
        || source_ids.len() + 1 != node_ids.len()
        || !source_ids.iter().all(|source_id| {
            node_ids.contains(source_id)
                && graph
                    .get_node(source_id)
                    .is_some_and(|node| node.shape == NodeShape::Rectangle)
        })
    {
        return None;
    }

    Some(incoming.len())
}

/// Return whether a target is the rejoin point of a compact TD/BT branch.
///
/// This exact shape is narrow enough to use the target-identity contract: each
/// incoming edge needs its own target-side port so the branch rejoin remains
/// countable to a human reader.
pub(crate) fn is_vertical_branch_rejoin_target(graph: &Graph, target_id: &str) -> bool {
    if !matches!(graph.direction, Direction::TD | Direction::BT)
        || graph.nodes.len() != 4
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
        return false;
    }

    let incoming_sources: Vec<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.to == target_id)
        .map(|edge| edge.from.as_str())
        .collect();
    if incoming_sources.len() != 2 {
        return false;
    }
    let source_ids: HashSet<&str> = incoming_sources.iter().copied().collect();
    if source_ids.len() != incoming_sources.len()
        || source_ids.contains(target_id)
        || !source_ids
            .iter()
            .all(|source_id| graph.get_node(source_id).is_some())
    {
        return false;
    }

    let common_parents: HashSet<&str> = graph
        .nodes
        .iter()
        .filter(|node| {
            node.id != target_id
                && !source_ids.contains(node.id.as_str())
                && source_ids.iter().all(|source_id| {
                    graph
                        .edges
                        .iter()
                        .any(|edge| edge.from == node.id && edge.to == *source_id)
                })
        })
        .map(|node| node.id.as_str())
        .collect();
    if common_parents.len() != 1 {
        return false;
    }
    let parent_id = *common_parents
        .iter()
        .next()
        .expect("one common branch parent");

    let expected_edges: HashSet<(&str, &str)> = source_ids
        .iter()
        .copied()
        .map(|source_id| (source_id, target_id))
        .chain(
            source_ids
                .iter()
                .copied()
                .map(|source_id| (parent_id, source_id)),
        )
        .collect();
    expected_edges.len() == graph.edges.len()
        && graph
            .edges
            .iter()
            .all(|edge| expected_edges.contains(&(edge.from.as_str(), edge.to.as_str())))
}

/// Primary-axis corridor needed by a vertical identity lane route.
///
/// Every selected vertical identity scene needs one independent turn lane per
/// source, with a blank primary cell between neighboring lanes. Horizontal
/// identity routes use their target-side rows and do not consume this gap.
pub(crate) const fn required_primary_gap(count: usize) -> usize {
    if count < MIN_TARGET_PORTS {
        0
    } else {
        count.saturating_mul(PORT_PITCH).saturating_add(2)
    }
}

/// Return the required target-side port count for an ordinary identity scene.
///
/// The policy is intentionally conservative.  It excludes subgraph portals,
/// contours, labels, cycles, and unsupported edge kinds until those ownership
/// contracts have their own scene lowerers.
pub(crate) fn target_port_count(graph: &Graph, target_id: &str) -> Option<usize> {
    if let Some(count) = labeled_terminal_fan_in_target_count(graph, target_id) {
        return Some(count);
    }

    if !matches!(
        graph.direction,
        Direction::TD | Direction::TB | Direction::BT | Direction::LR | Direction::RL
    ) || graph.nodes.is_empty()
        || !graph.subgraphs.is_empty()
        || graph.has_cycles()
        || graph
            .nodes
            .iter()
            .any(|node| !matches!(node.shape, NodeShape::Rectangle | NodeShape::Database))
        || graph.edges.iter().any(|edge| {
            edge.is_back_edge
                || edge.kind != EdgeKind::Arrow
                || edge.label.is_some()
                || edge.from == edge.to
        })
    {
        return None;
    }

    let target = graph.get_node(target_id)?;
    let is_vertical_branch_rejoin = is_vertical_branch_rejoin_target(graph, target_id);
    let incoming: Vec<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.to == target_id)
        .map(|edge| edge.from.as_str())
        .collect();
    if !(MIN_TARGET_PORTS..=MAX_TARGET_PORTS).contains(&incoming.len()) {
        return None;
    }

    let source_ids: HashSet<&str> = incoming.iter().copied().collect();
    if source_ids.len() != incoming.len() || source_ids.contains(target_id) {
        return None;
    }

    // The exact vertical branch-rejoin predicate is a deliberate identity
    // scene, not a generic convergence exception.  Keep this positive branch
    // next to the shared target-port count so measurement and routing agree.
    if is_vertical_branch_rejoin {
        return Some(incoming.len());
    }

    // Database contours have a shape-owned entry clearance and are only
    // claimed here when they are terminal targets.  The intermediate
    // database case has a separate routing contract because its outgoing
    // branch can compete with the target-side corridor of a later merge.
    if target.shape == NodeShape::Database && graph.edges.iter().any(|edge| edge.from == target_id)
    {
        return None;
    }

    // A parsed graph should have valid endpoints, but keep the policy
    // fail-closed when it is called from a direct API/test construction.
    let node_ids: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    source_ids
        .iter()
        .all(|source_id| {
            node_ids.contains(source_id)
                && graph.get_node(source_id).is_some_and(|node| {
                    matches!(node.shape, NodeShape::Rectangle | NodeShape::Database)
                })
        })
        .then_some(incoming.len())
}

/// Return all ordinary identity targets and their required port counts.
pub(crate) fn target_port_counts(graph: &Graph) -> HashMap<String, usize> {
    graph
        .nodes
        .iter()
        .filter_map(|node| target_port_count(graph, &node.id).map(|count| (node.id.clone(), count)))
        .collect()
}

/// Minimum target dimension for `count` distinct interior ports.
pub(crate) fn minimum_port_span(count: usize) -> usize {
    count
        .saturating_sub(1)
        .saturating_mul(PORT_PITCH)
        .saturating_add(3)
        .max(5)
}

/// Return centered target-side columns for TD/BT identity ports.
pub(crate) fn target_port_columns(x: usize, width: usize, count: usize) -> Vec<usize> {
    if !(MIN_TARGET_PORTS..=MAX_TARGET_PORTS).contains(&count) || width < minimum_port_span(count) {
        return Vec::new();
    }

    let center = x + width / 2;
    let start = center.saturating_sub(count.saturating_sub(1));
    let columns: Vec<usize> = (0..count)
        .map(|index| start.saturating_add(index.saturating_mul(PORT_PITCH)))
        .collect();
    let right = x.saturating_add(width);
    if columns
        .iter()
        .any(|column| *column <= x || *column >= right.saturating_sub(1))
    {
        Vec::new()
    } else {
        columns
    }
}

/// Return centered target-side rows for LR/RL identity ports.
pub(crate) fn target_port_rows(y: usize, height: usize, count: usize) -> Vec<usize> {
    if !(MIN_TARGET_PORTS..=MAX_TARGET_PORTS).contains(&count) || height < minimum_port_span(count)
    {
        return Vec::new();
    }

    let center = y + height / 2;
    let start = center.saturating_sub(count.saturating_sub(1));
    let rows: Vec<usize> = (0..count)
        .map(|index| start.saturating_add(index.saturating_mul(PORT_PITCH)))
        .collect();
    let bottom = y.saturating_add(height);
    if rows
        .iter()
        .any(|row| *row <= y || *row >= bottom.saturating_sub(1))
    {
        Vec::new()
    } else {
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::{
        is_vertical_branch_rejoin_target, labeled_terminal_fan_in_target_count, minimum_port_span,
        required_primary_gap, target_port_columns, target_port_count, target_port_rows,
    };
    use crate::graph::{Direction, Edge, Graph, Node, NodeShape};

    fn ordinary(direction: Direction, count: usize) -> Graph {
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

    fn labeled_terminal(direction: Direction) -> Graph {
        let mut graph = Graph::new();
        graph.direction = direction;
        graph.add_node(Node::new("A", "Source"));
        graph.add_node(Node::new("B", "Other"));
        graph.add_node(Node::new("T", "Target"));
        graph.add_edge(Edge::with_label("A", "T", "label 1"));
        graph.add_edge(Edge::with_label("B", "T", "label 2"));
        graph
    }

    fn branch_rejoin(direction: Direction) -> Graph {
        let mut graph = Graph::new();
        graph.direction = direction;
        for (id, label) in [("A", "A"), ("B", "B"), ("C", "C"), ("D", "D")] {
            graph.add_node(Node::new(id, label));
        }
        graph.add_edge(Edge::new("A", "B"));
        graph.add_edge(Edge::new("A", "C"));
        graph.add_edge(Edge::new("B", "D"));
        graph.add_edge(Edge::new("C", "D"));
        graph
    }

    #[test]
    fn recognizes_vertical_branch_rejoin_as_identity_target() {
        for direction in [Direction::TD, Direction::BT] {
            let graph = branch_rejoin(direction);
            assert!(is_vertical_branch_rejoin_target(&graph, "D"));
            assert_eq!(target_port_count(&graph, "D"), Some(2));
        }

        let horizontal = branch_rejoin(Direction::LR);
        assert!(!is_vertical_branch_rejoin_target(&horizontal, "D"));
        assert_eq!(target_port_count(&horizontal, "D"), Some(2));
    }

    #[test]
    fn keeps_pure_terminal_vertical_fan_in_on_identity_ports() {
        for direction in [Direction::TD, Direction::BT] {
            assert_eq!(target_port_count(&ordinary(direction, 2), "T"), Some(2));
        }
    }

    #[test]
    fn selects_only_the_exact_labeled_terminal_fan_in() {
        for direction in [
            Direction::TD,
            Direction::TB,
            Direction::BT,
            Direction::LR,
            Direction::RL,
        ] {
            let graph = labeled_terminal(direction);
            assert_eq!(
                labeled_terminal_fan_in_target_count(&graph, "T"),
                Some(2),
                "labeled terminal selector should cover {direction:?}"
            );
            assert_eq!(target_port_count(&graph, "T"), Some(2));
        }

        let mut near_miss = labeled_terminal(Direction::LR);
        near_miss.edges[1].label = None;
        assert_eq!(target_port_count(&near_miss, "T"), None);

        let mut extra_edge = labeled_terminal(Direction::TD);
        extra_edge.add_node(Node::new("O", "Output"));
        extra_edge.add_edge(Edge::new("T", "O"));
        assert_eq!(target_port_count(&extra_edge, "T"), None);
    }

    #[test]
    fn selects_two_three_and_four_source_ordinary_identity_targets() {
        assert_eq!(target_port_count(&ordinary(Direction::TD, 2), "T"), Some(2));
        assert_eq!(target_port_count(&ordinary(Direction::LR, 3), "T"), Some(3));
        assert_eq!(target_port_count(&ordinary(Direction::BT, 4), "T"), Some(4));
    }

    #[test]
    fn selects_four_source_identity_targets_in_every_supported_direction() {
        for direction in [
            Direction::TD,
            Direction::TB,
            Direction::BT,
            Direction::LR,
            Direction::RL,
        ] {
            assert_eq!(
                target_port_count(&ordinary(direction, 4), "T"),
                Some(4),
                "four-source identity target should be selected for {direction:?}"
            );
        }
    }

    #[test]
    fn rejects_labels_unsupported_shapes_subgraphs_and_cycles() {
        let mut labeled = ordinary(Direction::LR, 2);
        labeled.edges[0].label = Some("labeled".to_string());
        assert_eq!(target_port_count(&labeled, "T"), None);

        let mut shaped = ordinary(Direction::TD, 2);
        shaped.nodes[0].shape = NodeShape::Diamond;
        assert_eq!(target_port_count(&shaped, "T"), None);

        let mut cyclic = ordinary(Direction::BT, 2);
        cyclic.edges[0].is_back_edge = true;
        assert_eq!(target_port_count(&cyclic, "T"), None);
    }

    #[test]
    fn selects_terminal_database_targets_with_simple_sources() {
        let mut graph = ordinary(Direction::TD, 2);
        graph.nodes[0].shape = NodeShape::Database;
        graph.nodes.push(Node::with_shape(
            "D",
            "Database target",
            NodeShape::Database,
        ));
        graph.edges.clear();
        graph.add_edge(Edge::new("S0", "D"));
        graph.add_edge(Edge::new("S1", "D"));

        assert_eq!(target_port_count(&graph, "D"), Some(2));
    }

    #[test]
    fn rejects_intermediate_database_targets_until_their_branch_is_proven() {
        let mut graph = ordinary(Direction::TD, 2);
        graph.nodes.push(Node::with_shape(
            "D",
            "Database target",
            NodeShape::Database,
        ));
        graph.edges.clear();
        graph.add_edge(Edge::new("S0", "D"));
        graph.add_edge(Edge::new("S1", "D"));

        assert_eq!(target_port_count(&graph, "D"), Some(2));

        graph.add_node(Node::new("O", "Output"));
        graph.edges.push(Edge::new("D", "O"));
        assert_eq!(target_port_count(&graph, "D"), None);
    }

    #[test]
    fn target_ports_are_centered_with_a_blank_separator() {
        assert_eq!(minimum_port_span(2), 5);
        assert_eq!(minimum_port_span(3), 7);
        assert_eq!(minimum_port_span(4), 9);
        assert_eq!(required_primary_gap(2), 6);
        assert_eq!(required_primary_gap(3), 8);
        assert_eq!(required_primary_gap(4), 10);
        assert_eq!(target_port_columns(10, 8, 2), vec![13, 15]);
        assert_eq!(target_port_rows(10, 8, 2), vec![13, 15]);
        assert_eq!(target_port_columns(10, 10, 4), vec![12, 14, 16, 18]);
        assert_eq!(target_port_rows(10, 10, 4), vec![12, 14, 16, 18]);
        assert!(target_port_columns(10, 4, 2).is_empty());
        assert!(target_port_rows(10, 4, 2).is_empty());
    }
}
