//! Direction and gutter normalization for the coarse layout pipeline.

use std::collections::HashMap;

use crate::graph::{Direction, Graph};
use crate::portals::compute_envelopes;
use crate::style::BOX_HEIGHT;

use super::constraints::{
    rebalance_side_by_side_horizontal_top_level_sibling_gaps, rect_fully_inside,
    reserve_nested_horizontal_subgraph_headroom, shift_nodes_by_id_x, shift_nodes_from_x,
};
use super::placement::Placement;
use super::CoarseLayoutConfig;

/// Normalize oriented placement coordinates before stage-3 envelope construction.
///
/// This is intentionally a placement-only stage: it owns direction flips,
/// gutter padding, nested headroom, horizontal boundary clearance, and sibling
/// gap balancing, but not subgraph envelope construction or routing.
pub(super) fn normalize_orientation_and_gutters(
    graph: &Graph,
    config: &CoarseLayoutConfig,
    placement: &mut Placement,
) {
    // 2.5) Flip coordinates for BT/RL to match flow direction
    // Calculate strict content bounds
    let max_x = placement
        .node_rects
        .values()
        .map(|r| r.right())
        .max()
        .unwrap_or(0);
    let max_y = placement
        .node_rects
        .values()
        .map(|r| r.bottom())
        .max()
        .unwrap_or(0);

    if graph.direction == Direction::BT {
        for (id, p) in placement.positions.iter_mut() {
            let h = placement
                .node_rects
                .get(id)
                .map(|r| r.height)
                .unwrap_or(BOX_HEIGHT);
            p.y = max_y.saturating_sub(p.y).saturating_sub(h);
        }
        for r in placement.node_rects.values_mut() {
            r.y = max_y.saturating_sub(r.y).saturating_sub(r.height);
        }
    } else if graph.direction == Direction::RL {
        // Easier: Iterate keys of positions (node ids)
        for (id, p) in placement.positions.iter_mut() {
            if let Some(r) = placement.node_rects.get_mut(id) {
                let new_x = max_x.saturating_sub(r.x + r.width);
                p.x = new_x;
                r.x = new_x;
            }
        }
    }

    // Shift nodes to make room for subgraph gutters if any subgraphs exist
    if !graph.subgraphs.is_empty() {
        let shift = config.subgraph_gutter;
        for p in placement.positions.values_mut() {
            p.x += shift;
            p.y += shift;
        }
        for r in placement.node_rects.values_mut() {
            r.x += shift;
            r.y += shift;
        }
        // Canvas grows by the shift amount (padding on both sides)
        placement.canvas.width = max_x + shift * 2;
        placement.canvas.height = max_y + shift * 2;
    } else {
        // Tighten canvas to content if no subgraphs (optional, but cleaner)
        placement.canvas.width = max_x;
        placement.canvas.height = max_y;
    }

    reserve_nested_horizontal_subgraph_headroom(
        graph,
        &mut placement.positions,
        &mut placement.node_rects,
        config.subgraph_gutter,
        &mut placement.canvas.height,
    );

    if matches!(graph.direction, Direction::LR | Direction::RL) && !graph.subgraphs.is_empty() {
        for _ in 0..8 {
            let envelopes = compute_envelopes(graph, &placement.node_rects, config.subgraph_gutter);
            let mut required_env_shift: Option<(usize, usize)> = None;
            let mut external_node_shifts: HashMap<String, usize> = HashMap::new();

            for (subgraph_id, env) in &envelopes {
                for edge in graph.edges.iter().filter(|edge| !edge.is_back_edge) {
                    let (Some(from_rect), Some(to_rect)) = (
                        placement.node_rects.get(&edge.from),
                        placement.node_rects.get(&edge.to),
                    ) else {
                        continue;
                    };

                    let from_inside_tree = graph.is_node_in_subgraph_tree(&edge.from, subgraph_id);
                    let to_inside_tree = graph.is_node_in_subgraph_tree(&edge.to, subgraph_id);
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
                        graph.get_node_subgraph(&edge.to).is_some()
                    } else {
                        graph.get_node_subgraph(&edge.from).is_some()
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

                // A top-level node can sit between two side-by-side
                // subgraphs without sharing an edge with the subgraph whose
                // wall it overlaps.  The edge-driven pass above cannot see
                // that collision, so the later renderer receives overlapping
                // node/subgraph rectangles and loses target-entry ownership.
                // Treat only direct top-level nodes and top-level envelopes as
                // foreign here; declared parent/child content is intentionally
                // allowed to occupy its parent's interior composition.
                let Some(subgraph) = graph.get_subgraph(subgraph_id) else {
                    continue;
                };
                if subgraph.parent_id.is_some() {
                    continue;
                }
                for (node_id, external_rect) in &placement.node_rects {
                    if graph.get_node_subgraph(node_id).is_some()
                        || graph.is_node_in_subgraph_tree(node_id, subgraph_id)
                        || external_rect.y >= env.outer.bottom()
                        || env.outer.y >= external_rect.bottom()
                    {
                        continue;
                    }

                    if external_rect.x < env.outer.x {
                        if external_rect.right() <= env.outer.x {
                            continue;
                        }
                        let required_env_x = external_rect.right().saturating_add(2);
                        let threshold_x = env.outer.x;
                        let delta_x = required_env_x.saturating_sub(env.outer.x);
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
                    } else if external_rect.x < env.outer.right() {
                        let required_external_x = env.outer.right().saturating_add(2);
                        let delta_x = required_external_x.saturating_sub(external_rect.x);
                        external_node_shifts
                            .entry(node_id.clone())
                            .and_modify(|existing| *existing = (*existing).max(delta_x))
                            .or_insert(delta_x);
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
        }
    }

    rebalance_side_by_side_horizontal_top_level_sibling_gaps(
        graph,
        &mut placement.positions,
        &mut placement.node_rects,
        config.subgraph_gutter,
        &mut placement.canvas.width,
    );
}
