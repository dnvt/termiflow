use termiflow::{render, BaseStyle, RenderOptions};

#[test]
fn emoji_label_graphemes_survive_all_directions_styles_and_modes() {
    for direction in ["TD", "TB", "BT", "LR", "RL"] {
        let input = format!("graph {direction}\nA[Start 🚀] --> B[Process ⚙️]\nB --> C[Done ✅]\n");

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimize_render in [false, true] {
                let output = render(
                    &input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimize_render),
                )
                .unwrap_or_else(|err| {
                    panic!(
                        "emoji fixture failed for direction={direction} style={style:?} optimize={optimize_render}: {err}"
                    )
                });

                assert!(
                    output.contains("Process ⚙️"),
                    "base glyph and variation selector must remain one visible label for direction={direction} style={style:?} optimize={optimize_render}:\n{output}"
                );
                assert!(
                    !output.contains("Process ️"),
                    "variation selector must not survive without its base glyph for direction={direction} style={style:?} optimize={optimize_render}:\n{output}"
                );
            }
        }
    }
}
