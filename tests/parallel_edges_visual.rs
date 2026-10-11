use termiflow::{BaseStyle, RenderOptions};

fn render(input: &str, style: BaseStyle, optimized: bool) -> String {
    termiflow::render_with_feedback(
        input,
        RenderOptions::new()
            .with_style(style)
            .with_optimize_render(optimized),
    )
    .expect("render")
    .output
}

#[test]
fn parallel_edges_of_one_kind_show_every_label() {
    for direction in ["TD", "BT", "LR", "RL"] {
        for style in [BaseStyle::Unicode, BaseStyle::Ascii] {
            for optimized in [false, true] {
                let input = format!(
                    "graph {direction}\n    A[Left] --> B[Right]\n    A -->|one| B\n    A -->|two| B\n    A -->|one| B\n"
                );
                let frame = render(&input, style, optimized);
                assert!(
                    frame.contains("one, two"),
                    "one line carries each distinct label once \
                     ({direction} {style:?} optimized={optimized}):\n{frame}"
                );
            }
        }
    }
}

#[test]
fn the_returned_graph_keeps_the_original_parallel_edges() {
    let parsed = termiflow::parse(
        "graph LR\n    A -->|x| B\n    A -->|y| B\n    B --> C\n",
        false,
    )
    .expect("parse");
    let mut config = termiflow::Config::builder().build(&parsed.config);
    config.spacing = config.spacing.for_direction(parsed.graph.direction);
    let mut graph = parsed.graph;
    termiflow::measure::measure_graph(&mut graph, &config);

    let (graph, outcome) =
        termiflow::layout_and_render_with_feedback(graph, config).expect("render");
    let edges: Vec<_> = graph
        .edges
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str(), edge.label.as_deref()))
        .collect();
    assert_eq!(
        edges,
        vec![
            ("A", "B", Some("x")),
            ("A", "B", Some("y")),
            ("B", "C", None)
        ]
    );
    assert!(outcome.output.contains("x, y"), "{}", outcome.output);
    let owners: Vec<_> = outcome
        .semantic_frame
        .cells
        .iter()
        .filter_map(|cell| cell.owner_id.as_deref())
        .filter(|id| id.starts_with("edge:"))
        .collect();
    assert!(
        owners.iter().any(|id| id.starts_with("edge:2:B->C")),
        "the frame names B->C by its original index: {owners:?}"
    );
    assert!(owners.iter().all(|id| !id.starts_with("edge:1:")));
}
