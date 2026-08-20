//! Conservative default-layout balancing for nodes that both merge and split.
//!
//! The ordinary placement pass intentionally protects fan-in targets during
//! its upward sweep. That protection is useful for pure merges, but it leaves
//! a merge-then-split anchor biased toward the first outgoing branch. This
//! module makes the smallest layout-local correction for that shape without
//! depending on rendered glyphs or critic findings.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::geom::{Point, Rect};
use crate::graph::Graph;
use crate::orientation::{Axis, OrientedCoords};

const MAX_CONTEXT_NODES: usize = 16;
const MIDPOINT_TOLERANCE: usize = 1;

/// Recenter default-layout dual junctions before subgraph envelopes and routes
/// are projected. The pass is deterministic and intentionally conservative:
/// an unsafe local movement is rejected rather than widened into a global
/// layout adjustment.
pub(super) fn balance_dual_junctions(
    graph: &Graph,
    layers: &[Vec<usize>],
    coords: &OrientedCoords,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
) {
    let rank_by_id = rank_by_id(graph, layers);
    let anchors = dual_junction_ids(graph);

    for anchor_id in anchors {
        let Some(anchor_rect) = node_rects.get(&anchor_id).copied() else {
            continue;
        };
        let targets = outgoing_targets(graph, &anchor_id);
        let Some(target_midpoint) = secondary_midpoint(&targets, node_rects, coords) else {
            continue;
        };
        let anchor_center = secondary_center(&anchor_rect, coords);
        if anchor_center.abs_diff(target_midpoint) <= MIDPOINT_TOLERANCE {
            continue;
        }

        let delta = signed_delta(target_midpoint, anchor_center);
        let Some(context_ids) = reverse_context(graph, &anchor_id) else {
            continue;
        };
        let context_candidate = ShiftCandidate::new(context_ids, delta);
        let context_is_safe =
            context_candidate.is_safe(graph, &rank_by_id, coords, positions, node_rects);

        if context_is_safe {
            context_candidate.apply(coords, positions, node_rects);
            continue;
        }

        // If the incoming context is constrained, try moving the outgoing
        // branch group as one unit. This alternative is only retained when it
        // passes the same boundary, coordinate, and collision checks.
        let branch_delta = signed_delta(anchor_center, target_midpoint);
        let branch_candidate = ShiftCandidate::new(targets, branch_delta);
        if branch_candidate.is_safe(graph, &rank_by_id, coords, positions, node_rects) {
            branch_candidate.apply(coords, positions, node_rects);
        }
    }
}

pub(super) fn vertical_fanout_requires_headroom(
    graph: &Graph,
    layers: &[Vec<usize>],
    layer_idx: usize,
) -> bool {
    matches!(
        graph.direction,
        crate::graph::Direction::TD | crate::graph::Direction::TB | crate::graph::Direction::BT
    ) && layers.get(layer_idx).is_some_and(|layer| {
        layer.iter().any(|&node_idx| {
            graph
                .nodes
                .get(node_idx)
                .is_some_and(|node| is_dual_junction_anchor(graph, &node.id))
        })
    })
}

/// Mixed Thick/Dotted vertical fan-outs need one branch shaft cell before the
/// target arrow so the edge kind remains visible after the shared junction is
/// lowered. Keep this surcharge limited to a source rank with multiple
/// outgoing edge kinds; ordinary fan-outs retain their compact spacing.
pub(super) fn vertical_mixed_edge_kind_fanout_requires_headroom(
    graph: &Graph,
    layers: &[Vec<usize>],
    layer_idx: usize,
) -> bool {
    matches!(
        graph.direction,
        crate::graph::Direction::TD | crate::graph::Direction::TB | crate::graph::Direction::BT
    ) && layers.get(layer_idx).is_some_and(|layer| {
        layer.iter().any(|&node_idx| {
            let Some(node) = graph.nodes.get(node_idx) else {
                return false;
            };
            let outgoing: Vec<_> = graph
                .edges
                .iter()
                .filter(|edge| !edge.is_back_edge && edge.from == node.id)
                .collect();
            outgoing.len() > 1
                && outgoing.iter().any(|edge| {
                    matches!(
                        edge.kind,
                        crate::graph::EdgeKind::Thick | crate::graph::EdgeKind::Dotted
                    )
                })
        })
    })
}

fn rank_by_id(graph: &Graph, layers: &[Vec<usize>]) -> HashMap<String, usize> {
    layers
        .iter()
        .enumerate()
        .flat_map(|(rank, layer)| {
            layer.iter().filter_map(move |&index| {
                graph.nodes.get(index).map(|node| (node.id.clone(), rank))
            })
        })
        .collect()
}

fn dual_junction_ids(graph: &Graph) -> Vec<String> {
    let mut anchors: Vec<String> = graph
        .nodes
        .iter()
        .filter(|node| is_dual_junction_anchor(graph, &node.id))
        .map(|node| node.id.clone())
        .collect();
    anchors.sort_unstable();
    anchors
}

fn is_dual_junction_anchor(graph: &Graph, node_id: &str) -> bool {
    let incoming: HashSet<&str> = graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge && edge.to == node_id && edge.from != node_id)
        .map(|edge| edge.from.as_str())
        .collect();
    let outgoing: HashSet<&str> = graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge && edge.from == node_id && edge.to != node_id)
        .map(|edge| edge.to.as_str())
        .collect();

    incoming.len() >= 2
        && outgoing.len() >= 2
        // Keep dense crossing grids and shared junction networks in the
        // existing crossing-aware placement path. This pass is for a local
        // merge-then-split seam whose adjacent branches do not themselves
        // participate in another junction.
        && incoming.iter().all(|source_id| {
            graph
                .edges
                .iter()
                .filter(|edge| {
                    !edge.is_back_edge && edge.from == *source_id && edge.to != *source_id
                })
                .count()
                == 1
        })
        && outgoing.iter().all(|target_id| {
            graph
                .edges
                .iter()
                .filter(|edge| {
                    !edge.is_back_edge && edge.to == *target_id && edge.from != *target_id
                })
                .count()
                == 1
        })
}

fn outgoing_targets(graph: &Graph, anchor_id: &str) -> Vec<String> {
    let mut targets: Vec<String> = graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge && edge.from == anchor_id && edge.to != anchor_id)
        .map(|edge| edge.to.clone())
        .collect();
    targets.sort_unstable();
    targets.dedup();
    targets
}

fn secondary_center(rect: &Rect, coords: &OrientedCoords) -> usize {
    match coords.secondary {
        Axis::Horizontal => rect.x + rect.width / 2,
        Axis::Vertical => rect.y + rect.height / 2,
    }
}

fn secondary_midpoint(
    target_ids: &[String],
    node_rects: &HashMap<String, Rect>,
    coords: &OrientedCoords,
) -> Option<usize> {
    let mut centers: Vec<usize> = target_ids
        .iter()
        .filter_map(|id| {
            node_rects
                .get(id)
                .map(|rect| secondary_center(rect, coords))
        })
        .collect();
    if centers.len() < 2 {
        return None;
    }
    centers.sort_unstable();
    Some((centers[0] + centers[centers.len() - 1]) / 2)
}

fn signed_delta(target: usize, current: usize) -> isize {
    if target >= current {
        (target - current) as isize
    } else {
        -((current - target) as isize)
    }
}

fn reverse_context(graph: &Graph, anchor_id: &str) -> Option<Vec<String>> {
    let mut queue = VecDeque::from([anchor_id.to_string()]);
    let mut seen = HashSet::new();
    let mut context = Vec::new();

    while let Some(current_id) = queue.pop_front() {
        if !seen.insert(current_id.clone()) {
            continue;
        }
        if graph.get_node(&current_id).is_none() {
            continue;
        }

        context.push(current_id.clone());
        if context.len() > MAX_CONTEXT_NODES {
            return None;
        }

        let mut next_ids = Vec::new();
        for edge in &graph.edges {
            if edge.is_back_edge || edge.to != current_id || edge.from == current_id {
                continue;
            }
            if graph.edge_crosses_subgraph_boundary(&edge.from, &edge.to) {
                return None;
            }
            if !seen.contains(&edge.from) && graph.get_node(&edge.from).is_some() {
                next_ids.push(edge.from.clone());
            }
        }
        next_ids.sort_unstable();
        next_ids.dedup();
        queue.extend(next_ids);
    }

    (context.len() >= 2).then_some(context)
}

#[derive(Debug, Clone)]
struct ShiftCandidate {
    ids: Vec<String>,
    delta: isize,
}

impl ShiftCandidate {
    fn new(mut ids: Vec<String>, delta: isize) -> Self {
        ids.sort_unstable();
        ids.dedup();
        Self { ids, delta }
    }

    fn is_safe(
        &self,
        graph: &Graph,
        rank_by_id: &HashMap<String, usize>,
        coords: &OrientedCoords,
        positions: &HashMap<String, Point>,
        node_rects: &HashMap<String, Rect>,
    ) -> bool {
        if self.ids.is_empty() || self.delta == 0 {
            return false;
        }

        let moved: HashSet<&str> = self.ids.iter().map(String::as_str).collect();
        if self
            .ids
            .iter()
            .any(|id| graph.get_node(id).is_none() || !positions.contains_key(id))
        {
            return false;
        }

        // A shifted node must not be involved in a declared boundary crossing.
        // The route/envelope stages own those seams and are deliberately not
        // asked to infer a new portal contract from this local pass.
        for edge in graph.edges.iter().filter(|edge| !edge.is_back_edge) {
            if (moved.contains(edge.from.as_str()) || moved.contains(edge.to.as_str()))
                && graph.edge_crosses_subgraph_boundary(&edge.from, &edge.to)
            {
                return false;
            }
        }

        let mut candidate_rects = node_rects.clone();
        for id in &self.ids {
            let Some(rect) = candidate_rects.get_mut(id) else {
                return false;
            };
            let Some(new_secondary) = apply_signed_delta(secondary_start(rect, coords), self.delta)
            else {
                return false;
            };
            set_secondary_start(rect, coords, new_secondary);
        }

        for id in &self.ids {
            let Some(candidate_rect) = candidate_rects.get(id) else {
                return false;
            };
            let Some(rank) = rank_by_id.get(id) else {
                return false;
            };
            for (other_id, other_rect) in &candidate_rects {
                if other_id == id || rank_by_id.get(other_id) != Some(rank) {
                    continue;
                }
                if rectangles_overlap(candidate_rect, other_rect) {
                    return false;
                }
            }
        }

        true
    }

    fn apply(
        &self,
        coords: &OrientedCoords,
        positions: &mut HashMap<String, Point>,
        node_rects: &mut HashMap<String, Rect>,
    ) {
        for id in &self.ids {
            let Some(rect) = node_rects.get_mut(id) else {
                continue;
            };
            let Some(new_secondary) = apply_signed_delta(secondary_start(rect, coords), self.delta)
            else {
                continue;
            };
            set_secondary_start(rect, coords, new_secondary);
            if let Some(point) = positions.get_mut(id) {
                set_secondary_start(point, coords, new_secondary);
            }
        }
    }
}

trait SecondaryStart {
    fn secondary_start(&self, coords: &OrientedCoords) -> usize;
    fn set_secondary_start(&mut self, coords: &OrientedCoords, value: usize);
}

impl SecondaryStart for Rect {
    fn secondary_start(&self, coords: &OrientedCoords) -> usize {
        match coords.secondary {
            Axis::Horizontal => self.x,
            Axis::Vertical => self.y,
        }
    }

    fn set_secondary_start(&mut self, coords: &OrientedCoords, value: usize) {
        match coords.secondary {
            Axis::Horizontal => self.x = value,
            Axis::Vertical => self.y = value,
        }
    }
}

impl SecondaryStart for Point {
    fn secondary_start(&self, coords: &OrientedCoords) -> usize {
        match coords.secondary {
            Axis::Horizontal => self.x,
            Axis::Vertical => self.y,
        }
    }

    fn set_secondary_start(&mut self, coords: &OrientedCoords, value: usize) {
        match coords.secondary {
            Axis::Horizontal => self.x = value,
            Axis::Vertical => self.y = value,
        }
    }
}

fn secondary_start<T: SecondaryStart>(value: &T, coords: &OrientedCoords) -> usize {
    value.secondary_start(coords)
}

fn set_secondary_start<T: SecondaryStart>(value: &mut T, coords: &OrientedCoords, start: usize) {
    value.set_secondary_start(coords, start);
}

fn apply_signed_delta(value: usize, delta: isize) -> Option<usize> {
    if delta >= 0 {
        value.checked_add(delta as usize)
    } else {
        value.checked_sub(delta.unsigned_abs())
    }
}

fn rectangles_overlap(left: &Rect, right: &Rect) -> bool {
    left.x < right.right()
        && right.x < left.right()
        && left.y < right.bottom()
        && right.y < left.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Direction, Edge, EdgeKind, Node, Subgraph};

    fn dual_graph(direction: Direction) -> Graph {
        let mut graph = Graph::new();
        graph.direction = direction;
        for id in ["A", "B", "C", "D", "E"] {
            graph.add_node(Node::new(id, id));
        }
        for (from, to) in [("A", "C"), ("B", "C"), ("C", "D"), ("C", "E")] {
            graph.add_edge(Edge::new(from, to));
        }
        graph
    }

    fn test_rects(direction: Direction) -> HashMap<String, Rect> {
        let coords = OrientedCoords::new(direction);
        let mut rects = HashMap::new();
        let entries = [("A", 0), ("B", 13), ("C", 5), ("D", 5), ("E", 21)];
        for (id, secondary) in entries {
            let mut rect = Rect::new(0, 0, 5, 3);
            match coords.primary {
                Axis::Horizontal => {
                    rect.x = [0, 12, 24, 36, 36][["A", "B", "C", "D", "E"]
                        .iter()
                        .position(|candidate| *candidate == id)
                        .expect("test node")]
                }
                Axis::Vertical => {
                    rect.y = [0, 6, 12, 18, 18][["A", "B", "C", "D", "E"]
                        .iter()
                        .position(|candidate| *candidate == id)
                        .expect("test node")]
                }
            }
            match coords.secondary {
                Axis::Horizontal => rect.x = secondary,
                Axis::Vertical => rect.y = secondary,
            }
            rects.insert(id.to_string(), rect);
        }
        rects
    }

    fn layers(_graph: &Graph) -> Vec<Vec<usize>> {
        vec![vec![0, 1], vec![2], vec![3, 4]]
    }

    #[test]
    fn mixed_edge_kind_headroom_is_vertical_and_source_rank_gated() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;
        for id in ["A", "B", "C", "D"] {
            graph.add_node(Node::new(id, id));
        }
        graph.add_edge(Edge::new("A", "B"));
        let mut thick = Edge::new("A", "C");
        thick.kind = EdgeKind::Thick;
        graph.add_edge(thick);
        graph.add_edge(Edge::new("A", "D"));
        let layer_map = vec![vec![0], vec![1, 2, 3]];

        assert!(vertical_mixed_edge_kind_fanout_requires_headroom(
            &graph, &layer_map, 0
        ));
        assert!(!vertical_mixed_edge_kind_fanout_requires_headroom(
            &graph, &layer_map, 1
        ));

        graph.direction = Direction::LR;
        assert!(!vertical_mixed_edge_kind_fanout_requires_headroom(
            &graph, &layer_map, 0
        ));

        graph.direction = Direction::TD;
        graph.edges[1].kind = EdgeKind::Arrow;
        assert!(!vertical_mixed_edge_kind_fanout_requires_headroom(
            &graph, &layer_map, 0
        ));
    }

    fn points(rects: &HashMap<String, Rect>) -> HashMap<String, Point> {
        rects
            .iter()
            .map(|(id, rect)| (id.clone(), Point::new(rect.x, rect.y)))
            .collect()
    }

    #[test]
    fn dual_junctions_center_outgoing_targets_in_all_directions() {
        for direction in [Direction::TD, Direction::BT, Direction::LR, Direction::RL] {
            let graph = dual_graph(direction);
            let coords = OrientedCoords::new(direction);
            let layer_map = layers(&graph);
            assert_eq!(
                vertical_fanout_requires_headroom(&graph, &layer_map, 1),
                matches!(direction, Direction::TD | Direction::BT),
                "vertical dual-junction headroom policy for {direction:?}"
            );
            let mut rects = test_rects(direction);
            let mut positions = points(&rects);
            balance_dual_junctions(&graph, &layer_map, &coords, &mut positions, &mut rects);

            let anchor = rects["C"];
            let target_midpoint = (secondary_center(&rects["D"], &coords)
                + secondary_center(&rects["E"], &coords))
                / 2;
            assert!(
                secondary_center(&anchor, &coords).abs_diff(target_midpoint) <= MIDPOINT_TOLERANCE,
                "{direction:?}: anchor={anchor:?} D={:?} E={:?}",
                rects["D"],
                rects["E"]
            );
            assert_eq!(
                secondary_start(&rects["A"], &coords),
                secondary_start(&test_rects(direction)["A"], &coords) + 8
            );
            assert_eq!(
                secondary_start(&rects["B"], &coords),
                secondary_start(&test_rects(direction)["B"], &coords) + 8
            );
        }
    }

    #[test]
    fn anchor_and_relations_are_stable_when_edges_are_reordered() {
        let first = dual_graph(Direction::TD);
        let mut second = dual_graph(Direction::TD);
        second.edges.reverse();

        let run = |graph: &Graph| {
            let coords = OrientedCoords::new(graph.direction);
            let layer_map = layers(graph);
            let mut rects = test_rects(graph.direction);
            let mut positions = points(&rects);
            balance_dual_junctions(graph, &layer_map, &coords, &mut positions, &mut rects);
            rects
        };

        assert_eq!(run(&first), run(&second));
    }

    #[test]
    fn boundary_crossing_refuses_dual_junction_movement() {
        let mut graph = dual_graph(Direction::TD);
        let mut subgraph = Subgraph::new("group", Some("Group".to_string()));
        for id in ["C", "D", "E"] {
            subgraph.add_node(id);
        }
        graph.add_subgraph(subgraph);
        for id in ["C", "D", "E"] {
            graph.associate_node_with_subgraph(id, "group");
        }

        let coords = OrientedCoords::new(graph.direction);
        let layer_map = layers(&graph);
        let mut rects = test_rects(graph.direction);
        let before = rects.clone();
        let mut positions = points(&rects);
        balance_dual_junctions(&graph, &layer_map, &coords, &mut positions, &mut rects);
        assert_eq!(rects, before);
    }

    #[test]
    fn collision_refuses_context_and_branch_alternatives() {
        let mut graph = dual_graph(Direction::TD);
        graph.add_node(Node::new("X", "X"));
        let coords = OrientedCoords::new(graph.direction);
        let mut layer_map = layers(&graph);
        layer_map[0].push(5);
        let mut rects = test_rects(graph.direction);
        rects.insert("X".to_string(), Rect::new(8, 0, 5, 3));
        let before = rects.clone();
        let mut positions = points(&rects);
        balance_dual_junctions(&graph, &layer_map, &coords, &mut positions, &mut rects);
        assert_eq!(rects, before);
    }

    #[test]
    fn safe_branch_alternative_is_used_when_context_collides() {
        let mut graph = dual_graph(Direction::TD);
        graph.add_node(Node::new("X", "X"));
        let coords = OrientedCoords::new(graph.direction);
        let mut layer_map = layers(&graph);
        layer_map[0].push(5);
        let mut rects = test_rects(graph.direction);
        rects.get_mut("C").expect("C rect").x = 15;
        rects.get_mut("D").expect("D rect").x = 20;
        rects.get_mut("E").expect("E rect").x = 40;
        rects.insert("X".to_string(), Rect::new(15, 0, 5, 3));
        let before_anchor = rects["C"];
        let mut positions = points(&rects);

        balance_dual_junctions(&graph, &layer_map, &coords, &mut positions, &mut rects);

        assert_eq!(rects["C"], before_anchor);
        assert_eq!(rects["D"].x, 5);
        assert_eq!(rects["E"].x, 25);
    }

    #[test]
    fn oversized_reverse_context_refuses_movement() {
        let mut graph = dual_graph(Direction::TD);
        for index in 0..15 {
            graph.add_node(Node::new(format!("N{index}"), format!("N{index}")));
            let next = if index == 14 {
                "A".to_string()
            } else {
                format!("N{}", index + 1)
            };
            graph.add_edge(Edge::new(format!("N{index}"), next));
        }
        let coords = OrientedCoords::new(graph.direction);
        let layer_map = layers(&graph);
        let mut rects = test_rects(graph.direction);
        let before = rects.clone();
        let mut positions = points(&rects);
        balance_dual_junctions(&graph, &layer_map, &coords, &mut positions, &mut rects);
        assert_eq!(rects, before);
    }

    #[test]
    fn pure_fan_in_and_pure_fan_out_are_unchanged() {
        for shape in ["fanin", "fanout"] {
            let mut graph = Graph::new();
            graph.direction = Direction::TD;
            for id in ["A", "B", "C", "D", "E"] {
                graph.add_node(Node::new(id, id));
            }
            let edges = if shape == "fanin" {
                [("A", "C"), ("B", "C")].as_slice()
            } else {
                [("C", "D"), ("C", "E")].as_slice()
            };
            for &(from, to) in edges {
                graph.add_edge(Edge::new(from, to));
            }
            let coords = OrientedCoords::new(graph.direction);
            let layer_map = layers(&graph);
            let mut rects = test_rects(graph.direction);
            let before = rects.clone();
            let mut positions = points(&rects);
            balance_dual_junctions(&graph, &layer_map, &coords, &mut positions, &mut rects);
            assert_eq!(rects, before, "shape={shape}");
        }
    }
}
