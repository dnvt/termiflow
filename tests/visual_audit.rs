use std::fs;

use termiflow::{AuditVerdict, BaseStyle, CompositeStyle, RenderOptions};

const CURATED_FIXTURES: &[&str] = &[
    "tests/fixtures/inputs/flow_simple_td.md",
    "tests/fixtures/inputs/flow_simple_lr.md",
    "tests/fixtures/inputs/edge_converge_bt.md",
    "tests/fixtures/inputs/edge_branch_bt.md",
    "tests/fixtures/inputs/converge_cascade_td.md",
    "tests/fixtures/inputs/converge_cascade_bt.md",
    "tests/fixtures/inputs/converge_cascade_lr.md",
    "tests/fixtures/inputs/converge_cascade_rl.md",
    "tests/fixtures/inputs/converge_deep_bt.md",
    "tests/fixtures/inputs/crossing_grid_td.md",
    "tests/fixtures/inputs/crossing_grid_bt.md",
    "tests/fixtures/inputs/crossing_grid_lr.md",
    "tests/fixtures/inputs/crossing_grid_rl.md",
    "tests/fixtures/inputs/subgraph_single_td.md",
    "tests/fixtures/inputs/subgraph_fanout_td.md",
    "tests/fixtures/inputs/subgraph_complex_td.md",
    "tests/fixtures/inputs/subgraph_complex_bt.md",
    "tests/fixtures/inputs/subgraph_multi_td.md",
    "tests/fixtures/inputs/subgraph_labels_td.md",
    "tests/fixtures/inputs/subgraph_labels_bt.md",
    "tests/fixtures/inputs/subgraph_fanin_bt.md",
    "tests/fixtures/inputs/collision_sibling_subgraphs_bt.md",
    "tests/fixtures/inputs/collision_sibling_subgraphs_td.md",
    "tests/fixtures/inputs/label_basic_td.md",
    "tests/fixtures/inputs/label_basic_bt.md",
    "tests/fixtures/inputs/label_edge_long_lr.md",
    "tests/fixtures/inputs/label_junction_td.md",
    "tests/fixtures/inputs/collision_edge_corner_lr.md",
    "tests/fixtures/inputs/junction_mixed_bt.md",
    "tests/fixtures/inputs/cycle_long_lr.md",
    "tests/fixtures/inputs/cycle_long_rl.md",
    "tests/fixtures/inputs/cycle_nested_lr.md",
    "tests/fixtures/inputs/cycle_selfloop_td.md",
    "tests/fixtures/inputs/cycle_selfloop_bt.md",
    "tests/fixtures/inputs/cycle_selfloop_lr.md",
    "tests/fixtures/inputs/cycle_simple_lr.md",
    "tests/fixtures/inputs/cycle_simple_rl.md",
];

fn assert_fixture_suite_is_visually_clean(style: BaseStyle) {
    let mut failures = Vec::new();

    for fixture in CURATED_FIXTURES {
        let input = fs::read_to_string(fixture).unwrap_or_else(|err| {
            panic!("failed to read fixture {}: {}", fixture, err);
        });

        let outcome = termiflow::render_with_feedback(
            &input,
            RenderOptions::new()
                .with_style(style)
                .with_optimize_render(true),
        )
        .unwrap_or_else(|err| panic!("failed to render fixture {}: {}", fixture, err));

        let summary = outcome.critic_report.audit_summary();
        if summary.verdict != AuditVerdict::Clean {
            failures.push(format!(
                "{} [{:?}] verdict={:?} score={} findings={:?}\n{}",
                fixture, style, summary.verdict, summary.score, summary.highlights, outcome.output
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "visual audit failures:\n\n{}",
        failures.join("\n\n")
    );
}

fn skewed_branch_graph() -> termiflow::Graph {
    let mut graph = termiflow::Graph::new();
    graph.direction = termiflow::graph::Direction::TD;

    let mut anchor = termiflow::Node::new("A", "Start");
    anchor.x = 20;
    anchor.y = 0;
    anchor.width = 9;

    let mut left = termiflow::Node::new("B", "Left");
    left.x = 0;
    left.y = 8;
    left.width = 7;

    let mut middle = termiflow::Node::new("C", "Middle");
    middle.x = 12;
    middle.y = 8;
    middle.width = 7;

    let mut right = termiflow::Node::new("D", "Right");
    right.x = 42;
    right.y = 8;
    right.width = 7;

    graph.add_node(anchor);
    graph.add_node(left);
    graph.add_node(middle);
    graph.add_node(right);
    graph.add_edge(termiflow::Edge::new("A", "B"));
    graph.add_edge(termiflow::Edge::new("A", "C"));
    graph.add_edge(termiflow::Edge::new("A", "D"));
    graph
}

fn dense_branch_graph() -> termiflow::Graph {
    let mut graph = termiflow::Graph::new();
    graph.direction = termiflow::graph::Direction::TD;

    let mut anchor = termiflow::Node::new("A", "Start");
    anchor.x = 12;
    anchor.y = 0;
    anchor.width = 9;

    let mut left = termiflow::Node::new("B", "Left");
    left.x = 4;
    left.y = 8;
    left.width = 7;

    let mut middle = termiflow::Node::new("C", "Middle");
    middle.x = 11;
    middle.y = 8;
    middle.width = 7;

    let mut right = termiflow::Node::new("D", "Right");
    right.x = 18;
    right.y = 8;
    right.width = 7;

    graph.add_node(anchor);
    graph.add_node(left);
    graph.add_node(middle);
    graph.add_node(right);
    graph.add_edge(termiflow::Edge::new("A", "B"));
    graph.add_edge(termiflow::Edge::new("A", "C"));
    graph.add_edge(termiflow::Edge::new("A", "D"));
    graph
}

#[test]
fn curated_ascii_fixtures_pass_visual_audit() {
    assert_fixture_suite_is_visually_clean(BaseStyle::Ascii);
}

#[test]
fn curated_unicode_fixtures_pass_visual_audit() {
    assert_fixture_suite_is_visually_clean(BaseStyle::Unicode);
}

#[test]
fn skewed_branch_spacing_is_flagged_by_visual_audit() {
    let mut config = termiflow::Config::default();
    config.composite_style = CompositeStyle::from_base(BaseStyle::Ascii);

    let outcome = termiflow::render_canvas_with_feedback(&skewed_branch_graph(), &config)
        .unwrap_or_else(|err| panic!("failed to render skewed branch graph: {}", err));
    let summary = outcome.critic_report.audit_summary();

    assert_eq!(summary.verdict, AuditVerdict::NeedsReview);
    assert!(outcome
        .critic_report
        .findings
        .iter()
        .any(|finding| finding.code == termiflow::FindingCode::BranchSpacingImbalance));
}

#[test]
fn dense_branch_crowding_is_flagged_by_visual_audit() {
    let mut config = termiflow::Config::default();
    config.composite_style = CompositeStyle::from_base(BaseStyle::Ascii);

    let outcome = termiflow::render_canvas_with_feedback(&dense_branch_graph(), &config)
        .unwrap_or_else(|err| panic!("failed to render dense branch graph: {}", err));
    let summary = outcome.critic_report.audit_summary();

    assert_eq!(summary.verdict, AuditVerdict::NeedsReview);
    assert!(outcome
        .critic_report
        .findings
        .iter()
        .any(|finding| finding.code == termiflow::FindingCode::BranchCrowding));
}
