//! Final render pipeline orchestration.

use anyhow::Result;
use std::collections::{HashMap, HashSet};

use super::canvas::{self, Canvas};
use super::critic::{self, analyze, emit_debug_report};
use super::cycle::{self, route_cycle_edge};
use super::edge::{route_convergent_edges, route_divergent_edges};
use super::labels::{
    draw_convergent_edge_label, draw_edge_label, draw_routed_edge_label, pad_string,
};
use super::outcome::RenderOutcome;
use super::portal_projection::{
    annotate_node_region, annotate_subgraph_region, carve_subgraph_portals_on_canvas,
    finalize_dedicated_portal_markers, finalize_horizontal_side_portals,
    reinforce_subgraph_portals,
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
use crate::graph::{Direction, Graph, Node, NodeShape};
use crate::indexed_graph::{EdgeId, IndexedGraph};
use crate::layout_snapshot::LayoutSnapshot;
use crate::portals::{collect_portal_slots, node_rects_from_graph};
use crate::style::{truncate_label, BaseStyle, BOX_HEIGHT};

pub(super) fn render_with_feedback(graph: &Graph, config: &Config) -> Result<RenderOutcome> {
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

    let mut canvas = Canvas::new(width, height);
    let mut scene_recorder = SceneRecorder::new();
    let chars = config.composite_style.to_style_chars(BaseStyle::default());

    // Draw subgraphs (background layer)
    let subgraph_chars = config.composite_style.to_subgraph_chars();
    for subgraph in &graph.subgraphs {
        shapes::draw_subgraph(
            &mut canvas,
            &subgraph.bounds,
            subgraph.title.as_deref(),
            subgraph_chars,
            graph.direction,
        );
        annotate_subgraph_region(&mut canvas, subgraph, graph.direction);
    }
    // Carve portal openings in subgraph borders so external edges can pass through.
    // Portal behavior is fixed by the runtime boundary snapshot.
    let portals_enabled = !context.compatibility.disable_portals;
    let node_rects = node_rects_from_graph(graph);
    let portal_slots = if portals_enabled {
        collect_portal_slots(graph, &node_rects, graph.direction)
    } else {
        HashMap::new()
    };
    if portals_enabled {
        carve_subgraph_portals_on_canvas(&mut canvas, graph, &portal_slots, graph.direction);
    }

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
    for (_idx, e) in graph.edges.iter().enumerate() {
        let Some(from) = graph_index.node_by_name(&e.from) else {
            continue;
        };
        let Some(to) = graph_index.node_by_name(&e.to) else {
            continue;
        };

        if e.is_back_edge {
            cycle_edges.push((edge_owner_id(_idx, e), from, to));
            continue;
        }

        if !canvas.is_visible(from) || !canvas.is_visible(to) {
            continue;
        }

        sources_with_edges.insert(&e.from);

        if routed_edges.contains(&_idx) {
            continue;
        }

        edges_by_source.entry(&e.from).or_default().push(to);
    }

    // Group edges by target for convergence handling
    let mut edges_by_target: HashMap<&str, Vec<&Node>> = HashMap::new();
    for (_idx, e) in graph.edges.iter().enumerate() {
        if e.is_back_edge {
            continue;
        }
        if routed_edges.contains(&_idx) {
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

    // Draw any precomputed routes first.
    if has_precomputed_routes {
        precomputed::draw_routes(
            graph,
            &layout_snapshot,
            &mut canvas,
            &chars,
            &mut scene_recorder,
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
        if sources.len() > 1 {
            let Some(target) = graph_index.node_by_name(target_id) else {
                continue;
            };
            let mut source_refs: Vec<&Node> = sources.clone();
            source_refs.sort_by_key(|n| (n.y, n.x, n.id.clone()));
            route_convergent_edges(
                &source_refs,
                target,
                &mut canvas,
                &chars,
                &config.spacing,
                graph.direction,
                graph,
            );
            for source in sources {
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

    // Draw edge labels (route-aware for precomputed paths, heuristic for fallback paths)
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

    reinforce_subgraph_portals(
        &mut canvas,
        graph,
        &portal_slots,
        graph.direction,
        &chars,
        subgraph_chars,
    );

    // Draw boxes AFTER edges (boxes overwrite any edges passing through them)
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
        shapes::draw_node(
            &mut canvas,
            node.x,
            node.y,
            node.width,
            node.height.max(BOX_HEIGHT),
            label_lines,
            node.shape,
            &chars,
            graph.direction,
        );
        annotate_node_region(&mut canvas, node, &chars);
    }

    // Draw junction characters AFTER boxes so ports stay visible (boxes overwrite edges).
    // Shows where edges exit source boxes for all orientations (including edges with precomputed routes).
    let mut source_junction_records = Vec::new();
    for &source_id in &source_ids {
        let Some(from) = graph_index.node_by_name(source_id) else {
            continue;
        };
        if !canvas.is_visible(from) {
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

    restore_subgraph_borders(
        &mut canvas,
        graph,
        &portal_slots,
        graph.direction,
        &chars,
        subgraph_chars,
    );

    // Redraw subgraph titles last so portals/edges cannot corrupt the text.
    for subgraph in &graph.subgraphs {
        draw_subgraph_title(
            &mut canvas,
            &subgraph.bounds,
            subgraph.title.as_deref(),
            graph.direction,
        );
    }
    if graph.direction == Direction::BT {
        cleanup_bt_title_rows(&mut canvas, graph, &portal_slots, &chars);
    }

    // ASCII-only cleanup: avoid adjacent '+' on BT horizontal runs when only one stem exists.
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
                    if c0 == '+' && c1 == '+' {
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

    // Debug: print canvas content for convergent edge A7/A8 -> P4
    if context.diagnostics.timing {
        eprintln!("  Input 7/8 -> Process 4 area (y=2-6, x=100-130):");
        for y in 2..=6 {
            let row: String = (100..=130).map(|x| canvas.get(x, y)).collect();
            eprintln!("  y={y}: [{row}]");
        }
        // Mark positions: A7 center=108, A8 center=125, P4 center=101
        let markers: String = (100..=130)
            .map(|x| {
                if x == 108 || x == 125 || x == 101 {
                    '^'
                } else {
                    ' '
                }
            })
            .collect();
        eprintln!("  pos: [{markers}] (^=101,108,125)");
    }

    let optimize_render = config.optimize_render || context.compatibility.optimize_render;

    refresh_provenance(
        &mut canvas,
        graph,
        &chars,
        &portal_slots,
        graph.direction,
        &edge_label_placements,
    );

    if stabilize_straight_segments(&mut canvas, &chars) {
        refresh_provenance(
            &mut canvas,
            graph,
            &chars,
            &portal_slots,
            graph.direction,
            &edge_label_placements,
        );
    }
    if stabilize_junction_cells(&mut canvas, &chars) {
        refresh_provenance(
            &mut canvas,
            graph,
            &chars,
            &portal_slots,
            graph.direction,
            &edge_label_placements,
        );
    }
    if stabilize_degree_mismatches(&mut canvas, &chars) {
        refresh_provenance(
            &mut canvas,
            graph,
            &chars,
            &portal_slots,
            graph.direction,
            &edge_label_placements,
        );
    }
    if stabilize_arrow_shafts(&mut canvas, &chars) {
        refresh_provenance(
            &mut canvas,
            graph,
            &chars,
            &portal_slots,
            graph.direction,
            &edge_label_placements,
        );
    }
    if optimize_render && stabilize_routing_topology(&mut canvas, &chars) {
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
    if optimize_render {
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
    refresh_provenance(
        &mut canvas,
        graph,
        &chars,
        &portal_slots,
        graph.direction,
        &edge_label_placements,
    );
    if stabilize_routing_topology(&mut canvas, &chars) {
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
    })
}
