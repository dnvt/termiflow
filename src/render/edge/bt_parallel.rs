//! Topology-derived reservations for BT parallel scenes.
//!
//! A BT subgraph that fans out, fans back in, and crosses its outer boundary
//! on both sides is one scene.  Routing each edge independently lets the
//! portal projection and the two junction heuristics claim the same seam in
//! different ways.  This module asks the existing router to solve the whole
//! scene on a private canvas, then records the renderer-resolved cells as one
//! typed fallback reservation before lowering them onto the real canvas.

use std::collections::{HashMap, HashSet};

use crate::graph::{Direction, Graph, Node};
use crate::spacing::SpacingConfig;
use crate::style::StyleChars;

use super::super::bt_parallel_identity::{
    identity_graph, scene_for, target_entry_points, BtParallelIdentityScene,
};
use super::super::canvas::Canvas;
use super::super::fallback_route::FallbackRoutePlan;
use super::super::semantic::CellOwnerKind;
use super::subgraph::lower_bt_fallback_plan;
use super::{
    route_bt_parallel_identity_edges, route_convergent_edges, route_divergent_edges, RouteOwner,
};
use crate::portals::PortalSlots;

const STRATEGY: &str = "bt-parallel-scene-reservation";

/// Reserve one complete BT parallel scene when the graph topology proves that
/// the fan-out, fan-in, and both external boundary crossings belong together.
/// The returned edge indexes are omitted from the ordinary per-source and
/// per-target fallback loops.
pub(crate) fn plan_bt_parallel_scene(
    graph: &Graph,
    canvas: &mut Canvas,
    style: &StyleChars,
    spacing: &SpacingConfig,
    portal_slots: &mut HashMap<String, PortalSlots>,
) -> HashSet<usize> {
    if graph.direction != Direction::BT {
        return HashSet::new();
    }

    let Some(scene) = detect_scene(graph) else {
        return HashSet::new();
    };

    let Some(fanout_source) = graph.get_node(&scene.fanout_source_id) else {
        return HashSet::new();
    };
    let Some(fanin_target) = graph.get_node(&scene.fanin_target_id) else {
        return HashSet::new();
    };
    let Some(incoming) = graph.edges.get(scene.incoming_index) else {
        return HashSet::new();
    };
    let Some(outgoing) = graph.edges.get(scene.outgoing_index) else {
        return HashSet::new();
    };
    let Some(incoming_source) = graph.get_node(&incoming.from) else {
        return HashSet::new();
    };
    let Some(outgoing_target) = graph.get_node(&outgoing.to) else {
        return HashSet::new();
    };
    let branch_nodes: Vec<&Node> = scene
        .branch_ids
        .iter()
        .filter_map(|id| graph.get_node(id))
        .collect();
    if branch_nodes.len() != scene.branch_ids.len() {
        return HashSet::new();
    }

    // Keep the simulation in the same phase order as the main pipeline:
    // convergence first, then source-ordered divergence.  This preserves the
    // established overlap resolver while giving the result one owner.
    let baseline = canvas.clone();
    let mut simulation = baseline.clone();
    simulation.set_write_stage("edge-route-plan-simulation");

    let mut convergence_sources = branch_nodes.clone();
    convergence_sources.sort_by_key(|node| (node.y, node.x, node.id.clone()));
    let identity_graph = identity_graph(graph, &scene);
    let target_entries = target_entry_points(fanin_target);
    let identity_routed = target_entries.len() == convergence_sources.len()
        && route_bt_parallel_identity_edges(
            &convergence_sources,
            fanin_target,
            &mut simulation,
            style,
            Direction::BT,
            &identity_graph,
        );
    if !identity_routed {
        route_convergent_edges(
            &convergence_sources,
            fanin_target,
            &mut simulation,
            style,
            spacing,
            Direction::BT,
            graph,
            None,
        );
    }

    let mut source_targets: HashMap<String, Vec<&Node>> = HashMap::new();
    source_targets.insert(incoming_source.id.clone(), vec![fanout_source]);
    source_targets.insert(fanout_source.id.clone(), branch_nodes.clone());
    source_targets.insert(fanin_target.id.clone(), vec![outgoing_target]);

    let mut source_ids: Vec<String> = source_targets.keys().cloned().collect();
    source_ids.sort_unstable();
    for source_id in source_ids {
        let Some(from) = graph.get_node(&source_id) else {
            continue;
        };
        let Some(targets) = source_targets.get(&source_id) else {
            continue;
        };
        let mut targets = targets.clone();
        targets.sort_by_key(|node| (node.y, node.x, node.id.clone()));
        route_divergent_edges(
            from,
            &targets,
            &mut simulation,
            style,
            spacing,
            Direction::BT,
            graph,
        );
    }

    let paints = simulation.non_space_delta(&baseline);
    if paints.is_empty() {
        return HashSet::new();
    }

    let Some(subgraph) = graph.get_subgraph(&scene.subgraph_id) else {
        return HashSet::new();
    };
    let top_y = subgraph.bounds.y;
    let bottom_y = subgraph
        .bounds
        .y
        .saturating_add(subgraph.bounds.height.saturating_sub(1));
    let top_slot = fanin_target.center_x().clamp(
        subgraph.bounds.x.saturating_add(1),
        subgraph.bounds.x + subgraph.bounds.width.saturating_sub(2),
    );
    let bottom_slot = incoming_source.center_x().clamp(
        subgraph.bounds.x.saturating_add(1),
        subgraph.bounds.x + subgraph.bounds.width.saturating_sub(2),
    );

    // The BT parallel scene owns the external entry lane.  The source lane is
    // the only boundary column that can remain a straight shaft all the way
    // to the external node, so keep the shared render/layout slot aligned to
    // it rather than leaving the ordinary target-centered slot active.
    if let Some(slots) = portal_slots.get_mut(&scene.subgraph_id) {
        slots.bottom.retain(|x| *x == bottom_slot);
        slots.bottom.insert(bottom_slot);
    }

    let owner_id = format!("scene:{STRATEGY}:{}", scene.subgraph_id);
    let owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: owner_id.as_str(),
    };
    let mut plan = FallbackRoutePlan::new(owner_id.clone(), STRATEGY);
    plan.set_scene_coverage(
        scene
            .edge_indices
            .iter()
            .filter_map(|index| {
                graph
                    .edges
                    .get(*index)
                    .map(|edge| edge_owner_id_for_scene(graph, *index, edge))
            })
            .collect::<Vec<_>>(),
    );
    for paint in paints {
        let point = paint.point;
        // Node shapes are deliberately drawn after routes by the main
        // pipeline. Their interior and border metadata are therefore not
        // scene-owned paints; source junction ownership is restored by the
        // normal post-box stage.
        if graph.nodes.iter().any(|node| {
            point.x >= node.x
                && point.x < node.x.saturating_add(node.width)
                && point.y >= node.y
                && point.y
                    < node
                        .y
                        .saturating_add(node.height.max(crate::style::BOX_HEIGHT))
        }) {
            continue;
        }
        if point.y == top_y && point.x != top_slot {
            continue;
        }
        if point.y >= bottom_y {
            continue;
        }
        if point.y == bottom_y && point.x != bottom_slot {
            continue;
        }
        let glyph = if point.x == top_slot && point.y == top_y {
            style.edge_v
        } else {
            paint.glyph
        };
        plan.push_paint(point.x, point.y, glyph);
    }

    // Apply the same straight-lane rule to the external exit. The ordinary
    // route centers the outside target at a different column than the internal
    // fan-in, leaving an arrow one cell away from its visible shaft. A
    // source-aligned target port keeps the boundary opening and arrow legible.
    let exit_arrow_y = outgoing_target.bottom_y();
    let fanin_exit_y = fanin_target.y.saturating_sub(1);
    plan.paints.retain(|paint| paint.point.y > fanin_exit_y);
    plan.push_paint(top_slot, exit_arrow_y, style.arrow_up);
    for y in exit_arrow_y.saturating_add(1)..=fanin_exit_y {
        plan.push_paint(top_slot, y, style.edge_v);
    }

    // The external source sits one row below the physical bottom border. Keep
    // the entry lane straight through the boundary and use a source-aligned
    // interior target port. This avoids a visually meaningless one-cell
    // `+-+`/`├─┘` seam between the border and the external node.
    let target_arrow_y = fanout_source.bottom_y();
    plan.paints.retain(|paint| paint.point.y < target_arrow_y);
    plan.push_paint(bottom_slot, bottom_y, style.edge_v);
    plan.push_paint(bottom_slot, bottom_y.saturating_add(1), style.edge_v);
    plan.push_paint(bottom_slot, target_arrow_y, style.arrow_up);
    for y in target_arrow_y.saturating_add(1)..bottom_y {
        plan.push_paint(bottom_slot, y, style.edge_v);
    }

    // A BT fan-out branch endpoint has one upward shaft and one horizontal
    // arm. The generic overlap resolver can leave a T glyph there because it
    // does not know that the other arm is a branch-owned corner. Canonicalize
    // only the two topology-proven outer endpoints; the center remains the
    // real three-arm split junction.
    let fanout_row = fanout_source.y.saturating_sub(2);
    let mut branch_centers: Vec<usize> = branch_nodes.iter().map(|node| node.center_x()).collect();
    branch_centers.sort_unstable();
    if let (Some(left), Some(right)) = (branch_centers.first(), branch_centers.last()) {
        for paint in &mut plan.paints {
            if paint.point.y != fanout_row {
                continue;
            }
            if paint.point.x == *left {
                paint.glyph = style.corner_ul;
            } else if paint.point.x == *right {
                paint.glyph = style.corner_ur;
            }
        }
    }

    // The intended portal is the boundary lane derived from the fan-in/fan-out
    // node center.  Claiming it explicitly makes border restore and dedicated
    // marker projection agree on one physical opening.
    if plan
        .paints
        .iter()
        .any(|paint| paint.point.x == top_slot && paint.point.y == top_y)
    {
        plan.claim_boundary(
            scene.subgraph_id.clone(),
            "top",
            top_slot,
            top_y,
            style.edge_v,
        );
    }
    if plan
        .paints
        .iter()
        .any(|paint| paint.point.x == bottom_slot && paint.point.y == bottom_y)
    {
        plan.claim_boundary(
            scene.subgraph_id.clone(),
            "bottom",
            bottom_slot,
            bottom_y,
            style.edge_v,
        );
    }

    if plan.validation_error(canvas.width, canvas.height).is_some() {
        return HashSet::new();
    }
    let edge_indices: HashSet<usize> = scene.edge_indices.iter().copied().collect();
    canvas.set_write_stage("edge-route-plan");
    if !lower_bt_fallback_plan(plan, canvas, style, graph, Some(owner)) {
        return HashSet::new();
    }
    edge_indices
}

fn edge_owner_id_for_scene(graph: &Graph, index: usize, edge: &crate::graph::Edge) -> String {
    super::super::provenance::edge_owner_id(index, graph.edges.get(index).unwrap_or(edge))
}

fn detect_scene(graph: &Graph) -> Option<BtParallelIdentityScene> {
    scene_for(graph)
}

#[cfg(test)]
mod tests {
    use super::detect_scene;
    use crate::graph::{Direction, Edge, Graph, Node, Subgraph};

    #[test]
    fn detects_parallel_scene_from_topology_not_fixture_name() {
        let mut graph = Graph::new();
        graph.direction = Direction::BT;
        for id in ["In", "A", "B", "C", "D", "Out"] {
            graph.add_node(Node::new(id, id));
        }
        let subgraph = Subgraph::new("Process", Some("Process".to_owned()));
        graph.add_subgraph(subgraph.clone());
        for id in ["A", "B", "C", "D"] {
            graph.associate_node_with_subgraph(id, "Process");
        }
        graph.edges = vec![
            Edge::new("A", "B"),
            Edge::new("A", "C"),
            Edge::new("B", "D"),
            Edge::new("C", "D"),
            Edge::new("In", "A"),
            Edge::new("D", "Out"),
        ];
        // The detector is intentionally exercised after the graph has its
        // semantic membership; layout coordinates are not part of activation.
        assert!(detect_scene(&graph).is_some());
    }
}
