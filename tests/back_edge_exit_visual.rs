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

fn unicode_frames(input: &str) -> Vec<String> {
    [false, true]
        .into_iter()
        .map(|optimized| {
            termiflow::render_with_feedback(
                input,
                RenderOptions::new()
                    .with_style(BaseStyle::Unicode)
                    .with_optimize_render(optimized),
            )
            .expect("render")
            .output
        })
        .collect()
}

#[test]
fn adjacent_horizontal_back_edges_keep_separate_gutter_loops() {
    for direction in ["LR", "RL"] {
        let input = format!("graph {direction}\nA --> B --> C --> D\nB --> A\nC --> B\nD --> C\n");
        for frame in unicode_frames(&input) {
            assert!(
                !frame.contains('┴') && !frame.contains('┼'),
                "each back edge needs its own loop; a junction in the gutter merges \
                 them into a false fan-out ({direction}):\n{frame}"
            );
        }
    }
}

#[test]
fn horizontal_back_edge_gutter_does_not_run_along_a_subgraph_border() {
    let input = "graph LR
    In[Input] --> P1
    subgraph Pipe [Pipe]
        P1[Parse] --> P2[Check]
    end
    P2 --> Out[Output] --> Log[(Log)]
    Out --> P1
";
    for frame in unicode_frames(input) {
        assert!(
            !frame.contains('┴'),
            "the Output -> Parse back edge must not merge into the Pipe border:\n{frame}"
        );
    }
}

/// A TD back edge whose source has a sibling box on its right does not run
/// along the source's row through that box to the gutter: it jogs below
/// the rank and runs to the gutter there.
#[test]
fn td_back_edge_detours_below_a_sibling_on_its_row() {
    let input = "graph TD\n  In --> Dec\n  Dec --> Ok\n  Dec --> Bad\n  Bad --> In\n";
    for (style, exit) in [(BaseStyle::Unicode, "├─┐"), (BaseStyle::Ascii, "+-+")] {
        let frame = termiflow::render_with_feedback(input, RenderOptions::new().with_style(style))
            .expect("render")
            .output;
        let row = frame
            .lines()
            .find(|line| line.contains("Bad") && line.contains("Ok"))
            .expect("rank row");
        assert!(
            row.contains(exit) && !row.contains("Ok  │─") && !row.contains("Ok  |-"),
            "back edge from Bad runs through Ok ({style:?}):\n{frame}"
        );
    }
}
