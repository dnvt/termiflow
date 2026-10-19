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

/// gen_a_116: Reverse Proxy and Bastion Host fan into App Servers, whose
/// frame title "Internal Network" fills the frame nearly wall to wall, so it
/// has no room to slide off the trunk dropping through it. The trunk now
/// steps past the title on the row above the frame and turns back under it.
#[test]
fn a_fan_in_steps_past_a_title_with_no_room_to_slide() {
    let input = "graph TD
    subgraph Pub [Public Internet]
        Users((Users))
    end
    subgraph DMZ
        Proxy[Reverse Proxy]
        Bastion[Bastion Host]
    end
    subgraph Internal [Internal Network]
        AppSrv[App Servers]
        DBSrv[(DB Servers)]
        AppSrv --> DBSrv
    end
    Users --> Proxy
    Proxy --> AppSrv
    Admin[Admin Laptop] --> Bastion
    Bastion -.-> AppSrv
    Bastion -.-> DBSrv
";
    assert_line_beside_title(&render(input), "Internal Network");
}

/// gen_a_005: the fan-in into Cleanse drops just past "Transform", through
/// the title's padding, where the line shows; the title only shifts a cell
/// left and the trunk stays straight instead of stepping around it.
#[test]
fn a_fan_in_past_a_title_s_text_keeps_straight() {
    let input = "flowchart TB
    subgraph Extract
        Src1[(CRM)]
        Src2[(ERP)]
        Src3[Clickstream]
    end
    subgraph Transform
        Clean[Cleanse]
        Join[Join Sources]
        Agg[Aggregate]
    end
    subgraph Load
        DW[(Warehouse)]
    end
    Src1 & Src2 & Src3 --> Clean
    Clean --> Join --> Agg --> DW
    DW --> BI[BI Dashboards]
";
    let frame = render(input);
    assert!(
        frame.contains("┃ Transform │"),
        "the trunk passes the title one blank after it:\n{frame}"
    );
}

/// gen_c_129: a and b fan into d1 under Target's title, which is padded off
/// a sibling frame's lane and never slides. The trunk steps past it.
#[test]
fn a_fan_in_steps_past_a_padded_title() {
    let input = "graph TD
    subgraph A [A]
        a[a]
    end
    subgraph B [B]
        b[b]
    end
    subgraph C [C]
        c[c]
    end
    subgraph D [Target]
        d1[d1] --> d2[d2]
    end
    a --> d1
    b --> d1
    c --> d2
    d2 --> X[Out]
";
    for (style, frame) in render_styles(input) {
        assert_eq!(
            lines_hidden_under_title(&frame, "Target"),
            Vec::<usize>::new(),
            "no line hides under the title ({style:?}):\n{frame}"
        );
    }
}

/// A fan-in with a source in the target's own frame starts its trunk under
/// the title: it has no title to step past, and Internal Network's top wall
/// keeps just the two lines entering it (gen_a_116).
#[test]
fn a_fan_in_from_inside_the_frame_does_not_step_past_its_title() {
    let input = "graph TD
    subgraph Pub [Public Internet]
        Users((Users))
    end
    subgraph DMZ
        Proxy[Reverse Proxy]
        Bastion[Bastion Host]
    end
    subgraph Internal [Internal Network]
        AppSrv[App Servers]
        DBSrv[(DB Servers)]
        AppSrv --> DBSrv
    end
    Users --> Proxy
    Proxy --> AppSrv
    Admin[Admin Laptop] --> Bastion
    Bastion -.-> AppSrv
    Bastion -.-> DBSrv
";
    let frame = render(input);
    let rows: Vec<&str> = frame.lines().collect();
    let title = rows
        .iter()
        .position(|row| row.contains("Internal Network"))
        .expect("title");
    assert_eq!(
        rows[title - 1].matches('│').count(),
        2,
        "two lines cross Internal Network's top wall:\n{frame}"
    );
}

/// gen_a_246: the fan-in into Posting Engine joins its bus on the row right
/// above Core Banking, so the line under the title started at a junction,
/// not a plain run, and the title never slid off it. It slides now.
#[test]
fn a_title_slides_off_a_fan_in_joining_right_above_its_frame() {
    let input = "graph TD
    subgraph Bank [Core Banking]
        Accounts[(Accounts)]
        Posting[Posting Engine]
        Posting --> Accounts
    end
    subgraph Channels
        ATM[ATM]
        Online[Online Banking]
        Branch[Branch Teller]
    end
    ATM --> Switch[Card Switch]
    Switch --> Posting
    Online --> Posting
    Branch --> Posting
    Posting --> EOD[End of Day Batch]
    EOD --> GL[(General Ledger)]
";
    for (style, frame) in render_styles(input) {
        assert_eq!(
            lines_hidden_under_title(&frame, "Core Banking"),
            Vec::<usize>::new(),
            "no line hides under the title ({style:?}):\n{frame}"
        );
    }
}

/// gen_a_060: Events fans out to HDFS in Batch Layer and Kafka in Speed
/// Layer; HDFS's drop ran under "Batch Layer", which has no room to slide off
/// it in the narrow frame. The drop now leaves the bus right of the title and
/// turns back under it.
#[test]
fn a_fan_out_drop_steps_past_a_title_with_no_room_to_slide() {
    let input = "flowchart TB
    subgraph Batch [Batch Layer]
        HDFS[(HDFS)]
        Spark[Spark Jobs]
        BatchViews[(Batch Views)]
        HDFS --> Spark --> BatchViews
    end
    subgraph Speed [Speed Layer]
        Kafka[[Kafka]]
        Storm[Stream Processor]
        RTViews[(Realtime Views)]
        Kafka --> Storm --> RTViews
    end
    Events[Event Source] --> HDFS
    Events --> Kafka
    BatchViews --> Q[Query Service]
    RTViews --> Q
";
    for (style, frame) in render_styles(input) {
        assert_eq!(
            lines_hidden_under_title(&frame, "Batch Layer"),
            Vec::<usize>::new(),
            "no line hidden under Batch Layer ({style:?}):\n{frame}"
        );
    }
}

/// gen_c_108 (RL): frame titles sit on their top rows there too, but an RL
/// fan-out enters frames from the side; its drops keep their columns.
#[test]
fn an_rl_fan_out_into_frames_keeps_its_drops() {
    let input = "graph RL
    S[Scheduler] --> x
    S --> y
    S --> z
    subgraph R [Region]
        subgraph A [AZ a]
            subgraph Ah [host a]
                x[x]
            end
        end
        subgraph B [AZ b]
            subgraph Bh [host b]
                y[y]
            end
        end
        subgraph C [AZ c]
            subgraph Ch [host c]
                z[z]
            end
        end
    end
";
    let frame = render(input);
    let rows: Vec<&str> = frame.lines().collect();
    let arrows = rows.iter().filter(|row| row.contains('←')).count();
    assert_eq!(arrows, 3, "every drop ends in its arrow:\n{frame}");
    assert!(
        rows.iter().all(|row| !row.contains("────┐     ┃")),
        "no drop jogs under a title:\n{frame}"
    );
}

/// gen_a_020: Flink Job fans out from inside Stream Processing; its bus sits
/// under the title already, so no drop steps around the title (none climbs
/// back up beside Flink Job).
#[test]
fn a_fan_out_from_inside_a_frame_takes_no_title_step() {
    let input = "flowchart TB
    subgraph Ingest [Ingestion]
        Sensors[IoT Sensors] --> MQTT[MQTT Broker]
        MQTT --> Bridge[Kafka Bridge]
    end
    subgraph Stream [Stream Processing]
        Flink[Flink Job]
        Windows[Windowed Aggregates]
        Flink --> Windows
    end
    subgraph Serve [Serving]
        TSDB[(TimescaleDB)]
        Grafana[Grafana]
    end
    Bridge --> Flink
    Windows --> TSDB
    TSDB --> Grafana
    Flink --x Anom{{Anomaly Alert}}
";
    let frame = render(input);
    assert!(
        !frame.contains("││"),
        "no drop runs back up beside another line:\n{frame}"
    );
}

/// gen_a_241: Migration wave fans out across Wave 1: Stateless, whose title
/// has no stretch clear of both its entry lines; the line into Web Apps ran
/// under the "S". The title now shifts a cell so that line passes through
/// the space after "Wave 1:", with a blank still before the title.
#[test]
fn a_title_with_no_clear_stretch_lets_a_line_through_a_word_gap() {
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
        let line = if style == BaseStyle::Ascii {
            '|'
        } else {
            '│'
        };
        assert!(
            frame.contains(&format!("Wave 1:{line}Stateless")),
            "the line passes between the title's words ({style:?}):\n{frame}"
        );
        assert_eq!(
            lines_hidden_under_title(&frame, "Stateless"),
            Vec::<usize>::new(),
            "no line hidden under the title ({style:?}):\n{frame}"
        );
    }
}

/// gen_a_138 (BT): the fan-in from four access points leaves Network Closet
/// through its bottom wall right under the title's "t". The title moves so
/// the trunk passes between its words.
#[test]
fn a_bt_title_lets_an_exit_trunk_through_a_word_gap() {
    let input = "graph BT
    subgraph Floor1 [Floor 1]
        AP1[Access Point 1]
        AP2[Access Point 2]
    end
    subgraph Floor2 [Floor 2]
        AP3[Access Point 3]
        AP4[Access Point 4]
    end
    subgraph Closet [Network Closet]
        PoE[PoE Switch]
        Ctrl[WLAN Controller]
        PoE --> Ctrl
    end
    AP1 & AP2 --> PoE
    AP3 & AP4 --> PoE
    Ctrl --> Uplink((Uplink))
";
    for (style, frame) in render_styles(input) {
        let line = if style == BaseStyle::Ascii {
            '|'
        } else {
            '│'
        };
        assert!(
            frame.contains(&format!("Network{line}Closet")),
            "the trunk passes between the title's words ({style:?}):\n{frame}"
        );
    }
}

/// gen_a_086: a line runs just left of where "Data Stores" could start; the
/// search for a word gap to let another line through must not reach past
/// the title's first cell (it used to index before it and abort the render).
#[test]
fn a_word_gap_search_beside_a_line_left_of_the_title_renders() {
    let input = "graph TD
    subgraph Web [Web Tier]
        Next[Next.js]
    end
    subgraph Api [API Tier]
        Rails[Rails API]
        Sidekiq[Sidekiq]
    end
    subgraph Stores [Data Stores]
        PG[(Postgres)]
        Redis[(Redis)]
        S3[(S3)]
    end
    Next --> Rails
    Rails --> PG
    Rails --> Redis
    Redis --> Sidekiq
    Sidekiq --> PG
    Sidekiq --> S3
";
    for (style, frame) in render_styles(input) {
        assert!(
            frame.contains("Data Stores"),
            "the frame and its title render ({style:?}):\n{frame}"
        );
    }
}

/// gen_d_207 through the library pipeline the audit uses: with a route
/// through a box somewhere, lines are not reconnected after a title moves,
/// so the title must not blank the cell where Q1's fan-in passes between
/// "with" and "lengthy".
#[test]
fn a_title_moved_onto_a_word_gap_keeps_the_line_there_without_reconnection() {
    let input = "%% termiflow: spacing=spacious
%% termiflow: max_label=20
flowchart TD
subgraph o [Outer orchestration boundary for the whole mesh]
  subgraph l [Left producers with lengthy title]
    P1[Producer number one]
    P2[Producer number two]
    P3[Producer number three]
  end
  subgraph r [Right consumers with lengthy title]
    Q1[Consumer number one]
    Q2[Consumer number two]
    Q3[Consumer number three]
  end
end
P1 & P2 & P3 --> Q1 & Q2 & Q3
Q1 --> P1
Q3 --> P2
";
    let mut graph = termiflow::parse(input, false).expect("parse").graph;
    let mut config = termiflow::Config::default();
    config.optimize_render = false;
    config.composite_style = termiflow::CompositeStyle::from_base(BaseStyle::Unicode);
    config.spacing = config.spacing.for_direction(graph.direction);
    termiflow::measure::measure_graph(&mut graph, &config);
    let (_, outcome) =
        termiflow::layout_and_render_with_feedback(graph, config).expect("layout and render");
    let frame = outcome.output;
    assert!(
        frame.contains("with│lengthy"),
        "the fan-in passes between the title's words:\n{frame}"
    );
    assert_eq!(
        lines_hidden_under_title(&frame, "Right consumers"),
        Vec::<usize>::new(),
        "no line lost under the title:\n{frame}"
    );
}

/// The column of `text`'s first character in `row`.
fn column_of(row: &str, text: &str) -> usize {
    let cells: Vec<char> = row.chars().collect();
    let wanted: Vec<char> = text.chars().collect();
    cells
        .windows(wanted.len())
        .position(|window| window == wanted.as_slice())
        .unwrap_or_else(|| panic!("no {text} in {row}"))
}

/// gen_a_151: the fan-in into user.events drops under "Event Bus" straight
/// onto its arrow, right under the title row; with no plain line below the
/// title, it read as no line at all and the title stayed on it. The title
/// now slides clear, and the wall above where it lands stays shut even
/// though its "v" sits under a portal slot.
#[test]
fn a_title_slides_off_a_line_ending_in_its_arrow_just_under_it() {
    let input = "graph TD
    subgraph Producers
        Web[Web App]
        Mobile[Mobile App]
        Partner[Partner API]
    end
    subgraph Bus [Event Bus]
        Topic1[[user.events]]
        Topic2[[order.events]]
    end
    Web --> Topic1
    Mobile --> Topic1
    Web --> Topic2
    Partner --> Topic2
    Topic1 --> Lake[(Data Lake)]
    Topic2 --> Lake
    Topic2 --> Fraud[Fraud Service]
";
    for (style, frame) in render_styles(input) {
        assert_eq!(
            lines_hidden_under_title(&frame, "Event Bus"),
            Vec::<usize>::new(),
            "no line hidden under Event Bus ({style:?}):\n{frame}"
        );
        assert!(
            style != BaseStyle::Ascii || frame.is_ascii(),
            "an ASCII render stays ASCII:\n{frame}"
        );
        let rows: Vec<&str> = frame.lines().collect();
        let title = rows
            .iter()
            .position(|row| row.contains("Event Bus"))
            .expect("title row");
        let start = column_of(rows[title], "Event Bus");
        let wall: Vec<char> = rows[title - 1].chars().collect();
        assert!(
            wall[start..start + "Event Bus".len()]
                .iter()
                .all(|&c| matches!(c, '━' | '-')),
            "the wall above the moved title stays shut ({style:?}):\n{frame}"
        );
    }
}

/// gen_a_026: the "v" of "Continuous Delivery" sat under a portal slot and
/// was read as an arrow, so the wall above it opened and the bus above grew
/// a branch into the title. The wall stays shut there.
#[test]
fn a_v_in_a_title_does_not_open_the_wall_above_it() {
    let input = "graph TD
    subgraph CI [Continuous Integration]
        Checkout[Checkout] --> Deps[Install Deps]
        Deps --> Unit[Unit Tests]
        Deps --> Lint[Lint and Format]
    end
    subgraph CD [Continuous Delivery]
        Stage[Deploy Staging] --> Smoke[Smoke Tests]
        Smoke --> Gate{Manual Gate}
        Gate --> Prod[Deploy Prod]
    end
    Unit & Lint --> Artifact[[Build Artifact]]
    Artifact --> Stage
";
    for (style, frame) in render_styles(input) {
        let rows: Vec<&str> = frame.lines().collect();
        let title = rows
            .iter()
            .position(|row| row.contains("Delivery"))
            .expect("title row");
        let v = column_of(rows[title], "Delivery") + 4;
        let above = rows[title - 1].chars().nth(v);
        assert!(
            matches!(above, Some('━' | '-')),
            "the wall above the title's v stays shut ({style:?}):\n{frame}"
        );
    }
}
