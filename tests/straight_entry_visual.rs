use termiflow::{BaseStyle, RenderOptions};

const TD_SUBGRAPH_EXIT: &str = "graph TD
    A[Top] --> S1
    subgraph G [Loop]
        S1[Step 1] --> S2[Step 2]
    end
    S2 --> B[Below]
    B --> C[Lower]
    C --> S1
";

const BT_SUBGRAPH_ENTRY: &str = "graph BT
    A[Top] --> S1
    subgraph G [Loop]
        S1[Step 1] --> S2[Step 2]
    end
    S2 --> B[Below]
    B --> C[Lower]
    C --> S1
";

fn grid(frame: &str) -> Vec<Vec<char>> {
    frame.lines().map(|line| line.chars().collect()).collect()
}

/// Returns the shaft glyph directly behind the single arrowhead that enters
/// `label`'s box from the flow side (above for TD, below for BT).
fn shaft_behind_entry_arrow(frame: &str, label: &str, bottom_to_top: bool) -> char {
    let rows = grid(frame);
    let label_row = rows
        .iter()
        .position(|row| row.iter().collect::<String>().contains(label))
        .unwrap_or_else(|| panic!("label {label} not found:\n{frame}"));
    let needle: Vec<char> = label.chars().collect();
    let text_col = rows[label_row]
        .windows(needle.len())
        .position(|window| window == needle.as_slice())
        .expect("label column");
    let (arrow_row, shaft_row) = if bottom_to_top {
        (label_row + 2, label_row + 3)
    } else {
        (label_row - 2, label_row - 3)
    };
    let box_left = (0..text_col)
        .rev()
        .find(|&x| matches!(rows[label_row].get(x), Some('│' | '|')))
        .expect("box left wall");
    let box_right = (text_col..rows[label_row].len())
        .find(|&x| matches!(rows[label_row].get(x), Some('│' | '|')))
        .expect("box right wall");
    let arrows: Vec<usize> = (box_left + 1..box_right)
        .filter(|&x| matches!(rows[arrow_row].get(x), Some('↓' | '↑' | 'v' | '^')))
        .collect();
    assert_eq!(arrows.len(), 1, "one entry arrow into {label}:\n{frame}");
    rows[shaft_row][arrows[0]]
}

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

#[test]
fn td_subgraph_exit_keeps_a_straight_shaft_into_a_target_that_spans_the_lane() {
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let frame = render(TD_SUBGRAPH_EXIT, style, optimized);
            let shaft = shaft_behind_entry_arrow(&frame, "Below", false);
            assert!(
                matches!(shaft, '│' | '|'),
                "Below must be entered by a straight shaft, found {shaft:?} ({style:?} optimized={optimized}):\n{frame}"
            );
        }
    }
}

#[test]
fn bt_subgraph_entry_keeps_a_straight_shaft_into_a_target_that_spans_the_lane() {
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let frame = render(BT_SUBGRAPH_ENTRY, style, optimized);
            let shaft = shaft_behind_entry_arrow(&frame, "Step 1", true);
            assert!(
                matches!(shaft, '│' | '|'),
                "Step 1 must be entered by a straight shaft, found {shaft:?} ({style:?} optimized={optimized}):\n{frame}"
            );
        }
    }
}
