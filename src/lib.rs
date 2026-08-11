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
//! 2. **Layout** - Assign coordinates using the coarse layered layout pipeline
//! 3. **Render** - Draw boxes and edges on a 2D canvas

// ============================================================================
// Modules
// ============================================================================

pub mod config;
pub mod crossing;
pub mod display_profile;
pub mod geom;
pub mod graph;
pub(crate) mod indexed_graph;
pub mod json_input;
pub mod layout;
pub(crate) mod layout_render_contract;
mod layout_repair;
pub(crate) mod layout_snapshot;
pub mod measure;
pub mod orientation;
pub mod parser;
pub mod portals;
pub mod render;
pub(crate) mod route_plan;
pub(crate) mod runtime;
pub mod scaling;
pub mod spacing;
pub mod style;
pub mod tui;

// ============================================================================
// Re-exports for convenient access
// ============================================================================

pub use config::{effective_render_policy, Config, ConfigBuilder};
pub use crossing::{CrossingConfig, CrossingMinimizer, Heuristic};
pub use display_profile::{
    display_char_width, display_width, graphemes, split_text_to_width_chunks, truncate_to_width,
    DisplayProfile, DEFAULT_DISPLAY_PROFILE,
};
pub use graph::{Edge, EdgeKind, Graph, Node};
pub use json_input::parse_json_graph;
pub use layout::coarse_waterfall;
pub use parser::{parse, ParseConfig, ParseResult};
pub use render::critic::{
    AuditSummary, AuditVerdict, CriticFinding, CriticReport, FindingCode, FindingSeverity,
};
pub use render::render as render_canvas;
pub use render::{
    current_render_layer_contract, EdgeTrace, GeometryTrace, NodeTrace, RectTrace, RenderLayer,
    RenderLayerContract, RenderLayerSpec, SegmentAxis, SegmentTrace, SubgraphTrace,
};
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
    runtime::with_captured(|| {
        let parse_result = parser::parse(input, options.strict)?;
        render_parse_result_with_feedback(parse_result, options)
    })
}

/// Render a TermiFlow JSON graph (see `parse_json_graph`) to ASCII/Unicode art.
pub fn render_json(input: &str, options: RenderOptions) -> Result<String> {
    runtime::with_captured(|| {
        let (graph, config) = json_input::parse_json_graph(input)?;
        Ok(render_parse_result_with_feedback(ParseResult { graph, config }, options)?.output)
    })
}

fn render_parse_result_with_feedback(
    parse_result: ParseResult,
    options: RenderOptions,
) -> Result<RenderOutcome> {
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
    runtime::with_captured(|| layout_and_render_with_feedback_inner(graph, config))
}

fn layout_and_render_with_feedback_inner(
    graph: Graph,
    config: Config,
) -> Result<(Graph, RenderOutcome)> {
    let mut best_config = config.clone();
    let (mut best_graph, best_contract) = layout_graph(graph.clone(), &best_config.spacing, None)?;
    let mut best_outcome = render::render_with_feedback_with_contract(
        &best_graph,
        &best_config,
        best_contract.as_ref(),
    )?;
    best_outcome.warnings = best_graph.warnings.clone();
    best_outcome.layout_attempts = 1;

    let layout_repair_passes = runtime::current()
        .compatibility
        .layout_repair_passes
        .unwrap_or(config.layout_repair_passes);

    if config.optimize_render {
        let mut layout_repairs_applied = 0;
        let mut attempts = 1;
        let mut prior_positions = Some(layout_repair::node_positions(&best_graph));
        let mut budget_warnings = Vec::new();

        for _ in 0..layout_repair_passes {
            let candidate_batch = layout_repair::build_layout_repair_candidates(
                &best_graph,
                &best_config,
                &best_outcome,
            );
            if candidate_batch.candidates.is_empty() {
                break;
            }
            if candidate_batch.omitted > 0 {
                budget_warnings.push(layout_repair::budget_warning(candidate_batch.omitted));
            }

            let mut improved: Option<(
                Config,
                Graph,
                Option<crate::layout_render_contract::BtSiblingEndpointContract>,
                RenderOutcome,
            )> = None;

            for candidate in candidate_batch.candidates {
                attempts += 1;
                let mut candidate_config = best_config.clone();
                candidate_config.spacing = candidate.spacing;
                let candidate_prior_positions = candidate
                    .prior_positions
                    .or_else(|| prior_positions.clone());
                let (candidate_graph, candidate_contract) = layout_graph(
                    graph.clone(),
                    &candidate_config.spacing,
                    candidate_prior_positions,
                )?;
                let mut candidate_outcome = render::render_with_feedback_with_contract(
                    &candidate_graph,
                    &candidate_config,
                    candidate_contract.as_ref(),
                )?;
                candidate_outcome.warnings = candidate_graph.warnings.clone();

                let should_promote = improved.as_ref().map_or_else(
                    || layout_repair::is_better_outcome(&candidate_outcome, &best_outcome),
                    |(_, _, _, current_best)| {
                        layout_repair::is_better_outcome(&candidate_outcome, current_best)
                    },
                );

                if should_promote {
                    improved = Some((
                        candidate_config,
                        candidate_graph,
                        candidate_contract,
                        candidate_outcome,
                    ));
                }
            }

            let Some((candidate_config, candidate_graph, _candidate_contract, candidate_outcome)) =
                improved
            else {
                break;
            };

            if !layout_repair::is_better_outcome(&candidate_outcome, &best_outcome) {
                break;
            }

            best_config = candidate_config;
            best_graph = candidate_graph;
            best_outcome = candidate_outcome;
            prior_positions = Some(layout_repair::node_positions(&best_graph));
            layout_repairs_applied += 1;
            best_outcome.layout_repairs_applied = layout_repairs_applied;
            best_outcome.layout_attempts = attempts;
        }

        best_outcome.layout_repairs_applied = layout_repairs_applied;
        best_outcome.layout_attempts = attempts;
        best_outcome.warnings.extend(budget_warnings);
    }

    Ok((best_graph, best_outcome))
}

fn layout_graph(
    graph: Graph,
    spacing: &SpacingConfig,
    prior_positions: Option<std::collections::HashMap<String, geom::Point>>,
) -> Result<(
    Graph,
    Option<crate::layout_render_contract::BtSiblingEndpointContract>,
)> {
    let layout_config = layout::CoarseLayoutConfig::from_spacing(spacing);
    layout::apply_coarse_layout_with_contract(graph, prior_positions, layout_config)
}

#[cfg(test)]
mod tests {
    use super::layout_repair::build_layout_repair_candidates;
    use super::*;

    fn dummy_outcome(findings: Vec<CriticFinding>) -> RenderOutcome {
        RenderOutcome {
            output: String::new(),
            semantic_frame: render::semantic::SemanticFrame::default(),
            display_semantic_frame: render::semantic::SemanticFrame::default(),
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
            portal_trace: render::trace::PortalTrace::default(),
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

        let candidates =
            build_layout_repair_candidates(&graph, &Config::default(), &outcome).candidates;
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

        let candidates =
            build_layout_repair_candidates(&graph, &Config::default(), &outcome).candidates;
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

        let candidates =
            build_layout_repair_candidates(&graph, &Config::default(), &outcome).candidates;
        assert!(candidates.iter().any(|candidate| {
            candidate
                .prior_positions
                .as_ref()
                .and_then(|positions| positions.get("B").zip(positions.get("C")))
                .is_some_and(|(b, c)| b.x > 0 || c.x < 20)
        }));
    }

    #[test]
    fn layout_repair_candidates_include_upstream_context_for_fanout() {
        let mut graph = Graph::new();
        graph.direction = graph::Direction::TD;

        let mut upstream_left = Node::new("A", "A");
        upstream_left.x = 0;
        upstream_left.y = 0;
        upstream_left.width = 5;
        let mut upstream_right = Node::new("B", "B");
        upstream_right.x = 20;
        upstream_right.y = 0;
        upstream_right.width = 5;
        let mut anchor = Node::new("C", "C");
        anchor.x = 10;
        anchor.y = 8;
        anchor.width = 5;
        let mut branch_left = Node::new("D", "D");
        branch_left.x = 0;
        branch_left.y = 16;
        branch_left.width = 5;
        let mut branch_right = Node::new("E", "E");
        branch_right.x = 30;
        branch_right.y = 16;
        branch_right.width = 5;

        for node in [
            upstream_left,
            upstream_right,
            anchor,
            branch_left,
            branch_right,
        ] {
            graph.add_node(node);
        }
        graph.add_edge(Edge::new("A", "C"));
        graph.add_edge(Edge::new("B", "C"));
        graph.add_edge(Edge::new("C", "D"));
        graph.add_edge(Edge::new("C", "E"));

        let outcome = dummy_outcome(vec![CriticFinding {
            code: FindingCode::RouteSymmetryImbalance,
            severity: FindingSeverity::Info,
            penalty: 6,
            message: "fan-out at C is off-center".to_string(),
            cells: Vec::new(),
            owner_ids: vec!["C".to_string(), "D".to_string(), "E".to_string()],
        }]);

        let candidates =
            build_layout_repair_candidates(&graph, &Config::default(), &outcome).candidates;
        assert!(candidates.iter().any(|candidate| {
            candidate
                .prior_positions
                .as_ref()
                .and_then(|positions| {
                    positions
                        .get("A")
                        .zip(positions.get("B"))
                        .zip(positions.get("C"))
                        .zip(positions.get("D"))
                        .zip(positions.get("E"))
                })
                .is_some_and(|((((a, b), c), d), e)| {
                    a.x == 5 && b.x == 25 && c.x == 15 && d.x == 0 && e.x == 30
                })
        }));
    }

    #[test]
    fn layout_repair_candidates_include_downstream_context_for_fanin() {
        let mut graph = Graph::new();
        graph.direction = graph::Direction::TD;

        let mut source_left = Node::new("A", "A");
        source_left.x = 0;
        source_left.y = 0;
        source_left.width = 5;
        let mut source_right = Node::new("B", "B");
        source_right.x = 30;
        source_right.y = 0;
        source_right.width = 5;
        let mut anchor = Node::new("C", "C");
        anchor.x = 10;
        anchor.y = 8;
        anchor.width = 5;
        let mut downstream = Node::new("D", "D");
        downstream.x = 40;
        downstream.y = 16;
        downstream.width = 5;

        for node in [source_left, source_right, anchor, downstream] {
            graph.add_node(node);
        }
        graph.add_edge(Edge::new("A", "C"));
        graph.add_edge(Edge::new("B", "C"));
        graph.add_edge(Edge::new("C", "D"));

        let outcome = dummy_outcome(vec![CriticFinding {
            code: FindingCode::RouteSymmetryImbalance,
            severity: FindingSeverity::Info,
            penalty: 6,
            message: "fan-in at C is off-center".to_string(),
            cells: Vec::new(),
            owner_ids: vec!["C".to_string(), "A".to_string(), "B".to_string()],
        }]);

        let candidates =
            build_layout_repair_candidates(&graph, &Config::default(), &outcome).candidates;
        assert!(candidates.iter().any(|candidate| {
            candidate
                .prior_positions
                .as_ref()
                .and_then(|positions| {
                    positions
                        .get("A")
                        .zip(positions.get("B"))
                        .zip(positions.get("C"))
                        .zip(positions.get("D"))
                })
                .is_some_and(|(((a, b), c), d)| a.x == 0 && b.x == 30 && c.x == 15 && d.x == 45)
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

        let candidates =
            build_layout_repair_candidates(&graph, &Config::default(), &outcome).candidates;
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

        let candidates =
            build_layout_repair_candidates(&graph, &Config::default(), &outcome).candidates;
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
