use termiflow::{BaseStyle, RenderOptions};

fn render(input: &str) -> String {
    termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Unicode))
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

fn row(frame: &str, index: usize) -> &str {
    frame.lines().nth(index).unwrap_or_default()
}

/// gen_c_085: a frame holding only a nested frame was "shifted" below its
/// sibling by moving its direct members, of which it has none, so the two
/// stacks were drawn through each other. The whole tree moves now.
#[test]
fn a_frame_holding_only_a_nested_frame_stacks_below_its_sibling() {
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
    assert!(
        row_of(&frame, "Stack B") > row_of(&frame, "  a  "),
        "Stack B is drawn through Stack A:\n{frame}"
    );
}

/// gen_a_213: a BT source box straddling the bottom border of the frame it
/// feeds was not moved clear, since only boxes wholly inside the frame
/// were; Onboard Computer sat on Ground Segment's bottom border.
#[test]
fn a_bt_source_straddling_a_frame_border_moves_below_it() {
    let input = "graph BT
    subgraph Satellite
        Payload[Imaging Payload]
        OBC[Onboard Computer]
        Payload --> OBC
    end
    subgraph Ground [Ground Segment]
        Station[Ground Station]
        MOC[Mission Ops]
        Proc[Image Processing]
        Station --> MOC
        Station --> Proc
    end
    OBC -.->|downlink| Station
    MOC -.->|uplink commands| OBC
    Proc --> Archive[(Image Archive)]
";
    let frame = render(input);
    let border = row(&frame, row_of(&frame, "Ground Segment") + 1);
    assert!(
        !border.contains('┌'),
        "a box sits on Ground Segment's bottom border:\n{frame}"
    );
}

/// gen_c_115: the parent's own node feeding its nested frame from below
/// touched that frame's bottom border; it now keeps a connector row.
#[test]
fn a_bt_parent_node_keeps_a_row_below_its_nested_frame() {
    let input = "graph BT
    X[Trigger] --> p
    subgraph P [Parent]
        p[Dispatch]
        subgraph C [Children]
            c1[c1]
            c2[c2]
            c3[c3]
        end
    end
    p --> c1 & c2 & c3
";
    let frame = render(input);
    // Title row, border row, connector row, box top, then the label.
    assert!(
        row_of(&frame, "Dispatch") - row_of(&frame, "Children") >= 4,
        "Dispatch touches Children's border:\n{frame}"
    );
}

/// gen_a_123: the sources fanning into Home Hub had their box tops on the
/// fan's bus row; they now sit below it.
#[test]
fn bt_fan_sources_sit_below_their_bus() {
    let input = "graph BT
    Thermostat[Smart Thermostat] --> HubDev[Home Hub]
    Lights[Smart Lights] --> HubDev
    Lock[Door Lock] --> HubDev
    subgraph Cloud [Vendor Cloud]
        Auto[Automation Engine]
        History[(Event History)]
        Auto --> History
    end
    HubDev --> Auto
    App[Phone App] --> Auto
";
    let frame = render(input);
    let tops = row(&frame, row_of(&frame, "Smart Lights") - 1);
    assert!(
        !tops.contains("┐─") && !tops.contains("─┌"),
        "the fan bus runs along the source box tops:\n{frame}"
    );
}

/// gen_d_115: a BT fan-in trunk crossing a frame's title row and bottom
/// border without a reserved portal slot had that crossing restored to a
/// plain border, and its title-row cell blanked, cutting the line in two.
#[test]
fn a_bt_fan_in_trunk_stays_whole_through_a_frame_border() {
    let input = "graph BT
subgraph l [Left]
  A
end
subgraph r [Right]
  B
end
A <--> B
A --> B
";
    let frame = render(input);
    let rows: Vec<Vec<char>> = frame.lines().map(|row| row.chars().collect()).collect();
    let (x, arrow) = rows
        .iter()
        .enumerate()
        .find_map(|(y, cells)| cells.iter().position(|&c| c == '↑').map(|x| (x, y)))
        .unwrap_or_else(|| panic!("no arrow:\n{frame}"));
    let a_top = row_of(&frame, "  A  ") - 1;
    for (y, cells) in rows.iter().enumerate().take(a_top).skip(arrow + 1) {
        assert!(
            matches!(cells.get(x), Some('│' | '┃' | '┼' | '┴' | '┬')),
            "the trunk breaks at row {y}:\n{frame}"
        );
    }
}

/// gen_c_147: ASCII draws corners and tees alike as `+`, so a jog right
/// above a BT box (its exit turning toward a frame crossing) read as a line
/// into the box's top border and left a stray `+` beside the real exit.
#[test]
fn an_ascii_jog_above_a_bt_box_leaves_its_top_border_alone() {
    let input = "graph BT
    U[User] -- submit --> f
    subgraph O [App]
        subgraph I [Form]
            f[form]
        end
    end
    f -- saved --> D[(DB)]
    f -- error --> U
";
    let frame =
        termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Ascii))
            .expect("render")
            .output;
    let top = row(&frame, row_of(&frame, "|  User  |") - 1);
    let start = top.find('+').expect("box corner");
    let border: String = top[start..].chars().take(10).collect();
    assert_eq!(border.matches('+').count(), 3, "{frame}");
}

/// gen_c_090: a merge bus running down beside a frame's side wall had the
/// wall opened on every row it passed (a comb of `─┤` stubs); only the rows
/// where a line actually leaves the frame open it now.
#[test]
fn a_line_beside_a_frame_wall_opens_it_only_where_it_crosses() {
    let input = "graph LR
    a[a] --> m1
    b[b] --> m2
    c[c] --> m3
    subgraph Mid [Pass]
        m1[m1]
        m2[m2]
        m3[m3]
    end
    m1 --> x[x]
    m2 --> y[y]
    m3 --> x
";
    let frame = render(input);
    let rows: Vec<Vec<char>> = frame.lines().map(|row| row.chars().collect()).collect();
    let right = rows[0]
        .iter()
        .position(|&c| c == '┓')
        .unwrap_or_else(|| panic!("no frame corner:\n{frame}"));
    let openings = rows
        .iter()
        .filter(|cells| matches!(cells.get(right), Some('─' | '━')))
        .count();
    assert_eq!(openings, 2, "{frame}");
}

/// gen_a_134: a route running along a frame's border through its corner
/// had the corner rewritten to whichever line its neighbours connected,
/// once the wall below stopped being opened; frame corners are kept.
#[test]
fn a_route_along_a_frame_border_keeps_its_corners() {
    let input = "graph RL
    subgraph Marketing
        Campaign[Campaign Brief]
        Creative[Creative Assets]
        Campaign --> Creative
    end
    subgraph Channels
        Social[Social Ads]
        Search[Search Ads]
        Newsletter[Newsletter]
    end
    Creative --> Social
    Creative --> Search
    Creative --> Newsletter
    Social & Search & Newsletter --> UTM[UTM Tracking]
    UTM --> Attribution[(Attribution Model)]
";
    let frame = render(input);
    // Every run of a frame's border starts at a corner (the routes running
    // on along these borders are a separate defect).
    for line in frame.lines() {
        let cells: Vec<char> = line.chars().collect();
        for (x, pair) in cells.windows(2).enumerate() {
            if pair[1] == '━' && pair[0] != '━' {
                assert!(
                    matches!(pair[0], '┏' | '┌' | '┗' | '└'),
                    "a frame border lost its corner at column {x}:\n{frame}"
                );
            }
        }
    }
}

/// ASCII draws every corner and tee as `+`, so a box corner or a bus turning
/// beside a frame's side wall read as a line reaching it and opened the wall
/// (gen_c_090's Pass, gen_c_034's Ext A and Ext B); unicode was already clean.
#[test]
fn an_ascii_corner_beside_a_frame_wall_leaves_it_closed() {
    let input = "graph LR
    a[a] --> m1
    b[b] --> m2
    c[c] --> m3
    subgraph Mid [Pass]
        m1[m1]
        m2[m2]
        m3[m3]
    end
    m1 --> x[x]
    m2 --> y[y]
    m3 --> x
";
    let frame =
        termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Ascii))
            .expect("render")
            .output;
    let rows: Vec<Vec<char>> = frame.lines().map(|row| row.chars().collect()).collect();
    let right = rows[0]
        .iter()
        .rposition(|&c| c == '+')
        .unwrap_or_else(|| panic!("no frame corner:\n{frame}"));
    let openings = rows[1..rows.len() - 1]
        .iter()
        .filter(|cells| cells.get(right) == Some(&'-'))
        .count();
    assert_eq!(openings, 2, "{frame}");
}

/// gen_c_124: a fan bus junction beside an inner frame's wall kept a stray
/// arm into it, which opened the wall to nothing beyond.
#[test]
fn a_bus_junction_beside_a_frame_wall_does_not_open_it() {
    let input = "graph RL
    X[X] --> a
    X --> b
    subgraph A [A]
        subgraph Ai [Ai]
            a[a]
        end
    end
    subgraph B [B]
        subgraph Bi [Bi]
            b[b]
        end
    end
";
    let frame = render(input);
    let row = row(&frame, row_of(&frame, "  X  "));
    assert!(!row.contains("─┼"), "{frame}");
}

/// gen_c_034: the Vault fan-in's merge tee kept an arm out through Parent's
/// wall, with nothing beyond it; the stub is trimmed and the wall closed.
#[test]
fn a_merge_tee_beside_a_frame_wall_keeps_no_stub_through_it() {
    let input = "graph LR
    O1[Ext A] --> K
    O2[Ext B] --> K
    subgraph P [Parent]
        Q[Peer]
        subgraph I [Key]
            K[(Vault)]
        end
        Q --> K
    end
    K --> O3[Ext C]
    K --> Q
";
    let frame = render(input);
    let row = row(&frame, row_of(&frame, "Vault"));
    assert!(!row.contains("─┼") && !row.contains("━┼"), "{frame}");
}

/// gen_c_132: ok?'s split tee kept an arm out through Inner's wall to
/// nothing. Its arms are owned by the fan and by the edge to Done, one
/// source's fan all the same, so the stub is trimmed there too.
#[test]
fn a_split_tee_beside_a_frame_wall_keeps_no_stub_through_it() {
    let input = "graph RL
    In[In] --> q
    subgraph O [Outer]
        r[Retry]
        subgraph I [Inner]
            q{ok?}
        end
    end
    q -->|no| r
    q -->|yes| Done[Done]
    r --> q
";
    let frame = render(input);
    let row = row(&frame, row_of(&frame, "  Done  "));
    assert!(!row.contains("─┬") && !row.contains("━┬"), "{frame}");
}

/// gen_c_040: an RL node fed only from inside a frame overlapped it and was
/// pushed out past the frame's right wall, upstream of its own source, so its
/// arrowhead landed on its far side with nothing feeding it. It now stands
/// left of the frame, where RL flow ends.
#[test]
fn an_rl_node_fed_from_a_frame_stands_left_of_it() {
    let input = "graph RL
    E1[Edge] --> n
    subgraph O [Out]
        subgraph In [Extremely Long Inner Container Heading Text]
            n[n]
        end
    end
    n --> E2[Exit]
";
    let frame = render(input);
    let row: Vec<char> = row(&frame, row_of(&frame, "Exit")).chars().collect();
    let exit = row
        .windows(4)
        .position(|w| w == ['E', 'x', 'i', 't'])
        .expect("Exit");
    // The row above Exit's label row shows the frame walls uncrossed.
    let wall = self::row(&frame, row_of(&frame, "Exit") - 1)
        .chars()
        .position(|c| c == '┃')
        .expect("a wall");
    assert!(exit < wall, "Exit is not left of Out:\n{frame}");
}

/// gen_c_132: with Done left of Outer, Inner's RL drift toward its parent's
/// leading edge ran until the next outside node stopped it, stretching Outer
/// around Retry. The drift now keeps to the parent's free inner space.
#[test]
fn an_rl_nested_frame_drift_does_not_stretch_its_parent() {
    let input = "graph RL
    In[In] --> q
    subgraph O [Outer]
        r[Retry]
        subgraph I [Inner]
            q{ok?}
        end
    end
    q -->|no| r
    q -->|yes| Done[Done]
    r --> q
";
    let frame = render(input);
    let top: Vec<char> = frame
        .lines()
        .find(|line| line.contains('┏'))
        .expect("Outer's top border")
        .chars()
        .collect();
    let left = top.iter().position(|&c| c == '┏').expect("corner");
    let right = top.iter().position(|&c| c == '┓').expect("corner");
    assert!(right - left <= 50, "Outer is stretched:\n{frame}");
    assert!(
        row(&frame, row_of(&frame, "  Done  ")).contains("├←"),
        "Done's arrowhead is not on its near side:\n{frame}"
    );
}

/// gen_a_044: Build fans into the adjacent Test frame, which was then taken
/// as visually nested in Build; the gap rebalancer pushed it out past
/// Build's far wall, upstream of it in RL. Frames now keep RL order.
#[test]
fn rl_frames_a_frame_fans_into_keep_flow_order() {
    let input = "graph RL
    subgraph Build [Build Stage]
        Src[Source] --> Compile[Compile]
        Compile --> Bundle[Bundle Assets]
    end
    subgraph Test [Test Stage]
        E2E[E2E Tests]
        Perf[Perf Tests]
    end
    subgraph Release [Release Stage]
        Sign[Sign Binary]
        Publish[Publish]
    end
    Bundle --> E2E
    Bundle --> Perf
    E2E & Perf --> Sign
    Sign --> Publish
";
    let frame = render(input);
    let column = |title: &str| {
        frame
            .lines()
            .find_map(|line| line.find(title).map(|byte| line[..byte].chars().count()))
            .unwrap_or_else(|| panic!("no {title}:\n{frame}"))
    };
    let (release, test, build) = (
        column("Release Stage"),
        column("Test Stage"),
        column("Build Stage"),
    );
    assert!(
        release < test && test < build,
        "frames out of RL order:\n{frame}"
    );
}

/// gen_c_100: an RL terminal target left of the frames was dropped below
/// every frame-to-frame crossing (a corridor rule written for targets right
/// of or below them), so ETL -> Report became a long L-shaped drop. A target
/// downstream of all crossings now stays on its source's row.
#[test]
fn an_rl_terminal_target_left_of_the_crossings_stays_level() {
    let input = "graph RL
    subgraph A [Customer Facing Application Tier]
        a[UI]
    end
    subgraph B [Internal Reporting And Analytics Tier]
        b[ETL]
    end
    a --> b
    b --> r[Report]
";
    let frame = render(input);
    assert_eq!(
        row_of(&frame, "  Report  "),
        row_of(&frame, "  ETL  "),
        "{frame}"
    );
}

/// gen_a_229: Resolver Cache, fed from inside DNS Resolution, had its centre
/// row on the frame's bottom border, so the fan branch to it ran along the
/// border to the frame's corner. The node now stands two rows lower and the
/// branch drops through the border instead.
#[test]
fn an_rl_branch_leaving_a_frame_keeps_off_its_border() {
    let input = "graph RL
    subgraph DNS [DNS Resolution]
        Stub[Stub Resolver]
        Recursive[Recursive Resolver]
        Root[Root Server]
        TLD[TLD Server]
        Auth[Authoritative Server]
        Stub --> Recursive
        Recursive --> Root
        Recursive --> TLD
        Recursive --> Auth
    end
    App[Application] --> Stub
    Recursive --> Cache[(Resolver Cache)]
";
    let frame = render(input);
    let cache = row(&frame, row_of(&frame, "Resolver Cache"));
    assert!(!cache.contains('━'), "line on the frame border:\n{frame}");
}

/// gen_a_207: Compactor's line to Object Storage ran along Cold Path's bottom
/// border, which its centre row shared. It now runs below the frame.
#[test]
fn an_lr_line_past_a_frame_keeps_off_its_border() {
    let input = "flowchart LR
    subgraph Hot [Hot Path]
        API[Write API]
        Mem[(In-memory Index)]
        API --> Mem
    end
    subgraph Cold [Cold Path]
        Seg[Segment Writer]
        Obj[(Object Storage)]
        Seg --> Obj
    end
    Mem -->|flush every 2h| Seg
    Query[Query Frontend] --> Mem
    Query --> Obj
    Compactor[Compactor] --> Obj
";
    let frame = render(input);
    let compactor = row(&frame, row_of(&frame, "Compactor"));
    assert!(
        !compactor.contains('━'),
        "line on the frame border:\n{frame}"
    );
}

/// gen_a_052: the planned gutter for the LR back edges crossed token
/// endpoint's box, so every back edge fell back to the default geometry and
/// the loops ran on the frames' bottom borders. The gutter now steps below
/// the box and both loops run under the frames.
#[test]
fn an_lr_back_edge_gutter_steps_below_a_box() {
    let input = "flowchart LR
    subgraph Client [Client Side]
        App[Mobile App]
        PKCE[Generate PKCE Verifier]
        App --> PKCE
    end
    subgraph IdP [Identity Provider]
        Authorize[authorize endpoint]
        TokenEP[token endpoint]
        JWKS[(JWKS)]
    end
    PKCE -->|code challenge| Authorize
    Authorize -->|auth code| App
    App -->|code + verifier| TokenEP
    TokenEP -->|id token| App
    App -.->|verify signature| JWKS
";
    let frame = render(input);
    let last_border = frame
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains('┛'))
        .map(|(index, _)| index)
        .last()
        .expect("frame border");
    assert!(
        row_of(&frame, "auth code") > last_border && row_of(&frame, "id token") > last_border,
        "back-edge loops not below the frames:\n{frame}"
    );
}

/// gen_c_124: two same-rank stacks fed by one node overlapped, since the
/// frame to move held only a nested frame and moving its direct members
/// moved nothing; each frame's corners were drawn through the other's.
/// Parallel lanes now stack, nested frames and all.
#[test]
fn parallel_rl_frame_stacks_do_not_overlap() {
    let input = "graph RL
    X[X] --> a
    X --> b
    subgraph A [A]
        subgraph Ai [Ai]
            a[a]
        end
    end
    subgraph B [B]
        subgraph Bi [Bi]
            b[b]
        end
    end
";
    let frame = render(input);
    for corner in ['┏', '┓', '┗', '┛'] {
        assert_eq!(
            frame.matches(corner).count(),
            4,
            "frame corner {corner} lost to an overlap:\n{frame}"
        );
    }
}

/// gen_a_132: three shards fed by one router were laid out as a diagonal
/// cascade, and the lines to the lower shards ran along the upper shards'
/// bottom borders. The shards now stack and each line reaches its own.
#[test]
fn parallel_lr_frames_fed_by_one_node_stack() {
    let input = "flowchart LR
    Router[Query Router]
    subgraph Shard1 [Shard 1]
        P1[(Primary)] --> R1[(Replica)]
    end
    subgraph Shard2 [Shard 2]
        P2[(Primary)] --> R2[(Replica)]
    end
    subgraph Shard3 [Shard 3]
        P3[(Primary)] --> R3[(Replica)]
    end
    Router --> P1
    Router --> P2
    Router --> P3
";
    let frame = render(input);
    for line in frame.lines().filter(|line| line.contains("Primary")) {
        assert!(!line.contains('━'), "line on a frame border:\n{frame}");
    }
}
