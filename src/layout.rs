//! Coarse layout + Manhattan routing pipeline (default engine).
//!
//! This module owns the current coarse layout and Manhattan-routing pipeline.
//! The deprecated waterfall/spike names below remain compatibility aliases for
//! callers migrating to the coarse engine. The pipeline provides:
//! - Direction-agnostic layered placement on a coarse grid
//! - Obstacle-aware Manhattan routing with simple detours
//! - Subgraph gutter metadata for future avoidance/bundling

use std::collections::{HashMap, HashSet};

use anyhow::Result;

use crate::crossing::CrossingMinimizer;
use crate::geom::{EdgeRoute, Point, Rect};
use crate::graph::{Direction, Graph};
use crate::orientation::OrientedCoords;
use crate::portals::{compute_envelopes, SubgraphEnvelope};
use crate::spacing::SpacingConfig;
use crate::style::{box_width, BOX_MIN_WIDTH};

mod constraints;
mod dual_junction;
mod envelope_stage;
#[path = "layout_routing.rs"]
mod layout_routing;
mod normalization;
mod optimization;
mod pipeline;
mod placement;
mod pure_fan_in;
mod routing_stage;

use constraints::*;
use optimization::{assign_layers, mark_back_edges};
pub use pipeline::layout;
use placement::place_nodes;

/// Input for the experimental layout engine.
pub struct LayoutInput<'a> {
    pub graph: &'a Graph,
    pub prior_positions: Option<HashMap<String, Point>>,
}

/// Output of the experimental layout pipeline.
#[derive(Debug, Default)]
pub struct LayoutOutput {
    pub positions: HashMap<String, Point>,
    pub subgraph_envelopes: HashMap<String, SubgraphEnvelope>,
    pub routes: HashMap<usize, EdgeRoute>,
    pub canvas: Rect,
    pub warnings: Vec<String>,
    pub ranks: HashMap<String, usize>,
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
    /// Allow carving through subgraph borders (portals).
    pub enable_portals: bool,
}

impl Default for CoarseLayoutConfig {
    fn default() -> Self {
        Self::from_spacing(&SpacingConfig::default_config())
    }
}

impl CoarseLayoutConfig {
    /// Tighter spacing defaults for terminal-friendly diagrams.
    ///
    /// This is intentionally conservative (still leaves room for elbows/arrows)
    /// but reduces the default "big gaps" between ranks/columns.
    pub fn compact() -> Self {
        Self::from_spacing(&SpacingConfig::compact())
    }

    pub fn from_spacing(spacing: &SpacingConfig) -> Self {
        Self {
            node_padding: spacing.node_margin,
            subgraph_gutter: spacing.subgraph_gutter,
            min_horizontal_spacing: spacing.col_spacing,
            min_vertical_spacing: spacing.row_spacing,
            enable_portals: true,
        }
    }
}

pub fn coarse_waterfall_with_config(graph: Graph, mut config: CoarseLayoutConfig) -> Result<Graph> {
    crate::runtime::with_captured(|| {
        if crate::runtime::current().compatibility.disable_portals {
            config.enable_portals = false;
        }
        apply_coarse_layout(graph, None, config)
    })
}

/// Preferred entry point for the coarse layout engine.
pub fn coarse_waterfall(graph: Graph) -> Result<Graph> {
    coarse_waterfall_with_config(graph, CoarseLayoutConfig::default())
}

/// Deprecated compatibility alias for [`coarse_waterfall`].
#[deprecated(note = "Use coarse_waterfall; this alias runs the current coarse engine")]
pub fn waterfall(graph: Graph) -> Result<Graph> {
    coarse_waterfall(graph)
}

/// Coarse layout engine entry point.
pub fn apply_coarse_layout(
    mut graph: Graph,
    prior_positions: Option<HashMap<String, Point>>,
    config: CoarseLayoutConfig,
) -> Result<Graph> {
    let debug_timing = crate::runtime::current().diagnostics.timing;
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
        if let Some(bounds) = output.subgraph_envelopes.get(&subgraph.id) {
            subgraph.bounds = crate::graph::Rectangle::new(
                bounds.outer.x,
                bounds.outer.y,
                bounds.outer.width,
                bounds.outer.height,
            );
            subgraph.inner_bounds = crate::graph::Rectangle::new(
                bounds.inner.x,
                bounds.inner.y,
                bounds.inner.width,
                bounds.inner.height,
            );
        }
    }

    if debug_timing {
        for (sg_id, bounds) in &output.subgraph_envelopes {
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
                node.id, node.x, node.y, node.width, node.height
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
            for (i, seg) in route.segments.iter().enumerate() {
                eprintln!(
                    "  seg[{}]: ({}, {}) -> ({}, {})",
                    i, seg.from.x, seg.from.y, seg.to.x, seg.to.y
                );
            }
        }
    }

    if debug_timing {
        eprintln!("termiflow: apply {:?}", t_start.elapsed());
    }

    Ok(graph)
}

fn adjust_portal_slots_for_title(envelopes: &mut HashMap<String, SubgraphEnvelope>, graph: &Graph) {
    // BT titles are drawn on the bottom border row. Any bottom-border portal slots
    // must stay out of that title span (including its surrounding spaces).
    if !matches!(graph.direction, Direction::BT) {
        return;
    }

    for sg in &graph.subgraphs {
        let Some(title) = sg.title.as_deref() else {
            continue;
        };
        let Some(env) = envelopes.get_mut(&sg.id) else {
            continue;
        };

        let Some((start, end)) =
            crate::graph::subgraph_title_span(env.outer.x, env.outer.width, title, graph.direction)
        else {
            continue;
        };
        let min_x = env.outer.x.saturating_add(1);
        let max_x = env.outer.right().saturating_sub(2);
        if max_x < min_x {
            continue;
        }

        let shift_out_of_span = |x: usize| -> usize {
            let protected_start = start.saturating_sub(2);
            let protected_end = end.saturating_add(2).min(max_x);
            if x < protected_start || x > protected_end {
                return x;
            }
            let left = (protected_start > min_x).then(|| protected_start.saturating_sub(1));
            let right = (protected_end < max_x).then(|| protected_end + 1);
            match (left, right) {
                (Some(left), Some(right)) => {
                    let left_distance = x.abs_diff(left);
                    let right_distance = x.abs_diff(right);
                    if left_distance < right_distance {
                        left
                    } else if right_distance < left_distance {
                        right
                    } else if x <= (protected_start + protected_end) / 2 {
                        left
                    } else {
                        right
                    }
                }
                (Some(left), None) => left,
                (None, Some(right)) => right,
                (None, None) => x,
            }
        };

        if !env.portals.bottom.is_empty() {
            let mut shifted = HashSet::new();
            for &x in &env.portals.bottom {
                let cx = x.clamp(min_x, max_x);
                shifted.insert(shift_out_of_span(cx));
            }
            env.portals.bottom = shifted;
        }
    }
}

/// Deprecated compatibility alias for [`apply_coarse_layout`].
#[deprecated(note = "Use apply_coarse_layout; this alias runs the current coarse engine")]
pub fn apply_spike_layout(
    graph: Graph,
    prior_positions: Option<HashMap<String, Point>>,
    config: CoarseLayoutConfig,
) -> Result<Graph> {
    apply_coarse_layout(graph, prior_positions, config)
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests;
