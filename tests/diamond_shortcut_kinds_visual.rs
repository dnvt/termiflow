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

const TD_DECISION_BESIDE_A_FRAME: &str = "graph TD
    subgraph R [Team]
        F[Forensics]
        L[Legal]
    end
    F --> D{Decide?}
    D -->|yes| L
    D -->|no| X[Done]
    L --> X
";

#[test]
fn a_split_beside_a_frame_leaves_its_source_on_a_stem() {
    for input in [
        TD_DECISION_BESIDE_A_FRAME.to_string(),
        TD_DECISION_BESIDE_A_FRAME.replace("D{Decide?}", "D[Decide?]"),
    ] {
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let frame = render(&input, style, optimized);
                let rows: Vec<Vec<char>> =
                    frame.lines().map(|line| line.chars().collect()).collect();
                let (label_row, col) = rows
                    .iter()
                    .enumerate()
                    .find_map(|(row, line)| {
                        let text: String = line.iter().collect();
                        text.find("Decide?")
                            .map(|byte| (row, text[..byte].chars().count()))
                    })
                    .expect("decision");
                let side = |c: &char| matches!(c, '<' | '>' | '│' | '|');
                let left = rows[label_row][..col].iter().rposition(side).expect("left");
                let right = col + rows[label_row][col..].iter().position(side).expect("right");
                // The row under the source's bottom edge.
                let under = &rows[label_row + 2];
                assert!(
                    (left..=right).any(|x| matches!(under.get(x), Some('│' | '|'))),
                    "the split under Decide? must start on a stem, not turn right under \
                     the box ({style:?} optimized={optimized}):\n{frame}"
                );
            }
        }
    }
}

/// gen_c_105: o3 sits right above frame B and turns into it.
const TD_SOURCE_RIGHT_ABOVE_A_FRAME: &str = "graph TD
    subgraph A [A]
        a1[a1]
        a2[a2]
    end
    subgraph B [B]
        b1[b1]
        b2[b2]
    end
    a1 --> o1[o1] --> b1
    b1 --> o2[o2] --> a2
    a2 --> o3[o3] --> b2
";

#[test]
fn a_source_right_above_a_frame_keeps_its_turn_off_the_title_row() {
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let frame = render(TD_SOURCE_RIGHT_ABOVE_A_FRAME, style, optimized);
            let title = frame
                .lines()
                .find(|line| {
                    let text = line.trim_start();
                    (text.starts_with('┃') || text.starts_with('|'))
                        && text[text.chars().next().unwrap().len_utf8()..].starts_with("  B  ")
                })
                .unwrap_or_else(|| panic!("title row of B:\n{frame}"));
            assert!(
                !title
                    .chars()
                    .any(|c| matches!(c, '├' | '┤' | '┬' | '┴' | '┼' | '+')),
                "no line turns on B's title row ({style:?} optimized={optimized}):\n{frame}"
            );
        }
    }
}
