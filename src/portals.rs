//! Shared subgraph envelope + portal helpers for layout and render.
//!
//! Provides a single source of truth for:
//! - Subgraph inner/outer rectangles (with gutters)
//! - Portal slots per side derived from crossing edges
//! - Helpers to build node rects from a laid-out graph

use std::collections::{HashMap, HashSet};

use crate::geom::Rect;
use crate::graph::{Direction, Graph};
use crate::style::BOX_HEIGHT;

/// Portal coordinates along each side of a subgraph border.
#[derive(Debug, Clone, Default)]
pub struct PortalSlots {
    pub top: HashSet<usize>,
    pub bottom: HashSet<usize>,
    pub left: HashSet<usize>,
    pub right: HashSet<usize>,
}

/// Combined inner/outer bounds with portals.
#[derive(Debug, Clone)]
pub struct SubgraphEnvelope {
    pub outer: Rect,
    pub inner: Rect,
    pub portals: PortalSlots,
}

/// Build node rectangles (using BOX_HEIGHT) from a laid-out graph.
pub fn node_rects_from_graph(graph: &Graph) -> HashMap<String, Rect> {
    graph
        .nodes
        .iter()
        .map(|n| {
            (
                n.id.clone(),
                Rect::new(n.x, n.y, n.width, BOX_HEIGHT),
            )
        })
        .collect()
}

/// Compute subgraph envelopes (inner/outer) and portals for the given graph state.
pub fn compute_envelopes(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    gutter: usize,
) -> HashMap<String, SubgraphEnvelope> {
    let mut envelopes = HashMap::new();

    for subgraph in &graph.subgraphs {
        let mut content = Rect::default();
        let mut max_exit_y = 0;
        for node_id in &subgraph.node_ids {
            if let Some(r) = node_rects.get(node_id) {
                content = if content.is_empty() { *r } else { content.union(r) };
                max_exit_y = max_exit_y.max(r.bottom());
            }
        }
        if content.is_empty() {
            continue;
        }

        let mut has_external_edges = false;
        let mut has_outgoing = false;
        let mut outgoing_cross_count = 0usize;
        let mut incoming_cross_count = 0usize;
        for e in &graph.edges {
            let from_in = subgraph.contains_node(&e.from);
            let to_in = subgraph.contains_node(&e.to);
            if (from_in || to_in) && from_in != to_in {
                has_external_edges = true;
                if from_in {
                    has_outgoing = true;
                    outgoing_cross_count += 1;
                } else {
                    incoming_cross_count += 1;
                }
            }
        }

        // Minimal padding: spacer under the top border (larger when a visible title exists),
        // one at the bottom, minimal inner pad, and side gutters as configured.
        let inner_pad = 0;
        let side_pad = if has_external_edges { gutter } else { 2 };

        let title_fits = subgraph.title.as_ref().map_or(false, |t| {
            let title_len = format!("[  {}  ]", t).chars().count();
            title_len <= (content.width + side_pad * 2).saturating_sub(2)
        });

        let top_pad = if title_fits {
            if incoming_cross_count > 0 && outgoing_cross_count == 0 {
                5
            } else if incoming_cross_count > 0 {
                3
            } else {
                2
            }
        } else if has_external_edges {
            1
        } else {
            0
        };
        let bottom_pad = 1;

        let inner = content.inflate(inner_pad);
        let min_bottom_pad = if has_external_edges {
            let clearance = if outgoing_cross_count > 1 { 2 } else { 1 };
            max_exit_y
                .saturating_add(clearance)
                .saturating_sub(inner.y + inner.height)
        } else {
            0
        };
        let mut bottom_pad = bottom_pad.max(min_bottom_pad);
        if has_outgoing && outgoing_cross_count > 1 {
            bottom_pad = bottom_pad.max(gutter.saturating_add(2));
        }

        // Avoid overlapping the bottom border with an outgoing target box:
        // keep at least one empty row between the border and the target arrow row.
        if matches!(graph.direction, Direction::TD | Direction::TB) && has_outgoing {
            let inner_bottom_inclusive = inner.y + inner.height.saturating_sub(1);
            let mut min_target_y: Option<usize> = None;
            for e in &graph.edges {
                if !subgraph.contains_node(&e.from) || subgraph.contains_node(&e.to) {
                    continue;
                }
                let Some(target_rect) = node_rects.get(&e.to) else {
                    continue;
                };
                // Only consider targets placed below the subgraph content.
                if target_rect.y <= inner_bottom_inclusive {
                    continue;
                }
                min_target_y = Some(min_target_y.map_or(target_rect.y, |v| v.min(target_rect.y)));
            }
            if let Some(target_y) = min_target_y {
                let allowed_border_y = target_y.saturating_sub(2);
                let allowed_bottom_pad = allowed_border_y.saturating_sub(inner_bottom_inclusive);
                bottom_pad = bottom_pad.min(allowed_bottom_pad.max(min_bottom_pad));
            }
        }
        let outer = Rect::new(
            inner.x.saturating_sub(side_pad),
            inner.y.saturating_sub(top_pad),
            inner.width + side_pad * 2,
            inner.height + top_pad + bottom_pad,
        );

        envelopes.insert(
            subgraph.id.clone(),
            SubgraphEnvelope {
                outer,
                inner,
                portals: PortalSlots::default(),
            },
        );
    }

    // If two subgraphs overlap in canvas space and there is a cross-subgraph edge
    // from one to the other, treat the source subgraph as an outer container and
    // expand it to fully enclose the destination subgraph.
    //
    // This preserves separate stacked subgraphs (no overlap), while allowing
    // "visually nested" compositions to render as nested envelopes.
    if matches!(graph.direction, Direction::TD | Direction::TB | Direction::BT) {
        let intersects = |a: Rect, b: Rect| -> bool {
            a.x < b.right() && a.right() > b.x && a.y < b.bottom() && a.bottom() > b.y
        };
        for e in &graph.edges {
            let Some(from_sg) = graph.get_node_subgraph(&e.from) else {
                continue;
            };
            let Some(to_sg) = graph.get_node_subgraph(&e.to) else {
                continue;
            };
            if from_sg == to_sg {
                continue;
            }
            let (parent_id, child_id) = match graph.direction {
                Direction::BT => (to_sg, from_sg),
                _ => (from_sg, to_sg),
            };
            let (Some(parent), Some(child)) =
                (envelopes.get(parent_id), envelopes.get(child_id))
            else {
                continue;
            };
            if !intersects(parent.outer, child.outer) {
                continue;
            }
            let mut new_outer = parent.outer.union(&child.outer);
            // Ensure the parent border doesn't land on the same row as the child border;
            // give the parent at least one extra row of depth beyond the child.
            if matches!(graph.direction, Direction::TD | Direction::TB) {
                let desired_bottom = child.outer.bottom().saturating_add(1);
                if new_outer.bottom() < desired_bottom {
                    new_outer.height += desired_bottom - new_outer.bottom();
                }
            }
            if let Some(parent_mut) = envelopes.get_mut(parent_id) {
                parent_mut.outer = new_outer;
            }
        }
    }

    // Populate portals after envelopes are defined so we can clamp coordinates.
    let slots = collect_portal_slots(graph, node_rects, graph.direction);
    for (sg_id, portal) in slots {
        if let Some(env) = envelopes.get_mut(&sg_id) {
            env.portals = portal;
        }
    }

    envelopes
}

/// Shared portal slot discovery (used by layout + render).
pub fn collect_portal_slots(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    direction: Direction,
) -> HashMap<String, PortalSlots> {
    let mut slots: HashMap<String, PortalSlots> = HashMap::new();

    for edge in &graph.edges {
        let Some(from) = graph.get_node(&edge.from) else {
            continue;
        };
        let Some(to) = graph.get_node(&edge.to) else {
            continue;
        };

        let from_sg = graph.get_node_subgraph(&edge.from);
        let to_sg = graph.get_node_subgraph(&edge.to);

        if from_sg == to_sg {
            continue;
        }

        match direction {
            Direction::TD | Direction::TB => {
                if let Some(id) = to_sg {
                    slots
                        .entry(id.to_string())
                        .or_default()
                        .top
                        .insert(node_center_x(node_rects, &edge.to, to));
                }
                if let Some(id) = from_sg {
                    slots
                        .entry(id.to_string())
                        .or_default()
                        .bottom
                        .insert(node_center_x(node_rects, &edge.from, from));
                }
            }
            Direction::BT => {
                if let Some(id) = to_sg {
                    slots
                        .entry(id.to_string())
                        .or_default()
                        .bottom
                        .insert(node_center_x(node_rects, &edge.to, to));
                }
                if let Some(id) = from_sg {
                    slots
                        .entry(id.to_string())
                        .or_default()
                        .top
                        .insert(node_center_x(node_rects, &edge.from, from));
                }
            }
            Direction::LR => {
                if let Some(id) = to_sg {
                    slots
                        .entry(id.to_string())
                        .or_default()
                        .left
                        .insert(node_center_y(node_rects, &edge.to, to));
                }
                if let Some(id) = from_sg {
                    slots
                        .entry(id.to_string())
                        .or_default()
                        .right
                        .insert(node_center_y(node_rects, &edge.from, from));
                }
            }
            Direction::RL => {
                if let Some(id) = to_sg {
                    slots
                        .entry(id.to_string())
                        .or_default()
                        .right
                        .insert(node_center_y(node_rects, &edge.to, to));
                }
                if let Some(id) = from_sg {
                    slots
                        .entry(id.to_string())
                        .or_default()
                        .left
                        .insert(node_center_y(node_rects, &edge.from, from));
                }
            }
        }
    }

    slots
}

fn node_center_x(
    rects: &HashMap<String, Rect>,
    node_id: &str,
    fallback_node: &crate::graph::Node,
) -> usize {
    rects
        .get(node_id)
        .map(|r| r.x + r.width / 2)
        .unwrap_or_else(|| fallback_node.center_x())
}

fn node_center_y(
    rects: &HashMap<String, Rect>,
    node_id: &str,
    fallback_node: &crate::graph::Node,
) -> usize {
    rects
        .get(node_id)
        .map(|r| r.y + r.height / 2)
        .unwrap_or_else(|| fallback_node.center_y())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, Node, Subgraph};

    #[test]
    fn portal_slots_cross_subgraph_td() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        g.nodes.push(Node::new("a", "A"));
        g.nodes.push(Node::new("b", "B"));
        g.nodes.push(Node::new("c", "C"));
        g.edges.push(Edge::new("a", "b"));
        g.edges.push(Edge::new("b", "c"));

        let mut sg = Subgraph::new("sg", Some("G".into()));
        sg.add_node("b");
        g.add_subgraph(sg);
        g.associate_node_with_subgraph("b", "sg");

        // Pretend layout already placed nodes.
        g.get_node_mut("a").unwrap().x = 0;
        g.get_node_mut("a").unwrap().y = 0;
        g.get_node_mut("b").unwrap().x = 4;
        g.get_node_mut("b").unwrap().y = 5;
        g.get_node_mut("c").unwrap().x = 8;
        g.get_node_mut("c").unwrap().y = 12;

        let node_rects = node_rects_from_graph(&g);
        let slots = collect_portal_slots(&g, &node_rects, g.direction);
        let portals = slots.get("sg").expect("slots for sg");
        assert!(!portals.top.is_empty(), "incoming edge should create top portal");
        assert!(
            !portals.bottom.is_empty(),
            "outgoing edge should create bottom portal"
        );
    }
}
