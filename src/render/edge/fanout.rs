//! Fan-out routing into a shared subgraph.

use crate::graph::{Direction, EdgeKind, Graph, Node};
use crate::orientation::{is_before, OrientedCoords};
use crate::spacing::SpacingConfig;
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::edge_policy::title_safe_td_entry_x;
use super::super::semantic::CellOwnerKind;
use super::edge_primitives::{
    adjusted_edge_entry_point, draw_line_primary, draw_line_secondary, edge_exit_point,
    get_node_center,
};
use super::subgraph::{
    route_cross_subgraph_bt, route_cross_subgraph_td, route_divergent_into_subgraph_bt,
    route_divergent_into_subgraph_td,
};
use super::{
    edge_route_owner_id, set_route_char, set_route_edge_char, style_for_edge_kind, RouteOwner,
};

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

    if crate::runtime::current().diagnostics.fan_out {
        let target_xs: Vec<usize> = targets.iter().map(|(x, _, _)| *x).collect();
        eprintln!(
            "fanout stem=({stem_start_x}, {stem_start_y}) portal_y={portal_y} jx={junction_x} targets={target_xs:?}"
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

/// Route edges from a single source to multiple targets (divergence)
/// Works for all orientations using the abstraction layer
pub fn route_divergent_edges(
    from: &Node,
    to_nodes: &[&Node],
    canvas: &mut Canvas,
    style: &StyleChars,
    spacing: &SpacingConfig,
    direction: Direction,
    graph: &Graph,
) {
    if to_nodes.is_empty() || !canvas.is_visible(from) {
        return;
    }

    let coords = OrientedCoords::new(direction);
    let context = crate::runtime::current();
    let debug_timing = context.diagnostics.timing;
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
        Direction::LR | Direction::RL => spacing.stem_length_horizontal,
        _ => spacing.stem_length_vertical,
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
    if matches!(direction, Direction::TD | Direction::TB | Direction::BT)
        && target_positions.len() > 1
    {
        if let Some(target_sg) = visible_targets
            .first()
            .and_then(|n| graph.get_node_subgraph(&n.id))
        {
            let all_same = visible_targets
                .iter()
                .all(|n| graph.get_node_subgraph(&n.id) == Some(target_sg));
            let source_sg = graph.get_node_subgraph(&from.id);
            if all_same && source_sg != Some(target_sg) {
                if let Some(sg) = graph.get_subgraph(target_sg) {
                    match direction {
                        Direction::TD | Direction::TB => route_divergent_into_subgraph_td(
                            from,
                            &visible_targets,
                            canvas,
                            style,
                            sg,
                            direction,
                            graph,
                        ),
                        Direction::BT => route_divergent_into_subgraph_bt(
                            from,
                            &visible_targets,
                            canvas,
                            style,
                            sg,
                            graph,
                        ),
                        _ => unreachable!(),
                    }
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
                let (ax, ay) = adjusted_edge_entry_point(n, direction, graph);
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
                Direction::LR => desired_primary
                    .min(current_primary)
                    .max(stem_start_primary + 1),
                Direction::RL => desired_primary
                    .max(current_primary)
                    .min(stem_start_primary.saturating_sub(1)),
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
                let (ax, ay) = adjusted_edge_entry_point(n, direction, graph);
                coords.primary_coord(ax, ay)
            })
            .min_by_key(|p| junction_primary.abs_diff(*p));
        if let Some(arrow_primary) = nearest_arrow_primary {
            let gap = junction_primary.abs_diff(arrow_primary);
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
                        adjusted_primary = adjusted_primary
                            .min(stem_start_primary.saturating_sub(1).max(adjusted_primary));
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
        let route_owner_id = edge_route_owner_id(graph, &from.id, &target.id);
        let route_owner = RouteOwner {
            kind: CellOwnerKind::EdgeSegment,
            id: route_owner_id.as_str(),
        };

        let (arrow_x, arrow_y) = adjusted_edge_entry_point(target, direction, graph);

        if matches!(direction, Direction::TD | Direction::TB) {
            let from_sg = graph.get_node_subgraph(&from.id);
            let to_sg = graph.get_node_subgraph(&target.id);
            let cross_arrow_x = if from_sg != to_sg {
                title_safe_td_entry_x(target, arrow_x, arrow_y, stem_start_y, graph)
            } else {
                arrow_x
            };
            if context.diagnostics.cross {
                eprintln!(
                    "single-edge cross? {}({:?}) -> {}({:?})",
                    from.id, from_sg, target.id, to_sg
                );
            }
            if from_sg != to_sg
                && route_cross_subgraph_td(
                    from,
                    target,
                    stem_start_x,
                    stem_start_y,
                    cross_arrow_x,
                    arrow_y,
                    canvas,
                    style,
                    graph,
                    Some(route_owner),
                )
            {
                set_route_char(
                    canvas,
                    cross_arrow_x,
                    arrow_y,
                    coords.arrow_end(style),
                    Some(route_owner),
                );
                return;
            }
        } else if direction == Direction::BT {
            let from_sg = graph.get_node_subgraph(&from.id);
            let to_sg = graph.get_node_subgraph(&target.id);
            if from_sg != to_sg
                && route_cross_subgraph_bt(
                    from,
                    target,
                    stem_start_x,
                    stem_start_y,
                    arrow_x,
                    arrow_y,
                    canvas,
                    style,
                    graph,
                    Some(route_owner),
                )
            {
                set_route_char(
                    canvas,
                    arrow_x,
                    arrow_y,
                    coords.arrow_end(style),
                    Some(route_owner),
                );
                return;
            }
        }

        if debug_timing {
            eprintln!("  single target centers ({src_x},{src_y}) -> ({arrow_x},{arrow_y})");
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
                Some(route_owner),
            );
            if matches!(direction, Direction::TD | Direction::TB) {
                if let (Some(from_sg), Some(to_sg)) = (
                    graph.get_node_subgraph(&from.id),
                    graph.get_node_subgraph(&target.id),
                ) {
                    if from_sg != to_sg {
                        if let Some(sg) = graph.get_subgraph(to_sg) {
                            let border_y = sg.bounds.y;
                            if arrow_x < canvas.width && border_y < canvas.height && !sg.has_title()
                            {
                                set_route_edge_char(
                                    canvas,
                                    arrow_x,
                                    border_y,
                                    style.junction_down,
                                    style,
                                    Some(route_owner),
                                );
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
                        None,
                    );

                    // Turn onto the vertical spine at the source row.
                    let going_up = target_secondary < src_secondary;
                    let corner1 = match direction {
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
                        spine_x,
                        stem_start_y,
                        corner1,
                        style,
                        Some(route_owner),
                    );

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
                        Some(route_owner),
                    );

                    // Turn toward the target column.
                    let corner2 = match direction {
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
                    set_route_edge_char(canvas, bend_x, bend_y, corner2, style, Some(route_owner));

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
                        Some(route_owner),
                    );
                    set_route_char(
                        canvas,
                        arrow_x,
                        arrow_y,
                        coords.arrow_end(style),
                        Some(route_owner),
                    );
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
                    Some(route_owner),
                );

                // Turn toward the target column
                let corner = coords.corner_secondary_to_end(going_before, style);
                set_route_edge_char(canvas, bend_x, bend_y, corner, style, Some(route_owner));

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
                    Some(route_owner),
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
                    let junction_conflicts =
                        junction_cell != ' ' && !super::canvas::is_vertical(junction_cell, style);
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
                    Some(route_owner),
                );

                // 2. Turn at junction
                let corner = coords.corner_start_to_secondary(going_before, style);
                set_route_edge_char(
                    canvas,
                    junction_x,
                    junction_y,
                    corner,
                    style,
                    Some(route_owner),
                );

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
                    Some(route_owner),
                );

                // 4. Turn to target
                let corner2 = coords.corner_secondary_to_end(going_before, style);
                set_route_edge_char(canvas, bend_x, bend_y, corner2, style, Some(route_owner));

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
                    Some(route_owner),
                );
            }
        }

        // Arrow at target
        set_route_char(
            canvas,
            arrow_x,
            arrow_y,
            coords.arrow_end(style),
            Some(route_owner),
        );

        // If the edge exits a subgraph boundary (TD/TB), stamp a junction on the border
        // so the vertical line visually pierces the container instead of leaving a gap.
        if matches!(direction, Direction::TD | Direction::TB) {
            if let Some(from_sg) = graph.get_node_subgraph(&from.id) {
                if graph.get_node_subgraph(&target.id) != Some(from_sg) {
                    if let Some(sg) = graph.get_subgraph(from_sg) {
                        let border_y = sg.bounds.y + sg.bounds.height.saturating_sub(1);
                        if arrow_x < canvas.width && border_y < canvas.height {
                            set_route_edge_char(
                                canvas,
                                arrow_x,
                                border_y,
                                style.junction_down,
                                style,
                                Some(route_owner),
                            );
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
            let all_targets_same_sg = target_positions
                .iter()
                .all(|(_, _, n)| graph.get_node_subgraph(&n.id) == Some(target_sg_id));

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
                            graph,
                        );
                        return;
                    }
                }
            }
        }
    }

    // Multiple targets: draw branching structure
    let fanout_owner_id = format!("fanout:{}", from.id);
    let fanout_owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: fanout_owner_id.as_str(),
    };

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
        set_route_edge_char(
            canvas,
            px,
            py,
            coords.primary_edge_char(style),
            style,
            Some(fanout_owner),
        );
    }

    // 2. Calculate span on secondary axis
    let src_secondary = coords.secondary_coord(src_x, src_y);
    let target_secondaries: Vec<usize> = target_positions
        .iter()
        .map(|(_, _, target)| {
            let (arrow_x, arrow_y) = adjusted_edge_entry_point(target, direction, graph);
            coords.secondary_coord(arrow_x, arrow_y)
        })
        .collect();
    let first_secondary = target_secondaries
        .iter()
        .copied()
        .min()
        .unwrap_or(src_secondary);
    let last_secondary = target_secondaries
        .iter()
        .copied()
        .max()
        .unwrap_or(src_secondary);

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

    // Collect target drop positions for junction placement
    for pos in span_start..=span_end {
        let (span_x, span_y) = coords.with_secondary(junction_x, junction_y, pos);

        // Check if this position is a target drop point (not the source junction)
        let is_target_drop = pos != junction_secondary && target_secondaries.contains(&pos);

        let c = if pos == junction_secondary {
            // Junction at source position - stem meets vertical span
            match direction {
                Direction::TD | Direction::TB => style.junction_up, // ┴
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
                Direction::BT => style.junction_down, // ┬ (stem below, branches above)
            }
        } else if pos == span_start {
            // Corner at start of span - corners already have correct shape for drops
            // e.g., ┌ (corner_dl) connects right and down, which is exactly what we need
            start_corner
        } else if pos == span_end {
            // Corner at end of span - corners already have correct shape for drops
            // e.g., ┐ (corner_dr) connects left and down, which is exactly what we need
            end_corner
        } else if is_target_drop {
            // Target drop in middle of span - needs T-junction
            match direction {
                Direction::TD | Direction::TB => style.junction_down, // ┬ (horizontal span, drop below)
                Direction::BT => style.junction_up, // ┴ (horizontal span, drop above)
                Direction::LR => style.junction_right, // ├ (vertical span, drop right)
                Direction::RL => style.junction_left, // ┤ (vertical span, drop left)
            }
        } else {
            coords.secondary_edge_char(style)
        };
        set_route_edge_char(canvas, span_x, span_y, c, style, Some(fanout_owner));
    }

    // Connect the source column to the adjusted junction column if we nudged it.
    if junction_secondary != src_secondary {
        let (sx, sy) = coords.with_secondary(junction_x, junction_y, src_secondary);
        let (jx, jy) = coords.with_secondary(junction_x, junction_y, junction_secondary);
        draw_line_secondary(
            sx,
            sy,
            jx,
            jy,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(fanout_owner),
        );
    }

    // 4. Draw drops and arrows for each target
    for (_, _, target) in &target_positions {
        let branch_owner_id = edge_route_owner_id(graph, &from.id, &target.id);
        let branch_owner = RouteOwner {
            kind: CellOwnerKind::EdgeSegment,
            id: branch_owner_id.as_str(),
        };
        let edge_kind = graph
            .edges
            .iter()
            .find(|e| e.from == from.id && e.to == target.id && !e.is_back_edge)
            .map(|e| e.kind)
            .unwrap_or(EdgeKind::Arrow);
        let branch_style = style_for_edge_kind(style, edge_kind);
        let (arrow_x, arrow_y) = adjusted_edge_entry_point(target, direction, graph);
        let target_secondary = coords.secondary_coord(arrow_x, arrow_y);

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
                &branch_style,
                Some(graph),
                Some(branch_owner),
            );
        }

        // Tip: use edge-kind-specific character (circle/cross end markers, etc.)
        let tip = match edge_kind {
            EdgeKind::CircleEnd => style.circle_end,
            EdgeKind::CrossEnd => style.cross_end,
            EdgeKind::Open => coords.primary_edge_char(&branch_style), // no arrowhead
            _ => coords.arrow_end(style),
        };
        set_route_char(canvas, arrow_x, arrow_y, tip, Some(branch_owner));

        // A custom branch shaft can otherwise downgrade the shared fan-out
        // junction while overlap resolution sees only the branch style. Restore
        // the base-style tee after drawing the branch so edge-kind emphasis does
        // not erase topology at the merge point.
        let (junction_cell_x, junction_cell_y) =
            coords.with_secondary(junction_x, junction_y, target_secondary);
        let junction = match direction {
            Direction::TD | Direction::TB => style.junction_down,
            Direction::BT => style.junction_up,
            Direction::LR => style.junction_right,
            Direction::RL => style.junction_left,
        };
        set_route_char(
            canvas,
            junction_cell_x,
            junction_cell_y,
            junction,
            Some(fanout_owner),
        );
    }

    // Reinforce clean corners at the ends of the span so drops don't turn them into tees.
    // Only override if the existing character is a primary edge (vertical/horizontal line
    // from our drops) - if it's a subgraph border or other structure, use normal overlap
    // resolution to create proper junctions.
    let (start_pos_x, start_pos_y) = coords.with_secondary(junction_x, junction_y, span_start);
    let (end_pos_x, end_pos_y) = coords.with_secondary(junction_x, junction_y, span_end);
    let primary_edge = coords.primary_edge_char(style);

    if span_start != junction_secondary {
        let existing = canvas.get(start_pos_x, start_pos_y);
        if existing == primary_edge || existing == ' ' {
            set_route_char(
                canvas,
                start_pos_x,
                start_pos_y,
                start_corner,
                Some(fanout_owner),
            );
        } else {
            set_route_edge_char(
                canvas,
                start_pos_x,
                start_pos_y,
                start_corner,
                style,
                Some(fanout_owner),
            );
        }
    }
    if span_end != junction_secondary {
        let existing = canvas.get(end_pos_x, end_pos_y);
        if existing == primary_edge || existing == ' ' {
            set_route_char(canvas, end_pos_x, end_pos_y, end_corner, Some(fanout_owner));
        } else {
            set_route_edge_char(
                canvas,
                end_pos_x,
                end_pos_y,
                end_corner,
                style,
                Some(fanout_owner),
            );
        }
    }
}
