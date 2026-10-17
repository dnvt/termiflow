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
use super::super::topology::{
    canonical_routing_glyph, canvas_connections, char_connects_down, char_connects_left,
    char_connects_right, char_connects_up,
};
use super::edge_primitives::{
    adjusted_edge_entry_point, edge_entry_candidates, edge_exit_point, hits_foreign_subgraph_border,
};
use super::{set_route_char, set_route_edge_char, style_for_edge_kind, RouteOwner};

const STRATEGY: &str = "diamond-shortcut-scene-reservation";
pub(crate) const DETOUR_STRATEGY: &str = "blocked-edge-outside-detour";

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

/// Reserve an outside detour for each plain edge whose straight path would
/// run through another node's box, where the generic fan-in and fan-out
/// passes would draw it hidden under that box.
///
/// Edges in `claimed` already have an owner, and edges in `dropped` lost
/// their detour to a line it was in the way of. An edge whose detour cannot
/// be planned is left to the generic passes, as before.
///
/// A source's edges blocked on a line into their targets are detoured
/// together or not at all: a detour leaves on the source's stem, which a
/// sibling still drawn through a box would join to that box.  An edge a
/// fan-in gathers along a blocked line, before it turns toward its target,
/// is detoured where it can be; one that cannot be is left to the generic
/// passes without taking its siblings' detours with it.
pub(crate) fn plan_blocked_edge_detours(
    graph: &Graph,
    canvas: &mut Canvas,
    style: &StyleChars,
    claimed: &HashSet<usize>,
    dropped: &HashSet<usize>,
) -> HashSet<usize> {
    let blocked: Vec<usize> = (0..graph.edges.len())
        .filter(|index| {
            !claimed.contains(index) && generic_route_is_blocked(graph, canvas, claimed, *index)
        })
        .collect();
    let mut sources: Vec<&str> = Vec::new();
    for &index in &blocked {
        let source = graph.edges[index].from.as_str();
        if !sources.contains(&source) {
            sources.push(source);
        }
    }
    let mut planned = HashSet::new();
    for source in sources {
        let group: Vec<usize> = blocked
            .iter()
            .copied()
            .filter(|&index| graph.edges[index].from == source)
            .collect();
        let before = canvas.clone();
        let (lowered, left): (Vec<usize>, Vec<usize>) = group.iter().partition(|&&index| {
            !dropped.contains(&index)
                && plan_blocked_edge_detour(graph, canvas, style, claimed, index)
        });
        // A blocked edge left behind to a target a detour already reaches
        // shares that detour's picture; one to another target does not.
        let reached = |index: &usize| {
            lowered
                .iter()
                .any(|&other| graph.edges[other].to == graph.edges[*index].to)
        };
        if left
            .iter()
            .filter(|&&index| !only_blocked_before_its_turn(graph, canvas, index))
            .all(reached)
        {
            planned.extend(lowered);
        } else {
            *canvas = before;
        }
    }
    planned
}

/// Reserve an outside detour for the blocked edge at `index`, growing an
/// LR/RL canvas by the lane when that is what it takes.
fn plan_blocked_edge_detour(
    graph: &Graph,
    canvas: &mut Canvas,
    style: &StyleChars,
    claimed: &HashSet<usize>,
    index: usize,
) -> bool {
    let coords = OrientedCoords::new(graph.direction);
    let edge = &graph.edges[index];
    let (Some(source), Some(target)) = (graph.get_node(&edge.from), graph.get_node(&edge.to))
    else {
        return false;
    };
    // Cycle routes leave and enter on the sides of nodes on a back edge,
    // where an outside lane alongside them would be cut.
    if span_has_back_edge_node(graph, source, target, &coords) {
        return false;
    }
    // An LR/RL canvas sized tightly around the boxes has no outside lane
    // left; grow it by the lane only when the detour then fits.  A lane
    // right beside a box or a frame's wall reads as a second border, notably
    // in ASCII, where all are drawn alike, so it is taken only when no
    // clearer lane fits, one beside a wall last.
    let mut grown = canvas.clone();
    ensure_detour_lane_capacity(&mut grown, graph, &coords);
    let grows = (grown.width, grown.height) != (canvas.width, canvas.height);
    for clearance in [
        LaneClearance::Clear,
        LaneClearance::BesideBox,
        LaneClearance::BesideWall,
    ] {
        if lower_blocked_edge_detour(graph, canvas, style, claimed, index, clearance) {
            return true;
        }
        let mut trial = grown.clone();
        if grows && lower_blocked_edge_detour(graph, &mut trial, style, claimed, index, clearance) {
            *canvas = trial;
            return true;
        }
    }
    false
}

/// The detoured edges among `planned` that were in another line's way on
/// the finished canvas: one held a cell a later line of another edge needed
/// or another edge's label was written across, or left a plain line touching
/// its target where the edge would have arrived, which then reads as leaving
/// the target.
///
/// A later line out of the same source or into the same target joins the
/// detour as a shared trunk or bus instead.
pub(crate) fn detours_in_the_way(
    graph: &Graph,
    canvas: &Canvas,
    planned: &HashSet<usize>,
) -> HashSet<usize> {
    planned
        .iter()
        .copied()
        .filter(|&index| {
            let edge = &graph.edges[index];
            let owner_id = detour_owner_id(edge);
            let cut = canvas
                .refused_route_writes()
                .iter()
                .filter(|(holder, _, _, _)| *holder == owner_id)
                .any(|(_, _, _, writer)| {
                    let (from, to) = writer_ends(graph, writer.as_deref());
                    from != Some(edge.from.as_str()) && to != Some(edge.to.as_str())
                });
            // Another edge's label written across the detour cuts it too.
            let own_label = crate::render::provenance::edge_owner_id(index, edge);
            let labelled_over = canvas.fallback_route_plan(&owner_id).is_some_and(|plan| {
                plan.planned_cells().into_iter().any(|(x, y)| {
                    canvas.get_meta(x, y).is_some_and(|meta| {
                        meta.owner_kind == CellOwnerKind::EdgeLabel
                            && meta.owner_id.as_deref() != Some(own_label.as_str())
                    })
                })
            });
            // A detour taken only because the edge was blocked before its turn
            // stays only while no other line crosses it.
            let crossed = only_blocked_before_its_turn(graph, canvas, index)
                && canvas
                    .fallback_route_plan(&owner_id)
                    .is_some_and(|plan| !plan.shared_cells.is_empty());
            if cut || labelled_over || crossed {
                return true;
            }
            let Some(target) = graph.get_node(&edge.to) else {
                return false;
            };
            let (x, y) = adjusted_edge_entry_point(target, graph.direction, graph);
            let glyph = canvas.get(x, y);
            let owned = canvas
                .get_meta(x, y)
                .and_then(|meta| meta.owner_id.as_deref())
                == Some(owner_id.as_str());
            let toward_target = match graph.direction {
                Direction::TD | Direction::TB => char_connects_down(glyph),
                Direction::BT => char_connects_up(glyph),
                Direction::LR => char_connects_right(glyph),
                Direction::RL => char_connects_left(glyph),
            };
            !owned && toward_target && !super::super::canvas::is_arrow(glyph)
        })
        .collect()
}

/// The source and target a route owner id names, where it names them.
fn writer_ends<'a>(
    graph: &'a Graph,
    writer: Option<&'a str>,
) -> (Option<&'a str>, Option<&'a str>) {
    let Some(writer) = writer else {
        return (None, None);
    };
    if let Some(source) = writer.strip_prefix("fanout:") {
        return (Some(source), None);
    }
    if let Some(target) = writer.strip_prefix("fanin:") {
        return (None, Some(target));
    }
    writer
        .strip_prefix("edge:")
        .and_then(|rest| rest.split_once(':'))
        .and_then(|(index, _)| index.parse::<usize>().ok())
        .and_then(|index| graph.edges.get(index))
        .map_or((None, None), |edge| {
            (Some(edge.from.as_str()), Some(edge.to.as_str()))
        })
}

/// The lane of the detour reserved for the edge at `index`, its longest
/// run, where its label goes.
pub(crate) fn detour_route(
    graph: &Graph,
    canvas: &Canvas,
    index: usize,
) -> Option<crate::geom::EdgeRoute> {
    let plan = canvas.fallback_route_plan(&detour_owner_id(graph.edges.get(index)?))?;
    let lane = plan.segments.iter().max_by_key(|segment| {
        segment.from.x.abs_diff(segment.to.x) + segment.from.y.abs_diff(segment.to.y)
    })?;
    let mut route = crate::geom::EdgeRoute::new();
    route.push_segment(
        crate::geom::Point::new(lane.from.x, lane.from.y),
        crate::geom::Point::new(lane.to.x, lane.to.y),
    );
    Some(route)
}

fn detour_owner_id(edge: &crate::graph::Edge) -> String {
    format!("scene:{DETOUR_STRATEGY}:{}->{}", edge.from, edge.to)
}

/// Lower the blocked edge at `index` along the first outside lane that
/// takes it, returning whether one did.  A detour turns off its source's
/// stem one cell out; only when no lane takes it there does it turn further
/// along the stem, past a frame wall its leg would otherwise run beside.
fn lower_blocked_edge_detour(
    graph: &Graph,
    canvas: &mut Canvas,
    style: &StyleChars,
    claimed: &HashSet<usize>,
    index: usize,
    clearance: LaneClearance,
) -> bool {
    let coords = OrientedCoords::new(graph.direction);
    let edge = &graph.edges[index];
    let (Some(source), Some(target)) = (graph.get_node(&edge.from), graph.get_node(&edge.to))
    else {
        return false;
    };
    let owner_id = detour_owner_id(edge);
    let clean_only = only_blocked_before_its_turn(graph, canvas, index);
    let mut routes = Vec::new();
    for lane in detour_lane_candidates(graph, canvas, &coords, source, target, clearance) {
        // A detour that could only arrive on the target's own stem would
        // share it with another edge, and one crossing a gap where a generic
        // bus gathers into the target would merge with that bus, so either
        // enters from the lane side instead.  A port another route already
        // holds also leaves the side entry.
        let entry = choose_shortcut_entry(target, graph.direction, lane, graph);
        let side_only = entry == adjusted_edge_entry_point(target, graph.direction, graph)
            || target_gap_has_generic_bus(graph, claimed, index);
        let entries: &[bool] = if side_only { &[true] } else { &[false, true] };
        routes.extend(entries.iter().map(|&side_entry| (lane, entry, side_entry)));
    }
    let build = |(lane, entry, side_entry): (usize, (usize, usize), bool), turn_after: usize| {
        let mut plan = FallbackRoutePlan::new(&owner_id, DETOUR_STRATEGY);
        plan.set_scene_coverage([crate::render::provenance::edge_owner_id(index, edge)]);
        let crossings = if side_entry {
            append_side_entry_detour(
                &mut plan, source, target, lane, turn_after, &coords, style, edge.kind,
            )
        } else {
            append_shortcut_path(
                &mut plan,
                source,
                entry,
                lane,
                turn_after,
                graph.direction,
                style,
                edge.kind,
            )
        }
        .and_then(|()| detour_frame_crossings(&plan, graph, canvas, source, target))
        .and_then(|crossings| {
            corners_clear_of_boxes(&plan, graph, source, target).map(|()| crossings)
        })
        .and_then(|crossings| legs_clear_of_walls(&plan, graph, &coords).map(|()| crossings))
        .ok()?;
        if clearance != LaneClearance::BesideWall
            && lane_runs_beside_a_box(&plan, graph, &coords, lane)
                != (clearance == LaneClearance::BesideBox)
        {
            return None;
        }
        if clean_only && !crossings.is_empty() {
            return None;
        }
        // The line replaces the wall where it crosses, as at a portal.
        for (frame, side, x, y, glyph) in crossings {
            plan.claim_boundary(frame, side, x, y, glyph);
            plan.push_paint(x, y, glyph);
        }
        plan.validation_error(canvas.width, canvas.height)
            .is_none()
            .then_some(plan)
    };
    let start = edge_exit_point(source, graph.direction);
    let start_primary = coords.primary_coord(start.0, start.1);
    let (target_primary, _) = node_spans(target, &coords);
    let reach = target_primary
        .0
        .abs_diff(start_primary)
        .max(target_primary.1.abs_diff(start_primary));
    let plan = routes
        .iter()
        .find_map(|&route| build(route, 1))
        .or_else(|| {
            routes
                .iter()
                .find_map(|&route| (2..reach).find_map(|turn_after| build(route, turn_after)))
        });
    let Some(plan) = plan else {
        return false;
    };
    let owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: owner_id.as_str(),
    };
    canvas.set_write_stage("edge-route-plan");
    lower_plan(plan, canvas, style, owner)
}

/// Grow an LR/RL canvas so both outside lanes below the boxes fit.  Width is
/// left alone: it is bounded by the configured maximum canvas width.
fn ensure_detour_lane_capacity(canvas: &mut Canvas, graph: &Graph, coords: &OrientedCoords) {
    if coords.secondary != crate::orientation::Axis::Vertical {
        return;
    }
    let far_side = graph
        .nodes
        .iter()
        .map(|node| node_spans(node, coords).1 .1)
        .chain(
            graph
                .subgraphs
                .iter()
                .filter(|sg| sg.bounds.is_valid())
                .map(|sg| frame_spans(&sg.bounds, coords).1 .1),
        )
        .max()
        .unwrap_or(0);
    canvas.ensure_height(far_side.saturating_add(3));
}

/// The outside lanes a detour from `source` to `target` may take: those of
/// `shortcut_lane_candidates`, kept also clear of every frame along the way
/// that holds neither end, so the detour crosses only the walls of the
/// frames it leaves or enters.  Failing those, lanes outside every frame
/// along the way, which it then leaves and enters through their sides.
/// Only the lanes right beside the wall of a frame along the way when
/// `clearance` asks for those, or only the others.
fn detour_lane_candidates(
    graph: &Graph,
    canvas: &Canvas,
    coords: &OrientedCoords,
    source: &Node,
    target: &Node,
    clearance: LaneClearance,
) -> Vec<usize> {
    let (source_primary, _) = node_spans(source, coords);
    let (target_primary, _) = node_spans(target, coords);
    let low = source_primary.0.min(target_primary.0);
    let high = source_primary.1.max(target_primary.1);
    let along: Vec<(&crate::graph::Subgraph, (usize, usize))> = graph
        .subgraphs
        .iter()
        .filter(|sg| sg.bounds.is_valid())
        .map(|sg| (sg, frame_spans(&sg.bounds, coords)))
        .filter(|(_, (primary, _))| primary.1 >= low && primary.0 <= high)
        .map(|(sg, (_, secondary))| (sg, secondary))
        .collect();
    let foreign: Vec<(usize, usize)> = along
        .iter()
        .filter(|(sg, _)| {
            !graph.is_node_in_subgraph_tree(&source.id, &sg.id)
                && !graph.is_node_in_subgraph_tree(&target.id, &sg.id)
        })
        .map(|(_, secondary)| *secondary)
        .collect();
    let boxes = graph.nodes.iter().map(|node| node_spans(node, coords).1);
    let mut lanes = if foreign.is_empty() {
        shortcut_lane_candidates(graph, canvas, coords)
    } else {
        lanes_clear_of(canvas, coords, boxes.clone().chain(foreign).collect())
    };
    if !along.is_empty() {
        let outside = boxes.chain(along.iter().map(|(_, secondary)| *secondary));
        for lane in lanes_clear_of(canvas, coords, outside.collect()) {
            if !lanes.contains(&lane) {
                lanes.push(lane);
            }
        }
    }
    lanes.retain(|lane| {
        along
            .iter()
            .any(|(_, (start, end))| lane.abs_diff(*start) <= 1 || lane.abs_diff(*end) <= 1)
            == (clearance == LaneClearance::BesideWall)
    });
    lanes
}

/// How near a detour's lane runs to what it passes along the way.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LaneClearance {
    /// A blank line away from every box and frame wall.
    Clear,
    /// Right beside a box, but not a frame's wall.
    BesideBox,
    /// Right beside a frame's wall.
    BesideWall,
}

/// Whether the detour in `plan` runs along `lane` right beside a box, where
/// the lane reads as a second border.
fn lane_runs_beside_a_box(
    plan: &FallbackRoutePlan,
    graph: &Graph,
    coords: &OrientedCoords,
    lane: usize,
) -> bool {
    let run_axis = match coords.primary {
        crate::orientation::Axis::Horizontal => FallbackAxis::Horizontal,
        crate::orientation::Axis::Vertical => FallbackAxis::Vertical,
    };
    plan.segments
        .iter()
        .filter(|segment| {
            segment.axis == run_axis
                && coords.secondary_coord(segment.from.x, segment.from.y) == lane
        })
        .any(|segment| {
            let ends = [
                coords.primary_coord(segment.from.x, segment.from.y),
                coords.primary_coord(segment.to.x, segment.to.y),
            ];
            let (low, high) = (ends[0].min(ends[1]), ends[0].max(ends[1]));
            graph.nodes.iter().any(|node| {
                let (primary, secondary) = node_spans(node, coords);
                primary.1 >= low
                    && primary.0 <= high
                    && (lane + 1 == secondary.0 || lane == secondary.1 + 1)
            })
        })
}

/// A detour leg running right beside a frame's wall reads as a second wall,
/// and leaves no room to tell which side of it the line is on; one with a
/// single blank between it and a wall on either side reads as a third (in
/// ASCII `| | |`).
fn legs_clear_of_walls(
    plan: &FallbackRoutePlan,
    graph: &Graph,
    coords: &OrientedCoords,
) -> Result<(), String> {
    let leg_axis = match coords.secondary {
        crate::orientation::Axis::Horizontal => FallbackAxis::Horizontal,
        crate::orientation::Axis::Vertical => FallbackAxis::Vertical,
    };
    for segment in plan
        .segments
        .iter()
        .filter(|segment| segment.axis == leg_axis)
    {
        let primary = coords.primary_coord(segment.from.x, segment.from.y);
        let ends = [
            coords.secondary_coord(segment.from.x, segment.from.y),
            coords.secondary_coord(segment.to.x, segment.to.y),
        ];
        let (low, high) = (ends[0].min(ends[1]), ends[0].max(ends[1]));
        let walls: Vec<usize> = graph
            .subgraphs
            .iter()
            .filter(|sg| sg.bounds.is_valid())
            .map(|sg| frame_spans(&sg.bounds, coords))
            .filter(|(_, span)| low < span.1 && high > span.0)
            .flat_map(|(walls, _)| [walls.0, walls.1])
            .collect();
        let nearest = |before: bool| {
            walls
                .iter()
                .filter(|&&wall| (wall < primary) == before && wall != primary)
                .map(|wall| wall.abs_diff(primary))
                .min()
        };
        let (before, after) = (nearest(true), nearest(false));
        if before == Some(1) || after == Some(1) || (before == Some(2) && after == Some(2)) {
            return Err(format!(
                "detour leg at {primary} runs beside a frame's wall"
            ));
        }
    }
    Ok(())
}

/// The lanes just past `intervals` on either side, a blank line away first,
/// that fit on the canvas.
fn lanes_clear_of(
    canvas: &Canvas,
    coords: &OrientedCoords,
    intervals: Vec<(usize, usize)>,
) -> Vec<usize> {
    let limit = match coords.secondary {
        crate::orientation::Axis::Horizontal => canvas.width,
        crate::orientation::Axis::Vertical => canvas.height,
    };
    let (Some(minimum), Some(maximum)) = (
        intervals.iter().map(|(start, _)| *start).min(),
        intervals.iter().map(|(_, end)| *end).max(),
    ) else {
        return Vec::new();
    };
    // A blank line between the lane and a frame's border keeps the two apart.
    let mut candidates = vec![maximum.saturating_add(2), maximum.saturating_add(1)];
    candidates.extend(
        [minimum.checked_sub(2), minimum.checked_sub(1)]
            .into_iter()
            .flatten(),
    );
    candidates
        .into_iter()
        .filter(|candidate| {
            *candidate < limit
                && intervals
                    .iter()
                    .all(|(start, end)| *candidate < *start || *candidate > *end)
        })
        .collect()
}

/// A frame's (primary, secondary) inclusive spans, border included.
fn frame_spans(
    bounds: &crate::graph::Rectangle,
    coords: &OrientedCoords,
) -> ((usize, usize), (usize, usize)) {
    let x = (bounds.x, bounds.x + bounds.width.saturating_sub(1));
    let y = (bounds.y, bounds.y + bounds.height.saturating_sub(1));
    match coords.primary {
        crate::orientation::Axis::Horizontal => (x, y),
        crate::orientation::Axis::Vertical => (y, x),
    }
}

/// Check a detour's cells as `validate_plan_cells` does, except that a
/// straight run may cross the wall of a frame holding exactly one of its
/// ends at a right angle.  Returns those crossings for the plan to claim, so
/// the border restore leaves them open.
#[allow(clippy::type_complexity)]
fn detour_frame_crossings(
    plan: &FallbackRoutePlan,
    graph: &Graph,
    canvas: &Canvas,
    source: &Node,
    target: &Node,
) -> Result<Vec<(String, &'static str, usize, usize, char)>, String> {
    let mut runs = std::collections::BTreeMap::new();
    for segment in &plan.segments {
        let (from, to) = (segment.from, segment.to);
        match segment.axis {
            FallbackAxis::Horizontal => {
                for x in from.x.min(to.x)..=from.x.max(to.x) {
                    runs.insert((x, from.y), segment);
                }
            }
            FallbackAxis::Vertical => {
                for y in from.y.min(to.y)..=from.y.max(to.y) {
                    runs.insert((from.x, y), segment);
                }
            }
        }
    }
    let turns: BTreeSet<(usize, usize)> = plan
        .corners
        .iter()
        .map(|corner| (corner.point.x, corner.point.y))
        .chain(
            plan.paints
                .iter()
                .map(|paint| (paint.point.x, paint.point.y)),
        )
        .collect();
    let wall_crossing = |x: usize, y: usize| {
        let segment = runs.get(&(x, y)).filter(|_| !turns.contains(&(x, y)))?;
        graph
            .subgraphs
            .iter()
            .filter(|sg| sg.bounds.is_valid())
            .filter(|sg| {
                graph.is_node_in_subgraph_tree(&source.id, &sg.id)
                    != graph.is_node_in_subgraph_tree(&target.id, &sg.id)
            })
            .find_map(|sg| {
                let bounds = &sg.bounds;
                let (left, top) = (bounds.x, bounds.y);
                let right = left + bounds.width.saturating_sub(1);
                let bottom = top + bounds.height.saturating_sub(1);
                let side = match segment.axis {
                    FallbackAxis::Horizontal if y > top && y < bottom => (x == left)
                        .then_some("left")
                        .or((x == right).then_some("right")),
                    FallbackAxis::Vertical if x > left && x < right => (y == top)
                        .then_some("top")
                        .or((y == bottom).then_some("bottom")),
                    _ => None,
                }?;
                Some((sg.id.clone(), side, x, y, segment.glyph))
            })
    };
    let mut crossings = Vec::new();
    for (x, y) in plan.planned_cells() {
        if canvas.fallback_route_claims_cell(x, y) {
            return Err(format!("existing fallback reservation blocks ({x},{y})"));
        }
        if graph.nodes.iter().any(|node| node_contains(node, x, y)) {
            return Err(format!("node keepout blocks planned cell at ({x},{y})"));
        }
        if canvas.get(x, y) == ' ' {
            continue;
        }
        let border = canvas
            .get_meta(x, y)
            .is_some_and(|meta| meta.owner_kind == CellOwnerKind::SubgraphBorder);
        match wall_crossing(x, y).filter(|_| border) {
            Some(crossing) => crossings.push(crossing),
            None => return Err(format!("existing canvas content blocks ({x},{y})")),
        }
    }
    Ok(crossings)
}

/// Whether the edge at `index` is blocked only on the line a fan-in gathers
/// it along, before it turns toward its target.  Drawn generically it is
/// short, if hidden, so a detour for it must be clean: clear of frame walls
/// and of other lines.
fn only_blocked_before_its_turn(graph: &Graph, canvas: &Canvas, index: usize) -> bool {
    !edge_path_is_blocked(graph, canvas, index)
}

/// A detour corner right beside a box other than its own endpoints would
/// read as a port on that box, notably in ASCII where every corner is `+`.
fn corners_clear_of_boxes(
    plan: &FallbackRoutePlan,
    graph: &Graph,
    source: &Node,
    target: &Node,
) -> Result<(), String> {
    for corner in &plan.corners {
        let (x, y) = (corner.point.x, corner.point.y);
        let touches = graph.nodes.iter().any(|node| {
            node.id != source.id
                && node.id != target.id
                && [(0, -1), (0, 1), (-1, 0), (1, 0)]
                    .iter()
                    .any(|&(dx, dy): &(isize, isize)| {
                        x.checked_add_signed(dx)
                            .zip(y.checked_add_signed(dy))
                            .is_some_and(|(nx, ny)| node_contains(node, nx, ny))
                    })
        });
        if touches {
            return Err(format!("detour corner at ({x},{y}) touches a box"));
        }
    }
    Ok(())
}

/// Whether a node on a back edge lies along the flow from `source` to
/// `target`, endpoints included.
fn span_has_back_edge_node(
    graph: &Graph,
    source: &Node,
    target: &Node,
    coords: &OrientedCoords,
) -> bool {
    let (source_primary, _) = node_spans(source, coords);
    let (target_primary, _) = node_spans(target, coords);
    let low = source_primary.0.min(target_primary.0);
    let high = source_primary.1.max(target_primary.1);
    graph.nodes.iter().any(|node| {
        let (primary, _) = node_spans(node, coords);
        primary.1 >= low
            && primary.0 <= high
            && graph
                .edges
                .iter()
                .any(|edge| edge.is_back_edge && (edge.from == node.id || edge.to == node.id))
    })
}

/// Whether the generic passes will gather other edges into the target of the
/// edge at `index` with a bus in the gap before it: the target has two such
/// edges, or one comes from a source that also fans out elsewhere.
fn target_gap_has_generic_bus(graph: &Graph, claimed: &HashSet<usize>, index: usize) -> bool {
    let generic = |other: usize, edge: &crate::graph::Edge| {
        other != index && !claimed.contains(&other) && !edge.is_back_edge
    };
    let target = &graph.edges[index].to;
    let feeders: Vec<&str> = graph
        .edges
        .iter()
        .enumerate()
        .filter(|(other, edge)| generic(*other, edge) && &edge.to == target)
        .map(|(_, edge)| edge.from.as_str())
        .collect();
    feeders.len() >= 2
        || feeders.iter().any(|feeder| {
            graph
                .edges
                .iter()
                .enumerate()
                .filter(|(other, edge)| generic(*other, edge) && edge.from == *feeder)
                .count()
                >= 2
        })
}

/// Turn a detour corner, or a cell of its run a later route of the same
/// source or target was refused, into a tee where that route continues
/// through the cell, such as the source's other stem or a branch of its bus.
pub(crate) fn settle_blocked_edge_detour_joins(canvas: &mut Canvas, style: &StyleChars) {
    let mut points = canvas.fallback_route_corner_points(DETOUR_STRATEGY);
    points.extend(canvas.fallback_route_refused_points(DETOUR_STRATEGY));
    for (x, y) in points {
        let mut connections = canvas_connections(canvas, x, y);
        // A straight run keeps both its ends, one of which may meet a box
        // border not drawn yet.
        let glyph = canvas.get(x, y);
        if super::super::canvas::is_vertical(glyph, style) {
            connections.up = true;
            connections.down = true;
        } else if super::super::canvas::is_horizontal(glyph, style) {
            connections.left = true;
            connections.right = true;
        }
        if connections.arm_count() < 3 {
            continue;
        }
        if let Some(glyph) = canonical_routing_glyph(connections, style, CellOwnerKind::EdgeSegment)
        {
            canvas.set(x, y, glyph);
        }
    }
}

/// Whether the forward edge at `index`, labelled or not, would be drawn
/// straight through another node: the line it leaves on runs straight into
/// the target's side, and some node other than its endpoints lies between
/// them along the flow and covers that line.
pub(crate) fn edge_path_is_blocked(graph: &Graph, canvas: &Canvas, index: usize) -> bool {
    source_line_crosses_a_box(graph, canvas, index, true)
}

/// Whether the forward edge at `index` leaves on a line that runs through
/// another node between its ends along the flow; with `into_target`, only
/// when that line also runs straight into the target's side.
fn source_line_crosses_a_box(
    graph: &Graph,
    canvas: &Canvas,
    index: usize,
    into_target: bool,
) -> bool {
    let Some(edge) = graph.edges.get(index) else {
        return false;
    };
    if edge.is_back_edge || edge.kind == EdgeKind::Bidirectional || edge.from == edge.to {
        return false;
    }
    let (Some(source), Some(target)) = (graph.get_node(&edge.from), graph.get_node(&edge.to))
    else {
        return false;
    };
    if !canvas.is_visible(source) || !canvas.is_visible(target) {
        return false;
    }
    let coords = OrientedCoords::new(graph.direction);
    let start = edge_exit_point(source, graph.direction);
    let end = adjusted_edge_entry_point(target, graph.direction, graph);
    if ensure_flow(&coords, start, end).is_err() {
        return false;
    }
    let (low, high) = {
        let a = coords.primary_coord(start.0, start.1);
        let b = coords.primary_coord(end.0, end.1);
        (a.min(b), a.max(b))
    };
    let line = coords.secondary_coord(start.0, start.1);
    let (_, target_secondary) = node_spans(target, &coords);
    if into_target && (line < target_secondary.0 || line > target_secondary.1) {
        return false;
    }
    line_crosses_a_box(graph, canvas, source, target, (low, high), line)
}

/// Whether the generic passes would draw the blocked edge at `index` through
/// another node's box.  A target that other generic edges also feed gathers
/// them on the sources' lines, turning only just before the target, so the
/// edge is blocked wherever its own line crosses a box; any other target is
/// reached by a branch along its own entry line, which may pass the box.
fn generic_route_is_blocked(
    graph: &Graph,
    canvas: &Canvas,
    claimed: &HashSet<usize>,
    index: usize,
) -> bool {
    let edge = &graph.edges[index];
    let fan_in = graph.edges.iter().enumerate().any(|(other, other_edge)| {
        other != index
            && !claimed.contains(&other)
            && !other_edge.is_back_edge
            && other_edge.from != other_edge.to
            && other_edge.to == edge.to
    });
    if fan_in {
        return source_line_crosses_a_box(graph, canvas, index, false);
    }
    if !edge_path_is_blocked(graph, canvas, index) {
        return false;
    }
    let (Some(source), Some(target)) = (graph.get_node(&edge.from), graph.get_node(&edge.to))
    else {
        return false;
    };
    let coords = OrientedCoords::new(graph.direction);
    let start = edge_exit_point(source, graph.direction);
    let end = adjusted_edge_entry_point(target, graph.direction, graph);
    let a = coords.primary_coord(start.0, start.1);
    let b = coords.primary_coord(end.0, end.1);
    let line = coords.secondary_coord(end.0, end.1);
    line_crosses_a_box(graph, canvas, source, target, (a.min(b), a.max(b)), line)
}

/// Whether a node other than `source` and `target`, strictly between
/// `primary` along the flow, covers secondary `line`.
fn line_crosses_a_box(
    graph: &Graph,
    canvas: &Canvas,
    source: &Node,
    target: &Node,
    (low, high): (usize, usize),
    line: usize,
) -> bool {
    let coords = OrientedCoords::new(graph.direction);
    graph.nodes.iter().any(|node| {
        if node.id == source.id || node.id == target.id || !canvas.is_visible(node) {
            return false;
        }
        let (primary, secondary) = node_spans(node, &coords);
        primary.0 > low && primary.1 < high && line >= secondary.0 && line <= secondary.1
    })
}

/// A node's (primary, secondary) inclusive spans.
fn node_spans(node: &Node, coords: &OrientedCoords) -> ((usize, usize), (usize, usize)) {
    let x = (node.x, node.x + node.width.saturating_sub(1));
    let y = (
        node.y,
        node.y + node.height.max(BOX_HEIGHT).saturating_sub(1),
    );
    match coords.primary {
        crate::orientation::Axis::Horizontal => (x, y),
        crate::orientation::Axis::Vertical => (y, x),
    }
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
    !edge.is_back_edge && edge.kind != EdgeKind::Bidirectional && edge.label.is_none()
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

    let kind_of = |index: usize| graph.edges[index].kind;
    append_standard_path(
        &mut plan,
        source,
        middle,
        graph.direction,
        graph,
        style,
        kind_of(scene.first_index),
    )?;
    append_standard_path(
        &mut plan,
        middle,
        target,
        graph.direction,
        graph,
        style,
        kind_of(scene.second_index),
    )?;

    let shortcut_entry = choose_shortcut_entry(target, graph.direction, lane, graph);
    append_shortcut_path(
        &mut plan,
        source,
        shortcut_entry,
        lane,
        1,
        graph.direction,
        style,
        kind_of(scene.direct_index),
    )?;

    validate_plan_cells(&plan, graph, canvas)?;
    if let Some(reason) = plan.validation_error(canvas.width, canvas.height) {
        return Err(reason);
    }
    Ok(plan)
}

fn choose_shortcut_lane(graph: &Graph, canvas: &Canvas, coords: &OrientedCoords) -> Option<usize> {
    shortcut_lane_candidates(graph, canvas, coords)
        .into_iter()
        .next()
}

/// Lanes outside every node, far side first.
fn shortcut_lane_candidates(graph: &Graph, canvas: &Canvas, coords: &OrientedCoords) -> Vec<usize> {
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
    let (Some(minimum), Some(maximum)) = (
        intervals.iter().map(|(start, _)| *start).min(),
        intervals.iter().map(|(_, end)| *end).max(),
    ) else {
        return Vec::new();
    };

    // Prefer the far side of the scene.  The opposite candidate remains a
    // useful fallback for a diagram whose far edge is clipped by the canvas.
    let candidates = [
        maximum.saturating_add(1),
        maximum.saturating_add(2),
        minimum.saturating_sub(1),
        minimum.saturating_sub(2),
    ];
    candidates
        .into_iter()
        .filter(|candidate| {
            *candidate < limit
                && intervals
                    .iter()
                    .all(|(start, end)| *candidate < *start || *candidate > *end)
        })
        .collect()
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

#[allow(clippy::too_many_arguments)]
fn append_standard_path(
    plan: &mut FallbackRoutePlan,
    source: &Node,
    target: &Node,
    direction: Direction,
    graph: &Graph,
    style: &StyleChars,
    kind: EdgeKind,
) -> Result<(), String> {
    let base_style = style;
    let line_style = style_for_edge_kind(base_style, kind);
    let style = &line_style;
    let coords = OrientedCoords::new(direction);
    let start = edge_exit_point(source, direction);
    let start_secondary = coords.secondary_coord(start.0, start.1);
    let end = choose_entry_for_secondary(target, direction, start_secondary, graph);
    ensure_flow(&coords, start, end)?;

    let end_secondary = coords.secondary_coord(end.0, end.1);
    if start_secondary == end_secondary {
        push_primary(plan, start, end, &coords, style);
        plan.push_paint(end.0, end.1, diamond_tip(kind, base_style, style, &coords));
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
    plan.push_paint(end.0, end.1, diamond_tip(kind, base_style, style, &coords));
    Ok(())
}

/// An outside detour that leaves `source` on its flow side, runs along `lane`
/// and enters `target` on the side facing the lane with its own end glyph.
#[allow(clippy::too_many_arguments)]
fn append_side_entry_detour(
    plan: &mut FallbackRoutePlan,
    source: &Node,
    target: &Node,
    lane: usize,
    turn_after: usize,
    coords: &OrientedCoords,
    style: &StyleChars,
    kind: EdgeKind,
) -> Result<(), String> {
    let base_style = style;
    let line_style = style_for_edge_kind(base_style, kind);
    let style = &line_style;
    let point = |primary: usize, secondary: usize| match coords.primary {
        crate::orientation::Axis::Horizontal => (primary, secondary),
        crate::orientation::Axis::Vertical => (secondary, primary),
    };
    let (target_primary, target_secondary) = node_spans(target, coords);
    let entry_primary = (target_primary.0 + target_primary.1) / 2;
    let secondary_line = match coords.secondary {
        crate::orientation::Axis::Horizontal => style.edge_h,
        crate::orientation::Axis::Vertical => style.edge_v,
    };
    let tip = |arrow: char| match kind {
        EdgeKind::CircleEnd => base_style.circle_end,
        EdgeKind::CrossEnd => base_style.cross_end,
        EdgeKind::Open => secondary_line,
        _ => arrow,
    };
    let (end_secondary, tip) = if lane > target_secondary.1 + 1 {
        let arrow = match coords.secondary {
            crate::orientation::Axis::Horizontal => base_style.arrow_left,
            crate::orientation::Axis::Vertical => base_style.arrow_up,
        };
        (target_secondary.1 + 1, tip(arrow))
    } else if lane + 2 < target_secondary.0 {
        let arrow = match coords.secondary {
            crate::orientation::Axis::Horizontal => base_style.arrow_right,
            crate::orientation::Axis::Vertical => base_style.arrow_down,
        };
        (target_secondary.0 - 1, tip(arrow))
    } else {
        return Err("side entry has no room outside the target".to_owned());
    };
    let end = point(entry_primary, end_secondary);
    let start = edge_exit_point(source, coords.direction);
    let start_turn = coords.advance(start.0, start.1, turn_after);
    if !flow_before(
        coords.direction,
        coords.primary_coord(start_turn.0, start_turn.1),
        entry_primary,
    ) {
        return Err("side entry has no room for an outside detour".to_owned());
    }
    let start_lane = coords.with_secondary(start_turn.0, start_turn.1, lane);
    let end_lane = point(entry_primary, lane);

    push_primary(plan, start, start_turn, coords, style);
    plan.push_corner(
        start_turn.0,
        start_turn.1,
        corner_glyph(start_turn, start, start_lane, style),
    );
    push_secondary(plan, start_turn, start_lane, coords, style);
    plan.push_corner(
        start_lane.0,
        start_lane.1,
        corner_glyph(start_lane, start_turn, end_lane, style),
    );
    push_primary(plan, start_lane, end_lane, coords, style);
    plan.push_corner(
        end_lane.0,
        end_lane.1,
        corner_glyph(end_lane, start_lane, end, style),
    );
    push_secondary(plan, end_lane, end, coords, style);
    plan.push_paint(end.0, end.1, tip);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn append_shortcut_path(
    plan: &mut FallbackRoutePlan,
    source: &Node,
    end: (usize, usize),
    lane: usize,
    turn_after: usize,
    direction: Direction,
    style: &StyleChars,
    kind: EdgeKind,
) -> Result<(), String> {
    let base_style = style;
    let line_style = style_for_edge_kind(base_style, kind);
    let style = &line_style;
    let coords = OrientedCoords::new(direction);
    let start = edge_exit_point(source, direction);
    ensure_flow(&coords, start, end)?;

    let start_turn = coords.advance(start.0, start.1, turn_after);
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
    plan.push_paint(end.0, end.1, diamond_tip(kind, base_style, style, &coords));
    Ok(())
}

/// The glyph that ends a diamond-scene path at its target.
fn diamond_tip(
    kind: EdgeKind,
    style: &StyleChars,
    line_style: &StyleChars,
    coords: &OrientedCoords,
) -> char {
    match kind {
        EdgeKind::CircleEnd => style.circle_end,
        EdgeKind::CrossEnd => style.cross_end,
        EdgeKind::Open => coords.primary_edge_char(line_style),
        _ => coords.arrow_end(style),
    }
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
