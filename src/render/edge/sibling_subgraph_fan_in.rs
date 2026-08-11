//! Transactional two-edge fan-in across a sibling subgraph boundary.
//!
//! Generic convergence intentionally shares one target arrow.  This bounded
//! scene owns the two external source-to-target edges only when it can reserve
//! both source portals, both exterior corridors, and both terminal target
//! entries before any paint occurs.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::geom::Rect;
use crate::graph::{Direction, Graph, Node};
use crate::orientation::OrientedCoords;
use crate::portals::PortalSlots;
use crate::render::sibling_subgraph_fan_in_identity::{self, Scene};
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::edge::edge_primitives::{
    draw_line_primary, draw_line_secondary, edge_exit_point, is_subgraph_title_cell,
};
use super::super::fallback_route::{FallbackRoutePlan, PortalEntryDecision};
use super::super::fan_in_identity::{target_port_columns, target_port_rows};
use super::super::provenance::edge_owner_id;
use super::super::semantic::CellOwnerKind;
use super::{set_route_char, RouteOwner};

const STRATEGY: &str = "sibling-subgraph-fan-in-target-entry-identity";

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

#[derive(Debug, Clone, Copy)]
struct SceneGeometry {
    side: BoundarySide,
    route_direction: Direction,
}

struct PortalLaneRequest<'a> {
    graph: &'a Graph,
    bounds: &'a crate::graph::Rectangle,
    side: BoundarySide,
    direction: Direction,
    prefer_desired_lane: bool,
    used: &'a BTreeSet<usize>,
}

#[derive(Debug, Clone)]
struct SceneEdge {
    index: usize,
    source: Node,
    lane: usize,
    portal_lane: usize,
    owner_id: String,
}

#[derive(Debug, Clone)]
struct TargetRoute {
    edge_owner_id: String,
    source_exit: (usize, usize),
    source_departure: (usize, usize),
    source_turn: (usize, usize),
    portal: (usize, usize),
    outside: (usize, usize),
    turn: (usize, usize),
    target_turn: (usize, usize),
    target_entry: (usize, usize),
}

#[derive(Debug)]
struct ScenePlan {
    owner_id: String,
    source_subgraph_id: String,
    side: BoundarySide,
    route_direction: Direction,
    edges: Vec<SceneEdge>,
    target_routes: Vec<TargetRoute>,
    route_plan: FallbackRoutePlan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildPlanRejection {
    TargetMissing,
    SourceSubgraphMissing,
    SourceSubgraphBoundsInvalid,
    PhysicalGeometryUnavailable,
    TargetEntryCapacityUnavailable,
    SourceEdgeMaterialization,
    SourceLaneMismatch,
    PortalLaneUnavailable,
    TurnOrderUnavailable,
    IdentityTurnUnavailable,
    RouteCellOverlap,
    PlanOutOfCanvas,
}

impl BuildPlanRejection {
    const fn message(self) -> &'static str {
        match self {
            Self::TargetMissing => "build plan: target node unavailable",
            Self::SourceSubgraphMissing => "build plan: source subgraph unavailable",
            Self::SourceSubgraphBoundsInvalid => "build plan: source subgraph bounds invalid",
            Self::PhysicalGeometryUnavailable => {
                "build plan: physical source-target geometry unavailable"
            }
            Self::TargetEntryCapacityUnavailable => "build plan: target entry capacity unavailable",
            Self::SourceEdgeMaterialization => "build plan: source edge materialization failed",
            Self::SourceLaneMismatch => "build plan: source lane mismatch",
            Self::PortalLaneUnavailable => "build plan: portal lane unavailable",
            Self::TurnOrderUnavailable => "build plan: collision-free turn order unavailable",
            Self::IdentityTurnUnavailable => "build plan: identity turn unavailable",
            Self::RouteCellOverlap => "build plan: route cells overlap",
            Self::PlanOutOfCanvas => "build plan: planned cells exceed canvas",
        }
    }
}

/// Plan and lower the bounded sibling-subgraph scene.  Rejection is
/// fail-closed: the live canvas and portal slot map remain unchanged.
pub(crate) fn plan_sibling_subgraph_fan_in_scene(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    canvas: &mut Canvas,
    style: &StyleChars,
    portal_slots: &mut HashMap<String, PortalSlots>,
) -> HashSet<usize> {
    for scene in sibling_subgraph_fan_in_identity::scenes(graph) {
        let owner_id = scene_owner_id(&scene);
        let plan = match build_plan(&scene, graph, node_rects, canvas, style, portal_slots) {
            Ok(plan) => plan,
            Err(reason) => {
                canvas.record_fallback_route_rejection(owner_id, STRATEGY, reason.message());
                continue;
            }
        };
        if let Some(reason) = plan
            .route_plan
            .validation_error(canvas.width, canvas.height)
        {
            canvas.record_fallback_route_rejection(owner_id, STRATEGY, reason);
            continue;
        }
        if let Some(reason) = canvas_blocker(&plan, graph, canvas) {
            canvas.record_fallback_route_rejection(owner_id, STRATEGY, reason);
            continue;
        }
        let Some(slots) = portal_slots.get_mut(&plan.source_subgraph_id) else {
            canvas.record_fallback_route_rejection(
                owner_id,
                STRATEGY,
                "source subgraph has no portal slot record",
            );
            continue;
        };
        let lanes = plan
            .edges
            .iter()
            .map(|edge| edge.portal_lane)
            .collect::<Vec<_>>();
        if plan.side != incoming_context_side(graph.direction) {
            replace_side_lanes(slots, plan.side, lanes);
        } else {
            merge_side_lanes(slots, plan.side, lanes);
        }
        let covered = plan
            .edges
            .iter()
            .map(|edge| edge.index)
            .collect::<HashSet<_>>();
        lower_plan(plan, graph, canvas, style);
        return covered;
    }
    HashSet::new()
}

fn build_plan(
    scene: &Scene,
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    canvas: &Canvas,
    style: &StyleChars,
    portal_slots: &HashMap<String, PortalSlots>,
) -> Result<ScenePlan, BuildPlanRejection> {
    let target = graph
        .get_node(&scene.target_id)
        .ok_or(BuildPlanRejection::TargetMissing)?
        .clone();
    let source_subgraph = graph
        .get_subgraph(&scene.source_subgraph_id)
        .ok_or(BuildPlanRejection::SourceSubgraphMissing)?;
    if !source_subgraph.bounds.is_valid() {
        return Err(BuildPlanRejection::SourceSubgraphBoundsInvalid);
    }
    let target_rect = node_rects
        .get(&scene.target_id)
        .copied()
        .unwrap_or_else(|| Rect::new(target.x, target.y, target.width, target.height.max(1)));
    let geometry = physical_geometry(graph.direction, &source_subgraph.bounds, target_rect)
        .ok_or(BuildPlanRejection::PhysicalGeometryUnavailable)?;
    let target_entries = target_entry_points(&target, geometry.route_direction);
    if target_entries.len() != scene.edge_indexes.len() {
        return Err(BuildPlanRejection::TargetEntryCapacityUnavailable);
    }
    if scene
        .edge_indexes
        .iter()
        .zip(scene.source_ids.iter())
        .any(|(index, source_id)| {
            graph.edges.get(*index).is_none() || graph.get_node(source_id).is_none()
        })
    {
        return Err(BuildPlanRejection::SourceEdgeMaterialization);
    }

    let mut edges = scene
        .edge_indexes
        .iter()
        .zip(scene.source_ids.iter())
        .filter_map(|(index, source_id)| {
            let edge = graph.edges.get(*index)?;
            let source = graph.get_node(source_id)?.clone();
            let rect = node_rects.get(source_id).copied().unwrap_or_else(|| {
                Rect::new(source.x, source.y, source.width, source.height.max(1))
            });
            let lane = match geometry.route_direction {
                Direction::TD | Direction::TB | Direction::BT => rect.x + rect.width / 2,
                Direction::LR | Direction::RL => rect.y + rect.height / 2,
            };
            Some(SceneEdge {
                index: *index,
                source,
                lane,
                portal_lane: 0,
                owner_id: edge_owner_id(*index, edge),
            })
        })
        .collect::<Vec<_>>();
    if edges.len() != scene.edge_indexes.len() {
        return Err(BuildPlanRejection::SourceEdgeMaterialization);
    }
    edges.sort_by_key(|edge| (edge.lane, edge.source.id.clone(), edge.index));
    if matches!(geometry.route_direction, Direction::TD | Direction::TB)
        && geometry.side == BoundarySide::Bottom
    {
        // When the target is to the right of a bottom-exiting source
        // subgraph, place the rightmost source on the first (upper) target
        // channel.  Its horizontal rail then starts beyond the other
        // source's vertical stem; the inverse ordering would force the
        // second stem through that rail and make the scene reject.
        let target_center = target_rect.x.saturating_add(target_rect.width / 2);
        let source_center = source_subgraph
            .bounds
            .x
            .saturating_add(source_subgraph.bounds.width / 2);
        if target_center >= source_center {
            edges.reverse();
        }
    }

    // Preserve lanes already owned by other boundary crossings.  A complex
    // horizontal graph can have one subgraph receiving edges on the same
    // wall from a sibling while also sending a fan-in to an external target;
    // letting both scenes select the source-node centers creates a false
    // single portal and later provenance mismatches.  The sibling fan-in scene
    // gets a distinct source-side lane and routes to it transactionally.
    let mut used_portal_lanes: BTreeSet<usize> = portal_slots
        .get(&scene.source_subgraph_id)
        .map(|slots| match geometry.side {
            BoundarySide::Top => slots.top.iter().copied().collect(),
            BoundarySide::Bottom => slots.bottom.iter().copied().collect(),
            BoundarySide::Left => slots.left.iter().copied().collect(),
            BoundarySide::Right => slots.right.iter().copied().collect(),
        })
        .unwrap_or_default();
    // The portal collector already reserves the source-node centers for the
    // exact outgoing edges owned by this scene.  Those are reusable claims,
    // not competing crossings: keeping them in the occupied set forces the
    // second source onto an unnecessary alignment lane and can make the two
    // source-to-portal corridors intersect before they ever leave the
    // subgraph.  Keep every other boundary claim protected.
    let horizontal_scene = matches!(graph.direction, Direction::LR | Direction::RL);
    let scene_source_lanes = edges.iter().map(|edge| edge.lane).collect::<HashSet<_>>();
    if horizontal_scene {
        used_portal_lanes.retain(|lane| !scene_source_lanes.contains(lane));
    }
    for (index, edge) in edges.iter_mut().enumerate() {
        let portal_lane = select_portal_lane(
            &PortalLaneRequest {
                graph,
                bounds: &source_subgraph.bounds,
                side: geometry.side,
                direction: geometry.route_direction,
                prefer_desired_lane: horizontal_scene,
                used: &used_portal_lanes,
            },
            &edge.source,
            edge.lane,
            target_entries[index].1,
            index,
        )
        .ok_or(BuildPlanRejection::PortalLaneUnavailable)?;
        edge.portal_lane = portal_lane;
        used_portal_lanes.insert(edge.portal_lane);
    }

    let owner_id = scene_owner_id(scene);
    let turn_order = select_turn_order(
        &edges,
        &source_subgraph.bounds,
        geometry.side,
        &target_entries,
        geometry.route_direction,
    )
    .ok_or(BuildPlanRejection::TurnOrderUnavailable)?;
    let mut route_plan = FallbackRoutePlan::new(owner_id.clone(), STRATEGY);
    route_plan.set_scene_coverage(edges.iter().map(|edge| edge.owner_id.clone()));
    let coords = OrientedCoords::new(geometry.route_direction);
    let mut occupied: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    let mut target_routes = Vec::with_capacity(edges.len());

    for (index, edge) in edges.iter().enumerate() {
        let target_entry = target_entries[turn_order[index]];
        let source_exit = edge_exit_point(&edge.source, geometry.route_direction);
        let portal = boundary_point(&source_subgraph.bounds, geometry.side, edge.portal_lane);
        if coords.secondary_coord(source_exit.0, source_exit.1) != edge.lane {
            return Err(BuildPlanRejection::SourceLaneMismatch);
        }
        let source_departure = coords.advance(source_exit.0, source_exit.1, 1);
        let source_turn =
            coords.with_secondary(source_departure.0, source_departure.1, edge.portal_lane);
        let outside = coords.advance(portal.0, portal.1, 1);
        let (turn, target_turn) = identity_turns(
            outside,
            target_entry,
            index,
            geometry.route_direction,
            &coords,
        )
        .ok_or(BuildPlanRejection::IdentityTurnUnavailable)?;
        let route_cells = primary_cells(source_exit, source_departure, geometry.route_direction)
            .into_iter()
            .chain(secondary_cells(
                source_departure,
                source_turn,
                geometry.route_direction,
            ))
            .chain(primary_cells(source_turn, portal, geometry.route_direction))
            .chain(primary_cells(outside, turn, geometry.route_direction))
            .chain(secondary_cells(turn, target_turn, geometry.route_direction))
            .chain(primary_cells(
                target_turn,
                target_entry,
                geometry.route_direction,
            ))
            .collect::<BTreeSet<_>>();
        for cell in route_cells {
            if let Some(previous) = occupied.insert(cell, index) {
                if previous != index {
                    return Err(BuildPlanRejection::RouteCellOverlap);
                }
            }
        }

        push_primary(
            &mut route_plan,
            geometry.route_direction,
            source_exit,
            source_departure,
            style,
        );
        push_secondary(
            &mut route_plan,
            geometry.route_direction,
            source_departure,
            source_turn,
            style,
        );
        push_primary(
            &mut route_plan,
            geometry.route_direction,
            source_turn,
            portal,
            style,
        );
        push_source_alignment_corner(
            &mut route_plan,
            source_departure,
            source_turn,
            geometry.route_direction,
            style,
        );
        push_primary(
            &mut route_plan,
            geometry.route_direction,
            outside,
            turn,
            style,
        );
        push_secondary(
            &mut route_plan,
            geometry.route_direction,
            turn,
            target_turn,
            style,
        );
        push_primary(
            &mut route_plan,
            geometry.route_direction,
            target_turn,
            target_entry,
            style,
        );
        push_identity_corners(
            &mut route_plan,
            outside,
            turn,
            target_turn,
            target_entry,
            geometry.route_direction,
            style,
        );
        route_plan.claim_boundary(
            scene.source_subgraph_id.clone(),
            geometry.side.name(),
            portal.0,
            portal.1,
            primary_glyph(geometry.route_direction, style),
        );
        route_plan.set_target_entry_decision(PortalEntryDecision {
            edge_id: edge.owner_id.clone(),
            owner_id: owner_id.clone(),
            target_node_id: target.id.clone(),
            boundary_id: scene.source_subgraph_id.clone(),
            side: geometry.side.name().to_owned(),
            portal_x: portal.0,
            portal_y: portal.1,
            arrow_x: target_entry.0,
            arrow_y: target_entry.1,
        });
        route_plan.push_paint(target_entry.0, target_entry.1, coords.arrow_end(style));
        target_routes.push(TargetRoute {
            edge_owner_id: edge.owner_id.clone(),
            source_exit,
            source_departure,
            source_turn,
            portal,
            outside,
            turn,
            target_turn,
            target_entry,
        });
    }

    if route_plan
        .planned_cells()
        .iter()
        .any(|(x, y)| *x >= canvas.width || *y >= canvas.height)
    {
        return Err(BuildPlanRejection::PlanOutOfCanvas);
    }
    Ok(ScenePlan {
        owner_id,
        source_subgraph_id: scene.source_subgraph_id.clone(),
        side: geometry.side,
        route_direction: geometry.route_direction,
        edges,
        target_routes,
        route_plan,
    })
}

/// Select a turn-lane permutation before constructing any fallback plan.
///
/// Two source lanes can be ordered differently from the target-side entries
/// once a subgraph is crossed.  Trying the small permutation space here keeps
/// route ownership explicit while avoiding the common narrow-corridor case
/// where the first route's horizontal segment cuts through the second
/// route's vertical stem.  Larger scenes are rejected by the topology gate;
/// the generic fallback remains responsible for everything else.
fn select_turn_order(
    edges: &[SceneEdge],
    source_bounds: &crate::graph::Rectangle,
    side: BoundarySide,
    target_entries: &[(usize, usize)],
    direction: Direction,
) -> Option<Vec<usize>> {
    if edges.len() != target_entries.len() || edges.is_empty() {
        return None;
    }

    let candidates: Vec<Vec<usize>> = if edges.len() == 2 {
        vec![vec![0, 1], vec![1, 0]]
    } else {
        vec![(0..edges.len()).collect()]
    };
    let coords = OrientedCoords::new(direction);

    for order in candidates {
        let mut occupied: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        let mut valid = true;
        for (index, edge) in edges.iter().enumerate() {
            let source_exit = edge_exit_point(&edge.source, direction);
            let portal = boundary_point(source_bounds, side, edge.portal_lane);
            let outside = coords.advance(portal.0, portal.1, 1);
            let target_entry = target_entries[order[index]];
            let Some((turn, target_turn)) =
                identity_turns(outside, target_entry, index, direction, &coords)
            else {
                valid = false;
                break;
            };
            let source_departure = coords.advance(source_exit.0, source_exit.1, 1);
            let source_turn =
                coords.with_secondary(source_departure.0, source_departure.1, edge.portal_lane);
            let route_cells = primary_cells(source_exit, source_departure, direction)
                .into_iter()
                .chain(secondary_cells(source_departure, source_turn, direction))
                .chain(primary_cells(source_turn, portal, direction))
                .chain(primary_cells(outside, turn, direction))
                .chain(secondary_cells(turn, target_turn, direction))
                .chain(primary_cells(target_turn, target_entry, direction));
            for cell in route_cells {
                if let Some(previous) = occupied.insert(cell, index) {
                    if previous != index {
                        valid = false;
                        break;
                    }
                }
            }
            if !valid {
                break;
            }
        }
        if valid {
            return Some(order);
        }
    }
    None
}

fn physical_geometry(
    graph_direction: Direction,
    source_bounds: &crate::graph::Rectangle,
    target: Rect,
) -> Option<SceneGeometry> {
    let right = target.x >= source_bounds.x.saturating_add(source_bounds.width);
    let left = target.x.saturating_add(target.width) <= source_bounds.x;
    let below = target.y >= source_bounds.y.saturating_add(source_bounds.height);
    let above = target.y.saturating_add(target.height) <= source_bounds.y;

    let vertical_preferred = matches!(
        graph_direction,
        Direction::TD | Direction::TB | Direction::BT
    );
    if vertical_preferred {
        if below && right {
            let vertical_gap = target
                .y
                .saturating_sub(source_bounds.y.saturating_add(source_bounds.height));
            let horizontal_gap = target
                .x
                .saturating_sub(source_bounds.x.saturating_add(source_bounds.width));
            if vertical_gap
                < sibling_subgraph_fan_in_identity::required_primary_gap(
                    sibling_subgraph_fan_in_identity::TARGET_PORT_COUNT,
                )
                || horizontal_gap <= vertical_gap
            {
                return Some(SceneGeometry {
                    side: BoundarySide::Right,
                    route_direction: Direction::LR,
                });
            }
        }
        if below && left {
            let vertical_gap = target
                .y
                .saturating_sub(source_bounds.y.saturating_add(source_bounds.height));
            let horizontal_gap = source_bounds
                .x
                .saturating_sub(target.x.saturating_add(target.width));
            if vertical_gap
                < sibling_subgraph_fan_in_identity::required_primary_gap(
                    sibling_subgraph_fan_in_identity::TARGET_PORT_COUNT,
                )
                || horizontal_gap <= vertical_gap
            {
                return Some(SceneGeometry {
                    side: BoundarySide::Left,
                    route_direction: Direction::RL,
                });
            }
        }
        if above && right {
            let vertical_gap = source_bounds
                .y
                .saturating_sub(target.y.saturating_add(target.height));
            let horizontal_gap = target
                .x
                .saturating_sub(source_bounds.x.saturating_add(source_bounds.width));
            if vertical_gap
                < sibling_subgraph_fan_in_identity::required_primary_gap(
                    sibling_subgraph_fan_in_identity::TARGET_PORT_COUNT,
                )
                || horizontal_gap <= vertical_gap
            {
                return Some(SceneGeometry {
                    side: BoundarySide::Right,
                    route_direction: Direction::LR,
                });
            }
        }
        if above && left {
            let vertical_gap = source_bounds
                .y
                .saturating_sub(target.y.saturating_add(target.height));
            let horizontal_gap = source_bounds
                .x
                .saturating_sub(target.x.saturating_add(target.width));
            if vertical_gap
                < sibling_subgraph_fan_in_identity::required_primary_gap(
                    sibling_subgraph_fan_in_identity::TARGET_PORT_COUNT,
                )
                || horizontal_gap <= vertical_gap
            {
                return Some(SceneGeometry {
                    side: BoundarySide::Left,
                    route_direction: Direction::RL,
                });
            }
        }
        if below {
            return Some(SceneGeometry {
                side: BoundarySide::Bottom,
                route_direction: Direction::TD,
            });
        }
        if above {
            return Some(SceneGeometry {
                side: BoundarySide::Top,
                route_direction: Direction::BT,
            });
        }
    } else {
        // Prefer the physically separated vertical corridor when a terminal
        // target is below/above the source envelope and the vertical gap is
        // shorter than the horizontal detour.  This avoids making a source
        // node that is already a database target leave through the same side
        // as its incoming arrow, which collapses two meanings into one
        // border junction in LR/RL diagrams.
        if graph_direction == Direction::RL
            && below
            && target
                .y
                .saturating_sub(source_bounds.y + source_bounds.height)
                <= target
                    .x
                    .saturating_sub(source_bounds.x + source_bounds.width)
        {
            return Some(SceneGeometry {
                side: BoundarySide::Bottom,
                route_direction: Direction::TD,
            });
        }
        if graph_direction == Direction::RL
            && above
            && source_bounds.y.saturating_sub(target.y + target.height)
                <= target
                    .x
                    .saturating_sub(source_bounds.x + source_bounds.width)
        {
            return Some(SceneGeometry {
                side: BoundarySide::Top,
                route_direction: Direction::BT,
            });
        }
        if right {
            return Some(SceneGeometry {
                side: BoundarySide::Right,
                route_direction: Direction::LR,
            });
        }
        if left {
            return Some(SceneGeometry {
                side: BoundarySide::Left,
                route_direction: Direction::RL,
            });
        }
    }

    // If the preferred axis is not separated, accept a clean separation on
    // the other axis.  Diagonal layouts use the graph orientation only as a
    // deterministic tie-breaker above; the actual route direction remains
    // derived from the selected physical side.
    let separated = [right, left, below, above]
        .into_iter()
        .filter(|value| *value)
        .count();
    if separated != 1 {
        return None;
    }
    if right {
        Some(SceneGeometry {
            side: BoundarySide::Right,
            route_direction: Direction::LR,
        })
    } else if left {
        Some(SceneGeometry {
            side: BoundarySide::Left,
            route_direction: Direction::RL,
        })
    } else if below {
        Some(SceneGeometry {
            side: BoundarySide::Bottom,
            route_direction: Direction::TD,
        })
    } else if above {
        Some(SceneGeometry {
            side: BoundarySide::Top,
            route_direction: Direction::BT,
        })
    } else {
        None
    }
}

fn target_entry_points(target: &Node, direction: Direction) -> Vec<(usize, usize)> {
    let count = sibling_subgraph_fan_in_identity::TARGET_PORT_COUNT;
    match direction {
        Direction::TD | Direction::TB => target_port_columns(target.x, target.width, count)
            .into_iter()
            .map(|x| (x, target.y.saturating_sub(1)))
            .collect(),
        Direction::BT => target_port_columns(target.x, target.width, count)
            .into_iter()
            .map(|x| (x, target.bottom_y()))
            .collect(),
        Direction::LR => target_port_rows(target.y, target.height, count)
            .into_iter()
            .map(|y| (target.x.saturating_sub(1), y))
            .collect(),
        Direction::RL => target_port_rows(target.y, target.height, count)
            .into_iter()
            .map(|y| (target.x.saturating_add(target.width), y))
            .collect(),
    }
}

fn boundary_point(
    bounds: &crate::graph::Rectangle,
    side: BoundarySide,
    lane: usize,
) -> (usize, usize) {
    match side {
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

/// Pick a distinct boundary lane that keeps the complete source-to-portal
/// corridor clear of the source subgraph title.  A source node's center can
/// legitimately land on the last title cell in a compact BT envelope; moving
/// the portal one interior cell over is enough, but only when the route is
/// planned with the explicit secondary alignment segment below.
fn select_portal_lane(
    request: &PortalLaneRequest<'_>,
    source: &Node,
    desired_lane: usize,
    target_lane: usize,
    ordinal: usize,
) -> Option<usize> {
    let mut lanes = match request.side {
        BoundarySide::Top | BoundarySide::Bottom => (request.bounds.x
            ..request.bounds.x.saturating_add(request.bounds.width))
            .collect::<Vec<_>>(),
        BoundarySide::Left | BoundarySide::Right => (request.bounds.y
            ..request.bounds.y.saturating_add(request.bounds.height))
            .collect::<Vec<_>>(),
    };
    if !request.used.is_empty() {
        let interior_min = request.bounds.y.saturating_add(1);
        let interior_max = request
            .bounds
            .y
            .saturating_add(request.bounds.height.saturating_sub(2));
        if matches!(request.side, BoundarySide::Left | BoundarySide::Right) {
            let interior = lanes
                .iter()
                .copied()
                .filter(|lane| *lane >= interior_min && *lane <= interior_max)
                .collect::<Vec<_>>();
            if !interior.is_empty() {
                lanes = interior;
            }
            let source_clear_lane =
                if target_lane >= request.bounds.y.saturating_add(request.bounds.height) {
                    request
                        .bounds
                        .y
                        .saturating_add(request.bounds.height.saturating_sub(2))
                        .saturating_sub(ordinal)
                } else if target_lane <= request.bounds.y {
                    request.bounds.y.saturating_add(1).saturating_add(ordinal)
                } else if target_lane >= desired_lane {
                    desired_lane.saturating_add(2)
                } else {
                    desired_lane.saturating_sub(2)
                };
            lanes.sort_by_key(|lane| {
                (
                    (!request.prefer_desired_lane || *lane != desired_lane),
                    lane.abs_diff(source_clear_lane),
                    lane.abs_diff(target_lane),
                    lane.abs_diff(desired_lane),
                    *lane,
                )
            });
        } else {
            lanes.sort_by_key(|lane| (lane.abs_diff(desired_lane), *lane));
        }
    } else {
        lanes.sort_by_key(|lane| (lane.abs_diff(desired_lane), *lane));
    }

    let coords = OrientedCoords::new(request.direction);
    let source_exit = edge_exit_point(source, request.direction);
    for lane in lanes {
        if request.used.contains(&lane) {
            continue;
        }
        let portal = boundary_point(request.bounds, request.side, lane);
        let departure = coords.advance(source_exit.0, source_exit.1, 1);
        let source_turn = coords.with_secondary(departure.0, departure.1, lane);
        let title_crossed = primary_cells(source_exit, departure, request.direction)
            .into_iter()
            .chain(secondary_cells(departure, source_turn, request.direction))
            .chain(primary_cells(source_turn, portal, request.direction))
            .any(|(x, y)| is_subgraph_title_cell(request.graph, x, y));
        if !title_crossed {
            return Some(lane);
        }
    }
    None
}

fn merge_side_lanes<I>(slots: &mut PortalSlots, side: BoundarySide, lanes: I)
where
    I: IntoIterator<Item = usize>,
{
    let target = match side {
        BoundarySide::Top => &mut slots.top,
        BoundarySide::Bottom => &mut slots.bottom,
        BoundarySide::Left => &mut slots.left,
        BoundarySide::Right => &mut slots.right,
    };
    target.extend(lanes);
}

fn replace_side_lanes(
    slots: &mut PortalSlots,
    side: BoundarySide,
    lanes: impl IntoIterator<Item = usize>,
) {
    let target = match side {
        BoundarySide::Top => &mut slots.top,
        BoundarySide::Bottom => &mut slots.bottom,
        BoundarySide::Left => &mut slots.left,
        BoundarySide::Right => &mut slots.right,
    };
    target.clear();
    target.extend(lanes);
}

fn incoming_context_side(direction: Direction) -> BoundarySide {
    match direction {
        Direction::TD | Direction::TB => BoundarySide::Top,
        Direction::BT => BoundarySide::Bottom,
        Direction::LR => BoundarySide::Left,
        Direction::RL => BoundarySide::Right,
    }
}

fn push_source_alignment_corner(
    plan: &mut FallbackRoutePlan,
    departure: (usize, usize),
    source_turn: (usize, usize),
    direction: Direction,
    style: &StyleChars,
) {
    if departure == source_turn {
        return;
    }
    let coords = OrientedCoords::new(direction);
    let going_before = coords.secondary_coord(departure.0, departure.1)
        > coords.secondary_coord(source_turn.0, source_turn.1);
    plan.push_corner(
        departure.0,
        departure.1,
        coords.corner_start_to_secondary(going_before, style),
    );
    plan.push_corner(
        source_turn.0,
        source_turn.1,
        coords.corner_secondary_to_end(going_before, style),
    );
}

fn scene_owner_id(scene: &Scene) -> String {
    format!(
        "scene:{STRATEGY}:{}->{}",
        scene.source_subgraph_id, scene.target_id
    )
}

fn canvas_blocker(plan: &ScenePlan, graph: &Graph, canvas: &Canvas) -> Option<String> {
    let source_exits: HashSet<_> = plan
        .target_routes
        .iter()
        .map(|route| route.source_exit)
        .collect();
    let boundary_claims: HashSet<_> = plan
        .route_plan
        .boundary_claims
        .iter()
        .map(|claim| (claim.x, claim.y))
        .collect();
    let target_entries: HashSet<_> = plan
        .target_routes
        .iter()
        .map(|route| route.target_entry)
        .collect();
    for (x, y) in plan.route_plan.planned_cells() {
        if is_subgraph_title_cell(graph, x, y) {
            return Some(format!("route crosses subgraph title at ({x},{y})"));
        }
        let existing = canvas.get(x, y);
        if existing == ' '
            || source_exits.contains(&(x, y))
            || boundary_claims.contains(&(x, y))
            || target_entries.contains(&(x, y))
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

fn lower_plan(plan: ScenePlan, graph: &Graph, canvas: &mut Canvas, style: &StyleChars) {
    let coords = OrientedCoords::new(plan.route_direction);
    let scene_owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: plan.owner_id.as_str(),
    };
    canvas.set_write_stage("edge-route-sibling-subgraph-fan-in");
    canvas.record_fallback_route_plan(plan.route_plan.clone());
    for route in &plan.target_routes {
        let edge_owner = RouteOwner {
            kind: CellOwnerKind::EdgeSegment,
            id: route.edge_owner_id.as_str(),
        };
        draw_line_primary(
            route.source_exit.0,
            route.source_exit.1,
            route.source_departure.0,
            route.source_departure.1,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(edge_owner),
        );
        draw_line_secondary(
            route.source_departure.0,
            route.source_departure.1,
            route.source_turn.0,
            route.source_turn.1,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(edge_owner),
        );
        draw_line_primary(
            route.source_turn.0,
            route.source_turn.1,
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
    }
    for corner in &plan.route_plan.corners {
        set_route_char(
            canvas,
            corner.point.x,
            corner.point.y,
            corner.glyph,
            Some(scene_owner),
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
    for (route, edge) in plan.target_routes.iter().zip(plan.edges.iter()) {
        let edge_owner = RouteOwner {
            kind: CellOwnerKind::EdgeSegment,
            id: edge.owner_id.as_str(),
        };
        set_route_char(
            canvas,
            route.target_entry.0,
            route.target_entry.1,
            coords.arrow_end(style),
            Some(edge_owner),
        );
    }
}

fn identity_turns(
    outside: (usize, usize),
    target_entry: (usize, usize),
    index: usize,
    direction: Direction,
    coords: &OrientedCoords,
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
        // Keep the upper/first target channel closest to the target and move
        // subsequent channels toward the source.  If channels grow from the
        // source side, the second vertical rail crosses the first target's
        // final horizontal segment at the target-entry row.
        Direction::LR => {
            target_primary.saturating_sub(1usize.saturating_add(index.saturating_mul(2)))
        }
        Direction::RL => {
            target_primary.saturating_add(1usize.saturating_add(index.saturating_mul(2)))
        }
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
    let coords = OrientedCoords::new(direction);
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
    let mut cells = BTreeSet::new();
    match direction {
        Direction::TD | Direction::TB | Direction::BT => {
            for x in from.0.min(to.0)..=from.0.max(to.0) {
                cells.insert((x, from.1));
            }
        }
        Direction::LR | Direction::RL => {
            for y in from.1.min(to.1)..=from.1.max(to.1) {
                cells.insert((from.0, y));
            }
        }
    }
    cells
}
