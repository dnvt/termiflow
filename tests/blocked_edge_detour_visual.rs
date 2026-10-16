use termiflow::{BaseStyle, RenderOptions};

fn unicode(input: &str) -> String {
    termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Unicode))
        .expect("render")
        .output
}

fn arrowheads(frame: &str) -> usize {
    frame.chars().filter(|c| "→←↑↓".contains(*c)).count()
}

/// A skip-rank edge stacked in line with the boxes between its ends is
/// drawn around them, not hidden under them.
#[test]
fn a_skip_rank_edge_detours_around_the_boxes_in_its_way() {
    for direction in ["TD", "LR"] {
        let frame = unicode(&format!("graph {direction}\nA-->B\nB-->C\nC-->D\nA-->D\n"));
        assert_eq!(arrowheads(&frame), 4, "A->D hidden ({direction}):\n{frame}");
    }
}

/// In LR, a skip-rank edge into a target that other edges also reach is
/// still drawn, with its own head.
#[test]
fn an_lr_skip_rank_edge_into_a_shared_target_stays_visible() {
    let frame = unicode("graph LR\nP-->S\nS-->X\nX-->H\nH-->L\nS-->L\nL-->Z\n");
    assert_eq!(arrowheads(&frame), 6, "S->L hidden:\n{frame}");
}

/// A diamond scene that cannot be planned no longer drops its edges whose
/// straight path is clear: `A` keeps its stem to `B`.
#[test]
fn a_rejected_diamond_keeps_its_clear_edges() {
    let frame = unicode("graph TD\nA --> B\nB --> C\nC --> D\nA --> D\nB --> D\n");
    let under_a = frame.lines().nth(2).unwrap_or_default();
    assert!(under_a.contains('┬'), "A has no edge out:\n{frame}");
}

/// A skip-rank edge whose straight path runs along the border column of the
/// boxes in its way detours around them instead of hiding under the border.
#[test]
fn a_skip_rank_edge_along_a_border_column_detours() {
    for direction in ["TD", "BT"] {
        let frame = unicode(&format!(
            "graph {direction}\nA-->B\nB-->C\nC-->D\nA-->D\nA-->E\n"
        ));
        assert_eq!(arrowheads(&frame), 5, "A->D hidden ({direction}):\n{frame}");
    }
}

/// When the port its lane would reach is already held by another route
/// into the target, a skip-rank detour enters the target from the side.
#[test]
fn a_skip_rank_detour_enters_from_the_side_when_its_port_is_taken() {
    for direction in ["TD", "BT", "LR", "RL"] {
        let frame = unicode(&format!(
            "graph {direction}\nA-->B\nB-->C\nC-->D\nA-->D\nB-->D\n"
        ));
        assert_eq!(arrowheads(&frame), 5, "A->D hidden ({direction}):\n{frame}");
    }
}

/// An edge from outside a frame to a box inside it, with another box of
/// that frame in the way, is drawn around the box through the frame's wall
/// instead of hidden under the box.
#[test]
fn a_skip_rank_edge_into_a_frame_detours_around_the_boxes_in_its_way() {
    for direction in ["LR", "RL", "TD"] {
        let frame = unicode(&format!(
            "graph {direction}\n  subgraph R [R]\n    C --> D\n  end\n  U --> C\n  U --> D\n"
        ));
        assert_eq!(arrowheads(&frame), 3, "U->D hidden ({direction}):\n{frame}");
    }
}

/// gen_a_054: UI feeds the first box of one frame and the last box of the
/// next; UI -> QH ran under every box between them. It now runs below both
/// frames and enters its own through the bottom wall.
#[test]
fn an_edge_past_a_whole_frame_detours_below_it() {
    let frame = unicode(
        "graph RL
    subgraph W [W]
        A --> B
    end
    subgraph R [R]
        C --> D
    end
    B --> C
    U --> A
    U --> D
",
    );
    assert_eq!(arrowheads(&frame), 5, "U->D hidden:\n{frame}");
}

/// gen_c_027: Source -> Child one ran up through Parent node. Its detour
/// now passes Child two's line, which crosses it and carries on into Child
/// two.
#[test]
fn a_line_crossing_a_detour_carries_on() {
    let frame = unicode(
        "graph BT
    Src[Source] --> P1
    Src --> C1
    Src --> C2
    subgraph Par [Parent]
        P1[Parent node]
        subgraph Ch [Child]
            C1[Child one]
            C2[Child two]
        end
    end
",
    );
    let lines: Vec<&str> = frame.lines().collect();
    let label = lines
        .iter()
        .position(|line| line.contains("Parent node"))
        .expect("Parent node");
    assert!(
        !lines[label - 1].contains('┴'),
        "a line runs through Parent node:\n{frame}"
    );
    assert!(
        frame.contains('┼'),
        "Child two's line stops at the detour:\n{frame}"
    );
    // Child two's branch leaves Source's bus where the detour runs along it.
    let bus = lines
        .iter()
        .rev()
        .find(|line| line.contains('└') && line.contains('┬'))
        .expect("Source's bus");
    assert!(
        bus.contains('┴'),
        "a branch does not leave the bus it shares with the detour:\n{frame}"
    );
    assert_eq!(arrowheads(&frame), 3, "an edge is hidden:\n{frame}");
}

/// gen_a_247: a fan-out branch reaches its target along the target's own
/// row, here clear of the box on the source's row, so it keeps that row
/// instead of detouring out of the frame.
#[test]
fn a_fan_out_branch_clear_of_the_box_keeps_its_row() {
    let frame = unicode(
        "flowchart LR
    subgraph Ticketing [Help Center]
        Article[(Help Articles)]
        Search[Article Search]
        Search --> Article
    end
    Visitor((Visitor)) --> Search
    Search --> Found{Answer found?}
    Found --> Deflect[Deflected]
    Found --> Contact[Contact Form]
    Contact --> Ticket[Create Ticket]
    Ticket --> Gap[Content Gap Report]
    Gap --> Article
",
    );
    let bottom = frame
        .lines()
        .find(|line| line.contains('┗'))
        .expect("frame bottom");
    assert!(
        !bottom.contains('│'),
        "a line detours out of the frame:\n{frame}"
    );
}

/// gen_a_066: Feature Jobs -> Trainer used to leave Feature Engineering
/// through its bottom wall. Its detour now turns right above that wall, and
/// in ASCII, where the turn is a `+`, the wall below it stays whole.
#[test]
fn an_ascii_wall_under_a_detour_turn_stays_whole() {
    let frame = termiflow::render_with_feedback(
        "graph TD
    subgraph Pipeline [ML Pipeline]
        subgraph Features [Feature Engineering]
            Raw[(Raw Events)]
            FE[Feature Jobs]
            Raw --> FE
        end
        subgraph Training
            Trainer[Trainer]
            HPO[HPO Sweep]
            HPO --> Trainer
        end
        FE --> Trainer
    end
    Trainer --> Registry[[Model Registry]]
    Registry --> Serving[Online Serving]
    Serving -.->|feedback| Raw
",
        RenderOptions::new().with_style(BaseStyle::Ascii),
    )
    .expect("render")
    .output;
    let lines: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
    let title = lines
        .iter()
        .position(|line| String::from_iter(line).contains("Feature Engineering"))
        .expect("title");
    let wall = lines[title]
        .iter()
        .collect::<String>()
        .find("|  Feature Engineering")
        .expect("left wall");
    let bottom = lines
        .iter()
        .skip(title + 1)
        .find(|line| line.get(wall) == Some(&'+') && line.get(wall + 1) == Some(&'-'))
        .expect("bottom wall");
    let right = (wall + 1..bottom.len())
        .find(|&x| bottom[x] == '+')
        .expect("bottom-right corner");
    assert!(
        bottom[wall..=right].iter().all(|&c| c == '+' || c == '-'),
        "Feature Engineering's bottom wall is broken:\n{frame}"
    );
}

/// A labelled skip-rank edge takes the detour too, its label on the lane.
#[test]
fn a_labelled_skip_rank_edge_detours_with_its_label_on_the_lane() {
    let frame = unicode("graph LR\nA-->B\nB-->C\nC-->D\nA-->|skip| D\n");
    assert_eq!(arrowheads(&frame), 4, "A->D hidden:\n{frame}");
    let label = frame
        .lines()
        .find(|line| line.contains("skip"))
        .expect("label");
    assert!(
        label.contains("└─") && label.contains("─┘"),
        "the label is off its lane:\n{frame}"
    );
}

/// gen_d_023: A -> C's detour would run where A -> B's long label is drawn,
/// which cuts it, so it keeps its old route and its label stays between A
/// and C.
#[test]
fn a_detour_another_label_would_cut_is_dropped() {
    let frame = termiflow::render_with_feedback(
        "graph BT
A -->|this edge label is long enough that it should wrap onto two lines| B
B -->|another long wrapped edge label for the second hop| C
A -->|skip| C
",
        RenderOptions::new()
            .with_style(BaseStyle::Unicode)
            .with_wrap_labels(true)
            .with_max_label_lines(2),
    )
    .expect("render")
    .output;
    let lines: Vec<&str> = frame.lines().collect();
    let skip = lines
        .iter()
        .position(|line| line.contains("skip"))
        .expect("skip");
    let a = lines
        .iter()
        .position(|line| line.contains("│  A  │"))
        .expect("A");
    assert!(skip < a, "skip is not between A and C:\n{frame}");
}

/// gen_a_013: Paid -> Refunded leaves Paid on a detour; Paid -> Shipped
/// meets that detour's first run and joins it with a tee.
#[test]
fn a_sibling_line_joins_a_detour_with_a_tee() {
    let frame = unicode(
        "graph BT
    Idle((Idle)) -->|order placed| Pending[Pending]
    Pending -->|payment ok| Paid[Paid]
    Pending -->|timeout| Cancelled[Cancelled]
    Paid -->|picked| Shipped[Shipped]
    Shipped -->|delivered| Delivered(((Delivered)))
    Shipped -->|lost| Refund[Refunded]
    Paid -->|customer cancels| Refund
",
    );
    let lines: Vec<&str> = frame.lines().collect();
    let label = lines
        .iter()
        .position(|line| line.contains("customer cancels"))
        .expect("detour label");
    assert!(
        lines[label + 1].contains('┤'),
        "Paid -> Shipped stops at the detour:\n{frame}"
    );
}

/// gen_a_043: Solved? -> Close Ticket ran up through Tier 2 Support and the
/// second Solved?, its `yes` drawn beside the stem. It now detours, its
/// label on the lane that reaches Close Ticket.
#[test]
fn a_decision_branch_label_rides_its_detour() {
    let frame = unicode(
        "graph BT
    Customer((Customer)) --> Ticket[Open Ticket]
    Ticket --> L1[Tier 1 Support]
    L1 --> Solved1{Solved?}
    Solved1 -->|yes| Close[Close Ticket]
    Solved1 -->|no| L2[Tier 2 Support]
    L2 --> Solved2{Solved?}
    Solved2 -->|yes| Close
    Solved2 -->|no| Eng[Engineering Escalation]
    Eng --> Close
    Close --> CSAT[/CSAT Survey/]
",
    );
    let end_column = |needle: &str, corner: char| {
        let line = frame
            .lines()
            .find(|line| line.contains(needle))
            .expect(needle);
        let trimmed = line.trim_end();
        assert!(trimmed.ends_with(corner), "{needle} row:\n{frame}");
        trimmed.chars().count()
    };
    assert_eq!(
        end_column("─ yes ─", '┘'),
        end_column("Close Ticket", '┐'),
        "yes is not on the lane into Close Ticket:\n{frame}"
    );
}
