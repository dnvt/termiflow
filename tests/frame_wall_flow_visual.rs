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

/// The rows of `frame` as characters, and the index of the row of heads
/// (`↓`/`v`) right above the box holding `label`.
fn head_row_above(frame: &str, label: &str) -> (Vec<Vec<char>>, usize) {
    let lines: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
    let label_y = lines
        .iter()
        .position(|line| line.iter().collect::<String>().contains(label))
        .unwrap_or_else(|| panic!("{label} is missing:\n{frame}"));
    (lines, label_y - 2)
}

/// A frame member fanning out to targets right below the frame draws its
/// bus outside the frame, below the wall, instead of along the wall (where
/// the wall hides it) or right inside it.
#[test]
fn a_fan_out_leaving_a_frame_runs_its_bus_below_the_wall() {
    let inputs = [
        "flowchart TD\n  subgraph Scan [Malware Scan]\n    AV[Antivirus Scan] --> Clean{Clean?}\n  end\n  Client([Client]) --> AV\n  Clean --> Promote[Move to Public Bucket]\n  Clean --> Q[(Quarantine)]\n",
        "graph TD\n  In[Input] --> Dec\n  subgraph Chk [Checker]\n    Dec{Valid?}\n  end\n  Dec -->|yes| Ok[Accept]\n  Dec -->|no| Bad[Reject]\n  Bad --> In\n",
    ];
    for input in inputs {
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
                let target = if input.contains("Quarantine") {
                    "Quarantine"
                } else {
                    "Reject"
                };
                let (lines, heads) = head_row_above(&frame, target);
                // A labelled branch keeps its label between bus and head.
                let bus_y = if input.contains("yes") {
                    heads - 2
                } else {
                    heads - 1
                };
                let (corners, tee, vertical) = match style {
                    BaseStyle::Ascii => (('+', '+'), '+', '|'),
                    _ => (('┌', '┐'), '┴', '│'),
                };
                // The bus: the unbroken run of line from its first corner.
                let row = &lines[bus_y];
                let left = row.iter().position(|&c| c == corners.0);
                let right = left
                    .map(|left| {
                        left + row[left..]
                            .iter()
                            .take_while(|&&c| matches!(c, '-' | '+' | '─' | '┌' | '┐' | '┴'))
                            .count()
                            - 1
                    })
                    .filter(|&right| right > left.unwrap_or(right) && row[right] == corners.1);
                let (Some(left), Some(right)) = (left, right) else {
                    panic!("no bus above the heads ({style:?} optimized={optimized}):\n{frame}");
                };
                assert!(
                    row[left + 1..right].contains(&tee) && !row[left..=right].contains(&vertical),
                    "the bus is a clear row of its own ({style:?} optimized={optimized}):\n{frame}"
                );
                // The wall is right above the bus, or a stem row above it;
                // only the stem crosses it over the bus's span.
                let span = |y: usize| {
                    let row = &lines[y];
                    &row[left.min(row.len())..(right + 1).min(row.len())]
                };
                let wall_y = (1..=2)
                    .map(|up| bus_y - up)
                    .find(|&y| span(y).iter().any(|&c| matches!(c, '─' | '━' | '-')))
                    .unwrap_or_else(|| {
                        panic!("no wall right above the bus ({style:?} optimized={optimized}):\n{frame}")
                    });
                for y in wall_y..bus_y {
                    assert_eq!(
                        span(y).iter().filter(|&&c| c == vertical).count(),
                        1,
                        "only the stem crosses the wall above the bus ({style:?} optimized={optimized}):\n{frame}"
                    );
                }
            }
        }
    }
}

/// A fan-in joining lines from two frames turns right below the lower
/// frame's wall, instead of along it.
#[test]
fn a_fan_in_from_two_frames_turns_below_the_wall() {
    let input = "graph TD\n  X[Fork] --> a & b\n  subgraph S1 [S1]\n    a[a]\n  end\n  subgraph S2 [S2]\n    b[b]\n  end\n  a & b --> Y[Join]\n";
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
            let (lines, heads) = head_row_above(&frame, "Join");
            let turn: String = lines[heads - 1]
                .iter()
                .collect::<String>()
                .trim()
                .to_owned();
            let ascii = style == BaseStyle::Ascii;
            assert!(
                !turn.contains(if ascii { '|' } else { '│' })
                    && turn
                        .chars()
                        .filter(|&c| if ascii { c == '+' } else { c == '┴' })
                        .count()
                        == usize::from(ascii) * 2,
                "b's line turns on a row of its own ({style:?} optimized={optimized}):\n{frame}"
            );
        }
    }
}

/// Guard: a fan-out into other frames keeps its bus where it was, instead
/// of moving it beside a frame's wall, where the wall would hide it
/// (gen_c_083's hub fanning out to four frames); and one whose targets also
/// gather another frame member's line keeps its layout (gen_a_151's Event
/// Bus gained a stray crossing on its top wall).
#[test]
fn a_fan_out_into_other_frames_keeps_its_bus_off_their_walls() {
    let star = "graph BT\n  subgraph Cn [Center]\n    c((hub))\n  end\n  subgraph N [N]\n    n[n]\n  end\n  subgraph E [E]\n    e[e]\n  end\n  subgraph S [S]\n    s[s]\n  end\n  subgraph W [W]\n    w[w]\n  end\n  c --> n\n  c --> e\n  c --> s\n  c --> w\n";
    let shared = "graph TD\n  subgraph Producers\n    Web[Web App]\n    Mobile[Mobile App]\n    Partner[Partner API]\n  end\n  subgraph Bus [Event Bus]\n    Topic1[[user.events]]\n    Topic2[[order.events]]\n  end\n  Web --> Topic1\n  Mobile --> Topic1\n  Web --> Topic2\n  Partner --> Topic2\n  Topic1 --> Lake[(Data Lake)]\n  Topic2 --> Lake\n  Topic2 --> Fraud[Fraud Service]\n";
    for optimized in [false, true] {
        let render = |input: &str| {
            termiflow::render_with_feedback(
                input,
                RenderOptions::new().with_optimize_render(optimized),
            )
            .expect("render")
            .output
        };
        // Frames are drawn heavy, lines light: no light line on a wall.
        let frame = render(star);
        for row in frame.lines() {
            let cells: Vec<char> = row.chars().collect();
            let light = |c: char| matches!(c, '─' | '┴' | '┬' | '┼' | '┌' | '┐' | '└' | '┘');
            // A light cell between wall cells, or right beside a run of them.
            let on_wall = cells.windows(3).any(|w| {
                (w[0] == '━' && w[2] == '━' && light(w[1]))
                    || (light(w[0]) && w[1] == '━' && w[2] == '━')
                    || (w[0] == '━' && w[1] == '━' && light(w[2]))
            });
            assert!(
                !on_wall,
                "no line runs along a frame's wall (optimized={optimized}):\n{frame}"
            );
        }
        let frame = render(shared);
        let lines: Vec<&str> = frame.lines().collect();
        let title = lines
            .iter()
            .position(|line| line.contains("Event Bus"))
            .unwrap_or_else(|| panic!("Event Bus is missing:\n{frame}"));
        assert_eq!(
            lines[title - 1].chars().filter(|&c| c == '│').count(),
            2,
            "two lines enter Event Bus, one per topic (optimized={optimized}):\n{frame}"
        );
    }
}

/// BT: a fan-in from outside a frame into one of its members merges below
/// the frame's bottom wall, where one trunk then crosses it, instead of on
/// the wall, which hid the merge and left the sources' lines ending there.
#[test]
fn a_bt_fan_in_into_a_frame_merges_below_its_wall() {
    let input = "graph BT\n  subgraph App [Application]\n    API[REST API]\n    Worker[Background Worker]\n  end\n  API --> PG[(Postgres)]\n  UI[Web UI] --> API\n  Mobile[Mobile App] --> API\n";
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
            let lines: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
            let label_y = lines
                .iter()
                .position(|line| line.iter().collect::<String>().contains("Mobile App"))
                .unwrap_or_else(|| panic!("Mobile App is missing:\n{frame}"));
            let (corners, tee, vertical) = match style {
                BaseStyle::Ascii => (('+', '+'), '+', '|'),
                _ => (('┌', '┐'), '┴', '│'),
            };
            // The merge runs on the row right above the sources' tops.
            let bus = &lines[label_y - 2];
            let left = bus.iter().position(|&c| c == corners.0);
            let right = left
                .map(|left| {
                    left + bus[left..]
                        .iter()
                        .take_while(|&&c| matches!(c, '-' | '+' | '─' | '┌' | '┐' | '┴'))
                        .count()
                        - 1
                })
                .filter(|&right| right > left.unwrap_or(right) && bus[right] == corners.1);
            let (Some(left), Some(right)) = (left, right) else {
                panic!("no merge below the frame ({style:?} optimized={optimized}):\n{frame}");
            };
            assert!(
                bus[left + 1..right].contains(&tee),
                "the merge has its trunk ({style:?} optimized={optimized}):\n{frame}"
            );
            let wall = &lines[label_y - 3];
            let wall = &wall[left.min(wall.len())..(right + 1).min(wall.len())];
            assert_eq!(
                wall.iter().filter(|&&c| c == vertical).count(),
                1,
                "one trunk crosses the wall above the merge ({style:?} optimized={optimized}):\n{frame}"
            );
        }
    }
}

/// Guard: a BT fan-in whose wide source spans the trunk, right under the
/// frame, keeps merging where it did: a merge on that source's exit row
/// would send its line straight up the trunk and leave its branch hanging
/// (gen_a_168).
#[test]
fn a_bt_fan_in_from_a_source_under_the_trunk_keeps_its_branches() {
    let input = "graph BT\n  subgraph Cache [Multi-tier Cache]\n    L1[In-process LRU]\n    L2[(Redis)]\n    L1 --> L2\n  end\n  Req([Request]) --> L1\n  Pub[Invalidation Pub/Sub] -.-> L1\n";
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
            let lines: Vec<&str> = frame.lines().collect();
            let label_y = lines
                .iter()
                .position(|line| line.contains("Invalidation"))
                .unwrap_or_else(|| panic!("Pub is missing:\n{frame}"));
            let exits = lines[label_y - 2].trim_start();
            assert!(
                !exits.starts_with(['─', '-']),
                "no branch hangs loose above the sources ({style:?} optimized={optimized}):\n{frame}"
            );
        }
    }
}
