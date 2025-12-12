#[test]
fn render_options_default_matches_new() {
    let a = termiflow::RenderOptions::default();
    let b = termiflow::RenderOptions::new();

    assert_eq!(a.max_label_width, b.max_label_width);
    assert_eq!(a.wrap_labels, b.wrap_labels);
    assert_eq!(a.max_label_lines, b.max_label_lines);
    assert_eq!(a.strict, b.strict);
}

