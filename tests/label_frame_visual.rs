use termiflow::{BaseStyle, RenderOptions};

fn render(input: &str) -> String {
    termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Unicode))
        .expect("render")
        .output
}

/// Every frame (drawn heavy) keeps its border: no text on its top or bottom
/// row, and each wall cell is the wall or a line crossing it, never text or
/// a gap.
fn assert_frames_intact(frame: &str) {
    let grid: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
    for (y, row) in grid.iter().enumerate() {
        for x in (0..row.len()).filter(|&x| row[x] == '┏') {
            let right = (x + 1..row.len())
                .find(|&r| row[r] == '┓')
                .expect("top-right corner");
            let bottom = (y + 1..grid.len())
                .find(|&b| grid[b].get(x) == Some(&'┗'))
                .expect("bottom-left corner");
            for border in [y, bottom] {
                assert!(
                    !grid[border][x..=right].iter().any(|c| c.is_alphanumeric()),
                    "text on a frame border, row {border}:\n{frame}"
                );
            }
            for (wall_y, wall_row) in grid.iter().enumerate().take(bottom).skip(y + 1) {
                for wall_x in [x, right] {
                    let c = wall_row.get(wall_x).copied().unwrap_or(' ');
                    assert!(
                        c != ' ' && !c.is_alphanumeric(),
                        "frame wall broken at row {wall_y}, column {wall_x}:\n{frame}"
                    );
                }
            }
        }
    }
}

/// gen_c_031: "result" was drawn on System's top border and "enqueue" over
/// Queue's left wall. Both now sit beside their lines inside the frames.
#[test]
fn bt_labels_keep_off_frame_borders() {
    let input = "graph BT
    Req[Request] -->|enqueue| Job
    subgraph Sys [System]
        subgraph Q [Queue]
            Job[Job]
        end
    end
    Job -->|result| Resp[Response]
    Job -- retry --> Req
";
    let frame = render(input);
    assert_frames_intact(&frame);
    for label in ["enqueue", "result"] {
        assert!(frame.contains(label), "{label} is missing:\n{frame}");
    }
}

/// gen_a_165: data-hot-1's "ILM rollover" crossed Data Nodes' left wall,
/// which split it. It now sits on the other side of its line, inside.
#[test]
fn a_td_fan_in_label_keeps_off_a_frame_wall() {
    let input = "flowchart TB
    subgraph Cluster [Elasticsearch Cluster]
        subgraph Masters [Master Nodes]
            M1[master-1]
            M2[master-2]
            M3[master-3]
        end
        subgraph DataNodes [Data Nodes]
            D1[data-hot-1]
            D2[data-hot-2]
            D3[data-warm-1]
        end
    end
    Client[Ingest Client] --> D1
    Client --> D2
    D1 -.->|ILM rollover| D3
    D2 -.->|ILM rollover| D3
    M1 --- M2 --- M3
";
    let frame = render(input);
    assert_eq!(
        frame.matches("ILM rollover").count(),
        2,
        "a label is split by a frame wall:\n{frame}"
    );
}
