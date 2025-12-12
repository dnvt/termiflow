//! Unified, direction-agnostic edge routing.
//!
//! This module provides a single edge routing algorithm that works for all
//! diagram orientations (TD, LR, BT, RL) using the orientation abstraction.

use crate::graph::{Direction, Graph, Node};
use crate::orientation::{is_before, OrientedCoords};
use crate::style::{StyleChars, STEM_LENGTH_HORIZONTAL, STEM_LENGTH_VERTICAL};

use super::canvas::{is_junction, Canvas};
use super::is_textual;

/// Route edges from a single source to multiple targets (divergence)
/// Works for all orientations using the abstraction layer
pub fn route_divergent_edges(
    from: &Node,
    to_nodes: &[&Node],
    canvas: &mut Canvas,
    style: &StyleChars,
    direction: Direction,
    graph: &Graph,
) {
    if to_nodes.is_empty() || !canvas.is_visible(from) {
        return;
    }

    let coords = OrientedCoords::new(direction);
    let debug_timing = std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok();
    if debug_timing {
        let targets: Vec<&str> = to_nodes.iter().map(|n| n.id.as_str()).collect();
        eprintln!("render: route from {} to {:?}", from.id, targets);
    }

    // Filter to visible targets only
    let visible_targets: Vec<&Node> = to_nodes
        .iter()
        .filter(|n| canvas.is_visible(n))
        .copied()
        .collect();
    if visible_targets.is_empty() {
        return;
    }

    // Calculate source center based on orientation
    let (src_x, src_y) = get_node_center(from);

    // Calculate stem start position (edge of source box on primary axis)
    let (stem_start_x, stem_start_y) = edge_exit_point(from, direction);

    // Calculate junction position (stem length away from source)
    let stem_length = match direction {
        Direction::LR | Direction::RL => STEM_LENGTH_HORIZONTAL,
        _ => STEM_LENGTH_VERTICAL,
    };

    let (mut junction_x, mut junction_y) = coords.advance(stem_start_x, stem_start_y, stem_length);

    // Get target centers and sort them on secondary axis
    let mut target_positions: Vec<(usize, usize, &Node)> = visible_targets
        .iter()
        .map(|&n| {
            let (tx, ty) = get_node_center(n);
            (tx, ty, n)
        })
        .collect();

    target_positions.sort_by_key(|(x, y, _)| coords.secondary_coord(*x, *y));

    // If all targets share the same subgraph (different from the source), branch
    // inside that subgraph to keep the junction aligned with its interior.
    if matches!(direction, Direction::TD | Direction::TB) && target_positions.len() > 1 {
        if let Some(target_sg) = visible_targets.first().and_then(|n| graph.get_node_subgraph(&n.id))
        {
            let all_same = visible_targets
                .iter()
                .all(|n| graph.get_node_subgraph(&n.id) == Some(target_sg));
            let source_sg = graph.get_node_subgraph(&from.id);
            if all_same && source_sg != Some(target_sg) {
                if let Some(sg) = graph.get_subgraph(target_sg) {
                    route_divergent_into_subgraph_td(
                        from,
                        &visible_targets,
                        canvas,
                        style,
                        sg,
                        direction,
                    );
                    return;
                }
            }
        }
    }

    // For horizontal fan-outs, nudge the junction away from the targets so we keep
    // visible elbows/dashes before the arrows.
    if matches!(direction, Direction::LR | Direction::RL) && target_positions.len() > 1 {
        let arrow_primaries: Vec<usize> = target_positions
            .iter()
            .map(|(_, _, n)| {
                let (ax, ay) = edge_entry_point(n, direction);
                coords.primary_coord(ax, ay)
            })
            .collect();
        if let Some(closest_arrow) = match direction {
            Direction::LR => arrow_primaries.iter().min(),
            Direction::RL => arrow_primaries.iter().max(),
            _ => None,
        } {
            let stem_start_primary = coords.primary_coord(stem_start_x, stem_start_y);
            let current_primary = coords.primary_coord(junction_x, junction_y);
            let desired_primary = match direction {
                // Keep at least two visible dashes before the arrow when possible.
                Direction::LR => closest_arrow.saturating_sub(3),
                Direction::RL => closest_arrow.saturating_add(3),
                _ => current_primary,
            };
            let adjusted_primary = match direction {
                Direction::LR => desired_primary.min(current_primary).max(stem_start_primary + 1),
                Direction::RL => desired_primary.max(current_primary).min(
                    stem_start_primary.saturating_sub(1),
                ),
                _ => current_primary,
            };
            if adjusted_primary != current_primary {
                coords.set_primary(&mut junction_x, &mut junction_y, adjusted_primary);
            }
        }
    }

    // Ensure some horizontal breathing room between junction and nearest target arrow for LR/RL.
    if matches!(direction, Direction::LR | Direction::RL) && target_positions.len() > 1 {
        let stem_start_primary = coords.primary_coord(stem_start_x, stem_start_y);
        let junction_primary = coords.primary_coord(junction_x, junction_y);
        let nearest_arrow_primary = target_positions
            .iter()
            .map(|(_, _, n)| {
                let (ax, ay) = edge_entry_point(n, direction);
                coords.primary_coord(ax, ay)
            })
            .min_by_key(|p| {
                if junction_primary > *p {
                    junction_primary - *p
                } else {
                    *p - junction_primary
                }
            });
        if let Some(arrow_primary) = nearest_arrow_primary {
            let gap = if junction_primary > arrow_primary {
                junction_primary - arrow_primary
            } else {
                arrow_primary - junction_primary
            };
            // With `drop_start = junction + 1`, `gap=3` yields two dashes before the arrow.
            let min_gap = 3;
            if gap < min_gap {
                let adjust = min_gap - gap;
                let mut adjusted_primary = junction_primary;
                match direction {
                    Direction::LR => {
                        adjusted_primary = adjusted_primary.saturating_sub(adjust);
                        adjusted_primary = adjusted_primary.max(stem_start_primary + 1);
                    }
                    Direction::RL => {
                        adjusted_primary = adjusted_primary.saturating_add(adjust);
                        adjusted_primary = adjusted_primary.min(
                            stem_start_primary.saturating_sub(1).max(adjusted_primary),
                        );
                    }
                    _ => {}
                }
                coords.set_primary(&mut junction_x, &mut junction_y, adjusted_primary);
            }
        }
    }

    // Single target: direct route
    if target_positions.len() == 1 {
        let (target_x, target_y, target) = (
            target_positions[0].0,
            target_positions[0].1,
            target_positions[0].2,
        );

        let (arrow_x, arrow_y) = edge_entry_point(target, direction);

        if matches!(direction, Direction::TD | Direction::TB) {
            let from_sg = graph.get_node_subgraph(&from.id);
            let to_sg = graph.get_node_subgraph(&target.id);
            if std::env::var("DEBUG_CROSS").is_ok() {
                eprintln!(
                    "single-edge cross? {}({:?}) -> {}({:?})",
                    from.id, from_sg, target.id, to_sg
                );
            }
            if from_sg != to_sg {
                if route_cross_subgraph_td(
                    from,
                    target,
                    stem_start_x,
                    stem_start_y,
                    arrow_x,
                    arrow_y,
                    canvas,
                    style,
                    graph,
                ) {
                    canvas.set(arrow_x, arrow_y, coords.arrow_end(style));
                    return;
                }
            }
        } else if direction == Direction::BT {
            let from_sg = graph.get_node_subgraph(&from.id);
            let to_sg = graph.get_node_subgraph(&target.id);
            if from_sg != to_sg {
                if route_cross_subgraph_bt(
                    from,
                    target,
                    stem_start_x,
                    stem_start_y,
                    arrow_x,
                    arrow_y,
                    canvas,
                    style,
                    graph,
                ) {
                    canvas.set(arrow_x, arrow_y, coords.arrow_end(style));
                    return;
                }
            }
        }

        if debug_timing {
            eprintln!(
                "  single target centers ({},{}) -> ({},{})",
                src_x, src_y, arrow_x, arrow_y
            );
        }

        let src_secondary = coords.secondary_coord(src_x, src_y);
        let target_secondary = coords.secondary_coord(target_x, target_y);

        if src_secondary == target_secondary {
            // Aligned: straight line on primary axis
            draw_line_primary(
                stem_start_x,
                stem_start_y,
                arrow_x,
                arrow_y,
                &coords,
                canvas,
                style,
                Some(graph),
            );
            if matches!(direction, Direction::TD | Direction::TB) {
                if let (Some(from_sg), Some(to_sg)) =
                    (graph.get_node_subgraph(&from.id), graph.get_node_subgraph(&target.id))
                {
                    if from_sg != to_sg {
                        if let Some(sg) = graph.get_subgraph(to_sg) {
                            let border_y = sg.bounds.y;
                            if arrow_x < canvas.width && border_y < canvas.height {
                                if !sg.has_title() {
                                    canvas.set_edge_char(
                                        arrow_x,
                                        border_y,
                                        style.junction_down,
                                        style,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        } else {
            // L-shaped route
            let going_before = is_before(src_secondary, target_secondary);

            // Horizontal layouts look cleaner when we turn toward the row first, then
            // travel horizontally to the target.
            if matches!(direction, Direction::LR | Direction::RL) {
                // If the target sits above/below the source, go vertical first so the
                // corner hugs the target row instead of floating off the junction stem.
                if target_secondary != src_secondary {
                    let spine_x = junction_x;

                    // Horizontal run out of the source before turning.
                    draw_line_primary(
                        stem_start_x,
                        stem_start_y,
                        spine_x,
                        stem_start_y,
                        &coords,
                        canvas,
                        style,
                        Some(graph),
                    );

                    // Turn onto the vertical spine at the source row.
                    let going_up = target_secondary < src_secondary;
                    let corner1 = match direction {
                        Direction::LR => {
                            if going_up { style.corner_ur } else { style.corner_dr }
                        }
                        Direction::RL => {
                            if going_up { style.corner_ul } else { style.corner_dl }
                        }
                        _ => unreachable!(),
                    };
                    canvas.set_edge_char(spine_x, stem_start_y, corner1, style);

                    // Vertical segment to target row.
                    let (bend_x, bend_y) =
                        coords.with_secondary(spine_x, stem_start_y, target_secondary);
                    draw_line_secondary(
                        spine_x,
                        stem_start_y,
                        bend_x,
                        bend_y,
                        &coords,
                        canvas,
                        style,
                        Some(graph),
                    );

                    // Turn toward the target column.
                    let corner2 = match direction {
                        Direction::LR => {
                            if going_up { style.corner_dl } else { style.corner_ul }
                        }
                        Direction::RL => {
                            if going_up { style.corner_dr } else { style.corner_ur }
                        }
                        _ => unreachable!(),
                    };
                    canvas.set_edge_char(bend_x, bend_y, corner2, style);

                    // Final horizontal run to the arrow.
                    let (seg_start_x, seg_start_y) = coords.advance(bend_x, bend_y, 1);
                    draw_line_primary(
                        seg_start_x,
                        seg_start_y,
                        arrow_x,
                        arrow_y,
                        &coords,
                        canvas,
                        style,
                        Some(graph),
                    );
                    canvas.set(arrow_x, arrow_y, coords.arrow_end(style));
                    return;
                }

                let (bend_x, bend_y) =
                    coords.with_secondary(stem_start_x, stem_start_y, target_secondary);

                // Move vertically first
                draw_line_secondary(
                    stem_start_x,
                    stem_start_y,
                    bend_x,
                    bend_y,
                    &coords,
                    canvas,
                    style,
                    Some(graph),
                );

                // Turn toward the target column
                let corner = coords.corner_secondary_to_end(going_before, style);
                canvas.set_edge_char(bend_x, bend_y, corner, style);

                // Final horizontal run to the arrow
                let (seg_start_x, seg_start_y) = coords.advance(bend_x, bend_y, 1);
                draw_line_primary(
                    seg_start_x,
                    seg_start_y,
                    arrow_x,
                    arrow_y,
                    &coords,
                    canvas,
                    style,
                    Some(graph),
                );
            } else {
                // For BT fan-outs, the elbow row can overlap a previously rendered
                // convergence bar (e.g. a sibling fan-in into another target).
                // If so, prefer placing the elbow on the stem start row.
                if matches!(direction, Direction::BT) {
                    let (bend_x, _bend_y) =
                        coords.with_secondary(junction_x, junction_y, target_secondary);
                    let (x0, x1) = if junction_x <= bend_x {
                        (junction_x, bend_x)
                    } else {
                        (bend_x, junction_x)
                    };
                    let span_conflicts = if x1 > x0 + 1 {
                        ((x0 + 1)..x1).any(|x| canvas.get(x, junction_y) != ' ')
                    } else {
                        false
                    };
                    let junction_cell = canvas.get(junction_x, junction_y);
                    let junction_conflicts = junction_cell != ' ' && !super::canvas::is_vertical(junction_cell, style);
                    if span_conflicts {
                        let (cand_x, cand_y) = coords.retreat(junction_x, junction_y, 1);
                        let stem_start_primary = coords.primary_coord(stem_start_x, stem_start_y);
                        let cand_primary = coords.primary_coord(cand_x, cand_y);
                        if cand_primary <= stem_start_primary {
                            let (cand_bx, _) =
                                coords.with_secondary(cand_x, cand_y, target_secondary);
                            let (cx0, cx1) = if cand_x <= cand_bx {
                                (cand_x, cand_bx)
                            } else {
                                (cand_bx, cand_x)
                            };
                            let cand_conflicts = if cx1 > cx0 + 1 {
                                ((cx0 + 1)..cx1).any(|x| canvas.get(x, cand_y) != ' ')
                            } else {
                                false
                            };
                            if !cand_conflicts {
                                junction_x = cand_x;
                                junction_y = cand_y;
                            }
                        }
                    } else if junction_conflicts {
                        // If we would immediately intersect an existing horizontal bar,
                        // prefer shifting the elbow down onto the stem row.
                        let (cand_x, cand_y) = coords.retreat(junction_x, junction_y, 1);
                        let stem_start_primary = coords.primary_coord(stem_start_x, stem_start_y);
                        if coords.primary_coord(cand_x, cand_y) <= stem_start_primary {
                            junction_x = cand_x;
                            junction_y = cand_y;
                        }
                    }
                }

                // 1. Stem from source
                draw_line_primary(
                    stem_start_x,
                    stem_start_y,
                    junction_x,
                    junction_y,
                    &coords,
                    canvas,
                    style,
                    Some(graph),
                );

                // 2. Turn at junction
                let corner = coords.corner_start_to_secondary(going_before, style);
                canvas.set_edge_char(junction_x, junction_y, corner, style);

                // 3. Secondary span to target column
                let (bend_x, bend_y) =
                    coords.with_secondary(junction_x, junction_y, target_secondary);
                draw_line_secondary(
                    junction_x,
                    junction_y,
                    bend_x,
                    bend_y,
                    &coords,
                    canvas,
                    style,
                    Some(graph),
                );

                // 4. Turn to target
                let corner2 = coords.corner_secondary_to_end(going_before, style);
                canvas.set_edge_char(bend_x, bend_y, corner2, style);

                // 5. Final segment to arrow
                let (seg_start_x, seg_start_y) = coords.advance(bend_x, bend_y, 1);
                draw_line_primary(
                    seg_start_x,
                    seg_start_y,
                    arrow_x,
                    arrow_y,
                    &coords,
                    canvas,
                    style,
                    Some(graph),
                );
            }
        }

        // Arrow at target
        canvas.set(arrow_x, arrow_y, coords.arrow_end(style));

        // If the edge exits a subgraph boundary (TD/TB), stamp a junction on the border
        // so the vertical line visually pierces the container instead of leaving a gap.
        if matches!(direction, Direction::TD | Direction::TB) {
            if let Some(from_sg) = graph.get_node_subgraph(&from.id) {
                if graph.get_node_subgraph(&target.id) != Some(from_sg) {
                    if let Some(sg) = graph.get_subgraph(from_sg) {
                        let border_y = sg.bounds.y + sg.bounds.height.saturating_sub(1);
                        if arrow_x < canvas.width && border_y < canvas.height {
                            canvas.set_edge_char(arrow_x, border_y, style.junction_down, style);
                        }
                    }
                }
            }
        }
        return;
    }

    // Fan-out into a single subgraph: route to the subgraph interior before splitting
    // so junctions sit inside the container rather than on its border.
    if matches!(direction, Direction::TD | Direction::TB) {
        if let Some(target_sg_id) = target_positions
            .first()
            .and_then(|(_, _, n)| graph.get_node_subgraph(&n.id))
        {
            let source_sg = graph.get_node_subgraph(&from.id);
            let all_targets_same_sg = target_positions.iter().all(|(_, _, n)| {
                graph.get_node_subgraph(&n.id) == Some(target_sg_id)
            });

            if all_targets_same_sg && source_sg != Some(target_sg_id) {
                if let Some(sg) = graph.get_subgraph(target_sg_id) {
                    if sg.bounds.is_valid() {
                        route_fanout_into_subgraph_td(
                            from,
                            &target_positions,
                            canvas,
                            style,
                            sg,
                            direction,
                        );
                        return;
                    }
                }
            }
        }
    }

    // Multiple targets: draw branching structure

    // 1. Draw stem from source to junction (not including junction)
    let stem_length = {
        let start_primary = coords.primary_coord(stem_start_x, stem_start_y);
        let junction_primary = coords.primary_coord(junction_x, junction_y);
        match direction {
            Direction::LR | Direction::TD | Direction::TB => {
                junction_primary.saturating_sub(start_primary)
            }
            Direction::RL | Direction::BT => start_primary.saturating_sub(junction_primary),
        }
    };
    for i in 0..stem_length {
        let (px, py) = coords.advance(stem_start_x, stem_start_y, i);
        canvas.set_edge_char(px, py, coords.primary_edge_char(style), style);
    }

    // 2. Calculate span on secondary axis
    let first_secondary = coords.secondary_coord(target_positions[0].0, target_positions[0].1);
    let last_secondary = coords.secondary_coord(
        target_positions[target_positions.len() - 1].0,
        target_positions[target_positions.len() - 1].1,
    );
    let src_secondary = coords.secondary_coord(src_x, src_y);

    let span_start = first_secondary;
    let span_end = last_secondary;

    // If the source column sits on the edge of the span, nudge the junction inward
    // so the bar keeps corners at both ends.
    let mut junction_secondary = src_secondary;
    if span_end > span_start {
        if junction_secondary == span_start {
            junction_secondary = span_start + 1;
        } else if junction_secondary == span_end {
            junction_secondary = span_end - 1;
        }
    }

    // 3. Draw horizontal junction span with corners and junction
    let (start_corner, end_corner) = match direction {
        Direction::TD | Direction::TB => (style.corner_dl, style.corner_dr),
        Direction::BT => (style.corner_ul, style.corner_ur),
        Direction::LR => (style.corner_dl, style.corner_ul),
        Direction::RL => (style.corner_dr, style.corner_ur),
    };
    let has_target_at_junction = target_positions
        .iter()
        .any(|(x, y, _)| coords.secondary_coord(*x, *y) == junction_secondary);
    for pos in span_start..=span_end {
        let (span_x, span_y) = coords.with_secondary(junction_x, junction_y, pos);

        let c = if pos == junction_secondary {
            // Junction at source position - stem meets vertical span
            match direction {
                Direction::TD | Direction::TB => style.junction_up,    // ┴
                Direction::LR => {
                    if has_target_at_junction {
                        style.junction_right // ├ (branch right on this row)
                    } else {
                        style.junction_left // ┤ (no right branch on this row)
                    }
                }
                Direction::RL => {
                    if has_target_at_junction {
                        style.junction_left // ┤ (branch left on this row)
                    } else {
                        style.junction_right // ├ (no left branch on this row)
                    }
                }
                Direction::BT => style.junction_down,                 // ┬ (stem below, branches above)
            }
        } else if pos == span_start {
            // Corner at top/left end of span
            start_corner
        } else if pos == span_end {
            // Corner at bottom/right end of span
            end_corner
        } else {
            coords.secondary_edge_char(style)
        };
        canvas.set_edge_char(span_x, span_y, c, style);
    }

    // Connect the source column to the adjusted junction column if we nudged it.
    if junction_secondary != src_secondary {
        let (sx, sy) = coords.with_secondary(junction_x, junction_y, src_secondary);
        let (jx, jy) = coords.with_secondary(junction_x, junction_y, junction_secondary);
        draw_line_secondary(sx, sy, jx, jy, &coords, canvas, style, Some(graph));
    }

    // 4. Draw drops and arrows for each target
    for (target_x, target_y, target) in &target_positions {
        let target_secondary = coords.secondary_coord(*target_x, *target_y);
        let (arrow_x, arrow_y) = edge_entry_point(target, direction);

        // Draw vertical drop from junction+1 to arrow
        let (drop_x, drop_y) = coords.with_secondary(junction_x, junction_y, target_secondary);
        let (drop_start_x, drop_start_y) = coords.advance(drop_x, drop_y, 1);

        // Only draw if there's actually a drop to draw
        if drop_start_x != arrow_x || drop_start_y != arrow_y {
            draw_line_primary(
                drop_start_x,
                drop_start_y,
                arrow_x,
                arrow_y,
                &coords,
                canvas,
                style,
                Some(graph),
            );
        }

        // Arrow
        canvas.set(arrow_x, arrow_y, coords.arrow_end(style));
    }

    // Reinforce clean corners at the ends of the span so drops don’t turn them into tees.
    let (start_pos_x, start_pos_y) = coords.with_secondary(junction_x, junction_y, span_start);
    let (end_pos_x, end_pos_y) = coords.with_secondary(junction_x, junction_y, span_end);
    if span_start != junction_secondary {
        canvas.set(start_pos_x, start_pos_y, start_corner);
    }
    if span_end != junction_secondary {
        canvas.set(end_pos_x, end_pos_y, end_corner);
    }
}

fn route_fanout_into_subgraph_td(
    from: &Node,
    targets: &[(usize, usize, &Node)],
    canvas: &mut Canvas,
    style: &StyleChars,
    sg: &crate::graph::Subgraph,
    direction: Direction,
) {
    let coords = OrientedCoords::new(direction);
    let (stem_start_x, stem_start_y) = edge_exit_point(from, direction);

    let portal_center = sg.bounds.x + sg.bounds.width / 2;
    let min_target_x = targets.iter().map(|(x, _, _)| *x).min().unwrap_or(portal_center);
    let max_target_x = targets.iter().map(|(x, _, _)| *x).max().unwrap_or(portal_center);
    let junction_x = portal_center.clamp(min_target_x, max_target_x);

    let portal_y = sg.bounds.y.saturating_add(1);
    let min_arrow_y = targets
        .iter()
        .map(|(_, _, t)| edge_entry_point(t, direction).1)
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
            canvas.set_edge_char(x, stem_start_y, style.edge_h, style);
        }
        let corner = if junction_x > stem_start_x {
            style.corner_dr
        } else {
            style.corner_dl
        };
        canvas.set_edge_char(junction_x, stem_start_y, corner, style);
    } else {
        canvas.set_edge_char(
            junction_x,
            stem_start_y,
            coords.primary_edge_char(style),
            style,
        );
    }

    // Vertical spine down into the subgraph (including the portal row)
    if stem_start_y < junction_y {
        for y in (stem_start_y + 1)..=junction_y {
            canvas.set_edge_char(junction_x, y, coords.primary_edge_char(style), style);
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
        canvas.set_edge_char(junction_x, spine_y, coords.primary_edge_char(style), style);
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
        canvas.set_edge_char(span_x, span_y, c, style);
    }

    // Ensure the split junction reads as an upward tee (trunk enters from above).
    if matches!(direction, Direction::TD | Direction::TB) {
        canvas.set(junction_x, junction_y, style.junction_up);
    }

    for (target_x, target_y, target) in &sorted_targets {
        let target_secondary = coords.secondary_coord(*target_x, *target_y);
        let (arrow_x, arrow_y) = edge_entry_point(target, direction);
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
            );
        }

        canvas.set(arrow_x, arrow_y, coords.arrow_end(style));
    }
}

fn route_convergent_from_subgraph_td(
    sources: &[&Node],
    target: &Node,
    canvas: &mut Canvas,
    style: &StyleChars,
    sg: &crate::graph::Subgraph,
    direction: Direction,
) {
    let coords = OrientedCoords::new(direction);
    let (target_x, target_y) = get_node_center(target);
    let (arrow_x, arrow_y) = edge_entry_point(target, direction);

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
    let merge_x = match direction {
        Direction::TD | Direction::TB => target_x.clamp(
            sg.bounds.x.saturating_add(1),
            sg.bounds
                .x
                .saturating_add(sg.bounds.width.saturating_sub(2)),
        ),
        _ => sg.bounds.x + sg.bounds.width / 2,
    };

    let mut source_positions: Vec<(usize, usize, &Node)> = sources
        .iter()
        .map(|n| {
            let (sx, sy) = get_node_center(n);
            (sx, sy, *n)
        })
        .collect();
    source_positions.sort_by_key(|(x, y, _)| coords.secondary_coord(*x, *y));

    let target_secondary = coords.secondary_coord(target_x, target_y);
    let (span_start, span_end) = draw_source_lines_to_merge(
        &source_positions,
        merge_x,
        merge_y,
        &coords,
        canvas,
        style,
        direction,
    );

    let (final_span_start, final_span_end) = if matches!(direction, Direction::TD | Direction::TB)
    {
        (span_start, span_end)
    } else {
        (span_start.min(target_secondary), span_end.max(target_secondary))
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
    );

    // Adjust merge-bar ends for clarity near the subgraph exit.
    match direction {
        Direction::BT => {
            let (sx, sy) = coords.with_secondary(merge_x, merge_y, final_span_start);
            let (ex, ey) = coords.with_secondary(merge_x, merge_y, final_span_end);
            canvas.set(sx, sy, style.corner_ul);
            canvas.set(ex, ey, style.corner_ur);
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
                canvas.set(sx, sy, ch);
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
    canvas.set(merge_x, merge_y, junction_char);

    // Drop vertically out of the subgraph first, then fan horizontally if needed
    // (keeps the merge spine centered and avoids interior sideways runs).
    let (cursor_x, mut cursor_y) = coords.advance(merge_x, merge_y, 1);
    draw_line_primary(
        cursor_x,
        cursor_y,
        cursor_x,
        arrow_y,
        &coords,
        canvas,
        style,
        None,
    );
    cursor_y = arrow_y;

    if cursor_x != arrow_x {
        draw_line_secondary(
            cursor_x,
            cursor_y,
            arrow_x,
            cursor_y,
            &coords,
            canvas,
            style,
            None,
        );
    }

    // Clean up bottom border: keep only the center exit portal.
    let bottom_y = sg.bounds.y + sg.bounds.height.saturating_sub(1);
    if bottom_y < canvas.height {
        let border_fill = if sg.bounds.x + 1 < canvas.width {
            canvas.get(sg.bounds.x + 1, bottom_y)
        } else {
            coords.secondary_edge_char(style)
        };
        if std::env::var("DEBUG_FANIN").is_ok() {
            let portals: Vec<usize> = source_positions
                .iter()
                .map(|(sx, _, _)| coords.secondary_coord(*sx, bottom_y))
                .collect();
            eprintln!(
                "cleanup bottom_y={} fill='{}' portals={:?}",
                bottom_y, border_fill, portals
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
            canvas.set(merge_x, bottom_y, style.edge_v);
        }
    }

    canvas.set(arrow_x, arrow_y, coords.arrow_end(style));
}

fn route_convergent_from_subgraph_bt(
    sources: &[&Node],
    target: &Node,
    canvas: &mut Canvas,
    style: &StyleChars,
    sg: &crate::graph::Subgraph,
    direction: Direction,
) {
    if direction != Direction::BT || sources.is_empty() || !sg.bounds.is_valid() {
        return;
    }
    let coords = OrientedCoords::new(direction);
    let (arrow_x, arrow_y) = edge_entry_point(target, direction);

    let top_y = sg.bounds.y;
    let bottom_y = sg.bounds.y + sg.bounds.height.saturating_sub(1);
    let inside_top = top_y.saturating_add(1);

    // Merge near the top border (inside the subgraph) so we can exit cleanly through
    // the top portal without piercing the title span.
    let min_exit_y = sources
        .iter()
        .map(|n| edge_exit_point(n, direction).1)
        .min()
        .unwrap_or(inside_top.saturating_add(2));

    let mut merge_y = inside_top.saturating_add(1);
    merge_y = merge_y.min(bottom_y.saturating_sub(1)).max(inside_top);
    if merge_y > min_exit_y.saturating_sub(1) {
        merge_y = min_exit_y.saturating_sub(1).max(inside_top);
    }

    let merge_x = preferred_portal_x(&sg.bounds, sg.title.as_deref(), arrow_x, canvas);

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
    );

    draw_merge_line(merge_x, merge_y, span_start, span_end, &coords, canvas, style);

    if span_start < span_end {
        let (sx, sy) = coords.with_secondary(merge_x, merge_y, span_start);
        let (ex, ey) = coords.with_secondary(merge_x, merge_y, span_end);
        canvas.set_edge_char(sx, sy, style.corner_ul, style);
        canvas.set_edge_char(ex, ey, style.corner_ur, style);
    }
    canvas.set_edge_char(merge_x, merge_y, style.junction_up, style);

    let (final_start_x, final_start_y) = coords.advance(merge_x, merge_y, 1);
    draw_line_primary(
        final_start_x,
        final_start_y,
        arrow_x,
        arrow_y,
        &coords,
        canvas,
        style,
        None,
    );

    // Clean up the top border: keep only the merged exit portal, and restore any
    // other portal reinforcements that would otherwise clutter the title border.
    if top_y < canvas.height {
        let mut border_fill = if sg.bounds.x + 1 < canvas.width {
            canvas.get(sg.bounds.x + 1, top_y)
        } else {
            coords.secondary_edge_char(style)
        };
        if border_fill == ' ' || is_textual(border_fill) {
            border_fill = coords.secondary_edge_char(style);
        }
        for (sx, sy, _) in &source_positions {
            let sec = coords.secondary_coord(*sx, *sy);
            let (px, py) = coords.with_secondary(merge_x, top_y, sec);
            if px != merge_x && px < canvas.width && py < canvas.height {
                canvas.set(px, py, border_fill);
            }
        }
        if merge_x < canvas.width && !is_textual(canvas.get(merge_x, top_y)) {
            canvas.set(merge_x, top_y, style.edge_v);
        }
    }

    canvas.set(arrow_x, arrow_y, coords.arrow_end(style));
}

// Helper: Draw lines from sources to merge point (on primary axis)
fn draw_source_lines_to_merge(
    source_positions: &[(usize, usize, &Node)],
    merge_x: usize,
    merge_y: usize,
    coords: &OrientedCoords,
    canvas: &mut Canvas,
    style: &StyleChars,
    direction: Direction,
) -> (usize, usize) {
    let mut span_start = usize::MAX;
    let mut span_end = 0;

    for &(src_x, src_y, source) in source_positions {
        let (edge_x, edge_y) = edge_exit_point(source, direction);
        let src_secondary = coords.secondary_coord(src_x, src_y);

        // Update span bounds
        span_start = span_start.min(src_secondary);
        span_end = span_end.max(src_secondary);

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
                    canvas.set_edge_char(src_x, y, style.edge_v, style);
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
                    canvas.set_edge_char(x, src_y, style.edge_h, style);
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
                    canvas.set_edge_char(x, src_y, style.edge_h, style);
                }
            }
            Direction::BT => {
                // BT layout: draw vertical line between source top edge and merge row
                let (start, end) = if merge_col_y <= edge_y {
                    (merge_col_y + 1, edge_y + 1)
                } else {
                    (edge_y + 1, merge_col_y + 1)
                };
                for y in start..end {
                    canvas.set_edge_char(src_x, y, style.edge_v, style);
                }
            }
        }

        // Mark the exit point on the box border to make the junction explicit.
        match direction {
            Direction::LR => {
                let border_x = edge_x.saturating_sub(1);
                if border_x < canvas.width && src_y < canvas.height {
                    canvas.set_edge_char(border_x, src_y, style.junction_right, style);
                }
            }
            Direction::RL => {
                let border_x = edge_x.saturating_add(1);
                if border_x < canvas.width && src_y < canvas.height {
                    canvas.set_edge_char(border_x, src_y, style.junction_left, style);
                }
            }
            _ => {}
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
        canvas.set_edge_char(merge_col_x, merge_col_y, corner_char, style);
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
        // Topmost/leftmost position on span - edge from source turns down/right
        match direction {
            Direction::TD | Direction::TB => style.junction_right, // ├ - emphasize fan-in start
            Direction::LR => style.corner_dr,                 // ┐ - from left, turns down
            Direction::RL => style.corner_dl,                 // ┌ - from right, turns down
            Direction::BT => style.corner_dl,                 // ┌ - from below, turns right
        }
    } else if src_secondary == span_end {
        // Bottommost/rightmost position on span - edge from source turns up/left
        match direction {
            Direction::TD | Direction::TB => style.corner_dr, // ┘ - cleaner exit toward portal
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
fn draw_merge_line(
    merge_x: usize,
    merge_y: usize,
    span_start: usize,
    span_end: usize,
    coords: &OrientedCoords,
    canvas: &mut Canvas,
    style: &StyleChars,
) {
    for pos in span_start..=span_end {
        let (span_x, span_y) = coords.with_secondary(merge_x, merge_y, pos);
        let c = canvas.get(span_x, span_y);

        if c == ' ' || (is_secondary_line(c, coords, style) && !is_junction(c, style)) {
            canvas.set_edge_char(span_x, span_y, coords.secondary_edge_char(style), style);
        }
    }
}

/// Route edges from multiple sources to a single target (convergence)
pub fn route_convergent_edges(
    from_nodes: &[&Node],
    to: &Node,
    canvas: &mut Canvas,
    style: &StyleChars,
    direction: Direction,
    graph: &Graph,
) {
    if from_nodes.is_empty() || !canvas.is_visible(to) {
        return;
    }

    let coords = OrientedCoords::new(direction);
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
    let (arrow_x, arrow_y) = edge_entry_point(to, direction);
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
            let all_sources_same = visible_sources.iter().all(|n| {
                graph.get_node_subgraph(&n.id) == Some(source_sg_id)
            });

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
                        );
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
            let all_sources_same =
                visible_sources.iter().all(|n| graph.get_node_subgraph(&n.id) == Some(source_sg_id));

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
        Direction::LR | Direction::RL => STEM_LENGTH_HORIZONTAL,
        _ => STEM_LENGTH_VERTICAL,
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
    );

    // Expand span to include target if needed
    let final_span_start = actual_span_start.min(target_secondary);
    let final_span_end = actual_span_end.max(target_secondary);

    // Draw horizontal merge line
    draw_merge_line(
        merge_x,
        merge_y,
        final_span_start,
        final_span_end,
        &coords,
        canvas,
        style,
    );

    if matches!(direction, Direction::TD | Direction::TB) && final_span_start < final_span_end {
        let (sx, sy) = coords.with_secondary(merge_x, merge_y, final_span_start);
        let (ex, ey) = coords.with_secondary(merge_x, merge_y, final_span_end);
        canvas.set_edge_char(sx, sy, style.corner_ul, style);
        canvas.set_edge_char(ex, ey, style.corner_ur, style);
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
            canvas.set_edge_char(merge_x, merge_y_draw, junction_char, style);
        } else {
            for pos in final_span_start..=final_span_end {
                let (x, y) = coords.with_secondary(merge_x, merge_y, pos);
                let ch = if pos == coords.secondary_coord(merge_x, merge_y) {
                    junction_char
                } else {
                    coords.secondary_edge_char(style)
                };
                canvas.set_edge_char(x, y, ch, style);
            }
        }
    } else {
        canvas.set_edge_char(merge_x, merge_y, junction_char, style);
    }

    if matches!(direction, Direction::TD | Direction::TB) && final_span_start < final_span_end {
        let (sx, sy) = coords.with_secondary(merge_x, merge_y_draw, final_span_start);
        let (ex, ey) = coords.with_secondary(merge_x, merge_y_draw, final_span_end);
        canvas.set(sx, sy, style.corner_ul);
        canvas.set(ex, ey, style.corner_ur);
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
    );

    // Arrow
    canvas.set(arrow_x, arrow_y, coords.arrow_end(style));
}

// ============================================================================
// Helper Functions
// ============================================================================

fn get_node_center(node: &Node) -> (usize, usize) {
    (node.center_x(), node.center_y())
}

/// Where an incoming edge enters a target node (arrow position).
fn edge_entry_point(node: &Node, direction: Direction) -> (usize, usize) {
    match direction {
        Direction::TD | Direction::TB => (node.center_x(), node.y.saturating_sub(1)),
        Direction::LR => (node.x.saturating_sub(1), node.center_y()),
        Direction::RL => (node.x + node.width, node.center_y()),
        Direction::BT => (node.center_x(), node.bottom_y()),
    }
}

/// Where an outgoing edge exits a source node (stem start position).
fn edge_exit_point(node: &Node, direction: Direction) -> (usize, usize) {
    match direction {
        Direction::TD | Direction::TB => (node.center_x(), node.bottom_y()),
        Direction::LR => (node.x + node.width, node.center_y()),
        Direction::RL => (node.x.saturating_sub(1), node.center_y()),
        Direction::BT => (node.center_x(), node.y.saturating_sub(1)),
    }
}

fn draw_line_primary(
    x1: usize,
    y1: usize,
    x2: usize,
    y2: usize,
    coords: &OrientedCoords,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: Option<&Graph>,
) {
    let char = coords.primary_edge_char(style);

    match coords.primary {
        crate::orientation::Axis::Horizontal => {
            let (start, end) = if x1 < x2 { (x1, x2) } else { (x2, x1) };
            for x in start..=end {
                if let Some(g) = graph {
                    if is_subgraph_title_cell(g, x, y1) {
                        continue;
                    }
                }
                canvas.set_edge_char(x, y1, char, style);
            }
        }
        crate::orientation::Axis::Vertical => {
            let (start, end) = if y1 < y2 { (y1, y2) } else { (y2, y1) };
            for y in start..=end {
                if let Some(g) = graph {
                    if is_subgraph_title_cell(g, x1, y) {
                        continue;
                    }
                }
                canvas.set_edge_char(x1, y, char, style);
            }
        }
    }
}

fn draw_line_secondary(
    x1: usize,
    y1: usize,
    x2: usize,
    y2: usize,
    coords: &OrientedCoords,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: Option<&Graph>,
) {
    let char = coords.secondary_edge_char(style);

    match coords.secondary {
        crate::orientation::Axis::Horizontal => {
            let (start, end) = if x1 < x2 { (x1, x2) } else { (x2, x1) };
            for x in start..=end {
                if x != x1 && x != x2 {
                    // Skip corners
                    if let Some(g) = graph {
                        if is_subgraph_title_cell(g, x, y1) {
                            continue;
                        }
                    }
                    canvas.set_edge_char(x, y1, char, style);
                }
            }
        }
        crate::orientation::Axis::Vertical => {
            let (start, end) = if y1 < y2 { (y1, y2) } else { (y2, y1) };
            for y in start..=end {
                if y != y1 && y != y2 {
                    // Skip corners
                    if let Some(g) = graph {
                        if is_subgraph_title_cell(g, x1, y) {
                            continue;
                        }
                    }
                    canvas.set_edge_char(x1, y, char, style);
                }
            }
        }
    }
}

fn is_secondary_line(c: char, coords: &OrientedCoords, style: &StyleChars) -> bool {
    let expected = coords.secondary_edge_char(style);
    c == expected
}

fn is_subgraph_title_cell(graph: &Graph, x: usize, y: usize) -> bool {
    graph.subgraphs.iter().any(|sg| {
        sg.has_title()
            && sg.bounds.is_valid()
            && y == sg.bounds.y
            && x >= sg.bounds.x
            && x < sg.bounds.x.saturating_add(sg.bounds.width)
    })
}

fn preferred_portal_x(
    bounds: &crate::graph::Rectangle,
    title: Option<&str>,
    desired: usize,
    canvas: &Canvas,
) -> usize {
    let min = bounds.x.saturating_add(1);
    let max = bounds.x + bounds.width.saturating_sub(2);
    let _ = canvas;
    let mut x = desired.clamp(min, max);

    let Some(t) = title else {
        return x;
    };
    let title_fmt = format!("[  {}  ]", t);
    let len = title_fmt.chars().count();
    if len == 0 || len > bounds.width.saturating_sub(2) {
        return x;
    }
    let start = bounds.x + bounds.width.saturating_sub(len) / 2;
    let end = start + len.saturating_sub(1);
    if x < start || x > end {
        return x;
    }
    if end + 1 <= max {
        x = end + 1;
    } else if start > min {
        x = start.saturating_sub(1);
    }
    x
}

#[allow(dead_code)]
fn route_cross_subgraph_td(
    from: &Node,
    to: &Node,
    stem_start_x: usize,
    stem_start_y: usize,
    arrow_x: usize,
    arrow_y: usize,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: &Graph,
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
    let entering_from_above = stem_start_y < sg.bounds.y && arrow_y >= sg.bounds.y.saturating_add(1);
    if entering_from_above {
        let min_x = sg.bounds.x.saturating_add(1);
        let max_x = sg.bounds.x + sg.bounds.width.saturating_sub(2);
        let portal_x = arrow_x.clamp(min_x, max_x);

        let outside_y = sg.bounds.y.saturating_sub(1);
        let inside_y = sg.bounds.y.saturating_add(1);

        if stem_start_y <= outside_y {
            for y in stem_start_y..=outside_y {
                canvas.set_edge_char(stem_start_x, y, style.edge_v, style);
            }
        }

        // If we need to shift columns to enter within the subgraph bounds, do it
        // immediately above the border.
        if portal_x != stem_start_x && outside_y < canvas.height {
            let start_corner = if portal_x > stem_start_x {
                style.corner_ul
            } else {
                style.corner_ur
            };
            canvas.set_edge_char(stem_start_x, outside_y, start_corner, style);

            let (hx0, hx1) = if portal_x > stem_start_x {
                (stem_start_x + 1, portal_x.saturating_sub(1))
            } else {
                (portal_x + 1, stem_start_x.saturating_sub(1))
            };
            for x in hx0..=hx1 {
                canvas.set_edge_char(x, outside_y, style.edge_h, style);
            }

            let end_corner = if portal_x > stem_start_x {
                style.corner_dr
            } else {
                style.corner_dl
            };
            canvas.set_edge_char(portal_x, outside_y, end_corner, style);
        }

        if arrow_y >= inside_y && inside_y < canvas.height {
            for y in inside_y..=arrow_y {
                canvas.set_edge_char(portal_x, y, style.edge_v, style);
            }
        }

        if debug_timing {
            eprintln!(
                "  cross-subgraph enter-under-title {} -> {} at x={} border_y={}",
                from.id, to.id, portal_x, sg.bounds.y
            );
        }

        return true;
    }

    // Enter at the subgraph portal (bias toward the target center, away from title text).
    let mut portal_x = preferred_portal_x(&sg.bounds, sg.title.as_deref(), arrow_x, canvas);

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
                canvas.set_edge_char(cursor_x, y, style.edge_v, style);
            }
            cursor_y = exit_y;
            portal_x = preferred_portal_x(&sg.bounds, sg.title.as_deref(), arrow_x, canvas);
        }
    }

    let portal_y = arrow_y
        .saturating_sub(1)
        .max(sg.bounds.y.saturating_add(1))
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
        canvas.set_edge_char(cursor_x, cursor_y, start_corner, style);

        let (hx0, hx1) = if portal_x > cursor_x {
            (cursor_x + 1, portal_x.saturating_sub(1))
        } else {
            (portal_x + 1, cursor_x.saturating_sub(1))
        };
        for x in hx0..=hx1 {
            if is_subgraph_title_cell(graph, x, cursor_y) {
                continue;
            }
            canvas.set_edge_char(x, cursor_y, style.edge_h, style);
        }

        let end_corner = if portal_x > cursor_x {
            style.corner_dr
        } else {
            style.corner_dl
        };
        canvas.set_edge_char(portal_x, cursor_y, end_corner, style);
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
            canvas.set_edge_char(portal_x, y, style.edge_v, style);
        }
    }

    // Bridge to the arrow column if needed.
    if portal_x != arrow_x {
        let corner = if portal_x < arrow_x {
            style.corner_ul
        } else {
            style.corner_ur
        };
        canvas.set_edge_char(portal_x, arrow_y, corner, style);

        let (hx0, hx1) = if portal_x < arrow_x {
            (portal_x + 1, arrow_x)
        } else {
            (arrow_x, portal_x.saturating_sub(1))
        };
        for x in hx0..=hx1 {
            if is_subgraph_title_cell(graph, x, arrow_y) {
                continue;
            }
            canvas.set_edge_char(x, arrow_y, style.edge_h, style);
        }
    } else if arrow_y > portal_y {
        for y in (portal_y + 1)..=arrow_y {
            if is_subgraph_title_cell(graph, portal_x, y) {
                continue;
            }
            canvas.set_edge_char(portal_x, y, style.edge_v, style);
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
                canvas.set_edge_char(cursor_x, border_y, style.edge_v, style);
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
        canvas.set_edge_char(portal_x, tgt_border_y, style.edge_v, style);
    }

    true
}

fn route_cross_subgraph_bt(
    from: &Node,
    to: &Node,
    stem_start_x: usize,
    stem_start_y: usize,
    arrow_x: usize,
    arrow_y: usize,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: &Graph,
) -> bool {
    let from_sg = graph.get_node_subgraph(&from.id);
    let to_sg = graph.get_node_subgraph(&to.id);
    if from_sg == to_sg {
        return false;
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

    let coords = OrientedCoords::new(Direction::BT);
    let border_y = src_sg.bounds.y;
    let inside_y = border_y.saturating_add(1);
    let portal_x = preferred_portal_x(
        &src_sg.bounds,
        src_sg.title.as_deref(),
        stem_start_x,
        canvas,
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
    );

    // Shift horizontally inside the subgraph to avoid piercing the title span.
    if portal_x != stem_start_x {
        let start_corner = if portal_x > stem_start_x {
            style.corner_ul
        } else {
            style.corner_ur
        };
        canvas.set_edge_char(stem_start_x, inside_y, start_corner, style);

        let (hx0, hx1) = if portal_x > stem_start_x {
            (stem_start_x + 1, portal_x.saturating_sub(1))
        } else {
            (portal_x + 1, stem_start_x.saturating_sub(1))
        };
        for x in hx0..=hx1 {
            canvas.set_edge_char(x, inside_y, style.edge_h, style);
        }

        let end_corner = if portal_x > stem_start_x {
            style.corner_dr
        } else {
            style.corner_dl
        };
        canvas.set_edge_char(portal_x, inside_y, end_corner, style);
    }

    // Continue up across the border. Prefer bridging back toward the target column
    // immediately outside the border so the final approach remains vertical.
    let border_row_y = inside_y.saturating_sub(1);
    let outside_y = border_y.saturating_sub(1);
    draw_line_primary(
        portal_x,
        border_row_y,
        portal_x,
        outside_y,
        &coords,
        canvas,
        style,
        Some(graph),
    );

    if portal_x != arrow_x && border_y > 0 {
        let start_corner = if arrow_x > portal_x {
            style.corner_ul
        } else {
            style.corner_ur
        };
        canvas.set_edge_char(portal_x, outside_y, start_corner, style);

        let (hx0, hx1) = if arrow_x > portal_x {
            (portal_x + 1, arrow_x.saturating_sub(1))
        } else {
            (arrow_x + 1, portal_x.saturating_sub(1))
        };
        for x in hx0..=hx1 {
            canvas.set_edge_char(x, outside_y, style.edge_h, style);
        }

        let end_corner = if arrow_x > portal_x {
            style.corner_dr
        } else {
            style.corner_dl
        };
        canvas.set_edge_char(arrow_x, outside_y, end_corner, style);

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
        );
    } else {
        draw_line_primary(
            portal_x,
            outside_y,
            portal_x,
            arrow_y,
            &coords,
            canvas,
            style,
            Some(graph),
        );
        if portal_x != arrow_x {
            // Fallback: if we have no room above the border, bridge on the arrow row.
            let corner = if portal_x < arrow_x {
                style.corner_ul
            } else {
                style.corner_ur
            };
            canvas.set_edge_char(portal_x, arrow_y, corner, style);
            let (hx0, hx1) = if portal_x < arrow_x {
                (portal_x + 1, arrow_x)
            } else {
                (arrow_x, portal_x.saturating_sub(1))
            };
            for x in hx0..=hx1 {
                canvas.set_edge_char(x, arrow_y, style.edge_h, style);
            }
        }
    }

    // Ensure the top border reads as a clean pierce (not a junction).
    if portal_x < canvas.width && border_y < canvas.height && !is_textual(canvas.get(portal_x, border_y))
    {
        canvas.set(portal_x, border_y, style.edge_v);
    }

    true
}

fn route_divergent_into_subgraph_td(
    source: &Node,
    targets: &[&Node],
    canvas: &mut Canvas,
    style: &StyleChars,
    sg: &crate::graph::Subgraph,
    direction: Direction,
) {
    if targets.is_empty() || !sg.bounds.is_valid() {
        return;
    }
    let coords = OrientedCoords::new(direction);
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
    canvas.set_edge_char(stem_x, stem_y, coords.primary_edge_char(style), style);

    // Walk vertically down to just above the border, then (optionally) shift horizontally.
    // This avoids drawing through the title row.
    let turn_y = if stem_y < outside_y { outside_y } else { stem_y };
    if stem_y + 1 <= outside_y {
        for y in (stem_y + 1)..=outside_y {
            canvas.set_edge_char(stem_x, y, coords.primary_edge_char(style), style);
        }
    }
    if entry_x != stem_x && turn_y < canvas.height {
        let start_corner = if entry_x > stem_x {
            style.corner_ul
        } else {
            style.corner_ur
        };
        canvas.set_edge_char(stem_x, turn_y, start_corner, style);

        let (hx0, hx1) = if entry_x > stem_x {
            (stem_x.saturating_add(1), entry_x.saturating_sub(1))
        } else {
            (entry_x.saturating_add(1), stem_x.saturating_sub(1))
        };
        for x in hx0..=hx1 {
            canvas.set_edge_char(x, turn_y, style.edge_h, style);
        }

        let end_corner = if entry_x > stem_x {
            style.corner_dr
        } else {
            style.corner_dl
        };
        canvas.set_edge_char(entry_x, turn_y, end_corner, style);
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
        .map(|n| edge_entry_point(n, direction).1)
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
        canvas.set_edge_char(entry_x, spine_y, coords.primary_edge_char(style), style);
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
                canvas.set_edge_char(entry_x, y, coords.primary_edge_char(style), style);
            }
        }
    }

    // Branch row: horizontal bar with a center tee.
    for x in min_x..=max_x {
        canvas.set_edge_char(x, branch_y, style.edge_h, style);
    }
    canvas.set(min_x, branch_y, style.corner_dl);
    canvas.set(max_x, branch_y, style.corner_dr);
    // The trunk enters from above; keep the branch row as a "cap" (no down stroke)
    // so drops start on the next row.
    canvas.set(entry_x, branch_y, style.junction_up);

    // Drop to targets starting immediately after the branch row.
    for (tx, _, target) in target_positions {
        let (arrow_x, arrow_y) = edge_entry_point(target, direction);
        let start_y = branch_y.saturating_add(1);
        for y in start_y..arrow_y {
            canvas.set_edge_char(tx, y, style.edge_v, style);
        }
        canvas.set(arrow_x, arrow_y, coords.arrow_end(style));
    }
}
