use termiflow::{BaseStyle, RenderOptions};

fn render(input: &str, style: BaseStyle, optimized: bool) -> String {
    termiflow::render_with_feedback(
        input,
        RenderOptions::new()
            .with_style(style)
            .with_optimize_render(optimized),
    )
    .expect("render")
    .output
}

/// An external target sharing a rank with a subgraph member used to shift
/// the whole rank down; the member moved too, the border followed, and the
/// bounded shift loop added its full budget of blank rows.
#[test]
fn an_external_target_below_a_subgraph_does_not_stretch_the_frame() {
    let input = "graph TD
subgraph sg [State machine]
  I[Idle] --> R[Running]
  R --> P[Paused]
  P --> R
  R --> I
  R --> R
end
I --> X[Exit]
";
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        for optimized in [false, true] {
            let frame = render(input, style, optimized);
            let rows: Vec<&str> = frame.lines().collect();
            assert!(
                rows.len() < 60,
                "{} rows for a 4-node graph ({style:?} optimized={optimized}):\n{frame}",
                rows.len()
            );
            let exit_row = rows
                .iter()
                .position(|row| row.contains("Exit"))
                .expect("Exit row");
            // Exit used to share Running's rank, beside the frame wall.
            let paused_row = rows
                .iter()
                .position(|row| row.contains("Paused"))
                .expect("Paused row");
            assert!(
                exit_row > paused_row,
                "Exit sits outside and below the subgraph ({style:?} optimized={optimized}):\n{frame}"
            );
        }
    }
}

/// A top-level node between two members of a subgraph (one edge out, one
/// edge back in) cannot be moved below the frame: its edges would run back up.
#[test]
fn a_node_feeding_back_into_a_subgraph_stays_above_its_child() {
    let input = "graph TD
    Incident([Data Breach Suspected]) --> Contain[Contain Systems]
    subgraph Response [Response Team]
        Forensics[Forensic Analysis]
        Legal[Legal Counsel]
        Comms[Communications]
    end
    Contain --> Forensics
    Forensics --> Scope{PII exposed?}
    Scope -->|yes| Legal
    Scope -->|no| Remediate[Remediate]
    Legal --> Notify[Notify Regulators]
    Legal --> Comms
    Comms --> Customers[Notify Customers]
    Notify --> Remediate
";
    let frame = render(input, BaseStyle::Unicode, false);
    let row_of = |text: &str| {
        frame
            .lines()
            .position(|line| line.contains(text))
            .unwrap_or_else(|| panic!("{text} row:\n{frame}"))
    };
    assert!(
        row_of("PII exposed?") < row_of("Legal Counsel"),
        "the decision must stay above the node it feeds:\n{frame}"
    );
}

/// A labelled edge leaving a frame keeps its label off the frame's border.
#[test]
fn a_labelled_exit_keeps_its_label_off_the_border() {
    let input = "graph TD
    User[User] -->|login| Auth
    subgraph Svc [Service]
        Auth[Auth] -->|ok| Api[API]
    end
    Api -->|json| Client[Client]
    Auth -- denied --> Err[Error]
";
    let frame = render(input, BaseStyle::Unicode, false);
    let label_row = frame
        .lines()
        .find(|line| line.contains("denied"))
        .expect("denied label");
    assert!(
        !label_row.contains(['└', '┘', '┗', '┛']),
        "the label must sit on its own row, not the border:\n{frame}"
    );
}

/// A branch dropping past another box keeps a column between them.
#[test]
fn a_fan_out_branch_does_not_hug_another_box() {
    let input = "%% termiflow: spacing=compact
graph TD
subgraph sg [State machine]
  I[Idle] --> R[Running]
  R --> P[Paused]
  P --> R
  R --> I
  R --> R
end
I --> X[Exit]
";
    let frame = render(input, BaseStyle::Unicode, false);
    assert!(
        !frame.contains("┐│") && !frame.contains("┘│"),
        "a box corner must not touch a passing line:\n{frame}"
    );
}

/// BT: a top-level source on the same rank as a frame's lowest member is
/// moved below the frame on its own, instead of dragging the frame (and its
/// border) along on every pass.
#[test]
fn bt_source_sharing_a_rank_with_frame_members_stays_compact() {
    let input = "graph BT\n    Ext[Ext] --> t\n    subgraph P [P]\n        pn[pn] --> t\n        subgraph C [C]\n            t[target] --> u[next]\n        end\n    end\n    u --> Ext2[Ext2]\n";
    for optimized in [false, true] {
        let frame = termiflow::render_with_feedback(
            input,
            RenderOptions::new()
                .with_style(BaseStyle::Unicode)
                .with_optimize_render(optimized),
        )
        .expect("render")
        .output;
        assert!(
            frame.lines().count() < 60,
            "BT frame grew to {} rows (optimized={optimized}):\n{frame}",
            frame.lines().count()
        );
    }
}

/// TD: two frames that send edges to each other (their ranks interleave)
/// stay compact instead of growing until the shift pass limit.
#[test]
fn interleaved_frames_stay_compact() {
    let input = "graph TD\n    subgraph Web [Web Tier]\n        Next[Next.js]\n    end\n    subgraph Api [API Tier]\n        Rails[Rails API]\n        Sidekiq[Sidekiq]\n    end\n    subgraph Stores [Data Stores]\n        PG[(Postgres)]\n        Redis[(Redis)]\n        S3[(S3)]\n    end\n    Next --> Rails\n    Rails --> PG\n    Rails --> Redis\n    Redis --> Sidekiq\n    Sidekiq --> PG\n    Sidekiq --> S3\n";
    for optimized in [false, true] {
        let frame = termiflow::render_with_feedback(
            input,
            RenderOptions::new()
                .with_style(BaseStyle::Unicode)
                .with_optimize_render(optimized),
        )
        .expect("render")
        .output;
        assert!(
            frame.lines().count() < 120,
            "interleaved frames grew to {} rows (optimized={optimized})",
            frame.lines().count()
        );
    }
}

/// BT: a source inside a sibling frame whose rank falls between the members
/// of the frame it feeds stays put instead of dragging those members (and
/// the frame) down on every pass.
#[test]
fn bt_sibling_source_between_frame_members_stays_compact() {
    let input = "graph BT\n    subgraph L1 [L1]\n        top[top]\n        subgraph L2 [L2]\n            subgraph L3 [L3]\n                deep[deep]\n            end\n        end\n    end\n    subgraph Sib [Sibling]\n        s[s]\n    end\n    deep --> top\n    deep --> s\n    s --> top\n";
    for optimized in [false, true] {
        let frame = termiflow::render_with_feedback(
            input,
            RenderOptions::new()
                .with_style(BaseStyle::Unicode)
                .with_optimize_render(optimized),
        )
        .expect("render")
        .output;
        assert!(
            frame.lines().count() < 60,
            "BT frame grew to {} rows (optimized={optimized})",
            frame.lines().count()
        );
    }
}

/// The longest run of rows holding nothing but blanks and vertical lines.
fn longest_shaft_run(frame: &str) -> usize {
    let mut longest = 0;
    let mut run = 0;
    for row in frame.lines() {
        if row
            .chars()
            .all(|c| matches!(c, ' ' | '│' | '┃' | '║' | '╎' | '|' | ':'))
        {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    longest
}

/// A top-level node ranked between a frame's members that feeds back into
/// the frame asked the frame's top border to clear it; shifting the frame
/// moved that node too, so every pass added a frame height of blank rows.
#[test]
fn a_source_ranked_inside_a_frame_does_not_stretch_it() {
    let input = "graph TD
    Incident([Data Breach Suspected]) --> Contain[Contain Systems]
    subgraph Response [Response Team]
        Forensics[Forensic Analysis]
        Legal[Legal Counsel]
        Comms[Communications]
    end
    Contain --> Forensics
    Forensics --> Scope{PII exposed?}
    Scope -->|yes| Legal
    Scope -->|no| Remediate[Remediate]
    Legal --> Notify[Notify Regulators]
    Legal --> Comms
    Comms --> Customers[Notify Customers]
    Notify --> Remediate
";
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        let frame = render(input, style, false);
        assert!(
            frame.lines().count() < 80 && longest_shaft_run(&frame) < 12,
            "{} rows, longest shaft {} ({style:?}):\n{frame}",
            frame.lines().count(),
            longest_shaft_run(&frame)
        );
    }
}

/// A TD frame clearance asked for a whole-rank shift that moved the frame
/// along with what had to clear it; the loop repeated it to its budget and
/// stacked sibling frames gained long bare shafts. The shift now stops once
/// it repeats without progress.
#[test]
fn a_repeated_whole_rank_shift_does_not_stretch_sibling_frames() {
    let input = "flowchart TB
    subgraph Upstream
        Mirror[Package Mirror]
    end
    subgraph Build [Hermetic Build]
        Fetch[Fetch Pinned Deps]
        Compile[Compile]
        SBOM[Generate SBOM]
        Fetch --> Compile --> SBOM
    end
    subgraph Attest [Attestation]
        Sign[Sigstore Sign]
        Prov[SLSA Provenance]
    end
    Mirror --> Fetch
    SBOM --> Sign
    Compile --> Prov
    Sign & Prov --> Registry[(OCI Registry)]
";
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        let frame = render(input, style, false);
        assert!(
            longest_shaft_run(&frame) < 20,
            "longest shaft {} rows ({style:?}):\n{frame}",
            longest_shaft_run(&frame)
        );
    }
}

/// A BT shift whose request comes back unchanged moved what had to clear the
/// frame along with it; it is applied once and dropped, so nested BT frames
/// fed from below do not stretch over hundreds of bare rows.
#[test]
fn a_bt_fanning_source_beside_nested_frames_does_not_stretch_them() {
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
        let frame = render(input, style, false);
        assert!(
            frame.lines().count() < 120,
            "{} rows for a 5-node graph ({style:?}):\n{frame}",
            frame.lines().count()
        );
    }
}

/// A frame member with no incoming edge sat on the first rank however far
/// down its target was, so its frame spanned (and wrapped) the frames ranked
/// between; it now sits just before its target and the frames stack in flow
/// order.
#[test]
fn a_frame_source_sits_next_to_its_target_so_frames_stack() {
    let input = "flowchart TB
    subgraph Recv [Receiving]
        Dock[Loading Dock]
        Inspect[Inspection]
        Dock --> Inspect
    end
    subgraph Store [Storage]
        Putaway[Putaway]
        Bins[(Bin Locations)]
        Putaway --> Bins
    end
    subgraph Out [Outbound]
        Wave[Wave Planning]
        PickO[Picking]
        PackO[Packing]
        Wave --> PickO --> PackO
    end
    Inspect --> Putaway
    Bins --> PickO
";
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        let frame = render(input, style, false);
        let row_of = |text: &str| {
            frame
                .lines()
                .position(|row| row.contains(text))
                .unwrap_or_else(|| panic!("{text} ({style:?}):\n{frame}"))
        };
        assert!(
            row_of("Receiving") < row_of("Storage")
                && row_of("Storage") < row_of("Outbound")
                && longest_shaft_run(&frame) < 12,
            "frames stack in flow order without bare shafts ({style:?}):\n{frame}"
        );
    }
}

/// Within one envelope pass, a shift request set the first loop found
/// futile is not applied again by the second.
#[test]
fn nested_frames_do_not_repeat_a_futile_shift_within_a_pass() {
    let input = "graph TD
    E1[e1] --> r1
    E2[e2] --> m1
    subgraph R [Root]
        r1[r1] --> r2[r2]
        r1 --> r3[r3]
        subgraph M [Mid]
            m1[m1] --> m2[m2]
            m1 --> m3[m3]
            subgraph L [Leaf]
                l1[l1] --> l2[l2]
                l1 --> l3[l3]
            end
        end
    end
    r2 --> m2
    m3 --> l1
    E3[e3] --> l1
    l2 --> O1[o1]
    l3 --> O2[o2]
    r3 --> O3[o3]
    m2 --> O4[o4]
    O1 --> O5[o5]
";
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        let frame = render(input, style, false);
        assert!(
            frame.lines().count() < 150,
            "{} rows for 14 nodes ({style:?}):\n{frame}",
            frame.lines().count()
        );
    }
}

/// A cycle through a frame was broken at two members' exits while a third
/// member kept its forward edge to the same target, so those two ranked after
/// the rest of the cycle and the frame stretched over every rank between. The
/// entries leading back into them are now the back edges, and the frame's
/// members stay on one rank.
#[test]
fn a_cycle_through_a_frame_keeps_its_members_together() {
    let input = "graph TD
    subgraph Stores [Retail Stores]
        S1[Store 101]
        S2[Store 102]
        S3[Store 103]
    end
    subgraph HQ [Headquarters]
        Replen[Replenishment Engine]
        Forecast[Demand Forecast]
        Forecast --> Replen
    end
    S1 & S2 & S3 --> POSData[(POS Sales Data)]
    POSData --> Forecast
    Replen --> DC[Distribution Center]
    DC --> S1
    DC --> S2
    DC --> S3
";
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        let frame = render(input, style, false);
        let stores_row = frame
            .lines()
            .find(|row| row.contains("Store 101"))
            .expect("store row");
        assert!(
            stores_row.contains("Store 102")
                && stores_row.contains("Store 103")
                && frame.lines().count() < 60,
            "the stores share one rank and the frame stays short ({style:?}):\n{frame}"
        );
    }
}

/// Nodes outside a nested frame that only its members feed ranked right after
/// their sources, inside the frame's rank span; every shift meant to clear
/// them moved members along, and the frame stretched to 122 rows with the
/// outside nodes drawn inside it. They now rank after the frame's last rank.
#[test]
fn outside_nodes_fed_from_a_nested_frame_rank_below_it() {
    let input = "graph TD
    E1[e1] --> r1
    E2[e2] --> m1
    subgraph R [Root]
        r1[r1] --> r2[r2]
        r1 --> r3[r3]
        subgraph M [Mid]
            m1[m1] --> m2[m2]
            m1 --> m3[m3]
            subgraph L [Leaf]
                l1[l1] --> l2[l2]
                l1 --> l3[l3]
            end
        end
    end
    r2 --> m2
    m3 --> l1
    E3[e3] --> l1
    l2 --> O1[o1]
    l3 --> O2[o2]
    r3 --> O3[o3]
    m2 --> O4[o4]
    O1 --> O5[o5]
";
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        let frame = render(input, style, false);
        let rows: Vec<&str> = frame.lines().collect();
        let outside = rows
            .iter()
            .position(|row| {
                ["o1", "o2", "o3", "o4"]
                    .iter()
                    .all(|label| row.contains(label))
            })
            .expect("o1 to o4 share a row");
        let last_frame_border = rows
            .iter()
            .rposition(|row| row.starts_with(['└', '┗', '+']))
            .expect("Root's bottom border");
        assert!(
            rows.len() < 80 && longest_shaft_run(&frame) < 12 && outside > last_frame_border,
            "{} rows, longest shaft {}, outside nodes below the frames ({style:?}):\n{frame}",
            rows.len(),
            longest_shaft_run(&frame)
        );
    }
}
