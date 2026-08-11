//! Direction-neutral coordinate and line-projection primitives for edge routing.

use crate::graph::{Direction, Graph, Node};
use crate::orientation::OrientedCoords;
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::portal_projection::{subgraph_title_y, title_span};
use super::{set_route_edge_char, RouteOwner};

pub(super) fn get_node_center(node: &Node) -> (usize, usize) {
    (node.center_x(), node.center_y())
}

/// Where an incoming edge enters a target node (arrow position).
pub(super) fn edge_entry_point(node: &Node, direction: Direction) -> (usize, usize) {
    let shape_clearance = node.shape.incoming_edge_clearance(direction);

    match direction {
        Direction::TD | Direction::TB => {
            (node.center_x(), node.y.saturating_sub(1 + shape_clearance))
        }
        Direction::LR => (node.x.saturating_sub(1 + shape_clearance), node.center_y()),
        Direction::RL => (
            node.x
                .saturating_add(node.width)
                .saturating_add(shape_clearance),
            node.center_y(),
        ),
        Direction::BT => (
            node.center_x(),
            node.bottom_y().saturating_add(shape_clearance),
        ),
    }
}

pub(super) fn adjusted_edge_entry_point(
    node: &Node,
    direction: Direction,
    graph: &Graph,
) -> (usize, usize) {
    let default = edge_entry_point(node, direction);
    if !hits_foreign_subgraph_border(node, default.0, default.1, graph) {
        return default;
    }

    for candidate in edge_entry_candidates(node, direction) {
        if !hits_foreign_subgraph_border(node, candidate.0, candidate.1, graph) {
            return candidate;
        }
    }

    default
}

pub(crate) fn edge_entry_candidates(node: &Node, direction: Direction) -> Vec<(usize, usize)> {
    let mut candidates = Vec::new();
    let push_if_new = |candidates: &mut Vec<(usize, usize)>, candidate| {
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    };

    match direction {
        Direction::TD | Direction::TB | Direction::BT => {
            let y = edge_entry_point(node, direction).1;
            let center = node.center_x();
            push_if_new(&mut candidates, (center, y));

            let min_x = node.x.saturating_add(1);
            let max_x = node.x + node.width.saturating_sub(2);
            for delta in 1..=node.width {
                let left = center.saturating_sub(delta);
                if left >= min_x {
                    push_if_new(&mut candidates, (left, y));
                }
                let right = center.saturating_add(delta);
                if right <= max_x {
                    push_if_new(&mut candidates, (right, y));
                }
                if left < min_x && right > max_x {
                    break;
                }
            }
        }
        Direction::LR | Direction::RL => {
            let x = edge_entry_point(node, direction).0;
            let center = node.center_y();
            push_if_new(&mut candidates, (x, center));

            let min_y = node.y.saturating_add(1);
            let max_y = node.y + node.height.saturating_sub(2);
            for delta in 1..=node.height {
                let up = center.saturating_sub(delta);
                if up >= min_y {
                    push_if_new(&mut candidates, (x, up));
                }
                let down = center.saturating_add(delta);
                if down <= max_y {
                    push_if_new(&mut candidates, (x, down));
                }
                if up < min_y && down > max_y {
                    break;
                }
            }
        }
    }

    candidates
}

pub(super) fn hits_foreign_subgraph_border(node: &Node, x: usize, y: usize, graph: &Graph) -> bool {
    let own_subgraph = graph.get_node_subgraph(&node.id);

    graph.subgraphs.iter().any(|subgraph| {
        if !subgraph.bounds.is_valid() || own_subgraph == Some(subgraph.id.as_str()) {
            return false;
        }

        let min_x = subgraph.bounds.x;
        let max_x = subgraph.bounds.x + subgraph.bounds.width.saturating_sub(1);
        let min_y = subgraph.bounds.y;
        let max_y = subgraph.bounds.y + subgraph.bounds.height.saturating_sub(1);
        let within_x = x >= min_x && x <= max_x;
        let within_y = y >= min_y && y <= max_y;

        within_x && within_y && (x == min_x || x == max_x || y == min_y || y == max_y)
    })
}

/// Where an outgoing edge exits a source node (stem start position).
pub fn edge_exit_point(node: &Node, direction: Direction) -> (usize, usize) {
    match direction {
        Direction::TD | Direction::TB => (node.center_x(), node.bottom_y()),
        Direction::LR => (node.x + node.width, node.center_y()),
        Direction::RL => (node.x.saturating_sub(1), node.center_y()),
        Direction::BT => (node.center_x(), node.y.saturating_sub(1)),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_line_primary(
    x1: usize,
    y1: usize,
    x2: usize,
    y2: usize,
    coords: &OrientedCoords,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: Option<&Graph>,
    owner: Option<RouteOwner<'_>>,
) {
    let char = coords.primary_edge_char(style);

    match coords.primary {
        crate::orientation::Axis::Horizontal => {
            let (start, end) = if x1 < x2 { (x1, x2) } else { (x2, x1) };
            for x in start..=end {
                if let Some(g) = graph {
                    if is_subgraph_title_cell(g, x, y1) {
                        continue;
                    }
                }
                set_route_edge_char(canvas, x, y1, char, style, owner);
            }
        }
        crate::orientation::Axis::Vertical => {
            let (start, end) = if y1 < y2 { (y1, y2) } else { (y2, y1) };
            for y in start..=end {
                if let Some(g) = graph {
                    if is_subgraph_title_cell(g, x1, y) {
                        continue;
                    }
                }
                set_route_edge_char(canvas, x1, y, char, style, owner);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_line_secondary(
    x1: usize,
    y1: usize,
    x2: usize,
    y2: usize,
    coords: &OrientedCoords,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: Option<&Graph>,
    owner: Option<RouteOwner<'_>>,
) {
    let char = coords.secondary_edge_char(style);

    match coords.secondary {
        crate::orientation::Axis::Horizontal => {
            let (start, end) = if x1 < x2 { (x1, x2) } else { (x2, x1) };
            for x in start..=end {
                if x != x1 && x != x2 {
                    // Skip corners
                    if let Some(g) = graph {
                        if is_subgraph_title_cell(g, x, y1) {
                            continue;
                        }
                    }
                    set_route_edge_char(canvas, x, y1, char, style, owner);
                }
            }
        }
        crate::orientation::Axis::Vertical => {
            let (start, end) = if y1 < y2 { (y1, y2) } else { (y2, y1) };
            for y in start..=end {
                if y != y1 && y != y2 {
                    // Skip corners
                    if let Some(g) = graph {
                        if is_subgraph_title_cell(g, x1, y) {
                            continue;
                        }
                    }
                    set_route_edge_char(canvas, x1, y, char, style, owner);
                }
            }
        }
    }
}

pub(crate) fn is_subgraph_title_cell(graph: &Graph, x: usize, y: usize) -> bool {
    graph.subgraphs.iter().any(|sg| {
        if !sg.has_title() || !sg.bounds.is_valid() {
            return false;
        }
        let title_y = subgraph_title_y(&sg.bounds, graph.direction);
        let Some(title) = sg.title.as_deref() else {
            return false;
        };
        let Some((start_x, end_x)) = title_span(&sg.bounds, title, graph.direction) else {
            return false;
        };
        y == title_y && x >= start_x && x <= end_x
    })
}
