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
