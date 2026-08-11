//! One-port-per-edge horizontal fan-in routing.
//!
//! This is a deliberately small scene lowerer.  It is selected only by the
//! topology policy in [`crate::render::dedicated_fan_in`]; ordinary fan-in
//! continues to use the shared merge-bar implementation in `convergence`.

use crate::graph::{Direction, Graph, Node};
use crate::orientation::OrientedCoords;
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::dedicated_fan_in::{target_port_count, target_port_rows};
use super::super::dual_junction::target_port_count as dual_junction_target_port_count;
use super::super::semantic::CellOwnerKind;
use super::edge_primitives::{draw_line_primary, draw_line_secondary, edge_exit_point};
use super::{edge_route_owner_id, set_route_char, set_route_edge_char, RouteOwner};

/// Route a selected fan-in target with one visible arrowhead per declared edge.
///
/// Returns `true` when the target was claimed by the dedicated scene, even if
/// no individual path could be emitted.  The caller then keeps the scene
/// fail-closed instead of silently drawing the same edge through the generic
/// shared merge route as well.
#[allow(clippy::too_many_arguments)]
pub(crate) fn route_dedicated_fan_in_edges(
    sources: &[&Node],
    target: &Node,
    canvas: &mut Canvas,
    style: &StyleChars,
    direction: Direction,
    graph: &Graph,
) -> bool {
    let Some(port_count) = target_port_count(graph, &target.id)
        .or_else(|| dual_junction_target_port_count(graph, &target.id))
    else {
        return false;
    };
    if !matches!(direction, Direction::LR | Direction::RL)
        || sources.is_empty()
        || sources.len() != port_count
    {
        return true;
    }

    let rows = target_port_rows(target.y, target.height, port_count);
    if rows.len() != sources.len() {
        return true;
    }

    let mut ordered_sources = sources.to_vec();
    ordered_sources.sort_by_key(|source| (source.center_y(), source.x, source.id.clone()));

    let mut routed = true;
    for (source, target_row) in ordered_sources.into_iter().zip(rows) {
        routed &= route_one_edge(source, target, target_row, canvas, style, direction, graph);
    }
    routed
}

#[allow(clippy::too_many_arguments)]
fn route_one_edge(
    source: &Node,
    target: &Node,
    target_row: usize,
    canvas: &mut Canvas,
    style: &StyleChars,
    direction: Direction,
    graph: &Graph,
) -> bool {
    let coords = OrientedCoords::new(direction);
    let owner_id = edge_route_owner_id(graph, &source.id, &target.id);
    let owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: owner_id.as_str(),
    };
    let source_exit = edge_exit_point(source, direction);
    let (target_arrow_x, _) = match direction {
        Direction::LR => (target.x.saturating_sub(1), target_row),
        Direction::RL => (target.x.saturating_add(target.width), target_row),
        _ => return false,
    };

    let source_row = source.center_y();
    let target_primary = coords.primary_coord(target_arrow_x, target_row);
    let source_primary = coords.primary_coord(source_exit.0, source_exit.1);
    let turn_primary = match direction {
        Direction::LR => target_primary
            .saturating_sub(2)
            .max(source_primary.saturating_add(1)),
        Direction::RL => target_primary
            .saturating_add(2)
            .min(source_primary.saturating_sub(1)),
        _ => return false,
    };
    let mut turn_x = source_exit.0;
    let mut turn_y = source_exit.1;
    coords.set_primary(&mut turn_x, &mut turn_y, turn_primary);
    let (_, turn_target_y) = coords.with_secondary(turn_x, turn_y, target_row);

    if source_row == target_row {
        draw_line_primary(
            source_exit.0,
            source_exit.1,
            target_arrow_x,
            target_row,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
    } else {
        draw_line_primary(
            source_exit.0,
            source_exit.1,
            turn_x,
            turn_y,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
        let going_up = target_row < source_row;
        let first_corner = match direction {
            Direction::LR if going_up => style.corner_ur,
            Direction::LR => style.corner_dr,
            Direction::RL if going_up => style.corner_ul,
            Direction::RL => style.corner_dl,
            _ => return false,
        };
        set_route_edge_char(canvas, turn_x, turn_y, first_corner, style, Some(owner));

        draw_line_secondary(
            turn_x,
            turn_y,
            turn_x,
            turn_target_y,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
        let second_corner = match direction {
            Direction::LR if going_up => style.corner_dl,
            Direction::LR => style.corner_ul,
            Direction::RL if going_up => style.corner_dr,
            Direction::RL => style.corner_ur,
            _ => return false,
        };
        set_route_edge_char(
            canvas,
            turn_x,
            turn_target_y,
            second_corner,
            style,
            Some(owner),
        );

        let final_start = coords.advance(turn_x, turn_target_y, 1);
        draw_line_primary(
            final_start.0,
            final_start.1,
            target_arrow_x,
            target_row,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(owner),
        );
    }

    set_route_char(
        canvas,
        target_arrow_x,
        target_row,
        coords.arrow_end(style),
        Some(owner),
    );
    true
}
