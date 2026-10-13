use termiflow::{BaseStyle, RenderOptions};

/// In TD/BT a top-level node placed in a frame's rows sits clear of the
/// frame's side walls instead of inside or flush against them.
#[test]
fn a_top_level_node_clears_a_frame_it_does_not_belong_to() {
    for direction in ["TD", "BT"] {
        for optimized in [false, true] {
            let input = format!(
                "graph {direction}\n    x1[x1] --> a1\n    subgraph A [A]\n        a1[a1] --> a2[a2]\n    end\n    subgraph B [B]\n        b1[b1] --> b2[b2]\n    end\n    b2 --> y1[y1]\n"
            );
            let frame = termiflow::render_with_feedback(
                &input,
                RenderOptions::new()
                    .with_style(BaseStyle::Unicode)
                    .with_optimize_render(optimized),
            )
            .expect("render")
            .output;
            for name in ["x1", "y1"] {
                let row = frame
                    .lines()
                    .find(|line| line.contains(&format!("  {name}  ")))
                    .unwrap_or_else(|| panic!("{name} is missing:\n{frame}"));
                let chars: Vec<char> = row.chars().collect();
                let start = row[..row.find(&format!("  {name}  ")).unwrap()]
                    .chars()
                    .count();
                // The box's left side and the cell before it.
                let side = start.saturating_sub(1);
                let before = chars.get(side.saturating_sub(1)).copied().unwrap_or(' ');
                assert!(
                    !matches!(before, '│' | '┃'),
                    "{name} sits against a frame wall ({direction} optimized={optimized}):\n{frame}"
                );
            }
        }
    }
}
