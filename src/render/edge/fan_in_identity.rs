//! One-port-per-edge routing for ordinary, bounded fan-in scenes.
//!
//! The topology gate lives in `render::fan_in_identity` and is shared with
//! measurement.  This lowerer is intentionally conservative: it plans every
//! path, rejects node/border/route collisions, and only then commits a cloned
//! canvas.  Anything outside that small contract remains on the generic
//! convergence path.

use std::collections::HashSet;

use crate::graph::{Direction, Graph, Node};
use crate::orientation::{Axis, OrientedCoords};
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::fallback_route::FallbackRoutePlan;
use super::super::fan_in_identity::{
    is_vertical_branch_rejoin_target, target_port_columns, target_port_count, target_port_rows,
};
use super::super::semantic::CellOwnerKind;
use super::edge_primitives::{
    draw_line_primary, draw_line_secondary, edge_entry_point, edge_exit_point,
};
use super::{edge_route_owner_id, set_route_char, RouteOwner};

struct PlannedRoute<'a> {
    source: &'a Node,
    target: &'a Node,
    source_exit: (usize, usize),
    target_entry: (usize, usize),
    turn: (usize, usize),
    target_turn: (usize, usize),
    target_secondary: usize,
}

/// Lower an ordinary fan-in target with one visible arrowhead per incoming
/// edge.  Returns `true` only after the entire target route set is committed.
/// A rejected plan leaves `canvas` untouched so the caller can use the
/// generic convergence fallback without a partial identity scene behind it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn route_fan_in_identity_edges(
    sources: &[&Node],
    target: &Node,
    canvas: &mut Canvas,
    style: &StyleChars,
    direction: Direction,
    graph: &Graph,
) -> bool {
    route_fan_in_identity_edges_impl(sources, target, canvas, style, direction, graph, false)
}

/// Lower the bounded BT parallel scene's two target entries.  BT layouts can
/// legitimately have one free corridor row between the target and its branch
/// sources; in that case the two horizontal legs may share that row when their
/// spans are disjoint.  The transactional collision check still rejects any
/// real overlap.
#[allow(clippy::too_many_arguments)]
pub(crate) fn route_bt_parallel_identity_edges(
    sources: &[&Node],
    target: &Node,
    canvas: &mut Canvas,
    style: &StyleChars,
    direction: Direction,
    graph: &Graph,
) -> bool {
    route_fan_in_identity_edges_impl(sources, target, canvas, style, direction, graph, true)
}

/// Claim the exact TD/BT branch-rejoin scene before generic convergence.
///
/// The existing identity lowerer is transactional and remains responsible for
/// proving the two target routes.  A recognized branch-rejoin is nevertheless
/// considered claimed when that proof rejects the current geometry, so the
/// caller cannot silently replace the missing identity routes with one shared
/// convergence arrow.  Raw/semantic/geometry QA evidence then exposes the
/// incomplete candidate for repair.
#[allow(clippy::too_many_arguments)]
pub(crate) fn route_vertical_branch_rejoin_identity_edges(
    sources: &[&Node],
    target: &Node,
    canvas: &mut Canvas,
    style: &StyleChars,
    direction: Direction,
    graph: &Graph,
) -> bool {
    if !is_vertical_branch_rejoin_target(graph, &target.id) {
        return false;
    }

    let _routed =
        route_fan_in_identity_edges_impl(sources, target, canvas, style, direction, graph, false);
    true
}

#[allow(clippy::too_many_arguments)]
fn route_fan_in_identity_edges_impl(
    sources: &[&Node],
    target: &Node,
    canvas: &mut Canvas,
    style: &StyleChars,
    direction: Direction,
    graph: &Graph,
    bt_shared_turn: bool,
) -> bool {
    let Some(port_count) = target_port_count(graph, &target.id) else {
        return false;
    };
    if sources.len() != port_count {
        return false;
    }

    let target_ports = match direction {
        Direction::TD | Direction::TB | Direction::BT => {
            target_port_columns(target.x, target.width, port_count)
        }
        Direction::LR | Direction::RL => target_port_rows(target.y, target.height, port_count),
    };
    if target_ports.len() != port_count {
        return false;
    }

    let coords = OrientedCoords::new(direction);
    let mut ordered_sources = sources.to_vec();
    ordered_sources.sort_by(|left, right| {
        coords
            .secondary_coord(left.center_x(), left.center_y())
            .cmp(&coords.secondary_coord(right.center_x(), right.center_y()))
            .then_with(|| left.x.cmp(&right.x))
            .then_with(|| left.y.cmp(&right.y))
            .then_with(|| left.id.cmp(&right.id))
    });

    let target_base = edge_entry_point(target, direction);
    let target_primary = coords.primary_coord(target_base.0, target_base.1);
    let channel_ranks = if matches!(direction, Direction::TD | Direction::TB | Direction::BT) {
        let source_columns: Vec<usize> = ordered_sources
            .iter()
            .map(|source| {
                let exit = edge_exit_point(source, direction);
                coords.secondary_coord(exit.0, exit.1)
            })
            .collect();
        let Some(order) = channel_order(&source_columns, &target_ports, direction) else {
            return false;
        };
        let mut ranks = vec![0usize; port_count];
        for (rank, route) in order.into_iter().enumerate() {
            ranks[route] = rank;
        }
        Some(ranks)
    } else {
        None
    };
    let mut plans = Vec::with_capacity(port_count);
    let mut reserved_cells = HashSet::new();

    for (index, (source, target_secondary)) in ordered_sources
        .iter()
        .zip(target_ports.iter().copied())
        .enumerate()
    {
        let source_exit = edge_exit_point(source, direction);
        let source_primary = coords.primary_coord(source_exit.0, source_exit.1);
        let distance = source_primary.abs_diff(target_primary);
        if distance < 2 || !flows_toward_target(direction, source_primary, target_primary) {
            return false;
        }

        let target_entry = coords.with_secondary(target_base.0, target_base.1, target_secondary);
        let turn = match direction {
            Direction::TD | Direction::TB | Direction::BT => {
                let lane_rank = channel_ranks
                    .as_ref()
                    .and_then(|ranks| ranks.get(index).copied())
                    .unwrap_or(index);
                let lane_offset = if direction == Direction::BT && bt_shared_turn {
                    1
                } else {
                    1usize.saturating_add(lane_rank.saturating_mul(2))
                };
                let lane_primary = if direction == Direction::BT {
                    target_primary.saturating_add(lane_offset)
                } else {
                    source_primary.saturating_add(lane_offset)
                };
                let mut x = source_exit.0;
                let mut y = source_exit.1;
                coords.set_primary(&mut x, &mut y, lane_primary);
                (x, y)
            }
            Direction::LR | Direction::RL => {
                let turn_primary = match direction {
                    Direction::LR => target_primary
                        .saturating_sub(2)
                        .max(source_primary.saturating_add(1)),
                    Direction::RL => target_primary
                        .saturating_add(2)
                        .min(source_primary.saturating_sub(1)),
                    _ => unreachable!("horizontal identity route has horizontal direction"),
                };
                let mut x = source_exit.0;
                let mut y = source_exit.1;
                coords.set_primary(&mut x, &mut y, turn_primary);
                (x, y)
            }
        };
        let target_turn = coords.with_secondary(turn.0, turn.1, target_secondary);
        let route_cells = route_cells(source_exit, turn, target_turn, target_entry, &coords);

        let collision = route_cells.iter().find(|point| {
            !safe_route_cell(**point, canvas, graph) || !reserved_cells.insert(**point)
        });
        if collision.is_some() {
            return false;
        }

        plans.push(PlannedRoute {
            source,
            target,
            source_exit,
            target_entry,
            turn,
            target_turn,
            target_secondary,
        });
    }

    canvas.set_write_stage("edge-route-fan-in-identity");
    let mut trial = canvas.clone();
    trial.set_write_stage("edge-route-fan-in-identity");
    for plan in &plans {
        paint_route(plan, &mut trial, style, graph, &coords);
    }
    let mut fallback =
        FallbackRoutePlan::new(format!("fan-in-identity:{}", target.id), "fan-in-identity");
    fallback.set_scene_coverage(
        plans
            .iter()
            .map(|plan| edge_route_owner_id(graph, &plan.source.id, &plan.target.id)),
    );
    for plan in &plans {
        if has_shared_source_stem(graph, plan.source, target) {
            mark_shared_source_prefix(&mut fallback, plan, &coords);
        }
        record_identity_route_evidence(&mut fallback, plan, &coords, style);
    }
    trial.record_fallback_route_evidence(fallback);
    *canvas = trial;
    true
}

fn has_shared_source_stem(graph: &Graph, source: &Node, target: &Node) -> bool {
    graph
        .edges
        .iter()
        .any(|edge| !edge.is_back_edge && edge.from == source.id && edge.to != target.id)
}

fn mark_shared_source_prefix(
    fallback: &mut FallbackRoutePlan,
    plan: &PlannedRoute<'_>,
    coords: &OrientedCoords,
) {
    let source_secondary = coords.secondary_coord(plan.source_exit.0, plan.source_exit.1);
    let end = if source_secondary == plan.target_secondary {
        plan.source_exit
    } else {
        plan.turn
    };
    let start_primary = coords.primary_coord(plan.source_exit.0, plan.source_exit.1);
    let end_primary = coords.primary_coord(end.0, end.1);
    let range = if start_primary <= end_primary {
        Box::new(start_primary..=end_primary) as Box<dyn Iterator<Item = usize>>
    } else {
        Box::new((end_primary..=start_primary).rev()) as Box<dyn Iterator<Item = usize>>
    };
    for primary in range {
        let mut point = plan.source_exit;
        coords.set_primary(&mut point.0, &mut point.1, primary);
        fallback.allow_shared_cell(point.0, point.1);
    }
}

/// Derive a collision-free channel order for the four-port vertical route.
///
/// A horizontal lane must not pass through another source's vertical stem or
/// another target port's final stem.  The resulting ordering is a small
/// partial order; a cycle fails closed and leaves the generic lowerer in
/// control.
fn channel_order(
    source_columns: &[usize],
    target_ports: &[usize],
    direction: Direction,
) -> Option<Vec<usize>> {
    let count = source_columns.len();
    if count == 0 || target_ports.len() != count {
        return None;
    }

    let mut before = vec![vec![false; count]; count];
    let mut add_relation = |from: usize, to: usize| {
        if from != to {
            before[from][to] = true;
        }
    };
    let contains = |start: usize, end: usize, column: usize| {
        column >= start.min(end) && column <= start.max(end)
    };

    for route in 0..count {
        let start = source_columns[route].min(target_ports[route]);
        let end = source_columns[route].max(target_ports[route]);
        for other in 0..count {
            if route == other {
                continue;
            }
            if contains(start, end, source_columns[other]) {
                match direction {
                    Direction::BT => add_relation(route, other),
                    Direction::TD | Direction::TB => add_relation(other, route),
                    Direction::LR | Direction::RL => return None,
                }
            }
            if contains(start, end, target_ports[other]) {
                match direction {
                    Direction::BT => add_relation(other, route),
                    Direction::TD | Direction::TB => add_relation(route, other),
                    Direction::LR | Direction::RL => return None,
                }
            }
        }
    }

    let mut indegree = vec![0usize; count];
    for row in &before {
        for (to, required) in row.iter().enumerate() {
            if *required {
                indegree[to] = indegree[to].saturating_add(1);
            }
        }
    }

    let mut order = Vec::with_capacity(count);
    while order.len() < count {
        let next =
            (0..count).find(|candidate| indegree[*candidate] == 0 && !order.contains(candidate))?;
        order.push(next);
        for (to, required) in before[next].iter().enumerate() {
            if *required {
                indegree[to] = indegree[to].saturating_sub(1);
            }
        }
    }
    Some(order)
}

fn flows_toward_target(direction: Direction, source_primary: usize, target_primary: usize) -> bool {
    match direction {
        Direction::TD | Direction::TB | Direction::LR => source_primary < target_primary,
        Direction::BT | Direction::RL => source_primary > target_primary,
    }
}

fn route_cells(
    source_exit: (usize, usize),
    turn: (usize, usize),
    target_turn: (usize, usize),
    target_entry: (usize, usize),
    coords: &OrientedCoords,
) -> Vec<(usize, usize)> {
    let mut cells = Vec::new();
    append_primary_line(&mut cells, source_exit, turn, coords);
    append_secondary_line(&mut cells, turn, target_turn, coords);
    append_primary_line(&mut cells, target_turn, target_entry, coords);
    cells
}

fn append_primary_line(
    cells: &mut Vec<(usize, usize)>,
    start: (usize, usize),
    end: (usize, usize),
    coords: &OrientedCoords,
) {
    let start_primary = coords.primary_coord(start.0, start.1);
    let end_primary = coords.primary_coord(end.0, end.1);
    if start_primary <= end_primary {
        for primary in start_primary..=end_primary {
            let mut point = start;
            coords.set_primary(&mut point.0, &mut point.1, primary);
            push_unique(cells, point);
        }
    } else {
        for primary in (end_primary..=start_primary).rev() {
            let mut point = start;
            coords.set_primary(&mut point.0, &mut point.1, primary);
            push_unique(cells, point);
        }
    }
}

fn append_secondary_line(
    cells: &mut Vec<(usize, usize)>,
    start: (usize, usize),
    end: (usize, usize),
    coords: &OrientedCoords,
) {
    let start_secondary = coords.secondary_coord(start.0, start.1);
    let end_secondary = coords.secondary_coord(end.0, end.1);
    if start_secondary <= end_secondary {
        for secondary in start_secondary..=end_secondary {
            let point = coords.with_secondary(start.0, start.1, secondary);
            push_unique(cells, point);
        }
    } else {
        for secondary in (end_secondary..=start_secondary).rev() {
            let point = coords.with_secondary(start.0, start.1, secondary);
            push_unique(cells, point);
        }
    }
}

fn push_unique(cells: &mut Vec<(usize, usize)>, point: (usize, usize)) {
    if cells.last().copied() != Some(point) {
        cells.push(point);
    }
}

fn safe_route_cell(point: (usize, usize), canvas: &Canvas, graph: &Graph) -> bool {
    let (x, y) = point;
    x < canvas.width
        && y < canvas.height
        && canvas.get(x, y) == ' '
        && !graph.nodes.iter().any(|node| {
            x >= node.x
                && x < node.x.saturating_add(node.width)
                && y >= node.y
                && y < node.bottom_y()
        })
}

#[allow(clippy::too_many_arguments)]
fn paint_route(
    plan: &PlannedRoute<'_>,
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: &Graph,
    coords: &OrientedCoords,
) {
    let owner_id = edge_route_owner_id(graph, &plan.source.id, &plan.target.id);
    let owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: owner_id.as_str(),
    };

    let source_secondary = coords.secondary_coord(plan.source_exit.0, plan.source_exit.1);
    if source_secondary == plan.target_secondary {
        draw_line_primary(
            plan.source_exit.0,
            plan.source_exit.1,
            plan.target_entry.0,
            plan.target_entry.1,
            coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
    } else {
        // Draw the straight legs first. The line helpers include their
        // endpoints and can therefore temporarily resolve a corner-plus-line
        // as a tee; the explicit corner glyphs below are authoritative for
        // this collision-free route plan.
        draw_line_primary(
            plan.source_exit.0,
            plan.source_exit.1,
            plan.turn.0,
            plan.turn.1,
            coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
        let going_before = source_secondary > plan.target_secondary;
        draw_line_secondary(
            plan.turn.0,
            plan.turn.1,
            plan.target_turn.0,
            plan.target_turn.1,
            coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
        let final_start = coords.advance(plan.target_turn.0, plan.target_turn.1, 1);
        draw_line_primary(
            final_start.0,
            final_start.1,
            plan.target_entry.0,
            plan.target_entry.1,
            coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
        set_route_char(
            canvas,
            plan.turn.0,
            plan.turn.1,
            coords.corner_start_to_secondary(going_before, style),
            Some(owner),
        );
        set_route_char(
            canvas,
            plan.target_turn.0,
            plan.target_turn.1,
            coords.corner_secondary_to_end(going_before, style),
            Some(owner),
        );
    }

    set_route_char(
        canvas,
        plan.target_entry.0,
        plan.target_entry.1,
        coords.arrow_end(style),
        Some(owner),
    );
}

/// Record the same primitive route that `paint_route` just lowered.  Keeping
/// this beside the lowerer is intentional: evidence must describe the live
/// topology transaction, not reconstruct a route later from node centers.
fn record_identity_route_evidence(
    fallback: &mut FallbackRoutePlan,
    plan: &PlannedRoute<'_>,
    coords: &OrientedCoords,
    style: &StyleChars,
) {
    let source_secondary = coords.secondary_coord(plan.source_exit.0, plan.source_exit.1);
    if source_secondary == plan.target_secondary {
        push_primary_segment(fallback, plan.source_exit, plan.target_entry, coords, style);
    } else {
        push_primary_segment(fallback, plan.source_exit, plan.turn, coords, style);
        push_secondary_segment(fallback, plan.turn, plan.target_turn, coords, style);
        let final_start = coords.advance(plan.target_turn.0, plan.target_turn.1, 1);
        push_primary_segment(fallback, final_start, plan.target_entry, coords, style);
        let going_before = source_secondary > plan.target_secondary;
        fallback.push_corner(
            plan.turn.0,
            plan.turn.1,
            coords.corner_start_to_secondary(going_before, style),
        );
        fallback.push_corner(
            plan.target_turn.0,
            plan.target_turn.1,
            coords.corner_secondary_to_end(going_before, style),
        );
    }
    fallback.push_paint(
        plan.target_entry.0,
        plan.target_entry.1,
        coords.arrow_end(style),
    );
}

fn push_primary_segment(
    fallback: &mut FallbackRoutePlan,
    from: (usize, usize),
    to: (usize, usize),
    coords: &OrientedCoords,
    style: &StyleChars,
) {
    match coords.primary {
        Axis::Horizontal => {
            fallback.push_horizontal(from.1, from.0, to.0, coords.primary_edge_char(style))
        }
        Axis::Vertical => {
            fallback.push_vertical(from.0, from.1, to.1, coords.primary_edge_char(style))
        }
    }
}

fn push_secondary_segment(
    fallback: &mut FallbackRoutePlan,
    from: (usize, usize),
    to: (usize, usize),
    coords: &OrientedCoords,
    style: &StyleChars,
) {
    match coords.secondary {
        Axis::Horizontal => {
            fallback.push_horizontal(from.1, from.0, to.0, coords.secondary_edge_char(style))
        }
        Axis::Vertical => {
            fallback.push_vertical(from.0, from.1, to.1, coords.secondary_edge_char(style))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{route_bt_parallel_identity_edges, route_fan_in_identity_edges};
    use crate::graph::{Direction, Edge, Graph, Node};
    use crate::render::canvas::Canvas;
    use crate::style::{ASCII_CHARS, UNICODE_CHARS};

    fn node(id: &str, x: usize, y: usize, width: usize, height: usize) -> Node {
        let mut node = Node::new(id, id);
        node.x = x;
        node.y = y;
        node.width = width;
        node.height = height;
        node
    }

    #[test]
    fn routes_two_horizontal_sources_to_distinct_target_ports() {
        let mut graph = Graph::new();
        graph.direction = Direction::LR;
        graph.nodes.push(node("A", 0, 0, 8, 3));
        graph.nodes.push(node("B", 0, 4, 8, 3));
        graph.nodes.push(node("T", 14, 1, 13, 5));
        graph.edges.push(Edge::new("A", "T"));
        graph.edges.push(Edge::new("B", "T"));

        assert_eq!(
            crate::render::fan_in_identity::target_port_count(&graph, "T"),
            Some(2)
        );
        assert_eq!(
            crate::render::fan_in_identity::target_port_rows(1, 5, 2),
            vec![2, 4]
        );

        let sources = vec![&graph.nodes[0], &graph.nodes[1]];
        let mut canvas = Canvas::new(40, 20);
        assert!(route_fan_in_identity_edges(
            &sources,
            &graph.nodes[2],
            &mut canvas,
            &ASCII_CHARS,
            graph.direction,
            &graph,
        ));
        assert_eq!(canvas.get(13, 2), ASCII_CHARS.arrow_right);
        assert_eq!(canvas.get(13, 4), ASCII_CHARS.arrow_right);

        let traces = canvas.fallback_route_traces();
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].strategy, "fan-in-identity");
        assert_eq!(traces[0].covered_edge_ids.len(), 2);
        assert!(
            traces[0].mismatches.is_empty(),
            "fan-in identity evidence must match the painted route: {:?}",
            traces[0].mismatches
        );
    }

    #[test]
    fn bt_parallel_identity_uses_disjoint_shared_corridor_legs() {
        let mut graph = Graph::new();
        graph.direction = Direction::BT;
        graph.nodes.push(node("B", 4, 15, 12, 3));
        graph.nodes.push(node("C", 20, 15, 12, 3));
        graph.nodes.push(node("D", 14, 9, 9, 3));
        graph.edges.push(Edge::new("B", "D"));
        graph.edges.push(Edge::new("C", "D"));

        let sources = vec![&graph.nodes[0], &graph.nodes[1]];
        let mut canvas = Canvas::new(40, 25);
        assert!(route_bt_parallel_identity_edges(
            &sources,
            &graph.nodes[2],
            &mut canvas,
            &UNICODE_CHARS,
            graph.direction,
            &graph,
        ));

        assert_eq!(canvas.get(17, 12), UNICODE_CHARS.arrow_up);
        assert_eq!(canvas.get(19, 12), UNICODE_CHARS.arrow_up);
        assert_eq!(canvas.get(10, 13), UNICODE_CHARS.corner_dl);
        assert_eq!(canvas.get(26, 13), UNICODE_CHARS.corner_dr);
        assert_eq!(canvas.get(17, 13), UNICODE_CHARS.corner_ur);
        assert_eq!(canvas.get(19, 13), UNICODE_CHARS.corner_ul);
    }

    #[test]
    fn routes_a_downstream_cascade_target_without_shared_arrow() {
        let mut graph = Graph::new();
        graph.direction = Direction::LR;
        graph.nodes.push(node("M1", 14, 1, 13, 5));
        graph.nodes.push(node("M2", 14, 9, 13, 5));
        graph.nodes.push(node("F", 33, 5, 11, 5));
        graph.edges.push(Edge::new("M1", "F"));
        graph.edges.push(Edge::new("M2", "F"));

        assert_eq!(
            crate::render::fan_in_identity::target_port_count(&graph, "F"),
            Some(2)
        );

        let sources = vec![&graph.nodes[0], &graph.nodes[1]];
        let mut canvas = Canvas::new(50, 20);
        assert!(route_fan_in_identity_edges(
            &sources,
            &graph.nodes[2],
            &mut canvas,
            &ASCII_CHARS,
            graph.direction,
            &graph,
        ));
        assert_eq!(canvas.get(32, 6), ASCII_CHARS.arrow_right);
        assert_eq!(canvas.get(32, 8), ASCII_CHARS.arrow_right);
    }

    #[test]
    fn routes_four_sources_to_distinct_target_ports_in_all_directions() {
        for direction in [
            Direction::TD,
            Direction::TB,
            Direction::BT,
            Direction::LR,
            Direction::RL,
        ] {
            let mut graph = Graph::new();
            graph.direction = direction;
            match direction {
                Direction::TD | Direction::TB => {
                    for (index, x) in [0, 21, 44, 65].into_iter().enumerate() {
                        graph.nodes.push(node(&format!("S{index}"), x, 0, 11, 3));
                    }
                    graph.nodes.push(node("T", 32, 13, 10, 3));
                }
                Direction::BT => {
                    for (index, x) in [0, 21, 44, 65].into_iter().enumerate() {
                        graph.nodes.push(node(&format!("S{index}"), x, 12, 11, 3));
                    }
                    graph.nodes.push(node("T", 32, 0, 10, 3));
                }
                Direction::LR => {
                    for (index, y) in [0, 2, 4, 6].into_iter().enumerate() {
                        graph.nodes.push(node(&format!("S{index}"), 0, y, 8, 1));
                    }
                    graph.nodes.push(node("T", 20, 0, 10, 10));
                }
                Direction::RL => {
                    for (index, y) in [0, 2, 4, 6].into_iter().enumerate() {
                        graph.nodes.push(node(&format!("S{index}"), 30, y, 8, 1));
                    }
                    graph.nodes.push(node("T", 10, 0, 10, 10));
                }
            }
            for index in 0..4 {
                graph.edges.push(Edge::new(format!("S{index}"), "T"));
            }

            let sources: Vec<&Node> = graph.nodes[..4].iter().collect();
            let target = graph.nodes.last().expect("four-port target");
            let mut canvas = Canvas::new(100, 30);
            assert!(
                route_fan_in_identity_edges(
                    &sources,
                    target,
                    &mut canvas,
                    &ASCII_CHARS,
                    direction,
                    &graph,
                ),
                "four-port route should be provable for {direction:?}"
            );

            let arrow = match direction {
                Direction::TD | Direction::TB => ASCII_CHARS.arrow_down,
                Direction::BT => ASCII_CHARS.arrow_up,
                Direction::LR => ASCII_CHARS.arrow_right,
                Direction::RL => ASCII_CHARS.arrow_left,
            };
            let target_points: Vec<(usize, usize)> = match direction {
                Direction::TD | Direction::TB | Direction::BT => {
                    let columns = crate::render::fan_in_identity::target_port_columns(
                        target.x,
                        target.width,
                        4,
                    );
                    let y = if direction == Direction::BT {
                        target.bottom_y()
                    } else {
                        target.y.saturating_sub(1)
                    };
                    columns.into_iter().map(|x| (x, y)).collect()
                }
                Direction::LR => {
                    crate::render::fan_in_identity::target_port_rows(target.y, target.height, 4)
                        .into_iter()
                        .map(|y| (target.x.saturating_sub(1), y))
                        .collect()
                }
                Direction::RL => {
                    crate::render::fan_in_identity::target_port_rows(target.y, target.height, 4)
                        .into_iter()
                        .map(|y| (target.x.saturating_add(target.width), y))
                        .collect()
                }
            };
            assert_eq!(
                target_points
                    .iter()
                    .filter(|(x, y)| canvas.get(*x, *y) == arrow)
                    .count(),
                4,
                "all four target ports should have visible arrowheads for {direction:?}"
            );
        }
    }
}
