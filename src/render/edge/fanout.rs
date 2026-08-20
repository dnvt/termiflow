//! Fan-out routing into a shared subgraph.

use crate::graph::{Direction, EdgeKind, Graph, Node};
use crate::orientation::{is_before, OrientedCoords};
use crate::spacing::SpacingConfig;
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::edge_policy::{
    td_single_incoming_route_x, td_single_outgoing_route_x, title_safe_td_entry_x,
};
use super::super::fallback_route::FallbackRoutePlan;
use super::super::semantic::{CellOwnerKind, CellRole};
use super::edge_primitives::{
    adjusted_edge_entry_point, draw_line_primary, draw_line_secondary, edge_exit_point,
    get_node_center,
};
use super::subgraph::{
    route_cross_subgraph_bt, route_cross_subgraph_td, route_divergent_into_subgraph_bt,
    route_divergent_into_subgraph_horizontal, route_divergent_into_subgraph_td, BtRouteOutcome,
};
use super::{
    edge_route_owner_id, set_route_char, set_route_edge_char, set_route_endpoint_char,
    style_for_edge_kind, RouteOwner,
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

    // A source can contribute to a downstream fan-in while also branching to
    // another target.  Convergence is rendered before this pass, so keep the
    // later LR/RL spine away from an already-owned fan-in junction when the
    // complete measured candidate route has room.  This is deliberately
    // topology-gated; ordinary single-target L-routes retain their existing
    // geometry.
    if target_positions.len() == 1 && matches!(direction, Direction::LR | Direction::RL) {
        let target = target_positions[0].2;
        let request = MixedBranchRequest {
            from,
            target,
            stem_start: (stem_start_x, stem_start_y),
            junction: (junction_x, junction_y),
            canvas,
            direction,
            graph,
        };
        if let Some((candidate_x, candidate_y)) = mixed_branch_junction(request) {
            junction_x = candidate_x;
            junction_y = candidate_y;
        }
    }

    // A mixed fan-out needs one writable target-facing shaft cell for Thick and
    // Dotted branches.  With the compact vertical layout the shared span can
    // otherwise land immediately beside the target arrow row, leaving those
    // edge kinds with no branch cell to carry their kind-specific shaft glyph.
    // Move only this topology family one cell toward the source when the
    // candidate corridor is blank; ordinary fan-outs retain their geometry.
    if target_positions.len() > 1
        && matches!(direction, Direction::TD | Direction::TB | Direction::BT)
        && target_positions.iter().any(|(_, _, target)| {
            graph
                .edges
                .iter()
                .find(|edge| !edge.is_back_edge && edge.from == from.id && edge.to == target.id)
                .is_some_and(|edge| matches!(edge.kind, EdgeKind::Thick | EdgeKind::Dotted))
        })
    {
        if let Some((candidate_x, candidate_y)) = mixed_vertical_fanout_clearance(
            &target_positions,
            junction_x,
            junction_y,
            stem_start_x,
            stem_start_y,
            direction,
            graph,
            canvas,
        ) {
            junction_x = candidate_x;
            junction_y = candidate_y;
        }
    }

    // Single target: direct route
    if target_positions.len() == 1 {
        let (mut target_x, target_y, target) = (
            target_positions[0].0,
            target_positions[0].1,
            target_positions[0].2,
        );
        let route_owner_id = edge_route_owner_id(graph, &from.id, &target.id);
        let route_owner = RouteOwner {
            kind: CellOwnerKind::EdgeSegment,
            id: route_owner_id.as_str(),
        };

        let (mut arrow_x, arrow_y) = adjusted_edge_entry_point(target, direction, graph);

        if matches!(direction, Direction::TD | Direction::TB) {
            let from_sg = graph.get_node_subgraph(&from.id);
            let to_sg = graph.get_node_subgraph(&target.id);
            if from_sg.is_some() && to_sg.is_none() {
                if let Some(transaction_x) =
                    td_single_outgoing_route_x(from, target, stem_start_x, arrow_y, graph)
                {
                    arrow_x = transaction_x;
                    target_x = transaction_x;
                }
            }
            let cross_arrow_x = if from_sg != to_sg {
                td_single_incoming_route_x(from, target, arrow_x, arrow_y, graph).unwrap_or_else(
                    || title_safe_td_entry_x(target, arrow_x, arrow_y, stem_start_y, graph),
                )
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
            if from_sg != to_sg {
                match route_cross_subgraph_bt(
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
                ) {
                    BtRouteOutcome::Handled | BtRouteOutcome::Rejected => {
                        set_route_char(
                            canvas,
                            arrow_x,
                            arrow_y,
                            coords.arrow_end(style),
                            Some(route_owner),
                        );
                        return;
                    }
                    BtRouteOutcome::NotApplicable => {}
                }
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

    // Horizontal fan-out into one subgraph is one boundary scene: enter through
    // a single owned portal, keep a collector rail inside the boundary, and
    // branch to each target from that rail.  The generic fan-out below routes
    // each target independently and can leave the off-axis branches as
    // detached stubs when the source is outside a titled subgraph.
    if matches!(direction, Direction::LR | Direction::RL) && target_positions.len() > 1 {
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
                    if route_divergent_into_subgraph_horizontal(
                        from,
                        &visible_targets,
                        canvas,
                        style,
                        direction,
                        sg,
                        graph,
                    ) {
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
        if matches!(edge_kind, EdgeKind::CircleEnd | EdgeKind::CrossEnd) {
            set_route_endpoint_char(canvas, arrow_x, arrow_y, tip, branch_owner);
        } else {
            set_route_char(canvas, arrow_x, arrow_y, tip, Some(branch_owner));
        }

        // A custom branch shaft can otherwise downgrade the shared fan-out
        // junction while overlap resolution sees only the branch style. Restore
        // the base-style tee after drawing the branch so edge-kind emphasis does
        // not erase topology at the merge point.
        let (junction_cell_x, junction_cell_y) =
            coords.with_secondary(junction_x, junction_y, target_secondary);
        // A drop at either end of the span is an elbow, not a tee.  The
        // branch still terminates at the target arrow, but there is no arm
        // continuing past the endpoint.  Keeping the endpoint corner here
        // prevents the later branch write from turning a truthful `┌`/`┐`
        // (or its ASCII equivalent) into an ambiguous three-way junction.
        let junction = if target_secondary == span_start && span_start != junction_secondary {
            start_corner
        } else if target_secondary == span_end && span_end != junction_secondary {
            end_corner
        } else {
            match direction {
                Direction::TD | Direction::TB => style.junction_down,
                Direction::BT => style.junction_up,
                Direction::LR => style.junction_right,
                Direction::RL => style.junction_left,
            }
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

    // The generic horizontal fanout is still a renderer-owned fallback route,
    // even though its visible geometry is assembled from shared and branch
    // owners above. Record the exact primitive coverage after lowering so the
    // final evidence can validate every edge against the post-repair canvas.
    // This is intentionally limited to LR/RL here; vertical fallback scenes
    // have their own specialized route-plan seams.
    let mut edge_kinds = Vec::new();
    for (_, _, target) in &target_positions {
        if let Some(edge) = graph
            .edges
            .iter()
            .find(|edge| !edge.is_back_edge && edge.from == from.id && edge.to == target.id)
        {
            if !edge_kinds.contains(&edge.kind) {
                edge_kinds.push(edge.kind);
            }
        }
    }
    let can_record_fallback_plan = edge_kinds.len() >= 2
        && graph.subgraphs.is_empty()
        && matches!(direction, Direction::LR | Direction::RL)
        && target_positions.iter().all(|(_, _, target)| {
            graph
                .edges
                .iter()
                .find(|edge| !edge.is_back_edge && edge.from == from.id && edge.to == target.id)
                .is_some_and(|edge| edge.label.is_none())
        });
    if can_record_fallback_plan {
        let mut plan = FallbackRoutePlan::new(fanout_owner_id, "generic-horizontal-fanout");
        plan.set_scene_coverage(
            target_positions
                .iter()
                .map(|(_, _, target)| edge_route_owner_id(graph, &from.id, &target.id))
                .collect::<Vec<_>>(),
        );

        if stem_length > 0 {
            let (stem_end_x, _) = coords.advance(stem_start_x, stem_start_y, stem_length - 1);
            plan.push_horizontal(
                stem_start_y,
                stem_start_x,
                stem_end_x,
                coords.primary_edge_char(style),
            );
            if stem_length == 1 {
                plan.push_paint(
                    stem_start_x,
                    stem_start_y,
                    canvas.get(stem_start_x, stem_start_y),
                );
            }
        }

        plan.push_vertical(
            junction_x,
            span_start,
            span_end,
            coords.secondary_edge_char(style),
        );
        if span_start == span_end {
            let (span_x, span_y) = coords.with_secondary(junction_x, junction_y, span_start);
            plan.push_paint(span_x, span_y, canvas.get(span_x, span_y));
        }

        if junction_secondary != src_secondary {
            plan.push_vertical(
                junction_x,
                src_secondary,
                junction_secondary,
                coords.secondary_edge_char(style),
            );
        }

        for (_, _, target) in &target_positions {
            let edge_kind = graph
                .edges
                .iter()
                .find(|e| e.from == from.id && e.to == target.id && !e.is_back_edge)
                .map(|e| e.kind)
                .unwrap_or(EdgeKind::Arrow);
            let branch_style = style_for_edge_kind(style, edge_kind);
            let (arrow_x, arrow_y) = adjusted_edge_entry_point(target, direction, graph);
            let target_secondary = coords.secondary_coord(arrow_x, arrow_y);
            let (drop_x, drop_y) = coords.with_secondary(junction_x, junction_y, target_secondary);
            let (drop_start_x, drop_start_y) = coords.advance(drop_x, drop_y, 1);
            if drop_start_x != arrow_x || drop_start_y != arrow_y {
                plan.push_horizontal(drop_start_y, drop_start_x, arrow_x, branch_style.edge_h);
            }
            plan.push_paint(arrow_x, arrow_y, canvas.get(arrow_x, arrow_y));
        }

        canvas.record_fallback_route_plan(plan);
    }
}

// These are intentionally separate geometry inputs: collapsing them into a
// context object would obscure the oriented source/junction/target contract
// this narrow fallback checks.
#[allow(clippy::too_many_arguments)]
fn mixed_vertical_fanout_clearance(
    target_positions: &[(usize, usize, &Node)],
    junction_x: usize,
    junction_y: usize,
    stem_start_x: usize,
    stem_start_y: usize,
    direction: Direction,
    graph: &Graph,
    canvas: &Canvas,
) -> Option<(usize, usize)> {
    let coords = OrientedCoords::new(direction);
    let candidate = coords.retreat(junction_x, junction_y, 1);
    let source_primary = coords.primary_coord(stem_start_x, stem_start_y);
    let candidate_primary = coords.primary_coord(candidate.0, candidate.1);

    // Do not move the junction through the source exit or outside the canvas.
    let candidate_is_between_source_and_targets = match direction {
        Direction::TD | Direction::TB => candidate_primary > source_primary,
        Direction::BT => candidate_primary < source_primary,
        Direction::LR | Direction::RL => false,
    };
    if !candidate_is_between_source_and_targets
        || candidate.0 >= canvas.width
        || candidate.1 >= canvas.height
    {
        return None;
    }

    let target_secondaries: Vec<usize> = target_positions
        .iter()
        .map(|(_, _, target)| {
            let (arrow_x, arrow_y) = adjusted_edge_entry_point(target, direction, graph);
            coords.secondary_coord(arrow_x, arrow_y)
        })
        .collect();
    let span_start = target_secondaries.iter().copied().min()?;
    let span_end = target_secondaries.iter().copied().max()?;

    // The candidate span and the one-cell target-facing drops must be free
    // before this route is lowered.  A crowded corridor should keep the old
    // geometry and remain visible to the perceptual review queue.
    for secondary in span_start..=span_end {
        let (x, y) = coords.with_secondary(candidate.0, candidate.1, secondary);
        if canvas.get(x, y) != ' ' {
            return None;
        }
    }
    for secondary in target_secondaries {
        let (drop_x, drop_y) = coords.with_secondary(candidate.0, candidate.1, secondary);
        let (shaft_x, shaft_y) = coords.advance(drop_x, drop_y, 1);
        if canvas.get(shaft_x, shaft_y) != ' ' {
            return None;
        }
    }

    Some(candidate)
}

const MIXED_BRANCH_JUNCTION_SEPARATION: usize = 3;
const MIXED_BRANCH_TARGET_CLEARANCE: usize = 3;

struct MixedBranchRequest<'a> {
    from: &'a Node,
    target: &'a Node,
    stem_start: (usize, usize),
    junction: (usize, usize),
    canvas: &'a Canvas,
    direction: Direction,
    graph: &'a Graph,
}

struct MixedBranchGeometry<'a> {
    stem_start: (usize, usize),
    arrow: (usize, usize),
    source_secondary: usize,
    target_secondary: usize,
    source_primary: usize,
    direction: Direction,
    canvas: &'a Canvas,
}

/// Select a measured LR/RL fan-out spine when the source also contributes to
/// another fan-in.  The fan-in pass runs first, so a candidate can be rejected
/// from the current canvas without teaching generic routes about a particular
/// fixture.  Returning `None` is the fail-closed path: the caller keeps its
/// ordinary L-route and does not claim a partial scene.
fn mixed_branch_junction(request: MixedBranchRequest<'_>) -> Option<(usize, usize)> {
    if !mixed_branch_topology_supported(request.from, request.target, request.graph)
        || !matches!(request.direction, Direction::LR | Direction::RL)
    {
        return None;
    }

    let coords = OrientedCoords::new(request.direction);
    let (source_x, source_y) = get_node_center(request.from);
    let (target_x, target_y) = get_node_center(request.target);
    let source_secondary = coords.secondary_coord(source_x, source_y);
    let target_secondary = coords.secondary_coord(target_x, target_y);
    if source_secondary == target_secondary {
        return None;
    }

    let arrow = adjusted_edge_entry_point(request.target, request.direction, request.graph);
    let source_primary = coords.primary_coord(request.stem_start.0, request.stem_start.1);
    let current_primary = coords.primary_coord(request.junction.0, request.junction.1);
    let arrow_primary = coords.primary_coord(arrow.0, arrow.1);
    let target_limit = match request.direction {
        Direction::LR => arrow_primary.saturating_sub(MIXED_BRANCH_TARGET_CLEARANCE),
        Direction::RL => arrow_primary.saturating_add(MIXED_BRANCH_TARGET_CLEARANCE),
        _ => return None,
    };

    let current_in_range = match request.direction {
        Direction::LR => current_primary > source_primary && current_primary <= target_limit,
        Direction::RL => current_primary < source_primary && current_primary >= target_limit,
        _ => false,
    };
    if !current_in_range {
        return None;
    }

    let primary_span = match request.direction {
        Direction::LR => target_limit.saturating_sub(current_primary),
        Direction::RL => current_primary.saturating_sub(target_limit),
        _ => return None,
    };

    for delta in 0..=primary_span {
        let candidate_primary = match request.direction {
            Direction::LR => current_primary.saturating_add(delta),
            Direction::RL => current_primary.saturating_sub(delta),
            _ => return None,
        };
        let mut candidate = request.junction;
        coords.set_primary(&mut candidate.0, &mut candidate.1, candidate_primary);

        let geometry = MixedBranchGeometry {
            stem_start: request.stem_start,
            arrow,
            source_secondary,
            target_secondary,
            source_primary,
            direction: request.direction,
            canvas: request.canvas,
        };
        if mixed_branch_candidate_is_clear(candidate, &geometry) {
            return Some(candidate);
        }
    }

    None
}

/// The candidate is intentionally narrower than a generic graph-family
/// detector.  Unsupported visual semantics remain on their existing route
/// owners instead of being partially claimed by this helper.
fn mixed_branch_topology_supported(from: &Node, target: &Node, graph: &Graph) -> bool {
    if !graph.subgraphs.is_empty()
        || graph.nodes.iter().any(|node| {
            !matches!(
                node.shape,
                crate::graph::NodeShape::Rectangle | crate::graph::NodeShape::Database
            )
        })
        || graph
            .edges
            .iter()
            .any(|edge| edge.is_back_edge || edge.label.is_some() || edge.kind != EdgeKind::Arrow)
    {
        return false;
    }

    let outgoing: Vec<&str> = graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge && edge.from == from.id)
        .map(|edge| edge.to.as_str())
        .collect();
    if outgoing.len() < 2 || !outgoing.iter().any(|id| *id == target.id) {
        return false;
    }

    // The selected branch is mixed only when another outgoing target has a
    // real fan-in.  The current target itself is the remaining divergence
    // edge after the convergence pass, so it normally has one incoming edge.
    outgoing.iter().any(|id| {
        *id != target.id.as_str()
            && graph
                .edges
                .iter()
                .filter(|edge| !edge.is_back_edge && edge.to == *id)
                .count()
                > 1
    })
}

fn mixed_branch_candidate_is_clear(
    candidate: (usize, usize),
    geometry: &MixedBranchGeometry<'_>,
) -> bool {
    let coords = OrientedCoords::new(geometry.direction);
    let candidate_primary = coords.primary_coord(candidate.0, candidate.1);

    // A foreign fan-in junction may remain on the shared source shaft, but a
    // new fan-out corner cannot be adjacent to it: that is the exact visual
    // ambiguity this candidate is meant to remove.
    let source_end_primary = coords.primary_coord(geometry.arrow.0, geometry.stem_start.1);
    let source_span_start = geometry.source_primary.min(source_end_primary);
    let source_span_end = geometry.source_primary.max(source_end_primary);
    for primary in source_span_start..=source_span_end {
        let mut point = geometry.stem_start;
        coords.set_primary(&mut point.0, &mut point.1, primary);
        if geometry
            .canvas
            .get_meta(point.0, point.1)
            .is_some_and(|meta| matches!(meta.role, CellRole::Junction | CellRole::Corner))
            && point.0.abs_diff(candidate.0) + point.1.abs_diff(candidate.1)
                < MIXED_BRANCH_JUNCTION_SEPARATION
        {
            return false;
        }
    }

    // The candidate spine and its target-row bend must be empty.  The source
    // row is excluded here because the source shaft can legitimately share a
    // horizontal fan-in corridor; the junction-separation check above keeps
    // that sharing visually attributable.
    let secondary_start = geometry.source_secondary.min(geometry.target_secondary);
    let secondary_end = geometry.source_secondary.max(geometry.target_secondary);
    for secondary in secondary_start..=secondary_end {
        let point = coords.with_secondary(candidate.0, candidate.1, secondary);
        if secondary != geometry.source_secondary && geometry.canvas.get(point.0, point.1) != ' ' {
            return false;
        }
    }

    // Keep the final target-row approach clear through the arrow.  The arrow
    // cell itself is still empty at this stage, but including it here makes
    // the capacity proof explicit and direction-neutral.
    let final_start = match geometry.direction {
        Direction::LR => candidate_primary.saturating_add(1),
        Direction::RL => candidate_primary.saturating_sub(1),
        _ => return false,
    };
    let final_end = coords.primary_coord(geometry.arrow.0, geometry.arrow.1);
    let (low, high) = (final_start.min(final_end), final_start.max(final_end));
    for primary in low..=high {
        let mut point = (candidate.0, geometry.arrow.1);
        coords.set_primary(&mut point.0, &mut point.1, primary);
        if geometry.canvas.get(point.0, point.1) != ' ' {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod mixed_branch_tests {
    use super::{mixed_branch_junction, mixed_branch_topology_supported, MixedBranchRequest};
    use crate::graph::Direction;
    use crate::graph::{Edge, Graph, Node, NodeShape};
    use crate::render::canvas::Canvas;
    use crate::render::semantic::CellOwnerKind;

    fn mixed_graph() -> Graph {
        let mut graph = Graph::new();
        graph.direction = Direction::LR;

        let mut gateway = Node::new("Gateway", "API Gateway");
        gateway.x = 0;
        gateway.y = 4;
        gateway.width = 17;
        gateway.height = 3;
        let mut auth = Node::new("Auth", "Auth Service");
        auth.x = 27;
        auth.y = 6;
        auth.width = 18;
        auth.height = 3;
        let mut api = Node::new("API", "Main API");
        api.x = 27;
        api.y = 2;
        api.width = 14;
        api.height = 3;
        let mut cache = Node::new("Cache", "Redis Cache");
        cache.x = 55;
        cache.y = 0;
        cache.width = 17;
        cache.height = 3;
        let mut database = Node::with_shape("DB", "Database", NodeShape::Database);
        database.x = 55;
        database.y = 4;
        database.width = 14;
        database.height = 3;

        for node in [gateway, auth, api, cache, database] {
            graph.add_node(node);
        }
        graph.add_edge(Edge::new("Gateway", "Auth"));
        graph.add_edge(Edge::new("Gateway", "API"));
        graph.add_edge(Edge::new("Auth", "DB"));
        graph.add_edge(Edge::new("API", "DB"));
        graph.add_edge(Edge::new("API", "Cache"));
        graph
    }

    #[test]
    fn mixed_role_predicate_accepts_database_target_family() {
        let graph = mixed_graph();
        let api = graph.get_node("API").expect("API node");
        let cache = graph.get_node("Cache").expect("Cache node");
        assert!(mixed_branch_topology_supported(api, cache, &graph));
    }

    #[test]
    fn mixed_role_predicate_rejects_labeled_variants() {
        let mut graph = mixed_graph();
        graph.edges[4].label = Some("cache".to_owned());
        let api = graph.get_node("API").expect("API node");
        let cache = graph.get_node("Cache").expect("Cache node");
        assert!(!mixed_branch_topology_supported(api, cache, &graph));
    }

    #[test]
    fn mixed_role_candidate_leaves_two_cells_between_foreign_junctions() {
        let graph = mixed_graph();
        let api = graph.get_node("API").expect("API node");
        let cache = graph.get_node("Cache").expect("Cache node");
        let mut canvas = Canvas::new(80, 10);
        canvas.set_owned(48, 3, '+', CellOwnerKind::EdgeSegment, "fanin:DB", 5);

        assert_eq!(
            mixed_branch_junction(MixedBranchRequest {
                from: api,
                target: cache,
                stem_start: (41, 3),
                junction: (47, 3),
                canvas: &canvas,
                direction: Direction::LR,
                graph: &graph,
            }),
            Some((51, 3))
        );
    }
}
