use termiflow::{BaseStyle, RenderOptions};

/// A node pushed right to clear a frame's right wall takes the flow after it
/// along, instead of landing on (or beside) its own successor.
#[test]
fn a_node_clearing_a_frame_wall_keeps_the_gap_to_its_successor() {
    let input = "graph LR\nA --> B\nsubgraph g [Group with one member]\n  C\nend\nD --> E\n";
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        for optimized in [false, true] {
            let frame = termiflow::render_with_feedback(
                input,
                RenderOptions::new()
                    .with_style(style)
                    .with_optimize_render(optimized),
            )
            .expect("render")
            .output;
            let row = frame
                .lines()
                .find(|line| line.contains("  D  "))
                .unwrap_or_else(|| panic!("D is missing:\n{frame}"));
            assert!(
                row.contains("  E  "),
                "D and E share their row ({style:?} optimized={optimized}):\n{frame}"
            );
            let gap = &row[row.find("  D  ").expect("D") + 5..row.find("  E  ").expect("E")];
            assert!(
                gap.chars().filter(|ch| matches!(ch, '─' | '-')).count() >= 4,
                "D -> E keeps a visible shaft ({style:?} optimized={optimized}):\n{frame}"
            );
        }
    }
}

/// A thick or dotted line keeps its stroke where it crosses a frame's wall,
/// instead of turning thin there (`━━─━━`, `==-==`).
#[test]
fn a_styled_line_keeps_its_stroke_across_a_frame_wall() {
    let across = "flowchart LR\n  subgraph Write [Write Side]\n    ES[Event Store]\n  end\n  subgraph Read [Read Side]\n    P[Projector]\n  end\n  ES ==> P\n  ES -.-> Q[Audit]\n";
    let down = "flowchart TD\n  subgraph Plan [Plan]\n    A[Backlog]\n  end\n  subgraph Sprint [Sprint]\n    B[To Do]\n  end\n  A -.-> B\n  A ==> C[Board]\n";
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        for optimized in [false, true] {
            let render = |input: &str| {
                termiflow::render_with_feedback(
                    input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("render")
                .output
            };
            let frame = render(across);
            let row = frame
                .lines()
                .find(|line| line.contains("Projector"))
                .unwrap_or_else(|| panic!("Projector is missing:\n{frame}"));
            assert!(
                (row.contains("━━━") || row.contains("==="))
                    && !row.contains("━─━")
                    && !row.contains("=-="),
                "ES ==> P stays thick through the walls ({style:?} optimized={optimized}):\n{frame}"
            );
            let frame = render(down);
            let lines: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
            let label_y = lines
                .iter()
                .position(|line| line.iter().collect::<String>().contains("To Do"))
                .unwrap_or_else(|| panic!("To Do is missing:\n{frame}"));
            let head = lines[label_y - 2]
                .iter()
                .position(|&c| c == '↓' || c == 'v')
                .map(|x| (x, label_y - 2))
                .unwrap_or_else(|| panic!("no head over To Do:\n{frame}"));
            let shaft: String = lines[..head.1]
                .iter()
                .map(|line| line.get(head.0).copied().unwrap_or(' '))
                .collect();
            assert!(
                shaft.contains("╎╎") || shaft.contains("::"),
                "A -.-> B has a dotted shaft ({style:?} optimized={optimized}):\n{frame}"
            );
            assert!(
                !shaft.contains("╎│╎") && !shaft.contains(":|:"),
                "A -.-> B stays dotted through the walls ({style:?} optimized={optimized}):\n{frame}"
            );
        }
    }
}
