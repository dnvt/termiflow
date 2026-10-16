use termiflow::{BaseStyle, RenderOptions};

fn render(input: &str) -> String {
    termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Unicode))
        .expect("render")
        .output
}

/// gen_a_245: an edge leaving a frame on its own route ended in a plain
/// arrowhead whatever its kind; `--x` lost its cross.
#[test]
fn a_cross_end_edge_leaving_a_frame_keeps_its_cross() {
    let input = "flowchart TB
    subgraph Backends
        B3[b3]
    end
    B3 --x Drain[Draining]
";
    let frame = render(input);
    assert!(frame.contains('✕'), "no cross end:\n{frame}");
    assert!(
        !frame.contains('↓'),
        "an arrowhead in place of the cross:\n{frame}"
    );
}

/// gen_c_136: an open link `---` into a frame was drawn with an arrowhead.
#[test]
fn an_open_link_into_a_frame_has_no_arrowhead() {
    let input = "graph RL
    A[A] --- b
    subgraph S [S]
        b[b]
    end
";
    let frame = render(input);
    assert!(
        !frame.contains('←'),
        "an open link drawn as an arrow:\n{frame}"
    );
}
