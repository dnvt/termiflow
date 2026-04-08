//! Unified, direction-agnostic edge routing.
//!
//! This module provides a single edge routing algorithm that works for all
//! diagram orientations (TD, LR, BT, RL) using the orientation abstraction.

use crate::graph::{Direction, EdgeKind, Graph, Node};
use crate::orientation::{is_before, OrientedCoords};
use crate::spacing::SpacingConfig;
use crate::style::StyleChars;

use super::canvas::Canvas;
use super::provenance::edge_owner_id;
use super::semantic::CellOwnerKind;
use super::{is_textual, subgraph_title_y, title_span};

const ROUTE_Z_INDEX: u8 = 5;

#[derive(Copy, Clone)]
struct RouteOwner<'a> {
    kind: CellOwnerKind,
    id: &'a str,
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
            if std::env::var("DEBUG_CROSS").is_ok() {
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
                style,
                Some(graph),
                Some(branch_owner),
            );
        }

        // Tip: use edge-kind-specific character (circle/cross end markers, etc.)
        let edge_kind = graph
            .edges
            .iter()
            .find(|e| e.from == from.id && e.to == target.id && !e.is_back_edge)
            .map(|e| e.kind)
            .unwrap_or(EdgeKind::Arrow);
        let tip = match edge_kind {
            EdgeKind::CircleEnd => style.circle_end,
            EdgeKind::CrossEnd => style.cross_end,
            EdgeKind::Open => coords.primary_edge_char(style), // no arrowhead
            _ => coords.arrow_end(style),
        };
        set_route_char(canvas, arrow_x, arrow_y, tip, Some(branch_owner));
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

fn route_fanout_into_subgraph_td(
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
    let (arrow_x, arrow_y) = adjusted_edge_entry_point(target, direction, graph);
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
        Some(graph),
        Some(&target.id),
    );

    let (final_span_start, final_span_end) = if matches!(direction, Direction::TD | Direction::TB) {
        (span_start, span_end)
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
        Some(graph),
        Some(fanin_owner),
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
            Some(graph),
            Some(fanin_owner),
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
            canvas.set_owned(
                merge_x,
                top_y,
                style.edge_v,
                CellOwnerKind::PortalOpening,
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
    let (target_x, target_y) = get_node_center(target);
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
    let target_secondary = coords.secondary_coord(target_x, target_y);
    if target_secondary < span_start || target_secondary > span_end {
        return false;
    }

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
    let merge_y = arrow_y.clamp(
        sg.bounds.y.saturating_add(1),
        sg.bounds.y + sg.bounds.height.saturating_sub(2),
    );

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
        canvas.set_owned(
            border_x,
            merge_y,
            style.edge_h,
            CellOwnerKind::PortalOpening,
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

fn set_route_char(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    ch: char,
    owner: Option<RouteOwner<'_>>,
) {
    if let Some(owner) = owner {
        canvas.set_owned(x, y, ch, owner.kind, owner.id, ROUTE_Z_INDEX);
    } else {
        canvas.set(x, y, ch);
    }
}

fn set_route_edge_char(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    ch: char,
    style: &StyleChars,
    owner: Option<RouteOwner<'_>>,
) {
    if let Some(owner) = owner {
        canvas.set_edge_char_owned(x, y, ch, style, owner.kind, owner.id, ROUTE_Z_INDEX);
    } else {
        canvas.set_edge_char(x, y, ch, style);
    }
}

fn edge_route_owner_id(graph: &Graph, from_id: &str, to_id: &str) -> String {
    graph
        .edges
        .iter()
        .enumerate()
        .find_map(|(idx, edge)| {
            (!edge.is_back_edge && edge.from == from_id && edge.to == to_id)
                .then(|| edge_owner_id(idx, edge))
        })
        .unwrap_or_else(|| format!("edge:?:{from_id}->{to_id}"))
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

fn adjusted_edge_entry_point(node: &Node, direction: Direction, graph: &Graph) -> (usize, usize) {
    let default = edge_entry_point(node, direction);
    if !hits_foreign_subgraph_border(node, default.0, default.1, graph) {
        return default;
    }

    for candidate in edge_entry_candidates(node, direction) {
        if !hits_foreign_subgraph_border(node, candidate.0, candidate.1, graph) {
            return candidate;
        }
    }

    default
}

fn edge_entry_candidates(node: &Node, direction: Direction) -> Vec<(usize, usize)> {
    let mut candidates = Vec::new();
    let push_if_new = |candidates: &mut Vec<(usize, usize)>, candidate| {
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    };

    match direction {
        Direction::TD | Direction::TB | Direction::BT => {
            let y = edge_entry_point(node, direction).1;
            let center = node.center_x();
            push_if_new(&mut candidates, (center, y));

            let min_x = node.x.saturating_add(1);
            let max_x = node.x + node.width.saturating_sub(2);
            for delta in 1..=node.width {
                let left = center.saturating_sub(delta);
                if left >= min_x {
                    push_if_new(&mut candidates, (left, y));
                }
                let right = center.saturating_add(delta);
                if right <= max_x {
                    push_if_new(&mut candidates, (right, y));
                }
                if left < min_x && right > max_x {
                    break;
                }
            }
        }
        Direction::LR | Direction::RL => {
            let x = edge_entry_point(node, direction).0;
            let center = node.center_y();
            push_if_new(&mut candidates, (x, center));

            let min_y = node.y.saturating_add(1);
            let max_y = node.y + node.height.saturating_sub(2);
            for delta in 1..=node.height {
                let up = center.saturating_sub(delta);
                if up >= min_y {
                    push_if_new(&mut candidates, (x, up));
                }
                let down = center.saturating_add(delta);
                if down <= max_y {
                    push_if_new(&mut candidates, (x, down));
                }
                if up < min_y && down > max_y {
                    break;
                }
            }
        }
    }

    candidates
}

fn hits_foreign_subgraph_border(node: &Node, x: usize, y: usize, graph: &Graph) -> bool {
    let own_subgraph = graph.get_node_subgraph(&node.id);

    graph.subgraphs.iter().any(|subgraph| {
        if !subgraph.bounds.is_valid() || own_subgraph == Some(subgraph.id.as_str()) {
            return false;
        }

        let min_x = subgraph.bounds.x;
        let max_x = subgraph.bounds.x + subgraph.bounds.width.saturating_sub(1);
        let min_y = subgraph.bounds.y;
        let max_y = subgraph.bounds.y + subgraph.bounds.height.saturating_sub(1);
        let within_x = x >= min_x && x <= max_x;
        let within_y = y >= min_y && y <= max_y;

        within_x && within_y && (x == min_x || x == max_x || y == min_y || y == max_y)
    })
}

/// Where an outgoing edge exits a source node (stem start position).
pub fn edge_exit_point(node: &Node, direction: Direction) -> (usize, usize) {
    match direction {
        Direction::TD | Direction::TB => (node.center_x(), node.bottom_y()),
        Direction::LR => (node.x + node.width, node.center_y()),
        Direction::RL => (node.x.saturating_sub(1), node.center_y()),
        Direction::BT => (node.center_x(), node.y.saturating_sub(1)),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_line_primary(
    x1: usize,
    y1: usize,
    x2: usize,
    y2: usize,
    coords: &OrientedCoords,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: Option<&Graph>,
    owner: Option<RouteOwner<'_>>,
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
                set_route_edge_char(canvas, x, y1, char, style, owner);
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
                set_route_edge_char(canvas, x1, y, char, style, owner);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_line_secondary(
    x1: usize,
    y1: usize,
    x2: usize,
    y2: usize,
    coords: &OrientedCoords,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: Option<&Graph>,
    owner: Option<RouteOwner<'_>>,
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
                    set_route_edge_char(canvas, x, y1, char, style, owner);
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
                    set_route_edge_char(canvas, x1, y, char, style, owner);
                }
            }
        }
    }
}

fn is_subgraph_title_cell(graph: &Graph, x: usize, y: usize) -> bool {
    graph.subgraphs.iter().any(|sg| {
        if !sg.has_title() || !sg.bounds.is_valid() {
            return false;
        }
        let title_y = subgraph_title_y(&sg.bounds, graph.direction);
        y == title_y && x >= sg.bounds.x && x < sg.bounds.x.saturating_add(sg.bounds.width)
    })
}

fn preferred_portal_x(
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
            let (start, end) = title_span(bounds, t);
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

#[allow(dead_code, clippy::too_many_arguments)]
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
        let min_x = sg.bounds.x.saturating_add(1);
        let max_x = sg.bounds.x + sg.bounds.width.saturating_sub(2);
        let portal_x = arrow_x.clamp(min_x, max_x);

        let outside_y = sg.bounds.y.saturating_sub(1);
        let inside_y = sg.bounds.y.saturating_add(1);

        if stem_start_y <= outside_y {
            for y in stem_start_y..=outside_y {
                set_route_edge_char(canvas, stem_start_x, y, style.edge_v, style, owner);
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
            set_route_edge_char(canvas, stem_start_x, outside_y, start_corner, style, owner);

            let (hx0, hx1) = if portal_x > stem_start_x {
                (stem_start_x + 1, portal_x.saturating_sub(1))
            } else {
                (portal_x + 1, stem_start_x.saturating_sub(1))
            };
            for x in hx0..=hx1 {
                set_route_edge_char(canvas, x, outside_y, style.edge_h, style, owner);
            }

            let end_corner = if portal_x > stem_start_x {
                style.corner_dr
            } else {
                style.corner_dl
            };
            set_route_edge_char(canvas, portal_x, outside_y, end_corner, style, owner);
        }

        if arrow_y >= inside_y && inside_y < canvas.height {
            for y in inside_y..=arrow_y {
                set_route_edge_char(canvas, portal_x, y, style.edge_v, style, owner);
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
    let mut portal_x = preferred_portal_x(
        &sg.bounds,
        sg.title.as_deref(),
        arrow_x,
        canvas,
        graph.direction,
        true,
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

fn route_divergent_into_subgraph_td(
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

    // Branch row: horizontal bar with a center tee.
    for x in min_x..=max_x {
        set_route_edge_char(canvas, x, branch_y, style.edge_h, style, Some(fanout_owner));
    }
    set_route_char(canvas, min_x, branch_y, style.corner_dl, Some(fanout_owner));
    set_route_char(canvas, max_x, branch_y, style.corner_dr, Some(fanout_owner));
    // The trunk enters from above; keep the branch row as a "cap" (no down stroke)
    // so drops start on the next row.
    set_route_char(
        canvas,
        entry_x,
        branch_y,
        style.junction_up,
        Some(fanout_owner),
    );

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

fn route_divergent_into_subgraph_bt(
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
    use super::*;
    use crate::graph::{Direction, Graph, Node, Rectangle, Subgraph};

    fn make_node(id: &str, x: usize, y: usize, width: usize, height: usize) -> Node {
        let mut n = Node::new(id, id);
        n.x = x;
        n.y = y;
        n.width = width;
        n.height = height;
        n
    }

    // =========================================================================
    // edge_exit_point — all 4 directions
    // =========================================================================

    #[test]
    fn exit_point_td_is_bottom_center() {
        // Node at (10, 5), width=6, height=3 → bottom_y = 5+3=8, center_x = 10+3=13
        let n = make_node("a", 10, 5, 6, 3);
        assert_eq!(edge_exit_point(&n, Direction::TD), (13, 8));
    }

    #[test]
    fn exit_point_lr_is_right_center() {
        let n = make_node("a", 10, 5, 6, 3);
        // LR: right edge = x+width = 16, center_y = y + h/2 = 5+1 = 6
        assert_eq!(edge_exit_point(&n, Direction::LR), (16, 6));
    }

    #[test]
    fn exit_point_rl_is_left_center() {
        let n = make_node("a", 10, 5, 6, 3);
        // RL: left edge = x.saturating_sub(1) = 9, center_y = 6
        assert_eq!(edge_exit_point(&n, Direction::RL), (9, 6));
    }

    #[test]
    fn exit_point_bt_is_top_center() {
        let n = make_node("a", 10, 5, 6, 3);
        // BT: y.saturating_sub(1) = 4, center_x = 13
        assert_eq!(edge_exit_point(&n, Direction::BT), (13, 4));
    }

    #[test]
    fn exit_point_rl_at_x0_saturates() {
        let n = make_node("a", 0, 0, 6, 3);
        // x.saturating_sub(1) = 0
        assert_eq!(edge_exit_point(&n, Direction::RL), (0, 1));
    }

    // =========================================================================
    // edge_entry_point — all 4 directions
    // =========================================================================

    #[test]
    fn entry_point_td_is_above_center() {
        let n = make_node("a", 10, 5, 6, 3);
        // TD: center_x=13, y.saturating_sub(1)=4
        assert_eq!(edge_entry_point(&n, Direction::TD), (13, 4));
    }

    #[test]
    fn entry_point_lr_is_left_center() {
        let n = make_node("a", 10, 5, 6, 3);
        // LR: x.saturating_sub(1)=9, center_y=6
        assert_eq!(edge_entry_point(&n, Direction::LR), (9, 6));
    }

    #[test]
    fn entry_point_rl_is_right_center() {
        let n = make_node("a", 10, 5, 6, 3);
        // RL: x+width=16, center_y=6
        assert_eq!(edge_entry_point(&n, Direction::RL), (16, 6));
    }

    #[test]
    fn entry_point_bt_is_below_center() {
        let n = make_node("a", 10, 5, 6, 3);
        // BT: center_x=13, bottom_y=8
        assert_eq!(edge_entry_point(&n, Direction::BT), (13, 8));
    }

    // exit_point and entry_point are symmetric for the same node/direction
    #[test]
    fn exit_and_entry_points_are_symmetric() {
        let n = make_node("a", 10, 5, 6, 3);
        assert_eq!(
            edge_exit_point(&n, Direction::TD),
            edge_entry_point(&n, Direction::BT)
        );
        assert_eq!(
            edge_exit_point(&n, Direction::LR),
            edge_entry_point(&n, Direction::RL)
        );
        assert_eq!(
            edge_exit_point(&n, Direction::RL),
            edge_entry_point(&n, Direction::LR)
        );
        assert_eq!(
            edge_exit_point(&n, Direction::BT),
            edge_entry_point(&n, Direction::TD)
        );
    }

    // =========================================================================
    // hits_foreign_subgraph_border
    // =========================================================================

    fn graph_with_foreign_subgraph(sg_x: usize, sg_y: usize, sg_w: usize, sg_h: usize) -> Graph {
        let mut g = Graph::new();
        let mut sg = Subgraph::new("foreign", None);
        sg.bounds = Rectangle::new(sg_x, sg_y, sg_w, sg_h);
        g.add_subgraph(sg);
        g
    }

    #[test]
    fn hits_border_on_top_edge() {
        // Subgraph at (10,5) size 8×6 → top border y=5
        let g = graph_with_foreign_subgraph(10, 5, 8, 6);
        let n = make_node("n", 0, 0, 4, 3); // not in any subgraph
        assert!(hits_foreign_subgraph_border(&n, 14, 5, &g)); // x=14 in [10..17], y=5 = min_y
    }

    #[test]
    fn hits_border_on_left_edge() {
        let g = graph_with_foreign_subgraph(10, 5, 8, 6);
        let n = make_node("n", 0, 0, 4, 3);
        assert!(hits_foreign_subgraph_border(&n, 10, 8, &g)); // x=10 = min_x
    }

    #[test]
    fn no_hit_interior_of_subgraph() {
        let g = graph_with_foreign_subgraph(10, 5, 8, 6);
        let n = make_node("n", 0, 0, 4, 3);
        // (13, 8) is strictly inside the box — not on any border
        assert!(!hits_foreign_subgraph_border(&n, 13, 8, &g));
    }

    #[test]
    fn no_hit_outside_subgraph() {
        let g = graph_with_foreign_subgraph(10, 5, 8, 6);
        let n = make_node("n", 0, 0, 4, 3);
        assert!(!hits_foreign_subgraph_border(&n, 5, 5, &g)); // left of subgraph
        assert!(!hits_foreign_subgraph_border(&n, 20, 8, &g)); // right of subgraph
    }

    #[test]
    fn no_hit_for_own_subgraph() {
        // Node is in the same subgraph — should NOT count as a hit
        let mut g = Graph::new();
        let mut sg = Subgraph::new("own", None);
        sg.bounds = Rectangle::new(10, 5, 8, 6);
        g.add_subgraph(sg);
        g.add_node(make_node("n", 12, 6, 4, 3));
        g.associate_node_with_subgraph("n", "own");
        let n = g.get_node("n").expect("node 'n' was just added");
        assert!(!hits_foreign_subgraph_border(n, 14, 5, &g));
    }

    // =========================================================================
    // edge_entry_candidates — TD/BT: center-first, expanding outward
    // =========================================================================

    #[test]
    fn entry_candidates_td_starts_at_center() {
        let n = make_node("a", 10, 5, 6, 3);
        let candidates = edge_entry_candidates(&n, Direction::TD);
        // First candidate should be center_x, y-1
        assert!(!candidates.is_empty());
        let center_x = n.center_x(); // 10 + 3 = 13
        assert_eq!(candidates[0], (center_x, n.y.saturating_sub(1)));
    }

    #[test]
    fn entry_candidates_lr_starts_at_center() {
        let n = make_node("a", 10, 5, 6, 3);
        let candidates = edge_entry_candidates(&n, Direction::LR);
        assert!(!candidates.is_empty());
        let center_y = n.center_y(); // 5 + 1 = 6
        assert_eq!(candidates[0], (n.x.saturating_sub(1), center_y));
    }

    #[test]
    fn entry_candidates_no_duplicates() {
        let n = make_node("a", 10, 5, 6, 3);
        for dir in [Direction::TD, Direction::LR, Direction::RL, Direction::BT] {
            let candidates = edge_entry_candidates(&n, dir);
            let mut seen = std::collections::HashSet::new();
            for pt in &candidates {
                assert!(
                    seen.insert(*pt),
                    "duplicate candidate {pt:?} for direction {dir:?}"
                );
            }
        }
    }
}
