use termiflow::{BaseStyle, RenderOptions};

fn render(input: &str) -> String {
    termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Unicode))
        .expect("render")
        .output
}

/// The row holding the frame title `title`.
fn title_row<'a>(frame: &'a str, title: &str) -> &'a str {
    frame
        .lines()
        .find(|line| line.contains(&format!("  {title}")))
        .unwrap_or_else(|| panic!("no title {title}:\n{frame}"))
}

/// A TD line entering a frame passes its title row beside the title (frames
/// are drawn heavy, lines light), not hidden under the title text.
fn assert_line_beside_title(frame: &str, title: &str) {
    assert!(
        title_row(frame, title).contains('│'),
        "the entering line is hidden under {title}:\n{frame}"
    );
}

/// gen_c_081: x -> y entered Second down its centre, under the title, and
/// the arrow above y landed on the title row and was lost. The frame had the
/// room for a lane past the title only with a narrower margin.
#[test]
fn a_td_line_enters_a_narrow_titled_frame_past_its_title() {
    let input = "graph TD
    O[Origin] --> x
    subgraph S1 [First]
        x[x]
    end
    subgraph S2 [Second]
        y[y]
    end
    x --> y
    y --> O2[Result] --> O3[Store]
";
    let frame = render(input);
    assert_line_beside_title(&frame, "Second");
    let lines: Vec<&str> = frame.lines().collect();
    let y_row = lines
        .iter()
        .position(|line| line.contains("  y  "))
        .expect("y");
    assert!(
        lines[y_row - 2].contains('↓'),
        "no arrowhead above y:\n{frame}"
    );
}

/// gen_c_085: a -> b took Stack B's title-safe lane, which ran straight into
/// B inner's title. B inner grows by a column so the lane passes its title.
#[test]
fn a_td_line_passes_a_nested_frame_title() {
    let input = "graph TD
    subgraph A [Stack A]
        subgraph Ai [A inner]
            a[a]
        end
    end
    subgraph B [Stack B]
        subgraph Bi [B inner]
            b[b]
        end
    end
    a --> b
    b --> c[Out]
";
    let frame = render(input);
    assert_line_beside_title(&frame, "Stack B");
    assert_line_beside_title(&frame, "B inner");
}

/// gen_c_025: p -> q crosses two frames on its way into Right leaf, whose
/// title it ran through.
#[test]
fn a_td_line_through_nested_frames_passes_the_inner_title() {
    let input = "graph TD
    subgraph R [Root]
        subgraph L [Left]
            subgraph LL [Left leaf]
                p[p]
            end
        end
        subgraph Rt [Right]
            subgraph RR [Right leaf]
                q[q]
            end
        end
    end
    p --> q
    q --> z[Out]
";
    let frame = render(input);
    assert_line_beside_title(&frame, "Right leaf");
}

/// gen_c_097: p -> c passed Consumer's title in a lane and turned back on the
/// row above its arrow, where its label "data" was drawn over the turn. The
/// label now sits on the line's straight run above the frame.
#[test]
fn a_labelled_line_keeps_its_label_off_the_turn_below_a_title() {
    let input = "graph TD
    subgraph P [Producer]
        p[p]
    end
    subgraph C [Consumer]
        c[c]
    end
    p -->|data| c
    c -->|ack| p
";
    let frame = render(input);
    let lines: Vec<&str> = frame.lines().collect();
    let title = lines
        .iter()
        .position(|line| line.contains("  Consumer"))
        .expect("Consumer");
    let label = lines
        .iter()
        .position(|line| line.contains("data"))
        .expect("data");
    assert!(label < title, "the label is inside the frame:\n{frame}");
    assert!(
        lines[title + 1].contains('┘'),
        "no turn under the title:\n{frame}"
    );
}

/// Columns where a line crosses the wall next to `title` and carries on past
/// the title row, but the title row hides it under text.
fn lines_hidden_under_title(frame: &str, title: &str) -> Vec<usize> {
    let rows: Vec<Vec<char>> = frame.lines().map(|row| row.chars().collect()).collect();
    let at = rows
        .iter()
        .position(|row| row.iter().collect::<String>().contains(title))
        .unwrap_or_else(|| panic!("no title {title}:\n{frame}"));
    let line = |c: char| matches!(c, '│' | '|');
    (0..rows[at].len())
        .filter(|&x| {
            rows[at - 1].get(x).copied().is_some_and(line)
                && rows[at + 1]
                    .get(x)
                    .is_some_and(|&c| line(c) || matches!(c, '↓' | 'v'))
                && !line(rows[at][x])
        })
        .collect()
}

fn render_styles(input: &str) -> Vec<(BaseStyle, String)> {
    [BaseStyle::Unicode, BaseStyle::Ascii]
        .into_iter()
        .map(|style| {
            let frame =
                termiflow::render_with_feedback(input, RenderOptions::new().with_style(style))
                    .expect("render")
                    .output;
            (style, frame)
        })
        .collect()
}

/// gen_a_080: two lines enter Zero Trust Gateway, one under its title. No
/// stretch of the title row held the title with two blanks to each line, so
/// it stayed over the first line; it now moves between them with one.
#[test]
fn a_title_moves_between_two_entry_lines_with_one_blank_each_side() {
    let input = "flowchart TB
    subgraph Users [User Devices]
        Laptop[Laptops]
        Phones[Phones]
    end
    subgraph ZT [Zero Trust Gateway]
        IdCheck[Identity Check]
        Posture[Device Posture]
        Policy{Policy Engine}
        IdCheck --> Policy
        Posture --> Policy
    end
    subgraph Apps [Internal Apps]
        Wiki[Wiki]
        Admin[Admin Panel]
    end
    Laptop & Phones --> IdCheck
    Laptop & Phones --> Posture
    Policy -->|allow| Wiki
    Policy -->|allow admins| Admin
";
    for (style, frame) in render_styles(input) {
        assert_eq!(
            lines_hidden_under_title(&frame, "Zero Trust Gateway"),
            Vec::<usize>::new(),
            "no line hides under the title ({style:?}):\n{frame}"
        );
    }
}

/// gen_a_160: SLO Engine's title moves off the fan-in entering under it. A
/// title with room for two blanks to the line keeps them.
#[test]
fn a_moved_title_keeps_two_blanks_to_a_line_where_it_has_room() {
    let input = "flowchart TB
    subgraph Inputs
        Logs[(App Logs)]
        Traces[(Traces)]
    end
    subgraph SLO [SLO Engine]
        SLI[Compute SLIs]
        Budget[Error Budget]
        Burn{Burn rate high?}
        SLI --> Budget --> Burn
    end
    Logs --> SLI
    Traces --> SLI
    Burn --> Page>Page On-call]
    Burn --> Ticket[Low-priority Ticket]
    Budget --> Freeze[Feature Freeze Policy]
";
    for (style, frame) in render_styles(input) {
        let row = title_row(&frame, "SLO Engine");
        assert!(
            row.contains("│  SLO Engine") || row.contains("|  SLO Engine"),
            "two blanks between the line and the title ({style:?}):\n{frame}"
        );
    }
}

/// gen_a_241: the line into Databases runs under the space between "2:" and
/// "Stateful", where it still shows. With no room for two blanks to the
/// lines, the title stays put rather than sit beside a line like a label.
#[test]
fn a_line_under_a_space_in_a_title_leaves_it_in_place() {
    let input = "graph TD
    Assess[Cloud Readiness Assessment] --> Wave{Migration wave}
    subgraph W1 [Wave 1: Stateless]
        Web[Web Apps]
        Batch[Batch Jobs]
    end
    subgraph W2 [Wave 2: Stateful]
        DBs[(Databases)]
        Files[(File Shares)]
    end
    Wave --> Web
    Wave --> Batch
    Wave --> DBs
    Wave --> Files
    Web & Batch --> Validate[Validate in Cloud]
    DBs & Files --> Validate
    Validate --> Decom[Decommission On-prem]
";
    for (style, frame) in render_styles(input) {
        let row = title_row(&frame, "Wave 2: Stateful");
        assert!(
            row.contains("┃  Wave 2: Stateful") || row.contains("|  Wave 2: Stateful"),
            "the title stays at the frame's wall ({style:?}):\n{frame}"
        );
    }
}
