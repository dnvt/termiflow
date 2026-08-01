//! Layered node placement and spacing policy.

use std::collections::{HashMap, HashSet};

use crate::geom::{Point, Rect};
use crate::graph::{Direction, Graph};
use crate::orientation::{Axis, OrientedCoords};
use crate::style::BOX_HEIGHT;

use super::optimization::{balance_coordinates, node_extent_primary, node_extent_secondary};
use super::reserve_titled_horizontal_subgraph_headroom;
use super::CoarseLayoutConfig;

// Placement
// -----------------------------------------------------------------------------

/// Row spacing for simple edges without labels (minimal: stem → arrow)
const SPACING_MINIMAL: usize = 2;
/// Row spacing for labeled edges (stem → label → arrow)
const SPACING_LABELED: usize = 3;
/// Row spacing for fan-in (convergent) edges without labels (stems → junction → arrow)
const SPACING_FANIN: usize = 3;
/// Row spacing for fan-out (divergent) edges without labels (stem → junction → drops → arrows)
const SPACING_FANOUT: usize = 1;
/// Row spacing for multi-target edges with labels (stem → junction → label → arrow)
const SPACING_MULTI_LABELED: usize = 4;

#[derive(Debug)]
pub(super) struct Placement {
    pub(super) positions: HashMap<String, Point>,
    pub(super) node_rects: HashMap<String, Rect>,
    pub(super) canvas: Rect,
    pub(super) ranks: HashMap<String, usize>,
}

pub(super) fn gap_for_axis(axis: Axis, cfg: &CoarseLayoutConfig) -> usize {
    match axis {
        Axis::Horizontal => cfg.min_horizontal_spacing,
        Axis::Vertical => cfg.min_vertical_spacing,
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct LayoutSpacingPolicy {
    gutter: usize,
    node_padding: usize,
    min_horizontal: usize,
    min_vertical: usize,
}

impl LayoutSpacingPolicy {
    fn new(gutter: usize, node_padding: usize, min_horizontal: usize, min_vertical: usize) -> Self {
        Self {
            gutter,
            node_padding,
            min_horizontal,
            min_vertical,
        }
    }

    fn spacing_for_layer(&self, graph: &Graph, layers: &[Vec<usize>], layer_idx: usize) -> usize {
        let layer = &layers[layer_idx];

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
        if layer_idx + 1 < layers.len() {
            for &idx in &layers[layer_idx + 1] {
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

        let external_boundary_target_count = if has_fan_out && layer_idx + 1 < layers.len() {
            let mut targets: HashSet<&str> = HashSet::new();
            for &src_idx in layer {
                let source_id = &graph.nodes[src_idx].id;
                let source_sg = graph.get_node_subgraph(source_id);
                for &dst_idx in &layers[layer_idx + 1] {
                    let target_id = &graph.nodes[dst_idx].id;
                    let target_sg = graph.get_node_subgraph(target_id);
                    if source_sg == target_sg {
                        continue;
                    }
                    if graph.edges.iter().any(|edge| {
                        !edge.is_back_edge && edge.from == *source_id && edge.to == *target_id
                    }) {
                        targets.insert(target_id.as_str());
                    }
                }
            }
            targets.len()
        } else {
            0
        };

        // Base spacing by flow shape
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

        // When a boundary simultaneously contains fan-out and fan-in (diamond-ish shapes),
        // keep extra rows/cols so merge/junction bars don't collide with boxes.
        if has_fan_out && has_fan_in && !has_labels {
            spacing = spacing.max(SPACING_FANIN + 1);
        }

        // Subgraph boundary inflation between this layer and the next
        let mut boundary_crosses_subgraph = false;
        let mut crossing_into_titled_subgraph = false;
        if !graph.subgraphs.is_empty() && layer_idx + 1 < layers.len() {
            for &src_idx in layer {
                let src_id = &graph.nodes[src_idx].id;
                let src_sg = graph.get_node_subgraph(src_id);
                for &dst_idx in &layers[layer_idx + 1] {
                    let dst_id = &graph.nodes[dst_idx].id;
                    let dst_sg = graph.get_node_subgraph(dst_id);
                    if src_sg != dst_sg {
                        boundary_crosses_subgraph = true;
                        if let Some(sg_id) = dst_sg {
                            if let Some(sg) = graph.get_subgraph(sg_id) {
                                if let Some(title) = sg.title.as_ref() {
                                    // Rough fit check: the title text should fit inside the widest node plus modest padding.
                                    let title_len = crate::graph::subgraph_title_len(title);
                                    let widest_node = graph
                                        .nodes
                                        .iter()
                                        .filter(|n| sg.contains_node(&n.id))
                                        .map(|n| n.width)
                                        .max()
                                        .unwrap_or(0);
                                    if title_len <= widest_node.saturating_add(6) {
                                        crossing_into_titled_subgraph = true;
                                    }
                                }
                            }
                        }
                        break;
                    }
                }
                if boundary_crosses_subgraph {
                    break;
                }
            }

            if boundary_crosses_subgraph {
                if !has_fan_out && !has_fan_in && !has_labels {
                    // Leave a visible connector row plus an arrow head before the next node.
                    spacing = if crossing_into_titled_subgraph {
                        if matches!(graph.direction, Direction::TD | Direction::TB) {
                            SPACING_MINIMAL + 1
                        } else {
                            SPACING_MINIMAL + 2
                        }
                    } else {
                        SPACING_MINIMAL + 1
                    };
                } else {
                    let extra = if fanout_targets_same_subgraph {
                        self.gutter.saturating_sub(1)
                    } else if has_fan_out && has_fan_in {
                        self.gutter
                    } else if has_fan_out {
                        self.gutter * 2
                    } else {
                        self.gutter
                    };
                    spacing += extra;
                    spacing = spacing.max(SPACING_MINIMAL + 2);
                }

                // Fan-outs into a single subgraph can be tighter because the subgraph
                // itself reserves internal rows for trunk/split/drop rendering.
                if has_fan_out {
                    if fanout_targets_same_subgraph {
                        spacing = spacing.max(SPACING_MINIMAL + 2);
                    } else if external_boundary_target_count <= 1
                        && matches!(graph.direction, Direction::TD | Direction::TB)
                    {
                        // Mixed fan-outs that only pierce one sibling boundary do not need the
                        // full oversized cross-subgraph gap. The destination subgraph already
                        // reserves its own entry rows, so keep this boundary compact.
                        spacing = spacing.max(SPACING_MINIMAL + 3);
                    } else {
                        spacing = spacing.max(SPACING_MINIMAL + 5);
                    }
                }
            }
        }

        if boundary_crosses_subgraph && has_labels && !has_fan_out && !has_fan_in {
            spacing = spacing.saturating_sub(2).max(SPACING_LABELED + 1);
        }

        if fanout_targets_same_subgraph {
            // Leave a modest cushion for the junction row while keeping fan-outs compact.
            spacing = spacing.max(SPACING_FANOUT + 2);
        }

        // Horizontal layouts need a bit more primary gap for fan-outs to give
        // elbows/dashes room before hitting the targets.
        if matches!(graph.direction, Direction::LR | Direction::RL) && has_fan_out {
            spacing = spacing.max(SPACING_FANOUT + 4);
        }

        // Aspect ratio compensation for LR/RL layouts.
        // Terminal characters are ~2:1 height:width ratio, so horizontal layouts
        // need proportionally more spacing along the primary (horizontal) axis.
        // For complex topologies (fan-out, fan-in, labels) we apply a 2x multiplier.
        // For simple chains we honour the configured minimum horizontal spacing, which
        // already encodes the 2x compensation via SpacingConfig::for_direction.
        if matches!(graph.direction, Direction::LR | Direction::RL) {
            if !has_fan_out && !has_fan_in && !has_labels {
                spacing = self.min_horizontal.max(spacing * 2);
            } else {
                spacing *= 2;
            }
        }

        spacing
    }
}

fn compute_primary_gaps(
    graph: &Graph,
    layers: &[Vec<usize>],
    _coords: &OrientedCoords,
    config: &CoarseLayoutConfig,
) -> Vec<usize> {
    let mut gaps = Vec::with_capacity(layers.len());
    let policy = LayoutSpacingPolicy::new(
        config.subgraph_gutter,
        config.node_padding,
        config.min_horizontal_spacing,
        config.min_vertical_spacing,
    );
    for r in 0..layers.len() {
        gaps.push(policy.spacing_for_layer(graph, layers, r));
    }
    gaps
}

pub(super) fn place_nodes(
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

            let has_incoming = graph
                .edges
                .iter()
                .any(|e| !e.is_back_edge && e.to == node.id);

            if std::env::var("DEBUG_FANIN").is_ok() && node.id == "Merge" {
                eprintln!(
                    "layout fanin node={} parents={:?} incoming_edges={}",
                    node.id, parent_centers, has_incoming
                );
            }

            let prior_center = prior_positions
                .as_ref()
                .and_then(|positions| positions.get(&node.id))
                .map(|prior| match coords.secondary {
                    Axis::Horizontal => prior.x + node.width / 2,
                    Axis::Vertical => prior.y + node.height / 2,
                });

            let desired_center = if let Some(prior_center) = prior_center {
                // Repair candidates are explicit secondary-position nudges.
                // Honor them even for nodes with parents; otherwise the
                // barycenter below erases every branch-recenter candidate
                // before the renderer can evaluate it.
                prior_center
            } else if !parent_centers.is_empty() {
                let sum: usize = parent_centers.iter().sum();
                sum / parent_centers.len()
            } else if has_incoming && layer_idx > 0 {
                // Fall back to centering on the preceding layer when parents exist
                // but haven't been placed (e.g., subgraph portal alignment).
                let mut prev_centers = Vec::new();
                for &prev_idx in &layers[layer_idx - 1] {
                    if let Some(rect) = node_rects.get(&graph.nodes[prev_idx].id) {
                        let center = match coords.secondary {
                            Axis::Horizontal => rect.x + rect.width / 2,
                            Axis::Vertical => rect.y + rect.height / 2,
                        };
                        prev_centers.push(center);
                    }
                }

                if !prev_centers.is_empty() {
                    let sum: usize = prev_centers.iter().sum();
                    sum / prev_centers.len()
                } else {
                    0
                }
            } else {
                0
            };

            let desired_start = desired_center.saturating_sub(extent_sec / 2);
            let secondary_pos = desired_start.max(secondary_cursor);

            if std::env::var("DEBUG_FANIN").is_ok() && node.id == "Merge" {
                eprintln!(
                    "place {} desired_center={} extent={} start={} cursor={} -> pos={}",
                    node.id,
                    desired_center,
                    extent_sec,
                    desired_start,
                    secondary_cursor,
                    secondary_pos
                );
            }

            let mut x = 0usize;
            let mut y = 0usize;
            coords.set_primary(&mut x, &mut y, primary_pos);
            coords.set_secondary(&mut x, &mut y, secondary_pos);

            positions.insert(node.id.clone(), Point::new(x, y));
            node_rects.insert(node.id.clone(), Rect::new(x, y, node.width, node.height));
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
        prior_positions,
    );

    if std::env::var("DEBUG_FANIN").is_ok() {
        if let Some(rect) = node_rects.get("Merge") {
            eprintln!("post-balance Merge rect {:?}", rect);
        }
        if let Some(rect) = node_rects.get("S1") {
            eprintln!("post-balance S1 rect {:?}", rect);
        }
    }

    // Normalize coordinates (shift everything so min_x/min_y is 0)
    let min_x = node_rects.values().map(|r| r.x).min().unwrap_or(0);
    let min_y = node_rects.values().map(|r| r.y).min().unwrap_or(0);

    if std::env::var("DEBUG_FANIN").is_ok() {
        eprintln!("normalize min_x={} min_y={}", min_x, min_y);
    }

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

    let mut post_normalize_canvas_height =
        node_rects.values().map(|r| r.bottom()).max().unwrap_or(0);

    reserve_titled_horizontal_subgraph_headroom(
        graph,
        &mut positions,
        &mut node_rects,
        config.subgraph_gutter,
        &mut post_normalize_canvas_height,
    );

    if std::env::var("DEBUG_FANIN").is_ok() {
        if let Some(rect) = node_rects.get("Merge") {
            eprintln!("post-normalize Merge rect {:?}", rect);
        }
        if let Some(rect) = node_rects.get("S1") {
            eprintln!("post-normalize S1 rect {:?}", rect);
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
