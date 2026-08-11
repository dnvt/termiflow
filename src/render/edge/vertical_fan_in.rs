//! One-port-per-edge routing for the bounded vertical fan-in scene.
//!
//! The topology policy lives in `render::vertical_fan_in` and is shared with
//! measurement.  This lowerer plans every route before painting any cell so a
//! rejected geometry cannot leave a partial scene behind.

use crate::graph::{Direction, Graph, Node};
use crate::orientation::OrientedCoords;
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::dual_junction::target_port_count as dual_junction_target_port_count;
use super::super::vertical_fan_in::{
    nonterminal_target_port_count, target_port_columns, target_port_count,
};
use super::edge_primitives::{
    draw_line_primary, draw_line_secondary, edge_entry_point, edge_exit_point,
};
use super::{edge_route_owner_id, set_route_char, set_route_edge_char, RouteOwner};

struct PlannedRoute<'a> {
    source: &'a Node,
    target_port: usize,
    source_exit: (usize, usize),
    target_entry: (usize, usize),
    turn: (usize, usize),
}

/// Lower a selected vertical fan-in with one visible arrowhead per edge.
///
/// Returns `true` only after the complete route set has been validated and
/// painted.  Returning `false` leaves the canvas untouched and lets the
/// conservative generic convergence route handle unexpected geometry.
#[allow(clippy::too_many_arguments)]
pub(crate) fn route_vertical_fan_in_edges(
    sources: &[&Node],
    target: &Node,
    canvas: &mut Canvas,
    style: &StyleChars,
    direction: Direction,
    graph: &Graph,
) -> bool {
    let Some(port_count) = target_port_count(graph, &target.id)
        .or_else(|| nonterminal_target_port_count(graph, &target.id))
        .or_else(|| dual_junction_target_port_count(graph, &target.id))
    else {
        return false;
    };
    if !matches!(direction, Direction::TD | Direction::BT) || sources.len() != port_count {
        return false;
    }

    let target_ports = target_port_columns(target.x, target.width, port_count);
    if target_ports.len() != port_count {
        return false;
    }

    let mut ordered_sources = sources.to_vec();
    ordered_sources.sort_by_key(|source| (source.center_x(), source.x, source.id.clone()));
    let target_entry_base = edge_entry_point(target, direction);
    let coords = OrientedCoords::new(direction);
    let target_primary = coords.primary_coord(target_entry_base.0, target_entry_base.1);

    let mut plans = Vec::with_capacity(port_count);
    let mut horizontal_spans: Vec<(usize, usize)> = Vec::new();
    let mut source_columns = Vec::new();

    for (source, target_port) in ordered_sources.iter().zip(target_ports.iter().copied()) {
        let source_exit = edge_exit_point(source, direction);
        let source_primary = coords.primary_coord(source_exit.0, source_exit.1);
        let distance = source_primary.abs_diff(target_primary);
        if distance < 2 {
            return false;
        }

        let flow_is_forward = matches!(direction, Direction::TD);
        if (flow_is_forward && source_primary >= target_primary)
            || (!flow_is_forward && source_primary <= target_primary)
        {
            return false;
        }

        let target_entry =
            coords.with_secondary(target_entry_base.0, target_entry_base.1, target_port);
        let turn = coords.advance(source_exit.0, source_exit.1, 1);
        let source_column = coords.secondary_coord(source_exit.0, source_exit.1);
        source_columns.push(source_column);

        if source_column != target_port {
            let span = (
                source_column.min(target_port),
                source_column.max(target_port),
            );
            if horizontal_spans
                .iter()
                .any(|(start, end)| span.0 <= *end && *start <= span.1)
            {
                return false;
            }
            horizontal_spans.push(span);
        }

        if [source_exit, target_entry, turn]
            .iter()
            .any(|(x, y)| *x >= canvas.width || *y >= canvas.height)
        {
            return false;
        }

        plans.push(PlannedRoute {
            source,
            target_port,
            source_exit,
            target_entry,
            turn,
        });
    }

    if source_columns
        .windows(2)
        .any(|columns| columns[0] == columns[1])
    {
        return false;
    }

    canvas.set_write_stage("edge-route-vertical-fan-in");
    for plan in plans {
        let owner_id = edge_route_owner_id(graph, &plan.source.id, &target.id);
        let owner = RouteOwner {
            kind: crate::render::semantic::CellOwnerKind::EdgeSegment,
            id: owner_id.as_str(),
        };
        let source_column = coords.secondary_coord(plan.source_exit.0, plan.source_exit.1);
        if source_column == plan.target_port {
            draw_line_primary(
                plan.source_exit.0,
                plan.source_exit.1,
                plan.target_entry.0,
                plan.target_entry.1,
                &coords,
                canvas,
                style,
                Some(graph),
                Some(owner),
            );
        } else {
            let going_before = source_column > plan.target_port;
            draw_line_primary(
                plan.source_exit.0,
                plan.source_exit.1,
                plan.turn.0,
                plan.turn.1,
                &coords,
                canvas,
                style,
                Some(graph),
                Some(owner),
            );
            set_route_edge_char(
                canvas,
                plan.turn.0,
                plan.turn.1,
                coords.corner_start_to_secondary(going_before, style),
                style,
                Some(owner),
            );

            let target_turn = coords.with_secondary(plan.turn.0, plan.turn.1, plan.target_port);
            draw_line_secondary(
                plan.turn.0,
                plan.turn.1,
                target_turn.0,
                target_turn.1,
                &coords,
                canvas,
                style,
                Some(graph),
                Some(owner),
            );
            set_route_edge_char(
                canvas,
                target_turn.0,
                target_turn.1,
                coords.corner_secondary_to_end(going_before, style),
                style,
                Some(owner),
            );
            draw_line_primary(
                target_turn.0,
                target_turn.1,
                plan.target_entry.0,
                plan.target_entry.1,
                &coords,
                canvas,
                style,
                Some(graph),
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
    true
}
