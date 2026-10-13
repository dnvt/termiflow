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

/// TD back edges whose loops overlap take separate gutter columns, so each
/// one's run from its source ends in a corner instead of joining another
/// loop's column and merging with it.
#[test]
fn overlapping_td_back_edges_keep_separate_gutter_columns() {
    let input = "graph TD\nA --> B --> C --> D --> E --> F\nF --> A\nE --> B\nD --> C\nF --> C\n";
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        let frame = termiflow::render_with_feedback(input, RenderOptions::new().with_style(style))
            .expect("render")
            .output;
        let rows: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
        for source in ["D", "E"] {
            let label = format!("  {source}  ");
            let y = rows
                .iter()
                .position(|row| row.iter().collect::<String>().contains(&label))
                .expect("source row");
            // Follow the run from the source's right side to where it turns.
            let text: String = rows[y].iter().collect();
            let label_col = text[..text.find(&label).expect("label")].chars().count();
            let start = label_col + label.chars().count() + 2;
            let end = (start..rows[y].len())
                .find(|&x| !matches!(rows[y][x], '─' | '-'))
                .expect("run end");
            let below = rows.get(y + 1).and_then(|row| row.get(end)).copied();
            assert!(
                end > start && matches!(rows[y][end], '┘' | '+'),
                "{source}'s back edge must run out and turn up in the gutter ({style:?}):\n{frame}"
            );
            assert!(
                matches!(below, None | Some(' ')),
                "{source}'s back edge joins another loop's gutter column ({style:?}):\n{frame}"
            );
        }
    }
}

/// A TD back edge never detours when its jog would run down through a box on
/// a later rank: that box would read as the edge's source.
#[test]
fn td_back_edge_does_not_detour_through_a_box_below() {
    let input = "graph TD\n  In --> Dec\n  Dec --> Ok\n  Dec --> Bad\n  Bad --> In\n  Bad --> N1[Next node with a long name]\n";
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        let frame = termiflow::render_with_feedback(input, RenderOptions::new().with_style(style))
            .expect("render")
            .output;
        let lines: Vec<&str> = frame.lines().collect();
        let label_row = lines
            .iter()
            .position(|line| line.contains("Next node"))
            .expect("N1 row");
        let bottom = lines[label_row + 1].trim();
        let inner: String = bottom
            .chars()
            .skip(1)
            .take(bottom.chars().count() - 2)
            .collect();
        assert!(
            !inner.contains(['┬', '┴', '┼', '+']),
            "no edge may leave N1's bottom border ({style:?}):\n{frame}"
        );
    }
}

/// A labelled TD back edge that detours below a sibling keeps its label on
/// its detour run.
#[test]
fn a_detoured_td_back_edge_keeps_its_label_on_its_run() {
    let input =
        "graph TD\n  In --> Dec\n  Dec --> Ok\n  Dec --> Bad\n  Bad -->|retry label 1234| In\n";
    let frame = termiflow::render_with_feedback(input, RenderOptions::new())
        .expect("render")
        .output;
    let label_row = frame
        .lines()
        .find(|line| line.contains("retry label 1234"))
        .expect("label row");
    assert!(
        label_row.contains("└─ retry label 1234 ─┘"),
        "the label sits on the detour run:\n{frame}"
    );
}

/// A back edge ends with its own marker: a circle or cross end draws that
/// marker at the target instead of a plain arrowhead, and a bidirectional
/// back edge also points back into its source.
#[test]
fn back_edges_draw_their_end_markers() {
    let render = |direction: &str, edge: &str, style: BaseStyle| {
        let input = format!("graph {direction}\nA-->B-->C\n{edge}\n");
        termiflow::render_with_feedback(&input, RenderOptions::new().with_style(style))
            .expect("render")
            .output
    };
    for (style, wall, circle, cross, left, up) in [
        (BaseStyle::Unicode, '│', '○', '✕', '←', '↑'),
        (BaseStyle::Ascii, '|', 'o', 'x', '<', '^'),
    ] {
        // TD: the back edge enters A's right side on its label row.
        for (edge, marker) in [("C --o A", circle), ("C --x A", cross)] {
            let frame = render("TD", edge, style);
            let row = frame
                .lines()
                .find(|line| line.contains("  A  "))
                .expect("A row");
            assert!(
                row.contains(&format!("{wall}{marker}")),
                "{edge} ends with its marker at A ({style:?}):\n{frame}"
            );
        }
        let frame = render("TD", "C <--> A", style);
        for node in ["  A  ", "  C  "] {
            let row = frame.lines().find(|line| line.contains(node)).expect("row");
            assert!(
                row.contains(left),
                "a bidirectional back edge points into both ends ({style:?}):\n{frame}"
            );
        }
        // LR: the back edge leaves and enters through the row below the boxes.
        for (edge, marker) in [("C --o A", circle), ("C --x A", cross)] {
            let frame = render("LR", edge, style);
            let below = frame.lines().nth(3).expect("row below the boxes");
            assert!(
                below.trim_start().starts_with(marker),
                "{edge} ends with its marker below A ({style:?}):\n{frame}"
            );
        }
        let frame = render("LR", "C <--> A", style);
        let below = frame.lines().nth(3).expect("row below the boxes");
        assert_eq!(
            below.matches(up).count(),
            2,
            "a bidirectional back edge points into both ends ({style:?}):\n{frame}"
        );
    }
}

/// Back edges that share a node's side cell keep the plain arrow there and
/// draw no source head: one edge's circle, cross or head on a shared cell
/// would label every edge through it.
#[test]
fn back_edges_on_a_shared_cell_keep_a_plain_arrow() {
    let input =
        "graph TD\nA --> B --> C --> D --> E\nE -.-> A\nE ==> B\nD --o A\nC --x A\nE <--> C\n";
    for (style, arrow, markers) in [
        (BaseStyle::Unicode, '←', ['○', '✕']),
        (BaseStyle::Ascii, '<', ['o', 'x']),
    ] {
        let frame = termiflow::render_with_feedback(input, RenderOptions::new().with_style(style))
            .expect("render")
            .output;
        let row = |label: &str| {
            frame
                .lines()
                .find(|line| line.contains(label))
                .expect("node row")
                .to_owned()
        };
        let a = row("  A  ");
        assert!(
            a.contains(arrow) && !a.contains(markers),
            "A's shared entry keeps a plain arrow ({style:?}):\n{frame}"
        );
        assert!(
            !row("  E  ").contains(arrow),
            "E's shared exit draws no source head ({style:?}):\n{frame}"
        );
    }
}

/// A TD self-loop on the bottom rank is drawn: the canvas keeps the row its
/// loop closes on. A self-loop whose node's side cell another back edge
/// leaves from is not forced in, so that exit keeps a junction.
#[test]
fn bottom_rank_td_self_loops_are_drawn_unless_their_cell_is_shared() {
    let render = |input: &str| {
        termiflow::render_with_feedback(input, RenderOptions::new())
            .expect("render")
            .output
    };
    for input in [
        "graph TD\nA --> B\nB --> B\n",
        "graph TD\nA[Loop] -->|forever| A\n",
    ] {
        let frame = render(input);
        let node_row = frame
            .lines()
            .position(|line| line.contains("  B  ") || line.contains("  Loop  "))
            .expect("node row");
        assert!(
            frame
                .lines()
                .nth(node_row)
                .is_some_and(|row| row.contains('←'))
                && frame
                    .lines()
                    .skip(node_row + 2)
                    .any(|row| row.contains('┘')),
            "the bottom-rank self-loop is drawn and closes below its box:\n{frame}"
        );
    }
    // B both loops on itself and leaves for A: the exit keeps a junction of
    // its own, on the side or, when it has one, on the bottom border.
    let frame = render("graph TD\nA --> B\nA --> A\nB --> B\nB --> A\n");
    let lines: Vec<&str> = frame.lines().collect();
    let b_row = lines
        .iter()
        .position(|line| line.contains("  B  "))
        .expect("B row");
    assert!(
        lines[b_row].contains("│  B  ├") || lines[b_row + 1].contains('┬'),
        "B's back edge to A keeps an exit junction of its own:\n{frame}"
    );
}

/// A TD back edge whose run from the gutter into its target would cross a
/// sibling box on the target's row enters along a row below the target's
/// rank instead, with a jog up into the target's side.
#[test]
fn td_back_edge_enters_below_a_sibling_on_its_target_row() {
    let input = "graph TD\n  T[Top] --> L[Left]\n  T --> R[Right]\n  L --> S[Source]\n  S --> L\n";
    for (style, entry, through) in [
        (BaseStyle::Unicode, "←┐", "─│  Right"),
        (BaseStyle::Ascii, "<+", "-|  Right"),
    ] {
        let frame = termiflow::render_with_feedback(input, RenderOptions::new().with_style(style))
            .expect("render")
            .output;
        let row = frame
            .lines()
            .find(|line| line.contains("Left"))
            .expect("target row");
        assert!(
            row.contains(entry) && !row.contains(through),
            "the back edge must enter Left without running through Right ({style:?}):\n{frame}"
        );
    }
}

/// A TD back edge whose source also takes a back edge in (here its own
/// self-loop) leaves through the source's bottom border, so its exit and
/// the arrow in do not share the source's one side cell: the exit keeps a
/// junction of its own and the self-loop is drawn.
#[test]
fn a_td_back_edge_leaves_through_the_bottom_when_its_side_cell_takes_one_in() {
    let input = "graph TD\nA --> B\nA --> A\nB --> B\nB --> A\n";
    for (style, exit) in [(BaseStyle::Unicode, '┬'), (BaseStyle::Ascii, '+')] {
        let frame = termiflow::render_with_feedback(input, RenderOptions::new().with_style(style))
            .expect("render")
            .output;
        let lines: Vec<&str> = frame.lines().collect();
        let b_row = lines
            .iter()
            .position(|line| line.contains("  B  "))
            .expect("B row");
        let border: String = lines[b_row + 1].trim().chars().skip(1).collect();
        let border = border.trim_end_matches(['┘', '+']);
        assert!(
            border.contains(exit),
            "B's back edge leaves through its bottom border ({style:?}):\n{frame}"
        );
        assert!(
            lines
                .iter()
                .skip(b_row + 2)
                .any(|line| line.contains(['┘', '+'])),
            "B's self-loop is drawn below it ({style:?}):\n{frame}"
        );
    }
}
