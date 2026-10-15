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
