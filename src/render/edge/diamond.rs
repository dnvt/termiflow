//! Topology-derived reservations for ordinary diamond scenes.
//!
//! A graph shaped like `S -> M`, `M -> T`, and `S -> T` is not three unrelated
//! fan-in/fan-out edges.  The direct shortcut must retain its own visible
//! route around the middle node or the convergence/divergence passes can paint
//! it away and leave a misleading serial chain.  This module plans the whole
//! scene before those generic passes run.

use std::collections::{BTreeSet, HashSet};

use crate::graph::{Direction, EdgeKind, Graph, Node};
use crate::orientation::OrientedCoords;
use crate::style::{StyleChars, BOX_HEIGHT};

use super::super::canvas::Canvas;
use super::super::fallback_route::{FallbackAxis, FallbackRoutePlan};
use super::super::semantic::CellOwnerKind;
use super::edge_primitives::{
    adjusted_edge_entry_point, edge_entry_candidates, edge_exit_point, hits_foreign_subgraph_border,
};
use super::{set_route_char, set_route_edge_char, RouteOwner};

const STRATEGY: &str = "diamond-shortcut-scene-reservation";

#[derive(Debug, Clone)]
struct DiamondScene {
    direct_index: usize,
    first_index: usize,
    second_index: usize,
    source_id: String,
    middle_id: String,
    target_id: String,
}

/// Reserve every edge in each topology-proven simple diamond.
///
/// The second return value contains edge indexes that matched the scene but
/// were rejected.  Keeping rejected edges out of the generic fallback loops
/// is intentional: an unsafe scene must remain visibly/structurally
/// unresolved instead of silently falling back to the route that caused the
/// original shortcut loss.
pub(crate) fn plan_diamond_scenes(
    graph: &Graph,
    canvas: &mut Canvas,
    style: &StyleChars,
) -> (HashSet<usize>, HashSet<usize>) {
    let mut planned = HashSet::new();
    let mut rejected = HashSet::new();
    let mut used_nodes = HashSet::new();

    for scene in detect_scenes(graph) {
        if [&scene.source_id, &scene.middle_id, &scene.target_id]
            .iter()
            .any(|id| used_nodes.contains(id.as_str()))
        {
            continue;
        }

        let edge_ids = [scene.first_index, scene.second_index, scene.direct_index];
        let owner_id = format!(
            "scene:{STRATEGY}:{}->{}->{}",
            scene.source_id, scene.middle_id, scene.target_id
        );

        match build_plan(&scene, graph, canvas, style, &owner_id) {
            Ok(plan) => {
                let owner = RouteOwner {
                    kind: CellOwnerKind::EdgeSegment,
                    id: owner_id.as_str(),
                };
                canvas.set_write_stage("edge-route-plan");
                if lower_plan(plan, canvas, style, owner) {
                    planned.extend(edge_ids);
                    used_nodes.extend([
                        scene.source_id.clone(),
                        scene.middle_id.clone(),
                        scene.target_id.clone(),
                    ]);
                } else {
                    rejected.extend(edge_ids);
                }
            }
            Err(reason) => {
                canvas.record_fallback_route_rejection(owner_id, STRATEGY, reason);
                rejected.extend(edge_ids);
            }
        }
    }

    (planned, rejected)
}

fn detect_scenes(graph: &Graph) -> Vec<DiamondScene> {
    let coords = OrientedCoords::new(graph.direction);
    let mut scenes = Vec::new();

    for (direct_index, direct) in graph.edges.iter().enumerate() {
        if !eligible_edge(direct) {
            continue;
        }

        let Some(source) = graph.get_node(&direct.from) else {
            continue;
        };
        let Some(target) = graph.get_node(&direct.to) else {
            continue;
        };
        if !eligible_node(graph, source) || !eligible_node(graph, target) {
            continue;
        }

        let source_primary = coords.primary_coord(source.center_x(), source.center_y());
        let target_primary = coords.primary_coord(target.center_x(), target.center_y());
        if !flow_before(graph.direction, source_primary, target_primary) {
            continue;
        }

        for (first_index, first) in graph.edges.iter().enumerate() {
            if first_index == direct_index
                || !eligible_edge(first)
                || first.from != direct.from
                || first.to == direct.to
            {
                continue;
            }
            let Some(middle) = graph.get_node(&first.to) else {
                continue;
            };
            if !eligible_node(graph, middle) || middle.id == source.id || middle.id == target.id {
                continue;
            }

            let middle_primary = coords.primary_coord(middle.center_x(), middle.center_y());
            if !flow_before(graph.direction, source_primary, middle_primary)
                || !flow_before(graph.direction, middle_primary, target_primary)
            {
                continue;
            }

            for (second_index, second) in graph.edges.iter().enumerate() {
                if second_index == direct_index
                    || second_index == first_index
                    || !eligible_edge(second)
                    || second.from != middle.id
                    || second.to != target.id
                {
                    continue;
                }

                // Do not claim a shortcut if the same endpoint pair has
                // parallel copies.  Their ownership needs a separate scene
                // policy and must remain visible to that experiment.
                let direct_pair_count = graph
                    .edges
                    .iter()
                    .filter(|edge| {
                        eligible_edge(edge) && edge.from == direct.from && edge.to == direct.to
                    })
                    .count();
                if direct_pair_count != 1 {
                    continue;
                }

                scenes.push(DiamondScene {
                    direct_index,
                    first_index,
                    second_index,
                    source_id: direct.from.clone(),
                    middle_id: middle.id.clone(),
                    target_id: direct.to.clone(),
                });
                break;
            }
        }
    }

    scenes.sort_by_key(|scene| {
        (
            scene.source_id.clone(),
            scene.middle_id.clone(),
            scene.target_id.clone(),
            scene.direct_index,
        )
    });
    scenes
}

fn eligible_edge(edge: &crate::graph::Edge) -> bool {
    !edge.is_back_edge && edge.kind == EdgeKind::Arrow && edge.label.is_none()
}

fn eligible_node(graph: &Graph, node: &Node) -> bool {
    graph.get_node_subgraph(&node.id).is_none()
}

fn flow_before(direction: Direction, from: usize, to: usize) -> bool {
    match direction {
        Direction::TD | Direction::TB | Direction::LR => from < to,
        Direction::BT | Direction::RL => from > to,
    }
}

fn build_plan(
    scene: &DiamondScene,
    graph: &Graph,
    canvas: &Canvas,
    style: &StyleChars,
    owner_id: &str,
) -> Result<FallbackRoutePlan, String> {
    let source = graph
        .get_node(&scene.source_id)
        .ok_or_else(|| "source node disappeared".to_owned())?;
    let middle = graph
        .get_node(&scene.middle_id)
        .ok_or_else(|| "middle node disappeared".to_owned())?;
    let target = graph
        .get_node(&scene.target_id)
        .ok_or_else(|| "target node disappeared".to_owned())?;

    let coords = OrientedCoords::new(graph.direction);
    let lane = choose_shortcut_lane(graph, canvas, &coords)
        .ok_or_else(|| "no safe outside shortcut lane".to_owned())?;

    let mut plan = FallbackRoutePlan::new(owner_id, STRATEGY);
    let covered_edge_ids = [scene.first_index, scene.second_index, scene.direct_index]
        .into_iter()
        .map(|index| crate::render::provenance::edge_owner_id(index, &graph.edges[index]));
    plan.set_scene_coverage(covered_edge_ids);

    append_standard_path(&mut plan, source, middle, graph.direction, graph, style)?;
    append_standard_path(&mut plan, middle, target, graph.direction, graph, style)?;

    let shortcut_entry = choose_shortcut_entry(target, graph.direction, lane, graph);
    append_shortcut_path(
        &mut plan,
        source,
        shortcut_entry,
        lane,
        graph.direction,
        style,
    )?;

    validate_plan_cells(&plan, graph, canvas)?;
    if let Some(reason) = plan.validation_error(canvas.width, canvas.height) {
        return Err(reason);
    }
    Ok(plan)
}

fn choose_shortcut_lane(graph: &Graph, canvas: &Canvas, coords: &OrientedCoords) -> Option<usize> {
    let limit = match coords.secondary {
        crate::orientation::Axis::Horizontal => canvas.width,
        crate::orientation::Axis::Vertical => canvas.height,
    };
    let mut intervals = Vec::new();
    for node in &graph.nodes {
        let (start, end) = match coords.secondary {
            crate::orientation::Axis::Horizontal => {
                (node.x, node.x.saturating_add(node.width.saturating_sub(1)))
            }
            crate::orientation::Axis::Vertical => (
                node.y,
                node.y
                    .saturating_add(node.height.max(BOX_HEIGHT).saturating_sub(1)),
            ),
        };
        intervals.push((start, end));
    }
    let minimum = intervals.iter().map(|(start, _)| *start).min()?;
    let maximum = intervals.iter().map(|(_, end)| *end).max()?;

    // Prefer the far side of the scene.  The opposite candidate remains a
    // useful fallback for a diagram whose far edge is clipped by the canvas.
    let candidates = [
        maximum.saturating_add(1),
        maximum.saturating_add(2),
        minimum.saturating_sub(1),
        minimum.saturating_sub(2),
    ];
    candidates.into_iter().find(|candidate| {
        *candidate < limit
            && intervals
                .iter()
                .all(|(start, end)| *candidate < *start || *candidate > *end)
    })
}

fn choose_shortcut_entry(
    target: &Node,
    direction: Direction,
    lane: usize,
    graph: &Graph,
) -> (usize, usize) {
    let coords = OrientedCoords::new(direction);
    let base = adjusted_edge_entry_point(target, direction, graph);
    edge_entry_candidates(target, direction)
        .into_iter()
        .filter(|candidate| !hits_foreign_subgraph_border(target, candidate.0, candidate.1, graph))
        .filter(|candidate| {
            coords.secondary_coord(candidate.0, candidate.1)
                != coords.secondary_coord(base.0, base.1)
        })
        .min_by_key(|candidate| {
            coords
                .secondary_coord(candidate.0, candidate.1)
                .abs_diff(lane)
        })
        .unwrap_or(base)
}

fn choose_entry_for_secondary(
    target: &Node,
    direction: Direction,
    desired_secondary: usize,
    graph: &Graph,
) -> (usize, usize) {
    let coords = OrientedCoords::new(direction);
    edge_entry_candidates(target, direction)
        .into_iter()
        .filter(|candidate| !hits_foreign_subgraph_border(target, candidate.0, candidate.1, graph))
        .min_by_key(|candidate| {
            coords
                .secondary_coord(candidate.0, candidate.1)
                .abs_diff(desired_secondary)
        })
        .unwrap_or_else(|| adjusted_edge_entry_point(target, direction, graph))
}

fn append_standard_path(
    plan: &mut FallbackRoutePlan,
    source: &Node,
    target: &Node,
    direction: Direction,
    graph: &Graph,
    style: &StyleChars,
) -> Result<(), String> {
    let coords = OrientedCoords::new(direction);
    let start = edge_exit_point(source, direction);
    let start_secondary = coords.secondary_coord(start.0, start.1);
    let end = choose_entry_for_secondary(target, direction, start_secondary, graph);
    ensure_flow(&coords, start, end)?;

    let end_secondary = coords.secondary_coord(end.0, end.1);
    if start_secondary == end_secondary {
        push_primary(plan, start, end, &coords, style);
        plan.push_paint(end.0, end.1, coords.arrow_end(style));
        return Ok(());
    }

    let turn = coords.advance(start.0, start.1, 1);
    let bend = coords.with_secondary(turn.0, turn.1, end_secondary);
    if !flow_before(
        direction,
        coords.primary_coord(turn.0, turn.1),
        coords.primary_coord(end.0, end.1),
    ) {
        return Err("standard edge has no room for a visible elbow".to_owned());
    }
    push_primary(plan, start, turn, &coords, style);
    plan.push_corner(turn.0, turn.1, corner_glyph(turn, start, bend, style));
    push_secondary(plan, turn, bend, &coords, style);
    plan.push_corner(
        bend.0,
        bend.1,
        corner_glyph(bend, turn, coords.advance(bend.0, bend.1, 1), style),
    );
    let final_start = coords.advance(bend.0, bend.1, 1);
    push_primary(plan, final_start, end, &coords, style);
    plan.push_paint(end.0, end.1, coords.arrow_end(style));
    Ok(())
}

fn append_shortcut_path(
    plan: &mut FallbackRoutePlan,
    source: &Node,
    end: (usize, usize),
    lane: usize,
    direction: Direction,
    style: &StyleChars,
) -> Result<(), String> {
    let coords = OrientedCoords::new(direction);
    let start = edge_exit_point(source, direction);
    ensure_flow(&coords, start, end)?;

    let start_turn = coords.advance(start.0, start.1, 1);
    let end_turn = coords.retreat(end.0, end.1, 1);
    if !flow_before(
        direction,
        coords.primary_coord(start_turn.0, start_turn.1),
        coords.primary_coord(end_turn.0, end_turn.1),
    ) {
        return Err("shortcut has no room for an outside detour".to_owned());
    }

    let end_secondary = coords.secondary_coord(end.0, end.1);
    let start_lane = coords.with_secondary(start_turn.0, start_turn.1, lane);
    let end_lane = coords.with_secondary(end_turn.0, end_turn.1, lane);
    let target_lane = coords.with_secondary(end_turn.0, end_turn.1, end_secondary);

    push_primary(plan, start, start_turn, &coords, style);
    plan.push_corner(
        start_turn.0,
        start_turn.1,
        corner_glyph(start_turn, start, start_lane, style),
    );
    push_secondary(plan, start_turn, start_lane, &coords, style);
    plan.push_corner(
        start_lane.0,
        start_lane.1,
        corner_glyph(start_lane, start_turn, end_lane, style),
    );
    push_primary(plan, start_lane, end_lane, &coords, style);
    plan.push_corner(
        end_lane.0,
        end_lane.1,
        corner_glyph(end_lane, start_lane, target_lane, style),
    );
    push_secondary(plan, end_lane, target_lane, &coords, style);
    plan.push_corner(
        target_lane.0,
        target_lane.1,
        corner_glyph(
            target_lane,
            end_lane,
            coords.advance(target_lane.0, target_lane.1, 1),
            style,
        ),
    );
    let final_start = coords.advance(target_lane.0, target_lane.1, 1);
    push_primary(plan, final_start, end, &coords, style);
    plan.push_paint(end.0, end.1, coords.arrow_end(style));
    Ok(())
}

fn ensure_flow(
    coords: &OrientedCoords,
    start: (usize, usize),
    end: (usize, usize),
) -> Result<(), String> {
    let start_primary = coords.primary_coord(start.0, start.1);
    let end_primary = coords.primary_coord(end.0, end.1);
    if flow_before(coords.direction, start_primary, end_primary) {
        Ok(())
    } else {
        Err("edge endpoints are not ordered along the active flow axis".to_owned())
    }
}

fn corner_glyph(
    point: (usize, usize),
    first_neighbor: (usize, usize),
    second_neighbor: (usize, usize),
    style: &StyleChars,
) -> char {
    let neighbors = [first_neighbor, second_neighbor];
    let has_left = neighbors.iter().any(|(x, y)| *y == point.1 && *x < point.0);
    let has_right = neighbors.iter().any(|(x, y)| *y == point.1 && *x > point.0);
    let has_up = neighbors.iter().any(|(x, y)| *x == point.0 && *y < point.1);
    let has_down = neighbors.iter().any(|(x, y)| *x == point.0 && *y > point.1);

    match (has_left, has_right, has_up, has_down) {
        (true, false, false, true) => style.corner_dr,
        (false, true, false, true) => style.corner_dl,
        (true, false, true, false) => style.corner_ur,
        (false, true, true, false) => style.corner_ul,
        _ => style.cross,
    }
}

fn push_primary(
    plan: &mut FallbackRoutePlan,
    from: (usize, usize),
    to: (usize, usize),
    coords: &OrientedCoords,
    style: &StyleChars,
) {
    match coords.primary {
        crate::orientation::Axis::Horizontal => {
            plan.push_horizontal(from.1, from.0, to.0, style.edge_h);
        }
        crate::orientation::Axis::Vertical => {
            plan.push_vertical(from.0, from.1, to.1, style.edge_v);
        }
    }
}

fn push_secondary(
    plan: &mut FallbackRoutePlan,
    from: (usize, usize),
    to: (usize, usize),
    coords: &OrientedCoords,
    style: &StyleChars,
) {
    match coords.secondary {
        crate::orientation::Axis::Horizontal => {
            plan.push_horizontal(from.1, from.0, to.0, style.edge_h);
        }
        crate::orientation::Axis::Vertical => {
            plan.push_vertical(from.0, from.1, to.1, style.edge_v);
        }
    }
}

fn validate_plan_cells(
    plan: &FallbackRoutePlan,
    graph: &Graph,
    canvas: &Canvas,
) -> Result<(), String> {
    let cells = plan.planned_cells();
    let mut seen = BTreeSet::new();
    for (x, y) in cells {
        if !seen.insert((x, y)) {
            continue;
        }
        if canvas.fallback_route_claims_cell(x, y) {
            return Err(format!("existing fallback reservation blocks ({x},{y})"));
        }
        if canvas.get(x, y) != ' ' {
            return Err(format!("existing canvas content blocks ({x},{y})"));
        }
        if graph.nodes.iter().any(|node| node_contains(node, x, y)) {
            return Err(format!("node keepout blocks planned cell at ({x},{y})"));
        }
    }
    Ok(())
}

fn node_contains(node: &Node, x: usize, y: usize) -> bool {
    x >= node.x
        && x < node.x.saturating_add(node.width)
        && y >= node.y
        && y < node.y.saturating_add(node.height.max(BOX_HEIGHT))
}

fn lower_plan(
    plan: FallbackRoutePlan,
    canvas: &mut Canvas,
    style: &StyleChars,
    owner: RouteOwner<'_>,
) -> bool {
    if let Some(reason) = plan.validation_error(canvas.width, canvas.height) {
        canvas.record_fallback_route_rejection(plan.owner_id, plan.strategy, reason);
        return false;
    }
    canvas.record_fallback_route_plan(plan.clone());

    for segment in &plan.segments {
        match segment.axis {
            FallbackAxis::Horizontal => {
                let (start, end) = if segment.from.x <= segment.to.x {
                    (segment.from.x, segment.to.x)
                } else {
                    (segment.to.x, segment.from.x)
                };
                for x in start..=end {
                    set_route_edge_char(
                        canvas,
                        x,
                        segment.from.y,
                        segment.glyph,
                        style,
                        Some(owner),
                    );
                }
            }
            FallbackAxis::Vertical => {
                let (start, end) = if segment.from.y <= segment.to.y {
                    (segment.from.y, segment.to.y)
                } else {
                    (segment.to.y, segment.from.y)
                };
                for y in start..=end {
                    set_route_edge_char(
                        canvas,
                        segment.from.x,
                        y,
                        segment.glyph,
                        style,
                        Some(owner),
                    );
                }
            }
        }
    }
    for corner in &plan.corners {
        set_route_char(
            canvas,
            corner.point.x,
            corner.point.y,
            canonical_corner_glyph(&plan, corner.point.x, corner.point.y, corner.glyph, style),
            Some(owner),
        );
    }
    for paint in &plan.paints {
        set_route_char(
            canvas,
            paint.point.x,
            paint.point.y,
            paint.glyph,
            Some(owner),
        );
    }
    true
}

fn canonical_corner_glyph(
    plan: &FallbackRoutePlan,
    x: usize,
    y: usize,
    fallback: char,
    style: &StyleChars,
) -> char {
    let mut has_left = false;
    let mut has_right = false;
    let mut has_up = false;
    let mut has_down = false;

    for segment in &plan.segments {
        match segment.axis {
            FallbackAxis::Horizontal if segment.from.y == y => {
                let min_x = segment.from.x.min(segment.to.x);
                let max_x = segment.from.x.max(segment.to.x);
                if x >= min_x && x <= max_x {
                    has_left |= x > min_x;
                    has_right |= x < max_x;
                }
            }
            FallbackAxis::Vertical if segment.from.x == x => {
                let min_y = segment.from.y.min(segment.to.y);
                let max_y = segment.from.y.max(segment.to.y);
                if y >= min_y && y <= max_y {
                    has_up |= y > min_y;
                    has_down |= y < max_y;
                }
            }
            _ => {}
        }
    }

    match (has_left, has_right, has_up, has_down) {
        (true, true, false, true) => style.junction_down,
        (true, true, true, false) => style.junction_up,
        (true, false, true, true) => style.junction_left,
        (false, true, true, true) => style.junction_right,
        (true, true, true, true) => style.cross,
        _ => fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, Subgraph};
    use crate::style::ASCII_CHARS;

    fn diamond_graph(direction: Direction) -> Graph {
        let mut graph = Graph::new();
        graph.direction = direction;
        for (id, primary) in [("S", 0usize), ("M", 8), ("T", 16)] {
            let mut node = Node::new(id, id);
            node.width = 5;
            node.height = 3;
            match direction {
                Direction::TD | Direction::TB => {
                    node.x = 0;
                    node.y = primary;
                }
                Direction::BT => {
                    node.x = 0;
                    node.y = 16usize.saturating_sub(primary);
                }
                Direction::LR => {
                    node.x = primary;
                    node.y = 0;
                }
                Direction::RL => {
                    node.x = 16usize.saturating_sub(primary);
                    node.y = 0;
                }
            }
            graph.add_node(node);
        }
        graph.add_edge(Edge::new("S", "T"));
        graph.add_edge(Edge::new("S", "M"));
        graph.add_edge(Edge::new("M", "T"));
        graph
    }

    #[test]
    fn detector_accepts_only_oriented_simple_diamonds() {
        for direction in [
            Direction::TD,
            Direction::TB,
            Direction::BT,
            Direction::LR,
            Direction::RL,
        ] {
            let graph = diamond_graph(direction);
            assert_eq!(detect_scenes(&graph).len(), 1, "direction={direction:?}");
        }
    }

    #[test]
    fn detector_rejects_incomplete_fanin_and_fanout_shapes() {
        let mut fanin = diamond_graph(Direction::TD);
        fanin.edges = vec![Edge::new("S", "T"), Edge::new("M", "T")];
        assert!(detect_scenes(&fanin).is_empty());

        let mut fanout = diamond_graph(Direction::TD);
        fanout.edges = vec![Edge::new("S", "M"), Edge::new("S", "T")];
        assert!(detect_scenes(&fanout).is_empty());

        let mut reversed = diamond_graph(Direction::TD);
        for node in &mut reversed.nodes {
            node.y = 16usize.saturating_sub(node.y);
        }
        assert!(detect_scenes(&reversed).is_empty());
    }

    #[test]
    fn detector_rejects_parallel_direct_copies_and_subgraph_membership() {
        let mut parallel = diamond_graph(Direction::TD);
        parallel.edges.push(Edge::new("S", "T"));
        assert!(detect_scenes(&parallel).is_empty());

        let mut nested = diamond_graph(Direction::TD);
        nested.add_subgraph(Subgraph::new("group", Some("Group".to_owned())));
        nested.associate_node_with_subgraph("M", "group");
        assert!(detect_scenes(&nested).is_empty());
    }

    #[test]
    fn unsafe_capacity_is_rejected_without_generic_fallback_claims() {
        let mut graph = diamond_graph(Direction::TD);
        for node in &mut graph.nodes {
            node.width = 10;
        }
        let mut canvas = Canvas::new(10, 24);
        let (planned, rejected) = plan_diamond_scenes(&graph, &mut canvas, &ASCII_CHARS);

        assert!(planned.is_empty());
        assert_eq!(rejected.len(), 3);
        assert_eq!(canvas.fallback_route_rejections().len(), 1);
        assert!(canvas.fallback_route_traces().is_empty());
    }
}
