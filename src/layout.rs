//! Coarse layout + Manhattan routing pipeline (default engine).
//!
//! This module supersedes the legacy waterfall layout while keeping a legacy
//! implementation available in `layout_legacy` for compatibility. The coarse
//! engine provides:
//! - Direction-agnostic layered placement on a coarse grid
//! - Obstacle-aware Manhattan routing with simple detours
//! - Subgraph gutter metadata for future avoidance/bundling

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

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
    pub ranks: HashMap<String, usize>,
}

/// Outer/inner bounds for a subgraph (outer includes gutter).
#[derive(Debug, Clone, Copy)]
pub struct SubgraphBounds {
    pub outer: Rect,
    pub inner: Rect,
}

fn subgraph_port_center(bounds: &SubgraphBounds, _title: Option<&str>) -> usize {
    bounds.outer.x + bounds.outer.width / 2
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

/// Preferred entry point for the coarse layout engine.
pub fn coarse_waterfall(graph: Graph) -> Result<Graph> {
    apply_coarse_layout(graph, None, CoarseLayoutConfig::default())
}

/// Backwards-compatible alias for callers expecting `waterfall`.
#[deprecated(note = "Use coarse_waterfall or layout_legacy::waterfall for the old engine")]
pub fn waterfall(graph: Graph) -> Result<Graph> {
    coarse_waterfall(graph)
}

/// Coarse layout engine entry point.
pub fn layout(input: LayoutInput, config: CoarseLayoutConfig) -> Result<LayoutOutput> {
    let coords = OrientedCoords::new(input.graph.direction);
    let debug_timing = std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok();

    // 1) Layer assignment (lenient Kahn) and ordering.
    let t_layers = std::time::Instant::now();
    let mut layers = assign_layers(input.graph);

    // 1.5) Optimize layer order to minimize crossings
    optimize_layer_order(input.graph, &mut layers);
    if debug_timing {
        eprintln!(
            "termiflow: layers {:?} ({} layers)",
            t_layers.elapsed(),
            layers.len()
        );
    }

    // 2) Place nodes on coarse grid.
    let t_place = std::time::Instant::now();
    let mut placement = place_nodes(
        input.graph,
        &layers,
        &coords,
        &config,
        input.prior_positions.as_ref(),
    );
    if debug_timing {
        eprintln!(
            "termiflow: placement {:?} (canvas {}x{})",
            t_place.elapsed(),
            placement.canvas.width,
            placement.canvas.height
        );
    }

    // 2.5) Flip coordinates for BT/RL to match flow direction
    // Calculate strict content bounds
    let max_x = placement
        .node_rects
        .values()
        .map(|r| r.right())
        .max()
        .unwrap_or(0);
    let max_y = placement
        .node_rects
        .values()
        .map(|r| r.bottom())
        .max()
        .unwrap_or(0);

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
    let subgraph_bounds =
        compute_subgraph_bounds(input.graph, &placement.node_rects, config.subgraph_gutter);

    // 4) Occupancy grid seeded with node padding and subgraph gutters (with carved portals).
    let t_grid = std::time::Instant::now();
    let mut grid = OccupancyGrid::new(
        placement.canvas.right() + config.min_horizontal_spacing + config.subgraph_gutter + 4,
        placement.canvas.bottom() + config.min_vertical_spacing + config.subgraph_gutter + 4,
    );
    for rect in placement.node_rects.values() {
        grid.mark_rect(&rect.inflate(config.node_padding));
    }
    carve_node_portals(
        &mut grid,
        &placement.node_rects,
        &coords,
        config.node_padding,
        input.graph,
        &subgraph_bounds,
    );
    mark_subgraph_rings(&mut grid, &subgraph_bounds);
    carve_subgraph_portals(
        &mut grid,
        &subgraph_bounds,
        &coords,
        config.subgraph_gutter,
        input.graph,
    );
    if debug_timing {
        eprintln!(
            "termiflow: grid {:?} ({}x{})",
            t_grid.elapsed(),
            grid.width,
            grid.height
        );
    }

    // 5) Route edges with Manhattan + obstacle avoidance.
    let mut routes: HashMap<usize, EdgeRoute> = HashMap::new();
    let warnings = Vec::new();
    let t_route = std::time::Instant::now();
    for (edge_idx, edge) in input.graph.edges.iter().enumerate() {
        if edge.is_back_edge {
            // Skip routing here; back-edges are handled by the cycle renderer.
            continue;
        }

        if debug_timing {
            eprintln!("termiflow: route edge {} -> {}", edge.from, edge.to);
        }
        let from_rect = placement
            .node_rects
            .get(&edge.from)
            .cloned()
            .unwrap_or_default();
        let to_rect = placement
            .node_rects
            .get(&edge.to)
            .cloned()
            .unwrap_or_default();

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

        if debug_timing {
            eprintln!(
                "  start {:?} end {:?} avoid {}",
                start,
                end,
                avoid_rects.len()
            );
        }

        // Ensure endpoints are traversable even if padding or rings marked them as obstacles.
        grid.clear_point(start);
        grid.clear_point(end);

        match route_with_obstacles(start, end, &mut grid, &avoid_rects, &coords) {
            Some(route) => {
                routes.insert(edge_idx, route);
            }
            None => {
                if let Some(route) = fallback_subgraph_route(
                    edge,
                    start,
                    end,
                    input.graph,
                    &subgraph_bounds,
                    debug_timing,
                ) {
                    routes.insert(edge_idx, route);
                } else {
                    let route = fallback_manhattan_route(start, end, input.graph.direction);
                    if debug_timing {
                        eprintln!(
                            "termiflow: direct fallback route {} -> {} via {} segments",
                            edge.from,
                            edge.to,
                            route.segments.len()
                        );
                    }
                    routes.insert(edge_idx, route);
                }
            }
        }
    }
    if debug_timing {
        eprintln!(
            "termiflow: routing {:?} ({} edges)",
            t_route.elapsed(),
            input.graph.edges.len()
        );
    }

    Ok(LayoutOutput {
        positions: placement.positions,
        subgraph_bounds,
        routes,
        canvas: placement.canvas,
        warnings,
        ranks: placement.ranks,
    })
}

/// Convenience helper: run the coarse layout and apply positions back to the graph.
pub fn apply_coarse_layout(
    mut graph: Graph,
    prior_positions: Option<HashMap<String, Point>>,
    config: CoarseLayoutConfig,
) -> Result<Graph> {
    let debug_timing = std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok();
    let t_start = std::time::Instant::now();

    // Ensure all nodes have valid dimensions before layout
    for node in graph.nodes.iter_mut() {
        if node.width == 0 {
            node.width = box_width(&node.label).max(BOX_MIN_WIDTH);
        }
    }

    // Detect cycles and mark back-edges so the renderer can add gutters.
    let has_cycles = mark_back_edges(&mut graph);
    if has_cycles {
        graph
            .warnings
            .push("termiflow: warning: Cycle detected, rendering back-edges in gutter".to_string());
    }

    let t_layout_start = std::time::Instant::now();
    let output = layout(
        LayoutInput {
            graph: &graph,
            prior_positions,
        },
        config,
    )?;
    if debug_timing {
        eprintln!("termiflow: layout core {:?}", t_layout_start.elapsed());
    }

    for node in graph.nodes.iter_mut() {
        if let Some(p) = output.positions.get(&node.id) {
            node.x = p.x;
            node.y = p.y;
        }
        if let Some(rank) = output.ranks.get(&node.id) {
            node.rank = *rank;
        }
    }

    for subgraph in graph.subgraphs.iter_mut() {
        if let Some(bounds) = output.subgraph_bounds.get(&subgraph.id) {
            subgraph.bounds = crate::graph::Rectangle::new(
                bounds.outer.x,
                bounds.outer.y,
                bounds.outer.width,
                bounds.outer.height,
            );
        }
    }

    if debug_timing {
        for (sg_id, bounds) in &output.subgraph_bounds {
            eprintln!(
                "subgraph {} outer=({}, {}, {}x{}) inner=({}, {}, {}x{})",
                sg_id,
                bounds.outer.x,
                bounds.outer.y,
                bounds.outer.width,
                bounds.outer.height,
                bounds.inner.x,
                bounds.inner.y,
                bounds.inner.width,
                bounds.inner.height
            );
        }
        for node in &graph.nodes {
            eprintln!(
                "node {} @ ({}, {}) size {}x{}",
                node.id, node.x, node.y, node.width, BOX_HEIGHT
            );
        }
    }

    graph.edge_routes = output.routes;

    for w in output.warnings {
        graph.warnings.push(w);
    }

    if debug_timing {
        for (idx, route) in &graph.edge_routes {
            eprintln!("termiflow: route {} segments {}", idx, route.segments.len());
        }
    }

    if debug_timing {
        eprintln!("termiflow: apply {:?}", t_start.elapsed());
    }

    Ok(graph)
}

/// Backwards-compatible alias for callers using the previous spike API.
#[deprecated(note = "Use apply_coarse_layout instead")]
pub fn apply_spike_layout(
    graph: Graph,
    prior_positions: Option<HashMap<String, Point>>,
    config: CoarseLayoutConfig,
) -> Result<Graph> {
    apply_coarse_layout(graph, prior_positions, config)
}

// -----------------------------------------------------------------------------
// Placement
// -----------------------------------------------------------------------------

/// Row spacing for simple edges without labels (minimal: stem → arrow)
const SPACING_MINIMAL: usize = 2;
/// Row spacing for labeled edges (stem → label → arrow)
const SPACING_LABELED: usize = 3;
/// Row spacing for fan-in (convergent) edges without labels (stems → junction → arrow)
const SPACING_FANIN: usize = 3;
/// Row spacing for fan-out (divergent) edges without labels (stem → junction → drops → arrows)
const SPACING_FANOUT: usize = 4;
/// Row spacing for multi-target edges with labels (stem → junction → label → arrow)
const SPACING_MULTI_LABELED: usize = 4;

#[derive(Debug)]
struct Placement {
    positions: HashMap<String, Point>,
    node_rects: HashMap<String, Rect>,
    canvas: Rect,
    ranks: HashMap<String, usize>,
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
    _coords: &OrientedCoords,
    config: &CoarseLayoutConfig,
) -> Vec<usize> {
    let mut gaps = Vec::with_capacity(layers.len());

    for (r, layer) in layers.iter().enumerate() {
        // Check fan-out: source (in this layer) has multiple targets
        let mut has_fan_out = false;
        for &idx in layer {
            let source_id = &graph.nodes[idx].id;
            let target_count = graph
                .edges
                .iter()
                .filter(|e| !e.is_back_edge && &e.from == source_id)
                .count();
            if target_count > 1 {
                has_fan_out = true;
                break;
            }
        }

        // Check fan-in: target (in next layer) has multiple sources from this layer
        let mut has_fan_in = false;
        if r + 1 < layers.len() {
            for &idx in &layers[r + 1] {
                let target_id = &graph.nodes[idx].id;
                let source_count = graph
                    .edges
                    .iter()
                    .filter(|e| {
                        !e.is_back_edge
                            && &e.to == target_id
                            && layer
                                .iter()
                                .any(|&src_idx| graph.nodes[src_idx].id == e.from)
                    })
                    .count();
                if source_count > 1 {
                    has_fan_in = true;
                    break;
                }
            }
        }

        // Check for labeled edges from this rank
        let has_labels = layer.iter().any(|&idx| {
            let source_id = &graph.nodes[idx].id;
            graph
                .edges
                .iter()
                .any(|e| !e.is_back_edge && &e.from == source_id && e.label.is_some())
        });

        // Detect fan-out that targets a single subgraph to allow tighter vertical spacing.
        let fanout_targets_same_subgraph = if has_fan_out {
            let mut subgraph_ids: HashSet<&str> = HashSet::new();
            for &idx in layer {
                let source_id = &graph.nodes[idx].id;
                for e in graph
                    .edges
                    .iter()
                    .filter(|e| !e.is_back_edge && &e.from == source_id)
                {
                    if let Some(sg) = graph.get_node_subgraph(&e.to) {
                        subgraph_ids.insert(sg);
                    } else {
                        subgraph_ids.insert("");
                    }
                }
            }
            subgraph_ids.len() == 1
        } else {
            false
        };

        // Priority matches layout.rs logic
        let mut spacing = if has_fan_out || has_fan_in {
            if has_labels {
                SPACING_MULTI_LABELED
            } else if has_fan_out {
                SPACING_FANOUT
            } else {
                SPACING_FANIN
            }
        } else if has_labels {
            SPACING_LABELED
        } else {
            SPACING_MINIMAL
        };

        if !graph.subgraphs.is_empty() && r + 1 < layers.len() {
            let mut boundary_crosses_subgraph = false;
            for &src_idx in layer {
                let src_id = &graph.nodes[src_idx].id;
                let src_sg = graph.get_node_subgraph(src_id);
                for &dst_idx in &layers[r + 1] {
                    let dst_id = &graph.nodes[dst_idx].id;
                    let dst_sg = graph.get_node_subgraph(dst_id);
                    if src_sg != dst_sg {
                        boundary_crosses_subgraph = true;
                        break;
                    }
                }
                if boundary_crosses_subgraph {
                    break;
                }
            }
            if boundary_crosses_subgraph {
                let extra = if fanout_targets_same_subgraph {
                    config.subgraph_gutter.max(1)
                } else {
                    config.subgraph_gutter * 2
                };
                spacing += extra;
            }
        }

        if fanout_targets_same_subgraph {
            spacing = spacing.saturating_sub(2).max(SPACING_MINIMAL);
        }

        gaps.push(spacing);
    }
    gaps
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
    let mut ranks: HashMap<String, usize> = HashMap::new();

    // 1. Calculate Primary Positions (Ranks)
    let primary_gaps = compute_primary_gaps(graph, layers, coords, config);

    // Compute primary offsets per layer (cumulative max extent + gap)
    let mut primary_offsets: Vec<usize> = Vec::with_capacity(layers.len());
    let mut primary_cursor = 0usize;
    for (i, layer) in layers.iter().enumerate() {
        let max_extent = layer
            .iter()
            .map(|idx| node_extent_primary(&graph.nodes[*idx], coords))
            .max()
            .unwrap_or(BOX_HEIGHT);

        primary_offsets.push(primary_cursor);

        let gap = if i < primary_gaps.len() {
            primary_gaps[i]
        } else {
            config.min_vertical_spacing
        };
        primary_cursor = primary_cursor + max_extent + gap;
    }

    let secondary_gap = gap_for_axis(coords.secondary, config);

    // 2. Calculate Secondary Positions (Barycenter / Median Alignment)
    for (layer_idx, layer) in layers.iter().enumerate() {
        let primary_pos = primary_offsets[layer_idx];
        let mut secondary_cursor = 0usize;

        for &node_idx in layer {
            let node = &graph.nodes[node_idx];
            let extent_sec = node_extent_secondary(node, coords);

            // Calculate desired secondary position based on parents (barycenter)
            let mut parent_centers = Vec::new();
            for edge in &graph.edges {
                if !edge.is_back_edge && edge.to == node.id {
                    if let Some(parent_rect) = node_rects.get(&edge.from) {
                        let center = match coords.secondary {
                            Axis::Horizontal => parent_rect.x + parent_rect.width / 2,
                            Axis::Vertical => parent_rect.y + parent_rect.height / 2,
                        };
                        parent_centers.push(center);
                    }
                }
            }

            let desired_center = if !parent_centers.is_empty() {
                let sum: usize = parent_centers.iter().sum();
                sum / parent_centers.len()
            } else {
                if let Some(prior) = prior_positions.as_ref().and_then(|m| m.get(&node.id)) {
                    match coords.secondary {
                        Axis::Horizontal => prior.x + node.width / 2,
                        Axis::Vertical => prior.y + BOX_HEIGHT / 2,
                    }
                } else {
                    0
                }
            };

            let desired_start = desired_center.saturating_sub(extent_sec / 2);
            let secondary_pos = desired_start.max(secondary_cursor);

            let mut x = 0usize;
            let mut y = 0usize;
            coords.set_primary(&mut x, &mut y, primary_pos);
            coords.set_secondary(&mut x, &mut y, secondary_pos);

            positions.insert(node.id.clone(), Point::new(x, y));
            node_rects.insert(node.id.clone(), Rect::new(x, y, node.width, BOX_HEIGHT));
            ranks.insert(node.id.clone(), layer_idx);

            secondary_cursor = secondary_pos + extent_sec + secondary_gap;
        }
    }

    // 3. Balance Coordinates (Iterative refinement)
    balance_coordinates(
        graph,
        &mut positions,
        &mut node_rects,
        layers,
        coords,
        config,
    );

    // Normalize coordinates (shift everything so min_x/min_y is 0)
    let min_x = node_rects.values().map(|r| r.x).min().unwrap_or(0);
    let min_y = node_rects.values().map(|r| r.y).min().unwrap_or(0);

    if min_x > 0 || min_y > 0 {
        for p in positions.values_mut() {
            p.x = p.x.saturating_sub(min_x);
            p.y = p.y.saturating_sub(min_y);
        }
        for r in node_rects.values_mut() {
            r.x = r.x.saturating_sub(min_x);
            r.y = r.y.saturating_sub(min_y);
        }
    }

    // Compute canvas bounds
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
        ranks,
    }
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
        if let (Some(&from_idx), Some(&to_idx)) = (
            index_map.get(edge.from.as_str()),
            index_map.get(edge.to.as_str()),
        ) {
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

fn mark_back_edges(graph: &mut Graph) -> bool {
    if graph.nodes.is_empty() || graph.edges.is_empty() {
        return false;
    }

    let mut index_map: HashMap<&str, usize> = HashMap::new();
    for (idx, node) in graph.nodes.iter().enumerate() {
        index_map.insert(&node.id, idx);
    }

    // Build adjacency with edge indices for DFS
    let mut adj: Vec<Vec<(usize, usize)>> = vec![Vec::new(); graph.nodes.len()];
    for (edge_idx, edge) in graph.edges.iter().enumerate() {
        let (Some(&from_idx), Some(&to_idx)) = (
            index_map.get(edge.from.as_str()),
            index_map.get(edge.to.as_str()),
        ) else {
            continue;
        };
        adj[from_idx].push((to_idx, edge_idx));
    }

    let mut state = vec![0u8; graph.nodes.len()]; // 0=unvisited,1=visiting,2=done
    let mut has_cycle = false;
    let mut seen_edges: HashSet<usize> = HashSet::new();

    fn dfs(
        u: usize,
        state: &mut [u8],
        adj: &[Vec<(usize, usize)>],
        edges: &mut [crate::graph::Edge],
        has_cycle: &mut bool,
        seen_edges: &mut HashSet<usize>,
    ) {
        state[u] = 1;
        for &(v, edge_idx) in &adj[u] {
            match state[v] {
                0 => dfs(v, state, adj, edges, has_cycle, seen_edges),
                1 => {
                    *has_cycle = true;
                    if seen_edges.insert(edge_idx) {
                        if let Some(edge) = edges.get_mut(edge_idx) {
                            edge.is_back_edge = true;
                        }
                    }
                }
                _ => {}
            }
        }
        state[u] = 2;
    }

    for u in 0..graph.nodes.len() {
        if state[u] == 0 {
            dfs(
                u,
                &mut state,
                &adj,
                &mut graph.edges,
                &mut has_cycle,
                &mut seen_edges,
            );
        }
    }

    has_cycle
}

// -----------------------------------------------------------------------------
// Crossing Minimization (Barycenter)
// -----------------------------------------------------------------------------

fn optimize_layer_order(graph: &Graph, layers: &mut Vec<Vec<usize>>) {
    // Run a few passes of barycenter minimization
    for _ in 0..4 {
        // Down sweep
        for i in 1..layers.len() {
            sort_layer(graph, layers, i, i - 1);
        }
        // Up sweep
        for i in (0..layers.len() - 1).rev() {
            sort_layer(graph, layers, i, i + 1);
        }
    }
}

fn sort_layer(graph: &Graph, layers: &mut [Vec<usize>], target_idx: usize, ref_idx: usize) {
    let ref_layer = layers[ref_idx].clone();
    let target_layer = &mut layers[target_idx];

    let barycenters = calculate_barycenters(graph, target_layer, &ref_layer);

    #[derive(Debug)]
    struct Cluster {
        nodes: Vec<usize>,
        avg_barycenter: f32,
    }

    let mut clusters: Vec<Cluster> = Vec::new();
    let mut subgraph_clusters: HashMap<String, usize> = HashMap::new();

    for &node_idx in target_layer.iter() {
        let node_id = &graph.nodes[node_idx].id;
        let sg_id = graph.get_node_subgraph(node_id);

        if let Some(sg) = sg_id {
            if let Some(&cluster_idx) = subgraph_clusters.get(sg) {
                clusters[cluster_idx].nodes.push(node_idx);
            } else {
                let idx = clusters.len();
                clusters.push(Cluster {
                    nodes: vec![node_idx],
                    avg_barycenter: 0.0,
                });
                subgraph_clusters.insert(sg.to_string(), idx);
            }
        } else {
            clusters.push(Cluster {
                nodes: vec![node_idx],
                avg_barycenter: 0.0,
            });
        }
    }

    for cluster in &mut clusters {
        let mut sum = 0.0;
        let mut count = 0.0;

        cluster.nodes.sort_by(|&a, &b| {
            let ba = barycenters.get(&a).copied().unwrap_or(f32::MAX);
            let bb = barycenters.get(&b).copied().unwrap_or(f32::MAX);
            ba.partial_cmp(&bb).unwrap_or(Ordering::Equal)
        });

        for &node_idx in &cluster.nodes {
            if let Some(&val) = barycenters.get(&node_idx) {
                sum += val;
                count += 1.0;
            }
        }

        cluster.avg_barycenter = if count > 0.0 { sum / count } else { f32::MAX };
    }

    clusters.sort_by(|a, b| {
        a.avg_barycenter
            .partial_cmp(&b.avg_barycenter)
            .unwrap_or(Ordering::Equal)
    });

    *target_layer = clusters.into_iter().flat_map(|c| c.nodes).collect();
}

fn calculate_barycenters(
    graph: &Graph,
    target_layer: &[usize],
    ref_layer: &[usize],
) -> HashMap<usize, f32> {
    let mut barycenters = HashMap::new();

    let ref_positions: HashMap<&str, usize> = ref_layer
        .iter()
        .enumerate()
        .map(|(i, &idx)| (graph.nodes[idx].id.as_str(), i))
        .collect();

    for &node_idx in target_layer {
        let node_id = &graph.nodes[node_idx].id;
        let mut sum = 0.0;
        let mut count = 0.0;

        for edge in &graph.edges {
            if edge.is_back_edge {
                continue;
            }

            let neighbor_id = if &edge.from == node_id {
                &edge.to
            } else if &edge.to == node_id {
                &edge.from
            } else {
                continue;
            };

            if let Some(&pos) = ref_positions.get(neighbor_id.as_str()) {
                sum += pos as f32;
                count += 1.0;
            }
        }

        if count > 0.0 {
            barycenters.insert(node_idx, sum / count);
        }
    }
    barycenters
}

// -----------------------------------------------------------------------------
// Coordinate Balancing
// -----------------------------------------------------------------------------

fn balance_coordinates(
    graph: &Graph,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    layers: &[Vec<usize>],
    coords: &OrientedCoords,
    config: &CoarseLayoutConfig,
) {
    for _ in 0..2 {
        for i in 1..layers.len() {
            apply_balance_pass(
                graph,
                positions,
                node_rects,
                &layers[i],
                &layers[0..i],
                coords,
                config,
                true,
            );
        }
        for i in (0..layers.len() - 1).rev() {
            apply_balance_pass(
                graph,
                positions,
                node_rects,
                &layers[i],
                &layers[i + 1..],
                coords,
                config,
                false,
            );
        }
    }
}

fn apply_balance_pass(
    graph: &Graph,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    target_layer: &[usize],
    ref_layers: &[Vec<usize>],
    coords: &OrientedCoords,
    config: &CoarseLayoutConfig,
    is_down_sweep: bool,
) {
    let gap = gap_for_axis(coords.secondary, config);
    let mut min_pos = 0usize;

    for &node_idx in target_layer {
        let node_id = &graph.nodes[node_idx].id;
        let node_width = match coords.secondary {
            Axis::Horizontal => graph.nodes[node_idx].width,
            Axis::Vertical => BOX_HEIGHT,
        };

        let mut sum_centers = 0.0;
        let mut count = 0.0;

        for layer in ref_layers {
            for &ref_idx in layer {
                let ref_id = &graph.nodes[ref_idx].id;

                let connected = if is_down_sweep {
                    graph
                        .edges
                        .iter()
                        .any(|e| !e.is_back_edge && &e.from == ref_id && &e.to == node_id)
                } else {
                    graph
                        .edges
                        .iter()
                        .any(|e| !e.is_back_edge && &e.from == node_id && &e.to == ref_id)
                };

                if connected {
                    if let Some(rect) = node_rects.get(ref_id) {
                        let center = match coords.secondary {
                            Axis::Horizontal => rect.x + rect.width / 2,
                            Axis::Vertical => rect.y + rect.height / 2,
                        };
                        sum_centers += center as f32;
                        count += 1.0;
                    }
                }
            }
        }

        if count > 0.0 {
            let ideal_center = (sum_centers / count) as usize;
            let ideal_start = ideal_center.saturating_sub(node_width / 2);

            let new_pos = ideal_start.max(min_pos);

            if let Some(p) = positions.get_mut(node_id) {
                match coords.secondary {
                    Axis::Horizontal => p.x = new_pos,
                    Axis::Vertical => p.y = new_pos,
                }
            }
            if let Some(r) = node_rects.get_mut(node_id) {
                match coords.secondary {
                    Axis::Horizontal => r.x = new_pos,
                    Axis::Vertical => r.y = new_pos,
                }
            }
            min_pos = new_pos + node_width + gap;
        } else {
            let current_pos = match coords.secondary {
                Axis::Horizontal => positions[node_id].x,
                Axis::Vertical => positions[node_id].y,
            };

            let new_pos = current_pos.max(min_pos);

            if new_pos != current_pos {
                if let Some(p) = positions.get_mut(node_id) {
                    match coords.secondary {
                        Axis::Horizontal => p.x = new_pos,
                        Axis::Vertical => p.y = new_pos,
                    }
                }
                if let Some(r) = node_rects.get_mut(node_id) {
                    match coords.secondary {
                        Axis::Horizontal => r.x = new_pos,
                        Axis::Vertical => r.y = new_pos,
                    }
                }
            }
            min_pos = new_pos + node_width + gap;
        }
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
        // Add a small internal padding so edges have breathing room inside the subgraph.
        let inner = inner.inflate(1);
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
            grid.mark_rect(&Rect::new(
                outer.x,
                outer.y,
                outer.width,
                inner.y.saturating_sub(outer.y),
            ));
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
    graph: &Graph,
    subgraph_bounds: &HashMap<String, SubgraphBounds>,
) {
    let ring_zones: Vec<&SubgraphBounds> = subgraph_bounds.values().collect();

    for (node_id, rect) in node_rects {
        let entry = edge_entry_point(*rect, coords.direction);
        let exit = edge_exit_point(*rect, coords.direction);

        let (allowed_rect, in_subgraph) = graph
            .get_node_subgraph(node_id)
            .and_then(|sg_id| subgraph_bounds.get(sg_id))
            .map(|b| (b.inner.inflate(padding.max(1)), true))
            .unwrap_or_else(|| (Rect::new(0, 0, grid.width, grid.height), false));

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
            if !in_subgraph {
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
                let entry_point = Point::new(ex, ey);
                let in_ring = ring_zones
                    .iter()
                    .any(|b| b.outer.contains(entry_point) && !b.inner.contains(entry_point));
                if allowed_rect.contains(entry_point) && !in_ring {
                    grid.clear_point(entry_point);
                }
            }

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
            let exit_point = Point::new(xx, xy);
            let in_ring = ring_zones
                .iter()
                .any(|b| b.outer.contains(exit_point) && !b.inner.contains(exit_point));
            if allowed_rect.contains(exit_point) && !in_ring {
                grid.clear_point(exit_point);
            }
        }
    }
}

fn carve_subgraph_portals(
    grid: &mut OccupancyGrid,
    subgraphs: &HashMap<String, SubgraphBounds>,
    coords: &OrientedCoords,
    gutter: usize,
    graph: &Graph,
) {
    #[derive(Default)]
    struct PortalSlots {
        top: HashSet<usize>,
        bottom: HashSet<usize>,
        left: HashSet<usize>,
        right: HashSet<usize>,
    }

    // Collect portal coordinates per subgraph based on edges crossing the boundary.
    let mut slots: HashMap<&str, PortalSlots> = HashMap::new();
    for edge in &graph.edges {
        let Some(from) = graph.get_node(&edge.from) else {
            continue;
        };
        let Some(to) = graph.get_node(&edge.to) else {
            continue;
        };

        let from_sg = graph.get_node_subgraph(&edge.from);
        let to_sg = graph.get_node_subgraph(&edge.to);

        if from_sg == to_sg {
            continue;
        }

        match coords.direction {
            Direction::TD | Direction::TB => {
                if let Some(id) = to_sg {
                    slots.entry(id).or_default().top.insert(to.center_x());
                }
                if let Some(id) = from_sg {
                    slots.entry(id).or_default().bottom.insert(from.center_x());
                }
            }
            Direction::BT => {
                if let Some(id) = to_sg {
                    slots.entry(id).or_default().bottom.insert(to.center_x());
                }
                if let Some(id) = from_sg {
                    slots.entry(id).or_default().top.insert(from.center_x());
                }
            }
            Direction::LR => {
                if let Some(id) = to_sg {
                    slots.entry(id).or_default().left.insert(to.center_y());
                }
                if let Some(id) = from_sg {
                    slots.entry(id).or_default().right.insert(from.center_y());
                }
            }
            Direction::RL => {
                if let Some(id) = to_sg {
                    slots.entry(id).or_default().right.insert(to.center_y());
                }
                if let Some(id) = from_sg {
                    slots.entry(id).or_default().left.insert(from.center_y());
                }
            }
        }
    }

    let span = gutter.max(1) * 2 + 1;
    for (sg_id, bounds) in subgraphs {
        let Some(portals) = slots.get(sg_id.as_str()) else {
            continue;
        };

        let clamp_h = |x: usize| {
            let min = bounds.outer.x.saturating_add(1);
            let max = bounds.outer.right().saturating_sub(2);
            x.clamp(min, max)
        };
        let clamp_v = |y: usize| {
            let min = bounds.outer.y.saturating_add(1);
            let max = bounds.outer.bottom().saturating_sub(2);
            y.clamp(min, max)
        };
        let half = span / 2;

        for &x in &portals.top {
            let cx = clamp_h(x);
            let start_x = cx.saturating_sub(half);
            let end_x = start_x + span;
            for y in bounds.outer.y..=bounds.inner.y {
                for xi in start_x..end_x {
                    grid.clear_point(Point::new(xi, y));
                }
            }
        }
        for &x in &portals.bottom {
            let cx = clamp_h(x);
            let start_x = cx.saturating_sub(half);
            let end_x = start_x + span;
            for y in bounds.inner.bottom()..=bounds.outer.bottom().saturating_sub(1) {
                for xi in start_x..end_x {
                    grid.clear_point(Point::new(xi, y));
                }
            }
        }
        for &y in &portals.left {
            let cy = clamp_v(y);
            let start_y = cy.saturating_sub(half);
            let end_y = start_y + span;
            for x in bounds.outer.x..=bounds.inner.x {
                for yi in start_y..end_y {
                    grid.clear_point(Point::new(x, yi));
                }
            }
        }
        for &y in &portals.right {
            let cy = clamp_v(y);
            let start_y = cy.saturating_sub(half);
            let end_y = start_y + span;
            for x in bounds.inner.right()..=bounds.outer.right().saturating_sub(1) {
                for yi in start_y..end_y {
                    grid.clear_point(Point::new(x, yi));
                }
            }
        }

        if debug_timing {
            eprintln!(
                "subgraph {} portals top={} bottom={} left={} right={}",
                sg_id,
                portals.top.len(),
                portals.bottom.len(),
                portals.left.len(),
                portals.right.len()
            );
        }
    }
}

fn subgraph_ports(
    bounds: &SubgraphBounds,
    direction: Direction,
    title: Option<&str>,
) -> (Point, Point) {
    let cx = subgraph_port_center(bounds, title);
    let cy = bounds.outer.y + bounds.outer.height / 2;
    match direction {
        Direction::TD | Direction::TB => (
            Point::new(cx, bounds.outer.y.saturating_add(1)),
            Point::new(cx, bounds.outer.bottom().saturating_sub(2)),
        ),
        Direction::BT => (
            Point::new(cx, bounds.outer.bottom().saturating_sub(2)),
            Point::new(cx, bounds.outer.y.saturating_add(1)),
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

// -----------------------------------------------------------------------------
// Routing
// -----------------------------------------------------------------------------

const WEIGHT_FREE: u8 = 1;
const WEIGHT_EDGE: u8 = 10;
const WEIGHT_OBSTACLE: u8 = 255;
const COST_BEND: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    fn from_vec(dx: isize, dy: isize) -> Option<Self> {
        match (dx, dy) {
            (0, -1) => Some(Dir::Up),
            (0, 1) => Some(Dir::Down),
            (-1, 0) => Some(Dir::Left),
            (1, 0) => Some(Dir::Right),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
struct OccupancyGrid {
    width: usize,
    height: usize,
    weights: Vec<u8>,
}

impl OccupancyGrid {
    fn new(width: usize, height: usize) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            width,
            height,
            weights: vec![WEIGHT_FREE; width * height],
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
                self.weights[row_offset + x] = WEIGHT_OBSTACLE;
            }
        }
    }

    fn clear_point(&mut self, p: Point) {
        if self.in_bounds(p) {
            let idx = self.idx(p.x, p.y);
            self.weights[idx] = WEIGHT_FREE;
        }
    }

    fn cost_at(&self, p: Point) -> u8 {
        if !self.in_bounds(p) {
            return WEIGHT_OBSTACLE;
        }
        self.weights[self.idx(p.x, p.y)]
    }

    fn mark_path(&mut self, route: &EdgeRoute) {
        for seg in &route.segments {
            // Determine direction and range
            if seg.from.x == seg.to.x {
                // Vertical
                let (min_y, max_y) = if seg.from.y < seg.to.y {
                    (seg.from.y, seg.to.y)
                } else {
                    (seg.to.y, seg.from.y)
                };
                for y in min_y..=max_y {
                    if y < self.height {
                        let idx = self.idx(seg.from.x, y);
                        // Don't overwrite hard obstacles, but do overwrite free/edge
                        if self.weights[idx] != WEIGHT_OBSTACLE {
                            self.weights[idx] = WEIGHT_EDGE;
                        }
                    }
                }
            } else {
                // Horizontal
                let (min_x, max_x) = if seg.from.x < seg.to.x {
                    (seg.from.x, seg.to.x)
                } else {
                    (seg.to.x, seg.from.x)
                };
                for x in min_x..=max_x {
                    if x < self.width {
                        let idx = self.idx(x, seg.from.y);
                        if self.weights[idx] != WEIGHT_OBSTACLE {
                            self.weights[idx] = WEIGHT_EDGE;
                        }
                    }
                }
            }
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
struct PathNode {
    cost: usize,
    estimate: usize,
    point: Point,
    arrival_dir: Option<Dir>,
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

fn add_manhattan_segment(route: &mut EdgeRoute, from: Point, to: Point, direction: Direction) {
    if from == to {
        return;
    }
    if from.x == to.x || from.y == to.y {
        route.push_segment(from, to);
        return;
    }

    let mid = match direction {
        Direction::TD | Direction::TB | Direction::BT => Point::new(to.x, from.y),
        Direction::LR | Direction::RL => Point::new(from.x, to.y),
    };
    route.push_segment(from, mid);
    route.push_segment(mid, to);
}

fn fallback_subgraph_route(
    edge: &crate::graph::Edge,
    start: Point,
    end: Point,
    graph: &Graph,
    subgraph_bounds: &HashMap<String, SubgraphBounds>,
    debug_timing: bool,
) -> Option<EdgeRoute> {
    let from_sg = graph.get_node_subgraph(&edge.from);
    let to_sg = graph.get_node_subgraph(&edge.to);
    if from_sg.is_none() && to_sg.is_none() {
        return None;
    }

    let mut points = Vec::new();
    points.push(start);

    if let Some(sg_id) = from_sg {
        if let Some(bounds) = subgraph_bounds.get(sg_id) {
            let title = graph.get_subgraph(sg_id).and_then(|sg| sg.title.as_deref());
            let (_entry, exit) = subgraph_ports(bounds, graph.direction, title);
            points.push(exit);
        }
    }

    if let Some(sg_id) = to_sg {
        if let Some(bounds) = subgraph_bounds.get(sg_id) {
            let title = graph.get_subgraph(sg_id).and_then(|sg| sg.title.as_deref());
            let (entry, _exit) = subgraph_ports(bounds, graph.direction, title);
            points.push(entry);
        }
    }

    points.push(end);
    points.dedup();

    if points.len() < 2 {
        return None;
    }

    let mut route = EdgeRoute::new();
    for pair in points.windows(2) {
        add_manhattan_segment(&mut route, pair[0], pair[1], graph.direction);
    }

    if debug_timing {
        eprintln!(
            "termiflow: fallback subgraph route {} -> {} via {} segments",
            edge.from,
            edge.to,
            route.segments.len()
        );
    }

    Some(route)
}

fn fallback_manhattan_route(start: Point, end: Point, direction: Direction) -> EdgeRoute {
    let mut route = EdgeRoute::new();
    add_manhattan_segment(&mut route, start, end, direction);
    route
}

fn route_with_obstacles(
    start: Point,
    end: Point,
    grid: &mut OccupancyGrid,
    avoid_rects: &[Rect],
    coords: &OrientedCoords,
) -> Option<EdgeRoute> {
    let debug_timing = std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok();
    if start == end {
        let mut route = EdgeRoute::new();
        route.push_segment(start, end);
        return Some(route);
    }

    let mut came_from: HashMap<Point, Point> = HashMap::new();
    let mut best_cost: HashMap<(Point, Option<Dir>), usize> = HashMap::new();
    let mut open = BinaryHeap::new();

    open.push(PathNode {
        cost: 0,
        estimate: manhattan(start, end),
        point: start,
        arrival_dir: None,
    });

    // Initial cost for start point (any direction)
    best_cost.insert((start, None), 0);

    let mut found_end = false;
    let mut steps: usize = 0;
    let max_steps = grid
        .width
        .saturating_mul(grid.height)
        .saturating_mul(10)
        .max(10_000);

    while let Some(current) = open.pop() {
        steps += 1;
        if steps > max_steps {
            eprintln!(
                "termiflow: warning: routing aborted after {} steps ({:?} -> {:?})",
                steps, start, end
            );
            break;
        }
        if debug_timing && steps % 500 == 0 {
            eprintln!(
                "    routing step {} at {:?} (open={})",
                steps,
                current.point,
                open.len()
            );
        }
        if current.point == end {
            found_end = true;
            break;
        }

        let neighbors = ordered_neighbors(current.point, end, coords);
        if debug_timing && steps <= 1 {
            for next in &neighbors {
                let cost = grid.cost_at(*next);
                let blocked = avoid_rects.iter().any(|r| r.contains(*next));
                eprintln!(
                    "    neighbor {:?} cost={} blocked_by_rect={}",
                    next, cost, blocked
                );
            }
        }
        for next in neighbors {
            // Check hard obstacles (rects)
            if avoid_rects.iter().any(|r| r.contains(next)) && next != end {
                continue;
            }

            // Check grid cost
            let cell_cost = grid.cost_at(next);
            if cell_cost == WEIGHT_OBSTACLE && next != end {
                continue;
            }

            // Calculate movement direction
            let dx = next.x as isize - current.point.x as isize;
            let dy = next.y as isize - current.point.y as isize;
            let move_dir = Dir::from_vec(dx, dy);

            // Calculate new cost
            let mut new_cost = current.cost + cell_cost as usize;

            // Add bend penalty
            if let (Some(prev), Some(curr)) = (current.arrival_dir, move_dir) {
                if prev != curr {
                    new_cost += COST_BEND;
                }
            }

            let key = (next, move_dir);
            let known = best_cost.get(&key).copied().unwrap_or(usize::MAX);

            if new_cost < known {
                best_cost.insert(key, new_cost);
                came_from.insert(next, current.point);
                open.push(PathNode {
                    cost: new_cost,
                    estimate: manhattan(next, end),
                    point: next,
                    arrival_dir: move_dir,
                });
            }
        }
    }

    if !found_end {
        if debug_timing {
            eprintln!("    routing failed after {} steps", steps);
        }
        return None;
    }

    if debug_timing {
        eprintln!("    routing succeeded after {} steps", steps);
    }

    let mut path: Vec<Point> = Vec::new();
    let mut current = end;
    path.push(current);
    let mut visited: HashSet<Point> = HashSet::new();
    visited.insert(current);
    while let Some(prev) = came_from.get(&current) {
        if !visited.insert(*prev) {
            break;
        }
        current = *prev;
        path.push(current);
        if current == start {
            break;
        }
    }
    path.reverse();

    let route = compress_path(&path);

    // Mark the successful route on the grid to repel future edges
    grid.mark_path(&route);

    Some(route)
}

fn ordered_neighbors(current: Point, goal: Point, coords: &OrientedCoords) -> Vec<Point> {
    let dx = goal.x as isize - current.x as isize;
    let dy = goal.y as isize - current.y as isize;

    let primary_first = if coords.primary == Axis::Horizontal {
        vec![
            (dx.signum(), 0),
            (0, dy.signum()),
            (-dx.signum(), 0),
            (0, -dy.signum()),
        ]
    } else {
        vec![
            (0, dy.signum()),
            (dx.signum(), 0),
            (0, -dy.signum()),
            (-dx.signum(), 0),
        ]
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
        let next = Point::new(nx, ny);
        if next != current {
            neighbors.push(next);
        }
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
        let dir = (b.x as isize - a.x as isize, b.y as isize - a.y as isize);
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
        Direction::TD | Direction::TB => {
            Point::new(rect.x + rect.width / 2, rect.y.saturating_sub(1))
        }
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
    use crate::parser::parse;

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

    #[test]
    fn routes_cross_subgraph_boundaries() {
        let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_outside_td.md")
            .expect("read fixture");
        let parsed = parse(&input, false).expect("parse");
        let graph =
            apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

        if let Some(sg) = graph.subgraphs.first() {
            let _ = sg; // keep test quiet
        }

        // Expect routes for edges entering and exiting the subgraph.
        // Edge order in fixture: A->B (0), Start->A (1), B->End (2)
        for edge_idx in [1usize, 2usize] {
            let route = graph
                .edge_routes
                .get(&edge_idx)
                .unwrap_or_else(|| panic!("missing route for edge {}", edge_idx));
            assert!(
                !route.segments.is_empty(),
                "route {} should have segments",
                edge_idx
            );
            // Debug aid if this test ever fails
            for seg in &route.segments {
                eprintln!(
                    "edge {} segment from ({},{}) to ({},{})",
                    edge_idx, seg.from.x, seg.from.y, seg.to.x, seg.to.y
                );
            }
        }
    }

    #[test]
    fn marks_back_edges_and_leaves_cycle_routing_to_renderer() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;
        graph.nodes.push(Node::new("A", "A"));
        graph.nodes.push(Node::new("B", "B"));
        graph.edges.push(Edge::new("A", "B"));
        graph.edges.push(Edge::new("B", "A")); // back-edge creates a cycle

        let laid_out =
            apply_coarse_layout(graph, None, CoarseLayoutConfig::default()).expect("layout");

        assert!(laid_out.has_cycles(), "graph should be marked cyclic");
        assert!(
            laid_out.edges[1].is_back_edge,
            "back-edge should be flagged"
        );
        assert!(
            !laid_out.edges[0].is_back_edge,
            "forward edge should not be flagged"
        );
        // Only the forward edge should have a precomputed route; back-edges are rendered via the cycle gutter.
        assert!(
            laid_out.edge_routes.contains_key(&0),
            "forward edge should be routed"
        );
        assert!(
            !laid_out.edge_routes.contains_key(&1),
            "back-edge routing should be deferred to renderer"
        );
    }
}
