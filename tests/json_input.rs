use termiflow::{parse_json_graph, render_json, RenderOptions};

#[test]
fn json_graph_parses_and_renders() {
    let input = r#"
{
  "direction": "TD",
  "nodes": [
    {"id": "A", "label": "Start"},
    {"id": "B", "label": "End"}
  ],
  "edges": [
    {"from": "A", "to": "B", "label": "go"}
  ]
}
"#;

    let (graph, _cfg) = parse_json_graph(input).expect("parse json graph");
    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(graph.edges.len(), 1);

    let out = render_json(input, RenderOptions::new().with_crop(true)).unwrap();
    assert!(out.contains("Start"));
    assert!(out.contains("End"));
}
