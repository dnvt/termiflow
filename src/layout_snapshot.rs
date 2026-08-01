//! Immutable geometry captured at the layout/render boundary.
//!
//! The public [`Graph`](crate::graph::Graph) remains mutable because parsing,
//! layout, and compatibility callers rely on it. A render stage should not
//! repeatedly inspect mutable graph fields while deciding canvas extent or
//! route metadata, so this module provides a compact immutable copy.

use crate::geom::{EdgeRoute, Point, Rect};
use crate::graph::{Direction, Graph};
use crate::indexed_graph::EdgeId;
use crate::route_plan::{RoutePlan, RouteRejection};
use crate::style::BOX_HEIGHT;

/// Positioned node geometry in an immutable layout snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LayoutNodeSnapshot {
    pub(crate) id: String,
    pub(crate) rect: Rect,
    pub(crate) rank: usize,
    pub(crate) subgraph_id: Option<String>,
}

/// Positioned subgraph geometry in an immutable layout snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LayoutSubgraphSnapshot {
    pub(crate) id: String,
    pub(crate) bounds: Rect,
    pub(crate) inner_bounds: Rect,
    pub(crate) parent_id: Option<String>,
}

/// Immutable post-layout geometry and route metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LayoutSnapshot {
    pub(crate) direction: Direction,
    pub(crate) nodes: Vec<LayoutNodeSnapshot>,
    pub(crate) subgraphs: Vec<LayoutSubgraphSnapshot>,
    route_plan: RoutePlan,
    has_cycles: bool,
}

#[allow(dead_code)]
impl LayoutSnapshot {
    /// Copy the render-relevant state from a graph without retaining mutable
    /// references to it.
    pub(crate) fn from_graph(graph: &Graph) -> Self {
        let nodes = graph
            .nodes
            .iter()
            .map(|node| LayoutNodeSnapshot {
                id: node.id.clone(),
                rect: Rect::new(node.x, node.y, node.width, node.height.max(BOX_HEIGHT)),
                rank: node.rank,
                subgraph_id: graph.get_node_subgraph(&node.id).map(ToOwned::to_owned),
            })
            .collect();

        let subgraphs = graph
            .subgraphs
            .iter()
            .map(|subgraph| LayoutSubgraphSnapshot {
                id: subgraph.id.clone(),
                bounds: Rect::new(
                    subgraph.bounds.x,
                    subgraph.bounds.y,
                    subgraph.bounds.width,
                    subgraph.bounds.height,
                ),
                inner_bounds: Rect::new(
                    subgraph.inner_bounds.x,
                    subgraph.inner_bounds.y,
                    subgraph.inner_bounds.width,
                    subgraph.inner_bounds.height,
                ),
                parent_id: subgraph.parent_id.clone(),
            })
            .collect();

        Self {
            direction: graph.direction,
            nodes,
            subgraphs,
            route_plan: RoutePlan::from_graph(graph),
            has_cycles: graph.has_cycles(),
        }
    }

    #[inline]
    pub(crate) fn max_right(&self) -> usize {
        self.nodes
            .iter()
            .map(|node| node.rect.right())
            .chain(
                self.subgraphs
                    .iter()
                    .map(|subgraph| subgraph.bounds.right()),
            )
            .max()
            .unwrap_or(0)
    }

    #[inline]
    pub(crate) fn max_bottom(&self) -> usize {
        self.nodes
            .iter()
            .map(|node| node.rect.bottom())
            .chain(
                self.subgraphs
                    .iter()
                    .map(|subgraph| subgraph.bounds.bottom()),
            )
            .max()
            .unwrap_or(0)
    }

    #[inline]
    pub(crate) fn has_cycles(&self) -> bool {
        self.has_cycles
    }

    #[inline]
    pub(crate) fn route(&self, edge: EdgeId) -> Option<&EdgeRoute> {
        self.route_plan.route(edge)
    }

    /// Return the stable IDs of all copied route entries.
    pub(crate) fn route_ids(&self) -> impl Iterator<Item = EdgeId> + '_ {
        self.route_plan.route_ids()
    }

    #[inline]
    pub(crate) fn node(&self, id: &str) -> Option<&LayoutNodeSnapshot> {
        self.nodes.iter().find(|node| node.id == id)
    }

    #[inline]
    pub(crate) fn edge_route_count(&self) -> usize {
        self.route_plan.route_count()
    }

    #[inline]
    pub(crate) fn rejected_routes(&self) -> &[RouteRejection] {
        self.route_plan.rejections()
    }

    /// Return all route points for a bounded geometry diagnostic.
    pub(crate) fn route_points(&self, edge: EdgeId) -> impl Iterator<Item = Point> + '_ {
        self.route(edge).into_iter().flat_map(|route| {
            route
                .segments
                .iter()
                .flat_map(|segment| [segment.from, segment.to])
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, Node, Rectangle, Subgraph};

    #[test]
    fn snapshot_copies_geometry_and_routes() {
        let mut graph = Graph::new();
        let mut node = Node::new("a", "A");
        node.x = 4;
        node.y = 3;
        node.width = 5;
        node.height = 1;
        graph.add_node(node);
        graph.add_edge(Edge::new("a", "a"));
        graph.add_subgraph(Subgraph::new("sg", Some("Group".to_string())));
        graph
            .get_subgraph_mut("sg")
            .expect("subgraph exists")
            .bounds = Rectangle::new(2, 1, 12, 8);
        let mut route = EdgeRoute::new();
        route.push_segment(Point::new(6, 5), Point::new(6, 7));
        graph.edge_routes.insert(0, route);

        let snapshot = LayoutSnapshot::from_graph(&graph);
        assert_eq!(snapshot.max_right(), 14);
        assert_eq!(snapshot.max_bottom(), 9);
        assert_eq!(snapshot.edge_route_count(), 1);
        assert_eq!(snapshot.route_points(EdgeId::from_index(0)).count(), 2);
        assert_eq!(
            snapshot.route_ids().map(EdgeId::index).collect::<Vec<_>>(),
            vec![0]
        );

        graph.nodes[0].x = 100;
        graph.subgraphs[0].bounds.width = 1;
        graph.edge_routes.clear();

        assert_eq!(snapshot.node("a").map(|node| node.rect.x), Some(4));
        assert_eq!(snapshot.max_right(), 14);
        assert_eq!(snapshot.edge_route_count(), 1);
    }

    #[test]
    fn snapshot_preserves_direction_cycle_state_and_minimum_node_height() {
        let mut graph = Graph::new();
        graph.direction = Direction::RL;
        let mut node = Node::new("a", "A");
        node.height = 1;
        graph.add_node(node);
        let mut edge = Edge::new("a", "a");
        edge.is_back_edge = true;
        graph.add_edge(edge);

        let snapshot = LayoutSnapshot::from_graph(&graph);
        assert_eq!(snapshot.direction, Direction::RL);
        assert!(snapshot.has_cycles());
        assert_eq!(
            snapshot.node("a").map(|node| node.rect.height),
            Some(BOX_HEIGHT)
        );
    }
}
