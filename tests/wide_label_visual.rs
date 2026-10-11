use termiflow::{BaseStyle, RenderOptions};

fn render(input: &str, style: BaseStyle) -> String {
    termiflow::render_with_feedback(input, RenderOptions::new().with_style(style))
        .expect("render")
        .output
}

#[test]
fn wide_glyph_edge_labels_stay_contiguous() {
    for direction in ["TD", "BT", "LR", "RL"] {
        for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
            let input = format!("graph {direction}\n    A[Start] -->|読み取り| B[End]\n");
            let frame = render(&input, style);
            assert!(
                frame.contains("読み取り"),
                "no line or blank may show between wide glyphs ({direction} {style:?}):\n{frame}"
            );
        }
    }
}

#[test]
fn a_wide_label_over_a_fan_out_drop_keeps_the_corner() {
    let input = "graph TD\n    C{Check} -->|はい| D[Save]\n    C -->|いいえ| E[Show error]\n";
    let frame = render(input, BaseStyle::Unicode);
    assert!(
        frame.contains("はい") && frame.contains("いいえ"),
        "{frame}"
    );
    assert!(
        frame.lines().any(|line| line.trim_end().ends_with('┐')),
        "the right drop keeps its corner above the label:\n{frame}"
    );
}
