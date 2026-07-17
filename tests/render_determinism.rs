use std::fs;

use termiflow::{render, BaseStyle, RenderOptions};

fn assert_repeated_renders_match(path: &str, style: BaseStyle) {
    let input = fs::read_to_string(path).expect("read fixture");
    let expected = render(&input, RenderOptions::new().with_style(style)).expect("render fixture");

    for _ in 0..32 {
        let actual = render(&input, RenderOptions::new().with_style(style))
            .expect("render repeated fixture");
        assert_eq!(
            actual, expected,
            "render output changed between identical runs for {path} ({style:?})"
        );
    }
}

#[test]
fn td_subgraph_layout_is_deterministic() {
    for path in [
        "tests/fixtures/inputs/collision_sibling_triple_td.md",
        "tests/fixtures/inputs/subgraph_chain_td.md",
        "tests/fixtures/inputs/collision_sibling_triple_lr.md",
    ] {
        assert_repeated_renders_match(path, BaseStyle::Ascii);
        assert_repeated_renders_match(path, BaseStyle::Unicode);
    }
}
