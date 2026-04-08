//! Shared subgraph envelope + portal helpers for layout and render.
//!
//! Provides a single source of truth for:
//! - Subgraph inner/outer rectangles (with gutters)
//! - Portal slots per side derived from crossing edges
//! - Helpers to build node rects from a laid-out graph

use std::collections::{HashMap, HashSet};

use crate::geom::Rect;
use crate::graph::{Direction, Graph};

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

/// Build node rectangles from a laid-out graph.
pub fn node_rects_from_graph(graph: &Graph) -> HashMap<String, Rect> {
    graph
        .nodes
        .iter()
        .map(|n| (n.id.clone(), Rect::new(n.x, n.y, n.width, n.height)))
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
                content = if content.is_empty() {
                    *r
                } else {
                    content.union(r)
                };
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
        let mut incoming_outside_sources: HashSet<&str> = HashSet::new();
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
                    incoming_outside_sources.insert(e.from.as_str());
                }
            }
        }

        // Minimal padding: spacer under the top border (larger when a visible title exists),
        // one at the bottom, minimal inner pad, and side gutters as configured.
        let inner_pad = 0;
        let mut side_pad = if has_external_edges { gutter } else { 2 };

        // Ensure titled subgraphs are wide enough to display the title with portal clearance.
        let title_buffer = if matches!(graph.direction, Direction::BT) && incoming_cross_count > 1 {
            6
        } else if matches!(graph.direction, Direction::BT) && incoming_cross_count > 0 {
            4
        } else if has_external_edges {
            2
        } else {
            1
        };
        if let Some(t) = subgraph.title.as_ref() {
            let title_len = format!("[  {}  ]", t).chars().count();
            let min_outer_width = title_len.saturating_add(2 + title_buffer * 2);
            if content.width + side_pad * 2 < min_outer_width {
                let needed = min_outer_width.saturating_sub(content.width);
                side_pad = side_pad.max(needed.div_ceil(2));
            }
            if matches!(graph.direction, Direction::BT) && incoming_cross_count > 0 {
                side_pad = side_pad.max(title_buffer);
            }
        }

        let title_fits = subgraph.title.as_ref().is_some_and(|t| {
            let title_len = format!("[  {}  ]", t).chars().count();
            let available = (content.width + side_pad * 2).saturating_sub(2);
            title_len.saturating_add(title_buffer * 2) <= available
        });

        let mut top_pad = if title_fits {
            // Title lives on the border row; keep one empty row below it by default.
            //
            // Special-case: when a single external source fans out into multiple targets
            // inside this titled subgraph, we need extra internal rows to draw a trunk,
            // split bar, drops, and arrowheads without colliding with the title row.
            let is_fanout_entry = incoming_cross_count > 1 && incoming_outside_sources.len() == 1;
            if is_fanout_entry {
                5
            } else if incoming_cross_count > 0
                && matches!(graph.direction, Direction::TD | Direction::TB)
            {
                3
            } else {
                2
            }
        } else if has_external_edges {
            if incoming_cross_count > 0 {
                2
            } else {
                1
            }
        } else {
            0
        };

        // BT with outgoing edges: edges exit from TOP of sources and need merge space
        // above the sources (smaller y). This is the opposite of TD where outgoing
        // edges need bottom_pad.
        if matches!(graph.direction, Direction::BT) && has_outgoing && outgoing_cross_count > 1 {
            // Need space for: merge line + vertical stems from sources
            top_pad = top_pad.max(gutter.saturating_add(2));
        }

        let mut bottom_pad = 1;
        if matches!(graph.direction, Direction::BT) && incoming_cross_count > 0 {
            bottom_pad = bottom_pad.max(if title_fits { 3 } else { 2 });
        }

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
                // If the target is inside another subgraph, we need clearance for
                // that subgraph's top border (title row + border), not just the node.
                let effective_y = if let Some(target_sg_id) = graph.get_node_subgraph(&e.to) {
                    // Find the target subgraph and compute its topmost node Y
                    if let Some(target_sg) = graph.get_subgraph(target_sg_id) {
                        // Compute the minimum Y of all nodes in the target subgraph
                        let min_node_y = target_sg
                            .node_ids
                            .iter()
                            .filter_map(|id| node_rects.get(id))
                            .map(|r| r.y)
                            .min()
                            .unwrap_or(target_rect.y);
                        // Estimate top border position: nodes have padding above them for
                        // title (2-3 rows) and border (1 row)
                        let has_title = target_sg.title.is_some();
                        let title_clearance = if has_title { 3 } else { 1 };
                        min_node_y.saturating_sub(title_clearance)
                    } else {
                        target_rect.y
                    }
                } else {
                    target_rect.y
                };
                min_target_y = Some(min_target_y.map_or(effective_y, |v| v.min(effective_y)));
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
    //
    // IMPORTANT: Only expand if the child's content is INSIDE the parent's content
    // (true nesting). If the child's inner region is below/above the parent's inner
    // region (stacked), don't expand - let the layout constraint loop handle spacing.
    if matches!(
        graph.direction,
        Direction::TD | Direction::TB | Direction::BT
    ) {
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
            let (Some(parent), Some(child)) = (envelopes.get(parent_id), envelopes.get(child_id))
            else {
                continue;
            };
            if !intersects(parent.outer, child.outer) {
                continue;
            }
            // Check if child is truly nested (inner content starts within parent's inner)
            // vs stacked (child's inner is entirely below/above parent's inner).
            let is_stacked = match graph.direction {
                Direction::TD | Direction::TB => {
                    // In TD/TB, stacked means child's inner starts at or below parent's inner bottom
                    child.inner.y >= parent.inner.bottom()
                }
                Direction::BT => {
                    // In BT, stacked means child's inner bottom is at or above parent's inner top
                    child.inner.bottom() <= parent.inner.y
                }
                _ => false,
            };
            if is_stacked {
                // Don't expand for stacked subgraphs - let layout constraint loop handle spacing
                continue;
            }
            let mut new_outer = parent.outer.union(&child.outer);
            let bt_titled_nested = graph.direction == Direction::BT
                && (graph
                    .get_subgraph(parent_id)
                    .and_then(|sg| sg.title.as_ref())
                    .is_some()
                    || graph
                        .get_subgraph(child_id)
                        .and_then(|sg| sg.title.as_ref())
                        .is_some());
            // Ensure the parent border doesn't land on the same row as the child border;
            // give the parent at least one extra row of depth beyond the child.
            if matches!(graph.direction, Direction::TD | Direction::TB) {
                let desired_bottom = child.outer.bottom().saturating_add(1);
                if new_outer.bottom() < desired_bottom {
                    new_outer.height += desired_bottom - new_outer.bottom();
                }
            } else if graph.direction == Direction::BT {
                // BT: children are below parents, so expand TOP to give clearance
                let desired_top = child.outer.y.saturating_sub(1);
                if new_outer.y > desired_top {
                    let extra = new_outer.y - desired_top;
                    new_outer.y = desired_top;
                    new_outer.height += extra;
                }
                if bt_titled_nested {
                    // BT titles live on the bottom border row. If a visually nested BT parent
                    // stops on the same bottom row as its child, both titles fight for the
                    // same border. Leave one full spacer row between those border rows.
                    let desired_bottom = child.outer.bottom().saturating_add(2);
                    if new_outer.bottom() < desired_bottom {
                        new_outer.height += desired_bottom - new_outer.bottom();
                    }
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
    let mut shared_td_fanout_top_slots: HashMap<(String, String), usize> = HashMap::new();

    let shift_x_out_of_title = |sg_id: &str, desired_x: usize| -> usize {
        let Some(sg) = graph.get_subgraph(sg_id) else {
            return desired_x;
        };
        let Some(title) = sg.title.as_deref() else {
            return desired_x;
        };
        if !sg.bounds.is_valid() {
            return desired_x;
        }
        let title_fmt = format!("[  {}  ]", title);
        let len = title_fmt.chars().count();
        if len == 0 || len > sg.bounds.width.saturating_sub(2) {
            return desired_x;
        }
        let min_x = sg.bounds.x.saturating_add(1);
        let max_x = sg
            .bounds
            .x
            .saturating_add(sg.bounds.width.saturating_sub(2));
        if max_x < min_x {
            return desired_x;
        }
        let start = sg.bounds.x + sg.bounds.width.saturating_sub(len) / 2;
        let end = start + len.saturating_sub(1);
        let protected_start = start.saturating_sub(2);
        let protected_end = end.saturating_add(2).min(max_x);
        let x = desired_x.clamp(min_x, max_x);
        if x < protected_start || x > protected_end {
            return x;
        }
        if graph.direction == Direction::BT {
            let left = (protected_start > min_x).then(|| protected_start.saturating_sub(1));
            let right = (protected_end < max_x).then(|| protected_end + 1);
            match (left, right) {
                (Some(left), Some(right)) => {
                    let left_distance = x.abs_diff(left);
                    let right_distance = x.abs_diff(right);
                    if left_distance < right_distance {
                        left
                    } else if right_distance < left_distance {
                        right
                    } else if x <= (protected_start + protected_end) / 2 {
                        left
                    } else {
                        right
                    }
                }
                (Some(left), None) => left,
                (None, Some(right)) => right,
                (None, None) => x,
            }
        } else if protected_end < max_x {
            protected_end + 1
        } else if protected_start > min_x {
            protected_start.saturating_sub(1)
        } else {
            x
        }
    };

    let bt_nudge_from_corners = |sg_id: &str, x: usize| -> usize {
        let Some(sg) = graph.get_subgraph(sg_id) else {
            return x;
        };
        let Some(title) = sg.title.as_deref() else {
            return x;
        };
        if !sg.bounds.is_valid() {
            return x;
        }
        let min = sg.bounds.x.saturating_add(1);
        let max = sg
            .bounds
            .x
            .saturating_add(sg.bounds.width.saturating_sub(2));
        if max <= min {
            return x;
        }
        let title_fmt = format!("[  {}  ]", title);
        let len = title_fmt.chars().count();
        if len == 0 || len > sg.bounds.width.saturating_sub(2) {
            return x;
        }
        let start = sg.bounds.x + sg.bounds.width.saturating_sub(len) / 2;
        let end = start + len.saturating_sub(1);
        let in_title_text = |pos: usize| pos >= start && pos <= end;
        if x == min {
            let candidate = min.saturating_add(1);
            if candidate <= max && !in_title_text(candidate) {
                return candidate;
            }
        } else if x == max {
            let candidate = max.saturating_sub(1);
            if candidate >= min && !in_title_text(candidate) {
                return candidate;
            }
        }
        x
    };

    if matches!(direction, Direction::TD | Direction::TB) {
        let mut grouped_targets: HashMap<(String, String), Vec<usize>> = HashMap::new();
        for edge in &graph.edges {
            let Some(to) = graph.get_node(&edge.to) else {
                continue;
            };
            let Some(target_sg_id) = graph.get_node_subgraph(&edge.to) else {
                continue;
            };
            if graph.get_node_subgraph(&edge.from) == Some(target_sg_id) {
                continue;
            }

            grouped_targets
                .entry((edge.from.clone(), target_sg_id.to_string()))
                .or_default()
                .push(node_center_x(node_rects, &edge.to, to));
        }

        for ((from_id, sg_id), target_xs) in grouped_targets {
            if target_xs.len() < 2 {
                continue;
            }
            let Some(sg) = graph.get_subgraph(&sg_id) else {
                continue;
            };
            if !sg.bounds.is_valid() {
                continue;
            }

            let portal_center = sg.bounds.x + sg.bounds.width / 2;
            let min_target_x = target_xs.iter().copied().min().unwrap_or(portal_center);
            let max_target_x = target_xs.iter().copied().max().unwrap_or(portal_center);
            shared_td_fanout_top_slots.insert(
                (from_id, sg_id),
                portal_center.clamp(min_target_x, max_target_x),
            );
        }
    }

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
                    let mut x = node_center_x(node_rects, &edge.to, to);
                    if let Some(&shared_x) =
                        shared_td_fanout_top_slots.get(&(edge.from.clone(), id.to_string()))
                    {
                        x = shared_x;
                    } else {
                        x = shift_x_out_of_title(id, x);
                    }
                    slots.entry(id.to_string()).or_default().top.insert(x);
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
                    let mut x = node_center_x(node_rects, &edge.to, to);
                    x = shift_x_out_of_title(id, x);
                    x = bt_nudge_from_corners(id, x);
                    slots.entry(id.to_string()).or_default().bottom.insert(x);
                }
                if let Some(id) = from_sg {
                    let mut x = node_center_x(node_rects, &edge.from, from);
                    x = shift_x_out_of_title(id, x);
                    x = bt_nudge_from_corners(id, x);
                    slots.entry(id.to_string()).or_default().top.insert(x);
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
        assert!(
            !portals.top.is_empty(),
            "incoming edge should create top portal"
        );
        assert!(
            !portals.bottom.is_empty(),
            "outgoing edge should create bottom portal"
        );
    }

    #[test]
    fn portal_slots_collapse_shared_td_fanout_entry_to_single_top_slot() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        g.nodes.push(Node::new("router", "Router"));
        g.nodes.push(Node::new("h1", "Handler 1"));
        g.nodes.push(Node::new("h2", "Handler 2"));
        g.nodes.push(Node::new("h3", "Handler 3"));
        g.edges.push(Edge::new("router", "h1"));
        g.edges.push(Edge::new("router", "h2"));
        g.edges.push(Edge::new("router", "h3"));

        let mut sg = Subgraph::new("sg", Some("Handler Group".into()));
        sg.add_node("h1");
        sg.add_node("h2");
        sg.add_node("h3");
        sg.bounds = crate::graph::Rectangle {
            x: 0,
            y: 5,
            width: 57,
            height: 9,
        };
        g.add_subgraph(sg);
        g.associate_node_with_subgraph("h1", "sg");
        g.associate_node_with_subgraph("h2", "sg");
        g.associate_node_with_subgraph("h3", "sg");

        g.get_node_mut("router").unwrap().x = 18;
        g.get_node_mut("router").unwrap().y = 0;
        g.get_node_mut("h1").unwrap().x = 2;
        g.get_node_mut("h1").unwrap().y = 10;
        g.get_node_mut("h2").unwrap().x = 21;
        g.get_node_mut("h2").unwrap().y = 10;
        g.get_node_mut("h3").unwrap().x = 40;
        g.get_node_mut("h3").unwrap().y = 10;

        let node_rects = node_rects_from_graph(&g);
        let slots = collect_portal_slots(&g, &node_rects, g.direction);
        let portals = slots.get("sg").expect("slots for sg");

        assert_eq!(
            portals.top.len(),
            1,
            "shared TD fanout should reserve one top entry slot, got {:?}",
            portals.top
        );
    }
}
