use termiflow::{BaseStyle, RenderOptions};

fn render(input: &str, style: BaseStyle) -> String {
    termiflow::render_with_feedback(input, RenderOptions::new().with_style(style))
        .expect("render")
        .output
}

/// The index of the first row containing `text`.
fn row_of(frame: &str, text: &str) -> usize {
    frame
        .lines()
        .position(|row| row.contains(text))
        .unwrap_or_else(|| panic!("no row with {text}:\n{frame}"))
}

/// The frame's bottom wall: the first row after `member`'s that starts with
/// the frame's bottom-left corner.
fn bottom_wall_row(frame: &str, member: &str) -> usize {
    frame
        .lines()
        .enumerate()
        .skip(row_of(frame, member))
        .find(|(_, row)| row.starts_with(['┗', '+']))
        .map(|(index, _)| index)
        .unwrap_or_else(|| panic!("no bottom wall under {member}:\n{frame}"))
}

/// gen_c_054: a chain weaving out of Weave and back in (a -> b -> c -> d ->
/// e -> f, with a, c, e framed) put b and d past the frame's right wall, so
/// every line ran along or across the whole frame. b and d now sit under the
/// frame, each in its own column between the boxes it links; f, fed only from
/// inside, stays right of the frame on e's row.
#[test]
fn boxes_woven_between_frame_members_sit_under_the_frame() {
    let input = "graph LR
    subgraph S [Weave]
        a[a]
        c[c]
        e[e]
    end
    a --> b[b]
    b --> c
    c --> d[d]
    d --> e
    e --> f[f]
";
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        let frame = render(input, style);
        let wall = bottom_wall_row(&frame, "  a  ");
        let column = |name: &str| {
            frame.lines().nth(row_of(&frame, name)).and_then(|row| {
                row.chars()
                    .collect::<Vec<_>>()
                    .windows(name.chars().count())
                    .position(|window| window.iter().collect::<String>() == name)
            })
        };
        for (outside, left, right) in [("  b  ", "  a  ", "  c  "), ("  d  ", "  c  ", "  e  ")] {
            assert!(
                row_of(&frame, outside) > wall,
                "{outside:?} sits under the frame ({style:?}):\n{frame}"
            );
            assert!(
                column(left) < column(outside) && column(outside) < column(right),
                "{outside:?} stands between {left:?} and {right:?} ({style:?}):\n{frame}"
            );
        }
        assert_eq!(
            row_of(&frame, "  f  "),
            row_of(&frame, "  e  "),
            "f stays on e's row ({style:?}):\n{frame}"
        );
    }
}

/// gen_c_122: s fans out to o1, o2 and o3, which now all sit under the
/// frame; the fan's bus runs from s's row down through the bottom wall to
/// them instead of starting at o1 and leaving s's line dangling.
#[test]
fn a_fan_out_to_boxes_under_its_frame_reaches_them_through_the_wall() {
    let input = "graph LR
    subgraph S [S]
        s[s]
        r1[r1]
        r2[r2]
    end
    s --> o1[o1]
    s --> o2[o2]
    s --> o3[o3]
    o1 --> r1
    o2 --> r2
    o3 --> r1
";
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        let frame = render(input, style);
        let rows: Vec<Vec<char>> = frame.lines().map(|row| row.chars().collect()).collect();
        let (s_row, o1_row) = (row_of(&frame, "  s  "), row_of(&frame, "  o1  "));
        let text: Vec<char> = "o1".chars().collect();
        let o1_col = rows[o1_row]
            .windows(text.len())
            .position(|window| window == text.as_slice())
            .expect("o1 column");
        // The bus is the first line cell left of o1's arrow.
        let bus = (0..o1_col - 4)
            .rev()
            .find(|&x| matches!(rows[o1_row][x], '├' | '┌' | '+'))
            .unwrap_or_else(|| panic!("no bus left of o1 ({style:?}):\n{frame}"));
        assert!(
            (s_row + 1..o1_row).all(|y| matches!(rows[y][bus], '│' | '|')),
            "the bus runs from s's row down to o1 ({style:?}):\n{frame}"
        );
        assert!(
            matches!(rows[s_row][bus], '┐' | '+'),
            "s's line turns down onto the bus ({style:?}):\n{frame}"
        );
    }
}

/// gen_c_020 (RL): Split fans out to Paths 1-3, which all lead back into
/// Join; they sat above, beside and below the frame with lines across it.
/// They now sit under the frame, each between Split and Join.
#[test]
fn rl_boxes_woven_between_frame_members_sit_under_the_frame() {
    let input = "graph RL
    subgraph S [Splitter]
        Src[Split]
        Dst[Join]
    end
    Src --> P1[Path 1]
    Src --> P2[Path 2]
    Src --> P3[Path 3]
    P1 --> Dst
    P2 --> Dst
    P3 --> Dst
";
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        let frame = render(input, style);
        let wall = bottom_wall_row(&frame, "Join");
        for path in ["Path 1", "Path 2", "Path 3"] {
            assert!(
                row_of(&frame, path) > wall,
                "{path} sits under the frame ({style:?}):\n{frame}"
            );
        }
    }
}

/// gen_a_077: Outbox Relay feeds Outbox Table inside Producer Service but
/// gets no line from it, so it does not weave: it stays beside the frame
/// rather than dropping under it with its lines doubling back.
#[test]
fn a_box_only_feeding_a_frame_stays_beside_it() {
    let input = "flowchart LR
    subgraph Producer [Producer Service]
        Tx[Business Txn]
        Outbox[(Outbox Table)]
        Tx --> Outbox
    end
    Relay[Outbox Relay] --> Outbox
    Relay ==> Broker[[Message Broker]]
    subgraph Consumers
        Inv[Inventory]
        Email[Email]
    end
    Broker ==> Inv
    Broker ==> Email
    Inv --> Idem{Already processed?}
    Idem -->|yes| Skip[Skip]
    Idem -->|no| Handle[Handle Event]
";
    let frame = render(input, BaseStyle::Unicode);
    assert!(
        row_of(&frame, "Outbox Relay") < bottom_wall_row(&frame, "Business Txn"),
        "Outbox Relay stays level with the frame:\n{frame}"
    );
}
