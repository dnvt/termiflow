use termiflow::{BaseStyle, RenderOptions};

#[test]
fn a_word_gap_in_a_vertical_edge_label_stays_blank() {
    for direction in ["TD", "BT"] {
        for (style, line) in [(BaseStyle::Unicode, '│'), (BaseStyle::Ascii, '|')] {
            for optimized in [false, true] {
                let input = format!("graph {direction}\n    A[Source] -->|ab cd| B[Target]\n");
                let frame = termiflow::render_with_feedback(
                    &input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("render")
                .output;
                assert!(
                    frame.contains("ab cd") && !frame.contains(&format!("ab{line}cd")),
                    "the edge line must not show through the label's word gap \
                     ({direction} {style:?} optimized={optimized}):\n{frame}"
                );
            }
        }
    }
}
