use termiflow::{BaseStyle, FindingCode, RenderOptions};

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

const CONTROL_AND_WORKERS: &str = "graph BT
    subgraph Ctrl [Control]
        API[apiserver] --> Etcd[etcd]
        CM[controller-manager] --> API
    end
    subgraph W [Workers]
        K1[kubelet one]
    end
    K1 --> API
";

/// A BT frame feeding another frame whose lowest node shares its rank was
/// stacked above it (the tie went to id order), then wrapped inside it; it
/// now sits below the frame it feeds, a few rows under its border.
#[test]
fn a_bt_frame_feeding_a_frame_of_equal_rank_stacks_below_it() {
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        let frame = render(CONTROL_AND_WORKERS, style);
        let control_title = row_of(&frame, "Control ");
        let kubelet = row_of(&frame, "kubelet one");
        assert!(
            kubelet > control_title,
            "Workers is drawn inside Control:\n{frame}"
        );
        let workers_top = (control_title + 2..kubelet)
            .find(|&row| {
                frame
                    .lines()
                    .nth(row)
                    .is_some_and(|line| line.contains(['┏', '┌']) || line.contains("+-"))
            })
            .unwrap_or_else(|| panic!("no Workers border:\n{frame}"));
        assert!(
            workers_top - (control_title + 1) <= 4,
            "Workers sits far below Control:\n{frame}"
        );
    }
}

/// gen_a_098: a source fanning into two frames nested in one parent was
/// drawn inside one of them; the parent's members now rank above it.
#[test]
fn a_bt_source_fanning_into_a_parent_frame_stays_outside_it() {
    let input = "graph BT
    subgraph Org [GitHub Org]
        subgraph TeamA [Team Platform]
            RepoInfra[infra repo]
            RepoCI[ci-templates]
        end
        subgraph TeamB [Team Product]
            RepoApp[app repo]
            RepoDocs[docs repo]
        end
    end
    RepoApp --> RepoCI
    RepoDocs --> RepoCI
    RepoInfra --> RepoCI
    SSO[Okta SSO] --> RepoInfra
    SSO --> RepoApp
";
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        let frame = render(input, style);
        assert!(
            row_of(&frame, "Okta SSO") > row_of(&frame, "GitHub Org"),
            "Okta SSO is drawn inside GitHub Org:\n{frame}"
        );
    }
}

/// Stacked BT frames joined by an edge kept the overshoot of an earlier
/// shift as bare rows between them; the band closes to three rows.
#[test]
fn stacked_bt_frames_keep_three_rows_between_them() {
    let input = "graph BT
subgraph SG1 [Group 1]
    A[Node A]
end
subgraph SG2 [Group 2]
    B[Node B]
end
A --> B
";
    let frame = render(input, BaseStyle::Unicode);
    let upper_bottom = row_of(&frame, "Group 2") + 1;
    let lower_top = (upper_bottom + 1..)
        .find(|&row| {
            frame
                .lines()
                .nth(row)
                .is_some_and(|line| line.contains('┏'))
        })
        .unwrap_or_else(|| panic!("no lower frame:\n{frame}"));
    assert_eq!(lower_top - upper_bottom - 1, 3, "{frame}");
}

const SLO: &str = "flowchart TB
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

/// gen_a_160: the fan-in entering SLO Engine ran under its title, which hid
/// the line on the title row. The title moves aside and the line is whole.
#[test]
fn a_frame_title_moves_off_a_line_through_its_band() {
    let outcome = termiflow::render_with_feedback(SLO, RenderOptions::new()).expect("render");
    let frame = &outcome.output;
    let rows: Vec<Vec<char>> = frame.lines().map(|row| row.chars().collect()).collect();
    let title = row_of(frame, "SLO Engine");
    let arrow = (title..rows.len())
        .find_map(|y| rows[y].iter().position(|&c| c == '↓').map(|x| (x, y)))
        .unwrap_or_else(|| panic!("no entry arrow:\n{frame}"));
    let (x, y) = arrow;
    for (row, cells) in rows.iter().enumerate().take(y).skip(title - 1) {
        assert!(
            matches!(cells.get(x), Some('│' | '┃')),
            "the entry line breaks at row {row}:\n{frame}"
        );
    }
    let corrupted: Vec<_> = outcome
        .critic_report
        .findings
        .iter()
        .filter(|finding| finding.code == FindingCode::SubgraphTitleCorrupted)
        .collect();
    assert!(
        corrupted.is_empty(),
        "a moved title reads as corrupted: {corrupted:?}\n{frame}"
    );
}

/// gen_c_131: a source deep in one frame stack feeding a node nested in
/// another stack moved alone, so its frames overlapped the other stack's
/// border. The source's whole stack now moves below the stack it feeds.
#[test]
fn a_bt_stack_feeding_another_stack_moves_below_it_whole() {
    let input = "graph BT
    subgraph A [A]
        subgraph A2 [A2]
            subgraph A3 [A3]
                x[x]
            end
        end
    end
    subgraph B [B]
        subgraph B2 [B2]
            y[y]
        end
    end
    x --> y
";
    let frame = render(input, BaseStyle::Unicode);
    let b_bottom = row_of(&frame, "┃  B ") + 1;
    let a_top = frame
        .lines()
        .position(|row| row.trim_start().starts_with('┏') && row.contains('│'))
        .unwrap_or_else(|| panic!("no frame crossed by the edge:\n{frame}"));
    assert!(a_top > b_bottom, "A overlaps B:\n{frame}");
}

/// gen_c_107: an outside node feeding a frame from below sat six rows
/// under it, past an overshoot band crossed by labeled and back edges; the
/// band now closes to three rows there too.
#[test]
fn an_outside_node_below_a_bt_frame_keeps_three_rows_to_it() {
    let input = "graph BT
    C[Client] -->|GET| R
    C -->|POST| R
    R -->|200| C
    subgraph O [Server]
        subgraph I [Router]
            R[Route]
        end
    end
";
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        let frame = render(input, style);
        let border = row_of(&frame, "Server") + 1;
        let client_top = row_of(&frame, "Client") - 1;
        assert_eq!(client_top - border - 1, 3, "{frame}");
    }
}

/// gen_c_011: a BT target above a frame, ranked with the frame's lowest
/// members, sat on the frame's top border: the rank shift meant to open a
/// connector row moved it along, so its arrowhead had no row. It stays put
/// now while the frame moves down.
#[test]
fn a_bt_target_above_a_frame_keeps_a_row_for_its_arrowhead() {
    let input = "graph BT
    subgraph A [Org]
        subgraph B [Team]
            subgraph C [Squad]
                X[Dev] --> Y[Review]
            end
        end
    end
    Y --> Prod[Prod] --> Mon[Monitor]
    X --> Arch[(Archive)]
";
    let frame = render(input, BaseStyle::Unicode);
    let below = frame
        .lines()
        .nth(row_of(&frame, "Archive") + 2)
        .unwrap_or_default();
    assert!(below.contains('↑'), "no arrowhead under Archive:\n{frame}");
}

/// gen_c_095: frames A and C share the bottom rank, and the back edge
/// Hub -> a reaches A, so they were stacked rather than set side by side;
/// C landed above Hub on B's border and c's edge ran up into B. C now stands
/// left of A, where the back edge's run to the right gutter stays clear.
#[test]
fn a_bt_frame_a_back_edge_reaches_stands_right_of_its_rank_peer() {
    let input = "graph BT
    subgraph A [A]
        a[a]
    end
    subgraph B [B]
        b[b]
    end
    subgraph C [C]
        c[c]
    end
    a --> H((Hub))
    H --> b
    c --> H
    H --> a
";
    let frame = render(input, BaseStyle::Unicode);
    assert_eq!(
        row_of(&frame, "  c  "),
        row_of(&frame, "  a  "),
        "c is not beside a:\n{frame}"
    );
    assert!(
        row_of(&frame, "  Hub  ") < row_of(&frame, "  c  "),
        "c is not below Hub:\n{frame}"
    );
}

/// gen_a_228: Offer follows Interview Loop, so in BT it sits above it. The
/// Candidate Pipeline frame was moved down past it while stacking the
/// frames, which left Offer below Interview Loop with a line looping out of
/// the frame's top to reach it.
#[test]
fn an_outside_node_following_a_moved_frame_stays_after_its_source() {
    let frame = render(
        "graph BT
    subgraph Hiring [Hiring Plan]
        Headcount[Headcount Approved]
        JD[Write Job Description]
        Post[Post Job]
        Headcount --> JD --> Post
    end
    subgraph Pipeline [Candidate Pipeline]
        Sourced[Sourced]
        Phone[Phone Screen]
        Loop[Interview Loop]
        Sourced --> Phone --> Loop
    end
    Post --> Sourced
    Referral[Referrals] --> Phone
    Loop --> Offer([Offer])
",
        BaseStyle::Unicode,
    );
    assert!(
        row_of(&frame, "Offer") < row_of(&frame, "Interview Loop"),
        "Offer is not above Interview Loop:\n{frame}"
    );
}

/// gen_a_248: Controller, a box of the enclosing frame, follows Planning and
/// the actuators follow Controller, so in BT both sit above Motion Planner.
/// Stacking the nested frames moved Planning past them: Controller landed on
/// Planning's bottom wall and the actuators' line ran back down to it.
#[test]
fn a_chain_following_a_moved_nested_frame_stays_after_it() {
    let input = "graph BT
    subgraph Vehicle [Vehicle Stack]
        subgraph Perception
            Lidar[Lidar] --> Fusion[Sensor Fusion]
            Cam[Cameras] --> Fusion
        end
        subgraph Planning
            Predict[Prediction] --> Plan[Motion Planner]
        end
        Control[Controller]
        Fusion --> Predict
        Plan --> Control
    end
    Map[(HD Map)] --> Plan
    Control --> Actuators[Steering and Brakes]
";
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        let frame = render(input, style);
        assert!(
            row_of(&frame, "Controller") < row_of(&frame, "Motion Planner"),
            "Controller is not above Motion Planner ({style:?}):\n{frame}"
        );
        assert!(
            row_of(&frame, "Steering and Brakes") < row_of(&frame, "Controller"),
            "the actuators are not above Controller ({style:?}):\n{frame}"
        );
    }
}

/// gen_c_119: Loop follows c around a cycle through three frames, so in BT it
/// sits above c's frame; it was left on b's frame, its line to c running off
/// the top of the drawing.
#[test]
fn a_node_closing_a_cycle_through_stacked_frames_follows_the_last_one() {
    let input = "graph BT
    subgraph A [A]
        a[a]
    end
    subgraph B [B]
        b[b]
    end
    subgraph C [C]
        c[c]
    end
    a --> b --> c --> O((Loop)) --> a
";
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        let frame = render(input, style);
        assert!(
            row_of(&frame, "Loop") < row_of(&frame, "  c  "),
            "Loop is not above c ({style:?}):\n{frame}"
        );
    }
}

/// gen_a_118: Eval follows Policy Optimization, Ship follows Eval and Release
/// follows Ship. Each is fed once, so the whole chain keeps its order when
/// the Training Stages frame moves past it while stacking the frames.
#[test]
fn a_chain_of_nodes_following_a_moved_frame_keeps_its_order() {
    let input = "graph BT
    subgraph Data [Training Data]
        Prefs[(Preference Pairs)]
        SFTData[(SFT Data)]
    end
    subgraph Stages [Training Stages]
        SFT[Supervised Fine-tune]
        RM[Reward Model]
        PPO[Policy Optimization]
        SFT --> PPO
        RM --> PPO
    end
    SFTData --> SFT
    Prefs --> RM
    PPO --> Eval[Eval Suite]
    Eval --> Ship{Ship?}
    Ship --> Release[[Release Model]]
";
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        let frame = render(input, style);
        let rows = [
            "Release Model",
            "Ship?",
            "Eval Suite",
            "Policy Optimization",
        ]
        .map(|label| row_of(&frame, label));
        assert!(
            rows.windows(2).all(|pair| pair[0] < pair[1]),
            "Release, Ship, Eval and Policy Optimization are not stacked in flow \
             order ({style:?}):\n{frame}"
        );
    }
}

/// gen_a_118: each data store feeds the stage above it, whose box sits over
/// the Training Stages title. With no row above the title to turn on, the
/// router drew only the arrowheads; each line now enters past the title,
/// straight up to its arrow.
#[test]
fn lines_between_stacked_frames_cross_both_walls_past_the_title() {
    let input = "graph BT
    subgraph Data [Training Data]
        Prefs[(Preference Pairs)]
        SFTData[(SFT Data)]
    end
    subgraph Stages [Training Stages]
        SFT[Supervised Fine-tune]
        RM[Reward Model]
        PPO[Policy Optimization]
        SFT --> PPO
        RM --> PPO
    end
    SFTData --> SFT
    Prefs --> RM
    PPO --> Eval[Eval Suite]
    Eval --> Ship{Ship?}
    Ship --> Release[[Release Model]]
";
    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        let frame = render(input, style);
        let lines: Vec<&str> = frame.lines().collect();
        let stages_wall = lines[row_of(&frame, "Training Stages") + 1];
        let data_wall = lines[row_of(&frame, "Training Stages") + 1..]
            .iter()
            .find(|line| line.starts_with('┏') || line.starts_with('+'))
            .unwrap_or_else(|| panic!("no Training Data top wall:\n{frame}"));
        for wall in [stages_wall, data_wall] {
            let crossings = wall
                .chars()
                .skip(1)
                .filter(|c| matches!(c, '│' | '|'))
                .count();
            assert_eq!(
                crossings, 2,
                "both lines cross each frame wall ({style:?}):\n{frame}"
            );
        }
    }
}
