//! Convergence and fan-in route orchestration.

use crate::graph::{Direction, Graph, Node, NodeShape};
use crate::orientation::OrientedCoords;
use crate::spacing::SpacingConfig;
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::fallback_route::FallbackRoutePlan;
use super::super::fan_in_identity::is_vertical_branch_rejoin_target;
use super::super::portal_projection::{is_textual, stamp_portal_opening, PortalAxis};
use super::super::semantic::CellOwnerKind;
use super::edge_primitives::{
    adjusted_edge_entry_point, draw_line_primary, draw_line_secondary, edge_entry_candidates,
    edge_exit_point, fan_in_source_uses_left_port, get_node_center, hits_foreign_subgraph_border,
    is_subgraph_title_cell, is_subgraph_title_text_cell,
};
use super::subgraph::{preferred_portal_x, smallest_visual_container};
use super::{edge_route_owner_id, set_route_char, set_route_edge_char, RouteOwner, ROUTE_Z_INDEX};

fn strict_simple_subgraph_fanin(sources: &[&Node], target: &Node, graph: &Graph) -> bool {
    graph.subgraphs.len() == 1
        && sources.len() >= 3
        && graph.edges.len() == sources.len()
        && sources
            .iter()
            .all(|source| source.shape == crate::graph::NodeShape::Rectangle)
        && target.shape == crate::graph::NodeShape::Rectangle
        && graph.edges.iter().all(|edge| {
            !edge.is_back_edge
                && edge.label.is_none()
                && matches!(edge.kind, crate::graph::EdgeKind::Arrow)
                && edge.to == target.id
                && sources.iter().any(|source| source.id == edge.from)
        })
}

fn route_convergent_from_subgraph_td(
    sources: &[&Node],
    target: &Node,
    canvas: &mut Canvas,
    style: &StyleChars,
    sg: &crate::graph::Subgraph,
    direction: Direction,
    graph: &Graph,
) {
    let coords = OrientedCoords::new(direction);
    let (target_x, target_y) = get_node_center(target);
    let fanin_owner_id = format!("fanin:{}", target.id);
    let fanin_owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: fanin_owner_id.as_str(),
    };

    // Merge near the bottom border of the subgraph so the exit passes cleanly
    // through the bottom portal and leaves room for the sources above.
    let max_exit_y = sources
        .iter()
        .map(|n| edge_exit_point(n, direction).1)
        .max()
        .unwrap_or(0);
    let bottom_limit = sg.bounds.y + sg.bounds.height.saturating_sub(1);
    let mut merge_y = bottom_limit.saturating_sub(3);
    // Keep the merge bar below the lowest exit row, but never on the border.
    merge_y = merge_y.max(max_exit_y.saturating_add(1));
    merge_y = merge_y.min(bottom_limit.saturating_sub(1));
    let outer_container = smallest_visual_container(graph, sg, target);
    let mut source_positions: Vec<(usize, usize, &Node)> = sources
        .iter()
        .map(|n| {
            let (sx, sy) = get_node_center(n);
            (sx, sy, *n)
        })
        .collect();
    source_positions.sort_by_key(|(x, y, _)| coords.secondary_coord(*x, *y));
    let median_source_x = source_positions
        .get(source_positions.len() / 2)
        .map(|(x, _, _)| *x);
    let can_coalesce_vertical_collector =
        outer_container.is_none() && strict_simple_subgraph_fanin(sources, target, graph);
    let nested_source_center = if matches!(direction, Direction::TD | Direction::TB)
        && outer_container.is_some()
        && !source_positions.is_empty()
    {
        let span_start = source_positions
            .iter()
            .map(|(x, y, _)| coords.secondary_coord(*x, *y))
            .min()
            .unwrap_or(target_x);
        let span_end = source_positions
            .iter()
            .map(|(x, y, _)| coords.secondary_coord(*x, *y))
            .max()
            .unwrap_or(target_x);
        let span_center = (span_start + span_end) / 2;
        Some((span_center.saturating_mul(3) + target_x) / 4)
    } else {
        None
    };
    let preferred_merge_x = match direction {
        Direction::TD | Direction::TB => {
            let inset = if outer_container.is_some() && sg.bounds.width >= 9 {
                3
            } else {
                1
            };
            let min_x = sg.bounds.x.saturating_add(inset);
            let max_x = sg
                .bounds
                .x
                .saturating_add(sg.bounds.width.saturating_sub(inset + 1));
            nested_source_center
                .unwrap_or(target_x)
                .clamp(min_x, max_x.max(min_x))
        }
        _ => sg.bounds.x + sg.bounds.width / 2,
    };
    let preferred_merge_x = if can_coalesce_vertical_collector {
        median_source_x
            .filter(|candidate_x| {
                *candidate_x > sg.bounds.x
                    && *candidate_x < sg.bounds.x + sg.bounds.width.saturating_sub(1)
                    && !is_subgraph_title_cell(graph, *candidate_x, merge_y)
                    && edge_entry_candidates(target, direction)
                        .iter()
                        .any(|(x, y)| {
                            *x == *candidate_x
                                && !hits_foreign_subgraph_border(target, *x, *y, graph)
                        })
            })
            .unwrap_or(preferred_merge_x)
    } else {
        preferred_merge_x
    };
    let (arrow_x, arrow_y) = edge_entry_candidates(target, direction)
        .into_iter()
        .filter(|(candidate_x, candidate_y)| {
            !hits_foreign_subgraph_border(target, *candidate_x, *candidate_y, graph)
        })
        .min_by_key(|(candidate_x, _)| candidate_x.abs_diff(preferred_merge_x))
        .unwrap_or_else(|| adjusted_edge_entry_point(target, direction, graph));
    let merge_x = if let Some(outer) = outer_container {
        let relay_y = outer.bounds.y + outer.bounds.height;
        if relay_y == arrow_y && preferred_merge_x.abs_diff(arrow_x) <= 4 {
            arrow_x
        } else {
            preferred_merge_x
        }
    } else {
        preferred_merge_x
    };

    let target_secondary = coords.secondary_coord(target_x, target_y);
    let (span_start, span_end) = draw_source_lines_to_merge(
        &source_positions,
        merge_x,
        merge_y,
        &coords,
        canvas,
        style,
        direction,
        Some(graph),
        Some(&target.id),
        false,
    );

    let (final_span_start, final_span_end) = if matches!(direction, Direction::TD | Direction::TB) {
        let merge_secondary = coords.secondary_coord(merge_x, merge_y);
        (
            span_start.min(merge_secondary),
            span_end.max(merge_secondary),
        )
    } else {
        (
            span_start.min(target_secondary),
            span_end.max(target_secondary),
        )
    };
    if crate::runtime::current().diagnostics.fan_in {
        eprintln!(
            "fanin merge_x={merge_x} merge_y={merge_y} span=({final_span_start}, {final_span_end}) target_sec={target_secondary} target=({target_x}, {target_y}) arrow=({arrow_x}, {arrow_y})"
        );
    }

    draw_merge_line(
        merge_x,
        merge_y,
        final_span_start,
        final_span_end,
        &coords,
        canvas,
        style,
        Some(fanin_owner),
    );

    // Adjust merge-bar ends for clarity near the subgraph exit.
    match direction {
        Direction::BT => {
            let (sx, sy) = coords.with_secondary(merge_x, merge_y, final_span_start);
            let (ex, ey) = coords.with_secondary(merge_x, merge_y, final_span_end);
            set_route_edge_char(canvas, sx, sy, style.corner_ul, style, Some(fanin_owner));
            set_route_edge_char(canvas, ex, ey, style.corner_ur, style, Some(fanin_owner));
        }
        Direction::TD | Direction::TB => {
            let merge_secondary = coords.secondary_coord(merge_x, merge_y);
            for pos in final_span_start..=final_span_end {
                let (sx, sy) = coords.with_secondary(merge_x, merge_y, pos);
                let ch = if pos == final_span_start {
                    style.corner_ul
                } else if pos == final_span_end {
                    style.corner_ur
                } else if pos == merge_secondary {
                    style.junction_down
                } else {
                    coords.secondary_edge_char(style)
                };
                set_route_edge_char(canvas, sx, sy, ch, style, Some(fanin_owner));
            }
        }
        _ => {}
    }

    let junction_char = match direction {
        Direction::TD | Direction::TB => style.junction_down,
        Direction::LR => style.junction_right,
        Direction::RL => style.junction_left,
        Direction::BT => style.junction_up,
    };
    set_route_char(canvas, merge_x, merge_y, junction_char, Some(fanin_owner));

    // Drop vertically out of the subgraph first, then fan horizontally if needed.
    // If the target arrow row sits directly against the outer border, keep the
    // final turn one row above the arrow so the target still gets a visible shaft.
    let (mut cursor_x, mut cursor_y) = coords.advance(merge_x, merge_y, 1);
    if let Some(outer) = outer_container {
        let relay_y = outer.bounds.y + outer.bounds.height;
        if relay_y >= cursor_y && relay_y <= arrow_y {
            draw_line_primary(
                cursor_x,
                cursor_y,
                cursor_x,
                relay_y,
                &coords,
                canvas,
                style,
                Some(graph),
                Some(fanin_owner),
            );
            cursor_y = relay_y;

            let outer_exit_x = arrow_x;
            if cursor_x != outer_exit_x {
                let start_corner = if outer_exit_x > cursor_x {
                    style.corner_dr
                } else {
                    style.corner_dl
                };
                set_route_edge_char(
                    canvas,
                    cursor_x,
                    cursor_y,
                    start_corner,
                    style,
                    Some(fanin_owner),
                );

                let (hx0, hx1) = if outer_exit_x > cursor_x {
                    (cursor_x + 1, outer_exit_x.saturating_sub(1))
                } else {
                    (outer_exit_x + 1, cursor_x.saturating_sub(1))
                };
                for x in hx0..=hx1 {
                    if is_subgraph_title_cell(graph, x, cursor_y) {
                        continue;
                    }
                    set_route_edge_char(
                        canvas,
                        x,
                        cursor_y,
                        style.edge_h,
                        style,
                        Some(fanin_owner),
                    );
                }

                let end_corner = if outer_exit_x > cursor_x {
                    style.corner_ul
                } else {
                    style.corner_ur
                };
                set_route_edge_char(
                    canvas,
                    outer_exit_x,
                    cursor_y,
                    end_corner,
                    style,
                    Some(fanin_owner),
                );
                cursor_x = outer_exit_x;
            }
        }
    }

    if let Some(next) = step_past_title(
        canvas,
        style,
        graph,
        fanin_owner,
        &coords,
        target,
        (cursor_x, cursor_y),
        arrow_x,
    ) {
        (cursor_x, cursor_y) = next;
    }

    let turn_y = if cursor_x != arrow_x && arrow_y > cursor_y {
        arrow_y
            .saturating_sub(1)
            .min(bottom_limit.saturating_sub(1))
            .max(cursor_y)
    } else {
        arrow_y
    };

    draw_line_primary(
        cursor_x,
        cursor_y,
        cursor_x,
        turn_y,
        &coords,
        canvas,
        style,
        Some(graph),
        Some(fanin_owner),
    );
    cursor_y = turn_y;

    if cursor_x != arrow_x {
        let start_corner = if arrow_x > cursor_x {
            style.corner_ul
        } else {
            style.corner_ur
        };
        set_route_edge_char(
            canvas,
            cursor_x,
            cursor_y,
            start_corner,
            style,
            Some(fanin_owner),
        );

        let (hx0, hx1) = if cursor_x < arrow_x {
            (cursor_x.saturating_add(1), arrow_x.saturating_sub(1))
        } else {
            (arrow_x.saturating_add(1), cursor_x.saturating_sub(1))
        };
        for x in hx0..=hx1 {
            if is_subgraph_title_cell(graph, x, cursor_y) {
                continue;
            }
            set_route_edge_char(canvas, x, cursor_y, style.edge_h, style, Some(fanin_owner));
        }

        let end_corner = if arrow_x > cursor_x {
            style.corner_dr
        } else {
            style.corner_dl
        };
        set_route_edge_char(
            canvas,
            arrow_x,
            cursor_y,
            end_corner,
            style,
            Some(fanin_owner),
        );
        cursor_x = arrow_x;
    }

    if cursor_y < arrow_y {
        draw_line_primary(
            cursor_x,
            cursor_y.saturating_add(1),
            cursor_x,
            arrow_y,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(fanin_owner),
        );
    }

    // Clean up bottom border: keep only the center exit portal.
    let bottom_y = sg.bounds.y + sg.bounds.height.saturating_sub(1);
    if bottom_y < canvas.height {
        let portal_columns: Vec<usize> = source_positions
            .iter()
            .map(|(sx, _, _)| coords.secondary_coord(*sx, bottom_y))
            .collect();
        let border_fill = sample_bottom_border_fill(
            canvas,
            sg,
            bottom_y,
            merge_x,
            &portal_columns,
            coords.secondary_edge_char(style),
        );
        if crate::runtime::current().diagnostics.fan_in {
            eprintln!(
                "cleanup bottom_y={bottom_y} fill='{border_fill}' portals={portal_columns:?}"
            );
        }
        for (sx, sy, _) in &source_positions {
            let sec = coords.secondary_coord(*sx, *sy);
            let (px, py) = coords.with_secondary(merge_x, bottom_y, sec);
            if px != merge_x && px < canvas.width {
                canvas.set(px, py, border_fill);
            }
        }
        if merge_x < canvas.width {
            // Portal "hole" through the border (overwrite, don't merge into a junction).
            set_route_char(canvas, merge_x, bottom_y, style.edge_v, Some(fanin_owner));
        }
    }

    set_route_char(
        canvas,
        arrow_x,
        arrow_y,
        coords.arrow_end(style),
        Some(fanin_owner),
    );
}

/// A TD trunk at `cursor` dropping into the target's frame through its title
/// text is hidden by it (gen_a_116's fan-in into App Servers under "Internal
/// Network"): on the row above that frame, step to the nearest column clear
/// of the title, cross the wall there and turn back to the arrow on the row
/// under the title. Returns where the trunk carries on to the arrow.
#[allow(clippy::too_many_arguments)]
fn step_past_title(
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: &Graph,
    owner: RouteOwner<'_>,
    coords: &OrientedCoords,
    target: &Node,
    (cursor_x, cursor_y): (usize, usize),
    arrow_x: usize,
) -> Option<(usize, usize)> {
    let (entry_x, jog_y, under_y) = title_safe_frame_entry(graph, target, cursor_x)?;
    // A trunk starting inside the frame (a source in it too) has no title
    // to pass.
    if jog_y <= cursor_y {
        return None;
    }
    draw_line_primary(
        cursor_x,
        cursor_y,
        cursor_x,
        jog_y - 1,
        coords,
        canvas,
        style,
        Some(graph),
        Some(owner),
    );
    jog_row(canvas, style, owner, (cursor_x, entry_x), jog_y);
    draw_line_primary(
        entry_x,
        jog_y + 1,
        entry_x,
        under_y - 1,
        coords,
        canvas,
        style,
        Some(graph),
        Some(owner),
    );
    jog_row(canvas, style, owner, (entry_x, arrow_x), under_y);
    Some((arrow_x, under_y + 1))
}

/// A TD trunk's step sideways along row `y`, from the column it came down to
/// the one it carries on down from, with a corner at each end.
pub(super) fn jog_row(
    canvas: &mut Canvas,
    style: &StyleChars,
    owner: RouteOwner<'_>,
    (from_x, to_x): (usize, usize),
    y: usize,
) {
    let (first, last) = if to_x > from_x {
        (style.corner_ul, style.corner_dr)
    } else {
        (style.corner_ur, style.corner_dl)
    };
    set_route_edge_char(canvas, from_x, y, first, style, Some(owner));
    for x in from_x.min(to_x) + 1..from_x.max(to_x) {
        set_route_edge_char(canvas, x, y, style.edge_h, style, Some(owner));
    }
    set_route_edge_char(canvas, to_x, y, last, style, Some(owner));
}

/// Where a TD trunk dropping at `cursor_x` would cross the title text of a
/// frame the target is in: the nearest column clear of that title, the row
/// above the frame to step over on, and the row under the title to turn back
/// on. None when the trunk misses every such title, or the title has room to
/// slide off it.
pub(super) fn title_safe_frame_entry(
    graph: &Graph,
    target: &Node,
    cursor_x: usize,
) -> Option<(usize, usize, usize)> {
    let frame = graph
        .node_subgraph_chain(&target.id)
        .into_iter()
        .filter_map(|sg_id| graph.get_subgraph(sg_id))
        .find(|frame| is_subgraph_title_text_cell(graph, cursor_x, frame.bounds.y + 1))?;
    // A title with room beside the trunk slides off it once drawn, leaving the
    // line straight (gen_a_080, gen_a_160); one padded off a sibling's lane
    // stays put (gen_c_129's Target).
    let gutter = crate::portals::td_sibling_title_gutter(graph, &frame.id);
    let slides = gutter.leading_extra_padding == 0 && gutter.trailing_extra_padding == 0;
    let title_width = crate::graph::title_cells(
        &crate::graph::subgraph_title_text_with_padding_sides(frame.title.as_deref()?, 0, 0),
    )
    .len();
    let right_wall = frame.bounds.x + frame.bounds.width - 1;
    if slides && cursor_x + title_width < right_wall {
        return None;
    }
    let jog_y = frame.bounds.y.checked_sub(1)?;
    let under_y = frame.bounds.y + 2;
    let entry_x = crate::portals::title_safe_portal_x(
        frame.bounds.x,
        frame.bounds.width,
        frame.title.as_deref(),
        cursor_x,
        graph.direction,
        crate::portals::title_margin_for_direction(graph.direction),
        crate::portals::PortalColumnPreference::Nearest,
    );
    Some((entry_x, jog_y, under_y))
}

fn sample_bottom_border_fill(
    canvas: &Canvas,
    sg: &crate::graph::Subgraph,
    bottom_y: usize,
    merge_x: usize,
    portal_columns: &[usize],
    fallback: char,
) -> char {
    let left = sg.bounds.x.saturating_add(1);
    let right = sg
        .bounds
        .x
        .saturating_add(sg.bounds.width.saturating_sub(2));
    for x in left..=right {
        if x == merge_x || portal_columns.contains(&x) {
            continue;
        }
        let ch = canvas.get(x, bottom_y);
        // Another fan-in out of this frame may already cross the border.
        if ch != ' ' && !super::super::topology::char_connects_up(ch) {
            return ch;
        }
    }
    fallback
}

fn route_convergent_from_subgraph_bt(
    sources: &[&Node],
    target: &Node,
    canvas: &mut Canvas,
    style: &StyleChars,
    sg: &crate::graph::Subgraph,
    direction: Direction,
    graph: &Graph,
) {
    if direction != Direction::BT || sources.is_empty() || !sg.bounds.is_valid() {
        return;
    }
    let coords = OrientedCoords::new(direction);
    let (default_arrow_x, arrow_y) = adjusted_edge_entry_point(target, direction, graph);
    let fanin_owner_id = format!("fanin:{}", target.id);
    let fanin_owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: fanin_owner_id.as_str(),
    };

    let top_y = sg.bounds.y;
    let _bottom_y = sg.bounds.y + sg.bounds.height.saturating_sub(1);
    let inside_top = top_y.saturating_add(1);

    // Merge close to sources (just above them in BT terms = smaller y), but
    // not above inside_top so we stay inside the subgraph.
    let min_exit_y = sources
        .iter()
        .map(|n| edge_exit_point(n, direction).1)
        .min()
        .unwrap_or(inside_top.saturating_add(2));

    let mut merge_y = min_exit_y.saturating_sub(2);
    merge_y = merge_y.max(inside_top.saturating_add(1));

    let default_merge_x = preferred_portal_x(
        &sg.bounds,
        sg.title.as_deref(),
        default_arrow_x,
        canvas,
        direction,
        false,
    );

    let mut source_positions: Vec<(usize, usize, &Node)> = sources
        .iter()
        .map(|n| {
            let (sx, sy) = get_node_center(n);
            (sx, sy, *n)
        })
        .collect();
    source_positions.sort_by_key(|(x, y, _)| coords.secondary_coord(*x, *y));

    let merge_x = if strict_simple_subgraph_fanin(sources, target, graph) {
        source_positions
            .get(source_positions.len() / 2)
            .map(|(x, _, _)| *x)
            .filter(|candidate_x| {
                *candidate_x > sg.bounds.x
                    && *candidate_x < sg.bounds.x + sg.bounds.width.saturating_sub(1)
                    && !is_subgraph_title_cell(graph, *candidate_x, top_y)
                    && edge_entry_candidates(target, direction)
                        .iter()
                        .any(|(x, y)| {
                            *x == *candidate_x
                                && !hits_foreign_subgraph_border(target, *x, *y, graph)
                        })
            })
            .unwrap_or(default_merge_x)
    } else {
        default_merge_x
    };
    let arrow_x = if merge_x != default_merge_x {
        edge_entry_candidates(target, direction)
            .into_iter()
            .find(|(x, y)| *x == merge_x && !hits_foreign_subgraph_border(target, *x, *y, graph))
            .map(|(x, _)| x)
            .unwrap_or(default_arrow_x)
    } else {
        default_arrow_x
    };

    let (span_start, span_end) = draw_source_lines_to_merge(
        &source_positions,
        merge_x,
        merge_y,
        &coords,
        canvas,
        style,
        direction,
        Some(graph),
        Some(&target.id),
        false,
    );

    draw_merge_line(
        merge_x,
        merge_y,
        span_start,
        span_end,
        &coords,
        canvas,
        style,
        Some(fanin_owner),
    );

    if span_start < span_end {
        let (sx, sy) = coords.with_secondary(merge_x, merge_y, span_start);
        let (ex, ey) = coords.with_secondary(merge_x, merge_y, span_end);
        // BT corners: edges come from below and turn inward
        set_route_edge_char(canvas, sx, sy, style.corner_dl, style, Some(fanin_owner));
        set_route_edge_char(canvas, ex, ey, style.corner_dr, style, Some(fanin_owner));
    }
    set_route_edge_char(
        canvas,
        merge_x,
        merge_y,
        style.junction_up,
        style,
        Some(fanin_owner),
    );

    // Route toward the target. Mirror the TD variant: draw a straight trunk from
    // the merge point to arrow_y, then bridge horizontally at arrow_y if the
    // columns differ. This keeps all routing chars outside the subgraph border row.
    let (cursor_x, cursor_y) = coords.advance(merge_x, merge_y, 1);

    draw_line_primary(
        cursor_x,
        cursor_y,
        cursor_x,
        arrow_y,
        &coords,
        canvas,
        style,
        Some(graph),
        Some(fanin_owner),
    );

    if cursor_x != arrow_x {
        let start_corner = if arrow_x > cursor_x {
            style.corner_dl
        } else {
            style.corner_dr
        };
        set_route_edge_char(
            canvas,
            cursor_x,
            arrow_y,
            start_corner,
            style,
            Some(fanin_owner),
        );

        let (hx0, hx1) = if cursor_x < arrow_x {
            (cursor_x + 1, arrow_x.saturating_sub(1))
        } else {
            (arrow_x + 1, cursor_x.saturating_sub(1))
        };
        for x in hx0..=hx1 {
            set_route_edge_char(canvas, x, arrow_y, style.edge_h, style, Some(fanin_owner));
        }

        let end_corner = if arrow_x > cursor_x {
            style.corner_ur
        } else {
            style.corner_ul
        };
        set_route_edge_char(
            canvas,
            arrow_x,
            arrow_y,
            end_corner,
            style,
            Some(fanin_owner),
        );
    }

    // Clean up the top border: keep only the merged exit portal and restore any
    // other positions that would otherwise clutter the border.
    if top_y < canvas.height {
        let border_fill = coords.secondary_edge_char(style);

        // Clean up the top border at each source x-position
        for (sx, sy, _) in &source_positions {
            let sec = coords.secondary_coord(*sx, *sy);
            let (px, py) = coords.with_secondary(merge_x, top_y, sec);
            if px != merge_x && px < canvas.width && py < canvas.height {
                let existing = canvas.get(px, py);
                if !is_textual(existing) {
                    canvas.set(px, py, border_fill);
                }
            }
        }

        // Stamp the merge portal on the border as a PortalOpening so the
        // stabilize passes (which only process EdgeSegment / CycleEdge / Junction)
        // leave it alone.  A plain │ through the subgraph border is correct here;
        // the degree-mismatch stabilizer would otherwise upgrade it to ┼.
        if merge_x < canvas.width && !is_textual(canvas.get(merge_x, top_y)) {
            stamp_portal_opening(
                canvas,
                merge_x,
                top_y,
                style,
                PortalAxis::Vertical,
                "merge_portal",
                ROUTE_Z_INDEX,
            );
        }
    }

    set_route_char(
        canvas,
        arrow_x,
        arrow_y,
        coords.arrow_end(style),
        Some(fanin_owner),
    );
}

fn route_convergent_from_subgraph_lr(
    sources: &[&Node],
    target: &Node,
    canvas: &mut Canvas,
    style: &StyleChars,
    sg: &crate::graph::Subgraph,
    direction: Direction,
    graph: &Graph,
) -> bool {
    if !matches!(direction, Direction::LR | Direction::RL)
        || sources.is_empty()
        || !sg.bounds.is_valid()
    {
        return false;
    }

    let coords = OrientedCoords::new(direction);
    let (arrow_x, arrow_y) = adjusted_edge_entry_point(target, direction, graph);
    let fanin_owner_id = format!("fanin:{}", target.id);
    let fanin_owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: fanin_owner_id.as_str(),
    };

    let mut source_positions: Vec<(usize, usize, &Node)> = sources
        .iter()
        .map(|n| {
            let (sx, sy) = get_node_center(n);
            (sx, sy, *n)
        })
        .collect();
    source_positions.sort_by_key(|(x, y, _)| coords.secondary_coord(*x, *y));

    let span_start = source_positions
        .iter()
        .map(|(x, y, _)| coords.secondary_coord(*x, *y))
        .min()
        .unwrap_or(arrow_y);
    let span_end = source_positions
        .iter()
        .map(|(x, y, _)| coords.secondary_coord(*x, *y))
        .max()
        .unwrap_or(arrow_y);
    let left_border_x = sg.bounds.x;
    let right_border_x = sg.bounds.x + sg.bounds.width.saturating_sub(1);
    let min_inside_x = left_border_x.saturating_add(1);
    let max_inside_x = right_border_x.saturating_sub(1);
    if max_inside_x <= min_inside_x {
        return false;
    }

    let merge_x = match direction {
        Direction::LR => {
            let max_exit_x = sources
                .iter()
                .map(|n| edge_exit_point(n, direction).0)
                .max()
                .unwrap_or(min_inside_x);
            if strict_simple_subgraph_fanin(sources, target, graph)
                && max_inside_x >= max_exit_x.saturating_add(2)
            {
                max_inside_x
            } else {
                right_border_x
                    .saturating_sub(2)
                    .max(max_exit_x.saturating_add(1))
                    .clamp(min_inside_x, max_inside_x)
            }
        }
        Direction::RL => {
            let min_exit_x = sources
                .iter()
                .map(|n| edge_exit_point(n, direction).0)
                .min()
                .unwrap_or(max_inside_x);
            if strict_simple_subgraph_fanin(sources, target, graph)
                && min_exit_x >= min_inside_x.saturating_add(1)
            {
                min_inside_x
            } else {
                left_border_x
                    .saturating_add(2)
                    .min(min_exit_x.saturating_sub(1))
                    .clamp(min_inside_x, max_inside_x)
            }
        }
        _ => unreachable!(),
    };
    let border_x = match direction {
        Direction::LR => right_border_x,
        Direction::RL => left_border_x,
        _ => unreachable!(),
    };
    let outside_x = match direction {
        Direction::LR => border_x.saturating_add(1),
        Direction::RL => border_x.saturating_sub(1),
        _ => unreachable!(),
    };
    let centered_merge_y = ((span_start + span_end) / 2).clamp(
        sg.bounds.y.saturating_add(1),
        sg.bounds.y + sg.bounds.height.saturating_sub(2),
    );
    let merge_y = if centered_merge_y != arrow_y && outside_x != arrow_x {
        centered_merge_y
    } else {
        arrow_y.clamp(
            sg.bounds.y.saturating_add(1),
            sg.bounds.y + sg.bounds.height.saturating_sub(2),
        )
    };
    if crate::runtime::current().diagnostics.fan_in {
        eprintln!(
            "horizontal fanin target={} dir={:?} sg={} span=({}, {}) arrow=({}, {}) border_x={} outside_x={} centered_merge_y={} merge_y={}",
            target.id,
            direction,
            sg.id,
            span_start,
            span_end,
            arrow_x,
            arrow_y,
            border_x,
            outside_x,
            centered_merge_y,
            merge_y
        );
    }

    let (actual_span_start, actual_span_end) = draw_source_lines_to_merge(
        &source_positions,
        merge_x,
        merge_y,
        &coords,
        canvas,
        style,
        direction,
        Some(graph),
        Some(&target.id),
        false,
    );

    draw_merge_line(
        merge_x,
        merge_y,
        actual_span_start,
        actual_span_end,
        &coords,
        canvas,
        style,
        Some(fanin_owner),
    );
    set_route_edge_char(
        canvas,
        merge_x,
        merge_y,
        match direction {
            Direction::LR => style.junction_right,
            Direction::RL => style.junction_left,
            _ => unreachable!(),
        },
        style,
        Some(fanin_owner),
    );

    if merge_y == arrow_y {
        draw_line_primary(
            outside_x,
            merge_y,
            arrow_x,
            arrow_y,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(fanin_owner),
        );
    } else {
        let turn_x = match direction {
            Direction::LR => {
                let lower = outside_x;
                let upper = arrow_x.saturating_sub(1);
                if lower > upper {
                    return false;
                }
                arrow_x.saturating_sub(2).clamp(lower, upper)
            }
            Direction::RL => {
                let lower = arrow_x.saturating_add(1);
                let upper = outside_x;
                if lower > upper {
                    return false;
                }
                arrow_x.saturating_add(2).clamp(lower, upper)
            }
            _ => unreachable!(),
        };
        let going_before = merge_y > arrow_y;
        if crate::runtime::current().diagnostics.fan_in {
            eprintln!(
                "horizontal fanin jog target={} dir={:?} turn_x={} going_before={}",
                target.id, direction, turn_x, going_before
            );
        }

        draw_line_primary(
            outside_x,
            merge_y,
            turn_x,
            merge_y,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(fanin_owner),
        );
        set_route_edge_char(
            canvas,
            turn_x,
            merge_y,
            coords.corner_start_to_secondary(going_before, style),
            style,
            Some(fanin_owner),
        );
        draw_line_secondary(
            turn_x,
            merge_y,
            turn_x,
            arrow_y,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(fanin_owner),
        );
        set_route_edge_char(
            canvas,
            turn_x,
            arrow_y,
            coords.corner_secondary_to_end(going_before, style),
            style,
            Some(fanin_owner),
        );

        let (seg_start_x, seg_start_y) = coords.advance(turn_x, arrow_y, 1);
        draw_line_primary(
            seg_start_x,
            seg_start_y,
            arrow_x,
            arrow_y,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(fanin_owner),
        );
    }

    for (_, sy, _) in &source_positions {
        let border_y = coords.secondary_coord(border_x, *sy);
        if border_y != merge_y && border_x < canvas.width && border_y < canvas.height {
            let existing = canvas.get(border_x, border_y);
            if !is_textual(existing) {
                canvas.set(border_x, border_y, style.v);
            }
        }
    }

    if border_x < canvas.width
        && merge_y < canvas.height
        && !is_textual(canvas.get(border_x, merge_y))
    {
        stamp_portal_opening(
            canvas,
            border_x,
            merge_y,
            style,
            PortalAxis::Horizontal,
            "merge_portal",
            ROUTE_Z_INDEX,
        );
    }

    set_route_char(
        canvas,
        arrow_x,
        arrow_y,
        coords.arrow_end(style),
        Some(fanin_owner),
    );

    true
}

/// Leave each even-width source that takes its port on the left centre
/// column (see [`fan_in_source_uses_left_port`]) there, so its line runs straight
/// into a trunk on that column (`┼`) instead of landing beside it (`┬┴`),
/// and its exit stays on the column its own entry uses.
fn align_sources_to_trunk<'a>(
    source_positions: &[(usize, usize, &'a Node)],
    trunk_x: usize,
    direction: Direction,
    graph: Option<&Graph>,
) -> Vec<(usize, usize, &'a Node)> {
    let vertical = matches!(direction, Direction::TD | Direction::TB | Direction::BT);
    source_positions
        .iter()
        .map(|&(x, y, source)| {
            let left_port = vertical
                && x == source.center_x()
                && trunk_x != x
                && graph.is_some_and(|graph| {
                    fan_in_source_uses_left_port(source, graph)
                        // A root moves only onto its trunk; a fanned-out box
                        // follows the branch its entry takes.
                        && (trunk_x + 1 == x
                            || graph
                                .edges
                                .iter()
                                .any(|e| !e.is_back_edge && e.to == source.id))
                });
            (if left_port { x - 1 } else { x }, y, source)
        })
        .collect()
}

// Helper: Draw lines from sources to merge point (on primary axis)
#[allow(clippy::too_many_arguments)]
fn draw_source_lines_to_merge(
    source_positions: &[(usize, usize, &Node)],
    merge_x: usize,
    merge_y: usize,
    coords: &OrientedCoords,
    canvas: &mut Canvas,
    style: &StyleChars,
    direction: Direction,
    graph: Option<&Graph>,
    target_id: Option<&str>,
    relaxed_lanes: bool,
) -> (usize, usize) {
    let aligned = align_sources_to_trunk(source_positions, merge_x, direction, graph);
    let source_positions = aligned.as_slice();
    let drops: Vec<usize> = source_positions
        .iter()
        .map(|&(src_x, _, source)| {
            graph
                .zip(target_id)
                .filter(|_| {
                    matches!(direction, Direction::TD | Direction::TB)
                        || relaxed_lanes && direction == Direction::BT
                })
                .and_then(|(graph, target_id)| {
                    fan_in_lane_x(
                        graph,
                        canvas,
                        style,
                        source,
                        target_id,
                        src_x,
                        (merge_x, merge_y),
                        relaxed_lanes,
                    )
                })
                .unwrap_or(src_x)
        })
        .collect();
    // Pre-compute span bounds BEFORE drawing so corner characters are correct
    let mut span_start = usize::MAX;
    let mut span_end = 0;
    for (&(_, src_y, _), &src_x) in source_positions.iter().zip(&drops) {
        let src_secondary = coords.secondary_coord(src_x, src_y);
        span_start = span_start.min(src_secondary);
        span_end = span_end.max(src_secondary);
    }
    if matches!(direction, Direction::TD | Direction::TB) {
        let merge_secondary = coords.secondary_coord(merge_x, merge_y);
        span_start = span_start.min(merge_secondary);
        span_end = span_end.max(merge_secondary);
    }

    for (&(src_x, src_y, source), &drop_x) in source_positions.iter().zip(&drops) {
        let owner_id = graph
            .zip(target_id)
            .map(|(graph, target_id)| edge_route_owner_id(graph, &source.id, target_id));
        let owner = owner_id.as_deref().map(|owner_id| RouteOwner {
            kind: CellOwnerKind::EdgeSegment,
            id: owner_id,
        });
        let (edge_x, mut edge_y) = edge_exit_point(source, direction);
        let src_secondary = coords.secondary_coord(drop_x, src_y);
        if drop_x != src_x {
            // The jog to the clear drop column, on the row below the source.
            let (low, high) = (drop_x.min(src_x), drop_x.max(src_x));
            for x in low.saturating_add(1)..high {
                set_route_edge_char(canvas, x, edge_y, style.edge_h, style, owner);
            }
            let (source_end, drop_end) = match (direction == Direction::BT, drop_x > src_x) {
                (false, true) => (style.corner_ul, style.corner_dr),
                (false, false) => (style.corner_ur, style.corner_dl),
                (true, true) => (style.corner_dl, style.corner_ur),
                (true, false) => (style.corner_dr, style.corner_ul),
            };
            set_route_edge_char(canvas, src_x, edge_y, source_end, style, owner);
            set_route_edge_char(canvas, drop_x, edge_y, drop_end, style, owner);
            if direction != Direction::BT {
                edge_y += 1;
            }
        }
        let jogged = drop_x != src_x;
        let (source_x, src_x) = (src_x, drop_x);

        // Line from source to just before merge point (along primary axis)
        let (merge_col_x, merge_col_y) = coords.with_secondary(merge_x, merge_y, src_secondary);

        // Draw line from source to merge span (direction-aware)
        match direction {
            Direction::TD | Direction::TB => {
                // Vertical layout: draw vertical line between source edge and merge row
                let (start, end) = if edge_y <= merge_col_y {
                    (edge_y, merge_col_y)
                } else {
                    (merge_col_y, edge_y)
                };
                for y in start..end {
                    set_route_edge_char(canvas, src_x, y, style.edge_v, style, owner);
                }
            }
            Direction::LR => {
                // LR layout: draw horizontal line between source right edge and merge column
                let (start, end) = if edge_x <= merge_col_x {
                    (edge_x, merge_col_x)
                } else {
                    (merge_col_x, edge_x)
                };
                for x in start..end {
                    set_route_edge_char(canvas, x, src_y, style.edge_h, style, owner);
                }
            }
            Direction::RL => {
                // RL layout: draw horizontal line between source left edge and merge column
                let (start, end) = if merge_col_x <= edge_x {
                    (merge_col_x + 1, edge_x + 1)
                } else {
                    (edge_x + 1, merge_col_x + 1)
                };
                for x in start..end {
                    set_route_edge_char(canvas, x, src_y, style.edge_h, style, owner);
                }
            }
            Direction::BT => {
                // BT layout: draw vertical line from BELOW merge row down to source top border.
                // Start at merge_col_y + 1 to avoid overlapping the corner position.
                let node_border_y = if jogged { edge_y } else { source.y };
                let line_start = merge_col_y.saturating_add(1);
                if line_start < node_border_y {
                    for y in line_start..node_border_y {
                        set_route_edge_char(canvas, src_x, y, style.edge_v, style, owner);
                    }
                }
            }
        }

        // Mark the exit point on the box border to make the junction explicit.
        match direction {
            Direction::LR => {
                let border_x = edge_x.saturating_sub(1);
                if border_x < canvas.width && src_y < canvas.height {
                    set_route_edge_char(
                        canvas,
                        border_x,
                        src_y,
                        style.junction_right,
                        style,
                        owner,
                    );
                }
            }
            Direction::RL => {
                let border_x = edge_x.saturating_add(1);
                if border_x < canvas.width && src_y < canvas.height {
                    set_route_edge_char(canvas, border_x, src_y, style.junction_left, style, owner);
                }
            }
            Direction::TD | Direction::TB => {
                // Vertical layouts: place junction on bottom border of source
                let border_y = source.y + source.height.saturating_sub(1);
                if source_x < canvas.width && border_y < canvas.height {
                    set_route_edge_char(
                        canvas,
                        source_x,
                        border_y,
                        style.junction_down,
                        style,
                        owner,
                    );
                }
            }
            Direction::BT => {
                // BT: place junction on top border of source
                let border_y = source.y;
                if source_x < canvas.width && border_y < canvas.height {
                    set_route_edge_char(
                        canvas,
                        source_x,
                        border_y,
                        style.junction_up,
                        style,
                        owner,
                    );
                }
            }
        }

        // Corner where source line meets merge span
        let corner_char = get_convergence_corner(
            src_secondary,
            span_start,
            span_end,
            direction,
            style,
            coords,
        );
        set_route_edge_char(canvas, merge_col_x, merge_col_y, corner_char, style, owner);
    }

    (span_start, span_end)
}

/// A fan-in source runs straight along its centre column to the merge row.
/// When that run meets a box it reads as an edge into that box; take the
/// nearest clear column instead (clear of the boxes' sides first), reached
/// by a jog beside the source. The column meets no box, frame title or other
/// vertical line, and the jog meets no box or frame side.
///
/// Every fan-in takes such a lane in TD past a box inside a frame the source
/// is not in, when no other edge leaves the source on that side. A plain
/// fan-in (`relaxed`) with no labelled line, in a graph without frames, takes
/// one in BT as well, past any box, beside one other edge out of the source,
/// from a source whose rim draws the exit junction (a diamond's or circle's
/// does not, and a jog beside it loses its arm toward the source). Such a
/// lane keeps a column clear of other lines, crosses no frame border, which
/// keeps no pierce a route did not plan, and its run along the merge row
/// back to the merge column meets no frame side or other line.
#[allow(clippy::too_many_arguments)]
fn fan_in_lane_x(
    graph: &Graph,
    canvas: &Canvas,
    style: &StyleChars,
    source: &Node,
    target_id: &str,
    src_x: usize,
    (merge_x, merge_y): (usize, usize),
    relaxed: bool,
) -> Option<usize> {
    // Inside frames a lane moves the merge onto rows other routes and
    // portals were placed against (gen_c_145); a label into the target is
    // drawn later, over the merge row (gen_d_023, gen_a_013).
    let relaxed = relaxed
        && graph.subgraphs.is_empty()
        && !graph
            .edges
            .iter()
            .any(|edge| edge.to == target_id && edge.label.is_some());
    if graph.direction == Direction::BT && !relaxed {
        return None;
    }
    if relaxed
        && !matches!(
            source.shape,
            NodeShape::Rectangle
                | NodeShape::Rounded
                | NodeShape::Stadium
                | NodeShape::Hexagon
                | NodeShape::Subroutine
                | NodeShape::Database
        )
    {
        return None;
    }
    let (_, jog_y) = edge_exit_point(source, graph.direction);
    let (top, bottom) = if graph.direction == Direction::BT {
        (merge_y.checked_add(1).filter(|&row| row < jog_y)?, jog_y)
    } else {
        (jog_y, merge_y.checked_sub(1).filter(|&row| row > jog_y)?)
    };
    let meets_box = |x0: usize, x1: usize, y0: usize, y1: usize, margin: usize| {
        graph.nodes.iter().any(|node| {
            node.id != source.id
                && node.id != target_id
                && x1 + margin >= node.x
                && x0 < node.x + node.width + margin
                && node.y <= y1
                && y0 < node.y + node.height
        })
    };
    let blocked = graph.nodes.iter().any(|node| {
        node.id != source.id
            && node.id != target_id
            && src_x >= node.x
            && src_x < node.x + node.width
            && node.y <= bottom
            && top < node.y + node.height
            && (relaxed
                || graph
                    .get_node_subgraph(&node.id)
                    .is_some_and(|frame| !graph.is_node_in_subgraph_tree(&source.id, frame)))
    });
    // Other edges leaving the source on the same side: a jog cuts across a
    // fan's trunk, and in strict mode across any of them.
    let other_exits = graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge && edge.from == source.id && edge.to != target_id)
        .count();
    if !blocked || other_exits > usize::from(relaxed) {
        return None;
    }
    let crosses_frame_side = |low: usize, high: usize, row: usize| {
        graph.subgraphs.iter().any(|frame| {
            let bounds = &frame.bounds;
            let right = (bounds.x + bounds.width).saturating_sub(1);
            bounds.is_valid()
                && bounds.y <= row
                && row < bounds.y + bounds.height
                && ((low < bounds.x && bounds.x <= high) || (low <= right && right < high))
        })
    };
    let crosses_frame_border = |x: usize| {
        graph.subgraphs.iter().any(|frame| {
            let bounds = &frame.bounds;
            let rows = [bounds.y, (bounds.y + bounds.height).saturating_sub(1)];
            bounds.is_valid()
                && bounds.x <= x
                && x < bounds.x + bounds.width
                && rows.iter().any(|row| (top..=bottom).contains(row))
        })
    };
    let edge_free = |x: usize, y: usize| {
        canvas
            .get_meta(x, y)
            .is_none_or(|meta| meta.owner_kind != CellOwnerKind::EdgeSegment)
    };
    let on_frame_border = |x: usize, y: usize| {
        graph.subgraphs.iter().any(|frame| {
            let bounds = &frame.bounds;
            bounds.is_valid()
                && bounds.x <= x
                && x < bounds.x + bounds.width
                && (y == bounds.y || y + 1 == bounds.y + bounds.height)
        })
    };
    // The jog meets no line, nor the end of one just beside its row, which
    // it would join (gen_d_150).
    let jog_clear = |x: usize| {
        let (low, high) = (x.min(src_x), x.max(src_x));
        (low..=high).filter(|&cx| cx != src_x).all(|cx| {
            edge_free(cx, jog_y)
                && [jog_y.wrapping_sub(1), jog_y + 1]
                    .iter()
                    .all(|&y| !super::super::canvas::is_vertical(canvas.get(cx, y), style))
        })
    };
    // Back edges run from their ends' sides to the right gutter along the
    // ends' rows, drawn after the fan-ins; a lane right of such an end would
    // be crossed there, beside an arrowhead (gen_d_154).
    let back_edge_rows: Vec<(usize, usize)> = graph
        .edges
        .iter()
        .filter(|edge| edge.is_back_edge && edge.from != edge.to)
        .flat_map(|edge| [&edge.from, &edge.to])
        .filter_map(|id| graph.nodes.iter().find(|node| &node.id == id))
        .map(|node| (node.x + node.width, node.center_y()))
        .collect();
    let meets_back_edge_run = |x: usize| {
        back_edge_rows
            .iter()
            .any(|&(right, row)| right <= x && (top..=bottom).contains(&row))
    };
    let relaxed_clear = |x: usize| {
        let (low, high) = (x.min(merge_x), x.max(merge_x));
        jog_clear(x)
            && !meets_back_edge_run(x)
            && !crosses_frame_border(x)
            && !(low..=high).any(|cx| on_frame_border(cx, merge_y))
            && (top..=bottom).all(|y| edge_free(x, y))
            && (top..=bottom).all(|y| {
                [x.saturating_sub(1), x + 1]
                    .iter()
                    .all(|&cx| !super::super::canvas::is_vertical(canvas.get(cx, y), style))
            })
            && !crosses_frame_side(low, high, merge_y)
            && (low..=high)
                .filter(|&cx| cx != merge_x)
                .all(|cx| edge_free(cx, merge_y))
    };
    let clear = |x: usize, margin: usize| {
        let (low, high) = (x.min(src_x), x.max(src_x));
        !meets_box(x, x, top, bottom, margin)
            && !meets_box(low, high, jog_y, jog_y, 0)
            && !crosses_frame_side(low, high, jog_y)
            && (top..=bottom).all(|y| {
                (x.saturating_sub(1)..=x + 1).all(|cx| !is_subgraph_title_cell(graph, cx, y))
            })
            && (top..=bottom).all(|y| !super::super::canvas::is_vertical(canvas.get(x, y), style))
            && (!relaxed || relaxed_clear(x))
    };
    let window =
        src_x.saturating_sub(40)..=src_x.saturating_add(40).min(canvas.width.saturating_sub(1));
    let mut lanes: Vec<usize> = window.filter(|&x| x != src_x).collect();
    lanes.sort_by_key(|&x| (x.abs_diff(src_x), x));
    [1, 0]
        .into_iter()
        .find_map(|margin| lanes.iter().copied().find(|&x| clear(x, margin)))
}

/// Get the appropriate corner character for convergence based on position on span.
fn get_convergence_corner(
    src_secondary: usize,
    span_start: usize,
    span_end: usize,
    direction: Direction,
    style: &StyleChars,
    coords: &OrientedCoords,
) -> char {
    if src_secondary == span_start {
        // Topmost/leftmost position on span - edge from source turns right
        match direction {
            Direction::TD | Direction::TB => style.corner_ul, // └ - from above, turns right
            Direction::LR => style.corner_dr,                 // ┐ - from left, turns down
            Direction::RL => style.corner_dl,                 // ┌ - from right, turns down
            Direction::BT => style.corner_dl,                 // ┌ - from below, turns right
        }
    } else if src_secondary == span_end {
        // Bottommost/rightmost position on span - edge from source turns left
        match direction {
            Direction::TD | Direction::TB => style.corner_ur, // ┘ - from above, turns left
            Direction::LR => style.corner_ur,                 // ┘ - from left, turns up
            Direction::RL => style.corner_ul,                 // └ - from right, turns up
            Direction::BT => style.corner_dr,                 // ┐ - from below, turns left
        }
    } else {
        // Middle sources get junction
        coords.junction_merge(style)
    }
}

// Helper: Draw the horizontal merge line
#[allow(clippy::too_many_arguments)]
fn draw_merge_line(
    merge_x: usize,
    merge_y: usize,
    span_start: usize,
    span_end: usize,
    coords: &OrientedCoords,
    canvas: &mut Canvas,
    style: &StyleChars,
    owner: Option<RouteOwner<'_>>,
) {
    for pos in span_start..=span_end {
        // Skip end positions - corners will be drawn there by the caller.
        // The middle positions use set_edge_char to allow overlap resolution
        // when multiple groups share the same merge row.
        if pos == span_start || pos == span_end {
            continue;
        }
        let (span_x, span_y) = coords.with_secondary(merge_x, merge_y, pos);
        set_route_edge_char(
            canvas,
            span_x,
            span_y,
            coords.secondary_edge_char(style),
            style,
            owner,
        );
    }
}

/// Route edges from multiple sources to a single target (convergence)
#[allow(clippy::too_many_arguments)]
pub fn route_convergent_edges(
    from_nodes: &[&Node],
    to: &Node,
    canvas: &mut Canvas,
    style: &StyleChars,
    spacing: &SpacingConfig,
    direction: Direction,
    graph: &Graph,
    merge_lane: Option<(usize, usize)>,
) {
    if from_nodes.is_empty() || !canvas.is_visible(to) {
        return;
    }

    let coords = OrientedCoords::new(direction);
    let fanin_owner_id = format!("fanin:{}", to.id);
    let fanin_owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: fanin_owner_id.as_str(),
    };
    let debug = crate::runtime::current().diagnostics.timing;
    let branch_rejoin_baseline =
        is_vertical_branch_rejoin_target(graph, &to.id).then(|| canvas.clone());

    // Filter to visible sources
    let visible_sources: Vec<&Node> = from_nodes
        .iter()
        .filter(|n| canvas.is_visible(n))
        .copied()
        .collect();
    if visible_sources.is_empty() {
        return;
    }

    // Get target position
    let (target_x, target_y) = get_node_center(to);
    let (arrow_x, arrow_y) = adjusted_edge_entry_point(to, direction, graph);
    if debug {
        let ids: Vec<&str> = visible_sources.iter().map(|n| n.id.as_str()).collect();
        eprintln!(
            "render: convergent -> {} from {:?} merge_base=({}, {})",
            to.id, ids, arrow_x, arrow_y
        );
    }

    // Layout repair can place a terminal target on the opposite physical
    // side from the requested graph orientation (notably an RL target below
    // a sibling subgraph).  Route that bounded case using the physical axis
    // rather than feeding an invalid primary interval to the RL lowerer.
    let source_primary_min = visible_sources
        .iter()
        .map(|source| {
            coords.primary_coord(
                edge_exit_point(source, direction).0,
                edge_exit_point(source, direction).1,
            )
        })
        .min()
        .unwrap_or(0);
    let source_primary_max = visible_sources
        .iter()
        .map(|source| {
            coords.primary_coord(
                edge_exit_point(source, direction).0,
                edge_exit_point(source, direction).1,
            )
        })
        .max()
        .unwrap_or(0);
    let physical_orientation_mismatch = match direction {
        Direction::LR => arrow_x < source_primary_min,
        Direction::RL => arrow_x > source_primary_max,
        _ => false,
    };
    if physical_orientation_mismatch {
        let physical_direction = match direction {
            Direction::LR => Direction::RL,
            Direction::RL => Direction::LR,
            _ => unreachable!(),
        };
        route_convergent_edges(
            from_nodes,
            to,
            canvas,
            style,
            spacing,
            physical_direction,
            graph,
            merge_lane,
        );
        return;
    }

    // Merge inside the source subgraph before exiting when all parents share one.
    if matches!(direction, Direction::TD | Direction::TB) {
        if let Some(source_sg_id) = visible_sources
            .first()
            .and_then(|n| graph.get_node_subgraph(&n.id))
        {
            let target_sg = graph.get_node_subgraph(&to.id);
            let all_sources_same = visible_sources
                .iter()
                .all(|n| graph.get_node_subgraph(&n.id) == Some(source_sg_id));

            if all_sources_same && target_sg != Some(source_sg_id) {
                if let Some(sg) = graph.get_subgraph(source_sg_id) {
                    if sg.bounds.is_valid() {
                        route_convergent_from_subgraph_td(
                            &visible_sources,
                            to,
                            canvas,
                            style,
                            sg,
                            direction,
                            graph,
                        );
                        return;
                    }
                }
            }
        }
    } else if matches!(direction, Direction::LR | Direction::RL) {
        if let Some(source_sg_id) = visible_sources
            .first()
            .and_then(|n| graph.get_node_subgraph(&n.id))
        {
            let target_sg = graph.get_node_subgraph(&to.id);
            let all_sources_same = visible_sources
                .iter()
                .all(|n| graph.get_node_subgraph(&n.id) == Some(source_sg_id));

            if all_sources_same && target_sg != Some(source_sg_id) {
                if let Some(sg) = graph.get_subgraph(source_sg_id) {
                    if sg.bounds.is_valid()
                        && route_convergent_from_subgraph_lr(
                            &visible_sources,
                            to,
                            canvas,
                            style,
                            sg,
                            direction,
                            graph,
                        )
                    {
                        return;
                    }
                }
            }
        }
    } else if direction == Direction::BT {
        if let Some(source_sg_id) = visible_sources
            .first()
            .and_then(|n| graph.get_node_subgraph(&n.id))
        {
            let target_sg = graph.get_node_subgraph(&to.id);
            let all_sources_same = visible_sources
                .iter()
                .all(|n| graph.get_node_subgraph(&n.id) == Some(source_sg_id));

            if all_sources_same && target_sg != Some(source_sg_id) {
                if let Some(sg) = graph.get_subgraph(source_sg_id) {
                    if sg.bounds.is_valid() {
                        route_convergent_from_subgraph_bt(
                            &visible_sources,
                            to,
                            canvas,
                            style,
                            sg,
                            direction,
                            graph,
                        );
                        return;
                    }
                }
            }
        }
    }

    // Calculate merge point. For vertical layouts, merge just after the sources to
    // keep junctions near the fan-in; for horizontal layouts, merge near sources but
    // before the target arrow.
    let merge_distance = match direction {
        Direction::LR | Direction::RL => spacing.stem_length_horizontal,
        _ => spacing.stem_length_vertical,
    };
    let (mut merge_x, mut merge_y) = coords.retreat(arrow_x, arrow_y, merge_distance);

    // Track exits along primary axis.
    let mut min_exit = usize::MAX;
    let mut max_exit = 0usize;
    for src in &visible_sources {
        let (ex, ey) = edge_exit_point(src, direction);
        let primary = coords.primary_coord(ex, ey);
        min_exit = min_exit.min(primary);
        max_exit = max_exit.max(primary);
    }
    let mut merge_primary = coords.primary_coord(merge_x, merge_y);
    let arrow_primary = coords.primary_coord(arrow_x, arrow_y);
    // The first column outside the frames sources leave without the target,
    // a blank column past their border: right of them in LR, left in RL.
    // The last column before the frames the target is in without the sources,
    // a blank column short of their border: left of them in LR, right in RL.
    let entered_frames_edge = |rightward: bool| -> Option<usize> {
        let frames = graph
            .subgraphs
            .iter()
            .filter(|sg| sg.bounds.is_valid())
            .filter(|sg| graph.is_node_in_subgraph_tree(&to.id, &sg.id))
            .filter(|sg| {
                !visible_sources
                    .iter()
                    .any(|source| graph.is_node_in_subgraph_tree(&source.id, &sg.id))
            });
        if rightward {
            frames.filter_map(|sg| sg.bounds.x.checked_sub(2)).min()
        } else {
            frames.map(|sg| sg.bounds.x + sg.bounds.width + 1).max()
        }
    };
    let left_frames_edge = |rightward: bool| -> Option<usize> {
        let frames = graph
            .subgraphs
            .iter()
            .filter(|sg| sg.bounds.is_valid())
            .filter(|sg| !graph.is_node_in_subgraph_tree(&to.id, &sg.id))
            .filter(|sg| {
                visible_sources
                    .iter()
                    .any(|source| graph.is_node_in_subgraph_tree(&source.id, &sg.id))
            });
        if rightward {
            frames.map(|sg| sg.bounds.x + sg.bounds.width + 1).max()
        } else {
            frames.filter_map(|sg| sg.bounds.x.checked_sub(2)).min()
        }
    };

    match direction {
        Direction::LR => {
            // Merge just to the right of the furthest source, but before the target arrow.
            let min_merge = max_exit.saturating_add(1);
            // Prefer leaving two dashes before the arrow, but fall back to one dash
            // if space is tight relative to the sources.
            let max_merge_two = arrow_primary.saturating_sub(3);
            let max_merge_one = arrow_primary.saturating_sub(2);
            let max_merge = if max_merge_two >= min_merge {
                max_merge_two
            } else {
                max_merge_one
            };
            if min_merge > max_merge {
                merge_primary = max_merge;
            } else {
                merge_primary = merge_primary.max(min_merge);
                merge_primary = merge_primary.min(max_merge);
            }
            // Sources leaving frames the target is not in: merge a blank
            // column outside the outermost of them, not on its border.
            // It must also stay a blank column short of the frames the target
            // is in; where both do not fit, the merge stays put.
            if let Some(clear) = left_frames_edge(true)
                .filter(|&clear| clear <= max_merge)
                .filter(|&clear| entered_frames_edge(true).is_none_or(|limit| clear <= limit))
            {
                merge_primary = merge_primary.max(clear);
            }
        }
        Direction::RL => {
            // Merge just to the left of the closest source, but after the target arrow.
            let max_merge = min_exit.saturating_sub(1);
            // Prefer leaving two dashes before the arrow, but fall back to one dash
            // if space is tight relative to the sources.
            let min_merge_two = arrow_primary.saturating_add(3);
            let min_merge_one = arrow_primary.saturating_add(2);
            let min_merge = if max_merge >= min_merge_two {
                min_merge_two
            } else {
                min_merge_one
            };
            if max_merge < min_merge {
                merge_primary = max_merge;
            } else {
                merge_primary = merge_primary.min(max_merge);
                merge_primary = merge_primary.max(min_merge);
            }
            if let Some(clear) = left_frames_edge(false)
                .filter(|&clear| clear >= min_merge)
                .filter(|&clear| entered_frames_edge(false).is_none_or(|limit| clear >= limit))
            {
                merge_primary = merge_primary.min(clear);
            }
        }
        Direction::TD | Direction::TB => {
            // Merge just below the lowest source (leave a full row for stems), but above the target.
            let min_merge = max_exit.saturating_add(2);
            let limit = arrow_primary.saturating_sub(1);
            merge_primary = min_merge.min(limit);
            // A target inside a frame none of the sources share is entered
            // through one portal; merge outside that frame, on the row above
            // its top border, instead of on the border or title band.
            let entered_top = graph
                .node_subgraph_chain(&to.id)
                .into_iter()
                .filter(|sg_id| {
                    !visible_sources
                        .iter()
                        .any(|source| graph.is_node_in_subgraph_tree(&source.id, sg_id))
                })
                .filter_map(|sg_id| graph.get_subgraph(sg_id))
                .filter(|sg| sg.bounds.is_valid())
                .map(|sg| sg.bounds.y)
                .min();
            if let Some(top) = entered_top {
                // With no blank row between a source and the frame, merge on
                // the source exit row itself rather than on the border.
                if merge_primary >= top && top > max_exit {
                    merge_primary = top - 1;
                }
            }
            // Sources leaving frames the target is not in, at different
            // depths (no frame holds them all), merged on a row inside those
            // frames, often a border row the border then redraws over; merge
            // on the row below the lowest of their bottom borders instead.
            let shared_frame = visible_sources
                .first()
                .and_then(|source| graph.get_node_subgraph(&source.id))
                .is_some_and(|frame| {
                    visible_sources
                        .iter()
                        .all(|source| graph.get_node_subgraph(&source.id) == Some(frame))
                });
            let left_bottom = visible_sources
                .iter()
                .flat_map(|source| graph.node_subgraph_chain(&source.id))
                .filter(|sg_id| !graph.is_node_in_subgraph_tree(&to.id, sg_id))
                .filter_map(|sg_id| graph.get_subgraph(sg_id))
                .filter(|sg| sg.bounds.is_valid())
                .map(|sg| sg.bounds.y + sg.bounds.height)
                .max();
            // A merge one column wide is nudged up off the border later, so
            // it keeps merging inside, where one line then leaves the frames.
            let columns = visible_sources
                .iter()
                .map(|source| get_node_center(source).0)
                .chain([get_node_center(to).0]);
            let (low, high) =
                columns.fold((usize::MAX, 0), |(low, high), x| (low.min(x), high.max(x)));
            let tiny = high.saturating_sub(low) <= 1;
            if let Some(below) = left_bottom.filter(|_| !shared_frame && !tiny) {
                if below > merge_primary && below <= limit {
                    merge_primary = below;
                }
            }
        }
        Direction::BT => {
            // Merge one row above the source exits.  Using the source-exit row
            // itself collapses the fan-in corners, source junctions, and merge
            // bar into the same BT seam (for example, the parallel-path
            // subgraph).  Keep one shaft cell below the merge whenever the
            // source/target span has room.
            let max_merge = min_exit.saturating_sub(1);
            let limit = arrow_primary.saturating_add(1);
            merge_primary = max_merge.max(limit);
            // Sources leaving frames the target is not in each exit through a
            // portal of their own; merge outside those frames, on the row above
            // the highest top border, instead of inside them, where the merge
            // bar runs under the borders and leaves the portals as stubs.
            let left_top = visible_sources
                .iter()
                .flat_map(|source| graph.node_subgraph_chain(&source.id))
                .filter(|sg_id| !graph.is_node_in_subgraph_tree(&to.id, sg_id))
                .filter_map(|sg_id| graph.get_subgraph(sg_id))
                .filter(|sg| sg.bounds.is_valid())
                .map(|sg| sg.bounds.y)
                .min();
            if let Some(above) = left_top.and_then(|top| top.checked_sub(1)) {
                if above >= limit && above < merge_primary {
                    merge_primary = above;
                }
            }
            // A target inside a frame none of the sources share is entered
            // through one portal; merge outside that frame, on the row below
            // its bottom border, instead of on the border, where it is hidden.
            // With no blank row between a source and the frame, that is the
            // source exit row itself, unless that source's box spans the
            // trunk: its line would rise straight up the trunk there, leaving
            // its branch nowhere to start.
            let spans_trunk =
                |source: &&Node| source.x <= arrow_x && arrow_x < source.x + source.width;
            let entered_bottom = graph
                .node_subgraph_chain(&to.id)
                .into_iter()
                .filter(|sg_id| {
                    !visible_sources
                        .iter()
                        .any(|source| graph.is_node_in_subgraph_tree(&source.id, sg_id))
                })
                .filter_map(|sg_id| graph.get_subgraph(sg_id))
                .filter(|sg| sg.bounds.is_valid())
                .map(|sg| sg.bounds.y + sg.bounds.height.saturating_sub(1))
                .max();
            if let Some(bottom) = entered_bottom {
                let crowded = visible_sources.iter().any(|source| {
                    spans_trunk(source)
                        && coords.primary_coord(
                            edge_exit_point(source, direction).0,
                            edge_exit_point(source, direction).1,
                        ) == bottom + 1
                });
                if merge_primary <= bottom && bottom < min_exit && !crowded {
                    merge_primary = bottom + 1;
                }
            }
        }
    }

    if let Some((lane_index, lane_count)) = merge_lane {
        if let Some(lane_primary) = merge_lane_primary(
            direction,
            arrow_primary,
            min_exit,
            max_exit,
            lane_index,
            lane_count,
        ) {
            merge_primary = lane_primary;
        }
    }
    if direction == Direction::BT && is_vertical_branch_rejoin_target(graph, &to.id) {
        // The shared collector needs one visible shaft row before the target
        // arrow. The generic BT clamp can otherwise place the collector
        // directly adjacent to the arrow, making the target entry look
        // detached even though the route is topologically connected.
        let maximum_collector = min_exit;
        merge_primary = merge_primary
            .max(arrow_primary.saturating_add(2))
            .min(maximum_collector);
    }
    coords.set_primary(&mut merge_x, &mut merge_y, merge_primary);

    // Get source positions sorted on secondary axis
    let mut source_positions: Vec<(usize, usize, &Node)> = visible_sources
        .iter()
        .map(|n| {
            let (sx, sy) = get_node_center(n);
            (sx, sy, *n)
        })
        .collect();

    source_positions.sort_by_key(|(x, y, _)| coords.secondary_coord(*x, *y));

    // Calculate target position on secondary axis
    let target_secondary = coords.secondary_coord(target_x, target_y);

    // Draw from each source to merge point
    let (actual_span_start, actual_span_end) = draw_source_lines_to_merge(
        &source_positions,
        merge_x,
        merge_y,
        &coords,
        canvas,
        style,
        direction,
        Some(graph),
        Some(&to.id),
        true,
    );

    // Expand span to include target if needed
    let final_span_start = actual_span_start.min(target_secondary);
    let final_span_end = actual_span_end.max(target_secondary);

    // Draw corners FIRST, before the merge line.
    // Use set_edge_char so corners properly resolve with OTHER groups' lines.
    // Vertical stems stop before the merge row, so no overlap with same-group stems.
    if matches!(direction, Direction::TD | Direction::TB) && final_span_start < final_span_end {
        let (sx, sy) = coords.with_secondary(merge_x, merge_y, final_span_start);
        let (ex, ey) = coords.with_secondary(merge_x, merge_y, final_span_end);
        // Choose corner/junction based on whether span edge has source, target, or both
        let start_char = if final_span_start == target_secondary {
            if final_span_start == actual_span_start {
                style.junction_right // ├ - both source and target
            } else {
                style.corner_dl // ┌ - target only
            }
        } else {
            style.corner_ul // └ - source only
        };
        let end_char = if final_span_end == target_secondary {
            if final_span_end == actual_span_end {
                style.junction_left // ┤ - both source and target
            } else {
                style.corner_dr // ┐ - target only
            }
        } else {
            style.corner_ur // ┘ - source only
        };
        set_route_edge_char(canvas, sx, sy, start_char, style, Some(fanin_owner));
        set_route_edge_char(canvas, ex, ey, end_char, style, Some(fanin_owner));
    }

    // Draw horizontal merge line (skips non-empty cells like corners)
    draw_merge_line(
        merge_x,
        merge_y,
        final_span_start,
        final_span_end,
        &coords,
        canvas,
        style,
        Some(fanin_owner),
    );

    // ASCII box corners are represented by `+`, which connects in every
    // direction.  When the BT branch/rejoin collector sits directly above
    // the two source boxes, preserve those source-column junctions on the
    // collector row as well.  Unicode corners are directional and do not
    // need this extra lowering.
    if direction == Direction::BT
        && is_vertical_branch_rejoin_target(graph, &to.id)
        && style.edge_h == '-'
        && style.edge_v == '|'
    {
        for (source_x, source_y, source) in &source_positions {
            let source_secondary = coords.secondary_coord(*source_x, *source_y);
            let boundary_secondary = if source_secondary <= target_secondary {
                source.x.saturating_add(source.width.saturating_sub(1))
            } else {
                source.x
            };
            let (junction_x, junction_y) =
                coords.with_secondary(merge_x, merge_y, boundary_secondary);
            set_route_edge_char(
                canvas,
                junction_x,
                junction_y,
                style.junction_up,
                style,
                Some(fanin_owner),
            );
        }
    }

    let junction_char = match direction {
        Direction::TD | Direction::TB => style.junction_down,
        Direction::LR => style.junction_right, // ├ - edges from above/below, exits right
        Direction::RL => style.junction_left,  // ┤ - edges from above/below, exits left
        Direction::BT => style.junction_up,    // ┴ - edges from left/right, exits up
    };

    // Allow nudging the junction up a row when the span is tiny to avoid double rows.
    let mut merge_y_draw = merge_y;

    if matches!(direction, Direction::TD | Direction::TB) {
        let span_width = final_span_end.saturating_sub(final_span_start);
        // The nudge clears the row above; a frame border there must stay.
        let border_above = merge_y.checked_sub(1).is_some_and(|row| {
            graph.subgraphs.iter().any(|sg| {
                let bounds = &sg.bounds;
                bounds.is_valid()
                    && (bounds.y == row || bounds.y + bounds.height == row + 1)
                    && bounds.x <= final_span_start
                    && final_span_end < bounds.x + bounds.width
            })
        });
        if span_width <= 1 && !border_above {
            for pos in final_span_start..=final_span_end {
                let (x, y) = coords.with_secondary(merge_x, merge_y, pos);
                if !canvas.fallback_route_claims_cell(x, y) {
                    canvas.set(x, y, ' ');
                }
            }
            merge_y_draw = merge_y_draw.saturating_sub(1);
            for pos in final_span_start..=final_span_end {
                let (x, y) = coords.with_secondary(merge_x, merge_y_draw, pos);
                if !canvas.fallback_route_claims_cell(x, y) {
                    canvas.set(x, y, ' ');
                }
            }
            set_route_edge_char(
                canvas,
                merge_x,
                merge_y_draw,
                junction_char,
                style,
                Some(fanin_owner),
            );
        } else {
            for pos in final_span_start..=final_span_end {
                // Skip source endpoint positions - corners will be drawn there.
                // This prevents corners from being resolved to junctions when
                // they overlap with horizontal lines from the SAME group.
                // They'll still correctly resolve to junctions when overlapping
                // with lines from OTHER convergent groups (e.g., crossing_grid).
                if pos == actual_span_start || pos == actual_span_end {
                    continue;
                }
                let (x, y) = coords.with_secondary(merge_x, merge_y, pos);
                let ch = if pos == coords.secondary_coord(merge_x, merge_y) {
                    junction_char
                } else {
                    coords.secondary_edge_char(style)
                };
                set_route_edge_char(canvas, x, y, ch, style, Some(fanin_owner));
            }
        }
    } else {
        set_route_edge_char(
            canvas,
            merge_x,
            merge_y,
            junction_char,
            style,
            Some(fanin_owner),
        );
    }

    if matches!(direction, Direction::TD | Direction::TB) && final_span_start < final_span_end {
        let (sx, sy) = coords.with_secondary(merge_x, merge_y_draw, final_span_start);
        let (ex, ey) = coords.with_secondary(merge_x, merge_y_draw, final_span_end);
        // Choose corner/junction based on whether span edge has source, target, or both
        // - Source only: connects up (from source) and horizontal (to merge bar)
        // - Target only: connects down (to target) and horizontal (to merge bar)
        // - Both: connects up, down, and horizontal (junction)
        let start_char = if final_span_start == target_secondary {
            // Target is at span start
            if final_span_start == actual_span_start {
                // Also a source here - need junction (up+down+right)
                style.junction_right // ├
            } else {
                // Target only - corner (down+right)
                style.corner_dl // ┌
            }
        } else {
            // Source only at span start
            style.corner_ul // └ - up and right
        };
        let end_char = if final_span_end == target_secondary {
            // Target is at span end
            if final_span_end == actual_span_end {
                // Also a source here - need junction (up+down+left)
                style.junction_left // ┤
            } else {
                // Target only - corner (down+left)
                style.corner_dr // ┐
            }
        } else {
            // Source only at span end
            style.corner_ur // ┘ - up and left
        };
        set_route_edge_char(canvas, sx, sy, start_char, style, Some(fanin_owner));
        set_route_edge_char(canvas, ex, ey, end_char, style, Some(fanin_owner));
    }
    let (mut final_start_x, mut final_start_y) = coords.advance(merge_x, merge_y_draw, 1);
    // Through a target frame's title (gen_c_129's fan-in into Target).
    if let Some(next) = step_past_title(
        canvas,
        style,
        graph,
        fanin_owner,
        &coords,
        to,
        (final_start_x, final_start_y),
        arrow_x,
    ) {
        (final_start_x, final_start_y) = next;
    }
    draw_line_primary(
        final_start_x,
        final_start_y,
        arrow_x,
        arrow_y,
        &coords,
        canvas,
        style,
        Some(graph),
        Some(fanin_owner),
    );

    // Arrow
    set_route_char(
        canvas,
        arrow_x,
        arrow_y,
        coords.arrow_end(style),
        Some(fanin_owner),
    );

    if let Some(baseline) = branch_rejoin_baseline {
        let covered_edge_ids: Vec<String> = graph
            .edges
            .iter()
            .filter(|edge| edge.to == to.id)
            .map(|edge| edge_route_owner_id(graph, &edge.from, &edge.to))
            .collect();
        let mut plan = FallbackRoutePlan::new(fanin_owner_id.clone(), "branch-rejoin-convergence");
        plan.set_scene_coverage(covered_edge_ids.clone());
        for paint in canvas.non_space_delta(&baseline) {
            let inside_node = graph.nodes.iter().any(|node| {
                paint.point.x >= node.x
                    && paint.point.x < node.x.saturating_add(node.width)
                    && paint.point.y >= node.y
                    && paint.point.y < node.y.saturating_add(node.height)
            });
            if inside_node {
                continue;
            }
            let owner_id = canvas
                .get_meta(paint.point.x, paint.point.y)
                .and_then(|meta| meta.owner_id.as_deref());
            if owner_id == Some(fanin_owner_id.as_str())
                || covered_edge_ids
                    .iter()
                    .any(|edge_id| Some(edge_id.as_str()) == owner_id)
            {
                plan.push_paint(paint.point.x, paint.point.y, paint.glyph);
            }
        }
        if let Some(reason) = plan.validation_error(canvas.width, canvas.height) {
            canvas.record_fallback_route_rejection(
                plan.owner_id.clone(),
                plan.strategy.clone(),
                reason,
            );
        } else {
            canvas.record_fallback_route_plan(plan);
        }
    }
}

fn merge_lane_primary(
    direction: Direction,
    arrow_primary: usize,
    min_exit: usize,
    max_exit: usize,
    lane_index: usize,
    lane_count: usize,
) -> Option<usize> {
    if lane_count < 2 {
        return None;
    }
    let (start, end) = match direction {
        Direction::TD | Direction::TB | Direction::LR => {
            (max_exit.saturating_add(2), arrow_primary.saturating_sub(1))
        }
        Direction::BT | Direction::RL => {
            (arrow_primary.saturating_add(2), min_exit.saturating_sub(1))
        }
    };
    if end < start || end - start + 1 < lane_count {
        return None;
    }
    let horizontal = matches!(direction, Direction::LR | Direction::RL);
    let stride_two_capacity = lane_count
        .saturating_sub(1)
        .saturating_mul(2)
        .saturating_add(1);
    let stride = if horizontal && end - start + 1 >= stride_two_capacity {
        2
    } else {
        1
    };
    let index = lane_index.min(lane_count - 1);
    start.checked_add(index.saturating_mul(stride))
}

#[cfg(test)]
mod tests {
    use super::merge_lane_primary;
    use crate::graph::Direction;

    #[test]
    fn horizontal_merge_lanes_prefer_a_two_cell_stride() {
        assert_eq!(merge_lane_primary(Direction::LR, 30, 0, 12, 0, 3), Some(14));
        assert_eq!(merge_lane_primary(Direction::LR, 30, 0, 12, 1, 3), Some(16));
        assert_eq!(merge_lane_primary(Direction::LR, 30, 0, 12, 2, 3), Some(18));
        assert_eq!(merge_lane_primary(Direction::RL, 10, 20, 0, 2, 3), Some(16));
    }

    #[test]
    fn horizontal_merge_lanes_fall_back_when_stride_two_does_not_fit() {
        assert_eq!(merge_lane_primary(Direction::LR, 16, 0, 10, 0, 3), Some(12));
        assert_eq!(merge_lane_primary(Direction::LR, 16, 0, 10, 1, 3), Some(13));
        assert_eq!(merge_lane_primary(Direction::LR, 16, 0, 10, 2, 3), Some(14));
    }

    #[test]
    fn vertical_merge_lanes_remain_consecutive() {
        assert_eq!(merge_lane_primary(Direction::TD, 20, 0, 10, 0, 3), Some(12));
        assert_eq!(merge_lane_primary(Direction::TD, 20, 0, 10, 2, 3), Some(14));
        assert_eq!(merge_lane_primary(Direction::BT, 10, 20, 0, 2, 3), Some(14));
    }
}

// ============================================================================
// Helper Functions
// ============================================================================
