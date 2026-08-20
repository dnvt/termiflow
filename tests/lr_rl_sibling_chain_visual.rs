use std::fs;

use termiflow::render::semantic::CellOwnerKind;
use termiflow::{BaseStyle, RenderOptions};

#[test]
fn horizontal_sibling_chain_uses_clean_portal_pierces_in_both_mirrors() {
    for fixture in [
        "tests/fixtures/inputs/collision_sibling_triple_lr.md",
        "tests/fixtures/inputs/collision_sibling_triple_rl.md",
        "tests/fixtures/inputs/subgraph_chain_lr.md",
        "tests/fixtures/inputs/subgraph_chain_rl.md",
    ] {
        let input = fs::read_to_string(fixture).expect("read horizontal sibling fixture");
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let outcome = termiflow::render_with_feedback(
                    &input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("render horizontal sibling fixture");
                let minimum_bridge_width = if style == BaseStyle::Ascii {
                    "----------------"
                } else {
                    "────────────────"
                };
                assert!(
                    outcome.output.contains(minimum_bridge_width),
                    "sibling transitions need a visibly deliberate bridge rather than a tiny border-like elbow for {fixture}, {:?}, optimized={optimized}:\n{}",
                    style,
                    outcome.output
                );
                let horizontal_portals = outcome
                    .semantic_frame
                    .cells
                    .iter()
                    .filter(|cell| {
                        cell.owner_kind == CellOwnerKind::PortalOpening
                            && matches!(cell.ch, '-' | '─')
                    })
                    .count();
                assert!(
                    horizontal_portals >= 4,
                    "expected both LR/RL sibling transitions to retain horizontal portal openings for {fixture}, {:?}, optimized={optimized}:\n{}",
                    style,
                    outcome.output
                );
            }
        }
    }
}
