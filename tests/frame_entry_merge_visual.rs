use termiflow::{BaseStyle, RenderOptions};

const INPUT: &str = "graph TD
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

/// Three outside sources merge into one target inside Core Banking. The
/// bus belongs above the frame, and only the trunk crosses its top border.
#[test]
fn a_fan_in_into_a_frame_merges_outside_and_crosses_its_border_once() {
    let frame =
        termiflow::render_with_feedback(INPUT, RenderOptions::new().with_style(BaseStyle::Unicode))
            .expect("render")
            .output;
    let rows: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
    let title_row = rows
        .iter()
        .position(|row| row.iter().collect::<String>().contains("Core Banking"))
        .expect("Core Banking title row");
    let top = &rows[title_row - 1];
    let left = top
        .iter()
        .position(|c| matches!(c, '┌' | '┏'))
        .expect("frame top-left corner");
    let right = top
        .iter()
        .rposition(|c| matches!(c, '┐' | '┓'))
        .expect("frame top-right corner");
    let crossings = top[left..=right]
        .iter()
        .filter(|c| matches!(c, '│' | '┃' | '┼' | '┴' | '┬'))
        .count();
    assert_eq!(
        crossings, 1,
        "only the merged trunk crosses the top border:\n{frame}"
    );
    let title: String = rows[title_row].iter().collect();
    assert!(
        !title.contains('┴') && !title.contains('┬'),
        "the fan-in must not merge on the title row:\n{frame}"
    );
}

fn unicode(input: &str) -> Vec<Vec<char>> {
    termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Unicode))
        .expect("render")
        .output
        .lines()
        .map(|line| line.chars().collect())
        .collect()
}

fn show(rows: &[Vec<char>]) -> String {
    rows.iter()
        .map(|row| row.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// A BT fan-in climbs through the frame's bottom border and then the title
/// row; the blank padding beside the title must carry the line, not a gap.
#[test]
fn a_line_through_a_title_rows_padding_stays_connected() {
    let rows = unicode(
        "graph BT
    a[a] --> n
    b[b] --> n
    c[c] --> n
    subgraph lonely
        n[n]
    end
    n --> d[d]
    n --> e[e]
",
    );
    let title_row = rows
        .iter()
        .position(|row| row.iter().collect::<String>().contains("lonely"))
        .expect("title row");
    let arrow_x = rows[..title_row]
        .iter()
        .rev()
        .find_map(|row| row.iter().position(|c| *c == '↑'))
        .expect("arrow into n");
    let cell = rows[title_row].get(arrow_x).copied().unwrap_or(' ');
    assert!(
        matches!(cell, '│' | '┃'),
        "the trunk into n must cross the title row, found {cell:?}:\n{}",
        show(&rows)
    );
    let title: String = rows[title_row].iter().collect();
    assert!(
        title.contains("lonely "),
        "a blank must stay between the title and the line:\n{}",
        show(&rows)
    );
}

/// A branch whose target is centred under a frame wall enters one column over
/// rather than running down the wall and through its corner.
#[test]
fn a_fan_out_branch_does_not_drop_through_a_frame_corner() {
    let rows = unicode(
        "flowchart TB
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
    Inspect --x Return[Return to Vendor]
",
    );
    let (y, x) = rows
        .iter()
        .enumerate()
        .find_map(|(y, row)| row.iter().position(|c| *c == '✕').map(|x| (y, x)))
        .expect("cross end marker");
    let above = rows[y - 1][x];
    assert!(
        matches!(above, '║' | '│' | '┃'),
        "the branch must cross the border on its own column, not along a wall (found {above:?}):\n{}",
        show(&rows)
    );
}

/// In ASCII a frame corner is a `+`, which joins every direction; an edge
/// passing over it must stay a plain `-`, not become a `+` junction.
#[test]
fn an_ascii_frame_corner_does_not_turn_a_passing_edge_into_a_junction() {
    let frame =
        termiflow::render_with_feedback(INPUT, RenderOptions::new().with_style(BaseStyle::Ascii))
            .expect("render")
            .output;
    let rows: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
    let title_row = rows
        .iter()
        .position(|row| row.iter().collect::<String>().contains("Core Banking"))
        .expect("Core Banking title row");
    let top = &rows[title_row - 1];
    let corner = top.iter().position(|c| *c == '+').expect("frame corner");
    let above = rows[title_row - 2].get(corner).copied().unwrap_or(' ');
    assert_ne!(
        above, '+',
        "an edge passing over the frame corner must not become a junction:\n{frame}"
    );
}

#[test]
fn an_ascii_box_corner_under_an_edge_run_is_not_a_junction() {
    let input = "graph BT\nA[Source] --> B\nsubgraph SG [Group]\n    B[Target]\n    C[Other]\nend\nD[External] --> C\n";
    let frame =
        termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Ascii))
            .expect("render")
            .output;
    let source_top = frame
        .lines()
        .position(|line| line.contains("+-----+----+"))
        .unwrap_or_else(|| panic!("Source top border is missing:\n{frame}"));
    let run = frame
        .lines()
        .nth(source_top - 1)
        .expect("edge run above Source");
    assert_eq!(
        run.trim(),
        "+--------+       +--------+",
        "the corners of Source and External under the runs stay plain:\n{frame}"
    );
}

#[test]
fn an_ascii_edge_corner_above_an_edge_run_is_not_a_junction() {
    let input = "%% termiflow: spacing=compact\ngraph TD\nA --> B\nA --> A\nB --> B\nB --> A\n";
    let frame =
        termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Ascii))
            .expect("render")
            .output;
    let unicode =
        termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Unicode))
            .expect("render")
            .output;
    let row = |text: &str| {
        text.lines()
            .find(|line| line.contains("  B  "))
            .map(str::to_owned)
            .unwrap_or_else(|| panic!("B is missing:\n{text}"))
    };
    let pluses = row(&frame).matches('+').count();
    let joints = row(&unicode)
        .chars()
        .filter(|ch| "┼┬┴├┤┌┐└┘".contains(*ch))
        .count();
    assert_eq!(
        pluses, joints,
        "ASCII joints on B's row match the Unicode ones:\n{frame}\n{unicode}"
    );
}
