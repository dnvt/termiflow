//! Scene-owned routing for a strict horizontal sibling chain.
//!
//! A flat chain of titled siblings can otherwise collapse every cross-boundary
//! transition onto the internal-node row.  The result is technically
//! connected, but the middle sibling's incoming and outgoing roles read as one
//! continuous bus.  This lowerer allocates distinct quiet corridor rows for
//! the transitions, keeping the reservation transactional.  Layout supplies
//! enough lateral gap for each turned bridge to read as a transition rather
//! than a tiny box attached to two neighboring borders.

use std::collections::{BTreeSet, HashMap, HashSet};

use sha2::{Digest, Sha256};

use crate::graph::{Direction, EdgeKind, Graph, NodeShape, Rectangle};
use crate::portals::PortalSlots;
use crate::render::fallback_route::{FallbackAxis, FallbackRoutePlan, PortalEntryDecision};
use crate::render::provenance::edge_owner_id;
use crate::render::semantic::CellOwnerKind;
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::edge_primitives::{edge_entry_point, edge_exit_point, is_subgraph_title_cell};
use super::{set_route_char, set_route_edge_char, RouteOwner};
use crate::orientation::OrientedCoords;

const STRATEGY: &str = "lr-rl-sibling-chain";

#[derive(Debug, Clone)]
struct Transition {
    edge_index: usize,
    edge_id: String,
    source_subgraph_id: String,
    target_subgraph_id: String,
    source_node_id: String,
    target_node_id: String,
}

#[derive(Debug, Clone)]
struct Scene {
    transitions: Vec<Transition>,
    frame: Rectangle,
    node_y: usize,
    node_bottom: usize,
}

/// Reserve the cross-boundary edges of a strict horizontal sibling chain.
///
/// The internal edges remain on the layout/precomputed route.  This scene
/// owns only the boundary transitions, so an unsupported or cramped chain
/// falls back to the existing renderer without changing unrelated edges.
pub(crate) fn plan_lr_rl_sibling_chain_scene(
    graph: &Graph,
    canvas: &mut Canvas,
    style: &StyleChars,
    portal_slots: &mut HashMap<String, PortalSlots>,
) -> HashSet<usize> {
    let Some(scene) = detect_scene(graph) else {
        return HashSet::new();
    };
    let owner_id = scene_owner_id(&scene);
    // A strict chain is laid out on one common node row. Prefer that literal
    // topology: the cross-group edges can remain straight and do not need the
    // lower-band turns that look like a second, box-shaped subgraph wall. The
    // quiet-corridor plan remains the fail-closed fallback for future scene
    // variants whose aligned route cannot be proven collision-free.
    let mut lane_candidates = Vec::new();
    if let Some(lane) = aligned_lane(graph, &scene) {
        lane_candidates.push(vec![lane; scene.transitions.len()]);
    }
    if let Some(lanes) = choose_lanes(graph, &scene) {
        lane_candidates.push(lanes);
    }
    let Some((lanes, mut plan)) = lane_candidates.into_iter().find_map(|lanes| {
        let plan = build_plan(graph, &scene, &lanes, canvas.width, canvas.height, style)?;
        if plan.validation_error(canvas.width, canvas.height).is_some()
            || plan_blocker(&plan, graph, canvas).is_some()
        {
            return None;
        }
        Some((lanes, plan))
    }) else {
        return reject_scene(canvas, &owner_id, "no collision-free horizontal chain plan");
    };

    let owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: owner_id.as_str(),
    };
    let baseline = canvas.clone();
    let mut simulation = baseline.clone();
    simulation.set_write_stage("lr-rl-sibling-chain-simulation");
    lower_plan(plan.clone(), &mut simulation, style, owner);
    let Some(trace) = simulation
        .fallback_route_traces()
        .into_iter()
        .find(|trace| trace.owner_id == owner_id)
    else {
        return reject_scene(canvas, &owner_id, "chain simulation did not record a route");
    };
    if !trace.mismatches.is_empty() {
        return reject_scene(canvas, &owner_id, "chain simulation has route mismatches");
    }

    let mut occupied = BTreeSet::new();
    for (transition, lane) in scene.transitions.iter().zip(&lanes) {
        let cells = transition_cells(graph, transition, *lane).unwrap_or_default();
        if cells.iter().any(|cell| !occupied.insert(*cell)) {
            return reject_scene(canvas, &owner_id, "sibling chain rails share a route cell");
        }
    }

    if scene.transitions.iter().any(|transition| {
        !portal_slots.contains_key(&transition.source_subgraph_id)
            || !portal_slots.contains_key(&transition.target_subgraph_id)
    }) {
        return reject_scene(canvas, &owner_id, "sibling chain portal slots disappeared");
    }

    for (transition, lane) in scene.transitions.iter().zip(&lanes) {
        let Some(source_slots) = portal_slots.get_mut(&transition.source_subgraph_id) else {
            return reject_scene(canvas, &owner_id, "source sibling has no portal slots");
        };
        replace_side_lane(source_slots, graph.direction, true, *lane);
        let Some(target_slots) = portal_slots.get_mut(&transition.target_subgraph_id) else {
            return reject_scene(canvas, &owner_id, "target sibling has no portal slots");
        };
        replace_side_lane(target_slots, graph.direction, false, *lane);
    }

    plan.set_contract_digest(Some(chain_digest(&scene, &lanes)));
    canvas.set_write_stage("lr-rl-sibling-chain");
    lower_plan(plan, canvas, style, owner);
    scene
        .transitions
        .iter()
        .map(|transition| transition.edge_index)
        .collect()
}

fn detect_scene(graph: &Graph) -> Option<Scene> {
    if !matches!(graph.direction, Direction::LR | Direction::RL)
        || graph.subgraphs.len() < 3
        || graph.has_cycles()
        || graph.edges.iter().any(|edge| edge.is_back_edge)
    {
        return None;
    }

    let mut physical = graph.subgraphs.iter().collect::<Vec<_>>();
    physical.sort_by_key(|subgraph| (subgraph.bounds.x, subgraph.id.as_str()));
    if physical
        .windows(2)
        .any(|pair| pair[0].bounds.x.saturating_add(pair[0].bounds.width) > pair[1].bounds.x)
    {
        return None;
    }

    let frame = physical.first()?.bounds.clone();
    if !frame.is_valid()
        || physical.iter().any(|subgraph| {
            !subgraph.bounds.is_valid()
                || subgraph.parent_id.is_some()
                || !subgraph.child_ids.is_empty()
                || subgraph.title.is_none()
                || subgraph.node_ids.len() != 2
                || subgraph.bounds.y != frame.y
                || subgraph.bounds.height != frame.height
        })
    {
        return None;
    }

    let mut node_to_subgraph = HashMap::new();
    let mut common_node_y = None;
    let mut common_node_bottom = None;
    for subgraph in &physical {
        for node_id in &subgraph.node_ids {
            let node = graph.get_node(node_id)?;
            if node.shape != NodeShape::Rectangle
                || graph.get_node_subgraph(node_id) != Some(subgraph.id.as_str())
            {
                return None;
            }
            if common_node_y.replace(node.y).is_some_and(|y| y != node.y)
                || common_node_bottom
                    .replace(node.bottom_y())
                    .is_some_and(|bottom| bottom != node.bottom_y())
            {
                return None;
            }
            node_to_subgraph.insert(node_id.as_str(), subgraph.id.as_str());
        }
    }
    if node_to_subgraph.len() != graph.nodes.len() {
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
    if ordinary_edges.len() != graph.edges.len()
        || ordinary_edges.len() != physical.len().saturating_mul(2).saturating_sub(1)
    {
        return None;
    }

    for subgraph in &physical {
        let internal = ordinary_edges
            .iter()
            .filter(|(_, edge)| {
                node_to_subgraph.get(edge.from.as_str()) == Some(&subgraph.id.as_str())
                    && node_to_subgraph.get(edge.to.as_str()) == Some(&subgraph.id.as_str())
            })
            .collect::<Vec<_>>();
        if internal.len() != 1 {
            return None;
        }
        let edge = internal[0].1;
        let source = graph.get_node(&edge.from)?;
        let target = graph.get_node(&edge.to)?;
        let ordered = match graph.direction {
            Direction::LR => source.center_x() < target.center_x(),
            Direction::RL => source.center_x() > target.center_x(),
            _ => false,
        };
        if !ordered {
            return None;
        }
    }

    let flow = if graph.direction == Direction::LR {
        physical.clone()
    } else {
        physical.iter().rev().copied().collect()
    };
    let mut transitions = Vec::with_capacity(flow.len().saturating_sub(1));
    for pair in flow.windows(2) {
        let source_subgraph = pair[0];
        let target_subgraph = pair[1];
        let candidates = ordinary_edges
            .iter()
            .filter(|(_, edge)| {
                node_to_subgraph.get(edge.from.as_str()) == Some(&source_subgraph.id.as_str())
                    && node_to_subgraph.get(edge.to.as_str()) == Some(&target_subgraph.id.as_str())
                    && graph.edge_boundary_crossings(&edge.from, &edge.to).0
                        == vec![source_subgraph.id.as_str()]
                    && graph.edge_boundary_crossings(&edge.from, &edge.to).1
                        == vec![target_subgraph.id.as_str()]
            })
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            return None;
        }
        let (edge_index, edge) = *candidates[0];
        transitions.push(Transition {
            edge_index,
            edge_id: edge_owner_id(edge_index, edge),
            source_subgraph_id: source_subgraph.id.clone(),
            target_subgraph_id: target_subgraph.id.clone(),
            source_node_id: edge.from.clone(),
            target_node_id: edge.to.clone(),
        });
    }

    let crossing_count = ordinary_edges
        .iter()
        .filter(|(_, edge)| {
            node_to_subgraph.get(edge.from.as_str()) != node_to_subgraph.get(edge.to.as_str())
        })
        .count();
    if crossing_count != transitions.len() {
        return None;
    }

    Some(Scene {
        transitions,
        frame,
        node_y: common_node_y?,
        node_bottom: common_node_bottom?,
    })
}

fn choose_lanes(graph: &Graph, scene: &Scene) -> Option<Vec<usize>> {
    let frame_bottom = scene
        .frame
        .y
        .checked_add(scene.frame.height.saturating_sub(1))?;
    if scene.node_y <= scene.frame.y.saturating_add(1)
        || scene.node_bottom >= frame_bottom
        || scene.node_y >= scene.node_bottom
    {
        return None;
    }

    let title_safe = |row: usize| {
        graph.subgraphs.iter().all(|subgraph| {
            !(subgraph.bounds.x
                ..=subgraph
                    .bounds
                    .x
                    .saturating_add(subgraph.bounds.width.saturating_sub(1)))
                .any(|x| is_subgraph_title_cell(graph, x, row))
        })
    };
    let mut candidates = Vec::new();
    // Prefer the lower quiet band, leaving one genuinely empty row after the
    // node border. Starting at `node_bottom` makes the first corridor touch the
    // node-adjacent composition and can render as a tiny box-attached corner at
    // each sibling seam. Fall back to the upper band only when a longer chain
    // needs more lanes than the lower band provides.
    candidates
        .extend((scene.node_bottom.saturating_add(1)..frame_bottom).filter(|row| title_safe(*row)));
    candidates.extend(
        (scene.frame.y.saturating_add(1)..scene.node_y)
            .rev()
            .filter(|row| title_safe(*row)),
    );
    (candidates.len() >= scene.transitions.len()).then(|| {
        candidates
            .into_iter()
            .take(scene.transitions.len())
            .collect()
    })
}

fn aligned_lane(graph: &Graph, scene: &Scene) -> Option<usize> {
    let first = scene.transitions.first()?;
    let lane = edge_exit_point(graph.get_node(&first.source_node_id)?, graph.direction).1;
    if scene.transitions.iter().all(|transition| {
        let Some(source) = graph.get_node(&transition.source_node_id) else {
            return false;
        };
        let Some(target) = graph.get_node(&transition.target_node_id) else {
            return false;
        };
        edge_exit_point(source, graph.direction).1 == lane
            && edge_entry_point(target, graph.direction).1 == lane
    }) {
        Some(lane)
    } else {
        None
    }
}

fn build_plan(
    graph: &Graph,
    scene: &Scene,
    lanes: &[usize],
    width: usize,
    height: usize,
    style: &StyleChars,
) -> Option<FallbackRoutePlan> {
    let owner_id = scene_owner_id(scene);
    let mut plan = FallbackRoutePlan::new(owner_id.clone(), STRATEGY);
    plan.set_scene_coverage(
        scene
            .transitions
            .iter()
            .map(|transition| transition.edge_id.clone()),
    );
    let coords = OrientedCoords::new(graph.direction);
    let mut occupied = BTreeSet::new();

    for (transition, lane) in scene.transitions.iter().zip(lanes) {
        let source = graph.get_node(&transition.source_node_id)?;
        let target = graph.get_node(&transition.target_node_id)?;
        let source_subgraph = graph.get_subgraph(&transition.source_subgraph_id)?;
        let target_subgraph = graph.get_subgraph(&transition.target_subgraph_id)?;
        let source_boundary_x = boundary_x(&source_subgraph.bounds, graph.direction, true);
        let target_boundary_x = boundary_x(&target_subgraph.bounds, graph.direction, false);
        let source_inner_x = inside_x(source_boundary_x, graph.direction, true);
        let target_inner_x = inside_x(target_boundary_x, graph.direction, false);
        let source_outside_x = outside_x(source_boundary_x, graph.direction, true)?;
        let target_outside_x = outside_x(target_boundary_x, graph.direction, false)?;
        let source_exit = edge_exit_point(source, graph.direction);
        let target_entry = edge_entry_point(target, graph.direction);
        if source_exit.1 >= height
            || target_entry.1 >= height
            || source_exit.0 >= width
            || target_entry.0 >= width
            || !primary_order(graph.direction, source_exit.0, source_inner_x)
            || !primary_order(graph.direction, target_inner_x, target_entry.0)
            || source_boundary_x >= width
            || target_boundary_x >= width
            || *lane >= height
        {
            return None;
        }

        let mut edge_plan = FallbackRoutePlan::new(owner_id.clone(), STRATEGY);
        let source_side = side_name(graph.direction, true);
        let target_side = side_name(graph.direction, false);
        edge_plan.claim_boundary(
            source_subgraph.id.clone(),
            source_side,
            source_boundary_x,
            *lane,
            style.edge_h,
        );
        edge_plan.claim_boundary(
            target_subgraph.id.clone(),
            target_side,
            target_boundary_x,
            *lane,
            style.edge_h,
        );
        edge_plan.set_target_entry_decision(PortalEntryDecision {
            edge_id: transition.edge_id.clone(),
            owner_id: owner_id.clone(),
            target_node_id: target.id.clone(),
            boundary_id: target_subgraph.id.clone(),
            side: target_side.to_owned(),
            portal_x: target_boundary_x,
            portal_y: *lane,
            arrow_x: target_entry.0,
            arrow_y: target_entry.1,
        });

        edge_plan.push_horizontal(source_exit.1, source_exit.0, source_inner_x, style.edge_h);
        if source_exit.1 != *lane {
            let going_before = source_exit.1 > *lane;
            edge_plan.push_corner(
                source_inner_x,
                source_exit.1,
                coords.corner_start_to_secondary(going_before, style),
            );
            edge_plan.push_vertical(source_inner_x, source_exit.1, *lane, style.edge_v);
            edge_plan.push_corner(
                source_inner_x,
                *lane,
                coords.corner_secondary_to_end(going_before, style),
            );
        }
        edge_plan.push_horizontal(*lane, source_inner_x, source_boundary_x, style.edge_h);
        edge_plan.push_horizontal(*lane, source_boundary_x, target_boundary_x, style.edge_h);
        edge_plan.push_horizontal(*lane, target_boundary_x, target_inner_x, style.edge_h);
        if target_entry.1 != *lane {
            let going_before = *lane > target_entry.1;
            edge_plan.push_corner(
                target_inner_x,
                *lane,
                coords.corner_start_to_secondary(going_before, style),
            );
            edge_plan.push_vertical(target_inner_x, *lane, target_entry.1, style.edge_v);
            edge_plan.push_corner(
                target_inner_x,
                target_entry.1,
                coords.corner_secondary_to_end(going_before, style),
            );
        }
        edge_plan.push_horizontal(target_entry.1, target_inner_x, target_entry.0, style.edge_h);
        edge_plan.push_paint(target_entry.0, target_entry.1, coords.arrow_end(style));

        let cells = edge_plan.planned_cells();
        if cells.iter().any(|cell| !occupied.insert(*cell)) {
            return None;
        }
        plan.segments.extend(edge_plan.segments);
        plan.corners.extend(edge_plan.corners);
        plan.paints.extend(edge_plan.paints);
        plan.boundary_claims.extend(edge_plan.boundary_claims);
        plan.entry_decisions.extend(edge_plan.entry_decisions);

        // Keep the outside points in the plan even when a one-cell gap makes
        // them coincide with a boundary-adjacent horizontal segment.  They
        // are explicit topology checkpoints for the blocker below.
        let _ = (source_outside_x, target_outside_x);
    }

    Some(plan)
}

fn transition_cells(
    graph: &Graph,
    transition: &Transition,
    lane: usize,
) -> Option<BTreeSet<(usize, usize)>> {
    let source = graph.get_node(&transition.source_node_id)?;
    let target = graph.get_node(&transition.target_node_id)?;
    let source_subgraph = graph.get_subgraph(&transition.source_subgraph_id)?;
    let target_subgraph = graph.get_subgraph(&transition.target_subgraph_id)?;
    let source_boundary_x = boundary_x(&source_subgraph.bounds, graph.direction, true);
    let target_boundary_x = boundary_x(&target_subgraph.bounds, graph.direction, false);
    let source_inner_x = inside_x(source_boundary_x, graph.direction, true);
    let target_inner_x = inside_x(target_boundary_x, graph.direction, false);
    let source_exit = edge_exit_point(source, graph.direction);
    let target_entry = edge_entry_point(target, graph.direction);
    let mut cells = BTreeSet::new();
    add_horizontal(&mut cells, source_exit.0, source_inner_x, source_exit.1);
    add_vertical(&mut cells, source_inner_x, source_exit.1, lane);
    add_horizontal(&mut cells, source_inner_x, source_boundary_x, lane);
    add_horizontal(&mut cells, source_boundary_x, target_boundary_x, lane);
    add_horizontal(&mut cells, target_boundary_x, target_inner_x, lane);
    add_vertical(&mut cells, target_inner_x, lane, target_entry.1);
    add_horizontal(&mut cells, target_inner_x, target_entry.0, target_entry.1);
    cells.insert(target_entry);
    Some(cells)
}

fn add_horizontal(cells: &mut BTreeSet<(usize, usize)>, x1: usize, x2: usize, y: usize) {
    for x in x1.min(x2)..=x1.max(x2) {
        cells.insert((x, y));
    }
}

fn add_vertical(cells: &mut BTreeSet<(usize, usize)>, x: usize, y1: usize, y2: usize) {
    for y in y1.min(y2)..=y1.max(y2) {
        cells.insert((x, y));
    }
}

fn plan_blocker(plan: &FallbackRoutePlan, graph: &Graph, canvas: &Canvas) -> Option<String> {
    let claims = plan
        .boundary_claims
        .iter()
        .map(|claim| (claim.x, claim.y))
        .collect::<HashSet<_>>();
    let arrows = plan
        .paints
        .iter()
        .map(|paint| (paint.point.x, paint.point.y))
        .collect::<HashSet<_>>();
    let source_exits = graph
        .edges
        .iter()
        .enumerate()
        .filter(|(index, edge)| {
            plan.covered_edge_ids
                .iter()
                .any(|edge_id| edge_id == &edge_owner_id(*index, edge))
        })
        .filter_map(|(_, edge)| graph.get_node(&edge.from))
        .map(|node| edge_exit_point(node, graph.direction))
        .collect::<HashSet<_>>();

    for (x, y) in plan.planned_cells() {
        if is_subgraph_title_cell(graph, x, y) {
            return Some(format!("route crosses subgraph title at ({x},{y})"));
        }
        let allowed_endpoint =
            claims.contains(&(x, y)) || arrows.contains(&(x, y)) || source_exits.contains(&(x, y));
        if !allowed_endpoint
            && graph.nodes.iter().any(|node| {
                x >= node.x
                    && x < node.x.saturating_add(node.width)
                    && y >= node.y
                    && y < node.bottom_y()
            })
        {
            return Some(format!("route enters node at ({x},{y})"));
        }
        if !allowed_endpoint && canvas.get(x, y) != ' ' && !claims.contains(&(x, y)) {
            return Some(format!("route cell ({x},{y}) is occupied at ({x},{y})"));
        }
    }
    None
}

fn lower_plan(
    plan: FallbackRoutePlan,
    canvas: &mut Canvas,
    style: &StyleChars,
    owner: RouteOwner<'_>,
) {
    canvas.record_fallback_route_plan(plan.clone());
    for segment in &plan.segments {
        match segment.axis {
            FallbackAxis::Horizontal => {
                for x in segment.from.x.min(segment.to.x)..=segment.from.x.max(segment.to.x) {
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
                for y in segment.from.y.min(segment.to.y)..=segment.from.y.max(segment.to.y) {
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
            corner.glyph,
            Some(owner),
        );
    }
    for claim in &plan.boundary_claims {
        set_route_char(canvas, claim.x, claim.y, claim.expected_glyph, Some(owner));
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
}

fn boundary_x(bounds: &Rectangle, direction: Direction, source: bool) -> usize {
    match (direction, source) {
        (Direction::LR, true) => bounds.x.saturating_add(bounds.width.saturating_sub(1)),
        (Direction::LR, false) => bounds.x,
        (Direction::RL, true) => bounds.x,
        (Direction::RL, false) => bounds.x.saturating_add(bounds.width.saturating_sub(1)),
        _ => unreachable!("horizontal sibling chain is direction-gated"),
    }
}

fn inside_x(boundary_x: usize, direction: Direction, source: bool) -> usize {
    match (direction, source) {
        (Direction::LR, true) => boundary_x.saturating_sub(1),
        (Direction::LR, false) => boundary_x.saturating_add(1),
        (Direction::RL, true) => boundary_x.saturating_add(1),
        (Direction::RL, false) => boundary_x.saturating_sub(1),
        _ => unreachable!("horizontal sibling chain is direction-gated"),
    }
}

fn outside_x(boundary_x: usize, direction: Direction, source: bool) -> Option<usize> {
    match (direction, source) {
        (Direction::LR, true) | (Direction::RL, false) => boundary_x.checked_add(1),
        (Direction::LR, false) | (Direction::RL, true) => boundary_x.checked_sub(1),
        _ => None,
    }
}

fn primary_order(direction: Direction, left: usize, right: usize) -> bool {
    match direction {
        Direction::LR => left <= right,
        Direction::RL => left >= right,
        _ => false,
    }
}

fn side_name(direction: Direction, source: bool) -> &'static str {
    match (direction, source) {
        (Direction::LR, true) | (Direction::RL, false) => "right",
        (Direction::LR, false) | (Direction::RL, true) => "left",
        _ => unreachable!("horizontal sibling chain is direction-gated"),
    }
}

fn replace_side_lane(slots: &mut PortalSlots, direction: Direction, source: bool, lane: usize) {
    let side = side_name(direction, source);
    let target = match side {
        "left" => &mut slots.left,
        "right" => &mut slots.right,
        _ => unreachable!("known horizontal side"),
    };
    target.clear();
    target.insert(lane);
}

fn scene_owner_id(scene: &Scene) -> String {
    format!(
        "scene:{STRATEGY}:{}->{}",
        scene
            .transitions
            .first()
            .map(|transition| transition.source_subgraph_id.as_str())
            .unwrap_or("source"),
        scene
            .transitions
            .last()
            .map(|transition| transition.target_subgraph_id.as_str())
            .unwrap_or("target")
    )
}

fn chain_digest(scene: &Scene, lanes: &[usize]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(STRATEGY.as_bytes());
    for (transition, lane) in scene.transitions.iter().zip(lanes) {
        hasher.update(b"|");
        hasher.update(transition.edge_id.as_bytes());
        hasher.update(b":");
        hasher.update(lane.to_string().as_bytes());
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn reject_scene(canvas: &mut Canvas, owner_id: &str, reason: &str) -> HashSet<usize> {
    canvas.record_fallback_route_rejection(owner_id, STRATEGY, reason);
    HashSet::new()
}

#[cfg(all(test, feature = "maintainer-fixtures"))]
mod tests {
    use std::collections::BTreeSet;

    use super::{choose_lanes, detect_scene};

    #[test]
    fn strict_horizontal_chain_selector_is_direction_gated() {
        for (fixture, expected) in [
            ("collision_sibling_triple_lr.md", true),
            ("collision_sibling_triple_rl.md", true),
            ("collision_sibling_triple_bt.md", false),
            ("collision_sibling_subgraphs_rl.md", false),
        ] {
            let input = std::fs::read_to_string(format!("tests/fixtures/inputs/{fixture}"))
                .expect("fixture exists");
            let parsed = crate::parser::parse(&input, false).expect("fixture parses");
            let (graph, _) = crate::layout::apply_coarse_layout_with_contract(
                parsed.graph,
                None,
                crate::layout::CoarseLayoutConfig::default(),
            )
            .expect("fixture layout");
            let scene = detect_scene(&graph);
            assert_eq!(scene.is_some(), expected, "fixture={fixture}");
            if let Some(scene) = scene {
                let lanes = choose_lanes(&graph, &scene).expect("strict chain lanes");
                assert_eq!(lanes.len(), 2);
                assert!(
                    lanes.iter().all(|lane| *lane > scene.node_bottom),
                    "strict horizontal chain must leave one empty row after the node border: node_bottom={} lanes={lanes:?}",
                    scene.node_bottom
                );
                if fixture.contains("triple_") {
                    assert_eq!(
                        lanes.iter().collect::<BTreeSet<_>>().len(),
                        lanes.len(),
                        "collinear strict horizontal transitions need distinct corridor rows: lanes={lanes:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn strict_horizontal_sibling_chain_prefers_direct_aligned_transitions() {
        for fixture in [
            "collision_sibling_triple_lr.md",
            "collision_sibling_triple_rl.md",
        ] {
            let input = std::fs::read_to_string(format!("tests/fixtures/inputs/{fixture}"))
                .expect("read strict horizontal sibling-chain fixture");

            for style in [crate::BaseStyle::Ascii, crate::BaseStyle::Unicode] {
                for optimized in [false, true] {
                    let outcome = crate::render_with_feedback(
                        &input,
                        crate::RenderOptions::new()
                            .with_style(style)
                            .with_optimize_render(optimized),
                    )
                    .expect("render strict horizontal sibling-chain fixture");
                    let traces: Vec<_> = outcome
                        .portal_trace
                        .fallback_routes_for_test()
                        .iter()
                        .filter(|trace| trace.strategy == super::STRATEGY)
                        .collect();
                    assert_eq!(
                        traces.len(),
                        1,
                        "strict horizontal chain must be transactional for {fixture} {style:?} optimized={optimized}:\n{}",
                        outcome.output
                    );
                    let trace = traces[0];
                    assert!(trace.mismatches.is_empty());
                    let lanes: BTreeSet<_> =
                        trace.boundary_claims.iter().map(|claim| claim.y).collect();
                    assert_eq!(
                        lanes.len(),
                        1,
                        "aligned strict horizontal transitions should stay on one direct row for {fixture} {style:?} optimized={optimized}"
                    );
                    assert_eq!(trace.entry_decisions.len(), 2);
                    assert!(trace.contract_digest.is_some());
                    assert!(
                        outcome.critic_report.findings.is_empty(),
                        "strict horizontal chain retained critic findings for {fixture} {style:?} optimized={optimized}: {:?}\n{}",
                        outcome.critic_report.findings,
                        outcome.output
                    );
                    match style {
                        crate::BaseStyle::Ascii => assert!(
                            !outcome.output.contains("+<-+"),
                            "strict horizontal chain retained a compact ASCII boundary elbow for {fixture} optimized={optimized}:\n{}",
                            outcome.output
                        ),
                        crate::BaseStyle::Unicode => assert!(
                            !outcome.output.contains("├←─┐"),
                            "strict horizontal chain retained a compact Unicode boundary elbow for {fixture} optimized={optimized}:\n{}",
                            outcome.output
                        ),
                        _ => unreachable!("focused sibling-chain test only uses ASCII and Unicode"),
                    }
                    assert!(
                        !outcome.output.contains("└──────────────────┘"),
                        "strict horizontal chain retained a lower-corridor capsule for {fixture} {style:?} optimized={optimized}:\n{}",
                        outcome.output
                    );
                    for title in ["Group 1", "Group 2", "Group 3"] {
                        assert!(outcome.output.contains(title));
                    }
                }
            }
        }
    }

    #[test]
    fn strict_horizontal_sibling_chain_is_not_a_generic_fixture_match() {
        for fixture in [
            "collision_sibling_triple_bt.md",
            "collision_sibling_triple_td.md",
            "collision_sibling_subgraphs_rl.md",
        ] {
            let input = std::fs::read_to_string(format!("tests/fixtures/inputs/{fixture}"))
                .expect("read negative-control fixture");
            let outcome = crate::render_with_feedback(
                &input,
                crate::RenderOptions::new().with_style(crate::BaseStyle::Unicode),
            )
            .expect("render negative-control fixture");
            assert!(
                outcome
                    .portal_trace
                    .fallback_routes_for_test()
                    .iter()
                    .all(|trace| trace.strategy != super::STRATEGY),
                "strict horizontal chain leaked into negative control {fixture}:\n{}",
                outcome.output
            );
        }
    }
}
