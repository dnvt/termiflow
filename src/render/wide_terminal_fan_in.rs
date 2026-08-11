//! Structural policy for the proof-gated wide terminal fan-in scene.
//!
//! The ordinary convergence lowerer intentionally keeps a many-edge sink on
//! one collector. This policy is narrower than that general behavior and the
//! existing two/three-source vertical policy: it selects only a pure,
//! rectangle-only, unlabeled terminal fan-in with four through eight distinct
//! sources. The route lowerer may still reject the scene when the measured
//! corridor cannot prove a safe plan.

use std::collections::{HashMap, HashSet};

use crate::graph::{Direction, EdgeKind, Graph, NodeShape};

pub(crate) const MIN_WIDE_SOURCE_COUNT: usize = 4;
pub(crate) const MAX_WIDE_SOURCE_COUNT: usize = 8;
const WIDE_PORT_PITCH: usize = 3;
/// Keep neighboring channel rows visually separate so a horizontal segment
/// cannot be mistaken for a junction with another route's vertical leg.
pub(crate) const WIDE_CHANNEL_PITCH: usize = 2;
/// Keep horizontal source rows separated while exposing one target-side row
/// per incoming edge. The default three-row source box plus one blank row
/// gives a stable direct-route pitch of four.
pub(crate) const WIDE_HORIZONTAL_ROW_PITCH: usize = 4;

/// Return the incoming-edge count for a pure wide terminal fan-in target.
pub(crate) fn target_port_count(graph: &Graph, target_id: &str) -> Option<usize> {
    if !matches!(
        graph.direction,
        Direction::TD | Direction::BT | Direction::LR | Direction::RL
    ) || graph.nodes.len() < MIN_WIDE_SOURCE_COUNT + 1
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
    if !(MIN_WIDE_SOURCE_COUNT..=MAX_WIDE_SOURCE_COUNT).contains(&count)
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

/// Return all selected wide terminal targets and their port counts.
pub(crate) fn target_port_counts(graph: &Graph) -> HashMap<String, usize> {
    graph
        .nodes
        .iter()
        .filter_map(|node| target_port_count(graph, &node.id).map(|count| (node.id.clone(), count)))
        .collect()
}

/// Minimum target width for separated interior target ports.
pub(crate) fn minimum_port_width(count: usize) -> usize {
    count
        .saturating_sub(1)
        .saturating_mul(WIDE_PORT_PITCH)
        .saturating_add(4)
        .max(5)
}

/// Minimum target height for separated interior target ports in LR/RL.
pub(crate) fn minimum_port_height(count: usize) -> usize {
    if !(MIN_WIDE_SOURCE_COUNT..=MAX_WIDE_SOURCE_COUNT).contains(&count) {
        return 0;
    }
    count.saturating_mul(WIDE_HORIZONTAL_ROW_PITCH)
}

/// The primary-axis gap needed for one distinct horizontal channel per source.
///
/// The two extra cells are the source/target attachment boundaries. The
/// returned value is consumed by layout and the route proof, so a late route
/// lowerer cannot silently assume a larger corridor than layout reserved.
pub(crate) fn required_primary_gap(count: usize) -> usize {
    count.saturating_mul(WIDE_CHANNEL_PITCH).saturating_add(2)
}

/// Return centered, separated target-side columns for a selected wide target.
pub(crate) fn target_port_columns(x: usize, width: usize, count: usize) -> Vec<usize> {
    if !(MIN_WIDE_SOURCE_COUNT..=MAX_WIDE_SOURCE_COUNT).contains(&count)
        || width < minimum_port_width(count)
    {
        return Vec::new();
    }

    let center = x.saturating_add(width / 2);
    let start = center.saturating_sub(count.saturating_sub(1).saturating_mul(WIDE_PORT_PITCH) / 2);
    let columns: Vec<usize> = (0..count)
        .map(|index| start.saturating_add(index.saturating_mul(WIDE_PORT_PITCH)))
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

/// Return separated target-side rows for a horizontal wide fan-in.
pub(crate) fn target_port_rows(y: usize, height: usize, count: usize) -> Vec<usize> {
    if height < minimum_port_height(count) {
        return Vec::new();
    }

    (0..count)
        .map(|index| {
            y.saturating_add(1)
                .saturating_add(index.saturating_mul(WIDE_HORIZONTAL_ROW_PITCH))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        minimum_port_height, minimum_port_width, required_primary_gap, target_port_columns,
        target_port_count, target_port_rows,
    };
    use crate::graph::{Direction, Edge, Graph, Node};

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
    fn selects_only_four_to_eight_source_terminal_fan_in() {
        assert_eq!(
            target_port_count(&terminal_fan_in(Direction::BT, 4), "T"),
            Some(4)
        );
        assert_eq!(
            target_port_count(&terminal_fan_in(Direction::TD, 8), "T"),
            Some(8)
        );
        assert_eq!(
            target_port_count(&terminal_fan_in(Direction::LR, 4), "T"),
            Some(4)
        );
        assert_eq!(
            target_port_count(&terminal_fan_in(Direction::RL, 8), "T"),
            Some(8)
        );
        assert_eq!(
            target_port_count(&terminal_fan_in(Direction::BT, 3), "T"),
            None
        );
        assert_eq!(
            target_port_count(&terminal_fan_in(Direction::TD, 9), "T"),
            None
        );
    }

    #[test]
    fn rejects_wide_family_controls() {
        let mut labeled = terminal_fan_in(Direction::BT, 4);
        labeled.edges[0].label = Some("labeled".to_string());
        assert_eq!(target_port_count(&labeled, "T"), None);

        let mut cascade = terminal_fan_in(Direction::BT, 4);
        cascade.add_node(Node::new("O", "Other"));
        cascade.add_edge(Edge::new("T", "O"));
        assert_eq!(target_port_count(&cascade, "T"), None);
    }

    #[test]
    fn reserves_a_channel_and_two_attachment_cells_per_source() {
        assert_eq!(required_primary_gap(4), 10);
        assert_eq!(required_primary_gap(8), 18);
        assert_eq!(minimum_port_width(8), 25);
        assert_eq!(
            target_port_columns(33, 25, 8),
            vec![35, 38, 41, 44, 47, 50, 53, 56]
        );
        assert!(target_port_columns(33, 24, 8).is_empty());
    }

    #[test]
    fn reserves_one_direct_horizontal_target_row_per_source() {
        assert_eq!(minimum_port_height(4), 16);
        assert_eq!(minimum_port_height(8), 32);
        assert_eq!(
            target_port_rows(0, 32, 8),
            vec![1, 5, 9, 13, 17, 21, 25, 29]
        );
        assert!(target_port_rows(0, 31, 8).is_empty());
    }
}
