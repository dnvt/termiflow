//! Normalized geometry traces for architecture and oracle work.

use crate::geom::Segment;
use crate::graph::{Direction, Graph, Rectangle};
use crate::portals::{
    bt_title_margin_for_edge, node_rects_from_graph, nudge_portal_x_from_corners,
    title_safe_portal_x, PortalColumnPreference, PortalSlots,
};
use serde::Serialize;
use std::collections::BTreeSet;

use super::canvas::Canvas;
use super::fallback_route::{FallbackRouteRejection, FallbackRouteTrace};
use super::provenance::edge_owner_id;
use super::semantic::CellOwnerKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RectTrace {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NodeTrace {
    pub id: String,
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub rank: usize,
    pub subgraph_chain: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubgraphTrace {
    pub id: String,
    pub title: Option<String>,
    pub parent_id: Option<String>,
    pub child_ids: Vec<String>,
    pub node_ids: Vec<String>,
    pub bounds: RectTrace,
    pub inner_bounds: RectTrace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SegmentAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PointTrace {
    pub x: usize,
    pub y: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SegmentTrace {
    pub from: PointTrace,
    pub to: PointTrace,
    pub axis: SegmentAxis,
    pub length: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EdgeTrace {
    pub owner_id: String,
    pub from: String,
    pub to: String,
    pub is_back_edge: bool,
    pub exits: Vec<String>,
    pub enters: Vec<String>,
    pub segments: Vec<SegmentTrace>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GeometryTrace {
    pub direction: Direction,
    pub nodes: Vec<NodeTrace>,
    pub subgraphs: Vec<SubgraphTrace>,
    pub edges: Vec<EdgeTrace>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortalBoundaryTrace {
    pub edge_id: String,
    pub boundary_id: String,
    pub crossing: String,
    pub side: String,
    pub desired_x: usize,
    pub title_safe_x: usize,
    pub corner_nudged_x: usize,
    pub slot_x: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortalCellTrace {
    pub boundary_id: String,
    pub side: String,
    pub x: usize,
    pub y: usize,
    pub glyph: char,
    pub owner_kind: String,
    pub owner_id: Option<String>,
    pub write_stage: String,
}

/// Diagnostic-only portal and final-cell ownership trace.
///
/// This deliberately sits beside the existing geometry trace. It explains
/// whether a nested BT edge selected multiple portal columns before border
/// restoration, or whether a later projection/cleanup stage produced the
/// visually dense final cells. It does not participate in rendering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct PortalTrace {
    pub direction: Direction,
    pub boundaries: Vec<PortalBoundaryTrace>,
    pub cells: Vec<PortalCellTrace>,
    pub(crate) fallback_routes: Vec<FallbackRouteTrace>,
    pub(crate) fallback_route_rejections: Vec<FallbackRouteRejection>,
    pub(crate) contract_digest: Option<String>,
}

impl PortalTrace {
    #[cfg(test)]
    pub(crate) fn fallback_routes_for_test(&self) -> &[FallbackRouteTrace] {
        &self.fallback_routes
    }

    /// Return the scene-owned target entries for one node in stable edge order.
    ///
    /// QA oracles use this projection to verify that a visual head count is
    /// backed by distinct route decisions rather than by coincidental glyphs.
    pub fn target_entry_coordinates(&self, target_node_id: &str) -> Vec<(String, usize, usize)> {
        let mut entries = self
            .fallback_routes
            .iter()
            .flat_map(|route| route.entry_decisions.iter())
            .filter(|decision| decision.target_node_id == target_node_id)
            .map(|decision| (decision.edge_id.clone(), decision.arrow_x, decision.arrow_y))
            .collect::<Vec<_>>();
        entries.sort();
        entries
    }

    /// Return all fallback rejection reasons in stable owner/strategy order.
    pub fn fallback_rejection_reasons(&self) -> Vec<(String, String, String)> {
        let mut reasons = self
            .fallback_route_rejections
            .iter()
            .map(|rejection| {
                (
                    rejection.owner_id.clone(),
                    rejection.strategy.clone(),
                    rejection.reason.clone(),
                )
            })
            .collect::<Vec<_>>();
        reasons.sort();
        reasons
    }

    pub(crate) fn from_canvas(
        graph: &Graph,
        slots: &std::collections::HashMap<String, PortalSlots>,
        canvas: &Canvas,
        contract_digest: Option<&str>,
    ) -> Self {
        let fallback_routes = canvas.fallback_route_traces();
        let mut trace = Self {
            direction: graph.direction,
            boundaries: Vec::new(),
            cells: Vec::new(),
            fallback_routes: fallback_routes.clone(),
            fallback_route_rejections: canvas.fallback_route_rejections(),
            contract_digest: contract_digest.map(str::to_owned),
        };
        if graph.direction != Direction::BT {
            return trace;
        }

        let node_rects = node_rects_from_graph(graph);
        for (edge_index, edge) in graph.edges.iter().enumerate() {
            let edge_id = edge_owner_id(edge_index, edge);
            let (_, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
            let (exits, _) = graph.edge_boundary_crossings(&edge.from, &edge.to);
            let explicit_entry = fallback_routes
                .iter()
                .flat_map(|route| route.entry_decisions.iter())
                .find(|decision| {
                    decision.edge_id == edge_id
                        && decision.side == "bottom"
                        && enters.contains(&decision.boundary_id.as_str())
                });
            let desired_enter = explicit_entry.map_or_else(
                || {
                    graph
                        .get_node(&edge.to)
                        .map(|node| node_center_x(&node_rects, node))
                        .unwrap_or_default()
                },
                |decision| decision.arrow_x,
            );
            let desired_exit = graph
                .get_node(&edge.from)
                .map(|node| node_center_x(&node_rects, node))
                .unwrap_or_default();

            for boundary_id in &enters {
                let Some(boundary) = graph.get_subgraph(boundary_id) else {
                    continue;
                };
                let Some(title) = boundary.title.as_deref() else {
                    continue;
                };
                let decision_for_boundary = explicit_entry.filter(|decision| {
                    decision.boundary_id == boundary.id
                        && decision.portal_y
                            == boundary.bounds.y + boundary.bounds.height.saturating_sub(1)
                });
                let (title_safe_x, corner_nudged_x) = if let Some(decision) = decision_for_boundary
                {
                    (decision.portal_x, decision.portal_x)
                } else {
                    let title_margin =
                        bt_title_margin_for_edge(graph, &edge.from, &edge.to, boundary_id);
                    let title_safe_x = title_safe_portal_x(
                        boundary.bounds.x,
                        boundary.bounds.width,
                        Some(title),
                        desired_enter,
                        Direction::BT,
                        title_margin,
                        PortalColumnPreference::Directional,
                    );
                    let corner_nudged_x = nudge_portal_x_from_corners(
                        boundary.bounds.x,
                        boundary.bounds.width,
                        Some(title),
                        Direction::BT,
                        title_safe_x,
                    );
                    (title_safe_x, corner_nudged_x)
                };
                trace.boundaries.push(PortalBoundaryTrace {
                    edge_id: edge_id.clone(),
                    boundary_id: boundary.id.clone(),
                    crossing: "enter".to_owned(),
                    side: "bottom".to_owned(),
                    desired_x: desired_enter,
                    title_safe_x,
                    corner_nudged_x,
                    slot_x: slots.get(&boundary.id).and_then(|portal| {
                        portal
                            .bottom
                            .contains(&corner_nudged_x)
                            .then_some(corner_nudged_x)
                    }),
                });
            }

            for boundary_id in exits {
                let Some(boundary) = graph.get_subgraph(boundary_id) else {
                    continue;
                };
                let Some(_title) = boundary.title.as_deref() else {
                    continue;
                };
                // BT exits leave through the source boundary's top edge.  A
                // title-safe entry lane is a target-side concern; applying it
                // here reports a different column than the route actually
                // owns and can point at a nonexistent portal slot.  A
                // scene-owned fallback route has an explicit source boundary
                // claim, so bind the diagnostic trace to that physical claim
                // before falling back to the node-center reconstruction used
                // by ordinary routes.
                let explicit_source_x =
                    scene_source_portal_x(&fallback_routes, graph, &edge_id, &boundary.id);
                let (title_safe_x, corner_nudged_x) = if let Some(source_x) = explicit_source_x {
                    (source_x, source_x)
                } else {
                    let title_safe_x = source_exit_portal_x(&boundary.bounds, desired_exit);
                    (title_safe_x, title_safe_x)
                };
                trace.boundaries.push(PortalBoundaryTrace {
                    edge_id: edge_id.clone(),
                    boundary_id: boundary.id.clone(),
                    crossing: "exit".to_owned(),
                    side: "top".to_owned(),
                    desired_x: desired_exit,
                    title_safe_x,
                    corner_nudged_x,
                    slot_x: slots.get(&boundary.id).and_then(|portal| {
                        portal
                            .top
                            .contains(&corner_nudged_x)
                            .then_some(corner_nudged_x)
                    }),
                });
            }
        }

        trace.boundaries.sort_by(|a, b| {
            (
                a.edge_id.as_str(),
                a.boundary_id.as_str(),
                a.crossing.as_str(),
            )
                .cmp(&(
                    b.edge_id.as_str(),
                    b.boundary_id.as_str(),
                    b.crossing.as_str(),
                ))
        });

        let mut coordinates = BTreeSet::new();
        for boundary in &graph.subgraphs {
            if !boundary.bounds.is_valid() {
                continue;
            }
            let left = boundary.bounds.x.saturating_add(1);
            let right = boundary
                .bounds
                .x
                .saturating_add(boundary.bounds.width.saturating_sub(2));
            let top = boundary.bounds.y;
            let bottom = boundary
                .bounds
                .y
                .saturating_add(boundary.bounds.height.saturating_sub(1));
            for x in left..=right {
                coordinates.insert((boundary.id.clone(), "top".to_owned(), x, top));
                coordinates.insert((boundary.id.clone(), "bottom".to_owned(), x, bottom));
            }
            if boundary.title.is_some() {
                let title_y = crate::graph::subgraph_title_row(
                    boundary.bounds.y,
                    boundary.bounds.height,
                    Direction::BT,
                );
                for x in left..=right {
                    coordinates.insert((boundary.id.clone(), "title".to_owned(), x, title_y));
                }
            }
        }

        trace.cells = coordinates
            .into_iter()
            .filter_map(|(boundary_id, side, x, y)| {
                let meta = canvas.get_meta(x, y)?;
                Some(PortalCellTrace {
                    boundary_id,
                    side,
                    x,
                    y,
                    glyph: canvas.get(x, y),
                    owner_kind: format_owner_kind(meta.owner_kind),
                    owner_id: meta.owner_id.clone(),
                    write_stage: canvas.write_stage_at(x, y).unwrap_or("unknown").to_owned(),
                })
            })
            .collect();

        trace
    }
}

/// Resolve a source-side lane from a scene-owned route plan.
///
/// A fallback scene may carry several source-boundary claims for one
/// subgraph.  The older trace projection treated that as ambiguous and fell
/// back to the node center, which made only the left-most source lane appear
/// unreserved even though the final canvas had a valid scene claim.  Pair the
/// claim with the edge's stable ordinal inside the owning scene so the audit
/// evidence describes the physical route that was actually rendered.
fn scene_source_portal_x(
    fallback_routes: &[FallbackRouteTrace],
    graph: &Graph,
    edge_id: &str,
    boundary_id: &str,
) -> Option<usize> {
    let route = fallback_routes.iter().find(|route| {
        route
            .covered_edge_ids
            .iter()
            .any(|covered| covered == edge_id)
            && route
                .boundary_claims
                .iter()
                .any(|claim| claim.boundary_id == boundary_id && claim.side == "top")
    })?;
    let mut claims = route
        .boundary_claims
        .iter()
        .filter(|claim| claim.boundary_id == boundary_id && claim.side == "top")
        .collect::<Vec<_>>();
    claims.sort_by_key(|claim| (claim.x, claim.y));
    if claims.len() == 1 {
        return claims.first().map(|claim| claim.x);
    }

    let mut scene_edge_ids = graph
        .edges
        .iter()
        .enumerate()
        .filter_map(|(index, edge)| {
            let candidate_id = edge_owner_id(index, edge);
            let (exits, _) = graph.edge_boundary_crossings(&edge.from, &edge.to);
            (route
                .covered_edge_ids
                .iter()
                .any(|covered| covered == &candidate_id)
                && exits.contains(&boundary_id))
            .then_some(candidate_id)
        })
        .collect::<Vec<_>>();
    scene_edge_ids.sort();
    let ordinal = scene_edge_ids
        .iter()
        .position(|candidate| candidate == edge_id)?;
    claims.get(ordinal).map(|claim| claim.x)
}

fn node_center_x(
    node_rects: &std::collections::HashMap<String, crate::geom::Rect>,
    node: &crate::graph::Node,
) -> usize {
    node_rects
        .get(&node.id)
        .map(|rect| rect.x + rect.width / 2)
        .unwrap_or_else(|| node.center_x())
}

fn source_exit_portal_x(bounds: &Rectangle, desired: usize) -> usize {
    let left = bounds.x.saturating_add(1);
    let right = bounds
        .x
        .saturating_add(bounds.width.saturating_sub(2))
        .max(left);
    desired.clamp(left, right)
}

fn format_owner_kind(kind: CellOwnerKind) -> String {
    format!("{kind:?}")
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
