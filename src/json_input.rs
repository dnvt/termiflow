use anyhow::{anyhow, Result};
use serde::Deserialize;

use crate::graph::{Edge, Graph, Node, NodeShape, Subgraph};
use crate::parser::ParseConfig;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonGraph {
    #[serde(default)]
    direction: Option<String>,
    #[serde(default)]
    nodes: Vec<JsonNode>,
    #[serde(default)]
    edges: Vec<JsonEdge>,
    #[serde(default)]
    subgraphs: Vec<JsonSubgraph>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonNode {
    id: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    shape: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonEdge {
    from: String,
    to: String,
    #[serde(default)]
    label: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonSubgraph {
    id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    nodes: Vec<String>,
}

fn parse_direction(raw: Option<String>) -> crate::graph::Direction {
    match raw.as_deref().map(|s| s.trim().to_uppercase()) {
        Some(ref s) if s == "TD" => crate::graph::Direction::TD,
        Some(ref s) if s == "TB" => crate::graph::Direction::TB,
        Some(ref s) if s == "LR" => crate::graph::Direction::LR,
        Some(ref s) if s == "RL" => crate::graph::Direction::RL,
        Some(ref s) if s == "BT" => crate::graph::Direction::BT,
        _ => crate::graph::Direction::TD,
    }
}

fn parse_shape(raw: Option<&str>) -> NodeShape {
    let Some(s) = raw.map(|s| s.trim().to_lowercase()) else {
        return NodeShape::Rectangle;
    };
    match s.as_str() {
        "rectangle" | "rect" => NodeShape::Rectangle,
        "rounded" => NodeShape::Rounded,
        "diamond" => NodeShape::Diamond,
        "circle" => NodeShape::Circle,
        "stadium" => NodeShape::Stadium,
        "hexagon" => NodeShape::Hexagon,
        "database" => NodeShape::Database,
        "subroutine" => NodeShape::Subroutine,
        "asymmetric" => NodeShape::Asymmetric,
        "parallelogram" => NodeShape::Parallelogram,
        "parallelogram_alt" | "parallelogramalt" => NodeShape::ParallelogramAlt,
        "trapezoid" => NodeShape::Trapezoid,
        "trapezoid_alt" | "trapezoidalt" => NodeShape::TrapezoidAlt,
        _ => NodeShape::Rectangle,
    }
}

/// Parse TermiFlow's simple JSON graph schema into a `Graph`.
///
/// Schema (example):
/// ```json
/// {"direction":"TD","nodes":[{"id":"A","label":"Start"}],"edges":[{"from":"A","to":"B"}]}
/// ```
pub fn parse_json_graph(input: &str) -> Result<(Graph, ParseConfig)> {
    let parsed: JsonGraph =
        serde_json::from_str(input).map_err(|e| anyhow!("invalid json graph: {}", e))?;

    let mut graph = Graph::new();
    graph.direction = parse_direction(parsed.direction);

    for n in parsed.nodes {
        let label = n.label.clone().unwrap_or_else(|| n.id.clone());
        let mut node = Node::new(&n.id, &label);
        node.shape = parse_shape(n.shape.as_deref());
        graph.add_node(node);
    }

    // Add nodes referenced by edges/subgraphs that weren't declared explicitly.
    for e in &parsed.edges {
        if graph.get_node(&e.from).is_none() {
            graph.add_node(Node::new(&e.from, &e.from));
        }
        if graph.get_node(&e.to).is_none() {
            graph.add_node(Node::new(&e.to, &e.to));
        }
    }

    for sg in parsed.subgraphs {
        let mut subgraph = Subgraph::new(&sg.id, sg.title.clone());
        for nid in &sg.nodes {
            if graph.get_node(nid).is_none() {
                graph.add_node(Node::new(nid, nid));
            }
            subgraph.add_node(nid);
        }
        graph.add_subgraph(subgraph);
        for nid in sg.nodes {
            graph.associate_node_with_subgraph(&nid, &sg.id);
        }
    }

    for e in parsed.edges {
        let mut edge = Edge::new(&e.from, &e.to);
        edge.label = e.label;
        graph.add_edge(edge);
    }

    Ok((graph, ParseConfig::default()))
}

