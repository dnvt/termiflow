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

/// Whether a frame's left or right border, found from the corner on its title
/// row's border line, is a comb: a bus run just inside it, each of its tees
/// stubbing through the border.
fn border_is_a_comb(frame: &str, title: &str, right: bool) -> bool {
    let rows: Vec<Vec<char>> = frame.lines().map(|row| row.chars().collect()).collect();
    let title_row = rows
        .iter()
        .position(|row| row.iter().collect::<String>().contains(title))
        .expect("title row");
    let top = &rows[title_row - 1];
    let corners: Vec<usize> = top
        .iter()
        .enumerate()
        .filter(|(_, cell)| matches!(cell, '┌' | '┐' | '┏' | '┓'))
        .map(|(column, _)| column)
        .collect();
    let title_column = rows[title_row]
        .iter()
        .collect::<String>()
        .find(title)
        .map(|byte| {
            rows[title_row].iter().collect::<String>()[..byte]
                .chars()
                .count()
        })
        .expect("title column");
    let column = if right {
        corners.into_iter().filter(|&c| c > title_column).min()
    } else {
        corners.into_iter().filter(|&c| c < title_column).max()
    }
    .expect("frame corner");
    let inside = if right { column - 1 } else { column + 1 };
    let heavy = matches!(top.get(column), Some('┏' | '┓'));
    rows[title_row..]
        .iter()
        .take_while(|row| !matches!(row.get(column), Some('└' | '┘' | '┗' | '┛')))
        .any(|row| {
            // A line stubbing through the border into a tee just inside it, or
            // dead-ending on it with nothing inside.
            let stub = matches!(row.get(column), Some('─'))
                && matches!(row.get(inside), Some('├' | '┤' | '┼' | '┬' | '┴' | ' '));
            // A heavy frame border drawn over by a light bus.
            let bus = heavy && matches!(row.get(column), Some('│' | '├' | '┤' | '┼' | '┬' | '┴'));
            stub || bus
        })
}

/// A fan-in from frame members to a node outside merged on the frame's
/// border column; the merge now sits a blank column outside it.
#[test]
fn a_horizontal_fan_in_leaving_a_frame_merges_outside_its_border() {
    let input = "flowchart LR
    Doc[(Documents)] --> Chunker[Chunker]
    subgraph Indexing
        Embedder[Embedding Model]
        BM25[BM25 Index]
        ANN[(ANN Index)]
        Embedder --> ANN
    end
    Chunker --> Embedder
    Chunker --> BM25
    Query[Search Query] --> Hybrid[Hybrid Retriever]
    ANN --> Hybrid
    BM25 --> Hybrid
";
    let frame = render(input, BaseStyle::Unicode);
    assert!(
        !border_is_a_comb(&frame, "Indexing", true),
        "Indexing's right border stays a border:\n{frame}"
    );
}

/// A fan-out from inside a frame to nodes outside put its bus on the frame's
/// border column; the bus now starts a blank column outside it.
#[test]
fn a_horizontal_fan_out_leaving_a_frame_keeps_its_bus_off_the_border() {
    let input = "graph RL
    subgraph O [Outer]
        subgraph I [Inner]
            s[Signal]
        end
    end
    s -.-> t1[t1]
    s -.-> t2[t2]
    s -.-> t3[t3]
";
    let frame = render(input, BaseStyle::Unicode);
    assert!(
        !border_is_a_comb(&frame, "Outer", false),
        "Outer's left border stays a border:\n{frame}"
    );
}

/// Frames nested around content near the canvas edge were clamped there and
/// collapsed onto the outer frame's border: each inner frame's corner sat on
/// the outer frame's side. Every row between the outer frame's corners now
/// keeps its side.
#[test]
fn nested_frames_at_the_canvas_edge_keep_their_padding() {
    let input = "graph TD
subgraph Outer [Outer]
    subgraph Inner [Inner]
        subgraph Deep [Deep]
            A[Node]
        end
    end
end
B[In] --> A
A --> C[Out]
";
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        let frame = render(input, style);
        let rows: Vec<&str> = frame.lines().collect();
        let top = rows
            .iter()
            .position(|row| row.starts_with(['┌', '┏', '+']))
            .expect("outer top");
        let bottom = rows
            .iter()
            .rposition(|row| row.starts_with(['└', '┗', '+']))
            .expect("outer bottom");
        assert!(
            rows[top + 1..bottom]
                .iter()
                .all(|row| row.starts_with(['│', '┃', '|'])),
            "no inner frame collapses onto Outer's side ({style:?}):\n{frame}"
        );
    }
}

/// A TD nested frame fed by its parent's own node was not kept clear of
/// that node: the node sat inside the nested frame's columns, its border
/// running through the box.
#[test]
fn a_parent_node_feeding_a_nested_frame_sits_beside_it() {
    let input = "graph TD
    Ext[External] --> I1
    subgraph Outer [Outer]
        O1[Outer node]
        subgraph Inner [Inner]
            I1[Inner node] --> I2[Inner tail]
        end
        O1 --> I2
    end
";
    let frame = render(input, BaseStyle::Unicode);
    let (_, bottom) = box_borders(&frame, "Outer node");
    assert_eq!(
        tees(&bottom),
        1,
        "only Outer node's own exit marks its border:\n{frame}"
    );
}

/// A TD fan-in from three nesting depths merged on a row inside the frames,
/// the innermost frame's bottom border, which the border then redrew over,
/// cutting two branches off; it now merges below the frames.
#[test]
fn a_td_fan_in_from_nested_depths_merges_below_the_frames() {
    let input = "graph TD
    subgraph L1 [L1]
        a[depth1]
        subgraph L2 [L2]
            b[depth2]
            subgraph L3 [L3]
                c[depth3]
            end
        end
    end
    a --> Z[Collector]
    b --> Z
    c --> Z
";
    let frame = render(input, BaseStyle::Unicode);
    let rows: Vec<&str> = frame.lines().collect();
    let target = rows
        .iter()
        .position(|row| row.contains("Collector"))
        .expect("Collector");
    // The merge bar, right above the arrow into Collector's top border,
    // joins all three branches.
    let bar = rows[target - 3];
    assert!(
        bar.matches(['┴', '┘', '└', '├', '┤', '┬']).count() >= 3,
        "three branches reach the merge bar below the frames:\n{frame}"
    );
}

/// A BT nested frame carries its title over its bottom wall; a parent box at
/// the rank of the nested frame's boxes was laid on that wall, which ran
/// through the box's top border (gen_c_027). It now sits below the wall.
#[test]
fn a_parent_box_under_a_bt_nested_frame_clears_its_bottom_wall() {
    let input = "graph BT
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
";
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        for optimized in [false, true] {
            let frame = termiflow::render_with_feedback(
                input,
                RenderOptions::new()
                    .with_style(style)
                    .with_optimize_render(optimized),
            )
            .expect("render")
            .output;
            let lines: Vec<&str> = frame.lines().collect();
            let row_of = |text: &str| {
                lines
                    .iter()
                    .position(|line| line.contains(text))
                    .expect(text)
            };
            let child_wall = row_of("  Child  ") + 1;
            let parent_top = row_of("Parent node") - 1;
            assert!(
                child_wall < parent_top,
                "Parent node's top border must sit below Child's bottom wall \
                 ({style:?}, optimized={optimized}):\n{frame}"
            );
        }
    }
}

/// Rows open under a frame only for a box under the frame's own columns: a
/// parent box beside a nested frame keeps its place in the frame's rows.
#[test]
fn a_parent_box_beside_a_nested_frame_keeps_its_rows() {
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
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        for optimized in [false, true] {
            let frame = termiflow::render_with_feedback(
                input,
                RenderOptions::new()
                    .with_style(style)
                    .with_optimize_render(optimized),
            )
            .expect("render")
            .output;
            let lines: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
            let row_of = |text: &str| {
                lines
                    .iter()
                    .position(|line| line.iter().collect::<String>().contains(text))
                    .expect(text)
            };
            let title = row_of("  Key  ");
            let text: String = lines[title].iter().collect();
            let wall = text[..text.find("  Key  ").unwrap()].chars().count() - 1;
            let key_bottom = (title + 1..lines.len())
                .find(|&row| matches!(lines[row][wall], '┗' | '+'))
                .expect("Key's bottom wall");
            assert!(
                row_of("Peer") <= key_bottom,
                "Peer sits beside Key, not under it, and keeps its rows \
                 ({style:?}, optimized={optimized}):\n{frame}"
            );
        }
    }
}

/// gen_c_057: i2 -> o1 only leaves the nested frame; o1's frame already
/// holds it. The route still planned an entry into the outer frame past its
/// title, ran along the nested frame's bottom wall to it and came back to
/// o1's arrow, leaving a second path. It now leaves straight down.
#[test]
fn a_line_out_of_a_nested_frame_into_its_parent_leaves_straight() {
    let input = "graph TD
    Src[Src] --> i1
    subgraph Outer [Titled outer]
        o1[o1]
        subgraph inner
            i1[i1] --> i2[i2]
        end
        i2 --> o1
    end
    o1 --> Dst[Dst]
";
    for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
        let frame = render(input, style);
        let lines: Vec<&str> = frame.lines().collect();
        let i2_row = lines
            .iter()
            .position(|line| line.contains("  i2  "))
            .expect("i2");
        let wall = lines[i2_row + 2..]
            .iter()
            .position(|line| line.contains('┗') || line.matches('+').count() >= 2)
            .map(|offset| i2_row + 2 + offset)
            .expect("inner's bottom wall");
        let o1_row = lines
            .iter()
            .position(|line| line.contains("  o1  "))
            .expect("o1");
        for line in &lines[wall + 1..o1_row - 1] {
            assert!(
                !line.contains('─') && !line.contains("--"),
                "no line runs sideways between inner and o1 ({style:?}):\n{frame}"
            );
        }
    }
}
