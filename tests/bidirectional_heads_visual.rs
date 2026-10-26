use termiflow::{BaseStyle, RenderOptions};

/// The arrowhead that points back into `One` for each flow direction.
fn source_head(direction: &str, style: BaseStyle) -> char {
    match (direction, style) {
        ("TD", BaseStyle::Unicode) => '↑',
        ("TD", _) => '^',
        ("BT", BaseStyle::Unicode) => '↓',
        ("BT", _) => 'v',
        ("LR", BaseStyle::Unicode) => '←',
        ("LR", _) => '<',
        ("RL", BaseStyle::Unicode) => '→',
        _ => '>',
    }
}

#[test]
fn bidirectional_edge_into_a_fan_in_keeps_its_source_head() {
    for direction in ["TD", "BT", "LR", "RL"] {
        let input = format!("graph {direction}\nA[One] <--> B[Two]\nC[Three] --> B\n");
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let frame = termiflow::render_with_feedback(
                    &input,
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
                    .position(|row| row.iter().collect::<String>().contains("One"))
                    .expect("One");
                let head = source_head(direction, style);
                // The head sits right outside One's box on the side the edge leaves.
                let near = rows
                    .iter()
                    .enumerate()
                    .filter(|(y, _)| y.abs_diff(label_row) <= 2)
                    .any(|(_, row)| row.contains(&head));
                assert!(
                    near,
                    "One <--> Two needs a head pointing into One ({direction} {style:?} \
                     optimized={optimized}):\n{frame}"
                );
            }
        }
    }
}
