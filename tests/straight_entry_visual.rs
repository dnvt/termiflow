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

const BT_ALIGNED_TITLED_CHAIN: &str = "graph BT
    subgraph Q3 [Q3]
        n3[Test]
    end
    subgraph Q4 [Q4]
        n4[Ship]
    end
    n3 --> n4
";

const BT_TITLE_SAFE_LANE: &str = "graph BT
    subgraph G1 [Group 1]
        A[Node A]
    end
    subgraph G2 [Group 2]
        B[Node B]
    end
    A --> B
";

/// Counts the straight shaft cells behind the BT entry arrow into `label`
/// before the first non-vertical glyph.
fn bt_straight_run_into(frame: &str, label: &str) -> usize {
    let rows = grid(frame);
    let label_row = rows
        .iter()
        .position(|row| row.iter().collect::<String>().contains(label))
        .unwrap_or_else(|| panic!("label {label} not found:\n{frame}"));
    let arrow_row = label_row + 2;
    let arrow_x = rows[arrow_row]
        .iter()
        .position(|glyph| matches!(glyph, '↑' | '^'))
        .unwrap_or_else(|| panic!("entry arrow under {label}:\n{frame}"));
    rows[arrow_row + 1..]
        .iter()
        .take_while(|row| matches!(row.get(arrow_x), Some('│' | '|')))
        .count()
}

#[test]
fn bt_titled_entry_keeps_an_aligned_lane_straight_through_the_title_row() {
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let frame = render(BT_ALIGNED_TITLED_CHAIN, style, optimized);
            let run = bt_straight_run_into(&frame, "Ship");
            let rows = frame.lines().count();
            let ship_row = frame.lines().position(|l| l.contains("Ship")).unwrap();
            let test_row = frame.lines().position(|l| l.contains("Test")).unwrap();
            assert!(
                ship_row < test_row && run + 4 >= test_row - ship_row,
                "aligned Test -> Ship must stay on one lane (run {run}, rows {rows}) ({style:?} optimized={optimized}):\n{frame}"
            );
        }
    }
}

#[test]
fn bt_titled_entry_lands_on_a_title_safe_lane_inside_the_target() {
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let frame = render(BT_TITLE_SAFE_LANE, style, optimized);
            let run = bt_straight_run_into(&frame, "Node B");
            assert!(
                run >= 4,
                "the portal lane must run straight from the border into Node B (run {run}) ({style:?} optimized={optimized}):\n{frame}"
            );
        }
    }
}

/// A labelled edge keeps its centred entry: its label is centred on the
/// target, so a straight entry one column over would leave the label off the
/// shaft and read as a tee into the label (`ok┤`, ASCII `ok+`).
#[test]
fn a_labelled_entry_keeps_its_label_on_the_shaft() {
    let input = "flowchart TD
    X[Source node here] -->|ok| A
    subgraph S [Outer frame]
        A[A] --> B[Some much wider target]
    end
";
    for (style, tee) in [(BaseStyle::Unicode, "ok┤"), (BaseStyle::Ascii, "ok+")] {
        let frame = render(input, style, false);
        assert!(
            frame.contains("ok") && !frame.contains(tee),
            "the label must sit on the shaft, not beside it ({style:?}):\n{frame}"
        );
    }
}
