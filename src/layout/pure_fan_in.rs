//! Conservative default balancing for terminal pure fan-in targets.
//!
//! A pure fan-in target has no outgoing flow of its own, so moving only that
//! target toward the span of its direct sources does not require a broader
//! component relocation. This pass is intentionally separate from the
//! critic-driven repair loop and from dual-junction balancing: it owns one
//! small, semantic layout invariant and rejects shapes that belong to portal
//! or crossing-aware layout code.

use std::collections::HashMap;

use crate::geom::{Point, Rect};
use crate::graph::Graph;
use crate::orientation::{Axis, OrientedCoords};

const MIDPOINT_TOLERANCE: usize = 1;

/// Center eligible pure fan-in targets after the ordinary balance sweeps.
///
/// The target is the only node that may move. A candidate is accepted only
/// when all direct sources occupy the immediately preceding rank, no edge
/// crosses a declared subgraph boundary, and the target remains disjoint from
/// every other node in its rank. Stable IDs make repeated renders independent
/// of graph insertion order.
pub(super) fn balance_pure_fan_in_targets(
    graph: &Graph,
    layers: &[Vec<usize>],
    coords: &OrientedCoords,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
) {
    let rank_by_id = rank_by_id(graph, layers);
    let mut target_ids: Vec<String> = graph.nodes.iter().map(|node| node.id.clone()).collect();
    target_ids.sort_unstable();

    for target_id in target_ids {
        let Some(target_rank) = rank_by_id.get(&target_id).copied() else {
            continue;
        };
        if target_rank == 0 || has_outgoing_flow(graph, &target_id) {
            continue;
        }

        let sources = direct_sources(graph, &target_id);
        if sources.len() < 2
            || !sources
                .iter()
                .all(|source_id| rank_by_id.get(source_id).copied() == Some(target_rank - 1))
            || crosses_declared_boundary(graph, &sources, &target_id)
        {
            continue;
        }

        let Some(target_rect) = node_rects.get(&target_id).copied() else {
            continue;
        };
        let Some(midpoint) = secondary_midpoint(&sources, node_rects, coords) else {
            continue;
        };
        let current_center = secondary_center(&target_rect, coords);
        if current_center.abs_diff(midpoint) <= MIDPOINT_TOLERANCE {
            continue;
        }

        let desired_start = midpoint.saturating_sub(secondary_extent(&target_rect, coords) / 2);
        let current_start = secondary_start(&target_rect, coords);
        let delta = signed_delta(desired_start, current_start);
        let Some(candidate_start) = apply_signed_delta(current_start, delta) else {
            continue;
        };

        let mut candidate_rect = target_rect;
        set_secondary_start(&mut candidate_rect, coords, candidate_start);
        if secondary_center(&candidate_rect, coords).abs_diff(midpoint) > MIDPOINT_TOLERANCE
            || overlaps_same_rank(
                &target_id,
                target_rank,
                &candidate_rect,
                &rank_by_id,
                node_rects,
            )
        {
            continue;
        }

        let Some(point) = positions.get_mut(&target_id) else {
            continue;
        };
        set_secondary_start(point, coords, candidate_start);
        node_rects.insert(target_id, candidate_rect);
    }
}

fn rank_by_id(graph: &Graph, layers: &[Vec<usize>]) -> HashMap<String, usize> {
    layers
        .iter()
        .enumerate()
        .flat_map(|(rank, layer)| {
            layer.iter().filter_map(move |index| {
                graph.nodes.get(*index).map(|node| (node.id.clone(), rank))
            })
        })
        .collect()
}

fn direct_sources(graph: &Graph, target_id: &str) -> Vec<String> {
    let mut sources: Vec<String> = graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge && edge.to == target_id && edge.from != target_id)
        .map(|edge| edge.from.clone())
        .filter(|source_id| graph.get_node(source_id).is_some())
        .collect();
    sources.sort_unstable();
    sources.dedup();
    sources
}

fn has_outgoing_flow(graph: &Graph, source_id: &str) -> bool {
    graph
        .edges
        .iter()
        .any(|edge| !edge.is_back_edge && edge.from == source_id && edge.to != source_id)
}

fn crosses_declared_boundary(graph: &Graph, sources: &[String], target_id: &str) -> bool {
    sources
        .iter()
        .any(|source_id| graph.edge_crosses_subgraph_boundary(source_id, target_id))
}

fn secondary_midpoint(
    source_ids: &[String],
    node_rects: &HashMap<String, Rect>,
    coords: &OrientedCoords,
) -> Option<usize> {
    let mut centers: Vec<usize> = source_ids
        .iter()
        .filter_map(|source_id| {
            node_rects
                .get(source_id)
                .map(|rect| secondary_center(rect, coords))
        })
        .collect();
    if centers.len() != source_ids.len() || centers.len() < 2 {
        return None;
    }
    centers.sort_unstable();
    Some((centers[0] + centers[centers.len() - 1]) / 2)
}

fn overlaps_same_rank(
    target_id: &str,
    target_rank: usize,
    candidate_rect: &Rect,
    rank_by_id: &HashMap<String, usize>,
    node_rects: &HashMap<String, Rect>,
) -> bool {
    node_rects.iter().any(|(other_id, other_rect)| {
        other_id != target_id
            && rank_by_id.get(other_id).copied() == Some(target_rank)
            && rectangles_overlap(candidate_rect, other_rect)
    })
}

fn secondary_extent(rect: &Rect, coords: &OrientedCoords) -> usize {
    match coords.secondary {
        Axis::Horizontal => rect.width,
        Axis::Vertical => rect.height,
    }
}

fn secondary_center(rect: &Rect, coords: &OrientedCoords) -> usize {
    secondary_start(rect, coords) + secondary_extent(rect, coords) / 2
}

fn secondary_start<T: SecondaryStart>(value: &T, coords: &OrientedCoords) -> usize {
    value.secondary_start(coords)
}

fn set_secondary_start<T: SecondaryStart>(
    value: &mut T,
    coords: &OrientedCoords,
    secondary: usize,
) {
    value.set_secondary_start(coords, secondary);
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

fn signed_delta(target: usize, current: usize) -> isize {
    if target >= current {
        (target - current) as isize
    } else {
        -((current - target) as isize)
    }
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
    use crate::graph::{Direction, Edge, Node, Subgraph};

    type FanInFixture = (
        Graph,
        Vec<Vec<usize>>,
        OrientedCoords,
        HashMap<String, Point>,
        HashMap<String, Rect>,
    );

    fn fan_in_fixture(direction: Direction) -> FanInFixture {
        let mut graph = Graph::new();
        graph.direction = direction;
        for id in ["S1", "S2", "S3", "END"] {
            graph.add_node(Node::new(id, id));
        }
        for source in ["S1", "S2", "S3"] {
            graph.add_edge(Edge::new(source, "END"));
        }

        let coords = OrientedCoords::new(direction);
        let layers = vec![vec![0, 1, 2], vec![3]];
        let secondary_starts = [("S1", 0), ("S2", 13), ("S3", 26), ("END", 35)];
        let mut positions = HashMap::new();
        let mut node_rects = HashMap::new();
        for (index, (id, secondary)) in secondary_starts.iter().enumerate() {
            let primary = if index < 3 { 0 } else { 8 };
            let mut rect = Rect::new(0, 0, 5, 3);
            match coords.primary {
                Axis::Horizontal => rect.x = primary,
                Axis::Vertical => rect.y = primary,
            }
            set_secondary_start(&mut rect, &coords, *secondary);
            positions.insert((*id).to_string(), Point::new(rect.x, rect.y));
            node_rects.insert((*id).to_string(), rect);
        }
        (graph, layers, coords, positions, node_rects)
    }

    #[test]
    fn centers_pure_fan_in_targets_in_all_directions() {
        for direction in [Direction::TD, Direction::BT, Direction::LR, Direction::RL] {
            let (graph, layers, coords, mut positions, mut rects) = fan_in_fixture(direction);
            balance_pure_fan_in_targets(&graph, &layers, &coords, &mut positions, &mut rects);

            let source_midpoint = (secondary_center(&rects["S1"], &coords)
                + secondary_center(&rects["S3"], &coords))
                / 2;
            assert!(
                secondary_center(&rects["END"], &coords).abs_diff(source_midpoint)
                    <= MIDPOINT_TOLERANCE,
                "{direction:?}: source_midpoint={source_midpoint} end={:?}",
                rects["END"]
            );
            assert_eq!(positions["END"], Point::new(rects["END"].x, rects["END"].y));
        }
    }

    #[test]
    fn edge_order_does_not_change_pure_fan_in_result() {
        let (graph, layers, coords, mut first_positions, mut first_rects) =
            fan_in_fixture(Direction::TD);
        let mut reversed = graph.clone();
        reversed.edges.reverse();
        let mut second_positions = first_positions.clone();
        let mut second_rects = first_rects.clone();

        balance_pure_fan_in_targets(
            &graph,
            &layers,
            &coords,
            &mut first_positions,
            &mut first_rects,
        );
        balance_pure_fan_in_targets(
            &reversed,
            &layers,
            &coords,
            &mut second_positions,
            &mut second_rects,
        );

        assert_eq!(first_positions, second_positions);
        assert_eq!(first_rects, second_rects);
    }

    #[test]
    fn rejects_same_rank_collision() {
        let (mut graph, mut layers, coords, mut positions, mut rects) =
            fan_in_fixture(Direction::TD);
        graph.add_node(Node::new("Peer", "Peer"));
        layers[1].push(4);
        let mut peer = Rect::new(0, 0, 5, 3);
        peer.x = 13;
        peer.y = 8;
        rects.insert("Peer".to_string(), peer);
        positions.insert("Peer".to_string(), Point::new(peer.x, peer.y));

        let before = rects["END"];
        balance_pure_fan_in_targets(&graph, &layers, &coords, &mut positions, &mut rects);
        assert_eq!(rects["END"], before);
    }

    #[test]
    fn rejects_declared_boundary_crossing() {
        let (mut graph, layers, coords, mut positions, mut rects) = fan_in_fixture(Direction::TD);
        let mut subgraph = Subgraph::new("group", Some("Group".to_string()));
        subgraph.add_node("S1");
        graph.add_subgraph(subgraph);
        graph.associate_node_with_subgraph("S1", "group");

        let before = rects["END"];
        balance_pure_fan_in_targets(&graph, &layers, &coords, &mut positions, &mut rects);
        assert_eq!(rects["END"], before);
    }

    #[test]
    fn leaves_fan_in_target_with_outgoing_flow_unchanged() {
        let (mut graph, layers, coords, mut positions, mut rects) = fan_in_fixture(Direction::TD);
        graph.add_node(Node::new("Tail", "Tail"));
        graph.add_edge(Edge::new("END", "Tail"));
        let before = rects["END"];
        balance_pure_fan_in_targets(&graph, &layers, &coords, &mut positions, &mut rects);
        assert_eq!(rects["END"], before);
    }

    #[test]
    fn source_set_is_unique_and_back_edges_are_ignored() {
        let (mut graph, layers, coords, mut positions, mut rects) = fan_in_fixture(Direction::TD);
        graph.add_edge(Edge::new("S1", "END"));
        let mut back_edge = Edge::new("END", "S1");
        back_edge.is_back_edge = true;
        graph.add_edge(back_edge);
        balance_pure_fan_in_targets(&graph, &layers, &coords, &mut positions, &mut rects);
        let midpoint =
            (secondary_center(&rects["S1"], &coords) + secondary_center(&rects["S3"], &coords)) / 2;
        assert!(secondary_center(&rects["END"], &coords).abs_diff(midpoint) <= 1);
    }
}
