//! Subgraph-envelope and placement-constraint orchestration.

use std::collections::{HashMap, HashSet};

use crate::geom::Rect;
use crate::graph::{Direction, EdgeKind, Graph, NodeShape};
use crate::portals::{
    bt_sibling_chain_target_ids, compute_envelopes, nudge_portal_x_from_corners,
    title_safe_portal_x, PortalColumnPreference, SubgraphEnvelope, BT_SIBLING_CHAIN_TITLE_MARGIN,
};
use crate::render::sibling_subgraph_fan_in_identity;

use super::constraints::*;
use super::placement::Placement;
use super::{adjust_portal_slots_for_title, CoarseLayoutConfig, LayoutInput};

const BT_SIBLING_CORRIDOR_SIDE_GAP: usize = 2;
const BT_EXTERNAL_NODE_KEEP_OUT: usize = 1;
const BT_MULTI_ENTRY_SOURCE_CLEARANCE: usize = 2;
const BT_MULTI_ENTRY_MIN_LANE_GAP: usize = 3;
const TD_SIBLING_CONNECTOR_ROWS: usize = 2;

/// Return the source-to-envelope clearance needed by a complete BT multi-entry
/// scene. A one-row gap is enough for ordinary boundary edges, but a shared
/// multi-entry corridor needs one additional empty row so its horizontal run
/// cannot become a junction with the top corners of the source boxes.
///
/// The predicate intentionally mirrors the renderer's typed scene planner:
/// it is based only on graph topology, title metadata, and edge style, never on
/// a fixture name or a particular node label.
fn bt_multi_entry_source_clearance(graph: &Graph, subgraph_id: &str) -> usize {
    if graph.direction != Direction::BT {
        return 1;
    }
    let Some(subgraph) = graph.get_subgraph(subgraph_id) else {
        return 1;
    };
    if subgraph.title.is_none() {
        return 1;
    }

    let candidate_edges: Vec<_> = graph
        .edges
        .iter()
        .filter(|edge| {
            !edge.is_back_edge
                && edge.kind == EdgeKind::Arrow
                && graph.get_node_subgraph(&edge.from).is_none()
                && graph.get_node_subgraph(&edge.to) == Some(subgraph_id)
                && subgraph.node_ids.contains(&edge.to)
                && graph
                    .edge_boundary_crossings(&edge.from, &edge.to)
                    .0
                    .is_empty()
                && graph.edge_boundary_crossings(&edge.from, &edge.to).1 == vec![subgraph_id]
        })
        .collect();
    let candidate_source_ids: HashSet<&str> = candidate_edges
        .iter()
        .map(|edge| edge.from.as_str())
        .collect();
    if candidate_edges.len() < 3 || candidate_source_ids.len() < 3 {
        return 1;
    }

    let all_entries = graph
        .edges
        .iter()
        .filter(|edge| {
            !edge.is_back_edge
                && graph.edge_boundary_crossings(&edge.from, &edge.to).1 == vec![subgraph_id]
        })
        .count();
    if all_entries == candidate_edges.len() {
        BT_MULTI_ENTRY_SOURCE_CLEARANCE
    } else {
        1
    }
}

/// Return the top-to-bottom corridor occupied by an exact sibling BT edge.
///
/// This is deliberately derived from boundary crossings and live envelopes. It
/// does not identify fixtures or assume a particular node name. The renderer's
/// sibling fallback owns the route itself; this helper only identifies the
/// upstream geometry that must leave room for that route.
fn bt_sibling_corridors(
    graph: &Graph,
    envelopes: &HashMap<String, SubgraphEnvelope>,
) -> Vec<(String, String, Rect)> {
    let mut corridors = Vec::new();

    for edge in graph.edges.iter().filter(|edge| !edge.is_back_edge) {
        let (exit_subgraphs, enter_subgraphs) = graph.edge_boundary_crossings(&edge.from, &edge.to);
        if exit_subgraphs.len() != 1 || enter_subgraphs.len() != 1 {
            continue;
        }

        let source_id = exit_subgraphs[0];
        let target_id = enter_subgraphs[0];
        if source_id == target_id {
            continue;
        }
        let (Some(source), Some(target)) = (envelopes.get(source_id), envelopes.get(target_id))
        else {
            continue;
        };

        let corridor_top = target.outer.bottom();
        let corridor_bottom = source.outer.y;
        if corridor_bottom <= corridor_top {
            continue;
        }

        let corridor_left = source.outer.x.min(target.outer.x);
        let corridor_right = source.outer.right().max(target.outer.right());
        corridors.push((
            source_id.to_owned(),
            target_id.to_owned(),
            Rect::new(
                corridor_left,
                corridor_top,
                corridor_right.saturating_sub(corridor_left),
                corridor_bottom - corridor_top,
            ),
        ));
    }

    corridors.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.y.cmp(&right.2.y))
            .then_with(|| left.2.x.cmp(&right.2.x))
    });
    corridors.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1);
    corridors
}

/// Require an external node to participate in the sibling topology before it
/// can be relocated. This keeps the reservation local to nodes that can
/// actually pressure the source/target boundary route.
fn bt_external_node_touches_sibling_tree(
    graph: &Graph,
    node_id: &str,
    source_id: &str,
    target_id: &str,
) -> bool {
    graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge)
        .any(|edge| {
            if edge.from == node_id {
                graph.is_node_in_subgraph_tree(&edge.to, source_id)
                    || graph.is_node_in_subgraph_tree(&edge.to, target_id)
            } else if edge.to == node_id {
                graph.is_node_in_subgraph_tree(&edge.from, source_id)
                    || graph.is_node_in_subgraph_tree(&edge.from, target_id)
            } else {
                false
            }
        })
}

fn rects_overlap(a: Rect, b: Rect) -> bool {
    rects_overlap_horizontally(a, b) && rects_overlap_vertically(a, b)
}

/// Build obstacles for an external-node side placement.
///
/// Subgraph envelopes are the hard visual boundary. Other nodes receive their
/// normal one-cell keepout so a relocation cannot create a new touching box.
fn bt_external_relocation_obstacles(
    node_rects: &HashMap<String, Rect>,
    envelopes: &HashMap<String, SubgraphEnvelope>,
    node_id: &str,
) -> Vec<Rect> {
    let mut obstacles = envelopes.values().map(|env| env.outer).collect::<Vec<_>>();
    obstacles.extend(
        node_rects
            .iter()
            .filter(|(other_id, _)| other_id.as_str() != node_id)
            .map(|(_, rect)| rect.inflate(BT_EXTERNAL_NODE_KEEP_OUT)),
    );

    obstacles.sort_by_key(|rect| (rect.x, rect.y, rect.width, rect.height));
    obstacles
}

fn first_clear_right_x(current: Rect, start_x: usize, obstacles: &[Rect]) -> Option<usize> {
    let mut candidate_x = start_x;
    let max_attempts = obstacles.len().saturating_add(2).max(2);
    for _ in 0..max_attempts {
        let candidate = Rect::new(candidate_x, current.y, current.width, current.height);
        let conflicts: Vec<Rect> = obstacles
            .iter()
            .copied()
            .filter(|obstacle| rects_overlap(candidate, *obstacle))
            .collect();
        if conflicts.is_empty() {
            return Some(candidate_x);
        }

        let next_x = conflicts
            .iter()
            .map(|obstacle| {
                obstacle
                    .right()
                    .saturating_add(BT_SIBLING_CORRIDOR_SIDE_GAP)
            })
            .max()
            .unwrap_or(candidate_x);
        if next_x <= candidate_x {
            return None;
        }
        candidate_x = next_x;
    }
    None
}

fn first_clear_left_x(current: Rect, start_x: usize, obstacles: &[Rect]) -> Option<usize> {
    let mut candidate_x = start_x;
    let max_attempts = obstacles.len().saturating_add(2).max(2);
    for _ in 0..max_attempts {
        let candidate = Rect::new(candidate_x, current.y, current.width, current.height);
        let conflicts: Vec<Rect> = obstacles
            .iter()
            .copied()
            .filter(|obstacle| rects_overlap(candidate, *obstacle))
            .collect();
        if conflicts.is_empty() {
            return Some(candidate_x);
        }

        let next_x = conflicts
            .iter()
            .map(|obstacle| {
                obstacle
                    .x
                    .saturating_sub(current.width + BT_SIBLING_CORRIDOR_SIDE_GAP)
            })
            .min()?;
        if next_x >= candidate_x {
            return None;
        }
        candidate_x = next_x;
    }
    None
}

fn set_node_x(placement: &mut Placement, node_id: &str, x: usize) -> bool {
    let Some(rect) = placement.node_rects.get_mut(node_id) else {
        return false;
    };
    rect.x = x;
    if let Some(position) = placement.positions.get_mut(node_id) {
        position.x = x;
    }
    true
}

/// Align a strict titled BT sibling chain to one topology-owned title-safe
/// lane before envelopes and routes are finalized.
///
/// The fallback router can avoid a BT title by moving a portal column after
/// node placement, but that produces a visible horizontal hook when the
/// connected node centers are still on the old column. A complete sibling
/// chain has one boundary endpoint per member; when those endpoints are
/// already aligned, move the whole direct content together so the portal and
/// node lane share one immutable column. Mixed or ambiguous chains fail closed
/// and retain the existing fallback policy.
fn align_bt_sibling_chain_content(
    graph: &Graph,
    config: &CoarseLayoutConfig,
    placement: &mut Placement,
    envelopes: &mut HashMap<String, SubgraphEnvelope>,
) {
    if graph.direction != Direction::BT {
        return;
    }

    let previous_bounds: HashMap<String, (Rect, Rect)> = envelopes
        .iter()
        .map(|(id, envelope)| (id.clone(), (envelope.outer, envelope.inner)))
        .collect();
    let bounds: HashMap<String, Rect> = envelopes
        .iter()
        .map(|(id, envelope)| (id.clone(), envelope.outer))
        .collect();

    // The old alignment collapsed the middle sibling's incoming target role
    // and outgoing source role onto one center.  Keep the internal edge
    // vertical, but allocate adjacent cross-boundary transitions on distinct
    // content lanes so the target may accept the previous lane along its
    // bottom edge without a boundary elbow.
    let role_lane_plan = crate::layout_render_contract::plan_bt_sibling_content_lanes(
        graph,
        &placement.node_rects,
        &bounds,
    );
    if let Some(plan) = role_lane_plan {
        if apply_bt_sibling_role_lane_plan(graph, config, placement, envelopes, &plan) {
            return;
        }
        return;
    }

    let Some(target_ids) = bt_sibling_chain_target_ids(graph, &bounds) else {
        return;
    };

    let mut chain_ids = target_ids.clone();
    let mut endpoint_centers: HashMap<String, usize> = HashMap::new();
    let mut shared_lane = 0;

    for edge in graph.edges.iter().filter(|edge| !edge.is_back_edge) {
        let (exit_subgraphs, enter_subgraphs) = graph.edge_boundary_crossings(&edge.from, &edge.to);
        if exit_subgraphs.len() != 1
            || enter_subgraphs.len() != 1
            || !target_ids.contains(enter_subgraphs[0])
        {
            continue;
        }

        let source_id = exit_subgraphs[0].to_owned();
        let target_id = enter_subgraphs[0].to_owned();
        chain_ids.insert(source_id.clone());
        chain_ids.insert(target_id.clone());
        for (subgraph_id, node_id) in [(&source_id, &edge.from), (&target_id, &edge.to)] {
            let Some(rect) = placement.node_rects.get(node_id).copied() else {
                return;
            };
            let center = rect_center_x(rect);
            if endpoint_centers
                .insert(subgraph_id.clone(), center)
                .is_some_and(|prior| prior != center)
            {
                return;
            }
            shared_lane = shared_lane.max(center);
        }

        let Some(target_envelope) = envelopes.get(&target_id) else {
            return;
        };
        let Some(target_subgraph) = graph.get_subgraph(&target_id) else {
            return;
        };
        let Some(target_rect) = placement.node_rects.get(&edge.to).copied() else {
            return;
        };
        let min_lane = target_envelope.outer.x.saturating_add(2);
        let max_lane = target_envelope.outer.x + target_envelope.outer.width.saturating_sub(3);
        let Some(title_safe_lane) = (min_lane..=max_lane)
            .filter(|candidate| {
                title_safe_portal_x(
                    target_envelope.outer.x,
                    target_envelope.outer.width,
                    target_subgraph.title.as_deref(),
                    *candidate,
                    Direction::BT,
                    BT_SIBLING_CHAIN_TITLE_MARGIN,
                    PortalColumnPreference::Nearest,
                ) == *candidate
            })
            .min_by_key(|candidate| candidate.abs_diff(rect_center_x(target_rect)))
        else {
            return;
        };
        shared_lane = shared_lane.max(title_safe_lane);
    }

    if chain_ids.len() < 3 || endpoint_centers.len() != chain_ids.len() {
        return;
    }

    let mut moved = false;
    for (subgraph_id, center) in endpoint_centers {
        let delta_x = shared_lane.saturating_sub(center);
        if delta_x == 0 {
            continue;
        }
        let Some(subgraph) = graph.get_subgraph(&subgraph_id) else {
            return;
        };
        for node_id in &subgraph.node_ids {
            let Some(rect) = placement.node_rects.get(node_id).copied() else {
                return;
            };
            if !set_node_x(placement, node_id, rect.x.saturating_add(delta_x)) {
                return;
            }
        }
        moved = true;
    }

    if !moved {
        return;
    }

    placement.canvas.width = placement.canvas.width.max(
        placement
            .node_rects
            .values()
            .map(Rect::right)
            .max()
            .unwrap_or(0),
    );
    *envelopes = compute_envelopes(graph, &placement.node_rects, config.subgraph_gutter);
    for subgraph_id in &chain_ids {
        let Some((previous_outer, previous_inner)) = previous_bounds.get(subgraph_id) else {
            return;
        };
        let Some(envelope) = envelopes.get_mut(subgraph_id) else {
            return;
        };
        if envelope.outer.x > previous_outer.x {
            let right = envelope.outer.right();
            envelope.outer.x = previous_outer.x;
            envelope.outer.width = right.saturating_sub(previous_outer.x);
        }
        if envelope.inner.x > previous_inner.x {
            let right = envelope.inner.right();
            envelope.inner.x = previous_inner.x;
            envelope.inner.width = right.saturating_sub(previous_inner.x);
        }
    }
    placement.canvas.width = placement.canvas.width.max(
        envelopes
            .values()
            .map(|envelope| envelope.outer.right())
            .max()
            .unwrap_or(0),
    );
    adjust_portal_slots_for_title(envelopes, graph);

    // If the first role-aware pass ran before the legacy envelope rebalance
    // had equalized internal centers, the compatibility alignment above is a
    // seed only. Re-run the same typed allocator on the now-final geometry so
    // the renderer never receives the collapsed one-center result.
    let final_bounds: HashMap<String, Rect> = envelopes
        .iter()
        .map(|(id, envelope)| (id.clone(), envelope.outer))
        .collect();
    if let Some(plan) = crate::layout_render_contract::plan_bt_sibling_content_lanes(
        graph,
        &placement.node_rects,
        &final_bounds,
    ) {
        let _ = apply_bt_sibling_role_lane_plan(graph, config, placement, envelopes, &plan);
    }
}

fn apply_bt_sibling_role_lane_plan(
    graph: &Graph,
    config: &CoarseLayoutConfig,
    placement: &mut Placement,
    envelopes: &mut HashMap<String, SubgraphEnvelope>,
    plan: &crate::layout_render_contract::BtSiblingLanePlan,
) -> bool {
    let previous_bounds: HashMap<String, (Rect, Rect)> = envelopes
        .iter()
        .map(|(id, envelope)| (id.clone(), (envelope.outer, envelope.inner)))
        .collect();
    let mut moved = false;
    for (subgraph_id, desired_lane) in &plan.content_lanes {
        let Some(&reference_center) = plan.reference_centers.get(subgraph_id) else {
            return false;
        };
        let delta = *desired_lane as isize - reference_center as isize;
        if delta == 0 {
            continue;
        }
        let Some(subgraph) = graph.get_subgraph(subgraph_id) else {
            return false;
        };
        for node_id in &subgraph.node_ids {
            let Some(rect) = placement.node_rects.get(node_id).copied() else {
                return false;
            };
            let next_x = if delta.is_negative() {
                rect.x.saturating_sub(delta.unsigned_abs())
            } else {
                rect.x.saturating_add(delta as usize)
            };
            if !set_node_x(placement, node_id, next_x) {
                return false;
            }
        }
        moved = true;
    }
    if !moved {
        align_bt_sibling_envelope_columns(envelopes, plan);
        adjust_portal_slots_for_title(envelopes, graph);
        return true;
    }
    placement.canvas.width = placement.canvas.width.max(
        placement
            .node_rects
            .values()
            .map(Rect::right)
            .max()
            .unwrap_or(0),
    );
    *envelopes = compute_envelopes(graph, &placement.node_rects, config.subgraph_gutter);
    for (subgraph_id, (previous_outer, previous_inner)) in previous_bounds {
        let Some(envelope) = envelopes.get_mut(&subgraph_id) else {
            return false;
        };
        if envelope.outer.x > previous_outer.x {
            let right = envelope.outer.right().max(previous_outer.right());
            envelope.outer.x = previous_outer.x;
            envelope.outer.width = right.saturating_sub(previous_outer.x);
        }
        if envelope.inner.x > previous_inner.x {
            let right = envelope.inner.right().max(previous_inner.right());
            envelope.inner.x = previous_inner.x;
            envelope.inner.width = right.saturating_sub(previous_inner.x);
        }
    }
    align_bt_sibling_envelope_columns(envelopes, plan);
    adjust_portal_slots_for_title(envelopes, graph);
    true
}

/// Keep a strict vertical sibling chain on one visible frame column after
/// role-aware content shifts. The role allocator is allowed to widen a frame
/// when a target accepts a neighboring transition lane, but it must not turn
/// that width change into a staircase of left/right border columns.
fn align_bt_sibling_envelope_columns(
    envelopes: &mut HashMap<String, SubgraphEnvelope>,
    plan: &crate::layout_render_contract::BtSiblingLanePlan,
) {
    let chain_ids: Vec<&str> = plan
        .content_lanes
        .iter()
        .map(|(subgraph_id, _)| subgraph_id.as_str())
        .collect();
    let Some(shared_left) = chain_ids
        .iter()
        .filter_map(|id| envelopes.get(*id).map(|envelope| envelope.outer.x))
        .min()
    else {
        return;
    };
    let Some(shared_right) = chain_ids
        .iter()
        .filter_map(|id| envelopes.get(*id).map(|envelope| envelope.outer.right()))
        .max()
    else {
        return;
    };
    let shared_width = shared_right.saturating_sub(shared_left);
    if shared_width == 0 {
        return;
    }

    for subgraph_id in chain_ids {
        if let Some(envelope) = envelopes.get_mut(subgraph_id) {
            envelope.outer.x = shared_left;
            envelope.outer.width = shared_width;
        }
    }
}

fn rect_is_inside(inner: Rect, outer: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.right() <= outer.right()
        && inner.bottom() <= outer.bottom()
}

fn centered_rect_at_x(rect: Rect, center_x: usize) -> Option<Rect> {
    let x = center_x.checked_sub(rect.width / 2)?;
    Some(Rect::new(x, rect.y, rect.width, rect.height))
}

/// Align only the two ordinary external nodes of the exact parallel TD scene
/// with the already-selected internal portal lanes. The move is staged and
/// validated before either rectangle is mutated, so unsafe capacity fails
/// closed without a partial placement change.
fn align_td_external_portal_centers(
    graph: &Graph,
    placement: &mut Placement,
    envelopes: &HashMap<String, SubgraphEnvelope>,
) -> bool {
    let Some((subgraph_id, entry_external, entry_internal, exit_internal, exit_external)) =
        graph.td_parallel_external_attachment_ids()
    else {
        return false;
    };
    let Some(envelope) = envelopes.get(&subgraph_id) else {
        return false;
    };
    if envelope.portals.top.len() != 1 || envelope.portals.bottom.len() != 1 {
        return false;
    }

    let Some(entry_rect) = placement.node_rects.get(&entry_external).copied() else {
        return false;
    };
    let Some(entry_internal_rect) = placement.node_rects.get(&entry_internal).copied() else {
        return false;
    };
    let Some(exit_rect) = placement.node_rects.get(&exit_external).copied() else {
        return false;
    };
    let Some(exit_internal_rect) = placement.node_rects.get(&exit_internal).copied() else {
        return false;
    };

    let Some(&entry_lane) = envelope.portals.top.iter().next() else {
        return false;
    };
    let Some(&exit_lane) = envelope.portals.bottom.iter().next() else {
        return false;
    };
    let entry_internal_center = entry_internal_rect.x + entry_internal_rect.width / 2;
    let exit_internal_center = exit_internal_rect.x + exit_internal_rect.width / 2;
    if entry_lane != entry_internal_center || exit_lane != exit_internal_center {
        return false;
    }

    let Some(entry_candidate) = centered_rect_at_x(entry_rect, entry_lane) else {
        return false;
    };
    let Some(exit_candidate) = centered_rect_at_x(exit_rect, exit_lane) else {
        return false;
    };
    let canvas = placement.canvas;
    if !rect_is_inside(entry_candidate, canvas)
        || !rect_is_inside(exit_candidate, canvas)
        || rects_overlap(entry_candidate, envelope.outer)
        || rects_overlap(exit_candidate, envelope.outer)
        || rects_overlap(entry_candidate, exit_candidate)
    {
        return false;
    }

    for (node_id, node_rect) in &placement.node_rects {
        if node_id != &entry_external
            && node_id != &exit_external
            && (rects_overlap(entry_candidate, *node_rect)
                || rects_overlap(exit_candidate, *node_rect))
        {
            return false;
        }
    }

    let changed = entry_rect.x != entry_candidate.x || exit_rect.x != exit_candidate.x;
    if !changed {
        return false;
    }
    if !set_node_x(&mut *placement, &entry_external, entry_candidate.x)
        || !set_node_x(&mut *placement, &exit_external, exit_candidate.x)
    {
        return false;
    }
    true
}

/// Align a single external TD/TB fan-out source with its shared top portal.
///
/// The renderer deliberately lowers this topology through one boundary lane.
/// If an even-width source lands one cell beside that lane, the shortest legal
/// grid route necessarily emits adjacent corners on the approach row. Move the
/// source only when the topology has one unambiguous external fan-out source,
/// and validate the candidate before mutating placement so ambiguous or
/// crowded scenes retain their existing fail-closed route policy.
fn align_td_external_fanout_centers(
    graph: &Graph,
    placement: &mut Placement,
    envelopes: &HashMap<String, SubgraphEnvelope>,
) -> bool {
    if !matches!(graph.direction, Direction::TD | Direction::TB) {
        return false;
    }

    let mut proposals: Vec<(String, usize, Rect)> = Vec::new();
    for subgraph in &graph.subgraphs {
        if subgraph.parent_id.is_some() || !subgraph.child_ids.is_empty() || !subgraph.has_title() {
            continue;
        }
        let Some(envelope) = envelopes.get(&subgraph.id) else {
            continue;
        };
        if envelope.portals.top.len() != 1 {
            continue;
        }

        let entries: Vec<_> = graph
            .edges
            .iter()
            .filter(|edge| {
                !edge.is_back_edge
                    && edge.kind == EdgeKind::Arrow
                    && edge.label.is_none()
                    && graph.get_node_subgraph(&edge.from).is_none()
                    && graph.get_node_subgraph(&edge.to) == Some(subgraph.id.as_str())
                    && graph
                        .edge_boundary_crossings(&edge.from, &edge.to)
                        .0
                        .is_empty()
                    && graph.edge_boundary_crossings(&edge.from, &edge.to).1
                        == vec![subgraph.id.as_str()]
            })
            .collect();
        let source_ids: HashSet<&str> = entries.iter().map(|edge| edge.from.as_str()).collect();
        let target_ids: HashSet<&str> = entries.iter().map(|edge| edge.to.as_str()).collect();
        if entries.len() < 2 || source_ids.len() != 1 || target_ids.len() < 2 {
            continue;
        }

        let source_id = entries[0].from.clone();
        let Some(source_rect) = placement.node_rects.get(&source_id).copied() else {
            continue;
        };
        let Some(&portal_lane) = envelope.portals.top.iter().next() else {
            continue;
        };
        let Some(candidate) = centered_rect_at_x(source_rect, portal_lane) else {
            continue;
        };
        if candidate == source_rect {
            continue;
        }
        if !rect_is_inside(candidate, placement.canvas)
            || rects_overlap(candidate, envelope.outer)
            || graph
                .nodes
                .iter()
                .filter(|node| node.id != source_id)
                .filter_map(|node| placement.node_rects.get(&node.id))
                .any(|rect| rects_overlap(candidate, *rect))
        {
            continue;
        }

        if proposals.iter().any(|(existing_id, existing_x, _)| {
            existing_id == &source_id && *existing_x != candidate.x
        }) {
            return false;
        }
        if !proposals
            .iter()
            .any(|(existing_id, _, _)| existing_id == &source_id)
        {
            proposals.push((source_id, candidate.x, candidate));
        }
    }

    if proposals.is_empty() {
        return false;
    }
    if proposals
        .iter()
        .enumerate()
        .any(|(index, (_, _, candidate))| {
            proposals
                .iter()
                .skip(index + 1)
                .any(|(_, _, other)| rects_overlap(*candidate, *other))
        })
    {
        return false;
    }

    let mut changed = false;
    for (source_id, candidate_x, _) in proposals {
        changed |= set_node_x(placement, &source_id, candidate_x);
    }
    changed
}

/// Align a small, one-to-one set of external TD/TB sources with the live
/// title-safe portal lane of one flat titled subgraph.
///
/// A source box whose center is beside that lane forces the generic
/// cross-subgraph route to paint a horizontal hook before the title-safe
/// portal. That is technically connected but reads as a stray elbow. The move
/// is deliberately narrower than the general portal collector: it is only
/// eligible for the existing strict one-flat-subgraph, one-internal-node,
/// two-edge route transaction; the direct entry set must be complete and
/// one-to-one; the displacement must be at most six cells to cover the full
/// title-safe shift in this bounded route transaction; and every staged
/// rectangle must remain clear of the canvas, the subgraph, and unrelated
/// nodes. Ambiguous or crowded scenes retain the existing fail-closed route
/// policy.
fn align_td_external_entry_centers(
    graph: &Graph,
    placement: &mut Placement,
    envelopes: &HashMap<String, SubgraphEnvelope>,
) -> bool {
    if !matches!(graph.direction, Direction::TD | Direction::TB) {
        return false;
    }

    const MAX_TD_ENTRY_PORTAL_SHIFT: usize = 6;
    let mut proposals: Vec<(String, Rect)> = Vec::new();
    for subgraph in &graph.subgraphs {
        if graph.subgraphs.len() != 1
            || graph.edges.iter().filter(|edge| !edge.is_back_edge).count() != 2
            || subgraph.parent_id.is_some()
            || !subgraph.child_ids.is_empty()
            || !subgraph.has_title()
            || subgraph.node_ids.len() != 1
        {
            continue;
        }
        let Some(envelope) = envelopes.get(&subgraph.id) else {
            continue;
        };
        let Some(&portal_lane) = envelope.portals.top.iter().next() else {
            continue;
        };
        if envelope.portals.top.len() != 1 {
            continue;
        }

        let entries: Vec<_> = graph
            .edges
            .iter()
            .filter(|edge| {
                !edge.is_back_edge
                    && edge.kind == EdgeKind::Arrow
                    && edge.label.is_none()
                    && graph.get_node_subgraph(&edge.from).is_none()
                    && graph.get_node_subgraph(&edge.to) == Some(subgraph.id.as_str())
                    && graph
                        .edge_boundary_crossings(&edge.from, &edge.to)
                        .0
                        .is_empty()
                    && graph.edge_boundary_crossings(&edge.from, &edge.to).1
                        == vec![subgraph.id.as_str()]
            })
            .collect();
        // A single direct entry is safe only under the same strict guards
        // below: it must be the complete direct-entry set, use one distinct
        // source/target pair, stay within the bounded displacement, and clear
        // every unrelated rectangle. Keeping this path here lets the exact
        // single-subgraph TD scene remove a stray approach hook without
        // broadening ambiguous fan-in or nested portal placement.
        if entries.is_empty() {
            continue;
        }

        let all_direct_entries = graph
            .edges
            .iter()
            .filter(|edge| {
                !edge.is_back_edge
                    && graph.edge_boundary_crossings(&edge.from, &edge.to).1
                        == vec![subgraph.id.as_str()]
            })
            .count();
        let source_ids: HashSet<&str> = entries.iter().map(|edge| edge.from.as_str()).collect();
        let target_ids: HashSet<&str> = entries.iter().map(|edge| edge.to.as_str()).collect();
        if entries.len() != all_direct_entries
            || source_ids.len() != entries.len()
            || target_ids.len() != entries.len()
        {
            continue;
        }

        let mut local_proposals = Vec::new();
        for edge in entries {
            let Some(source_rect) = placement.node_rects.get(&edge.from).copied() else {
                local_proposals.clear();
                break;
            };
            let source_center = source_rect.x + source_rect.width / 2;
            if source_center.abs_diff(portal_lane) > MAX_TD_ENTRY_PORTAL_SHIFT {
                local_proposals.clear();
                break;
            }
            let Some(candidate) = centered_rect_at_x(source_rect, portal_lane) else {
                local_proposals.clear();
                break;
            };
            if candidate == source_rect {
                continue;
            }
            local_proposals.push((edge.from.clone(), candidate));
        }
        if local_proposals.is_empty() {
            continue;
        }

        let source_ids: HashSet<&str> = local_proposals
            .iter()
            .map(|(source_id, _)| source_id.as_str())
            .collect();
        if local_proposals.iter().any(|(_, candidate)| {
            !rect_is_inside(*candidate, placement.canvas)
                || rects_overlap(*candidate, envelope.outer)
                || graph
                    .nodes
                    .iter()
                    .filter(|node| !source_ids.contains(node.id.as_str()))
                    .filter_map(|node| placement.node_rects.get(&node.id))
                    .any(|rect| rects_overlap(*candidate, *rect))
        }) {
            continue;
        }
        if local_proposals
            .iter()
            .enumerate()
            .any(|(index, (_, candidate))| {
                local_proposals
                    .iter()
                    .skip(index + 1)
                    .any(|(_, other)| rects_overlap(*candidate, *other))
            })
        {
            continue;
        }

        proposals.extend(local_proposals);
    }

    if proposals.is_empty() {
        return false;
    }
    let mut changed = false;
    for (source_id, candidate) in proposals {
        changed |= set_node_x(placement, &source_id, candidate.x);
    }
    changed
}

/// Align a strict terminal external target with the exit portal of a flat,
/// titled vertical subgraph. A one-cell source/target parity mismatch makes
/// the generic route emit two adjacent boundary shafts; when the edge is
/// labeled, the label can hide that defect until spacing exposes it. Move
/// only the terminal target, and only when the target has one direct labeled
/// exit, the live portal lane is already source-aligned, and the candidate is
/// clear of every hard obstacle.
fn align_vertical_external_exit_centers(
    graph: &Graph,
    placement: &mut Placement,
    envelopes: &HashMap<String, SubgraphEnvelope>,
) -> bool {
    if !matches!(
        graph.direction,
        Direction::TD | Direction::TB | Direction::BT
    ) {
        return false;
    }

    let mut proposals: Vec<(String, usize)> = Vec::new();
    for subgraph in &graph.subgraphs {
        if subgraph.parent_id.is_some() || !subgraph.child_ids.is_empty() || !subgraph.has_title() {
            continue;
        }
        let Some(envelope) = envelopes.get(&subgraph.id) else {
            continue;
        };
        let exit_lane = match graph.direction {
            Direction::TD | Direction::TB => envelope.portals.bottom.iter().next().copied(),
            Direction::BT => envelope.portals.top.iter().next().copied(),
            Direction::LR | Direction::RL => None,
        };
        let Some(exit_lane) = exit_lane else {
            continue;
        };

        let exits: Vec<_> = graph
            .edges
            .iter()
            .filter(|edge| {
                !edge.is_back_edge
                    && edge.kind == EdgeKind::Arrow
                    && edge.label.is_some()
                    && graph.get_node_subgraph(&edge.from) == Some(subgraph.id.as_str())
                    && graph.get_node_subgraph(&edge.to).is_none()
                    && graph.edge_boundary_crossings(&edge.from, &edge.to).0
                        == vec![subgraph.id.as_str()]
                    && graph
                        .edge_boundary_crossings(&edge.from, &edge.to)
                        .1
                        .is_empty()
            })
            .collect();
        let all_direct_exits = graph
            .edges
            .iter()
            .filter(|edge| {
                !edge.is_back_edge
                    && graph.edge_boundary_crossings(&edge.from, &edge.to).0
                        == vec![subgraph.id.as_str()]
                    && graph
                        .edge_boundary_crossings(&edge.from, &edge.to)
                        .1
                        .is_empty()
            })
            .count();
        if exits.len() != 1 || all_direct_exits != exits.len() {
            continue;
        }

        let edge = exits[0];
        let Some(source_rect) = placement.node_rects.get(&edge.from).copied() else {
            continue;
        };
        let Some(target_rect) = placement.node_rects.get(&edge.to).copied() else {
            continue;
        };
        if graph
            .edges
            .iter()
            .any(|candidate| !candidate.is_back_edge && candidate.from == edge.to)
        {
            continue;
        }

        let source_center = rect_center_x(source_rect);
        if source_center != exit_lane {
            continue;
        }
        let Some(candidate) = centered_rect_at_x(target_rect, source_center) else {
            continue;
        };
        if candidate == target_rect
            || !rect_is_inside(candidate, placement.canvas)
            || rects_overlap(candidate, envelope.outer)
        {
            continue;
        }

        let target_id = edge.to.as_str();
        if graph
            .nodes
            .iter()
            .filter(|node| node.id.as_str() != target_id)
            .filter_map(|node| placement.node_rects.get(&node.id))
            .any(|rect| rects_overlap(*rect, candidate))
        {
            continue;
        }

        proposals.push((edge.to.clone(), candidate.x));
    }

    if proposals.is_empty() {
        return false;
    }

    let mut changed = false;
    for (target_id, candidate_x) in proposals {
        changed |= set_node_x(placement, &target_id, candidate_x);
    }
    changed
}

/// Align the strict three-rail BT sibling scene to the same title-safe lanes
/// used by the renderer's transactional parallel lowerer. The first pair can
/// otherwise remain centered under the title, forcing that rail to move only
/// at the boundary and leaving a one-cell hook. This policy is deliberately
/// limited to two flat titled sibling subgraphs, three unlabeled one-to-one
/// edges, and collision-free staged node moves.
fn align_bt_parallel_sibling_lanes(
    graph: &Graph,
    placement: &mut Placement,
    envelopes: &HashMap<String, SubgraphEnvelope>,
) -> bool {
    if graph.direction != Direction::BT || graph.subgraphs.len() != 2 {
        return false;
    }

    let mut subgraphs: Vec<_> = graph
        .subgraphs
        .iter()
        .filter(|subgraph| {
            subgraph.parent_id.is_none()
                && subgraph.child_ids.is_empty()
                && subgraph.title.is_some()
                && subgraph.node_ids.len() >= 3
        })
        .collect();
    if subgraphs.len() != 2 {
        return false;
    }
    subgraphs.sort_by_key(|subgraph| {
        envelopes
            .get(&subgraph.id)
            .map(|envelope| (envelope.outer.y, envelope.outer.x))
            .unwrap_or((usize::MAX, usize::MAX))
    });
    let target_subgraph = subgraphs[0];
    let source_subgraph = subgraphs[1];

    let source_ids: HashSet<&str> = source_subgraph
        .node_ids
        .iter()
        .map(String::as_str)
        .collect();
    let target_ids: HashSet<&str> = target_subgraph
        .node_ids
        .iter()
        .map(String::as_str)
        .collect();
    if source_ids.len() != 3
        || target_ids.len() != 3
        || source_ids.len() + target_ids.len() != graph.nodes.len()
    {
        return false;
    }
    if graph
        .nodes
        .iter()
        .any(|node| node.shape != NodeShape::Rectangle)
    {
        return false;
    }

    let mut edges: Vec<_> = graph
        .edges
        .iter()
        .filter(|edge| {
            !edge.is_back_edge
                && edge.kind == EdgeKind::Arrow
                && edge.label.is_none()
                && source_ids.contains(edge.from.as_str())
                && target_ids.contains(edge.to.as_str())
        })
        .collect();
    if edges.len() != 3
        || graph.edges.iter().filter(|edge| !edge.is_back_edge).count() != 3
        || edges
            .iter()
            .flat_map(|edge| [edge.from.as_str(), edge.to.as_str()])
            .collect::<HashSet<_>>()
            .len()
            != 6
    {
        return false;
    }
    edges.sort_by_key(|edge| {
        (
            placement
                .node_rects
                .get(&edge.from)
                .map(|rect| rect_center_x(*rect)),
            placement
                .node_rects
                .get(&edge.to)
                .map(|rect| rect_center_x(*rect)),
            edge.from.as_str(),
            edge.to.as_str(),
        )
    });

    let Some(source_bounds) = envelopes.get(&source_subgraph.id).map(|env| env.outer) else {
        return false;
    };
    let Some(target_bounds) = envelopes.get(&target_subgraph.id).map(|env| env.outer) else {
        return false;
    };
    if target_bounds.y >= source_bounds.y || target_bounds.bottom() > source_bounds.y {
        return false;
    }
    let min_x = source_bounds
        .x
        .saturating_add(2)
        .max(target_bounds.x.saturating_add(2));
    let max_x = source_bounds
        .x
        .saturating_add(source_bounds.width.saturating_sub(3))
        .min(
            target_bounds
                .x
                .saturating_add(target_bounds.width.saturating_sub(3)),
        );
    if min_x > max_x {
        return false;
    }
    let candidates: Vec<usize> = (min_x..=max_x)
        .filter(|lane| {
            [
                (source_bounds, source_subgraph.title.as_deref()),
                (target_bounds, target_subgraph.title.as_deref()),
            ]
            .into_iter()
            .all(|(bounds, title)| {
                nudge_portal_x_from_corners(
                    bounds.x,
                    bounds.width,
                    title,
                    Direction::BT,
                    title_safe_portal_x(
                        bounds.x,
                        bounds.width,
                        title,
                        *lane,
                        Direction::BT,
                        0,
                        PortalColumnPreference::Directional,
                    ),
                ) == *lane
            })
        })
        .collect();
    if candidates.len() < 3 {
        return false;
    }

    let Some(desired): Option<Vec<(usize, usize)>> = edges
        .iter()
        .map(|edge| {
            Some((
                rect_center_x(*placement.node_rects.get(&edge.from)?),
                rect_center_x(*placement.node_rects.get(&edge.to)?),
            ))
        })
        .collect()
    else {
        return false;
    };
    let mut best: Option<(usize, Vec<usize>)> = None;
    for first_index in 0..candidates.len() {
        for second_index in first_index + 1..candidates.len() {
            for third_index in second_index + 1..candidates.len() {
                let lanes = vec![
                    candidates[first_index],
                    candidates[second_index],
                    candidates[third_index],
                ];
                if lanes.windows(2).any(|pair| pair[1] - pair[0] < 4) {
                    continue;
                }
                let cost = lanes
                    .iter()
                    .zip(&desired)
                    .map(|(lane, (source, target))| lane.abs_diff(*source) + lane.abs_diff(*target))
                    .sum();
                if best.as_ref().is_none_or(|(best_cost, best_lanes)| {
                    cost < *best_cost || (cost == *best_cost && lanes < *best_lanes)
                }) {
                    best = Some((cost, lanes));
                }
            }
        }
    }
    let Some((_, lanes)) = best else {
        return false;
    };

    let mut proposals = Vec::with_capacity(edges.len() * 2);
    for (edge, lane) in edges.iter().zip(lanes) {
        let Some(source_rect) = placement.node_rects.get(&edge.from).copied() else {
            return false;
        };
        let Some(target_rect) = placement.node_rects.get(&edge.to).copied() else {
            return false;
        };
        let Some(source_candidate) = centered_rect_at_x(source_rect, lane) else {
            return false;
        };
        let Some(target_candidate) = centered_rect_at_x(target_rect, lane) else {
            return false;
        };
        proposals.push((edge.from.clone(), source_candidate));
        proposals.push((edge.to.clone(), target_candidate));
    }

    let moved_ids: HashSet<&str> = proposals.iter().map(|(id, _)| id.as_str()).collect();
    if proposals.iter().any(|(_, candidate)| {
        !rect_is_inside(*candidate, placement.canvas)
            || graph
                .nodes
                .iter()
                .filter(|node| !moved_ids.contains(node.id.as_str()))
                .filter_map(|node| placement.node_rects.get(&node.id))
                .any(|rect| rects_overlap(*rect, *candidate))
    }) {
        return false;
    }
    if proposals.iter().enumerate().any(|(index, (_, candidate))| {
        proposals
            .iter()
            .skip(index + 1)
            .any(|(_, other)| rects_overlap(*candidate, *other))
    }) {
        return false;
    }

    let mut changed = false;
    for (node_id, candidate) in proposals {
        changed |= set_node_x(placement, &node_id, candidate.x);
    }
    changed
}

/// Return the one-to-one external BT entries of a flat titled subgraph.
///
/// This is intentionally stricter than a fan-in count. A source must feed one
/// direct child, every target must have one distinct source, and there may not
/// be another direct entry into the same subgraph. That keeps the placement
/// translation local and prevents a shared source or ambiguous target from
/// being moved for only one of its routes.
fn bt_external_entry_center_pairs(
    graph: &Graph,
    subgraph_id: &str,
) -> Option<Vec<(String, String)>> {
    if graph.direction != Direction::BT {
        return None;
    }
    let subgraph = graph.get_subgraph(subgraph_id)?;
    if subgraph.parent_id.is_some()
        || !subgraph.child_ids.is_empty()
        || subgraph.title.is_none()
        || subgraph.node_ids.len() < 3
    {
        return None;
    }

    let entries: Vec<(String, String)> = graph
        .edges
        .iter()
        .filter(|edge| {
            !edge.is_back_edge
                && edge.kind == EdgeKind::Arrow
                && graph.get_node_subgraph(&edge.from).is_none()
                && graph.get_node_subgraph(&edge.to) == Some(subgraph_id)
                && graph
                    .edge_boundary_crossings(&edge.from, &edge.to)
                    .0
                    .is_empty()
                && graph.edge_boundary_crossings(&edge.from, &edge.to).1 == vec![subgraph_id]
        })
        .map(|edge| (edge.from.clone(), edge.to.clone()))
        .collect();
    if entries.len() < 3 {
        return None;
    }

    let all_direct_entries = graph
        .edges
        .iter()
        .filter(|edge| {
            !edge.is_back_edge
                && graph.edge_boundary_crossings(&edge.from, &edge.to).1 == vec![subgraph_id]
        })
        .count();
    if all_direct_entries != entries.len() {
        return None;
    }

    let source_ids: HashSet<&str> = entries.iter().map(|(source, _)| source.as_str()).collect();
    let target_ids: HashSet<&str> = entries.iter().map(|(_, target)| target.as_str()).collect();
    if source_ids.len() != entries.len() || target_ids.len() != entries.len() {
        return None;
    }

    let mut entries = entries;
    entries.sort_by(|left, right| {
        let left_source_x = placement_free_center_x(graph, &left.0);
        let right_source_x = placement_free_center_x(graph, &right.0);
        let left_target_x = placement_free_center_x(graph, &left.1);
        let right_target_x = placement_free_center_x(graph, &right.1);
        left_source_x
            .cmp(&right_source_x)
            .then_with(|| left_target_x.cmp(&right_target_x))
            .then_with(|| left.cmp(right))
    });

    Some(entries)
}

fn placement_free_center_x(graph: &Graph, node_id: &str) -> usize {
    graph
        .get_node(node_id)
        .map(|node| node.center_x())
        .unwrap_or(usize::MAX)
}

fn bt_multi_entry_lane_candidates(
    bounds: Rect,
    title: Option<&str>,
    entry_count: usize,
) -> Option<Vec<usize>> {
    let min_x = bounds.x.saturating_add(2);
    let max_x = bounds.x.saturating_add(bounds.width.saturating_sub(3));
    if min_x > max_x || entry_count > 8 {
        return None;
    }

    let title_floor = title
        .and_then(|title| {
            crate::graph::subgraph_title_span(bounds.x, bounds.width, title, Direction::BT)
        })
        .map(|(_, end)| end.saturating_add(1));
    let candidates: Vec<usize> = (min_x..=max_x)
        .filter(|candidate| {
            title_safe_portal_x(
                bounds.x,
                bounds.width,
                title,
                *candidate,
                Direction::BT,
                0,
                PortalColumnPreference::Nearest,
            ) == *candidate
                && title_floor.is_none_or(|floor| *candidate >= floor)
        })
        .collect();
    (candidates.len() >= entry_count).then_some(candidates)
}

// The recursive search keeps the immutable candidate/cost inputs and its
// mutable best-result accumulator explicit; bundling them would obscure the
// lexicographic optimization contract.
#[allow(clippy::too_many_arguments)]
fn search_bt_multi_entry_source_lanes(
    candidates: &[usize],
    desired: &[(usize, usize)],
    source_rects: &[Rect],
    index: usize,
    chosen: &mut Vec<usize>,
    source_cost: usize,
    target_cost: usize,
    best: &mut Option<(usize, usize, Vec<usize>)>,
) {
    if index == desired.len() {
        let replace =
            best.as_ref()
                .is_none_or(|(best_source_cost, best_target_cost, best_lanes)| {
                    source_cost < *best_source_cost
                        || (source_cost == *best_source_cost
                            && (target_cost < *best_target_cost
                                || (target_cost == *best_target_cost
                                    && chosen.as_slice() < best_lanes.as_slice())))
                });
        if replace {
            *best = Some((source_cost, target_cost, chosen.clone()));
        }
        return;
    }

    let (source_x, target_x) = desired[index];
    for &candidate in candidates {
        if chosen.last().is_some_and(|previous| candidate <= *previous)
            || chosen
                .iter()
                .any(|lane| lane.abs_diff(candidate) < BT_MULTI_ENTRY_MIN_LANE_GAP)
        {
            continue;
        }
        if (candidate != source_x && candidate.abs_diff(source_x) < 2)
            || (candidate != target_x && candidate.abs_diff(target_x) < 2)
        {
            continue;
        }
        let Some(candidate_rect) = centered_rect_at_x(source_rects[index], candidate) else {
            continue;
        };
        if chosen.iter().enumerate().any(|(other, previous_lane)| {
            centered_rect_at_x(source_rects[other], *previous_lane).is_some_and(|previous_rect| {
                rects_overlap(
                    candidate_rect.inflate(BT_EXTERNAL_NODE_KEEP_OUT),
                    previous_rect.inflate(BT_EXTERNAL_NODE_KEEP_OUT),
                )
            })
        }) {
            continue;
        }

        let target_interval = bt_ordered_interval(target_x, candidate);
        if chosen.iter().enumerate().any(|(other, previous_lane)| {
            let previous_target_interval = bt_ordered_interval(desired[other].1, *previous_lane);
            !bt_intervals_separated(target_interval, previous_target_interval)
        }) {
            continue;
        }

        let next_source_cost = source_cost.saturating_add(candidate.abs_diff(source_x));
        let next_target_cost = target_cost.saturating_add(candidate.abs_diff(target_x));
        if best
            .as_ref()
            .is_some_and(|(best_source_cost, best_target_cost, _)| {
                next_source_cost > *best_source_cost
                    || (next_source_cost == *best_source_cost
                        && next_target_cost >= *best_target_cost)
            })
        {
            continue;
        }
        chosen.push(candidate);
        search_bt_multi_entry_source_lanes(
            candidates,
            desired,
            source_rects,
            index + 1,
            chosen,
            next_source_cost,
            next_target_cost,
            best,
        );
        chosen.pop();
    }
}

fn bt_ordered_interval(left: usize, right: usize) -> (usize, usize) {
    (left.min(right), left.max(right))
}

fn bt_intervals_separated(left: (usize, usize), right: (usize, usize)) -> bool {
    left.1.saturating_add(1) < right.0 || right.1.saturating_add(1) < left.0
}

/// Align a strict flat BT multi-entry scene to the renderer's deterministic
/// title-safe lanes. Matching only target centers is insufficient: a target
/// center can be inside the title span, forcing the route planner to select a
/// different lane and paint a border-adjacent turn. This policy stages source
/// rectangles on the same lane candidates used by that scene and fails closed
/// when source boxes cannot be separated without touching another node.
///
/// All candidate rectangles are staged and checked before any placement is
/// mutated. The policy fails closed on collisions, boundary contact, or a
/// non-BT/non-one-to-one topology; the generic router remains authoritative in
/// those cases.
fn align_bt_external_entry_centers(
    graph: &Graph,
    placement: &mut Placement,
    envelopes: &HashMap<String, SubgraphEnvelope>,
) -> bool {
    if graph.direction != Direction::BT {
        return false;
    }

    let Some((subgraph_id, entries)) = graph.subgraphs.iter().find_map(|subgraph| {
        bt_external_entry_center_pairs(graph, &subgraph.id)
            .map(|entries| (subgraph.id.as_str(), entries))
    }) else {
        return false;
    };
    let Some(envelope) = envelopes.get(subgraph_id) else {
        return false;
    };

    let source_ids: HashSet<&str> = entries.iter().map(|(source, _)| source.as_str()).collect();
    let Some(lane_candidates) = bt_multi_entry_lane_candidates(
        envelope.outer,
        graph
            .get_subgraph(subgraph_id)
            .and_then(|subgraph| subgraph.title.as_deref()),
        entries.len(),
    ) else {
        return false;
    };
    let mut source_rects = Vec::with_capacity(entries.len());
    let mut desired = Vec::with_capacity(entries.len());
    for (source_id, target_id) in &entries {
        let Some(source_rect) = placement.node_rects.get(source_id).copied() else {
            return false;
        };
        let Some(target_rect) = placement.node_rects.get(target_id).copied() else {
            return false;
        };
        if source_rect.y <= target_rect.y {
            return false;
        }

        source_rects.push(source_rect);
        desired.push((rect_center_x(source_rect), rect_center_x(target_rect)));
    }

    let mut lane_assignment: Option<(usize, usize, Vec<usize>)> = None;
    search_bt_multi_entry_source_lanes(
        &lane_candidates,
        &desired,
        &source_rects,
        0,
        &mut Vec::new(),
        0,
        0,
        &mut lane_assignment,
    );
    let Some((_, _, lanes)) = lane_assignment else {
        return false;
    };

    let candidates: Vec<(String, Rect)> = entries
        .iter()
        .zip(lanes)
        .filter_map(|((source_id, _), lane)| {
            centered_rect_at_x(*placement.node_rects.get(source_id)?, lane)
                .map(|candidate| (source_id.clone(), candidate))
        })
        .collect();
    if candidates.len() != entries.len() {
        return false;
    }

    for (index, (_, candidate)) in candidates.iter().enumerate() {
        if candidate.x < placement.canvas.x || candidate.bottom() > placement.canvas.bottom() {
            return false;
        }
        for (other_index, (_, other_candidate)) in candidates.iter().enumerate() {
            if index != other_index
                && rects_overlap(
                    candidate.inflate(BT_EXTERNAL_NODE_KEEP_OUT),
                    *other_candidate,
                )
            {
                return false;
            }
        }
        for (node_id, node_rect) in &placement.node_rects {
            if !source_ids.contains(node_id.as_str())
                && rects_overlap(
                    candidate.inflate(BT_EXTERNAL_NODE_KEEP_OUT),
                    node_rect.inflate(BT_EXTERNAL_NODE_KEEP_OUT),
                )
            {
                return false;
            }
        }
    }

    let changed = candidates.iter().any(|(source_id, candidate)| {
        placement
            .node_rects
            .get(source_id)
            .is_some_and(|current| current.x != candidate.x)
    });
    if !changed {
        return false;
    }

    for (source_id, candidate) in candidates {
        if !set_node_x(placement, &source_id, candidate.x) {
            return false;
        }
    }
    placement.canvas.width = placement.canvas.width.max(
        placement
            .node_rects
            .values()
            .map(Rect::right)
            .max()
            .unwrap_or(0),
    );
    true
}

/// Move topology-connected external blockers out of exact sibling BT
/// corridors. The loop is bounded and recomputes envelopes after each move so
/// the route owner always consumes current geometry rather than stale bounds.
fn reserve_bt_sibling_corridors(
    graph: &Graph,
    config: &CoarseLayoutConfig,
    placement: &mut Placement,
    envelopes: &mut HashMap<String, SubgraphEnvelope>,
) {
    for _ in 0..8 {
        let Some((node_id, corridor)) = bt_sibling_corridors(graph, envelopes)
            .into_iter()
            .flat_map(|(source_id, target_id, corridor)| {
                placement
                    .node_rects
                    .iter()
                    .filter(move |(node_id, rect)| {
                        graph.get_node_subgraph(node_id).is_none()
                            && bt_external_node_touches_sibling_tree(
                                graph, node_id, &source_id, &target_id,
                            )
                            && rects_overlap(rect.inflate(BT_EXTERNAL_NODE_KEEP_OUT), corridor)
                    })
                    .map(move |(node_id, _)| (node_id.clone(), corridor))
            })
            .min_by(|left, right| {
                left.0
                    .cmp(&right.0)
                    .then_with(|| left.1.y.cmp(&right.1.y))
                    .then_with(|| left.1.x.cmp(&right.1.x))
            })
        else {
            break;
        };

        let Some(current) = placement.node_rects.get(&node_id).copied() else {
            break;
        };
        let obstacles =
            bt_external_relocation_obstacles(&placement.node_rects, envelopes, &node_id);

        let left_candidate = corridor
            .x
            .checked_sub(current.width + BT_SIBLING_CORRIDOR_SIDE_GAP)
            .and_then(|x| first_clear_left_x(current, x, &obstacles));
        let right_candidate = first_clear_right_x(
            current,
            corridor
                .right()
                .saturating_add(BT_SIBLING_CORRIDOR_SIDE_GAP),
            &obstacles,
        );

        let mut candidates = Vec::new();
        if let Some(x) = left_candidate {
            candidates.push((x.abs_diff(current.x), 0usize, x));
        }
        if let Some(x) = right_candidate {
            candidates.push((x.abs_diff(current.x), 1usize, x));
        }
        candidates.sort_unstable();

        let Some((_, _, next_x)) = candidates.into_iter().next() else {
            break;
        };
        if next_x == current.x || !set_node_x(placement, &node_id, next_x) {
            break;
        }

        let max_right = placement
            .node_rects
            .values()
            .map(|rect| rect.right())
            .max()
            .unwrap_or(placement.canvas.right());
        placement.canvas.width = placement.canvas.width.max(max_right);
        *envelopes = compute_envelopes(graph, &placement.node_rects, config.subgraph_gutter);
        adjust_portal_slots_for_title(envelopes, graph);
    }
}

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
                    let multiple_external_sources = incoming_into_subgraph_from
                        .keys()
                        .filter(|(_, target_sg)| target_sg == sg_id)
                        .map(|(source_id, _)| source_id)
                        .collect::<HashSet<_>>()
                        .len()
                        > 1;
                    let clearance = if incoming_count > 1 || multiple_external_sources {
                        // Independent entries need a distinct exterior
                        // approach row; otherwise the first route is forced
                        // to share the subgraph's top-border/title band.
                        2
                    } else {
                        1
                    };
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
                let required_to_top = from_env
                    .outer
                    .bottom()
                    .saturating_add(TD_SIBLING_CONNECTOR_ROWS);
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
                    let (_, enter_subgraphs) =
                        input.graph.edge_boundary_crossings(&edge.from, &edge.to);
                    let (exit_subgraphs, _) =
                        input.graph.edge_boundary_crossings(&edge.from, &edge.to);
                    let extra_nested_exit_row = enter_subgraphs.is_empty()
                        && exit_subgraphs.len() > 1
                        && input.graph.get_node_subgraph(&edge.to).is_none();
                    let required_target_y = env
                        .outer
                        .bottom()
                        .saturating_add(if extra_nested_exit_row { 2 } else { 1 });
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

    // Reserve the inter-subgraph corridor before BT route ownership is handed
    // to the renderer. An external node connected to either sibling can consume
    // every safe target lane even when the envelopes themselves are disjoint.
    // Relocate only that topology-connected blocker, then recompute the live
    // envelopes/portal slots used by the downstream route stage.
    if input.graph.direction == Direction::BT && !subgraph_envelopes.is_empty() {
        reserve_bt_sibling_corridors(input.graph, config, placement, &mut subgraph_envelopes);
    }

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
                let source_clearance = bt_multi_entry_source_clearance(input.graph, sg_id);
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
                    let required_source_y = env.outer.bottom().saturating_add(source_clearance);
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
            let source_clearance = bt_multi_entry_source_clearance(input.graph, subgraph_id);
            let required_source_y = env.outer.bottom().saturating_add(source_clearance);
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
                    let (_, enter_subgraphs) =
                        input.graph.edge_boundary_crossings(&edge.from, &edge.to);
                    let (exit_subgraphs, _) =
                        input.graph.edge_boundary_crossings(&edge.from, &edge.to);
                    let extra_nested_exit_row = enter_subgraphs.is_empty()
                        && exit_subgraphs.len() > 1
                        && input.graph.get_node_subgraph(&edge.to).is_none();
                    let required_target_y = env
                        .outer
                        .bottom()
                        .saturating_add(if extra_nested_exit_row { 2 } else { 1 });
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

    // Apply the BT sibling-chain alignment after every bounded horizontal
    // rebalance has settled. The renderer chooses title-safe portals from
    // these final envelope bounds; moving the content earlier would be
    // discarded by a later envelope recomputation and leave a false lane
    // agreement between layout and route ownership.
    if input.graph.direction == Direction::BT {
        align_bt_sibling_chain_content(input.graph, config, placement, &mut subgraph_envelopes);
    }

    if input.graph.direction == Direction::BT
        && align_bt_parallel_sibling_lanes(input.graph, placement, &subgraph_envelopes)
    {
        subgraph_envelopes =
            compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
        adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
    }

    // Keep direct external BT entries on the same columns as their target
    // centers. This is intentionally after envelope/sibling rebalance so the
    // translation is the final placement owner consumed by route staging.
    if input.graph.direction == Direction::BT
        && align_bt_external_entry_centers(input.graph, placement, &subgraph_envelopes)
    {
        subgraph_envelopes =
            compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
        adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
    }

    // Keep the ordinary external attachments on the same secondary-axis lanes
    // selected for the exact single-subgraph parallel TD scene. This runs after
    // all other envelope constraints so a successful move is the final layout
    // owner and the portal collector below sees live geometry.
    if matches!(input.graph.direction, Direction::TD | Direction::TB)
        && align_td_external_portal_centers(input.graph, placement, &subgraph_envelopes)
    {
        subgraph_envelopes =
            compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
        adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
    }

    // Keep a single external TD/TB fan-out source on its shared top portal
    // lane. This is the final placement owner for the one-cell parity case
    // that would otherwise force adjacent approach corners in the renderer.
    if matches!(input.graph.direction, Direction::TD | Direction::TB)
        && align_td_external_fanout_centers(input.graph, placement, &subgraph_envelopes)
    {
        subgraph_envelopes =
            compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
        adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
    }

    // Keep a narrow one-to-one set of external TD/TB entries on their direct
    // target centers. This removes adjacent approach corners caused by a
    // one-cell source/target parity mismatch without taking ownership of
    // ambiguous multi-entry collector scenes.
    if matches!(input.graph.direction, Direction::TD | Direction::TB)
        && align_td_external_entry_centers(input.graph, placement, &subgraph_envelopes)
    {
        subgraph_envelopes =
            compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
        adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
    }

    // Keep a strict labeled external exit on the selected portal lane. This
    // is the counterpart to the entry alignment above and is intentionally
    // terminal-only so ordinary multi-branch exits remain fail-closed.
    if align_vertical_external_exit_centers(input.graph, placement, &subgraph_envelopes) {
        subgraph_envelopes =
            compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
        adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
    }

    // A disconnected top-level node may be laid out in the gap between two
    // horizontal subgraphs. If its box is wider than that gap, it can overlap
    // a foreign subgraph even though no edge crosses that boundary. Move the
    // affected envelope or the foreign node until their rectangles have a
    // visible gutter before portal ownership is projected.
    if matches!(input.graph.direction, Direction::LR | Direction::RL) {
        reserve_horizontal_external_target_corridors(input.graph, placement, &subgraph_envelopes);
        enforce_horizontal_foreign_node_clearance(input.graph, placement, config);
        subgraph_envelopes =
            compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
        adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
    }

    subgraph_envelopes
}

/// Keep a terminal external target out of the horizontal bands occupied by
/// edges crossing between distinct top-level subgraphs. Without this reserve,
/// a target can be placed in the inter-subgraph gap and the target-entry scene
/// then competes with a sibling-to-sibling route for the same rows.
fn reserve_horizontal_external_target_corridors(
    graph: &Graph,
    placement: &mut Placement,
    envelopes: &HashMap<String, SubgraphEnvelope>,
) {
    if !matches!(graph.direction, Direction::LR | Direction::RL) {
        return;
    }

    let crossing_ranges = graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge)
        .filter_map(|edge| {
            let from_subgraph = graph.get_node_subgraph(&edge.from)?;
            let to_subgraph = graph.get_node_subgraph(&edge.to)?;
            if top_level_subgraph_id(graph, from_subgraph)
                == top_level_subgraph_id(graph, to_subgraph)
            {
                return None;
            }
            let from = placement.node_rects.get(&edge.from).copied()?;
            let to = placement.node_rects.get(&edge.to).copied()?;
            Some((from.y.min(to.y), from.bottom().max(to.bottom())))
        })
        .collect::<Vec<_>>();
    if crossing_ranges.is_empty() {
        return;
    }

    let terminal_targets = graph
        .nodes
        .iter()
        .filter(|node| {
            graph.get_node_subgraph(&node.id).is_none()
                && graph
                    .edges
                    .iter()
                    .filter(|edge| !edge.is_back_edge && edge.to == node.id)
                    .any(|edge| graph.get_node_subgraph(&edge.from).is_some())
                && !graph
                    .edges
                    .iter()
                    .any(|edge| !edge.is_back_edge && edge.from == node.id)
        })
        .map(|node| node.id.clone())
        .collect::<Vec<_>>();

    for node_id in terminal_targets {
        let Some(rect) = placement.node_rects.get(&node_id).copied() else {
            continue;
        };
        if graph.direction == Direction::LR
            && sibling_subgraph_fan_in_identity::scenes(graph)
                .into_iter()
                .any(|scene| {
                    scene.target_id == node_id
                        && align_lr_sibling_terminal_target(graph, placement, envelopes, &scene)
                })
        {
            continue;
        }
        let Some(max_crossing_bottom) = crossing_ranges
            .iter()
            .filter(|(top, bottom)| rect.y < *bottom && *top < rect.bottom())
            .map(|(_, bottom)| *bottom)
            .max()
        else {
            continue;
        };
        // RL's physical fallback can use a bottom-portal identity scene when
        // a target is below/right. LR retains the original compact gutter;
        // its horizontal sibling seam contract is already covered by the
        // existing parity regression.
        let extra_clearance = if graph.direction == Direction::RL {
            8
        } else {
            2
        };
        // Leave enough visual gutter below the last sibling-to-sibling
        // crossing.  One row is enough to avoid rectangle overlap, but it
        // leaves a terminal target touching the source subgraph's lower
        // envelope and forces a horizontal fan-in to reuse the same side as
        // an incoming database edge.  The extra clearance lets the renderer
        // prove a clean bottom-portal route when the target is below/right.
        let required_y = max_crossing_bottom.saturating_add(extra_clearance);
        if rect.y >= required_y {
            continue;
        }
        let delta = required_y - rect.y;
        if let Some(updated) = placement.node_rects.get_mut(&node_id) {
            updated.y = updated.y.saturating_add(delta);
        }
        if let Some(position) = placement.positions.get_mut(&node_id) {
            position.y = position.y.saturating_add(delta);
        }
        placement.canvas.height = placement
            .canvas
            .height
            .max(rect.bottom().saturating_add(delta));
    }
}

/// Align the exact LR sibling-subgraph terminal scene after every other
/// envelope mutation has run.  This is the final owner of terminal y-position
/// for horizontal flow; placing the move earlier is overwritten by the
/// crossing-band reserve above the route stage.
fn align_lr_sibling_terminal_target(
    graph: &Graph,
    placement: &mut Placement,
    envelopes: &HashMap<String, SubgraphEnvelope>,
    scene: &sibling_subgraph_fan_in_identity::Scene,
) -> bool {
    let Some(source) = envelopes
        .get(&scene.source_subgraph_id)
        .map(|env| env.outer)
    else {
        return false;
    };
    let Some(target) = placement.node_rects.get(&scene.target_id).copied() else {
        return false;
    };
    let right = target.x >= source.right();
    let left = target.right() <= source.x;
    if !right && !left {
        return false;
    }

    let aligned_y = source
        .y
        .saturating_add(source.height / 2)
        .saturating_sub(target.height / 2);
    let candidate = Rect::new(target.x, aligned_y, target.width, target.height);
    if !rect_is_inside(candidate, placement.canvas) {
        return false;
    }

    let keepout = candidate.inflate(1);
    if placement
        .node_rects
        .iter()
        .any(|(node_id, other)| node_id != &scene.target_id && rects_overlap(keepout, *other))
    {
        return false;
    }
    if envelopes.iter().any(|(subgraph_id, envelope)| {
        graph.get_node_subgraph(&scene.target_id) != Some(subgraph_id.as_str())
            && rects_overlap(keepout, envelope.outer)
    }) {
        return false;
    }

    if target.y == aligned_y {
        return true;
    }
    let Some(updated) = placement.node_rects.get_mut(&scene.target_id) else {
        return false;
    };
    updated.y = aligned_y;
    if let Some(position) = placement.positions.get_mut(&scene.target_id) {
        position.y = aligned_y;
    }
    placement.canvas.height = placement.canvas.height.max(updated.bottom());
    true
}

fn enforce_horizontal_foreign_node_clearance(
    graph: &Graph,
    placement: &mut Placement,
    config: &CoarseLayoutConfig,
) {
    for _ in 0..8 {
        let envelopes = compute_envelopes(graph, &placement.node_rects, config.subgraph_gutter);
        let mut subgraph_shifts: HashMap<String, usize> = HashMap::new();
        let mut external_shifts: HashMap<String, usize> = HashMap::new();

        for (subgraph_id, envelope) in &envelopes {
            let Some(subgraph) = graph.get_subgraph(subgraph_id) else {
                continue;
            };
            if subgraph.parent_id.is_some() {
                continue;
            }

            for (node_id, node_rect) in &placement.node_rects {
                if graph.get_node_subgraph(node_id).is_some()
                    || graph.is_node_in_subgraph_tree(node_id, subgraph_id)
                    || !rects_overlap_horizontally(*node_rect, envelope.outer)
                    || !rects_overlap_vertically(*node_rect, envelope.outer)
                {
                    continue;
                }

                if node_rect.x < envelope.outer.x {
                    let required_left = node_rect.right().saturating_add(2);
                    let delta = required_left.saturating_sub(envelope.outer.x);
                    if delta > 0 {
                        subgraph_shifts
                            .entry(subgraph_id.clone())
                            .and_modify(|existing| *existing = (*existing).max(delta))
                            .or_insert(delta);
                    }
                } else {
                    let required_left = envelope.outer.right().saturating_add(2);
                    let delta = required_left.saturating_sub(node_rect.x);
                    if delta > 0 {
                        external_shifts
                            .entry(node_id.clone())
                            .and_modify(|existing| *existing = (*existing).max(delta))
                            .or_insert(delta);
                    }
                }
            }
        }

        if subgraph_shifts.is_empty() && external_shifts.is_empty() {
            break;
        }

        for (subgraph_id, delta) in subgraph_shifts {
            shift_nodes_in_subgraph_tree_x_signed(
                graph,
                &mut placement.positions,
                &mut placement.node_rects,
                &subgraph_id,
                delta as isize,
            );
        }

        for (node_id, delta) in external_shifts {
            if let Some(rect) = placement.node_rects.get_mut(&node_id) {
                rect.x = rect.x.saturating_add(delta);
            }
            if let Some(position) = placement.positions.get_mut(&node_id) {
                position.x = position.x.saturating_add(delta);
            }
        }

        placement.canvas.width = placement.canvas.width.max(
            placement
                .node_rects
                .values()
                .map(Rect::right)
                .max()
                .unwrap_or(0),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, Node, Subgraph};

    fn parallel_td_graph() -> Graph {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;
        for node_id in ["In", "A", "B", "C", "D", "Out"] {
            graph.nodes.push(Node::new(node_id, node_id));
        }
        for (from, to) in [
            ("A", "B"),
            ("A", "C"),
            ("B", "D"),
            ("C", "D"),
            ("In", "A"),
            ("D", "Out"),
        ] {
            graph.edges.push(Edge::new(from, to));
        }

        let mut subgraph = Subgraph::new("Process", Some("Process".to_string()));
        for node_id in ["A", "B", "C", "D"] {
            subgraph.add_node(node_id);
        }
        graph.add_subgraph(subgraph);
        for node_id in ["A", "B", "C", "D"] {
            graph.associate_node_with_subgraph(node_id, "Process");
        }
        graph
    }

    #[test]
    fn td_parallel_selector_is_topology_scoped() {
        let graph = parallel_td_graph();
        let ids = graph.td_parallel_external_attachment_ids();
        assert_eq!(
            ids,
            Some((
                "Process".to_string(),
                "In".to_string(),
                "A".to_string(),
                "D".to_string(),
                "Out".to_string(),
            ))
        );

        let mut labeled = graph.clone();
        labeled.edges[0].label = Some("branch".to_string());
        assert!(labeled.td_parallel_external_attachment_ids().is_none());

        let mut extra_boundary = graph.clone();
        extra_boundary.edges.push(Edge::new("In", "B"));
        assert!(extra_boundary
            .td_parallel_external_attachment_ids()
            .is_none());

        let mut horizontal = graph;
        horizontal.direction = Direction::LR;
        assert!(horizontal.td_parallel_external_attachment_ids().is_none());
    }

    #[test]
    fn bt_multi_entry_layout_prefers_source_aligned_title_safe_lanes() {
        let candidates = (16..=36).collect::<Vec<_>>();
        let desired = vec![(16, 9), (26, 21), (36, 33)];
        let source_rects = vec![
            Rect::new(15, 12, 2, 3),
            Rect::new(25, 12, 2, 3),
            Rect::new(35, 12, 2, 3),
        ];
        let mut best = None;
        search_bt_multi_entry_source_lanes(
            &candidates,
            &desired,
            &source_rects,
            0,
            &mut Vec::new(),
            0,
            0,
            &mut best,
        );

        let (_, _, lanes) = best.expect("source lane assignment");
        assert_eq!(lanes, vec![16, 26, 36]);
    }
}
