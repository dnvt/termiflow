//! Scene-owned BT routing for direct parallel edges between sibling subgraphs.
//!
//! A pair of titled sibling subgraphs with several aligned edges is not a
//! generic fan-in/fan-out scene.  Each edge needs its own rail at both
//! boundaries; otherwise the first rail can become visually fused with the
//! upper title and the three transitions read as one corridor.  This planner
//! recognizes only the small, provable topology and lowers the complete set
//! as one transactional fallback reservation.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::graph::{Direction, EdgeKind, Graph, Node, NodeShape, Subgraph};
use crate::portals::{
    nudge_portal_x_from_corners, title_safe_portal_x, PortalColumnPreference, PortalSlots,
};
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::fallback_route::{FallbackRoutePlan, PortalEntryDecision};
use super::super::provenance::edge_owner_id;
use super::super::semantic::CellOwnerKind;
use super::edge_primitives::{edge_entry_candidates, edge_exit_point};
use super::subgraph::lower_bt_fallback_plan;
use super::RouteOwner;

const STRATEGY: &str = "bt-parallel-sibling-scene";
const MIN_LANE_GAP: usize = 4;
// Keep one quiet cell beyond the title token before the first direct-parallel
// rail. The wrapper cell alone still makes the rail read as a continuation of
// the title gutter (`Target |`), especially in compact ASCII frames.
const TITLE_MARGIN: usize = 1;
const SOURCE_TURN_OFFSET: usize = 2;
const TARGET_TURN_CLEARANCE: usize = 2;

#[derive(Debug, Clone)]
struct ParallelEdge {
    index: usize,
    edge_id: String,
    source_id: String,
    target_id: String,
}

#[derive(Debug)]
struct ParallelScene {
    source_subgraph_id: String,
    target_subgraph_id: String,
    edges: Vec<ParallelEdge>,
}

/// Reserve direct parallel BT edges between two titled sibling subgraphs.
pub(crate) fn plan_bt_parallel_sibling_scene(
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
    let Some(source_subgraph) = graph.get_subgraph(&scene.source_subgraph_id) else {
        return reject_scene(canvas, &scene, "source sibling boundary disappeared");
    };
    let Some(target_subgraph) = graph.get_subgraph(&scene.target_subgraph_id) else {
        return reject_scene(canvas, &scene, "target sibling boundary disappeared");
    };
    let Some(lanes) = assign_lanes(&scene, source_subgraph, target_subgraph, graph) else {
        return reject_scene(canvas, &scene, "no common title-safe lane assignment");
    };

    let owner_id = format!(
        "scene:{STRATEGY}:{}->{}",
        scene.source_subgraph_id, scene.target_subgraph_id
    );
    let owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: owner_id.as_str(),
    };
    let mut plan = FallbackRoutePlan::new(owner_id.clone(), STRATEGY);
    plan.set_scene_coverage(scene.edges.iter().map(|edge| edge.edge_id.clone()));

    let baseline = canvas.clone();
    let source_top_y = source_subgraph.bounds.y;
    let target_bottom_y = target_subgraph
        .bounds
        .y
        .saturating_add(target_subgraph.bounds.height.saturating_sub(1));
    let Some(source_turn_y) = source_top_y
        .checked_add(SOURCE_TURN_OFFSET)
        .filter(|turn_y| *turn_y < source_top_y.saturating_add(source_subgraph.bounds.height))
    else {
        return reject_scene(canvas, &scene, "source boundary has no routing room");
    };
    let Some(target_title_row) = target_subgraph.title.as_deref().map(|_| {
        crate::graph::subgraph_title_row(
            target_subgraph.bounds.y,
            target_subgraph.bounds.height,
            Direction::BT,
        )
    }) else {
        return reject_scene(canvas, &scene, "target sibling has no title row");
    };
    let Some(target_turn_y) = target_title_row
        .checked_sub(TARGET_TURN_CLEARANCE)
        .filter(|turn_y| *turn_y > target_subgraph.bounds.y && *turn_y < target_bottom_y)
    else {
        return reject_scene(canvas, &scene, "target title has no quiet attachment row");
    };

    let mut occupied = BTreeSet::new();
    for (edge, lane) in scene.edges.iter().zip(&lanes) {
        let Some(source) = graph.get_node(&edge.source_id) else {
            return reject_scene(canvas, &scene, "scene source node disappeared");
        };
        let Some(target) = graph.get_node(&edge.target_id) else {
            return reject_scene(canvas, &scene, "scene target node disappeared");
        };
        let (source_x, source_y) = edge_exit_point(source, Direction::BT);
        let (arrow_x, arrow_y) = choose_target_entry(target, *lane);

        let mut edge_plan = FallbackRoutePlan::new(owner_id.clone(), STRATEGY);
        let decision = PortalEntryDecision {
            edge_id: edge.edge_id.clone(),
            owner_id: owner_id.clone(),
            target_node_id: target.id.clone(),
            boundary_id: target_subgraph.id.clone(),
            side: "bottom".to_owned(),
            portal_x: *lane,
            portal_y: target_bottom_y,
            arrow_x,
            arrow_y,
        };
        edge_plan.set_target_entry_decision(decision);
        edge_plan.claim_boundary(
            source_subgraph.id.clone(),
            "top",
            *lane,
            source_top_y,
            style.edge_v,
        );
        edge_plan.claim_boundary(
            target_subgraph.id.clone(),
            "bottom",
            *lane,
            target_bottom_y,
            style.edge_v,
        );

        edge_plan.push_vertical(source_x, source_y, source_turn_y, style.edge_v);
        append_turn(&mut edge_plan, source_x, source_turn_y, *lane, style);
        edge_plan.push_vertical(*lane, source_turn_y, source_top_y, style.edge_v);
        edge_plan.push_vertical(*lane, source_top_y, target_bottom_y, style.edge_v);
        edge_plan.push_vertical(*lane, target_bottom_y, target_turn_y, style.edge_v);
        append_turn(&mut edge_plan, *lane, target_turn_y, arrow_x, style);
        edge_plan.push_vertical(arrow_x, target_turn_y, arrow_y, style.edge_v);
        edge_plan.push_paint(arrow_x, arrow_y, style.arrow_up);

        let edge_cells = edge_plan.planned_cells();
        if edge_cells.iter().any(|cell| occupied.contains(cell)) {
            return reject_scene(canvas, &scene, "parallel sibling rails share a route cell");
        }
        occupied.extend(edge_cells);
        plan.segments.extend(edge_plan.segments);
        plan.corners.extend(edge_plan.corners);
        plan.paints.extend(edge_plan.paints);
        plan.boundary_claims.extend(edge_plan.boundary_claims);
        plan.entry_decisions.extend(edge_plan.entry_decisions);
    }

    if let Some(reason) = plan_blocker(&plan, &baseline, graph) {
        return reject_scene(canvas, &scene, reason.as_str());
    }
    if let Some(reason) = plan.validation_error(canvas.width, canvas.height) {
        return reject_scene(canvas, &scene, reason.as_str());
    }

    let mut simulation = baseline.clone();
    simulation.set_write_stage("bt-parallel-sibling-simulation");
    if !lower_bt_fallback_plan(plan.clone(), &mut simulation, style, graph, Some(owner)) {
        return reject_scene(canvas, &scene, "private parallel scene lowering rejected");
    }
    let Some(trace) = simulation
        .fallback_route_traces()
        .into_iter()
        .find(|trace| trace.owner_id == owner_id)
    else {
        return reject_scene(
            canvas,
            &scene,
            "private parallel scene trace was not recorded",
        );
    };
    if !trace.mismatches.is_empty() {
        return reject_scene(
            canvas,
            &scene,
            "private parallel scene trace has mismatches",
        );
    }

    let Some(source_slots) = portal_slots.get_mut(&source_subgraph.id) else {
        return reject_scene(canvas, &scene, "source sibling has no portal slot record");
    };
    source_slots.top.clear();
    source_slots.top.extend(lanes.iter().copied());
    let Some(target_slots) = portal_slots.get_mut(&target_subgraph.id) else {
        return reject_scene(canvas, &scene, "target sibling has no portal slot record");
    };
    target_slots.bottom.clear();
    target_slots.bottom.extend(lanes.iter().copied());

    canvas.set_write_stage("bt-parallel-sibling-scene");
    if !lower_bt_fallback_plan(plan, canvas, style, graph, Some(owner)) {
        return reject_scene(canvas, &scene, "live parallel scene lowering rejected");
    }
    if crate::runtime::current().diagnostics.routes {
        eprintln!(
            "bt parallel sibling scene accepted source={} target={} lanes={lanes:?}",
            scene.source_subgraph_id, scene.target_subgraph_id
        );
    }
    scene.edges.into_iter().map(|edge| edge.index).collect()
}

fn detect_scene(graph: &Graph) -> Option<ParallelScene> {
    if graph.subgraphs.len() != 2
        || graph.has_cycles()
        || graph.edges.iter().any(|edge| edge.is_back_edge)
    {
        return None;
    }

    let mut subgraphs: Vec<&Subgraph> = graph
        .subgraphs
        .iter()
        .filter(|subgraph| {
            subgraph.title.is_some()
                && subgraph.parent_id
                    == graph
                        .subgraphs
                        .first()
                        .and_then(|item| item.parent_id.clone())
                && subgraph.child_ids.is_empty()
                && subgraph.bounds.is_valid()
                && subgraph.node_ids.len() >= 3
        })
        .collect();
    if subgraphs.len() != 2 {
        return None;
    }
    subgraphs.sort_by_key(|subgraph| (subgraph.bounds.y, subgraph.bounds.x, &subgraph.id));
    let target_subgraph = subgraphs[0];
    let source_subgraph = subgraphs[1];
    if target_subgraph.bounds.y >= source_subgraph.bounds.y
        || target_subgraph.bounds.y + target_subgraph.bounds.height > source_subgraph.bounds.y
    {
        return None;
    }

    let mut node_to_subgraph = HashMap::new();
    for subgraph in [&source_subgraph, &target_subgraph] {
        for node_id in &subgraph.node_ids {
            let node = graph.get_node(node_id)?;
            if node.shape != NodeShape::Rectangle
                || node_to_subgraph
                    .insert(node_id.as_str(), subgraph.id.as_str())
                    .is_some()
            {
                return None;
            }
        }
    }
    if node_to_subgraph.len() != graph.nodes.len() {
        return None;
    }

    let ordinary_edges: Vec<_> = graph.edges.iter().enumerate().collect();
    if ordinary_edges.len() != 3
        || ordinary_edges
            .iter()
            .any(|(_, edge)| edge.kind != EdgeKind::Arrow || edge.label.is_some())
    {
        return None;
    }

    let mut edges = Vec::with_capacity(ordinary_edges.len());
    let mut source_ids = HashSet::new();
    let mut target_ids = HashSet::new();
    for (index, edge) in ordinary_edges {
        if node_to_subgraph.get(edge.from.as_str()) != Some(&source_subgraph.id.as_str())
            || node_to_subgraph.get(edge.to.as_str()) != Some(&target_subgraph.id.as_str())
        {
            return None;
        }
        let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
        if exits != vec![source_subgraph.id.as_str()]
            || enters != vec![target_subgraph.id.as_str()]
            || !source_ids.insert(edge.from.as_str())
            || !target_ids.insert(edge.to.as_str())
        {
            return None;
        }
        edges.push(ParallelEdge {
            index,
            edge_id: edge_owner_id(index, edge),
            source_id: edge.from.clone(),
            target_id: edge.to.clone(),
        });
    }

    edges.sort_by_key(|edge| {
        (
            graph.get_node(&edge.source_id).map(Node::center_x),
            graph.get_node(&edge.target_id).map(Node::center_x),
            edge.index,
        )
    });
    Some(ParallelScene {
        source_subgraph_id: source_subgraph.id.clone(),
        target_subgraph_id: target_subgraph.id.clone(),
        edges,
    })
}

fn assign_lanes(
    scene: &ParallelScene,
    source_subgraph: &Subgraph,
    target_subgraph: &Subgraph,
    graph: &Graph,
) -> Option<Vec<usize>> {
    let candidates = common_title_safe_lanes(source_subgraph, target_subgraph);
    if candidates.len() < scene.edges.len() {
        return None;
    }
    let desired: Vec<(usize, usize)> = scene
        .edges
        .iter()
        .map(|edge| {
            Some((
                graph.get_node(&edge.source_id)?.center_x(),
                graph.get_node(&edge.target_id)?.center_x(),
            ))
        })
        .collect::<Option<_>>()?;
    if desired
        .windows(2)
        .any(|pair| pair[0].0 > pair[1].0 || pair[0].1 > pair[1].1)
    {
        return None;
    }

    let mut best: Option<(usize, Vec<usize>)> = None;
    for first in 0..candidates.len() {
        for second in first + 1..candidates.len() {
            for third in second + 1..candidates.len() {
                let lanes = vec![candidates[first], candidates[second], candidates[third]];
                if lanes
                    .windows(2)
                    .any(|pair| pair[1] - pair[0] < MIN_LANE_GAP)
                {
                    continue;
                }
                let cost = lanes
                    .iter()
                    .zip(&desired)
                    .map(|(lane, (source_x, target_x))| {
                        lane.abs_diff(*source_x) + lane.abs_diff(*target_x)
                    })
                    .sum();
                let replace = best.as_ref().is_none_or(|(best_cost, best_lanes)| {
                    cost < *best_cost || (cost == *best_cost && lanes < *best_lanes)
                });
                if replace {
                    best = Some((cost, lanes));
                }
            }
        }
    }
    best.map(|(_, lanes)| lanes)
}

fn common_title_safe_lanes(source: &Subgraph, target: &Subgraph) -> Vec<usize> {
    let min_x = source
        .bounds
        .x
        .saturating_add(2)
        .max(target.bounds.x.saturating_add(2));
    let max_x = source
        .bounds
        .x
        .saturating_add(source.bounds.width.saturating_sub(3))
        .min(
            target
                .bounds
                .x
                .saturating_add(target.bounds.width.saturating_sub(3)),
        );
    if min_x > max_x {
        return Vec::new();
    }

    (min_x..=max_x)
        .filter(|x| title_safe_and_not_corner(source, *x) && title_safe_and_not_corner(target, *x))
        .collect()
}

fn title_safe_and_not_corner(subgraph: &Subgraph, x: usize) -> bool {
    let selected = title_safe_portal_x(
        subgraph.bounds.x,
        subgraph.bounds.width,
        subgraph.title.as_deref(),
        x,
        Direction::BT,
        TITLE_MARGIN,
        PortalColumnPreference::Directional,
    );
    nudge_portal_x_from_corners(
        subgraph.bounds.x,
        subgraph.bounds.width,
        subgraph.title.as_deref(),
        Direction::BT,
        selected,
    ) == x
}

fn choose_target_entry(target: &Node, preferred_x: usize) -> (usize, usize) {
    let candidates = edge_entry_candidates(target, Direction::BT);
    let min_x = target.x.saturating_add(1);
    let max_x = target.x + target.width.saturating_sub(2);
    // When title clearance pushes a portal outside the target's usable
    // interior, return to the node center and make the bridge visibly long
    // enough to read as an intentional attachment. A one-cell hook beside a
    // title is exactly the ambiguity this scene owns.
    let desired_x = if preferred_x < min_x || preferred_x > max_x {
        target.center_x()
    } else {
        preferred_x
    };
    candidates
        .into_iter()
        .min_by_key(|(x, y)| (x.abs_diff(desired_x), *y, *x))
        .unwrap_or_else(|| {
            (
                target.center_x(),
                target
                    .bottom_y()
                    .saturating_add(crate::style::BOX_HEIGHT - 3),
            )
        })
}

fn append_turn(
    plan: &mut FallbackRoutePlan,
    from_x: usize,
    y: usize,
    to_x: usize,
    style: &StyleChars,
) {
    if from_x == to_x {
        return;
    }
    let (start_corner, end_corner) = if to_x > from_x {
        (style.corner_dl, style.corner_ur)
    } else {
        (style.corner_dr, style.corner_ul)
    };
    plan.push_corner(from_x, y, start_corner);
    plan.push_horizontal(y, from_x, to_x, style.edge_h);
    plan.push_corner(to_x, y, end_corner);
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
            return Some(format!("subgraph title blocks scene cell ({x},{y})"));
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

fn reject_scene(canvas: &mut Canvas, scene: &ParallelScene, reason: &str) -> HashSet<usize> {
    let owner_id = format!(
        "scene:{STRATEGY}:{}->{}",
        scene.source_subgraph_id, scene.target_subgraph_id
    );
    if crate::runtime::current().diagnostics.routes {
        eprintln!(
            "bt parallel sibling scene rejected source={} target={} edges={} reason={reason}",
            scene.source_subgraph_id,
            scene.target_subgraph_id,
            scene.edges.len()
        );
    }
    canvas.record_fallback_route_rejection(owner_id, STRATEGY, reason);
    HashSet::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lane_assignment_keeps_three_direct_edges_separate() {
        let source = Subgraph::new("source", Some("Source".to_owned()));
        let target = Subgraph::new("target", Some("Target".to_owned()));
        let mut source = source;
        let mut target = target;
        source.bounds = crate::graph::Rectangle::new(0, 16, 43, 10);
        target.bounds = crate::graph::Rectangle::new(0, 0, 43, 10);
        let scene = ParallelScene {
            source_subgraph_id: "source".to_owned(),
            target_subgraph_id: "target".to_owned(),
            edges: vec![
                ParallelEdge {
                    index: 0,
                    edge_id: "e0".to_owned(),
                    source_id: "a".to_owned(),
                    target_id: "d".to_owned(),
                },
                ParallelEdge {
                    index: 1,
                    edge_id: "e1".to_owned(),
                    source_id: "b".to_owned(),
                    target_id: "e".to_owned(),
                },
                ParallelEdge {
                    index: 2,
                    edge_id: "e2".to_owned(),
                    source_id: "c".to_owned(),
                    target_id: "f".to_owned(),
                },
            ],
        };
        let mut graph = Graph::new();
        graph.direction = Direction::BT;
        for (id, x) in [
            ("a", 6),
            ("b", 18),
            ("c", 30),
            ("d", 6),
            ("e", 18),
            ("f", 30),
        ] {
            let mut node = Node::new(id, id);
            node.x = x;
            node.y = if id <= "c" { 20 } else { 2 };
            graph.add_node(node);
        }
        let lanes = assign_lanes(&scene, &source, &target, &graph).expect("three lanes");
        assert_eq!(lanes.len(), 3);
        assert!(lanes
            .windows(2)
            .all(|pair| pair[1] - pair[0] >= MIN_LANE_GAP));
        assert!(
            lanes[0] > 9,
            "first rail keeps a quiet cell beyond the Target title: {lanes:?}"
        );
    }
}
