use termiflow::{BaseStyle, RenderOptions};

fn render(input: &str, style: BaseStyle) -> String {
    termiflow::render_with_feedback(input, RenderOptions::new().with_style(style))
        .expect("render")
        .output
}

/// The top and bottom border rows of the box labelled `label`.
fn box_borders(frame: &str, label: &str) -> (String, String) {
    let rows: Vec<Vec<char>> = frame.lines().map(|row| row.chars().collect()).collect();
    for (index, row) in rows.iter().enumerate() {
        for side in ['│', '|'] {
            let needle: Vec<char> = format!("{side}  {label}  {side}").chars().collect();
            let Some(start) = row
                .windows(needle.len())
                .position(|window| window == needle.as_slice())
            else {
                continue;
            };
            let span = start..start + needle.len();
            let border = |at: usize| -> String {
                rows.get(at)
                    .and_then(|row| row.get(span.clone()))
                    .map(|cells| cells.iter().collect())
                    .unwrap_or_default()
            };
            return (border(index - 1), border(index + 1));
        }
    }
    panic!("no box labelled {label}:\n{frame}");
}

/// Junction cells on a box border: where a line leaves or passes through it.
fn tees(border: &str) -> usize {
    let inner: Vec<char> = border.chars().collect();
    inner[1..inner.len() - 1]
        .iter()
        .filter(|&&cell| !matches!(cell, '─' | '-'))
        .count()
}

const NESTED: &str = "graph TD
    E1[e1] --> r1
    E2[e2] --> m1
    subgraph R [Root]
        r1[r1] --> r2[r2]
        r1 --> r3[r3]
        subgraph M [Mid]
            m1[m1] --> m2[m2]
        end
    end
    r2 --> m2
    r3 --> O3[o3]
";

/// An entry into a nested frame ran its shared lane straight down through
/// the boxes the parent places above that frame; it now takes a clear column
/// beside them.
#[test]
fn a_nested_entry_runs_beside_the_boxes_above_the_inner_frame() {
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        let frame = render(NESTED, style);
        let (r1_top, r1_bottom) = box_borders(&frame, "r1");
        let (r3_top, _) = box_borders(&frame, "r3");
        assert!(
            tees(&r1_top) == 0 && tees(&r1_bottom) == 1 && tees(&r3_top) == 0,
            "no line runs through r1 or r3 ({style:?}):\n{frame}"
        );
    }
}

const THREE_LEVELS: &str = "graph TD
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

/// An exit straight down from r3 to its target below the frames ran through
/// the nested frame and m1 placed under it, and e3's fan-in drop ran through
/// m2; both now take a clear column beside the nested frame's boxes.
#[test]
fn routes_past_a_nested_frame_go_around_its_boxes() {
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        let frame = render(THREE_LEVELS, style);
        for label in ["m1", "m2", "m3"] {
            let (top, bottom) = box_borders(&frame, label);
            assert!(
                tees(&top) == 0 && tees(&bottom) == 1,
                "only {label}'s own exit leaves its box ({style:?}):\n{frame}"
            );
        }
    }
}

/// A nested frame placed on the rank after its parent's own nodes drew its
/// top border across them and its title over their edges.
#[test]
fn a_nested_frame_starts_below_its_parents_nodes() {
    let input = "graph TD
    E[Entry] --> p
    subgraph L1 [L1]
        subgraph L2 [L2]
            p[p]
            subgraph L3 [L3]
                q[q]
            end
        end
    end
    p --> q
    q --> p
    q --> X[Exit]
";
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        let frame = render(input, style);
        assert!(
            ['|', '│', '┃']
                .iter()
                .any(|side| frame.contains(&format!("{side}  L3 "))),
            "L3's title sits clear inside its frame ({style:?}):\n{frame}"
        );
    }
}

/// ASCII draws every corner and tee as `+`, so a jog starting right under a
/// box read as a line into its bottom border and left a stray `+` beside the
/// real exit.
#[test]
fn a_jog_under_a_box_leaves_its_ascii_border_alone() {
    let input = "graph TD
    subgraph P [Parent]
        subgraph C [Child]
            cn[Child node]
        end
    end
    E[Entry] --> cn
";
    let frame = render(input, BaseStyle::Ascii);
    let (_, bottom) = box_borders(&frame, "Entry");
    assert_eq!(
        tees(&bottom),
        1,
        "only Entry's exit marks its border:\n{frame}"
    );
}

/// A BT fan-in from sources in sibling frames merged on the row above the
/// sources, inside their frames: the merge bar ran under the frame tops and
/// the portal each source leaves by poked through its border into nothing.
/// It now merges above the frames, each source leaving by its own portal.
#[test]
fn a_bt_fan_in_from_sibling_frames_merges_above_them() {
    let input = "graph BT
    subgraph A [Logs]
        a[app]
    end
    subgraph B [Metrics]
        b[prom]
    end
    subgraph C [Traces]
        c[otel]
    end
    subgraph Sink [Observability]
        s[(Warehouse)]
    end
    a --> s
    b --> s
    c --> s
";
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        let frame = render(input, style);
        let rows: Vec<&str> = frame.lines().collect();
        let label = rows
            .iter()
            .position(|row| row.contains("app"))
            .expect("app");
        let tops = rows[label - 4];
        assert_eq!(
            tops.matches(['│', '|']).count(),
            3,
            "each source crosses its frame's top once ({style:?}):\n{frame}"
        );
    }
}

/// A horizontal fan-out into a frame put its bus on the frame's border
/// column, turning the border into a `├─` comb; the bus now stays outside.
#[test]
fn a_horizontal_fan_out_into_a_frame_keeps_its_bus_off_the_border() {
    let input = "graph LR
Router[Request Router]
subgraph SG1 [Handler Group]
H1[Handler 1]
H2[Handler 2]
H3[Handler 3]
end
Router --> H1
Router --> H2
Router --> H3
";
    let frame = render(input, BaseStyle::Unicode);
    assert!(
        !frame.contains("├─ "),
        "the frame border is not a comb of bus stubs:\n{frame}"
    );
}

/// An LR nested frame drifted toward its parent's leading edge until it
/// was a gap from the outside node feeding it, dragging the parent into that
/// node's own frame; it now stops short of the other frame.
#[test]
fn an_lr_nested_frame_does_not_drag_its_parent_into_a_neighbour() {
    let input = "graph LR
    subgraph A [A]
        subgraph Ai [Ai]
            x[x]
        end
    end
    subgraph B [B]
        y[y] --> z[z]
    end
    x --> y
    z --> x
";
    let frame = render(input, BaseStyle::Unicode);
    assert!(
        !frame.contains("││") && !frame.contains("┃┃"),
        "frames A and B do not overlap:\n{frame}"
    );
}
