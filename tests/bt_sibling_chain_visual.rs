use std::{collections::BTreeSet, fs};

use termiflow::{BaseStyle, RenderOptions};

#[test]
fn strict_bt_sibling_chain_allocates_distinct_target_portal_lanes() {
    let input = fs::read_to_string("tests/fixtures/inputs/subgraph_chain_bt.md")
        .expect("read strict BT sibling-chain fixture");

    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let outcome = termiflow::render_with_feedback(
                &input,
                RenderOptions::new()
                    .with_style(style)
                    .with_optimize_render(optimized),
            )
            .expect("render strict BT sibling-chain fixture");

            let target_entries: Vec<_> = outcome
                .portal_trace
                .boundaries
                .iter()
                .filter(|boundary| {
                    boundary.crossing == "enter"
                        && matches!(boundary.boundary_id.as_str(), "SG2" | "SG3")
                })
                .collect();
            assert_eq!(target_entries.len(), 2);
            let target_lanes: Vec<_> = target_entries
                .iter()
                .map(|boundary| {
                    boundary.slot_x.unwrap_or_else(|| {
                        panic!(
                            "strict BT sibling target entry is missing a reserved portal lane for {} on {style:?} optimized={optimized}:\n{}",
                            boundary.edge_id, outcome.output
                        )
                    })
                })
                .collect();
            assert_ne!(
                target_lanes[0],
                target_lanes[1],
                "strict BT sibling transitions must not share one target portal lane for {style:?} optimized={optimized}:\n{}",
                outcome.output
            );
            let source_exits: Vec<_> = outcome
                .portal_trace
                .boundaries
                .iter()
                .filter(|boundary| {
                    boundary.crossing == "exit"
                        && matches!(boundary.boundary_id.as_str(), "SG1" | "SG2")
                })
                .collect();
            assert_eq!(source_exits.len(), 2);
            let source_lanes: Vec<_> = source_exits
                .iter()
                .map(|boundary| {
                    boundary.slot_x.unwrap_or_else(|| {
                        panic!(
                            "strict BT sibling source exit is missing a reserved portal lane for {} on {style:?} optimized={optimized}:\n{}",
                            boundary.edge_id, outcome.output
                        )
                    })
                })
                .collect();
            assert_ne!(
                source_lanes[0],
                source_lanes[1],
                "strict BT sibling transitions must not share one source portal lane for {style:?} optimized={optimized}:\n{}",
                outcome.output
            );
            for edge_id in outcome
                .portal_trace
                .boundaries
                .iter()
                .map(|boundary| boundary.edge_id.clone())
                .collect::<BTreeSet<_>>()
            {
                let source_lane = outcome
                    .portal_trace
                    .boundaries
                    .iter()
                    .find(|boundary| boundary.edge_id == edge_id && boundary.crossing == "exit")
                    .and_then(|boundary| boundary.slot_x)
                    .expect("strict BT transition source lane");
                let target_lane = outcome
                    .portal_trace
                    .boundaries
                    .iter()
                    .find(|boundary| boundary.edge_id == edge_id && boundary.crossing == "enter")
                    .and_then(|boundary| boundary.slot_x)
                    .expect("strict BT transition target lane");
                assert_ne!(
                    source_lane, target_lane,
                    "strict BT sibling transition should expose a visible corridor turn for {edge_id} on {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
            }
            assert!(
                outcome.output.contains("Input Stage")
                    && outcome.output.contains("Transform Stage")
                    && outcome.output.contains("Output Stage"),
                "all titled sibling stages must remain readable for {style:?} optimized={optimized}:\n{}",
                outcome.output
            );
            match style {
                BaseStyle::Ascii => assert!(
                    !outcome.output.contains("+-+"),
                    "BT sibling lanes must leave a visible horizontal run between ASCII corners for optimized={optimized}:\n{}",
                    outcome.output
                ),
                BaseStyle::Unicode => assert!(
                    !outcome.output.contains("┌─┘") && !outcome.output.contains("└─┐"),
                    "BT sibling lanes must leave a visible horizontal run between Unicode corners for optimized={optimized}:\n{}",
                    outcome.output
                ),
                _ => unreachable!("focused sibling-chain test only uses ASCII and Unicode"),
            }
        }
    }
}

#[test]
fn strict_bt_sibling_chain_separates_middle_boundary_roles() {
    for fixture in ["subgraph_chain_bt", "collision_sibling_triple_bt"] {
        let input = fs::read_to_string(format!("tests/fixtures/inputs/{fixture}.md"))
            .expect("read strict BT sibling-chain fixture");

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let outcome = termiflow::render_with_feedback(
                    &input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("render strict BT sibling-chain fixture");

                let mut lanes_by_boundary =
                    std::collections::BTreeMap::<String, std::collections::BTreeSet<usize>>::new();
                for boundary in &outcome.portal_trace.boundaries {
                    if boundary.crossing == "exit" || boundary.crossing == "enter" {
                        if let Some(lane) = boundary.slot_x {
                            lanes_by_boundary
                                .entry(boundary.boundary_id.clone())
                                .or_default()
                                .insert(lane);
                        }
                    }
                }

                let middle_id = if fixture == "subgraph_chain_bt" {
                    "SG2"
                } else {
                    "G2"
                };
                let middle = lanes_by_boundary.get(middle_id).unwrap_or_else(|| {
                    panic!(
                        "strict BT chain has no middle {middle_id} boundary roles for {fixture} {style:?} optimized={optimized}:\n{}",
                        outcome.output
                    )
                });
                assert_eq!(
                    middle.len(),
                    2,
                    "strict BT chain should expose exactly two middle boundary roles for {fixture} {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
            }
        }
    }
}
