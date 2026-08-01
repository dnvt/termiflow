//! Subgraph boundary and portal routing policy.

use crate::graph::{Direction, Graph, Node};
use crate::orientation::OrientedCoords;
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::edge_policy::title_safe_td_entry_x;
use super::super::portal_projection::{is_textual, stamp_portal_opening, title_span};
use super::super::semantic::CellOwnerKind;
use super::edge_primitives::{
    adjusted_edge_entry_point, draw_line_primary, edge_exit_point, get_node_center,
    is_subgraph_title_cell,
};
use super::{edge_route_owner_id, set_route_char, set_route_edge_char, RouteOwner, ROUTE_Z_INDEX};

pub(super) fn preferred_portal_x(
    bounds: &crate::graph::Rectangle,
    title: Option<&str>,
    desired: usize,
    canvas: &Canvas,
    direction: Direction,
    avoid_title: bool,
) -> usize {
    let min = bounds.x.saturating_add(1);
    let max = bounds.x + bounds.width.saturating_sub(2);
    let _ = canvas;
    let mut x = desired.clamp(min, max);

    let mut protected_title_span: Option<(usize, usize)> = None;
    if avoid_title {
        if let Some(t) = title {
            let Some((start, end)) = title_span(bounds, t, direction) else {
                return x;
            };
            protected_title_span = Some((start, end));
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
        }
    }

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

fn nearest_title_safe_x(
    bounds: &crate::graph::Rectangle,
    title: Option<&str>,
    desired: usize,
    direction: Direction,
) -> usize {
    let min = bounds.x.saturating_add(1);
    let max = bounds.x + bounds.width.saturating_sub(2);
    let x = desired.clamp(min, max);
    let Some(title) = title else {
        return x;
    };

    let Some((start, end)) = title_span(bounds, title, direction) else {
        return x;
    };
    let protected_start = start.saturating_sub(2);
    let protected_end = end.saturating_add(2).min(max);
    if x < protected_start || x > protected_end {
        return x;
    }

    let left = (protected_start > min).then(|| protected_start.saturating_sub(1));
    let right = (protected_end < max).then(|| protected_end + 1);
    match (left, right) {
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
    }
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
    let debug_timing = std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok();
    let from_sg = graph.get_node_subgraph(&from.id);
    let to_sg = graph.get_node_subgraph(&to.id);
    if from_sg == to_sg {
        return false;
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

    // Common case: edge enters a subgraph from above in TD/TB. Visually, we want the
    // stem to pass *under* the title (i.e., avoid drawing on the border/title row),
    // so the title stays readable and the top border remains clean.
    let entering_from_above =
        stem_start_y < sg.bounds.y && arrow_y >= sg.bounds.y.saturating_add(1);
    if entering_from_above {
        let arrow_x = title_safe_td_entry_x(to, arrow_x, arrow_y, stem_start_y, graph);
        let (_, enter_subgraphs) = graph.edge_boundary_crossings(&from.id, &to.id);
        let mut current_x = stem_start_x;
        let mut current_y = stem_start_y;
        let final_entry_x = preferred_portal_x(
            &sg.bounds,
            sg.title.as_deref(),
            arrow_x,
            canvas,
            graph.direction,
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
                    graph.direction,
                )
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

            let entry_x = nearest_title_safe_x(
                &ancestor_sg.bounds,
                ancestor_sg.title.as_deref(),
                shared_entry_x,
                graph.direction,
            );

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
        if current_x != arrow_x && arrow_y > current_y {
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
                if is_subgraph_title_cell(graph, x, bridge_y) {
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
                if is_subgraph_title_cell(graph, arrow_x, y) {
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
            if is_subgraph_title_cell(graph, turn_x, y) {
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
            if is_subgraph_title_cell(graph, turn_x, y) {
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
                if is_subgraph_title_cell(graph, cursor_x, y) {
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
            if is_subgraph_title_cell(graph, x, cursor_y) {
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
            if is_subgraph_title_cell(graph, portal_x, y) {
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
            if is_subgraph_title_cell(graph, x, arrow_y) {
                continue;
            }
            set_route_edge_char(canvas, x, arrow_y, style.edge_h, style, owner);
        }
    } else if arrow_y > portal_y {
        for y in (portal_y + 1)..=arrow_y {
            if is_subgraph_title_cell(graph, portal_x, y) {
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
) -> bool {
    let coords = OrientedCoords::new(Direction::BT);
    let from_sg = graph.get_node_subgraph(&from.id);
    let to_sg = graph.get_node_subgraph(&to.id);
    if from_sg == to_sg {
        return false;
    }

    if let Some(tgt_id) = to_sg {
        let Some(tgt_sg) = graph.get_subgraph(tgt_id) else {
            return false;
        };
        if tgt_sg.bounds.is_valid() {
            let tgt_border_y = tgt_sg.bounds.y + tgt_sg.bounds.height.saturating_sub(1);
            let entering_from_below = stem_start_y > tgt_border_y && arrow_y < tgt_border_y;
            if entering_from_below {
                let (_, enter_subgraphs) = graph.edge_boundary_crossings(&from.id, &to.id);
                if enter_subgraphs.len() <= 1 {
                    let entry_x = preferred_portal_x(
                        &tgt_sg.bounds,
                        tgt_sg.title.as_deref(),
                        arrow_x,
                        canvas,
                        Direction::BT,
                        true,
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
                        draw_line_primary(
                            current_x,
                            current_y,
                            current_x,
                            outside_y,
                            &coords,
                            canvas,
                            style,
                            Some(graph),
                            owner,
                        );

                        let entry_x = if *ancestor_id == tgt_id {
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
                                current_x,
                                Direction::BT,
                            )
                        };

                        if entry_x != current_x && outside_y < canvas.height {
                            let start_corner = if entry_x > current_x {
                                style.corner_dl
                            } else {
                                style.corner_dr
                            };
                            set_route_edge_char(
                                canvas,
                                current_x,
                                outside_y,
                                start_corner,
                                style,
                                owner,
                            );

                            let (hx0, hx1) = if entry_x > current_x {
                                (current_x + 1, entry_x.saturating_sub(1))
                            } else {
                                (entry_x + 1, current_x.saturating_sub(1))
                            };
                            for x in hx0..=hx1 {
                                set_route_edge_char(
                                    canvas,
                                    x,
                                    outside_y,
                                    style.edge_h,
                                    style,
                                    owner,
                                );
                            }

                            let end_corner = if entry_x > current_x {
                                style.corner_ur
                            } else {
                                style.corner_ul
                            };
                            set_route_edge_char(
                                canvas, entry_x, outside_y, end_corner, style, owner,
                            );
                        }

                        current_x = entry_x;
                        current_y = border_y.saturating_sub(1);
                    }

                    draw_line_primary(
                        current_x,
                        current_y,
                        current_x,
                        arrow_y,
                        &coords,
                        canvas,
                        style,
                        Some(graph),
                        owner,
                    );
                }

                return true;
            }
        }
    }

    let Some(src_id) = from_sg else {
        return false;
    };
    let Some(src_sg) = graph.get_subgraph(src_id) else {
        return false;
    };
    if !src_sg.bounds.is_valid() {
        return false;
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

    true
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

    // Connect source to the subgraph entry (outside the border).
    let (stem_x, stem_y) = edge_exit_point(source, direction);
    let entry_x = stem_x.clamp(min_inner_x, max_inner_x);
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
