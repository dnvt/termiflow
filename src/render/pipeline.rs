//! Final render pipeline orchestration.

use anyhow::Result;
use std::collections::{HashMap, HashSet};

use super::canvas::{self, Canvas};
use super::critic::{self, analyze, emit_debug_report};
use super::cycle::{self, route_cycle_edge};
use super::edge::{
    plan_boundary_fan_in_scene, plan_bt_multi_entry_scene, plan_bt_parallel_scene,
    plan_bt_parallel_sibling_scene, plan_bt_sibling_scene, plan_bt_sibling_target_scene,
    plan_dense_crossing_scenes, plan_diamond_scenes, plan_lr_rl_sibling_chain_scene,
    plan_lr_rl_sibling_target_scene, plan_sibling_subgraph_fan_in_scene,
    plan_td_sibling_target_scene, repair_database_source_border, route_convergent_edges,
    route_database_intermediate_scene, route_dedicated_fan_in_edges, route_divergent_edges,
    route_fan_in_identity_edges, route_vertical_branch_rejoin_identity_edges,
    route_vertical_fan_in_edges, route_wide_terminal_fan_in_edges,
};
use super::labels::{
    draw_convergent_edge_label, draw_edge_label, draw_routed_edge_label, pad_string,
};
use super::outcome::RenderOutcome;
use super::portal_projection::{
    annotate_node_region, annotate_subgraph_region, carve_subgraph_portals_on_canvas,
    finalize_dedicated_portal_markers, finalize_horizontal_side_portals,
    finalize_td_parallel_portal_seams, reinforce_subgraph_portals,
};
use super::portal_restore::{cleanup_bt_title_rows, draw_subgraph_title, restore_subgraph_borders};
use super::precomputed;
use super::provenance::{edge_owner_id, refresh_provenance};
use super::repair::{
    optimize_canvas, stabilize_arrow_shafts, stabilize_degree_mismatches, stabilize_junction_cells,
    stabilize_routing_topology, stabilize_straight_segments,
};
use super::scene::{Scene, SceneIntent, SceneRecorder};
use super::semantic::{CellOwnerKind, CellRole, SemanticFrame};
use super::shapes;

use crate::config::Config;
use crate::graph::{Direction, EdgeKind, Graph, Node, NodeShape};
use crate::indexed_graph::{EdgeId, IndexedGraph};
use crate::layout_snapshot::LayoutSnapshot;
use crate::orientation::OrientedCoords;
use crate::portals::{node_rects_from_graph, td_sibling_title_gutter};
use crate::style::{display_width, truncate_label, BaseStyle, BOX_HEIGHT};

pub(super) fn render_with_feedback(graph: &Graph, config: &Config) -> Result<RenderOutcome> {
    render_with_feedback_with_contract(graph, config, None)
}

pub(super) fn render_with_feedback_with_contract(
    graph: &Graph,
    config: &Config,
    endpoint_contract: Option<&crate::layout_render_contract::BtSiblingEndpointContract>,
) -> Result<RenderOutcome> {
    if graph.nodes.is_empty() {
        return Ok(RenderOutcome {
            output: String::new(),
            semantic_frame: SemanticFrame::default(),
            display_semantic_frame: SemanticFrame::default(),
            critic_report: critic::CriticReport {
                score: 0,
                findings: Vec::new(),
                notes: vec![
                    "nodes=0".to_string(),
                    "edges=0".to_string(),
                    "subgraphs=0".to_string(),
                    "frame=0x0".to_string(),
                    "non_space_cells=0".to_string(),
                ],
            },
            warnings: Vec::new(),
            optimized: false,
            repair_passes: 0,
            layout_attempts: 0,
            layout_repairs_applied: 0,
            portal_trace: super::trace::PortalTrace::default(),
        });
    }

    let context = crate::runtime::current();

    // Capture immutable geometry and indexes once at the render boundary.
    // The public Graph remains authoritative for all projection behavior.
    let layout_snapshot = LayoutSnapshot::from_graph(graph);
    let graph_index = IndexedGraph::new(graph);
    let max_right = layout_snapshot.max_right();
    let max_bottom = layout_snapshot.max_bottom();

    // Add gutter space for back-edges:
    // - TD/BT: right gutter (add to width)
    // - LR/RL: bottom gutter (add to height)
    let is_horizontal = matches!(graph.direction, Direction::LR | Direction::RL);
    let cycle_gutter = config.spacing.cycle_gutter;
    let width_gutter = if layout_snapshot.has_cycles() && !is_horizontal {
        cycle_gutter
    } else {
        0
    };
    let height_gutter = if layout_snapshot.has_cycles() && is_horizontal {
        cycle_gutter
    } else {
        0
    };

    let col_spacing = config.spacing.col_spacing;
    let row_spacing = config.spacing.row_spacing;
    let max_canvas_width = config.spacing.max_canvas_width;
    let max_canvas_height = config.spacing.max_canvas_height;

    let mut width = (max_right + col_spacing + width_gutter).min(max_canvas_width);
    width = width
        .max(max_right.saturating_add(1).min(max_canvas_width))
        .max(1);

    let mut height = (max_bottom + row_spacing + height_gutter).min(max_canvas_height);
    height = height
        .max(max_bottom.saturating_add(1).min(max_canvas_height))
        .max(1);

    let portals_enabled = !context.compatibility.disable_portals;
    let node_rects = node_rects_from_graph(graph);
    let mut portal_slots = if portals_enabled {
        crate::portals::collect_portal_slots_with_contract(
            graph,
            &node_rects,
            graph.direction,
            endpoint_contract,
        )
    } else {
        HashMap::new()
    };

    let mut canvas = Canvas::new(width, height);
    let mut scene_recorder = SceneRecorder::new();
    let chars = config.composite_style.to_style_chars(BaseStyle::default());

    // Draw subgraphs (background layer)
    canvas.set_write_stage("subgraph-shape");
    let subgraph_chars = config.composite_style.to_subgraph_chars();
    for subgraph in &graph.subgraphs {
        shapes::draw_subgraph(
            &mut canvas,
            &subgraph.bounds,
            subgraph.title.as_deref(),
            subgraph_chars,
            graph.direction,
        );
        annotate_subgraph_region(
            &mut canvas,
            subgraph,
            graph.direction,
            td_sibling_title_gutter(graph, &subgraph.id),
        );
    }
    // Carve portal openings in subgraph borders so external edges can pass through.
    // Portal behavior is fixed by the runtime boundary snapshot.
    if portals_enabled {
        canvas.set_write_stage("portal-carve");
        carve_subgraph_portals_on_canvas(&mut canvas, graph, &portal_slots, graph.direction);
    }

    // A BT subgraph can contain one coupled parallel scene whose external
    // entry, internal fan-out, internal fan-in, and external exit must share a
    // single reservation.  Activate only from graph topology; fixture names
    // never influence route selection.
    let mut planned_scene_edges = HashSet::new();
    let boundary_fan_in_edges =
        plan_boundary_fan_in_scene(graph, &node_rects, &mut canvas, &chars, &mut portal_slots);
    let mut render_stabilization_protected = !boundary_fan_in_edges.is_empty();
    planned_scene_edges.extend(boundary_fan_in_edges);
    let sibling_subgraph_fan_in_edges = plan_sibling_subgraph_fan_in_scene(
        graph,
        &node_rects,
        &mut canvas,
        &chars,
        &mut portal_slots,
    );
    planned_scene_edges.extend(sibling_subgraph_fan_in_edges);
    if matches!(graph.direction, Direction::LR | Direction::RL) {
        let horizontal_sibling_chain_edges =
            plan_lr_rl_sibling_chain_scene(graph, &mut canvas, &chars, &mut portal_slots);
        render_stabilization_protected |= !horizontal_sibling_chain_edges.is_empty();
        planned_scene_edges.extend(horizontal_sibling_chain_edges);
        let horizontal_sibling_target_edges =
            plan_lr_rl_sibling_target_scene(graph, &mut canvas, &chars, &mut portal_slots);
        render_stabilization_protected |= !horizontal_sibling_target_edges.is_empty();
        planned_scene_edges.extend(horizontal_sibling_target_edges);
    }
    if graph.direction == Direction::TD {
        let td_sibling_target_edges = plan_td_sibling_target_scene(
            graph,
            &mut canvas,
            &chars,
            &config.spacing,
            &mut portal_slots,
        );
        render_stabilization_protected |= !td_sibling_target_edges.is_empty();
        planned_scene_edges.extend(td_sibling_target_edges);
    }
    if graph.direction == Direction::BT {
        planned_scene_edges.extend(plan_bt_parallel_scene(
            graph,
            &mut canvas,
            &chars,
            &config.spacing,
            &mut portal_slots,
        ));
        planned_scene_edges.extend(plan_bt_parallel_sibling_scene(
            graph,
            &mut canvas,
            &chars,
            &mut portal_slots,
        ));
        planned_scene_edges.extend(plan_bt_multi_entry_scene(
            graph,
            &mut canvas,
            &chars,
            &mut portal_slots,
        ));
        planned_scene_edges.extend(plan_bt_sibling_scene(
            graph,
            &mut canvas,
            &chars,
            &mut portal_slots,
            endpoint_contract,
        ));
        planned_scene_edges.extend(plan_bt_sibling_target_scene(
            graph,
            &mut canvas,
            &chars,
            &config.spacing,
            &mut portal_slots,
        ));
    }
    let database_scene_claimed =
        route_database_intermediate_scene(&mut canvas, &chars, graph.direction, graph);
    if database_scene_claimed {
        render_stabilization_protected = true;
        planned_scene_edges.extend(0..graph.edges.len());
    }
    let (diamond_edges, rejected_diamond_edges) = if database_scene_claimed {
        (HashSet::new(), HashSet::new())
    } else {
        plan_diamond_scenes(graph, &mut canvas, &chars)
    };
    planned_scene_edges.extend(diamond_edges);
    let dense_scene_edges = plan_dense_crossing_scenes(graph, &mut canvas, &chars);
    planned_scene_edges.extend(dense_scene_edges.iter().copied());
    // A topology match that could not be safely lowered is deliberately
    // fail-closed.  The edge remains in the evidence as untraced rather than
    // silently re-entering the generic convergence/divergence passes.
    planned_scene_edges.extend(rejected_diamond_edges);

    // Get visible nodes
    let visible_nodes: Vec<&Node> = graph
        .nodes
        .iter()
        .filter(|n| canvas.is_visible(n))
        .collect();

    // Precomputed routes from the layout/routing stages (may be partial).
    let mut routed_edges: HashSet<usize> = HashSet::new();
    for edge_id in layout_snapshot.route_ids() {
        if layout_snapshot
            .route(edge_id)
            .is_some_and(|route| !route.segments.is_empty())
        {
            routed_edges.insert(edge_id.index());
        }
    }
    let has_precomputed_routes = !routed_edges.is_empty();

    // Group forward edges by source node for expanded routing
    let mut edges_by_source: HashMap<&str, Vec<&Node>> = HashMap::new();
    let mut cycle_edges: Vec<(String, &Node, &Node)> = Vec::new();
    let mut sources_with_edges: HashSet<&str> = HashSet::new();

    // First pass: group edges by source (skip edges that already have routed paths)
    for (edge_idx, e) in graph.edges.iter().enumerate() {
        let Some(from) = graph_index.node_by_name(&e.from) else {
            continue;
        };
        let Some(to) = graph_index.node_by_name(&e.to) else {
            continue;
        };

        if e.is_back_edge {
            cycle_edges.push((edge_owner_id(edge_idx, e), from, to));
            continue;
        }

        if !canvas.is_visible(from) || !canvas.is_visible(to) {
            continue;
        }

        sources_with_edges.insert(&e.from);

        if routed_edges.contains(&edge_idx) || planned_scene_edges.contains(&edge_idx) {
            continue;
        }

        edges_by_source.entry(&e.from).or_default().push(to);
    }

    // Group edges by target for convergence handling
    let mut edges_by_target: HashMap<&str, Vec<&Node>> = HashMap::new();
    for (edge_idx, e) in graph.edges.iter().enumerate() {
        if e.is_back_edge {
            continue;
        }
        if routed_edges.contains(&edge_idx) || planned_scene_edges.contains(&edge_idx) {
            continue;
        }
        let Some(from) = graph_index.node_by_name(&e.from) else {
            continue;
        };
        let Some(to) = graph_index.node_by_name(&e.to) else {
            continue;
        };
        if canvas.is_visible(from) && canvas.is_visible(to) {
            edges_by_target.entry(&e.to).or_default().push(from);
        }
    }

    if context.diagnostics.timing {
        eprintln!("render: sources_with_edges {sources_with_edges:?}");
    }

    // Identify convergent labeled edges (those going to targets with multiple sources)
    let convergent_targets: HashSet<&str> = edges_by_target
        .iter()
        .filter(|(_, sources)| sources.len() > 1)
        .map(|(target, _)| *target)
        .collect();

    // Dense crossing grids need distinct fallback merge lanes for overlapping
    // target spans. Keep the hint narrowly scoped to ordinary, untraced
    // non-subgraph fan-in so subgraph portals and precomputed routes retain
    // their existing owners and geometry.
    let merge_lane_hints = dense_convergence_lane_hints(graph, &edges_by_target);
    canvas.set_explicit_crossings_enabled(
        dense_explicit_crossing_policy(graph, &edges_by_source, &edges_by_target, &routed_edges)
            || !dense_scene_edges.is_empty(),
    );

    // Draw any precomputed routes first.
    canvas.set_write_stage("edge-route");
    if has_precomputed_routes {
        precomputed::draw_routes(
            graph,
            &layout_snapshot,
            &mut canvas,
            &chars,
            &mut scene_recorder,
            &planned_scene_edges,
        );
    }

    // Process remaining edges: prioritize convergence (multiple sources → one target)
    let mut processed_edges: HashSet<(&str, &str)> = HashSet::new();

    // First, handle convergence cases (multiple sources → one target).
    // Use a stable ordering on target IDs to keep routing deterministic.
    let mut convergent_target_ids: Vec<&str> = edges_by_target.keys().copied().collect();
    convergent_target_ids.sort_unstable();
    for target_id in convergent_target_ids {
        let Some(sources) = edges_by_target.get(target_id) else {
            continue;
        };
        let pending_sources: Vec<&Node> = sources
            .iter()
            .filter(|source| !processed_edges.contains(&(source.id.as_str(), target_id)))
            .copied()
            .collect();
        if pending_sources.len() > 1 {
            let Some(target) = graph_index.node_by_name(target_id) else {
                continue;
            };
            let mut source_refs: Vec<&Node> = pending_sources;
            source_refs.sort_by_key(|n| (n.y, n.x, n.id.clone()));
            if route_vertical_branch_rejoin_identity_edges(
                &source_refs,
                target,
                &mut canvas,
                &chars,
                graph.direction,
                graph,
            ) || route_wide_terminal_fan_in_edges(
                &source_refs,
                target,
                &mut canvas,
                &chars,
                graph.direction,
                graph,
            ) {
                render_stabilization_protected = true;
            } else if !route_vertical_fan_in_edges(
                &source_refs,
                target,
                &mut canvas,
                &chars,
                graph.direction,
                graph,
            ) && !route_dedicated_fan_in_edges(
                &source_refs,
                target,
                &mut canvas,
                &chars,
                graph.direction,
                graph,
            ) && !route_fan_in_identity_edges(
                &source_refs,
                target,
                &mut canvas,
                &chars,
                graph.direction,
                graph,
            ) {
                route_convergent_edges(
                    &source_refs,
                    target,
                    &mut canvas,
                    &chars,
                    &config.spacing,
                    graph.direction,
                    graph,
                    merge_lane_hints.get(target_id).copied(),
                );
            }
            for source in source_refs {
                processed_edges.insert((&source.id, target_id));
            }
        }
    }

    // Then, handle remaining divergence cases (one source → multiple targets)
    let mut source_ids: Vec<&str> = sources_with_edges.iter().copied().collect();
    source_ids.sort_unstable();
    for &source_id in &source_ids {
        let Some(from) = graph_index.node_by_name(source_id) else {
            continue;
        };
        if let Some(targets) = edges_by_source.get_mut(source_id) {
            // Filter out already processed edges
            let unprocessed: Vec<&Node> = targets
                .iter()
                .filter(|t| !processed_edges.contains(&(source_id, t.id.as_str())))
                .copied()
                .collect();

            if !unprocessed.is_empty() {
                let mut target_refs: Vec<&Node> = unprocessed;
                target_refs.sort_by_key(|n| (n.y, n.x, n.id.clone()));
                route_divergent_edges(
                    from,
                    &target_refs,
                    &mut canvas,
                    &chars,
                    &config.spacing,
                    graph.direction,
                    graph,
                );
            }
        }
    }

    // Draw back-edges (cycle edges) that were not pre-routed.
    for (owner_id, from, to) in cycle_edges {
        route_cycle_edge(
            from,
            to,
            &mut canvas,
            &chars,
            &config.spacing,
            graph.direction,
            Some(owner_id.as_str()),
        );
    }

    canvas.set_write_stage("edge-crossing-finalize");
    canvas.finalize_explicit_crossings(&chars);

    ensure_vertical_edge_label_width(&mut canvas, graph, config);

    // Generic horizontal fanout routes share a collector immediately outside
    // the source. Keep the source's shape-owned wall distinct from that
    // collector; single-edge ports and planned/dense routes retain their
    // existing junction ownership.
    let horizontal_fanout_sources: HashSet<&str> =
        if matches!(graph.direction, Direction::LR | Direction::RL) {
            source_ids
                .iter()
                .copied()
                .filter(|source_id| {
                    edges_by_source.get(*source_id).is_some_and(|targets| {
                        let mut edge_kinds = Vec::new();
                        let unprocessed_count = targets
                            .iter()
                            .filter(|target| {
                                !processed_edges.contains(&(*source_id, target.id.as_str()))
                            })
                            .filter_map(|target| {
                                graph.edges.iter().find(|edge| {
                                    !edge.is_back_edge
                                        && edge.from == **source_id
                                        && edge.to == target.id
                                })
                            })
                            .inspect(|edge| {
                                if !edge_kinds.contains(&edge.kind) {
                                    edge_kinds.push(edge.kind);
                                }
                            })
                            .count();
                        unprocessed_count >= 2 && edge_kinds.len() >= 2
                    })
                })
                .collect()
        } else {
            HashSet::new()
        };

    // Draw edge labels (route-aware for precomputed paths, heuristic for fallback paths)
    canvas.set_write_stage("edge-label");
    let mut edge_label_placements = Vec::new();
    for (edge_idx, edge) in graph.edges.iter().enumerate() {
        let Some(label) = edge.label.as_deref() else {
            continue;
        };
        let (Some(from), Some(to)) = (
            graph_index.node_by_name(&edge.from),
            graph_index.node_by_name(&edge.to),
        ) else {
            continue;
        };
        if !canvas.is_visible(from) || !canvas.is_visible(to) {
            continue;
        }

        if let Some(route) = layout_snapshot.route(EdgeId::from_index(edge_idx)) {
            if let Some(placement) = draw_routed_edge_label(
                &mut canvas,
                route,
                label,
                &chars,
                graph,
                config,
                edge_idx,
                edge,
            ) {
                edge_label_placements.push(placement);
            }
            continue;
        }

        // Fall back to heuristic placement for edges without precomputed routes
        let is_convergent = convergent_targets.contains(to.id.as_str());
        if is_convergent {
            if let Some(placement) = draw_convergent_edge_label(
                &mut canvas,
                from,
                to,
                label,
                graph.direction,
                &chars,
                config,
                edge_idx,
                edge,
            ) {
                edge_label_placements.push(placement);
            }
        } else if let Some(placement) = draw_edge_label(
            &mut canvas,
            from,
            to,
            label,
            graph.direction,
            &chars,
            config,
            edge_idx,
            edge,
            graph,
        ) {
            edge_label_placements.push(placement);
        }
    }

    canvas.set_write_stage("portal-reinforce");
    reinforce_subgraph_portals(
        &mut canvas,
        graph,
        &portal_slots,
        graph.direction,
        &chars,
        subgraph_chars,
    );

    // Draw boxes AFTER edges (boxes overwrite any edges passing through them)
    canvas.set_write_stage("node-shape");
    for node in &visible_nodes {
        let fallback;
        let label_lines: &[String] = if node.label_lines.is_empty() {
            fallback = vec![truncate_label(
                &node.label,
                config.max_label_width.min(node.width.saturating_sub(4)),
            )];
            &fallback
        } else {
            &node.label_lines
        };
        shapes::draw_node_with_fanout_policy(
            &mut canvas,
            node.x,
            node.y,
            node.width,
            node.height.max(BOX_HEIGHT),
            label_lines,
            node.shape,
            &chars,
            graph.direction,
            horizontal_fanout_sources.contains(node.id.as_str()),
        );
        annotate_node_region(&mut canvas, node, &chars);
    }
    if database_scene_claimed {
        canvas.set_write_stage("database-source-border");
        repair_database_source_border(&mut canvas, &chars, graph.direction, graph);
    }

    // Draw junction characters AFTER boxes so ports stay visible (boxes overwrite edges).
    canvas.set_write_stage("source-junction");
    // Shows where edges exit source boxes for all orientations (including edges with precomputed routes).
    let mut source_junction_records = Vec::new();
    for &source_id in &source_ids {
        let Some(from) = graph_index.node_by_name(source_id) else {
            continue;
        };
        if !canvas.is_visible(from) {
            continue;
        }
        if horizontal_fanout_sources.contains(from.id.as_str())
            && shapes::supports_horizontal_fanout_wall(from.shape)
        {
            continue;
        }
        // Diamond points and asymmetric Flag contours are shape-owned, not
        // generic box ports. Their routes already start outside the contour,
        // so stamping a source junction here would replace a shape cell with
        // a detached-looking route glyph.
        if matches!(from.shape, NodeShape::Diamond | NodeShape::Asymmetric) {
            continue;
        }
        // Dense scene lowering owns every outgoing route and already emits
        // dedicated side/edge ports. The generic one-port junction would
        // overwrite that reservation at the node border and visually collapse
        // the two independent exits back into one marker.
        let dense_scene_owns_all_outgoing = graph
            .edges
            .iter()
            .enumerate()
            .filter(|(_, edge)| !edge.is_back_edge && edge.from == from.id)
            .all(|(edge_id, _)| planned_scene_edges.contains(&edge_id));
        if dense_scene_owns_all_outgoing {
            continue;
        }

        let (mut junction_x, junction_y, junction_char) = match graph.direction {
            Direction::LR => (
                from.x + from.width - 1,
                cycle::center_y(from),
                chars.junction_right,
            ),
            Direction::RL => (from.x, cycle::center_y(from), chars.junction_left),
            Direction::TD | Direction::TB => (
                from.center_x(),
                from.bottom_y().saturating_sub(1),
                chars.junction_down,
            ),
            Direction::BT => (from.center_x(), from.y, chars.junction_up),
        };

        // For non-rectangular shapes, the edge stem may not align with `center_x()` when
        // widths are even; prefer the actual outgoing stem column if we can detect it.
        if matches!(graph.direction, Direction::TD | Direction::TB)
            && from.shape == NodeShape::Database
        {
            let below_y = junction_y.saturating_add(1);
            if below_y < canvas.height {
                let mut xs: Vec<usize> = Vec::new();
                for x in (from.x + 1)..(from.x + from.width.saturating_sub(1)) {
                    let c = canvas.get(x, below_y);
                    if canvas::is_vertical(c, &chars)
                        || canvas::is_junction(c, &chars)
                        || canvas::is_arrow(c)
                    {
                        xs.push(x);
                    }
                }
                if !xs.is_empty() {
                    xs.sort_unstable();
                    junction_x = xs[xs.len() / 2];
                }
            }
        }

        if graph.direction == Direction::BT {
            let above_y = from.y.saturating_sub(1);
            if above_y < canvas.height && from.x + 2 <= from.x + from.width.saturating_sub(1) {
                let mut xs: Vec<usize> = Vec::new();
                for x in (from.x + 1)..(from.x + from.width.saturating_sub(1)) {
                    let c = canvas.get(x, above_y);
                    if canvas::is_vertical(c, &chars)
                        || canvas::is_junction(c, &chars)
                        || canvas::is_arrow(c)
                    {
                        xs.push(x);
                    }
                }
                if !xs.is_empty() {
                    let center_x = from.center_x();
                    xs.sort_unstable_by_key(|pos| ((*pos).abs_diff(center_x), *pos));
                    junction_x = xs[0];
                }
            }
        }

        if junction_x < canvas.width && junction_y < canvas.height {
            let mut source_junction_scene = Scene::new();
            source_junction_scene.push(SceneIntent::edge_inferred(
                junction_x,
                junction_y,
                junction_char,
            ));
            source_junction_scene.resolve_with_recorder(
                &mut canvas,
                &chars,
                &mut scene_recorder,
                "source-junction",
            );
            source_junction_records.push((junction_x, junction_y, format!("junction:{source_id}")));
        }
    }

    canvas.set_write_stage("border-restore");
    restore_subgraph_borders(
        &mut canvas,
        graph,
        &portal_slots,
        graph.direction,
        &chars,
        subgraph_chars,
    );

    // Redraw subgraph titles last so portals/edges cannot corrupt the text.
    canvas.set_write_stage("title-redraw");
    for subgraph in &graph.subgraphs {
        draw_subgraph_title(
            &mut canvas,
            &subgraph.bounds,
            subgraph.title.as_deref(),
            graph.direction,
            td_sibling_title_gutter(graph, &subgraph.id),
        );
    }
    if graph.direction == Direction::BT {
        canvas.set_write_stage("bt-title-cleanup");
        cleanup_bt_title_rows(&mut canvas, graph, &portal_slots, &chars);
    }

    // ASCII-only cleanup: avoid adjacent '+' on BT horizontal runs when only one stem exists.
    canvas.set_write_stage("ascii-cleanup");
    if graph.direction == Direction::BT && chars.tl == '+' && chars.h == '-' && chars.v == '|' {
        let is_verticalish = |c: char| -> bool {
            canvas::is_vertical(c, &chars)
                || canvas::is_junction(c, &chars)
                || c == chars.arrow_up
                || c == chars.arrow_down
        };
        if canvas.width > 1 {
            for y in 0..canvas.height {
                let mut x = 0usize;
                while x + 1 < canvas.width {
                    let c0 = canvas.get(x, y);
                    let c1 = canvas.get(x + 1, y);
                    if c0 == '+'
                        && c1 == '+'
                        && !canvas.fallback_route_claims_cell(x, y)
                        && !canvas.fallback_route_claims_cell(x + 1, y)
                    {
                        // A route-owned corner is already an explicit
                        // topology decision. Do not flatten it into a
                        // horizontal stroke merely to remove an adjacent
                        // ASCII `++`; the critic must be able to see the
                        // route's junction at its declared turn.
                        if canvas_has_explicit_route_cell(&canvas, x, y)
                            || canvas_has_explicit_route_cell(&canvas, x + 1, y)
                        {
                            x = x.saturating_add(1);
                            continue;
                        }
                        let above0 = if y > 0 { canvas.get(x, y - 1) } else { ' ' };
                        let below0 = if y + 1 < canvas.height {
                            canvas.get(x, y + 1)
                        } else {
                            ' '
                        };
                        let above1 = if y > 0 { canvas.get(x + 1, y - 1) } else { ' ' };
                        let below1 = if y + 1 < canvas.height {
                            canvas.get(x + 1, y + 1)
                        } else {
                            ' '
                        };
                        let has_vert0 = is_verticalish(above0) || is_verticalish(below0);
                        let has_vert1 = is_verticalish(above1) || is_verticalish(below1);
                        if has_vert0 != has_vert1 {
                            if !has_vert0 {
                                canvas.set(x, y, chars.edge_h);
                            } else {
                                canvas.set(x + 1, y, chars.edge_h);
                            }
                            x = x.saturating_add(1);
                            continue;
                        }
                    }
                    x += 1;
                }
            }
        }
    }

    let optimize_render = config.optimize_render || context.compatibility.optimize_render;

    canvas.set_write_stage("provenance");
    refresh_provenance(
        &mut canvas,
        graph,
        &chars,
        &portal_slots,
        graph.direction,
        &edge_label_placements,
    );

    canvas.set_write_stage("stabilize");
    if !render_stabilization_protected && stabilize_straight_segments(&mut canvas, &chars) {
        canvas.set_write_stage("provenance");
        refresh_provenance(
            &mut canvas,
            graph,
            &chars,
            &portal_slots,
            graph.direction,
            &edge_label_placements,
        );
    }
    canvas.set_write_stage("stabilize");
    if !render_stabilization_protected && stabilize_junction_cells(&mut canvas, &chars) {
        canvas.set_write_stage("provenance");
        refresh_provenance(
            &mut canvas,
            graph,
            &chars,
            &portal_slots,
            graph.direction,
            &edge_label_placements,
        );
    }
    canvas.set_write_stage("stabilize");
    if !render_stabilization_protected && stabilize_degree_mismatches(&mut canvas, &chars) {
        canvas.set_write_stage("provenance");
        refresh_provenance(
            &mut canvas,
            graph,
            &chars,
            &portal_slots,
            graph.direction,
            &edge_label_placements,
        );
    }
    canvas.set_write_stage("stabilize");
    if !render_stabilization_protected && stabilize_arrow_shafts(&mut canvas, &chars) {
        canvas.set_write_stage("provenance");
        refresh_provenance(
            &mut canvas,
            graph,
            &chars,
            &portal_slots,
            graph.direction,
            &edge_label_placements,
        );
    }
    canvas.set_write_stage("stabilize");
    if !render_stabilization_protected
        && optimize_render
        && stabilize_routing_topology(&mut canvas, &chars)
    {
        canvas.set_write_stage("provenance");
        refresh_provenance(
            &mut canvas,
            graph,
            &chars,
            &portal_slots,
            graph.direction,
            &edge_label_placements,
        );
    }

    let debug_critic = config.debug_critic || context.diagnostics.critic;
    let repair_passes = context
        .compatibility
        .render_repair_passes
        .unwrap_or(config.render_repair_passes);

    let mut applied_repair_passes = 0;
    if optimize_render && !render_stabilization_protected {
        canvas.set_write_stage("optimize");
        let _ = optimize_canvas(
            graph,
            &mut canvas,
            graph.direction,
            &chars,
            subgraph_chars,
            &portal_slots,
            &edge_label_placements,
            repair_passes,
        );
        applied_repair_passes = repair_passes;
    }

    canvas.set_write_stage("portal-finalize");
    finalize_horizontal_side_portals(
        &mut canvas,
        graph,
        &layout_snapshot,
        &portal_slots,
        graph.direction,
        &chars,
        subgraph_chars,
    );
    finalize_dedicated_portal_markers(&mut canvas, graph, &portal_slots, &chars);
    canvas.set_write_stage("provenance");
    refresh_provenance(
        &mut canvas,
        graph,
        &chars,
        &portal_slots,
        graph.direction,
        &edge_label_placements,
    );
    canvas.set_write_stage("stabilize");
    if !render_stabilization_protected && stabilize_routing_topology(&mut canvas, &chars) {
        canvas.set_write_stage("provenance");
        refresh_provenance(
            &mut canvas,
            graph,
            &chars,
            &portal_slots,
            graph.direction,
            &edge_label_placements,
        );
    }

    // The final topology stabilization can canonicalize visually detected
    // LR/RL side-wall pierces back into junctions when those pierces are not
    // represented in the semantic portal slot set. Re-stamp dedicated portal
    // openings after that pass so the emitted frame preserves clean border
    // pierces in the final canvas.
    canvas.set_write_stage("portal-finalize");
    finalize_horizontal_side_portals(
        &mut canvas,
        graph,
        &layout_snapshot,
        &portal_slots,
        graph.direction,
        &chars,
        subgraph_chars,
    );
    finalize_dedicated_portal_markers(&mut canvas, graph, &portal_slots, &chars);
    if graph.direction == Direction::BT {
        canvas.set_write_stage("bt-title-finalize");
        cleanup_bt_title_rows(&mut canvas, graph, &portal_slots, &chars);
    }

    // Repair passes can compose a new overlap after the initial edge lowering;
    // apply the same endpoint guard to those late candidates before the frame
    // becomes the semantic/critic input.
    canvas.set_write_stage("edge-crossing-finalize");
    canvas.finalize_explicit_crossings(&chars);

    canvas.set_write_stage("source-junction-ownership");
    let mut source_junction_ownership_scene = Scene::new();
    for (x, y, owner_id) in source_junction_records {
        let glyph = canvas.get(x, y);
        let Some(meta) = canvas.get_meta(x, y) else {
            continue;
        };
        if canvas::is_junction(glyph, &chars) && meta.owner_id.is_none() && meta.z_index == 0 {
            source_junction_ownership_scene.push(SceneIntent::owned(
                x,
                y,
                glyph,
                CellOwnerKind::Junction,
                owner_id,
                CellRole::Junction,
                5,
            ));
        }
    }
    source_junction_ownership_scene.resolve_with_recorder(
        &mut canvas,
        &chars,
        &mut scene_recorder,
        "source-junction-ownership",
    );

    canvas.set_write_stage("portal-seam-finalize");
    finalize_td_parallel_portal_seams(&mut canvas, graph, &portal_slots, &chars, subgraph_chars);

    let portal_trace = super::trace::PortalTrace::from_canvas(
        graph,
        &portal_slots,
        &canvas,
        endpoint_contract.map(|contract| contract.digest.as_str()),
    );
    let semantic_frame = SemanticFrame::from_canvas(&canvas);
    let display_semantic_frame = semantic_frame.crop_and_pad(config.crop, config.pad);
    let critic_report = analyze(graph, &semantic_frame, graph.direction, &chars);
    if debug_critic {
        emit_debug_report(&critic_report);
    }

    let output = if config.crop {
        canvas.to_string_cropped(config.pad)
    } else {
        pad_string(&canvas.to_string(), config.pad)
    };

    Ok(RenderOutcome {
        output,
        semantic_frame,
        display_semantic_frame,
        critic_report,
        warnings: graph.warnings.clone(),
        optimized: optimize_render,
        repair_passes: applied_repair_passes,
        layout_attempts: 1,
        layout_repairs_applied: 0,
        portal_trace,
    })
}

fn canvas_has_explicit_route_cell(canvas: &Canvas, x: usize, y: usize) -> bool {
    canvas.get_meta(x, y).is_some_and(|meta| {
        meta.owner_id.is_some()
            && matches!(
                meta.owner_kind,
                CellOwnerKind::EdgeSegment
                    | CellOwnerKind::Junction
                    | CellOwnerKind::PortalOpening
                    | CellOwnerKind::ArrowHead
                    | CellOwnerKind::CycleEdge
            )
    })
}

fn ensure_vertical_edge_label_width(canvas: &mut Canvas, graph: &Graph, config: &Config) {
    if !matches!(
        graph.direction,
        Direction::TD | Direction::TB | Direction::BT
    ) {
        return;
    }

    let required_width = graph
        .edges
        .iter()
        .filter_map(|edge| edge.label.as_deref())
        .map(|label| display_width(label).min(config.max_edge_label_width))
        .max()
        .unwrap_or(0)
        .min(config.spacing.max_canvas_width);
    canvas.ensure_width(required_width);
}

fn dense_convergence_lane_hints<'a>(
    graph: &'a Graph,
    edges_by_target: &HashMap<&'a str, Vec<&'a Node>>,
) -> HashMap<&'a str, (usize, usize)> {
    let coords = OrientedCoords::new(graph.direction);
    let mut groups: HashMap<usize, Vec<(&'a str, usize, usize, usize)>> = HashMap::new();

    for (&target_id, sources) in edges_by_target {
        if sources.len() < 2
            || graph.get_node_subgraph(target_id).is_some()
            || sources
                .iter()
                .any(|source| graph.get_node_subgraph(&source.id).is_some())
        {
            continue;
        }
        let Some(target) = graph.nodes.iter().find(|node| node.id == target_id) else {
            continue;
        };
        let target_secondary = coords.secondary_coord(target.center_x(), target.center_y());
        let mut span_start = target_secondary;
        let mut span_end = target_secondary;
        for source in sources {
            let secondary = coords.secondary_coord(source.center_x(), source.center_y());
            span_start = span_start.min(secondary);
            span_end = span_end.max(secondary);
        }
        let primary = coords.primary_coord(target.center_x(), target.center_y());
        groups.entry(primary).or_default().push((
            target_id,
            target_secondary,
            span_start,
            span_end,
        ));
    }

    let mut hints = HashMap::new();
    for entries in groups.values_mut() {
        entries.sort_unstable_by_key(|(target_id, target_secondary, _, _)| {
            (*target_secondary, *target_id)
        });
        let crowded = entries
            .iter()
            .enumerate()
            .any(|(index, (_, _, start, end))| {
                entries
                    .iter()
                    .enumerate()
                    .any(|(other_index, (_, _, other_start, other_end))| {
                        index != other_index && *start <= *other_end && *other_start <= *end
                    })
            });
        if !crowded {
            continue;
        }
        let lane_count = entries.len();
        for (lane_index, (target_id, _, _, _)) in entries.iter().enumerate() {
            // The first target group in secondary-axis order benefits from
            // the lane nearest the target-side corridor.  This keeps the
            // long source-to-merge stems from weaving through lower groups in
            // both horizontal and vertical dense scenes.
            let lane_index = lane_count.saturating_sub(1).saturating_sub(lane_index);
            hints.insert(*target_id, (lane_index, lane_count));
        }
    }
    hints
}

/// Return whether this render is the deliberately narrow topology family in
/// which an interior perpendicular overlap can be made explicit.  The policy
/// is derived from the graph and measured layout coordinates; fixture names,
/// labels, and fixed positions are never consulted.
fn dense_explicit_crossing_policy(
    graph: &Graph,
    edges_by_source: &HashMap<&str, Vec<&Node>>,
    edges_by_target: &HashMap<&str, Vec<&Node>>,
    routed_edges: &HashSet<usize>,
) -> bool {
    if !graph.subgraphs.is_empty()
        || !routed_edges.is_empty()
        || graph
            .nodes
            .iter()
            .any(|node| node.shape != NodeShape::Rectangle)
        || graph
            .edges
            .iter()
            .any(|edge| edge.is_back_edge || edge.label.is_some() || edge.kind != EdgeKind::Arrow)
    {
        return false;
    }

    type RankPair<'a> = (HashSet<&'a str>, HashSet<&'a str>);
    let mut rank_pairs: HashMap<(usize, usize), RankPair<'_>> = HashMap::new();
    for edge in &graph.edges {
        let (Some(source), Some(target)) = (
            graph.nodes.iter().find(|node| node.id == edge.from),
            graph.nodes.iter().find(|node| node.id == edge.to),
        ) else {
            return false;
        };
        let (low, high, source_is_low) = if source.rank <= target.rank {
            (source.rank, target.rank, true)
        } else {
            (target.rank, source.rank, false)
        };
        if high != low.saturating_add(1) {
            continue;
        }
        let pair = rank_pairs.entry((low, high)).or_default();
        if source_is_low {
            pair.0.insert(source.id.as_str());
            pair.1.insert(target.id.as_str());
        } else {
            pair.0.insert(target.id.as_str());
            pair.1.insert(source.id.as_str());
        }
    }

    let has_dense_adjacent_rank_pair = rank_pairs
        .values()
        .any(|(sources, targets)| sources.len() >= 3 && targets.len() >= 3);
    let has_multiple_fanout = edges_by_source.values().any(|targets| targets.len() >= 2);
    let has_multiple_fanin = edges_by_target.values().any(|sources| sources.len() >= 2);
    if !has_dense_adjacent_rank_pair || !has_multiple_fanout || !has_multiple_fanin {
        return false;
    }

    let coords = OrientedCoords::new(graph.direction);
    let mut measured_spans = Vec::new();
    for (&target_id, sources) in edges_by_target {
        if sources.len() < 2 {
            continue;
        }
        let Some(target) = graph.nodes.iter().find(|node| node.id == target_id) else {
            return false;
        };
        let target_secondary = coords.secondary_coord(target.center_x(), target.center_y());
        let mut span_start = target_secondary;
        let mut span_end = target_secondary;
        for source in sources {
            let secondary = coords.secondary_coord(source.center_x(), source.center_y());
            span_start = span_start.min(secondary);
            span_end = span_end.max(secondary);
        }
        measured_spans.push((span_start, span_end));
    }

    measured_spans.iter().enumerate().any(|(index, (_, _))| {
        measured_spans
            .iter()
            .enumerate()
            .any(|(other_index, (other_start, other_end))| {
                index != other_index
                    && measured_spans[index].0 <= *other_end
                    && *other_start <= measured_spans[index].1
            })
    })
}
