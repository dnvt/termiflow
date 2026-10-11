use termiflow::{BaseStyle, RenderOptions};

/// A label on an edge leaving a frame sideways sits clear of the frame wall:
/// the wall's column on the label row stays part of the edge line.
#[test]
fn an_edge_label_does_not_overwrite_a_frame_wall() {
    for (direction, label_side) in [("LR", "→"), ("RL", "←")] {
        for optimized in [false, true] {
            let input = format!(
                "graph {direction}\n    subgraph S [Auth Flow]\n        A[Login] --> B[Check]\n    end\n    B -->|success| C[Out]\n"
            );
            let frame = termiflow::render_with_feedback(
                &input,
                RenderOptions::new()
                    .with_style(BaseStyle::Unicode)
                    .with_optimize_render(optimized),
            )
            .expect("render")
            .output;
            let lines: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
            let row = lines
                .iter()
                .find(|line| line.iter().collect::<String>().contains("success"))
                .unwrap_or_else(|| panic!("label row is missing:\n{frame}"));
            let walls: Vec<usize> = lines
                .iter()
                .find(|line| line.iter().collect::<String>().contains("Auth Flow"))
                .map(|line| {
                    line.iter()
                        .enumerate()
                        .filter(|(_, ch)| matches!(**ch, '│' | '┃'))
                        .map(|(x, _)| x)
                        .collect()
                })
                .unwrap_or_default();
            assert!(!walls.is_empty(), "{frame}");
            for x in walls {
                let ch = row.get(x).copied().unwrap_or(' ');
                assert!(
                    !ch.is_alphanumeric(),
                    "the label covers the frame wall at column {x} \
                     ({direction} {label_side} optimized={optimized}):\n{frame}"
                );
            }
        }
    }
}
