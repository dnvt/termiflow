//! Scene-owned TD target ports for mixed internal/cross sibling arrivals.
//!
//! The exact collision scene has one internal arrival and one sibling
//! crossing into the same target. Generic convergence intentionally merges
//! those arrivals, so this lowerer proves two target entries on a clone before
//! claiming the complete four-edge scene.

use std::collections::{HashMap, HashSet};

use crate::graph::{Direction, Graph, Node, Subgraph};
use crate::orientation::{is_before, OrientedCoords};
use crate::portals::PortalSlots;
use crate::spacing::SpacingConfig;
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::fallback_route::FallbackRoutePlan;
use super::super::provenance::edge_owner_id;
use super::super::semantic::CellOwnerKind;
use super::edge_primitives::{
    draw_line_primary, draw_line_secondary, edge_entry_candidates, edge_exit_point,
    is_subgraph_title_cell,
};
use super::subgraph::lower_td_fallback_plan;
use super::{set_route_char, RouteOwner};

const STRATEGY: &str = "td-sibling-target-entry-identity";
const PREFERRED_TARGET_ENTRY_GAP: usize = 3;
const MINIMUM_TARGET_ENTRY_GAP: usize = 2;

/// Reserve the exact TD mixed target scene as one transaction.
pub(crate) fn plan_td_sibling_target_scene(
    graph: &Graph,
    canvas: &mut Canvas,
    style: &StyleChars,
    _spacing: &SpacingConfig,
    portal_slots: &mut HashMap<String, PortalSlots>,
) -> HashSet<usize> {
    let Some(scene) = crate::render::sibling_target_entry_identity::td_scene(graph) else {
        if crate::runtime::current().diagnostics.timing {
            eprintln!(
                "  {STRATEGY} selector rejected direction={:?} subgraphs={} nodes={} edges={}",
                graph.direction,
                graph.subgraphs.len(),
                graph.nodes.len(),
                graph.edges.len()
            );
        }
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
    let Some(source_end) = graph.get_node(&scene.source_end_node_id) else {
        return reject_scene(canvas, &owner_id, "source end node disappeared");
    };
    let Some(source_start) = graph.get_node(&scene.source_start_node_id) else {
        return reject_scene(canvas, &owner_id, "source start node disappeared");
    };
    let Some(target_start) = graph.get_node(&scene.target_start_node_id) else {
        return reject_scene(canvas, &owner_id, "target start node disappeared");
    };
    let Some(target_end) = graph.get_node(&scene.target_end_node_id) else {
        return reject_scene(canvas, &owner_id, "target end node disappeared");
    };
    let Some(source_subgraph) = graph.get_subgraph(&scene.source_subgraph_id) else {
        return reject_scene(canvas, &owner_id, "source subgraph disappeared");
    };
    let Some(target_subgraph) = graph.get_subgraph(&scene.target_subgraph_id) else {
        return reject_scene(canvas, &owner_id, "target subgraph disappeared");
    };
    if !source_subgraph.bounds.is_valid() || !target_subgraph.bounds.is_valid() {
        return reject_scene(canvas, &owner_id, "scene subgraph bounds are invalid");
    }

    // The selector proves the complete four-edge topology. This transaction
    // owns the mixed-target arrivals A -> C, C -> D, and B -> D; the ordinary
    // A -> B edge remains on the single-edge lowerer and cannot be rewritten
    // by this partial scene reservation.
    let edge_indexes = [
        scene.source_internal_edge_index,
        scene.target_internal_edge_index,
        scene.start_cross_edge_index,
        scene.end_cross_edge_index,
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

    let target_candidates = edge_entry_candidates(target_end, Direction::TD)
        .into_iter()
        .filter(|(x, y)| {
            *x < canvas.width
                && *y < canvas.height
                && !is_subgraph_title_cell(graph, *x, y.saturating_sub(1))
        })
        .collect::<Vec<_>>();
    let mut target_ports = None;
    for minimum_gap in [PREFERRED_TARGET_ENTRY_GAP, MINIMUM_TARGET_ENTRY_GAP] {
        let mut best = None;
        for (left_index, left) in target_candidates.iter().enumerate() {
            for right in target_candidates.iter().skip(left_index + 1) {
                let gap = left.0.abs_diff(right.0);
                if gap < minimum_gap {
                    continue;
                }
                let distance = left.0.abs_diff(target_end.center_x())
                    + right.0.abs_diff(target_end.center_x());
                let pair = if left.0 <= right.0 {
                    (*left, *right)
                } else {
                    (*right, *left)
                };
                // Prefer the pair closest to the title-safe right-side
                // portal when centered pairs tie.  This shortens the
                // target-side bridge without changing the required gap.
                let score = (
                    distance,
                    gap,
                    usize::MAX.saturating_sub(pair.1 .0),
                    usize::MAX.saturating_sub(pair.0 .0),
                );
                if best
                    .as_ref()
                    .is_none_or(|(best_score, _)| score < *best_score)
                {
                    best = Some((score, pair));
                }
            }
        }
        if let Some((_, pair)) = best {
            target_ports = Some(vec![pair.0, pair.1]);
            break;
        }
    }
    let Some(target_ports) = target_ports else {
        return reject_scene(
            canvas,
            &owner_id,
            "target has no separated title-safe entries",
        );
    };

    let baseline = canvas.clone();
    let debug = crate::runtime::current().diagnostics.timing;
    for (internal_port, cross_port) in [
        (target_ports[0], target_ports[1]),
        (target_ports[1], target_ports[0]),
    ] {
        let mut simulation = baseline.clone();
        simulation.set_write_stage("td-sibling-target-entry-simulation");

        let start_cross_owner_id = format!("{owner_id}:start-cross");
        let start_cross_owner = RouteOwner {
            kind: CellOwnerKind::EdgeSegment,
            id: start_cross_owner_id.as_str(),
        };
        let start_cross_arrow = edge_entry_candidates(target_start, Direction::TD)
            .into_iter()
            .find(|(x, y)| {
                *x < canvas.width
                    && *y < canvas.height
                    && !is_subgraph_title_cell(graph, *x, y.saturating_sub(1))
            });
        let Some((start_cross_arrow_x, start_cross_arrow_y)) = start_cross_arrow else {
            continue;
        };
        let Some(start_source_portal_x) = nearest_portal_slot(
            portal_slots,
            &source_subgraph.id,
            "bottom",
            source_start.center_x(),
        ) else {
            continue;
        };
        let Some(start_target_portal_x) = nearest_portal_slot(
            portal_slots,
            &target_subgraph.id,
            "top",
            start_cross_arrow_x,
        ) else {
            continue;
        };
        let source_border_y = source_subgraph
            .bounds
            .y
            .saturating_add(source_subgraph.bounds.height.saturating_sub(1));
        let target_border_y = target_subgraph.bounds.y;
        let start_corridor_y = source_border_y.saturating_add(1);
        let before_start_cross = simulation.clone();
        let (start_source_exit_x, _) = edge_exit_point(source_start, Direction::TD);
        if !route_td_sibling_cross(
            source_start,
            start_source_portal_x,
            start_target_portal_x,
            start_corridor_y,
            start_source_exit_x,
            start_cross_arrow_x,
            start_cross_arrow_y,
            source_border_y,
            target_border_y,
            start_cross_arrow_y.saturating_sub(1),
            &mut simulation,
            style,
            graph,
            start_cross_owner,
        ) {
            if debug {
                eprintln!("  {STRATEGY} start cross route rejected");
            }
            continue;
        }
        if writes_existing_non_boundary(
            &before_start_cross,
            &simulation,
            graph,
            start_cross_owner.id,
        ) {
            if debug {
                eprintln!("  {STRATEGY} start cross route collides with an existing cell");
            }
            continue;
        }

        let source_internal_owner_id = format!("{owner_id}:source-internal");
        let source_internal_owner = RouteOwner {
            kind: CellOwnerKind::EdgeSegment,
            id: source_internal_owner_id.as_str(),
        };
        let Some((source_internal_arrow_x, source_internal_arrow_y)) =
            edge_entry_candidates(source_end, Direction::TD)
                .into_iter()
                .next()
        else {
            continue;
        };
        if !route_td_shared_branch(
            source_start,
            source_internal_arrow_x,
            source_internal_arrow_y,
            &mut simulation,
            style,
            graph,
            source_internal_owner,
        ) {
            if debug {
                eprintln!("  {STRATEGY} source internal route rejected");
            }
            continue;
        }
        if writes_existing_non_boundary(&baseline, &simulation, graph, source_internal_owner.id) {
            if debug {
                eprintln!("  {STRATEGY} source internal route collides with an existing cell");
            }
            continue;
        }

        let internal_owner_id = format!("{owner_id}:internal");
        let internal_owner = RouteOwner {
            kind: CellOwnerKind::EdgeSegment,
            id: internal_owner_id.as_str(),
        };
        if !route_td_internal_target(
            target_start,
            internal_port.0,
            internal_port.1,
            &mut simulation,
            style,
            graph,
            internal_owner,
        ) {
            if debug {
                eprintln!(
                    "  {STRATEGY} internal target route rejected at x={}",
                    internal_port.0
                );
            }
            continue;
        }
        if writes_existing_non_boundary(&before_start_cross, &simulation, graph, internal_owner.id)
        {
            if debug {
                eprintln!("  {STRATEGY} internal route collides with an existing cell");
            }
            continue;
        }

        let (cross_source_x, _) = edge_exit_point(source_end, Direction::TD);
        let Some(end_source_portal_x) = nearest_portal_slot(
            portal_slots,
            &source_subgraph.id,
            "bottom",
            source_end.center_x(),
        ) else {
            continue;
        };
        let Some(end_target_portal_x) =
            nearest_portal_slot(portal_slots, &target_subgraph.id, "top", cross_port.0)
        else {
            continue;
        };
        let cross_owner_id = format!("{owner_id}:cross");
        let cross_owner = RouteOwner {
            kind: CellOwnerKind::EdgeSegment,
            id: cross_owner_id.as_str(),
        };
        let before_cross = simulation.clone();
        if !route_td_sibling_cross(
            source_end,
            end_source_portal_x,
            end_target_portal_x,
            target_border_y.saturating_sub(1),
            cross_source_x,
            cross_port.0,
            cross_port.1,
            source_border_y,
            target_border_y,
            // Keep one clear row above C's top border before the B -> D
            // branch turns toward the D entry. A bridge on C's top-adjacent
            // row creates a false tee at the left corner of C.
            target_start.y.saturating_sub(2),
            &mut simulation,
            style,
            graph,
            cross_owner,
        ) {
            if debug {
                eprintln!(
                    "  {STRATEGY} cross target route rejected at x={}",
                    cross_port.0
                );
            }
            continue;
        }
        if writes_existing_non_boundary(&before_cross, &simulation, graph, cross_owner.id) {
            if debug {
                eprintln!("  {STRATEGY} cross route collides with a prior route cell");
            }
            continue;
        }
        set_route_char(
            &mut simulation,
            cross_port.0,
            cross_port.1,
            style.arrow_down,
            Some(cross_owner),
        );

        // Portal carving may leave a legal slot glyph unchanged in the
        // baseline canvas, so the route helper has no glyph delta to expose
        // at that border. Reassert the two topology-owned openings in the
        // scene simulation; the committed plan then carries their ownership
        // explicitly instead of relying on a later border pass.
        let source_portal_y = source_subgraph
            .bounds
            .y
            .saturating_add(source_subgraph.bounds.height.saturating_sub(1));
        if let Some(source_portal_x) = nearest_portal_slot(
            portal_slots,
            &source_subgraph.id,
            "bottom",
            source_start.center_x(),
        ) {
            set_route_char(
                &mut simulation,
                source_portal_x,
                source_portal_y,
                style.edge_v,
                Some(start_cross_owner),
            );
        }
        if let Some(target_portal_x) = nearest_portal_slot(
            portal_slots,
            &target_subgraph.id,
            "top",
            start_cross_arrow_x,
        ) {
            set_route_char(
                &mut simulation,
                target_portal_x,
                target_subgraph.bounds.y,
                style.edge_v,
                Some(start_cross_owner),
            );
        }
        if let Some(source_portal_x) = nearest_portal_slot(
            portal_slots,
            &source_subgraph.id,
            "bottom",
            source_end.center_x(),
        ) {
            set_route_char(
                &mut simulation,
                source_portal_x,
                source_portal_y,
                style.edge_v,
                Some(cross_owner),
            );
        }
        if let Some(target_portal_x) =
            nearest_portal_slot(portal_slots, &target_subgraph.id, "top", cross_port.0)
        {
            set_route_char(
                &mut simulation,
                target_portal_x,
                target_subgraph.bounds.y,
                style.edge_v,
                Some(cross_owner),
            );
        }

        if simulation.get(internal_port.0, internal_port.1) != style.arrow_down
            || simulation.get(cross_port.0, cross_port.1) != style.arrow_down
            || internal_port.0 == cross_port.0
        {
            if debug {
                eprintln!("  {STRATEGY} target arrow verification failed");
            }
            continue;
        }
        if changed_scene_cell_enters_node_or_title(&baseline, &simulation, graph) {
            if debug {
                eprintln!("  {STRATEGY} candidate enters node or title cell");
            }
            continue;
        }

        let mut plan = FallbackRoutePlan::new(owner_id.clone(), STRATEGY);
        plan.set_scene_coverage(covered_edge_ids.clone());
        let mut source_portals = HashSet::new();
        let mut target_portals = HashSet::new();
        let mut claimed_boundary_points = HashSet::new();
        for subgraph in [source_subgraph, target_subgraph] {
            for y in subgraph.bounds.y
                ..=subgraph
                    .bounds
                    .y
                    .saturating_add(subgraph.bounds.height.saturating_sub(1))
            {
                for x in subgraph.bounds.x
                    ..=subgraph
                        .bounds
                        .x
                        .saturating_add(subgraph.bounds.width.saturating_sub(1))
                {
                    let Some(side) = scene_boundary_side(subgraph, x, y) else {
                        continue;
                    };
                    let owned_by_cross = simulation
                        .get_meta(x, y)
                        .and_then(|meta| meta.owner_id.as_deref())
                        .is_some_and(|owner_id| {
                            owner_id == cross_owner.id || owner_id == start_cross_owner.id
                        });
                    if owned_by_cross && simulation.get(x, y) == style.edge_v {
                        plan.claim_boundary(subgraph.id.clone(), side, x, y, style.edge_v);
                        claimed_boundary_points.insert((x, y));
                        if std::ptr::eq(subgraph, source_subgraph) {
                            source_portals.insert(x);
                        } else {
                            target_portals.insert(x);
                        }
                    }
                }
            }
        }
        if source_portals.is_empty() {
            if let Some(x) = nearest_portal_slot(
                portal_slots,
                &source_subgraph.id,
                "bottom",
                source_start.center_x(),
            ) {
                let y = source_subgraph
                    .bounds
                    .y
                    .saturating_add(source_subgraph.bounds.height.saturating_sub(1));
                if simulation.get(x, y) == style.edge_v {
                    plan.claim_boundary(source_subgraph.id.clone(), "bottom", x, y, style.edge_v);
                    claimed_boundary_points.insert((x, y));
                    source_portals.insert(x);
                }
            }
        }
        if target_portals.is_empty() {
            if let Some(x) = nearest_portal_slot(
                portal_slots,
                &target_subgraph.id,
                "top",
                start_cross_arrow_x,
            ) {
                let y = target_subgraph.bounds.y;
                if simulation.get(x, y) == style.edge_v {
                    plan.claim_boundary(target_subgraph.id.clone(), "top", x, y, style.edge_v);
                    claimed_boundary_points.insert((x, y));
                    target_portals.insert(x);
                }
            }
        }
        for paint in simulation.non_space_delta(&baseline) {
            if claimed_boundary_points.contains(&(paint.point.x, paint.point.y)) {
                continue;
            }
            if let Some(side) = scene_boundary_side(source_subgraph, paint.point.x, paint.point.y) {
                if paint.glyph == style.edge_v {
                    plan.claim_boundary(
                        source_subgraph.id.clone(),
                        side,
                        paint.point.x,
                        paint.point.y,
                        style.edge_v,
                    );
                    source_portals.insert(paint.point.x);
                    continue;
                }
            }
            if let Some(side) = scene_boundary_side(target_subgraph, paint.point.x, paint.point.y) {
                if paint.glyph == style.edge_v {
                    plan.claim_boundary(
                        target_subgraph.id.clone(),
                        side,
                        paint.point.x,
                        paint.point.y,
                        style.edge_v,
                    );
                    target_portals.insert(paint.point.x);
                    continue;
                }
            }
            plan.push_paint(paint.point.x, paint.point.y, paint.glyph);
        }
        if source_portals.is_empty() || target_portals.is_empty() {
            if debug {
                eprintln!("  {STRATEGY} candidate has no explicit portal claims");
            }
            continue;
        }

        if let Some(reason) = plan.validation_error(canvas.width, canvas.height) {
            if debug {
                eprintln!("  {STRATEGY} plan rejected: {reason}");
            }
            continue;
        }

        let mut committed = baseline.clone();
        committed.set_write_stage("td-sibling-target-entry-commit-simulation");
        lower_td_fallback_plan(plan.clone(), &mut committed, style, Some(owner));
        if !same_glyphs(&committed, &simulation) {
            if debug {
                eprintln!("  {STRATEGY} committed plan differs from simulation");
            }
            continue;
        }

        let boundary_claims = plan.boundary_claims.clone();
        canvas.set_write_stage("td-sibling-target-entry");
        lower_td_fallback_plan(plan, canvas, style, Some(owner));
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
        "no collision-free separated TD target entries",
    )
}

#[allow(clippy::too_many_arguments)]
fn route_td_internal_target(
    source: &Node,
    arrow_x: usize,
    arrow_y: usize,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: &Graph,
    owner: RouteOwner<'_>,
) -> bool {
    let coords = OrientedCoords::new(Direction::TD);
    let source_exit = edge_exit_point(source, Direction::TD);
    if arrow_x >= canvas.width
        || arrow_y >= canvas.height
        || source_exit.1 >= arrow_y.saturating_sub(1)
    {
        return false;
    }

    let target_turn = (arrow_x, arrow_y.saturating_sub(1));
    if source_exit.0 == arrow_x {
        draw_line_primary(
            source_exit.0,
            source_exit.1,
            arrow_x,
            arrow_y,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
    } else {
        let source_turn = (source_exit.0, target_turn.1);
        draw_line_primary(
            source_exit.0,
            source_exit.1,
            source_turn.0,
            source_turn.1,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
        super::edge_primitives::draw_line_secondary(
            source_turn.0,
            source_turn.1,
            target_turn.0,
            target_turn.1,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
        draw_line_primary(
            arrow_x,
            target_turn.1.saturating_add(1),
            arrow_x,
            arrow_y,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
        let going_before = is_before(source_exit.0, arrow_x);
        set_route_char(
            canvas,
            source_turn.0,
            source_turn.1,
            coords.corner_start_to_secondary(going_before, style),
            Some(owner),
        );
        set_route_char(
            canvas,
            target_turn.0,
            target_turn.1,
            coords.corner_secondary_to_end(going_before, style),
            Some(owner),
        );
    }
    set_route_char(canvas, arrow_x, arrow_y, style.arrow_down, Some(owner));
    true
}

/// Continue a source stem that is already shared with a sibling crossing, and
/// branch one internal edge to its same-subgraph target. The source junction
/// is a real three-arm tee: the shared stem continues down while the internal
/// edge leaves to the target on the left or right.
#[allow(clippy::too_many_arguments)]
fn route_td_shared_branch(
    source: &Node,
    arrow_x: usize,
    arrow_y: usize,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: &Graph,
    owner: RouteOwner<'_>,
) -> bool {
    let coords = OrientedCoords::new(Direction::TD);
    let (source_x, source_y) = edge_exit_point(source, Direction::TD);
    let branch_y = arrow_y.saturating_sub(1);
    if source_x >= canvas.width
        || arrow_x >= canvas.width
        || arrow_y >= canvas.height
        || source_y >= branch_y
        || source_x == arrow_x
    {
        return false;
    }

    draw_line_primary(
        source_x,
        source_y,
        source_x,
        branch_y,
        &coords,
        canvas,
        style,
        Some(graph),
        Some(owner),
    );
    draw_line_secondary(
        source_x,
        branch_y,
        arrow_x,
        branch_y,
        &coords,
        canvas,
        style,
        Some(graph),
        Some(owner),
    );
    set_route_char(
        canvas,
        source_x,
        branch_y,
        if source_x > arrow_x {
            style.junction_left
        } else {
            style.junction_right
        },
        Some(owner),
    );
    set_route_char(
        canvas,
        arrow_x,
        branch_y,
        coords.corner_secondary_to_end(is_before(source_x, arrow_x), style),
        Some(owner),
    );
    draw_line_primary(
        arrow_x,
        branch_y.saturating_add(1),
        arrow_x,
        arrow_y,
        &coords,
        canvas,
        style,
        Some(graph),
        Some(owner),
    );
    set_route_char(canvas, arrow_x, arrow_y, style.arrow_down, Some(owner));
    true
}

/// Lower one explicit TD sibling crossing with a corridor row selected by the
/// owning scene. Generic sibling routing is allowed to reuse a portal-centered
/// lane, but two arrivals that have distinct target identities need separate
/// physical corridors before they are allowed to approach the target.
#[allow(clippy::too_many_arguments)]
fn route_td_sibling_cross(
    source: &Node,
    source_portal_x: usize,
    target_portal_x: usize,
    corridor_y: usize,
    source_exit_x: usize,
    arrow_x: usize,
    arrow_y: usize,
    source_border_y: usize,
    target_border_y: usize,
    target_bridge_y: usize,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: &Graph,
    owner: RouteOwner<'_>,
) -> bool {
    let coords = OrientedCoords::new(Direction::TD);
    let (_, source_exit_y) = edge_exit_point(source, Direction::TD);
    if source_portal_x >= canvas.width
        || target_portal_x >= canvas.width
        || source_exit_x >= canvas.width
        || arrow_x >= canvas.width
        || arrow_y >= canvas.height
        || source_exit_y > source_border_y
        || corridor_y <= source_border_y
        || corridor_y >= target_border_y
        || target_bridge_y < target_border_y
        || target_bridge_y >= arrow_y
    {
        return false;
    }

    // Leave the source node on its existing stem, then take the selected
    // bottom portal. The exact scene currently uses aligned source portals;
    // the one-cell-outside turn keeps the helper fail-closed if that geometry
    // changes without introducing a route through a node or title.
    if source_exit_x == source_portal_x {
        draw_line_primary(
            source_exit_x,
            source_exit_y,
            source_portal_x,
            source_border_y,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
    } else {
        let source_turn_y = source_border_y.saturating_sub(1);
        if source_exit_y > source_turn_y {
            return false;
        }
        draw_line_primary(
            source_exit_x,
            source_exit_y,
            source_exit_x,
            source_turn_y,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
        draw_line_secondary(
            source_exit_x,
            source_turn_y,
            source_portal_x,
            source_turn_y,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
        let going_before = is_before(source_exit_x, source_portal_x);
        set_route_char(
            canvas,
            source_exit_x,
            source_turn_y,
            coords.corner_start_to_secondary(going_before, style),
            Some(owner),
        );
        set_route_char(
            canvas,
            source_portal_x,
            source_turn_y,
            coords.corner_secondary_to_end(going_before, style),
            Some(owner),
        );
        draw_line_primary(
            source_portal_x,
            source_turn_y,
            source_portal_x,
            source_border_y,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
    }
    set_route_char(
        canvas,
        source_portal_x,
        source_border_y,
        style.edge_v,
        Some(owner),
    );
    draw_line_primary(
        source_portal_x,
        source_border_y,
        source_portal_x,
        corridor_y,
        &coords,
        canvas,
        style,
        Some(graph),
        Some(owner),
    );

    if source_portal_x != target_portal_x {
        draw_line_secondary(
            source_portal_x,
            corridor_y,
            target_portal_x,
            corridor_y,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
        let going_before = is_before(source_portal_x, target_portal_x);
        set_route_char(
            canvas,
            source_portal_x,
            corridor_y,
            coords.corner_start_to_secondary(going_before, style),
            Some(owner),
        );
        set_route_char(
            canvas,
            target_portal_x,
            corridor_y,
            coords.corner_secondary_to_end(going_before, style),
            Some(owner),
        );
    }

    let target_vertical_start = if source_portal_x == target_portal_x {
        corridor_y
    } else {
        corridor_y.saturating_add(1)
    };
    draw_line_primary(
        target_portal_x,
        target_vertical_start,
        target_portal_x,
        target_bridge_y,
        &coords,
        canvas,
        style,
        Some(graph),
        Some(owner),
    );
    set_route_char(
        canvas,
        target_portal_x,
        target_border_y,
        style.edge_v,
        Some(owner),
    );

    if target_portal_x == arrow_x {
        draw_line_primary(
            target_portal_x,
            target_bridge_y,
            arrow_x,
            arrow_y,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
    } else {
        draw_line_secondary(
            target_portal_x,
            target_bridge_y,
            arrow_x,
            target_bridge_y,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
        let going_before = is_before(target_portal_x, arrow_x);
        set_route_char(
            canvas,
            target_portal_x,
            target_bridge_y,
            coords.corner_start_to_secondary(going_before, style),
            Some(owner),
        );
        set_route_char(
            canvas,
            arrow_x,
            target_bridge_y,
            coords.corner_secondary_to_end(going_before, style),
            Some(owner),
        );
        draw_line_primary(
            arrow_x,
            target_bridge_y.saturating_add(1),
            arrow_x,
            arrow_y,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
    }
    set_route_char(canvas, arrow_x, arrow_y, style.arrow_down, Some(owner));
    true
}

fn scene_boundary_side(subgraph: &Subgraph, x: usize, y: usize) -> Option<&'static str> {
    if !subgraph.bounds.is_valid() {
        return None;
    }
    let right = subgraph
        .bounds
        .x
        .saturating_add(subgraph.bounds.width.saturating_sub(1));
    let bottom = subgraph
        .bounds
        .y
        .saturating_add(subgraph.bounds.height.saturating_sub(1));
    if y == subgraph.bounds.y && x > subgraph.bounds.x && x < right {
        Some("top")
    } else if y == bottom && x > subgraph.bounds.x && x < right {
        Some("bottom")
    } else {
        None
    }
}

fn nearest_portal_slot(
    portal_slots: &HashMap<String, PortalSlots>,
    subgraph_id: &str,
    side: &str,
    desired: usize,
) -> Option<usize> {
    let slots = portal_slots.get(subgraph_id)?;
    let candidates = match side {
        "top" => &slots.top,
        "bottom" => &slots.bottom,
        "left" => &slots.left,
        "right" => &slots.right,
        _ => return None,
    };
    candidates
        .iter()
        .min_by_key(|candidate| (candidate.abs_diff(desired), **candidate))
        .copied()
}

fn changed_scene_cell_enters_node_or_title(
    baseline: &Canvas,
    simulation: &Canvas,
    graph: &Graph,
) -> bool {
    (0..simulation.height).any(|y| {
        (0..simulation.width).any(|x| {
            if simulation.get(x, y) == baseline.get(x, y) {
                return false;
            }
            let in_node = graph.nodes.iter().any(|node| {
                x >= node.x
                    && x < node.x.saturating_add(node.width)
                    && y >= node.y
                    && y < node.bottom_y()
            });
            in_node || is_subgraph_title_cell(graph, x, y)
        })
    })
}

fn writes_existing_non_boundary(
    before: &Canvas,
    after: &Canvas,
    graph: &Graph,
    owner_id: &str,
) -> bool {
    (0..after.height).any(|y| {
        (0..after.width).any(|x| {
            let owned_by_candidate = after
                .get_meta(x, y)
                .and_then(|meta| meta.owner_id.as_deref())
                == Some(owner_id);
            if !owned_by_candidate || before.get(x, y) == ' ' {
                return false;
            }
            let at_scene_boundary = graph
                .subgraphs
                .iter()
                .any(|subgraph| scene_boundary_side(subgraph, x, y).is_some());
            if !at_scene_boundary {
                return true;
            }
            false
        })
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
