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

/// Back edges into targets that share a row with other boxes each enter
/// through a jog of their own from a clear row below the rank, even when the
/// first rows there are taken by a merge bus or another detour, instead of
/// running along the row behind the boxes beside them.
#[test]
fn back_edges_into_a_crowded_row_each_enter_by_their_own_jog() {
    let input = "graph TD\nA --> B --> A\nC --> D --> E --> C\nF --> F\n";
    for (style, jog, behind) in [
        (BaseStyle::Unicode, "←┐", "←───│"),
        (BaseStyle::Ascii, "<+", "<---|"),
    ] {
        let frame = termiflow::render_with_feedback(input, RenderOptions::new().with_style(style))
            .expect("render")
            .output;
        let top = frame
            .lines()
            .find(|row| row.contains("  C  ") && row.contains("  A  "))
            .expect("top rank");
        assert!(
            top.matches(jog).count() >= 2 && !top.contains(behind),
            "C and A each take their own jog, nothing runs behind a box ({style:?}):\n{frame}"
        );
    }
}

/// An open back edge has no head; its line used to end flat against the
/// target's border. It now tees into the border, as its source end does.
#[test]
fn an_open_back_edge_tees_into_its_target() {
    for (direction, start_border) in [("TD", "Start  ├"), ("LR", "└────┬────┘")]
    {
        let input =
            format!("graph {direction}\n  A[Start] --> B[Middle]\n  B --> C[Finish]\n  C --- A\n");
        for optimized in [false, true] {
            let frame = termiflow::render_with_feedback(
                &input,
                RenderOptions::new()
                    .with_style(BaseStyle::Unicode)
                    .with_optimize_render(optimized),
            )
            .expect("render")
            .output;
            assert!(
                frame.lines().any(|row| row.contains(start_border)),
                "the open back edge tees into Start ({direction} optimized={optimized}):\n{frame}"
            );
        }
    }
}

/// The search that picks back edges started from the first declared node, a
/// subgraph member here, and broke the cycle at Candidate's entries: Candidate
/// then ranked last and drew at the far end of the RL flow, every edge
/// looping around. It now starts from the edges' first-named source, so the
/// cycle breaks at Iterate -> Candidate and Candidate starts the flow.
#[test]
fn a_cycle_breaks_at_the_edge_that_closes_it() {
    let input = "graph RL
    subgraph Eval [Model Evaluation]
        Bench[Benchmark Suite]
        RedTeam[Red Team]
    end
    Candidate[Candidate Model] --> Bench
    Candidate --> RedTeam
    Bench & RedTeam --> Gate{Meets bar?}
    Gate --> Iterate[Iterate on Training]
    Iterate --> Candidate
";
    let frame =
        termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Unicode))
            .expect("render")
            .output;
    let column = |label: &str| {
        frame
            .lines()
            .find_map(|row| row.find(label).map(|at| row[..at].chars().count()))
            .expect(label)
    };
    assert!(
        column("Candidate Model") > column("Meets bar?"),
        "Candidate starts the RL flow, right of the gate:\n{frame}"
    );
}

/// A back edge leaving a diamond's bottom in LR/RL started detached below
/// the flat border; it now gets the tee a rectangle source gets.
#[test]
fn a_back_edge_leaving_a_diamond_tees_its_border() {
    let input = "graph RL
    User((User)) --> Login[Login Page]
    Login --> Creds{Valid?}
    Creds -->|no| Lock{Too many?}
    Lock -->|no| Login
    Creds -->|yes| Home[Home]
";
    let frame =
        termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Unicode))
            .expect("render")
            .output;
    assert!(
        frame
            .lines()
            .any(|row| row.contains('╲') && row.contains('┬')),
        "the back edge leaves Too many? through a tee:\n{frame}"
    );
}

/// gen_d_094: two separate cycles side by side. A's back edge from C went
/// out to the right gutter past the second cycle and came back along A's
/// row straight through D. Its loop now closes in the free column between
/// the two cycles.
#[test]
fn a_td_back_edge_loop_closes_short_of_a_box_on_its_row() {
    let input = "graph TD\nA --> B --> C --> A\nD --> E --> F --> D\n";
    let frame =
        termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Unicode))
            .expect("render")
            .output;
    let row = frame
        .lines()
        .find(|line| line.contains("  A  "))
        .expect("A's row");
    let between = &row[row.find("  A  ").expect("A") + 5..row.find("  D  ").expect("D")];
    assert!(
        between.contains('┐'),
        "A's back edge runs along its row through D:\n{frame}"
    );
}

/// gen_a_222: Rework's back edge to QC dropped from the middle of Rework
/// straight through Scrap below it, so every lane was dropped and the
/// default loop crossed Scrap. The leg now drops beside Scrap.
#[test]
fn an_lr_back_edge_leg_drops_beside_a_box_below_its_end() {
    let input = "flowchart LR
    subgraph Prod [Factory Floor]
        Line1[Assembly Line 1]
        Line2[Assembly Line 2]
        QC{QC Station}
        Line1 --> QC
        Line2 --> QC
    end
    QC --> Pass[Finished Goods]
    QC --> Rework[Rework Cell]
    Rework --> QC
    QC --> Scrap[Scrap]
    Pass --> Ship[Shipping]
";
    let frame =
        termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Unicode))
            .expect("render")
            .output;
    let row = frame
        .lines()
        .find(|line| line.contains("  Scrap  "))
        .expect("Scrap's row");
    let after = &row[row.find("  Scrap  ").expect("Scrap")..];
    assert_eq!(
        after.matches('│').count(),
        2,
        "Rework's back edge does not pass beside Scrap:\n{frame}"
    );
}

/// gen_d_095: B and D share the top rank and both loop back to Hub. B's back
/// edge ran along its row to the right gutter straight through D. It now
/// leaves through B's top and runs above the rank.
#[test]
fn a_bt_back_edge_with_a_box_on_its_row_leaves_through_the_top() {
    let input = "graph BT\nH[Hub] --> A --> B --> H\nH --> C --> D --> H\n";
    let frame =
        termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Unicode))
            .expect("render")
            .output;
    let row = frame
        .lines()
        .find(|line| line.contains("  B  "))
        .expect("B's row");
    let between = &row[row.find("  B  ").expect("B") + 5..row.find("  D  ").expect("D")];
    assert!(
        !between.contains('─'),
        "B's back edge runs through D:\n{frame}"
    );
}

fn unicode(input: &str) -> String {
    termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Unicode))
        .expect("render")
        .output
}

/// gen_a_224: Extend -> Run ran down from Extend through Kill Experiment and
/// Ship Variant, stacked below it. It now leaves Extend's top, runs over the
/// boxes and comes down into Run's top.
#[test]
fn an_rl_back_edge_with_boxes_below_goes_over_the_top() {
    let frame = unicode(
        "graph RL
    Signal[Market Signal] --> Hyp[Write Hypothesis]
    Hyp --> Exp[Design A/B Test]
    Exp --> Run[Run Experiment]
    Run --> Sig{Statistically significant?}
    Sig --> Ship[Ship Variant]
    Sig --> Extend[Extend Duration]
    Extend --> Run
    Sig --> Kill[Kill Experiment]
",
    );
    let lines: Vec<&str> = frame.lines().collect();
    let row = |label: &str| {
        lines
            .iter()
            .position(|line| line.contains(label))
            .expect(label)
    };
    assert!(
        lines[row("Extend Duration") - 1].contains('┴'),
        "Extend's back edge does not leave its top:\n{frame}"
    );
    assert!(
        !lines[row("Kill Experiment") - 1].contains('┬')
            && !lines[row("Kill Experiment") - 1].contains('┴'),
        "a line still runs through Kill Experiment:\n{frame}"
    );
    assert!(
        lines[row("Run Experiment") - 2].contains('↓'),
        "the back edge does not come down into Run:\n{frame}"
    );
}

/// gen_d_004: the labelled `retry` back edge goes over the top with its
/// label on the lane, above every box.
#[test]
fn a_labelled_back_edge_over_the_top_keeps_its_label_on_the_lane() {
    let frame = unicode(
        "graph RL
A{Is the incoming payload signed by a trusted issuer} -->|yes| B[Accept and forward to processing pipeline]
A -->|no| C{Should we retry verification with the backup key}
C -->|retry| A
C -->|give up| D[Reject with a descriptive error message]
",
    );
    let lines: Vec<&str> = frame.lines().collect();
    let retry = lines
        .iter()
        .position(|line| line.contains("retry ─"))
        .expect("retry");
    assert_eq!(retry, 0, "retry is not on a lane over the boxes:\n{frame}");
}

/// gen_d_120: of three stacked back-edge pairs only the top one can go
/// over the top; the others still cross a box below and keep the default
/// route, but the top pair keeps its lane.
#[test]
fn a_back_edge_over_the_top_stays_when_others_keep_the_default() {
    let frame = unicode("graph RL\nA --> X\nX --> A\nB --> Y\nY --> B\nC --> Z\nZ --> C\n");
    let lines: Vec<&str> = frame.lines().collect();
    let x = lines
        .iter()
        .position(|line| line.contains("│  X  │") || line.contains("│  X  ├"))
        .expect("X");
    assert!(
        x >= 3 && lines[0].contains('┌') && lines[x - 1].contains('┴'),
        "X -> A does not go over the top:\n{frame}"
    );
}

/// gen_d_155: of two RL back edges between stacked boxes in nested frames,
/// D -> A has no lane that clears C below it, and every planned lane was
/// then dropped, C -> B's too. C -> B keeps its lane now: down out of C,
/// through Inner two's bottom wall, under both frames and up into B.
#[test]
fn a_back_edge_keeps_its_lane_when_another_has_none() {
    let input = "graph RL
subgraph o [Outer]
  subgraph i1 [Inner one]
    A
    B
  end
  subgraph i2 [Inner two]
    C
    D
  end
end
A --> D
B --> C
C --> B
D --> A
";
    let frame = termiflow::render_with_feedback(input, termiflow::RenderOptions::new())
        .expect("render")
        .output;
    let rows: Vec<&str> = frame.lines().collect();
    let walls = rows
        .iter()
        .position(|row| row.contains("┗━") && row.matches('┛').count() == 2)
        .unwrap_or_else(|| panic!("no inner bottom walls:\n{frame}"));
    let lane = &rows[walls + 2];
    assert!(
        lane.contains('└') && lane.contains('┘') && lane.contains('─'),
        "C -> B runs under the inner frames:\n{frame}"
    );
}
