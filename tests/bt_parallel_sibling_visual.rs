use std::collections::BTreeSet;
use std::fs;

use termiflow::{BaseStyle, RenderOptions};

#[test]
fn direct_bt_parallel_edges_keep_three_boundary_lanes_distinct() {
    let input = fs::read_to_string("tests/fixtures/inputs/collision_parallel_edges_bt.md")
        .expect("read direct BT parallel-edge fixture");

    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let outcome = termiflow::render_with_feedback(
                &input,
                RenderOptions::new()
                    .with_style(style)
                    .with_optimize_render(optimized),
            )
            .expect("render direct BT parallel-edge fixture");

            let source_lanes: BTreeSet<_> = outcome
                .portal_trace
                .boundaries
                .iter()
                .filter(|boundary| boundary.crossing == "exit" && boundary.boundary_id == "SG1")
                .filter_map(|boundary| boundary.slot_x)
                .collect();
            let target_lanes: BTreeSet<_> = outcome
                .portal_trace
                .boundaries
                .iter()
                .filter(|boundary| boundary.crossing == "enter" && boundary.boundary_id == "SG2")
                .filter_map(|boundary| boundary.slot_x)
                .collect();

            assert_eq!(
                source_lanes.len(),
                3,
                "direct BT source boundary must expose three distinct lanes for {style:?} optimized={optimized}:\n{}",
                outcome.output
            );
            assert_eq!(
                target_lanes.len(),
                3,
                "direct BT target boundary must expose three distinct lanes for {style:?} optimized={optimized}:\n{}",
                outcome.output
            );
            assert_eq!(
                source_lanes, target_lanes,
                "direct BT source and target lanes must stay paired for {style:?} optimized={optimized}:\n{}",
                outcome.output
            );
            for boundary in
                outcome.portal_trace.boundaries.iter().filter(|boundary| {
                    boundary.boundary_id == "SG1" || boundary.boundary_id == "SG2"
                })
            {
                assert_eq!(
                    boundary.slot_x,
                    Some(boundary.desired_x),
                    "strict BT parallel scene must keep each portal on its paired node lane for {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
            }
            assert!(
                outcome.output.contains("Source") && outcome.output.contains("Target"),
                "direct BT titled siblings must remain readable for {style:?} optimized={optimized}:\n{}",
                outcome.output
            );
            let target_line = outcome
                .output
                .lines()
                .find(|line| line.contains("Target"))
                .expect("direct BT target title row");
            let target_start = target_line
                .find("Target")
                .expect("target title starts on its own row");
            let first_rail_offset = target_line[target_start + "Target".len()..]
                .chars()
                .position(|glyph| matches!(glyph, '|' | '│'))
                .expect("target title row has a first vertical rail");
            assert!(
                first_rail_offset >= 3,
                "the first BT rail must leave three quiet cells after the visible Target title for {style:?} optimized={optimized}:\n{}",
                outcome.output
            );

            match style {
                BaseStyle::Ascii => {
                    assert!(
                        !outcome.output.contains("Target |"),
                        "the first BT rail must not touch the Target title gutter for optimized={optimized}:\n{}",
                        outcome.output
                    );
                    assert!(
                        !outcome.output.lines().any(|line| {
                            line.contains("++") || line.contains("+-+")
                        }),
                        "BT parallel turns must expose a readable shaft between ASCII corners for optimized={optimized}:\n{}",
                        outcome.output
                    );
                }
                BaseStyle::Unicode => {
                    assert!(
                        !outcome.output.contains("Target │"),
                        "the first BT rail must not touch the Target title gutter for optimized={optimized}:\n{}",
                        outcome.output
                    );
                    assert!(
                        !outcome.output.lines().any(|line| {
                            line.contains("┌┘")
                                || line.contains("└┐")
                                || line.contains("┌─┘")
                                || line.contains("└─┐")
                        }),
                        "BT parallel turns must expose a readable shaft between Unicode corners for optimized={optimized}:\n{}",
                        outcome.output
                    );
                }
                _ => unreachable!("focused direct-parallel test only uses ASCII and Unicode"),
            }
        }
    }
}

#[test]
fn direct_bt_parallel_edges_show_directional_border_seams() {
    let input = fs::read_to_string("tests/fixtures/inputs/collision_parallel_edges_bt.md")
        .expect("read direct BT parallel-edge fixture");

    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let outcome = termiflow::render_with_feedback(
                &input,
                RenderOptions::new()
                    .with_style(style)
                    .with_optimize_render(optimized),
            )
            .expect("render direct BT parallel-edge fixture");
            let lines: Vec<Vec<char>> = outcome
                .output
                .lines()
                .map(|line| line.chars().collect())
                .collect();

            match style {
                BaseStyle::Ascii => {
                    let rows_with_three_portals = lines
                        .iter()
                        .filter(|line| {
                            line.iter()
                                .enumerate()
                                .filter(|(x, glyph)| {
                                    *x > 0 && *x + 1 < line.len() && **glyph == '+'
                                })
                                .count()
                                == 3
                        })
                        .count();
                    assert_eq!(
                        rows_with_three_portals, 2,
                        "direct BT ASCII rails need three seam openings on both titled boundaries for optimized={optimized}:\n{}",
                        outcome.output
                    );
                }
                BaseStyle::Unicode => {
                    let upper_seams = lines
                        .iter()
                        .flat_map(|line| line.iter())
                        .filter(|glyph| **glyph == '┯')
                        .count();
                    let lower_seams = lines
                        .iter()
                        .flat_map(|line| line.iter())
                        .filter(|glyph| **glyph == '┷')
                        .count();
                    assert_eq!(
                        upper_seams, 3,
                        "direct BT Unicode source boundary needs three directional seams for optimized={optimized}:\n{}",
                        outcome.output
                    );
                    assert_eq!(
                        lower_seams, 3,
                        "direct BT Unicode target boundary needs three directional seams for optimized={optimized}:\n{}",
                        outcome.output
                    );
                }
                _ => unreachable!("focused direct-parallel seam test uses ASCII and Unicode"),
            }
        }
    }
}
