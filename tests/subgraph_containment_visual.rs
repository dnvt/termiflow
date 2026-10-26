use termiflow::{BaseStyle, RenderOptions};

const TD_EXTERNAL_SOURCE_BESIDE_ISOLATED_MEMBER: &str = "graph TD
    GW[API Gateway]
    subgraph Services [Core Services]
        Auth[Auth Service]
        Orders[Order Service]
        Catalog[Catalog Service]
    end
    GW --> Auth
    GW --> Orders
";

#[test]
fn external_source_stays_outside_a_subgraph_whose_isolated_member_shares_its_rank() {
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let frame = termiflow::render_with_feedback(
                TD_EXTERNAL_SOURCE_BESIDE_ISOLATED_MEMBER,
                RenderOptions::new()
                    .with_style(style)
                    .with_optimize_render(optimized),
            )
            .expect("render")
            .output;
            let row_of = |needle: &str| {
                frame
                    .lines()
                    .position(|line| line.contains(needle))
                    .unwrap_or_else(|| panic!("{needle} not found:\n{frame}"))
            };
            assert!(
                row_of("API Gateway") < row_of("Core Services"),
                "API Gateway is not a member of Core Services and must sit above its frame \
                 ({style:?} optimized={optimized}):\n{frame}"
            );
        }
    }
}

const TD_OUTSIDE_NODE_BETWEEN_MEMBERS: &str = "graph TD
    subgraph S [Frame]
        a[Alpha]
        c[Gamma]
    end
    a --> b[Beta]
    b --> c
";

const TD_WIDE_OUTSIDE_NODE_BETWEEN_MEMBERS: &str = "graph TD
    subgraph S [Frame]
        a[A]
        c[C]
    end
    a --> b[A very wide outside node label here]
    b --> c
";

fn render_all(input: &str) -> Vec<(String, String)> {
    let mut frames = Vec::new();
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let frame = termiflow::render_with_feedback(
                input,
                RenderOptions::new()
                    .with_style(style)
                    .with_optimize_render(optimized),
            )
            .expect("render")
            .output;
            frames.push((format!("{style:?} optimized={optimized}"), frame));
        }
    }
    frames
}

/// The frame titled `title`: its top row and its left and right wall columns.
fn frame_walls(lines: &[Vec<char>], title: &str) -> (usize, usize, usize) {
    let title_row = lines
        .iter()
        .position(|line| line.iter().collect::<String>().contains(title))
        .expect("frame title");
    let top = title_row - 1;
    let corner = |c: &char| matches!(c, '┏' | '┓' | '+');
    let left = lines[top].iter().position(corner).expect("top-left corner");
    let right = lines[top]
        .iter()
        .rposition(corner)
        .expect("top-right corner");
    (top, left, right)
}

#[test]
fn an_outside_node_between_two_frame_members_sits_beside_the_frame() {
    for (input, label) in [
        (TD_OUTSIDE_NODE_BETWEEN_MEMBERS, "Beta"),
        (TD_WIDE_OUTSIDE_NODE_BETWEEN_MEMBERS, "A very wide"),
    ] {
        for (case, frame) in render_all(input) {
            let lines: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
            let (_, left, right) = frame_walls(&lines, "Frame");
            let (row, col) = lines
                .iter()
                .enumerate()
                .find_map(|(row, line)| {
                    let text: String = line.iter().collect();
                    text.find(label)
                        .map(|byte| (row, text[..byte].chars().count()))
                })
                .expect("outside node label");
            let side = |c: &char| matches!(c, '│' | '|');
            let box_left = lines[row][..col].iter().rposition(side).expect("box left");
            let box_right = col + lines[row][col..].iter().position(side).expect("box right");
            assert!(
                box_right < left || right < box_left,
                "{label} is not in the frame and its box must sit beside the frame's walls \
                 ({case}):\n{frame}"
            );
        }
    }
}

const TD_CYCLE_THROUGH_A_FRAME: &str = "graph TD
    L[Left] --> s1
    R[Right] --> s2
    subgraph S [Shared]
        s1[s1]
        s2[s2]
    end
    s1 --> R
    s2 --> L
";

#[test]
fn a_line_to_a_box_beside_its_frame_leaves_no_mark_on_the_wall_row() {
    for (case, frame) in render_all(TD_CYCLE_THROUGH_A_FRAME) {
        let lines: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
        let (top, left, right) = frame_walls(&lines, "Shared");
        let bottom = (top + 1..lines.len())
            .rev()
            .find(|&row| matches!(lines[row].get(left), Some('┗' | '+')))
            .expect("bottom-left corner");
        let after: String = lines[bottom].iter().skip(right + 1).collect();
        assert!(
            after.trim().is_empty(),
            "nothing crosses the bottom wall of Shared, so nothing marks the row \
             beyond it ({case}):\n{frame}"
        );
        let right_box: String = lines
            .iter()
            .find(|line| line.iter().collect::<String>().contains("Right"))
            .expect("Right")
            .iter()
            .collect();
        let col = right_box[..right_box.find("Right").unwrap()]
            .chars()
            .count();
        assert!(
            col > right,
            "Right is not in Shared and must sit beside it ({case}):\n{frame}"
        );
    }
}
