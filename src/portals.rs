//! Shared subgraph envelope + portal helpers for layout and render.
//!
//! Provides a single source of truth for:
//! - Subgraph inner/outer rectangles (with gutters)
//! - Portal slots per side derived from crossing edges
//! - Helpers to build node rects from a laid-out graph

use std::collections::{HashMap, HashSet};

use crate::geom::Rect;
use crate::graph::{Direction, Graph};

mod envelopes;

pub use envelopes::{compute_envelopes, node_rects_from_graph};
use envelopes::{current_node_rect, current_subgraph_bounds};

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

/// Shared portal slot discovery (used by layout + render).
pub fn collect_portal_slots(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    direction: Direction,
) -> HashMap<String, PortalSlots> {
    collect_portal_slots_with_bounds(graph, node_rects, direction, None)
}

fn collect_portal_slots_with_bounds(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    direction: Direction,
    current_bounds: Option<&HashMap<String, Rect>>,
) -> HashMap<String, PortalSlots> {
    let mut slots: HashMap<String, PortalSlots> = HashMap::new();
    let mut shared_td_fanout_top_slots: HashMap<(String, String), usize> = HashMap::new();
    let mut shared_td_fanin_bottom_slots: HashMap<(String, String), usize> = HashMap::new();
    let mut shared_horizontal_fanin_side_slots: HashMap<(String, String), usize> = HashMap::new();

    let shift_x_out_of_title = |sg_id: &str, desired_x: usize| -> usize {
        let Some(sg) = graph.get_subgraph(sg_id) else {
            return desired_x;
        };
        let Some(bounds) = current_subgraph_bounds(graph, current_bounds, sg_id) else {
            return desired_x;
        };
        let Some(title) = sg.title.as_deref() else {
            return desired_x;
        };
        if bounds.is_empty() {
            return desired_x;
        }
        let Some((start, end)) =
            crate::graph::subgraph_title_span(bounds.x, bounds.width, title, graph.direction)
        else {
            return desired_x;
        };
        let min_x = bounds.x.saturating_add(1);
        let max_x = bounds.x.saturating_add(bounds.width.saturating_sub(2));
        if max_x < min_x {
            return desired_x;
        }
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
        let Some(bounds) = current_subgraph_bounds(graph, current_bounds, sg_id) else {
            return x;
        };
        let Some(title) = sg.title.as_deref() else {
            return x;
        };
        if bounds.is_empty() {
            return x;
        }
        let min = bounds.x.saturating_add(1);
        let max = bounds.x.saturating_add(bounds.width.saturating_sub(2));
        if max <= min {
            return x;
        }
        let Some((start, end)) =
            crate::graph::subgraph_title_span(bounds.x, bounds.width, title, graph.direction)
        else {
            return x;
        };
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
        let mut grouped_sources: HashMap<(String, String), Vec<usize>> = HashMap::new();
        for edge in &graph.edges {
            let Some(from) = graph.get_node(&edge.from) else {
                continue;
            };
            let Some(to) = graph.get_node(&edge.to) else {
                continue;
            };
            let (_, enter_subgraphs) = graph.edge_boundary_crossings(&edge.from, &edge.to);
            for target_sg_id in enter_subgraphs {
                grouped_targets
                    .entry((edge.from.clone(), target_sg_id.to_string()))
                    .or_default()
                    .push(node_center_x(node_rects, &edge.to, to));
            }

            let (exit_subgraphs, _) = graph.edge_boundary_crossings(&edge.from, &edge.to);
            for source_sg_id in exit_subgraphs {
                grouped_sources
                    .entry((edge.to.clone(), source_sg_id.to_string()))
                    .or_default()
                    .push(node_center_x(node_rects, &edge.from, from));
            }
        }

        for ((from_id, sg_id), target_xs) in grouped_targets {
            if target_xs.len() < 2 {
                continue;
            }
            let Some(bounds) = current_subgraph_bounds(graph, current_bounds, &sg_id) else {
                continue;
            };
            if bounds.is_empty() {
                continue;
            }

            let portal_center = bounds.x + bounds.width / 2;
            let min_target_x = target_xs.iter().copied().min().unwrap_or(portal_center);
            let max_target_x = target_xs.iter().copied().max().unwrap_or(portal_center);
            shared_td_fanout_top_slots.insert(
                (from_id, sg_id),
                portal_center.clamp(min_target_x, max_target_x),
            );
        }

        for ((to_id, sg_id), source_xs) in grouped_sources {
            if source_xs.len() < 2 {
                continue;
            }
            let Some(bounds) = current_subgraph_bounds(graph, current_bounds, &sg_id) else {
                continue;
            };
            if bounds.is_empty() {
                continue;
            }
            let Some(target) = graph.get_node(&to_id) else {
                continue;
            };

            let min_source_x = source_xs.iter().copied().min().unwrap_or(bounds.x);
            let max_source_x = source_xs.iter().copied().max().unwrap_or(bounds.x);
            let target_center_x = node_center_x(node_rects, &to_id, target);
            let inset = if bounds.width >= 9 { 1 } else { 0 };
            let min_x = bounds.x.saturating_add(inset);
            let max_x = bounds
                .x
                .saturating_add(bounds.width.saturating_sub(inset + 1));
            let shared_x = target_center_x
                .clamp(min_source_x, max_source_x)
                .clamp(min_x, max_x.max(min_x));
            shared_td_fanin_bottom_slots.insert((to_id, sg_id), shared_x);
        }
    } else if matches!(direction, Direction::LR | Direction::RL) {
        let mut grouped_sources: HashMap<(String, String), Vec<usize>> = HashMap::new();
        for edge in &graph.edges {
            let Some(from) = graph.get_node(&edge.from) else {
                continue;
            };
            let (exit_subgraphs, _) = graph.edge_boundary_crossings(&edge.from, &edge.to);
            for source_sg_id in exit_subgraphs {
                grouped_sources
                    .entry((edge.to.clone(), source_sg_id.to_string()))
                    .or_default()
                    .push(node_center_y(node_rects, &edge.from, from));
            }
        }

        for ((to_id, sg_id), source_ys) in grouped_sources {
            if source_ys.len() < 2 {
                continue;
            }
            let Some(bounds) = current_subgraph_bounds(graph, current_bounds, &sg_id) else {
                continue;
            };
            if bounds.is_empty() {
                continue;
            }

            let min_source_y = source_ys.iter().copied().min().unwrap_or(bounds.y);
            let max_source_y = source_ys.iter().copied().max().unwrap_or(bounds.y);
            let portal_y = ((min_source_y + max_source_y) / 2).clamp(
                bounds.y.saturating_add(1),
                bounds.y + bounds.height.saturating_sub(2),
            );
            shared_horizontal_fanin_side_slots.insert((to_id, sg_id), portal_y);
        }
    }

    for edge in &graph.edges {
        let Some(from) = graph.get_node(&edge.from) else {
            continue;
        };
        let Some(to) = graph.get_node(&edge.to) else {
            continue;
        };

        let (exit_subgraphs, enter_subgraphs) = graph.edge_boundary_crossings(&edge.from, &edge.to);
        if exit_subgraphs.is_empty() && enter_subgraphs.is_empty() {
            continue;
        }

        match direction {
            Direction::TD | Direction::TB => {
                for &id in &enter_subgraphs {
                    let Some(target_bounds) = current_subgraph_bounds(graph, current_bounds, id)
                    else {
                        continue;
                    };

                    let source_x = node_center_x(node_rects, &edge.from, from);
                    let source_exit_y = node_exit_y(node_rects, &edge.from, from);
                    let source_rect = current_node_rect(node_rects, &edge.from, from);
                    let target_top_interior = target_bounds.y.saturating_add(1);
                    let target_bottom_interior = target_bounds
                        .y
                        .saturating_add(target_bounds.height.saturating_sub(2));
                    let visually_nested_parent = graph.subgraphs.iter().any(|candidate| {
                        let Some(candidate_bounds) =
                            current_subgraph_bounds(graph, current_bounds, &candidate.id)
                        else {
                            return false;
                        };
                        if candidate.id == id || candidate_bounds.is_empty() {
                            return false;
                        }
                        let child_right = target_bounds.x + target_bounds.width;
                        let child_bottom = target_bounds.y + target_bounds.height;
                        let source_right = source_rect.right();
                        let source_bottom = source_rect.bottom();
                        target_bounds.x >= candidate_bounds.x
                            && target_bounds.y >= candidate_bounds.y
                            && child_right <= candidate_bounds.x + candidate_bounds.width
                            && child_bottom <= candidate_bounds.y + candidate_bounds.height
                            && source_rect.x >= candidate_bounds.x
                            && source_rect.y >= candidate_bounds.y
                            && source_right <= candidate_bounds.x + candidate_bounds.width
                            && source_bottom <= candidate_bounds.y + candidate_bounds.height
                    });

                    let can_side_enter = visually_nested_parent
                        && !target_bounds.is_empty()
                        && source_exit_y >= target_top_interior
                        && source_exit_y <= target_bottom_interior;

                    if can_side_enter && source_x < target_bounds.x {
                        slots
                            .entry(id.to_string())
                            .or_default()
                            .left
                            .insert(source_exit_y);
                        continue;
                    }
                    if can_side_enter
                        && source_x
                            > target_bounds
                                .x
                                .saturating_add(target_bounds.width.saturating_sub(1))
                    {
                        slots
                            .entry(id.to_string())
                            .or_default()
                            .right
                            .insert(source_exit_y);
                        continue;
                    }

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
                for id in exit_subgraphs {
                    let Some(exit_bounds) = current_subgraph_bounds(graph, current_bounds, id)
                    else {
                        continue;
                    };
                    let source_rect = current_node_rect(node_rects, &edge.from, from);
                    let suppress_exit = enter_subgraphs.iter().any(|target_id| {
                        let Some(target_bounds) =
                            current_subgraph_bounds(graph, current_bounds, target_id)
                        else {
                            return false;
                        };
                        let target_right = target_bounds.x + target_bounds.width;
                        let target_bottom = target_bounds.y + target_bounds.height;
                        let source_right = source_rect.right();
                        let source_bottom = source_rect.bottom();
                        !target_bounds.is_empty()
                            && !exit_bounds.is_empty()
                            && target_bounds.x >= exit_bounds.x
                            && target_bounds.y >= exit_bounds.y
                            && target_right <= exit_bounds.x + exit_bounds.width
                            && target_bottom <= exit_bounds.y + exit_bounds.height
                            && source_rect.x >= exit_bounds.x
                            && source_rect.y >= exit_bounds.y
                            && source_right <= exit_bounds.x + exit_bounds.width
                            && source_bottom <= exit_bounds.y + exit_bounds.height
                    });
                    if suppress_exit {
                        continue;
                    }
                    let slot_x = shared_td_fanin_bottom_slots
                        .get(&(edge.to.clone(), id.to_string()))
                        .copied()
                        .unwrap_or_else(|| node_center_x(node_rects, &edge.from, from));
                    slots
                        .entry(id.to_string())
                        .or_default()
                        .bottom
                        .insert(slot_x);
                }
            }
            Direction::BT => {
                let nested_bt_entry = enter_subgraphs.len() > 1;
                for id in enter_subgraphs {
                    if nested_bt_entry {
                        continue;
                    }
                    let mut x = node_center_x(node_rects, &edge.to, to);
                    x = shift_x_out_of_title(id, x);
                    x = bt_nudge_from_corners(id, x);
                    slots.entry(id.to_string()).or_default().bottom.insert(x);
                }
                for id in exit_subgraphs {
                    let mut x = node_center_x(node_rects, &edge.from, from);
                    x = shift_x_out_of_title(id, x);
                    x = bt_nudge_from_corners(id, x);
                    slots.entry(id.to_string()).or_default().top.insert(x);
                }
            }
            Direction::LR => {
                for id in enter_subgraphs {
                    slots
                        .entry(id.to_string())
                        .or_default()
                        .left
                        .insert(node_center_y(node_rects, &edge.to, to));
                }
                for id in exit_subgraphs {
                    let slot_y = shared_horizontal_fanin_side_slots
                        .get(&(edge.to.clone(), id.to_string()))
                        .copied()
                        .unwrap_or_else(|| node_center_y(node_rects, &edge.from, from));
                    slots
                        .entry(id.to_string())
                        .or_default()
                        .right
                        .insert(slot_y);
                }
            }
            Direction::RL => {
                for id in enter_subgraphs {
                    slots
                        .entry(id.to_string())
                        .or_default()
                        .right
                        .insert(node_center_y(node_rects, &edge.to, to));
                }
                for id in exit_subgraphs {
                    let slot_y = shared_horizontal_fanin_side_slots
                        .get(&(edge.to.clone(), id.to_string()))
                        .copied()
                        .unwrap_or_else(|| node_center_y(node_rects, &edge.from, from));
                    slots.entry(id.to_string()).or_default().left.insert(slot_y);
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

fn node_exit_y(
    rects: &HashMap<String, Rect>,
    node_id: &str,
    fallback_node: &crate::graph::Node,
) -> usize {
    rects
        .get(node_id)
        .map(|r| r.y + r.height)
        .unwrap_or_else(|| fallback_node.bottom_y())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, Node, Subgraph};

    fn rect_inside(outer: Rect, inner: Rect) -> bool {
        inner.x >= outer.x
            && inner.y >= outer.y
            && inner.right() <= outer.right()
            && inner.bottom() <= outer.bottom()
    }

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

    #[test]
    fn portal_slots_collapse_shared_td_fanin_exit_to_single_bottom_slot() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        g.nodes.push(Node::new("d1", "User DB"));
        g.nodes.push(Node::new("d2", "Order DB"));
        g.nodes.push(Node::new("rsp", "Response"));
        g.edges.push(Edge::new("d1", "rsp"));
        g.edges.push(Edge::new("d2", "rsp"));

        let mut sg = Subgraph::new("sg", Some("Data Layer".into()));
        sg.add_node("d1");
        sg.add_node("d2");
        sg.bounds = crate::graph::Rectangle::new(0, 10, 41, 15);
        g.add_subgraph(sg);
        g.associate_node_with_subgraph("d1", "sg");
        g.associate_node_with_subgraph("d2", "sg");

        g.get_node_mut("d1").unwrap().x = 24;
        g.get_node_mut("d1").unwrap().y = 14;
        g.get_node_mut("d1").unwrap().width = 13;
        g.get_node_mut("d1").unwrap().height = 3;
        g.get_node_mut("d2").unwrap().x = 4;
        g.get_node_mut("d2").unwrap().y = 20;
        g.get_node_mut("d2").unwrap().width = 14;
        g.get_node_mut("d2").unwrap().height = 3;
        g.get_node_mut("rsp").unwrap().x = 10;
        g.get_node_mut("rsp").unwrap().y = 28;
        g.get_node_mut("rsp").unwrap().width = 22;
        g.get_node_mut("rsp").unwrap().height = 3;

        let node_rects = node_rects_from_graph(&g);
        let slots = collect_portal_slots(&g, &node_rects, g.direction);
        let portals = slots.get("sg").expect("slots for sg");

        assert_eq!(
            portals.bottom.len(),
            1,
            "shared TD fanin should reserve one bottom exit slot, got {:?}",
            portals.bottom
        );
    }

    #[test]
    fn portal_slots_td_visually_nested_child_can_use_left_side_entry() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        g.nodes.push(Node::new("s2", "Order Service"));
        g.nodes.push(Node::new("d2", "Order DB"));
        g.edges.push(Edge::new("s2", "d2"));

        let mut outer = Subgraph::new("outer", Some("Service".into()));
        outer.bounds = crate::graph::Rectangle::new(0, 6, 47, 29);
        outer.add_node("s2");

        let mut inner = Subgraph::new("inner", Some("Data".into()));
        inner.bounds = crate::graph::Rectangle::new(22, 16, 23, 17);
        inner.add_node("d2");

        g.add_subgraph(outer);
        g.add_subgraph(inner);
        g.associate_node_with_subgraph("s2", "outer");
        g.associate_node_with_subgraph("d2", "inner");

        g.get_node_mut("s2").unwrap().x = 2;
        g.get_node_mut("s2").unwrap().y = 19;
        g.get_node_mut("s2").unwrap().width = 19;
        g.get_node_mut("s2").unwrap().height = 3;
        g.get_node_mut("d2").unwrap().x = 24;
        g.get_node_mut("d2").unwrap().y = 26;
        g.get_node_mut("d2").unwrap().width = 14;
        g.get_node_mut("d2").unwrap().height = 3;

        let node_rects = node_rects_from_graph(&g);
        let slots = collect_portal_slots(&g, &node_rects, g.direction);
        let portals = slots.get("inner").expect("slots for inner");

        assert!(
            portals.left.contains(&22),
            "expected the visually nested child to expose a left-side TD entry slot: {portals:?}"
        );
        assert!(
            portals.top.is_empty(),
            "expected side-entry routing to avoid a redundant top slot for this edge: {portals:?}"
        );
        assert!(
            slots.get("outer").is_none_or(|outer_portals| outer_portals.bottom.is_empty()),
            "expected the containing parent to avoid a redundant bottom exit slot when the edge stays visually inside it: {:?}",
            slots.get("outer")
        );
    }

    #[test]
    fn portal_slots_td_side_entry_uses_live_node_rects_not_stale_graph_coords() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        g.nodes.push(Node::new("s2", "Order Service"));
        g.nodes.push(Node::new("d2", "Order DB"));
        g.edges.push(Edge::new("s2", "d2"));

        let mut outer = Subgraph::new("outer", Some("Service".into()));
        outer.bounds = crate::graph::Rectangle::new(0, 6, 54, 29);
        outer.add_node("s2");

        let mut inner = Subgraph::new("inner", Some("Data".into()));
        inner.bounds = crate::graph::Rectangle::new(25, 16, 27, 17);
        inner.add_node("d2");

        g.add_subgraph(outer);
        g.add_subgraph(inner);
        g.get_subgraph_mut("inner").unwrap().parent_id = Some("outer".into());
        g.get_subgraph_mut("outer").unwrap().add_child("inner");
        g.associate_node_with_subgraph("s2", "outer");
        g.associate_node_with_subgraph("d2", "inner");

        // Simulate a layout loop where graph node positions are stale but node_rects
        // carry the live geometry that portal discovery must honor.
        g.get_node_mut("s2").unwrap().x = 0;
        g.get_node_mut("s2").unwrap().y = 0;
        g.get_node_mut("s2").unwrap().width = 19;
        g.get_node_mut("s2").unwrap().height = 3;
        g.get_node_mut("d2").unwrap().x = 0;
        g.get_node_mut("d2").unwrap().y = 0;
        g.get_node_mut("d2").unwrap().width = 14;
        g.get_node_mut("d2").unwrap().height = 3;

        let node_rects = HashMap::from([
            ("s2".to_string(), Rect::new(2, 19, 19, 3)),
            ("d2".to_string(), Rect::new(27, 26, 14, 3)),
        ]);

        let slots = collect_portal_slots(&g, &node_rects, g.direction);
        let portals = slots.get("inner").expect("slots for inner");

        assert!(
            portals.left.contains(&22),
            "expected the visually nested child to keep a left-side TD entry slot from live rects: {portals:?}"
        );
        assert!(
            slots.get("outer").is_none_or(|outer_portals| outer_portals.bottom.is_empty()),
            "expected the containing parent to suppress redundant bottom exits when live rects show the edge staying inside it: {:?}",
            slots.get("outer")
        );
    }

    #[test]
    fn portal_slots_external_to_nested_child_open_all_entered_ancestors() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        g.nodes.push(Node::new("src", "Source"));
        g.nodes.push(Node::new("dst", "Target"));
        g.edges.push(Edge::new("src", "dst"));

        g.add_subgraph(Subgraph::new("parent", Some("Parent".into())));
        g.add_subgraph(Subgraph::new("child", Some("Child".into())));
        g.get_subgraph_mut("child").unwrap().parent_id = Some("parent".into());
        g.get_subgraph_mut("parent").unwrap().add_child("child");
        g.associate_node_with_subgraph("dst", "child");

        g.get_node_mut("src").unwrap().x = 10;
        g.get_node_mut("src").unwrap().y = 0;
        g.get_node_mut("dst").unwrap().x = 12;
        g.get_node_mut("dst").unwrap().y = 10;

        let node_rects = node_rects_from_graph(&g);
        let slots = collect_portal_slots(&g, &node_rects, g.direction);

        assert!(!slots
            .get("parent")
            .expect("parent slots should exist")
            .top
            .is_empty());
        assert!(!slots
            .get("child")
            .expect("child slots should exist")
            .top
            .is_empty());
    }

    #[test]
    fn portal_slots_child_to_external_open_all_exited_ancestors() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        g.nodes.push(Node::new("src", "Source"));
        g.nodes.push(Node::new("dst", "Target"));
        g.edges.push(Edge::new("src", "dst"));

        g.add_subgraph(Subgraph::new("parent", Some("Parent".into())));
        g.add_subgraph(Subgraph::new("child", Some("Child".into())));
        g.get_subgraph_mut("child").unwrap().parent_id = Some("parent".into());
        g.get_subgraph_mut("parent").unwrap().add_child("child");
        g.associate_node_with_subgraph("src", "child");

        g.get_node_mut("src").unwrap().x = 12;
        g.get_node_mut("src").unwrap().y = 10;
        g.get_node_mut("dst").unwrap().x = 10;
        g.get_node_mut("dst").unwrap().y = 20;

        let node_rects = node_rects_from_graph(&g);
        let slots = collect_portal_slots(&g, &node_rects, g.direction);

        assert!(!slots
            .get("parent")
            .expect("parent slots should exist")
            .bottom
            .is_empty());
        assert!(!slots
            .get("child")
            .expect("child slots should exist")
            .bottom
            .is_empty());
    }

    #[test]
    fn compute_envelopes_builds_parent_from_child_when_parent_has_no_direct_nodes() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        g.add_subgraph(Subgraph::new("parent", Some("Parent".into())));
        g.add_subgraph(Subgraph::new("child", Some("Child".into())));
        g.get_subgraph_mut("child").unwrap().parent_id = Some("parent".into());
        g.get_subgraph_mut("parent").unwrap().add_child("child");

        g.add_node(Node::new("n1", "Inner"));
        g.get_node_mut("n1").unwrap().x = 10;
        g.get_node_mut("n1").unwrap().y = 8;
        g.associate_node_with_subgraph("n1", "child");

        let node_rects = node_rects_from_graph(&g);
        let envelopes = compute_envelopes(&g, &node_rects, 2);
        let parent = envelopes.get("parent").expect("parent envelope");
        let child = envelopes.get("child").expect("child envelope");

        assert!(rect_inside(parent.inner, child.outer.inflate(1)));
        assert!(rect_inside(parent.outer, child.outer));
    }

    #[test]
    fn compute_envelopes_counts_descendant_edges_as_parent_external_edges() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        g.add_subgraph(Subgraph::new("parent", Some("Parent".into())));
        g.add_subgraph(Subgraph::new("child", Some("Child".into())));
        g.get_subgraph_mut("child").unwrap().parent_id = Some("parent".into());
        g.get_subgraph_mut("parent").unwrap().add_child("child");

        g.add_node(Node::new("inside", "Inside"));
        g.add_node(Node::new("outside", "Outside"));
        g.get_node_mut("inside").unwrap().x = 12;
        g.get_node_mut("inside").unwrap().y = 8;
        g.get_node_mut("outside").unwrap().x = 35;
        g.get_node_mut("outside").unwrap().y = 16;
        g.associate_node_with_subgraph("inside", "child");
        g.add_edge(Edge::new("inside", "outside"));

        let node_rects = node_rects_from_graph(&g);
        let envelopes = compute_envelopes(&g, &node_rects, 3);
        let parent = envelopes.get("parent").expect("parent envelope");

        assert!(
            parent.outer.width > parent.inner.width + 4,
            "parent should reserve external-edge gutter for descendant crossings: outer={:?} inner={:?}",
            parent.outer,
            parent.inner
        );
    }

    #[test]
    fn compute_envelopes_keep_parent_visibly_outside_nested_child() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        g.add_subgraph(Subgraph::new("parent", Some("Parent".into())));
        g.add_subgraph(Subgraph::new("child", Some("Child".into())));
        g.get_subgraph_mut("child").unwrap().parent_id = Some("parent".into());
        g.get_subgraph_mut("parent").unwrap().add_child("child");

        g.add_node(Node::new("parent_node", "Parent Node"));
        g.add_node(Node::new("child_node", "Child Node"));
        g.add_node(Node::new("outside", "Outside"));
        g.get_node_mut("parent_node").unwrap().x = 2;
        g.get_node_mut("parent_node").unwrap().y = 6;
        g.get_node_mut("child_node").unwrap().x = 24;
        g.get_node_mut("child_node").unwrap().y = 12;
        g.get_node_mut("outside").unwrap().x = 12;
        g.get_node_mut("outside").unwrap().y = 20;
        g.associate_node_with_subgraph("parent_node", "parent");
        g.associate_node_with_subgraph("child_node", "child");
        g.add_edge(Edge::new("child_node", "outside"));

        let node_rects = node_rects_from_graph(&g);
        let envelopes = compute_envelopes(&g, &node_rects, 2);
        let parent = envelopes.get("parent").expect("parent envelope");
        let child = envelopes.get("child").expect("child envelope");

        assert!(
            parent.outer.y < child.outer.y || parent.outer.bottom() > child.outer.bottom(),
            "parent should stay visibly outside nested child: parent={:?} child={:?}",
            parent.outer,
            child.outer
        );
        assert!(rect_inside(parent.outer, child.outer));
    }
}
