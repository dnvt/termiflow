use termiflow::{BaseStyle, RenderOptions};

#[test]
fn an_inline_horizontal_label_leaves_a_shaft_at_both_box_sides() {
    for direction in ["LR", "RL"] {
        for (style, tees) in [
            (BaseStyle::Unicode, ["├ one", "three ┤"]),
            (BaseStyle::Ascii, ["+ one", "three +"]),
        ] {
            for optimized in [false, true] {
                let input =
                    format!("graph {direction}\n    A[Source] -->|one, two, three| B[Target]\n");
                let frame = termiflow::render_with_feedback(
                    &input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("render")
                .output;
                let row = frame
                    .lines()
                    .find(|line| line.contains("one, two, three"))
                    .unwrap_or_else(|| panic!("label row is missing:\n{frame}"));
                assert!(
                    tees.iter().all(|tee| !row.contains(tee)),
                    "a box side must meet a line, not a blank \
                     ({direction} {style:?} optimized={optimized}):\n{frame}"
                );
            }
        }
    }
}
