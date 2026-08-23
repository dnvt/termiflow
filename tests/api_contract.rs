//! A0 compile probes for the documented public API and configuration order.
//!
//! These tests intentionally use the public facade as an external crate would.
//! They are compatibility probes, not a new production abstraction.

use termiflow::graph::{Direction, Edge, Graph, Node, NodeShape, Subgraph};
use termiflow::render;
use termiflow::{
    display_width, parse, parse_json_graph, render_json, render_with_feedback, BaseStyle,
    CompositeStyle, Config, ParseConfig, RenderOptions, SpacingConfig, SpacingMode,
    DEFAULT_DISPLAY_PROFILE,
};

#[test]
fn public_facade_and_graph_types_compile() {
    let parsed = parse("graph LR\n    A[Start] --> B[End]\n", false).unwrap();
    assert_eq!(parsed.graph.direction, Direction::LR);
    assert_eq!(parsed.graph.nodes.len(), 2);

    let mut graph = Graph::new();
    graph.add_node(Node::with_shape("A", "Start", NodeShape::Rounded));
    graph.add_node(Node::new("B", "End"));
    graph.add_edge(Edge::new("A", "B"));
    let mut group = Subgraph::new("group", Some("Group".to_string()));
    group.add_node("A");
    graph.add_subgraph(group);
    graph.associate_node_with_subgraph("A", "group");
    assert!(graph.get_node("A").is_some());
    assert!(graph.has_subgraphs());

    let config = Config::builder().build(&ParseConfig::default());
    let output = render::render(&graph, &config).unwrap();
    assert!(!output.is_empty());
}

#[test]
fn high_level_mermaid_json_and_render_feedback_remain_available() {
    let source = "graph TD\n    A[Start] --> B[End]\n";
    let options = RenderOptions::new()
        .with_style(BaseStyle::Ascii)
        .with_max_label(12)
        .with_max_edge_label_width(12)
        .with_wrap_labels(true)
        .with_max_label_lines(2)
        .with_crop(true)
        .with_pad(1)
        .with_compact(false)
        .with_optimize_render(true)
        .with_render_repair_passes(2)
        .with_layout_repair_passes(2)
        .with_debug_critic(true);

    let output = termiflow::render(source, options.clone()).unwrap();
    assert!(!output.is_empty());
    let feedback = render_with_feedback(source, options).unwrap();
    assert_eq!(feedback.output, output);

    let json = r#"{
        "direction": "TD",
        "nodes": [{"id":"A","label":"Start"},{"id":"B","label":"End"}],
        "edges": [{"from":"A","to":"B"}]
    }"#;
    let (graph, _) = parse_json_graph(json).unwrap();
    assert_eq!(graph.nodes.len(), 2);
    assert!(!render_json(json, RenderOptions::default())
        .unwrap()
        .is_empty());
}

#[test]
fn route_clarity_audit_surface_is_public_and_hash_bound() {
    let source = include_str!("fixtures/inputs/collision_sibling_triple_bt.md");
    let parsed = parse(source, false).unwrap();
    let config = Config::from_parse_config(&parsed.config);
    let policy = termiflow::effective_render_policy(
        &config,
        parsed.graph.direction,
        DEFAULT_DISPLAY_PROFILE.name,
        "Fixed",
        false,
        false,
    );
    let frame = termiflow::render_with_feedback(source, RenderOptions::default()).unwrap();
    let report = termiflow::analyze_route_clarity_for_audit(
        source.as_bytes(),
        frame.output.as_bytes(),
        &policy,
        false,
    )
    .unwrap();

    assert_eq!(report["source_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(report["frame_sha256"].as_str().unwrap().len(), 64);
    assert!(matches!(
        report["status"].as_str(),
        Some("risk" | "inconclusive")
    ));
    assert!(!report["findings"].as_array().unwrap().is_empty());
}

#[test]
fn configuration_precedence_is_defaults_then_directives_then_api_override() {
    let directives = ParseConfig {
        max_label: Some(7),
        max_edge_label: Some(8),
        wrap_labels: Some(true),
        max_label_lines: Some(3),
        spacing_mode: Some(SpacingMode::Compact),
        ..ParseConfig::default()
    };

    let from_directives = Config::from_parse_config(&directives);
    assert_eq!(from_directives.max_label_width, 7);
    assert_eq!(from_directives.max_edge_label_width, 8);
    assert!(from_directives.wrap_labels);
    assert_eq!(from_directives.max_label_lines, 3);
    assert_eq!(
        from_directives.spacing.row_spacing,
        SpacingConfig::compact().row_spacing
    );

    let from_api = Config::builder()
        .max_label_width(11)
        .max_edge_label_width(12)
        .wrap_labels(false)
        .spacing(SpacingConfig::spacious())
        .build(&directives);
    assert_eq!(from_api.max_label_width, 11);
    assert_eq!(from_api.max_edge_label_width, 12);
    assert!(!from_api.wrap_labels);
    assert_eq!(
        from_api.spacing.row_spacing,
        SpacingConfig::spacious().row_spacing
    );
}

#[test]
fn display_and_layer_contracts_are_public_and_deterministic() {
    assert!(display_width("hello") > 0);
    assert_eq!(
        DEFAULT_DISPLAY_PROFILE.name,
        "unicode-width+extended-graphemes"
    );

    let labels: Vec<_> = termiflow::current_render_layer_contract()
        .iter()
        .map(|spec| spec.layer.label())
        .collect();
    assert_eq!(
        labels,
        vec![
            "reservation",
            "topology",
            "semantic-cells",
            "glyph-projection",
            "terminal-transport"
        ]
    );
    assert_eq!(
        CompositeStyle::default()
            .to_style_chars(BaseStyle::Unicode)
            .h,
        '─'
    );
}
