use std::fs;

use termiflow::{layout_and_render_with_feedback, measure, Config, GeometryTrace};

fn trace_for_fixture(path: &str) -> GeometryTrace {
    let input = fs::read_to_string(path).expect("read fixture");
    let parse_result = termiflow::parse(&input, false).expect("parse fixture");

    let mut config = Config::builder().build(&parse_result.config);
    config.spacing = config.spacing.for_direction(parse_result.graph.direction);

    let mut graph = parse_result.graph;
    measure::measure_graph(&mut graph, &config);

    let (laid_out, _outcome) =
        layout_and_render_with_feedback(graph, config).expect("layout and render fixture");
    GeometryTrace::from_graph(&laid_out)
}

fn assert_subgraph_complex_direction_trace(path: &str) {
    let trace = trace_for_fixture(path);

    assert_eq!(trace.subgraphs.len(), 2);
    assert!(trace
        .subgraphs
        .iter()
        .all(|subgraph| subgraph.bounds.width > 0 && subgraph.inner_bounds.width > 0));

    let api_to_service = trace
        .edges
        .iter()
        .find(|edge| edge.from == "API" && edge.to == "S1")
        .expect("api-to-service edge");
    assert_eq!(
        api_to_service.enters,
        vec!["SG1".to_string()],
        "expected API -> S1 to enter Service Layer for {path}"
    );

    let service_to_data = trace
        .edges
        .iter()
        .find(|edge| edge.from == "S1" && edge.to == "D1")
        .expect("service-to-data edge");
    assert_eq!(
        service_to_data.enters,
        vec!["SG2".to_string()],
        "expected S1 -> D1 to enter Data Layer for {path}"
    );

    let data_to_response: Vec<_> = trace
        .edges
        .iter()
        .filter(|edge| edge.to == "Response")
        .collect();
    assert_eq!(
        data_to_response.len(),
        2,
        "expected two response edges for {path}"
    );
    for edge in data_to_response {
        assert_eq!(
            edge.exits,
            vec!["SG2".to_string()],
            "expected {} -> Response to exit Data Layer for {path}",
            edge.from
        );
    }
}

#[test]
fn subgraph_complex_direction_matrix_captures_boundary_crossings() {
    for path in [
        "tests/fixtures/inputs/subgraph_complex_td.md",
        "tests/fixtures/inputs/subgraph_complex_bt.md",
        "tests/fixtures/inputs/subgraph_complex_lr.md",
        "tests/fixtures/inputs/subgraph_complex_rl.md",
    ] {
        assert_subgraph_complex_direction_trace(path);
    }
}
