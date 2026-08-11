use termiflow::{parse, Graph};

fn assert_same_layout(expected: &Graph, actual: &Graph) {
    assert_eq!(expected.direction, actual.direction);
    assert_eq!(expected.warnings, actual.warnings);
    assert_eq!(expected.node_subgraph, actual.node_subgraph);
    assert_eq!(expected.edge_routes, actual.edge_routes);
    assert_eq!(expected.nodes.len(), actual.nodes.len());
    assert_eq!(expected.edges.len(), actual.edges.len());
    assert_eq!(expected.subgraphs.len(), actual.subgraphs.len());

    for (expected, actual) in expected.nodes.iter().zip(&actual.nodes) {
        assert_eq!(expected.id, actual.id);
        assert_eq!(expected.label, actual.label);
        assert_eq!(expected.label_lines, actual.label_lines);
        assert_eq!(expected.shape, actual.shape);
        assert_eq!(expected.click_target, actual.click_target);
        assert_eq!((expected.x, expected.y), (actual.x, actual.y));
        assert_eq!(
            (expected.width, expected.height, expected.rank),
            (actual.width, actual.height, actual.rank)
        );
    }

    for (expected, actual) in expected.edges.iter().zip(&actual.edges) {
        assert_eq!(expected.from, actual.from);
        assert_eq!(expected.to, actual.to);
        assert_eq!(expected.label, actual.label);
        assert_eq!(expected.is_back_edge, actual.is_back_edge);
        assert_eq!(expected.kind, actual.kind);
    }

    for (expected, actual) in expected.subgraphs.iter().zip(&actual.subgraphs) {
        assert_eq!(expected.id, actual.id);
        assert_eq!(expected.title, actual.title);
        assert_eq!(expected.parent_id, actual.parent_id);
        assert_eq!(expected.child_ids, actual.child_ids);
        assert_eq!(expected.node_ids, actual.node_ids);
        assert_eq!(expected.bounds, actual.bounds);
        assert_eq!(expected.inner_bounds, actual.inner_bounds);
        assert_eq!(expected.rank_range, actual.rank_range);
    }
}

#[test]
fn legacy_layout_aliases_match_preferred_coarse_entry_points() {
    for direction in ["TD", "LR", "BT", "RL"] {
        let input = format!(
            "graph {direction}\nsubgraph X [X]\n    A[A]\nend\nB[In] --> A\nA --> C[Out]\n"
        );

        let parsed = parse(&input, false).expect("parse compatibility fixture");
        let preferred = termiflow::coarse_waterfall(parsed.graph.clone()).expect("coarse layout");
        let waterfall =
            termiflow::layout::waterfall(parsed.graph.clone()).expect("waterfall alias");
        assert_same_layout(&preferred, &waterfall);

        let config = termiflow::layout::CoarseLayoutConfig::default();
        let preferred_with_config =
            termiflow::layout::apply_coarse_layout(parsed.graph.clone(), None, config.clone())
                .expect("coarse layout with config");
        let spike =
            termiflow::layout::apply_spike_layout(parsed.graph, None, config).expect("spike alias");
        assert_same_layout(&preferred_with_config, &spike);
    }
}
