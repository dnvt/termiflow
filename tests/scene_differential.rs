use std::fs;

use termiflow::{layout_and_render_with_feedback, measure, parse, Config};

fn render_fixture(path: &str) -> termiflow::RenderOutcome {
    let input = fs::read_to_string(path).expect("read scene differential fixture");
    let parsed = parse(&input, false).expect("parse scene differential fixture");
    let mut config = Config::builder().build(&parsed.config);
    config.spacing = config.spacing.for_direction(parsed.graph.direction);
    let mut graph = parsed.graph;
    measure::measure_graph(&mut graph, &config);
    let (_, outcome) = layout_and_render_with_feedback(graph, config).expect("layout fixture");
    outcome
}

#[test]
fn scene_observer_preserves_legacy_differential_surfaces() {
    for path in [
        "tests/fixtures/inputs/flow_simple_td.md",
        "tests/fixtures/inputs/cycle_simple_td.md",
        "tests/fixtures/inputs/subgraph_complex_lr.md",
        "tests/fixtures/inputs/subgraph_complex_bt.md",
    ] {
        let first = render_fixture(path);
        let second = render_fixture(path);

        assert_eq!(first.output, second.output, "output drift for {path}");
        assert_eq!(
            first.semantic_frame, second.semantic_frame,
            "semantic frame drift for {path}"
        );
        assert_eq!(
            first.display_semantic_frame, second.display_semantic_frame,
            "display frame drift for {path}"
        );
        assert_eq!(
            first.critic_report, second.critic_report,
            "critic drift for {path}"
        );
        assert_eq!(first.warnings, second.warnings, "warning drift for {path}");
        assert_eq!(
            first.repair_passes, second.repair_passes,
            "repair drift for {path}"
        );
        assert_eq!(
            first.layout_repairs_applied, second.layout_repairs_applied,
            "layout repair drift for {path}"
        );
    }
}
