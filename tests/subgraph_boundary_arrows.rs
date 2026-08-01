//! Regression coverage for titled-subgraph boundary entries.

use std::fs;

use termiflow::{
    layout_and_render_with_feedback, measure, parse, render::evidence, BaseStyle, CompositeStyle,
    Config, FindingCode,
};

const FIXTURES: &[&str] = &[
    "subgraph_outside_bt",
    "subgraph_outside_lr",
    "subgraph_outside_rl",
    "subgraph_outside_td",
    "subgraph_single_bt",
    "subgraph_single_lr",
    "subgraph_single_rl",
    "subgraph_single_td",
];

fn render_fixture(path: &str, style: BaseStyle, optimized: bool) -> evidence::RenderEvidence {
    let input_path = format!("tests/fixtures/inputs/{path}.md");
    let input = fs::read_to_string(&input_path).expect("read boundary-arrow fixture");
    let mut parsed = parse(&input, false)
        .expect("parse boundary-arrow fixture")
        .graph;
    let mut config = Config {
        composite_style: CompositeStyle::from_base(style),
        optimize_render: optimized,
        ..Default::default()
    };
    config.spacing = config.spacing.for_direction(parsed.direction);
    measure::measure_graph(&mut parsed, &config);
    let (graph, outcome) =
        layout_and_render_with_feedback(parsed, config).expect("render boundary-arrow fixture");
    evidence::build(&graph, &outcome)
}

#[test]
fn titled_subgraph_boundary_entries_have_connected_arrows_across_homologs() {
    let mut failures = Vec::new();

    for fixture in FIXTURES {
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let report = render_fixture(fixture, style, optimized);
                let critic_codes: Vec<_> = report
                    .critic
                    .findings
                    .iter()
                    .map(|finding| finding.code)
                    .collect();
                if !report.raw.shaftless_arrowheads.is_empty()
                    || critic_codes.contains(&FindingCode::ArrowWithoutVisibleShaft)
                {
                    failures.push(format!(
                        "{fixture} {style:?} optimized={optimized}: raw={:?} critic={:?}",
                        report.raw.shaftless_arrowheads, critic_codes
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "titled subgraph boundary-arrow regressions:\n{}",
        failures.join("\n")
    );
}
