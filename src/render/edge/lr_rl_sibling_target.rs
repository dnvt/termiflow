//! Transactional LR/RL routing for the exact mixed sibling-target scene.
//!
//! The scene has one internal and one cross-subgraph arrival at D. Generic
//! horizontal convergence gives both arrivals one target marker, so this
//! lowerer owns the complete four-edge topology and reserves two D-side rows
//! before generic routing starts.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::graph::{Direction, Graph, Node, Rectangle, Subgraph};
use crate::orientation::OrientedCoords;
use crate::portals::PortalSlots;
use crate::render::fan_in_identity::target_port_rows;
use crate::render::sibling_target_entry_identity::HorizontalScene;
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::fallback_route::{FallbackAxis, FallbackRoutePlan};
use super::super::provenance::edge_owner_id;
use super::super::semantic::CellOwnerKind;
use super::edge_primitives::{edge_entry_point, edge_exit_point, is_subgraph_title_cell};
use super::{set_route_char, set_route_edge_char, RouteOwner};

const STRATEGY: &str = "lr-rl-sibling-target-entry-identity";
// Keep branch and receiver turns one quiet cell away from the source node
// and the target portal.  A one-cell turn makes the ASCII `+>+`/`<+` shoulder
// read like a box corner or a shared border seam in the mixed sibling scene.
const SOURCE_TURN_CLEARANCE: usize = 2;
const TARGET_PORTAL_CLEARANCE: usize = 2;
const INTERNAL_TARGET_TURN_CLEARANCE: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BoundarySide {
    Left,
    Right,
}

impl BoundarySide {
    fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

/// Plan and lower the exact horizontal mixed-target scene. A rejection does
/// not mutate either the canvas or the portal-slot map.
pub(crate) fn plan_lr_rl_sibling_target_scene(
    graph: &Graph,
    canvas: &mut Canvas,
    style: &StyleChars,
    portal_slots: &mut HashMap<String, PortalSlots>,
) -> HashSet<usize> {
    let Some(scene) = crate::render::sibling_target_entry_identity::horizontal_scene(graph) else {
        return HashSet::new();
    };
    let owner_id = scene_owner_id(&scene);
    let Some(source_slots) = portal_slots.get(&scene.source_subgraph_id) else {
        return reject_scene(canvas, &owner_id, "source subgraph has no portal slots");
    };
    let Some(target_slots) = portal_slots.get(&scene.target_subgraph_id) else {
        return reject_scene(canvas, &owner_id, "target subgraph has no portal slots");
    };
    if source_slots.top.is_empty()
        && source_slots.bottom.is_empty()
        && source_slots.left.is_empty()
        && source_slots.right.is_empty()
        || target_slots.top.is_empty()
            && target_slots.bottom.is_empty()
            && target_slots.left.is_empty()
            && target_slots.right.is_empty()
    {
        return reject_scene(canvas, &owner_id, "scene subgraph portal slots are empty");
    }

    let Some(plan) = build_plan(&scene, graph, canvas, style) else {
        return reject_scene(
            canvas,
            &owner_id,
            "no collision-free horizontal target plan",
        );
    };
    if let Some(reason) = plan.validation_error(canvas.width, canvas.height) {
        return reject_scene(canvas, &owner_id, &reason);
    }
    if let Some(reason) = canvas_blocker(&plan, graph, canvas) {
        return reject_scene(canvas, &owner_id, &reason);
    }

    let baseline = canvas.clone();
    let owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: owner_id.as_str(),
    };
    let mut simulation = baseline.clone();
    simulation.set_write_stage("lr-rl-sibling-target-entry-simulation");
    lower_plan(plan.clone(), &mut simulation, style, owner);
    if changed_scene_cell_enters_node_or_title(&baseline, &simulation, graph) {
        return reject_scene(canvas, &owner_id, "candidate enters a node or title cell");
    }

    let mut committed = baseline.clone();
    committed.set_write_stage("lr-rl-sibling-target-entry-commit-simulation");
    lower_plan(plan.clone(), &mut committed, style, owner);
    if !same_glyphs(&committed, &simulation) {
        return reject_scene(
            canvas,
            &owner_id,
            "commit simulation differs from candidate",
        );
    }

    let source_side = source_boundary_side(graph.direction);
    let target_side = target_boundary_side(graph.direction);
    let source_lanes = plan
        .boundary_claims
        .iter()
        .filter(|claim| claim.boundary_id == scene.source_subgraph_id)
        .map(|claim| claim.y)
        .collect::<HashSet<_>>();
    let target_lanes = plan
        .boundary_claims
        .iter()
        .filter(|claim| claim.boundary_id == scene.target_subgraph_id)
        .map(|claim| claim.y)
        .collect::<HashSet<_>>();
    if source_lanes.len() != 2 || target_lanes.len() != 2 {
        return reject_scene(
            canvas,
            &owner_id,
            "scene does not claim two source and target lanes",
        );
    }

    let Some(source_slots) = portal_slots.get_mut(&scene.source_subgraph_id) else {
        return reject_scene(canvas, &owner_id, "source portal slots disappeared");
    };
    replace_side_lanes(source_slots, source_side, source_lanes.iter().copied());
    let Some(target_slots) = portal_slots.get_mut(&scene.target_subgraph_id) else {
        return reject_scene(canvas, &owner_id, "target portal slots disappeared");
    };
    replace_side_lanes(target_slots, target_side, target_lanes.iter().copied());

    canvas.set_write_stage("lr-rl-sibling-target-entry");
    lower_plan(plan, canvas, style, owner);
    scene_edge_indexes(&scene).into_iter().collect()
}

fn build_plan(
    scene: &HorizontalScene,
    graph: &Graph,
    canvas: &Canvas,
    style: &StyleChars,
) -> Option<FallbackRoutePlan> {
    let source_start = graph.get_node(&scene.source_start_node_id)?;
    let source_end = graph.get_node(&scene.source_end_node_id)?;
    let target_start = graph.get_node(&scene.target_start_node_id)?;
    let target_end = graph.get_node(&scene.target_end_node_id)?;
    let source_subgraph = graph.get_subgraph(&scene.source_subgraph_id)?;
    let target_subgraph = graph.get_subgraph(&scene.target_subgraph_id)?;
    if !source_subgraph.bounds.is_valid() || !target_subgraph.bounds.is_valid() {
        return None;
    }
    let flow_ordered = match graph.direction {
        Direction::LR => {
            source_subgraph
                .bounds
                .x
                .saturating_add(source_subgraph.bounds.width)
                <= target_subgraph.bounds.x
        }
        Direction::RL => {
            target_subgraph
                .bounds
                .x
                .saturating_add(target_subgraph.bounds.width)
                <= source_subgraph.bounds.x
        }
        _ => false,
    };
    if !flow_ordered {
        return None;
    }

    let target_rows = target_port_rows(target_end.y, target_end.height, 2);
    if target_rows.len() != 2 || target_rows[0] >= target_rows[1] {
        return None;
    }
    let direction = graph.direction;
    let source_side = source_boundary_side(direction);
    let target_side = target_boundary_side(direction);
    // Keep the first cross-edge rail on the first target's center row. This
    // leaves a blank separation below the upper source node; an ASCII `+`
    // node corner must not visually fuse with a nearby horizontal rail.
    let source_start_lane = target_start.center_y();
    let source_end_lane = source_end.center_y();
    if source_start_lane == source_end_lane
        || !valid_side_lane(&source_subgraph.bounds, source_side, source_start_lane)
        || !valid_side_lane(&source_subgraph.bounds, source_side, source_end_lane)
        || !valid_side_lane(&target_subgraph.bounds, target_side, source_start_lane)
        || !valid_side_lane(&target_subgraph.bounds, target_side, source_end_lane)
        || is_subgraph_title_cell(
            graph,
            boundary_point(&source_subgraph.bounds, source_side, source_start_lane).0,
            boundary_point(&source_subgraph.bounds, source_side, source_start_lane).1,
        )
        || is_subgraph_title_cell(
            graph,
            boundary_point(&source_subgraph.bounds, source_side, source_end_lane).0,
            boundary_point(&source_subgraph.bounds, source_side, source_end_lane).1,
        )
        || is_subgraph_title_cell(
            graph,
            boundary_point(&target_subgraph.bounds, target_side, source_start_lane).0,
            boundary_point(&target_subgraph.bounds, target_side, source_start_lane).1,
        )
        || is_subgraph_title_cell(
            graph,
            boundary_point(&target_subgraph.bounds, target_side, source_end_lane).0,
            boundary_point(&target_subgraph.bounds, target_side, source_end_lane).1,
        )
    {
        return None;
    }

    let owner_id = scene_owner_id(scene);
    let mut plan = FallbackRoutePlan::new(owner_id, STRATEGY);
    plan.set_scene_coverage(scene_edge_indexes(scene).into_iter().filter_map(|index| {
        graph
            .edges
            .get(index)
            .map(|edge| edge_owner_id(index, edge))
    }));

    let source_start_exit = edge_exit_point(source_start, direction);
    let source_end_exit = edge_exit_point(source_end, direction);
    let target_start_entry = edge_entry_point(target_start, direction);
    let source_end_entry = edge_entry_point(source_end, direction);
    let target_end_upper = target_entry(target_end, direction, target_rows[0]);
    let target_end_lower = target_entry(target_end, direction, target_rows[1]);

    let source_start_portal =
        boundary_point(&source_subgraph.bounds, source_side, source_start_lane);
    let source_end_portal = boundary_point(&source_subgraph.bounds, source_side, source_end_lane);
    let target_start_portal =
        boundary_point(&target_subgraph.bounds, target_side, source_start_lane);
    let target_end_portal = boundary_point(&target_subgraph.bounds, target_side, source_end_lane);

    let start_cross_cells = route_cross(
        &mut plan,
        source_start_exit,
        source_start_portal,
        target_start_portal,
        target_start_entry,
        source_subgraph,
        target_subgraph,
        source_side,
        target_side,
        true,
        graph,
        style,
    )?;
    let source_internal_cells = route_internal(
        &mut plan,
        source_start_exit,
        source_end_entry,
        direction,
        style,
    )?;
    let end_cross_cells = route_cross(
        &mut plan,
        source_end_exit,
        source_end_portal,
        target_end_portal,
        target_end_upper,
        source_subgraph,
        target_subgraph,
        source_side,
        target_side,
        false,
        graph,
        style,
    )?;
    let target_internal_cells = route_internal(
        &mut plan,
        edge_exit_point(target_start, direction),
        target_end_lower,
        direction,
        style,
    )?;

    validate_route_overlap(&[
        (2, start_cross_cells),
        (0, source_internal_cells),
        (3, end_cross_cells),
        (1, target_internal_cells),
    ])?;

    plan.push_paint(
        source_end_entry.0,
        source_end_entry.1,
        OrientedCoords::new(direction).arrow_end(style),
    );
    plan.push_paint(
        target_start_entry.0,
        target_start_entry.1,
        OrientedCoords::new(direction).arrow_end(style),
    );
    plan.push_paint(
        target_end_upper.0,
        target_end_upper.1,
        OrientedCoords::new(direction).arrow_end(style),
    );
    plan.push_paint(
        target_end_lower.0,
        target_end_lower.1,
        OrientedCoords::new(direction).arrow_end(style),
    );
    if plan
        .planned_cells()
        .iter()
        .any(|(x, y)| *x >= canvas.width || *y >= canvas.height)
    {
        return None;
    }
    Some(plan)
}

#[allow(clippy::too_many_arguments)]
fn route_cross(
    plan: &mut FallbackRoutePlan,
    source_exit: (usize, usize),
    source_portal: (usize, usize),
    target_portal: (usize, usize),
    target_entry: (usize, usize),
    source_subgraph: &Subgraph,
    target_subgraph: &Subgraph,
    source_side: BoundarySide,
    target_side: BoundarySide,
    branch_at_source: bool,
    graph: &Graph,
    style: &StyleChars,
) -> Option<BTreeSet<(usize, usize)>> {
    if source_portal.1 != target_portal.1 {
        return None;
    }
    let direction = graph.direction;
    let coords = OrientedCoords::new(direction);
    let target_outside = coords.advance(target_portal.0, target_portal.1, TARGET_PORTAL_CLEARANCE);
    let target_turn = coords.with_secondary(target_outside.0, target_outside.1, target_entry.1);
    let source_bend = coords.advance(source_exit.0, source_exit.1, SOURCE_TURN_CLEARANCE);
    let source_turn = coords.with_secondary(source_bend.0, source_bend.1, source_portal.1);
    let mut cells = BTreeSet::new();
    add_primary(plan, &mut cells, source_exit, source_bend, style);
    add_secondary(plan, &mut cells, source_bend, source_turn, style);
    add_primary(plan, &mut cells, source_turn, source_portal, style);
    add_primary(
        plan,
        &mut cells,
        coords.advance(source_portal.0, source_portal.1, 1),
        target_outside,
        style,
    );
    add_secondary(plan, &mut cells, target_outside, target_turn, style);
    add_primary(plan, &mut cells, target_turn, target_entry, style);

    if source_turn != source_bend {
        let source_glyph = if branch_at_source {
            if source_portal.1 < source_exit.1 {
                style.junction_up
            } else {
                style.junction_down
            }
        } else {
            coords.corner_start_to_secondary(source_portal.1 < source_exit.1, style)
        };
        plan.push_corner(source_bend.0, source_bend.1, source_glyph);
        cells.insert(source_bend);
        plan.push_corner(
            source_turn.0,
            source_turn.1,
            coords.corner_secondary_to_end(source_portal.1 < source_exit.1, style),
        );
        cells.insert(source_turn);
    }
    let going_before = source_portal.1 > target_entry.1;
    if target_turn != target_outside {
        plan.push_corner(
            target_outside.0,
            target_outside.1,
            coords.corner_start_to_secondary(going_before, style),
        );
        cells.insert(target_outside);
    }
    if target_turn != target_outside && target_turn != target_entry {
        plan.push_corner(
            target_turn.0,
            target_turn.1,
            coords.corner_secondary_to_end(going_before, style),
        );
        cells.insert(target_turn);
    }

    let source_claim = boundary_point(&source_subgraph.bounds, source_side, source_portal.1);
    let target_claim = boundary_point(&target_subgraph.bounds, target_side, target_portal.1);
    plan.claim_boundary(
        source_subgraph.id.clone(),
        source_side.name(),
        source_claim.0,
        source_claim.1,
        style.edge_h,
    );
    plan.claim_boundary(
        target_subgraph.id.clone(),
        target_side.name(),
        target_claim.0,
        target_claim.1,
        style.edge_h,
    );
    Some(cells)
}

fn route_internal(
    plan: &mut FallbackRoutePlan,
    source_exit: (usize, usize),
    target_entry: (usize, usize),
    direction: Direction,
    style: &StyleChars,
) -> Option<BTreeSet<(usize, usize)>> {
    let coords = OrientedCoords::new(direction);
    let target_turn = coords.retreat(
        target_entry.0,
        target_entry.1,
        INTERNAL_TARGET_TURN_CLEARANCE,
    );
    let source_turn = coords.with_secondary(target_turn.0, source_exit.1, source_exit.1);
    let mut cells = BTreeSet::new();
    add_primary(plan, &mut cells, source_exit, source_turn, style);
    add_secondary(plan, &mut cells, source_turn, target_turn, style);
    add_primary(plan, &mut cells, target_turn, target_entry, style);
    if source_turn != target_turn {
        plan.push_corner(
            source_turn.0,
            source_turn.1,
            coords.corner_start_to_secondary(source_exit.1 > target_entry.1, style),
        );
        cells.insert(source_turn);
    }
    if target_turn != target_entry {
        plan.push_corner(
            target_turn.0,
            target_turn.1,
            coords.corner_secondary_to_end(source_exit.1 > target_entry.1, style),
        );
        cells.insert(target_turn);
    }
    Some(cells)
}

fn add_primary(
    plan: &mut FallbackRoutePlan,
    cells: &mut BTreeSet<(usize, usize)>,
    from: (usize, usize),
    to: (usize, usize),
    style: &StyleChars,
) {
    plan.push_horizontal(from.1, from.0, to.0, style.edge_h);
    for x in from.0.min(to.0)..=from.0.max(to.0) {
        cells.insert((x, from.1));
    }
}

fn add_secondary(
    plan: &mut FallbackRoutePlan,
    cells: &mut BTreeSet<(usize, usize)>,
    from: (usize, usize),
    to: (usize, usize),
    style: &StyleChars,
) {
    plan.push_vertical(from.0, from.1, to.1, style.edge_v);
    for y in from.1.min(to.1)..=from.1.max(to.1) {
        cells.insert((from.0, y));
    }
}

fn validate_route_overlap(routes: &[(usize, BTreeSet<(usize, usize)>)]) -> Option<()> {
    let mut owners = HashMap::new();
    for (route_index, cells) in routes {
        for cell in cells {
            if let Some(previous) = owners.insert(*cell, *route_index) {
                let shared_source_stem =
                    (previous == 0 && *route_index == 2) || (previous == 2 && *route_index == 0);
                if !shared_source_stem {
                    return None;
                }
            }
        }
    }
    Some(())
}

fn canvas_blocker(plan: &FallbackRoutePlan, graph: &Graph, canvas: &Canvas) -> Option<String> {
    let boundary_claims = plan
        .boundary_claims
        .iter()
        .map(|claim| (claim.x, claim.y))
        .collect::<HashSet<_>>();
    let source_exits = graph
        .edges
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            plan.covered_edge_ids.iter().any(|id| {
                id == &edge_owner_id(*index, graph.edges.get(*index).expect("edge exists"))
            })
        })
        .filter_map(|(_, edge)| graph.get_node(&edge.from))
        .map(|node| edge_exit_point(node, graph.direction))
        .collect::<HashSet<_>>();
    let arrows = plan
        .paints
        .iter()
        .map(|paint| (paint.point.x, paint.point.y))
        .collect::<HashSet<_>>();

    for (x, y) in plan.planned_cells() {
        if is_subgraph_title_cell(graph, x, y) {
            return Some(format!("route crosses subgraph title at ({x},{y})"));
        }
        let allowed_attachment = source_exits.contains(&(x, y)) || arrows.contains(&(x, y));
        let in_node = graph.nodes.iter().any(|node| {
            x >= node.x
                && x < node.x.saturating_add(node.width)
                && y >= node.y
                && y < node.bottom_y()
        });
        if in_node && !allowed_attachment {
            return Some(format!("route enters node at ({x},{y})"));
        }
        let on_boundary = graph
            .subgraphs
            .iter()
            .any(|subgraph| boundary_side(subgraph, x, y).is_some());
        if on_boundary && !boundary_claims.contains(&(x, y)) {
            return Some(format!(
                "route touches unclaimed subgraph boundary at ({x},{y})"
            ));
        }
        let existing = canvas.get(x, y);
        if existing != ' ' && !allowed_attachment && !boundary_claims.contains(&(x, y)) {
            return Some(format!(
                "route cell ({x},{y}) is occupied by {:?}",
                canvas.get_meta(x, y).map(|meta| meta.owner_kind)
            ));
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
            graph.nodes.iter().any(|node| {
                x >= node.x
                    && x < node.x.saturating_add(node.width)
                    && y >= node.y
                    && y < node.bottom_y()
            }) || is_subgraph_title_cell(graph, x, y)
        })
    })
}

fn same_glyphs(left: &Canvas, right: &Canvas) -> bool {
    left.width == right.width
        && left.height == right.height
        && (0..left.height).all(|y| (0..left.width).all(|x| left.get(x, y) == right.get(x, y)))
}

fn scene_edge_indexes(scene: &HorizontalScene) -> [usize; 4] {
    [
        scene.source_internal_edge_index,
        scene.target_internal_edge_index,
        scene.start_cross_edge_index,
        scene.end_cross_edge_index,
    ]
}

fn scene_owner_id(scene: &HorizontalScene) -> String {
    format!(
        "scene:{STRATEGY}:{}->{}",
        scene.source_subgraph_id, scene.target_subgraph_id
    )
}

fn source_boundary_side(direction: Direction) -> BoundarySide {
    match direction {
        Direction::LR => BoundarySide::Right,
        Direction::RL => BoundarySide::Left,
        _ => unreachable!("horizontal scene is direction-gated"),
    }
}

fn target_boundary_side(direction: Direction) -> BoundarySide {
    match direction {
        Direction::LR => BoundarySide::Left,
        Direction::RL => BoundarySide::Right,
        _ => unreachable!("horizontal scene is direction-gated"),
    }
}

fn boundary_point(bounds: &Rectangle, side: BoundarySide, lane: usize) -> (usize, usize) {
    match side {
        BoundarySide::Left => (bounds.x, lane),
        BoundarySide::Right => (
            bounds.x.saturating_add(bounds.width.saturating_sub(1)),
            lane,
        ),
    }
}

fn valid_side_lane(bounds: &Rectangle, side: BoundarySide, lane: usize) -> bool {
    let (_, y) = boundary_point(bounds, side, lane);
    bounds.is_valid()
        && y > bounds.y
        && y < bounds.y.saturating_add(bounds.height.saturating_sub(1))
}

fn target_entry(node: &Node, direction: Direction, row: usize) -> (usize, usize) {
    match direction {
        Direction::LR => (node.x.saturating_sub(1), row),
        Direction::RL => (node.x.saturating_add(node.width), row),
        _ => unreachable!("horizontal scene is direction-gated"),
    }
}

fn boundary_side(subgraph: &Subgraph, x: usize, y: usize) -> Option<BoundarySide> {
    if !subgraph.bounds.is_valid() {
        return None;
    }
    let right = subgraph
        .bounds
        .x
        .saturating_add(subgraph.bounds.width.saturating_sub(1));
    if x == subgraph.bounds.x
        && y > subgraph.bounds.y
        && y < subgraph.bounds.y + subgraph.bounds.height.saturating_sub(1)
    {
        Some(BoundarySide::Left)
    } else if x == right
        && y > subgraph.bounds.y
        && y < subgraph.bounds.y + subgraph.bounds.height.saturating_sub(1)
    {
        Some(BoundarySide::Right)
    } else {
        None
    }
}

fn replace_side_lanes(
    slots: &mut PortalSlots,
    side: BoundarySide,
    lanes: impl IntoIterator<Item = usize>,
) {
    let target = match side {
        BoundarySide::Left => &mut slots.left,
        BoundarySide::Right => &mut slots.right,
    };
    target.clear();
    target.extend(lanes);
}

fn reject_scene(canvas: &mut Canvas, owner_id: &str, reason: &str) -> HashSet<usize> {
    canvas.record_fallback_route_rejection(owner_id, STRATEGY, reason);
    HashSet::new()
}
