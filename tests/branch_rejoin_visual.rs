use std::collections::HashSet;
use std::fs;

use termiflow::{BaseStyle, RenderOptions};

fn arrow(style: BaseStyle, direction: &str) -> char {
    match (style, direction) {
        (BaseStyle::Ascii, "td") => 'v',
        (BaseStyle::Unicode, "td") => '↓',
        (BaseStyle::Ascii, "bt") => '^',
        (BaseStyle::Unicode, "bt") => '↑',
        _ => unreachable!("focused branch/rejoin test only uses TD/BT fixtures"),
    }
}

#[test]
fn vertical_branch_rejoin_matches_human_readable_collector_contract() {
    for fixture in ["flow_branch", "junction_corners"] {
        for direction in ["td", "bt"] {
            let input =
                fs::read_to_string(format!("tests/fixtures/inputs/{fixture}_{direction}.md"))
                    .expect("read branch/rejoin fixture");

            for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
                for optimized in [false, true] {
                    let options = RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized);
                    let first = termiflow::render_with_feedback(&input, options.clone())
                        .expect("render branch/rejoin fixture");
                    let second = termiflow::render_with_feedback(&input, options)
                        .expect("repeat render branch/rejoin fixture");

                    assert_eq!(
                        first.output,
                        second.output,
                        "branch/rejoin output must be deterministic for {fixture}_{direction} {style:?} optimized={optimized}"
                    );

                    assert_eq!(
                        first.output.matches(arrow(style, direction)).count(),
                        4,
                        "branch/rejoin must expose all four directed edges for {fixture}_{direction} {style:?} optimized={optimized}:\n{}",
                        first.output
                    );
                    let arrow_owners: HashSet<String> = first
                        .semantic_frame
                        .cells
                        .iter()
                        .filter(|cell| cell.role == termiflow::render::semantic::CellRole::ArrowTip)
                        .filter_map(|cell| cell.owner_id.clone())
                        .collect();
                    assert_eq!(
                        arrow_owners.len(),
                        4,
                        "branch/rejoin must preserve one owner per arrow for {fixture}_{direction} {style:?} optimized={optimized}: {arrow_owners:?}\n{}",
                        first.output
                    );
                    assert!(
                        first.critic_report.findings.is_empty(),
                        "branch/rejoin must remain machine-clean for {fixture}_{direction} {style:?} optimized={optimized}: {:?}\n{}",
                        first.critic_report.findings,
                        first.output
                    );
                }
            }
        }
    }
}

#[test]
fn pure_vertical_convergence_keeps_two_target_arrows() {
    for direction in ["td", "bt"] {
        let input = fs::read_to_string(format!(
            "tests/fixtures/inputs/edge_converge_{direction}.md"
        ))
        .expect("read pure convergence fixture");
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let outcome = termiflow::render_with_feedback(
                    &input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("render pure convergence fixture");
                assert_eq!(
                    outcome.output.matches(arrow(style, direction)).count(),
                    2,
                    "pure vertical convergence must retain two target arrows for {direction} {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
            }
        }
    }
}
