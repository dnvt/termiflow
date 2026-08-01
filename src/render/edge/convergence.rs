//! Convergence and fan-in route orchestration.

use crate::graph::{Direction, Graph, Node};
use crate::orientation::OrientedCoords;
use crate::spacing::SpacingConfig;
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::portal_projection::{is_textual, stamp_portal_opening};
use super::super::semantic::CellOwnerKind;
use super::edge_primitives::{
    adjusted_edge_entry_point, draw_line_primary, draw_line_secondary, edge_entry_candidates,
    edge_exit_point, get_node_center, hits_foreign_subgraph_border, is_subgraph_title_cell,
};
use super::subgraph::{preferred_portal_x, smallest_visual_container};
use super::{edge_route_owner_id, set_route_char, set_route_edge_char, RouteOwner, ROUTE_Z_INDEX};

pub(super) fn route_fanout_into_subgraph_td(
    from: &Node,
    targets: &[(usize, usize, &Node)],
    canvas: &mut Canvas,
    style: &StyleChars,
    sg: &crate::graph::Subgraph,
    direction: Direction,
    graph: &Graph,
) {
    let coords = OrientedCoords::new(direction);
    let (stem_start_x, stem_start_y) = edge_exit_point(from, direction);
    let fanout_owner_id = format!("fanout:{}", from.id);
    let fanout_owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: fanout_owner_id.as_str(),
    };

    let portal_center = sg.bounds.x + sg.bounds.width / 2;
    let min_target_x = targets
        .iter()
        .map(|(x, _, _)| *x)
        .min()
        .unwrap_or(portal_center);
    let max_target_x = targets
        .iter()
        .map(|(x, _, _)| *x)
        .max()
        .unwrap_or(portal_center);
    let junction_x = portal_center.clamp(min_target_x, max_target_x);

    let portal_y = sg.bounds.y.saturating_add(1);
    let min_arrow_y = targets
        .iter()
        .map(|(_, _, t)| adjusted_edge_entry_point(t, direction, graph).1)
        .min()
        .unwrap_or(portal_y.saturating_add(2));

    if std::env::var("DEBUG_FANOUT").is_ok() {
        let target_xs: Vec<usize> = targets.iter().map(|(x, _, _)| *x).collect();
        eprintln!(
            "fanout stem=({}, {}) portal_y={} jx={} targets={:?}",
            stem_start_x, stem_start_y, portal_y, junction_x, target_xs
        );
    }

    // Leave a dedicated spine row before the split so the center column is visible,
    // and keep the split above the arrow row.
    let delta = min_arrow_y.saturating_sub(portal_y);
    let mut junction_y = portal_y.saturating_add(delta / 2);
    let min_split_y = portal_y.saturating_add(1);
    let max_split_y = min_arrow_y.saturating_sub(2).max(min_split_y);
    if junction_y < min_split_y {
        junction_y = min_split_y;
    } else if junction_y > max_split_y {
        junction_y = max_split_y;
    }

    // Align horizontally to the subgraph center before dropping in.
    if stem_start_x != junction_x {
        let (hx0, hx1) = if stem_start_x < junction_x {
            (stem_start_x, junction_x)
        } else {
            (junction_x, stem_start_x)
        };
        for x in hx0..=hx1 {
            set_route_edge_char(
                canvas,
                x,
                stem_start_y,
                style.edge_h,
                style,
                Some(fanout_owner),
            );
        }
        let corner = if junction_x > stem_start_x {
            style.corner_dr
        } else {
            style.corner_dl
        };
        set_route_edge_char(
            canvas,
            junction_x,
            stem_start_y,
            corner,
            style,
            Some(fanout_owner),
        );
    } else {
        set_route_edge_char(
            canvas,
            junction_x,
            stem_start_y,
            coords.primary_edge_char(style),
            style,
            Some(fanout_owner),
        );
    }

    // Vertical spine down into the subgraph (including the portal row)
    if stem_start_y < junction_y {
        for y in (stem_start_y + 1)..=junction_y {
            set_route_edge_char(
                canvas,
                junction_x,
                y,
                coords.primary_edge_char(style),
                style,
                Some(fanout_owner),
            );
        }
    }

    let mut sorted_targets = targets.to_vec();
    sorted_targets.sort_by_key(|(x, y, _)| coords.secondary_coord(*x, *y));

    let first_secondary = coords.secondary_coord(sorted_targets[0].0, sorted_targets[0].1);
    let last_secondary = coords.secondary_coord(
        sorted_targets[sorted_targets.len() - 1].0,
        sorted_targets[sorted_targets.len() - 1].1,
    );
    let junction_secondary = coords.secondary_coord(junction_x, junction_y);

    // Draw a visible spine row just above the junction.
    if junction_y > portal_y {
        let spine_y = junction_y.saturating_sub(1);
        set_route_edge_char(
            canvas,
            junction_x,
            spine_y,
            coords.primary_edge_char(style),
            style,
            Some(fanout_owner),
        );
    }

    let span_start = first_secondary.min(junction_secondary);
    let span_end = last_secondary.max(junction_secondary);

    for pos in span_start..=span_end {
        let (span_x, span_y) = coords.with_secondary(junction_x, junction_y, pos);
        let c = if pos == junction_secondary {
            match direction {
                Direction::TD | Direction::TB => style.junction_up,
                Direction::LR => style.junction_left,
                Direction::RL => style.junction_right,
                Direction::BT => style.junction_down,
            }
        } else if pos == span_start {
            match direction {
                Direction::TD | Direction::TB => style.corner_dl,
                Direction::LR => style.corner_dl,
                Direction::RL => style.corner_dr,
                Direction::BT => style.corner_ul,
            }
        } else if pos == span_end {
            match direction {
                Direction::TD | Direction::TB => style.corner_dr,
                Direction::LR => style.corner_ul,
                Direction::RL => style.corner_ur,
                Direction::BT => style.corner_ur,
            }
        } else {
            coords.secondary_edge_char(style)
        };
        set_route_edge_char(canvas, span_x, span_y, c, style, Some(fanout_owner));
    }

    // Ensure the split junction reads as an upward tee (trunk enters from above).
    if matches!(direction, Direction::TD | Direction::TB) {
        set_route_char(
            canvas,
            junction_x,
            junction_y,
            style.junction_up,
            Some(fanout_owner),
        );
    }

    for (target_x, target_y, target) in &sorted_targets {
        let branch_owner_id = edge_route_owner_id(graph, &from.id, &target.id);
        let branch_owner = RouteOwner {
            kind: CellOwnerKind::EdgeSegment,
            id: branch_owner_id.as_str(),
        };
        let target_secondary = coords.secondary_coord(*target_x, *target_y);
        let (arrow_x, arrow_y) = adjusted_edge_entry_point(target, direction, graph);
        let (drop_x, drop_y) = coords.with_secondary(junction_x, junction_y, target_secondary);
        let (drop_start_x, drop_start_y) = coords.advance(drop_x, drop_y, 1);
        if drop_start_x != arrow_x || drop_start_y != arrow_y {
            draw_line_primary(
                drop_start_x,
                drop_start_y,
                arrow_x,
                arrow_y,
                &coords,
                canvas,
                style,
                None,
                Some(branch_owner),
            );
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
    if std::env::var("DEBUG_FANIN").is_ok() {
        eprintln!(
            "fanin merge_x={} merge_y={} span=({}, {}) target_sec={} target=({}, {}) arrow=({}, {})",
            merge_x,
            merge_y,
            final_span_start,
            final_span_end,
            target_secondary,
            target_x,
            target_y,
            arrow_x,
            arrow_y
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
        if std::env::var("DEBUG_FANIN").is_ok() {
            eprintln!(
                "cleanup bottom_y={} fill='{}' portals={:?}",
                bottom_y, border_fill, portal_columns
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
        if ch != ' ' {
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
    let (arrow_x, arrow_y) = adjusted_edge_entry_point(target, direction, graph);
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

    let merge_x = preferred_portal_x(
        &sg.bounds,
        sg.title.as_deref(),
        arrow_x,
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
            stamp_portal_opening(canvas, merge_x, top_y, style, "merge_portal", ROUTE_Z_INDEX);
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
            right_border_x
                .saturating_sub(2)
                .max(max_exit_x.saturating_add(1))
                .clamp(min_inside_x, max_inside_x)
        }
        Direction::RL => {
            let min_exit_x = sources
                .iter()
                .map(|n| edge_exit_point(n, direction).0)
                .min()
                .unwrap_or(max_inside_x);
            left_border_x
                .saturating_add(2)
                .min(min_exit_x.saturating_sub(1))
                .clamp(min_inside_x, max_inside_x)
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
    if std::env::var("DEBUG_FANIN").is_ok() {
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
            Direction::LR => arrow_x
                .saturating_sub(2)
                .clamp(outside_x, arrow_x.saturating_sub(1)),
            Direction::RL => arrow_x
                .saturating_add(2)
                .clamp(arrow_x.saturating_add(1), outside_x),
            _ => unreachable!(),
        };
        let going_before = merge_y > arrow_y;
        if std::env::var("DEBUG_FANIN").is_ok() {
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
) -> (usize, usize) {
    // Pre-compute span bounds BEFORE drawing so corner characters are correct
    let mut span_start = usize::MAX;
    let mut span_end = 0;
    for &(src_x, src_y, _) in source_positions {
        let src_secondary = coords.secondary_coord(src_x, src_y);
        span_start = span_start.min(src_secondary);
        span_end = span_end.max(src_secondary);
    }
    if matches!(direction, Direction::TD | Direction::TB) {
        let merge_secondary = coords.secondary_coord(merge_x, merge_y);
        span_start = span_start.min(merge_secondary);
        span_end = span_end.max(merge_secondary);
    }

    for &(src_x, src_y, source) in source_positions {
        let owner_id = graph
            .zip(target_id)
            .map(|(graph, target_id)| edge_route_owner_id(graph, &source.id, target_id));
        let owner = owner_id.as_deref().map(|owner_id| RouteOwner {
            kind: CellOwnerKind::EdgeSegment,
            id: owner_id,
        });
        let (edge_x, edge_y) = edge_exit_point(source, direction);
        let src_secondary = coords.secondary_coord(src_x, src_y);

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
                let node_border_y = source.y;
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
                if src_x < canvas.width && border_y < canvas.height {
                    set_route_edge_char(canvas, src_x, border_y, style.junction_down, style, owner);
                }
            }
            Direction::BT => {
                // BT: place junction on top border of source
                let border_y = source.y;
                if src_x < canvas.width && border_y < canvas.height {
                    set_route_edge_char(canvas, src_x, border_y, style.junction_up, style, owner);
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
pub fn route_convergent_edges(
    from_nodes: &[&Node],
    to: &Node,
    canvas: &mut Canvas,
    style: &StyleChars,
    spacing: &SpacingConfig,
    direction: Direction,
    graph: &Graph,
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
    let debug = std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok();

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
        }
        Direction::TD | Direction::TB => {
            // Merge just below the lowest source (leave a full row for stems), but above the target.
            let min_merge = max_exit.saturating_add(2);
            let limit = arrow_primary.saturating_sub(1);
            merge_primary = min_merge.min(limit);
        }
        Direction::BT => {
            // Merge above the highest source (leave a full row for stems), but below the target.
            let max_merge = min_exit.saturating_sub(2);
            // Leave a full cell between merge and arrow so the arrow isn't adjacent to a junction.
            let limit = arrow_primary.saturating_add(2);
            merge_primary = max_merge.max(limit);
        }
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
        if span_width <= 1 {
            for pos in final_span_start..=final_span_end {
                let (x, y) = coords.with_secondary(merge_x, merge_y, pos);
                canvas.set(x, y, ' ');
            }
            merge_y_draw = merge_y_draw.saturating_sub(1);
            for pos in final_span_start..=final_span_end {
                let (x, y) = coords.with_secondary(merge_x, merge_y_draw, pos);
                canvas.set(x, y, ' ');
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
    let (final_start_x, final_start_y) = coords.advance(merge_x, merge_y_draw, 1);
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
}

// ============================================================================
// Helper Functions
// ============================================================================
