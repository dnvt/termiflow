use termiflow::{BaseStyle, RenderOptions};

fn cycle(direction: &str) -> String {
    format!("graph {direction}\n  A[Start] --> B[Middle]\n  B --> C[Finish]\n  C --> A\n")
}

#[test]
fn horizontal_back_edge_leaves_its_source_through_a_bottom_junction() {
    for direction in ["LR", "RL"] {
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let frame = termiflow::render_with_feedback(
                    &cycle(direction),
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("render")
                .output;
                let rows: Vec<Vec<char>> =
                    frame.lines().map(|line| line.chars().collect()).collect();
                let label_row = rows
                    .iter()
                    .position(|row| row.iter().collect::<String>().contains("Finish"))
                    .expect("Finish row");
                let text: Vec<char> = "Finish".chars().collect();
                let text_col = rows[label_row]
                    .windows(text.len())
                    .position(|window| window == text.as_slice())
                    .expect("Finish column");
                let border = &rows[label_row + 1];
                let below = &rows[label_row + 2];
                let exit_x = (text_col.saturating_sub(4)..text_col + text.len() + 4)
                    .find(|&x| matches!(below.get(x), Some('│' | '|' | ':' | '╎')))
                    .unwrap_or_else(|| panic!("back edge leaves Finish downward:\n{frame}"));
                assert!(
                    matches!(border.get(exit_x), Some('┬' | '+')),
                    "Finish's bottom border must carry a junction where the back edge \
                     departs, found {:?} ({direction} {style:?} optimized={optimized}):\n{frame}",
                    border.get(exit_x)
                );
            }
        }
    }
}
