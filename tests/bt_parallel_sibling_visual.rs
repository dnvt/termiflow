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
            assert!(
                outcome.output.contains("Source") && outcome.output.contains("Target"),
                "direct BT titled siblings must remain readable for {style:?} optimized={optimized}:\n{}",
                outcome.output
            );

            match style {
                BaseStyle::Ascii => assert!(
                    !outcome.output.contains("Target |"),
                    "the first BT rail must not touch the Target title gutter for optimized={optimized}:\n{}",
                    outcome.output
                ),
                BaseStyle::Unicode => assert!(
                    !outcome.output.contains("Target │"),
                    "the first BT rail must not touch the Target title gutter for optimized={optimized}:\n{}",
                    outcome.output
                ),
                _ => unreachable!("focused direct-parallel test only uses ASCII and Unicode"),
            }
        }
    }
}
