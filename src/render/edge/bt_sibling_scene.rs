//! Scene-owned BT sibling transitions with one shared target-entry authority.
//!
//! The planner is intentionally topology-derived. It solves the complete
//! strict sibling chain on a cloned canvas, retains the exact target entry in
//! the fallback plan, and only then mutates portal slots and the live canvas.

use std::cmp::Reverse;
use std::collections::{BTreeSet, HashMap, HashSet};

use crate::graph::{Direction, EdgeKind, Graph, Node};
use crate::portals::{
    nudge_portal_x_from_corners, title_safe_portal_x, PortalColumnPreference, PortalSlots,
    BT_SIBLING_CHAIN_MIN_CORRIDOR_GAP, BT_SIBLING_CHAIN_TITLE_MARGIN,
};
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::fallback_route::{FallbackRoutePlan, PortalEntryDecision};
use super::super::provenance::edge_owner_id;
use super::super::semantic::CellOwnerKind;
use super::edge_primitives::{edge_entry_candidates, edge_exit_point};
use super::subgraph::lower_bt_fallback_plan;
use super::RouteOwner;

const STRATEGY: &str = "bt-shared-target-entry-decision";

#[derive(Debug, Clone)]
struct Transition {
    edge_index: usize,
    edge_id: String,
    source_subgraph_id: String,
    target_subgraph_id: String,
    source_node_id: String,
    target_node_id: String,
}

/// Reserve a complete strict BT sibling chain as one scene transaction.
pub(crate) fn plan_bt_sibling_scene(
    graph: &Graph,
    canvas: &mut Canvas,
    style: &StyleChars,
    portal_slots: &mut HashMap<String, PortalSlots>,
    endpoint_contract: Option<&crate::layout_render_contract::BtSiblingEndpointContract>,
) -> HashSet<usize> {
    if graph.direction != Direction::BT {
        return HashSet::new();
    }
    let Some(transitions) = detect_strict_chain(graph) else {
        return HashSet::new();
    };

    let owner_id = format!(
        "scene:{STRATEGY}:{}",
        transitions
            .iter()
            .map(|transition| transition.target_subgraph_id.as_str())
            .collect::<Vec<_>>()
            .join(",")
    );
    let owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: owner_id.as_str(),
    };
    let mut plan = FallbackRoutePlan::new(owner_id.clone(), STRATEGY);
    plan.set_contract_digest(endpoint_contract.map(|contract| contract.digest.clone()));
    plan.set_scene_coverage(
        transitions
            .iter()
            .map(|transition| transition.edge_id.clone()),
    );

    let baseline = canvas.clone();
    let mut occupied = BTreeSet::new();
    let mut decisions = Vec::new();
    let mut source_portal_lanes = Vec::new();

    // Build bottom-to-top so each source boundary is already a stable scene
    // boundary when the next transition is checked.
    for transition in &transitions {
        let contract_transition =
            endpoint_contract.and_then(|contract| contract.for_edge(transition.edge_index));
        let Some(source) = graph.get_node(&transition.source_node_id) else {
            return reject_scene(canvas, &owner_id, "scene source node disappeared");
        };
        let Some(target) = graph.get_node(&transition.target_node_id) else {
            return reject_scene(canvas, &owner_id, "scene target node disappeared");
        };
        let Some(source_subgraph) = graph.get_subgraph(&transition.source_subgraph_id) else {
            return reject_scene(canvas, &owner_id, "scene source boundary disappeared");
        };
        let Some(target_subgraph) = graph.get_subgraph(&transition.target_subgraph_id) else {
            return reject_scene(canvas, &owner_id, "scene target boundary disappeared");
        };

        let (source_x, source_stem_y) = edge_exit_point(source, Direction::BT);
        let preferred_target_x = contract_transition
            .map(|contract| contract.target_lane)
            .unwrap_or_else(|| source.center_x());
        let Some((arrow_x, arrow_y)) =
            choose_target_entry(target, target_subgraph, preferred_target_x)
        else {
            return reject_scene(
                canvas,
                &owner_id,
                "target has no title-safe entry candidate",
            );
        };
        if contract_transition.is_some_and(|contract| arrow_x != contract.target_lane) {
            return reject_scene(
                canvas,
                &owner_id,
                "target entry disagrees with layout endpoint contract",
            );
        }
        let source_top_y = source_subgraph.bounds.y;
        let Some(source_turn_y) = source_top_y
            .checked_add(1)
            .filter(|turn_y| *turn_y < source_stem_y)
        else {
            return reject_scene(
                canvas,
                &owner_id,
                "source has no room for a distinct boundary lane",
            );
        };
        let source_portal_x = if let Some(contract) = contract_transition {
            let min_x = source_subgraph.bounds.x.saturating_add(1);
            let max_x = source_subgraph
                .bounds
                .x
                .saturating_add(source_subgraph.bounds.width.saturating_sub(2));
            (contract.source_lane >= min_x && contract.source_lane <= max_x)
                .then_some(contract.source_lane)
        } else {
            choose_source_portal(source_subgraph, source.center_x(), None)
        };
        let Some(source_portal_x) = source_portal_x else {
            return reject_scene(
                canvas,
                &owner_id,
                "source boundary has no contract or title-safe lane candidate",
            );
        };
        if contract_transition.is_some_and(|contract| source_portal_x != contract.source_lane) {
            return reject_scene(
                canvas,
                &owner_id,
                "source portal disagrees with layout endpoint contract",
            );
        }
        let target_bottom_y = target_subgraph
            .bounds
            .y
            .saturating_add(target_subgraph.bounds.height.saturating_sub(1));
        let corridor_y = contract_transition
            .and_then(|contract| {
                let row = contract.corridor.y + contract.corridor.height / 2;
                (contract.corridor.x == source_portal_x
                    && contract.corridor.x == arrow_x
                    && row > target_bottom_y
                    && row < source_top_y)
                    .then_some(row)
            })
            .or_else(|| {
                choose_corridor_row(
                    source_top_y,
                    target_bottom_y,
                    source_portal_x,
                    arrow_x,
                    &baseline,
                    graph,
                )
            });
        let Some(corridor_y) = corridor_y else {
            return reject_scene(
                canvas,
                &owner_id,
                "scene has no clear exterior corridor row",
            );
        };

        let decision = PortalEntryDecision {
            edge_id: transition.edge_id.clone(),
            owner_id: owner_id.clone(),
            target_node_id: target.id.clone(),
            boundary_id: target_subgraph.id.clone(),
            side: "bottom".to_owned(),
            portal_x: arrow_x,
            portal_y: target_bottom_y,
            arrow_x,
            arrow_y,
        };
        decisions.push(decision.clone());
        source_portal_lanes.push((source_subgraph.id.clone(), source_portal_x));
        plan.set_target_entry_decision(decision);
        plan.claim_boundary(
            source_subgraph.id.clone(),
            "top",
            source_portal_x,
            source_top_y,
            style.edge_v,
        );
        plan.claim_boundary(
            target_subgraph.id.clone(),
            "bottom",
            arrow_x,
            target_bottom_y,
            style.edge_v,
        );
        plan.push_vertical(source_x, source_stem_y, source_turn_y, style.edge_v);
        if source_x != source_portal_x {
            let start_corner = if source_portal_x > source_x {
                style.corner_dl
            } else {
                style.corner_dr
            };
            let end_corner = if source_portal_x > source_x {
                style.corner_ur
            } else {
                style.corner_ul
            };
            plan.push_corner(source_x, source_turn_y, start_corner);
            plan.push_horizontal(source_turn_y, source_x, source_portal_x, style.edge_h);
            plan.push_corner(source_portal_x, source_turn_y, end_corner);
        }
        plan.push_vertical(source_portal_x, source_turn_y, source_top_y, style.edge_v);
        plan.push_vertical(source_portal_x, source_top_y, corridor_y, style.edge_v);
        if source_portal_x != arrow_x {
            let start_corner = if arrow_x > source_portal_x {
                style.corner_dl
            } else {
                style.corner_dr
            };
            let end_corner = if arrow_x > source_portal_x {
                style.corner_ur
            } else {
                style.corner_ul
            };
            plan.push_corner(source_portal_x, corridor_y, start_corner);
            plan.push_horizontal(corridor_y, source_portal_x, arrow_x, style.edge_h);
            plan.push_corner(arrow_x, corridor_y, end_corner);
        }
        plan.push_vertical(arrow_x, corridor_y, target_bottom_y, style.edge_v);
        plan.push_vertical(arrow_x, target_bottom_y, arrow_y, style.edge_v);
        plan.push_paint(arrow_x, arrow_y, style.arrow_up);

        let edge_cells = plan
            .planned_cells()
            .difference(&occupied)
            .copied()
            .collect::<BTreeSet<_>>();
        if edge_cells.len() + occupied.len() != plan.planned_cells().len() {
            return reject_scene(canvas, &owner_id, "scene transitions share a route cell");
        }
        if let Some(reason) = plan_blocker(&plan, &baseline, graph) {
            return reject_scene(canvas, &owner_id, reason.as_str());
        }
        occupied.extend(edge_cells);
    }

    if decisions.len() != transitions.len()
        || decisions
            .iter()
            .map(|decision| (&decision.edge_id, &decision.boundary_id))
            .collect::<BTreeSet<_>>()
            .len()
            != decisions.len()
    {
        return reject_scene(
            canvas,
            &owner_id,
            "scene target-entry decisions are incomplete",
        );
    }
    if let Some(reason) = plan.validation_error(canvas.width, canvas.height) {
        return reject_scene(canvas, &owner_id, reason.as_str());
    }

    let mut simulation = baseline.clone();
    simulation.set_write_stage("bt-shared-target-entry-simulation");
    if !lower_bt_fallback_plan(plan.clone(), &mut simulation, style, graph, Some(owner)) {
        return reject_scene(
            canvas,
            &owner_id,
            "private scene lowering rejected the decision set",
        );
    }
    let simulation_trace = simulation.fallback_route_traces();
    if simulation_trace
        .iter()
        .any(|trace| !trace.mismatches.is_empty())
    {
        return reject_scene(
            canvas,
            &owner_id,
            "private scene trace disagrees with its decisions",
        );
    }

    // The exact decision set is also the source for the final slot reservation.
    // Keep unrelated slots intact, but replace the strict chain's target-side
    // center slots so final projection cannot reconstruct a different lane.
    for decision in &decisions {
        let Some(slots) = portal_slots.get_mut(&decision.boundary_id) else {
            return reject_scene(
                canvas,
                &owner_id,
                "target boundary has no portal slot record",
            );
        };
        slots.bottom.clear();
        slots.bottom.insert(decision.portal_x);
    }
    for (boundary_id, source_portal_x) in source_portal_lanes {
        let Some(slots) = portal_slots.get_mut(&boundary_id) else {
            return reject_scene(
                canvas,
                &owner_id,
                "source boundary has no portal slot record",
            );
        };
        slots.top.clear();
        slots.top.insert(source_portal_x);
    }

    canvas.set_write_stage("bt-shared-target-entry");
    if !lower_bt_fallback_plan(plan, canvas, style, graph, Some(owner)) {
        return reject_scene(
            canvas,
            &owner_id,
            "live scene lowering rejected the decision set",
        );
    }
    transitions
        .into_iter()
        .map(|transition| transition.edge_index)
        .collect()
}

/// Return the titled boundaries owned by the strict BT sibling-chain scene.
///
/// Portal projection uses this selector to apply a final, topology-owned
/// border seam after generic repair passes. Keeping the predicate here makes
/// the visual marker follow the same scene contract as the route planner.
pub(crate) fn strict_chain_subgraph_ids(graph: &Graph) -> Option<HashSet<String>> {
    (graph.direction == Direction::BT).then_some(())?;
    let transitions = detect_strict_chain(graph)?;
    let mut ids = HashSet::new();
    for transition in transitions {
        ids.insert(transition.source_subgraph_id);
        ids.insert(transition.target_subgraph_id);
    }
    Some(ids)
}

/// Return the titled boundaries owned by the exact two-parallel-rail BT
/// sibling target-entry scene. The target-entry planner already consumes this
/// graph-owned selector; final portal projection and critic acceptance reuse
/// it so border seams cannot broaden to generic sibling crossings.
pub(crate) fn sibling_target_entry_subgraph_ids(graph: &Graph) -> Option<HashSet<String>> {
    (graph.direction == Direction::BT).then_some(())?;
    let scene = graph.bt_sibling_target_entry_scene()?;
    Some(HashSet::from([
        scene.source_subgraph_id,
        scene.target_subgraph_id,
    ]))
}

/// Return the titled boundaries owned by the exact direct three-rail BT
/// sibling scene. The graph selector owns the topology; final projection and
/// critic acceptance reuse the same result so this seam cannot broaden to
/// crossed, nested, or generic parallel edges.
pub(crate) fn direct_parallel_sibling_subgraph_ids(graph: &Graph) -> Option<HashSet<String>> {
    let scene = graph.bt_direct_parallel_sibling_scene()?;
    Some(HashSet::from([
        scene.source_subgraph_id,
        scene.target_subgraph_id,
    ]))
}

fn detect_strict_chain(graph: &Graph) -> Option<Vec<Transition>> {
    if graph.subgraphs.len() < 2
        || graph.has_cycles()
        || graph.edges.iter().any(|edge| edge.is_back_edge)
    {
        return None;
    }
    let parent_id = graph.subgraphs.first()?.parent_id.clone();
    let mut chain: Vec<_> = graph
        .subgraphs
        .iter()
        .filter(|subgraph| {
            subgraph.parent_id == parent_id
                && subgraph.child_ids.is_empty()
                && subgraph.title.is_some()
                && subgraph.node_ids.len() == 2
                && subgraph.bounds.is_valid()
        })
        .map(|subgraph| subgraph.id.clone())
        .collect();
    if chain.len() != graph.subgraphs.len() {
        return None;
    }
    chain.sort_by_key(|id| {
        let bounds = &graph
            .get_subgraph(id)
            .expect("chain boundary exists")
            .bounds;
        (Reverse(bounds.y), Reverse(bounds.x), Reverse(id.clone()))
    });
    if chain.windows(2).any(|pair| {
        let lower = &graph
            .get_subgraph(&pair[0])
            .expect("chain boundary exists")
            .bounds;
        let upper = &graph
            .get_subgraph(&pair[1])
            .expect("chain boundary exists")
            .bounds;
        lower.y <= upper.y || upper.y + upper.height > lower.y
    }) {
        return None;
    }

    let mut node_to_subgraph = HashMap::new();
    for subgraph_id in &chain {
        let subgraph = graph.get_subgraph(subgraph_id)?;
        for node_id in &subgraph.node_ids {
            let node = graph.get_node(node_id)?;
            if node.shape != crate::graph::NodeShape::Rectangle {
                return None;
            }
            node_to_subgraph.insert(node_id.as_str(), subgraph_id.as_str());
        }
    }
    if node_to_subgraph.len() != graph.nodes.len() {
        return None;
    }

    let ordinary_edges: Vec<_> = graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge)
        .collect();
    if ordinary_edges.len() != chain.len() * 2 - 1
        || ordinary_edges
            .iter()
            .any(|edge| edge.kind != EdgeKind::Arrow || edge.label.is_some())
    {
        return None;
    }
    for subgraph_id in &chain {
        if ordinary_edges
            .iter()
            .filter(|edge| {
                node_to_subgraph.get(edge.from.as_str()) == Some(&subgraph_id.as_str())
                    && node_to_subgraph.get(edge.to.as_str()) == Some(&subgraph_id.as_str())
            })
            .count()
            != 1
        {
            return None;
        }
    }

    let mut transitions = Vec::new();
    for pair in chain.windows(2) {
        let source_subgraph_id = pair[0].clone();
        let target_subgraph_id = pair[1].clone();
        let candidates: Vec<_> = ordinary_edges
            .iter()
            .filter(|edge| {
                let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
                exits == vec![source_subgraph_id.as_str()]
                    && enters == vec![target_subgraph_id.as_str()]
                    && node_to_subgraph.get(edge.from.as_str())
                        == Some(&source_subgraph_id.as_str())
                    && node_to_subgraph.get(edge.to.as_str()) == Some(&target_subgraph_id.as_str())
            })
            .collect();
        if candidates.len() != 1 {
            return None;
        }
        let edge = candidates[0];
        let edge_index = graph
            .edges
            .iter()
            .position(|candidate| std::ptr::eq(candidate, *edge))?;
        transitions.push(Transition {
            edge_index,
            edge_id: edge_owner_id(edge_index, edge),
            source_subgraph_id,
            target_subgraph_id,
            source_node_id: edge.from.clone(),
            target_node_id: edge.to.clone(),
        });
    }
    Some(transitions)
}

fn choose_target_entry(
    target: &Node,
    target_subgraph: &crate::graph::Subgraph,
    preferred_x: usize,
) -> Option<(usize, usize)> {
    let mut candidates: Vec<_> = edge_entry_candidates(target, Direction::BT)
        .into_iter()
        .filter(|(x, _)| {
            let title_safe = title_safe_portal_x(
                target_subgraph.bounds.x,
                target_subgraph.bounds.width,
                target_subgraph.title.as_deref(),
                *x,
                Direction::BT,
                BT_SIBLING_CHAIN_TITLE_MARGIN,
                PortalColumnPreference::Directional,
            );
            nudge_portal_x_from_corners(
                target_subgraph.bounds.x,
                target_subgraph.bounds.width,
                target_subgraph.title.as_deref(),
                Direction::BT,
                title_safe,
            ) == *x
        })
        .collect();
    candidates.sort_by_key(|(x, _)| {
        (
            x.abs_diff(preferred_x),
            x.abs_diff(target.center_x()),
            Reverse(*x),
        )
    });
    candidates.into_iter().next()
}

fn choose_source_portal(
    source_subgraph: &crate::graph::Subgraph,
    preferred_x: usize,
    required_x: Option<usize>,
) -> Option<usize> {
    let min_x = source_subgraph.bounds.x.saturating_add(1);
    let max_x = source_subgraph
        .bounds
        .x
        .saturating_add(source_subgraph.bounds.width.saturating_sub(2));
    if min_x > max_x {
        return None;
    }

    let mut candidates: Vec<_> = (min_x..=max_x)
        .filter(|x| {
            let title_safe = title_safe_portal_x(
                source_subgraph.bounds.x,
                source_subgraph.bounds.width,
                source_subgraph.title.as_deref(),
                *x,
                Direction::BT,
                BT_SIBLING_CHAIN_TITLE_MARGIN,
                PortalColumnPreference::Directional,
            );
            nudge_portal_x_from_corners(
                source_subgraph.bounds.x,
                source_subgraph.bounds.width,
                source_subgraph.title.as_deref(),
                Direction::BT,
                title_safe,
            ) == *x
        })
        .collect();
    candidates.sort_by_key(|x| {
        (
            x.abs_diff(preferred_x),
            x.abs_diff(source_subgraph.bounds.x + source_subgraph.bounds.width / 2),
            Reverse(*x),
        )
    });
    if let Some(required_x) = required_x {
        if candidates.contains(&required_x) {
            return Some(required_x);
        }
    }
    candidates.into_iter().next()
}

fn choose_corridor_row(
    source_top_y: usize,
    target_bottom_y: usize,
    source_x: usize,
    target_x: usize,
    canvas: &Canvas,
    graph: &Graph,
) -> Option<usize> {
    let inter_border_gap = source_top_y.saturating_sub(target_bottom_y.saturating_add(1));
    if inter_border_gap < BT_SIBLING_CHAIN_MIN_CORRIDOR_GAP {
        return None;
    }
    let first = target_bottom_y.checked_add(2)?;
    let last = source_top_y.checked_sub(2)?;
    let midpoint = first + (last - first) / 2;
    (first..=last)
        .min_by_key(|row| (row.abs_diff(midpoint), *row))
        .filter(|row| {
            let (left, right) = (source_x.min(target_x), source_x.max(target_x));
            (left..=right).all(|x| {
                canvas.get(x, *row) == ' '
                    && !graph.nodes.iter().any(|node| {
                        (x, *row) != edge_exit_point(node, Direction::BT)
                            && node_keepout_contains(node, x, *row)
                    })
            })
        })
}

fn plan_blocker(plan: &FallbackRoutePlan, canvas: &Canvas, graph: &Graph) -> Option<String> {
    for (x, y) in plan.planned_cells() {
        let is_claim = plan
            .boundary_claims
            .iter()
            .any(|claim| claim.x == x && claim.y == y);
        let is_arrow = plan
            .entry_decisions
            .iter()
            .any(|decision| decision.arrow_x == x && decision.arrow_y == y);
        let is_source_attachment = graph
            .nodes
            .iter()
            .any(|node| edge_exit_point(node, Direction::BT) == (x, y));
        if canvas.fallback_route_claims_cell(x, y) {
            return Some(format!("fallback route claim blocks scene cell ({x},{y})"));
        }
        if !is_claim
            && !is_arrow
            && !is_source_attachment
            && graph
                .nodes
                .iter()
                .any(|node| node_keepout_contains(node, x, y))
        {
            return Some(format!("node keepout blocks scene cell ({x},{y})"));
        }
        if !is_claim && canvas.get(x, y) != ' ' {
            return Some(format!("existing canvas cell blocks scene cell ({x},{y})"));
        }
    }
    None
}

fn node_keepout_contains(node: &Node, x: usize, y: usize) -> bool {
    let right = node.x.saturating_add(node.width);
    let bottom = node
        .y
        .saturating_add(node.height.max(crate::style::BOX_HEIGHT));
    x >= node.x.saturating_sub(1) && x <= right && y >= node.y.saturating_sub(1) && y <= bottom
}

fn reject_scene(canvas: &mut Canvas, owner_id: &str, reason: &str) -> HashSet<usize> {
    canvas.record_fallback_route_rejection(owner_id, STRATEGY, reason);
    HashSet::new()
}
