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

/// Two nested subgraphs needing equal-size shifts used to be picked by
/// `HashMap` order, so identical renders of this RL chain differed between
/// runs (exploration corpus gen_c_064: up to 10 distinct frames).
#[test]
fn equal_nested_shift_ties_are_deterministic() {
    let input = "graph RL\n    subgraph P1 [West]\n        subgraph A [A]\n            a[a]\n        end\n        subgraph B [B]\n            b[b]\n        end\n    end\n    subgraph P2 [East]\n        subgraph C [C]\n            c[c]\n        end\n        subgraph D [D]\n            d[d]\n        end\n    end\n    a --> b --> c --> d\n";
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        let expected =
            render(input, RenderOptions::new().with_style(style)).expect("render tie fixture");
        for _ in 0..32 {
            let actual =
                render(input, RenderOptions::new().with_style(style)).expect("render tie fixture");
            assert_eq!(
                actual, expected,
                "tie-shift render changed between runs ({style:?})"
            );
        }
    }
}
