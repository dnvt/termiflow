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

/// Whether a box has two centre columns a vertical edge could use.
///
/// An even-width box has two centre columns, `center_x() - 1` and
/// `center_x()`. A trunk running down the left one lands one column beside
/// a branch on the right one (`┴┬`); the box can take that branch on its
/// left centre column instead, as long as its entry and exit stay on one
/// column.
fn has_left_centre_port(node: &Node, graph: &Graph) -> bool {
    use crate::graph::NodeShape;

    matches!(
        graph.direction,
        Direction::TD | Direction::TB | Direction::BT
    ) && matches!(
        node.shape,
        NodeShape::Rectangle | NodeShape::Rounded | NodeShape::Stadium | NodeShape::Subroutine
    ) && node.width >= 4
        && node.width.is_multiple_of(2)
        && !graph
            .edges
            .iter()
            .any(|e| e.is_back_edge && (e.from == node.id || e.to == node.id))
}

fn non_back_edges<'a>(
    graph: &'a Graph,
    keep: impl Fn(&crate::graph::Edge) -> bool + 'a,
) -> impl Iterator<Item = &'a crate::graph::Edge> + 'a {
    graph
        .edges
        .iter()
        .filter(move |e| !e.is_back_edge && keep(e))
}

/// Whether a fan-in source may leave on its left centre column.
///
/// Convergence draws before fan-outs, so a source with an incoming edge
/// moves only when that edge is a plain fan-out branch the generic fan-out
/// router will also take on the left column (see [`fan_out_target_uses_left_port`]).
pub(super) fn fan_in_source_uses_left_port(source: &Node, graph: &Graph) -> bool {
    if !has_left_centre_port(source, graph)
        || non_back_edges(graph, |e| e.from == source.id).count() != 1
    {
        return false;
    }
    let incoming: Vec<_> = non_back_edges(graph, |e| e.to == source.id).collect();
    match incoming.as_slice() {
        [] => true,
        [edge] => {
            let Some(from) = graph.get_node(&edge.from) else {
                return false;
            };
            let targets: Vec<_> = non_back_edges(graph, |e| e.from == from.id)
                .filter_map(|e| graph.get_node_subgraph(&e.to))
                .collect();
            let fan_out = non_back_edges(graph, |e| e.from == from.id).count();
            // Fan-outs into one foreign subgraph use their own router.
            let subgraph_router = targets.len() == fan_out
                && targets
                    .iter()
                    .all(|sg| Some(*sg) == targets.first().copied())
                && graph.get_node_subgraph(&from.id) != targets.first().copied();
            edge.label.is_none()
                && fan_out > 1
                && !subgraph_router
                && from.center_x() + 1 == source.center_x()
        }
        _ => false,
    }
}

/// Whether a fan-out target may take its branch on its left centre column.
///
/// The target's exit, if any, must already be drawn there.
pub(super) fn fan_out_target_uses_left_port(
    from: &Node,
    target: &Node,
    canvas: &Canvas,
    style: &StyleChars,
    graph: &Graph,
) -> bool {
    if !has_left_centre_port(target, graph) {
        return false;
    }
    let incoming: Vec<_> = non_back_edges(graph, |e| e.to == target.id).collect();
    let [edge] = incoming.as_slice() else {
        return false;
    };
    if edge.from != from.id || edge.label.is_some() {
        return false;
    }
    match non_back_edges(graph, |e| e.from == target.id).count() {
        0 => true,
        1 => {
            let exit_y = match graph.direction {
                Direction::BT => target.y.checked_sub(1),
                _ => Some(target.y + target.height),
            };
            let leaves = |x: usize| {
                exit_y.is_some_and(|y| {
                    x < canvas.width && y < canvas.height && {
                        let c = canvas.get(x, y);
                        super::super::canvas::is_vertical(c, style)
                            || super::super::canvas::is_junction(c, style)
                    }
                })
            };
            let centre = target.center_x();
            leaves(centre - 1) && !leaves(centre)
        }
        _ => false,
    }
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
                    if is_subgraph_title_text_cell(g, x1, y) {
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
                        if is_subgraph_title_text_cell(g, x1, y) {
                            continue;
                        }
                    }
                    set_route_edge_char(canvas, x1, y, char, style, owner);
                }
            }
        }
    }
}

/// A visible title character (not the padding space around it). A vertical
/// line may pass through the padding beside a title; skipping that cell would
/// leave a gap in the line.
fn is_subgraph_title_text_cell(graph: &Graph, x: usize, y: usize) -> bool {
    graph.subgraphs.iter().any(|sg| {
        let (Some(title), true) = (sg.title.as_deref(), sg.bounds.is_valid()) else {
            return false;
        };
        y == subgraph_title_y(&sg.bounds, graph.direction)
            && crate::graph::subgraph_title_text_span(
                sg.bounds.x,
                sg.bounds.width,
                title,
                graph.direction,
            )
            .is_some_and(|(start, end)| x >= start && x <= end)
    })
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
