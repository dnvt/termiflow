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
/// through its bottom wall. Its detour now crosses that wall and turns below
/// it, and in ASCII, where every turn is a `+`, the wall stays whole but for
/// that crossing.
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
    let row = (title + 1..lines.len())
        .find(|&y| lines[y].get(wall) == Some(&'+') && lines[y].get(wall + 1) == Some(&'-'))
        .expect("bottom wall");
    let bottom = &lines[row];
    let right = (wall + 1..bottom.len())
        .find(|&x| bottom[x] == '+')
        .expect("bottom-right corner");
    let crossing = |x: usize| {
        bottom[x] == '|'
            && lines[row - 1].get(x) == Some(&'|')
            && matches!(lines[row + 1].get(x), Some('|' | '+'))
    };
    assert!(
        (wall..=right).all(|x| bottom[x] == '-' || x == wall || x == right || crossing(x)),
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

/// gen_c_035: every lane inside the frames crossed a title, so Form -> DB
/// stayed hidden under Gateway. It now runs outside the frames, leaving and
/// entering them through their sides.
#[test]
fn a_detour_with_no_lane_inside_its_frames_runs_outside_them() {
    let frame = unicode(
        "graph BT
    subgraph S1 [Front]
        f[Form]
    end
    subgraph S2 [Back]
        g[Gateway]
        subgraph S2a [DB layer]
            db[(DB)]
        end
    end
    f --> db
    f --> g
    g --> db
",
    );
    assert_eq!(arrowheads(&frame), 3, "Form -> DB hidden:\n{frame}");
}

/// gen_a_211: Level -> Allow is gathered along Level's line through the
/// boxes below it but has no detour; Level -> Block keeps its own.
#[test]
fn a_fan_in_edge_without_a_detour_leaves_its_siblings_detoured() {
    let frame = unicode(
        "graph TD
    Login[Login Attempt] --> Risk[Risk Engine]
    Risk --> Level{Risk level}
    Level --> Allow[Allow]
    Level --> StepUp[Step-up MFA]
    Level --> Block[Block and Alert]
    StepUp --> MFAok{MFA passed?}
    MFAok --> Allow
    MFAok --> Block
",
    );
    let block = frame
        .lines()
        .find(|line| line.contains("Block and Alert"))
        .expect("Block row");
    assert!(block.contains('←'), "Level -> Block hidden:\n{frame}");
}

/// gen_c_092: the skip lane under the frames it passes keeps a blank row
/// between it and their bottom walls, rather than reading as a second wall.
#[test]
fn a_detour_lane_keeps_off_frame_walls() {
    let frame = unicode(
        "graph RL
    subgraph A [First]
        a[a]
    end
    subgraph B [Second]
        b[b]
    end
    subgraph C [Third]
        c[c]
    end
    a --> b --> c
    a -->|skip| c
",
    );
    let lines: Vec<&str> = frame.lines().collect();
    let bottom = lines
        .iter()
        .rposition(|line| line.contains('┗'))
        .expect("frame bottom");
    assert!(
        lines.iter().any(|line| line.contains(" skip ")),
        "no skip lane:\n{frame}"
    );
    assert!(
        !lines[bottom + 1].contains('─'),
        "the skip lane runs right under the frames' bottom walls:\n{frame}"
    );
}

/// gen_a_070: Score -> Archive is gathered into Archive down Score's own
/// line, through Email Drip and Engaged?, before it turns; it now detours
/// down the right side, clear of other lines, its label beside it.
#[test]
fn a_fan_in_edge_blocked_before_its_turn_detours() {
    let frame = unicode(
        "flowchart TB
    Lead[Inbound Lead] --> Enrich[Clearbit Enrich]
    Enrich --> Score{Lead score}
    Score -->|hot| SDR[Assign SDR]
    Score -->|warm| Drip[Email Drip]
    Score -->|cold| Archive[Archive]
    Drip --> Engaged{Engaged?}
    Engaged -->|yes| SDR
    Engaged -->|no| Archive
    SDR --> Demo[Book Demo]
",
    );
    let archive = frame
        .lines()
        .find(|line| line.contains("Archive"))
        .expect("Archive row");
    assert!(archive.contains("│←┘"), "Score -> Archive hidden:\n{frame}");
    assert!(
        frame.contains("│ cold"),
        "cold is not on its lane:\n{frame}"
    );
}

/// gen_a_179: State -> SIS, blocked on a line into SIS, takes the lane under
/// the schools before Portal -> SIS, which is blocked only before its turn,
/// can; the reverse left State's line through High School.
#[test]
fn an_edge_blocked_into_its_target_takes_its_lane_first() {
    let frame = unicode(
        "graph RL
    subgraph School [School District]
        subgraph Schools
            HS[High School]
            MS[Middle School]
            ES[Elementary]
        end
        SIS[(Student Info System)]
        LMS[LMS]
    end
    HS & MS & ES --> SIS
    SIS --> LMS
    Parents((Parents)) --> Portal[Parent Portal]
    Portal --> SIS
    State[State Reporting] --> SIS
",
    );
    let high_school = frame
        .lines()
        .find(|line| line.contains("High School"))
        .expect("High School row");
    assert!(
        high_school.contains("High School  │"),
        "a line runs through High School:\n{frame}"
    );
}

/// gen_a_056: Guest -> ISP is blocked only before its turn; its detours
/// would cross the Printers and Corporate WiFi stems or HQ Office's wall,
/// so none is taken.
#[test]
fn a_detour_for_an_edge_blocked_before_its_turn_crosses_no_line_or_wall() {
    let frame = unicode(
        "graph TD
    subgraph Office [HQ Office]
        WiFi[Corporate WiFi]
        Guest[Guest WiFi]
        Printer[Printers]
    end
    subgraph Remote
        VPN[VPN Client]
    end
    WiFi --> Core[Core Switch]
    Printer --> Core
    VPN --> Concentrator[VPN Concentrator]
    Concentrator --> Core
    Guest --> ISP((ISP))
    Core --> FW[Firewall]
    FW --> ISP
",
    );
    let inside: Vec<&str> = frame
        .lines()
        .take_while(|line| !line.contains('┗'))
        .collect();
    assert!(
        inside.iter().all(|line| !line.contains('┼')),
        "a detour crosses the stems inside HQ Office:\n{frame}"
    );
    let wall = inside[0].chars().position(|c| c == '┏').expect("HQ Office");
    assert!(
        inside[1..]
            .iter()
            .all(|line| line.chars().nth(wall) == Some('┃')),
        "a detour crosses HQ Office's left wall:\n{frame}"
    );
}

/// gen_d_150: A -> H is blocked only before its turn; its detour would cross
/// E's trunk, and D's fan-in would then lose its second head to a merged
/// bus, so it is not taken.
#[test]
fn a_crossed_detour_for_an_edge_blocked_before_its_turn_is_dropped() {
    let frame = unicode(
        "%% termiflow: spacing=spacious
graph BT
A --> B --> C --> D
E --> F --> G --> H
A --> H
E --> D
B --> G
F --> C
",
    );
    let lines: Vec<&str> = frame.lines().collect();
    let d = lines
        .iter()
        .find(|line| line.contains("│  D  │"))
        .expect("D row");
    let (left, width) = (d.chars().position(|c| c == '│').expect("D"), 7);
    let heads = lines
        .iter()
        .find(|line| line.contains('↑'))
        .expect("head row");
    let under_d = heads.chars().skip(left).take(width).filter(|c| *c == '↑');
    assert_eq!(under_d.count(), 2, "D lost a head:\n{frame}");
}

/// gen_a_007: Mgr -> Rej was gathered into Rejected along Mgr's own row,
/// through the Finance diamond.  Its detour leaves on the run Mgr's fan-out
/// shares, which is now that fan-out's own line rather than a stray one the
/// detour seemed to cut, so it is kept, its label with it.
#[test]
fn a_detour_leaving_on_a_fan_out_run_is_kept() {
    let input = "flowchart LR
    Req[Expense Request] --> Mgr{Manager Approves?}
    Mgr -- no --> Rej[Rejected]
    Mgr -- yes --> Amt{Over 5000 USD?}
    Amt -- yes --> Fin{Finance Approves?}
    Amt -- no --> Pay[Reimburse]
    Fin -- yes --> Pay
    Fin -- no --> Rej
    Pay --> Notify([Notify Employee])
    Rej --> Notify
";
    let frame = unicode(input);
    assert!(
        frame.contains("<  Finance Approves?  >"),
        "a line runs through Finance Approves?:\n{frame}"
    );
    let below = frame
        .lines()
        .skip_while(|line| !line.contains("Rejected"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        below.contains("─ no ─"),
        "Mgr -> Rej has no lane below:\n{frame}"
    );
    // In ASCII the turn under the "yes" label is a `+` like any tee; the
    // line above the label must not grow an arm toward it.
    let ascii =
        termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Ascii))
            .expect("render")
            .output;
    let reimburse = ascii
        .lines()
        .find(|line| line.contains("Reimburse  +-+"))
        .expect("Reimburse row");
    assert!(
        reimburse.contains("+----->+  Reimburse"),
        "the arrow into Reimburse grew a stray `+`:\n{ascii}"
    );
}

/// gen_a_082: Ok -> Release ran along Schedule Shipment's top border; its
/// detour now keeps a blank row between its lane and Order Complete, and the
/// "yes" label nudged off its turn stays whole beside Schedule Shipment.
#[test]
fn a_detour_lane_keeps_a_blank_row_off_the_boxes_it_passes() {
    let frame = unicode(
        "flowchart LR
    Order[Create Order] --> Reserve[Reserve Inventory]
    Reserve --> Charge[Charge Payment]
    Charge --> Ok{Charge succeeded?}
    Ok -->|yes| Ship[Schedule Shipment]
    Ok -->|no| Release[Release Inventory]
    Release --> CancelO[Cancel Order]
    Ship --> ShipOk{Carrier accepted?}
    ShipOk -->|no| Refund[Refund Payment]
    Refund --> Release
    ShipOk -->|yes| Complete(((Order Complete)))
",
    );
    let lines: Vec<&str> = frame.lines().collect();
    let lane = lines
        .iter()
        .rposition(|line| line.contains("─ no ─"))
        .unwrap_or_else(|| panic!("Ok -> Release has no lane below:\n{frame}"));
    assert!(
        !lines[lane - 1].contains('╰'),
        "the lane runs right under Order Complete:\n{frame}"
    );
    assert!(
        !frame.contains("┐yes"),
        "the yes label is wedged against its branch's turn:\n{frame}"
    );
    assert_eq!(
        frame.matches("yes").count(),
        2,
        "a yes label is cut:\n{frame}"
    );
}

/// gen_c_092: the skip detour left a's line one cell out, right inside the
/// First frame's wall; it now turns past the wall, in the gap between the
/// frames, and no leg runs beside a wall.
#[test]
fn a_detour_leg_turns_past_a_frame_wall_it_would_run_beside() {
    let frame = unicode(
        "graph RL
    subgraph A [First]
        a[a]
    end
    subgraph B [Second]
        b[b]
    end
    subgraph C [Third]
        c[c]
    end
    a --> b --> c
    a -->|skip| c
",
    );
    assert!(
        frame.contains(" skip "),
        "the skip edge is not drawn:\n{frame}"
    );
    assert!(
        !frame.contains("┃│") && !frame.contains("│┃"),
        "a detour leg runs beside a frame wall:\n{frame}"
    );
}

/// gen_c_069: cn -> O ran under Parent node; with no lane taking it one
/// cell out of cn's frame, its detour now turns further along cn's line,
/// past the wall, and O shows both its heads.
#[test]
fn a_detour_turns_further_along_its_line_when_it_must() {
    let frame = unicode(
        "graph TD
    subgraph P [Parent]
        pn[Parent node]
        subgraph C [Child]
            cn[Child node]
        end
    end
    E[Entry] --> cn
    cn --> pn
    cn --> O[Outside]
    pn --> O
",
    );
    assert_eq!(arrowheads(&frame), 4, "cn -> O hidden:\n{frame}");
}

/// gen_a_177: the only leg left for Auth -> Txn Svc ran down between Core
/// Domain's and Accounts' walls with a single blank on either side, which
/// reads as a third wall (`| | |` in ASCII); it is not taken, and the edge
/// keeps its generic route.
#[test]
fn a_detour_leg_stays_out_of_a_corridor_between_walls() {
    let frame = unicode(
        "flowchart LR
    subgraph Users [User Plane]
        Web[Web]
        iOS[iOS]
        Android[Android]
    end
    subgraph Edge
        GW[API Gateway]
        Auth[OIDC Auth]
        GW --> Auth
    end
    subgraph Core [Core Domain]
        subgraph Accounts [Accounts]
            AccSvc[Account Svc]
            AccDB[(Accounts DB)]
            AccSvc --> AccDB
        end
        subgraph Txns [Transactions]
            TxSvc[Txn Svc]
            TxDB[(Txn DB)]
            TxSvc --> TxDB
        end
        AccSvc --> TxSvc
    end
    subgraph Async [Async]
        Bus[[Event Bus]]
        Statements[Statement Gen]
        Alerts[Alerts]
        Bus --> Statements
        Bus --> Alerts
    end
    Web & iOS & Android --> GW
    Auth --> AccSvc
    Auth --> TxSvc
    TxSvc ==> Bus
    Statements --> PDF[/PDF Storage/]
    Alerts --> Push[Push Service]
",
    );
    assert!(
        !frame.contains("┃ │ ┃"),
        "a detour leg runs between two walls:\n{frame}"
    );
}
