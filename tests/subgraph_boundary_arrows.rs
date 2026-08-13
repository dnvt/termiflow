//! Regression coverage for titled-subgraph boundary entries.

use std::collections::BTreeSet;
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
    "subgraph_parallel_td",
    "subgraph_shapes_bt",
    "subgraph_shapes_lr",
    "subgraph_shapes_rl",
    "subgraph_shapes_td",
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

fn render_fixture_output(
    path: &str,
    style: BaseStyle,
    optimized: bool,
) -> (termiflow::Graph, String) {
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
    (graph, outcome.output)
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

#[test]
fn exact_two_bt_siblings_keep_each_target_entry_distinct() {
    let mut failures = Vec::new();

    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let report = render_fixture("collision_sibling_subgraphs_bt", style, optimized);
            if report.raw.arrowheads != 4
                || !report.raw.shaftless_arrowheads.is_empty()
                || !report.geometry.errors.is_empty()
                || !report.geometry.untraced_fallback_edges.is_empty()
                || !report.critic.findings.is_empty()
            {
                failures.push(format!(
                    "{style:?} optimized={optimized}: arrows={} shaftless={:?} geometry={:?} untraced={:?} critic={:?}",
                    report.raw.arrowheads,
                    report.raw.shaftless_arrowheads,
                    report.geometry.errors,
                    report.geometry.untraced_fallback_edges,
                    report.critic.findings
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "two-sibling BT target-entry regressions:\n{}",
        failures.join("\n")
    );
}

#[test]
fn exact_td_mixed_target_keeps_both_entries_visible_across_matrix() {
    let mut failures = Vec::new();

    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let report = render_fixture("collision_sibling_subgraphs_td", style, optimized);
            if report.raw.arrowheads != 4
                || !report.raw.shaftless_arrowheads.is_empty()
                || !report.geometry.errors.is_empty()
                || !report.geometry.untraced_fallback_edges.is_empty()
                || !report.critic.findings.is_empty()
            {
                failures.push(format!(
                    "{style:?} optimized={optimized}: arrows={} shaftless={:?} geometry={:?} untraced={:?} critic={:?}",
                    report.raw.arrowheads,
                    report.raw.shaftless_arrowheads,
                    report.geometry.errors,
                    report.geometry.untraced_fallback_edges,
                    report.critic.findings
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "TD mixed sibling/internal target-entry regressions:\n{}",
        failures.join("\n")
    );
}

#[test]
fn exact_horizontal_mixed_target_keeps_both_entries_visible_across_matrix() {
    let mut failures = Vec::new();

    for fixture in [
        "collision_sibling_subgraphs_lr",
        "collision_sibling_subgraphs_rl",
    ] {
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let report = render_fixture(fixture, style, optimized);
                if report.raw.arrowheads != 4
                    || !report.raw.shaftless_arrowheads.is_empty()
                    || !report.geometry.errors.is_empty()
                    || !report.geometry.untraced_fallback_edges.is_empty()
                    || !report.critic.findings.is_empty()
                {
                    failures.push(format!(
                        "{fixture} {style:?} optimized={optimized}: arrows={} shaftless={:?} geometry={:?} untraced={:?} critic={:?}",
                        report.raw.arrowheads,
                        report.raw.shaftless_arrowheads,
                        report.geometry.errors,
                        report.geometry.untraced_fallback_edges,
                        report.critic.findings
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "horizontal mixed sibling/internal target-entry regressions:\n{}",
        failures.join("\n")
    );
}

#[test]
fn complex_external_fan_in_keeps_distinct_response_entries_across_matrix() {
    let mut failures = Vec::new();
    let expected_edges = BTreeSet::from([
        "edge:4:D1->Response".to_owned(),
        "edge:5:D2->Response".to_owned(),
    ]);

    for fixture in [
        "subgraph_complex_lr",
        "subgraph_complex_rl",
        "subgraph_complex_td",
        "subgraph_complex_bt",
    ] {
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let report = render_fixture(fixture, style, optimized);
                let entries = report.portal_trace.target_entry_coordinates("Response");
                let coordinates = entries
                    .iter()
                    .map(|(_, x, y)| (*x, *y))
                    .collect::<BTreeSet<_>>();
                let edge_ids = entries
                    .iter()
                    .map(|(edge_id, _, _)| edge_id.clone())
                    .collect::<BTreeSet<_>>();
                if report.raw.arrowheads != 6
                    || !report.raw.shaftless_arrowheads.is_empty()
                    || entries.len() != 2
                    || coordinates.len() != 2
                    || edge_ids != expected_edges
                    || !report.portal_trace.fallback_rejection_reasons().is_empty()
                    || !report.geometry.errors.is_empty()
                    || !report.critic.findings.is_empty()
                {
                    failures.push(format!(
                        "{fixture} {style:?} optimized={optimized}: arrows={} shaftless={:?} entries={entries:?} geometry={:?} rejections={:?} critic={:?}",
                        report.raw.arrowheads,
                        report.raw.shaftless_arrowheads,
                        report.geometry.errors,
                        report.portal_trace.fallback_rejection_reasons(),
                        report.critic.findings
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "complex external fan-in target-entry regressions:\n{}",
        failures.join("\n")
    );
}

#[test]
fn mixed_sibling_target_remains_outside_external_fan_in_contract() {
    for fixture in [
        "collision_sibling_subgraphs_lr",
        "collision_sibling_subgraphs_rl",
        "collision_sibling_subgraphs_td",
        "collision_sibling_subgraphs_bt",
    ] {
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let report = render_fixture(fixture, style, optimized);
                assert!(
                    report
                        .portal_trace
                        .target_entry_coordinates("D")
                        .is_empty(),
                    "mixed internal/external D target must not be claimed by the external sibling scene for {fixture} {style:?} optimized={optimized}"
                );
            }
        }
    }
}

#[test]
fn exact_two_bt_siblings_keep_cross_boundary_lanes_unique_per_edge() {
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let input_path = "tests/fixtures/inputs/collision_sibling_subgraphs_bt.md";
            let input = fs::read_to_string(input_path).expect("read collision fixture");
            let mut parsed = parse(&input, false).expect("parse collision fixture").graph;
            let mut config = Config {
                composite_style: CompositeStyle::from_base(style),
                optimize_render: optimized,
                ..Default::default()
            };
            config.spacing = config.spacing.for_direction(parsed.direction);
            measure::measure_graph(&mut parsed, &config);
            let (graph, outcome) =
                layout_and_render_with_feedback(parsed, config).expect("render collision fixture");
            let report = evidence::build(&graph, &outcome);

            let mut lanes = Vec::new();
            for (index, edge) in graph.edges.iter().enumerate().filter(|(_, edge)| {
                let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
                exits.contains(&"Left") && enters.contains(&"Right")
            }) {
                let edge_id = format!("edge:{index}:{}->{}", edge.from, edge.to);
                let source_lane = report
                    .portal_trace
                    .boundaries
                    .iter()
                    .find(|boundary| {
                        boundary.edge_id == edge_id
                            && boundary.boundary_id == "Left"
                            && boundary.crossing == "exit"
                    })
                    .map(|boundary| boundary.title_safe_x)
                    .unwrap_or_else(|| panic!("missing source lane for {edge_id}"));
                let target_lane = report
                    .portal_trace
                    .boundaries
                    .iter()
                    .find(|boundary| {
                        boundary.edge_id == edge_id
                            && boundary.boundary_id == "Right"
                            && boundary.crossing == "enter"
                    })
                    .map(|boundary| boundary.title_safe_x)
                    .unwrap_or_else(|| panic!("missing target lane for {edge_id}"));
                lanes.push((edge_id, source_lane, target_lane));
            }

            assert_eq!(lanes.len(), 2, "expected both sibling crossing lanes");
            assert_ne!(
                lanes[0].1, lanes[1].1,
                "distinct edges must not share a source-boundary lane: {lanes:?}"
            );
            assert_ne!(
                lanes[0].2, lanes[1].2,
                "distinct edges must not share a target-boundary lane: {lanes:?}"
            );
        }
    }
}

#[test]
fn exact_two_bt_siblings_leave_a_quiet_row_before_each_target_title() {
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let (_graph, frame) =
                render_fixture_output("collision_sibling_subgraphs_bt", style, optimized);
            let title_row = frame
                .lines()
                .position(|line| line.contains("Right Group"))
                .expect("BT target title row");
            let clearance_row = title_row.saturating_sub(1);
            let row = frame
                .lines()
                .nth(clearance_row)
                .expect("BT title clearance row");
            assert!(
                !row.contains('-') && !row.contains('─'),
                "BT target title clearance row must not contain a horizontal route turn for {style:?} optimized={optimized}:\n{frame}"
            );
        }
    }
}

#[test]
fn direct_td_terminal_entries_use_target_center_portals_without_hooks() {
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let (_graph, frame) =
                render_fixture_output("collision_edge_corner_td", style, optimized);
            assert!(
                !frame.contains(if style == BaseStyle::Ascii {
                    "+-+"
                } else {
                    "└─┐"
                }) && !frame.contains(if style == BaseStyle::Ascii {
                    "+-+"
                } else {
                    "┌─┘"
                }),
                "expected no one-cell TD portal hooks for {style:?} optimized={optimized}\n{frame}"
            );
            for label in ["Source", "External", "Group", "Target", "Other"] {
                assert!(
                    frame.contains(label),
                    "expected {label:?} to remain visible for {style:?} optimized={optimized}\n{frame}"
                );
            }
        }
    }
}

#[test]
fn bt_parallel_scene_keeps_both_internal_target_entries_visible() {
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let report = render_fixture("subgraph_parallel_bt", style, optimized);
            assert_eq!(
                report.raw.arrowheads, 6,
                "BT parallel scene should expose one arrowhead per edge for {style:?} optimized={optimized}"
            );
            assert!(
                report.raw.shaftless_arrowheads.is_empty(),
                "BT parallel scene should not create shaftless target entries for {style:?} optimized={optimized}: {:?}",
                report.raw.shaftless_arrowheads
            );
            assert!(
                report.geometry.errors.is_empty()
                    && report.geometry.untraced_fallback_edges.is_empty(),
                "BT parallel scene route contract should remain traced for {style:?} optimized={optimized}: geometry={:?} untraced={:?}",
                report.geometry.errors,
                report.geometry.untraced_fallback_edges
            );
            assert!(
                report.critic.findings.is_empty(),
                "BT parallel target-entry route should be critic-clean for {style:?} optimized={optimized}: {:?}",
                report.critic.findings
            );
        }
    }
}

#[test]
fn td_parallel_portal_seams_are_not_critic_findings() {
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let report = render_fixture("subgraph_parallel_td", style, optimized);
            assert!(
                report.critic.findings.is_empty(),
                "topology-owned TD portal seams should not be reported as defects for {style:?} optimized={optimized}: {:?}",
                report.critic.findings
            );
        }
    }
}
