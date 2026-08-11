//! Boundary-owned lanes for the strict subgraph fan-in scene.
//!
//! Ordinary convergence merges several source edges inside the subgraph and
//! leaves one portal. This bounded policy keeps the target arrow shared when
//! the target has no legal multi-port geometry, but preserves one source-owned
//! lane through the source boundary before the exterior collector.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::geom::Rect;
use crate::graph::{Direction, Graph, Node};
use crate::portals::{strict_simple_subgraph_fanin_lanes, PortalSlots};
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::edge::edge_primitives::{
    adjusted_edge_entry_point, draw_line_primary, draw_line_secondary, edge_entry_candidates,
    edge_exit_point, hits_foreign_subgraph_border, is_subgraph_title_cell,
};
use super::super::fallback_route::{FallbackRoutePlan, PortalEntryDecision};
use super::super::provenance::edge_owner_id;
use super::super::semantic::CellOwnerKind;
use super::super::subgraph_fan_in_identity;
use super::{set_route_char, RouteOwner};

const STRATEGY: &str = "strict-subgraph-boundary-fan-in-lanes";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BoundarySide {
    Top,
    Bottom,
    Left,
    Right,
}

impl BoundarySide {
    fn name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

#[derive(Debug, Clone)]
struct SceneEdge {
    index: usize,
    source: Node,
    lane: usize,
    owner_id: String,
}

#[derive(Debug)]
struct BoundaryFanInScene {
    subgraph_id: String,
    target: Node,
    edges: Vec<SceneEdge>,
    lanes: Vec<usize>,
    side: BoundarySide,
}

#[derive(Debug)]
struct BoundaryFanInPlan {
    route_plan: FallbackRoutePlan,
    target_routes: Vec<TargetRoute>,
}

#[derive(Debug, Clone)]
struct TargetRoute {
    edge_owner_id: String,
    source_exit: (usize, usize),
    portal: (usize, usize),
    outside: (usize, usize),
    turn: (usize, usize),
    target_turn: (usize, usize),
    target_entry: (usize, usize),
}

/// Plan and lower the strict boundary fan-in scene. A topology match that
/// cannot be routed is recorded and left for generic convergence.
pub(crate) fn plan_boundary_fan_in_scene(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    canvas: &mut Canvas,
    style: &StyleChars,
    portal_slots: &mut HashMap<String, PortalSlots>,
) -> HashSet<usize> {
    let Some(scene) = detect_scene(graph, node_rects) else {
        return HashSet::new();
    };
    let Some(plan) = build_plan(&scene, graph, canvas, style) else {
        canvas.record_fallback_route_rejection(
            scene_owner_id(&scene),
            STRATEGY,
            "strict boundary fan-in scene has no collision-free exterior collector",
        );
        return HashSet::new();
    };
    if let Some(reason) = plan
        .route_plan
        .validation_error(canvas.width, canvas.height)
    {
        canvas.record_fallback_route_rejection(scene_owner_id(&scene), STRATEGY, reason);
        return HashSet::new();
    }
    if let Some(reason) = canvas_blocker(&scene, &plan.route_plan, graph, canvas) {
        canvas.record_fallback_route_rejection(scene_owner_id(&scene), STRATEGY, reason);
        return HashSet::new();
    }

    let Some(slots) = portal_slots.get_mut(&scene.subgraph_id) else {
        canvas.record_fallback_route_rejection(
            scene_owner_id(&scene),
            STRATEGY,
            "strict boundary fan-in scene has no portal slot record",
        );
        return HashSet::new();
    };
    match scene.side {
        BoundarySide::Top => {
            slots.top.clear();
            slots.top.extend(scene.lanes.iter().copied());
        }
        BoundarySide::Bottom => {
            slots.bottom.clear();
            slots.bottom.extend(scene.lanes.iter().copied());
        }
        BoundarySide::Left => {
            slots.left.clear();
            slots.left.extend(scene.lanes.iter().copied());
        }
        BoundarySide::Right => {
            slots.right.clear();
            slots.right.extend(scene.lanes.iter().copied());
        }
    }

    lower_plan(&scene, plan, graph, canvas, style);
    scene.edges.iter().map(|edge| edge.index).collect()
}

fn detect_scene(graph: &Graph, node_rects: &HashMap<String, Rect>) -> Option<BoundaryFanInScene> {
    let subgraph = graph.subgraphs.first()?;
    if graph.subgraphs.len() != 1 || !subgraph.bounds.is_valid() {
        return None;
    }

    let direction = graph.direction;
    let (target, mut edges, lanes) = graph.nodes.iter().find_map(|target| {
        let lanes = strict_simple_subgraph_fanin_lanes(
            graph,
            node_rects,
            &target.id,
            &subgraph.id,
            direction,
        )?;
        let mut edges = graph
            .edges
            .iter()
            .enumerate()
            .filter(|(_, edge)| {
                edge.to == target.id
                    && graph.get_node_subgraph(&edge.from) == Some(subgraph.id.as_str())
            })
            .filter_map(|(index, edge)| {
                let source = graph.get_node(&edge.from)?.clone();
                Some(SceneEdge {
                    index,
                    lane: secondary_coord(&source, node_rects, direction),
                    source,
                    owner_id: edge_owner_id(index, edge),
                })
            })
            .collect::<Vec<_>>();
        edges.sort_by_key(|edge| (edge.lane, edge.source.id.clone(), edge.index));
        if edges.len() != lanes.len()
            || edges.iter().map(|edge| edge.lane).collect::<Vec<_>>() != lanes
        {
            return None;
        }
        Some((target.clone(), edges, lanes))
    })?;
    edges.shrink_to_fit();

    Some(BoundaryFanInScene {
        subgraph_id: subgraph.id.clone(),
        target,
        edges,
        lanes,
        side: match direction {
            Direction::TD | Direction::TB => BoundarySide::Bottom,
            Direction::BT => BoundarySide::Top,
            Direction::LR => BoundarySide::Right,
            Direction::RL => BoundarySide::Left,
        },
    })
}

fn secondary_coord(node: &Node, node_rects: &HashMap<String, Rect>, direction: Direction) -> usize {
    node_rects
        .get(&node.id)
        .map(|rect| match direction {
            Direction::LR | Direction::RL => rect.y + rect.height / 2,
            Direction::TD | Direction::TB | Direction::BT => rect.x + rect.width / 2,
        })
        .unwrap_or_else(|| match direction {
            Direction::LR | Direction::RL => node.center_y(),
            Direction::TD | Direction::TB | Direction::BT => node.center_x(),
        })
}

fn scene_owner_id(scene: &BoundaryFanInScene) -> String {
    format!("scene:{STRATEGY}:{}:{}", scene.subgraph_id, scene.target.id)
}

fn boundary_point(scene: &BoundaryFanInScene, lane: usize, graph: &Graph) -> (usize, usize) {
    let bounds = &graph
        .get_subgraph(&scene.subgraph_id)
        .expect("detected boundary fan-in subgraph remains present")
        .bounds;
    match scene.side {
        BoundarySide::Top => (lane, bounds.y),
        BoundarySide::Bottom => (
            lane,
            bounds.y.saturating_add(bounds.height.saturating_sub(1)),
        ),
        BoundarySide::Left => (bounds.x, lane),
        BoundarySide::Right => (
            bounds.x.saturating_add(bounds.width.saturating_sub(1)),
            lane,
        ),
    }
}

fn build_plan(
    scene: &BoundaryFanInScene,
    graph: &Graph,
    canvas: &Canvas,
    style: &StyleChars,
) -> Option<BoundaryFanInPlan> {
    if subgraph_fan_in_identity::target_port_count(graph, &scene.target.id).is_some() {
        return build_identity_plan(scene, graph, canvas, style);
    }

    build_collector_plan(scene, graph, canvas, style).map(|route_plan| BoundaryFanInPlan {
        route_plan,
        target_routes: Vec::new(),
    })
}

fn build_identity_plan(
    scene: &BoundaryFanInScene,
    graph: &Graph,
    canvas: &Canvas,
    style: &StyleChars,
) -> Option<BoundaryFanInPlan> {
    let count = subgraph_fan_in_identity::target_port_count(graph, &scene.target.id)?;
    if count != scene.edges.len() {
        return None;
    }
    let target_entries =
        subgraph_fan_in_identity::target_entry_points(&scene.target, graph.direction, count);
    if target_entries.len() != scene.edges.len() {
        return None;
    }

    let coords = crate::orientation::OrientedCoords::new(graph.direction);
    let owner_id = scene_owner_id(scene);
    let mut plan = FallbackRoutePlan::new(owner_id.clone(), STRATEGY);
    plan.set_scene_coverage(scene.edges.iter().map(|edge| edge.owner_id.clone()));
    let mut target_routes = Vec::with_capacity(scene.edges.len());
    let mut occupied: BTreeMap<(usize, usize), usize> = BTreeMap::new();

    for (index, (edge, target_entry)) in scene.edges.iter().zip(target_entries).enumerate() {
        let source_exit = edge_exit_point(&edge.source, graph.direction);
        let portal = boundary_point(scene, edge.lane, graph);
        if coords.secondary_coord(source_exit.0, source_exit.1) != edge.lane {
            return None;
        }
        let outside = coords.advance(portal.0, portal.1, 1);
        let (turn, target_turn) =
            identity_turns(outside, target_entry, index, graph.direction, &coords)?;

        let route_cells = primary_cells(source_exit, portal, graph.direction)
            .into_iter()
            .chain(primary_cells(outside, turn, graph.direction))
            .chain(secondary_cells(turn, target_turn, graph.direction))
            .chain(primary_cells(target_turn, target_entry, graph.direction))
            .collect::<BTreeSet<_>>();
        for cell in route_cells.iter().copied() {
            if let Some(previous) = occupied.insert(cell, index) {
                if previous != index {
                    return None;
                }
            }
        }

        push_primary(&mut plan, graph.direction, source_exit, portal, style);
        push_primary(&mut plan, graph.direction, outside, turn, style);
        push_secondary(&mut plan, graph.direction, turn, target_turn, style);
        push_primary(&mut plan, graph.direction, target_turn, target_entry, style);
        push_identity_corners(
            &mut plan,
            outside,
            turn,
            target_turn,
            target_entry,
            graph.direction,
            style,
        );
        plan.claim_boundary(
            scene.subgraph_id.clone(),
            scene.side.name(),
            portal.0,
            portal.1,
            primary_glyph(graph.direction, style),
        );
        plan.set_target_entry_decision(PortalEntryDecision {
            edge_id: edge.owner_id.clone(),
            owner_id: owner_id.clone(),
            target_node_id: scene.target.id.clone(),
            boundary_id: scene.subgraph_id.clone(),
            side: scene.side.name().to_owned(),
            portal_x: portal.0,
            portal_y: portal.1,
            arrow_x: target_entry.0,
            arrow_y: target_entry.1,
        });
        plan.push_paint(target_entry.0, target_entry.1, coords.arrow_end(style));
        target_routes.push(TargetRoute {
            edge_owner_id: edge.owner_id.clone(),
            source_exit,
            portal,
            outside,
            turn,
            target_turn,
            target_entry,
        });
    }

    if plan
        .planned_cells()
        .iter()
        .any(|(x, y)| *x >= canvas.width || *y >= canvas.height)
    {
        return None;
    }
    Some(BoundaryFanInPlan {
        route_plan: plan,
        target_routes,
    })
}

fn build_collector_plan(
    scene: &BoundaryFanInScene,
    graph: &Graph,
    canvas: &Canvas,
    style: &StyleChars,
) -> Option<FallbackRoutePlan> {
    let coords = crate::orientation::OrientedCoords::new(graph.direction);
    let target_arrow = target_arrow(scene, graph);
    let subgraph = graph.get_subgraph(&scene.subgraph_id)?;
    let boundary_primary = match scene.side {
        BoundarySide::Top | BoundarySide::Bottom => subgraph.bounds.y,
        BoundarySide::Left => subgraph.bounds.x,
        BoundarySide::Right => subgraph
            .bounds
            .x
            .saturating_add(subgraph.bounds.width.saturating_sub(1)),
    };
    let target_primary = coords.primary_coord(target_arrow.0, target_arrow.1);
    let moving_forward = matches!(
        graph.direction,
        Direction::TD | Direction::TB | Direction::LR
    );
    if (moving_forward && target_primary <= boundary_primary)
        || (!moving_forward && target_primary >= boundary_primary)
    {
        return None;
    }

    let first_boundary = boundary_point(scene, scene.lanes[0], graph);
    let outside_boundary = coords.advance(first_boundary.0, first_boundary.1, 1);
    if coords.primary_coord(outside_boundary.0, outside_boundary.1) == boundary_primary
        || outside_boundary.0 >= canvas.width
        || outside_boundary.1 >= canvas.height
    {
        return None;
    }

    let target_secondary = coords.secondary_coord(target_arrow.0, target_arrow.1);
    let min_secondary = scene.lanes.iter().copied().min()?.min(target_secondary);
    let max_secondary = scene.lanes.iter().copied().max()?.max(target_secondary);
    let collector_anchor = collector_anchor(target_arrow, graph.direction, &coords);
    let collector_start =
        coords.with_secondary(collector_anchor.0, collector_anchor.1, min_secondary);
    let collector_end =
        coords.with_secondary(collector_anchor.0, collector_anchor.1, max_secondary);
    let collector_center =
        coords.with_secondary(collector_anchor.0, collector_anchor.1, target_secondary);
    let owner_id = scene_owner_id(scene);

    let mut plan = FallbackRoutePlan::new(owner_id.clone(), STRATEGY);
    plan.set_scene_coverage(scene.edges.iter().map(|edge| edge.owner_id.clone()));
    plan.set_arrow_attachment(target_arrow.0, target_arrow.1);
    let collector_cells = secondary_cells(collector_start, collector_end, graph.direction);
    let target_link_cells = primary_cells(collector_center, target_arrow, graph.direction);
    let collector_cells = collector_cells
        .into_iter()
        .chain(target_link_cells)
        .collect::<BTreeSet<_>>();
    let mut occupied: HashMap<(usize, usize), usize> = HashMap::new();

    for edge in &scene.edges {
        let source_exit = edge_exit_point(&edge.source, graph.direction);
        let portal = boundary_point(scene, edge.lane, graph);
        if coords.secondary_coord(source_exit.0, source_exit.1) != edge.lane {
            return None;
        }
        let outside = coords.advance(portal.0, portal.1, 1);
        let collector = coords.with_secondary(collector_start.0, collector_start.1, edge.lane);
        push_primary(&mut plan, graph.direction, source_exit, portal, style);
        push_primary(&mut plan, graph.direction, outside, collector, style);
        for cell in primary_cells(source_exit, portal, graph.direction)
            .into_iter()
            .chain(primary_cells(outside, collector, graph.direction))
        {
            if let Some(previous) = occupied.insert(cell, edge.index) {
                if previous != edge.index && !collector_cells.contains(&cell) {
                    return None;
                }
            }
        }
        plan.claim_boundary(
            scene.subgraph_id.clone(),
            scene.side.name(),
            portal.0,
            portal.1,
            primary_glyph(graph.direction, style),
        );
        plan.set_target_entry_decision(PortalEntryDecision {
            edge_id: edge.owner_id.clone(),
            owner_id: owner_id.clone(),
            target_node_id: scene.target.id.clone(),
            boundary_id: scene.subgraph_id.clone(),
            side: scene.side.name().to_owned(),
            portal_x: portal.0,
            portal_y: portal.1,
            arrow_x: target_arrow.0,
            arrow_y: target_arrow.1,
        });
    }

    push_secondary(
        &mut plan,
        graph.direction,
        collector_start,
        collector_end,
        style,
    );
    push_primary(
        &mut plan,
        graph.direction,
        collector_center,
        target_arrow,
        style,
    );
    for lane in &scene.lanes {
        let point = coords.with_secondary(collector_start.0, collector_start.1, *lane);
        if point != target_arrow {
            plan.push_corner(
                point.0,
                point.1,
                collector_glyph(*lane, &scene.lanes, graph.direction, style),
            );
        }
    }
    plan.push_paint(target_arrow.0, target_arrow.1, coords.arrow_end(style));
    if plan
        .planned_cells()
        .iter()
        .any(|(x, y)| *x >= canvas.width || *y >= canvas.height)
    {
        return None;
    }
    Some(plan)
}

fn identity_turns(
    outside: (usize, usize),
    target_entry: (usize, usize),
    index: usize,
    direction: Direction,
    coords: &crate::orientation::OrientedCoords,
) -> Option<((usize, usize), (usize, usize))> {
    let outside_primary = coords.primary_coord(outside.0, outside.1);
    let target_primary = coords.primary_coord(target_entry.0, target_entry.1);
    let target_secondary = coords.secondary_coord(target_entry.0, target_entry.1);
    let moving_forward = matches!(direction, Direction::TD | Direction::TB | Direction::LR);
    if (moving_forward && target_primary < outside_primary)
        || (!moving_forward && target_primary > outside_primary)
    {
        return None;
    }

    let turn_primary = match direction {
        Direction::TD | Direction::TB | Direction::BT => {
            if moving_forward {
                outside_primary.saturating_add(index.saturating_mul(2))
            } else {
                outside_primary.saturating_sub(index.saturating_mul(2))
            }
        }
        Direction::LR => outside_primary.saturating_add(index),
        Direction::RL => outside_primary.saturating_sub(index),
    };
    if (moving_forward && turn_primary > target_primary)
        || (!moving_forward && turn_primary < target_primary)
    {
        return None;
    }

    let mut turn = outside;
    coords.set_primary(&mut turn.0, &mut turn.1, turn_primary);
    let target_turn = coords.with_secondary(turn.0, turn.1, target_secondary);
    Some((turn, target_turn))
}

fn push_identity_corners(
    plan: &mut FallbackRoutePlan,
    outside: (usize, usize),
    turn: (usize, usize),
    target_turn: (usize, usize),
    target_entry: (usize, usize),
    direction: Direction,
    style: &StyleChars,
) {
    let coords = crate::orientation::OrientedCoords::new(direction);
    let going_before = coords.secondary_coord(outside.0, outside.1)
        > coords.secondary_coord(target_entry.0, target_entry.1);
    if target_turn != turn {
        plan.push_corner(
            if turn == outside { outside.0 } else { turn.0 },
            if turn == outside { outside.1 } else { turn.1 },
            coords.corner_start_to_secondary(going_before, style),
        );
    }
    if target_turn != turn && target_turn != target_entry {
        plan.push_corner(
            target_turn.0,
            target_turn.1,
            coords.corner_secondary_to_end(going_before, style),
        );
    }
}

fn primary_glyph(direction: Direction, style: &StyleChars) -> char {
    match direction {
        Direction::TD | Direction::TB | Direction::BT => style.edge_v,
        Direction::LR | Direction::RL => style.edge_h,
    }
}

fn target_arrow(scene: &BoundaryFanInScene, graph: &Graph) -> (usize, usize) {
    let coords = crate::orientation::OrientedCoords::new(graph.direction);
    let preferred_secondary = scene.lanes[scene.lanes.len() / 2];
    edge_entry_candidates(&scene.target, graph.direction)
        .into_iter()
        .filter(|(x, y)| !hits_foreign_subgraph_border(&scene.target, *x, *y, graph))
        .min_by_key(|(x, y)| {
            (
                coords.secondary_coord(*x, *y).abs_diff(preferred_secondary),
                coords.primary_coord(*x, *y),
                *x,
                *y,
            )
        })
        .unwrap_or_else(|| adjusted_edge_entry_point(&scene.target, graph.direction, graph))
}

fn collector_anchor(
    target_arrow: (usize, usize),
    direction: Direction,
    coords: &crate::orientation::OrientedCoords,
) -> (usize, usize) {
    match direction {
        Direction::LR | Direction::RL => coords.retreat(target_arrow.0, target_arrow.1, 1),
        Direction::TD | Direction::TB | Direction::BT => target_arrow,
    }
}

fn push_primary(
    plan: &mut FallbackRoutePlan,
    direction: Direction,
    from: (usize, usize),
    to: (usize, usize),
    style: &StyleChars,
) {
    match direction {
        Direction::TD | Direction::TB | Direction::BT => {
            plan.push_vertical(from.0, from.1, to.1, style.edge_v)
        }
        Direction::LR | Direction::RL => plan.push_horizontal(from.1, from.0, to.0, style.edge_h),
    }
}

fn push_secondary(
    plan: &mut FallbackRoutePlan,
    direction: Direction,
    from: (usize, usize),
    to: (usize, usize),
    style: &StyleChars,
) {
    match direction {
        Direction::TD | Direction::TB | Direction::BT => {
            plan.push_horizontal(from.1, from.0, to.0, style.edge_h)
        }
        Direction::LR | Direction::RL => plan.push_vertical(from.0, from.1, to.1, style.edge_v),
    }
}

fn primary_cells(
    from: (usize, usize),
    to: (usize, usize),
    direction: Direction,
) -> BTreeSet<(usize, usize)> {
    let mut cells = BTreeSet::new();
    match direction {
        Direction::TD | Direction::TB | Direction::BT => {
            for y in from.1.min(to.1)..=from.1.max(to.1) {
                cells.insert((from.0, y));
            }
        }
        Direction::LR | Direction::RL => {
            for x in from.0.min(to.0)..=from.0.max(to.0) {
                cells.insert((x, from.1));
            }
        }
    }
    cells
}

fn secondary_cells(
    from: (usize, usize),
    to: (usize, usize),
    direction: Direction,
) -> BTreeSet<(usize, usize)> {
    match direction {
        Direction::TD | Direction::TB | Direction::BT => {
            let mut cells = BTreeSet::new();
            for x in from.0.min(to.0)..=from.0.max(to.0) {
                cells.insert((x, from.1));
            }
            cells
        }
        Direction::LR | Direction::RL => {
            let mut cells = BTreeSet::new();
            for y in from.1.min(to.1)..=from.1.max(to.1) {
                cells.insert((from.0, y));
            }
            cells
        }
    }
}

fn collector_glyph(lane: usize, lanes: &[usize], direction: Direction, style: &StyleChars) -> char {
    if lane == lanes[0] {
        return match direction {
            Direction::TD | Direction::TB => style.corner_ul,
            Direction::BT => style.corner_dl,
            Direction::LR => style.corner_dr,
            Direction::RL => style.corner_dl,
        };
    }
    if lane == *lanes.last().expect("fan-in lanes are non-empty") {
        return match direction {
            Direction::TD | Direction::TB => style.corner_ur,
            Direction::BT => style.corner_dr,
            Direction::LR => style.corner_ur,
            Direction::RL => style.corner_ul,
        };
    }
    match direction {
        Direction::TD | Direction::TB => style.junction_down,
        Direction::BT => style.junction_up,
        Direction::LR | Direction::RL => style.cross,
    }
}

fn canvas_blocker(
    scene: &BoundaryFanInScene,
    plan: &FallbackRoutePlan,
    graph: &Graph,
    canvas: &Canvas,
) -> Option<String> {
    let source_exits: HashSet<_> = scene
        .edges
        .iter()
        .map(|edge| edge_exit_point(&edge.source, graph.direction))
        .collect();
    let boundary_claims: HashSet<_> = plan
        .boundary_claims
        .iter()
        .map(|claim| (claim.x, claim.y))
        .collect();
    let arrows: HashSet<_> = plan
        .entry_decisions
        .iter()
        .map(|decision| (decision.arrow_x, decision.arrow_y))
        .chain(plan.arrow_attachment.map(|point| (point.x, point.y)))
        .collect();
    for (x, y) in plan.planned_cells() {
        if is_subgraph_title_cell(graph, x, y) {
            return Some(format!("route crosses subgraph title at ({x},{y})"));
        }
        let existing = canvas.get(x, y);
        if existing == ' '
            || source_exits.contains(&(x, y))
            || boundary_claims.contains(&(x, y))
            || arrows.contains(&(x, y))
        {
            continue;
        }
        return Some(format!(
            "route cell ({x},{y}) is occupied by {:?}",
            canvas.get_meta(x, y).map(|meta| meta.owner_kind)
        ));
    }
    None
}

fn lower_plan(
    scene: &BoundaryFanInScene,
    plan: BoundaryFanInPlan,
    graph: &Graph,
    canvas: &mut Canvas,
    style: &StyleChars,
) {
    if !plan.target_routes.is_empty() {
        lower_identity_plan(plan, graph, canvas, style);
        return;
    }

    let plan = plan.route_plan;
    let coords = crate::orientation::OrientedCoords::new(graph.direction);
    let scene_owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: plan.owner_id.as_str(),
    };
    let target_arrow = plan.arrow_attachment.expect("fan-in plan has arrow");
    let target_secondary = coords.secondary_coord(target_arrow.x, target_arrow.y);
    let min_secondary = scene
        .lanes
        .iter()
        .copied()
        .min()
        .unwrap_or(target_secondary);
    let max_secondary = scene
        .lanes
        .iter()
        .copied()
        .max()
        .unwrap_or(target_secondary);
    let collector_anchor =
        collector_anchor((target_arrow.x, target_arrow.y), graph.direction, &coords);
    let collector_start =
        coords.with_secondary(collector_anchor.0, collector_anchor.1, min_secondary);
    let collector_end =
        coords.with_secondary(collector_anchor.0, collector_anchor.1, max_secondary);
    let collector_center =
        coords.with_secondary(collector_anchor.0, collector_anchor.1, target_secondary);

    canvas.set_write_stage("edge-route-boundary-fan-in");
    canvas.record_fallback_route_plan(plan.clone());
    for edge in &scene.edges {
        let edge_owner = RouteOwner {
            kind: CellOwnerKind::EdgeSegment,
            id: edge.owner_id.as_str(),
        };
        let source_exit = edge_exit_point(&edge.source, graph.direction);
        let portal = boundary_point(scene, edge.lane, graph);
        let outside = coords.advance(portal.0, portal.1, 1);
        let collector = coords.with_secondary(collector_start.0, collector_start.1, edge.lane);
        draw_line_primary(
            source_exit.0,
            source_exit.1,
            portal.0,
            portal.1,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(edge_owner),
        );
        draw_line_primary(
            outside.0,
            outside.1,
            collector.0,
            collector.1,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(edge_owner),
        );
    }
    draw_line_secondary(
        collector_start.0,
        collector_start.1,
        collector_end.0,
        collector_end.1,
        &coords,
        canvas,
        style,
        Some(graph),
        Some(scene_owner),
    );
    draw_line_primary(
        collector_center.0,
        collector_center.1,
        target_arrow.x,
        target_arrow.y,
        &coords,
        canvas,
        style,
        Some(graph),
        Some(scene_owner),
    );
    for corner in &plan.corners {
        set_route_char(
            canvas,
            corner.point.x,
            corner.point.y,
            corner.glyph,
            Some(scene_owner),
        );
    }
    for claim in &plan.boundary_claims {
        set_route_char(
            canvas,
            claim.x,
            claim.y,
            claim.expected_glyph,
            Some(scene_owner),
        );
    }
    set_route_char(
        canvas,
        target_arrow.x,
        target_arrow.y,
        coords.arrow_end(style),
        Some(scene_owner),
    );
}

fn lower_identity_plan(
    plan: BoundaryFanInPlan,
    graph: &Graph,
    canvas: &mut Canvas,
    style: &StyleChars,
) {
    let coords = crate::orientation::OrientedCoords::new(graph.direction);
    let scene_owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: plan.route_plan.owner_id.as_str(),
    };

    canvas.set_write_stage("edge-route-boundary-fan-in-identity");
    canvas.record_fallback_route_plan(plan.route_plan.clone());
    for route in &plan.target_routes {
        let edge_owner = RouteOwner {
            kind: CellOwnerKind::EdgeSegment,
            id: route.edge_owner_id.as_str(),
        };
        draw_line_primary(
            route.source_exit.0,
            route.source_exit.1,
            route.portal.0,
            route.portal.1,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(edge_owner),
        );
        draw_line_primary(
            route.outside.0,
            route.outside.1,
            route.turn.0,
            route.turn.1,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(edge_owner),
        );
        draw_line_secondary(
            route.turn.0,
            route.turn.1,
            route.target_turn.0,
            route.target_turn.1,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(edge_owner),
        );
        draw_line_primary(
            route.target_turn.0,
            route.target_turn.1,
            route.target_entry.0,
            route.target_entry.1,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(edge_owner),
        );
        push_identity_route_corners(canvas, route, graph.direction, style, edge_owner);
        set_route_char(
            canvas,
            route.target_entry.0,
            route.target_entry.1,
            coords.arrow_end(style),
            Some(edge_owner),
        );
    }

    for claim in &plan.route_plan.boundary_claims {
        set_route_char(
            canvas,
            claim.x,
            claim.y,
            claim.expected_glyph,
            Some(scene_owner),
        );
    }
}

fn push_identity_route_corners(
    canvas: &mut Canvas,
    route: &TargetRoute,
    direction: Direction,
    style: &StyleChars,
    owner: RouteOwner<'_>,
) {
    let coords = crate::orientation::OrientedCoords::new(direction);
    let going_before = coords.secondary_coord(route.outside.0, route.outside.1)
        > coords.secondary_coord(route.target_entry.0, route.target_entry.1);
    if route.target_turn != route.turn {
        set_route_char(
            canvas,
            if route.turn == route.outside {
                route.outside.0
            } else {
                route.turn.0
            },
            if route.turn == route.outside {
                route.outside.1
            } else {
                route.turn.1
            },
            coords.corner_start_to_secondary(going_before, style),
            Some(owner),
        );
    }
    if route.target_turn != route.turn && route.target_turn != route.target_entry {
        set_route_char(
            canvas,
            route.target_turn.0,
            route.target_turn.1,
            coords.corner_secondary_to_end(going_before, style),
            Some(owner),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::collector_glyph;
    use crate::graph::Direction;
    use crate::style::BaseStyle;

    #[test]
    fn horizontal_collector_center_keeps_both_source_and_target_arms() {
        let style = BaseStyle::Unicode.chars();

        assert_eq!(
            collector_glyph(2, &[1, 2, 3], Direction::LR, style),
            style.cross
        );
        assert_eq!(
            collector_glyph(2, &[1, 2, 3], Direction::RL, style),
            style.cross
        );
    }
}
