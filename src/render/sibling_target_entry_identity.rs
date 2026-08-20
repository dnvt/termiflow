//! Structural capability for the narrow mixed sibling-target scene.
//!
//! A target can receive one edge from inside its own titled subgraph and one
//! edge from a sibling subgraph. Generic convergence sees only the target and
//! collapses those arrivals to one marker, so the renderer needs a typed
//! topology proof before it can reserve independent target entries.

use crate::graph::{Direction, EdgeKind, Graph, NodeShape};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TdScene {
    pub source_subgraph_id: String,
    pub target_subgraph_id: String,
    pub source_start_node_id: String,
    pub source_end_node_id: String,
    pub target_start_node_id: String,
    pub target_end_node_id: String,
    pub source_internal_edge_index: usize,
    pub target_internal_edge_index: usize,
    pub start_cross_edge_index: usize,
    pub end_cross_edge_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HorizontalScene {
    pub source_subgraph_id: String,
    pub target_subgraph_id: String,
    pub source_start_node_id: String,
    pub source_end_node_id: String,
    pub target_start_node_id: String,
    pub target_end_node_id: String,
    pub source_internal_edge_index: usize,
    pub target_internal_edge_index: usize,
    pub start_cross_edge_index: usize,
    pub end_cross_edge_index: usize,
}

/// Return the exact flat two-sibling TD scene whose target has one internal
/// and one cross-subgraph incoming edge. The predicate is intentionally
/// stricter than ordinary fan-in: it proves the complete four-edge topology
/// before the renderer claims any edge indexes.
pub(crate) fn td_scene(graph: &Graph) -> Option<TdScene> {
    if graph.direction != Direction::TD
        || graph.subgraphs.len() != 2
        || graph.nodes.len() != 4
        || graph.edges.len() != 4
        || graph.has_cycles()
    {
        return None;
    }

    let subgraphs = graph.subgraphs.iter().collect::<Vec<_>>();
    if subgraphs.iter().any(|subgraph| {
        !subgraph.bounds.is_valid()
            || subgraph.parent_id.is_some()
            || !subgraph.child_ids.is_empty()
            || subgraph.title.is_none()
            || subgraph.node_ids.len() != 2
            || !subgraph.node_ids.iter().all(|node_id| {
                graph.get_node(node_id).is_some()
                    && graph.get_node_subgraph(node_id) == Some(subgraph.id.as_str())
            })
    }) {
        return None;
    }
    if graph
        .nodes
        .iter()
        .any(|node| node.shape != NodeShape::Rectangle)
        || subgraphs[0].bounds.y == subgraphs[1].bounds.y
    {
        return None;
    }

    let ordinary_edges = graph
        .edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| {
            !edge.is_back_edge && edge.kind == EdgeKind::Arrow && edge.label.is_none()
        })
        .collect::<Vec<_>>();
    if ordinary_edges.len() != graph.edges.len() {
        return None;
    }

    let source_subgraph = subgraphs.iter().min_by_key(|subgraph| subgraph.bounds.y)?;
    let target_subgraph = subgraphs
        .iter()
        .find(|subgraph| subgraph.id != source_subgraph.id)?;
    if source_subgraph.bounds.y >= target_subgraph.bounds.y {
        return None;
    }

    let internal_edges = |subgraph: &crate::graph::Subgraph| {
        ordinary_edges
            .iter()
            .filter(|(_, edge)| {
                subgraph.node_ids.contains(&edge.from) && subgraph.node_ids.contains(&edge.to)
            })
            .copied()
            .collect::<Vec<_>>()
    };
    let source_internal = internal_edges(source_subgraph);
    let target_internal = internal_edges(target_subgraph);
    if source_internal.len() != 1 || target_internal.len() != 1 {
        return None;
    }

    let source_start_node_id = source_internal[0].1.from.clone();
    let source_end_node_id = source_internal[0].1.to.clone();
    let target_start_node_id = target_internal[0].1.from.clone();
    let target_end_node_id = target_internal[0].1.to.clone();
    let source_start = graph.get_node(&source_start_node_id)?;
    let source_end = graph.get_node(&source_end_node_id)?;
    let target_start = graph.get_node(&target_start_node_id)?;
    let target_end = graph.get_node(&target_end_node_id)?;
    if source_start.center_y() >= source_end.center_y()
        || target_start.center_y() >= target_end.center_y()
    {
        return None;
    }

    let cross_edges = ordinary_edges
        .iter()
        .filter(|(_, edge)| {
            source_subgraph.node_ids.contains(&edge.from)
                && target_subgraph.node_ids.contains(&edge.to)
        })
        .copied()
        .collect::<Vec<_>>();
    if cross_edges.len() != 2 {
        return None;
    }

    let start_cross = cross_edges
        .iter()
        .find(|(_, edge)| edge.from == source_start_node_id && edge.to == target_start_node_id)?;
    let end_cross = cross_edges
        .iter()
        .find(|(_, edge)| edge.from == source_end_node_id && edge.to == target_end_node_id)?;

    Some(TdScene {
        source_subgraph_id: source_subgraph.id.clone(),
        target_subgraph_id: target_subgraph.id.clone(),
        source_start_node_id,
        source_end_node_id,
        target_start_node_id,
        target_end_node_id,
        source_internal_edge_index: source_internal[0].0,
        target_internal_edge_index: target_internal[0].0,
        start_cross_edge_index: start_cross.0,
        end_cross_edge_index: end_cross.0,
    })
}

/// Layout-stage form of [`td_scene`] that does not depend on subgraph bounds.
/// Envelope bounds are not copied back into the parsed graph until rendering,
/// so placement repairs must select the same exact topology using the caller's
/// already-resolved source and target subgraph IDs.
pub(crate) fn td_scene_for_layout(
    graph: &Graph,
    source_subgraph_id: &str,
    target_subgraph_id: &str,
) -> Option<TdScene> {
    if graph.direction != Direction::TD
        || graph.subgraphs.len() != 2
        || graph.nodes.len() != 4
        || graph.edges.len() != 4
        || graph.has_cycles()
    {
        return None;
    }
    let source_subgraph = graph.get_subgraph(source_subgraph_id)?;
    let target_subgraph = graph.get_subgraph(target_subgraph_id)?;
    if source_subgraph.parent_id.is_some()
        || target_subgraph.parent_id.is_some()
        || !source_subgraph.child_ids.is_empty()
        || !target_subgraph.child_ids.is_empty()
        || source_subgraph.title.is_none()
        || target_subgraph.title.is_none()
        || source_subgraph.node_ids.len() != 2
        || target_subgraph.node_ids.len() != 2
        || source_subgraph.node_ids.iter().any(|node_id| {
            graph.get_node(node_id).is_none()
                || graph.get_node_subgraph(node_id) != Some(source_subgraph_id)
        })
        || target_subgraph.node_ids.iter().any(|node_id| {
            graph.get_node(node_id).is_none()
                || graph.get_node_subgraph(node_id) != Some(target_subgraph_id)
        })
        || graph
            .nodes
            .iter()
            .any(|node| node.shape != NodeShape::Rectangle)
    {
        return None;
    }

    let ordinary_edges = graph
        .edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| {
            !edge.is_back_edge && edge.kind == EdgeKind::Arrow && edge.label.is_none()
        })
        .collect::<Vec<_>>();
    if ordinary_edges.len() != graph.edges.len() {
        return None;
    }
    let internal_edges = |subgraph: &crate::graph::Subgraph| {
        ordinary_edges
            .iter()
            .filter(|(_, edge)| {
                subgraph.node_ids.contains(&edge.from) && subgraph.node_ids.contains(&edge.to)
            })
            .copied()
            .collect::<Vec<_>>()
    };
    let source_internal = internal_edges(source_subgraph);
    let target_internal = internal_edges(target_subgraph);
    if source_internal.len() != 1 || target_internal.len() != 1 {
        return None;
    }
    let source_start_node_id = source_internal[0].1.from.clone();
    let source_end_node_id = source_internal[0].1.to.clone();
    let target_start_node_id = target_internal[0].1.from.clone();
    let target_end_node_id = target_internal[0].1.to.clone();
    let cross_edges = ordinary_edges
        .iter()
        .filter(|(_, edge)| {
            source_subgraph.node_ids.contains(&edge.from)
                && target_subgraph.node_ids.contains(&edge.to)
        })
        .copied()
        .collect::<Vec<_>>();
    if cross_edges.len() != 2 {
        return None;
    }
    let start_cross = cross_edges
        .iter()
        .find(|(_, edge)| edge.from == source_start_node_id && edge.to == target_start_node_id)?;
    let end_cross = cross_edges
        .iter()
        .find(|(_, edge)| edge.from == source_end_node_id && edge.to == target_end_node_id)?;
    Some(TdScene {
        source_subgraph_id: source_subgraph_id.to_owned(),
        target_subgraph_id: target_subgraph_id.to_owned(),
        source_start_node_id,
        source_end_node_id,
        target_start_node_id,
        target_end_node_id,
        source_internal_edge_index: source_internal[0].0,
        target_internal_edge_index: target_internal[0].0,
        start_cross_edge_index: start_cross.0,
        end_cross_edge_index: end_cross.0,
    })
}

/// Return the exact flat two-sibling LR/RL scene whose target has one
/// internal and one cross-subgraph incoming edge.  The horizontal lowerer
/// consumes this proof before it reserves side rows or boundary portals.
pub(crate) fn horizontal_scene(graph: &Graph) -> Option<HorizontalScene> {
    if !matches!(graph.direction, Direction::LR | Direction::RL)
        || graph.subgraphs.len() != 2
        || graph.nodes.len() != 4
        || graph.edges.len() != 4
        || graph.has_cycles()
    {
        return None;
    }

    let subgraphs = graph.subgraphs.iter().collect::<Vec<_>>();
    if subgraphs.iter().any(|subgraph| {
        subgraph.parent_id.is_some()
            || !subgraph.child_ids.is_empty()
            || subgraph.title.is_none()
            || subgraph.node_ids.len() != 2
            || !subgraph.node_ids.iter().all(|node_id| {
                graph.get_node(node_id).is_some()
                    && graph.get_node_subgraph(node_id) == Some(subgraph.id.as_str())
            })
    }) {
        return None;
    }
    if graph
        .nodes
        .iter()
        .any(|node| node.shape != NodeShape::Rectangle)
    {
        return None;
    }

    let ordinary_edges = graph
        .edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| {
            !edge.is_back_edge && edge.kind == EdgeKind::Arrow && edge.label.is_none()
        })
        .collect::<Vec<_>>();
    if ordinary_edges.len() != graph.edges.len() {
        return None;
    }

    let first = subgraphs[0];
    let second = subgraphs[1];
    let first_to_second = ordinary_edges
        .iter()
        .filter(|(_, edge)| {
            first.node_ids.contains(&edge.from) && second.node_ids.contains(&edge.to)
        })
        .count();
    let second_to_first = ordinary_edges
        .iter()
        .filter(|(_, edge)| {
            second.node_ids.contains(&edge.from) && first.node_ids.contains(&edge.to)
        })
        .count();
    let (source_subgraph, target_subgraph) = match (first_to_second, second_to_first) {
        (2, 0) => (first, second),
        (0, 2) => (second, first),
        _ => return None,
    };
    let internal_edges = |subgraph: &crate::graph::Subgraph| {
        ordinary_edges
            .iter()
            .filter(|(_, edge)| {
                subgraph.node_ids.contains(&edge.from) && subgraph.node_ids.contains(&edge.to)
            })
            .copied()
            .collect::<Vec<_>>()
    };
    let source_internal = internal_edges(source_subgraph);
    let target_internal = internal_edges(target_subgraph);
    if source_internal.len() != 1 || target_internal.len() != 1 {
        return None;
    }

    let source_start_node_id = source_internal[0].1.from.clone();
    let source_end_node_id = source_internal[0].1.to.clone();
    let target_start_node_id = target_internal[0].1.from.clone();
    let target_end_node_id = target_internal[0].1.to.clone();
    let cross_edges = ordinary_edges
        .iter()
        .filter(|(_, edge)| {
            source_subgraph.node_ids.contains(&edge.from)
                && target_subgraph.node_ids.contains(&edge.to)
        })
        .copied()
        .collect::<Vec<_>>();
    if cross_edges.len() != 2 {
        return None;
    }

    let start_cross = cross_edges
        .iter()
        .find(|(_, edge)| edge.from == source_start_node_id && edge.to == target_start_node_id)?;
    let end_cross = cross_edges
        .iter()
        .find(|(_, edge)| edge.from == source_end_node_id && edge.to == target_end_node_id)?;

    Some(HorizontalScene {
        source_subgraph_id: source_subgraph.id.clone(),
        target_subgraph_id: target_subgraph.id.clone(),
        source_start_node_id,
        source_end_node_id,
        target_start_node_id,
        target_end_node_id,
        source_internal_edge_index: source_internal[0].0,
        target_internal_edge_index: target_internal[0].0,
        start_cross_edge_index: start_cross.0,
        end_cross_edge_index: end_cross.0,
    })
}

pub(crate) fn horizontal_target_port_counts(graph: &Graph) -> Vec<(String, usize)> {
    horizontal_scene(graph)
        .map(|scene| (scene.target_end_node_id, 2))
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{horizontal_scene, td_scene};
    use crate::graph::{Direction, Edge, Graph, Node, Rectangle, Subgraph};

    fn graph() -> Graph {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;
        for (id, label) in [
            ("A", "Node A"),
            ("B", "Node B"),
            ("C", "Node C"),
            ("D", "Node D"),
        ] {
            graph.add_node(Node::new(id, label));
        }
        let mut left = Subgraph::new("Left", Some("Left Group".to_owned()));
        left.bounds = Rectangle::new(0, 0, 30, 18);
        left.add_node("A");
        left.add_node("B");
        let mut right = Subgraph::new("Right", Some("Right Group".to_owned()));
        right.bounds = Rectangle::new(0, 22, 30, 18);
        right.add_node("C");
        right.add_node("D");
        graph.add_subgraph(left);
        graph.add_subgraph(right);
        for id in ["A", "B"] {
            graph.associate_node_with_subgraph(id, "Left");
        }
        for id in ["C", "D"] {
            graph.associate_node_with_subgraph(id, "Right");
        }
        graph.add_edge(Edge::new("A", "B"));
        graph.add_edge(Edge::new("C", "D"));
        graph.add_edge(Edge::new("A", "C"));
        graph.add_edge(Edge::new("B", "D"));
        graph.nodes[0].y = 4;
        graph.nodes[1].y = 10;
        graph.nodes[2].y = 26;
        graph.nodes[3].y = 32;
        graph
    }

    fn horizontal_graph(direction: Direction) -> Graph {
        let mut graph = graph();
        graph.direction = direction;
        if direction == Direction::LR {
            graph.subgraphs[0].bounds = Rectangle::new(0, 0, 30, 18);
            graph.subgraphs[1].bounds = Rectangle::new(32, 0, 30, 18);
        } else {
            graph.subgraphs[0].bounds = Rectangle::new(32, 0, 30, 18);
            graph.subgraphs[1].bounds = Rectangle::new(0, 0, 30, 18);
        }
        for node in &mut graph.nodes {
            node.y = match node.id.as_str() {
                "A" => 7,
                "B" => 5,
                "C" => 9,
                "D" => 7,
                _ => 0,
            };
        }
        graph
    }

    #[test]
    fn selects_exact_mixed_target_topology() {
        let scene = td_scene(&graph()).expect("mixed TD scene");
        assert_eq!(scene.source_subgraph_id, "Left");
        assert_eq!(scene.target_subgraph_id, "Right");
        assert_eq!(
            [
                scene.source_internal_edge_index,
                scene.target_internal_edge_index,
                scene.start_cross_edge_index,
                scene.end_cross_edge_index,
            ],
            [0, 1, 2, 3]
        );
    }

    #[test]
    fn rejects_other_directions_labels_and_extra_topology() {
        let mut labeled = graph();
        labeled.edges[3].label = Some("crossing".to_owned());
        assert!(td_scene(&labeled).is_none());

        let mut other_direction = graph();
        other_direction.direction = Direction::BT;
        assert!(td_scene(&other_direction).is_none());

        let mut extra = graph();
        extra.add_node(Node::new("X", "Extra"));
        assert!(td_scene(&extra).is_none());
    }

    #[test]
    fn selects_exact_horizontal_mixed_target_topology() {
        for direction in [Direction::LR, Direction::RL] {
            let scene =
                horizontal_scene(&horizontal_graph(direction)).expect("mixed horizontal scene");
            assert_eq!(scene.source_subgraph_id, "Left");
            assert_eq!(scene.target_subgraph_id, "Right");
            assert_eq!(
                [
                    scene.source_internal_edge_index,
                    scene.target_internal_edge_index,
                    scene.start_cross_edge_index,
                    scene.end_cross_edge_index,
                ],
                [0, 1, 2, 3]
            );
        }
    }

    #[test]
    fn rejects_horizontal_scene_negatives() {
        let mut labeled = horizontal_graph(Direction::LR);
        labeled.edges[3].label = Some("crossing".to_owned());
        assert!(horizontal_scene(&labeled).is_none());

        let wrong_direction = horizontal_graph(Direction::TD);
        assert!(horizontal_scene(&wrong_direction).is_none());

        let mut extra = horizontal_graph(Direction::RL);
        extra.add_node(Node::new("X", "Extra"));
        assert!(horizontal_scene(&extra).is_none());

        let mut mismatch = horizontal_graph(Direction::LR);
        mismatch.edges[3].to = "C".to_owned();
        assert!(horizontal_scene(&mismatch).is_none());
    }
}
