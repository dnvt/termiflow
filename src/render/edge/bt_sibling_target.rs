//! Scene-owned target ports for a mixed BT sibling/internal convergence.
//!
//! Two sibling-subgraph crossings can enter a target that already has an
//! internal incoming edge.  Generic convergence intentionally uses one merge
//! arrow, which is correct for ordinary fan-in but loses edge identity here.
//! This module reserves the complete small scene on a clone before lowering it
//! as one typed route reservation.

use std::collections::{HashMap, HashSet};

use crate::graph::{Direction, Graph, Node, Subgraph};
use crate::portals::PortalSlots;
use crate::spacing::SpacingConfig;
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::fallback_route::FallbackRoutePlan;
use super::super::fallback_route::PortalEntryDecision;
use super::super::provenance::edge_owner_id;
use super::super::semantic::CellOwnerKind;
use super::edge_primitives::{
    adjusted_edge_entry_point, draw_line_primary, edge_entry_candidates, edge_exit_point,
};
use super::subgraph::{lower_bt_fallback_plan, route_cross_subgraph_bt, BtRouteOutcome};
use super::{route_divergent_edges, set_route_char, set_route_edge_char, RouteOwner};

const STRATEGY: &str = "bt-sibling-target-entry-identity";
const PREFERRED_TARGET_ENTRY_GAP: usize = 3;
const MINIMUM_TARGET_ENTRY_GAP: usize = 2;
const MINIMUM_SOURCE_WALL_GAP: usize = 4;

/// Reserve the exact mixed sibling/internal target scene as one transaction.
///
/// The scene owns all four edges so ordinary convergence cannot redraw the two
/// incoming edges after the target ports have been separated.  A rejected
/// candidate leaves the live canvas untouched and returns no edge indexes,
/// allowing the existing generic route to remain the conservative fallback.
pub(crate) fn plan_bt_sibling_target_scene(
    graph: &Graph,
    canvas: &mut Canvas,
    style: &StyleChars,
    spacing: &SpacingConfig,
    portal_slots: &mut HashMap<String, PortalSlots>,
) -> HashSet<usize> {
    let Some(scene) = graph.bt_sibling_target_entry_scene() else {
        return HashSet::new();
    };
    let owner_id = format!(
        "scene:{STRATEGY}:{}->{}",
        scene.source_subgraph_id, scene.target_subgraph_id
    );
    let owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: owner_id.as_str(),
    };
    let Some(source_lower) = graph.get_node(&scene.source_lower_node_id) else {
        return reject_scene(canvas, &owner_id, "source lower node disappeared");
    };
    let Some(source_upper) = graph.get_node(&scene.source_upper_node_id) else {
        return reject_scene(canvas, &owner_id, "source upper node disappeared");
    };
    let Some(target_lower) = graph.get_node(&scene.target_lower_node_id) else {
        return reject_scene(canvas, &owner_id, "target lower node disappeared");
    };
    let Some(target_upper) = graph.get_node(&scene.target_upper_node_id) else {
        return reject_scene(canvas, &owner_id, "target upper node disappeared");
    };
    let Some(target_subgraph) = graph.get_subgraph(&scene.target_subgraph_id) else {
        return reject_scene(canvas, &owner_id, "target subgraph disappeared");
    };
    let Some(source_subgraph) = graph.get_subgraph(&scene.source_subgraph_id) else {
        return reject_scene(canvas, &owner_id, "source subgraph disappeared");
    };
    if !source_subgraph.bounds.is_valid() || !target_subgraph.bounds.is_valid() {
        return reject_scene(canvas, &owner_id, "scene subgraph bounds are invalid");
    }

    let edge_indexes = [
        scene.source_internal_edge_index,
        scene.target_internal_edge_index,
        scene.lower_cross_edge_index,
        scene.upper_cross_edge_index,
    ];
    let covered_edge_ids = edge_indexes
        .iter()
        .filter_map(|index| {
            graph
                .edges
                .get(*index)
                .map(|edge| edge_owner_id(*index, edge))
        })
        .collect::<Vec<_>>();
    if covered_edge_ids.len() != edge_indexes.len() {
        return reject_scene(canvas, &owner_id, "scene edge coverage is incomplete");
    }

    let target_arrow_y = edge_entry_candidates(target_upper, Direction::BT)
        .first()
        .map(|(_, y)| *y)
        .unwrap_or_else(|| target_upper.bottom_y());
    let mut target_ports = edge_entry_candidates(target_upper, Direction::BT)
        .into_iter()
        .filter(|(_, y)| *y == target_arrow_y)
        .map(|(x, _)| x)
        .collect::<Vec<_>>();
    target_ports.sort_unstable();
    target_ports.dedup();

    let mut port_pairs = Vec::new();
    for minimum_gap in [PREFERRED_TARGET_ENTRY_GAP, MINIMUM_TARGET_ENTRY_GAP] {
        for &cross_port in &target_ports {
            for &internal_port in &target_ports {
                if cross_port >= internal_port || internal_port.abs_diff(cross_port) < minimum_gap {
                    continue;
                }
                port_pairs.push((
                    cross_port,
                    internal_port,
                    cross_port.abs_diff(target_upper.center_x())
                        + internal_port.abs_diff(target_upper.center_x()),
                ));
            }
        }
        if !port_pairs.is_empty() {
            break;
        }
    }
    port_pairs.sort_by_key(|(cross_port, internal_port, distance)| {
        (
            *distance,
            cross_port.abs_diff(target_upper.center_x()),
            *internal_port,
        )
    });

    let baseline = canvas.clone();
    let debug = crate::runtime::current().diagnostics.timing;
    for (cross_port, internal_port, _) in port_pairs {
        let mut simulation = baseline.clone();
        simulation.set_write_stage("bt-sibling-target-entry-simulation");

        // Keep both internal edges and both cross edges in this one scene
        // transaction. A generic multi-target fan-out never enters the
        // sibling-boundary planner for its cross edge, allowing a portal-
        // centred shaft to be reconstructed later. Preserve the lower source
        // fan-out junction, then hand only its cross branch to the sibling
        // boundary planner so the source and target lanes participate in the
        // same cloned ownership proof as the upper cross edge below.
        route_divergent_edges(
            source_lower,
            &[source_upper],
            &mut simulation,
            style,
            spacing,
            Direction::BT,
            graph,
        );

        let (lower_source_x, lower_source_y) = edge_exit_point(source_lower, Direction::BT);
        let (lower_arrow_x, lower_arrow_y) =
            adjusted_edge_entry_point(target_lower, Direction::BT, graph);
        // The lower source also owns an internal branch. Keep the scene's
        // cross branch at least three columns from that source lane so its
        // horizontal shoulder retains two visible shaft cells instead of
        // forming the tiny `+-+` switchback against the internal route.
        let Some(lower_cross_arrow_x) = edge_entry_candidates(target_lower, Direction::BT)
            .into_iter()
            .filter(|(candidate_x, candidate_y)| {
                *candidate_y == lower_arrow_y
                    && candidate_x.abs_diff(lower_source_x) >= PREFERRED_TARGET_ENTRY_GAP
                    // Keep the lower fan-out lane away from both physical
                    // source walls.  A widened source node can move the
                    // receiver's preferred lane onto the source border; the
                    // resulting one-cell shoulder makes the route look like
                    // an accidental `++`/`┼┘` seam even though every edge is
                    // owned by this scene.  This is intentionally scoped to
                    // the exact graph-owned four-edge topology.
                    && candidate_x.abs_diff(source_subgraph.bounds.x) >= MINIMUM_SOURCE_WALL_GAP
                    && candidate_x.abs_diff(
                        source_subgraph
                            .bounds
                            .x
                            .saturating_add(source_subgraph.bounds.width.saturating_sub(1)),
                    ) >= MINIMUM_SOURCE_WALL_GAP
            })
            .min_by_key(|(candidate_x, _)| candidate_x.abs_diff(lower_arrow_x))
            .map(|(candidate_x, _)| candidate_x)
        else {
            // Never fall back to the parity-biased receiver lane after the
            // topology-owned wall-clearance proof has rejected every
            // candidate. That would silently reintroduce the very seam this
            // scene is responsible for preventing; rejecting this port pair
            // keeps the whole transaction fail-closed.
            continue;
        };
        let lower_fanout_y = lower_source_y.saturating_sub(spacing.stem_length_vertical);
        if lower_cross_arrow_x >= simulation.width || lower_fanout_y >= simulation.height {
            continue;
        }
        if lower_source_x != lower_cross_arrow_x {
            let (left, right) = if lower_source_x < lower_cross_arrow_x {
                (lower_source_x, lower_cross_arrow_x)
            } else {
                (lower_cross_arrow_x, lower_source_x)
            };
            for x in left.saturating_add(1)..right {
                set_route_edge_char(
                    &mut simulation,
                    x,
                    lower_fanout_y,
                    style.edge_h,
                    style,
                    Some(owner),
                );
            }
            set_route_char(
                &mut simulation,
                lower_source_x,
                lower_fanout_y,
                style.junction_down,
                Some(owner),
            );
            set_route_char(
                &mut simulation,
                lower_cross_arrow_x,
                lower_fanout_y,
                style.corner_ur,
                Some(owner),
            );
        }
        let lower_cross_outcome = route_cross_subgraph_bt(
            source_lower,
            target_lower,
            lower_cross_arrow_x,
            lower_fanout_y,
            lower_cross_arrow_x,
            lower_arrow_y,
            &mut simulation,
            style,
            graph,
            Some(owner),
        );
        if lower_cross_outcome != BtRouteOutcome::Handled {
            if debug {
                eprintln!("  {STRATEGY} lower cross route outcome={lower_cross_outcome:?}");
            }
            continue;
        }
        if lower_source_x != lower_cross_arrow_x {
            set_route_char(
                &mut simulation,
                lower_source_x,
                lower_fanout_y,
                style.junction_down,
                Some(owner),
            );
            set_route_char(
                &mut simulation,
                lower_cross_arrow_x,
                lower_fanout_y,
                style.corner_ur,
                Some(owner),
            );
        }
        set_route_char(
            &mut simulation,
            lower_cross_arrow_x,
            lower_arrow_y,
            style.arrow_up,
            Some(owner),
        );

        if !draw_internal_target_route(
            target_lower,
            internal_port,
            target_arrow_y,
            &mut simulation,
            style,
            graph,
            owner,
        ) {
            if debug {
                eprintln!(
                    "  {STRATEGY} ports cross={cross_port} internal={internal_port}: internal route rejected"
                );
            }
            continue;
        }

        let (source_stem_x, source_stem_y) = edge_exit_point(source_upper, Direction::BT);
        let cross_outcome = route_cross_subgraph_bt(
            source_upper,
            target_upper,
            source_stem_x,
            source_stem_y,
            cross_port,
            target_arrow_y,
            &mut simulation,
            style,
            graph,
            Some(owner),
        );
        if cross_outcome != BtRouteOutcome::Handled {
            if debug {
                eprintln!(
                    "  {STRATEGY} ports cross={cross_port} internal={internal_port}: cross outcome={cross_outcome:?}"
                );
            }
            continue;
        }
        set_route_char(
            &mut simulation,
            cross_port,
            target_arrow_y,
            style.arrow_up,
            Some(owner),
        );

        if simulation.get(cross_port, target_arrow_y) != style.arrow_up
            || simulation.get(internal_port, target_arrow_y) != style.arrow_up
            || cross_port == internal_port
        {
            if debug {
                eprintln!(
                    "  {STRATEGY} ports cross={cross_port} internal={internal_port}: arrow verification failed"
                );
            }
            continue;
        }

        normalize_bt_scene_boundary(
            &baseline,
            &mut simulation,
            source_subgraph,
            source_subgraph.bounds.y,
            style,
            owner,
        );
        normalize_bt_scene_boundary(
            &baseline,
            &mut simulation,
            target_subgraph,
            target_subgraph
                .bounds
                .y
                .saturating_add(target_subgraph.bounds.height.saturating_sub(1)),
            style,
            owner,
        );

        let mut plan = FallbackRoutePlan::new(owner_id.clone(), STRATEGY);
        plan.set_scene_coverage(covered_edge_ids.clone());
        let delta = simulation.non_space_delta(&baseline);
        for paint in delta.iter().cloned() {
            if is_node_interior(graph, paint.point.x, paint.point.y) {
                continue;
            }
            if let Some((boundary_id, side)) = scene_boundary_at(
                source_subgraph,
                target_subgraph,
                paint.point.x,
                paint.point.y,
            ) {
                plan.claim_boundary(
                    boundary_id.to_owned(),
                    side,
                    paint.point.x,
                    paint.point.y,
                    style.edge_v,
                );
                continue;
            }
            plan.push_paint(paint.point.x, paint.point.y, paint.glyph);
        }

        // The widened lower cross branch deliberately enters the receiver on
        // `lower_cross_arrow_x`, not on the receiver's parity-biased center.
        // Carry that physical lane into the same decision record consumed by
        // portal tracing; otherwise the visual route is correct while the
        // evidence layer reconstructs the stale center column and reports a
        // false missing portal slot.
        let target_bottom = target_subgraph
            .bounds
            .y
            .saturating_add(target_subgraph.bounds.height.saturating_sub(1));
        if let Some(portal_x) = plan
            .boundary_claims
            .iter()
            .find(|claim| {
                claim.boundary_id == target_subgraph.id
                    && claim.side == "bottom"
                    && claim.y == target_bottom
                    && claim.x == lower_cross_arrow_x
            })
            .map(|claim| claim.x)
        {
            let Some(edge) = graph.edges.get(scene.lower_cross_edge_index) else {
                continue;
            };
            plan.set_target_entry_decision(PortalEntryDecision {
                edge_id: edge_owner_id(scene.lower_cross_edge_index, edge),
                owner_id: owner_id.clone(),
                target_node_id: target_lower.id.clone(),
                boundary_id: target_subgraph.id.clone(),
                side: "bottom".to_owned(),
                portal_x,
                portal_y: target_bottom,
                arrow_x: lower_cross_arrow_x,
                arrow_y: lower_arrow_y,
            });
        }

        if plan.validation_error(canvas.width, canvas.height).is_some() {
            continue;
        }
        let mut committed_simulation = baseline.clone();
        committed_simulation.set_write_stage("bt-sibling-target-entry-commit-simulation");
        let consolidated = lower_bt_fallback_plan(
            plan.clone(),
            &mut committed_simulation,
            style,
            graph,
            Some(owner),
        );
        if !consolidated || !same_glyphs(&committed_simulation, &simulation) {
            if debug {
                eprintln!(
                    "  {STRATEGY} ports cross={cross_port} internal={internal_port}: consolidated plan differs from simulation"
                );
                if consolidated {
                    let mut shown = 0;
                    for y in 0..simulation.height {
                        for x in 0..simulation.width {
                            if committed_simulation.get(x, y) != simulation.get(x, y) {
                                eprintln!(
                                    "    glyph diff ({x},{y}) committed={:?} simulation={:?} baseline={:?}",
                                    committed_simulation.get(x, y),
                                    simulation.get(x, y),
                                    baseline.get(x, y)
                                );
                                shown += 1;
                                if shown >= 12 {
                                    break;
                                }
                            }
                        }
                        if shown >= 12 {
                            break;
                        }
                    }
                }
            }
            continue;
        }

        let boundary_claims = plan.boundary_claims.clone();
        canvas.set_write_stage("bt-sibling-target-entry");
        if !lower_bt_fallback_plan(plan, canvas, style, graph, Some(owner)) {
            return reject_scene(canvas, &owner_id, "live scene lowering rejected the plan");
        }
        // This exact topology owns every external crossing on the two sibling
        // boundaries. Replace the precomputed generic slots with the physical
        // lanes proven by the scene transaction; appending would leave stale
        // center-biased seams for final portal projection to redraw beside the
        // actual receiver-aligned rails.
        portal_slots
            .entry(scene.source_subgraph_id.clone())
            .or_default()
            .top
            .clear();
        portal_slots
            .entry(scene.target_subgraph_id.clone())
            .or_default()
            .bottom
            .clear();
        for claim in boundary_claims {
            let slots = portal_slots.entry(claim.boundary_id).or_default();
            match claim.side.as_str() {
                "top" => {
                    slots.top.insert(claim.x);
                }
                "bottom" => {
                    slots.bottom.insert(claim.x);
                }
                _ => {}
            }
        }
        return edge_indexes.into_iter().collect();
    }

    reject_scene(
        canvas,
        &owner_id,
        "no collision-free separated target ports",
    )
}

fn draw_internal_target_route(
    source: &Node,
    arrow_x: usize,
    arrow_y: usize,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: &Graph,
    owner: RouteOwner<'_>,
) -> bool {
    let (source_x, source_y) = edge_exit_point(source, Direction::BT);
    if source_y <= arrow_y.saturating_add(1) || arrow_x >= canvas.width || arrow_y >= canvas.height
    {
        return false;
    }
    let turn_y = source_y.saturating_sub(1);
    draw_line_primary(
        source_x,
        source_y,
        source_x,
        turn_y,
        &crate::orientation::OrientedCoords::new(Direction::BT),
        canvas,
        style,
        Some(graph),
        Some(owner),
    );
    if source_x != arrow_x {
        let start_corner = if arrow_x > source_x {
            style.corner_dl
        } else {
            style.corner_dr
        };
        let end_corner = if arrow_x > source_x {
            style.corner_ur
        } else {
            style.corner_ul
        };
        let (left, right) = if source_x < arrow_x {
            (source_x + 1, arrow_x.saturating_sub(1))
        } else {
            (arrow_x + 1, source_x.saturating_sub(1))
        };
        for x in left..=right {
            set_route_edge_char(canvas, x, turn_y, style.edge_h, style, Some(owner));
        }
        set_route_edge_char(canvas, source_x, turn_y, start_corner, style, Some(owner));
        set_route_edge_char(canvas, arrow_x, turn_y, end_corner, style, Some(owner));
    }
    draw_line_primary(
        arrow_x,
        turn_y,
        arrow_x,
        arrow_y,
        &crate::orientation::OrientedCoords::new(Direction::BT),
        canvas,
        style,
        Some(graph),
        Some(owner),
    );
    if source_x != arrow_x {
        // The edge resolver cannot tell whether a corner's vertical leg
        // continues past the turn.  This route is scene-owned, so restore the
        // exact two-leg corners after both adjoining shafts have been drawn;
        // otherwise an endpoint is promoted to a misleading T-junction.
        let start_corner = if arrow_x > source_x {
            style.corner_dl
        } else {
            style.corner_dr
        };
        let end_corner = if arrow_x > source_x {
            style.corner_ur
        } else {
            style.corner_ul
        };
        set_route_char(canvas, source_x, turn_y, start_corner, Some(owner));
        set_route_char(canvas, arrow_x, turn_y, end_corner, Some(owner));
    }
    set_route_char(canvas, arrow_x, arrow_y, style.arrow_up, Some(owner));
    true
}

fn normalize_bt_scene_boundary(
    baseline: &Canvas,
    simulation: &mut Canvas,
    subgraph: &Subgraph,
    boundary_y: usize,
    style: &StyleChars,
    owner: RouteOwner<'_>,
) {
    if !subgraph.bounds.is_valid() || boundary_y >= simulation.height {
        return;
    }
    let right = subgraph
        .bounds
        .x
        .saturating_add(subgraph.bounds.width.saturating_sub(1));
    for x in subgraph.bounds.x.saturating_add(1)..right {
        let current = simulation.get(x, boundary_y);
        if current != ' ' && current != baseline.get(x, boundary_y) {
            set_route_char(simulation, x, boundary_y, style.edge_v, Some(owner));
        }
    }
}

fn scene_boundary_at<'a>(
    source_subgraph: &'a Subgraph,
    target_subgraph: &'a Subgraph,
    x: usize,
    y: usize,
) -> Option<(&'a str, &'static str)> {
    let source_right = source_subgraph
        .bounds
        .x
        .saturating_add(source_subgraph.bounds.width.saturating_sub(1));
    if y == source_subgraph.bounds.y && x > source_subgraph.bounds.x && x < source_right {
        return Some((source_subgraph.id.as_str(), "top"));
    }
    let target_bottom = target_subgraph
        .bounds
        .y
        .saturating_add(target_subgraph.bounds.height.saturating_sub(1));
    let target_right = target_subgraph
        .bounds
        .x
        .saturating_add(target_subgraph.bounds.width.saturating_sub(1));
    if y == target_bottom && x > target_subgraph.bounds.x && x < target_right {
        return Some((target_subgraph.id.as_str(), "bottom"));
    }
    None
}

fn is_node_interior(graph: &Graph, x: usize, y: usize) -> bool {
    graph.nodes.iter().any(|node| {
        let right = node.x.saturating_add(node.width);
        let bottom = node
            .y
            .saturating_add(node.height.max(crate::style::BOX_HEIGHT));
        x >= node.x && x < right && y >= node.y && y < bottom
    })
}

fn same_glyphs(left: &Canvas, right: &Canvas) -> bool {
    left.width == right.width
        && left.height == right.height
        && (0..left.height).all(|y| (0..left.width).all(|x| left.get(x, y) == right.get(x, y)))
}

fn reject_scene(canvas: &mut Canvas, owner_id: &str, reason: &str) -> HashSet<usize> {
    canvas.record_fallback_route_rejection(owner_id, STRATEGY, reason);
    HashSet::new()
}
