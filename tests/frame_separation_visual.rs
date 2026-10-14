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
