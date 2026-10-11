use termiflow::{BaseStyle, RenderOptions};

/// A node pushed right to clear a frame's right wall takes the flow after it
/// along, instead of landing on (or beside) its own successor.
#[test]
fn a_node_clearing_a_frame_wall_keeps_the_gap_to_its_successor() {
    let input = "graph LR\nA --> B\nsubgraph g [Group with one member]\n  C\nend\nD --> E\n";
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
            let row = frame
                .lines()
                .find(|line| line.contains("  D  "))
                .unwrap_or_else(|| panic!("D is missing:\n{frame}"));
            assert!(
                row.contains("  E  "),
                "D and E share their row ({style:?} optimized={optimized}):\n{frame}"
            );
            let gap = &row[row.find("  D  ").expect("D") + 5..row.find("  E  ").expect("E")];
            assert!(
                gap.chars().filter(|ch| matches!(ch, '─' | '-')).count() >= 4,
                "D -> E keeps a visible shaft ({style:?} optimized={optimized}):\n{frame}"
            );
        }
    }
}
