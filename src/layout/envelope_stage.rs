//! Subgraph-envelope and placement-constraint orchestration.

use std::collections::HashMap;

use crate::graph::Direction;
use crate::portals::{compute_envelopes, SubgraphEnvelope};

use super::constraints::*;
use super::placement::Placement;
use super::{adjust_portal_slots_for_title, CoarseLayoutConfig, LayoutInput};

/// Resolve stage-3 subgraph envelopes and placement constraints.
pub(super) fn resolve_subgraph_envelopes(
    input: &LayoutInput<'_>,
    config: &CoarseLayoutConfig,
    placement: &mut Placement,
    debug_timing: bool,
) -> HashMap<String, SubgraphEnvelope> {
    // 3) Subgraph bounds + gutters.
    let mut subgraph_envelopes =
        compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
    adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);

    if matches!(input.graph.direction, Direction::LR | Direction::RL)
        && !subgraph_envelopes.is_empty()
    {
        let gap = nested_horizontal_follow_gap(config);
        for _ in 0..8 {
            let mut required_shift_by_id: HashMap<String, isize> = HashMap::new();

            for child_subgraph in input
                .graph
                .subgraphs
                .iter()
                .filter(|subgraph| subgraph.parent_id.is_some())
            {
                let Some(parent_id) = child_subgraph.parent_id.as_deref() else {
                    continue;
                };
                let (Some(parent_env), Some(child_env)) = (
                    subgraph_envelopes.get(parent_id),
                    subgraph_envelopes.get(&child_subgraph.id),
                ) else {
                    continue;
                };
                if !rect_fully_inside(parent_env.outer, child_env.outer) {
                    continue;
                }

                let Some(target_left) = preferred_declared_nested_horizontal_left(
                    input.graph,
                    &placement.node_rects,
                    parent_id,
                    &child_subgraph.id,
                    parent_env,
                    child_env,
                    input.graph.direction,
                    gap,
                ) else {
                    continue;
                };

                if target_left == child_env.outer.x {
                    continue;
                }

                let delta = target_left as isize - child_env.outer.x as isize;
                required_shift_by_id
                    .entry(child_subgraph.id.clone())
                    .and_modify(|existing| {
                        if delta.abs() > existing.abs() {
                            *existing = delta;
                        }
                    })
                    .or_insert(delta);
            }

            let Some((subgraph_id, delta_x)) = required_shift_by_id
                .iter()
                .max_by_key(|(_, delta)| delta.abs())
                .map(|(id, delta)| (id.clone(), *delta))
            else {
                break;
            };

            shift_nodes_in_subgraph_tree_x_signed(
                input.graph,
                &mut placement.positions,
                &mut placement.node_rects,
                &subgraph_id,
                delta_x,
            );

            let max_right = placement
                .node_rects
                .values()
                .map(|rect| rect.right())
                .max()
                .unwrap_or(placement.canvas.right());
            placement.canvas.width = placement.canvas.width.max(max_right);

            subgraph_envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
        }

        for _ in 0..8 {
            let envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            let mut required_env_shift: Option<(usize, usize)> = None;
            let mut external_node_shifts: HashMap<String, usize> = HashMap::new();

            for (subgraph_id, env) in &envelopes {
                for edge in input.graph.edges.iter().filter(|edge| !edge.is_back_edge) {
                    let (Some(from_rect), Some(to_rect)) = (
                        placement.node_rects.get(&edge.from),
                        placement.node_rects.get(&edge.to),
                    ) else {
                        continue;
                    };

                    let from_inside_tree = input
                        .graph
                        .is_node_in_subgraph_tree(&edge.from, subgraph_id);
                    let to_inside_tree =
                        input.graph.is_node_in_subgraph_tree(&edge.to, subgraph_id);
                    if from_inside_tree == to_inside_tree {
                        continue;
                    }

                    let inside_rect = if from_inside_tree {
                        *from_rect
                    } else {
                        *to_rect
                    };
                    if !rect_fully_inside(env.outer, inside_rect) {
                        continue;
                    }

                    let external_rect = if from_inside_tree {
                        *to_rect
                    } else {
                        *from_rect
                    };
                    let external_is_subgraph = if from_inside_tree {
                        input.graph.get_node_subgraph(&edge.to).is_some()
                    } else {
                        input.graph.get_node_subgraph(&edge.from).is_some()
                    };
                    if external_is_subgraph {
                        continue;
                    }

                    if external_rect.x < env.outer.x {
                        let overlaps_left_wall = external_rect.right() > env.outer.x;
                        if overlaps_left_wall {
                            let required_env_x = external_rect.right().saturating_add(2);
                            let threshold_x = env.outer.x;
                            let delta_x = required_env_x - env.outer.x;
                            match required_env_shift {
                                Some((best_x, best_delta)) => {
                                    if threshold_x < best_x
                                        || (threshold_x == best_x && delta_x > best_delta)
                                    {
                                        required_env_shift = Some((threshold_x, delta_x));
                                    }
                                }
                                None => required_env_shift = Some((threshold_x, delta_x)),
                            }
                        }
                    } else {
                        let overlaps_right_wall = external_rect.x < env.outer.right();
                        if overlaps_right_wall {
                            let required_external_x = env.outer.right().saturating_add(2);
                            let external_node_id = if from_inside_tree {
                                edge.to.clone()
                            } else {
                                edge.from.clone()
                            };
                            let delta_x = required_external_x - external_rect.x;
                            external_node_shifts
                                .entry(external_node_id)
                                .and_modify(|existing| *existing = (*existing).max(delta_x))
                                .or_insert(delta_x);
                        }
                    }
                }
            }

            if required_env_shift.is_none() && external_node_shifts.is_empty() {
                break;
            }

            if let Some((threshold_x, delta_x)) = required_env_shift {
                shift_nodes_from_x(
                    &mut placement.positions,
                    &mut placement.node_rects,
                    threshold_x,
                    delta_x,
                );
            }
            if !external_node_shifts.is_empty() {
                shift_nodes_by_id_x(
                    &mut placement.positions,
                    &mut placement.node_rects,
                    &external_node_shifts,
                );
            }

            let max_right = placement
                .node_rects
                .values()
                .map(|rect| rect.right())
                .max()
                .unwrap_or(placement.canvas.right());
            placement.canvas.width = placement.canvas.width.max(max_right);

            subgraph_envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
        }
    }

    if is_vertical_flow(input.graph.direction) && !subgraph_envelopes.is_empty() {
        let route_budgeted_subgraphs = route_budgeted_subgraphs(input.graph);
        for _ in 0..8 {
            let mut widened_any = false;
            for subgraph_id in &route_budgeted_subgraphs {
                if widen_subgraph_for_internal_route_span(
                    input.graph,
                    &mut placement.positions,
                    &mut placement.node_rects,
                    subgraph_id,
                    config.min_horizontal_spacing,
                ) > 0
                {
                    widened_any = true;
                }
                if widen_subgraph_for_outgoing_route_pressure(
                    input.graph,
                    &mut placement.positions,
                    &mut placement.node_rects,
                    subgraph_id,
                ) > 0
                {
                    widened_any = true;
                }
            }
            if widened_any {
                let max_right = placement
                    .node_rects
                    .values()
                    .map(|r| r.right())
                    .max()
                    .unwrap_or(placement.canvas.right());
                placement.canvas.width = placement.canvas.width.max(max_right);
                subgraph_envelopes =
                    compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
                adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
            }

            let mut required_shift_by_id: HashMap<String, isize> = HashMap::new();

            let sg_ids: Vec<&String> = subgraph_envelopes.keys().collect();
            for parent_id in &sg_ids {
                let Some(parent_env) = subgraph_envelopes.get(*parent_id) else {
                    continue;
                };
                for child_id in &sg_ids {
                    if parent_id == child_id {
                        continue;
                    }
                    let Some(child_env) = subgraph_envelopes.get(*child_id) else {
                        continue;
                    };
                    if !rect_fully_inside(parent_env.outer, child_env.outer) {
                        continue;
                    }
                    let child_has_external_outgoing = input.graph.edges.iter().any(|edge| {
                        !edge.is_back_edge
                            && input.graph.is_node_in_subgraph_tree(&edge.from, child_id)
                            && !input.graph.is_node_in_subgraph_tree(&edge.to, child_id)
                    });
                    if !child_has_external_outgoing {
                        continue;
                    }

                    let preferred_center_x = preferred_subgraph_center_x(
                        input.graph,
                        &placement.node_rects,
                        child_id,
                        rect_center_x(child_env.outer),
                    );
                    let route_pressure_shift = outgoing_route_pressure_shift_x(
                        input.graph,
                        &placement.node_rects,
                        child_id,
                    );
                    let preferred_left =
                        preferred_center_x.saturating_sub(child_env.outer.width / 2);

                    let mut min_left = 0usize;
                    let mut max_left: Option<usize> = None;

                    for (node_id, node_rect) in placement.node_rects.iter() {
                        if input.graph.is_node_in_subgraph_tree(node_id, child_id) {
                            continue;
                        }
                        if !rect_fully_inside(parent_env.outer, *node_rect)
                            || !rects_overlap_vertically(*node_rect, child_env.outer)
                        {
                            continue;
                        }

                        if node_rect.right() <= child_env.outer.x {
                            min_left = min_left.max(node_rect.right().saturating_add(1));
                        } else if node_rect.x >= child_env.outer.right() {
                            let candidate = node_rect
                                .x
                                .saturating_sub(child_env.outer.width.saturating_add(1));
                            max_left =
                                Some(max_left.map_or(candidate, |limit| limit.min(candidate)));
                        } else {
                            min_left = min_left.max(node_rect.right().saturating_add(1));
                        }
                    }

                    let unclamped_left = if let Some(limit) = max_left {
                        preferred_left.clamp(min_left, limit.max(min_left))
                    } else {
                        preferred_left.max(min_left)
                    };
                    let target_left = if let Some(limit) = max_left {
                        unclamped_left
                            .saturating_add(route_pressure_shift)
                            .min(limit.max(unclamped_left))
                    } else {
                        unclamped_left.saturating_add(route_pressure_shift)
                    };

                    if target_left != child_env.outer.x {
                        let delta = target_left as isize - child_env.outer.x as isize;
                        required_shift_by_id
                            .entry((**child_id).clone())
                            .and_modify(|existing| {
                                if delta.abs() > existing.abs() {
                                    *existing = delta;
                                }
                            })
                            .or_insert(delta);
                    }
                }
            }

            let Some((sg_id, delta_x)) = required_shift_by_id
                .iter()
                .max_by_key(|(_, delta)| delta.abs())
                .map(|(id, delta)| (id.clone(), *delta))
            else {
                if widened_any {
                    continue;
                }
                break;
            };

            shift_nodes_in_subgraph_tree_x_signed(
                input.graph,
                &mut placement.positions,
                &mut placement.node_rects,
                &sg_id,
                delta_x,
            );

            let max_right = placement
                .node_rects
                .values()
                .map(|r| r.right())
                .max()
                .unwrap_or(placement.canvas.right());
            placement.canvas.width = placement.canvas.width.max(max_right);

            subgraph_envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
        }
    }

    // Ensure we have at least one row between a subgraph bottom border and any
    // external target box below it. Otherwise the renderer's arrow would land on
    // the border row (missing the arrow at the target entry point).
    if matches!(input.graph.direction, Direction::TD | Direction::TB)
        && !subgraph_envelopes.is_empty()
    {
        for _ in 0..8 {
            let mut required_shift_by_rank: HashMap<usize, usize> = HashMap::new();

            let mut subgraph_min_rank: HashMap<&str, usize> = HashMap::new();
            for sg in &input.graph.subgraphs {
                let min_rank = subgraph_tree_rank_range(input.graph, &placement.ranks, &sg.id)
                    .map(|(min_rank, _)| min_rank);
                if let Some(r) = min_rank {
                    subgraph_min_rank.insert(sg.id.as_str(), r);
                }
            }

            let mut incoming_into_subgraph_from: HashMap<(String, String), usize> = HashMap::new();
            for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                let (_, enter_subgraphs) =
                    input.graph.edge_boundary_crossings(&edge.from, &edge.to);
                for to_sg in enter_subgraphs {
                    *incoming_into_subgraph_from
                        .entry((edge.from.clone(), to_sg.to_string()))
                        .or_default() += 1;
                }
            }

            // Ensure declared parents keep a visible title/border band above nested children.
            for child_subgraph in input
                .graph
                .subgraphs
                .iter()
                .filter(|subgraph| subgraph.parent_id.is_some())
            {
                let Some(parent_id) = child_subgraph.parent_id.as_deref() else {
                    continue;
                };
                let (Some(parent_env), Some(child_env)) = (
                    subgraph_envelopes.get(parent_id),
                    subgraph_envelopes.get(&child_subgraph.id),
                ) else {
                    continue;
                };
                let Some(&shift_rank) = subgraph_min_rank.get(child_subgraph.id.as_str()) else {
                    continue;
                };

                let parent_has_title = input
                    .graph
                    .get_subgraph(parent_id)
                    .and_then(|subgraph| subgraph.title.as_ref())
                    .is_some();
                let required_child_top =
                    parent_env
                        .outer
                        .y
                        .saturating_add(if parent_has_title { 3 } else { 2 });
                if child_env.outer.y >= required_child_top {
                    continue;
                }

                let delta = required_child_top - child_env.outer.y;
                required_shift_by_rank
                    .entry(shift_rank)
                    .and_modify(|existing| *existing = (*existing).max(delta))
                    .or_insert(delta);
            }

            // Ensure enough clearance above a subgraph top border for incoming edges.
            for (sg_id, env) in subgraph_envelopes.iter() {
                let Some(&shift_rank) = subgraph_min_rank.get(sg_id.as_str()) else {
                    continue;
                };
                for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                    let (_, enter_subgraphs) =
                        input.graph.edge_boundary_crossings(&edge.from, &edge.to);
                    if !enter_subgraphs.contains(&sg_id.as_str()) {
                        continue;
                    }
                    // Don't apply this spacing rule for edges whose source already sits inside
                    // another subgraph (nested compositions). Those are handled by internal
                    // subgraph padding and routing, and enforcing "outside" clearance here
                    // can cause runaway vertical expansion.
                    if input.graph.get_node_subgraph(&edge.from).is_some() {
                        continue;
                    }
                    let Some(from_rect) = placement.node_rects.get(&edge.from) else {
                        continue;
                    };
                    // Single incoming edge: one connector row is enough.
                    // Fan-out entry (same external source → multiple targets): keep two rows so
                    // the trunk can be visible before entering the subgraph.
                    let incoming_count = incoming_into_subgraph_from
                        .get(&(edge.from.clone(), sg_id.clone()))
                        .copied()
                        .unwrap_or(1);
                    let clearance = if incoming_count > 1 { 2 } else { 1 };
                    let required_border_y = from_rect.bottom().saturating_add(clearance);
                    if env.outer.y < required_border_y {
                        let delta = required_border_y - env.outer.y;
                        required_shift_by_rank
                            .entry(shift_rank)
                            .and_modify(|d| *d = (*d).max(delta))
                            .or_insert(delta);
                    }
                }
            }

            // Ensure at least one empty row between stacked subgraphs when an edge crosses
            // from one to the next (so the connector is visible outside both borders).
            for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                let (Some(from_sg), Some(to_sg)) = (
                    input.graph.get_node_subgraph(&edge.from),
                    input.graph.get_node_subgraph(&edge.to),
                ) else {
                    continue;
                };
                if from_sg == to_sg {
                    continue;
                }
                let (Some(from_env), Some(to_env)) = (
                    subgraph_envelopes.get(from_sg),
                    subgraph_envelopes.get(to_sg),
                ) else {
                    continue;
                };
                // Only skip if subgraphs are truly nested (one fully inside the other).
                // Overlapping-but-not-nested subgraphs need spacing applied.
                let is_nested = rect_fully_inside(from_env.outer, to_env.outer)
                    || rect_fully_inside(to_env.outer, from_env.outer);
                if is_nested {
                    continue;
                }
                let required_to_top = from_env.outer.bottom().saturating_add(1);
                if to_env.outer.y >= required_to_top {
                    continue;
                }
                let Some(&shift_rank) = subgraph_min_rank.get(to_sg) else {
                    continue;
                };
                let delta = required_to_top - to_env.outer.y;
                required_shift_by_rank
                    .entry(shift_rank)
                    .and_modify(|d| *d = (*d).max(delta))
                    .or_insert(delta);
            }

            for (subgraph_id, env) in &subgraph_envelopes {
                for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                    if !input
                        .graph
                        .is_node_in_subgraph_tree(&edge.from, subgraph_id)
                        || input.graph.is_node_in_subgraph_tree(&edge.to, subgraph_id)
                    {
                        continue;
                    }
                    // If the destination is inside another subgraph, let that subgraph's
                    // padding handle arrow/label clearance. This rule is specifically for
                    // edges that exit a subgraph into open (non-subgraph) space.
                    if input.graph.get_node_subgraph(&edge.to).is_some() {
                        continue;
                    }
                    let Some(to_rect) = placement.node_rects.get(&edge.to) else {
                        continue;
                    };
                    let required_target_y = env.outer.bottom().saturating_add(1);
                    if to_rect.y >= required_target_y {
                        continue;
                    }
                    let Some(rank) = placement.ranks.get(&edge.to) else {
                        continue;
                    };
                    let delta = required_target_y - to_rect.y;
                    required_shift_by_rank
                        .entry(*rank)
                        .and_modify(|d| *d = (*d).max(delta))
                        .or_insert(delta);
                }
            }

            let Some((&min_rank, &delta_y)) = required_shift_by_rank.iter().min_by_key(|(r, _)| *r)
            else {
                break;
            };

            shift_nodes_from_rank_td(
                &mut placement.positions,
                &mut placement.node_rects,
                &placement.ranks,
                min_rank,
                delta_y,
            );

            let max_bottom = placement
                .node_rects
                .values()
                .map(|r| r.bottom())
                .max()
                .unwrap_or(placement.canvas.bottom());
            placement.canvas.height = placement.canvas.height.max(max_bottom);

            subgraph_envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
        }
    }

    compact_stacked_vertical_top_level_sibling_subgraphs(
        input.graph,
        &mut placement.positions,
        &mut placement.node_rects,
        config.subgraph_gutter,
        &mut placement.canvas.height,
    );
    subgraph_envelopes =
        compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
    adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);

    // BT: ensure clearance above subgraph top borders (for outgoing edges to external
    // targets above) and between stacked subgraphs (so connectors don't overwrite
    // titles/corners on adjacent borders).
    if input.graph.direction == Direction::BT && !subgraph_envelopes.is_empty() {
        for _ in 0..8 {
            // A root subgraph may have an outgoing edge to a top-level node that
            // is ordered above it by rank but still lands inside the enlarged
            // root envelope. Move the complete declared root tree down until
            // the external target is visibly outside. Restrict this to root
            // trees and top-level targets so sibling-subgraph spacing remains
            // owned by the existing cross-subgraph constraints below.
            let mut required_root_tree_shift: HashMap<String, usize> = HashMap::new();
            for (subgraph_id, env) in &subgraph_envelopes {
                let Some(subgraph) = input.graph.get_subgraph(subgraph_id) else {
                    continue;
                };
                if subgraph.parent_id.is_some() {
                    continue;
                }

                for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                    if !input
                        .graph
                        .is_node_in_subgraph_tree(&edge.from, subgraph_id)
                        || input.graph.is_node_in_subgraph_tree(&edge.to, subgraph_id)
                        || input.graph.get_node_subgraph(&edge.to).is_some()
                    {
                        continue;
                    }
                    let Some(target_rect) = placement.node_rects.get(&edge.to) else {
                        continue;
                    };
                    let overlaps = rects_overlap_vertically(*target_rect, env.outer)
                        && rects_overlap_horizontally(*target_rect, env.outer);
                    if !overlaps {
                        continue;
                    }

                    let required_root_y = target_rect.bottom().saturating_add(1);
                    if env.outer.y < required_root_y {
                        let delta = required_root_y - env.outer.y;
                        required_root_tree_shift
                            .entry(subgraph_id.clone())
                            .and_modify(|existing| *existing = (*existing).max(delta))
                            .or_insert(delta);
                    }
                }
            }

            if !required_root_tree_shift.is_empty() {
                let mut shifts: Vec<(String, usize)> =
                    required_root_tree_shift.into_iter().collect();
                shifts.sort_unstable_by(|left, right| left.0.cmp(&right.0));
                for (subgraph_id, delta) in shifts {
                    shift_nodes_in_subgraph_tree_y_signed(
                        input.graph,
                        &mut placement.positions,
                        &mut placement.node_rects,
                        &subgraph_id,
                        delta as isize,
                    );
                }

                let max_bottom = placement
                    .node_rects
                    .values()
                    .map(|rect| rect.bottom())
                    .max()
                    .unwrap_or(placement.canvas.bottom());
                placement.canvas.height = placement.canvas.height.max(max_bottom);
                subgraph_envelopes =
                    compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
                adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
                continue;
            }

            let mut required_shift_by_rank: HashMap<usize, usize> = HashMap::new();

            let mut subgraph_max_rank: HashMap<&str, usize> = HashMap::new();
            for sg in &input.graph.subgraphs {
                let max_rank = subgraph_tree_rank_range(input.graph, &placement.ranks, &sg.id)
                    .map(|(_, max_rank)| max_rank);
                if let Some(r) = max_rank {
                    subgraph_max_rank.insert(sg.id.as_str(), r);
                }
            }

            // Keep at least one connector row between an external target box above and the
            // subgraph top border it is connected to.
            for (sg_id, env) in subgraph_envelopes.iter() {
                let Some(&shift_rank) = subgraph_max_rank.get(sg_id.as_str()) else {
                    continue;
                };
                for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                    let (exit_subgraphs, _) =
                        input.graph.edge_boundary_crossings(&edge.from, &edge.to);
                    if !exit_subgraphs.contains(&sg_id.as_str()) {
                        continue;
                    }
                    let Some(to_rect) = placement.node_rects.get(&edge.to) else {
                        continue;
                    };
                    // Only when the destination is above this envelope.
                    if to_rect.bottom() > env.outer.y.saturating_add(1) {
                        continue;
                    }
                    let required_border_y = to_rect.bottom().saturating_add(1);
                    if env.outer.y >= required_border_y {
                        continue;
                    }
                    let delta = required_border_y - env.outer.y;
                    required_shift_by_rank
                        .entry(shift_rank)
                        .and_modify(|d| *d = (*d).max(delta))
                        .or_insert(delta);
                }
            }

            // Ensure at least one connector row between a subgraph bottom border and any
            // external source node that feeds into content inside that envelope. In BT this
            // matters for both direct targets and visually nested parent envelopes; otherwise
            // an enlarged outer border can land on top of the lower source box.
            for (sg_id, env) in subgraph_envelopes.iter() {
                let Some(subgraph) = input.graph.get_subgraph(sg_id) else {
                    continue;
                };
                let has_title = subgraph.title.is_some();
                if subgraph.parent_id.is_none() && subgraph.child_ids.is_empty() && !has_title {
                    continue;
                }
                for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                    let Some(from_rect) = placement.node_rects.get(&edge.from) else {
                        continue;
                    };
                    if input.graph.is_node_in_subgraph_tree(&edge.from, sg_id) {
                        continue;
                    }
                    if !input.graph.is_node_in_subgraph_tree(&edge.to, sg_id) {
                        continue;
                    }
                    if !rect_fully_inside(env.outer, *from_rect) {
                        continue;
                    }
                    // The source node must start at least one row below the outer envelope
                    // bottom so there is room for the routing connector between them.
                    let required_source_y = env.outer.bottom().saturating_add(1);
                    if from_rect.y >= required_source_y {
                        continue;
                    }
                    let Some(&rank) = placement.ranks.get(&edge.from) else {
                        continue;
                    };
                    let delta = required_source_y - from_rect.y;
                    required_shift_by_rank
                        .entry(rank)
                        .and_modify(|d| *d = (*d).max(delta))
                        .or_insert(delta);
                }
            }

            // Ensure at least one empty row between stacked subgraphs when an edge crosses
            // from the lower subgraph to the upper one (BT flows upward).
            for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                let (Some(from_sg), Some(to_sg)) = (
                    input.graph.get_node_subgraph(&edge.from),
                    input.graph.get_node_subgraph(&edge.to),
                ) else {
                    continue;
                };
                if from_sg == to_sg {
                    continue;
                }
                let (Some(from_env), Some(to_env)) = (
                    subgraph_envelopes.get(from_sg),
                    subgraph_envelopes.get(to_sg),
                ) else {
                    continue;
                };
                // In BT, `to_sg` is visually above `from_sg` (smaller y). Only skip if
                // subgraphs are truly nested (one fully inside the other).
                let is_nested = rect_fully_inside(from_env.outer, to_env.outer)
                    || rect_fully_inside(to_env.outer, from_env.outer);
                if is_nested {
                    continue;
                }
                let required_from_top = to_env.outer.bottom().saturating_add(1);
                if from_env.outer.y >= required_from_top {
                    continue;
                }
                let Some(&shift_rank) = subgraph_max_rank.get(from_sg) else {
                    continue;
                };
                let delta = required_from_top - from_env.outer.y;
                required_shift_by_rank
                    .entry(shift_rank)
                    .and_modify(|d| *d = (*d).max(delta))
                    .or_insert(delta);
            }

            let Some((&max_rank, &delta_y)) = required_shift_by_rank.iter().max_by_key(|(r, _)| *r)
            else {
                break;
            };

            shift_nodes_up_to_rank_bt(
                &mut placement.positions,
                &mut placement.node_rects,
                &placement.ranks,
                max_rank,
                delta_y,
            );

            let max_bottom = placement
                .node_rects
                .values()
                .map(|r| r.bottom())
                .max()
                .unwrap_or(placement.canvas.bottom());
            placement.canvas.height = placement.canvas.height.max(max_bottom);

            subgraph_envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
        }

        let mut source_shifts: HashMap<String, usize> = HashMap::new();
        for (subgraph_id, env) in subgraph_envelopes.iter() {
            let has_title = input
                .graph
                .get_subgraph(subgraph_id)
                .and_then(|subgraph| subgraph.title.as_ref())
                .is_some();
            let contains_child_envelope = subgraph_envelopes.iter().any(|(other_id, other_env)| {
                other_id != subgraph_id && rect_fully_inside(env.outer, other_env.outer)
            });
            if !contains_child_envelope && !has_title {
                continue;
            }
            let required_source_y = env.outer.bottom().saturating_add(1);
            for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                let (Some(from_rect), Some(to_rect)) = (
                    placement.node_rects.get(&edge.from),
                    placement.node_rects.get(&edge.to),
                ) else {
                    continue;
                };
                if rect_fully_inside(env.outer, *from_rect)
                    || !rect_fully_inside(env.outer, *to_rect)
                {
                    continue;
                }
                if input.graph.get_node_subgraph(&edge.from).is_some() {
                    continue;
                }
                let overlaps_envelope_horizontally =
                    from_rect.x < env.outer.right() && env.outer.x < from_rect.right();
                if !overlaps_envelope_horizontally || from_rect.y >= required_source_y {
                    continue;
                }

                let delta = required_source_y - from_rect.y;
                source_shifts
                    .entry(edge.from.clone())
                    .and_modify(|existing| *existing = (*existing).max(delta))
                    .or_insert(delta);
            }
        }

        if !source_shifts.is_empty() {
            shift_nodes_by_id_y(
                &mut placement.positions,
                &mut placement.node_rects,
                &source_shifts,
            );
            let max_bottom = placement
                .node_rects
                .values()
                .map(|r| r.bottom())
                .max()
                .unwrap_or(placement.canvas.bottom());
            placement.canvas.height = placement.canvas.height.max(max_bottom);
        }
    }

    // Warn about overlapping (but not nested) subgraphs that couldn't be resolved.
    if debug_timing && subgraph_envelopes.len() > 1 {
        let sg_ids: Vec<&String> = subgraph_envelopes.keys().collect();
        for i in 0..sg_ids.len() {
            for j in (i + 1)..sg_ids.len() {
                let env1 = &subgraph_envelopes[sg_ids[i]];
                let env2 = &subgraph_envelopes[sg_ids[j]];
                // Check if they intersect
                let intersects = env1.outer.x < env2.outer.right()
                    && env1.outer.right() > env2.outer.x
                    && env1.outer.y < env2.outer.bottom()
                    && env1.outer.bottom() > env2.outer.y;
                if intersects {
                    let nested = rect_fully_inside(env1.outer, env2.outer)
                        || rect_fully_inside(env2.outer, env1.outer);
                    if !nested {
                        eprintln!(
                            "termiflow: warning: subgraphs {} and {} overlap",
                            sg_ids[i], sg_ids[j]
                        );
                    }
                }
            }
        }
    }

    rebalance_titled_vertical_subgraph_content_x(
        input.graph,
        &mut placement.positions,
        &mut placement.node_rects,
        config.subgraph_gutter,
        &mut placement.canvas.width,
    );
    rebalance_titled_vertical_subgraph_content_y(
        input.graph,
        &mut placement.positions,
        &mut placement.node_rects,
        config.subgraph_gutter,
        &mut placement.canvas.height,
    );
    subgraph_envelopes =
        compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
    adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);

    enforce_declared_nested_envelopes(input.graph, &mut subgraph_envelopes);
    adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);

    // The earlier vertical constraint loop also protects nested title bands. Those
    // lower-rank constraints can consume its bounded passes before an external target
    // is moved far enough below a parent whose envelope grew from nested content. Apply
    // the final graph-membership-aware target clearance after all envelope expansion so
    // a top-level target cannot remain geometrically inside a declared root tree.
    if matches!(input.graph.direction, Direction::TD | Direction::TB)
        && !subgraph_envelopes.is_empty()
    {
        for _ in 0..8 {
            let mut required_shift_by_rank: HashMap<usize, usize> = HashMap::new();
            for (subgraph_id, env) in &subgraph_envelopes {
                for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                    if !input
                        .graph
                        .is_node_in_subgraph_tree(&edge.from, subgraph_id)
                        || input.graph.is_node_in_subgraph_tree(&edge.to, subgraph_id)
                        || input.graph.get_node_subgraph(&edge.to).is_some()
                    {
                        continue;
                    }
                    let Some(to_rect) = placement.node_rects.get(&edge.to) else {
                        continue;
                    };
                    let required_target_y = env.outer.bottom().saturating_add(1);
                    if to_rect.y >= required_target_y {
                        continue;
                    }
                    let Some(&rank) = placement.ranks.get(&edge.to) else {
                        continue;
                    };
                    let delta = required_target_y - to_rect.y;
                    required_shift_by_rank
                        .entry(rank)
                        .and_modify(|existing| *existing = (*existing).max(delta))
                        .or_insert(delta);
                }
            }

            let Some((&min_rank, &delta_y)) =
                required_shift_by_rank.iter().min_by_key(|(rank, _)| *rank)
            else {
                break;
            };

            shift_nodes_from_rank_td(
                &mut placement.positions,
                &mut placement.node_rects,
                &placement.ranks,
                min_rank,
                delta_y,
            );

            let max_bottom = placement
                .node_rects
                .values()
                .map(|rect| rect.bottom())
                .max()
                .unwrap_or(placement.canvas.bottom());
            placement.canvas.height = placement.canvas.height.max(max_bottom);
            subgraph_envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
            enforce_declared_nested_envelopes(input.graph, &mut subgraph_envelopes);
            adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
        }
    }

    subgraph_envelopes
}
