//! TermiFlow - Terminal Mermaid Diagram Renderer
//!
//! A library for rendering Mermaid flowchart diagrams as ASCII/Unicode art.
//!
//! # Quick Start
//!
//! ```rust
//! use termiflow::{render, RenderOptions};
//!
//! let input = "graph TD\n    A[Start] --> B[End]";
//! let output = render(input, RenderOptions::default()).unwrap();
//! println!("{}", output);
//! ```
//!
//! # Pipeline
//!
//! The rendering pipeline has three stages:
//! 1. **Parse** - Convert Mermaid syntax to a graph structure
//! 2. **Layout** - Assign coordinates using the waterfall algorithm
//! 3. **Render** - Draw boxes and edges on a 2D canvas

// ============================================================================
// Modules
// ============================================================================

pub mod config;
pub mod crossing;
pub mod geom;
pub mod graph;
pub mod layout;
pub mod measure;
pub mod orientation;
pub mod parser;
pub mod portals;
pub mod render;
pub mod scaling;
pub mod spacing;
pub mod style;
pub mod tui;

// ============================================================================
// Re-exports for convenient access
// ============================================================================

pub use config::{Config, ConfigBuilder};
pub use crossing::{CrossingConfig, CrossingMinimizer, Heuristic};
pub use graph::{Edge, EdgeKind, Graph, Node};
pub use layout::coarse_waterfall;
pub use parser::{parse, ParseConfig, ParseResult};
pub use render::critic::{
    AuditSummary, AuditVerdict, CriticFinding, CriticReport, FindingCode, FindingSeverity,
};
pub use render::render as render_canvas;
pub use render::{render_with_feedback as render_canvas_with_feedback, RenderOutcome};
pub use scaling::{CanvasBudget, DiagramMetrics, ScalingMode};
pub use spacing::{SpacingConfig, SpacingMode};
pub use style::{BaseStyle, CompositeStyle};
pub use tui::{AnsiDiffPresenter, FrameDelta, TerminalFrame, TerminalPresenter};

// ============================================================================
// High-Level API
// ============================================================================

use anyhow::Result;
/// Options for rendering a diagram
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// Border style (default: Unicode)
    pub style: BaseStyle,
    /// Maximum label width before truncation (default: 20)
    pub max_label_width: usize,
    /// Maximum edge label width before truncation (default: 20)
    pub max_edge_label_width: usize,
    /// Enable multiline label wrapping (default: false)
    pub wrap_labels: bool,
    /// Maximum number of label lines when wrapping is enabled (default: 1)
    pub max_label_lines: usize,
    /// Strict mode - fail on any parse warning (default: false)
    pub strict: bool,
    /// Crop empty margins around output (default: true)
    pub crop: bool,
    /// Add padding around output (default: 0)
    pub pad: usize,
    /// Use a tighter layout spacing (default: false)
    pub compact: bool,
    /// Optional composite style override (takes precedence over `style`)
    pub composite_style: Option<CompositeStyle>,
    /// Enable bounded render repair passes after the initial draw.
    pub optimize_render: bool,
    /// Maximum number of repair passes when render optimization is enabled.
    pub render_repair_passes: usize,
    /// Maximum number of layout candidate repair passes when render optimization is enabled.
    pub layout_repair_passes: usize,
    /// Emit critic findings for the final rendered frame.
    pub debug_critic: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderOptions {
    pub fn new() -> Self {
        Self {
            style: BaseStyle::default(),
            max_label_width: 20,
            max_edge_label_width: 20,
            wrap_labels: false,
            max_label_lines: 1,
            strict: false,
            crop: true,
            pad: 0,
            compact: false,
            composite_style: None,
            optimize_render: false,
            render_repair_passes: 2,
            layout_repair_passes: 2,
            debug_critic: false,
        }
    }

    pub fn with_style(mut self, style: BaseStyle) -> Self {
        self.style = style;
        self
    }

    pub fn with_max_label(mut self, width: usize) -> Self {
        self.max_label_width = width;
        self
    }

    pub fn with_max_edge_label_width(mut self, width: usize) -> Self {
        self.max_edge_label_width = width;
        self
    }

    pub fn with_wrap_labels(mut self, wrap: bool) -> Self {
        self.wrap_labels = wrap;
        self
    }

    pub fn with_max_label_lines(mut self, lines: usize) -> Self {
        self.max_label_lines = lines;
        self
    }

    pub fn strict(mut self) -> Self {
        self.strict = true;
        self
    }

    pub fn with_crop(mut self, crop: bool) -> Self {
        self.crop = crop;
        self
    }

    pub fn with_pad(mut self, pad: usize) -> Self {
        self.pad = pad;
        self
    }

    pub fn with_compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    pub fn with_composite_style(mut self, style: CompositeStyle) -> Self {
        self.composite_style = Some(style);
        self
    }

    pub fn with_optimize_render(mut self, optimize_render: bool) -> Self {
        self.optimize_render = optimize_render;
        self
    }

    pub fn with_render_repair_passes(mut self, render_repair_passes: usize) -> Self {
        self.render_repair_passes = render_repair_passes.max(1);
        self
    }

    pub fn with_layout_repair_passes(mut self, layout_repair_passes: usize) -> Self {
        self.layout_repair_passes = layout_repair_passes.max(1);
        self
    }

    pub fn with_debug_critic(mut self, debug_critic: bool) -> Self {
        self.debug_critic = debug_critic;
        self
    }
}

/// Render a Mermaid diagram to ASCII/Unicode art.
///
/// This is the main entry point for library usage. It combines parsing,
/// layout, and rendering into a single function.
///
/// # Arguments
/// * `input` - Mermaid flowchart source (e.g., "graph TD\nA --> B")
/// * `options` - Rendering options (style, label width, etc.)
///
/// # Returns
/// * `Ok(String)` - The rendered diagram as a string
/// * `Err` - Parse or layout error
///
/// # Example
/// ```rust
/// use termiflow::{render, RenderOptions, BaseStyle};
///
/// let diagram = render(
///     "graph TD\n    A[Hello] --> B[World]",
///     RenderOptions::new().with_style(BaseStyle::Rounded)
/// ).unwrap();
/// ```
pub fn render(input: &str, options: RenderOptions) -> Result<String> {
    Ok(render_with_feedback(input, options)?.output)
}

/// Render a Mermaid diagram and return critic/semantic feedback for the final frame.
pub fn render_with_feedback(input: &str, options: RenderOptions) -> Result<RenderOutcome> {
    // Parse
    let parse_result = parser::parse(input, options.strict)?;

    let defaults = RenderOptions::default();
    let mut builder = Config::builder();

    if options.max_label_width != defaults.max_label_width {
        builder = builder.max_label_width(options.max_label_width);
    }
    if options.max_edge_label_width != defaults.max_edge_label_width {
        builder = builder.max_edge_label_width(options.max_edge_label_width);
    }
    if options.wrap_labels != defaults.wrap_labels {
        builder = builder.wrap_labels(options.wrap_labels);
    }
    if options.max_label_lines != defaults.max_label_lines {
        builder = builder.max_label_lines(options.max_label_lines);
    }
    if options.crop != defaults.crop {
        builder = builder.crop(options.crop);
    }
    if options.pad != defaults.pad {
        builder = builder.pad(options.pad);
    }
    if options.strict != defaults.strict {
        builder = builder.strict(options.strict);
    }
    if options.compact {
        builder = builder.spacing(spacing::SpacingConfig::compact());
    }
    if options.optimize_render != defaults.optimize_render {
        builder = builder.optimize_render(options.optimize_render);
    }
    if options.render_repair_passes != defaults.render_repair_passes {
        builder = builder.render_repair_passes(options.render_repair_passes);
    }
    if options.layout_repair_passes != defaults.layout_repair_passes {
        builder = builder.layout_repair_passes(options.layout_repair_passes);
    }
    if options.debug_critic != defaults.debug_critic {
        builder = builder.debug_critic(options.debug_critic);
    }
    if options.composite_style.is_some() || options.style != defaults.style {
        builder = builder.style(
            options
                .composite_style
                .unwrap_or_else(|| CompositeStyle::from_base(options.style)),
        );
    }

    // Build config from explicit options + in-file directives + file config.
    let mut config = builder.build(&parse_result.config);
    config.spacing = config.spacing.for_direction(parse_result.graph.direction);

    // Measure labels + node height (opt-in via config)
    let mut graph = parse_result.graph;
    measure::measure_graph(&mut graph, &config);

    let (_graph, outcome) = layout_and_render_with_feedback(graph, config)?;
    Ok(outcome)
}

/// Render with default options (Unicode style, 20-char labels)
pub fn render_default(input: &str) -> Result<String> {
    render(input, RenderOptions::default())
}

/// Lay out a measured graph and render it with bounded layout candidate repair.
pub fn layout_and_render_with_feedback(
    graph: Graph,
    config: Config,
) -> Result<(Graph, RenderOutcome)> {
    let mut best_config = config.clone();
    let mut best_graph = layout_graph(graph.clone(), &best_config.spacing, None)?;
    let mut best_outcome = render::render_with_feedback(&best_graph, &best_config)?;
    best_outcome.warnings = best_graph.warnings.clone();
    best_outcome.layout_attempts = 1;

    let layout_repair_passes = std::env::var("TERMIFLOW_LAYOUT_REPAIR_PASSES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|value| value.max(1))
        .unwrap_or(config.layout_repair_passes);

    if config.optimize_render {
        let mut layout_repairs_applied = 0;
        let mut attempts = 1;
        let mut prior_positions = Some(node_positions(&best_graph));

        for _ in 0..layout_repair_passes {
            let candidates =
                build_layout_repair_candidates(&best_graph, &best_config, &best_outcome);
            if candidates.is_empty() {
                break;
            }

            let mut improved: Option<(Config, Graph, RenderOutcome)> = None;

            for candidate in candidates {
                attempts += 1;
                let mut candidate_config = best_config.clone();
                candidate_config.spacing = candidate.spacing;
                let candidate_prior_positions = candidate
                    .prior_positions
                    .or_else(|| prior_positions.clone());
                let candidate_graph = layout_graph(
                    graph.clone(),
                    &candidate_config.spacing,
                    candidate_prior_positions,
                )?;
                let mut candidate_outcome =
                    render::render_with_feedback(&candidate_graph, &candidate_config)?;
                candidate_outcome.warnings = candidate_graph.warnings.clone();

                let should_promote = improved.as_ref().map_or_else(
                    || is_better_outcome(&candidate_outcome, &best_outcome),
                    |(_, _, current_best)| is_better_outcome(&candidate_outcome, current_best),
                );

                if should_promote {
                    improved = Some((candidate_config, candidate_graph, candidate_outcome));
                }
            }

            let Some((candidate_config, candidate_graph, candidate_outcome)) = improved else {
                break;
            };

            if !is_better_outcome(&candidate_outcome, &best_outcome) {
                break;
            }

            best_config = candidate_config;
            best_graph = candidate_graph;
            best_outcome = candidate_outcome;
            prior_positions = Some(node_positions(&best_graph));
            layout_repairs_applied += 1;
            best_outcome.layout_repairs_applied = layout_repairs_applied;
            best_outcome.layout_attempts = attempts;
        }

        best_outcome.layout_repairs_applied = layout_repairs_applied;
        best_outcome.layout_attempts = attempts;
    }

    Ok((best_graph, best_outcome))
}

fn layout_graph(
    graph: Graph,
    spacing: &SpacingConfig,
    prior_positions: Option<std::collections::HashMap<String, geom::Point>>,
) -> Result<Graph> {
    let layout_config = layout::CoarseLayoutConfig::from_spacing(spacing);
    layout::apply_coarse_layout(graph, prior_positions, layout_config)
}

fn node_positions(graph: &Graph) -> std::collections::HashMap<String, geom::Point> {
    graph
        .nodes
        .iter()
        .map(|node| (node.id.clone(), geom::Point::new(node.x, node.y)))
        .collect()
}

fn build_layout_repair_candidates(
    graph: &Graph,
    config: &Config,
    outcome: &RenderOutcome,
) -> Vec<LayoutRepairCandidate> {
    use render::critic::FindingCode;

    let mut candidates = Vec::new();
    let base_positions = node_positions(graph);
    let secondary_delta = secondary_nudge_delta(graph.direction, &config.spacing);

    let mut has_primary_spacing_pressure = false;
    let mut has_secondary_spacing_pressure = false;
    let mut has_label_pressure = false;
    let mut has_canvas_clipping = false;

    for finding in &outcome.critic_report.findings {
        match finding.code {
            FindingCode::ChainTooCrampedLR
            | FindingCode::ArrowTouchesNodeBorder
            | FindingCode::ArrowTouchesSubgraphBorder
            | FindingCode::EdgeLabelCollidesWithNode => has_primary_spacing_pressure = true,
            FindingCode::CrowdedEdgeLabel => has_label_pressure = true,
            FindingCode::CanvasClipped => has_canvas_clipping = true,
            FindingCode::RouteCrossesNodeInterior => {
                for owner_id in &finding.owner_ids {
                    if graph.get_node(owner_id).is_some() {
                        push_targeted_node_nudge_candidates(
                            &mut candidates,
                            graph.direction,
                            &config.spacing,
                            &base_positions,
                            std::slice::from_ref(owner_id),
                            secondary_delta,
                        );
                    }
                }
            }
            FindingCode::SubgraphTitleCorrupted => {
                for owner_id in &finding.owner_ids {
                    if graph.get_subgraph(owner_id).is_some() {
                        push_targeted_subgraph_nudge_candidates(
                            &mut candidates,
                            graph,
                            &config.spacing,
                            &base_positions,
                            owner_id,
                            secondary_delta,
                        );
                    }
                }
            }
            FindingCode::RouteSymmetryImbalance => {
                push_branch_recenter_candidate(
                    &mut candidates,
                    graph,
                    &config.spacing,
                    &base_positions,
                    &finding.owner_ids,
                );
            }
            FindingCode::BranchSpacingImbalance => {
                push_branch_spacing_candidate(
                    &mut candidates,
                    graph,
                    &config.spacing,
                    &base_positions,
                    &finding.owner_ids,
                );
            }
            FindingCode::BranchCrowding => {
                has_secondary_spacing_pressure = true;
                push_branch_spread_candidate(
                    &mut candidates,
                    graph,
                    &config.spacing,
                    &base_positions,
                    &finding.owner_ids,
                );
            }
            _ => {}
        }

        if matches!(
            finding.code,
            FindingCode::CrowdedEdgeLabel | FindingCode::EdgeLabelCollidesWithNode
        ) {
            for owner_id in &finding.owner_ids {
                push_edge_label_nudge_candidates(
                    &mut candidates,
                    graph,
                    &config.spacing,
                    &base_positions,
                    owner_id,
                    secondary_delta,
                );
            }
        }

        if finding.code == FindingCode::ArrowTouchesNodeBorder {
            for owner_id in &finding.owner_ids {
                if graph.get_node(owner_id).is_some() {
                    push_targeted_node_nudge_candidates(
                        &mut candidates,
                        graph.direction,
                        &config.spacing,
                        &base_positions,
                        std::slice::from_ref(owner_id),
                        secondary_delta,
                    );
                }
            }
        }

        if finding.code == FindingCode::ArrowTouchesSubgraphBorder {
            for owner_id in &finding.owner_ids {
                if graph.get_subgraph(owner_id).is_some() {
                    push_targeted_subgraph_nudge_candidates(
                        &mut candidates,
                        graph,
                        &config.spacing,
                        &base_positions,
                        owner_id,
                        secondary_delta,
                    );
                }
            }
        }
    }

    if has_primary_spacing_pressure {
        let mut spacing = config.spacing.clone();
        if matches!(graph.direction, graph::Direction::LR | graph::Direction::RL) {
            spacing.col_spacing += 2;
            spacing.stem_length_horizontal += 2;
        } else {
            spacing.row_spacing += 1;
            spacing.stem_length_vertical += 1;
        }
        push_spacing_candidate(&mut candidates, spacing);
    }

    if has_label_pressure {
        let mut spacing = config.spacing.clone();
        spacing.row_spacing += 1;
        if matches!(graph.direction, graph::Direction::LR | graph::Direction::RL) {
            spacing.col_spacing += 1;
        }
        push_spacing_candidate(&mut candidates, spacing);
    }

    if has_secondary_spacing_pressure {
        let mut spacing = config.spacing.clone();
        match graph.direction {
            graph::Direction::TD | graph::Direction::TB | graph::Direction::BT => {
                spacing.col_spacing += 2;
            }
            graph::Direction::LR | graph::Direction::RL => {
                spacing.row_spacing += 1;
            }
        }
        push_spacing_candidate(&mut candidates, spacing);
    }

    if has_canvas_clipping {
        let mut spacing = config.spacing.clone();
        spacing.max_canvas_width = spacing.max_canvas_width.saturating_mul(2);
        spacing.max_canvas_height = spacing.max_canvas_height.saturating_mul(2);
        push_spacing_candidate(&mut candidates, spacing);
    }

    if candidates.is_empty() && config.optimize_render && outcome.critic_report.score > 0 {
        let mut spacing = config.spacing.clone();
        spacing.col_spacing += 1;
        spacing.row_spacing += 1;
        push_spacing_candidate(&mut candidates, spacing);
    }

    candidates
}

#[derive(Debug, Clone)]
struct LayoutRepairCandidate {
    spacing: SpacingConfig,
    prior_positions: Option<std::collections::HashMap<String, geom::Point>>,
}

fn is_better_outcome(candidate: &RenderOutcome, baseline: &RenderOutcome) -> bool {
    (
        candidate.critic_report.score,
        candidate.critic_report.findings.len(),
        candidate
            .semantic_frame
            .width
            .saturating_mul(candidate.semantic_frame.height),
    ) < (
        baseline.critic_report.score,
        baseline.critic_report.findings.len(),
        baseline
            .semantic_frame
            .width
            .saturating_mul(baseline.semantic_frame.height),
    )
}

fn push_spacing_candidate(candidates: &mut Vec<LayoutRepairCandidate>, spacing: SpacingConfig) {
    push_unique_layout_candidate(
        candidates,
        LayoutRepairCandidate {
            spacing,
            prior_positions: None,
        },
    );
}

fn push_unique_layout_candidate(
    candidates: &mut Vec<LayoutRepairCandidate>,
    candidate: LayoutRepairCandidate,
) {
    if !candidates
        .iter()
        .any(|existing| layout_candidate_eq(existing, &candidate))
    {
        candidates.push(candidate);
    }
}

fn spacing_eq(a: &SpacingConfig, b: &SpacingConfig) -> bool {
    a.box_height == b.box_height
        && a.box_min_width == b.box_min_width
        && a.box_padding == b.box_padding
        && a.row_spacing == b.row_spacing
        && a.col_spacing == b.col_spacing
        && a.node_margin == b.node_margin
        && a.subgraph_gutter == b.subgraph_gutter
        && a.stem_length_vertical == b.stem_length_vertical
        && a.stem_length_horizontal == b.stem_length_horizontal
        && a.max_label_width == b.max_label_width
        && a.max_canvas_width == b.max_canvas_width
        && a.max_canvas_height == b.max_canvas_height
        && a.cycle_gutter == b.cycle_gutter
}

fn layout_candidate_eq(a: &LayoutRepairCandidate, b: &LayoutRepairCandidate) -> bool {
    spacing_eq(&a.spacing, &b.spacing) && a.prior_positions == b.prior_positions
}

fn secondary_nudge_delta(direction: graph::Direction, spacing: &SpacingConfig) -> usize {
    match direction {
        graph::Direction::TD | graph::Direction::TB | graph::Direction::BT => {
            (spacing.col_spacing / 2).max(1)
        }
        graph::Direction::LR | graph::Direction::RL => (spacing.row_spacing / 2).max(1),
    }
}

fn push_edge_label_nudge_candidates(
    candidates: &mut Vec<LayoutRepairCandidate>,
    graph: &Graph,
    spacing: &SpacingConfig,
    base_positions: &std::collections::HashMap<String, geom::Point>,
    owner_id: &str,
    delta: usize,
) {
    let Some(edge) = graph.edges.iter().enumerate().find_map(|(idx, edge)| {
        (render::provenance::edge_owner_id(idx, edge) == owner_id).then_some(edge)
    }) else {
        return;
    };

    let outward = build_secondary_nudged_positions(
        base_positions,
        graph.direction,
        &[edge.from.as_str()],
        &[edge.to.as_str()],
        delta,
    );
    push_unique_layout_candidate(
        candidates,
        LayoutRepairCandidate {
            spacing: spacing.clone(),
            prior_positions: Some(outward),
        },
    );

    let inward = build_secondary_nudged_positions(
        base_positions,
        graph.direction,
        &[edge.to.as_str()],
        &[edge.from.as_str()],
        delta,
    );
    push_unique_layout_candidate(
        candidates,
        LayoutRepairCandidate {
            spacing: spacing.clone(),
            prior_positions: Some(inward),
        },
    );
}

fn push_targeted_node_nudge_candidates(
    candidates: &mut Vec<LayoutRepairCandidate>,
    direction: graph::Direction,
    spacing: &SpacingConfig,
    base_positions: &std::collections::HashMap<String, geom::Point>,
    node_ids: &[String],
    delta: usize,
) {
    let refs: Vec<&str> = node_ids.iter().map(String::as_str).collect();
    push_secondary_nudge_candidate(
        candidates,
        direction,
        spacing,
        base_positions,
        &refs,
        delta,
        true,
    );
    push_secondary_nudge_candidate(
        candidates,
        direction,
        spacing,
        base_positions,
        &refs,
        delta,
        false,
    );
}

fn push_targeted_subgraph_nudge_candidates(
    candidates: &mut Vec<LayoutRepairCandidate>,
    graph: &Graph,
    spacing: &SpacingConfig,
    base_positions: &std::collections::HashMap<String, geom::Point>,
    subgraph_id: &str,
    delta: usize,
) {
    let Some(subgraph) = graph.get_subgraph(subgraph_id) else {
        return;
    };
    let node_ids: Vec<&str> = subgraph.node_ids.iter().map(String::as_str).collect();
    push_secondary_nudge_candidate(
        candidates,
        graph.direction,
        spacing,
        base_positions,
        &node_ids,
        delta,
        true,
    );
    push_secondary_nudge_candidate(
        candidates,
        graph.direction,
        spacing,
        base_positions,
        &node_ids,
        delta,
        false,
    );
}

fn push_secondary_nudge_candidate(
    candidates: &mut Vec<LayoutRepairCandidate>,
    direction: graph::Direction,
    spacing: &SpacingConfig,
    base_positions: &std::collections::HashMap<String, geom::Point>,
    node_ids: &[&str],
    delta: usize,
    positive: bool,
) {
    let nudged = build_secondary_nudged_positions(
        base_positions,
        direction,
        if positive { &[] } else { node_ids },
        if positive { node_ids } else { &[] },
        delta,
    );
    push_unique_layout_candidate(
        candidates,
        LayoutRepairCandidate {
            spacing: spacing.clone(),
            prior_positions: Some(nudged),
        },
    );
}

fn push_branch_recenter_candidate(
    candidates: &mut Vec<LayoutRepairCandidate>,
    graph: &Graph,
    spacing: &SpacingConfig,
    base_positions: &std::collections::HashMap<String, geom::Point>,
    owner_ids: &[String],
) {
    let Some((anchor_id, branch_ids)) = owner_ids.split_first() else {
        return;
    };
    if branch_ids.is_empty() {
        return;
    }

    let Some(anchor) = graph.get_node(anchor_id) else {
        return;
    };
    let mut secondaries: Vec<usize> = branch_ids
        .iter()
        .filter_map(|node_id| graph.get_node(node_id))
        .map(|node| secondary_center(node, graph.direction))
        .collect();
    if secondaries.len() < 2 {
        return;
    }
    secondaries.sort_unstable();
    let min_secondary = secondaries[0];
    let max_secondary = secondaries[secondaries.len() - 1];
    if max_secondary <= min_secondary {
        return;
    }

    let anchor_secondary = secondary_center(anchor, graph.direction);
    let midpoint = (min_secondary + max_secondary) / 2;
    let delta = signed_delta(anchor_secondary, midpoint);
    if delta == 0 {
        return;
    }

    let refs: Vec<&str> = branch_ids.iter().map(String::as_str).collect();
    let nudged =
        build_signed_secondary_shift_positions(base_positions, graph.direction, &refs, delta);
    push_unique_layout_candidate(
        candidates,
        LayoutRepairCandidate {
            spacing: spacing.clone(),
            prior_positions: Some(nudged),
        },
    );
}

fn push_branch_spacing_candidate(
    candidates: &mut Vec<LayoutRepairCandidate>,
    graph: &Graph,
    spacing: &SpacingConfig,
    base_positions: &std::collections::HashMap<String, geom::Point>,
    owner_ids: &[String],
) {
    let Some((_anchor_id, branch_ids)) = owner_ids.split_first() else {
        return;
    };
    if branch_ids.len() < 3 {
        return;
    }

    let mut branches: Vec<(&str, usize)> = branch_ids
        .iter()
        .filter_map(|node_id| {
            graph
                .get_node(node_id)
                .map(|node| (node_id.as_str(), secondary_center(node, graph.direction)))
        })
        .collect();
    if branches.len() < 3 {
        return;
    }

    branches.sort_unstable_by_key(|(_, secondary)| *secondary);
    let min_secondary = branches[0].1;
    let max_secondary = branches[branches.len() - 1].1;
    if max_secondary <= min_secondary {
        return;
    }

    let span = max_secondary - min_secondary;
    let denominator = branches.len() - 1;
    let coords = orientation::OrientedCoords::new(graph.direction);
    let mut positions = base_positions.clone();
    let mut changed = false;

    for (index, (node_id, current_secondary)) in branches.iter().enumerate() {
        let target_secondary = min_secondary + ((span * index) + (denominator / 2)) / denominator;
        let delta = signed_delta(target_secondary, *current_secondary);
        if delta == 0 {
            continue;
        }

        let Some(point) = positions.get_mut(*node_id) else {
            continue;
        };
        match coords.secondary {
            orientation::Axis::Horizontal => point.x = apply_signed_delta(point.x, delta),
            orientation::Axis::Vertical => point.y = apply_signed_delta(point.y, delta),
        }
        changed = true;
    }

    if changed {
        push_unique_layout_candidate(
            candidates,
            LayoutRepairCandidate {
                spacing: spacing.clone(),
                prior_positions: Some(positions),
            },
        );
    }
}

fn push_branch_spread_candidate(
    candidates: &mut Vec<LayoutRepairCandidate>,
    graph: &Graph,
    spacing: &SpacingConfig,
    base_positions: &std::collections::HashMap<String, geom::Point>,
    owner_ids: &[String],
) {
    let Some((_anchor_id, branch_ids)) = owner_ids.split_first() else {
        return;
    };
    if branch_ids.len() < 2 {
        return;
    }

    let mut branches: Vec<(&str, usize, usize)> = branch_ids
        .iter()
        .filter_map(|node_id| {
            graph.get_node(node_id).map(|node| {
                (
                    node_id.as_str(),
                    secondary_start(node, graph.direction),
                    secondary_end(node, graph.direction),
                )
            })
        })
        .collect();
    if branches.len() < 2 {
        return;
    }

    branches.sort_unstable_by_key(|(_, start, _)| *start);
    let desired_gap = desired_branch_spread_gap(graph.direction);
    let original_min = branches[0].1;
    let original_max = branches[branches.len() - 1].2;

    let mut adjusted_starts = Vec::with_capacity(branches.len());
    let mut prev_end = None;
    for (_, start, end) in &branches {
        let span = end.saturating_sub(*start);
        let adjusted_start =
            prev_end.map_or(*start, |end_bound| (*start).max(end_bound + desired_gap));
        adjusted_starts.push(adjusted_start);
        prev_end = Some(adjusted_start + span);
    }

    let Some(&last_start) = adjusted_starts.last() else {
        return;
    };
    let last_span = branches[branches.len() - 1]
        .2
        .saturating_sub(branches[branches.len() - 1].1);
    let adjusted_min = adjusted_starts[0];
    let adjusted_max = last_start + last_span;
    let recenter_delta = signed_delta(
        (original_min + original_max) / 2,
        (adjusted_min + adjusted_max) / 2,
    );

    let coords = orientation::OrientedCoords::new(graph.direction);
    let mut positions = base_positions.clone();
    let mut changed = false;

    for ((node_id, start, _), adjusted_start) in branches.iter().zip(adjusted_starts) {
        let final_start = apply_signed_delta(adjusted_start, recenter_delta);
        let delta = signed_delta(final_start, *start);
        if delta == 0 {
            continue;
        }

        let Some(point) = positions.get_mut(*node_id) else {
            continue;
        };
        match coords.secondary {
            orientation::Axis::Horizontal => point.x = apply_signed_delta(point.x, delta),
            orientation::Axis::Vertical => point.y = apply_signed_delta(point.y, delta),
        }
        changed = true;
    }

    if changed {
        push_unique_layout_candidate(
            candidates,
            LayoutRepairCandidate {
                spacing: spacing.clone(),
                prior_positions: Some(positions),
            },
        );
    }
}

fn build_secondary_nudged_positions(
    base_positions: &std::collections::HashMap<String, geom::Point>,
    direction: graph::Direction,
    negative_ids: &[&str],
    positive_ids: &[&str],
    delta: usize,
) -> std::collections::HashMap<String, geom::Point> {
    let mut positions = base_positions.clone();
    let coords = orientation::OrientedCoords::new(direction);

    for node_id in negative_ids {
        if let Some(point) = positions.get_mut(*node_id) {
            match coords.secondary {
                orientation::Axis::Horizontal => point.x = point.x.saturating_sub(delta),
                orientation::Axis::Vertical => point.y = point.y.saturating_sub(delta),
            }
        }
    }

    for node_id in positive_ids {
        if let Some(point) = positions.get_mut(*node_id) {
            match coords.secondary {
                orientation::Axis::Horizontal => point.x += delta,
                orientation::Axis::Vertical => point.y += delta,
            }
        }
    }

    positions
}

fn build_signed_secondary_shift_positions(
    base_positions: &std::collections::HashMap<String, geom::Point>,
    direction: graph::Direction,
    node_ids: &[&str],
    delta: isize,
) -> std::collections::HashMap<String, geom::Point> {
    let mut positions = base_positions.clone();
    let coords = orientation::OrientedCoords::new(direction);

    for node_id in node_ids {
        if let Some(point) = positions.get_mut(*node_id) {
            match coords.secondary {
                orientation::Axis::Horizontal => point.x = apply_signed_delta(point.x, delta),
                orientation::Axis::Vertical => point.y = apply_signed_delta(point.y, delta),
            }
        }
    }

    positions
}

fn secondary_center(node: &Node, direction: graph::Direction) -> usize {
    match direction {
        graph::Direction::TD | graph::Direction::TB | graph::Direction::BT => node.center_x(),
        graph::Direction::LR | graph::Direction::RL => node.center_y(),
    }
}

fn secondary_start(node: &Node, direction: graph::Direction) -> usize {
    match direction {
        graph::Direction::TD | graph::Direction::TB | graph::Direction::BT => node.x,
        graph::Direction::LR | graph::Direction::RL => node.y,
    }
}

fn secondary_end(node: &Node, direction: graph::Direction) -> usize {
    match direction {
        graph::Direction::TD | graph::Direction::TB | graph::Direction::BT => node.x + node.width,
        graph::Direction::LR | graph::Direction::RL => {
            node.y + node.height.max(crate::style::BOX_HEIGHT)
        }
    }
}

fn desired_branch_spread_gap(direction: graph::Direction) -> usize {
    match direction {
        graph::Direction::TD | graph::Direction::TB | graph::Direction::BT => 3,
        graph::Direction::LR | graph::Direction::RL => 1,
    }
}

fn signed_delta(anchor: usize, midpoint: usize) -> isize {
    if anchor >= midpoint {
        (anchor - midpoint) as isize
    } else {
        -((midpoint - anchor) as isize)
    }
}

fn apply_signed_delta(value: usize, delta: isize) -> usize {
    if delta >= 0 {
        value.saturating_add(delta as usize)
    } else {
        value.saturating_sub((-delta) as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_outcome(findings: Vec<CriticFinding>) -> RenderOutcome {
        RenderOutcome {
            output: String::new(),
            semantic_frame: render::semantic::SemanticFrame::default(),
            critic_report: CriticReport {
                score: findings.iter().map(|finding| finding.penalty).sum(),
                findings,
                notes: Vec::new(),
            },
            warnings: Vec::new(),
            optimized: false,
            repair_passes: 0,
            layout_attempts: 1,
            layout_repairs_applied: 0,
        }
    }

    #[test]
    fn layout_repair_candidates_include_targeted_node_nudges() {
        let mut graph = Graph::new();
        graph.direction = graph::Direction::TD;
        let mut node = Node::new("A", "A");
        node.x = 4;
        node.y = 2;
        node.width = 5;
        graph.add_node(node);

        let outcome = dummy_outcome(vec![CriticFinding {
            code: FindingCode::RouteCrossesNodeInterior,
            severity: FindingSeverity::Warning,
            penalty: 12,
            message: "routing intrudes into node interior A".to_string(),
            cells: vec![(5, 3)],
            owner_ids: vec!["A".to_string()],
        }]);

        let candidates = build_layout_repair_candidates(&graph, &Config::default(), &outcome);
        assert!(candidates.iter().any(|candidate| {
            candidate
                .prior_positions
                .as_ref()
                .and_then(|positions| positions.get("A"))
                .is_some_and(|point| point.x != 4)
        }));
    }

    #[test]
    fn layout_repair_candidates_include_targeted_edge_label_nudges() {
        let mut graph = Graph::new();
        graph.direction = graph::Direction::LR;
        let mut a = Node::new("A", "A");
        a.x = 0;
        a.y = 0;
        let mut b = Node::new("B", "B");
        b.x = 12;
        b.y = 0;
        graph.add_node(a);
        graph.add_node(b);
        graph.add_edge(Edge::with_label("A", "B", "label"));

        let outcome = dummy_outcome(vec![CriticFinding {
            code: FindingCode::CrowdedEdgeLabel,
            severity: FindingSeverity::Info,
            penalty: 8,
            message: "edge label edge:0:A->B is crowded".to_string(),
            cells: vec![(6, 1)],
            owner_ids: vec!["edge:0:A->B".to_string()],
        }]);

        let candidates = build_layout_repair_candidates(&graph, &Config::default(), &outcome);
        assert!(candidates.iter().any(|candidate| {
            candidate
                .prior_positions
                .as_ref()
                .and_then(|positions| positions.get("A").zip(positions.get("B")))
                .is_some_and(|(a, b)| a.y != 0 || b.y != 0)
        }));
    }

    #[test]
    fn layout_repair_candidates_include_branch_recenter_positions() {
        let mut graph = Graph::new();
        graph.direction = graph::Direction::TD;

        let mut anchor = Node::new("A", "A");
        anchor.x = 8;
        anchor.y = 0;
        anchor.width = 5;
        let mut left = Node::new("B", "B");
        left.x = 0;
        left.y = 8;
        left.width = 5;
        let mut right = Node::new("C", "C");
        right.x = 20;
        right.y = 8;
        right.width = 5;

        graph.add_node(anchor);
        graph.add_node(left);
        graph.add_node(right);
        graph.add_edge(Edge::new("A", "B"));
        graph.add_edge(Edge::new("A", "C"));

        let outcome = dummy_outcome(vec![CriticFinding {
            code: FindingCode::RouteSymmetryImbalance,
            severity: FindingSeverity::Info,
            penalty: 6,
            message: "fan-out at A is off-center".to_string(),
            cells: Vec::new(),
            owner_ids: vec!["A".to_string(), "B".to_string(), "C".to_string()],
        }]);

        let candidates = build_layout_repair_candidates(&graph, &Config::default(), &outcome);
        assert!(candidates.iter().any(|candidate| {
            candidate
                .prior_positions
                .as_ref()
                .and_then(|positions| positions.get("B").zip(positions.get("C")))
                .is_some_and(|(b, c)| b.x > 0 || c.x < 20)
        }));
    }

    #[test]
    fn layout_repair_candidates_include_branch_spacing_positions() {
        let mut graph = Graph::new();
        graph.direction = graph::Direction::TD;

        let mut anchor = Node::new("A", "A");
        anchor.x = 20;
        anchor.y = 0;
        anchor.width = 9;

        let mut left = Node::new("B", "B");
        left.x = 0;
        left.y = 8;
        left.width = 7;

        let mut middle = Node::new("C", "C");
        middle.x = 12;
        middle.y = 8;
        middle.width = 7;

        let mut right = Node::new("D", "D");
        right.x = 42;
        right.y = 8;
        right.width = 7;

        graph.add_node(anchor);
        graph.add_node(left);
        graph.add_node(middle);
        graph.add_node(right);
        graph.add_edge(Edge::new("A", "B"));
        graph.add_edge(Edge::new("A", "C"));
        graph.add_edge(Edge::new("A", "D"));

        let outcome = dummy_outcome(vec![CriticFinding {
            code: FindingCode::BranchSpacingImbalance,
            severity: FindingSeverity::Info,
            penalty: 5,
            message: "fan-out at A has uneven branch spacing".to_string(),
            cells: Vec::new(),
            owner_ids: vec![
                "A".to_string(),
                "B".to_string(),
                "C".to_string(),
                "D".to_string(),
            ],
        }]);

        let candidates = build_layout_repair_candidates(&graph, &Config::default(), &outcome);
        assert!(candidates.iter().any(|candidate| {
            candidate
                .prior_positions
                .as_ref()
                .and_then(|positions| {
                    positions
                        .get("B")
                        .zip(positions.get("C"))
                        .zip(positions.get("D"))
                })
                .is_some_and(|((b, c), d)| b.x == 0 && c.x > 12 && d.x == 42)
        }));
    }

    #[test]
    fn layout_repair_candidates_include_branch_spread_positions() {
        let mut graph = Graph::new();
        graph.direction = graph::Direction::TD;

        let mut anchor = Node::new("A", "A");
        anchor.x = 12;
        anchor.y = 0;
        anchor.width = 9;

        let mut left = Node::new("B", "B");
        left.x = 4;
        left.y = 8;
        left.width = 7;

        let mut middle = Node::new("C", "C");
        middle.x = 11;
        middle.y = 8;
        middle.width = 7;

        let mut right = Node::new("D", "D");
        right.x = 18;
        right.y = 8;
        right.width = 7;

        graph.add_node(anchor);
        graph.add_node(left);
        graph.add_node(middle);
        graph.add_node(right);
        graph.add_edge(Edge::new("A", "B"));
        graph.add_edge(Edge::new("A", "C"));
        graph.add_edge(Edge::new("A", "D"));

        let outcome = dummy_outcome(vec![CriticFinding {
            code: FindingCode::BranchCrowding,
            severity: FindingSeverity::Info,
            penalty: 6,
            message: "fan-out at A has cramped sibling gaps".to_string(),
            cells: Vec::new(),
            owner_ids: vec![
                "A".to_string(),
                "B".to_string(),
                "C".to_string(),
                "D".to_string(),
            ],
        }]);

        let candidates = build_layout_repair_candidates(&graph, &Config::default(), &outcome);
        assert!(candidates.iter().any(|candidate| {
            candidate
                .prior_positions
                .as_ref()
                .and_then(|positions| {
                    positions
                        .get("B")
                        .zip(positions.get("C"))
                        .zip(positions.get("D"))
                })
                .is_some_and(|((b, c), d)| b.x < 4 && c.x == 11 && d.x > 18)
        }));
        assert!(candidates
            .iter()
            .any(|candidate| candidate.prior_positions.is_none()));
    }
}
