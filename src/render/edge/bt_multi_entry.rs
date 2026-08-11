//! Topology-derived reservations for BT subgraphs with multiple external entries.
//!
//! Several independent external edges entering one titled BT subgraph are a
//! single visual boundary scene.  Routing them one at a time makes each edge
//! choose a different local portal and leaves the title row and the only
//! exterior corridor to resolve their turns independently.  This module
//! assigns lanes together, validates the complete scene, and lowers it as one
//! typed fallback reservation.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::graph::{Direction, EdgeKind, Graph, Node, Subgraph};
use crate::portals::{title_safe_portal_x, PortalColumnPreference, PortalSlots};
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::fallback_route::FallbackRoutePlan;
use super::super::semantic::CellOwnerKind;
use super::edge_primitives::{adjusted_edge_entry_point, edge_exit_point};
use super::subgraph::lower_bt_fallback_plan;
use super::RouteOwner;

const STRATEGY: &str = "bt-multi-entry-scene-reservation";
const MIN_LANE_GAP: usize = 3;
const TITLE_CLEARANCE: usize = 0;

#[derive(Debug, Clone)]
struct EntryEdge {
    index: usize,
    source_id: String,
    target_id: String,
}

#[derive(Debug)]
struct EntryScene {
    subgraph_id: String,
    edges: Vec<EntryEdge>,
}

/// Reserve a complete BT multi-entry scene when graph topology proves that all
/// direct external entries belong to one titled target boundary.
pub(crate) fn plan_bt_multi_entry_scene(
    graph: &Graph,
    canvas: &mut Canvas,
    style: &StyleChars,
    portal_slots: &mut HashMap<String, PortalSlots>,
) -> HashSet<usize> {
    if graph.direction != Direction::BT {
        return HashSet::new();
    }

    let Some(scene) = detect_scene(graph) else {
        return HashSet::new();
    };
    let Some(subgraph) = graph.get_subgraph(&scene.subgraph_id) else {
        return HashSet::new();
    };
    let Some(inside_y) = bt_title_safe_row(subgraph) else {
        return reject_scene(canvas, &scene, "target has no BT title-safe attachment row");
    };

    let bottom_y = subgraph
        .bounds
        .y
        .saturating_add(subgraph.bounds.height.saturating_sub(1));
    let outside_y = bottom_y.saturating_add(1);
    if outside_y >= canvas.height {
        return reject_scene(
            canvas,
            &scene,
            "target boundary has no exterior corridor row",
        );
    }

    let Some(lanes) = assign_lanes(&scene, subgraph, graph, canvas) else {
        return reject_scene(
            canvas,
            &scene,
            "no deterministic title-safe non-overlapping lane assignment",
        );
    };
    if crate::runtime::current().diagnostics.routes {
        eprintln!("bt multi-entry scene candidate lanes={lanes:?}");
    }

    let owner_id = format!("scene:{STRATEGY}:{}", scene.subgraph_id);
    let owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: owner_id.as_str(),
    };
    let mut plan = FallbackRoutePlan::new(owner_id.clone(), STRATEGY);
    let covered_edge_ids: Vec<String> = scene
        .edges
        .iter()
        .filter_map(|entry| {
            graph
                .edges
                .get(entry.index)
                .map(|edge| crate::render::provenance::edge_owner_id(entry.index, edge))
        })
        .collect();
    plan.set_scene_coverage(covered_edge_ids);

    let mut occupied = BTreeSet::new();
    for (entry, lane) in scene.edges.iter().zip(&lanes) {
        let Some(source) = graph.get_node(&entry.source_id) else {
            return reject_scene(canvas, &scene, "scene source node disappeared");
        };
        let Some(target) = graph.get_node(&entry.target_id) else {
            return reject_scene(canvas, &scene, "scene target node disappeared");
        };
        let Some(edge) = graph.edges.get(entry.index) else {
            return reject_scene(canvas, &scene, "scene edge disappeared");
        };
        let (arrow_x, arrow_y) = adjusted_edge_entry_point(target, Direction::BT, graph);
        let (source_x, source_y) = edge_exit_point(source, Direction::BT);
        let mut edge_plan = FallbackRoutePlan::new(owner_id.clone(), STRATEGY);
        append_edge_path(
            &mut edge_plan,
            source_x,
            source_y,
            *lane,
            arrow_x,
            arrow_y,
            outside_y,
            bottom_y,
            inside_y,
            style,
        );
        edge_plan.claim_boundary(subgraph.id.clone(), "bottom", *lane, bottom_y, style.edge_v);

        let edge_cells = edge_plan.planned_cells();
        if edge_cells.iter().any(|cell| occupied.contains(cell)) {
            return reject_scene(canvas, &scene, "scene lanes share a route cell");
        }
        if let Some(reason) = edge_plan_blocker(
            &edge_plan,
            (source_x, source_y),
            (arrow_x, arrow_y),
            canvas,
            graph,
        ) {
            return reject_scene(canvas, &scene, reason.as_str());
        }
        occupied.extend(edge_cells);
        plan.segments.extend(edge_plan.segments);
        plan.corners.extend(edge_plan.corners);
        plan.paints.extend(edge_plan.paints);
        plan.boundary_claims.extend(edge_plan.boundary_claims);

        if edge.kind != EdgeKind::Arrow {
            return reject_scene(
                canvas,
                &scene,
                "multi-entry scene requires uniform arrow edge style",
            );
        }
    }

    if let Some(reason) = plan.validation_error(canvas.width, canvas.height) {
        return reject_scene(canvas, &scene, reason.as_str());
    }

    if let Some(slots) = portal_slots.get_mut(&scene.subgraph_id) {
        slots.bottom.clear();
        slots.bottom.extend(lanes.iter().copied());
    }

    canvas.set_write_stage("edge-route-plan");
    if !lower_bt_fallback_plan(plan, canvas, style, graph, Some(owner)) {
        return HashSet::new();
    }
    if crate::runtime::current().diagnostics.routes {
        eprintln!(
            "bt multi-entry scene accepted subgraph={} edges={} lanes={lanes:?}",
            scene.subgraph_id,
            scene.edges.len()
        );
    }
    scene.edges.into_iter().map(|entry| entry.index).collect()
}

fn detect_scene(graph: &Graph) -> Option<EntryScene> {
    for subgraph in &graph.subgraphs {
        if subgraph.title.is_none() || !subgraph.bounds.is_valid() {
            continue;
        }

        let mut candidates = Vec::new();
        for (index, edge) in graph.edges.iter().enumerate() {
            if edge.is_back_edge || edge.kind != EdgeKind::Arrow {
                continue;
            }
            if graph.get_node_subgraph(&edge.from).is_some()
                || graph.get_node_subgraph(&edge.to) != Some(subgraph.id.as_str())
                || !subgraph.node_ids.contains(&edge.to)
            {
                continue;
            }
            let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
            if exits.is_empty() && enters == vec![subgraph.id.as_str()] {
                candidates.push(EntryEdge {
                    index,
                    source_id: edge.from.clone(),
                    target_id: edge.to.clone(),
                });
            }
        }

        let distinct_sources = candidates
            .iter()
            .map(|entry| entry.source_id.as_str())
            .collect::<HashSet<_>>()
            .len();
        if candidates.len() < 3 || distinct_sources < 3 {
            continue;
        }
        candidates.sort_by_key(|entry| {
            let source_x = graph
                .get_node(&entry.source_id)
                .map(Node::center_x)
                .unwrap_or(usize::MAX);
            let target_x = graph
                .get_node(&entry.target_id)
                .map(Node::center_x)
                .unwrap_or(usize::MAX);
            (source_x, target_x, entry.index)
        });

        let all_entries_to_scene = graph
            .edges
            .iter()
            .enumerate()
            .filter(|(_, edge)| {
                !edge.is_back_edge
                    && graph.edge_boundary_crossings(&edge.from, &edge.to).1
                        == vec![subgraph.id.as_str()]
            })
            .count();
        if all_entries_to_scene != candidates.len() {
            continue;
        }

        return Some(EntryScene {
            subgraph_id: subgraph.id.clone(),
            edges: candidates,
        });
    }
    None
}

fn bt_title_safe_row(subgraph: &Subgraph) -> Option<usize> {
    let title_row =
        crate::graph::subgraph_title_row(subgraph.bounds.y, subgraph.bounds.height, Direction::BT);
    // Keep the branch row one clear row above the title row.  The immediately
    // preceding row is technically title-safe, but makes the horizontal fan-in
    // read as a title underline/hook when the target is compact.
    title_row.checked_sub(2).filter(|row| {
        *row > subgraph.bounds.y
            && *row
                < subgraph
                    .bounds
                    .y
                    .saturating_add(subgraph.bounds.height.saturating_sub(1))
    })
}

fn assign_lanes(
    scene: &EntryScene,
    subgraph: &Subgraph,
    graph: &Graph,
    canvas: &Canvas,
) -> Option<Vec<usize>> {
    let min_x = subgraph.bounds.x.saturating_add(2);
    let max_x = subgraph
        .bounds
        .x
        .saturating_add(subgraph.bounds.width.saturating_sub(3));
    if min_x > max_x || scene.edges.len() > 8 {
        return None;
    }

    let title_floor = subgraph
        .title
        .as_deref()
        .and_then(|title| {
            crate::graph::subgraph_title_span(
                subgraph.bounds.x,
                subgraph.bounds.width,
                title,
                Direction::BT,
            )
        })
        .map(|(_, end)| end.saturating_add(TITLE_CLEARANCE + 1));
    let candidates: Vec<usize> = (min_x..=max_x)
        .filter(|candidate| {
            title_safe_portal_x(
                subgraph.bounds.x,
                subgraph.bounds.width,
                subgraph.title.as_deref(),
                *candidate,
                Direction::BT,
                0,
                PortalColumnPreference::Nearest,
            ) == *candidate
                && title_floor.is_none_or(|floor| *candidate >= floor)
                && canvas.get(*candidate, subgraph.bounds.y) != ' '
        })
        .collect();
    if candidates.len() < scene.edges.len() {
        return None;
    }

    let desired: Vec<(usize, usize)> = scene
        .edges
        .iter()
        .map(|entry| {
            let source_x = graph
                .get_node(&entry.source_id)
                .map(Node::center_x)
                .unwrap_or(usize::MAX);
            let target_x = graph
                .get_node(&entry.target_id)
                .map(Node::center_x)
                .unwrap_or(usize::MAX);
            (source_x, target_x)
        })
        .collect();
    let mut best: Option<(usize, usize, Vec<usize>)> = None;
    search_lane_assignments(&candidates, &desired, 0, &mut Vec::new(), 0, 0, &mut best);
    best.map(|(_, _, lanes)| lanes)
}

fn search_lane_assignments(
    candidates: &[usize],
    desired: &[(usize, usize)],
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

    for &candidate in candidates {
        if chosen
            .iter()
            .any(|lane| lane.abs_diff(candidate) < MIN_LANE_GAP)
        {
            continue;
        }
        let (source_x, target_x) = desired[index];
        if (candidate != source_x && candidate.abs_diff(source_x) < 2)
            || (candidate != target_x && candidate.abs_diff(target_x) < 2)
        {
            continue;
        }
        if desired
            .iter()
            .enumerate()
            .any(|(other, (other_source_x, _))| other != index && *other_source_x == candidate)
        {
            continue;
        }
        let corridor_left = source_x.min(candidate);
        let corridor_right = source_x.max(candidate);
        if desired
            .iter()
            .enumerate()
            .any(|(other, (other_source_x, _))| {
                other != index
                    && *other_source_x >= corridor_left
                    && *other_source_x <= corridor_right
            })
        {
            continue;
        }
        let source_interval = ordered_interval(source_x, candidate);
        let target_interval = ordered_interval(target_x, candidate);
        if chosen.iter().enumerate().any(|(other, previous_lane)| {
            let (previous_source_x, previous_target_x) = desired[other];
            !intervals_separated(
                source_interval,
                ordered_interval(previous_source_x, *previous_lane),
            ) || !intervals_separated(
                target_interval,
                ordered_interval(previous_target_x, *previous_lane),
            )
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
        search_lane_assignments(
            candidates,
            desired,
            index + 1,
            chosen,
            next_source_cost,
            next_target_cost,
            best,
        );
        chosen.pop();
    }
}

fn ordered_interval(left: usize, right: usize) -> (usize, usize) {
    (left.min(right), left.max(right))
}

fn intervals_separated(left: (usize, usize), right: (usize, usize)) -> bool {
    left.1.saturating_add(1) < right.0 || right.1.saturating_add(1) < left.0
}

#[allow(clippy::too_many_arguments)]
fn append_edge_path(
    plan: &mut FallbackRoutePlan,
    source_x: usize,
    source_y: usize,
    lane: usize,
    arrow_x: usize,
    arrow_y: usize,
    outside_y: usize,
    bottom_y: usize,
    inside_y: usize,
    style: &StyleChars,
) {
    plan.push_vertical(source_x, source_y, outside_y, style.edge_v);
    if source_x != lane {
        let start_corner = if lane > source_x {
            style.corner_dl
        } else {
            style.corner_dr
        };
        let end_corner = if lane > source_x {
            style.corner_ur
        } else {
            style.corner_ul
        };
        plan.push_corner(source_x, outside_y, start_corner);
        plan.push_horizontal(outside_y, source_x, lane, style.edge_h);
        plan.push_corner(lane, outside_y, end_corner);
    }
    plan.push_vertical(lane, outside_y, bottom_y, style.edge_v);
    plan.push_vertical(lane, bottom_y, inside_y, style.edge_v);

    if lane != arrow_x {
        let start_corner = if arrow_x > lane {
            style.corner_dl
        } else {
            style.corner_dr
        };
        let end_corner = if arrow_x > lane {
            style.corner_ur
        } else {
            style.corner_ul
        };
        plan.push_corner(lane, inside_y, start_corner);
        plan.push_horizontal(inside_y, lane, arrow_x, style.edge_h);
        plan.push_corner(arrow_x, inside_y, end_corner);
        plan.push_vertical(arrow_x, inside_y, arrow_y, style.edge_v);
    } else {
        plan.push_vertical(lane, inside_y, arrow_y, style.edge_v);
    }
    plan.push_paint(arrow_x, arrow_y, style.arrow_up);
}

fn edge_plan_blocker(
    plan: &FallbackRoutePlan,
    source_attachment: (usize, usize),
    arrow_attachment: (usize, usize),
    canvas: &Canvas,
    graph: &Graph,
) -> Option<String> {
    for (x, y) in plan.planned_cells() {
        let is_attachment = (x, y) == source_attachment || (x, y) == arrow_attachment;
        if canvas.fallback_route_claims_cell(x, y) {
            return Some(format!("existing fallback reservation blocks ({x},{y})"));
        }
        if graph.nodes.iter().any(|node| {
            let right = node.x.saturating_add(node.width);
            let bottom = node
                .y
                .saturating_add(node.height.max(crate::style::BOX_HEIGHT));
            !is_attachment
                && x >= node.x.saturating_sub(1)
                && x <= right
                && y >= node.y.saturating_sub(1)
                && y <= bottom
        }) {
            return Some(format!("node keepout blocks planned cell at ({x},{y})"));
        }
        if graph.subgraphs.iter().any(|subgraph| {
            let Some(title) = subgraph.title.as_deref() else {
                return false;
            };
            let Some((start, end)) = crate::graph::subgraph_title_span(
                subgraph.bounds.x,
                subgraph.bounds.width,
                title,
                Direction::BT,
            ) else {
                return false;
            };
            let title_y = crate::graph::subgraph_title_row(
                subgraph.bounds.y,
                subgraph.bounds.height,
                Direction::BT,
            );
            x >= start && x <= end && y == title_y
        }) {
            return Some(format!("title text blocks planned cell at ({x},{y})"));
        }
        if canvas.get(x, y) != ' '
            && !plan
                .boundary_claims
                .iter()
                .any(|claim| claim.x == x && claim.y == y)
        {
            return Some(format!("canvas cell blocks planned route at ({x},{y})"));
        }
    }
    None
}

fn reject_scene(canvas: &mut Canvas, scene: &EntryScene, reason: &str) -> HashSet<usize> {
    let owner_id = format!("scene:{STRATEGY}:{}", scene.subgraph_id);
    if crate::runtime::current().diagnostics.routes {
        eprintln!(
            "bt multi-entry scene rejected subgraph={} edges={} reason={reason}",
            scene.subgraph_id,
            scene.edges.len()
        );
    }
    canvas.record_fallback_route_rejection(owner_id, STRATEGY, reason);
    HashSet::new()
}

#[cfg(test)]
mod tests {
    use super::search_lane_assignments;

    #[test]
    fn lane_assignment_keeps_three_entries_separate() {
        let candidates = (15..=36).collect::<Vec<_>>();
        let desired = vec![(6, 9), (18, 21), (30, 33)];
        let mut best = None;
        search_lane_assignments(&candidates, &desired, 0, &mut Vec::new(), 0, 0, &mut best);

        let (_, _, lanes) = best.expect("lane assignment");
        assert_eq!(lanes.len(), 3);
        assert!(lanes.windows(2).all(|pair| pair[0].abs_diff(pair[1]) >= 3));
    }

    #[test]
    fn lane_assignment_prefers_source_alignment_when_total_cost_ties() {
        let candidates = (16..=36).collect::<Vec<_>>();
        let desired = vec![(16, 9), (26, 21), (36, 33)];
        let mut best = None;
        search_lane_assignments(&candidates, &desired, 0, &mut Vec::new(), 0, 0, &mut best);

        let (_, _, lanes) = best.expect("lane assignment");
        assert_eq!(lanes, vec![16, 26, 36]);
    }
}
