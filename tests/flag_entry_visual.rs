use termiflow::{BaseStyle, RenderOptions};

fn render(input: &str, style: BaseStyle) -> String {
    termiflow::render_with_feedback(input, RenderOptions::new().with_style(style))
        .expect("render")
        .output
}

/// The character just left of the flag's left point on its middle row.
fn before_flag_point(frame: &str) -> Option<char> {
    let row = frame.lines().find(|line| line.contains("<  Flag"))?;
    let point = row.find("<  Flag")?;
    row[..point].chars().last()
}

/// An LR arrow into a flag (`>text]`) sits against its left point, plain
/// or labelled, like every other contour: a blank cell there reads as a
/// detached arrow.
#[test]
fn an_lr_arrow_touches_the_flag_point() {
    for input in ["graph LR\nA-->F>Flag]\n", "graph LR\nA-->|no| F>Flag]\n"] {
        for (style, head) in [(BaseStyle::Unicode, '→'), (BaseStyle::Ascii, '>')] {
            let frame = render(input, style);
            assert_eq!(
                before_flag_point(&frame),
                Some(head),
                "arrow detached from the flag ({style:?}):\n{frame}"
            );
        }
    }
}
