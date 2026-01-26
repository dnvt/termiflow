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
pub mod cycle;
pub mod edge;
pub mod shapes;

// Re-exports
pub use canvas::Canvas;

use anyhow::Result;

use crate::config::Config;
use crate::geom::{EdgeRoute, Segment};
use crate::graph::{Graph, Node, NodeShape};
use crate::portals::{collect_portal_slots, node_rects_from_graph, PortalSlots};
use crate::style::{
    display_width, truncate_label, BaseStyle, BOX_HEIGHT, COL_SPACING, CYCLE_GUTTER,
    MAX_CANVAS_HEIGHT, MAX_CANVAS_WIDTH, ROW_SPACING, STEM_LENGTH_VERTICAL,
};

use crate::graph::Direction;
use cycle::route_cycle_edge;
use edge::{route_convergent_edges, route_divergent_edges};
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
    if graph.nodes.is_empty() {
        return Ok(String::new());
    }

    // Calculate canvas size from laid-out nodes and subgraphs
    let nodes_right = graph.nodes.iter().map(|n| n.x + n.width).max().unwrap_or(0);
    let nodes_bottom = graph
        .nodes
        .iter()
        .map(|n| n.bottom_y())
        .max()
        .unwrap_or(0);

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
    let width_gutter = if graph.has_cycles() && !is_horizontal {
        CYCLE_GUTTER
    } else {
        0
    };
    let height_gutter = if graph.has_cycles() && is_horizontal {
        CYCLE_GUTTER
    } else {
        0
    };

    let mut width = max_right + COL_SPACING + width_gutter;
    if width > MAX_CANVAS_WIDTH {
        width = MAX_CANVAS_WIDTH;
        eprintln!(
            "termiflow: warning: Graph too wide ({} chars), clipping to {}",
            max_right + COL_SPACING + width_gutter,
            MAX_CANVAS_WIDTH
        );
    }
    width = width
        .max(max_right.saturating_add(1).min(MAX_CANVAS_WIDTH))
        .max(1);

    let mut height = max_bottom + ROW_SPACING + height_gutter;
    if height > MAX_CANVAS_HEIGHT {
        height = MAX_CANVAS_HEIGHT;
        eprintln!(
            "termiflow: warning: Graph too tall ({} rows), clipping to {}",
            max_bottom + ROW_SPACING + height_gutter,
            MAX_CANVAS_HEIGHT
        );
    }
    height = height
        .max(max_bottom.saturating_add(1).min(MAX_CANVAS_HEIGHT))
        .max(1);

    if graph.has_cycles() && !is_horizontal && width <= CYCLE_GUTTER {
        eprintln!("termiflow: warning: Back-edges skipped (gutter clipped)");
    }
    if graph.has_cycles() && is_horizontal && height <= CYCLE_GUTTER {
        eprintln!("termiflow: warning: Back-edges skipped (gutter clipped)");
    }

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
        );
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
        carve_subgraph_portals_on_canvas(&mut canvas, graph, &portal_slots);
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
    let mut cycle_edges: Vec<(&Node, &Node)> = Vec::new();
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
            cycle_edges.push((from, to));
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

    // First, handle convergence cases (multiple sources → one target)
    for (target_id, sources) in &edges_by_target {
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
                    graph.direction,
                    graph,
                );
            }
        }
    }

    // Draw back-edges (cycle edges) that were not pre-routed.
    for (from, to) in cycle_edges {
        route_cycle_edge(from, to, &mut canvas, &chars, graph.direction);
    }

    // Draw edge labels (route-aware for precomputed paths, heuristic for fallback paths)
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
            draw_routed_edge_label(&mut canvas, route, label, &chars, graph);
            continue;
        }

        // Fall back to heuristic placement for edges without precomputed routes
        let is_convergent = convergent_targets.contains(to.id.as_str());
        if is_convergent {
            draw_convergent_edge_label(&mut canvas, from, to, label, graph.direction);
        } else {
            draw_edge_label(&mut canvas, from, to, label, graph.direction, &chars);
        }
    }

    reinforce_subgraph_portals(&mut canvas, graph, &portal_slots, graph.direction, &chars);

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

        if junction_x < canvas.width && junction_y < canvas.height {
            canvas.set_edge_char(junction_x, junction_y, junction_char, &chars);
        }
    }

    // Debug: print canvas content for convergent edge A7/A8 -> P4
    if std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok() {
        eprintln!("  Input 7/8 -> Process 4 area (y=2-6, x=100-130):");
        for y in 2..=6 {
            let row: String = (100..=130)
                .map(|x| canvas.get(x, y))
                .collect();
            eprintln!("  y={}: [{}]", y, row);
        }
        // Mark positions: A7 center=108, A8 center=125, P4 center=101
        let markers: String = (100..=130)
            .map(|x| if x == 108 || x == 125 || x == 101 { '^' } else { ' ' })
            .collect();
        eprintln!("  pos: [{}] (^=101,108,125)", markers);
    }

    let output = if config.crop {
        canvas.to_string_cropped(config.pad)
    } else {
        pad_string(&canvas.to_string(), config.pad)
    };

    Ok(output)
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
        (Up, Left) | (Left, Up) => Some(chars.corner_ur),     // ┘
        (Up, Right) | (Right, Up) => Some(chars.corner_ul),   // └
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
        sg.title.is_some()
            && sg.bounds.is_valid()
            && y == sg.bounds.y
            && x >= sg.bounds.x
            && x < sg.bounds.x.saturating_add(sg.bounds.width)
    })
}

fn draw_segment(
    seg: &Segment,
    dir: Dir,
    canvas: &mut Canvas,
    chars: &StyleChars,
    skip_start: bool,
    skip_end: bool,
    graph: &Graph,
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
                    canvas.set_edge_char(x, seg.from.y, chars.edge_h, chars);
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
                    canvas.set_edge_char(seg.from.x, y, chars.edge_v, chars);
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
        let (Some(from), Some(to)) = (graph.get_node(&edge.from), graph.get_node(&edge.to)) else {
            continue;
        };
        if !canvas.is_visible(from) || !canvas.is_visible(to) {
            continue;
        }

        // Back-edges should render with cycle styling even when pre-routed.
        let mut route_chars = *chars;
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
                            canvas.set_edge_char(src_center_x, y, route_chars.edge_v, &route_chars);
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
                            canvas.set_edge_char(src_center_x, y, route_chars.edge_v, &route_chars);
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
                            canvas.set_edge_char(x, src_center_y, route_chars.edge_h, &route_chars);
                        }
                    }
                }
                Direction::RL => {
                    if matches!(first_dir, Some(Dir::Up) | Some(Dir::Down)) {
                        let exit_x = from.x.saturating_sub(1);
                        for x in (route_start.x + 1)..=exit_x {
                            canvas.set_edge_char(x, src_center_y, route_chars.edge_h, &route_chars);
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

            draw_segment(seg, dir, canvas, &route_chars, skip_start, skip_end, graph);

            if is_turn {
                if let Some(nd) = next_dir {
                    if let Some(corner) = corner_for_turn(dir, nd, &route_chars) {
                        if !is_subgraph_title_cell(graph, seg.to.x, seg.to.y) {
                            canvas.set_edge_char(seg.to.x, seg.to.y, corner, &route_chars);
                        }
                    }
                }
            }
        }

        if let Some(last_seg) = route.segments.last() {
            let dir = match graph.direction {
                Direction::TD | Direction::TB => Dir::Down,
                Direction::BT => Dir::Up,
                Direction::LR => Dir::Right,
                Direction::RL => Dir::Left,
            };
            let arrow = arrow_for_dir(dir, &route_chars);
            if !is_subgraph_title_cell(graph, last_seg.to.x, last_seg.to.y) {
                canvas.set(last_seg.to.x, last_seg.to.y, arrow);
            }
        }

        // Draw start corner AFTER segments so it overwrites the horizontal line
        if let Some((x, y, corner)) = needs_start_corner {
            canvas.set(x, y, corner);
        }
    }
}

fn carve_subgraph_portals_on_canvas(
    canvas: &mut Canvas,
    graph: &Graph,
    slots: &HashMap<String, PortalSlots>,
) {
    for (sg_id, portals) in slots {
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

        for &x in &portals.top {
            let px = clamp_horizontal(bounds, x);
            let top_candidates = if sg.title.is_some() {
                vec![top_y.saturating_add(1)]
            } else {
                vec![top_y, top_y.saturating_add(1)]
            };
            carve_vertical_slot(canvas, px, &top_candidates);
        }
        for &x in &portals.bottom {
            let px = clamp_horizontal(bounds, x);
            carve_vertical_slot(canvas, px, &[bottom_y, bottom_y.saturating_sub(1)]);
        }
        for &y in &portals.left {
            let py = clamp_vertical(bounds, y);
            carve_horizontal_slot(canvas, py, &[left_x.saturating_add(1), left_x]);
        }
        for &y in &portals.right {
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
) {
    fn is_verticalish(c: char, chars: &StyleChars) -> bool {
        canvas::is_vertical(c, chars) || canvas::is_junction(c, chars) || canvas::is_arrow(c)
    }

    for (sg_id, portals) in slots {
        let Some(sg) = graph.get_subgraph(sg_id) else {
            continue;
        };
        let bounds = &sg.bounds;
        if !bounds.is_valid() {
            continue;
        }
        let title_span = sg
            .title
            .as_deref()
            .map(|t| title_span(bounds, t));

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
                    let used = is_verticalish(above, chars) || is_verticalish(below, chars);
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
                    let used = is_verticalish(above, chars) || is_verticalish(below, chars);
                    if used
                        && bottom_y < canvas.height
                        && !is_textual(canvas.get(px, bottom_y))
                    {
                        // This is a portal "hole" on the border, not a T-junction.
                        // Overwrite the horizontal border character instead of merging.
                        canvas.set(px, bottom_y, chars.edge_v);
                    }
                }
            }
            Direction::BT => {
                // Title still sits on the top border in BT.
                for &x in &portals.top {
                    let mut px = clamp_horizontal(bounds, x);
                    if let Some((s, e)) = title_span {
                        px = shift_out_of_span(px, s, e, left_x, right_x);
                    }
                    let existing = canvas.get(px, top_y);
                    if top_y < canvas.height && !is_textual(existing) && !canvas::is_arrow(existing) {
                        canvas.set(px, top_y, chars.edge_v);
                    }
                }
                for &x in &portals.bottom {
                    let px = clamp_horizontal(bounds, x);
                    let existing = canvas.get(px, bottom_y);
                    if bottom_y < canvas.height
                        && !is_textual(existing)
                        && !canvas::is_arrow(existing)
                    {
                        canvas.set(px, bottom_y, chars.edge_v);
                    }
                }
            }
            Direction::LR | Direction::RL => {
                for &y in &portals.left {
                    let py = clamp_vertical(bounds, y);
                    let existing = canvas.get(left_x, py);
                    if left_x < canvas.width && !is_textual(existing) && !canvas::is_arrow(existing) {
                        canvas.set(left_x, py, chars.edge_h);
                    }
                }
                for &y in &portals.right {
                    let py = clamp_vertical(bounds, y);
                    let existing = canvas.get(right_x, py);
                    if right_x < canvas.width && !is_textual(existing) && !canvas::is_arrow(existing) {
                        canvas.set(right_x, py, chars.edge_h);
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
                    for &x in &portals.bottom {
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
                        let used = is_verticalish(above, chars) || is_verticalish(below, chars);
                        if !used && canvas.get(px, bottom_y) == chars.edge_v {
                            canvas.set(px, bottom_y, fill_ch);
                        }
                    }
                }
            }
        }
    }
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

fn title_span(bounds: &crate::graph::Rectangle, title: &str) -> (usize, usize) {
    let title_fmt = format!("[  {}  ]", title);
    let len = title_fmt.chars().count();
    let start = bounds.x + bounds.width.saturating_sub(len) / 2;
    let end = start + len.saturating_sub(1);
    (start, end)
}

fn shift_out_of_span(x: usize, span_start: usize, span_end: usize, min: usize, max: usize) -> usize {
    if x < span_start || x > span_end {
        return x;
    }
    if span_end + 1 < max {
        span_end + 1
    } else if span_start > min {
        span_start.saturating_sub(1)
    } else {
        x
    }
}

// ============================================================================
// Edge Label Drawing
// ============================================================================

use crate::style::StyleChars;

/// Draw an edge label on the appropriate segment between two nodes.
/// For TD/BT: labels go on vertical segments
/// For LR/RL: labels go on horizontal segments
fn draw_edge_label(
    canvas: &mut Canvas,
    from: &Node,
    to: &Node,
    label: &str,
    direction: Direction,
    style: &StyleChars,
) {
    use cycle::{center_x, center_y};

    let display_label = format_edge_label(label);
    let label_width = display_width(&display_label);

    match direction {
        Direction::TD | Direction::TB => {
            // Vertical layout: place label on vertical segment
            let src_center_x = center_x(from);
            let edge_x = center_x(to);
            let stem_start_y = from.bottom_y();
            let arrow_y = to.y.saturating_sub(1);

            // For straight edges (aligned), place label in middle of vertical span
            // For L-shaped edges, place after junction
            let mut label_y = if src_center_x == edge_x {
                let span = arrow_y.saturating_sub(stem_start_y);
                let mid = stem_start_y + span / 2;
                let lower = stem_start_y.saturating_add(1);
                let upper = arrow_y.saturating_sub(2);
                let mut y = stem_start_y.saturating_add(1).max(mid);
                if lower <= upper {
                    y = y.max(lower).min(upper);
                } else {
                    y = arrow_y.saturating_sub(1);
                }
                y
            } else {
                // L-shaped: place just below the junction
                let junction_y = stem_start_y + STEM_LENGTH_VERTICAL;
                junction_y.saturating_add(1)
            };

            // Avoid overwriting borders/text (e.g., subgraph labels). If the chosen row
            // is textual, nudge the label down until we hit a free edge row before the arrow.
            while label_y > stem_start_y && label_y < arrow_y && label_y < canvas.height {
                if !is_textual(canvas.get(edge_x, label_y)) {
                    break;
                }
                label_y += 1;
            }
            if label_y + 1 < arrow_y {
                label_y = arrow_y.saturating_sub(1);
            }
            label_y = label_y.min(arrow_y.saturating_sub(1));

            // Center the label around the edge position
            let mut label_start_x = edge_x.saturating_sub(label_width / 2);
            if overlaps_node(
                &[from, to],
                label_start_x,
                label_y,
                label_width,
            ) && label_start_x + label_width + 1 < canvas.width
            {
                label_start_x += 1;
            }

            // Draw the label characters
            let mut x_pos = label_start_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height {
                    if !is_textual(canvas.get(x_pos, label_y)) {
                        canvas.set(x_pos, label_y, c);
                    }
                }
                x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }
        }
        Direction::BT => {
            // Bottom-to-top: similar to TD but arrows point up
            let src_center_x = center_x(from);
            let edge_x = center_x(to);
            let stem_start_y = from.y.saturating_sub(1);
            let arrow_y = to.bottom_y();

            let label_y = if src_center_x == edge_x {
                // Straight edge: place label in middle of vertical span
                let (top, bottom) = if stem_start_y < arrow_y {
                    (stem_start_y, arrow_y)
                } else {
                    (arrow_y, stem_start_y)
                };
                top + (bottom - top) / 2
            } else {
                // L-shaped: use junction-based positioning
                stem_start_y
                    .saturating_sub(STEM_LENGTH_VERTICAL.saturating_add(1))
            };

            let label_start_x = edge_x.saturating_sub(label_width / 2);
            let mut x_pos = label_start_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height && !is_textual(canvas.get(x_pos, label_y)) {
                    canvas.set(x_pos, label_y, c);
                }
                x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }
        }
        Direction::LR => {
            // Left-to-right: place label INLINE with edge (on the connection line)
            // Format: ├─ label ─→
            let edge_y = center_y(to);
            let stem_start_x = from.x + from.width;
            let arrow_x = to.x.saturating_sub(1);

            // Calculate the middle of the horizontal span for label placement
            let span_width = arrow_x.saturating_sub(stem_start_x);
            let label_with_padding = label_width + 2; // " label "

            if span_width >= label_with_padding + 2 {
                // Enough room for: ─ label ─
                let label_start_x = stem_start_x + (span_width - label_with_padding) / 2;

                // Draw leading edge segment (from box to label)
                for x in stem_start_x..label_start_x {
                    canvas.set(x, edge_y, style.edge_h);
                }

                // Draw space before label
                canvas.set(label_start_x, edge_y, ' ');

                // Draw label characters
                let mut x_pos = label_start_x + 1;
                for c in display_label.chars() {
                    if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                        canvas.set(x_pos, edge_y, c);
                    }
                    x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                }

                // Draw space after label
                if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                    canvas.set(x_pos, edge_y, ' ');
                }
                x_pos += 1;

                // Draw trailing edge segment (from label to arrow)
                for x in x_pos..arrow_x {
                    if x < canvas.width {
                        canvas.set(x, edge_y, style.edge_h);
                    }
                }
            } else {
                // Not enough room - place label above the edge
                let label_x = stem_start_x + span_width / 2;
                let mut label_start_x = label_x.saturating_sub(label_width / 2);
                let label_row = edge_y.saturating_sub(1);

                if overlaps_node(&[from, to], label_start_x, label_row, label_width) {
                    label_start_x = adjust_horizontal_label_slot(
                        label_start_x,
                        arrow_x + 1,
                        stem_start_x.saturating_sub(1),
                        label_row,
                        label_width,
                        &[from, to],
                    );
                }

                let mut x_pos = label_start_x;
                for c in display_label.chars() {
                    if x_pos < canvas.width && label_row < canvas.height {
                        canvas.set(x_pos, label_row, c);
                    }
                    x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                }
            }
        }
        Direction::RL => {
            // Right-to-left: place label INLINE with edge (on the connection line)
            // Format: ←─ label ─┤ (arrow on left, junction on right)
            let edge_y = center_y(to);
            let arrow_x = to.x + to.width; // Arrow is after target box
            let stem_end_x = from.x; // Edge ends at left side of source box

            // Calculate the span between arrow and source box
            let span_width = stem_end_x.saturating_sub(arrow_x);
            let label_with_padding = label_width + 2; // " label "

            if span_width >= label_with_padding + 2 {
                // Enough room for: ─ label ─
                let label_start_x = arrow_x + (span_width - label_with_padding) / 2;

                // Draw leading edge segment (from arrow to label)
                for x in (arrow_x + 1)..label_start_x {
                    if x < canvas.width {
                        canvas.set(x, edge_y, style.edge_h);
                    }
                }

                // Draw space before label
                if label_start_x < canvas.width {
                    canvas.set(label_start_x, edge_y, ' ');
                }

                // Draw label characters
                let mut x_pos = label_start_x + 1;
                for c in display_label.chars() {
                    if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                        canvas.set(x_pos, edge_y, c);
                    }
                    x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                }

                // Draw space after label
                if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                    canvas.set(x_pos, edge_y, ' ');
                }
                x_pos += 1;

                // Draw trailing edge segment (from label to source box)
                for x in x_pos..stem_end_x {
                    if x < canvas.width {
                        canvas.set(x, edge_y, style.edge_h);
                    }
                }
            } else {
                // Not enough room - place label above the edge
                let label_x = arrow_x + span_width / 2;
                let label_start_x = label_x.saturating_sub(label_width / 2);
                let label_row = edge_y.saturating_sub(1);

                let mut x_pos = label_start_x;
                for c in display_label.chars() {
                    if x_pos < canvas.width && label_row < canvas.height {
                        canvas.set(x_pos, label_row, c);
                    }
                    x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                }
            }
        }
    }
}

/// Draw an edge label using a precomputed Manhattan route. Picks the longest
/// segment (preferring horizontal) and centers the label along it.
fn draw_routed_edge_label(
    canvas: &mut Canvas,
    route: &EdgeRoute,
    label: &str,
    _style: &StyleChars,
    graph: &Graph,
) {
    if route.segments.is_empty() {
        return;
    }

    let display_label = format_edge_label(label);
    let label_width = display_width(&display_label);

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

    let Some((seg, _, is_horizontal)) = best else {
        return;
    };

    if is_horizontal {
        let mut y = seg.from.y;
        if border_spans
            .iter()
            .any(|b| y == b.y || y == b.y + b.height.saturating_sub(1))
        {
            if y + 1 < canvas.height {
                y += 1;
            } else if y > 0 {
                y -= 1;
            }
        }
        let (min_x, max_x) = if seg.from.x <= seg.to.x {
            (seg.from.x, seg.to.x)
        } else {
            (seg.to.x, seg.from.x)
        };
        let mid_x = min_x + (max_x.saturating_sub(min_x)) / 2;
        let mut start_x = mid_x.saturating_sub(label_width / 2);
        if start_x < min_x {
            start_x = min_x;
        }
        if start_x + label_width > canvas.width {
            start_x = canvas.width.saturating_sub(label_width);
        }
        start_x = adjust_horizontal_label_slot(start_x, min_x, max_x, y, label_width, &nodes);

        let mut x_pos = start_x;
        for c in display_label.chars() {
            if y < canvas.height && x_pos < canvas.width {
                canvas.set(x_pos, y, c);
            }
            x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
        }
    } else {
        let x = seg.from.x;
        let (min_y, max_y) = if seg.from.y <= seg.to.y {
            (seg.from.y, seg.to.y)
        } else {
            (seg.to.y, seg.from.y)
        };
        let mut mid_y = min_y + (max_y.saturating_sub(min_y)) / 2;
        if border_spans
            .iter()
            .any(|b| mid_y == b.y || mid_y == b.y + b.height.saturating_sub(1))
        {
            if mid_y + 1 <= max_y && mid_y + 1 < canvas.height {
                mid_y += 1;
            } else if mid_y > min_y {
                mid_y -= 1;
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
            }
            x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
        }
    }
}

/// Truncate and format edge label consistently across routing modes.
fn format_edge_label(label: &str) -> String {
    format_edge_label_with_limit(label, 12)
}

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

fn adjust_horizontal_label_slot(
    start_x: usize,
    min_x: usize,
    max_x: usize,
    y: usize,
    width: usize,
    nodes: &[&Node],
) -> usize {
    let candidate = start_x;
    if !overlaps_node(nodes, candidate, y, width) {
        return candidate;
    }

    // Try small shifts within segment bounds.
    for delta in 1..=4 {
        if candidate >= delta
            && !overlaps_node(nodes, candidate - delta, y, width)
            && candidate - delta >= min_x
        {
            return candidate - delta;
        }
        if candidate + width + delta <= max_x
            && !overlaps_node(nodes, candidate + delta, y, width)
        {
            return candidate + delta;
        }
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CompositeStyle;
    use crate::Edge;
    use crate::graph::Subgraph;

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
        output
            .lines()
            .nth(y)
            .and_then(|line| line.chars().nth(x))
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
        let portal_y = graph
            .get_subgraph("sg")
            .map(|sg| sg.bounds.y)
            .unwrap_or(0);
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
        route.push_segment(crate::geom::Point::new(5, 3), crate::geom::Point::new(12, 3));
        route.push_segment(crate::geom::Point::new(12, 3), crate::geom::Point::new(12, 4));
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
/// Labels are placed on the vertical segment from the source before the merge point.
fn draw_convergent_edge_label(
    canvas: &mut Canvas,
    from: &Node,
    _to: &Node,
    label: &str,
    direction: Direction,
) {
    use cycle::{center_x, center_y};

    let display_label = format_edge_label_with_limit(label, 10); // Slightly shorter for convergent labels
    let label_width = display_width(&display_label);

    match direction {
        Direction::TD | Direction::TB => {
            // Place label on vertical line from source, before merge point
            let src_x = center_x(from);
            let stem_start_y = from.bottom_y();
            // Place label just below the source box on the vertical stem
            let label_y = stem_start_y + 1;

            // Center label horizontally around source's edge position
            let label_start_x = src_x.saturating_sub(label_width / 2);

            let mut x_pos = label_start_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height {
                    canvas.set(x_pos, label_y, c);
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
                }
                x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }
        }
    }
}
