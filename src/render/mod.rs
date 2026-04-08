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
//! - `edge` - Normal edge routing (all directions)
//! - `cycle` - Cycle/loop edge routing through gutters
//! - `shapes` - Box drawing for all 9 node shapes

pub mod canvas;
pub mod critic;
pub mod cycle;
pub mod edge;
pub mod provenance;
pub mod repair;
pub mod semantic;
pub mod shapes;
pub mod topology;

// Re-exports
pub use canvas::Canvas;

use anyhow::Result;
use critic::{analyze, emit_debug_report};

use crate::config::Config;
use crate::geom::{EdgeRoute, Segment};
use crate::graph::{EdgeKind, Graph, Node, NodeShape};
use crate::portals::{collect_portal_slots, node_rects_from_graph, PortalSlots};
use crate::style::{display_width, truncate_label, BaseStyle, BOX_HEIGHT};

use crate::graph::Direction;
use cycle::route_cycle_edge;
use edge::{route_convergent_edges, route_divergent_edges};
use provenance::{edge_owner_id, refresh_provenance, EdgeLabelPlacement};
use repair::{
    optimize_canvas, stabilize_arrow_shafts, stabilize_degree_mismatches, stabilize_junction_cells,
    stabilize_routing_topology, stabilize_straight_segments,
};
use semantic::{CellOwnerKind, SemanticFrame};
use std::collections::{HashMap, HashSet};

// ============================================================================
// Main Render Function
// ============================================================================

/// Detailed render output including semantic and critic information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOutcome {
    pub output: String,
    pub semantic_frame: SemanticFrame,
    pub critic_report: critic::CriticReport,
    pub warnings: Vec<String>,
    pub optimized: bool,
    pub repair_passes: usize,
    pub layout_attempts: usize,
    pub layout_repairs_applied: usize,
}

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

    // Calculate canvas size from laid-out nodes and subgraphs
    let nodes_right = graph.nodes.iter().map(|n| n.x + n.width).max().unwrap_or(0);
    let nodes_bottom = graph.nodes.iter().map(|n| n.bottom_y()).max().unwrap_or(0);

    let sg_right = graph
        .subgraphs
        .iter()
        .map(|sg| sg.bounds.x + sg.bounds.width)
        .max()
        .unwrap_or(0);
    let sg_bottom = graph
        .subgraphs
        .iter()
        .map(|sg| sg.bounds.y + sg.bounds.height)
        .max()
        .unwrap_or(0);

    let max_right = nodes_right.max(sg_right);
    let max_bottom = nodes_bottom.max(sg_bottom);

    // Add gutter space for back-edges:
    // - TD/BT: right gutter (add to width)
    // - LR/RL: bottom gutter (add to height)
    let is_horizontal = matches!(graph.direction, Direction::LR | Direction::RL);
    let cycle_gutter = config.spacing.cycle_gutter;
    let width_gutter = if graph.has_cycles() && !is_horizontal {
        cycle_gutter
    } else {
        0
    };
    let height_gutter = if graph.has_cycles() && is_horizontal {
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
    for (edge_idx, route) in &graph.edge_routes {
        if !route.segments.is_empty() {
            routed_edges.insert(*edge_idx);
        }
    }
    let has_precomputed_routes = !routed_edges.is_empty();

    // Group forward edges by source node for expanded routing
    let mut edges_by_source: HashMap<&str, Vec<&Node>> = HashMap::new();
    let mut cycle_edges: Vec<(String, &Node, &Node)> = Vec::new();
    let mut sources_with_edges: HashSet<&str> = HashSet::new();

    // First pass: group edges by source (skip edges that already have routed paths)
    for (_idx, e) in graph.edges.iter().enumerate() {
        let Some(from) = graph.get_node(&e.from) else {
            continue;
        };
        let Some(to) = graph.get_node(&e.to) else {
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
        let Some(from) = graph.get_node(&e.from) else {
            continue;
        };
        let Some(to) = graph.get_node(&e.to) else {
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
        draw_precomputed_routes(graph, &mut canvas, &chars);
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
            let Some(target) = graph.get_node(target_id) else {
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
        let Some(from) = graph.get_node(source_id) else {
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
        let (Some(from), Some(to)) = (graph.get_node(&edge.from), graph.get_node(&edge.to)) else {
            continue;
        };
        if !canvas.is_visible(from) || !canvas.is_visible(to) {
            continue;
        }

        if let Some(route) = graph.edge_routes.get(&edge_idx) {
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
    for &source_id in &source_ids {
        let Some(from) = graph.get_node(source_id) else {
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
            canvas.set_edge_char(junction_x, junction_y, junction_char, &chars);
        }
    }

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
        cleanup_bt_title_rows(&mut canvas, graph, &chars);
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

    refresh_provenance(
        &mut canvas,
        graph,
        &chars,
        &portal_slots,
        graph.direction,
        &edge_label_placements,
    );
    let semantic_frame = SemanticFrame::from_canvas(&canvas);
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
        critic_report,
        warnings: graph.warnings.clone(),
        optimized: optimize_render,
        repair_passes: applied_repair_passes,
        layout_attempts: 1,
        layout_repairs_applied: 0,
    })
}

fn pad_string(input: &str, pad: usize) -> String {
    if pad == 0 {
        return input.to_string();
    }

    let prefix = " ".repeat(pad);
    let mut out: Vec<String> = Vec::new();

    for _ in 0..pad {
        out.push(String::new());
    }
    for line in input.lines() {
        if line.is_empty() {
            out.push(String::new());
        } else {
            out.push(format!("{prefix}{line}"));
        }
    }
    for _ in 0..pad {
        out.push(String::new());
    }

    out.join("\n")
}

fn annotate_subgraph_region(
    canvas: &mut Canvas,
    subgraph: &crate::graph::Subgraph,
    direction: Direction,
) {
    let bounds = &subgraph.bounds;
    if !bounds.is_valid() {
        return;
    }

    let x0 = bounds.x;
    let x1 = bounds.x + bounds.width.saturating_sub(1);
    let y0 = bounds.y;
    let y1 = bounds.y + bounds.height.saturating_sub(1);

    for x in x0..=x1 {
        canvas.set_meta_only(
            x,
            y0,
            semantic::CellOwnerKind::SubgraphBorder,
            Some(&subgraph.id),
            semantic::CellRole::Border,
            1,
        );
        canvas.set_meta_only(
            x,
            y1,
            semantic::CellOwnerKind::SubgraphBorder,
            Some(&subgraph.id),
            semantic::CellRole::Border,
            1,
        );
    }
    for y in y0..=y1 {
        canvas.set_meta_only(
            x0,
            y,
            semantic::CellOwnerKind::SubgraphBorder,
            Some(&subgraph.id),
            semantic::CellRole::Border,
            1,
        );
        canvas.set_meta_only(
            x1,
            y,
            semantic::CellOwnerKind::SubgraphBorder,
            Some(&subgraph.id),
            semantic::CellRole::Border,
            1,
        );
    }

    if let Some(title) = subgraph.title.as_deref() {
        let title_fmt = format!("[  {}  ]", title);
        let title_len = title_fmt.chars().count();
        if title_len <= bounds.width.saturating_sub(2) {
            let start_x = bounds.x + (bounds.width - title_len) / 2;
            let title_y = subgraph_title_y(bounds, direction);
            for (i, _) in title_fmt.chars().enumerate() {
                let x = start_x + i;
                if x < canvas.width {
                    canvas.set_meta_only(
                        x,
                        title_y,
                        semantic::CellOwnerKind::SubgraphTitle,
                        Some(&subgraph.id),
                        semantic::CellRole::Text,
                        2,
                    );
                }
            }
        }
    }
}

fn annotate_node_region(canvas: &mut Canvas, node: &Node, chars: &crate::style::StyleChars) {
    for y in node.y..node.y + node.height.max(BOX_HEIGHT) {
        for x in node.x..node.x + node.width {
            if x >= canvas.width || y >= canvas.height {
                continue;
            }
            let ch = canvas.get(x, y);
            let (owner_kind, role) = if ch == ' ' {
                (semantic::CellOwnerKind::NodeFill, semantic::CellRole::Fill)
            } else if crate::render::canvas::is_horizontal(ch, chars)
                || crate::render::canvas::is_vertical(ch, chars)
                || crate::render::canvas::is_junction(ch, chars)
                || crate::render::canvas::is_corner(ch, chars)
                || matches!(ch, '(' | ')' | '<' | '>' | '/' | '\\')
            {
                (
                    semantic::CellOwnerKind::NodeBorder,
                    semantic::CellRole::Border,
                )
            } else {
                (semantic::CellOwnerKind::NodeLabel, semantic::CellRole::Text)
            };
            canvas.set_meta_only(x, y, owner_kind, Some(&node.id), role, 3);
        }
    }
}

// ============================================================================
// Precomputed Edge Route Rendering (experimental)
// ============================================================================

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

const PRECOMPUTED_ROUTE_Z_INDEX: u8 = 5;

#[derive(Copy, Clone)]
struct PrecomputedRouteOwner<'a> {
    kind: CellOwnerKind,
    id: &'a str,
}

fn dir_from_segment(seg: &Segment) -> Option<Dir> {
    if seg.from.x == seg.to.x {
        if seg.to.y > seg.from.y {
            Some(Dir::Down)
        } else if seg.to.y < seg.from.y {
            Some(Dir::Up)
        } else {
            None
        }
    } else if seg.from.y == seg.to.y {
        if seg.to.x > seg.from.x {
            Some(Dir::Right)
        } else if seg.to.x < seg.from.x {
            Some(Dir::Left)
        } else {
            None
        }
    } else {
        None
    }
}

fn opposite_dir(d: Dir) -> Dir {
    match d {
        Dir::Up => Dir::Down,
        Dir::Down => Dir::Up,
        Dir::Left => Dir::Right,
        Dir::Right => Dir::Left,
    }
}

fn corner_for_turn(prev: Dir, next: Dir, chars: &StyleChars) -> Option<char> {
    use Dir::*;
    let a = opposite_dir(prev);
    let b = next;
    // Corner character needed based on the two arms (where we came from, where we're going):
    // ┘ (corner_ur) = UP + LEFT arms
    // └ (corner_ul) = UP + RIGHT arms
    // ┐ (corner_dr) = DOWN + LEFT arms
    // ┌ (corner_dl) = DOWN + RIGHT arms
    match (a, b) {
        (Up, Left) | (Left, Up) => Some(chars.corner_ur), // ┘
        (Up, Right) | (Right, Up) => Some(chars.corner_ul), // └
        (Down, Left) | (Left, Down) => Some(chars.corner_dr), // ┐
        (Down, Right) | (Right, Down) => Some(chars.corner_dl), // ┌
        _ => None,
    }
}

fn arrow_for_dir(dir: Dir, chars: &StyleChars) -> char {
    match dir {
        Dir::Up => chars.arrow_up,
        Dir::Down => chars.arrow_down,
        Dir::Left => chars.arrow_left,
        Dir::Right => chars.arrow_right,
    }
}

fn is_subgraph_title_cell(graph: &Graph, x: usize, y: usize) -> bool {
    graph.subgraphs.iter().any(|sg| {
        if sg.title.is_none() || !sg.bounds.is_valid() {
            return false;
        }
        let title_y = subgraph_title_y(&sg.bounds, graph.direction);
        y == title_y && x >= sg.bounds.x && x < sg.bounds.x.saturating_add(sg.bounds.width)
    })
}

#[allow(clippy::too_many_arguments)]
fn draw_segment(
    seg: &Segment,
    dir: Dir,
    canvas: &mut Canvas,
    chars: &StyleChars,
    skip_start: bool,
    skip_end: bool,
    graph: &Graph,
    owner: PrecomputedRouteOwner<'_>,
) {
    match dir {
        Dir::Left | Dir::Right => {
            let (min, max) = if seg.from.x <= seg.to.x {
                (seg.from.x, seg.to.x)
            } else {
                (seg.to.x, seg.from.x)
            };

            // Apply adjustments based on which end is 'start' and 'end'
            // If moving Right (from=min), skip_start increases min, skip_end decreases max
            // If moving Left (from=max), skip_start decreases max, skip_end increases min

            let (draw_start, draw_end) = if seg.from.x == min {
                // Moving Right
                (
                    min + if skip_start { 1 } else { 0 },
                    max.saturating_sub(if skip_end { 1 } else { 0 }),
                )
            } else {
                // Moving Left
                (
                    min + if skip_end { 1 } else { 0 },
                    max.saturating_sub(if skip_start { 1 } else { 0 }),
                )
            };

            if draw_start <= draw_end {
                for x in draw_start..=draw_end {
                    if is_subgraph_title_cell(graph, x, seg.from.y) {
                        continue;
                    }
                    set_precomputed_route_edge_char(
                        canvas,
                        x,
                        seg.from.y,
                        chars.edge_h,
                        chars,
                        owner,
                    );
                }
            }
        }
        Dir::Up | Dir::Down => {
            let (min, max) = if seg.from.y <= seg.to.y {
                (seg.from.y, seg.to.y)
            } else {
                (seg.to.y, seg.from.y)
            };

            let (draw_start, draw_end) = if seg.from.y == min {
                // Moving Down
                (
                    min + if skip_start { 1 } else { 0 },
                    max.saturating_sub(if skip_end { 1 } else { 0 }),
                )
            } else {
                // Moving Up
                (
                    min + if skip_end { 1 } else { 0 },
                    max.saturating_sub(if skip_start { 1 } else { 0 }),
                )
            };

            if draw_start <= draw_end {
                for y in draw_start..=draw_end {
                    if is_subgraph_title_cell(graph, seg.from.x, y) {
                        continue;
                    }
                    set_precomputed_route_edge_char(
                        canvas,
                        seg.from.x,
                        y,
                        chars.edge_v,
                        chars,
                        owner,
                    );
                }
            }
        }
    }
}

fn draw_precomputed_routes(graph: &Graph, canvas: &mut Canvas, chars: &StyleChars) {
    let mut edge_ids: Vec<usize> = graph.edge_routes.keys().copied().collect();
    edge_ids.sort_unstable();

    for edge_idx in edge_ids {
        let Some(route) = graph.edge_routes.get(&edge_idx) else {
            continue;
        };
        if route.segments.is_empty() {
            continue;
        }

        let Some(edge) = graph.edges.get(edge_idx) else {
            continue;
        };
        let owner_id = edge_owner_id(edge_idx, edge);
        let owner = PrecomputedRouteOwner {
            kind: if edge.is_back_edge {
                CellOwnerKind::CycleEdge
            } else {
                CellOwnerKind::EdgeSegment
            },
            id: owner_id.as_str(),
        };
        let (Some(from), Some(to)) = (graph.get_node(&edge.from), graph.get_node(&edge.to)) else {
            continue;
        };
        if !canvas.is_visible(from) || !canvas.is_visible(to) {
            continue;
        }

        // Apply edge-kind-specific shaft characters.
        let mut route_chars = *chars;
        match edge.kind {
            EdgeKind::Arrow
            | EdgeKind::Open
            | EdgeKind::Bidirectional
            | EdgeKind::CircleEnd
            | EdgeKind::CrossEnd => {} // use default edge chars
            EdgeKind::Thick => {
                // Heavy/bold shaft chars
                route_chars.edge_h = '━';
                route_chars.edge_v = '┃';
            }
            EdgeKind::Dotted => {
                route_chars.edge_h = chars.dotted_h;
                route_chars.edge_v = chars.dotted_v;
            }
        }
        // Back-edges always override with cycle styling.
        if edge.is_back_edge {
            route_chars.edge_h = chars.back_h;
            route_chars.edge_v = chars.back_v;
        }

        // Track if we need to draw a corner for perpendicular first segment
        let mut needs_start_corner: Option<(usize, usize, char)> = None;

        // Draw stem from source node exit to route start if the route starts
        // with a perpendicular segment (horizontal in TD/BT, vertical in LR/RL).
        // This handles cases where the route detours before dropping to target.
        if let Some(first_seg) = route.segments.first() {
            let first_dir = dir_from_segment(first_seg);
            let route_start = first_seg.from;
            let src_center_x = from.center_x();
            let src_center_y = from.center_y();

            match graph.direction {
                Direction::TD | Direction::TB => {
                    // In TD/BT, first segment should be vertical (Down/Up)
                    // If it's horizontal (Left/Right), we need a connecting stem and corner
                    if matches!(first_dir, Some(Dir::Left) | Some(Dir::Right)) {
                        // Box border is at y = from.y + from.height - 1
                        // We need to draw from box border down to route start to create junction
                        let box_border_y = from.y + from.height - 1;
                        if std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok() {
                            eprintln!(
                                "  TD horizontal-first: src_center_x={} box_border_y={} route_start.y={}",
                                src_center_x, box_border_y, route_start.y
                            );
                        }
                        // Draw vertical stem from box border to the route start row (exclusive)
                        // This will create a junction on the box border via resolve_overlap
                        for y in box_border_y..route_start.y {
                            if std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok() {
                                eprintln!("    drawing stem at ({}, {})", src_center_x, y);
                            }
                            set_precomputed_route_edge_char(
                                canvas,
                                src_center_x,
                                y,
                                route_chars.edge_v,
                                &route_chars,
                                owner,
                            );
                        }
                        // Queue corner to be drawn AFTER segments (so it overwrites)
                        // At the source center, we need a corner character that connects:
                        // - UP (to the box border junction above)
                        // - LEFT/RIGHT (horizontal segment to turn point)
                        // Use corner characters ┘ (up/left) or └ (up/right) - no down arm needed
                        let corner = if first_dir == Some(Dir::Left) {
                            route_chars.corner_ur // ┘ - connects up, left
                        } else {
                            route_chars.corner_ul // └ - connects up, right
                        };
                        if std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok() {
                            eprintln!(
                                "    needs_start_corner=({}, {}, '{}')",
                                src_center_x, route_start.y, corner
                            );
                        }
                        needs_start_corner = Some((src_center_x, route_start.y, corner));
                    }
                }
                Direction::BT => {
                    if matches!(first_dir, Some(Dir::Left) | Some(Dir::Right)) {
                        // Box top border is at y = from.y (for BT, edges exit from top)
                        // Draw vertical stem from route start up to box border (inclusive)
                        let box_border_y = from.y;
                        for y in (route_start.y + 1)..=box_border_y {
                            set_precomputed_route_edge_char(
                                canvas,
                                src_center_x,
                                y,
                                route_chars.edge_v,
                                &route_chars,
                                owner,
                            );
                        }
                        let corner = if first_dir == Some(Dir::Left) {
                            route_chars.corner_ur // ┘ - going left from here
                        } else {
                            route_chars.corner_ul // └ - going right from here
                        };
                        needs_start_corner = Some((src_center_x, route_start.y, corner));
                    }
                }
                Direction::LR => {
                    if matches!(first_dir, Some(Dir::Up) | Some(Dir::Down)) {
                        let exit_x = from.x + from.width;
                        for x in exit_x..route_start.x {
                            set_precomputed_route_edge_char(
                                canvas,
                                x,
                                src_center_y,
                                route_chars.edge_h,
                                &route_chars,
                                owner,
                            );
                        }
                    }
                }
                Direction::RL => {
                    if matches!(first_dir, Some(Dir::Up) | Some(Dir::Down)) {
                        let exit_x = from.x.saturating_sub(1);
                        for x in (route_start.x + 1)..=exit_x {
                            set_precomputed_route_edge_char(
                                canvas,
                                x,
                                src_center_y,
                                route_chars.edge_h,
                                &route_chars,
                                owner,
                            );
                        }
                    }
                }
            }
        }

        for i in 0..route.segments.len() {
            let seg = &route.segments[i];
            let Some(dir) = dir_from_segment(seg) else {
                continue;
            };

            let mut next_dir = None;
            if i + 1 < route.segments.len() {
                next_dir = dir_from_segment(&route.segments[i + 1]);
            }

            let is_turn = if let Some(nd) = next_dir {
                nd != dir
            } else {
                false
            };

            let skip_start = i > 0;
            let skip_end = is_turn;

            draw_segment(
                seg,
                dir,
                canvas,
                &route_chars,
                skip_start,
                skip_end,
                graph,
                owner,
            );

            if is_turn {
                if let Some(nd) = next_dir {
                    if let Some(corner) = corner_for_turn(dir, nd, &route_chars) {
                        if !is_subgraph_title_cell(graph, seg.to.x, seg.to.y) {
                            set_precomputed_route_edge_char(
                                canvas,
                                seg.to.x,
                                seg.to.y,
                                corner,
                                &route_chars,
                                owner,
                            );
                        }
                    }
                }
            }
        }

        if let Some(last_seg) = route.segments.last() {
            let dir = dir_from_segment(last_seg).unwrap_or(match graph.direction {
                Direction::TD | Direction::TB => Dir::Down,
                Direction::BT => Dir::Up,
                Direction::LR => Dir::Right,
                Direction::RL => Dir::Left,
            });
            // Determine the terminal cell character based on edge kind.
            if !is_subgraph_title_cell(graph, last_seg.to.x, last_seg.to.y) {
                let tip = if edge.kind == EdgeKind::Open {
                    // Open links: draw shaft char (no end marker)
                    match dir {
                        Dir::Left | Dir::Right => route_chars.edge_h,
                        Dir::Up | Dir::Down => route_chars.edge_v,
                    }
                } else if edge.kind == EdgeKind::CircleEnd {
                    chars.circle_end // non-directional circle marker
                } else if edge.kind == EdgeKind::CrossEnd {
                    chars.cross_end // non-directional cross marker
                } else {
                    arrow_for_dir(dir, &route_chars)
                };
                set_precomputed_route_char(canvas, last_seg.to.x, last_seg.to.y, tip, owner);
            }
        }

        // For bidirectional edges, draw a reverse arrowhead at the route start.
        if edge.kind == EdgeKind::Bidirectional {
            if let Some(first_seg) = route.segments.first() {
                if let Some(fwd) = dir_from_segment(first_seg) {
                    let rev = match fwd {
                        Dir::Up => Dir::Down,
                        Dir::Down => Dir::Up,
                        Dir::Left => Dir::Right,
                        Dir::Right => Dir::Left,
                    };
                    let rev_arrow = arrow_for_dir(rev, &route_chars);
                    set_precomputed_route_char(
                        canvas,
                        first_seg.from.x,
                        first_seg.from.y,
                        rev_arrow,
                        owner,
                    );
                }
            }
        }

        // Draw start corner AFTER segments so it overwrites the horizontal line
        if let Some((x, y, corner)) = needs_start_corner {
            set_precomputed_route_char(canvas, x, y, corner, owner);
        }
    }
}

fn set_precomputed_route_char(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    ch: char,
    owner: PrecomputedRouteOwner<'_>,
) {
    canvas.set_owned(x, y, ch, owner.kind, owner.id, PRECOMPUTED_ROUTE_Z_INDEX);
}

fn set_precomputed_route_edge_char(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    ch: char,
    chars: &StyleChars,
    owner: PrecomputedRouteOwner<'_>,
) {
    canvas.set_edge_char_owned(
        x,
        y,
        ch,
        chars,
        owner.kind,
        owner.id,
        PRECOMPUTED_ROUTE_Z_INDEX,
    );
}

fn carve_subgraph_portals_on_canvas(
    canvas: &mut Canvas,
    graph: &Graph,
    slots: &HashMap<String, PortalSlots>,
    direction: Direction,
) {
    let mut sg_ids: Vec<&str> = slots.keys().map(|id| id.as_str()).collect();
    sg_ids.sort_unstable();

    for sg_id in sg_ids {
        let Some(portals) = slots.get(sg_id) else {
            continue;
        };
        let Some(sg) = graph.get_subgraph(sg_id) else {
            continue;
        };
        let bounds = &sg.bounds;
        if !bounds.is_valid() {
            continue;
        }

        let top_y = bounds.y;
        let bottom_y = bounds.y + bounds.height.saturating_sub(1);
        let left_x = bounds.x;
        let right_x = bounds.x + bounds.width.saturating_sub(1);

        for x in sorted_slot_positions(&portals.top) {
            let px = clamp_horizontal(bounds, x);
            let top_candidates = if matches!(direction, Direction::BT) {
                vec![top_y]
            } else if sg.title.is_some() {
                vec![top_y.saturating_add(1)]
            } else {
                vec![top_y, top_y.saturating_add(1)]
            };
            carve_vertical_slot(canvas, px, &top_candidates);
        }
        for x in sorted_slot_positions(&portals.bottom) {
            let px = clamp_horizontal(bounds, x);
            carve_vertical_slot(canvas, px, &[bottom_y, bottom_y.saturating_sub(1)]);
        }
        for y in sorted_slot_positions(&portals.left) {
            let py = clamp_vertical(bounds, y);
            carve_horizontal_slot(canvas, py, &[left_x.saturating_add(1), left_x]);
        }
        for y in sorted_slot_positions(&portals.right) {
            let py = clamp_vertical(bounds, y);
            carve_horizontal_slot(canvas, py, &[right_x.saturating_sub(1), right_x]);
        }
    }
}

fn reinforce_subgraph_portals(
    canvas: &mut Canvas,
    graph: &Graph,
    slots: &HashMap<String, PortalSlots>,
    direction: Direction,
    chars: &StyleChars,
    subgraph_chars: &StyleChars,
) {
    fn is_verticalish(c: char, chars: &StyleChars, subgraph_chars: &StyleChars) -> bool {
        canvas::is_vertical(c, chars)
            || canvas::is_junction(c, chars)
            || canvas::is_junction(c, subgraph_chars)
            || canvas::is_arrow(c)
    }
    fn is_horizontalish(c: char, chars: &StyleChars, subgraph_chars: &StyleChars) -> bool {
        canvas::is_horizontal(c, chars)
            || canvas::is_junction(c, chars)
            || canvas::is_junction(c, subgraph_chars)
            || canvas::is_arrow(c)
    }

    let mut sg_ids: Vec<&str> = slots.keys().map(|id| id.as_str()).collect();
    sg_ids.sort_unstable();

    for sg_id in sg_ids {
        let Some(portals) = slots.get(sg_id) else {
            continue;
        };
        let Some(sg) = graph.get_subgraph(sg_id) else {
            continue;
        };
        let bounds = &sg.bounds;
        if !bounds.is_valid() {
            continue;
        }
        let title_span = sg.title.as_deref().map(|t| title_span(bounds, t));

        let top_y = bounds.y;
        let bottom_y = bounds.y + bounds.height.saturating_sub(1);
        let left_x = bounds.x;
        let right_x = bounds.x + bounds.width.saturating_sub(1);

        match direction {
            Direction::TD | Direction::TB => {
                let top_slots: Vec<usize> = portals.top.iter().copied().collect();
                let bottom_slots: Vec<usize> = portals.bottom.iter().copied().collect();

                for x in top_slots {
                    let px = clamp_horizontal(bounds, x);
                    let ty = top_y.saturating_add(1);
                    let above = if ty > 0 { canvas.get(px, ty - 1) } else { ' ' };
                    let below = if ty + 1 < canvas.height {
                        canvas.get(px, ty + 1)
                    } else {
                        ' '
                    };
                    let used = is_verticalish(above, chars, subgraph_chars)
                        || is_verticalish(below, chars, subgraph_chars);
                    let existing = canvas.get(px, ty);
                    if used
                        && ty < canvas.height
                        && !is_textual(existing)
                        && !canvas::is_arrow(existing)
                    {
                        canvas.set(px, ty, chars.edge_v);
                    }
                }
                for x in bottom_slots {
                    let px = clamp_horizontal(bounds, x);
                    let above = if bottom_y > 0 {
                        canvas.get(px, bottom_y - 1)
                    } else {
                        ' '
                    };
                    let below = if bottom_y + 1 < canvas.height {
                        canvas.get(px, bottom_y + 1)
                    } else {
                        ' '
                    };
                    let used = is_verticalish(above, chars, subgraph_chars)
                        || is_verticalish(below, chars, subgraph_chars);
                    if used && bottom_y < canvas.height && !is_textual(canvas.get(px, bottom_y)) {
                        // This is a portal "hole" on the border, not a T-junction.
                        // Overwrite the horizontal border character instead of merging.
                        canvas.set(px, bottom_y, chars.edge_v);
                    }
                }
            }
            Direction::BT => {
                // BT titles now live on the bottom border row. Keep portal holes on the
                // physical borders, but nudge them out of corners/title spans so routing
                // enters cleanly without punching through border text.
                let inner_min_x = left_x.saturating_add(1);
                let inner_max_x = right_x.saturating_sub(1).max(inner_min_x);
                let is_in_title_text = |x: usize| -> bool {
                    let Some((s, e)) = title_span else {
                        return false;
                    };
                    x >= s && x <= e
                };
                let nudge_from_corners = |mut x: usize| -> usize {
                    if inner_max_x <= inner_min_x {
                        return x;
                    }
                    if x == inner_min_x {
                        let candidate = inner_min_x.saturating_add(1);
                        if !is_in_title_text(candidate) && candidate <= inner_max_x {
                            x = candidate;
                        }
                    } else if x == inner_max_x {
                        let candidate = inner_max_x.saturating_sub(1);
                        if !is_in_title_text(candidate) && candidate >= inner_min_x {
                            x = candidate;
                        }
                    }
                    x
                };
                for x in sorted_slot_positions(&portals.top) {
                    let mut px = clamp_horizontal(bounds, x);
                    px = nudge_from_corners(px);
                    let existing = canvas.get(px, top_y);
                    if top_y < canvas.height && !is_textual(existing) && !canvas::is_arrow(existing)
                    {
                        let above = if top_y > 0 {
                            canvas.get(px, top_y - 1)
                        } else {
                            ' '
                        };
                        let below = if top_y + 1 < canvas.height {
                            canvas.get(px, top_y + 1)
                        } else {
                            ' '
                        };
                        let has_above = is_verticalish(above, chars, subgraph_chars);
                        let has_below = is_verticalish(below, chars, subgraph_chars);
                        let used = has_above || has_below;
                        if used {
                            // For BT top border, always use a clean vertical portal hole.
                            // Do NOT place junction characters on the border - they corrupt
                            // the visual appearance. Junctions belong inside the subgraph.
                            canvas.set(px, top_y, chars.edge_v);
                        } else {
                            canvas.set(px, top_y, subgraph_chars.h);
                        }
                    }
                }
                for x in sorted_slot_positions(&portals.bottom) {
                    let mut px = clamp_horizontal(bounds, x);
                    px = nudge_from_corners(px);
                    let existing = canvas.get(px, bottom_y);
                    if bottom_y < canvas.height
                        && !is_textual(existing)
                        && !canvas::is_arrow(existing)
                    {
                        let above = if bottom_y > 0 {
                            canvas.get(px, bottom_y - 1)
                        } else {
                            ' '
                        };
                        let below = if bottom_y + 1 < canvas.height {
                            canvas.get(px, bottom_y + 1)
                        } else {
                            ' '
                        };
                        let has_above = is_verticalish(above, chars, subgraph_chars);
                        let has_below = is_verticalish(below, chars, subgraph_chars);
                        let used = has_above || has_below;
                        if used {
                            // The BT bottom border is also the title row. Treat border pierces
                            // as clean vertical holes so junctions stay off the title row.
                            canvas.set(px, bottom_y, chars.edge_v);
                        } else {
                            canvas.set(px, bottom_y, subgraph_chars.h);
                        }
                    }
                }
            }
            Direction::LR | Direction::RL => {
                for y in sorted_slot_positions(&portals.left) {
                    let py = clamp_vertical(bounds, y);
                    let existing = canvas.get(left_x, py);
                    if left_x < canvas.width && !is_textual(existing) && !canvas::is_arrow(existing)
                    {
                        let left = if left_x > 0 {
                            canvas.get(left_x - 1, py)
                        } else {
                            ' '
                        };
                        let right = if left_x + 1 < canvas.width {
                            canvas.get(left_x + 1, py)
                        } else {
                            ' '
                        };
                        let has_left = is_horizontalish(left, chars, subgraph_chars);
                        let has_right = is_horizontalish(right, chars, subgraph_chars);
                        let glyph = if has_left || has_right {
                            chars.edge_h
                        } else {
                            subgraph_chars.v
                        };
                        canvas.set(left_x, py, glyph);
                    }
                }
                for y in sorted_slot_positions(&portals.right) {
                    let py = clamp_vertical(bounds, y);
                    let existing = canvas.get(right_x, py);
                    if right_x < canvas.width
                        && !is_textual(existing)
                        && !canvas::is_arrow(existing)
                    {
                        let left = if right_x > 0 {
                            canvas.get(right_x - 1, py)
                        } else {
                            ' '
                        };
                        let right = if right_x + 1 < canvas.width {
                            canvas.get(right_x + 1, py)
                        } else {
                            ' '
                        };
                        let has_left = is_horizontalish(left, chars, subgraph_chars);
                        let has_right = is_horizontalish(right, chars, subgraph_chars);
                        let glyph = if has_left || has_right {
                            chars.edge_h
                        } else {
                            subgraph_chars.v
                        };
                        canvas.set(right_x, py, glyph);
                    }
                }
            }
        }

        // Repair any carved portal holes that ended up unused (e.g. nested subgraphs where
        // edges don't actually cross the outer border). Only applies to the bottom border
        // since titles live on the top border.
        if bottom_y < canvas.height && right_x > left_x.saturating_add(2) {
            let mut fill: Option<char> = None;
            for x in (left_x + 1)..right_x {
                let ch = canvas.get(x, bottom_y);
                if ch != ' '
                    && !canvas::is_vertical(ch, chars)
                    && !canvas::is_junction(ch, chars)
                    && !canvas::is_arrow(ch)
                    && !is_textual(ch)
                {
                    fill = Some(ch);
                    break;
                }
            }
            if let Some(fill_ch) = fill {
                for x in (left_x + 1)..right_x {
                    let ch = canvas.get(x, bottom_y);
                    if ch == ' ' {
                        canvas.set(x, bottom_y, fill_ch);
                    }
                }

                // Also undo any portal reinforcement that picked a slot no edge actually uses.
                if matches!(direction, Direction::TD | Direction::TB) {
                    for x in sorted_slot_positions(&portals.bottom) {
                        let px = clamp_horizontal(bounds, x);
                        let above = if bottom_y > 0 {
                            canvas.get(px, bottom_y - 1)
                        } else {
                            ' '
                        };
                        let below = if bottom_y + 1 < canvas.height {
                            canvas.get(px, bottom_y + 1)
                        } else {
                            ' '
                        };
                        let used = is_verticalish(above, chars, subgraph_chars)
                            || is_verticalish(below, chars, subgraph_chars);
                        if !used && canvas.get(px, bottom_y) == chars.edge_v {
                            canvas.set(px, bottom_y, fill_ch);
                        }
                    }
                }
            }
        }
    }
}

fn sorted_slot_positions(slots: &HashSet<usize>) -> Vec<usize> {
    let mut ordered: Vec<usize> = slots.iter().copied().collect();
    ordered.sort_unstable();
    ordered
}

fn clamp_horizontal(bounds: &crate::graph::Rectangle, x: usize) -> usize {
    let min = bounds.x.saturating_add(1);
    let max = bounds.x.saturating_add(bounds.width.saturating_sub(2));
    if max < min {
        min
    } else {
        x.clamp(min, max)
    }
}

fn clamp_vertical(bounds: &crate::graph::Rectangle, y: usize) -> usize {
    let min = bounds.y.saturating_add(1);
    let max = bounds.y.saturating_add(bounds.height.saturating_sub(2));
    if max < min {
        min
    } else {
        y.clamp(min, max)
    }
}

fn carve_vertical_slot(canvas: &mut Canvas, x: usize, candidates: &[usize]) {
    for &y in candidates {
        if x < canvas.width && y < canvas.height {
            let existing = canvas.get(x, y);
            if !is_textual(existing) {
                canvas.set(x, y, ' ');
                return;
            }
        }
    }
}

fn carve_horizontal_slot(canvas: &mut Canvas, y: usize, candidates: &[usize]) {
    for &x in candidates {
        if x < canvas.width && y < canvas.height {
            let existing = canvas.get(x, y);
            if !is_textual(existing) {
                canvas.set(x, y, ' ');
                return;
            }
        }
    }
}

pub(super) fn is_textual(c: char) -> bool {
    c.is_alphanumeric() || c == '[' || c == ']'
}

pub(super) fn subgraph_title_y(bounds: &crate::graph::Rectangle, direction: Direction) -> usize {
    if matches!(direction, Direction::BT) {
        bounds.y + bounds.height.saturating_sub(1)
    } else {
        bounds.y
    }
}

fn title_span(bounds: &crate::graph::Rectangle, title: &str) -> (usize, usize) {
    let title_fmt = format!("[  {}  ]", title);
    let len = title_fmt.chars().count();
    let start = bounds.x + bounds.width.saturating_sub(len) / 2;
    let end = start + len.saturating_sub(1);
    (start, end)
}

fn draw_subgraph_title(
    canvas: &mut Canvas,
    rect: &crate::graph::Rectangle,
    title: Option<&str>,
    direction: Direction,
) {
    let Some(t) = title else {
        return;
    };
    if !rect.is_valid() {
        return;
    }
    let title_fmt = format!("[  {}  ]", t);
    let title_len = title_fmt.chars().count();
    if title_len > rect.width.saturating_sub(2) {
        return;
    }
    let start_x = rect.x + (rect.width - title_len) / 2;
    let title_y = subgraph_title_y(rect, direction);
    if title_y >= canvas.height {
        return;
    }
    for (i, c) in title_fmt.chars().enumerate() {
        if start_x + i < canvas.width {
            canvas.set(start_x + i, title_y, c);
        }
    }
}

fn cleanup_bt_title_rows(canvas: &mut Canvas, graph: &Graph, chars: &crate::style::StyleChars) {
    for subgraph in &graph.subgraphs {
        let Some(title) = subgraph.title.as_deref() else {
            continue;
        };
        if !subgraph.bounds.is_valid() || subgraph.bounds.height <= 2 {
            continue;
        }

        let title_y = subgraph_title_y(&subgraph.bounds, Direction::BT);
        let bottom_y = subgraph.bounds.y + subgraph.bounds.height.saturating_sub(1);
        if title_y >= canvas.height {
            continue;
        }
        let (title_start, title_end) = title_span(&subgraph.bounds, title);
        let inner_left = subgraph.bounds.x.saturating_add(1);
        let inner_right = subgraph.bounds.x + subgraph.bounds.width.saturating_sub(2);

        for x in inner_left..=inner_right {
            if x >= title_start && x <= title_end {
                continue;
            }

            let current = canvas.get(x, title_y);
            if current == ' ' || is_textual(current) {
                continue;
            }

            let has_vertical_above =
                title_y > 0 && topology::char_connects_down(canvas.get(x, title_y - 1));
            let has_vertical_below = title_y + 1 < canvas.height
                && topology::char_connects_up(canvas.get(x, title_y + 1));

            if title_y == bottom_y {
                if has_vertical_above || has_vertical_below {
                    canvas.set(x, title_y, chars.edge_v);
                } else {
                    canvas.set(x, title_y, chars.edge_h);
                }
            } else if has_vertical_above && has_vertical_below {
                canvas.set(x, title_y, chars.edge_v);
            } else {
                canvas.set(x, title_y, ' ');
            }
        }
    }
}

// ============================================================================
// Edge Label Drawing
// ============================================================================

use crate::style::StyleChars;

/// Draw an edge label on the appropriate segment between two nodes.
/// For TD/BT: labels go on vertical segments
/// For LR/RL: labels go on horizontal segments
#[allow(clippy::too_many_arguments)]
fn draw_edge_label(
    canvas: &mut Canvas,
    from: &Node,
    to: &Node,
    label: &str,
    direction: Direction,
    style: &StyleChars,
    config: &Config,
    edge_idx: usize,
    edge: &crate::graph::Edge,
    graph: &Graph,
) -> Option<EdgeLabelPlacement> {
    use cycle::{center_x, center_y};

    let display_label = format_edge_label_with_limit(label, config.max_edge_label_width);
    let label_width = display_width(&display_label);
    let owner_id = edge_owner_id(edge_idx, edge);
    let mut cells = Vec::new();

    match direction {
        Direction::TD | Direction::TB => {
            // Vertical layout: place label on vertical segment
            let edge_x = center_x(to);
            let stem_start_y = from.bottom_y();
            let arrow_y = to.y.saturating_sub(1);
            let lower_bound = stem_start_y.saturating_add(1);
            let upper_bound = arrow_y.saturating_sub(1);
            let mut label_y = arrow_y.saturating_sub(1);

            if lower_bound <= upper_bound {
                label_y = label_y.max(lower_bound).min(upper_bound);

                let mut found = None;
                let mut probe_y = label_y;
                loop {
                    if !is_textual(canvas.get(edge_x, probe_y)) {
                        found = Some(probe_y);
                        break;
                    }

                    if probe_y == lower_bound {
                        break;
                    }
                    probe_y = probe_y.saturating_sub(1);
                }

                if let Some(y) = found {
                    label_y = y;
                }
            } else {
                label_y = label_y.min(arrow_y.saturating_sub(1));
            }

            // Center the label around the edge position
            let max_label_start = canvas.width.saturating_sub(label_width);
            let mut label_start_x = edge_x.saturating_sub(label_width / 2).min(max_label_start);
            if overlaps_node(&[from, to], label_start_x, label_y, label_width)
                && label_start_x + label_width + 1 < canvas.width
            {
                label_start_x += 1;
            }

            // Draw the label characters
            let mut x_pos = label_start_x;
            for c in display_label.chars() {
                if x_pos < canvas.width
                    && label_y < canvas.height
                    && !is_textual(canvas.get(x_pos, label_y))
                {
                    canvas.set(x_pos, label_y, c);
                    record_label_cell(&mut cells, x_pos, label_y);
                }
                x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }
        }
        Direction::BT => {
            // Bottom-to-top: similar to TD but arrows point up
            let edge_x = center_x(to);
            let stem_start_y = from.y.saturating_sub(1);
            let arrow_y = to.bottom_y();
            let lower_bound = arrow_y.saturating_add(1);
            let upper_bound = stem_start_y.saturating_sub(1);
            let mut label_y = lower_bound;

            if lower_bound <= upper_bound {
                let mut found = None;
                let mut probe_y = label_y;
                while probe_y <= upper_bound && probe_y < canvas.height {
                    if !is_textual(canvas.get(edge_x, probe_y)) {
                        found = Some(probe_y);
                        break;
                    }
                    probe_y += 1;
                }
                if let Some(y) = found {
                    label_y = y;
                }
            } else {
                label_y = lower_bound.min(stem_start_y);
            }

            let label_start_x =
                pick_bt_vertical_label_start(canvas, &[from, to], edge_x, label_y, label_width);
            let mut x_pos = label_start_x;
            for c in display_label.chars() {
                if x_pos < canvas.width
                    && label_y < canvas.height
                    && !is_textual(canvas.get(x_pos, label_y))
                {
                    canvas.set(x_pos, label_y, c);
                    record_label_cell(&mut cells, x_pos, label_y);
                }
                x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }
        }
        Direction::LR => {
            let edge_y = center_y(to);
            let stem_start_x = from.x + from.width;
            let arrow_x = to.x.saturating_sub(1);
            let span_width = arrow_x.saturating_sub(stem_start_x);
            let outside_row =
                pick_outside_horizontal_label_row(edge_y, canvas.height, &[from, to], graph);
            let can_fit_full_inline = label_width + 3 <= span_width;

            if can_fit_full_inline {
                let label_start_x = stem_start_x + (span_width - (label_width + 3)) / 2;

                for x in stem_start_x..label_start_x {
                    canvas.set(x, edge_y, style.edge_h);
                }

                canvas.set(label_start_x, edge_y, ' ');

                let mut x_pos = label_start_x + 1;
                for c in display_label.chars() {
                    if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                        canvas.set(x_pos, edge_y, c);
                        record_label_cell(&mut cells, x_pos, edge_y);
                    }
                    x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                }

                if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                    canvas.set(x_pos, edge_y, ' ');
                }
                x_pos += 1;

                for x in x_pos..arrow_x {
                    if x < canvas.width {
                        canvas.set(x, edge_y, style.edge_h);
                    }
                }
            } else if let Some(label_row) = outside_row {
                let label_x = stem_start_x + span_width / 2;
                let max_label_start = canvas.width.saturating_sub(label_width);
                let mut label_start_x =
                    label_x.saturating_sub(label_width / 2).min(max_label_start);
                label_start_x = adjust_horizontal_label_slot(
                    label_start_x,
                    0,
                    canvas.width,
                    label_row,
                    label_width,
                    &[from, to],
                    graph,
                );

                let mut x_pos = label_start_x;
                for c in display_label.chars() {
                    if x_pos < canvas.width && label_row < canvas.height {
                        canvas.set(x_pos, label_row, c);
                        record_label_cell(&mut cells, x_pos, label_row);
                    }
                    x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                }
            } else {
                let inline_limit = config
                    .max_edge_label_width
                    .min(span_width.saturating_sub(3).max(1));
                let inline_label = format_edge_label_with_limit(label, inline_limit);
                let inline_width = display_width(&inline_label);
                let label_start_x =
                    stem_start_x + (span_width.saturating_sub(inline_width + 3)) / 2;

                for x in stem_start_x..label_start_x {
                    canvas.set(x, edge_y, style.edge_h);
                }

                canvas.set(label_start_x, edge_y, ' ');

                let mut x_pos = label_start_x + 1;
                for c in inline_label.chars() {
                    if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                        canvas.set(x_pos, edge_y, c);
                        record_label_cell(&mut cells, x_pos, edge_y);
                    }
                    x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                }

                if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                    canvas.set(x_pos, edge_y, ' ');
                }
                x_pos += 1;

                for x in x_pos..arrow_x {
                    if x < canvas.width {
                        canvas.set(x, edge_y, style.edge_h);
                    }
                }
            }
        }
        Direction::RL => {
            let edge_y = center_y(to);
            let arrow_x = to.x + to.width; // Arrow is after target box
            let stem_end_x = from.x; // Edge ends at left side of source box
            let gap_start_x = arrow_x.saturating_add(1);
            let span_width = stem_end_x.saturating_sub(gap_start_x);
            let outside_row =
                pick_outside_horizontal_label_row(edge_y, canvas.height, &[from, to], graph);
            let can_fit_full_inline = label_width + 4 <= span_width;

            if can_fit_full_inline {
                let label_start_x = gap_start_x + 1 + (span_width - (label_width + 4)) / 2;

                for x in gap_start_x..label_start_x {
                    if x < canvas.width {
                        canvas.set(x, edge_y, style.edge_h);
                    }
                }

                if label_start_x < canvas.width {
                    canvas.set(label_start_x, edge_y, ' ');
                }

                let mut x_pos = label_start_x + 1;
                for c in display_label.chars() {
                    if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                        canvas.set(x_pos, edge_y, c);
                        record_label_cell(&mut cells, x_pos, edge_y);
                    }
                    x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                }

                if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                    canvas.set(x_pos, edge_y, ' ');
                }
                x_pos += 1;

                for x in x_pos..stem_end_x {
                    if x < canvas.width {
                        canvas.set(x, edge_y, style.edge_h);
                    }
                }
            } else if let Some(label_row) = outside_row {
                let label_x = gap_start_x + span_width / 2;
                let max_label_start = canvas.width.saturating_sub(label_width);
                let mut label_start_x =
                    label_x.saturating_sub(label_width / 2).min(max_label_start);
                label_start_x = adjust_horizontal_label_slot(
                    label_start_x,
                    0,
                    canvas.width,
                    label_row,
                    label_width,
                    &[from, to],
                    graph,
                );

                let mut x_pos = label_start_x;
                for c in display_label.chars() {
                    if x_pos < canvas.width && label_row < canvas.height {
                        canvas.set(x_pos, label_row, c);
                        record_label_cell(&mut cells, x_pos, label_row);
                    }
                    x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                }
            } else {
                let inline_limit = config
                    .max_edge_label_width
                    .min(span_width.saturating_sub(4).max(1));
                let inline_label = format_edge_label_with_limit(label, inline_limit);
                let inline_width = display_width(&inline_label);
                let label_start_x =
                    gap_start_x + 1 + (span_width.saturating_sub(inline_width + 4)) / 2;

                for x in gap_start_x..label_start_x {
                    if x < canvas.width {
                        canvas.set(x, edge_y, style.edge_h);
                    }
                }

                if label_start_x < canvas.width {
                    canvas.set(label_start_x, edge_y, ' ');
                }

                let mut x_pos = label_start_x + 1;
                for c in inline_label.chars() {
                    if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                        canvas.set(x_pos, edge_y, c);
                        record_label_cell(&mut cells, x_pos, edge_y);
                    }
                    x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                }

                if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                    canvas.set(x_pos, edge_y, ' ');
                }
                x_pos += 1;

                for x in x_pos..stem_end_x {
                    if x < canvas.width {
                        canvas.set(x, edge_y, style.edge_h);
                    }
                }
            }
        }
    }

    build_label_placement(owner_id, cells)
}

/// Draw an edge label using a precomputed Manhattan route. Picks the longest
/// segment (preferring horizontal) and centers the label along it.
#[allow(clippy::too_many_arguments)]
fn draw_routed_edge_label(
    canvas: &mut Canvas,
    route: &EdgeRoute,
    label: &str,
    style: &StyleChars,
    graph: &Graph,
    config: &Config,
    edge_idx: usize,
    edge: &crate::graph::Edge,
) -> Option<EdgeLabelPlacement> {
    if route.segments.is_empty() {
        return None;
    }

    let display_label = format_edge_label_with_limit(label, config.max_edge_label_width);
    let label_width = display_width(&display_label);
    let owner_id = edge_owner_id(edge_idx, edge);
    let mut cells = Vec::new();

    let nodes: Vec<&Node> = graph.nodes.iter().collect();
    let border_spans: Vec<crate::graph::Rectangle> = graph
        .subgraphs
        .iter()
        .map(|sg| sg.bounds.clone())
        .filter(|b| b.is_valid())
        .collect();

    // Choose longest segment, prefer horizontal for readability, avoid subgraph borders when possible.
    let mut best: Option<(&Segment, usize, bool)> = None; // (segment, length, is_horizontal)
    for seg in &route.segments {
        let is_horizontal = seg.from.y == seg.to.y;
        let length = if is_horizontal {
            seg.from.x.abs_diff(seg.to.x)
        } else {
            seg.from.y.abs_diff(seg.to.y)
        };
        let on_border = border_spans.iter().any(|b| segment_on_border(seg, b));

        match best {
            None => best = Some((seg, length, is_horizontal)),
            Some((prev_seg, best_len, best_horizontal)) => {
                let prev_on_border = border_spans.iter().any(|b| segment_on_border(prev_seg, b));
                let prefer_current = match (prev_on_border, on_border) {
                    (true, false) => true,
                    (false, true) => false,
                    _ => {
                        (is_horizontal && !best_horizontal)
                            || (is_horizontal == best_horizontal && length > best_len)
                    }
                };
                if prefer_current {
                    best = Some((seg, length, is_horizontal));
                }
            }
        }
    }

    let (seg, _, is_horizontal) = best?;

    if is_horizontal {
        let mut y = seg.from.y;
        if border_spans
            .iter()
            .any(|b| y == b.y || y == b.y + b.height.saturating_sub(1))
        {
            if y + 1 < canvas.height {
                y += 1;
            } else if y > 0 {
                y = y.saturating_sub(1);
            }
        }
        let (min_x, max_x) = if seg.from.x <= seg.to.x {
            (seg.from.x, seg.to.x)
        } else {
            (seg.to.x, seg.from.x)
        };
        let gap_start_x = min_x;
        let gap_end_x = max_x.saturating_add(1);
        let gap_width = gap_end_x.saturating_sub(gap_start_x);
        let mid_x = gap_start_x + gap_width / 2;
        let centered_start_x = mid_x.saturating_sub(label_width / 2);
        let outside_row = pick_outside_horizontal_label_row(y, canvas.height, &nodes, graph);
        let reserve_leading_shaft = graph.direction == Direction::RL;
        let inline_margin = if reserve_leading_shaft { 4 } else { 3 };
        let inline_collides = overlaps_node(&nodes, centered_start_x, y, label_width);
        let can_fit_full_inline = !inline_collides && label_width + inline_margin <= gap_width;

        if can_fit_full_inline {
            let start_x = gap_start_x
                + usize::from(reserve_leading_shaft)
                + (gap_width - (label_width + inline_margin)) / 2;
            for x in gap_start_x..start_x {
                if y < canvas.height && x < canvas.width {
                    canvas.set(x, y, style.edge_h);
                }
            }

            if start_x < canvas.width && y < canvas.height {
                canvas.set(start_x, y, ' ');
            }

            let mut x_pos = start_x + 1;
            for c in display_label.chars() {
                if y < canvas.height && x_pos < canvas.width {
                    canvas.set(x_pos, y, c);
                    record_label_cell(&mut cells, x_pos, y);
                }
                x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }

            if x_pos < canvas.width && y < canvas.height {
                canvas.set(x_pos, y, ' ');
            }
            x_pos += 1;

            for x in x_pos..gap_end_x {
                if y < canvas.height && x < canvas.width {
                    canvas.set(x, y, style.edge_h);
                }
            }
        } else if let Some(label_row) = outside_row {
            let max_label_start = canvas.width.saturating_sub(label_width);
            let mut start_x = centered_start_x.min(max_label_start);
            start_x = adjust_horizontal_label_slot(
                start_x,
                0,
                canvas.width,
                label_row,
                label_width,
                &nodes,
                graph,
            );

            let mut x_pos = start_x;
            for c in display_label.chars() {
                if label_row < canvas.height && x_pos < canvas.width {
                    canvas.set(x_pos, label_row, c);
                    record_label_cell(&mut cells, x_pos, label_row);
                }
                x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }
        } else {
            let inline_limit = config
                .max_edge_label_width
                .min(gap_width.saturating_sub(inline_margin).max(1));
            let inline_label = format_edge_label_with_limit(label, inline_limit);
            let inline_width = display_width(&inline_label);
            let start_x = gap_start_x
                + usize::from(reserve_leading_shaft)
                + (gap_width.saturating_sub(inline_width + inline_margin)) / 2;

            for x in gap_start_x..start_x {
                if y < canvas.height && x < canvas.width {
                    canvas.set(x, y, style.edge_h);
                }
            }

            if start_x < canvas.width && y < canvas.height {
                canvas.set(start_x, y, ' ');
            }

            let mut x_pos = start_x + 1;
            for c in inline_label.chars() {
                if y < canvas.height && x_pos < canvas.width {
                    canvas.set(x_pos, y, c);
                    record_label_cell(&mut cells, x_pos, y);
                }
                x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }

            if x_pos < canvas.width && y < canvas.height {
                canvas.set(x_pos, y, ' ');
            }
            x_pos += 1;

            for x in x_pos..gap_end_x {
                if y < canvas.height && x < canvas.width {
                    canvas.set(x, y, style.edge_h);
                }
            }
        }
    } else {
        let x = seg.from.x;
        let (min_y, max_y) = if seg.from.y <= seg.to.y {
            (seg.from.y, seg.to.y)
        } else {
            (seg.to.y, seg.from.y)
        };
        let mut mid_y = if canvas::is_arrow(canvas.get(x, min_y)) && min_y < max_y {
            min_y + 1
        } else if canvas::is_arrow(canvas.get(x, max_y)) && max_y > min_y {
            max_y.saturating_sub(1)
        } else {
            min_y + (max_y.saturating_sub(min_y)) / 2
        };
        if border_spans
            .iter()
            .any(|b| mid_y == b.y || mid_y == b.y + b.height.saturating_sub(1))
        {
            if canvas::is_arrow(canvas.get(x, min_y)) && mid_y < max_y && mid_y + 1 < canvas.height
            {
                mid_y += 1;
            } else if mid_y > min_y {
                mid_y = mid_y.saturating_sub(1);
            }
        }
        let mut start_x = x.saturating_sub(label_width / 2);
        if start_x + label_width > canvas.width {
            start_x = canvas.width.saturating_sub(label_width);
        }

        // Avoid drawing over node interiors if possible.
        let mut x_pos = start_x;
        for c in display_label.chars() {
            if mid_y < canvas.height && x_pos < canvas.width {
                canvas.set(x_pos, mid_y, c);
                record_label_cell(&mut cells, x_pos, mid_y);
            }
            x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
        }
    }

    build_label_placement(owner_id, cells)
}

/// Truncate and format edge label to the specified maximum width.
fn format_edge_label_with_limit(label: &str, max_len: usize) -> String {
    if display_width(label) <= max_len {
        return label.to_string();
    }

    let mut truncated = String::new();
    let mut width = 0;
    for c in label.chars() {
        let char_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
        if width + char_width > max_len - 1 {
            truncated.push('…');
            break;
        }
        truncated.push(c);
        width += char_width;
    }
    truncated
}

fn segment_on_border(seg: &Segment, bounds: &crate::graph::Rectangle) -> bool {
    if !bounds.is_valid() {
        return false;
    }
    // Horizontal along top/bottom
    if seg.from.y == seg.to.y {
        let y = seg.from.y;
        if y == bounds.y || y == bounds.y + bounds.height.saturating_sub(1) {
            let (min_x, max_x) = if seg.from.x <= seg.to.x {
                (seg.from.x, seg.to.x)
            } else {
                (seg.to.x, seg.from.x)
            };
            let span_left = bounds.x;
            let span_right = bounds.x + bounds.width.saturating_sub(1);
            return max_x >= span_left && min_x <= span_right;
        }
    } else if seg.from.x == seg.to.x {
        let x = seg.from.x;
        if x == bounds.x || x == bounds.x + bounds.width.saturating_sub(1) {
            let (min_y, max_y) = if seg.from.y <= seg.to.y {
                (seg.from.y, seg.to.y)
            } else {
                (seg.to.y, seg.from.y)
            };
            let span_top = bounds.y;
            let span_bottom = bounds.y + bounds.height.saturating_sub(1);
            return max_y >= span_top && min_y <= span_bottom;
        }
    }
    false
}

fn overlaps_node(nodes: &[&Node], x: usize, y: usize, width: usize) -> bool {
    for n in nodes {
        if y >= n.y && y < n.bottom_y() {
            let nx0 = n.x;
            let nx1 = n.x + n.width;
            if x < nx1 && x + width > nx0 {
                return true;
            }
        }
    }
    false
}

fn pick_bt_vertical_label_start(
    canvas: &Canvas,
    nodes: &[&Node],
    edge_x: usize,
    y: usize,
    width: usize,
) -> usize {
    if width == 0 {
        return edge_x;
    }

    let max_start = canvas.width.saturating_sub(width);
    let centered = edge_x.saturating_sub(width / 2).min(max_start);
    let centered_covers_edge = centered <= edge_x && edge_x < centered.saturating_add(width);

    if (!centered_covers_edge || canvas.get(edge_x, y) == ' ')
        && !overlaps_node(nodes, centered, y, width)
    {
        return centered;
    }

    let candidates = [
        edge_x
            .saturating_sub(width.saturating_add(1))
            .min(max_start),
        edge_x.saturating_add(2).min(max_start),
        centered,
    ];
    let mut best = centered;
    let mut best_score = usize::MAX;

    for start in candidates {
        if start + width > canvas.width {
            continue;
        }

        let covers_edge = start <= edge_x && edge_x < start.saturating_add(width);
        let overlaps = overlaps_node(nodes, start, y, width);
        let occupied = (start..start + width)
            .filter(|x| canvas.get(*x, y) != ' ')
            .count();
        let distance = start.abs_diff(centered);
        let score = usize::from(covers_edge) * 1000
            + usize::from(overlaps) * 100
            + occupied * 10
            + distance;

        if score < best_score {
            best_score = score;
            best = start;
        }
    }

    best
}

fn adjust_horizontal_label_slot(
    start_x: usize,
    min_x: usize,
    max_x: usize,
    y: usize,
    width: usize,
    nodes: &[&Node],
    graph: &Graph,
) -> usize {
    let candidate = start_x;
    if !overlaps_node(nodes, candidate, y, width)
        && !overlaps_reserved_subgraph_cells(graph, candidate, y, width)
    {
        return candidate;
    }

    // Try small shifts within segment bounds.
    for delta in 1..=4 {
        if candidate >= delta
            && !overlaps_node(nodes, candidate - delta, y, width)
            && !overlaps_reserved_subgraph_cells(graph, candidate - delta, y, width)
            && candidate - delta >= min_x
        {
            return candidate - delta;
        }
        if candidate + width + delta <= max_x
            && !overlaps_node(nodes, candidate + delta, y, width)
            && !overlaps_reserved_subgraph_cells(graph, candidate + delta, y, width)
        {
            return candidate + delta;
        }
    }
    candidate
}

fn pick_outside_horizontal_label_row(
    edge_y: usize,
    canvas_height: usize,
    nodes: &[&Node],
    graph: &Graph,
) -> Option<usize> {
    let mut candidates = Vec::new();

    for delta in [2usize, 3usize] {
        if let Some(row) = edge_y.checked_sub(delta) {
            candidates.push(row);
        }
        let row = edge_y.saturating_add(delta);
        if row < canvas_height {
            candidates.push(row);
        }
    }

    candidates.into_iter().find(|row| {
        let intersects_node = nodes
            .iter()
            .any(|node| *row >= node.y && *row < node.bottom_y());
        !intersects_node && !is_reserved_subgraph_label_row(graph, *row)
    })
}

fn is_reserved_subgraph_label_row(graph: &Graph, y: usize) -> bool {
    graph.subgraphs.iter().any(|sg| {
        if !sg.bounds.is_valid() {
            return false;
        }

        let bottom_y = sg.bounds.y + sg.bounds.height.saturating_sub(1);
        if y == sg.bounds.y || y == bottom_y {
            return true;
        }

        sg.title.is_some() && y == subgraph_title_y(&sg.bounds, graph.direction)
    })
}

fn overlaps_reserved_subgraph_cells(graph: &Graph, start_x: usize, y: usize, width: usize) -> bool {
    let end_x = start_x.saturating_add(width);

    graph.subgraphs.iter().any(|sg| {
        if !sg.bounds.is_valid() {
            return false;
        }

        let left = sg.bounds.x;
        let right = sg.bounds.x + sg.bounds.width.saturating_sub(1);
        let top = sg.bounds.y;
        let bottom = sg.bounds.y + sg.bounds.height.saturating_sub(1);

        (start_x..end_x).any(|x| {
            let on_horizontal_border = (y == top || y == bottom) && x >= left && x <= right;
            let on_vertical_border = (x == left || x == right) && y >= top && y <= bottom;
            let on_vertical_border_gutter = y >= top
                && y <= bottom
                && (x == left.saturating_sub(1) || x == right.saturating_add(1));
            on_horizontal_border
                || on_vertical_border
                || on_vertical_border_gutter
                || is_subgraph_title_cell(graph, x, y)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn char_at(output: &str, x: usize, y: usize) -> Option<char> {
        output.lines().nth(y).and_then(|line| line.chars().nth(x))
    }

    #[test]
    fn cross_subgraph_edge_pierces_border_td() {
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
        // With titled subgraphs we intentionally avoid drawing on the title row;
        // the "pierce" should appear just inside the container.
        let glyph = char_at(&output, portal_x, portal_y.saturating_add(1)).unwrap_or(' ');
        let is_pierced = matches!(glyph, '│' | '┬' | '┴' | '┼');
        assert!(
            is_pierced,
            "expected vertical pierce just inside at ({portal_x},{}), got '{glyph}'\n{output}",
            portal_y.saturating_add(1)
        );
    }

    #[test]
    fn cross_subgraph_edge_pierces_border_lr() {
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
        let portal_y = graph
            .get_subgraph("sg")
            .map(|sg| sg.bounds.y + 1)
            .unwrap_or(0);
        let portal_x = graph.get_subgraph("sg").map(|sg| sg.bounds.x).unwrap_or(0);
        let glyph = char_at(&output, portal_x, portal_y).unwrap_or(' ');
        let is_pierced = !glyph.is_alphabetic();
        assert!(
            is_pierced,
            "expected horizontal pierce at ({portal_x},{portal_y}), got '{glyph}'\n{output}"
        );
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

/// Draw an edge label for convergent edges (multiple sources to one target).
/// Labels are placed on the branch's outer side before the merge point so they
/// do not crowd the shared junction corridor.
#[allow(clippy::too_many_arguments)]
fn draw_convergent_edge_label(
    canvas: &mut Canvas,
    from: &Node,
    to: &Node,
    label: &str,
    direction: Direction,
    config: &Config,
    edge_idx: usize,
    edge: &crate::graph::Edge,
) -> Option<EdgeLabelPlacement> {
    use cycle::{center_x, center_y};

    // Use slightly shorter limit for convergent labels to avoid crowding at merge points
    let convergent_limit = config.max_edge_label_width.saturating_sub(2).max(8);
    let display_label = format_edge_label_with_limit(label, convergent_limit);
    let label_width = display_width(&display_label);
    let owner_id = edge_owner_id(edge_idx, edge);
    let mut cells = Vec::new();

    match direction {
        Direction::TD | Direction::TB => {
            // Place label on vertical line from source, before merge point
            let src_x = center_x(from);
            let target_x = center_x(to);
            let stem_start_y = from.bottom_y();
            // Place label just below the source box on the vertical stem
            let label_y = stem_start_y + 1;

            // Move the label away from the shared merge corridor when the source
            // approaches the target from the left or right.
            let label_start_x = if src_x + 1 < target_x {
                src_x.saturating_sub(label_width)
            } else if src_x > target_x + 1 {
                src_x.saturating_add(2)
            } else {
                src_x.saturating_sub(label_width / 2)
            };

            let mut x_pos = label_start_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height {
                    canvas.set(x_pos, label_y, c);
                    record_label_cell(&mut cells, x_pos, label_y);
                }
                x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }
        }
        Direction::BT => {
            let src_x = center_x(from);
            let stem_start_y = from.y.saturating_sub(1);
            let label_y = stem_start_y.saturating_sub(1);

            let label_start_x = src_x.saturating_sub(label_width / 2);
            let mut x_pos = label_start_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height {
                    canvas.set(x_pos, label_y, c);
                    record_label_cell(&mut cells, x_pos, label_y);
                }
                x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }
        }
        Direction::LR => {
            // Place label on horizontal line from source, before merge
            let src_y = center_y(from);
            let stem_start_x = from.x + from.width;
            let label_x = stem_start_x + 1;
            // Place label above the edge line
            let label_y = src_y.saturating_sub(1);

            let mut x_pos = label_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height {
                    canvas.set(x_pos, label_y, c);
                    record_label_cell(&mut cells, x_pos, label_y);
                }
                x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }
        }
        Direction::RL => {
            let src_y = center_y(from);
            let stem_start_x = from.x.saturating_sub(1);
            let label_x = stem_start_x.saturating_sub(label_width);
            let label_y = src_y.saturating_sub(1);

            let mut x_pos = label_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height {
                    canvas.set(x_pos, label_y, c);
                    record_label_cell(&mut cells, x_pos, label_y);
                }
                x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }
        }
    }

    build_label_placement(owner_id, cells)
}

fn record_label_cell(cells: &mut Vec<(usize, usize)>, x: usize, y: usize) {
    cells.push((x, y));
}

fn build_label_placement(
    owner_id: String,
    cells: Vec<(usize, usize)>,
) -> Option<EdgeLabelPlacement> {
    if cells.is_empty() {
        None
    } else {
        Some(EdgeLabelPlacement { owner_id, cells })
    }
}
