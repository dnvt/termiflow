use termiflow::{BaseStyle, RenderOptions};

fn unicode(input: &str) -> String {
    termiflow::render_with_feedback(input, RenderOptions::new().with_style(BaseStyle::Unicode))
        .expect("render")
        .output
}

fn arrowheads(frame: &str) -> usize {
    frame.chars().filter(|c| "→←↑↓".contains(*c)).count()
}

/// A skip-rank edge stacked in line with the boxes between its ends is
/// drawn around them, not hidden under them.
#[test]
fn a_skip_rank_edge_detours_around_the_boxes_in_its_way() {
    for direction in ["TD", "LR"] {
        let frame = unicode(&format!("graph {direction}\nA-->B\nB-->C\nC-->D\nA-->D\n"));
        assert_eq!(arrowheads(&frame), 4, "A->D hidden ({direction}):\n{frame}");
    }
}

/// In LR, a skip-rank edge into a target that other edges also reach is
/// still drawn, with its own head.
#[test]
fn an_lr_skip_rank_edge_into_a_shared_target_stays_visible() {
    let frame = unicode("graph LR\nP-->S\nS-->X\nX-->H\nH-->L\nS-->L\nL-->Z\n");
    assert_eq!(arrowheads(&frame), 6, "S->L hidden:\n{frame}");
}

/// A diamond scene that cannot be planned no longer drops its edges whose
/// straight path is clear: `A` keeps its stem to `B`.
#[test]
fn a_rejected_diamond_keeps_its_clear_edges() {
    let frame = unicode("graph TD\nA --> B\nB --> C\nC --> D\nA --> D\nB --> D\n");
    let under_a = frame.lines().nth(2).unwrap_or_default();
    assert!(under_a.contains('┬'), "A has no edge out:\n{frame}");
}

/// A skip-rank edge whose straight path runs along the border column of the
/// boxes in its way detours around them instead of hiding under the border.
#[test]
fn a_skip_rank_edge_along_a_border_column_detours() {
    for direction in ["TD", "BT"] {
        let frame = unicode(&format!(
            "graph {direction}\nA-->B\nB-->C\nC-->D\nA-->D\nA-->E\n"
        ));
        assert_eq!(arrowheads(&frame), 5, "A->D hidden ({direction}):\n{frame}");
    }
}
