//! Subgraph boundary and portal routing policy.

use crate::geom::Rect;
use crate::graph::{Direction, EdgeKind, Graph, Node};
use crate::orientation::OrientedCoords;
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::edge_policy::{td_single_incoming_route_x, title_safe_td_entry_x};
use super::super::fallback_route::{
    FallbackAxis, FallbackRoutePlan, FallbackSegment, PortalEntryDecision,
};
use super::super::portal_projection::{
    is_textual, stamp_portal_opening, subgraph_title_y, title_span, PortalAxis,
};
use super::super::semantic::CellOwnerKind;
use super::edge_primitives::{
    adjusted_edge_entry_point, draw_line_primary, draw_line_secondary, edge_exit_point,
    get_node_center, is_subgraph_title_cell,
};
use super::{
    edge_route_owner_id, set_route_char, set_route_edge_char, set_route_endpoint_char,
    style_for_edge_kind, RouteOwner, ROUTE_Z_INDEX,
};
use crate::portals::{
    bt_external_side_receiver_lane, bt_nested_boundary_lane_with_quiet_turn,
    bt_sibling_chain_target_ids, bt_single_external_entry_source_center_allowed,
    bt_target_portal_x_avoiding_single_cell_turn_with_source_center, bt_title_margin_for_edge,
    td_nested_boundary_lane, td_sibling_portal_x, td_sibling_title_gutter,
    td_single_external_entry_uses_literal_gutter_lane, td_terminal_entry_portal_lanes,
    td_terminal_entry_scene_subgraph, td_terminal_entry_target_center, title_margin_for_direction,
    title_safe_portal_x, PortalColumnPreference,
};

pub(super) fn preferred_portal_x(
    bounds: &crate::graph::Rectangle,
    title: Option<&str>,
    desired: usize,
    canvas: &Canvas,
    direction: Direction,
    avoid_title: bool,
) -> usize {
    preferred_portal_x_with_margin(
        bounds,
        title,
        desired,
        canvas,
        direction,
        avoid_title,
        title_margin_for_direction(direction),
    )
}

fn preferred_portal_x_with_margin(
    bounds: &crate::graph::Rectangle,
    title: Option<&str>,
    desired: usize,
    canvas: &Canvas,
    direction: Direction,
    avoid_title: bool,
    title_margin: usize,
) -> usize {
    let min = bounds.x.saturating_add(1);
    let max = bounds.x + bounds.width.saturating_sub(2);
    let _ = canvas;
    let protected_title_span = if avoid_title {
        title.and_then(|t| title_span(bounds, t, direction))
    } else {
        None
    };
    let mut x = title_safe_portal_x(
        bounds.x,
        bounds.width,
        avoid_title.then_some(title).flatten(),
        desired,
        direction,
        title_margin,
        PortalColumnPreference::Directional,
    );

    if direction == Direction::BT {
        if let Some((s, e)) = protected_title_span {
            let in_title_text = |pos: usize| pos >= s && pos <= e;
            if x == min {
                let candidate = min.saturating_add(1);
                if candidate <= max && !in_title_text(candidate) {
                    x = candidate;
                }
            } else if x == max {
                let candidate = max.saturating_sub(1);
                if candidate >= min && !in_title_text(candidate) {
                    x = candidate;
                }
            }
        }
    }
    x
}

/// Legacy BT portal selection retained for routes that do not need the H12
/// fallback contract. The shared policy above remains the source of truth for
/// fallback plans and new route reservations, while established non-obstacle
/// crossings keep their prior geometry.
fn legacy_preferred_portal_x(
    bounds: &crate::graph::Rectangle,
    title: Option<&str>,
    desired: usize,
    direction: Direction,
) -> usize {
    let min = bounds.x.saturating_add(1);
    let max = bounds.x + bounds.width.saturating_sub(2);
    let mut x = desired.clamp(min, max);
    let Some(title) = title else {
        return x;
    };
    let Some((start, end)) = title_span(bounds, title, direction) else {
        return x;
    };
    let protected_start = start.saturating_sub(2);
    let protected_end = end.saturating_add(2).min(max);
    if x >= protected_start && x <= protected_end {
        if direction == Direction::BT {
            let left = (protected_start > min).then(|| protected_start.saturating_sub(1));
            let right = (protected_end < max).then(|| protected_end + 1);
            x = match (left, right) {
                (Some(left), Some(right)) => {
                    let left_distance = x.abs_diff(left);
                    let right_distance = x.abs_diff(right);
                    if left_distance < right_distance {
                        left
                    } else if right_distance < left_distance {
                        right
                    } else if x <= (protected_start + protected_end) / 2 {
                        left
                    } else {
                        right
                    }
                }
                (Some(left), None) => left,
                (None, Some(right)) => right,
                (None, None) => x,
            };
        } else if protected_end < max {
            x = protected_end + 1;
        } else if protected_start > min {
            x = protected_start.saturating_sub(1);
        }
    }
    if direction == Direction::BT {
        let in_title_text = |pos: usize| pos >= start && pos <= end;
        if x == min {
            let candidate = min.saturating_add(1);
            if candidate <= max && !in_title_text(candidate) {
                x = candidate;
            }
        } else if x == max {
            let candidate = max.saturating_sub(1);
            if candidate >= min && !in_title_text(candidate) {
                x = candidate;
            }
        }
    }
    x
}

fn nearest_title_safe_x(
    bounds: &crate::graph::Rectangle,
    title: Option<&str>,
    desired: usize,
    direction: Direction,
) -> usize {
    title_safe_portal_x(
        bounds.x,
        bounds.width,
        title,
        desired,
        direction,
        title_margin_for_direction(direction),
        PortalColumnPreference::Nearest,
    )
}

/// Avoid a one-cell horizontal elbow in a nested BT corridor.  Two adjacent
/// corner glyphs are technically connected but read as a stray `++` in ASCII;
/// when the nearest title-safe column is only one cell from the current lane,
/// choose the nearest alternative with at least one shaft cell between corners.
fn bt_turn_safe_x(
    bounds: &crate::graph::Rectangle,
    title: Option<&str>,
    desired: usize,
    current_x: usize,
) -> usize {
    let initial = nearest_title_safe_x(bounds, title, desired, Direction::BT);
    if initial == current_x || initial.abs_diff(current_x) >= 2 {
        return initial;
    }

    let min_x = bounds.x.saturating_add(1);
    let max_x = bounds.x + bounds.width.saturating_sub(2);
    let mut candidates = Vec::new();
    for candidate in [
        current_x.saturating_sub(2),
        current_x.saturating_add(2),
        initial.saturating_sub(2),
        initial.saturating_add(2),
    ] {
        if candidate < min_x
            || candidate > max_x
            || candidate.abs_diff(current_x) < 2
            || nearest_title_safe_x(bounds, title, candidate, Direction::BT) != candidate
        {
            continue;
        }
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    candidates
        .into_iter()
        .min_by_key(|candidate| (candidate.abs_diff(initial), candidate.abs_diff(current_x)))
        .unwrap_or(initial)
}

fn bounds_contains_subgraph(
    outer: &crate::graph::Rectangle,
    inner: &crate::graph::Rectangle,
) -> bool {
    outer.is_valid()
        && inner.is_valid()
        && inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

fn bounds_contains_node(bounds: &crate::graph::Rectangle, node: &Node) -> bool {
    let node_right = node.x + node.width;
    let node_bottom = node.y + node.height.max(crate::style::BOX_HEIGHT);
    bounds.is_valid()
        && node.x >= bounds.x
        && node.y >= bounds.y
        && node_right <= bounds.x + bounds.width
        && node_bottom <= bounds.y + bounds.height
}

fn has_visual_container_for_nested_entry(
    graph: &Graph,
    source: &Node,
    target_sg: &crate::graph::Subgraph,
) -> bool {
    graph.subgraphs.iter().any(|candidate| {
        candidate.id != target_sg.id
            && bounds_contains_subgraph(&candidate.bounds, &target_sg.bounds)
            && (graph.is_node_in_subgraph_tree(&source.id, &candidate.id)
                || bounds_contains_node(&candidate.bounds, source))
    })
}

pub(super) fn smallest_visual_container<'a>(
    graph: &'a Graph,
    inner: &crate::graph::Subgraph,
    target: &Node,
) -> Option<&'a crate::graph::Subgraph> {
    graph
        .subgraphs
        .iter()
        .filter(|candidate| {
            candidate.id != inner.id
                && bounds_contains_subgraph(&candidate.bounds, &inner.bounds)
                && !bounds_contains_node(&candidate.bounds, target)
        })
        .min_by_key(|candidate| candidate.bounds.width * candidate.bounds.height)
}

fn td_title_safe_entry_y(subgraph: &crate::graph::Subgraph) -> usize {
    let min_inside = subgraph.bounds.y.saturating_add(1);
    let max_inside = subgraph
        .bounds
        .y
        .saturating_add(subgraph.bounds.height.saturating_sub(2));
    let desired = if subgraph.has_title() {
        subgraph.bounds.y.saturating_add(3)
    } else {
        min_inside
    };
    desired.clamp(min_inside, max_inside)
}

/// Return the direct, vertically ordered sibling pair owned by one edge.
///
/// The direct corridor planner intentionally excludes nested boundaries and
/// mixed fan-in/fan-out groups. Those route families need their own shared
/// scene contract; treating them as one direct edge would hide ownership
/// collisions instead of resolving them.
fn direct_td_sibling_pair<'a>(
    graph: &'a Graph,
    from: &Node,
    to: &Node,
) -> Option<(&'a crate::graph::Subgraph, &'a crate::graph::Subgraph)> {
    if !matches!(graph.direction, Direction::TD | Direction::TB) {
        return None;
    }

    let (exit_subgraphs, enter_subgraphs) = graph.edge_boundary_crossings(&from.id, &to.id);
    if exit_subgraphs.len() != 1 || enter_subgraphs.len() != 1 {
        return None;
    }
    let source_id = exit_subgraphs[0];
    let target_id = enter_subgraphs[0];
    if source_id == target_id {
        return None;
    }

    let source = graph.get_subgraph(source_id)?;
    let target = graph.get_subgraph(target_id)?;
    if source.parent_id.as_deref() != target.parent_id.as_deref()
        || !source.bounds.is_valid()
        || !target.bounds.is_valid()
        || source.bounds.y >= target.bounds.y
        || source.bounds.y.saturating_add(source.bounds.height) > target.bounds.y
    {
        return None;
    }

    let is_this_edge =
        |edge: &crate::graph::Edge| !edge.is_back_edge && edge.from == from.id && edge.to == to.id;
    if !graph.edges.iter().any(is_this_edge) {
        return None;
    }

    let direct_pair_count = graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge)
        .filter(|edge| {
            let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
            exits.len() == 1 && enters.len() == 1 && exits[0] == source_id && enters[0] == target_id
        })
        .count();
    (direct_pair_count == 1).then_some((source, target))
}

fn td_sibling_corridor_row(source_border_y: usize, target_border_y: usize) -> Option<usize> {
    // A two-row gap still has one valid topology-owned turn row: the route
    // turns immediately below the source border, then keeps a straight portal
    // shaft into the target border.  Reject only a one-row gap, where there
    // is no cell left to separate the turn from either boundary.
    if target_border_y.saturating_sub(source_border_y) < 3 {
        return None;
    }
    let first = source_border_y.saturating_add(1);
    let last = target_border_y.checked_sub(1)?;
    (first <= last).then_some(first + last.saturating_sub(first) / 2)
}

/// Keep the target turn below its title row and one cell above the arrow when
/// the target lane must jog back to the node centerline.
fn td_sibling_bridge_y(target: &crate::graph::Subgraph, arrow_y: usize) -> Option<usize> {
    let min_inside = target.bounds.y.saturating_add(1);
    let max_inside = target
        .bounds
        .y
        .saturating_add(target.bounds.height.saturating_sub(2));
    if max_inside < min_inside {
        return None;
    }

    let title_safe_max = if target.has_title() {
        td_title_safe_entry_y(target)
            .saturating_sub(1)
            .max(min_inside)
    } else {
        max_inside
    };
    let max_safe = max_inside.min(title_safe_max);
    // If the target has no quiet row between its title-safe band and the
    // arrow, let the final horizontal leg share the arrow row. This produces
    // an arrow-facing entry instead of a tiny title-adjacent corner pair.
    let desired = if arrow_y == max_safe.saturating_add(1) {
        arrow_y
    } else {
        arrow_y.saturating_sub(1).min(max_safe)
    };
    (desired >= min_inside).then_some(desired)
}

#[allow(clippy::too_many_arguments)]
fn route_td_sibling_corridor(
    from: &Node,
    to: &Node,
    stem_start_x: usize,
    stem_start_y: usize,
    arrow_x: usize,
    arrow_y: usize,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: &Graph,
    owner: Option<RouteOwner<'_>>,
) -> bool {
    let Some((source, target)) = direct_td_sibling_pair(graph, from, to) else {
        return false;
    };
    let source_border_y = source
        .bounds
        .y
        .saturating_add(source.bounds.height.saturating_sub(1));
    let target_border_y = target.bounds.y;
    let Some(corridor_y) = td_sibling_corridor_row(source_border_y, target_border_y) else {
        return false;
    };
    let Some(bridge_y_base) = td_sibling_bridge_y(target, arrow_y) else {
        return false;
    };

    let source_lane = stem_start_x.clamp(
        source.bounds.x.saturating_add(1),
        source
            .bounds
            .x
            .saturating_add(source.bounds.width.saturating_sub(2)),
    );
    let preferred_target_lane = if matches!(graph.direction, Direction::TD | Direction::TB) {
        // A lone direct sibling crossing has no competing lane to separate.
        // Keep its entry on the node center whenever one quiet cell remains
        // after the title text; the wider two-cell generic keep-out otherwise
        // creates a side-lane hook in tight titled envelopes. Multi-crossing
        // chains still use td_sibling_portal_x below and retain their distinct
        // title-gutter lanes.
        let center_x = target.bounds.x + target.bounds.width / 2;
        let relaxed_center = title_safe_portal_x(
            target.bounds.x,
            target.bounds.width,
            target.title.as_deref(),
            center_x,
            graph.direction,
            title_margin_for_direction(graph.direction).saturating_sub(1),
            PortalColumnPreference::Nearest,
        );
        if relaxed_center == center_x {
            center_x
        } else {
            preferred_portal_x(
                &target.bounds,
                target.title.as_deref(),
                arrow_x,
                canvas,
                graph.direction,
                true,
            )
        }
    } else {
        preferred_portal_x(
            &target.bounds,
            target.title.as_deref(),
            arrow_x,
            canvas,
            graph.direction,
            true,
        )
    };
    let target_lane = td_sibling_portal_x(graph, &from.id, &to.id, arrow_x, graph.direction)
        .unwrap_or(preferred_target_lane);
    // A jogged target lane needs one clean row above the arrow so its
    // horizontal leg cannot touch the receiving node's top corner. The
    // aligned tight case may share the arrow row because it has no jog at all.
    let bridge_y = if target_lane != arrow_x {
        bridge_y_base.min(arrow_y.saturating_sub(1))
    } else {
        bridge_y_base
    };

    let owner_id = owner
        .map(|route_owner| route_owner.id.to_owned())
        .unwrap_or_else(|| edge_route_owner_id(graph, &from.id, &to.id));
    let mut plan = FallbackRoutePlan::new(owner_id, "td-sibling-boundary-corridor");
    plan.set_source_attachment(source.id.clone(), "bottom", source_lane, source_border_y);
    plan.set_target_attachment(target.id.clone(), "top", target_lane, target_border_y);
    plan.set_arrow_attachment(arrow_x, arrow_y);
    plan.claim_boundary(
        source.id.clone(),
        "bottom",
        source_lane,
        source_border_y,
        style.edge_v,
    );
    plan.claim_boundary(
        target.id.clone(),
        "top",
        target_lane,
        target_border_y,
        style.edge_v,
    );
    plan.push_vertical(source_lane, stem_start_y, corridor_y, style.edge_v);

    if source_lane != target_lane {
        let start_corner = if target_lane > source_lane {
            style.corner_ul
        } else {
            style.corner_ur
        };
        let end_corner = if target_lane > source_lane {
            style.corner_dr
        } else {
            style.corner_dl
        };
        plan.push_corner(source_lane, corridor_y, start_corner);
        plan.push_horizontal(corridor_y, source_lane, target_lane, style.edge_h);
        plan.push_corner(target_lane, corridor_y, end_corner);
    }
    plan.push_vertical(target_lane, corridor_y, bridge_y, style.edge_v);

    if target_lane != arrow_x {
        let start_corner = if arrow_x > target_lane {
            style.corner_ul
        } else {
            style.corner_ur
        };
        let end_corner = if arrow_x > target_lane {
            style.corner_dr
        } else {
            style.corner_dl
        };
        plan.push_corner(target_lane, bridge_y, start_corner);
        plan.push_horizontal(bridge_y, target_lane, arrow_x, style.edge_h);
        if bridge_y != arrow_y {
            plan.push_corner(arrow_x, bridge_y, end_corner);
        }
    }
    if bridge_y != arrow_y {
        plan.push_vertical(arrow_x, bridge_y, arrow_y, style.edge_v);
    }

    if let Some(reason) = plan.validation_error(canvas.width, canvas.height) {
        canvas.record_fallback_route_rejection(
            plan.owner_id.clone(),
            plan.strategy.clone(),
            reason,
        );
        return false;
    }

    lower_td_fallback_plan(plan, canvas, style, owner);
    true
}

/// Select the BT interior row immediately above a title for a cross-subgraph
/// turn.
///
/// BT titles occupy the bottom interior row. A route turn immediately above
/// that row is the normal compact attachment. Specialized sibling corridors
/// may request one more quiet row through `bt_title_safe_entry_y_with_margin`;
/// keeping that choice local prevents ordinary BT diagrams from acquiring a
/// detached arrow-and-title seam.
fn bt_title_safe_entry_y(subgraph: &crate::graph::Subgraph) -> Option<usize> {
    bt_title_safe_entry_y_with_margin(subgraph, 0)
}

fn bt_title_safe_entry_y_with_margin(
    subgraph: &crate::graph::Subgraph,
    extra_quiet_rows: usize,
) -> Option<usize> {
    if !subgraph.bounds.is_valid() {
        return None;
    }

    let min_inside = subgraph.bounds.y.saturating_add(1);
    let border_y = subgraph
        .bounds
        .y
        .saturating_add(subgraph.bounds.height.saturating_sub(1));
    let max_inside = border_y.saturating_sub(1);
    if max_inside < min_inside {
        return None;
    }

    if !subgraph.has_title() {
        return Some(max_inside);
    }

    let title_y =
        crate::graph::subgraph_title_row(subgraph.bounds.y, subgraph.bounds.height, Direction::BT);
    let safe_y = title_y.checked_sub(1 + extra_quiet_rows)?;
    (safe_y >= min_inside && safe_y <= max_inside).then_some(safe_y)
}

/// Return whether a TD/TB fallback route must avoid a title cell.
///
/// The ordinary title predicate protects the complete rendered token,
/// including its wrapper spaces. A selected direct sibling lane may use one of
/// those wrapper cells as an intentional portal gutter, but only for the exact
/// target edge that owns the lane. Keeping this exception at the route owner
/// prevents unrelated diagrams from silently changing their title contracts.
fn is_td_sibling_title_cell(
    graph: &Graph,
    from: &Node,
    to: &Node,
    x: usize,
    y: usize,
    arrow_x: usize,
) -> bool {
    let ordinary_title_cell = is_subgraph_title_cell(graph, x, y);
    if !matches!(graph.direction, Direction::TD | Direction::TB) {
        return ordinary_title_cell;
    }

    let Some(target_id) = graph.get_node_subgraph(&to.id) else {
        return ordinary_title_cell;
    };
    let Some(target) = graph.get_subgraph(target_id) else {
        return ordinary_title_cell;
    };
    let (_, enter_subgraphs) = graph.edge_boundary_crossings(&from.id, &to.id);
    if enter_subgraphs.len() != 1 || enter_subgraphs[0] != target_id {
        return ordinary_title_cell;
    }
    let title_y = subgraph_title_y(&target.bounds, graph.direction);
    if y != title_y {
        return ordinary_title_cell;
    }
    let Some(title) = target.title.as_deref() else {
        return ordinary_title_cell;
    };
    let title_gutter = td_sibling_title_gutter(graph, target_id);
    let Some((token_start, token_end)) = crate::graph::subgraph_title_span_with_padding_sides(
        target.bounds.x,
        target.bounds.width,
        title,
        graph.direction,
        title_gutter.leading_extra_padding,
        title_gutter.trailing_extra_padding,
    ) else {
        return ordinary_title_cell;
    };
    if x < token_start || x > token_end {
        return ordinary_title_cell;
    }

    let selected_lane = td_sibling_portal_x(graph, &from.id, &to.id, arrow_x, graph.direction);
    !(selected_lane == Some(x) && (x == token_start || x == token_end))
}

fn lower_bt_fallback_segment(
    segment: &FallbackSegment,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: &Graph,
    owner: Option<RouteOwner<'_>>,
) {
    match segment.axis {
        super::super::fallback_route::FallbackAxis::Vertical => draw_line_primary(
            segment.from.x,
            segment.from.y,
            segment.to.x,
            segment.to.y,
            &OrientedCoords::new(Direction::BT),
            canvas,
            style,
            Some(graph),
            owner,
        ),
        super::super::fallback_route::FallbackAxis::Horizontal => {
            let (start, end) = if segment.from.x <= segment.to.x {
                (segment.from.x, segment.to.x)
            } else {
                (segment.to.x, segment.from.x)
            };
            for x in start..=end {
                if !is_subgraph_title_cell(graph, x, segment.from.y) {
                    set_route_edge_char(canvas, x, segment.from.y, style.edge_h, style, owner);
                }
            }
        }
    }
}

fn lower_td_fallback_segment(
    segment: &FallbackSegment,
    canvas: &mut Canvas,
    style: &StyleChars,
    owner: Option<RouteOwner<'_>>,
) {
    match segment.axis {
        FallbackAxis::Vertical => {
            let (start, end) = if segment.from.y <= segment.to.y {
                (segment.from.y, segment.to.y)
            } else {
                (segment.to.y, segment.from.y)
            };
            for y in start..=end {
                set_route_edge_char(canvas, segment.from.x, y, segment.glyph, style, owner);
            }
        }
        FallbackAxis::Horizontal => {
            let (start, end) = if segment.from.x <= segment.to.x {
                (segment.from.x, segment.to.x)
            } else {
                (segment.to.x, segment.from.x)
            };
            for x in start..=end {
                set_route_edge_char(canvas, x, segment.from.y, segment.glyph, style, owner);
            }
        }
    }
}

pub(super) fn lower_td_fallback_plan(
    plan: FallbackRoutePlan,
    canvas: &mut Canvas,
    style: &StyleChars,
    owner: Option<RouteOwner<'_>>,
) {
    canvas.record_fallback_route_plan(plan.clone());
    for segment in &plan.segments {
        lower_td_fallback_segment(segment, canvas, style, owner);
    }
    for corner in &plan.corners {
        set_route_char(canvas, corner.point.x, corner.point.y, corner.glyph, owner);
    }
    for claim in &plan.boundary_claims {
        set_route_char(canvas, claim.x, claim.y, claim.expected_glyph, owner);
    }
    for paint in &plan.paints {
        set_route_char(canvas, paint.point.x, paint.point.y, paint.glyph, owner);
    }
}

/// Lower the strict flat TD/TB terminal-entry scene as one route transaction.
///
/// A per-edge generic route makes the first title-safe lane look like a shared
/// rail, then lets the next edge overwrite its receiver ownership. Planning
/// the complete scene once keeps the lane/bridge map and all terminal arrows
/// in one immutable fallback trace. Unsupported near-misses return `false` so
/// the established route policy remains responsible for them.
#[allow(clippy::too_many_arguments)]
fn route_td_terminal_entry_quiet_band(
    from: &Node,
    to: &Node,
    stem_start_x: usize,
    stem_start_y: usize,
    arrow_x: usize,
    arrow_y: usize,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: &Graph,
    owner: Option<RouteOwner<'_>>,
) -> bool {
    let Some(scene) = td_terminal_entry_scene_subgraph(graph) else {
        return false;
    };
    let Some(target_subgraph_id) = graph.get_node_subgraph(&to.id) else {
        return false;
    };
    if target_subgraph_id != scene.id
        || graph.get_node_subgraph(&from.id).is_some()
        || !scene.bounds.is_valid()
        || edge_exit_point(from, graph.direction) != (stem_start_x, stem_start_y)
    {
        return false;
    }

    let current_edge_id = edge_route_owner_id(graph, &from.id, &to.id);
    if canvas.fallback_route_covers_edge(&current_edge_id) {
        return true;
    }

    let border_y = scene.bounds.y;
    let outside_y = border_y.saturating_sub(1);
    let quiet_row = border_y.saturating_add(2);
    let scene_owner_id = format!("scene:td-terminal-quiet-band:{}", scene.id);
    let mut plan = FallbackRoutePlan::new(scene_owner_id.clone(), "td-terminal-quiet-band");
    let mut covered_edge_ids = Vec::new();
    let mut first_attachment = None;
    let target_centers = scene
        .node_ids
        .iter()
        .filter_map(|node_id| Some((node_id.clone(), graph.get_node(node_id)?.center_x())))
        .collect::<std::collections::HashMap<_, _>>();
    let Some(lanes) = td_terminal_entry_portal_lanes(
        graph,
        &scene.id,
        Rect::new(
            scene.bounds.x,
            scene.bounds.y,
            scene.bounds.width,
            scene.bounds.height,
        ),
        graph.direction,
        &target_centers,
    ) else {
        return false;
    };

    for edge in graph.edges.iter().filter(|edge| !edge.is_back_edge) {
        let Some(source) = graph.get_node(&edge.from) else {
            return false;
        };
        let Some(target) = graph.get_node(&edge.to) else {
            return false;
        };
        let source_x_y = edge_exit_point(source, graph.direction);
        let (target_arrow_x, target_arrow_y) =
            adjusted_edge_entry_point(target, graph.direction, graph);
        let target_arrow_x = if edge.from == from.id && edge.to == to.id {
            arrow_x
        } else {
            target_arrow_x
        };
        let target_arrow_y = if edge.from == from.id && edge.to == to.id {
            arrow_y
        } else {
            target_arrow_y
        };
        let Some(&lane) = lanes.get(&edge.to) else {
            return false;
        };
        let bridge_y = target_arrow_y.saturating_sub(1);
        if source_x_y.1 > outside_y
            || target_arrow_y <= border_y
            || bridge_y <= quiet_row
            || bridge_y >= target_arrow_y
            || target_arrow_x >= canvas.width
            || target_arrow_y >= canvas.height
        {
            return false;
        }

        let edge_id = edge_route_owner_id(graph, &edge.from, &edge.to);
        covered_edge_ids.push(edge_id.clone());
        let going_right = lane > source_x_y.0;
        if source_x_y.1 < outside_y {
            plan.push_vertical(source_x_y.0, source_x_y.1, outside_y, style.edge_v);
        }
        if lane != source_x_y.0 {
            plan.push_corner(
                source_x_y.0,
                outside_y,
                if going_right {
                    style.corner_ul
                } else {
                    style.corner_ur
                },
            );
            plan.push_horizontal(outside_y, source_x_y.0, lane, style.edge_h);
            plan.push_corner(
                lane,
                outside_y,
                if going_right {
                    style.corner_dr
                } else {
                    style.corner_dl
                },
            );
        }
        plan.push_vertical(lane, outside_y, bridge_y, style.edge_v);
        plan.claim_boundary(scene.id.clone(), "top", lane, border_y, style.edge_v);
        if lane != target_arrow_x {
            let arrow_is_right = target_arrow_x > lane;
            plan.push_corner(
                lane,
                bridge_y,
                if arrow_is_right {
                    style.corner_ul
                } else {
                    style.corner_ur
                },
            );
            plan.push_horizontal(bridge_y, lane, target_arrow_x, style.edge_h);
            plan.push_corner(
                target_arrow_x,
                bridge_y,
                if arrow_is_right {
                    style.corner_dr
                } else {
                    style.corner_dl
                },
            );
        }
        plan.push_vertical(target_arrow_x, bridge_y, target_arrow_y, style.edge_v);
        plan.push_paint(
            target_arrow_x,
            target_arrow_y,
            OrientedCoords::new(graph.direction).arrow_end(style),
        );
        plan.set_target_entry_decision(PortalEntryDecision {
            edge_id,
            owner_id: scene_owner_id.clone(),
            target_node_id: target.id.clone(),
            boundary_id: scene.id.clone(),
            side: "top".to_owned(),
            portal_x: lane,
            portal_y: border_y,
            arrow_x: target_arrow_x,
            arrow_y: target_arrow_y,
        });
        if first_attachment.is_none() {
            first_attachment = Some((lane, target_arrow_x, target_arrow_y));
        }
    }

    if covered_edge_ids.len() < 2 || !covered_edge_ids.iter().any(|id| id == &current_edge_id) {
        return false;
    }
    plan.set_scene_coverage(covered_edge_ids);
    if let Some((lane, target_arrow_x, target_arrow_y)) = first_attachment {
        plan.set_target_attachment(scene.id.clone(), "top", lane, border_y);
        plan.set_arrow_attachment(target_arrow_x, target_arrow_y);
    }
    if let Some(reason) = plan.validation_error(canvas.width, canvas.height) {
        canvas.record_fallback_route_rejection(
            plan.owner_id.clone(),
            plan.strategy.clone(),
            reason,
        );
        return false;
    }

    lower_td_fallback_plan(plan, canvas, style, owner);
    true
}

/// Route a TD/TB edge from a nested node through every owned bottom boundary.
///
/// The generic single-subgraph route can only reserve the immediate source
/// border. For a declared nested chain, later border restoration then sees a
/// shaft without an owner and manufactures adjacent seams/junctions. Keep the
/// complete chain in one validated plan, using one title-safe lane shared by
/// all crossed bottoms.
#[allow(clippy::too_many_arguments)]
fn route_td_nested_exit_boundary_chain(
    from: &Node,
    to: &Node,
    stem_start_x: usize,
    stem_start_y: usize,
    arrow_x: usize,
    arrow_y: usize,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: &Graph,
    owner: Option<RouteOwner<'_>>,
) -> bool {
    if !matches!(graph.direction, Direction::TD | Direction::TB) {
        return false;
    }
    let (exit_subgraphs, enter_subgraphs) = graph.edge_boundary_crossings(&from.id, &to.id);
    if !enter_subgraphs.is_empty() || exit_subgraphs.len() < 2 {
        return false;
    }

    let boundaries = exit_subgraphs
        .iter()
        .filter_map(|boundary_id| graph.get_subgraph(boundary_id))
        .filter(|boundary| boundary.bounds.is_valid())
        .collect::<Vec<_>>();
    if boundaries.len() != exit_subgraphs.len() {
        return false;
    }

    let lane = td_nested_boundary_lane(graph, &exit_subgraphs, stem_start_x)
        .unwrap_or_else(|| stem_start_x.max(1));
    let first_boundary = boundaries[0];
    let first_bottom = first_boundary
        .bounds
        .y
        .saturating_add(first_boundary.bounds.height.saturating_sub(1));
    let turn_y = first_bottom.saturating_sub(1);
    if stem_start_y > turn_y || turn_y >= canvas.height {
        return false;
    }

    let owner_id = owner
        .map(|route_owner| route_owner.id.to_owned())
        .unwrap_or_else(|| edge_route_owner_id(graph, &from.id, &to.id));
    let mut plan = FallbackRoutePlan::new(owner_id, "td-exit-boundary-chain");
    plan.set_source_attachment(first_boundary.id.clone(), "bottom", lane, first_bottom);
    let last_boundary = boundaries.last().expect("validated nested boundary chain");
    let last_bottom = last_boundary
        .bounds
        .y
        .saturating_add(last_boundary.bounds.height.saturating_sub(1));
    plan.set_target_attachment(last_boundary.id.clone(), "bottom", lane, last_bottom);
    plan.set_arrow_attachment(arrow_x, arrow_y);

    let coords = OrientedCoords::new(graph.direction);
    plan.push_vertical(stem_start_x, stem_start_y, turn_y, style.edge_v);
    if lane != stem_start_x {
        let going_before = lane < stem_start_x;
        plan.push_corner(
            stem_start_x,
            turn_y,
            coords.corner_start_to_secondary(going_before, style),
        );
        plan.push_horizontal(turn_y, stem_start_x, lane, style.edge_h);
        plan.push_corner(
            lane,
            turn_y,
            coords.corner_secondary_to_end(going_before, style),
        );
    }

    let mut current_y = turn_y;
    for boundary in &boundaries {
        let bottom_y = boundary
            .bounds
            .y
            .saturating_add(boundary.bounds.height.saturating_sub(1));
        if current_y < bottom_y {
            plan.push_vertical(lane, current_y, bottom_y, style.edge_v);
        }
        plan.claim_boundary(boundary.id.clone(), "bottom", lane, bottom_y, style.edge_v);
        current_y = bottom_y.saturating_add(1);
    }

    if arrow_y < current_y || arrow_y >= canvas.height {
        return false;
    }
    if lane == arrow_x {
        plan.push_vertical(lane, current_y, arrow_y, style.edge_v);
    } else {
        // Keep one clear row between the final outer portal and the target
        // turn. The target arrow remains a vertical attachment rather than
        // becoming the endpoint of a horizontal rail.
        let bridge_y = arrow_y.saturating_sub(1).max(current_y);
        plan.push_vertical(lane, current_y, bridge_y, style.edge_v);
        let going_before = arrow_x < lane;
        plan.push_corner(
            lane,
            bridge_y,
            coords.corner_start_to_secondary(going_before, style),
        );
        plan.push_horizontal(bridge_y, lane, arrow_x, style.edge_h);
        plan.push_corner(
            arrow_x,
            bridge_y,
            coords.corner_secondary_to_end(going_before, style),
        );
        plan.push_vertical(arrow_x, bridge_y, arrow_y, style.edge_v);
    }

    if let Some(reason) = plan.validation_error(canvas.width, canvas.height) {
        canvas.record_fallback_route_rejection(
            plan.owner_id.clone(),
            plan.strategy.clone(),
            reason,
        );
        return false;
    }
    lower_td_fallback_plan(plan, canvas, style, owner);
    true
}

pub(super) fn lower_bt_fallback_plan(
    plan: FallbackRoutePlan,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: &Graph,
    owner: Option<RouteOwner<'_>>,
) -> bool {
    if let Some(reason) = plan.validation_error(canvas.width, canvas.height) {
        canvas.record_fallback_route_rejection(
            plan.owner_id.clone(),
            plan.strategy.clone(),
            reason,
        );
        return false;
    }

    // Register the intent before lowering.  The final trace is captured after
    // border/title/stabilization stages, so a later overwrite remains visible
    // as a route mismatch in the audit packet.
    canvas.record_fallback_route_plan(plan.clone());
    for segment in &plan.segments {
        lower_bt_fallback_segment(segment, canvas, style, graph, owner);
    }
    for corner in &plan.corners {
        set_route_char(canvas, corner.point.x, corner.point.y, corner.glyph, owner);
    }
    // Boundary claims are clean pierces, not intersections.  Lower them with
    // explicit ownership after the ordinary overlap pass so an existing
    // horizontal border cannot manufacture a `+` at a promised vertical hole.
    for claim in &plan.boundary_claims {
        set_route_char(canvas, claim.x, claim.y, claim.expected_glyph, owner);
    }
    // Scene reservations carry the renderer-resolved glyph at every claimed
    // cell.  Lower these exact paints last so a shared fan-in/fan-out cell is
    // owned by the scene plan rather than reconstructed independently by a
    // later heuristic.
    for paint in &plan.paints {
        set_route_char(canvas, paint.point.x, paint.point.y, paint.glyph, owner);
    }
    true
}

#[allow(dead_code, clippy::too_many_arguments)]
pub(super) fn route_cross_subgraph_td(
    from: &Node,
    to: &Node,
    stem_start_x: usize,
    stem_start_y: usize,
    arrow_x: usize,
    arrow_y: usize,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: &Graph,
    owner: Option<RouteOwner<'_>>,
) -> bool {
    let debug_timing = crate::runtime::current().diagnostics.timing;
    let from_sg = graph.get_node_subgraph(&from.id);
    let to_sg = graph.get_node_subgraph(&to.id);
    if from_sg == to_sg {
        return false;
    }

    if route_td_nested_exit_boundary_chain(
        from,
        to,
        stem_start_x,
        stem_start_y,
        arrow_x,
        arrow_y,
        canvas,
        style,
        graph,
        owner,
    ) {
        if debug_timing {
            eprintln!(
                "  cross-subgraph nested exit chain {} -> {}",
                from.id, to.id
            );
        }
        return true;
    }

    // Use the target subgraph bounds to enter just below the top border.
    let Some(sg_id) = to_sg else {
        return false;
    };
    let Some(sg) = graph.get_subgraph(sg_id) else {
        return false;
    };
    if !sg.bounds.is_valid() {
        return false;
    }

    if route_td_terminal_entry_quiet_band(
        from,
        to,
        stem_start_x,
        stem_start_y,
        arrow_x,
        arrow_y,
        canvas,
        style,
        graph,
        owner,
    ) {
        if debug_timing {
            eprintln!(
                "  cross-subgraph strict terminal quiet band {} -> {}",
                from.id, to.id
            );
        }
        return true;
    }

    // Direct stacked siblings get a paired, boundary-owned corridor. Mixed
    // fan-in/fan-out routes deliberately continue through their existing
    // orchestrators until that broader scene contract has its own plan.
    if route_td_sibling_corridor(
        from,
        to,
        stem_start_x,
        stem_start_y,
        arrow_x,
        arrow_y,
        canvas,
        style,
        graph,
        owner,
    ) {
        if debug_timing {
            eprintln!(
                "  cross-subgraph direct sibling corridor {} -> {}",
                from.id, to.id
            );
        }
        return true;
    }

    // Common case: edge enters a subgraph from above in TD/TB. Visually, we want the
    // stem to pass *under* the title (i.e., avoid drawing on the border/title row),
    // so the title stays readable and the top border remains clean.
    let entering_from_above =
        stem_start_y < sg.bounds.y && arrow_y >= sg.bounds.y.saturating_add(1);
    if entering_from_above {
        let requested_arrow_x = title_safe_td_entry_x(to, arrow_x, arrow_y, stem_start_y, graph);
        let (_, enter_subgraphs) = graph.edge_boundary_crossings(&from.id, &to.id);
        let mut current_x = stem_start_x;
        let mut current_y = stem_start_y;
        let direct_target_center = td_terminal_entry_target_center(
            graph,
            &from.id,
            &to.id,
            sg_id,
            Rect::new(sg.bounds.x, sg.bounds.y, sg.bounds.width, sg.bounds.height),
            graph.direction,
            get_node_center(to).0,
        );
        let final_entry_x = direct_target_center.unwrap_or_else(|| {
            let title_margin = if td_single_external_entry_uses_literal_gutter_lane(
                graph, &from.id, &to.id, sg_id,
            ) {
                0
            } else {
                title_margin_for_direction(graph.direction)
            };
            preferred_portal_x_with_margin(
                &sg.bounds,
                sg.title.as_deref(),
                requested_arrow_x,
                canvas,
                graph.direction,
                true,
                title_margin,
            )
        });
        let direct_sibling_entry = enter_subgraphs.len() == 1;
        let baseline_sibling_entry_x = if direct_sibling_entry {
            td_sibling_portal_x(graph, &from.id, &to.id, requested_arrow_x, graph.direction)
                .unwrap_or(final_entry_x)
        } else {
            td_sibling_portal_x(graph, &from.id, &to.id, final_entry_x, graph.direction)
                .unwrap_or(final_entry_x)
        };
        let transaction_x = td_single_incoming_route_x(from, to, requested_arrow_x, arrow_y, graph);
        let sibling_entry_x = transaction_x.unwrap_or(baseline_sibling_entry_x);
        let arrow_x = transaction_x.unwrap_or(requested_arrow_x);
        let nested_lane_desired = if enter_subgraphs.len() > 1 {
            requested_arrow_x
        } else {
            sibling_entry_x
        };
        let shared_entry_x = td_nested_boundary_lane(graph, &enter_subgraphs, nested_lane_desired)
            .unwrap_or_else(|| {
                enter_subgraphs
                    .iter()
                    .rev()
                    .filter_map(|ancestor_id| graph.get_subgraph(ancestor_id))
                    .filter(|ancestor_sg| ancestor_sg.bounds.is_valid())
                    .fold(sibling_entry_x, |entry_x, ancestor_sg| {
                        if direct_sibling_entry && ancestor_sg.id == sg.id {
                            entry_x
                        } else {
                            nearest_title_safe_x(
                                &ancestor_sg.bounds,
                                ancestor_sg.title.as_deref(),
                                entry_x,
                                graph.direction,
                            )
                        }
                    })
            });
        for ancestor_id in enter_subgraphs.iter().rev() {
            let Some(ancestor_sg) = graph.get_subgraph(ancestor_id) else {
                continue;
            };
            if !ancestor_sg.bounds.is_valid() {
                continue;
            }

            let outside_y = ancestor_sg.bounds.y.saturating_sub(1);
            if current_y <= outside_y {
                for y in current_y..=outside_y {
                    set_route_edge_char(canvas, current_x, y, style.edge_v, style, owner);
                }
            }

            let entry_x = if direct_sibling_entry && ancestor_sg.id == sg.id {
                sibling_entry_x
            } else {
                nearest_title_safe_x(
                    &ancestor_sg.bounds,
                    ancestor_sg.title.as_deref(),
                    shared_entry_x,
                    graph.direction,
                )
            };

            if entry_x != current_x && outside_y < canvas.height {
                let start_corner = if entry_x > current_x {
                    style.corner_ul
                } else {
                    style.corner_ur
                };
                set_route_edge_char(canvas, current_x, outside_y, start_corner, style, owner);

                let (hx0, hx1) = if entry_x > current_x {
                    (current_x + 1, entry_x.saturating_sub(1))
                } else {
                    (entry_x + 1, current_x.saturating_sub(1))
                };
                for x in hx0..=hx1 {
                    set_route_edge_char(canvas, x, outside_y, style.edge_h, style, owner);
                }

                let end_corner = if entry_x > current_x {
                    style.corner_dr
                } else {
                    style.corner_dl
                };
                set_route_edge_char(canvas, entry_x, outside_y, end_corner, style, owner);
            }

            current_x = entry_x;
            current_y = ancestor_sg.bounds.y.saturating_add(1).min(
                ancestor_sg
                    .bounds
                    .y
                    .saturating_add(ancestor_sg.bounds.height.saturating_sub(2)),
            );
        }

        let mut bridge_y = td_title_safe_entry_y(sg).max(current_y).min(arrow_y);
        if transaction_x.is_some() {
            bridge_y = arrow_y;
        } else if current_x != arrow_x && arrow_y > current_y {
            bridge_y = bridge_y.min(arrow_y.saturating_sub(1)).max(current_y);
        }
        if bridge_y >= current_y && current_y < canvas.height {
            for y in current_y..=bridge_y {
                set_route_edge_char(canvas, current_x, y, style.edge_v, style, owner);
            }
        }

        if current_x != arrow_x {
            let start_corner = if arrow_x > current_x {
                style.corner_ul
            } else {
                style.corner_ur
            };
            set_route_edge_char(canvas, current_x, bridge_y, start_corner, style, owner);

            let (hx0, hx1) = if arrow_x > current_x {
                (current_x.saturating_add(1), arrow_x.saturating_sub(1))
            } else {
                (arrow_x.saturating_add(1), current_x.saturating_sub(1))
            };
            for x in hx0..=hx1 {
                if is_td_sibling_title_cell(graph, from, to, x, bridge_y, arrow_x) {
                    continue;
                }
                set_route_edge_char(canvas, x, bridge_y, style.edge_h, style, owner);
            }

            let end_corner = if arrow_x > current_x {
                style.corner_dr
            } else {
                style.corner_dl
            };
            set_route_edge_char(canvas, arrow_x, bridge_y, end_corner, style, owner);
        }

        if arrow_y > bridge_y && arrow_x < canvas.width {
            for y in bridge_y.saturating_add(1)..=arrow_y {
                if is_td_sibling_title_cell(graph, from, to, arrow_x, y, arrow_x) {
                    continue;
                }
                set_route_edge_char(canvas, arrow_x, y, style.edge_v, style, owner);
            }
        }

        if debug_timing {
            eprintln!(
                "  cross-subgraph enter-under-title {} -> {} portal_x={} bridge_y={} border_y={}",
                from.id, to.id, current_x, bridge_y, sg.bounds.y
            );
        }

        return true;
    }

    let target_left_border = sg.bounds.x;
    let target_right_border = sg.bounds.x + sg.bounds.width.saturating_sub(1);
    let target_top_interior = sg.bounds.y.saturating_add(1);
    let target_bottom_interior = sg.bounds.y + sg.bounds.height.saturating_sub(2);

    let has_visual_container = has_visual_container_for_nested_entry(graph, from, sg);
    let can_side_enter = has_visual_container
        && stem_start_y >= target_top_interior
        && stem_start_y <= target_bottom_interior;
    if can_side_enter && stem_start_x < target_left_border {
        let entry_y = stem_start_y.clamp(target_top_interior, target_bottom_interior);
        set_route_edge_char(
            canvas,
            stem_start_x,
            stem_start_y,
            style.corner_ul,
            style,
            owner,
        );
        for x in (stem_start_x + 1)..target_left_border {
            set_route_edge_char(canvas, x, entry_y, style.edge_h, style, owner);
        }
        stamp_portal_opening(
            canvas,
            target_left_border,
            entry_y,
            style,
            PortalAxis::Horizontal,
            "side_entry_portal",
            ROUTE_Z_INDEX,
        );

        let turn_x = arrow_x.clamp(
            sg.bounds.x.saturating_add(1),
            sg.bounds.x + sg.bounds.width.saturating_sub(2),
        );
        let inside_start = target_left_border.saturating_add(1);
        if turn_x >= inside_start {
            let start_corner = style.corner_dr;
            set_route_edge_char(canvas, inside_start, entry_y, start_corner, style, owner);
            for x in (inside_start + 1)..turn_x {
                set_route_edge_char(canvas, x, entry_y, style.edge_h, style, owner);
            }
        }
        if turn_x != inside_start {
            set_route_edge_char(canvas, turn_x, entry_y, style.corner_dl, style, owner);
        }
        let (vy0, vy1) = if entry_y < arrow_y {
            (entry_y.saturating_add(1), arrow_y)
        } else {
            (arrow_y, entry_y.saturating_sub(1))
        };
        for y in vy0..=vy1 {
            if is_td_sibling_title_cell(graph, from, to, turn_x, y, arrow_x) {
                continue;
            }
            set_route_edge_char(canvas, turn_x, y, style.edge_v, style, owner);
        }
        if turn_x != arrow_x {
            let corner = if turn_x < arrow_x {
                style.corner_ul
            } else {
                style.corner_ur
            };
            set_route_edge_char(canvas, turn_x, arrow_y, corner, style, owner);
            let (hx0, hx1) = if turn_x < arrow_x {
                (turn_x + 1, arrow_x)
            } else {
                (arrow_x, turn_x.saturating_sub(1))
            };
            for x in hx0..=hx1 {
                set_route_edge_char(canvas, x, arrow_y, style.edge_h, style, owner);
            }
        }
        return true;
    }
    if can_side_enter && stem_start_x > target_right_border {
        let entry_y = stem_start_y.clamp(target_top_interior, target_bottom_interior);
        set_route_edge_char(
            canvas,
            stem_start_x,
            stem_start_y,
            style.corner_ur,
            style,
            owner,
        );
        for x in (target_right_border + 1)..stem_start_x {
            set_route_edge_char(canvas, x, entry_y, style.edge_h, style, owner);
        }
        stamp_portal_opening(
            canvas,
            target_right_border,
            entry_y,
            style,
            PortalAxis::Horizontal,
            "side_entry_portal",
            ROUTE_Z_INDEX,
        );

        let turn_x = arrow_x.clamp(
            sg.bounds.x.saturating_add(1),
            sg.bounds.x + sg.bounds.width.saturating_sub(2),
        );
        let inside_start = target_right_border.saturating_sub(1);
        if turn_x <= inside_start {
            let start_corner = style.corner_dl;
            set_route_edge_char(canvas, inside_start, entry_y, start_corner, style, owner);
            for x in (turn_x + 1)..inside_start {
                set_route_edge_char(canvas, x, entry_y, style.edge_h, style, owner);
            }
        }
        if turn_x != inside_start {
            set_route_edge_char(canvas, turn_x, entry_y, style.corner_dr, style, owner);
        }
        let (vy0, vy1) = if entry_y < arrow_y {
            (entry_y.saturating_add(1), arrow_y)
        } else {
            (arrow_y, entry_y.saturating_sub(1))
        };
        for y in vy0..=vy1 {
            if is_td_sibling_title_cell(graph, from, to, turn_x, y, arrow_x) {
                continue;
            }
            set_route_edge_char(canvas, turn_x, y, style.edge_v, style, owner);
        }
        if turn_x != arrow_x {
            let corner = if turn_x < arrow_x {
                style.corner_ul
            } else {
                style.corner_ur
            };
            set_route_edge_char(canvas, turn_x, arrow_y, corner, style, owner);
            let (hx0, hx1) = if turn_x < arrow_x {
                (turn_x + 1, arrow_x)
            } else {
                (arrow_x, turn_x.saturating_sub(1))
            };
            for x in hx0..=hx1 {
                set_route_edge_char(canvas, x, arrow_y, style.edge_h, style, owner);
            }
        }
        return true;
    }

    // Enter at the subgraph portal. In the generic interior-entry path we no longer
    // bias away from the title span because the route is not piercing the title row.
    let mut portal_x = preferred_portal_x(
        &sg.bounds,
        sg.title.as_deref(),
        arrow_x,
        canvas,
        graph.direction,
        false,
    );
    portal_x =
        td_sibling_portal_x(graph, &from.id, &to.id, portal_x, graph.direction).unwrap_or(portal_x);

    // Track the current drawing cursor (starts at the source exit).
    let cursor_x = stem_start_x;
    let mut cursor_y = stem_start_y;

    // Walk to just below the source subgraph border (if any) to keep turns outside.
    let mut walked_to_source_border = false;
    if let Some(src_id) = from_sg {
        if let Some(src_sg) = graph.get_subgraph(src_id) {
            let src_border_y = src_sg
                .bounds
                .y
                .saturating_add(src_sg.bounds.height.saturating_sub(1));
            let exit_y = src_border_y.min(arrow_y);
            walked_to_source_border = exit_y == src_border_y;
            for y in cursor_y..=exit_y {
                if is_td_sibling_title_cell(graph, from, to, cursor_x, y, arrow_x) {
                    continue;
                }
                set_route_edge_char(canvas, cursor_x, y, style.edge_v, style, owner);
            }
            cursor_y = exit_y;
            portal_x = preferred_portal_x(
                &sg.bounds,
                sg.title.as_deref(),
                arrow_x,
                canvas,
                graph.direction,
                true,
            );
            let (_, enter_subgraphs) = graph.edge_boundary_crossings(&from.id, &to.id);
            let sibling_desired_x = (enter_subgraphs.len() == 1).then_some(arrow_x);
            portal_x = td_sibling_portal_x(
                graph,
                &from.id,
                &to.id,
                sibling_desired_x.unwrap_or(portal_x),
                graph.direction,
            )
            .unwrap_or(portal_x);
        }
    }

    let portal_y = arrow_y
        .saturating_sub(1)
        .max(td_title_safe_entry_y(sg))
        .max(cursor_y.saturating_add(1))
        .min(arrow_y);
    if debug_timing {
        eprintln!(
            "  cross-subgraph {:?}->{:?} via portal ({}, {}) from ({}, {})",
            from.id, to.id, portal_x, portal_y, stem_start_x, stem_start_y
        );
    }

    // Turn horizontally just outside the source border if needed.
    if portal_x != cursor_x {
        let start_corner = if portal_x > cursor_x {
            style.corner_ul
        } else {
            style.corner_ur
        };
        set_route_edge_char(canvas, cursor_x, cursor_y, start_corner, style, owner);

        let (hx0, hx1) = if portal_x > cursor_x {
            (cursor_x + 1, portal_x.saturating_sub(1))
        } else {
            (portal_x + 1, cursor_x.saturating_sub(1))
        };
        for x in hx0..=hx1 {
            if is_td_sibling_title_cell(graph, from, to, x, cursor_y, arrow_x) {
                continue;
            }
            set_route_edge_char(canvas, x, cursor_y, style.edge_h, style, owner);
        }

        let end_corner = if portal_x > cursor_x {
            style.corner_dr
        } else {
            style.corner_dl
        };
        set_route_edge_char(canvas, portal_x, cursor_y, end_corner, style, owner);
    }

    if portal_y > cursor_y {
        let start_y = if portal_x == cursor_x {
            cursor_y
        } else {
            cursor_y.saturating_add(1)
        };
        for y in start_y..=portal_y {
            if is_td_sibling_title_cell(graph, from, to, portal_x, y, arrow_x) {
                continue;
            }
            set_route_edge_char(canvas, portal_x, y, style.edge_v, style, owner);
        }
    }

    // Bridge to the arrow column if needed.
    if portal_x != arrow_x {
        let corner = if portal_x < arrow_x {
            style.corner_ul
        } else {
            style.corner_ur
        };
        set_route_edge_char(canvas, portal_x, arrow_y, corner, style, owner);

        let (hx0, hx1) = if portal_x < arrow_x {
            (portal_x + 1, arrow_x)
        } else {
            (arrow_x, portal_x.saturating_sub(1))
        };
        for x in hx0..=hx1 {
            if is_td_sibling_title_cell(graph, from, to, x, arrow_y, arrow_x) {
                continue;
            }
            set_route_edge_char(canvas, x, arrow_y, style.edge_h, style, owner);
        }
    } else if arrow_y > portal_y {
        for y in (portal_y + 1)..=arrow_y {
            if is_td_sibling_title_cell(graph, from, to, portal_x, y, arrow_x) {
                continue;
            }
            set_route_edge_char(canvas, portal_x, y, style.edge_v, style, owner);
        }
    }

    // Reinstate clean verticals on pierced borders to avoid junction artifacts.
    if walked_to_source_border {
        let Some(src_sg_id) = from_sg else {
            return true;
        };
        if let Some(src_sg) = graph.get_subgraph(src_sg_id) {
            let border_y = src_sg.bounds.y + src_sg.bounds.height.saturating_sub(1);
            if portal_x < canvas.width && border_y < canvas.height {
                set_route_edge_char(canvas, cursor_x, border_y, style.edge_v, style, owner);
            }
        }
    }
    let tgt_border_y = sg.bounds.y;
    // Don't reinforce the target's top border when it contains a title: edges should
    // pass under the title row, leaving the border/text clean.
    if !sg.has_title()
        && portal_x < canvas.width
        && tgt_border_y < canvas.height
        && !is_textual(canvas.get(portal_x, tgt_border_y))
    {
        set_route_edge_char(canvas, portal_x, tgt_border_y, style.edge_v, style, owner);
    }

    true
}

fn bt_node_keepout_contains(node: &Node, x: usize, y: usize) -> bool {
    let node_right = node.x.saturating_add(node.width);
    let node_bottom = node
        .y
        .saturating_add(node.height.max(crate::style::BOX_HEIGHT));
    x >= node.x.saturating_sub(1)
        && x <= node_right
        && y >= node.y.saturating_sub(1)
        && y <= node_bottom
}

fn bt_sibling_target_lane_candidates(
    bounds: &crate::graph::Rectangle,
    title: Option<&str>,
    desired: usize,
    source_lane: usize,
    canvas: &Canvas,
    prefer_non_collinear: bool,
) -> Vec<usize> {
    // Keep one clear interior cell between a lane and either physical border.
    // The old target-only policy allowed the minimum interior column, which is
    // technically in-bounds but reads as a border hook when the lane changes.
    let min = bounds.x.saturating_add(2);
    let max = bounds.x + bounds.width.saturating_sub(3);
    if min > max {
        return Vec::new();
    }

    let preferred = preferred_portal_x(bounds, title, desired, canvas, Direction::BT, true);
    let mut candidates = (min..=max)
        .filter(|candidate| {
            nearest_title_safe_x(bounds, title, *candidate, Direction::BT) == *candidate
        })
        .collect::<Vec<_>>();
    // For parallel sibling crossings, keeping the exterior lane aligned to the
    // source stem is more readable than centering every target independently.
    // The typed plan can still choose the nearest title-safe lane when the
    // source column is outside this target container.
    if prefer_non_collinear {
        // The exact mixed scene owns an explicit target arrow column. Keep its
        // outer rail aligned with that column whenever the lane is title-safe;
        // making the rail detour toward the source center creates a needless
        // turn immediately above the target title.
        candidates.sort_by_key(|candidate| {
            (
                candidate.abs_diff(desired),
                candidate.abs_diff(preferred),
                candidate.abs_diff(source_lane),
                *candidate,
            )
        });
    } else {
        candidates.sort_by_key(|candidate| {
            (
                *candidate != source_lane,
                candidate.abs_diff(source_lane),
                candidate.abs_diff(preferred),
                *candidate,
            )
        });
    }
    candidates
}

fn bt_sibling_plan_blocker(
    plan: &FallbackRoutePlan,
    x: usize,
    y: usize,
    source_stem: (usize, usize),
    arrow: (usize, usize),
    canvas: &Canvas,
    graph: &Graph,
) -> Option<String> {
    if plan
        .boundary_claims
        .iter()
        .any(|claim| claim.x == x && claim.y == y)
    {
        return None;
    }

    let owned_by_plan = canvas
        .get_meta(x, y)
        .and_then(|meta| meta.owner_id.as_deref())
        == Some(plan.owner_id.as_str());
    if canvas.fallback_route_claims_cell(x, y) && !owned_by_plan {
        return Some(format!(
            "fallback route claim blocks planned target lane cell at ({x},{y})"
        ));
    }

    let is_source_fanout_anchor = graph.bt_sibling_target_entry_scene().is_some()
        && (x, y) == (source_stem.0, source_stem.1.saturating_sub(1));
    let is_attachment = (x, y) == source_stem || (x, y) == arrow || is_source_fanout_anchor;
    if !is_attachment {
        if let Some(node) = graph
            .nodes
            .iter()
            .find(|node| bt_node_keepout_contains(node, x, y))
        {
            return Some(format!(
                "node keepout {} blocks planned cell at ({x},{y})",
                node.id
            ));
        }

        if let Some(subgraph) = graph.subgraphs.iter().find(|subgraph| {
            let Some(title) = subgraph.title.as_deref() else {
                return false;
            };
            let Some((start, end)) = title_span(&subgraph.bounds, title, Direction::BT) else {
                return false;
            };
            subgraph_title_y(&subgraph.bounds, Direction::BT) == y && x >= start && x <= end
        }) {
            return Some(format!(
                "subgraph title {} blocks planned cell at ({x},{y})",
                subgraph.id
            ));
        }
    }

    let is_shared_source_attachment =
        graph.bt_sibling_target_entry_scene().is_some() && (x, y) == source_stem;
    if canvas.get(x, y) != ' ' && !owned_by_plan && !is_shared_source_attachment {
        let owner = canvas
            .get_meta(x, y)
            .and_then(|meta| meta.owner_id.as_deref())
            .unwrap_or("unknown");
        return Some(format!(
            "canvas owner {owner} blocks planned cell at ({x},{y})"
        ));
    }
    None
}

fn choose_bt_sibling_corridor_row(
    source_top_y: usize,
    target_bottom_y: usize,
    source_x: usize,
    target_x: usize,
    canvas: &Canvas,
    graph: &Graph,
) -> Option<usize> {
    let first = target_bottom_y.checked_add(2)?;
    let last = source_top_y.checked_sub(2)?;
    if first > last {
        return None;
    }

    let midpoint = first + (last - first) / 2;
    let mut candidates = (first..=last).collect::<Vec<_>>();
    candidates.sort_by_key(|row| (row.abs_diff(midpoint), *row));

    let left = source_x.min(target_x);
    let right = source_x.max(target_x);
    candidates.into_iter().find(|row| {
        let node_keepout = graph
            .nodes
            .iter()
            .any(|node| (left..=right).any(|x| bt_node_keepout_contains(node, x, *row)));
        !node_keepout && (left..=right).all(|x| canvas.get(x, *row) == ' ')
    })
}

/// Detect the bounded layout reservation that moved a topology-connected
/// external node to a side of this sibling corridor. That specific geometry
/// can afford a slightly longer target turn, which avoids rendering adjacent
/// corner glyphs while leaving legacy sibling layouts untouched.
fn bt_external_side_reservation_present(
    graph: &Graph,
    source_sg: &crate::graph::Subgraph,
    target_sg: &crate::graph::Subgraph,
) -> bool {
    let corridor_left = source_sg.bounds.x.min(target_sg.bounds.x);
    let corridor_right = source_sg
        .bounds
        .x
        .saturating_add(source_sg.bounds.width)
        .max(target_sg.bounds.x.saturating_add(target_sg.bounds.width));
    let corridor_top = target_sg.bounds.y.saturating_add(target_sg.bounds.height);
    let corridor_bottom = source_sg.bounds.y;

    graph.nodes.iter().any(|node| {
        if graph.get_node_subgraph(&node.id).is_some()
            || node.y >= corridor_bottom
            || node.bottom_y() <= corridor_top
        {
            return false;
        }
        let touches_sibling_tree =
            graph
                .edges
                .iter()
                .filter(|edge| !edge.is_back_edge)
                .any(|edge| {
                    if edge.from == node.id {
                        graph.is_node_in_subgraph_tree(&edge.to, &source_sg.id)
                            || graph.is_node_in_subgraph_tree(&edge.to, &target_sg.id)
                    } else if edge.to == node.id {
                        graph.is_node_in_subgraph_tree(&edge.from, &source_sg.id)
                            || graph.is_node_in_subgraph_tree(&edge.from, &target_sg.id)
                    } else {
                        false
                    }
                });
        if !touches_sibling_tree {
            return false;
        }

        node.x >= corridor_right.saturating_add(2)
            || node.x.saturating_add(node.width).saturating_add(2) <= corridor_left
    })
}

/// The layout stage may stage a target receiver on the exact arrow lane for a
/// bounded external-side BT scene. When that structural contract is true, a
/// target-side detour is actively harmful: it leaves the receiver aligned to
/// the arrow but routes through an unrelated title-safe lane first.
fn bt_external_side_receiver_lane_is_staged(
    graph: &Graph,
    source_sg: &crate::graph::Subgraph,
    target_sg: &crate::graph::Subgraph,
    arrow_x: usize,
) -> bool {
    let Some(scene) = graph.bt_external_side_receiver_scene() else {
        return false;
    };
    if scene.source_subgraph_id != source_sg.id || scene.target_subgraph_id != target_sg.id {
        return false;
    }

    let Some(receiver) = target_sg
        .node_ids
        .iter()
        .filter_map(|node_id| graph.get_node(node_id))
        .find(|node| node.center_x() == arrow_x)
    else {
        return false;
    };
    receiver.center_x()
        == bt_external_side_receiver_lane(
            target_sg.bounds.x,
            target_sg.bounds.width,
            target_sg.title.as_deref(),
            arrow_x,
        )
}

/// Use the stricter fallback contract only when an external obstacle actually
/// pressures the sibling corridor (or has just been moved out of it). Ordinary
/// sibling crossings retain their established route geometry and snapshots.
fn bt_sibling_route_requires_fallback(
    graph: &Graph,
    source_sg: &crate::graph::Subgraph,
    target_sg: &crate::graph::Subgraph,
) -> bool {
    if bt_external_side_reservation_present(graph, source_sg, target_sg) {
        return true;
    }

    let corridor_left = source_sg.bounds.x.min(target_sg.bounds.x);
    let corridor_right = source_sg
        .bounds
        .x
        .saturating_add(source_sg.bounds.width)
        .max(target_sg.bounds.x.saturating_add(target_sg.bounds.width));
    let corridor_top = target_sg.bounds.y.saturating_add(target_sg.bounds.height);
    let corridor_bottom = source_sg.bounds.y;
    if corridor_bottom <= corridor_top {
        return false;
    }

    graph.nodes.iter().any(|node| {
        if graph.get_node_subgraph(&node.id).is_some() {
            return false;
        }
        let node_left = node.x.saturating_sub(1);
        let node_right = node.x.saturating_add(node.width).saturating_add(1);
        let node_top = node.y.saturating_sub(1);
        let node_bottom = node
            .y
            .saturating_add(node.height.max(crate::style::BOX_HEIGHT))
            .saturating_add(1);
        node_left < corridor_right
            && corridor_left < node_right
            && node_top < corridor_bottom
            && corridor_top < node_bottom
    })
}

fn bt_sibling_parallel_edge_count(
    graph: &Graph,
    source_sg: &crate::graph::Subgraph,
    target_sg: &crate::graph::Subgraph,
) -> usize {
    graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge)
        .filter(|edge| {
            let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
            exits.len() == 1
                && enters.len() == 1
                && exits[0] == source_sg.id
                && enters[0] == target_sg.id
        })
        .count()
}

struct BtSiblingRoutePlanContext<'a> {
    source_sg: &'a crate::graph::Subgraph,
    target_sg: &'a crate::graph::Subgraph,
    stem_start: (usize, usize),
    arrow: (usize, usize),
    canvas: &'a Canvas,
    style: &'a StyleChars,
    graph: &'a Graph,
    owner_id: String,
}

fn build_bt_sibling_route_plan(
    context: BtSiblingRoutePlanContext<'_>,
) -> Result<FallbackRoutePlan, String> {
    let BtSiblingRoutePlanContext {
        source_sg,
        target_sg,
        stem_start: (stem_start_x, stem_start_y),
        arrow: (arrow_x, arrow_y),
        canvas,
        style,
        graph,
        owner_id,
    } = context;
    let source_top_y = source_sg.bounds.y;
    let target_bottom_y = target_sg
        .bounds
        .y
        .saturating_add(target_sg.bounds.height.saturating_sub(1));
    if source_top_y <= target_bottom_y {
        return Err("source and target BT boundaries are not vertically ordered".to_owned());
    }

    let source_lane = stem_start_x.clamp(
        source_sg.bounds.x.saturating_add(1),
        source_sg.bounds.x + source_sg.bounds.width.saturating_sub(2),
    );
    let inside_y = bt_title_safe_entry_y_with_margin(target_sg, 1)
        .ok_or_else(|| "target has no BT title-safe row".to_owned())?;
    if arrow_y >= inside_y {
        return Err("target arrow is not above the BT title-safe attachment row".to_owned());
    }

    let mut candidates = bt_sibling_target_lane_candidates(
        &target_sg.bounds,
        target_sg.title.as_deref(),
        arrow_x,
        source_lane,
        canvas,
        graph.bt_sibling_target_entry_scene().is_some(),
    );
    if candidates.is_empty() {
        return Err("target has no title-safe interior lane candidates".to_owned());
    }
    if bt_external_side_reservation_present(graph, source_sg, target_sg) {
        let preferred_reserved_lane = bt_external_side_receiver_lane(
            target_sg.bounds.x,
            target_sg.bounds.width,
            target_sg.title.as_deref(),
            arrow_x,
        );
        if bt_external_side_receiver_lane_is_staged(graph, source_sg, target_sg, arrow_x) {
            candidates.sort_by_key(|candidate| {
                (
                    candidate.abs_diff(preferred_reserved_lane),
                    candidate.abs_diff(arrow_x),
                    *candidate,
                )
            });
        } else {
            candidates.sort_by_key(|candidate| {
                (
                    candidate.abs_diff(arrow_x) < 2,
                    candidate.abs_diff(preferred_reserved_lane),
                    candidate.abs_diff(arrow_x),
                    *candidate,
                )
            });
        }
    }

    let mut rejections = Vec::new();
    for target_lane in candidates {
        let Some(corridor_y) = choose_bt_sibling_corridor_row(
            source_top_y,
            target_bottom_y,
            source_lane,
            target_lane,
            canvas,
            graph,
        ) else {
            rejections.push(format!("x={target_lane}: no safe corridor row"));
            continue;
        };

        let mut plan = FallbackRoutePlan::new(owner_id.clone(), "bt-sibling-boundary-corridor");
        plan.set_source_attachment(source_sg.id.clone(), "top", source_lane, source_top_y);
        plan.set_target_attachment(target_sg.id.clone(), "bottom", target_lane, target_bottom_y);
        plan.set_arrow_attachment(arrow_x, arrow_y);
        plan.claim_boundary(
            source_sg.id.clone(),
            "top",
            source_lane,
            source_top_y,
            style.edge_v,
        );
        plan.claim_boundary(
            target_sg.id.clone(),
            "bottom",
            target_lane,
            target_bottom_y,
            style.edge_v,
        );
        plan.push_vertical(source_lane, stem_start_y, corridor_y, style.edge_v);

        if source_lane != target_lane {
            let start_corner = if target_lane > source_lane {
                style.corner_dl
            } else {
                style.corner_dr
            };
            let end_corner = if target_lane > source_lane {
                style.corner_ur
            } else {
                style.corner_ul
            };
            plan.push_corner(source_lane, corridor_y, start_corner);
            plan.push_horizontal(corridor_y, source_lane, target_lane, style.edge_h);
            plan.push_corner(target_lane, corridor_y, end_corner);
        }
        plan.push_vertical(target_lane, corridor_y, inside_y, style.edge_v);

        if target_lane != arrow_x {
            let start_corner = if arrow_x > target_lane {
                style.corner_dl
            } else {
                style.corner_dr
            };
            let end_corner = if arrow_x > target_lane {
                style.corner_ur
            } else {
                style.corner_ul
            };
            plan.push_corner(target_lane, inside_y, start_corner);
            plan.push_horizontal(inside_y, target_lane, arrow_x, style.edge_h);
            plan.push_corner(arrow_x, inside_y, end_corner);
            plan.push_vertical(arrow_x, inside_y.saturating_sub(1), arrow_y, style.edge_v);
        } else {
            plan.push_vertical(target_lane, inside_y, arrow_y, style.edge_v);
        }

        if let Some(reason) = plan.validation_error(canvas.width, canvas.height) {
            rejections.push(format!("x={target_lane}: {reason}"));
            continue;
        }
        if let Some(reason) = plan.planned_cells().into_iter().find_map(|(x, y)| {
            bt_sibling_plan_blocker(
                &plan,
                x,
                y,
                (stem_start_x, stem_start_y),
                (arrow_x, arrow_y),
                canvas,
                graph,
            )
        }) {
            rejections.push(format!("x={target_lane}: {reason}"));
            continue;
        }

        return Ok(plan);
    }

    Err(format!(
        "no safe BT sibling target lane; candidates rejected: {}",
        rejections.join("; ")
    ))
}

#[allow(clippy::too_many_arguments)]
fn route_simple_bt_cross_subgraph_legacy(
    from: &Node,
    stem_start_x: usize,
    stem_start_y: usize,
    arrow_x: usize,
    arrow_y: usize,
    tgt_sg: &crate::graph::Subgraph,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: &Graph,
    owner: Option<RouteOwner<'_>>,
) -> BtRouteOutcome {
    let coords = OrientedCoords::new(Direction::BT);
    if !tgt_sg.bounds.is_valid() {
        return BtRouteOutcome::NotApplicable;
    }

    let tgt_border_y = tgt_sg.bounds.y + tgt_sg.bounds.height.saturating_sub(1);
    let entry_x = legacy_preferred_portal_x(
        &tgt_sg.bounds,
        tgt_sg.title.as_deref(),
        arrow_x,
        Direction::BT,
    );
    let outside_y = tgt_border_y.saturating_add(1);
    let inside_y = tgt_border_y.saturating_sub(1);

    draw_line_primary(
        stem_start_x,
        stem_start_y,
        stem_start_x,
        outside_y,
        &coords,
        canvas,
        style,
        Some(graph),
        owner,
    );

    if entry_x != stem_start_x && outside_y < canvas.height {
        let start_corner = if entry_x > stem_start_x {
            style.corner_dl
        } else {
            style.corner_dr
        };
        set_route_edge_char(canvas, stem_start_x, outside_y, start_corner, style, owner);

        let (hx0, hx1) = if entry_x > stem_start_x {
            (stem_start_x + 1, entry_x.saturating_sub(1))
        } else {
            (entry_x + 1, stem_start_x.saturating_sub(1))
        };
        for x in hx0..=hx1 {
            set_route_edge_char(canvas, x, outside_y, style.edge_h, style, owner);
        }

        let end_corner = if entry_x > stem_start_x {
            style.corner_ur
        } else {
            style.corner_ul
        };
        set_route_edge_char(canvas, entry_x, outside_y, end_corner, style, owner);
    }

    if tgt_border_y < canvas.height {
        set_route_edge_char(canvas, entry_x, tgt_border_y, style.edge_v, style, owner);
    }

    if entry_x != arrow_x && inside_y < canvas.height {
        let start_corner = if arrow_x > entry_x {
            style.corner_dl
        } else {
            style.corner_dr
        };
        set_route_edge_char(canvas, entry_x, inside_y, start_corner, style, owner);

        let (hx0, hx1) = if arrow_x > entry_x {
            (entry_x + 1, arrow_x.saturating_sub(1))
        } else {
            (arrow_x + 1, entry_x.saturating_sub(1))
        };
        for x in hx0..=hx1 {
            set_route_edge_char(canvas, x, inside_y, style.edge_h, style, owner);
        }

        let end_corner = if arrow_x > entry_x {
            style.corner_ur
        } else {
            style.corner_ul
        };
        set_route_edge_char(canvas, arrow_x, inside_y, end_corner, style, owner);

        if arrow_y < inside_y {
            draw_line_primary(
                arrow_x,
                inside_y.saturating_sub(1),
                arrow_x,
                arrow_y,
                &coords,
                canvas,
                style,
                Some(graph),
                owner,
            );
        }
    } else {
        draw_line_primary(
            entry_x,
            inside_y,
            entry_x,
            arrow_y,
            &coords,
            canvas,
            style,
            Some(graph),
            owner,
        );
    }

    let _ = from;
    BtRouteOutcome::Handled
}

/// Route an external BT source through every nested bottom boundary before
/// attaching to the target node. The generic BT entry path only understands
/// one target boundary and otherwise leaves the source-box seam to restoration.
#[allow(clippy::too_many_arguments)]
fn route_bt_nested_entry_boundary_chain(
    from: &Node,
    to: &Node,
    stem_start_x: usize,
    stem_start_y: usize,
    arrow_x: usize,
    arrow_y: usize,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: &Graph,
    owner: Option<RouteOwner<'_>>,
) -> bool {
    if graph.direction != Direction::BT {
        return false;
    }
    let (exit_subgraphs, enter_subgraphs) = graph.edge_boundary_crossings(&from.id, &to.id);
    if !exit_subgraphs.is_empty() || enter_subgraphs.len() < 2 {
        return false;
    }

    let boundaries = enter_subgraphs
        .iter()
        .rev()
        .filter_map(|boundary_id| graph.get_subgraph(boundary_id))
        .filter(|boundary| boundary.bounds.is_valid())
        .collect::<Vec<_>>();
    if boundaries.len() != enter_subgraphs.len() {
        return false;
    }

    let lane = bt_nested_boundary_lane_with_quiet_turn(
        graph,
        &enter_subgraphs,
        arrow_x,
        stem_start_x,
        arrow_x,
        None,
    )
    .unwrap_or_else(|| arrow_x.max(1));
    let outer = boundaries[0];
    let outer_bottom = outer
        .bounds
        .y
        .saturating_add(outer.bounds.height.saturating_sub(1));
    if stem_start_y < outer_bottom || arrow_y >= outer_bottom {
        return false;
    }

    let owner_id = owner
        .map(|route_owner| route_owner.id.to_owned())
        .unwrap_or_else(|| edge_route_owner_id(graph, &from.id, &to.id));
    let mut plan = FallbackRoutePlan::new(owner_id, "bt-enter-boundary-chain");
    plan.set_source_attachment(outer.id.clone(), "bottom", lane, outer_bottom);
    let inner = boundaries.last().expect("validated nested boundary chain");
    let inner_bottom = inner
        .bounds
        .y
        .saturating_add(inner.bounds.height.saturating_sub(1));
    plan.set_target_attachment(inner.id.clone(), "bottom", lane, inner_bottom);
    plan.set_arrow_attachment(arrow_x, arrow_y);

    let coords = OrientedCoords::new(Direction::BT);
    let source_turn_y = outer_bottom.saturating_add(1);
    if stem_start_y < source_turn_y || source_turn_y >= canvas.height {
        return false;
    }
    plan.push_vertical(stem_start_x, stem_start_y, source_turn_y, style.edge_v);
    if lane != stem_start_x {
        let going_before = lane < stem_start_x;
        plan.push_corner(
            stem_start_x,
            source_turn_y,
            coords.corner_start_to_secondary(going_before, style),
        );
        plan.push_horizontal(source_turn_y, stem_start_x, lane, style.edge_h);
        plan.push_corner(
            lane,
            source_turn_y,
            coords.corner_secondary_to_end(going_before, style),
        );
    }

    let mut current_y = source_turn_y;
    for boundary in &boundaries {
        let bottom_y = boundary
            .bounds
            .y
            .saturating_add(boundary.bounds.height.saturating_sub(1));
        if current_y > bottom_y {
            plan.push_vertical(lane, current_y, bottom_y, style.edge_v);
        }
        plan.claim_boundary(boundary.id.clone(), "bottom", lane, bottom_y, style.edge_v);
        current_y = bottom_y.saturating_sub(1);
    }

    if arrow_y > current_y || arrow_y >= canvas.height {
        return false;
    }
    if lane == arrow_x {
        plan.push_vertical(lane, current_y, arrow_y, style.edge_v);
    } else {
        // Leave the arrow as a vertical attachment: turn one row below it,
        // then climb into the BT target instead of ending a horizontal rail
        // on an upward arrowhead.
        let bridge_y = arrow_y.saturating_add(1).min(current_y);
        plan.push_vertical(lane, current_y, bridge_y, style.edge_v);
        let going_before = arrow_x < lane;
        plan.push_corner(
            lane,
            bridge_y,
            coords.corner_start_to_secondary(going_before, style),
        );
        plan.push_horizontal(bridge_y, lane, arrow_x, style.edge_h);
        plan.push_corner(
            arrow_x,
            bridge_y,
            coords.corner_secondary_to_end(going_before, style),
        );
        plan.push_vertical(arrow_x, bridge_y, arrow_y, style.edge_v);
    }

    if let Some(reason) = plan.validation_error(canvas.width, canvas.height) {
        canvas.record_fallback_route_rejection(
            plan.owner_id.clone(),
            plan.strategy.clone(),
            reason,
        );
        return false;
    }
    lower_bt_fallback_plan(plan, canvas, style, graph, owner)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BtRouteOutcome {
    Handled,
    Rejected,
    NotApplicable,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn route_cross_subgraph_bt(
    from: &Node,
    to: &Node,
    stem_start_x: usize,
    stem_start_y: usize,
    arrow_x: usize,
    arrow_y: usize,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: &Graph,
    owner: Option<RouteOwner<'_>>,
) -> BtRouteOutcome {
    let coords = OrientedCoords::new(Direction::BT);
    let from_sg = graph.get_node_subgraph(&from.id);
    let to_sg = graph.get_node_subgraph(&to.id);
    if from_sg == to_sg {
        return BtRouteOutcome::NotApplicable;
    }

    if let Some(tgt_id) = to_sg {
        let Some(tgt_sg) = graph.get_subgraph(tgt_id) else {
            return BtRouteOutcome::NotApplicable;
        };
        if tgt_sg.bounds.is_valid() {
            let tgt_border_y = tgt_sg.bounds.y + tgt_sg.bounds.height.saturating_sub(1);
            let entering_from_below = stem_start_y > tgt_border_y && arrow_y < tgt_border_y;
            if entering_from_below {
                let (exit_subgraphs, enter_subgraphs) =
                    graph.edge_boundary_crossings(&from.id, &to.id);
                if route_bt_nested_entry_boundary_chain(
                    from,
                    to,
                    stem_start_x,
                    stem_start_y,
                    arrow_x,
                    arrow_y,
                    canvas,
                    style,
                    graph,
                    owner,
                ) {
                    if crate::runtime::current().diagnostics.timing {
                        eprintln!(
                            "  cross-subgraph nested entry chain {} -> {}",
                            from.id, to.id
                        );
                    }
                    return BtRouteOutcome::Handled;
                }
                if exit_subgraphs.len() == 1 && enter_subgraphs.len() == 1 {
                    let Some(source_id) = exit_subgraphs.first() else {
                        return BtRouteOutcome::NotApplicable;
                    };
                    let Some(source_sg) = graph.get_subgraph(source_id) else {
                        canvas.record_fallback_route_rejection(
                            owner
                                .map(|route_owner| route_owner.id.to_owned())
                                .unwrap_or_else(|| format!("fallback:{}->{}", from.id, to.id)),
                            "bt-sibling-boundary-corridor",
                            "source boundary is missing",
                        );
                        return BtRouteOutcome::Rejected;
                    };
                    let sibling_parallel_edges =
                        bt_sibling_parallel_edge_count(graph, source_sg, tgt_sg);
                    if sibling_parallel_edges == 2
                        || bt_sibling_route_requires_fallback(graph, source_sg, tgt_sg)
                    {
                        let owner_id = owner
                            .map(|route_owner| route_owner.id.to_owned())
                            .unwrap_or_else(|| format!("fallback:{}->{}", from.id, to.id));
                        match build_bt_sibling_route_plan(BtSiblingRoutePlanContext {
                            source_sg,
                            target_sg: tgt_sg,
                            stem_start: (stem_start_x, stem_start_y),
                            arrow: (arrow_x, arrow_y),
                            canvas,
                            style,
                            graph,
                            owner_id,
                        }) {
                            Ok(plan) => {
                                if lower_bt_fallback_plan(plan, canvas, style, graph, owner) {
                                    return BtRouteOutcome::Handled;
                                }
                                if sibling_parallel_edges == 2
                                    && !bt_sibling_route_requires_fallback(graph, source_sg, tgt_sg)
                                {
                                    return route_simple_bt_cross_subgraph_legacy(
                                        from,
                                        stem_start_x,
                                        stem_start_y,
                                        arrow_x,
                                        arrow_y,
                                        tgt_sg,
                                        canvas,
                                        style,
                                        graph,
                                        owner,
                                    );
                                }
                                return BtRouteOutcome::Rejected;
                            }
                            Err(reason) => {
                                canvas.record_fallback_route_rejection(
                                    owner
                                        .map(|route_owner| route_owner.id.to_owned())
                                        .unwrap_or_else(|| {
                                            format!("fallback:{}->{}", from.id, to.id)
                                        }),
                                    "bt-sibling-boundary-corridor",
                                    reason,
                                );
                                return BtRouteOutcome::Rejected;
                            }
                        }
                    }
                }
                if enter_subgraphs.len() <= 1 {
                    let outside_y = tgt_border_y.saturating_add(1);
                    let Some(base_inside_y) = bt_title_safe_entry_y(tgt_sg) else {
                        return BtRouteOutcome::NotApplicable;
                    };
                    let bounds: std::collections::HashMap<String, crate::geom::Rect> = graph
                        .subgraphs
                        .iter()
                        .map(|subgraph| {
                            (
                                subgraph.id.clone(),
                                crate::geom::Rect::new(
                                    subgraph.bounds.x,
                                    subgraph.bounds.y,
                                    subgraph.bounds.width,
                                    subgraph.bounds.height,
                                ),
                            )
                        })
                        .collect();
                    let chain_targets = bt_sibling_chain_target_ids(graph, &bounds);
                    let scene_literal_entry = graph
                        .bt_sibling_target_entry_scene()
                        .is_some_and(|scene| scene.target_subgraph_id == tgt_id);
                    let title_margin = if scene_literal_entry {
                        0
                    } else {
                        bt_title_margin_for_edge(graph, &from.id, &to.id, tgt_id)
                    };
                    let entry_x = preferred_portal_x_with_margin(
                        &tgt_sg.bounds,
                        tgt_sg.title.as_deref(),
                        arrow_x,
                        canvas,
                        Direction::BT,
                        true,
                        title_margin,
                    );
                    let allow_source_center = bt_single_external_entry_source_center_allowed(
                        graph, &from.id, &to.id, tgt_id,
                    );
                    let entry_x = bt_target_portal_x_avoiding_single_cell_turn_with_source_center(
                        tgt_sg.bounds.x,
                        tgt_sg.bounds.width,
                        tgt_sg.title.as_deref(),
                        entry_x,
                        stem_start_x,
                        from_sg
                            .is_none()
                            .then(|| (from.x, from.x + from.width.saturating_sub(1))),
                        arrow_x,
                        title_margin,
                        allow_source_center,
                    );
                    let inside_y = if chain_targets
                        .as_ref()
                        .is_some_and(|targets| targets.contains(tgt_id))
                    {
                        let Some(reserved_inside_y) = base_inside_y.checked_sub(1) else {
                            return BtRouteOutcome::Rejected;
                        };
                        if arrow_y >= reserved_inside_y {
                            canvas.record_fallback_route_rejection(
                                owner
                                    .map(|route_owner| route_owner.id.to_owned())
                                    .unwrap_or_else(|| format!("fallback:{}->{}", from.id, to.id)),
                                "bt-sibling-chain-scene-clearance",
                                "reserved target row collides with target arrow",
                            );
                            return BtRouteOutcome::Rejected;
                        }
                        reserved_inside_y
                    } else {
                        base_inside_y
                    };

                    draw_line_primary(
                        stem_start_x,
                        stem_start_y,
                        stem_start_x,
                        outside_y,
                        &coords,
                        canvas,
                        style,
                        Some(graph),
                        owner,
                    );

                    if entry_x != stem_start_x && outside_y < canvas.height {
                        let start_corner = if entry_x > stem_start_x {
                            style.corner_dl
                        } else {
                            style.corner_dr
                        };
                        set_route_edge_char(
                            canvas,
                            stem_start_x,
                            outside_y,
                            start_corner,
                            style,
                            owner,
                        );

                        let (hx0, hx1) = if entry_x > stem_start_x {
                            (stem_start_x + 1, entry_x.saturating_sub(1))
                        } else {
                            (entry_x + 1, stem_start_x.saturating_sub(1))
                        };
                        for x in hx0..=hx1 {
                            set_route_edge_char(canvas, x, outside_y, style.edge_h, style, owner);
                        }

                        let end_corner = if entry_x > stem_start_x {
                            style.corner_ur
                        } else {
                            style.corner_ul
                        };
                        set_route_edge_char(canvas, entry_x, outside_y, end_corner, style, owner);
                    }

                    if tgt_border_y < canvas.height {
                        set_route_edge_char(
                            canvas,
                            entry_x,
                            tgt_border_y,
                            style.edge_v,
                            style,
                            owner,
                        );
                    }

                    // Keep the physical BT portal continuous through the
                    // bottom title row. The turn stays one row above the
                    // title, while this clean vertical pierce makes the
                    // boundary ownership legible without writing over title
                    // text (the selected column is title-safe).
                    if tgt_sg.has_title() && tgt_border_y > 0 {
                        set_route_edge_char(
                            canvas,
                            entry_x,
                            tgt_border_y.saturating_sub(1),
                            style.edge_v,
                            style,
                            owner,
                        );
                    }

                    // A titled BT target has one protected title row between the
                    // physical border and the turn row. Keep the title-safe portal
                    // column continuous through that row before turning toward the
                    // target arrow.
                    if inside_y < tgt_border_y.saturating_sub(1) {
                        draw_line_primary(
                            entry_x,
                            tgt_border_y.saturating_sub(1),
                            entry_x,
                            inside_y,
                            &coords,
                            canvas,
                            style,
                            Some(graph),
                            owner,
                        );
                    }

                    if entry_x != arrow_x && inside_y < canvas.height {
                        let start_corner = if arrow_x > entry_x {
                            style.corner_dl
                        } else {
                            style.corner_dr
                        };
                        set_route_edge_char(canvas, entry_x, inside_y, start_corner, style, owner);

                        let (hx0, hx1) = if arrow_x > entry_x {
                            (entry_x + 1, arrow_x.saturating_sub(1))
                        } else {
                            (arrow_x + 1, entry_x.saturating_sub(1))
                        };
                        for x in hx0..=hx1 {
                            set_route_edge_char(canvas, x, inside_y, style.edge_h, style, owner);
                        }

                        let end_corner = if arrow_x > entry_x {
                            style.corner_ur
                        } else {
                            style.corner_ul
                        };
                        set_route_edge_char(canvas, arrow_x, inside_y, end_corner, style, owner);

                        if arrow_y < inside_y {
                            draw_line_primary(
                                arrow_x,
                                inside_y.saturating_sub(1),
                                arrow_x,
                                arrow_y,
                                &coords,
                                canvas,
                                style,
                                Some(graph),
                                owner,
                            );
                        }
                    } else {
                        draw_line_primary(
                            entry_x,
                            inside_y,
                            entry_x,
                            arrow_y,
                            &coords,
                            canvas,
                            style,
                            Some(graph),
                            owner,
                        );
                    }
                } else {
                    // Plan the complete outer-to-inner path before touching the
                    // canvas.  The shared desired column is folded through every
                    // title-safe boundary, then each boundary records its actual
                    // portal column.  This prevents a later border restore from
                    // turning an omitted physical crossing into a source-tail seam.
                    let final_entry_x = preferred_portal_x(
                        &tgt_sg.bounds,
                        tgt_sg.title.as_deref(),
                        arrow_x,
                        canvas,
                        Direction::BT,
                        true,
                    );
                    let shared_entry_x = enter_subgraphs
                        .iter()
                        .rev()
                        .filter_map(|ancestor_id| graph.get_subgraph(ancestor_id))
                        .filter(|ancestor_sg| ancestor_sg.bounds.is_valid())
                        .fold(final_entry_x, |entry_x, ancestor_sg| {
                            nearest_title_safe_x(
                                &ancestor_sg.bounds,
                                ancestor_sg.title.as_deref(),
                                entry_x,
                                Direction::BT,
                            )
                        });

                    let owner_id = owner
                        .map(|route_owner| route_owner.id.to_owned())
                        .unwrap_or_else(|| format!("fallback:{}->{}", from.id, to.id));
                    let mut plan = FallbackRoutePlan::new(owner_id, "bt-enter-boundary-chain");
                    let mut current_x = stem_start_x;
                    let mut current_y = stem_start_y;
                    for ancestor_id in enter_subgraphs.iter().rev() {
                        let Some(ancestor_sg) = graph.get_subgraph(ancestor_id) else {
                            continue;
                        };
                        if !ancestor_sg.bounds.is_valid() {
                            continue;
                        }

                        let border_y = ancestor_sg
                            .bounds
                            .y
                            .saturating_add(ancestor_sg.bounds.height.saturating_sub(1));
                        let outside_y = border_y.saturating_add(1);
                        let Some(entry_y) = bt_title_safe_entry_y(ancestor_sg) else {
                            return BtRouteOutcome::NotApplicable;
                        };
                        plan.push_vertical(current_x, current_y, outside_y, style.edge_v);

                        let desired_entry_x = if *ancestor_id == tgt_id {
                            preferred_portal_x(
                                &ancestor_sg.bounds,
                                ancestor_sg.title.as_deref(),
                                arrow_x,
                                canvas,
                                Direction::BT,
                                true,
                            )
                        } else {
                            nearest_title_safe_x(
                                &ancestor_sg.bounds,
                                ancestor_sg.title.as_deref(),
                                shared_entry_x,
                                Direction::BT,
                            )
                        };
                        let entry_x = bt_turn_safe_x(
                            &ancestor_sg.bounds,
                            ancestor_sg.title.as_deref(),
                            desired_entry_x,
                            current_x,
                        );

                        if entry_x != current_x {
                            let start_corner = if entry_x > current_x {
                                style.corner_dl
                            } else {
                                style.corner_dr
                            };
                            let end_corner = if entry_x > current_x {
                                style.corner_ur
                            } else {
                                style.corner_ul
                            };
                            plan.push_corner(current_x, outside_y, start_corner);
                            plan.push_horizontal(outside_y, current_x, entry_x, style.edge_h);
                            plan.push_corner(entry_x, outside_y, end_corner);
                        }

                        // Include the physical border row itself.  It is the
                        // contract that keeps the outside elbow connected to the
                        // title-safe interior shaft after border restoration.
                        plan.claim_boundary(
                            ancestor_sg.id.clone(),
                            "bottom",
                            entry_x,
                            border_y,
                            style.edge_v,
                        );
                        plan.push_vertical(entry_x, outside_y, entry_y, style.edge_v);

                        current_x = entry_x;
                        current_y = entry_y;
                    }

                    if current_x != arrow_x {
                        let start_corner = if arrow_x > current_x {
                            style.corner_dl
                        } else {
                            style.corner_dr
                        };
                        let end_corner = if arrow_x > current_x {
                            style.corner_ur
                        } else {
                            style.corner_ul
                        };
                        plan.push_corner(current_x, current_y, start_corner);
                        plan.push_horizontal(current_y, current_x, arrow_x, style.edge_h);
                        plan.push_corner(arrow_x, current_y, end_corner);
                        plan.push_vertical(arrow_x, current_y, arrow_y, style.edge_v);
                    } else {
                        plan.push_vertical(current_x, current_y, arrow_y, style.edge_v);
                    }

                    if !lower_bt_fallback_plan(plan, canvas, style, graph, owner) {
                        return BtRouteOutcome::NotApplicable;
                    }
                }

                return BtRouteOutcome::Handled;
            }
        }
    }

    // A nested BT source must exit every boundary it owns.  The legacy fallback
    // only routed the immediate source subgraph, leaving the later parent
    // borders to restoration/cleanup and producing detached tails.  Build the
    // complete inner-to-outer chain before lowering it.
    let (exit_subgraphs, enter_subgraphs) = graph.edge_boundary_crossings(&from.id, &to.id);
    if enter_subgraphs.is_empty() && exit_subgraphs.len() > 1 {
        let owner_id = owner
            .map(|route_owner| route_owner.id.to_owned())
            .unwrap_or_else(|| format!("fallback:{}->{}", from.id, to.id));
        let mut plan = FallbackRoutePlan::new(owner_id, "bt-exit-boundary-chain");
        let mut current_x = stem_start_x;
        let mut current_y = stem_start_y;

        for boundary_id in &exit_subgraphs {
            let Some(boundary) = graph.get_subgraph(boundary_id) else {
                return BtRouteOutcome::NotApplicable;
            };
            if !boundary.bounds.is_valid() {
                return BtRouteOutcome::NotApplicable;
            }

            let border_y = boundary.bounds.y;
            let inside_y = border_y.saturating_add(1);
            let outside_y = border_y.saturating_sub(1);
            // The exits cross the top edge.  BT titles live on the bottom
            // interior row, so preserving the source lane avoids needless
            // one-cell elbows without sacrificing title clearance.
            let portal_x = stem_start_x.clamp(
                boundary.bounds.x.saturating_add(1),
                boundary.bounds.x + boundary.bounds.width.saturating_sub(2),
            );

            plan.push_vertical(current_x, current_y, inside_y, style.edge_v);
            if portal_x != current_x {
                let start_corner = if portal_x > current_x {
                    style.corner_dl
                } else {
                    style.corner_dr
                };
                let end_corner = if portal_x > current_x {
                    style.corner_ur
                } else {
                    style.corner_ul
                };
                plan.push_corner(current_x, inside_y, start_corner);
                plan.push_horizontal(inside_y, current_x, portal_x, style.edge_h);
                plan.push_corner(portal_x, inside_y, end_corner);
            }
            plan.claim_boundary(boundary.id.clone(), "top", portal_x, border_y, style.edge_v);
            plan.push_vertical(
                portal_x,
                inside_y.saturating_sub(1),
                outside_y,
                style.edge_v,
            );
            current_x = portal_x;
            current_y = outside_y;
        }

        if current_x != arrow_x {
            let start_corner = if arrow_x > current_x {
                style.corner_dl
            } else {
                style.corner_dr
            };
            let end_corner = if arrow_x > current_x {
                style.corner_ur
            } else {
                style.corner_ul
            };
            plan.push_corner(current_x, current_y, start_corner);
            plan.push_horizontal(current_y, current_x, arrow_x, style.edge_h);
            plan.push_corner(arrow_x, current_y, end_corner);
            plan.push_vertical(arrow_x, current_y, arrow_y, style.edge_v);
        } else {
            plan.push_vertical(current_x, current_y, arrow_y, style.edge_v);
        }

        if lower_bt_fallback_plan(plan, canvas, style, graph, owner) {
            return BtRouteOutcome::Handled;
        }
        return BtRouteOutcome::NotApplicable;
    }

    let Some(src_id) = from_sg else {
        return BtRouteOutcome::NotApplicable;
    };
    let Some(src_sg) = graph.get_subgraph(src_id) else {
        return BtRouteOutcome::NotApplicable;
    };
    if !src_sg.bounds.is_valid() {
        return BtRouteOutcome::NotApplicable;
    }

    let border_y = src_sg.bounds.y;
    let max_inside_y = border_y + src_sg.bounds.height.saturating_sub(2);
    let inside_y = border_y.saturating_add(1).min(max_inside_y);
    let portal_x = preferred_portal_x(
        &src_sg.bounds,
        src_sg.title.as_deref(),
        stem_start_x,
        canvas,
        Direction::BT,
        false,
    );

    // Walk up from the source exit to the row just inside the subgraph top border.
    draw_line_primary(
        stem_start_x,
        stem_start_y,
        stem_start_x,
        inside_y,
        &coords,
        canvas,
        style,
        Some(graph),
        owner,
    );

    // Shift horizontally inside the subgraph to avoid piercing the title span.
    // In BT mode, vertical line comes from below (source), turns horizontal, then up
    if portal_x != stem_start_x {
        // At stem_start_x: vertical from below turns horizontal
        // corner_dl (┌) if turning right, corner_dr (┐) if turning left
        let start_corner = if portal_x > stem_start_x {
            style.corner_dl // came from below, turn right
        } else {
            style.corner_dr // came from below, turn left
        };
        set_route_edge_char(canvas, stem_start_x, inside_y, start_corner, style, owner);

        let (hx0, hx1) = if portal_x > stem_start_x {
            (stem_start_x + 1, portal_x.saturating_sub(1))
        } else {
            (portal_x + 1, stem_start_x.saturating_sub(1))
        };
        for x in hx0..=hx1 {
            if is_subgraph_title_cell(graph, x, inside_y) {
                continue;
            }
            set_route_edge_char(canvas, x, inside_y, style.edge_h, style, owner);
        }

        // At portal_x: horizontal turns to go up through border
        // corner_ur (┘) if came from left, corner_ul (└) if came from right
        let end_corner = if portal_x > stem_start_x {
            style.corner_ur // came from left, turn up
        } else {
            style.corner_ul // came from right, turn up
        };
        set_route_edge_char(canvas, portal_x, inside_y, end_corner, style, owner);
    }

    // Continue up across the title row and border. Prefer bridging back toward the
    // target column on the actual border row so the title row only ever carries
    // a clean vertical portal pierce.
    let border_row_y = border_y;
    let outside_y = border_y.saturating_sub(1);
    let bridge_on_border_row = portal_x != arrow_x;

    if inside_y > border_row_y {
        draw_line_primary(
            portal_x,
            inside_y.saturating_sub(1),
            portal_x,
            border_row_y,
            &coords,
            canvas,
            style,
            Some(graph),
            owner,
        );
    }

    if !bridge_on_border_row {
        draw_line_primary(
            portal_x,
            border_row_y,
            portal_x,
            outside_y,
            &coords,
            canvas,
            style,
            Some(graph),
            owner,
        );
    }

    if bridge_on_border_row {
        let start_corner = if arrow_x > portal_x {
            style.corner_dl
        } else {
            style.corner_dr
        };
        set_route_edge_char(canvas, portal_x, border_row_y, start_corner, style, owner);

        let (hx0, hx1) = if arrow_x > portal_x {
            (portal_x + 1, arrow_x.saturating_sub(1))
        } else {
            (arrow_x + 1, portal_x.saturating_sub(1))
        };
        for x in hx0..=hx1 {
            set_route_edge_char(canvas, x, border_row_y, style.edge_h, style, owner);
        }

        let end_corner = if arrow_x > portal_x {
            style.corner_ur
        } else {
            style.corner_ul
        };
        set_route_edge_char(canvas, arrow_x, border_row_y, end_corner, style, owner);

        if arrow_y < border_row_y {
            draw_line_primary(
                arrow_x,
                border_row_y.saturating_sub(1),
                arrow_x,
                arrow_y,
                &coords,
                canvas,
                style,
                Some(graph),
                owner,
            );
        }
    } else if portal_x != arrow_x && border_y > 0 {
        // In BT mode, vertical line comes from below (larger y), turns horizontal
        // corner_dl (┌) if turning right, corner_dr (┐) if turning left
        let start_corner = if arrow_x > portal_x {
            style.corner_dl // came from below, turn right
        } else {
            style.corner_dr // came from below, turn left
        };
        set_route_edge_char(canvas, portal_x, outside_y, start_corner, style, owner);

        let (hx0, hx1) = if arrow_x > portal_x {
            (portal_x + 1, arrow_x.saturating_sub(1))
        } else {
            (arrow_x + 1, portal_x.saturating_sub(1))
        };
        for x in hx0..=hx1 {
            set_route_edge_char(canvas, x, outside_y, style.edge_h, style, owner);
        }

        // Horizontal line turns up toward arrow
        // corner_ur (┘) if came from left, corner_ul (└) if came from right
        let end_corner = if arrow_x > portal_x {
            style.corner_ur // came from left, turn up
        } else {
            style.corner_ul // came from right, turn up
        };
        set_route_edge_char(canvas, arrow_x, outside_y, end_corner, style, owner);

        let v_start_y = outside_y.saturating_sub(1);
        draw_line_primary(
            arrow_x,
            v_start_y,
            arrow_x,
            arrow_y,
            &coords,
            canvas,
            style,
            Some(graph),
            owner,
        );
    } else if !bridge_on_border_row {
        draw_line_primary(
            portal_x,
            outside_y,
            portal_x,
            arrow_y,
            &coords,
            canvas,
            style,
            Some(graph),
            owner,
        );
        if portal_x != arrow_x {
            // Fallback: if we have no room above the border, bridge on the arrow row.
            // In BT mode, vertical comes from below, turns horizontal
            let corner = if portal_x < arrow_x {
                style.corner_dl // came from below, turn right
            } else {
                style.corner_dr // came from below, turn left
            };
            set_route_edge_char(canvas, portal_x, arrow_y, corner, style, owner);
            let (hx0, hx1) = if portal_x < arrow_x {
                (portal_x + 1, arrow_x)
            } else {
                (arrow_x, portal_x.saturating_sub(1))
            };
            for x in hx0..=hx1 {
                set_route_edge_char(canvas, x, arrow_y, style.edge_h, style, owner);
            }
        }
    }

    // Ensure the top border reads as a clean pierce (not a junction).
    if portal_x < canvas.width
        && border_y < canvas.height
        && !is_textual(canvas.get(portal_x, border_y))
        && !bridge_on_border_row
    {
        set_route_char(canvas, portal_x, border_y, style.edge_v, owner);
    }

    BtRouteOutcome::Handled
}

pub(super) fn route_divergent_into_subgraph_td(
    source: &Node,
    targets: &[&Node],
    canvas: &mut Canvas,
    style: &StyleChars,
    sg: &crate::graph::Subgraph,
    direction: Direction,
    graph: &Graph,
) {
    if targets.is_empty() || !sg.bounds.is_valid() {
        return;
    }
    let coords = OrientedCoords::new(direction);
    let fanout_owner_id = format!("fanout:{}", source.id);
    let fanout_owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: fanout_owner_id.as_str(),
    };
    // Branch row just below the entry (title stays on the border row).
    let mut target_positions: Vec<(usize, usize, &Node)> = targets
        .iter()
        .map(|n| {
            let (tx, ty) = get_node_center(n);
            (tx, ty, *n)
        })
        .collect();
    target_positions.sort_by_key(|(x, y, _)| coords.secondary_coord(*x, *y));

    // Enter just inside the top border so we can show a spine row before branching.
    // Do not draw on the border/title row: edges should pass under the title.
    let border_y = sg.bounds.y;
    let outside_y = border_y.saturating_sub(1);
    let entry_y = border_y.saturating_add(1);
    let min_inner_x = sg.bounds.x.saturating_add(1);
    let max_inner_x = sg.bounds.x + sg.bounds.width.saturating_sub(2);

    // The portal collector collapses a fan-out into one top-boundary lane at
    // the subgraph center (clamped to the target span). Reuse that exact lane
    // here so the source shaft, border opening, and internal branch junction
    // share one column instead of producing adjacent `||`/`++` artifacts.
    let portal_center = sg.bounds.x + sg.bounds.width / 2;
    let min_target_x = target_positions
        .iter()
        .map(|(x, _, _)| *x)
        .min()
        .unwrap_or(portal_center);
    let max_target_x = target_positions
        .iter()
        .map(|(x, _, _)| *x)
        .max()
        .unwrap_or(portal_center);

    // Connect source to the subgraph entry (outside the border).
    let (stem_x, stem_y) = edge_exit_point(source, direction);
    let portal_x = portal_center.clamp(min_target_x, max_target_x);
    let entry_x = portal_x.clamp(min_inner_x, max_inner_x);
    set_route_edge_char(
        canvas,
        stem_x,
        stem_y,
        coords.primary_edge_char(style),
        style,
        Some(fanout_owner),
    );

    // Walk vertically down to just above the border, then (optionally) shift horizontally.
    // This avoids drawing through the title row.
    let turn_y = if stem_y < outside_y {
        outside_y
    } else {
        stem_y
    };
    if stem_y < outside_y {
        for y in (stem_y + 1)..=outside_y {
            set_route_edge_char(
                canvas,
                stem_x,
                y,
                coords.primary_edge_char(style),
                style,
                Some(fanout_owner),
            );
        }
    }
    if entry_x != stem_x && turn_y < canvas.height {
        let start_corner = if entry_x > stem_x {
            style.corner_ul
        } else {
            style.corner_ur
        };
        set_route_edge_char(
            canvas,
            stem_x,
            turn_y,
            start_corner,
            style,
            Some(fanout_owner),
        );

        let (hx0, hx1) = if entry_x > stem_x {
            (stem_x.saturating_add(1), entry_x.saturating_sub(1))
        } else {
            (entry_x.saturating_add(1), stem_x.saturating_sub(1))
        };
        for x in hx0..=hx1 {
            set_route_edge_char(canvas, x, turn_y, style.edge_h, style, Some(fanout_owner));
        }

        if entry_x.abs_diff(stem_x) == 1 {
            // A one-cell lateral shift has no interior horizontal cell. Keep
            // the turn legible as `corner + edge + next-row shaft` instead of
            // placing two adjacent corners (`++`/`└┐`) on the approach row.
            set_route_edge_char(
                canvas,
                entry_x,
                turn_y,
                style.edge_h,
                style,
                Some(fanout_owner),
            );
        } else {
            let end_corner = if entry_x > stem_x {
                style.corner_dr
            } else {
                style.corner_dl
            };
            set_route_edge_char(
                canvas,
                entry_x,
                turn_y,
                end_corner,
                style,
                Some(fanout_owner),
            );
        }
    }

    let min_x = min_target_x.min(entry_x);
    let max_x = max_target_x.max(entry_x);

    let min_arrow_y = targets
        .iter()
        .map(|n| adjusted_edge_entry_point(n, direction, graph).1)
        .min()
        .unwrap_or(entry_y + 3);

    // Spine row (center column only) then a dedicated branch row.
    let spine_y = entry_y;
    if spine_y < canvas.height {
        // Clear any pre-carved portal reinforcements on this row for target columns,
        // then draw a single spine down the center.
        for (tx, _, _) in &target_positions {
            if *tx < canvas.width {
                canvas.set(*tx, spine_y, ' ');
            }
        }
        set_route_edge_char(
            canvas,
            entry_x,
            spine_y,
            coords.primary_edge_char(style),
            style,
            Some(fanout_owner),
        );
    }

    let mut branch_y = spine_y.saturating_add(1);
    if branch_y + 1 >= min_arrow_y {
        branch_y = min_arrow_y.saturating_sub(2);
    }
    branch_y = branch_y.max(spine_y.saturating_add(1));

    // Ensure the trunk stays connected from the spine row to the branch row.
    if branch_y > spine_y.saturating_add(1) {
        for y in (spine_y + 1)..branch_y {
            if entry_x < canvas.width && y < canvas.height {
                set_route_edge_char(
                    canvas,
                    entry_x,
                    y,
                    coords.primary_edge_char(style),
                    style,
                    Some(fanout_owner),
                );
            }
        }
    }

    // Branch row: horizontal bar with an entry junction that matches the actual
    // drop topology at the entry column.
    for x in min_x..=max_x {
        set_route_edge_char(canvas, x, branch_y, style.edge_h, style, Some(fanout_owner));
    }
    set_route_char(canvas, min_x, branch_y, style.corner_dl, Some(fanout_owner));
    set_route_char(canvas, max_x, branch_y, style.corner_dr, Some(fanout_owner));
    let entry_has_drop = target_positions.iter().any(|(tx, _, _)| *tx == entry_x);
    let entry_char = if min_x == max_x {
        style.edge_v
    } else if entry_x == min_x {
        if entry_has_drop {
            style.junction_right
        } else {
            style.corner_dl
        }
    } else if entry_x == max_x {
        if entry_has_drop {
            style.junction_left
        } else {
            style.corner_dr
        }
    } else if entry_has_drop {
        style.cross
    } else {
        style.junction_up
    };
    set_route_char(canvas, entry_x, branch_y, entry_char, Some(fanout_owner));

    // Drop to targets starting immediately after the branch row.
    for (tx, _, target) in target_positions {
        let branch_owner_id = edge_route_owner_id(graph, &source.id, &target.id);
        let branch_owner = RouteOwner {
            kind: CellOwnerKind::EdgeSegment,
            id: branch_owner_id.as_str(),
        };
        let (arrow_x, arrow_y) = adjusted_edge_entry_point(target, direction, graph);
        let start_y = branch_y.saturating_add(1);
        for y in start_y..arrow_y {
            set_route_edge_char(canvas, tx, y, style.edge_v, style, Some(branch_owner));
        }
        set_route_char(
            canvas,
            arrow_x,
            arrow_y,
            coords.arrow_end(style),
            Some(branch_owner),
        );
    }
}

/// Route a horizontal fan-out from an external source into one subgraph.
///
/// The source enters through one boundary portal and then branches from a
/// collector rail that is guaranteed to remain outside every target node. A
/// shared collector is important here: routing each target independently can
/// leave the off-axis branches disconnected from the source-side portal.
#[allow(clippy::too_many_arguments)]
pub(super) fn route_divergent_into_subgraph_horizontal(
    source: &Node,
    targets: &[&Node],
    canvas: &mut Canvas,
    style: &StyleChars,
    direction: Direction,
    sg: &crate::graph::Subgraph,
    graph: &Graph,
) -> bool {
    if targets.is_empty()
        || !matches!(direction, Direction::LR | Direction::RL)
        || !sg.bounds.is_valid()
    {
        return false;
    }

    let coords = OrientedCoords::new(direction);
    let fanout_owner_id = format!("fanout:{}", source.id);
    let fanout_owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: fanout_owner_id.as_str(),
    };

    let mut target_positions: Vec<(usize, usize, &Node)> = targets
        .iter()
        .map(|node| {
            let (x, y) = get_node_center(node);
            (x, y, *node)
        })
        .collect();
    target_positions.sort_by_key(|(_, y, node)| (*y, node.id.clone()));

    let border_x = match direction {
        Direction::LR => sg.bounds.x,
        Direction::RL => sg
            .bounds
            .x
            .saturating_add(sg.bounds.width.saturating_sub(1)),
        _ => unreachable!(),
    };
    let outside_x = match direction {
        Direction::LR => border_x.checked_sub(1),
        Direction::RL => border_x.checked_add(1),
        _ => None,
    };
    let Some(outside_x) = outside_x else {
        return false;
    };
    if outside_x >= canvas.width {
        return false;
    }

    let interior_min = sg.bounds.x.saturating_add(1);
    let interior_max = sg
        .bounds
        .x
        .saturating_add(sg.bounds.width.saturating_sub(2));
    if interior_max < interior_min {
        return false;
    }

    let mut arrows = Vec::with_capacity(target_positions.len());
    for (_, _, target) in &target_positions {
        let arrow = adjusted_edge_entry_point(target, direction, graph);
        if arrow.0 >= canvas.width || arrow.1 >= canvas.height {
            return false;
        }
        arrows.push((arrow.0, arrow.1, *target));
    }

    let min_arrow_x = arrows.iter().map(|(x, _, _)| *x).min().unwrap_or(0);
    let max_arrow_x = arrows.iter().map(|(x, _, _)| *x).max().unwrap_or(0);
    // Leave one empty interior cell between the collector and the wall. The
    // final side-portal pass treats a route immediately adjacent to a wall as
    // evidence of a crossing on every row; the gap keeps the collector from
    // turning the entire subgraph side into horizontal portal markers.
    let collector_min = interior_min.saturating_add(1);
    let collector_max = interior_max.saturating_sub(1);
    if collector_max < collector_min {
        return false;
    }
    let collector_x = match direction {
        Direction::LR => min_arrow_x
            .saturating_sub(2)
            .clamp(collector_min, collector_max),
        Direction::RL => max_arrow_x
            .saturating_add(2)
            .clamp(collector_min, collector_max),
        _ => unreachable!(),
    };

    // If the collector would overlap a target or leave no room for a branch,
    // reject the specialized scene before writing anything and let the generic
    // path make the safer fallback decision.
    let collector_clear = target_positions.iter().all(|(_, _, target)| {
        let right = target.x.saturating_add(target.width);
        collector_x < target.x || collector_x > right
    });
    let branches_fit = arrows.iter().all(|(arrow_x, _, _)| match direction {
        Direction::LR => collector_x < *arrow_x,
        Direction::RL => collector_x > *arrow_x,
        _ => false,
    });
    if !collector_clear || !branches_fit {
        return false;
    }

    let min_y = arrows.iter().map(|(_, y, _)| *y).min().unwrap_or(0);
    let max_y = arrows.iter().map(|(_, y, _)| *y).max().unwrap_or(min_y);
    let (source_x, source_y) = edge_exit_point(source, direction);
    let entry_y = source_y.clamp(min_y, max_y);

    let source_is_outside = match direction {
        Direction::LR => source_x < border_x,
        Direction::RL => source_x > border_x,
        _ => false,
    };
    if !source_is_outside {
        return false;
    }

    // Bring the external source to the selected portal row.  This bend is
    // outside the subgraph, so the title and interior collector remain clean.
    draw_line_primary(
        source_x,
        source_y,
        outside_x,
        source_y,
        &coords,
        canvas,
        style,
        Some(graph),
        Some(fanout_owner),
    );
    if source_y != entry_y {
        let going_up = entry_y < source_y;
        let first_corner = match direction {
            Direction::LR => {
                if going_up {
                    style.corner_ur
                } else {
                    style.corner_dr
                }
            }
            Direction::RL => {
                if going_up {
                    style.corner_ul
                } else {
                    style.corner_dl
                }
            }
            _ => unreachable!(),
        };
        set_route_edge_char(
            canvas,
            outside_x,
            source_y,
            first_corner,
            style,
            Some(fanout_owner),
        );
        draw_line_secondary(
            outside_x,
            source_y,
            outside_x,
            entry_y,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(fanout_owner),
        );
        let second_corner = match direction {
            Direction::LR => {
                if going_up {
                    style.corner_dl
                } else {
                    style.corner_ul
                }
            }
            Direction::RL => {
                if going_up {
                    style.corner_dr
                } else {
                    style.corner_ur
                }
            }
            _ => unreachable!(),
        };
        set_route_edge_char(
            canvas,
            outside_x,
            entry_y,
            second_corner,
            style,
            Some(fanout_owner),
        );
    }

    draw_line_primary(
        outside_x,
        entry_y,
        collector_x,
        entry_y,
        &coords,
        canvas,
        style,
        Some(graph),
        Some(fanout_owner),
    );
    stamp_portal_opening(
        canvas,
        border_x,
        entry_y,
        style,
        PortalAxis::Horizontal,
        fanout_owner_id.as_str(),
        ROUTE_Z_INDEX,
    );

    // The collector owns the shared vertical rail.  Its branch rows are
    // canonicalized after branch lowering so branch-specific edge styles do
    // not erase the split junction.
    for y in min_y..=max_y {
        set_route_edge_char(
            canvas,
            collector_x,
            y,
            style.edge_v,
            style,
            Some(fanout_owner),
        );
    }

    for (arrow_x, arrow_y, target) in &arrows {
        let branch_owner_id = edge_route_owner_id(graph, &source.id, &target.id);
        let branch_owner = RouteOwner {
            kind: CellOwnerKind::EdgeSegment,
            id: branch_owner_id.as_str(),
        };
        let edge_kind = graph
            .edges
            .iter()
            .find(|edge| !edge.is_back_edge && edge.from == source.id && edge.to == target.id)
            .map(|edge| edge.kind)
            .unwrap_or(EdgeKind::Arrow);
        let branch_style = style_for_edge_kind(style, edge_kind);
        draw_line_primary(
            collector_x,
            *arrow_y,
            *arrow_x,
            *arrow_y,
            &coords,
            canvas,
            &branch_style,
            Some(graph),
            Some(branch_owner),
        );
        let tip = match edge_kind {
            EdgeKind::CircleEnd => style.circle_end,
            EdgeKind::CrossEnd => style.cross_end,
            EdgeKind::Open => coords.primary_edge_char(&branch_style),
            _ => coords.arrow_end(style),
        };
        if matches!(edge_kind, EdgeKind::CircleEnd | EdgeKind::CrossEnd) {
            set_route_endpoint_char(canvas, *arrow_x, *arrow_y, tip, branch_owner);
        } else {
            set_route_char(canvas, *arrow_x, *arrow_y, tip, Some(branch_owner));
        }
    }

    for (_, y, _) in &target_positions {
        let glyph = if *y == entry_y {
            style.cross
        } else if *y == min_y {
            match direction {
                Direction::LR => style.corner_dl,
                Direction::RL => style.corner_ur,
                _ => unreachable!(),
            }
        } else if *y == max_y {
            match direction {
                Direction::LR => style.corner_ul,
                Direction::RL => style.corner_dr,
                _ => unreachable!(),
            }
        } else {
            match direction {
                Direction::LR => style.junction_right,
                Direction::RL => style.junction_left,
                _ => unreachable!(),
            }
        };
        set_route_char(canvas, collector_x, *y, glyph, Some(fanout_owner));
    }

    true
}

pub(super) fn route_divergent_into_subgraph_bt(
    source: &Node,
    targets: &[&Node],
    canvas: &mut Canvas,
    style: &StyleChars,
    sg: &crate::graph::Subgraph,
    graph: &Graph,
) {
    if targets.is_empty() || !sg.bounds.is_valid() {
        return;
    }

    let coords = OrientedCoords::new(Direction::BT);
    let fanout_owner_id = format!("fanout:{}", source.id);
    let fanout_owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: fanout_owner_id.as_str(),
    };

    let mut target_positions: Vec<(usize, usize, &Node)> = targets
        .iter()
        .map(|n| {
            let (tx, ty) = get_node_center(n);
            (tx, ty, *n)
        })
        .collect();
    target_positions.sort_by_key(|(x, y, _)| coords.secondary_coord(*x, *y));

    let border_y = sg.bounds.y + sg.bounds.height.saturating_sub(1);
    let outside_y = border_y.saturating_add(1);
    let entry_y = border_y.saturating_sub(1);

    let (stem_x, stem_y) = edge_exit_point(source, Direction::BT);
    let entry_x = preferred_portal_x(
        &sg.bounds,
        sg.title.as_deref(),
        stem_x,
        canvas,
        Direction::BT,
        true,
    );

    set_route_edge_char(
        canvas,
        stem_x,
        stem_y,
        coords.primary_edge_char(style),
        style,
        Some(fanout_owner),
    );

    if stem_y > outside_y {
        draw_line_primary(
            stem_x,
            stem_y,
            stem_x,
            outside_y,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(fanout_owner),
        );
    }

    if entry_x != stem_x && outside_y < canvas.height {
        let start_corner = if entry_x > stem_x {
            style.corner_dl
        } else {
            style.corner_dr
        };
        set_route_edge_char(
            canvas,
            stem_x,
            outside_y,
            start_corner,
            style,
            Some(fanout_owner),
        );

        let (hx0, hx1) = if entry_x > stem_x {
            (stem_x.saturating_add(1), entry_x.saturating_sub(1))
        } else {
            (entry_x.saturating_add(1), stem_x.saturating_sub(1))
        };
        for x in hx0..=hx1 {
            set_route_edge_char(
                canvas,
                x,
                outside_y,
                style.edge_h,
                style,
                Some(fanout_owner),
            );
        }

        let end_corner = if entry_x > stem_x {
            style.corner_ur
        } else {
            style.corner_ul
        };
        set_route_edge_char(
            canvas,
            entry_x,
            outside_y,
            end_corner,
            style,
            Some(fanout_owner),
        );
    }

    if entry_y < canvas.height {
        set_route_edge_char(
            canvas,
            entry_x,
            entry_y,
            coords.primary_edge_char(style),
            style,
            Some(fanout_owner),
        );
    }

    let min_x = target_positions
        .iter()
        .map(|(x, _, _)| *x)
        .min()
        .unwrap_or(entry_x)
        .min(entry_x);
    let max_x = target_positions
        .iter()
        .map(|(x, _, _)| *x)
        .max()
        .unwrap_or(entry_x)
        .max(entry_x);
    let max_arrow_y = targets
        .iter()
        .map(|n| adjusted_edge_entry_point(n, Direction::BT, graph).1)
        .max()
        .unwrap_or(entry_y.saturating_sub(3));

    let mut branch_y = entry_y.saturating_sub(1);
    if branch_y <= max_arrow_y {
        branch_y = max_arrow_y.saturating_add(1);
    }
    branch_y = branch_y.min(entry_y.saturating_sub(1));

    if branch_y + 1 < entry_y {
        for y in (branch_y + 1)..entry_y {
            if entry_x < canvas.width && y < canvas.height {
                set_route_edge_char(
                    canvas,
                    entry_x,
                    y,
                    coords.primary_edge_char(style),
                    style,
                    Some(fanout_owner),
                );
            }
        }
    }

    for x in min_x..=max_x {
        set_route_edge_char(canvas, x, branch_y, style.edge_h, style, Some(fanout_owner));
    }
    set_route_char(canvas, min_x, branch_y, style.corner_ul, Some(fanout_owner));
    set_route_char(canvas, max_x, branch_y, style.corner_ur, Some(fanout_owner));
    set_route_char(
        canvas,
        entry_x,
        branch_y,
        style.junction_down,
        Some(fanout_owner),
    );

    for (tx, _, target) in target_positions {
        let branch_owner_id = edge_route_owner_id(graph, &source.id, &target.id);
        let branch_owner = RouteOwner {
            kind: CellOwnerKind::EdgeSegment,
            id: branch_owner_id.as_str(),
        };
        let (arrow_x, arrow_y) = adjusted_edge_entry_point(target, Direction::BT, graph);
        if arrow_y + 1 < branch_y {
            for y in (arrow_y + 1)..branch_y {
                set_route_edge_char(canvas, tx, y, style.edge_v, style, Some(branch_owner));
            }
        }
        set_route_char(
            canvas,
            arrow_x,
            arrow_y,
            coords.arrow_end(style),
            Some(branch_owner),
        );
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{
        bt_sibling_plan_blocker, bt_sibling_target_lane_candidates, bt_title_safe_entry_y,
        bt_title_safe_entry_y_with_margin, nearest_title_safe_x, preferred_portal_x,
        td_sibling_corridor_row,
    };
    use crate::graph::{Direction, Graph, Node, Rectangle, Subgraph};
    use crate::portals::{title_margin_for_direction, title_safe_portal_x, PortalColumnPreference};
    use crate::render::canvas::Canvas;
    use crate::render::fallback_route::{FallbackAxis, FallbackRoutePlan};

    #[test]
    fn preferred_portal_uses_the_shared_directional_policy() {
        let bounds = Rectangle::new(10, 0, 16, 10);
        let canvas = Canvas::new(40, 20);

        for direction in [Direction::BT, Direction::TD, Direction::LR, Direction::RL] {
            assert_eq!(
                preferred_portal_x(&bounds, Some("S2"), 15, &canvas, direction, true,),
                title_safe_portal_x(
                    bounds.x,
                    bounds.width,
                    Some("S2"),
                    15,
                    direction,
                    title_margin_for_direction(direction),
                    PortalColumnPreference::Directional,
                ),
                "renderer and shared policy diverged for {direction:?}"
            );
        }
    }

    #[test]
    fn nearest_nested_portal_uses_the_shared_nearest_policy() {
        let bounds = Rectangle::new(10, 0, 16, 10);

        for direction in [Direction::BT, Direction::TD, Direction::LR, Direction::RL] {
            assert_eq!(
                nearest_title_safe_x(&bounds, Some("S2"), 15, direction),
                title_safe_portal_x(
                    bounds.x,
                    bounds.width,
                    Some("S2"),
                    15,
                    direction,
                    title_margin_for_direction(direction),
                    PortalColumnPreference::Nearest,
                ),
                "nested renderer and shared policy diverged for {direction:?}"
            );
        }
    }

    #[test]
    fn bt_title_safe_entry_row_is_strictly_above_the_title() {
        let mut subgraph = Subgraph::new("target", Some("Target".to_owned()));
        subgraph.bounds = Rectangle::new(2, 10, 20, 8);

        // BT title row is y=16 and the physical bottom border is y=17.
        assert_eq!(bt_title_safe_entry_y(&subgraph), Some(15));
        assert_eq!(bt_title_safe_entry_y_with_margin(&subgraph, 1), Some(14));
    }

    #[test]
    fn untitled_bt_entry_keeps_the_nearest_interior_row() {
        let mut subgraph = Subgraph::new("target", None);
        subgraph.bounds = Rectangle::new(2, 10, 20, 8);

        assert_eq!(bt_title_safe_entry_y(&subgraph), Some(16));
    }

    #[test]
    fn narrow_titled_bt_subgraph_fails_closed_without_a_safe_turn_row() {
        let mut subgraph = Subgraph::new("target", Some("Target".to_owned()));
        subgraph.bounds = Rectangle::new(2, 10, 20, 3);

        assert_eq!(bt_title_safe_entry_y(&subgraph), None);
    }

    #[test]
    fn td_sibling_corridor_requires_clearance_on_both_sides() {
        assert_eq!(td_sibling_corridor_row(8, 12), Some(10));
        assert_eq!(td_sibling_corridor_row(13, 16), Some(14));
        assert_eq!(td_sibling_corridor_row(13, 15), None);
    }

    #[test]
    fn bt_sibling_target_lane_candidates_keep_a_clear_border_cell() {
        let bounds = Rectangle::new(0, 0, 46, 18);
        let canvas = Canvas::new(50, 60);
        let candidates =
            bt_sibling_target_lane_candidates(&bounds, Some("Data Layer"), 14, 14, &canvas, false);

        assert!(!candidates.contains(&1), "lane may not hug the left border");
        assert!(
            !candidates.contains(&44),
            "lane may not hug the right border"
        );
        assert!(
            candidates.contains(&14),
            "preferred title-safe lane remains eligible"
        );
    }

    #[test]
    fn bt_sibling_plan_rejects_full_span_external_node_keepout() {
        let mut graph = Graph::new();
        let mut response = Node::new("Response", "Response Builder");
        response.x = 10;
        response.y = 20;
        response.width = 22;
        response.height = 3;
        graph.nodes.push(response);

        let canvas = Canvas::new(50, 60);
        let mut plan = FallbackRoutePlan::new("edge:3:S2->D2", "bt-sibling-boundary-corridor");
        plan.push_vertical(14, 15, 23, '|');

        let blocker = plan.planned_cells().into_iter().find_map(|(x, y)| {
            bt_sibling_plan_blocker(&plan, x, y, (18, 32), (13, 5), &canvas, &graph)
        });

        let blocker = blocker.expect("external node must block the full target shaft");
        assert!(blocker.contains("Response"));
        assert!(blocker.contains("(14,19)"));
    }

    #[test]
    #[cfg(feature = "maintainer-fixtures")]
    fn complex_bt_external_node_is_relocated_for_sibling_route() {
        let input = include_str!("../../../tests/fixtures/inputs/subgraph_complex_bt.md");
        let outcome = crate::render_with_feedback(input, crate::RenderOptions::default())
            .expect("render fixture");

        assert!(!outcome
            .portal_trace
            .fallback_route_rejections
            .iter()
            .any(|rejection| rejection.owner_id == "edge:3:S2->D2"));
        assert!(outcome
            .portal_trace
            .fallback_routes
            .iter()
            .any(|route| route.owner_id == "edge:3:S2->D2"));
        let route = outcome
            .portal_trace
            .fallback_routes
            .iter()
            .find(|route| route.owner_id == "edge:3:S2->D2")
            .expect("complex BT receiver route should be traced");
        assert_eq!(route.planned_segments.len(), 3);
        assert!(route
            .planned_segments
            .iter()
            .all(|segment| segment.axis == FallbackAxis::Vertical));
        assert!(route
            .boundary_claims
            .iter()
            .filter(|claim| claim.boundary_id == "SG2")
            .all(|claim| claim.x == 18));

        let source_cells: Vec<_> = outcome
            .semantic_frame
            .cells
            .iter()
            .enumerate()
            .filter(|(_, cell)| cell.owner_id.as_deref() == Some("edge:1:API->S1"))
            .collect();
        assert!(
            source_cells.len() >= 2,
            "source shaft should remain visible"
        );
        let source_xs: BTreeSet<_> = source_cells
            .iter()
            .map(|(index, _)| index % outcome.semantic_frame.width)
            .collect();
        assert_eq!(
            source_xs.len(),
            1,
            "API -> S1 should use one vertical lane: {source_cells:?}"
        );
    }
}
