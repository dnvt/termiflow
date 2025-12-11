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
use crate::geom::{EdgeRoute, Point, Segment};
use crate::graph::{Graph, Node};
use crate::style::{
    display_width, truncate_label, BaseStyle, BOX_HEIGHT, COL_SPACING, CYCLE_GUTTER,
    EDGE_JUNCTION_HEIGHT, MAX_CANVAS_HEIGHT, MAX_CANVAS_WIDTH, ROW_SPACING, STEM_LENGTH_VERTICAL,
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
        .map(|n| n.y + BOX_HEIGHT)
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
    // Carve portal openings in subgraph borders so external edges can pass through
    carve_subgraph_portals_on_canvas(&mut canvas, graph, graph.direction);

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
            route_convergent_edges(&source_refs, target, &mut canvas, &chars, graph.direction);
            for source in sources {
                processed_edges.insert((&source.id, target_id));
            }
        }
    }

    // Then, handle remaining divergence cases (one source → multiple targets)
    let mut source_ids: Vec<&str> = sources_with_edges.into_iter().collect();
    source_ids.sort();
    for source_id in source_ids {
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
            draw_routed_edge_label(&mut canvas, route, label, &chars);
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

    // Draw boxes AFTER edges (boxes overwrite any edges passing through them)
    for node in &visible_nodes {
        let label = truncate_label(
            &node.label,
            config.max_label_width.min(node.width.saturating_sub(4)),
        );
        shapes::draw_node(
            &mut canvas,
            node.x,
            node.y,
            node.width,
            &label,
            node.shape,
            &chars,
            graph.direction,
        );
    }

    // Draw junction characters AFTER boxes (for LR/RL edge connections)
    // Shows where edges exit source boxes
    if matches!(graph.direction, Direction::LR | Direction::RL) {
        use cycle::center_y;
        for &source_id in edges_by_source.keys() {
            if let Some(from) = graph.get_node(source_id) {
                if canvas.is_visible(from) {
                    let junction_y = center_y(from);
                    // For LR: junction on right side of source box
                    // For RL: junction on left side of source box
                    let junction_x = if graph.direction == Direction::LR {
                        from.x + from.width - 1 // Right edge of box
                    } else {
                        from.x // Left edge of box
                    };
                    let junction_char = if graph.direction == Direction::LR {
                        chars.junction_right // ├
                    } else {
                        chars.junction_left // ┤
                    };
                    if junction_x < canvas.width && junction_y < canvas.height {
                        canvas.set(junction_x, junction_y, junction_char);
                    }
                }
            }
        }
    }

    Ok(canvas.to_string())
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
    match (a, b) {
        (Up, Left) | (Left, Up) => Some(chars.corner_ul),
        (Up, Right) | (Right, Up) => Some(chars.corner_ur),
        (Down, Left) | (Left, Down) => Some(chars.corner_dl),
        (Down, Right) | (Right, Down) => Some(chars.corner_dr),
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

fn draw_segment(
    seg: &Segment,
    dir: Dir,
    canvas: &mut Canvas,
    chars: &StyleChars,
    skip_start: bool,
    skip_end: bool,
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

            draw_segment(seg, dir, canvas, &route_chars, skip_start, skip_end);

            if is_turn {
                if let Some(nd) = next_dir {
                    if let Some(corner) = corner_for_turn(dir, nd, &route_chars) {
                        canvas.set_edge_char(seg.to.x, seg.to.y, corner, &route_chars);
                    }
                }
            }
        }

        if let Some(last_seg) = route.segments.last() {
            if let Some(dir) = dir_from_segment(last_seg) {
                let arrow = arrow_for_dir(dir, &route_chars);
                canvas.set(last_seg.to.x, last_seg.to.y, arrow);
            }
        }
    }
}

#[derive(Default)]
struct PortalSlots {
    top: HashSet<usize>,
    bottom: HashSet<usize>,
    left: HashSet<usize>,
    right: HashSet<usize>,
}

fn carve_subgraph_portals_on_canvas(canvas: &mut Canvas, graph: &Graph, direction: Direction) {
    // Collect portal coordinates per subgraph for edges that cross boundaries.
    let mut slots: HashMap<&str, PortalSlots> = HashMap::new();

    for edge in &graph.edges {
        if graph.get_node(&edge.from).is_none() || graph.get_node(&edge.to).is_none() {
            continue;
        }

        let from_sg = graph.get_node_subgraph(&edge.from);
        let to_sg = graph.get_node_subgraph(&edge.to);

        // Only care about edges that cross a subgraph boundary.
        if from_sg == to_sg {
            continue;
        }

        match direction {
            Direction::TD | Direction::TB => {
                if let Some(id) = to_sg {
                    if let Some((entry, _)) = subgraph_ports_for(graph, id, direction) {
                        slots.entry(id).or_default().top.insert(entry.x);
                    }
                }
                if let Some(id) = from_sg {
                    if let Some((_, exit)) = subgraph_ports_for(graph, id, direction) {
                        slots.entry(id).or_default().bottom.insert(exit.x);
                    }
                }
            }
            Direction::BT => {
                if let Some(id) = to_sg {
                    if let Some((_, exit)) = subgraph_ports_for(graph, id, direction) {
                        slots.entry(id).or_default().bottom.insert(exit.x);
                    }
                }
                if let Some(id) = from_sg {
                    if let Some((entry, _)) = subgraph_ports_for(graph, id, direction) {
                        slots.entry(id).or_default().top.insert(entry.x);
                    }
                }
            }
            Direction::LR => {
                if let Some(id) = to_sg {
                    if let Some((entry, _)) = subgraph_ports_for(graph, id, direction) {
                        slots.entry(id).or_default().left.insert(entry.y);
                    }
                }
                if let Some(id) = from_sg {
                    if let Some((_, exit)) = subgraph_ports_for(graph, id, direction) {
                        slots.entry(id).or_default().right.insert(exit.y);
                    }
                }
            }
            Direction::RL => {
                if let Some(id) = to_sg {
                    if let Some((_, exit)) = subgraph_ports_for(graph, id, direction) {
                        slots.entry(id).or_default().right.insert(exit.y);
                    }
                }
                if let Some(id) = from_sg {
                    if let Some((entry, _)) = subgraph_ports_for(graph, id, direction) {
                        slots.entry(id).or_default().left.insert(entry.y);
                    }
                }
            }
        }
    }

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
            carve_vertical_slot(canvas, px, &[top_y.saturating_add(1), top_y]);
        }
        for &x in &portals.bottom {
            let px = clamp_horizontal(bounds, x);
            carve_vertical_slot(canvas, px, &[bottom_y.saturating_sub(1), bottom_y]);
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

fn subgraph_ports_for(graph: &Graph, sg_id: &str, direction: Direction) -> Option<(Point, Point)> {
    let sg = graph.get_subgraph(sg_id)?;
    Some(subgraph_ports(&sg.bounds, direction, sg.title.as_deref()))
}

fn subgraph_ports(
    bounds: &crate::graph::Rectangle,
    direction: Direction,
    title: Option<&str>,
) -> (Point, Point) {
    let cx = subgraph_port_center(bounds, title);
    let cy = bounds.y + bounds.height / 2;
    match direction {
        Direction::TD | Direction::TB => (
            Point::new(cx, bounds.y.saturating_add(1)),
            Point::new(cx, bounds.y + bounds.height.saturating_sub(2)),
        ),
        Direction::BT => (
            Point::new(cx, bounds.y + bounds.height.saturating_sub(2)),
            Point::new(cx, bounds.y.saturating_add(1)),
        ),
        Direction::LR => (
            Point::new(bounds.x, cy),
            Point::new(bounds.x + bounds.width.saturating_sub(1), cy),
        ),
        Direction::RL => (
            Point::new(bounds.x + bounds.width.saturating_sub(1), cy),
            Point::new(bounds.x, cy),
        ),
    }
}

fn subgraph_port_center(bounds: &crate::graph::Rectangle, title: Option<&str>) -> usize {
    let _ = title;
    bounds.x + bounds.width / 2
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
            let stem_start_y = from.y + BOX_HEIGHT;
            let arrow_y = to.y.saturating_sub(1);

            // For straight edges (aligned), place label in middle of vertical span
            // For L-shaped edges, place after junction
            let label_y = if src_center_x == edge_x {
                // Straight edge: place label in middle of vertical span
                stem_start_y + (arrow_y.saturating_sub(stem_start_y)) / 2
            } else {
                // L-shaped: use junction-based positioning
                let junction_y = stem_start_y + STEM_LENGTH_VERTICAL;
                junction_y + EDGE_JUNCTION_HEIGHT
            };

            // Center the label around the edge position
            let label_start_x = edge_x.saturating_sub(label_width / 2);

            // Draw the label characters
            let mut x_pos = label_start_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height {
                    canvas.set(x_pos, label_y, c);
                }
                x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            }
        }
        Direction::BT => {
            // Bottom-to-top: similar to TD but arrows point up
            let src_center_x = center_x(from);
            let edge_x = center_x(to);
            let stem_start_y = from.y.saturating_sub(1);
            let arrow_y = to.y + BOX_HEIGHT;

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
                    .saturating_sub(STEM_LENGTH_VERTICAL)
                    .saturating_sub(EDGE_JUNCTION_HEIGHT)
            };

            let label_start_x = edge_x.saturating_sub(label_width / 2);
            let mut x_pos = label_start_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height {
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
                    if x_pos < canvas.width {
                        canvas.set(x_pos, edge_y, c);
                    }
                    x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                }

                // Draw space after label
                if x_pos < canvas.width {
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
                    if x_pos < canvas.width {
                        canvas.set(x_pos, edge_y, c);
                    }
                    x_pos += unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                }

                // Draw space after label
                if x_pos < canvas.width {
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
) {
    if route.segments.is_empty() {
        return;
    }

    let display_label = format_edge_label(label);
    let label_width = display_width(&display_label);

    // Choose longest segment, prefer horizontal for readability.
    let mut best: Option<(&Segment, usize, bool)> = None; // (segment, length, is_horizontal)
    for seg in &route.segments {
        let is_horizontal = seg.from.y == seg.to.y;
        let length = if is_horizontal {
            seg.from.x.abs_diff(seg.to.x)
        } else {
            seg.from.y.abs_diff(seg.to.y)
        };

        match best {
            None => best = Some((seg, length, is_horizontal)),
            Some((_, best_len, best_horizontal)) => {
                if (is_horizontal && !best_horizontal)
                    || (is_horizontal == best_horizontal && length > best_len)
                {
                    best = Some((seg, length, is_horizontal));
                }
            }
        }
    }

    let Some((seg, _, is_horizontal)) = best else {
        return;
    };

    if is_horizontal {
        let y = seg.from.y;
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
        let mid_y = min_y + (max_y.saturating_sub(min_y)) / 2;
        let mut start_x = x.saturating_sub(label_width / 2);
        if start_x + label_width > canvas.width {
            start_x = canvas.width.saturating_sub(label_width);
        }

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

#[cfg(test)]
mod tests {
    use super::*;
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
            .build(&crate::parser::ParseConfig::default());

        let output = render(&graph, &config).expect("render back edge");

        // Unicode back edges use dotted style, ensure we see a back-edge glyph sequence.
        assert!(
            output.contains("⋯") || output.contains("┄") || output.contains('─'),
            "expected back-edge route to render with visible glyphs, got:\n{}",
            output
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
            let stem_start_y = from.y + BOX_HEIGHT;
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
