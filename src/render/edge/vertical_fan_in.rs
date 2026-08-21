//! One-port-per-edge routing for the bounded vertical fan-in scene.
//!
//! The topology policy lives in `render::vertical_fan_in` and is shared with
//! measurement.  This lowerer plans every route before painting any cell so a
//! rejected geometry cannot leave a partial scene behind.

use crate::graph::{Direction, Graph, Node};
use crate::orientation::{Axis, OrientedCoords};
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::dual_junction::target_port_count as dual_junction_target_port_count;
use super::super::fallback_route::FallbackRoutePlan;
use super::super::vertical_fan_in::{
    nonterminal_target_port_count, target_port_columns, target_port_count,
};
use super::edge_primitives::{
    draw_line_primary, draw_line_secondary, edge_entry_point, edge_exit_point,
};
use super::{edge_route_owner_id, set_route_char, RouteOwner};

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
    let mut fallback = FallbackRoutePlan::new(
        format!("vertical-fan-in:{}", target.id),
        "vertical-fan-in-identity",
    );
    fallback.set_scene_coverage(
        ordered_sources
            .iter()
            .map(|source| edge_route_owner_id(graph, &source.id, &target.id)),
    );
    for plan in plans {
        if has_shared_source_stem(graph, plan.source, target) {
            mark_shared_source_prefix(&mut fallback, &plan, &coords);
        }
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
            // The primary line helper includes its endpoint, so the turn cell
            // temporarily contains a shaft.  This route plan has already
            // proved the corner is collision-free; write the authoritative
            // corner directly instead of resolving it as a false tee.
            set_route_char(
                canvas,
                plan.turn.0,
                plan.turn.1,
                coords.corner_start_to_secondary(going_before, style),
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
            // The final primary leg starts after target_turn, but retain the
            // direct corner write so a prior shaft cannot manufacture a
            // junction glyph at the receiver elbow.
            set_route_char(
                canvas,
                target_turn.0,
                target_turn.1,
                coords.corner_secondary_to_end(going_before, style),
                Some(owner),
            );
            let final_start = coords.advance(target_turn.0, target_turn.1, 1);
            draw_line_primary(
                final_start.0,
                final_start.1,
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

        if source_column == plan.target_port {
            push_primary_segment(
                &mut fallback,
                plan.source_exit,
                plan.target_entry,
                &coords,
                style,
            );
        } else {
            push_primary_segment(&mut fallback, plan.source_exit, plan.turn, &coords, style);
            let target_turn = coords.with_secondary(plan.turn.0, plan.turn.1, plan.target_port);
            push_secondary_segment(&mut fallback, plan.turn, target_turn, &coords, style);
            let going_before = source_column > plan.target_port;
            let final_start = coords.advance(target_turn.0, target_turn.1, 1);
            push_primary_segment(
                &mut fallback,
                final_start,
                plan.target_entry,
                &coords,
                style,
            );
            fallback.push_corner(
                plan.turn.0,
                plan.turn.1,
                coords.corner_start_to_secondary(going_before, style),
            );
            fallback.push_corner(
                target_turn.0,
                target_turn.1,
                coords.corner_secondary_to_end(going_before, style),
            );
        }
        fallback.push_paint(
            plan.target_entry.0,
            plan.target_entry.1,
            coords.arrow_end(style),
        );
    }
    canvas.record_fallback_route_evidence(fallback);
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
    let start_primary = coords.primary_coord(plan.source_exit.0, plan.source_exit.1);
    let end_primary = coords.primary_coord(plan.turn.0, plan.turn.1);
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
    use super::route_vertical_fan_in_edges;
    use crate::graph::{Direction, Edge, Graph, Node};
    use crate::render::canvas::Canvas;
    use crate::style::ASCII_CHARS;

    fn node(id: &str, x: usize, y: usize, width: usize, height: usize) -> Node {
        let mut node = Node::new(id, id);
        node.x = x;
        node.y = y;
        node.width = width;
        node.height = height;
        node
    }

    #[test]
    fn records_vertical_identity_fallback_evidence() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;
        graph.nodes.push(node("A", 0, 0, 8, 3));
        graph.nodes.push(node("B", 20, 0, 8, 3));
        graph.nodes.push(node("T", 10, 8, 10, 3));
        graph.edges.push(Edge::new("A", "T"));
        graph.edges.push(Edge::new("B", "T"));

        let sources = vec![&graph.nodes[0], &graph.nodes[1]];
        let mut canvas = Canvas::new(40, 20);
        assert!(route_vertical_fan_in_edges(
            &sources,
            &graph.nodes[2],
            &mut canvas,
            &ASCII_CHARS,
            graph.direction,
            &graph,
        ));

        let traces = canvas.fallback_route_traces();
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].strategy, "vertical-fan-in-identity");
        assert_eq!(traces[0].covered_edge_ids.len(), 2);
        assert!(
            traces[0].mismatches.is_empty(),
            "vertical fan-in evidence must match the painted route: {:?}",
            traces[0].mismatches
        );
    }
}
