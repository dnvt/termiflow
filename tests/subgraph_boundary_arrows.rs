//! Regression coverage for titled-subgraph boundary entries.

use std::collections::BTreeSet;
use std::fs;

use termiflow::render::semantic::{CellOwnerKind, CellRole};
use termiflow::{
    layout_and_render_with_feedback, measure, parse, render::evidence, BaseStyle, CompositeStyle,
    Config, FindingCode, DEFAULT_DISPLAY_PROFILE,
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

fn render_fixture_outcome(
    path: &str,
    style: BaseStyle,
    optimized: bool,
) -> (termiflow::Graph, termiflow::RenderOutcome) {
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
    layout_and_render_with_feedback(parsed, config).expect("render boundary-arrow fixture")
}

fn rows_with_multiple_arrowheads(frame: &str, arrow: char) -> Vec<Vec<usize>> {
    frame
        .lines()
        .map(|line| {
            line.chars()
                .enumerate()
                .filter_map(|(column, glyph)| (glyph == arrow).then_some(column))
                .collect::<Vec<_>>()
        })
        .filter(|columns| columns.len() >= 2)
        .collect()
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
fn strict_bt_sibling_portals_show_directional_border_seams() {
    for fixture in [
        "collision_sibling_triple_bt",
        "subgraph_chain_bt",
        "subgraph_multi_bt",
    ] {
        for optimized in [false, true] {
            let (_graph, frame) = render_fixture_output(fixture, BaseStyle::Unicode, optimized);
            assert!(
                frame.contains('┬') && frame.contains('┴'),
                "strict BT sibling portals need directional border seams for {fixture} optimized={optimized}:\n{frame}"
            );
        }
    }
}

#[test]
fn exact_bt_sibling_target_portals_show_directional_border_seams() {
    for optimized in [false, true] {
        let (_graph, frame) = render_fixture_output(
            "collision_sibling_subgraphs_bt",
            BaseStyle::Unicode,
            optimized,
        );
        assert!(
            frame.contains('┬') && frame.contains('┴'),
            "exact BT sibling target portals need directional border seams optimized={optimized}:\n{frame}"
        );
    }
}

#[test]
fn two_group_bt_scene_compacts_and_traces_the_cross_boundary_edge() {
    let mut failures = Vec::new();

    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let report = render_fixture("subgraph_multi_bt", style, optimized);
            if report.raw.arrowheads != 3
                || !report.raw.shaftless_arrowheads.is_empty()
                || !report.geometry.errors.is_empty()
                || !report.geometry.untraced_fallback_edges.is_empty()
                || report.display.height > 34
                || !report.critic.findings.is_empty()
            {
                failures.push(format!(
                    "{style:?} optimized={optimized}: arrows={} shaftless={:?} geometry={:?} untraced={:?} height={} critic={:?}",
                    report.raw.arrowheads,
                    report.raw.shaftless_arrowheads,
                    report.geometry.errors,
                    report.geometry.untraced_fallback_edges,
                    report.display.height,
                    report.critic.findings
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "two-group BT scene corridor regressions:\n{}",
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
                || !report.portal_trace.fallback_rejection_reasons().is_empty()
                || !report.critic.findings.is_empty()
            {
                failures.push(format!(
                    "{style:?} optimized={optimized}: arrows={} shaftless={:?} geometry={:?} untraced={:?} rejections={:?} critic={:?}",
                    report.raw.arrowheads,
                    report.raw.shaftless_arrowheads,
                    report.geometry.errors,
                    report.geometry.untraced_fallback_edges,
                    report.portal_trace.fallback_rejection_reasons(),
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
fn mixed_vertical_sibling_targets_keep_a_readable_entry_gap() {
    for fixture in [
        "collision_sibling_subgraphs_td",
        "collision_sibling_subgraphs_bt",
    ] {
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let (_graph, frame) = render_fixture_output(fixture, style, optimized);
                let arrow = match (fixture, style) {
                    ("collision_sibling_subgraphs_td", BaseStyle::Ascii) => 'v',
                    ("collision_sibling_subgraphs_td", BaseStyle::Unicode) => '↓',
                    ("collision_sibling_subgraphs_bt", BaseStyle::Ascii) => '^',
                    ("collision_sibling_subgraphs_bt", BaseStyle::Unicode) => '↑',
                    _ => unreachable!("fixture matrix is exhaustive"),
                };
                let rows = rows_with_multiple_arrowheads(&frame, arrow);
                assert_eq!(
                    rows.len(),
                    1,
                    "expected one mixed-target row with two {arrow:?} tips for {fixture} {style:?} optimized={optimized}:\n{frame}"
                );
                assert!(
                    rows[0][1].saturating_sub(rows[0][0]) >= 3,
                    "mixed sibling target tips need at least one visual spacer cell for {fixture} {style:?} optimized={optimized}: columns={:?}\n{frame}",
                    rows[0]
                );
            }
        }
    }
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
fn horizontal_mixed_target_receivers_keep_a_quiet_shaft_before_the_arrow() {
    for fixture in [
        "collision_sibling_subgraphs_lr",
        "collision_sibling_subgraphs_rl",
    ] {
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let report = render_fixture(fixture, style, optimized);
                let (_graph, output) = render_fixture_output(fixture, style, optimized);
                let forbidden = match style {
                    BaseStyle::Ascii => ["+>+", "+<+"],
                    BaseStyle::Unicode => ["┌→┤", "├←┐"],
                    _ => unreachable!("test matrix only includes ASCII and Unicode"),
                };
                assert!(
                    forbidden.iter().all(|pattern| !output.contains(pattern)),
                    "mixed horizontal receiver must keep a quiet shaft before its arrow for {fixture} {style:?} optimized={optimized}:\n{}",
                    output
                );
                assert_eq!(
                    report.raw.arrowheads, 4,
                    "quiet receiver shaft must not remove an arrow for {fixture} {style:?} optimized={optimized}:\n{}",
                    output
                );
                assert!(
                    report.geometry.untraced_fallback_edges.is_empty(),
                    "quiet receiver shaft must preserve traceability for {fixture} {style:?} optimized={optimized}: {:?}\n{}",
                    report.geometry.untraced_fallback_edges,
                    output
                );
            }
        }
    }
}

#[test]
fn direct_bt_subgraph_portals_keep_a_quiet_turn_shaft() {
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let report = render_fixture("subgraph_direct_bt", style, optimized);
            let (_graph, frame) = render_fixture_output("subgraph_direct_bt", style, optimized);
            let forbidden = match style {
                BaseStyle::Ascii => ["++", "+-+"],
                BaseStyle::Unicode => ["└┐", "┌┘"],
                _ => unreachable!("test matrix only includes ASCII and Unicode"),
            };
            assert!(
                forbidden.iter().all(|pattern| !frame.contains(pattern)),
                "direct BT portal turn must keep a quiet shaft for {style:?} optimized={optimized}:\n{frame}"
            );
            let title_row = frame
                .lines()
                .position(|line| line.contains("Group 2"))
                .expect("direct BT target title row");
            let title_clearance_row = frame
                .lines()
                .nth(title_row.saturating_sub(1))
                .expect("direct BT target title clearance row");
            assert!(
                !title_clearance_row.contains('-') && !title_clearance_row.contains('─'),
                "direct BT target title clearance row must stay free of a horizontal turn for {style:?} optimized={optimized}:\n{frame}"
            );
            if style == BaseStyle::Unicode {
                assert!(
                    !frame.contains("└─┐") && !frame.contains("┌─┘"),
                    "direct BT portal turn must not form a one-cell Unicode hook for optimized={optimized}:\n{frame}"
                );
            }
            assert_eq!(
                report.raw.arrowheads, 1,
                "direct BT portal repair must preserve the target arrow for {style:?} optimized={optimized}:\n{frame}"
            );
            assert!(
                report.raw.shaftless_arrowheads.is_empty() && report.geometry.errors.is_empty(),
                "direct BT portal repair must preserve a connected target entry for {style:?} optimized={optimized}: raw={:?} geometry={:?}\n{frame}",
                report.raw.shaftless_arrowheads,
                report.geometry.errors
            );
        }
    }
}

#[test]
fn narrow_bt_external_portals_keep_the_source_node_turn_clear() {
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let report = render_fixture("subgraph_narrow_bt", style, optimized);
            let (_graph, frame) = render_fixture_output("subgraph_narrow_bt", style, optimized);
            let forbidden = match style {
                BaseStyle::Ascii => ["++", "+-+"],
                BaseStyle::Unicode => ["└┐", "┌┘"],
                _ => unreachable!("test matrix only includes ASCII and Unicode"),
            };
            assert!(
                forbidden.iter().all(|pattern| !frame.contains(pattern)),
                "narrow BT external portal must keep a quiet source turn for {style:?} optimized={optimized}:\n{frame}"
            );
            if style == BaseStyle::Unicode {
                assert!(
                    !frame.contains("└─┐") && !frame.contains("┌─┘"),
                    "narrow BT external portal must not form a one-cell Unicode hook for optimized={optimized}:\n{frame}"
                );
            }
            assert_eq!(
                report.raw.arrowheads, 2,
                "narrow BT portal repair must preserve both arrows for {style:?} optimized={optimized}:\n{frame}"
            );
            assert!(
                report.raw.shaftless_arrowheads.is_empty() && report.geometry.errors.is_empty(),
                "narrow BT portal repair must preserve connected entries for {style:?} optimized={optimized}: raw={:?} geometry={:?}\n{frame}",
                report.raw.shaftless_arrowheads,
                report.geometry.errors
            );
        }
    }
}

#[test]
fn flat_td_external_entries_use_one_title_gutter_lane_across_matrix() {
    for fixture in ["subgraph_single_td", "subgraph_outside_td"] {
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let report = render_fixture(fixture, style, optimized);
                let (_graph, frame) = render_fixture_output(fixture, style, optimized);
                let forbidden = match style {
                    BaseStyle::Ascii => ["+-+", "+--+"],
                    BaseStyle::Unicode => ["┌─┘", "└─┐"],
                    _ => unreachable!("test matrix only includes ASCII and Unicode"),
                };
                assert!(
                    forbidden.iter().all(|pattern| !frame.contains(pattern)),
                    "flat TD external entry must not form a title-row hook for {fixture} {style:?} optimized={optimized}:\n{frame}"
                );
                assert!(
                    report.raw.shaftless_arrowheads.is_empty()
                        && report.geometry.errors.is_empty(),
                    "flat TD title-gutter lane must preserve connected arrows for {fixture} {style:?} optimized={optimized}: raw={:?} geometry={:?}\n{frame}",
                    report.raw.shaftless_arrowheads,
                    report.geometry.errors
                );
            }
        }
    }
}

#[test]
fn strict_td_terminal_entries_reserve_a_quiet_title_row_and_clean_receivers() {
    for (fixture, title, expected_arrows) in [
        ("collision_edge_along_border_td", "Target Group", 3usize),
        ("collision_edge_corner_td", "Group", 2usize),
    ] {
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let (graph, outcome) = render_fixture_outcome(fixture, style, optimized);
                let frame = &outcome.display_semantic_frame;
                let output = &outcome.output;
                let arrow = if style == BaseStyle::Ascii {
                    'v'
                } else {
                    '↓'
                };
                let title_row = output
                    .lines()
                    .position(|line| line.contains(title))
                    .expect("strict TD title row");
                let arrow_row = output
                    .lines()
                    .position(|line| {
                        line.chars().filter(|glyph| *glyph == arrow).count() == expected_arrows
                    })
                    .expect("strict TD arrow row");
                assert!(
                    arrow_row > title_row + 1,
                    "strict TD receiver scene needs a quiet row below the title for {fixture} {style:?} optimized={optimized}\n{output}"
                );
                for x in 0..frame.width {
                    let cell = frame.get(x, title_row + 1).expect("title clearance cell");
                    assert!(!matches!(
                        (cell.owner_kind, cell.role),
                        (CellOwnerKind::EdgeSegment, CellRole::Horizontal)
                            | (CellOwnerKind::Junction, CellRole::Horizontal)
                            | (CellOwnerKind::Junction, CellRole::Junction)
                    ), "route-owned horizontal/junction cell at ({x}, {}) below title for {fixture} {style:?} optimized={optimized}: {cell:?}\n{output}", title_row + 1);
                }
                let report = evidence::build(&graph, &outcome);
                assert_eq!(report.raw.arrowheads, expected_arrows);
                assert!(
                    report.raw.shaftless_arrowheads.is_empty()
                        && report.geometry.errors.is_empty()
                        && report.geometry.untraced_fallback_edges.is_empty()
                        && report.portal_trace.fallback_rejection_reasons().is_empty()
                        && report.critic.findings.is_empty(),
                    "strict TD receiver scene is not machine-clean for {fixture} {style:?} optimized={optimized}: raw={:?} geometry={:?} rejections={:?} critic={:?}\n{output}",
                    report.raw,
                    report.geometry,
                    report.portal_trace.fallback_rejection_reasons(),
                    report.critic.findings
                );
                assert!(
                    arrow_row > title_row,
                    "strict TD receiver row must be below the title for {fixture} {style:?} optimized={optimized}"
                );

                if fixture == "collision_edge_along_border_td" {
                    let portal_columns: BTreeSet<_> = frame
                        .cells
                        .iter()
                        .enumerate()
                        .filter(|(_, cell)| cell.role == CellRole::Portal)
                        .map(|(index, _)| index % frame.width)
                        .collect();
                    let portal_columns: Vec<_> = portal_columns.into_iter().collect();
                    assert_eq!(
                        portal_columns.len(),
                        expected_arrows,
                        "strict TD terminal scene should expose one opening per target for {style:?} optimized={optimized}: {output}"
                    );
                    assert!(
                        portal_columns.windows(2).all(|columns| {
                            columns[1].saturating_sub(columns[0]) >= 4
                        }),
                        "strict TD terminal scene should keep a readable gap between portal openings for {style:?} optimized={optimized}: columns={portal_columns:?}\n{output}"
                    );
                    let paired_openings = match style {
                        BaseStyle::Ascii => "|-|",
                        BaseStyle::Unicode => "│─│",
                        _ => unreachable!("strict TD test only uses ASCII and Unicode"),
                    };
                    assert!(
                        !output.lines().any(|line| line.contains(paired_openings)),
                        "strict TD terminal scene should not compose adjacent portal openings as border punctuation for {style:?} optimized={optimized}: {output}"
                    );
                }
            }
        }
    }
}

#[test]
fn nested_bt_external_entry_keeps_a_quiet_target_turn() {
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let report = render_fixture("subgraph_nested_bt", style, optimized);
            let (_graph, frame) = render_fixture_output("subgraph_nested_bt", style, optimized);
            let forbidden = match style {
                BaseStyle::Ascii => ["++", "+-+"],
                BaseStyle::Unicode => ["└┐", "┌┘"],
                _ => unreachable!("test matrix only includes ASCII and Unicode"),
            };
            let deep_title_row = frame
                .lines()
                .position(|line| line.contains("Deep"))
                .expect("nested BT direct child title row");
            let deep_clearance_row = frame
                .lines()
                .nth(deep_title_row.saturating_sub(1))
                .expect("nested BT direct child title clearance row");
            assert!(
                !deep_clearance_row.contains('-') && !deep_clearance_row.contains('─'),
                "nested BT direct child title clearance row must stay free of a horizontal turn for {style:?} optimized={optimized}:\n{frame}"
            );
            assert!(
                forbidden.iter().all(|pattern| !frame.contains(pattern)),
                "nested BT external entry must keep a quiet target turn for {style:?} optimized={optimized}:\n{frame}"
            );
            if style == BaseStyle::Unicode {
                assert!(
                    !frame.contains("└─┐") && !frame.contains("┌─┘"),
                    "nested BT external entry must not form a one-cell Unicode hook for optimized={optimized}:\n{frame}"
                );
            }
            assert_eq!(
                report.raw.arrowheads, 2,
                "nested BT quiet-turn repair must preserve both arrows for {style:?} optimized={optimized}:\n{frame}"
            );
            assert!(
                report.raw.shaftless_arrowheads.is_empty()
                    && report.geometry.errors.is_empty()
                    && report.geometry.untraced_fallback_edges.is_empty(),
                "nested BT quiet-turn repair must preserve connected, traced entries for {style:?} optimized={optimized}: raw={:?} geometry={:?} untraced={:?}\n{frame}",
                report.raw.shaftless_arrowheads,
                report.geometry.errors,
                report.geometry.untraced_fallback_edges
            );
        }
    }
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
fn exact_two_bt_siblings_replace_stale_generic_portal_slots() {
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let report = render_fixture("collision_sibling_subgraphs_bt", style, optimized);
            let target_entries = report
                .portal_trace
                .boundaries
                .iter()
                .filter(|boundary| boundary.boundary_id == "Right" && boundary.crossing == "enter")
                .collect::<Vec<_>>();
            assert_eq!(target_entries.len(), 2);
            assert!(
                target_entries
                    .iter()
                    .all(|boundary| boundary.slot_x == Some(boundary.title_safe_x)),
                "exact BT sibling scene must project the route-owned target lanes, not stale generic slots, for {style:?} optimized={optimized}: {target_entries:?}"
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
fn exact_two_bt_siblings_keep_lower_cross_branch_clear_of_internal_switchback() {
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let (_graph, frame) =
                render_fixture_output("collision_sibling_subgraphs_bt", style, optimized);
            let ambiguous_shoulder = match style {
                BaseStyle::Ascii => "+-+",
                BaseStyle::Unicode => "┬─┘",
                _ => unreachable!("regression matrix only uses ASCII and Unicode"),
            };
            assert!(
                !frame.lines().any(|row| row.contains(ambiguous_shoulder)),
                "BT sibling lower cross branch must keep a visible clearance shaft for {style:?} optimized={optimized}:\n{frame}"
            );
        }
    }
}

#[test]
fn exact_two_bt_siblings_route_clarity_is_clean_across_matrix() {
    let input_path = "tests/fixtures/inputs/collision_sibling_subgraphs_bt.md";
    let input = fs::read_to_string(input_path).expect("read exact BT mixed sibling fixture");

    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let mut parsed = parse(&input, false)
                .expect("parse exact BT mixed sibling fixture")
                .graph;
            let mut config = Config {
                composite_style: CompositeStyle::from_base(style),
                optimize_render: optimized,
                ..Default::default()
            };
            config.spacing = config.spacing.for_direction(parsed.direction);
            let policy = termiflow::effective_render_policy(
                &config,
                parsed.direction,
                DEFAULT_DISPLAY_PROFILE.name,
                "Fixed",
                false,
                false,
            );
            measure::measure_graph(&mut parsed, &config);
            let (_graph, outcome) =
                layout_and_render_with_feedback(parsed, config).expect("render exact BT scene");
            let report = termiflow::analyze_route_clarity_for_audit(
                input.as_bytes(),
                outcome.output.as_bytes(),
                &policy,
                optimized,
            )
            .expect("analyze exact BT scene route clarity");
            assert!(
                !report["findings"].as_array().is_some_and(|findings| {
                    findings.iter().any(|finding| {
                        finding["code"] == "bt_title_boundary_hook_requires_human_review"
                    })
                }),
                "exact BT mixed sibling scene must not retain title-boundary-hook findings for {style:?} optimized={optimized}: {report}"
            );
        }
    }
}

#[test]
fn exact_two_bt_siblings_wide_labels_keep_lower_lane_clear_of_source_walls() {
    let input_path = "tests/fixtures/inputs/collision_sibling_subgraphs_bt_wide_labels.md";
    let input = fs::read_to_string(input_path).expect("read wide-label BT sibling fixture");

    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let mut parsed = parse(&input, false)
                .expect("parse wide-label BT sibling fixture")
                .graph;
            let mut config = Config {
                composite_style: CompositeStyle::from_base(style),
                optimize_render: optimized,
                ..Default::default()
            };
            config.spacing = config.spacing.for_direction(parsed.direction);
            measure::measure_graph(&mut parsed, &config);
            let (graph, outcome) = layout_and_render_with_feedback(parsed, config)
                .expect("render wide-label BT sibling fixture");
            let report = evidence::build(&graph, &outcome);

            assert_eq!(report.raw.arrowheads, 4);
            assert!(report.raw.shaftless_arrowheads.is_empty());
            assert!(report.geometry.errors.is_empty());
            assert!(report.geometry.untraced_fallback_edges.is_empty());
            assert!(report.critic.findings.is_empty(),
                "wide-label BT sibling scene must be critic-clean for {style:?} optimized={optimized}: {:?}\n{}",
                report.critic.findings, outcome.output);

            let source = graph
                .subgraphs
                .iter()
                .find(|subgraph| subgraph.id == "Left")
                .expect("wide-label source subgraph");
            let source_left = source.bounds.x;
            let source_right = source
                .bounds
                .x
                .saturating_add(source.bounds.width.saturating_sub(1));
            let lower_entry = outcome
                .portal_trace
                .target_entry_coordinates("C")
                .into_iter()
                .find(|(edge_id, _, _)| edge_id == "edge:2:A->C")
                .expect("wide-label lower cross entry");
            assert!(lower_entry.1.abs_diff(source_left) >= 4);
            assert!(lower_entry.1.abs_diff(source_right) >= 4);

            if style == BaseStyle::Ascii {
                assert!(
                    !outcome.output.contains("++"),
                    "wide-label ASCII sibling scene must keep a gap around the source wall\n{}",
                    outcome.output
                );
            } else {
                assert!(!outcome.output.contains("┼┘"));
            }
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
                }) && !frame.contains(if style == BaseStyle::Ascii {
                    "++"
                } else {
                    "└┐"
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
fn stacked_td_sibling_corridors_trace_every_cross_boundary_edge() {
    let input = fs::read_to_string("tests/fixtures/inputs/collision_sibling_triple_td.md")
        .expect("read stacked TD sibling fixture");

    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let mut parsed = parse(&input, false)
                .expect("parse stacked TD sibling fixture")
                .graph;
            let mut config = Config {
                composite_style: CompositeStyle::from_base(style),
                optimize_render: optimized,
                ..Default::default()
            };
            config.spacing = config.spacing.for_direction(parsed.direction);
            measure::measure_graph(&mut parsed, &config);
            let (graph, outcome) = layout_and_render_with_feedback(parsed, config)
                .expect("render stacked TD sibling fixture");
            let report = evidence::build(&graph, &outcome);

            assert_eq!(
                report.geometry.traced_edges, 5,
                "all TD sibling edges must be traced for {style:?} optimized={optimized}: {:?}",
                report.geometry
            );
            assert!(
                report.geometry.untraced_fallback_edges.is_empty(),
                "TD sibling corridors must not fall back to generic border elbows for {style:?} optimized={optimized}: {:?}",
                report.geometry.untraced_fallback_edges
            );
            assert!(
                report.geometry.errors.is_empty(),
                "TD sibling corridor trace must be error-free for {style:?} optimized={optimized}: {:?}",
                report.geometry.errors
            );
            assert!(
                !outcome.output.lines().any(|line| {
                    line.contains(if style == BaseStyle::Ascii {
                        "+-+"
                    } else {
                        "└─┐"
                    }) || line.contains(if style == BaseStyle::Ascii {
                        "+-+"
                    } else {
                        "┌─┘"
                    })
                }),
                "TD sibling corridor must not emit a degenerate one-cell hook for {style:?} optimized={optimized}:\n{}",
                outcome.output
            );
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
fn bt_parallel_unicode_portals_show_directional_border_seams() {
    for optimized in [false, true] {
        let (_graph, frame) =
            render_fixture_output("subgraph_parallel_bt", BaseStyle::Unicode, optimized);
        assert!(
            frame.contains('┬') && frame.contains('┴'),
            "BT parallel portal crossings should use explicit Unicode border seams for optimized={optimized}:\n{frame}"
        );
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

#[test]
fn strict_bt_sibling_chain_compacts_only_excess_inter_group_corridor() {
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let (graph, outcome) =
                render_fixture_outcome("collision_sibling_triple_bt", style, optimized);
            let report = evidence::build(&graph, &outcome);
            let mut subgraphs = report.geometry_trace.subgraphs.clone();
            subgraphs.sort_by_key(|subgraph| subgraph.bounds.y);

            assert_eq!(subgraphs.len(), 3);
            for pair in subgraphs.windows(2) {
                let upper = &pair[0].bounds;
                let lower = &pair[1].bounds;
                assert_eq!(
                    lower.y.saturating_sub(upper.y + upper.height),
                    3,
                    "strict BT sibling chain should retain exactly one route corridor row for {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
            }
            assert!(
                outcome.output.contains("Group 1")
                    && outcome.output.contains("Group 2")
                    && outcome.output.contains("Group 3")
            );
            assert!(report.geometry.errors.is_empty());
            assert!(report.geometry.untraced_fallback_edges.is_empty());
        }
    }
}

#[test]
fn collision_sibling_triple_route_ownership_oracle_covers_matrix() {
    let mut failures = Vec::new();

    for direction in ["bt", "lr", "rl", "td"] {
        let input_path = format!("tests/fixtures/inputs/collision_sibling_triple_{direction}.md");
        let input = fs::read_to_string(&input_path).expect("read triple sibling fixture");

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let mut parsed = parse(&input, false)
                    .expect("parse triple sibling fixture")
                    .graph;
                let mut config = Config {
                    composite_style: CompositeStyle::from_base(style),
                    optimize_render: optimized,
                    ..Default::default()
                };
                config.spacing = config.spacing.for_direction(parsed.direction);
                measure::measure_graph(&mut parsed, &config);
                let (graph, outcome) = layout_and_render_with_feedback(parsed, config)
                    .expect("render triple sibling fixture");
                let report = evidence::build(&graph, &outcome);

                let mut crossings = BTreeSet::new();
                for edge in &graph.edges {
                    let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
                    if exits.is_empty() && enters.is_empty() {
                        continue;
                    }
                    if exits.len() != 1 || enters.len() != 1 {
                        failures.push(format!(
                            "{direction} {style:?} optimized={optimized}: edge {} has boundary crossings exits={exits:?} enters={enters:?}",
                            format!("{}->{}", edge.from, edge.to),
                        ));
                        continue;
                    }
                    crossings.insert((
                        edge.from.clone(),
                        edge.to.clone(),
                        exits[0].to_owned(),
                        enters[0].to_owned(),
                    ));
                }

                let expected_crossings = BTreeSet::from([
                    (
                        "A2".to_owned(),
                        "B".to_owned(),
                        "G1".to_owned(),
                        "G2".to_owned(),
                    ),
                    (
                        "B2".to_owned(),
                        "C".to_owned(),
                        "G2".to_owned(),
                        "G3".to_owned(),
                    ),
                ]);
                if crossings != expected_crossings {
                    failures.push(format!(
                        "{direction} {style:?} optimized={optimized}: boundary ownership changed: {crossings:?}"
                    ));
                }

                if report.geometry.traced_edges != 5
                    || report.geometry.errors.len() != 0
                    || !report.geometry.untraced_fallback_edges.is_empty()
                    || report.raw.arrowheads != 5
                    || !report.raw.shaftless_arrowheads.is_empty()
                    || !report.critic.findings.is_empty()
                {
                    failures.push(format!(
                        "{direction} {style:?} optimized={optimized}: geometry={:?} raw={:?} critic={:?}",
                        report.geometry, report.raw, report.critic.findings
                    ));
                }

                for (from, to, exit, enter) in expected_crossings {
                    let Some(edge) = report
                        .geometry_trace
                        .edges
                        .iter()
                        .find(|edge| edge.from == from && edge.to == to)
                    else {
                        failures.push(format!(
                            "{direction} {style:?} optimized={optimized}: missing geometry trace for {from}->{to}"
                        ));
                        continue;
                    };
                    if edge.exits != [exit.clone()] || edge.enters != [enter.clone()] {
                        failures.push(format!(
                            "{direction} {style:?} optimized={optimized}: geometry ownership for {}->{} is exits={:?} enters={:?}",
                            from, to, edge.exits, edge.enters
                        ));
                    }
                }

                if direction == "bt" {
                    let mut portal_claims = report
                        .portal_trace
                        .boundaries
                        .iter()
                        .filter(|boundary| {
                            (boundary.boundary_id == "G1" && boundary.crossing == "exit")
                                || (boundary.boundary_id == "G2"
                                    && (boundary.crossing == "exit"
                                        || boundary.crossing == "enter"))
                                || (boundary.boundary_id == "G3" && boundary.crossing == "enter")
                        })
                        .collect::<Vec<_>>();
                    portal_claims.sort_by(|left, right| {
                        (
                            left.edge_id.as_str(),
                            left.boundary_id.as_str(),
                            left.crossing.as_str(),
                        )
                            .cmp(&(
                                right.edge_id.as_str(),
                                right.boundary_id.as_str(),
                                right.crossing.as_str(),
                            ))
                    });
                    if portal_claims.len() != 4
                        || portal_claims.iter().any(|claim| claim.slot_x.is_none())
                    {
                        failures.push(format!(
                            "{direction} {style:?} optimized={optimized}: portal claims are not fully owned: {portal_claims:?}"
                        ));
                    }

                    for claim in portal_claims {
                        let Some(slot_x) = claim.slot_x else {
                            continue;
                        };
                        let Some(boundary) = graph.get_subgraph(&claim.boundary_id) else {
                            failures.push(format!(
                                "{direction} {style:?} optimized={optimized}: missing subgraph {}",
                                claim.boundary_id
                            ));
                            continue;
                        };
                        let y = if claim.crossing == "exit" {
                            boundary.bounds.y
                        } else {
                            boundary
                                .bounds
                                .y
                                .saturating_add(boundary.bounds.height.saturating_sub(1))
                        };
                        let owned = report.portal_trace.cells.iter().find(|cell| {
                            cell.boundary_id == claim.boundary_id
                                && cell.side == claim.side
                                && cell.x == slot_x
                                && cell.y == y
                        });
                        if owned.is_none_or(|cell| cell.owner_kind != "PortalOpening") {
                            failures.push(format!(
                                "{direction} {style:?} optimized={optimized}: portal slot ({slot_x},{y}) for {} is not a PortalOpening: {owned:?}",
                                claim.edge_id
                            ));
                        }
                    }
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "triple sibling route-ownership oracle failures:\n{}",
        failures.join("\n")
    );
}
