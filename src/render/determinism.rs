//! Private, serializable determinism contracts for geometry and route inputs.
//!
//! The public `Graph` remains the compatibility model. This module makes the
//! ordering and fallback decisions around that model executable without
//! introducing a public IR or changing rendering behavior.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::graph::{EdgeKind, Graph};

use super::trace::GeometryTrace;
use crate::route_plan::RoutePlan;

const SCHEMA: &str = "termiflow.render_determinism.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
enum RouteDisposition {
    Accepted,
    Empty,
    Rejected,
    Fallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct DuplicateRecord {
    id: String,
    count: usize,
    policy: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
struct EdgeKey {
    from: String,
    to: String,
    label: Option<String>,
    kind: &'static str,
    back_edge: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct EdgeOrderRecord {
    owner: String,
    key: EdgeKey,
    occurrence: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RouteRecord {
    edge_index: usize,
    disposition: RouteDisposition,
    rejected_segments: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct DeterminismContract {
    schema: &'static str,
    duplicate_nodes: Vec<DuplicateRecord>,
    duplicate_edges: Vec<DuplicateRecord>,
    edge_order: Vec<EdgeOrderRecord>,
    routes: Vec<RouteRecord>,
    tie_breakers: Vec<&'static str>,
    geometry_snapshot_sha256: String,
}

impl DeterminismContract {
    pub(crate) fn from_graph(graph: &Graph, geometry: &GeometryTrace) -> Self {
        let duplicate_nodes = duplicate_records(
            graph.nodes.iter().map(|node| node.id.as_str()),
            "first declaration wins",
        );

        let edge_keys: Vec<_> = graph.edges.iter().map(edge_key).collect();
        let duplicate_edges = duplicate_records(
            edge_keys.iter().map(edge_key_string),
            "occurrences remain indexed by canonical occurrence",
        );

        let mut occurrence_by_key: BTreeMap<EdgeKey, usize> = BTreeMap::new();
        let mut edge_order: Vec<_> = edge_keys
            .into_iter()
            .map(|key| {
                let occurrence = occurrence_by_key.entry(key.clone()).or_default();
                let record = EdgeOrderRecord {
                    owner: format!(
                        "edge:{}->{}:{}:{:?}#{}",
                        key.from, key.to, key.kind, key.label, *occurrence
                    ),
                    key,
                    occurrence: *occurrence,
                };
                *occurrence += 1;
                record
            })
            .collect();
        edge_order.sort_by(|left, right| {
            left.key
                .cmp(&right.key)
                .then_with(|| left.occurrence.cmp(&right.occurrence))
                .then_with(|| left.owner.cmp(&right.owner))
        });

        let route_plan = RoutePlan::from_graph(graph);
        let mut rejected_by_edge: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for rejection in route_plan.rejections() {
            rejected_by_edge
                .entry(rejection.edge.index())
                .or_default()
                .push(rejection.segment);
        }
        for segments in rejected_by_edge.values_mut() {
            segments.sort_unstable();
        }

        let mut route_indices: BTreeSet<usize> = (0..graph.edges.len()).collect();
        route_indices.extend(route_plan.route_ids().map(|edge| edge.index()));
        let routes = route_indices
            .into_iter()
            .map(|edge_index| {
                let rejected_segments = rejected_by_edge.remove(&edge_index).unwrap_or_default();
                let disposition = if !rejected_segments.is_empty() {
                    RouteDisposition::Rejected
                } else if let Some(route) =
                    route_plan.route(crate::indexed_graph::EdgeId::from_index(edge_index))
                {
                    if route.segments.is_empty() {
                        RouteDisposition::Empty
                    } else {
                        RouteDisposition::Accepted
                    }
                } else {
                    RouteDisposition::Fallback
                };
                RouteRecord {
                    edge_index,
                    disposition,
                    rejected_segments,
                }
            })
            .collect();

        Self {
            schema: SCHEMA,
            duplicate_nodes,
            duplicate_edges,
            edge_order,
            routes,
            tie_breakers: vec![
                "canonical edge key: from, to, label, kind, back_edge",
                "duplicate edge occurrence within canonical key",
                "route edge index, then segment index for rejection",
                "geometry trace arrays sorted by stable semantic owner ID",
            ],
            geometry_snapshot_sha256: geometry_snapshot_sha256(geometry),
        }
    }

    pub(crate) fn stable_json(&self) -> String {
        serde_json::to_string(self).expect("determinism contract is serializable")
    }
}

fn duplicate_records<I, S>(ids: I, policy: &'static str) -> Vec<DuplicateRecord>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut counts = BTreeMap::<String, usize>::new();
    for id in ids {
        *counts.entry(id.as_ref().to_string()).or_default() += 1;
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(id, count)| DuplicateRecord { id, count, policy })
        .collect()
}

fn edge_key_string(key: &EdgeKey) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{:?}\u{1f}{}\u{1f}{}",
        key.from, key.to, key.label, key.kind, key.back_edge
    )
}

fn edge_key(edge: &crate::graph::Edge) -> EdgeKey {
    EdgeKey {
        from: edge.from.clone(),
        to: edge.to.clone(),
        label: edge.label.clone(),
        kind: edge_kind_name(edge.kind),
        back_edge: edge.is_back_edge,
    }
}

fn edge_kind_name(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Arrow => "arrow",
        EdgeKind::Open => "open",
        EdgeKind::Thick => "thick",
        EdgeKind::Dotted => "dotted",
        EdgeKind::Bidirectional => "bidirectional",
        EdgeKind::CircleEnd => "circle-end",
        EdgeKind::CrossEnd => "cross-end",
    }
}

fn geometry_snapshot_sha256(geometry: &GeometryTrace) -> String {
    let mut edges: Vec<_> = geometry
        .edges
        .iter()
        .map(|edge| {
            serde_json::json!({
                "from": edge.from,
                "to": edge.to,
                "is_back_edge": edge.is_back_edge,
                "exits": edge.exits,
                "enters": edge.enters,
                "segments": edge.segments,
            })
        })
        .collect();
    edges.sort_by_key(|edge| edge.to_string());
    let canonical = serde_json::json!({
        "direction": geometry.direction,
        "nodes": geometry.nodes,
        "subgraphs": geometry.subgraphs,
        "edges": edges,
    });
    let bytes = serde_json::to_vec(&canonical).expect("geometry trace is serializable");
    let mut digest = Sha256::new();
    digest.update(bytes);
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{EdgeRoute, Point, Segment};
    use crate::graph::{Direction, Edge, Graph, Node};

    fn graph_with_edges(edges: &[(&str, &str, Option<&str>)]) -> Graph {
        let mut graph = Graph::new();
        graph.direction = Direction::LR;
        for id in ["A", "B", "C"] {
            graph.add_node(Node::new(id, id));
        }
        for (from, to, label) in edges {
            let mut edge = Edge::new(*from, *to);
            edge.label = label.map(str::to_string);
            graph.add_edge(edge);
        }
        graph
    }

    fn contract_json(graph: &Graph) -> String {
        let trace = GeometryTrace::from_graph(graph);
        DeterminismContract::from_graph(graph, &trace).stable_json()
    }

    #[test]
    fn canonical_edge_contract_is_invariant_to_equivalent_shuffle() {
        let first = graph_with_edges(&[("A", "B", None), ("A", "B", Some("x")), ("B", "C", None)]);
        let second = graph_with_edges(&[("B", "C", None), ("A", "B", Some("x")), ("A", "B", None)]);

        assert_eq!(contract_json(&first), contract_json(&second));
    }

    #[test]
    fn duplicate_nodes_are_first_wins_and_duplicate_edges_are_indexed_occurrences() {
        let mut graph = graph_with_edges(&[("A", "B", None), ("A", "B", None)]);
        graph.nodes.push(Node::new("A", "first"));
        graph.nodes.push(Node::new("A", "second"));

        let contract = DeterminismContract::from_graph(&graph, &GeometryTrace::from_graph(&graph));

        assert_eq!(
            contract.duplicate_nodes,
            vec![DuplicateRecord {
                id: "A".to_string(),
                count: 3,
                policy: "first declaration wins",
            }]
        );
        assert_eq!(contract.duplicate_edges[0].count, 2);
        assert_eq!(
            contract.duplicate_edges[0].policy,
            "occurrences remain indexed by canonical occurrence"
        );
        assert_eq!(contract.edge_order[0].occurrence, 0);
        assert_eq!(contract.edge_order[1].occurrence, 1);
    }

    #[test]
    fn route_dispositions_distinguish_fallback_empty_and_rejected() {
        let mut graph = graph_with_edges(&[("A", "B", None), ("B", "C", None), ("A", "C", None)]);
        graph.edge_routes.insert(1, EdgeRoute::new());
        graph.edge_routes.insert(
            2,
            EdgeRoute {
                segments: vec![Segment::new(Point::new(0, 0), Point::new(2, 1))],
            },
        );
        let contract = DeterminismContract::from_graph(&graph, &GeometryTrace::from_graph(&graph));

        assert_eq!(contract.routes[0].disposition, RouteDisposition::Fallback);
        assert_eq!(contract.routes[1].disposition, RouteDisposition::Empty);
        assert_eq!(contract.routes[2].disposition, RouteDisposition::Rejected);
        assert_eq!(contract.routes[2].rejected_segments, vec![0]);
    }

    #[test]
    fn geometry_snapshot_digest_is_stable_for_repeated_serialization() {
        let graph = graph_with_edges(&[("A", "B", None), ("B", "C", None)]);
        let geometry = GeometryTrace::from_graph(&graph);
        let first = DeterminismContract::from_graph(&graph, &geometry).stable_json();
        let second = DeterminismContract::from_graph(&graph, &geometry).stable_json();

        assert_eq!(first, second);
        assert!(first.contains("geometry_snapshot_sha256"));
    }
}
