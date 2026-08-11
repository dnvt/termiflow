//! Explicit routing for small dense bipartite rank pairs.
//!
//! The ordinary fan-in/fan-out passes intentionally share a collector bar.
//! That is compact and useful for ordinary diagrams, but a 3x3 crossing grid
//! then loses endpoint identity: two declared edges can share the same raw
//! corridor and only one arrowhead survives at the target.  This scene lowers
//! the whole rank pair as one deterministic reservation instead.  Each edge
//! gets a distinct source/target port and a distinct primary-axis lane;
//! perpendicular intersections are left to Canvas's explicit-crossing marker.
//! The topology match is deliberately narrow and never uses fixture names.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::graph::{Direction, EdgeKind, Graph, Node, NodeShape};
use crate::orientation::OrientedCoords;
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::edge_primitives::{
    draw_line_primary, draw_line_secondary, edge_entry_point, edge_exit_point,
};
use super::{set_route_char, set_route_edge_char, RouteOwner};

const DENSE_SIDE: usize = 3;
const DENSE_EDGES: usize = 6;

#[derive(Debug, Clone)]
struct DensePair {
    edge_ids: Vec<usize>,
    source_ids: Vec<String>,
    target_ids: Vec<String>,
}

/// Plan all eligible dense rank-pair scenes before generic fan-in/fan-out
/// lowering.  A rejected match is not claimed, so the existing generic path
/// remains the conservative fallback for unsupported geometry.
pub(crate) fn plan_dense_crossing_scenes(
    graph: &Graph,
    canvas: &mut Canvas,
    style: &StyleChars,
) -> HashSet<usize> {
    let mut planned = HashSet::new();
    let pairs = detect_pairs(graph);
    if !pairs.is_empty() {
        // The scene intentionally reserves perpendicular crossings as
        // pass-throughs. Enable their explicit marker before any route cell
        // is written; the pipeline-level policy is installed later as well,
        // but cannot retroactively recover an already-resolved overlap.
        canvas.set_explicit_crossings_enabled(true);
    }
    for pair in pairs {
        if pair
            .edge_ids
            .iter()
            .any(|edge_id| planned.contains(edge_id))
        {
            continue;
        }
        if lower_pair(&pair, graph, canvas, style) {
            planned.extend(pair.edge_ids);
        }
    }
    planned
}

fn detect_pairs(graph: &Graph) -> Vec<DensePair> {
    if !eligible_graph(graph) {
        return Vec::new();
    }

    let mut by_rank: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (edge_id, edge) in graph.edges.iter().enumerate() {
        let (Some(source), Some(target)) = (graph.get_node(&edge.from), graph.get_node(&edge.to))
        else {
            continue;
        };
        let (low, high) = if source.rank < target.rank {
            (source.rank, target.rank)
        } else if target.rank < source.rank {
            (target.rank, source.rank)
        } else {
            continue;
        };
        if high == low.saturating_add(1) {
            by_rank.entry((low, high)).or_default().push(edge_id);
        }
    }

    let mut pairs = Vec::new();
    for edge_ids in by_rank.into_values() {
        let mut source_ids = BTreeSet::new();
        let mut target_ids = BTreeSet::new();
        for &edge_id in &edge_ids {
            let edge = &graph.edges[edge_id];
            let Some(source) = graph.get_node(&edge.from) else {
                continue;
            };
            let Some(target) = graph.get_node(&edge.to) else {
                continue;
            };
            if source.rank < target.rank {
                source_ids.insert(source.id.clone());
                target_ids.insert(target.id.clone());
            } else {
                source_ids.insert(target.id.clone());
                target_ids.insert(source.id.clone());
            }
        }
        if source_ids.len() != DENSE_SIDE
            || target_ids.len() != DENSE_SIDE
            || edge_ids.len() != DENSE_EDGES
        {
            continue;
        }

        // The graph rank identifies the logical adjacent layers.  The edge
        // direction still determines which side is upstream after BT/RL
        // normalization, so validate the exact 2-regular relation below.
        let sources: Vec<String> = source_ids.into_iter().collect();
        let targets: Vec<String> = target_ids.into_iter().collect();
        let mut seen_pairs = BTreeSet::new();
        let mut source_degree: HashMap<&str, usize> = HashMap::new();
        let mut target_degree: HashMap<&str, usize> = HashMap::new();
        for &edge_id in &edge_ids {
            let edge = &graph.edges[edge_id];
            let (source, target) = if sources.iter().any(|id| id == &edge.from) {
                (&edge.from, &edge.to)
            } else {
                continue;
            };
            if !targets.iter().any(|id| id == target) {
                continue;
            }
            if !seen_pairs.insert((source.clone(), target.clone())) {
                continue;
            }
            *source_degree.entry(source.as_str()).or_default() += 1;
            *target_degree.entry(target.as_str()).or_default() += 1;
        }
        if seen_pairs.len() != DENSE_EDGES
            || sources
                .iter()
                .any(|id| source_degree.get(id.as_str()) != Some(&(DENSE_SIDE - 1)))
            || targets
                .iter()
                .any(|id| target_degree.get(id.as_str()) != Some(&(DENSE_SIDE - 1)))
        {
            continue;
        }

        let mut ordered_edges = edge_ids;
        ordered_edges.sort_unstable();
        pairs.push(DensePair {
            edge_ids: ordered_edges,
            source_ids: sources,
            target_ids: targets,
        });
    }
    pairs.sort_by_key(|pair| {
        (
            pair.source_ids.first().cloned().unwrap_or_default(),
            pair.target_ids.first().cloned().unwrap_or_default(),
        )
    });
    pairs
}

fn eligible_graph(graph: &Graph) -> bool {
    graph.subgraphs.is_empty()
        && graph
            .nodes
            .iter()
            .all(|node| node.shape == NodeShape::Rectangle)
        && graph
            .edges
            .iter()
            .all(|edge| !edge.is_back_edge && edge.kind == EdgeKind::Arrow && edge.label.is_none())
}

fn lower_pair(pair: &DensePair, graph: &Graph, canvas: &mut Canvas, style: &StyleChars) -> bool {
    let coords = OrientedCoords::new(graph.direction);
    let mut edges_by_source: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut edges_by_target: HashMap<&str, Vec<usize>> = HashMap::new();
    let source_set: HashSet<&str> = pair.source_ids.iter().map(String::as_str).collect();
    let target_set: HashSet<&str> = pair.target_ids.iter().map(String::as_str).collect();

    for &edge_id in &pair.edge_ids {
        let edge = &graph.edges[edge_id];
        if !source_set.contains(edge.from.as_str()) || !target_set.contains(edge.to.as_str()) {
            return false;
        }
        edges_by_source
            .entry(edge.from.as_str())
            .or_default()
            .push(edge_id);
        edges_by_target
            .entry(edge.to.as_str())
            .or_default()
            .push(edge_id);
    }
    if edges_by_source.values().any(|edges| edges.len() != 2)
        || edges_by_target.values().any(|edges| edges.len() != 2)
    {
        return false;
    }

    let mut source_ports = HashMap::new();
    let mut target_ports = HashMap::new();
    for (source_id, mut edge_ids) in edges_by_source {
        let Some(source) = graph.get_node(source_id) else {
            return false;
        };
        edge_ids.sort_by_key(|edge_id| {
            graph
                .get_node(&graph.edges[*edge_id].to)
                .map(|node| coords.secondary_coord(node.center_x(), node.center_y()))
                .unwrap_or_default()
        });
        let ports = source_secondary_ports(source, graph.direction, edge_ids.len());
        if ports.len() != edge_ids.len() {
            return false;
        }
        for (edge_id, port) in edge_ids.into_iter().zip(ports) {
            source_ports.insert(edge_id, port);
        }
    }
    for (target_id, mut edge_ids) in edges_by_target {
        let Some(target) = graph.get_node(target_id) else {
            return false;
        };
        edge_ids.sort_by_key(|edge_id| {
            graph
                .get_node(&graph.edges[*edge_id].from)
                .map(|node| coords.secondary_coord(node.center_x(), node.center_y()))
                .unwrap_or_default()
        });
        let ports = target_secondary_ports(target, graph.direction, edge_ids.len());
        if ports.len() != edge_ids.len() {
            return false;
        }
        for (edge_id, port) in edge_ids.into_iter().zip(ports) {
            target_ports.insert(edge_id, port);
        }
    }

    let mut lane_order = pair.edge_ids.clone();
    lane_order
        .sort_unstable_by_key(|edge_id| (target_ports[edge_id], source_ports[edge_id], *edge_id));
    let mut lanes = HashMap::new();
    for (index, edge_id) in lane_order.into_iter().enumerate() {
        let edge = &graph.edges[edge_id];
        let Some(source) = graph.get_node(&edge.from) else {
            return false;
        };
        let Some(target) = graph.get_node(&edge.to) else {
            return false;
        };
        let source_primary = coords.primary_coord(
            edge_exit_point(source, graph.direction).0,
            edge_exit_point(source, graph.direction).1,
        );
        let target_entry = edge_entry_point(target, graph.direction);
        let target_primary = coords.primary_coord(target_entry.0, target_entry.1);
        let distance = source_primary.abs_diff(target_primary);
        // A one-cell lane pitch makes a horizontal segment touch the next
        // lane's vertical stem in TD/BT scenes.  That turns an otherwise
        // explicit crossing into an ASCII `+`/Unicode tee during the
        // stabilization pass.  Reserve a blank primary cell between every
        // lane in every orientation so adjacent routes cannot fuse.
        let lane_pitch = 2;
        let required_lane_span = pair
            .edge_ids
            .len()
            .saturating_mul(lane_pitch)
            .saturating_add(1);
        if distance < required_lane_span {
            return false;
        }
        // Keep a blank primary cell between adjacent lanes. In a terminal,
        // `││`/`||` reads like a doubled box border and obscures which lane
        // owns a turn; in vertical scenes the same gap prevents a tee-shaped
        // contact between neighboring routes.
        let lane_offset = (index + 1).saturating_mul(lane_pitch);
        let lane = if flows_forward(graph.direction) {
            source_primary.saturating_add(lane_offset)
        } else {
            source_primary.saturating_sub(lane_offset)
        };
        lanes.insert(edge_id, lane);
    }

    canvas.set_write_stage("edge-route-dense-scene");
    for &edge_id in &pair.edge_ids {
        let edge = &graph.edges[edge_id];
        let Some(source) = graph.get_node(&edge.from) else {
            return false;
        };
        let Some(target) = graph.get_node(&edge.to) else {
            return false;
        };
        let target_entry = edge_entry_point(target, graph.direction);
        let source_secondary = source_ports[&edge_id];
        let target_secondary = target_ports[&edge_id];
        let lane_primary = lanes[&edge_id];
        let owner_id = crate::render::provenance::edge_owner_id(edge_id, edge);
        let owner = RouteOwner {
            kind: crate::render::semantic::CellOwnerKind::EdgeSegment,
            id: owner_id.as_str(),
        };

        // Give each edge a direct port on the source boundary.  A shared
        // center stem would add a second, visually ambiguous junction beside
        // the box; the two declared edges are already distinguishable by
        // their reserved ports and lanes.
        let source_port = dedicated_source_port(source, graph.direction, source_secondary);

        let mut lane_source = source_port;
        coords.set_primary(&mut lane_source.0, &mut lane_source.1, lane_primary);
        draw_line_primary(
            source_port.0,
            source_port.1,
            lane_source.0,
            lane_source.1,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
        let going_before = source_secondary > target_secondary;
        if source_secondary != target_secondary {
            set_route_edge_char(
                canvas,
                lane_source.0,
                lane_source.1,
                coords.corner_start_to_secondary(going_before, style),
                style,
                Some(owner),
            );
        }

        // Dedicated lane between the two node ranks.  All six lanes have a
        // different primary coordinate, so only intentional perpendicular
        // crossings can overlap.
        let lane_target = coords.with_secondary(lane_source.0, lane_source.1, target_secondary);
        if source_secondary != target_secondary {
            draw_line_secondary(
                lane_source.0,
                lane_source.1,
                lane_target.0,
                lane_target.1,
                &coords,
                canvas,
                style,
                Some(graph),
                Some(owner),
            );
            set_route_edge_char(
                canvas,
                lane_target.0,
                lane_target.1,
                coords.corner_secondary_to_end(going_before, style),
                style,
                Some(owner),
            );
        }

        // Keep the arrow on the edge's dedicated target-side port.  Routing
        // every edge back to the canonical center entry would collapse the
        // two arrows for a target into one cell again, defeating the whole
        // scene reservation.
        let target_port_entry =
            coords.with_secondary(target_entry.0, target_entry.1, target_secondary);
        draw_line_primary(
            lane_target.0,
            lane_target.1,
            target_port_entry.0,
            target_port_entry.1,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
        set_route_char(
            canvas,
            target_port_entry.0,
            target_port_entry.1,
            coords.arrow_end(style),
            Some(owner),
        );
    }
    true
}

fn flows_forward(direction: Direction) -> bool {
    matches!(direction, Direction::TD | Direction::TB | Direction::LR)
}

fn dedicated_source_port(node: &Node, direction: Direction, secondary: usize) -> (usize, usize) {
    match direction {
        Direction::TD | Direction::TB => (secondary, node.bottom_y()),
        Direction::BT => (secondary, node.y),
        Direction::LR => (
            node.x.saturating_add(node.width.saturating_sub(1)),
            secondary,
        ),
        Direction::RL => (node.x, secondary),
    }
}

fn source_secondary_ports(node: &Node, direction: Direction, count: usize) -> Vec<usize> {
    if count == 0 {
        return Vec::new();
    }
    match direction {
        Direction::TD | Direction::TB | Direction::BT => {
            let center = node.center_x();
            let offset = if node.width >= 7 { 2 } else { 1 };
            vec![center.saturating_sub(offset), center.saturating_add(offset)]
        }
        Direction::LR | Direction::RL => {
            let height = node.height.max(9);
            // Source exits use the outer interior rows. Target entries use
            // the inner pair below, so an undeclared target cannot share the
            // source corridor's side-wall cells.
            let first = node.y.saturating_add(1);
            let second = node.y.saturating_add(height).saturating_sub(2);
            vec![first, second]
        }
    }
    .into_iter()
    .take(count)
    .collect()
}

fn target_secondary_ports(node: &Node, direction: Direction, count: usize) -> Vec<usize> {
    if count == 0 {
        return Vec::new();
    }
    match direction {
        Direction::TD | Direction::TB | Direction::BT => {
            let center = node.center_x();
            let offset = if node.width >= 11 {
                4
            } else if node.width >= 7 {
                2
            } else {
                1
            };
            vec![center.saturating_sub(offset), center.saturating_add(offset)]
        }
        Direction::LR | Direction::RL => {
            let height = node.height.max(9);
            let first = node.y.saturating_add(3);
            let second = node.y.saturating_add(height).saturating_sub(4);
            vec![first, second]
        }
    }
    .into_iter()
    .take(count)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_dense_ports_straddle_center_without_box_edges() {
        let mut node = Node::new("n", "n");
        node.y = 4;
        node.height = 9;
        assert_eq!(source_secondary_ports(&node, Direction::LR, 2), vec![5, 11]);
        assert_eq!(target_secondary_ports(&node, Direction::LR, 2), vec![7, 9]);
    }

    #[test]
    fn vertical_dense_ports_are_interior() {
        let mut node = Node::new("n", "n");
        node.width = 13;
        assert_eq!(source_secondary_ports(&node, Direction::TD, 2), vec![4, 8]);
        assert_eq!(target_secondary_ports(&node, Direction::TD, 2), vec![2, 10]);
    }
}
