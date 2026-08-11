//! Clone-planned routing for pure four-to-eight-source terminal fan-in.
//!
//! Each source receives a distinct horizontal channel. Channels are ordered
//! monotonically with source order: BT routes left-to-right from the target
//! side toward the source side, while TD mirrors that order. Every route is
//! planned and collision-checked on a cloned canvas before the original canvas
//! is replaced, so a failed proof cannot leave a partial wide scene behind.

use std::collections::HashMap;

use crate::graph::{Direction, Graph, Node};
use crate::orientation::OrientedCoords;
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::wide_terminal_fan_in::{
    required_primary_gap, target_port_columns, target_port_count, target_port_rows,
    WIDE_CHANNEL_PITCH,
};
use super::edge_primitives::{
    draw_line_primary, draw_line_secondary, edge_entry_point, edge_exit_point,
};
use super::{edge_route_owner_id, set_route_char, RouteOwner};

struct PlannedRoute<'a> {
    source: &'a Node,
    target_port: usize,
    source_exit: (usize, usize),
    source_turn: (usize, usize),
    target_turn: (usize, usize),
    target_entry: (usize, usize),
    path: Vec<(usize, usize)>,
}

fn point_with_axes(coords: &OrientedCoords, primary: usize, secondary: usize) -> (usize, usize) {
    let mut x = 0;
    let mut y = 0;
    coords.set_primary(&mut x, &mut y, primary);
    coords.set_secondary(&mut x, &mut y, secondary);
    (x, y)
}

fn primary_path(
    coords: &OrientedCoords,
    secondary: usize,
    first: usize,
    last: usize,
) -> Vec<(usize, usize)> {
    let (start, end) = if first <= last {
        (first, last)
    } else {
        (last, first)
    };
    (start..=end)
        .map(|primary| point_with_axes(coords, primary, secondary))
        .collect()
}

fn secondary_path(
    coords: &OrientedCoords,
    primary: usize,
    first: usize,
    last: usize,
) -> Vec<(usize, usize)> {
    let (start, end) = if first <= last {
        (first, last)
    } else {
        (last, first)
    };
    (start..=end)
        .map(|secondary| point_with_axes(coords, primary, secondary))
        .collect()
}

fn node_contains(node: &Node, x: usize, y: usize) -> bool {
    x >= node.x
        && x < node.x.saturating_add(node.width)
        && y >= node.y
        && y < node.y.saturating_add(node.height)
}

fn route_path(
    coords: &OrientedCoords,
    source_exit: (usize, usize),
    source_turn: (usize, usize),
    target_turn: (usize, usize),
    target_entry: (usize, usize),
) -> Vec<(usize, usize)> {
    let mut path = primary_path(
        coords,
        coords.secondary_coord(source_exit.0, source_exit.1),
        coords.primary_coord(source_exit.0, source_exit.1),
        coords.primary_coord(source_turn.0, source_turn.1),
    );
    path.extend(secondary_path(
        coords,
        coords.primary_coord(source_turn.0, source_turn.1),
        coords.secondary_coord(source_turn.0, source_turn.1),
        coords.secondary_coord(target_turn.0, target_turn.1),
    ));
    path.extend(primary_path(
        coords,
        coords.secondary_coord(target_turn.0, target_turn.1),
        coords.primary_coord(target_turn.0, target_turn.1),
        coords.primary_coord(target_entry.0, target_entry.1),
    ));
    path
}

fn plans_are_disjoint<'a>(
    plans: &[PlannedRoute<'a>],
    canvas: &Canvas,
    target: &Node,
    graph: &Graph,
) -> bool {
    let mut occupied: HashMap<(usize, usize), String> = HashMap::new();
    for plan in plans {
        if plan.path.is_empty() {
            return false;
        }
        let owner_id = edge_route_owner_id(graph, &plan.source.id, &target.id);
        for &(x, y) in &plan.path {
            if x >= canvas.width
                || y >= canvas.height
                || node_contains(target, x, y)
                || graph
                    .nodes
                    .iter()
                    .any(|node| node.id != target.id && node_contains(node, x, y))
                || canvas.get(x, y) != ' '
            {
                return false;
            }
            if let Some(existing_owner) = occupied.get(&(x, y)) {
                if existing_owner != &owner_id {
                    return false;
                }
            } else {
                occupied.insert((x, y), owner_id.clone());
            }
        }
    }
    true
}

fn verify_trial_ownership<'a>(
    trial: &Canvas,
    plans: &[PlannedRoute<'a>],
    target: &Node,
    graph: &Graph,
) -> bool {
    plans.iter().all(|plan| {
        let owner_id = edge_route_owner_id(graph, &plan.source.id, &target.id);
        plan.path.iter().all(|&(x, y)| {
            trial
                .get_meta(x, y)
                .is_some_and(|meta| meta.owner_id.as_deref() == Some(owner_id.as_str()))
        })
    })
}

/// Derive a collision-free numeric order for channel rows.
///
/// A horizontal span can contain another route's source or target column. In
/// that case the other route's vertical leg must stop on the safe side of the
/// span. These relations form a small partial order; a cycle means this
/// orthogonal route shape is not provable and the caller must fall back.
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
                indegree[to] += 1;
            }
        }
    }

    let mut order = Vec::with_capacity(count);
    while order.len() < count {
        let next = (0..count)
            .filter(|candidate| indegree[*candidate] == 0 && !order.contains(candidate))
            .min()?;
        order.push(next);
        for (to, required) in before[next].iter().enumerate() {
            if *required {
                indegree[to] = indegree[to].saturating_sub(1);
            }
        }
    }
    Some(order)
}

/// Route LR/RL wide terminal fan-in directly across one aligned row per
/// source. The target is measured tall enough to expose the same row pitch,
/// so no shared comb or perpendicular channel is necessary.
#[allow(clippy::too_many_arguments)]
fn route_horizontal_wide_terminal_fan_in_edges(
    sources: &[&Node],
    target: &Node,
    canvas: &mut Canvas,
    style: &StyleChars,
    direction: Direction,
    graph: &Graph,
    port_count: usize,
) -> bool {
    let coords = OrientedCoords::new(direction);
    let target_entry_base = edge_entry_point(target, direction);
    let target_rows = target_port_rows(target.y, target.height, port_count);
    if target_rows.len() != port_count {
        return false;
    }

    let mut ordered_sources = sources.to_vec();
    ordered_sources.sort_by_key(|source| {
        let exit = edge_exit_point(source, direction);
        (
            coords.secondary_coord(exit.0, exit.1),
            source.x,
            source.y,
            source.id.clone(),
        )
    });

    let mut plans = Vec::with_capacity(port_count);
    for (source, target_row) in ordered_sources.iter().zip(target_rows) {
        let source_exit = edge_exit_point(source, direction);
        if coords.secondary_coord(source_exit.0, source_exit.1) != target_row {
            return false;
        }

        let target_entry =
            coords.with_secondary(target_entry_base.0, target_entry_base.1, target_row);
        let path = primary_path(
            &coords,
            target_row,
            coords.primary_coord(source_exit.0, source_exit.1),
            coords.primary_coord(target_entry.0, target_entry.1),
        );
        plans.push(PlannedRoute {
            source,
            target_port: target_row,
            source_exit,
            source_turn: source_exit,
            target_turn: target_entry,
            target_entry,
            path,
        });
    }

    if plans
        .windows(2)
        .any(|window| window[0].target_port >= window[1].target_port)
        || !plans_are_disjoint(&plans, canvas, target, graph)
    {
        return false;
    }

    let mut trial = canvas.clone();
    trial.set_write_stage("edge-route-wide-terminal-fan-in-horizontal");
    for plan in &plans {
        let owner_id = edge_route_owner_id(graph, &plan.source.id, &target.id);
        let owner = RouteOwner {
            kind: crate::render::semantic::CellOwnerKind::EdgeSegment,
            id: owner_id.as_str(),
        };
        draw_line_primary(
            plan.source_exit.0,
            plan.source_exit.1,
            plan.target_entry.0,
            plan.target_entry.1,
            &coords,
            &mut trial,
            style,
            Some(graph),
            Some(owner),
        );
        set_route_char(
            &mut trial,
            plan.target_entry.0,
            plan.target_entry.1,
            coords.arrow_end(style),
            Some(owner),
        );
    }

    if !verify_trial_ownership(&trial, &plans, target, graph) {
        return false;
    }

    *canvas = trial;
    true
}

/// Lower a selected wide terminal fan-in, returning `true` only after the
/// complete route set has passed the cloned-canvas proof.
#[allow(clippy::too_many_arguments)]
pub(crate) fn route_wide_terminal_fan_in_edges(
    sources: &[&Node],
    target: &Node,
    canvas: &mut Canvas,
    style: &StyleChars,
    direction: Direction,
    graph: &Graph,
) -> bool {
    let Some(port_count) = target_port_count(graph, &target.id) else {
        return false;
    };
    if sources.len() != port_count {
        return false;
    }

    if matches!(direction, Direction::LR | Direction::RL) {
        return route_horizontal_wide_terminal_fan_in_edges(
            sources, target, canvas, style, direction, graph, port_count,
        );
    }

    let target_ports = target_port_columns(target.x, target.width, port_count);
    if target_ports.len() != port_count {
        return false;
    }

    let mut ordered_sources = sources.to_vec();
    let coords = OrientedCoords::new(direction);
    ordered_sources.sort_by_key(|source| {
        (
            coords.secondary_coord(source.x, source.y),
            source.x,
            source.y,
            source.id.clone(),
        )
    });

    let target_entry_base = edge_entry_point(target, direction);
    let target_primary = coords.primary_coord(target_entry_base.0, target_entry_base.1);
    let source_primary = ordered_sources
        .iter()
        .map(|source| {
            let exit = edge_exit_point(source, direction);
            coords.primary_coord(exit.0, exit.1)
        })
        .collect::<Vec<_>>();
    let Some(reference_source_primary) = source_primary.first().copied() else {
        return false;
    };
    if source_primary
        .iter()
        .any(|primary| *primary != reference_source_primary)
    {
        return false;
    }

    let available = reference_source_primary
        .abs_diff(target_primary)
        .saturating_sub(1);
    if available < port_count || available < required_primary_gap(port_count).saturating_sub(2) {
        return false;
    }
    let flow_is_forward = matches!(direction, Direction::TD);
    if (flow_is_forward && reference_source_primary >= target_primary)
        || (!flow_is_forward && reference_source_primary <= target_primary)
    {
        return false;
    }

    let source_columns = ordered_sources
        .iter()
        .map(|source| {
            let exit = edge_exit_point(source, direction);
            coords.secondary_coord(exit.0, exit.1)
        })
        .collect::<Vec<_>>();
    let Some(channel_order) = channel_order(&source_columns, &target_ports, direction) else {
        return false;
    };
    let mut channel_ranks = vec![0usize; port_count];
    for (rank, route) in channel_order.into_iter().enumerate() {
        channel_ranks[route] = rank;
    }

    let mut plans = Vec::with_capacity(port_count);
    for (index, (source, target_port)) in ordered_sources
        .iter()
        .zip(target_ports.iter().copied())
        .enumerate()
    {
        let source_exit = edge_exit_point(source, direction);
        let source_secondary = coords.secondary_coord(source_exit.0, source_exit.1);
        let channel_rank = channel_ranks[index];
        let channel_offset = channel_rank
            .saturating_add(1)
            .saturating_mul(WIDE_CHANNEL_PITCH);
        let channel_primary = if flow_is_forward {
            reference_source_primary.saturating_add(channel_offset)
        } else {
            target_primary.saturating_add(channel_offset)
        };
        if (flow_is_forward && channel_primary <= reference_source_primary)
            || (!flow_is_forward && channel_primary >= reference_source_primary)
            || channel_primary == target_primary
        {
            return false;
        }

        let source_turn = point_with_axes(&coords, channel_primary, source_secondary);
        let target_turn = point_with_axes(&coords, channel_primary, target_port);
        let target_entry =
            coords.with_secondary(target_entry_base.0, target_entry_base.1, target_port);
        let path = route_path(&coords, source_exit, source_turn, target_turn, target_entry);
        plans.push(PlannedRoute {
            source,
            target_port,
            source_exit,
            source_turn,
            target_turn,
            target_entry,
            path,
        });
    }

    if plans
        .windows(2)
        .any(|window| window[0].target_port >= window[1].target_port)
        || !plans_are_disjoint(&plans, canvas, target, graph)
    {
        return false;
    }

    if crate::runtime::current().diagnostics.fan_in {
        eprintln!(
            "wide fan-in target={} ports={target_ports:?} source_columns={source_columns:?} channel_ranks={channel_ranks:?}",
            target.id
        );
    }

    let mut trial = canvas.clone();
    trial.set_write_stage("edge-route-wide-terminal-fan-in");
    for plan in &plans {
        let owner_id = edge_route_owner_id(graph, &plan.source.id, &target.id);
        let owner = RouteOwner {
            kind: crate::render::semantic::CellOwnerKind::EdgeSegment,
            id: owner_id.as_str(),
        };
        let source_secondary = coords.secondary_coord(plan.source_exit.0, plan.source_exit.1);
        let going_before = source_secondary > plan.target_port;
        draw_line_primary(
            plan.source_exit.0,
            plan.source_exit.1,
            plan.source_turn.0,
            plan.source_turn.1,
            &coords,
            &mut trial,
            style,
            Some(graph),
            Some(owner),
        );
        set_route_char(
            &mut trial,
            plan.source_turn.0,
            plan.source_turn.1,
            coords.corner_start_to_secondary(going_before, style),
            Some(owner),
        );
        draw_line_secondary(
            plan.source_turn.0,
            plan.source_turn.1,
            plan.target_turn.0,
            plan.target_turn.1,
            &coords,
            &mut trial,
            style,
            Some(graph),
            Some(owner),
        );
        draw_line_primary(
            plan.target_turn.0,
            plan.target_turn.1,
            plan.target_entry.0,
            plan.target_entry.1,
            &coords,
            &mut trial,
            style,
            Some(graph),
            Some(owner),
        );
        set_route_char(
            &mut trial,
            plan.target_turn.0,
            plan.target_turn.1,
            coords.corner_secondary_to_end(going_before, style),
            Some(owner),
        );
        set_route_char(
            &mut trial,
            plan.target_entry.0,
            plan.target_entry.1,
            coords.arrow_end(style),
            Some(owner),
        );
    }

    if !verify_trial_ownership(&trial, &plans, target, graph) {
        return false;
    }

    *canvas = trial;
    true
}
