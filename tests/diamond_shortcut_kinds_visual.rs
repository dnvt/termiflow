use termiflow::{BaseStyle, RenderOptions};

fn render(input: &str, style: BaseStyle, optimized: bool) -> String {
    termiflow::render_with_feedback(
        input,
        RenderOptions::new()
            .with_style(style)
            .with_optimize_render(optimized),
    )
    .expect("render")
    .output
}

/// Count the arrowheads that enter `label`'s box from above (TD).
fn arrows_into(frame: &str, label: &str) -> usize {
    let rows: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
    let label_row = rows
        .iter()
        .position(|row| row.iter().collect::<String>().contains(label))
        .unwrap_or_else(|| panic!("{label} not found:\n{frame}"));
    rows[label_row - 2]
        .iter()
        .filter(|glyph| matches!(glyph, '↓' | 'v'))
        .count()
}

#[test]
fn shortcut_keeps_its_own_route_when_the_first_leg_is_not_a_plain_arrow() {
    for first_leg in ["---", "-.->", "==>", "--o"] {
        let input = format!("graph TD\nA[One] {first_leg} B[Two]\nA --> R[Reg]\nB --> R\n");
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let frame = render(&input, style, optimized);
                assert_eq!(
                    arrows_into(&frame, "Reg"),
                    2,
                    "One -> Reg must keep its own route beside Two ({first_leg} {style:?} \
                     optimized={optimized}):\n{frame}"
                );
                if first_leg == "---" {
                    assert_eq!(
                        arrows_into(&frame, "Two"),
                        0,
                        "an open link draws no arrowhead ({style:?} optimized={optimized}):\n{frame}"
                    );
                }
            }
        }
    }
}
