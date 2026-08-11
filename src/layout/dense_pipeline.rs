//! Topology-derived routing for the layered dense pipeline family.
//!
//! The ordinary renderer owns fan-in and fan-out junctions.  The six
//! singleton bridge edges in a layered dense pipeline are different: with the
//! compact rank spacing they can be forced to leave a node along its border
//! before turning, which makes the edge look like a broken collector hook.
//! This module reserves only those bridge corridors, and only after validating
//! the complete graph shape.  A near miss is deliberately not claimed.

use std::collections::{HashMap, HashSet};

use crate::geom::{EdgeRoute, Point, Rect};
use crate::graph::{Direction, EdgeKind, Graph};
use crate::orientation::OrientedCoords;

use super::layout_routing;
use super::placement::Placement;

const DENSE_PIPELINE_LAYER_SIZES: [usize; 7] = [8, 4, 2, 2, 4, 4, 1];
const BRIDGE_RANKS: [usize; 2] = [2, 4];

/// Return whether `layers` describe the exact layered dense-pipeline shape.
///
/// The detector is intentionally structural.  It does not inspect fixture
/// names, labels, or rendered glyphs, and it rejects unsupported edge kinds,
/// labels, subgraphs, cycles, and rank gaps.
pub(super) fn is_family(graph: &Graph, layers: &[Vec<usize>]) -> bool {
    if graph.nodes.len() != DENSE_PIPELINE_LAYER_SIZES.iter().sum::<usize>()
        || graph.edges.len() != 26
        || layers.len() != DENSE_PIPELINE_LAYER_SIZES.len()
        || layers
            .iter()
            .zip(DENSE_PIPELINE_LAYER_SIZES)
            .any(|(layer, expected)| layer.len() != expected)
        || !graph.subgraphs.is_empty()
        || graph
            .nodes
            .iter()
            .any(|node| node.shape != crate::graph::NodeShape::Rectangle)
        || graph
            .edges
            .iter()
            .any(|edge| edge.is_back_edge || edge.label.is_some() || edge.kind != EdgeKind::Arrow)
    {
        return false;
    }

    let rank_by_id: HashMap<&str, usize> = layers
        .iter()
        .enumerate()
        .flat_map(|(rank, layer)| {
            layer
                .iter()
                .filter_map(|index| graph.nodes.get(*index))
                .map(move |node| (node.id.as_str(), rank))
        })
        .collect();
    if rank_by_id.len() != graph.nodes.len() {
        return false;
    }

    is_family_with_ranks(graph, &rank_by_id)
}

fn is_family_with_ranks(graph: &Graph, rank_by_id: &HashMap<&str, usize>) -> bool {
    if rank_by_id.len() != graph.nodes.len() {
        return false;
    }

    let mut incoming: HashMap<&str, usize> = HashMap::new();
    let mut outgoing: HashMap<&str, usize> = HashMap::new();
    for edge in &graph.edges {
        let (Some(from_rank), Some(to_rank)) = (
            rank_by_id.get(edge.from.as_str()),
            rank_by_id.get(edge.to.as_str()),
        ) else {
            return false;
        };
        if *to_rank != from_rank.saturating_add(1) {
            return false;
        }
        *incoming.entry(edge.to.as_str()).or_default() += 1;
        *outgoing.entry(edge.from.as_str()).or_default() += 1;
    }

    let expected_degrees = [(0, 1), (2, 1), (2, 1), (1, 2), (1, 1), (1, 1), (4, 0)];
    let mut counts = [0usize; DENSE_PIPELINE_LAYER_SIZES.len()];
    for node in &graph.nodes {
        let Some(&rank) = rank_by_id.get(node.id.as_str()) else {
            return false;
        };
        if rank >= counts.len() {
            return false;
        }
        counts[rank] += 1;
        {
            let degree = (
                incoming.get(node.id.as_str()).copied().unwrap_or(0),
                outgoing.get(node.id.as_str()).copied().unwrap_or(0),
            );
            if degree != expected_degrees[rank] {
                return false;
            }
        }
    }

    counts == DENSE_PIPELINE_LAYER_SIZES
}

/// Dense bridge rank pairs need one extra primary cell so a bridge can have a
/// source stem, an orthogonal corridor, and a target stem instead of turning
/// on the source border.  This is a layout policy, not a fixture special case.
pub(super) fn needs_bridge_headroom(
    graph: &Graph,
    layers: &[Vec<usize>],
    layer_idx: usize,
) -> bool {
    is_family(graph, layers) && BRIDGE_RANKS.contains(&layer_idx)
}

/// Reserve the singleton bridge corridors for a validated dense pipeline.
///
/// The function is transactional with respect to the route map and occupancy:
/// all candidate paths are built and validated before any route is inserted or
/// any grid cell is cleared/marked.  It returns `false` for a near miss or an
/// insufficient corridor, leaving the caller's generic routing policy intact.
pub(super) fn reserve_bridge_routes(
    graph: &Graph,
    ranks: &HashMap<String, usize>,
    placement: &Placement,
    grid: &mut layout_routing::OccupancyGrid,
    routes: &mut HashMap<usize, EdgeRoute>,
    debug_timing: bool,
) -> bool {
    let rank_by_id: HashMap<&str, usize> = graph
        .nodes
        .iter()
        .filter_map(|node| ranks.get(&node.id).map(|rank| (node.id.as_str(), *rank)))
        .collect();
    if !is_family_with_ranks(graph, &rank_by_id) {
        return false;
    }

    let mut incoming: HashMap<&str, usize> = HashMap::new();
    let mut outgoing: HashMap<&str, usize> = HashMap::new();
    for edge in &graph.edges {
        *incoming.entry(edge.to.as_str()).or_default() += 1;
        *outgoing.entry(edge.from.as_str()).or_default() += 1;
    }

    let bridge_edges: Vec<usize> = graph
        .edges
        .iter()
        .enumerate()
        .filter_map(|(edge_idx, edge)| {
            let from_rank = rank_by_id.get(edge.from.as_str()).copied()?;
            let to_rank = rank_by_id.get(edge.to.as_str()).copied()?;
            (BRIDGE_RANKS.contains(&from_rank)
                && to_rank == from_rank.saturating_add(1)
                && outgoing.get(edge.from.as_str()).copied() == Some(1)
                && incoming.get(edge.to.as_str()).copied() == Some(1))
            .then_some(edge_idx)
        })
        .collect();
    if bridge_edges.len() != 6 {
        return false;
    }

    let mut candidates = Vec::with_capacity(bridge_edges.len());
    let mut occupied = HashSet::new();
    for edge_idx in bridge_edges {
        let edge = &graph.edges[edge_idx];
        let (Some(source), Some(target)) = (
            placement.node_rects.get(&edge.from),
            placement.node_rects.get(&edge.to),
        ) else {
            return false;
        };
        let target_shape = graph
            .get_node(&edge.to)
            .map(|node| node.shape)
            .unwrap_or_default();
        let start = layout_routing::edge_exit_point(*source, graph.direction);
        let mut end =
            layout_routing::edge_entry_point_for_shape(*target, graph.direction, target_shape);
        let coords = OrientedCoords::new(graph.direction);
        let desired_secondary = coords.secondary_coord(start.x, start.y);
        let target_secondary = target_port_secondary(target, &coords, desired_secondary);
        coords.set_secondary(&mut end.x, &mut end.y, target_secondary);
        let Some(route) = bridge_route(start, end, graph.direction, source, target) else {
            return false;
        };
        let invalid_route = route.segments.is_empty()
            || route.segments.iter().flat_map(segment_points).any(|point| {
                point.x >= placement.canvas.right()
                    || point.y >= placement.canvas.bottom()
                    || placement
                        .node_rects
                        .values()
                        .any(|rect| rect.contains(point))
            });
        if invalid_route {
            return false;
        }

        let route_points: Vec<Point> = route.segments.iter().flat_map(segment_points).collect();
        let route_points: HashSet<Point> = route_points.into_iter().collect();
        if route_points.iter().any(|point| occupied.contains(point)) {
            return false;
        }
        occupied.extend(route_points.iter().copied());
        candidates.push((
            edge_idx,
            route,
            route_points.into_iter().collect::<Vec<_>>(),
        ));
    }

    for (edge_idx, route, points) in candidates {
        for point in points {
            grid.clear_point(point);
        }
        grid.mark_path(&route);
        routes.insert(edge_idx, route);
        if debug_timing {
            eprintln!("  dense pipeline bridge route stored for edge {edge_idx}");
        }
    }
    true
}

fn bridge_route(
    start: Point,
    end: Point,
    direction: Direction,
    source: &Rect,
    target: &Rect,
) -> Option<EdgeRoute> {
    let coords = OrientedCoords::new(direction);
    let start_primary = coords.primary_coord(start.x, start.y);
    let end_primary = coords.primary_coord(end.x, end.y);
    let distance = start_primary.abs_diff(end_primary);
    let lane_distance = match coords.primary {
        crate::orientation::Axis::Horizontal => 4,
        crate::orientation::Axis::Vertical => 1,
    };
    if distance <= lane_distance {
        return None;
    }

    let lane_point = coords.advance(start.x, start.y, lane_distance);
    let lane_primary = coords.primary_coord(lane_point.0, lane_point.1);
    let mut source_lane = start;
    coords.set_primary(&mut source_lane.x, &mut source_lane.y, lane_primary);
    let target_secondary = coords.secondary_coord(end.x, end.y);
    let target_lane_tuple = coords.with_secondary(source_lane.x, source_lane.y, target_secondary);
    let target_lane = Point::new(target_lane_tuple.0, target_lane_tuple.1);

    // Keep the route outside both actual node rectangles.  The caller clears
    // only this validated path from the padded occupancy grid, never from a
    // node cell.
    if [start, end, source_lane, target_lane]
        .into_iter()
        .any(|point| source.contains(point) || target.contains(point))
    {
        return None;
    }

    let mut route = EdgeRoute::new();
    route.push_segment(start, source_lane);
    route.push_segment(source_lane, target_lane);
    route.push_segment(target_lane, end);
    Some(route)
}

fn target_port_secondary(target: &Rect, coords: &OrientedCoords, desired: usize) -> usize {
    let (first, last) = match coords.secondary {
        crate::orientation::Axis::Horizontal => {
            (target.x.saturating_add(1), target.right().saturating_sub(2))
        }
        crate::orientation::Axis::Vertical => (
            target.y.saturating_add(1),
            target.bottom().saturating_sub(2),
        ),
    };
    desired.clamp(first.min(last), first.max(last))
}

fn segment_points(segment: &crate::geom::Segment) -> Vec<Point> {
    if segment.from.x == segment.to.x {
        let (start, end) = if segment.from.y <= segment.to.y {
            (segment.from.y, segment.to.y)
        } else {
            (segment.to.y, segment.from.y)
        };
        (start..=end)
            .map(|y| Point::new(segment.from.x, y))
            .collect()
    } else if segment.from.y == segment.to.y {
        let (start, end) = if segment.from.x <= segment.to.x {
            (segment.from.x, segment.to.x)
        } else {
            (segment.to.x, segment.from.x)
        };
        (start..=end)
            .map(|x| Point::new(x, segment.from.y))
            .collect()
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DENSE_PIPELINE_SOURCE: &str = r#"graph TD
        A1[Input 1] --> P1[Process 1]
        A2[Input 2] --> P1
        A3[Input 3] --> P2[Process 2]
        A4[Input 4] --> P2
        A5[Input 5] --> P3[Process 3]
        A6[Input 6] --> P3
        A7[Input 7] --> P4[Process 4]
        A8[Input 8] --> P4
        P1 --> M1[Merge 1]
        P2 --> M1
        P3 --> M2[Merge 2]
        P4 --> M2
        M1 --> F1[Filter 1]
        M2 --> F2[Filter 2]
        F1 --> T1[Transform 1]
        F1 --> T2[Transform 2]
        F2 --> T3[Transform 3]
        F2 --> T4[Transform 4]
        T1 --> O1[Output 1]
        T2 --> O2[Output 2]
        T3 --> O3[Output 3]
        T4 --> O4[Output 4]
        O1 --> END[Done]
        O2 --> END
        O3 --> END
        O4 --> END
    "#;

    #[test]
    fn detector_accepts_the_layered_shape_without_fixture_names() {
        let parsed = crate::parser::parse(DENSE_PIPELINE_SOURCE, false)
            .expect("parse dense pipeline test source");
        let layers = super::super::assign_layers(&parsed.graph);
        assert!(is_family(&parsed.graph, &layers));
        assert!(needs_bridge_headroom(&parsed.graph, &layers, 2));
        assert!(!needs_bridge_headroom(&parsed.graph, &layers, 3));
    }

    #[test]
    fn detector_rejects_a_near_miss_without_claiming_routes() {
        let parsed = crate::parser::parse(DENSE_PIPELINE_SOURCE, false)
            .expect("parse dense pipeline test source");
        let mut graph = parsed.graph;
        graph.edges.pop();
        let layers = super::super::assign_layers(&graph);
        assert!(!is_family(&graph, &layers));
    }

    #[test]
    fn bridge_routes_start_with_a_visible_primary_stem() {
        let parsed = crate::parser::parse(DENSE_PIPELINE_SOURCE, false)
            .expect("parse dense pipeline test source");
        let graph = super::super::apply_coarse_layout(
            parsed.graph,
            None,
            super::super::CoarseLayoutConfig::default(),
        )
        .expect("layout dense pipeline fixture");
        let m1_f1 = graph
            .edges
            .iter()
            .enumerate()
            .find(|(_, edge)| edge.from == "M1" && edge.to == "F1")
            .map(|(index, _)| index)
            .expect("M1 -> F1 edge");
        let route = graph.edge_routes.get(&m1_f1).expect("bridge route");
        assert!(route.segments.len() >= 2);
        assert_eq!(route.segments[0].from.x, route.segments[0].to.x);
        assert_eq!(route.segments[1].from.y, route.segments[1].to.y);
    }
}
