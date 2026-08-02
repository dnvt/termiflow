//! Unified, direction-agnostic edge routing.
//!
//! This module provides a single edge routing algorithm that works for all
//! diagram orientations (TD, LR, BT, RL) using the orientation abstraction.

mod convergence;
mod edge_primitives;
mod subgraph;

use crate::graph::{Direction, EdgeKind, Graph, Node};
use crate::orientation::{is_before, OrientedCoords};
use crate::spacing::SpacingConfig;
use crate::style::StyleChars;

use super::canvas::Canvas;
use super::edge_policy::title_safe_td_entry_x;
use super::provenance::edge_owner_id;
use super::semantic::CellOwnerKind;
pub use convergence::route_convergent_edges;
use convergence::route_fanout_into_subgraph_td;
pub use edge_primitives::edge_exit_point;
use edge_primitives::{
    adjusted_edge_entry_point, draw_line_primary, draw_line_secondary, get_node_center,
};
pub(super) use edge_primitives::{edge_entry_candidates, is_subgraph_title_cell};
#[cfg(test)]
use edge_primitives::{edge_entry_point, hits_foreign_subgraph_border};
use subgraph::{
    route_cross_subgraph_bt, route_cross_subgraph_td, route_divergent_into_subgraph_bt,
    route_divergent_into_subgraph_td,
};

const ROUTE_Z_INDEX: u8 = 5;

#[derive(Copy, Clone)]
struct RouteOwner<'a> {
    kind: CellOwnerKind,
    id: &'a str,
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

fn style_for_edge_kind(style: &StyleChars, kind: EdgeKind) -> StyleChars {
    let mut branch_style = *style;
    match kind {
        EdgeKind::Thick => {
            // The precomputed route uses heavy Unicode shafts. Keep the same
            // semantic distinction in fallback routes, with readable ASCII
            // approximations when the base style is ASCII-like.
            branch_style.edge_h = if style.edge_h == '-' { '=' } else { '━' };
            branch_style.edge_v = if style.edge_v == '|' { '|' } else { '┃' };
        }
        EdgeKind::Dotted => {
            branch_style.edge_h = if style.edge_h == '-' {
                '.'
            } else {
                style.dotted_h
            };
            branch_style.edge_v = style.dotted_v;
        }
        EdgeKind::Arrow
        | EdgeKind::Open
        | EdgeKind::Bidirectional
        | EdgeKind::CircleEnd
        | EdgeKind::CrossEnd => {}
    }
    branch_style
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
            let cross_arrow_x = if from_sg != to_sg {
                title_safe_td_entry_x(target, arrow_x, arrow_y, stem_start_y, graph)
            } else {
                arrow_x
            };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Direction, Graph, Node, Rectangle, Subgraph};
    use crate::style::{ASCII_CHARS, UNICODE_CHARS};

    #[test]
    fn fallback_branch_style_preserves_thick_and_dotted_shafts() {
        let ascii_thick = style_for_edge_kind(&ASCII_CHARS, EdgeKind::Thick);
        assert_eq!(ascii_thick.edge_h, '=');
        assert_eq!(ascii_thick.edge_v, '|');

        let ascii_dotted = style_for_edge_kind(&ASCII_CHARS, EdgeKind::Dotted);
        assert_eq!(ascii_dotted.edge_h, '.');
        assert_eq!(ascii_dotted.edge_v, ':');

        let unicode_thick = style_for_edge_kind(&UNICODE_CHARS, EdgeKind::Thick);
        assert_eq!(unicode_thick.edge_h, '━');
        assert_eq!(unicode_thick.edge_v, '┃');

        let unicode_dotted = style_for_edge_kind(&UNICODE_CHARS, EdgeKind::Dotted);
        assert_eq!(unicode_dotted.edge_h, '╌');
        assert_eq!(unicode_dotted.edge_v, '╎');
    }

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
