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

/// gen_a_245: Client feeds Backend Pool inside Load Balancing and was left
/// across the frame's left wall. It now stands beside the frame, its line
/// entering through the wall.
#[test]
fn a_box_feeding_a_frame_stands_clear_of_its_wall() {
    let input = "flowchart TB
    subgraph LB [Load Balancing]
        HC[Health Checks]
        Pool[Backend Pool]
        HC --> Pool
    end
    subgraph Backends
        B1[backend-1]
        B2[backend-2]
        B3[backend-3]
    end
    Pool --> B1
    Pool --> B2
    Pool --> B3
    B3 --x Drain[Draining]
    Client([Client]) --> Pool
";
    assert_frames_intact(&render(input));
}

/// gen_a_195: Lease Expiry feeds Dynamic DB Creds inside HashiCorp Vault and
/// was left across the frame's right wall.
#[test]
fn a_box_feeding_a_frame_from_the_right_stands_clear_of_its_wall() {
    let input = "flowchart TB
    subgraph Vault [HashiCorp Vault]
        AuthM[Kubernetes Auth]
        Policy[Policy Check]
        Dyn[Dynamic DB Creds]
        AuthM --> Policy --> Dyn
    end
    Pod[App Pod] --> AuthM
    Dyn --> Pod
    Dyn --> DB[(Postgres)]
    Lease[Lease Expiry] -.-> Dyn
";
    assert_frames_intact(&render(input));
}

/// gen_c_118: a1 -> b2 runs from one frame into the next, on the next layer,
/// and "cross1" was drawn over both walls: the gap between the frames had no
/// room for it. The gap now holds the label.
#[test]
fn an_lr_label_between_frames_sits_in_their_gap() {
    let input = "graph LR
    subgraph A [A]
        a1[a1]
        a2[a2]
    end
    subgraph B [B]
        b1[b1]
        b2[b2]
    end
    a1 -->|cross1| b2
    a2 -->|cross2| b1
";
    let frame = render(input);
    assert_frames_intact(&frame);
    for label in ["cross1", "cross2"] {
        assert!(frame.contains(label), "{label} is missing:\n{frame}");
    }
}

/// gen_d_032: long labels on edges between two narrow frames were drawn
/// across the frames' walls. B -> C's now sits in the gap between the
/// frames, and A -> D's beside its detour outside them.
#[test]
fn long_labels_between_narrow_frames_keep_off_their_walls() {
    let frame = render(
        "graph TD
subgraph left [Left cluster]
  A --> B
end
subgraph right [Right cluster]
  C --> D
end
B -->|cross cluster replication stream with long caption| C
A -->|secondary cross cluster heartbeat with long caption| D
",
    );
    assert_frames_intact(&frame);
    let lines: Vec<&str> = frame.lines().collect();
    let title = lines
        .iter()
        .position(|line| line.contains("Right cluster"))
        .expect("Right cluster");
    let wall: String = lines[title - 1].chars().take_while(|&c| c != '┓').collect();
    assert!(
        wall.contains('│'),
        "B -> C no longer crosses into Right cluster:\n{frame}"
    );
    let beside = frame
        .lines()
        .find(|line| line.contains("secondary"))
        .expect("A -> D label");
    assert!(
        beside.contains("│ secondary"),
        "A -> D's label is not beside its lane:\n{frame}"
    );
}

/// gen_d_051: a frame title in wide (CJK) characters was written one cell
/// per character, so its row came out shorter on screen than the frame and
/// its right wall stood out of line. Every row of a frame now has the same
/// display width.
#[test]
fn a_wide_character_title_keeps_its_frame_square() {
    let frame = render(
        "graph BT
subgraph g1 [処理グループ]
  A[受信] --> B[解析]
end
subgraph g2 [保存グループ]
  C[書き込み] --> D[確認]
end
B --> C
",
    );
    let lines: Vec<&str> = frame.lines().collect();
    for title in ["処理グループ", "保存グループ"] {
        let row = lines
            .iter()
            .position(|line| line.contains(title))
            .expect(title);
        let wall = |line: &str| {
            let end = line.rfind('┃').expect("right wall") + '┃'.len_utf8();
            termiflow::display_width(&line[..end])
        };
        assert_eq!(
            wall(lines[row]),
            wall(lines[row - 1]),
            "{title}'s row is out of line with its frame:\n{frame}"
        );
    }
}

/// gen_c_094 and gen_c_126: an LR frame on the next layer was moved toward
/// its feeder before the frame on the layer before grew around its nested
/// frame, and the two overlapped, one drawn over the other's wall. They now
/// stand apart.
#[test]
fn frames_on_adjacent_layers_do_not_overlap() {
    let one_way = "graph LR
    subgraph L1 [Top]
        subgraph L2a [Branch A]
            subgraph L3 [Leaf]
                x[x]
            end
        end
        subgraph L2b [Branch B]
            y[y]
        end
    end
    x --> y
    y --> z[Out]
";
    let both_ways = "graph LR
    subgraph R [Root]
        subgraph A [A]
            subgraph Aa [Aa]
                p[p]
            end
        end
        subgraph B [B]
            subgraph Bb [Bb]
                q[q]
            end
        end
    end
    p --> q
    q --> p
";
    for input in [one_way, both_ways] {
        let frame = render(input);
        assert_frames_intact(&frame);
        assert!(!frame.contains("┃┃"), "two walls touch:\n{frame}");
    }
}
