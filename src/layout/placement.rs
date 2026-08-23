//! Layered node placement and spacing policy.

use std::collections::{HashMap, HashSet};

use crate::geom::{Point, Rect};
use crate::graph::{Direction, Graph, NodeShape};
use crate::orientation::{Axis, OrientedCoords};
use crate::render::fan_in_identity;
use crate::render::sibling_subgraph_fan_in_identity;
use crate::render::subgraph_fan_in_identity;
use crate::render::wide_terminal_fan_in;
use crate::style::BOX_HEIGHT;

use super::dense_pipeline;
use super::dual_junction::{
    balance_dual_junctions, vertical_fanout_requires_headroom,
    vertical_mixed_edge_kind_fanout_requires_headroom,
};
use super::optimization::{balance_coordinates, node_extent_primary, node_extent_secondary};
use super::pure_fan_in::balance_pure_fan_in_targets;
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
const DENSE_EDGE_COUNT: usize = 6;
const DENSE_LANE_PITCH: usize = 2;
const DENSE_HORIZONTAL_COMPRESSION: usize = 2;
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
        let dense_horizontal = is_dense_horizontal_graph(graph, layers);

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

        // Exact dual-junction fan-outs need one extra primary cell between
        // the shared anchor and the outgoing arrow entry. Without this local
        // headroom, the generic branch route has room for the junction bar
        // but no visible shaft before the arrowhead. Keep the surcharge
        // topology-gated so ordinary fan-outs retain their established
        // compact geometry.
        if vertical_fanout_requires_headroom(graph, layers, layer_idx) {
            spacing = spacing.max(SPACING_FANOUT + 3);
        }

        // Thick and Dotted branches need a writable target-facing shaft cell
        // in compact vertical mixed fan-outs. Reserve that local row before
        // the renderer's shared junction span is projected.
        if vertical_mixed_edge_kind_fanout_requires_headroom(graph, layers, layer_idx) {
            spacing = spacing.max(SPACING_FANOUT + 4);
        }

        // Database/Cylinder targets need one additional primary cell because
        // the renderer's shape-owned entry policy places their arrowhead one
        // cell farther from the contour. Keep this local to the preceding
        // rank so ordinary rectangles and unrelated ranks retain compact
        // spacing.
        if database_intermediate_scene_requires_headroom(graph, layers, layer_idx) {
            // The strict source→cache→database diamond needs one more primary
            // cell than a single database entry: the source-owned tee now
            // branches after a visible stem, while the intermediate receiver
            // still needs a clean shaft cell with no side-axis route beside
            // its arrowhead.
            spacing = spacing.max(SPACING_MINIMAL + 2);
        } else if database_target_requires_headroom(graph, layers, layer_idx) {
            spacing = spacing.max(SPACING_MINIMAL + 1);
        }

        // Vertical diamond targets consume one additional primary cell before
        // their arrow entry. Reserve that corridor locally so the arrowhead
        // cannot collapse onto the source contour. Horizontal layouts already
        // receive their direction-specific primary-axis multiplier; keep this
        // surcharge vertical-only to avoid perturbing established LR/RL
        // spacing.
        if vertical_diamond_target_requires_headroom(graph, layers, layer_idx) {
            spacing = spacing.max(SPACING_MINIMAL + 1);
        }

        // When a boundary simultaneously contains fan-out and fan-in (diamond-ish shapes),
        // keep extra rows/cols so merge/junction bars don't collide with boxes.
        if has_fan_out && has_fan_in && !has_labels {
            spacing = spacing.max(SPACING_FANIN + 1);
        }

        // Wide terminal fan-in is lowered with one independent horizontal
        // channel per source. Reserve the exact corridor consumed by that
        // proof-gated renderer; ordinary convergence keeps its compact gap.
        if matches!(graph.direction, Direction::TD | Direction::BT) {
            if let Some(count) = self.wide_terminal_fan_in_count(graph, layers, layer_idx) {
                spacing = spacing.max(wide_terminal_fan_in::required_primary_gap(count));
            }
            if let Some(count) = self.identity_fan_in_count(graph, layers, layer_idx) {
                spacing = spacing.max(fan_in_identity::required_primary_gap(count));
            }
            if let Some(count) = self.subgraph_identity_fan_in_count(graph, layers, layer_idx) {
                spacing = spacing.max(subgraph_fan_in_identity::required_primary_gap(count));
            }
            if let Some(count) =
                self.sibling_subgraph_identity_fan_in_count(graph, layers, layer_idx)
            {
                spacing = spacing.max(sibling_subgraph_fan_in_identity::required_primary_gap(
                    count,
                ));
            }
        }

        // Dense crossing fan-ins need one independent merge lane per target
        // group.  The fallback convergence renderer cannot preserve edge
        // identity when several overlapping target spans are forced onto one
        // row/column, so reserve only the minimum additional primary gap for
        // that shape.  Ordinary fan-in and non-overlapping fan-outs keep the
        // existing compact spacing policy.
        let dense_fan_in_lanes = self.overlapping_fan_in_lane_count(graph, layers, layer_idx);
        if dense_fan_in_lanes > 1 {
            spacing = spacing.max(SPACING_FANIN + dense_fan_in_lanes);
        }

        // A small dense bipartite rank pair is lowered by the renderer as six
        // independent routes rather than one shared collector. Reserve two
        // additional primary cells so every route can receive a distinct
        // lane without borrowing the source/target attachment rows. The
        // topology test is structural and intentionally independent of fixture
        // names or rendered glyphs.
        if dense_crossing_pair(graph, layers, layer_idx) {
            let dense_primary_gap = DENSE_EDGE_COUNT
                .saturating_mul(DENSE_LANE_PITCH)
                .saturating_add(2);
            if matches!(
                graph.direction,
                Direction::TD | Direction::TB | Direction::BT
            ) {
                spacing = spacing.max(dense_primary_gap);
            } else {
                spacing = spacing.max(SPACING_FANIN + dense_fan_in_lanes + 2);
            }
        }

        // The layered dense pipeline has six singleton bridge edges whose
        // source and target ranks otherwise leave only a border-adjacent cell
        // for a turn. Reserve one additional primary cell for a visible stem
        // and corridor; the scene detector remains topology-only and
        // fail-closed for all near-miss graphs.
        if dense_pipeline::needs_bridge_headroom(graph, layers, layer_idx) {
            spacing = spacing.max(SPACING_MINIMAL + 1);
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
                    } else if has_fan_out
                        && external_boundary_target_count <= 1
                        && matches!(graph.direction, Direction::TD | Direction::TB)
                    {
                        // A single external target already contributes local database or
                        // shape headroom above. Do not stack the full multi-boundary fan-out
                        // surcharge on that rank; the destination subgraph reserves the
                        // connector corridor itself.
                        self.gutter
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

        // Multiple external entries into a titled vertical subgraph need a
        // dedicated approach row. Without it, the generic convergence bar is
        // forced onto the target envelope's top-border row, visually fusing
        // edge ownership with the container boundary. Keep this surcharge
        // limited to the preceding rank and the multi-entry topology, after
        // the boundary policy has selected its base spacing.
        if vertical_titled_subgraph_entry_requires_headroom(graph, layers, layer_idx) {
            spacing = spacing.max(SPACING_MINIMAL + 2);
        }

        // A labeled edge that crosses a titled subgraph boundary needs one
        // additional approach row even when it is a single entry or exits the
        // subgraph.  Without this row, the route-aware label chooser has no
        // legal vertical cell between the arrow and the envelope and falls
        // back to painting the label on the top/bottom border.  Keep the rule
        // structural: it applies to any titled boundary in the vertical
        // directions, not to a fixture or node name.
        if boundary_crosses_subgraph && has_labels && !has_fan_out && !has_fan_in {
            spacing = spacing.saturating_sub(2).max(SPACING_LABELED + 1);
        }

        if vertical_titled_subgraph_boundary_label_requires_headroom(graph, layers, layer_idx) {
            spacing = spacing.max(SPACING_LABELED + 3);
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
            let horizontal_minimum = self
                .min_horizontal
                .saturating_sub(if dense_horizontal {
                    DENSE_HORIZONTAL_COMPRESSION
                } else {
                    0
                })
                .max(SPACING_MINIMAL);
            if !has_fan_out && !has_fan_in && !has_labels {
                spacing = horizontal_minimum.max(spacing * 2);
            } else {
                spacing = spacing
                    .saturating_mul(2)
                    .saturating_sub(if dense_horizontal {
                        DENSE_HORIZONTAL_COMPRESSION
                    } else {
                        0
                    })
                    .max(SPACING_MINIMAL);
            }

            // Reserve enough primary-axis room for the widest edge label emitted
            // from this layer.  The renderer can place a label tightly when the
            // route has no margin, but it must not have to detach a complete label
            // onto an unrelated row just because the layout stopped six cells
            // short of an eight-cell label.  The four-cell allowance matches the
            // LR/RL inline label margins and keeps the policy bounded by the
            // public label-width contract.
            if has_labels {
                let widest_label = layer
                    .iter()
                    .flat_map(|&idx| {
                        let source_id = &graph.nodes[idx].id;
                        graph
                            .edges
                            .iter()
                            .filter(move |edge| {
                                !edge.is_back_edge && edge.from.as_str() == source_id.as_str()
                            })
                            .filter_map(|edge| edge.label.as_deref())
                            .map(crate::display_profile::display_width)
                    })
                    .max()
                    .unwrap_or(0)
                    .min(crate::spacing::MAX_LABEL_WIDTH);
                spacing = spacing.max(widest_label.saturating_add(4));
            }
        }

        spacing
    }

    fn overlapping_fan_in_lane_count(
        &self,
        graph: &Graph,
        layers: &[Vec<usize>],
        layer_idx: usize,
    ) -> usize {
        let Some(next_layer) = layers.get(layer_idx + 1) else {
            return 0;
        };
        let layer = &layers[layer_idx];
        let mut spans = Vec::new();

        for &target_idx in next_layer {
            let target_id = &graph.nodes[target_idx].id;
            if graph.get_node_subgraph(target_id).is_some() {
                continue;
            }
            let source_positions: Vec<usize> = layer
                .iter()
                .enumerate()
                .filter_map(|(position, &source_idx)| {
                    let source_id = &graph.nodes[source_idx].id;
                    if graph.get_node_subgraph(source_id).is_some() {
                        return None;
                    }
                    graph
                        .edges
                        .iter()
                        .any(|edge| {
                            !edge.is_back_edge && edge.from == *source_id && edge.to == *target_id
                        })
                        .then_some(position)
                })
                .collect();
            if source_positions.len() > 1 {
                spans.push((
                    source_positions.iter().copied().min().unwrap_or(0),
                    source_positions.iter().copied().max().unwrap_or(0),
                ));
            }
        }

        if spans.len() < 2 {
            return 0;
        }

        // Count the largest connected interval component. Inclusive overlap
        // is intentional: a shared endpoint is still a visually shared stem.
        let mut visited = vec![false; spans.len()];
        let mut largest = 0;
        for start in 0..spans.len() {
            if visited[start] {
                continue;
            }
            visited[start] = true;
            let mut component = vec![start];
            let mut cursor = 0;
            while cursor < component.len() {
                let current = spans[component[cursor]];
                for candidate in 0..spans.len() {
                    if visited[candidate] {
                        continue;
                    }
                    let overlaps =
                        spans[candidate].0 <= current.1 && current.0 <= spans[candidate].1;
                    if overlaps {
                        visited[candidate] = true;
                        component.push(candidate);
                    }
                }
                cursor += 1;
            }
            largest = largest.max(component.len());
        }
        largest
    }

    fn wide_terminal_fan_in_count(
        &self,
        graph: &Graph,
        layers: &[Vec<usize>],
        layer_idx: usize,
    ) -> Option<usize> {
        let next_layer = layers.get(layer_idx + 1)?;
        next_layer.iter().find_map(|target_idx| {
            let target_id = graph.nodes.get(*target_idx)?.id.as_str();
            wide_terminal_fan_in::target_port_count(graph, target_id)
        })
    }

    fn identity_fan_in_count(
        &self,
        graph: &Graph,
        layers: &[Vec<usize>],
        layer_idx: usize,
    ) -> Option<usize> {
        let next_layer = layers.get(layer_idx + 1)?;
        next_layer.iter().find_map(|target_idx| {
            let target_id = graph.nodes.get(*target_idx)?.id.as_str();
            fan_in_identity::target_port_count(graph, target_id)
        })
    }

    fn subgraph_identity_fan_in_count(
        &self,
        graph: &Graph,
        layers: &[Vec<usize>],
        layer_idx: usize,
    ) -> Option<usize> {
        let next_layer = layers.get(layer_idx + 1)?;
        next_layer.iter().find_map(|target_idx| {
            let target_id = graph.nodes.get(*target_idx)?.id.as_str();
            subgraph_fan_in_identity::target_port_count(graph, target_id)
        })
    }

    fn sibling_subgraph_identity_fan_in_count(
        &self,
        graph: &Graph,
        layers: &[Vec<usize>],
        layer_idx: usize,
    ) -> Option<usize> {
        let current_layer = layers.get(layer_idx)?;
        let next_layer = layers.get(layer_idx + 1)?;
        next_layer.iter().find_map(|target_idx| {
            let target_id = graph.nodes.get(*target_idx)?.id.as_str();
            let count = sibling_subgraph_fan_in_identity::target_port_counts(graph)
                .into_iter()
                .find_map(|(candidate_id, count)| (candidate_id == target_id).then_some(count))?;
            let has_current_source = graph.edges.iter().any(|edge| {
                edge.to == target_id
                    && current_layer
                        .iter()
                        .any(|source_idx| graph.nodes[*source_idx].id == edge.from)
            });
            has_current_source.then_some(count)
        })
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

fn is_dense_horizontal_graph(graph: &Graph, layers: &[Vec<usize>]) -> bool {
    if !matches!(graph.direction, Direction::LR | Direction::RL) {
        return false;
    }

    let non_back_edges = graph.edges.iter().filter(|edge| !edge.is_back_edge).count();
    graph.nodes.len() >= 20 && non_back_edges >= 20 && layers.iter().any(|layer| layer.len() >= 4)
}

fn database_target_requires_headroom(
    graph: &Graph,
    layers: &[Vec<usize>],
    layer_idx: usize,
) -> bool {
    let Some(next_layer) = layers.get(layer_idx + 1) else {
        return false;
    };

    let current_layer_ids: HashSet<&str> = layers[layer_idx]
        .iter()
        .filter_map(|index| graph.nodes.get(*index))
        .map(|node| node.id.as_str())
        .collect();

    next_layer.iter().any(|target_idx| {
        let Some(target) = graph.nodes.get(*target_idx) else {
            return false;
        };
        target.shape == NodeShape::Database
            && graph.edges.iter().any(|edge| {
                !edge.is_back_edge
                    && edge.to == target.id
                    && current_layer_ids.contains(edge.from.as_str())
            })
    })
}

fn database_intermediate_scene_requires_headroom(
    graph: &Graph,
    layers: &[Vec<usize>],
    layer_idx: usize,
) -> bool {
    if !matches!(
        graph.direction,
        Direction::TD | Direction::TB | Direction::BT
    ) || graph.nodes.len() != 3
        || graph.edges.len() != 3
        || !graph.subgraphs.is_empty()
        || graph.edges.iter().any(|edge| {
            edge.is_back_edge || edge.kind != crate::graph::EdgeKind::Arrow || edge.label.is_some()
        })
    {
        return false;
    }

    let Some(source) = graph.nodes.iter().find(|node| {
        node.shape == NodeShape::Rectangle
            && graph
                .edges
                .iter()
                .filter(|edge| !edge.is_back_edge && edge.from == node.id)
                .count()
                == 2
            && graph
                .edges
                .iter()
                .all(|edge| edge.is_back_edge || edge.to != node.id)
    }) else {
        return false;
    };
    let databases: Vec<_> = graph
        .nodes
        .iter()
        .filter(|node| node.shape == NodeShape::Database)
        .collect();
    if databases.len() != 2 {
        return false;
    }

    let Some(intermediate) = databases.iter().find(|node| {
        graph
            .edges
            .iter()
            .filter(|edge| !edge.is_back_edge && edge.from == node.id)
            .count()
            == 1
            && graph
                .edges
                .iter()
                .filter(|edge| !edge.is_back_edge && edge.to == node.id)
                .count()
                == 1
    }) else {
        return false;
    };
    let Some(target) = databases.iter().find(|node| {
        node.id != intermediate.id
            && graph
                .edges
                .iter()
                .filter(|edge| !edge.is_back_edge && edge.from == node.id)
                .count()
                == 0
            && graph
                .edges
                .iter()
                .filter(|edge| !edge.is_back_edge && edge.to == node.id)
                .count()
                == 2
    }) else {
        return false;
    };
    let has_edge = |from: &str, to: &str| {
        graph
            .edges
            .iter()
            .any(|edge| !edge.is_back_edge && edge.from == from && edge.to == to)
    };
    if !(has_edge(&source.id, &intermediate.id)
        && has_edge(&source.id, &target.id)
        && has_edge(&intermediate.id, &target.id))
    {
        return false;
    }

    layers.get(layer_idx).is_some_and(|layer| {
        layer
            .iter()
            .any(|index| graph.nodes[*index].id == source.id)
    }) && layers.get(layer_idx + 1).is_some_and(|layer| {
        layer
            .iter()
            .any(|index| graph.nodes[*index].id == intermediate.id)
    })
}

fn vertical_diamond_target_requires_headroom(
    graph: &Graph,
    layers: &[Vec<usize>],
    layer_idx: usize,
) -> bool {
    if !matches!(
        graph.direction,
        Direction::TD | Direction::TB | Direction::BT
    ) {
        return false;
    }

    let Some(next_layer) = layers.get(layer_idx + 1) else {
        return false;
    };

    let current_layer_ids: HashSet<&str> = layers[layer_idx]
        .iter()
        .filter_map(|index| graph.nodes.get(*index))
        .map(|node| node.id.as_str())
        .collect();

    next_layer.iter().any(|target_idx| {
        let Some(target) = graph.nodes.get(*target_idx) else {
            return false;
        };
        target.shape == NodeShape::Diamond
            && graph.edges.iter().any(|edge| {
                !edge.is_back_edge
                    && edge.to == target.id
                    && current_layer_ids.contains(edge.from.as_str())
            })
    })
}

fn vertical_titled_subgraph_entry_requires_headroom(
    graph: &Graph,
    layers: &[Vec<usize>],
    layer_idx: usize,
) -> bool {
    if !matches!(graph.direction, Direction::TD | Direction::TB) {
        return false;
    }

    let Some(next_layer) = layers.get(layer_idx + 1) else {
        return false;
    };
    let current_layer_ids: HashSet<&str> = layers[layer_idx]
        .iter()
        .filter_map(|index| graph.nodes.get(*index))
        .map(|node| node.id.as_str())
        .collect();

    let mut external_entries = 0;
    for target_idx in next_layer {
        let Some(target) = graph.nodes.get(*target_idx) else {
            continue;
        };
        let Some(subgraph_id) = graph.get_node_subgraph(&target.id) else {
            continue;
        };
        let Some(subgraph) = graph.get_subgraph(subgraph_id) else {
            continue;
        };
        if subgraph.title.is_none() {
            continue;
        }
        external_entries += graph
            .edges
            .iter()
            .filter(|edge| {
                !edge.is_back_edge
                    && edge.to == target.id
                    && current_layer_ids.contains(edge.from.as_str())
                    && graph.get_node_subgraph(&edge.from) != Some(subgraph_id)
            })
            .count();
    }

    external_entries >= 2
}

fn vertical_titled_subgraph_boundary_label_requires_headroom(
    graph: &Graph,
    layers: &[Vec<usize>],
    layer_idx: usize,
) -> bool {
    if !matches!(
        graph.direction,
        Direction::TD | Direction::TB | Direction::BT
    ) {
        return false;
    }

    let Some(next_layer) = layers.get(layer_idx + 1) else {
        return false;
    };

    layers[layer_idx].iter().any(|source_idx| {
        let Some(source) = graph.nodes.get(*source_idx) else {
            return false;
        };
        let source_subgraph = graph.get_node_subgraph(&source.id);

        next_layer.iter().any(|target_idx| {
            let Some(target) = graph.nodes.get(*target_idx) else {
                return false;
            };
            let target_subgraph = graph.get_node_subgraph(&target.id);
            if source_subgraph == target_subgraph {
                return false;
            }

            let crosses_titled_boundary = source_subgraph
                .and_then(|id| graph.get_subgraph(id))
                .is_some_and(|subgraph| subgraph.title.is_some())
                || target_subgraph
                    .and_then(|id| graph.get_subgraph(id))
                    .is_some_and(|subgraph| subgraph.title.is_some());

            crosses_titled_boundary
                && graph.edges.iter().any(|edge| {
                    !edge.is_back_edge
                        && edge.label.is_some()
                        && edge.from == source.id
                        && edge.to == target.id
                })
        })
    })
}

pub(super) fn place_nodes(
    graph: &Graph,
    layers: &[Vec<usize>],
    coords: &OrientedCoords,
    config: &CoarseLayoutConfig,
    prior_positions: Option<&HashMap<String, Point>>,
) -> Placement {
    let debug_fan_in = crate::runtime::current().diagnostics.fan_in;
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

    let secondary_gap = if has_dense_crossing_family(graph, layers) {
        gap_for_axis(coords.secondary, config).max(3)
    } else {
        gap_for_axis(coords.secondary, config)
    };

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

            if debug_fan_in && node.id == "Merge" {
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

            if debug_fan_in && node.id == "Merge" {
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

    balance_pure_fan_in_targets(graph, layers, coords, &mut positions, &mut node_rects);

    balance_dual_junctions(graph, layers, coords, &mut positions, &mut node_rects);

    if has_dense_crossing_family(graph, layers) {
        // A dense rank pair is easier to inspect when the secondary-axis
        // slots remain stable across the whole flow. Barycenter balancing can
        // otherwise stagger the middle rank between its neighbors, forcing
        // every reserved lane to make an extra hook beside a node border.
        align_dense_rank_slots(
            graph,
            layers,
            coords,
            secondary_gap,
            &mut positions,
            &mut node_rects,
        );
    }

    if debug_fan_in {
        if let Some(rect) = node_rects.get("Merge") {
            eprintln!("post-balance Merge rect {rect:?}");
        }
        if let Some(rect) = node_rects.get("S1") {
            eprintln!("post-balance S1 rect {rect:?}");
        }
    }

    // Normalize coordinates (shift everything so min_x/min_y is 0)
    let min_x = node_rects.values().map(|r| r.x).min().unwrap_or(0);
    let min_y = node_rects.values().map(|r| r.y).min().unwrap_or(0);

    if debug_fan_in {
        eprintln!("normalize min_x={min_x} min_y={min_y}");
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

    if debug_fan_in {
        if let Some(rect) = node_rects.get("Merge") {
            eprintln!("post-normalize Merge rect {rect:?}");
        }
        if let Some(rect) = node_rects.get("S1") {
            eprintln!("post-normalize S1 rect {rect:?}");
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

fn has_dense_crossing_family(graph: &Graph, layers: &[Vec<usize>]) -> bool {
    (0..layers.len().saturating_sub(1))
        .any(|layer_idx| dense_crossing_pair(graph, layers, layer_idx))
}

fn align_dense_rank_slots(
    graph: &Graph,
    layers: &[Vec<usize>],
    coords: &OrientedCoords,
    secondary_gap: usize,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
) {
    let max_extent = graph
        .nodes
        .iter()
        .map(|node| node_extent_secondary(node, coords))
        .max()
        .unwrap_or(BOX_HEIGHT);
    let pitch = max_extent + secondary_gap.max(3);

    for layer in layers {
        for (slot, &node_idx) in layer.iter().enumerate() {
            let Some(node) = graph.nodes.get(node_idx) else {
                continue;
            };
            let Some(rect) = node_rects.get_mut(&node.id) else {
                continue;
            };
            let mut x = rect.x;
            let mut y = rect.y;
            coords.set_secondary(&mut x, &mut y, slot.saturating_mul(pitch));
            rect.x = x;
            rect.y = y;
            if let Some(position) = positions.get_mut(&node.id) {
                position.x = x;
                position.y = y;
            }
        }
    }
}

fn dense_crossing_pair(graph: &Graph, layers: &[Vec<usize>], layer_idx: usize) -> bool {
    let Some(next_layer) = layers.get(layer_idx + 1) else {
        return false;
    };
    let layer_ids: HashSet<&str> = layers[layer_idx]
        .iter()
        .filter_map(|index| graph.nodes.get(*index))
        .map(|node| node.id.as_str())
        .collect();
    let next_ids: HashSet<&str> = next_layer
        .iter()
        .filter_map(|index| graph.nodes.get(*index))
        .map(|node| node.id.as_str())
        .collect();
    if layer_ids.is_empty() || next_ids.is_empty() {
        return false;
    }

    let mut source_ids = HashSet::new();
    let mut target_ids = HashSet::new();
    let mut relation = HashSet::new();
    for edge in &graph.edges {
        if edge.is_back_edge || !layer_ids.contains(edge.from.as_str()) {
            continue;
        }
        if !next_ids.contains(edge.to.as_str()) {
            continue;
        }
        source_ids.insert(edge.from.as_str());
        target_ids.insert(edge.to.as_str());
        relation.insert((edge.from.as_str(), edge.to.as_str()));
    }
    if source_ids.len() != 3 || target_ids.len() != 3 || relation.len() != 6 {
        return false;
    }
    source_ids
        .iter()
        .all(|source| relation.iter().filter(|(from, _)| from == source).count() == 2)
        && target_ids
            .iter()
            .all(|target| relation.iter().filter(|(_, to)| to == target).count() == 2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, Node};

    fn two_layer_graph(target_shape: NodeShape) -> (Graph, Vec<Vec<usize>>) {
        let mut graph = Graph::new();
        graph.add_node(Node::new("source", "Source"));
        graph.add_node(Node::with_shape("target", "Target", target_shape));
        graph.add_edge(Edge::new("source", "target"));
        (graph, vec![vec![0], vec![1]])
    }

    fn dense_horizontal_graph() -> (Graph, Vec<Vec<usize>>) {
        let mut graph = Graph::new();
        graph.direction = Direction::LR;
        for index in 0..20 {
            graph.add_node(Node::new(format!("n{index}"), format!("N{index}")));
        }
        for rank in 0..3 {
            for offset in 0..5 {
                let source = rank * 5 + offset;
                let target = (rank + 1) * 5 + offset;
                graph.add_edge(Edge::new(format!("n{source}"), format!("n{target}")));
            }
        }
        for offset in 0..5 {
            graph.add_edge(Edge::new(format!("n{offset}"), format!("n{}", 10 + offset)));
        }
        let layers = vec![
            (0..5).collect(),
            (5..10).collect(),
            (10..15).collect(),
            (15..20).collect(),
        ];
        (graph, layers)
    }

    #[test]
    fn database_target_headroom_is_local_to_the_preceding_rank() {
        let (database_graph, database_layers) = two_layer_graph(NodeShape::Database);
        let (rectangle_graph, rectangle_layers) = two_layer_graph(NodeShape::Rectangle);
        let config = CoarseLayoutConfig::default();
        let policy = LayoutSpacingPolicy::new(
            config.subgraph_gutter,
            config.node_padding,
            config.min_horizontal_spacing,
            config.min_vertical_spacing,
        );

        assert_eq!(
            policy.spacing_for_layer(&database_graph, &database_layers, 0),
            SPACING_MINIMAL + 1
        );
        assert_eq!(
            policy.spacing_for_layer(&rectangle_graph, &rectangle_layers, 0),
            SPACING_MINIMAL
        );
        assert!(!database_target_requires_headroom(
            &database_graph,
            &database_layers,
            1
        ));
    }

    #[test]
    fn horizontal_density_compression_is_topology_gated() {
        let (dense_graph, dense_layers) = dense_horizontal_graph();
        let (mut sparse_graph, sparse_layers) = two_layer_graph(NodeShape::Rectangle);
        sparse_graph.direction = Direction::LR;
        let config = CoarseLayoutConfig::default();
        let policy = LayoutSpacingPolicy::new(
            config.subgraph_gutter,
            config.node_padding,
            config.min_horizontal_spacing,
            config.min_vertical_spacing,
        );

        assert!(is_dense_horizontal_graph(&dense_graph, &dense_layers));
        assert!(!is_dense_horizontal_graph(&sparse_graph, &sparse_layers));
        assert_eq!(
            policy.spacing_for_layer(&dense_graph, &dense_layers, 0),
            8,
            "dense horizontal fan-out should use the compressed doubled gap"
        );
        assert_eq!(
            policy.spacing_for_layer(&sparse_graph, &sparse_layers, 0),
            4,
            "a sparse horizontal control keeps its existing local gap"
        );
    }

    #[test]
    fn strict_database_scene_headroom_gives_the_source_tee_a_quiet_stem() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;
        graph.add_node(Node::new("api", "REST API"));
        graph.add_node(Node::with_shape("cache", "Redis", NodeShape::Database));
        graph.add_node(Node::with_shape(
            "database",
            "PostgreSQL",
            NodeShape::Database,
        ));
        graph.add_edge(Edge::new("api", "database"));
        graph.add_edge(Edge::new("api", "cache"));
        graph.add_edge(Edge::new("cache", "database"));
        let layers = vec![vec![0], vec![1], vec![2]];
        let config = CoarseLayoutConfig::default();
        let policy = LayoutSpacingPolicy::new(
            config.subgraph_gutter,
            config.node_padding,
            config.min_horizontal_spacing,
            config.min_vertical_spacing,
        );

        assert!(database_intermediate_scene_requires_headroom(
            &graph, &layers, 0
        ));
        assert_eq!(
            policy.spacing_for_layer(&graph, &layers, 0),
            SPACING_MINIMAL + 2
        );
    }

    #[test]
    fn vertical_diamond_target_headroom_is_local_and_direction_gated() {
        let (mut diamond_graph, diamond_layers) = two_layer_graph(NodeShape::Diamond);
        let (mut rectangle_graph, rectangle_layers) = two_layer_graph(NodeShape::Rectangle);
        let config = CoarseLayoutConfig::default();
        let policy = LayoutSpacingPolicy::new(
            config.subgraph_gutter,
            config.node_padding,
            config.min_horizontal_spacing,
            config.min_vertical_spacing,
        );

        assert_eq!(
            policy.spacing_for_layer(&diamond_graph, &diamond_layers, 0),
            SPACING_MINIMAL + 1
        );
        assert_eq!(
            policy.spacing_for_layer(&rectangle_graph, &rectangle_layers, 0),
            SPACING_MINIMAL
        );

        for direction in [Direction::LR, Direction::RL] {
            diamond_graph.direction = direction;
            rectangle_graph.direction = direction;
            assert!(!vertical_diamond_target_requires_headroom(
                &diamond_graph,
                &diamond_layers,
                0
            ));
            assert_eq!(
                policy.spacing_for_layer(&diamond_graph, &diamond_layers, 0),
                policy.spacing_for_layer(&rectangle_graph, &rectangle_layers, 0),
                "horizontal diamond spacing must remain the existing control spacing for {direction:?}"
            );
        }

        let mut three_layer = Graph::new();
        three_layer.add_node(Node::new("source", "Source"));
        three_layer.add_node(Node::new("middle", "Middle"));
        three_layer.add_node(Node::with_shape("target", "Target", NodeShape::Diamond));
        three_layer.add_edge(Edge::new("source", "middle"));
        three_layer.add_edge(Edge::new("middle", "target"));
        let three_layers = vec![vec![0], vec![1], vec![2]];

        assert!(!vertical_diamond_target_requires_headroom(
            &three_layer,
            &three_layers,
            0
        ));
        assert!(vertical_diamond_target_requires_headroom(
            &three_layer,
            &three_layers,
            1
        ));
        assert!(!vertical_diamond_target_requires_headroom(
            &three_layer,
            &three_layers,
            2
        ));
    }

    #[test]
    fn titled_vertical_subgraph_multi_entry_headroom_is_local_and_direction_gated() {
        let mut graph = Graph::new();
        graph.add_node(Node::new("x1", "X1"));
        graph.add_node(Node::new("x2", "X2"));
        graph.add_node(Node::new("a", "A"));
        graph.add_node(Node::new("b", "B"));
        graph.add_edge(Edge::new("x1", "a"));
        graph.add_edge(Edge::new("x2", "b"));
        let mut subgraph = crate::graph::Subgraph::new("sg", Some("Target Group".to_owned()));
        subgraph.add_node("a");
        subgraph.add_node("b");
        graph.add_subgraph(subgraph);
        graph.associate_node_with_subgraph("a", "sg");
        graph.associate_node_with_subgraph("b", "sg");
        let layers = vec![vec![0, 1], vec![2, 3]];
        let config = CoarseLayoutConfig::default();
        let policy = LayoutSpacingPolicy::new(
            config.subgraph_gutter,
            config.node_padding,
            config.min_horizontal_spacing,
            config.min_vertical_spacing,
        );

        assert_eq!(graph.get_node_subgraph("a"), Some("sg"));
        assert!(vertical_titled_subgraph_entry_requires_headroom(
            &graph, &layers, 0
        ));
        assert_eq!(
            policy.spacing_for_layer(&graph, &layers, 0),
            SPACING_MINIMAL + 2
        );
        graph.direction = Direction::LR;
        assert!(!vertical_titled_subgraph_entry_requires_headroom(
            &graph, &layers, 0
        ));
    }

    #[test]
    fn titled_vertical_subgraph_labeled_boundary_headroom_is_topology_gated() {
        let mut graph = Graph::new();
        graph.add_node(Node::new("inside", "Inside"));
        graph.add_node(Node::new("outside", "Outside"));
        let mut edge = Edge::new("inside", "outside");
        edge.label = Some("handoff".to_owned());
        graph.add_edge(edge);

        let mut subgraph = crate::graph::Subgraph::new("sg", Some("Group".to_owned()));
        subgraph.add_node("inside");
        graph.add_subgraph(subgraph);
        graph.associate_node_with_subgraph("inside", "sg");
        let layers = vec![vec![0], vec![1]];

        for direction in [Direction::TD, Direction::TB, Direction::BT] {
            graph.direction = direction;
            assert!(
                vertical_titled_subgraph_boundary_label_requires_headroom(&graph, &layers, 0),
                "expected labeled titled boundary headroom for {direction:?}"
            );
        }

        graph.direction = Direction::LR;
        assert!(!vertical_titled_subgraph_boundary_label_requires_headroom(
            &graph, &layers, 0
        ));
    }
}
