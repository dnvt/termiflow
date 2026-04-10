//! Normalized geometry traces for architecture and oracle work.

use crate::geom::Segment;
use crate::graph::{Direction, Graph, Rectangle};

use super::provenance::edge_owner_id;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RectTrace {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeTrace {
    pub id: String,
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub rank: usize,
    pub subgraph_chain: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubgraphTrace {
    pub id: String,
    pub title: Option<String>,
    pub parent_id: Option<String>,
    pub child_ids: Vec<String>,
    pub node_ids: Vec<String>,
    pub bounds: RectTrace,
    pub inner_bounds: RectTrace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointTrace {
    pub x: usize,
    pub y: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentTrace {
    pub from: PointTrace,
    pub to: PointTrace,
    pub axis: SegmentAxis,
    pub length: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeTrace {
    pub owner_id: String,
    pub from: String,
    pub to: String,
    pub is_back_edge: bool,
    pub exits: Vec<String>,
    pub enters: Vec<String>,
    pub segments: Vec<SegmentTrace>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeometryTrace {
    pub direction: Direction,
    pub nodes: Vec<NodeTrace>,
    pub subgraphs: Vec<SubgraphTrace>,
    pub edges: Vec<EdgeTrace>,
}

impl GeometryTrace {
    pub fn from_graph(graph: &Graph) -> Self {
        let mut nodes: Vec<NodeTrace> = graph
            .nodes
            .iter()
            .map(|node| NodeTrace {
                id: node.id.clone(),
                x: node.x,
                y: node.y,
                width: node.width,
                height: node.height,
                rank: node.rank,
                subgraph_chain: graph
                    .node_subgraph_chain(&node.id)
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
            })
            .collect();
        nodes.sort_by(|a, b| a.id.cmp(&b.id));

        let mut subgraphs: Vec<SubgraphTrace> = graph
            .subgraphs
            .iter()
            .map(|subgraph| {
                let mut node_ids: Vec<String> = subgraph.node_ids.iter().cloned().collect();
                node_ids.sort();

                SubgraphTrace {
                    id: subgraph.id.clone(),
                    title: subgraph.title.clone(),
                    parent_id: subgraph.parent_id.clone(),
                    child_ids: subgraph.child_ids.clone(),
                    node_ids,
                    bounds: rect_trace(&subgraph.bounds),
                    inner_bounds: rect_trace(&subgraph.inner_bounds),
                }
            })
            .collect();
        subgraphs.sort_by(|a, b| a.id.cmp(&b.id));

        let mut edges: Vec<EdgeTrace> = graph
            .edges
            .iter()
            .enumerate()
            .map(|(edge_idx, edge)| {
                let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
                let segments = graph
                    .edge_routes
                    .get(&edge_idx)
                    .map(|route| route.segments.iter().map(segment_trace).collect())
                    .unwrap_or_default();

                EdgeTrace {
                    owner_id: edge_owner_id(edge_idx, edge),
                    from: edge.from.clone(),
                    to: edge.to.clone(),
                    is_back_edge: edge.is_back_edge,
                    exits: exits.into_iter().map(str::to_string).collect(),
                    enters: enters.into_iter().map(str::to_string).collect(),
                    segments,
                }
            })
            .collect();
        edges.sort_by(|a, b| a.owner_id.cmp(&b.owner_id));

        Self {
            direction: graph.direction,
            nodes,
            subgraphs,
            edges,
        }
    }

    pub fn edge(&self, owner_id: &str) -> Option<&EdgeTrace> {
        self.edges.iter().find(|edge| edge.owner_id == owner_id)
    }
}

fn rect_trace(rect: &Rectangle) -> RectTrace {
    RectTrace {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

fn segment_trace(segment: &Segment) -> SegmentTrace {
    let axis = if segment.from.x == segment.to.x {
        SegmentAxis::Vertical
    } else {
        SegmentAxis::Horizontal
    };
    let length = if axis == SegmentAxis::Vertical {
        segment.from.y.abs_diff(segment.to.y)
    } else {
        segment.from.x.abs_diff(segment.to.x)
    };

    SegmentTrace {
        from: PointTrace {
            x: segment.from.x,
            y: segment.from.y,
        },
        to: PointTrace {
            x: segment.to.x,
            y: segment.to.y,
        },
        axis,
        length,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Direction, Edge, Graph, Node, Rectangle, Subgraph};

    #[test]
    fn geometry_trace_captures_boundary_crossings_and_segments() {
        let mut graph = Graph::new();
        graph.direction = Direction::LR;

        let mut source = Node::new("S", "Source");
        source.x = 0;
        source.y = 0;
        source.width = 8;

        let mut target = Node::new("T", "Target");
        target.x = 20;
        target.y = 0;
        target.width = 8;

        graph.add_node(source);
        graph.add_node(target);
        graph.associate_node_with_subgraph("T", "SG");

        let mut subgraph = Subgraph::new("SG", Some("Data".to_string()));
        subgraph.bounds = Rectangle::new(16, 0, 16, 6);
        subgraph.inner_bounds = Rectangle::new(17, 1, 14, 4);
        subgraph.add_node("T");
        graph.add_subgraph(subgraph);

        let mut edge = Edge::new("S", "T");
        edge.label = Some("read".to_string());
        graph.add_edge(edge);

        let mut route = crate::geom::EdgeRoute::new();
        route.push_segment(
            crate::geom::Point::new(8, 2),
            crate::geom::Point::new(16, 2),
        );
        route.push_segment(
            crate::geom::Point::new(16, 2),
            crate::geom::Point::new(20, 2),
        );
        graph.edge_routes.insert(0, route);

        let trace = GeometryTrace::from_graph(&graph);
        let edge = trace.edge("edge:0:S->T").expect("edge trace");

        assert_eq!(edge.enters, vec!["SG".to_string()]);
        assert_eq!(edge.exits, Vec::<String>::new());
        assert_eq!(edge.segments.len(), 2);
        assert_eq!(edge.segments[0].axis, SegmentAxis::Horizontal);
        assert_eq!(trace.subgraphs.len(), 1);
    }
}
