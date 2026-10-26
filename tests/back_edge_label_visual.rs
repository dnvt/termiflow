use termiflow::{BaseStyle, RenderOptions};

/// A back edge's label sits on its own cycle route, padded by the route's
/// line on both sides, instead of being clipped by a box or floating apart.
#[test]
fn a_back_edge_label_sits_on_its_cycle_route() {
    for direction in ["TD", "BT", "LR", "RL"] {
        for (style, line) in [(BaseStyle::Unicode, '─'), (BaseStyle::Ascii, '-')] {
            for optimized in [false, true] {
                let input = format!("graph {direction}\nA --> B\nB -->|response| A\n");
                let frame = termiflow::render_with_feedback(
                    &input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("render")
                .output;
                let padded = format!("{line} response {line}");
                assert!(
                    frame.contains(&padded),
                    "the label sits inline on the cycle route \
                     ({direction} {style:?} optimized={optimized}):\n{frame}"
                );
            }
        }
    }
}
