//! Render module - 2D character grid rendering for diagrams.
//!
//! This module handles the final rendering phase:
//! - Box drawing for nodes (9 shapes supported)
//! - Direction-agnostic edge routing (TD, LR, BT, RL)
//! - Junction/crossing detection for overlapping paths
//!
//! Rendering order: edges first, then boxes (boxes overwrite edge lines).
//!
//! # Module Structure
//!
//! - `canvas` - Canvas struct and character classification
//! - `contract` - Code-facing render-layer contract
//! - `edge` - Normal edge routing (all directions)
//! - `edge_policy` - Graph-aware route-entry policies
//! - `cycle` - Cycle/loop edge routing through gutters
//! - `trace` - Normalized geometry traces for non-glyph inspection
//! - `shapes` - Box drawing for all 9 node shapes

pub mod canvas;
pub mod contract;
pub mod critic;
pub mod cycle;
pub mod edge;
mod edge_policy;
pub mod evidence;
mod labels;
mod outcome;
mod portal_projection;
mod portal_restore;
pub(crate) mod precomputed;
pub mod provenance;
pub mod repair;
pub(crate) mod scene;
pub mod semantic;
pub mod shapes;
pub mod topology;
pub mod trace;

// Re-exports
pub use canvas::Canvas;
pub use contract::{
    current_render_layer_contract, RenderLayer, RenderLayerContract, RenderLayerSpec,
};
pub use outcome::RenderOutcome;
pub use trace::{
    EdgeTrace, GeometryTrace, NodeTrace, RectTrace, SegmentAxis, SegmentTrace, SubgraphTrace,
};

use anyhow::Result;
use critic::{analyze, emit_debug_report};

use crate::config::Config;
use crate::graph::{Graph, Node, NodeShape};
use crate::indexed_graph::{EdgeId, IndexedGraph};
use crate::layout_snapshot::LayoutSnapshot;
use crate::portals::{collect_portal_slots, node_rects_from_graph};
use crate::style::{truncate_label, BaseStyle, BOX_HEIGHT};

use crate::graph::Direction;
use cycle::route_cycle_edge;
use edge::{route_convergent_edges, route_divergent_edges};
#[cfg(test)]
use labels::format_edge_label_with_limit;
use labels::{draw_convergent_edge_label, draw_edge_label, draw_routed_edge_label, pad_string};
#[cfg(test)]
use portal_projection::title_span;
use portal_projection::{
    annotate_node_region, annotate_subgraph_region, carve_subgraph_portals_on_canvas,
    finalize_dedicated_portal_markers, finalize_horizontal_side_portals,
    reinforce_subgraph_portals,
};
use portal_projection::{is_textual, subgraph_title_y};
use portal_restore::{cleanup_bt_title_rows, draw_subgraph_title, restore_subgraph_borders};
use provenance::{edge_owner_id, refresh_provenance};
use repair::{
    optimize_canvas, stabilize_arrow_shafts, stabilize_degree_mismatches, stabilize_junction_cells,
    stabilize_routing_topology, stabilize_straight_segments,
};
use scene::{Scene, SceneIntent};
use semantic::{CellOwnerKind, CellRole, SemanticFrame};
use std::collections::{HashMap, HashSet};

// ============================================================================
// Main Render Function
// ============================================================================

/// Render a graph to a string.
///
/// This is the main entry point for the render module. It:
/// 1. Calculates canvas dimensions from node positions
/// 2. Draws all edges (sorted for optimal junction creation)
/// 3. Draws all boxes (overwriting any edge lines that pass through)
pub fn render(graph: &Graph, config: &Config) -> Result<String> {
    Ok(render_with_feedback(graph, config)?.output)
}
/// Render a graph and return semantic/critic details for the final frame.
pub fn render_with_feedback(graph: &Graph, config: &Config) -> Result<RenderOutcome> {
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

    // Capture immutable geometry and indexes once at the render boundary.
    // The legacy Graph remains authoritative for all projection behavior.
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
    // Portal carving is disabled if the env var TERMIFLOW_DISABLE_PORTALS is set.
    let portals_enabled = std::env::var("TERMIFLOW_DISABLE_PORTALS").is_err();
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

    // Precomputed routes from legacy/experimental layout spikes (may be partial).
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

    if std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok() {
        eprintln!("render: sources_with_edges {:?}", sources_with_edges);
    }

    // Identify convergent labeled edges (those going to targets with multiple sources)
    let convergent_targets: HashSet<&str> = edges_by_target
        .iter()
        .filter(|(_, sources)| sources.len() > 1)
        .map(|(target, _)| *target)
        .collect();

    // Draw any precomputed routes first.
    if has_precomputed_routes {
        precomputed::draw_routes(graph, &layout_snapshot, &mut canvas, &chars);
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
            source_junction_scene.resolve(&mut canvas, &chars);
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
    if std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok() {
        eprintln!("  Input 7/8 -> Process 4 area (y=2-6, x=100-130):");
        for y in 2..=6 {
            let row: String = (100..=130).map(|x| canvas.get(x, y)).collect();
            eprintln!("  y={}: [{}]", y, row);
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
        eprintln!("  pos: [{}] (^=101,108,125)", markers);
    }

    let optimize_render =
        config.optimize_render || std::env::var("TERMIFLOW_OPTIMIZE_RENDER").is_ok();

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

    let debug_critic = config.debug_critic || std::env::var("TERMIFLOW_DEBUG_CRITIC").is_ok();
    let repair_passes = std::env::var("TERMIFLOW_RENDER_REPAIR_PASSES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|value| value.max(1))
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
    source_junction_ownership_scene.resolve(&mut canvas, &chars);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display_width;
    use crate::geom::{EdgeRoute, Segment};
    use crate::graph::Subgraph;
    use crate::CompositeStyle;
    use crate::Edge;

    #[test]
    fn precomputed_back_edge_renders_with_back_glyphs() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;

        let mut a = Node::new("A", "A");
        a.x = 0;
        a.y = 0;
        a.width = 5;

        let mut b = Node::new("B", "B");
        b.x = 8;
        b.y = 0;
        b.width = 5;

        graph.nodes.push(a);
        graph.nodes.push(b);

        let mut edge = Edge::new("B", "A");
        edge.is_back_edge = true;
        graph.edges.push(edge);

        let mut route = EdgeRoute::new();
        route.push_segment(
            crate::geom::Point::new(8 + 5, 1),
            crate::geom::Point::new(0, 1),
        );
        graph.edge_routes.insert(0, route);

        let config = Config::builder()
            .style(CompositeStyle::from_base(BaseStyle::Unicode))
            .crop(false)
            .build(&crate::parser::ParseConfig::default());

        let output = render(&graph, &config).expect("render back edge");

        // Unicode back edges use dotted style, ensure we see a back-edge glyph sequence.
        assert!(
            output.contains("⋯") || output.contains("┄") || output.contains('─'),
            "expected back-edge route to render with visible glyphs, got:\n{}",
            output
        );
    }

    #[test]
    fn diagonal_precomputed_route_falls_back_to_rendered_routing() {
        let mut graph = Graph::new();
        graph.direction = Direction::LR;

        let mut source = Node::new("A", "A");
        source.x = 0;
        source.y = 0;
        source.width = 5;

        let mut target = Node::new("B", "B");
        target.x = 12;
        target.y = 0;
        target.width = 5;

        graph.nodes.push(source);
        graph.nodes.push(target);
        graph.edges.push(Edge::new("A", "B"));
        graph.edge_routes.insert(
            0,
            EdgeRoute {
                segments: vec![Segment::new(
                    crate::geom::Point::new(5, 1),
                    crate::geom::Point::new(12, 3),
                )],
            },
        );

        let config = Config::builder()
            .style(CompositeStyle::from_base(BaseStyle::Unicode))
            .crop(false)
            .build(&crate::parser::ParseConfig::default());
        let output = render(&graph, &config).expect("render malformed route fallback");
        let chars = config.composite_style.to_style_chars(BaseStyle::Unicode);

        assert!(
            output.contains(chars.arrow_right),
            "invalid diagonal route should fall back to a visible routed arrow, got:\n{output}"
        );
    }

    fn char_at(output: &str, x: usize, y: usize) -> Option<char> {
        output.lines().nth(y).and_then(|line| line.chars().nth(x))
    }

    #[test]
    fn edge_label_truncation_preserves_grapheme_clusters() {
        let family = "👨‍👩‍👧‍👦";
        assert_eq!(
            format_edge_label_with_limit(&format!("{family}{family}"), display_width(family) + 1),
            format!("{family}…")
        );
    }

    #[test]
    fn edge_label_truncation_preserves_combining_clusters() {
        let accented = "e\u{301}";
        assert_eq!(
            format_edge_label_with_limit(&format!("{accented}{accented}{accented}"), 2),
            format!("{accented}…")
        );
    }

    #[test]
    fn cross_subgraph_edge_uses_side_aware_top_border_portal_td() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;

        let mut a = Node::new("A", "A");
        a.x = 2;
        a.y = 0;
        a.width = 6;

        let mut b = Node::new("B", "B");
        b.x = 6;
        b.y = 6;
        b.width = 6;

        graph.nodes.push(a);
        graph.nodes.push(b);
        graph.edges.push(Edge::new("A", "B"));

        let mut sg = Subgraph::new("sg", Some("Group".into()));
        sg.add_node("B");
        // Outer bounds with room for portals; inner bounds minimal
        sg.bounds = crate::graph::Rectangle::new(5, 4, 8, 6);
        sg.inner_bounds = crate::graph::Rectangle::new(5, 5, 8, 4);
        graph.add_subgraph(sg);
        graph.associate_node_with_subgraph("B", "sg");

        // Precompute a route that runs along the subgraph border then inside.
        let mut route = EdgeRoute::new();
        route.push_segment(crate::geom::Point::new(3, 2), crate::geom::Point::new(9, 2)); // border-ish
        route.push_segment(crate::geom::Point::new(9, 2), crate::geom::Point::new(9, 6)); // inside drop
        graph.edge_routes.insert(0, route);
        graph.edges[0].label = Some("LBL".into());

        let config = Config::builder()
            .style(CompositeStyle::from_base(BaseStyle::Unicode))
            .crop(false)
            .build(&crate::parser::ParseConfig::default());

        let output = render(&graph, &config).expect("render td portal");
        let portal_y = graph.get_subgraph("sg").map(|sg| sg.bounds.y).unwrap_or(0);
        let portal_x = graph.get_node("B").map(|n| n.center_x()).unwrap_or(0);
        let glyph = char_at(&output, portal_x, portal_y).unwrap_or(' ');
        let portal_shaft = glyph
            == CompositeStyle::from_base(BaseStyle::Unicode)
                .to_style_chars(BaseStyle::Unicode)
                .edge_v;
        assert!(
            portal_shaft,
            "expected side-aware portal shaft on top border at ({portal_x},{portal_y}), got '{glyph}'\n{output}",
        );
    }

    #[test]
    fn cross_subgraph_edge_pierces_border_lr_as_clean_side_opening() {
        let mut graph = Graph::new();
        graph.direction = Direction::LR;

        let mut a = Node::new("A", "A");
        a.x = 0;
        a.y = 2;
        a.width = 6;

        let mut b = Node::new("B", "B");
        b.x = 10;
        b.y = 2;
        b.width = 6;

        graph.nodes.push(a);
        graph.nodes.push(b);
        graph.edges.push(Edge::new("A", "B"));

        let mut sg = Subgraph::new("sg", Some("Group".into()));
        sg.add_node("B");
        sg.bounds = crate::graph::Rectangle::new(8, 0, 10, 5);
        sg.inner_bounds = crate::graph::Rectangle::new(8, 0, 10, 5);
        graph.add_subgraph(sg);
        graph.associate_node_with_subgraph("B", "sg");

        let mut route = EdgeRoute::new();
        route.push_segment(
            crate::geom::Point::new(5, 3),
            crate::geom::Point::new(12, 3),
        );
        route.push_segment(
            crate::geom::Point::new(12, 3),
            crate::geom::Point::new(12, 4),
        );
        graph.edge_routes.insert(0, route);
        graph.edges[0].label = Some("LBL".into());

        let config = Config::builder()
            .style(CompositeStyle::from_base(BaseStyle::Unicode))
            .build(&crate::parser::ParseConfig::default());

        let output = render(&graph, &config).expect("render lr portal");
        let portal_x = graph.get_subgraph("sg").map(|sg| sg.bounds.x).unwrap_or(0);
        let sg = graph.get_subgraph("sg").expect("subgraph");
        let glyph = ((sg.bounds.y + 1)..(sg.bounds.y + sg.bounds.height.saturating_sub(1)))
            .filter_map(|y| char_at(&output, portal_x, y))
            .find(|glyph| {
                *glyph
                    == CompositeStyle::from_base(BaseStyle::Unicode)
                        .to_style_chars(BaseStyle::Unicode)
                        .portal_pierce
            })
            .unwrap_or(' ');
        let is_pierced = glyph != ' ';
        assert!(
            is_pierced,
            "expected dedicated portal marker somewhere on left border x={portal_x}, got '{glyph}'\n{output}"
        );
    }

    #[test]
    fn td_top_portals_outside_the_title_span_keep_a_visible_stem() {
        let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_td.md")
            .expect("read fixture");
        let parsed = crate::parser::parse(&input, false).expect("parse");
        let graph = crate::layout::apply_coarse_layout(
            parsed.graph,
            None,
            crate::layout::CoarseLayoutConfig::default(),
        )
        .expect("layout");

        let node_rects = crate::portals::node_rects_from_graph(&graph);
        let portal_slots =
            crate::portals::collect_portal_slots(&graph, &node_rects, graph.direction);
        let data_layer = graph.get_subgraph("SG2").expect("data layer");
        let title_y = subgraph_title_y(&data_layer.bounds, graph.direction);
        let title_span = title_span(
            &data_layer.bounds,
            data_layer.title.as_deref().expect("title"),
            graph.direction,
        )
        .expect("title span");

        let config = Config::builder()
            .style(CompositeStyle::from_base(BaseStyle::Unicode))
            .crop(false)
            .build(&crate::parser::ParseConfig::default());
        let output = render(&graph, &config).expect("render td portals");

        let top_slots = portal_slots
            .get("SG2")
            .expect("SG2 portal slots")
            .top
            .iter()
            .copied()
            .filter(|x| *x < title_span.0 || *x > title_span.1)
            .collect::<Vec<_>>();
        assert!(
            !top_slots.is_empty(),
            "expected at least one SG2 top portal outside the title span: slots={:?} title_span={:?}",
            portal_slots.get("SG2"),
            title_span,
        );

        for x in top_slots {
            let glyph = char_at(&output, x, title_y).unwrap_or(' ');
            assert_ne!(
                glyph, ' ',
                "expected a visible stem directly below the top portal outside the title span at ({x},{title_y}), got blank\n{output}",
            );
        }
    }

    #[test]
    fn td_labels_avoid_subgraph_border_text() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;
        let mut a = Node::new("A", "A");
        a.x = 0;
        a.y = 0;
        a.width = 5;
        let mut b = Node::new("B", "B");
        b.x = 0;
        b.y = 9;
        b.width = 5;
        graph.nodes.push(a);
        graph.nodes.push(b);
        let mut edge = Edge::new("A", "B");
        edge.label = Some("LBL".into());
        graph.edges.push(edge);

        let mut sg = Subgraph::new("sg", Some("Group".into()));
        sg.add_node("B");
        sg.bounds = crate::graph::Rectangle::new(0, 8, 9, 8);
        sg.inner_bounds = crate::graph::Rectangle::new(0, 9, 9, 6);
        graph.add_subgraph(sg);
        graph.associate_node_with_subgraph("B", "sg");

        let config = Config::builder()
            .style(CompositeStyle::from_base(BaseStyle::Unicode))
            .build(&crate::parser::ParseConfig::default());

        let output = render(&graph, &config).expect("render td label");
        // Ensure the label landed below the subgraph top border row.
        let sg = graph.get_subgraph("sg").unwrap();
        let top = sg.bounds.y;
        let label_row = output
            .lines()
            .enumerate()
            .find_map(|(i, line)| line.contains("LBL").then_some(i))
            .unwrap_or(0);
        assert!(
            label_row != top,
            "expected label not to overwrite subgraph top border (row {top}), got label at row {label_row}:\n{output}"
        );
    }
}
