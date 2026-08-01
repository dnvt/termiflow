//! Immutable, typed route input for render projection.
//!
//! `Graph::edge_routes` remains a permissive public compatibility field. This
//! module validates its geometry once at the layout/render boundary so the
//! projection path can fail closed when it receives unsupported segments.

use std::collections::HashMap;

use crate::geom::{EdgeRoute, Point};
use crate::graph::Graph;
use crate::indexed_graph::EdgeId;

/// One compatibility route rejected from render projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RouteRejection {
    pub(crate) edge: EdgeId,
    pub(crate) segment: usize,
    pub(crate) from: Point,
    pub(crate) to: Point,
}

/// Immutable axis-aligned route plan copied from a compatibility graph.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct RoutePlan {
    routes: HashMap<EdgeId, EdgeRoute>,
    rejections: Vec<RouteRejection>,
}

impl RoutePlan {
    /// Copy valid routes and record unsupported geometry without mutating the graph.
    pub(crate) fn from_graph(graph: &Graph) -> Self {
        let mut entries: Vec<(usize, &EdgeRoute)> = graph
            .edge_routes
            .iter()
            .map(|(index, route)| (*index, route))
            .collect();
        entries.sort_unstable_by_key(|(index, _)| *index);

        let mut plan = Self::default();
        for (index, route) in entries {
            let edge = EdgeId::from_index(index);
            if let Some((segment, invalid)) = route
                .segments
                .iter()
                .enumerate()
                .find(|(_, segment)| !is_axis_aligned(segment.from, segment.to))
            {
                plan.rejections.push(RouteRejection {
                    edge,
                    segment,
                    from: invalid.from,
                    to: invalid.to,
                });
                continue;
            }
            plan.routes.insert(edge, route.clone());
        }
        plan
    }

    #[inline]
    pub(crate) fn route(&self, edge: EdgeId) -> Option<&EdgeRoute> {
        self.routes.get(&edge)
    }

    #[inline]
    pub(crate) fn route_ids(&self) -> impl Iterator<Item = EdgeId> + '_ {
        self.routes.keys().copied()
    }

    #[inline]
    pub(crate) fn route_count(&self) -> usize {
        self.routes.len()
    }

    #[inline]
    pub(crate) fn rejections(&self) -> &[RouteRejection] {
        &self.rejections
    }
}

fn is_axis_aligned(from: Point, to: Point) -> bool {
    from.x == to.x || from.y == to.y
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{Point, Segment};
    use crate::graph::{Edge, Node};

    fn graph_with_edge() -> Graph {
        let mut graph = Graph::new();
        graph.add_node(Node::new("A", "A"));
        graph.add_node(Node::new("B", "B"));
        graph.add_edge(Edge::new("A", "B"));
        graph
    }

    #[test]
    fn retains_valid_route_bytes_and_empty_entries() {
        let mut graph = graph_with_edge();
        let mut route = EdgeRoute::new();
        route.push_segment(Point::new(2, 1), Point::new(8, 1));
        graph.edge_routes.insert(0, route.clone());
        graph.edge_routes.insert(3, EdgeRoute::new());

        let plan = RoutePlan::from_graph(&graph);

        assert_eq!(plan.route(EdgeId::from_index(0)), Some(&route));
        assert!(plan
            .route(EdgeId::from_index(3))
            .is_some_and(|r| r.segments.is_empty()));
        assert_eq!(plan.route_count(), 2);
        assert!(plan.rejections().is_empty());
    }

    #[test]
    fn rejects_diagonal_route_without_mutating_graph() {
        let mut graph = graph_with_edge();
        let diagonal = EdgeRoute {
            segments: vec![Segment::new(Point::new(1, 1), Point::new(4, 3))],
        };
        graph.edge_routes.insert(0, diagonal.clone());

        let plan = RoutePlan::from_graph(&graph);

        assert!(plan.route(EdgeId::from_index(0)).is_none());
        assert_eq!(
            plan.rejections(),
            &[RouteRejection {
                edge: EdgeId::from_index(0),
                segment: 0,
                from: Point::new(1, 1),
                to: Point::new(4, 3),
            }]
        );
        assert_eq!(graph.edge_routes.get(&0), Some(&diagonal));
    }
}
