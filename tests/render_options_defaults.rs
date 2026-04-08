#[test]
fn render_options_default_matches_new() {
    let a = termiflow::RenderOptions::default();
    let b = termiflow::RenderOptions::new();

    assert_eq!(a.max_label_width, b.max_label_width);
    assert_eq!(a.max_edge_label_width, b.max_edge_label_width);
    assert_eq!(a.wrap_labels, b.wrap_labels);
    assert_eq!(a.max_label_lines, b.max_label_lines);
    assert_eq!(a.strict, b.strict);
    assert_eq!(a.crop, b.crop);
    assert_eq!(a.pad, b.pad);
    assert_eq!(a.compact, b.compact);
    assert_eq!(a.optimize_render, b.optimize_render);
    assert_eq!(a.render_repair_passes, b.render_repair_passes);
    assert_eq!(a.layout_repair_passes, b.layout_repair_passes);
    assert_eq!(a.debug_critic, b.debug_critic);
    assert!(a.composite_style.is_none());
    assert!(b.composite_style.is_none());
}
