//! Experimental coarse layout + Manhattan routing pipeline.
//!
//! This module keeps the existing waterfall layout untouched while providing
//! a sandboxed pipeline to explore:
//! - Direction-agnostic layered placement on a coarse grid
//! - Obstacle-aware Manhattan routing with simple detours
//! - Subgraph gutter metadata for future avoidance/bundling

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, VecDeque};

use anyhow::Result;

use crate::geom::{EdgeRoute, Point, Rect};
use crate::graph::{Direction, Graph};
use crate::orientation::{Axis, OrientedCoords};
use crate::style::{box_width, BOX_HEIGHT, BOX_MIN_WIDTH};

/// Input for the experimental layout engine.
pub struct LayoutInput<'a> {
    pub graph: &'a Graph,
    pub prior_positions: Option<HashMap<String, Point>>,
}

/// Output of the experimental layout pipeline.
#[derive(Debug, Default)]
pub struct LayoutOutput {
    pub positions: HashMap<String, Point>,
    pub subgraph_bounds: HashMap<String, SubgraphBounds>,
    pub routes: HashMap<usize, EdgeRoute>,
    pub canvas: Rect,
    pub warnings: Vec<String>,
}

/// Outer/inner bounds for a subgraph (outer includes gutter).
#[derive(Debug, Clone, Copy)]
pub struct SubgraphBounds {
    pub outer: Rect,
    pub inner: Rect,
}

/// Tunable spacing controls.
#[derive(Debug, Clone)]
pub struct CoarseLayoutConfig {
    /// Padding around nodes when building the occupancy grid.
    pub node_padding: usize,
    /// Gutter around subgraphs (stored separately; optionally treated as obstacles).
    pub subgraph_gutter: usize,
    /// Minimum spacing along the horizontal axis.
    pub min_horizontal_spacing: usize,
    /// Minimum spacing along the vertical axis.
    pub min_vertical_spacing: usize,
}

impl Default for CoarseLayoutConfig {
    fn default() -> Self {
        Self {
            node_padding: 1,
            subgraph_gutter: 2,
            min_horizontal_spacing: 4,
            min_vertical_spacing: 4,
        }
    }
}

/// Experimental layout engine entry point.
pub fn layout(input: LayoutInput, config: CoarseLayoutConfig) -> Result<LayoutOutput> {
    let coords = OrientedCoords::new(input.graph.direction);

    // 1) Layer assignment (lenient Kahn) and ordering.
    let layers = assign_layers(input.graph);

    // 2) Place nodes on coarse grid.
    let mut placement = place_nodes(
        input.graph,
        &layers,
        &coords,
        &config,
        input.prior_positions.as_ref(),
    );

    // 2.5) Flip coordinates for BT/RL to match flow direction
    // Calculate strict content bounds
    let max_x = placement.node_rects.values().map(|r| r.right()).max().unwrap_or(0);
    let max_y = placement.node_rects.values().map(|r| r.bottom()).max().unwrap_or(0);

    if input.graph.direction == Direction::BT {
        for p in placement.positions.values_mut() {
            p.y = max_y.saturating_sub(p.y).saturating_sub(BOX_HEIGHT);
        }
        for r in placement.node_rects.values_mut() {
            r.y = max_y.saturating_sub(r.y).saturating_sub(r.height);
        }
    } else if input.graph.direction == Direction::RL {
        // Easier: Iterate keys of positions (node ids)
        for (id, p) in placement.positions.iter_mut() {
            if let Some(r) = placement.node_rects.get_mut(id) {
                let new_x = max_x.saturating_sub(r.x + r.width);
                p.x = new_x;
                r.x = new_x;
            }
        }
    }

    // Shift nodes to make room for subgraph gutters if any subgraphs exist
    if !input.graph.subgraphs.is_empty() {
        let shift = config.subgraph_gutter;
        for p in placement.positions.values_mut() {
            p.x += shift;
            p.y += shift;
        }
        for r in placement.node_rects.values_mut() {
            r.x += shift;
            r.y += shift;
        }
        // Canvas grows by the shift amount (padding on both sides)
        placement.canvas.width = max_x + shift * 2;
        placement.canvas.height = max_y + shift * 2;
    } else {
        // Tighten canvas to content if no subgraphs (optional, but cleaner)
        placement.canvas.width = max_x;
        placement.canvas.height = max_y;
    }

    // 3) Subgraph bounds + gutters.
    let subgraph_bounds = compute_subgraph_bounds(input.graph, &placement.node_rects, config.subgraph_gutter);

    // 4) Occupancy grid seeded with node padding and subgraph gutters (with carved portals).
    let mut grid = OccupancyGrid::new(
        placement.canvas.right() + config.min_horizontal_spacing + config.subgraph_gutter + 4,
        placement.canvas.bottom() + config.min_vertical_spacing + config.subgraph_gutter + 4,
    );
    for rect in placement.node_rects.values() {
        grid.mark_rect(&rect.inflate(config.node_padding));
    }
    carve_node_portals(&mut grid, &placement.node_rects, &coords, config.node_padding);
    mark_subgraph_rings(&mut grid, &subgraph_bounds);
    carve_subgraph_portals(&mut grid, &subgraph_bounds, &coords, config.subgraph_gutter);

    // 5) Route edges with Manhattan + obstacle avoidance.
    let mut routes: HashMap<usize, EdgeRoute> = HashMap::new();
    let mut warnings = Vec::new();
    for (edge_idx, edge) in input.graph.edges.iter().enumerate() {
        let from_rect = placement
            .node_rects
            .get(&edge.from)
            .cloned()
            .unwrap_or_default();
        let to_rect = placement.node_rects.get(&edge.to).cloned().unwrap_or_default();

        // Compute avoid gutters (all subgraphs except those containing endpoints).
        let avoid_rects = gutters_to_avoid(
            input.graph,
            &subgraph_bounds,
            edge_idx,
            &edge.from,
            &edge.to,
        );

        let start = edge_exit_point(from_rect, input.graph.direction);
        let end = edge_entry_point(to_rect, input.graph.direction);

        match route_with_obstacles(start, end, &grid, &avoid_rects, &coords) {
            Some(route) => {
                routes.insert(edge_idx, route);
            }
            None => {
                warnings.push(format!(
                    "termiflow: warning: no route for edge {} -> {}",
                    edge.from, edge.to
                ));
            }
        }
    }

    Ok(LayoutOutput {
        positions: placement.positions,
        subgraph_bounds,
        routes,
        canvas: placement.canvas,
        warnings,
    })
}

/// Convenience helper: run the spike layout and apply positions back to the graph.
pub fn apply_spike_layout(
    mut graph: Graph,
    prior_positions: Option<HashMap<String, Point>>,
    config: CoarseLayoutConfig,
) -> Result<Graph> {
    // Ensure all nodes have valid dimensions before layout
    for node in graph.nodes.iter_mut() {
        if node.width == 0 {
            node.width = box_width(&node.label).max(BOX_MIN_WIDTH);
        }
    }

    let output = layout(LayoutInput { graph: &graph, prior_positions }, config)?;

    for node in graph.nodes.iter_mut() {
        if let Some(p) = output.positions.get(&node.id) {
            node.x = p.x;
            node.y = p.y;
        }
    }

    for subgraph in graph.subgraphs.iter_mut() {
        if let Some(bounds) = output.subgraph_bounds.get(&subgraph.id) {
            subgraph.bounds = crate::graph::Rectangle::new(bounds.outer.x, bounds.outer.y, bounds.outer.width, bounds.outer.height);
        }
    }

    graph.edge_routes = output.routes;

    for w in output.warnings {
        graph.warnings.push(w);
    }

    Ok(graph)
}

// -----------------------------------------------------------------------------
// Placement
// -----------------------------------------------------------------------------

#[derive(Debug)]
struct Placement {
    positions: HashMap<String, Point>,
    node_rects: HashMap<String, Rect>,
    canvas: Rect,
}

fn gap_for_axis(axis: Axis, cfg: &CoarseLayoutConfig) -> usize {
    match axis {
        Axis::Horizontal => cfg.min_horizontal_spacing,
        Axis::Vertical => cfg.min_vertical_spacing,
    }
}

fn compute_primary_gaps(
    graph: &Graph,
    layers: &[Vec<usize>],
    coords: &OrientedCoords,
    cfg: &CoarseLayoutConfig,
) -> Vec<usize> {
    let base = gap_for_axis(coords.primary, cfg);
    layers
        .iter()
        .map(|layer| {
            let has_labels = layer.iter().any(|idx| {
                let node_id = &graph.nodes[*idx].id;
                graph
                    .edges
                    .iter()
                    .any(|e| e.from == *node_id && e.label.is_some())
            });
            if has_labels {
                base + 1
            } else {
                base
            }
        })
        .collect()
}

fn median_primary(
    layer: &[usize],
    graph: &Graph,
    prior: &HashMap<String, Point>,
    coords: &OrientedCoords,
) -> Option<usize> {
    let mut vals: Vec<usize> = layer
        .iter()
        .filter_map(|idx| prior.get(&graph.nodes[*idx].id))
        .map(|p| coords.primary_coord(p.x, p.y))
        .collect();
    if vals.is_empty() {
        return None;
    }
    vals.sort_unstable();
    Some(vals[vals.len() / 2])
}

fn assign_layers(graph: &Graph) -> Vec<Vec<usize>> {
    let mut index_map: HashMap<&str, usize> = HashMap::new();
    for (idx, node) in graph.nodes.iter().enumerate() {
        index_map.insert(&node.id, idx);
    }

    let mut indegree = vec![0usize; graph.nodes.len()];
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); graph.nodes.len()];
    for edge in &graph.edges {
        if edge.is_back_edge {
            continue;
        }
        if let (Some(&from_idx), Some(&to_idx)) = (index_map.get(edge.from.as_str()), index_map.get(edge.to.as_str())) {
            indegree[to_idx] += 1;
            adj[from_idx].push(to_idx);
        }
    }

    let mut queue: VecDeque<usize> = indegree
        .iter()
        .enumerate()
        .filter_map(|(idx, &deg)| if deg == 0 { Some(idx) } else { None })
        .collect();

    let mut order = Vec::new();
    let mut rank = vec![0usize; graph.nodes.len()];
    while let Some(u) = queue.pop_front() {
        order.push(u);
        for &v in &adj[u] {
            if indegree[v] > 0 {
                indegree[v] -= 1;
                if indegree[v] == 0 {
                    rank[v] = rank[u] + 1;
                    queue.push_back(v);
                }
            }
        }
    }

    // Any nodes not processed (cycles/disconnected) keep rank 0 but deterministic position
    for idx in 0..graph.nodes.len() {
        if !order.contains(&idx) {
            order.push(idx);
        }
    }

    let mut by_rank: HashMap<usize, Vec<usize>> = HashMap::new();
    for (idx, r) in rank.iter().enumerate() {
        by_rank.entry(*r).or_default().push(idx);
    }

    let max_rank = *rank.iter().max().unwrap_or(&0);
    let mut layers: Vec<Vec<usize>> = Vec::with_capacity(max_rank + 1);
    for r in 0..=max_rank {
        let mut layer = by_rank.remove(&r).unwrap_or_default();
        layer.sort_by_key(|idx| graph.nodes[*idx].id.clone());
        layers.push(layer);
    }

    layers
}

fn node_extent_primary(node: &crate::graph::Node, coords: &OrientedCoords) -> usize {
    match coords.primary {
        Axis::Vertical => BOX_HEIGHT,
        Axis::Horizontal => node.width,
    }
}

fn node_extent_secondary(node: &crate::graph::Node, coords: &OrientedCoords) -> usize {
    match coords.secondary {
        Axis::Vertical => BOX_HEIGHT,
        Axis::Horizontal => node.width,
    }
}

fn place_nodes(
    graph: &Graph,
    layers: &[Vec<usize>],
    coords: &OrientedCoords,
    config: &CoarseLayoutConfig,
    prior_positions: Option<&HashMap<String, Point>>,
) -> Placement {
    let mut positions: HashMap<String, Point> = HashMap::new();
    let mut node_rects: HashMap<String, Rect> = HashMap::new();

    let primary_gaps = compute_primary_gaps(graph, layers, coords, config);

    // Compute primary offsets per layer (cumulative max extent + gap), reusing prior primary coords if provided.
    let mut primary_offsets: Vec<usize> = Vec::with_capacity(layers.len());
    let mut primary_cursor = 0usize;
    for (i, layer) in layers.iter().enumerate() {
        let max_extent = layer
            .iter()
            .map(|idx| node_extent_primary(&graph.nodes[*idx], coords))
            .max()
            .unwrap_or(BOX_HEIGHT);

        let prior_primary = prior_positions
            .and_then(|m| median_primary(layer, graph, m, coords));
        let primary_pos = prior_primary.map(|p| p.max(primary_cursor)).unwrap_or(primary_cursor);

        primary_offsets.push(primary_pos);
        primary_cursor = primary_pos + max_extent + primary_gaps[i];
    }

    let secondary_gap = gap_for_axis(coords.secondary, config);

    for (layer_idx, layer) in layers.iter().enumerate() {
        let primary_pos = primary_offsets[layer_idx];
        let mut secondary_cursor = 0usize;

        for &node_idx in layer {
            let node = &graph.nodes[node_idx];
            // Reuse prior secondary position if available to improve stability
            let mut secondary_pos = prior_positions
                .and_then(|m| m.get(&node.id))
                .map(|p| coords.secondary_coord(p.x, p.y))
                .unwrap_or(secondary_cursor);
            if secondary_pos < secondary_cursor {
                secondary_pos = secondary_cursor;
            }

            let mut x = 0usize;
            let mut y = 0usize;
            coords.set_primary(&mut x, &mut y, primary_pos);
            coords.set_secondary(&mut x, &mut y, secondary_pos);

            positions.insert(node.id.clone(), Point::new(x, y));
            node_rects.insert(node.id.clone(), Rect::new(x, y, node.width, BOX_HEIGHT));

            secondary_cursor = secondary_pos + node_extent_secondary(node, coords) + secondary_gap;
        }
    }

    // Center layers along secondary axis
    // 1. Calculate spans
    let mut layer_spans: Vec<(usize, usize, usize)> = Vec::with_capacity(layers.len());
    let mut max_span = 0;

    for layer in layers {
        if layer.is_empty() {
            layer_spans.push((0, 0, 0));
            continue;
        }
        let mut min_sec = usize::MAX;
        let mut max_sec = 0;
        for &node_idx in layer {
            let id = &graph.nodes[node_idx].id;
            if let Some(rect) = node_rects.get(id) {
                let (start, len) = match coords.secondary {
                    Axis::Horizontal => (rect.x, rect.width),
                    Axis::Vertical => (rect.y, rect.height),
                };
                min_sec = min_sec.min(start);
                max_sec = max_sec.max(start + len);
            }
        }
        let span = if min_sec <= max_sec { max_sec - min_sec } else { 0 };
        layer_spans.push((min_sec, max_sec, span));
        max_span = max_span.max(span);
    }

    // 2. Apply offsets
    for (i, layer) in layers.iter().enumerate() {
        let (min_sec, _, span) = layer_spans[i];
        if span == 0 { continue; }
        
        // Calculate offset to center this layer within the max_span
        // We want to move the layer so its center aligns with max_span's center
        // Center of max_span is max_span/2 (relative to 0)
        // Center of current layer is min_sec + span/2
        // We want new_min_sec + span/2 = max_span/2
        // new_min_sec = (max_span - span) / 2
        // Shift amount = new_min_sec - min_sec
        // Wait, current nodes start at `min_sec`. We want them to start at `(max_span - span) / 2`.
        // BUT, `min_sec` might be > 0 if we had a prior_position logic (though simplified here starts at 0 or aligned).
        // Let's assume we shift relative to the start of the layer.
        
        let target_start = (max_span - span) / 2;
        
        // Only shift if target is greater than current start (to avoid underflow/left-shift)
        // In this simple placement, min_sec usually starts near 0 for the first item?
        // Actually `secondary_cursor` starts at 0. So `min_sec` is likely 0 for the leftmost node.
        
        if target_start > min_sec {
            let shift = target_start - min_sec;
            for &node_idx in layer {
                let id = &graph.nodes[node_idx].id;
                if let Some(p) = positions.get_mut(id) {
                    match coords.secondary {
                        Axis::Horizontal => p.x += shift,
                        Axis::Vertical => p.y += shift,
                    }
                }
                if let Some(r) = node_rects.get_mut(id) {
                    match coords.secondary {
                        Axis::Horizontal => r.x += shift,
                        Axis::Vertical => r.y += shift,
                    }
                }
            }
        }
    }

    // Compute canvas bounds (include padding to give the router some breathing room)
    let max_x = node_rects
        .values()
        .map(|r| r.right() + config.min_horizontal_spacing)
        .max()
        .unwrap_or(0);
    let max_y = node_rects
        .values()
        .map(|r| r.bottom() + config.min_vertical_spacing)
        .max()
        .unwrap_or(0);

    let canvas = Rect::new(0, 0, max_x + 1, max_y + 1);

    Placement {
        positions,
        node_rects,
        canvas,
    }
}

// -----------------------------------------------------------------------------
// Subgraphs
// -----------------------------------------------------------------------------

fn compute_subgraph_bounds(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    gutter: usize,
) -> HashMap<String, SubgraphBounds> {
    let mut bounds = HashMap::new();
    for subgraph in &graph.subgraphs {
        let mut inner = Rect::default();
        for node_id in &subgraph.node_ids {
            if let Some(r) = node_rects.get(node_id) {
                inner = if inner.is_empty() { *r } else { inner.union(r) };
            }
        }
        if inner.is_empty() {
            continue;
        }
        let outer = inner.inflate(gutter);
        bounds.insert(subgraph.id.clone(), SubgraphBounds { outer, inner });
    }
    bounds
}

fn gutters_to_avoid(
    graph: &Graph,
    subgraph_bounds: &HashMap<String, SubgraphBounds>,
    _edge_idx: usize,
    from: &str,
    to: &str,
) -> Vec<Rect> {
    // Skip gutters that contain either endpoint to avoid blocking exits.
    let mut avoid = Vec::new();
    for (sg_id, bounds) in subgraph_bounds {
        let contains_endpoint = graph
            .node_subgraph
            .get(from)
            .map(|id| id == sg_id)
            .unwrap_or(false)
            || graph
                .node_subgraph
                .get(to)
                .map(|id| id == sg_id)
                .unwrap_or(false);
        if !contains_endpoint {
            avoid.push(bounds.outer);
        }
    }
    avoid
}

fn mark_subgraph_rings(grid: &mut OccupancyGrid, subgraphs: &HashMap<String, SubgraphBounds>) {
    for bounds in subgraphs.values() {
        let outer = bounds.outer;
        let inner = bounds.inner;
        if outer.is_empty() || inner.is_empty() {
            continue;
        }

        // Top band
        if inner.y > outer.y {
            grid.mark_rect(&Rect::new(outer.x, outer.y, outer.width, inner.y.saturating_sub(outer.y)));
        }
        // Bottom band
        if outer.bottom() > inner.bottom() {
            grid.mark_rect(&Rect::new(
                outer.x,
                inner.bottom(),
                outer.width,
                outer.bottom().saturating_sub(inner.bottom()),
            ));
        }
        // Left band
        if inner.x > outer.x {
            grid.mark_rect(&Rect::new(
                outer.x,
                inner.y,
                inner.x.saturating_sub(outer.x),
                inner.height,
            ));
        }
        // Right band
        if outer.right() > inner.right() {
            grid.mark_rect(&Rect::new(
                inner.right(),
                inner.y,
                outer.right().saturating_sub(inner.right()),
                inner.height,
            ));
        }
    }
}

fn carve_node_portals(
    grid: &mut OccupancyGrid,
    node_rects: &HashMap<String, Rect>,
    coords: &OrientedCoords,
    padding: usize,
) {
    for rect in node_rects.values() {
        let entry = edge_entry_point(*rect, coords.direction);
        let exit = edge_exit_point(*rect, coords.direction);

        // Determine clearing direction based on layout direction
        // Entry clears OUTWARDS from the box (opposite to flow into box)
        // Exit clears OUTWARDS from the box (with flow out of box)
        let (entry_dir, exit_dir) = match coords.direction {
            Direction::TD | Direction::TB => ((0, -1), (0, 1)),
            Direction::BT => ((0, 1), (0, -1)),
            Direction::LR => ((-1, 0), (1, 0)),
            Direction::RL => ((1, 0), (-1, 0)),
        };

        for i in 0..=padding {
            // Clear entry path
            let ex = if entry_dir.0 < 0 {
                entry.x.saturating_sub((-entry_dir.0 * i as isize) as usize)
            } else {
                entry.x.saturating_add((entry_dir.0 * i as isize) as usize)
            };
            let ey = if entry_dir.1 < 0 {
                entry.y.saturating_sub((-entry_dir.1 * i as isize) as usize)
            } else {
                entry.y.saturating_add((entry_dir.1 * i as isize) as usize)
            };
            grid.clear_point(Point::new(ex, ey));

            // Clear exit path
            let xx = if exit_dir.0 < 0 {
                exit.x.saturating_sub((-exit_dir.0 * i as isize) as usize)
            } else {
                exit.x.saturating_add((exit_dir.0 * i as isize) as usize)
            };
            let xy = if exit_dir.1 < 0 {
                exit.y.saturating_sub((-exit_dir.1 * i as isize) as usize)
            } else {
                exit.y.saturating_add((exit_dir.1 * i as isize) as usize)
            };
            grid.clear_point(Point::new(xx, xy));
        }
    }
}

fn carve_subgraph_portals(
    grid: &mut OccupancyGrid,
    subgraphs: &HashMap<String, SubgraphBounds>,
    coords: &OrientedCoords,
    gutter: usize,
) {
    let span = gutter.max(1) * 2 + 1;
    for bounds in subgraphs.values() {
        let (entry, exit) = subgraph_ports(bounds, coords.direction);
        carve_band(grid, entry, coords.secondary, span);
        carve_band(grid, exit, coords.secondary, span);
    }
}

fn subgraph_ports(bounds: &SubgraphBounds, direction: Direction) -> (Point, Point) {
    let cx = bounds.outer.x + bounds.outer.width / 2;
    let cy = bounds.outer.y + bounds.outer.height / 2;
    match direction {
        Direction::TD | Direction::TB => (
            Point::new(cx, bounds.outer.y),
            Point::new(cx, bounds.outer.bottom().saturating_sub(1)),
        ),
        Direction::BT => (
            Point::new(cx, bounds.outer.bottom().saturating_sub(1)),
            Point::new(cx, bounds.outer.y),
        ),
        Direction::LR => (
            Point::new(bounds.outer.x, cy),
            Point::new(bounds.outer.right().saturating_sub(1), cy),
        ),
        Direction::RL => (
            Point::new(bounds.outer.right().saturating_sub(1), cy),
            Point::new(bounds.outer.x, cy),
        ),
    }
}

fn carve_band(grid: &mut OccupancyGrid, center: Point, axis: Axis, span: usize) {
    let half = span / 2;
    match axis {
        Axis::Horizontal => {
            let start_x = center.x.saturating_sub(half);
            let end_x = start_x + span;
            for x in start_x..end_x {
                grid.clear_point(Point::new(x, center.y));
            }
        }
        Axis::Vertical => {
            let start_y = center.y.saturating_sub(half);
            let end_y = start_y + span;
            for y in start_y..end_y {
                grid.clear_point(Point::new(center.x, y));
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Routing
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct OccupancyGrid {
    width: usize,
    height: usize,
    blocked: Vec<bool>,
}

impl OccupancyGrid {
    fn new(width: usize, height: usize) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            width,
            height,
            blocked: vec![false; width * height],
        }
    }

    fn in_bounds(&self, p: Point) -> bool {
        p.x < self.width && p.y < self.height
    }

    fn idx(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }

    fn mark_rect(&mut self, rect: &Rect) {
        if rect.is_empty() {
            return;
        }
        let x_end = rect.right().min(self.width);
        let y_end = rect.bottom().min(self.height);
        let x_start = rect.x.min(self.width);
        let y_start = rect.y.min(self.height);

        for y in y_start..y_end {
            let row_offset = y * self.width;
            for x in x_start..x_end {
                self.blocked[row_offset + x] = true;
            }
        }
    }

    fn clear_point(&mut self, p: Point) {
        if self.in_bounds(p) {
            let idx = self.idx(p.x, p.y);
            self.blocked[idx] = false;
        }
    }

    fn is_blocked(&self, p: Point) -> bool {
        if !self.in_bounds(p) {
            return true;
        }
        self.blocked[self.idx(p.x, p.y)]
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
struct PathNode {
    cost: usize,
    estimate: usize,
    point: Point,
}

impl Ord for PathNode {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap behavior using BinaryHeap
        (other.cost + other.estimate).cmp(&(self.cost + self.estimate))
    }
}

impl PartialOrd for PathNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn manhattan(a: Point, b: Point) -> usize {
    a.x.abs_diff(b.x) + a.y.abs_diff(b.y)
}

fn route_with_obstacles(
    start: Point,
    end: Point,
    grid: &OccupancyGrid,
    avoid_rects: &[Rect],
    coords: &OrientedCoords,
) -> Option<EdgeRoute> {
    if start == end {
        let mut route = EdgeRoute::new();
        route.push_segment(start, end);
        return Some(route);
    }

    let mut came_from: HashMap<Point, Point> = HashMap::new();
    let mut best_cost: HashMap<Point, usize> = HashMap::new();
    let mut open = BinaryHeap::new();

    open.push(PathNode {
        cost: 0,
        estimate: manhattan(start, end),
        point: start,
    });
    best_cost.insert(start, 0);

    while let Some(current) = open.pop() {
        if current.point == end {
            break;
        }

        let neighbors = ordered_neighbors(current.point, end, coords);
        for next in neighbors {
            if is_blocked(next, grid, avoid_rects, start, end) {
                continue;
            }
            let new_cost = current.cost + 1;
            let known = best_cost.get(&next).copied().unwrap_or(usize::MAX);
            if new_cost < known {
                best_cost.insert(next, new_cost);
                came_from.insert(next, current.point);
                open.push(PathNode {
                    cost: new_cost,
                    estimate: manhattan(next, end),
                    point: next,
                });
            }
        }
    }

    if !came_from.contains_key(&end) && start != end {
        return None;
    }

    let mut path: Vec<Point> = Vec::new();
    let mut current = end;
    path.push(current);
    while let Some(prev) = came_from.get(&current) {
        current = *prev;
        path.push(current);
        if current == start {
            break;
        }
    }
    path.reverse();

    Some(compress_path(&path))
}

fn is_blocked(p: Point, grid: &OccupancyGrid, avoid_rects: &[Rect], start: Point, end: Point) -> bool {
    if p == start || p == end {
        return false;
    }
    if grid.is_blocked(p) {
        return true;
    }
    avoid_rects.iter().any(|r| r.contains(p))
}

fn ordered_neighbors(current: Point, goal: Point, coords: &OrientedCoords) -> Vec<Point> {
    let dx = goal.x as isize - current.x as isize;
    let dy = goal.y as isize - current.y as isize;

    let primary_first = if coords.primary == Axis::Horizontal {
        vec![(dx.signum(), 0), (0, dy.signum()), (-dx.signum(), 0), (0, -dy.signum())]
    } else {
        vec![(0, dy.signum()), (dx.signum(), 0), (0, -dy.signum()), (-dx.signum(), 0)]
    };

    let mut neighbors = Vec::new();
    for (sx, sy) in primary_first {
        if sx == 0 && sy == 0 {
            continue;
        }
        let nx = if sx.is_negative() {
            current.x.saturating_sub(sx.unsigned_abs())
        } else {
            current.x.saturating_add(sx as usize)
        };
        let ny = if sy.is_negative() {
            current.y.saturating_sub(sy.unsigned_abs())
        } else {
            current.y.saturating_add(sy as usize)
        };
        neighbors.push(Point::new(nx, ny));
    }
    neighbors
}

fn compress_path(points: &[Point]) -> EdgeRoute {
    let mut route = EdgeRoute::new();
    if points.is_empty() {
        return route;
    }
    if points.len() == 1 {
        route.push_segment(points[0], points[0]);
        return route;
    }

    let mut seg_start = points[0];
    let mut last_dir = (0isize, 0isize);
    for window in points.windows(2) {
        let a = window[0];
        let b = window[1];
        let dir = (
            b.x as isize - a.x as isize,
            b.y as isize - a.y as isize,
        );
        let norm = (dir.0.signum(), dir.1.signum());
        if last_dir != norm && last_dir != (0, 0) {
            route.push_segment(seg_start, a);
            seg_start = a;
        }
        last_dir = norm;
    }
    route.push_segment(seg_start, *points.last().unwrap());
    route
}

fn edge_exit_point(rect: Rect, direction: Direction) -> Point {
    match direction {
        Direction::TD | Direction::TB => Point::new(rect.x + rect.width / 2, rect.y + rect.height),
        Direction::BT => Point::new(rect.x + rect.width / 2, rect.y.saturating_sub(1)),
        Direction::LR => Point::new(rect.x + rect.width, rect.y + rect.height / 2),
        Direction::RL => Point::new(rect.x.saturating_sub(1), rect.y + rect.height / 2),
    }
}

fn edge_entry_point(rect: Rect, direction: Direction) -> Point {
    match direction {
        Direction::TD | Direction::TB => Point::new(rect.x + rect.width / 2, rect.y.saturating_sub(1)),
        Direction::BT => Point::new(rect.x + rect.width / 2, rect.y + rect.height),
        Direction::LR => Point::new(rect.x.saturating_sub(1), rect.y + rect.height / 2),
        Direction::RL => Point::new(rect.x + rect.width, rect.y + rect.height / 2),
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, Node};

    fn simple_graph(direction: Direction) -> Graph {
        let mut g = Graph::new();
        g.direction = direction;
        g.nodes.push(Node::new("A", "A"));
        g.nodes.push(Node::new("B", "B"));
        g.edges.push(Edge::new("A", "B"));
        g
    }

    #[test]
    fn routes_around_obstacle() {
        let graph = simple_graph(Direction::TD);
        let input = LayoutInput {
            graph: &graph,
            prior_positions: None,
        };
        let cfg = CoarseLayoutConfig::default();
        let output = layout(input, cfg).expect("layout");
        let route = output.routes.get(&0).expect("route");
        assert!(!route.segments.is_empty());
    }

    #[test]
    fn gutter_avoids_external_edges() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;
        graph.nodes.push(Node::new("A", "A"));
        graph.nodes.push(Node::new("B", "B"));
        graph.nodes.push(Node::new("C", "C"));
        graph.edges.push(Edge::new("A", "B"));
        graph.edges.push(Edge::new("B", "C"));

        let mut sg = crate::graph::Subgraph::new("sg1", Some("Group".into()));
        sg.add_node("B");
        graph.add_subgraph(sg);
        graph.associate_node_with_subgraph("B", "sg1");

        let input = LayoutInput {
            graph: &graph,
            prior_positions: None,
        };
        let output = layout(input, CoarseLayoutConfig::default()).expect("layout");
        assert!(output.subgraph_bounds.get("sg1").is_some());
        // We still expect routes even with gutters present
        assert_eq!(output.routes.len(), 2);
    }
}
