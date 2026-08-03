//! Fan-out routing into a shared subgraph.

use crate::graph::{Direction, Graph, Node};
use crate::orientation::OrientedCoords;
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::semantic::CellOwnerKind;
use super::edge_primitives::{adjusted_edge_entry_point, draw_line_primary, edge_exit_point};
use super::{edge_route_owner_id, set_route_char, set_route_edge_char, RouteOwner};

pub(super) fn route_fanout_into_subgraph_td(
    from: &Node,
    targets: &[(usize, usize, &Node)],
    canvas: &mut Canvas,
    style: &StyleChars,
    sg: &crate::graph::Subgraph,
    direction: Direction,
    graph: &Graph,
) {
    let coords = OrientedCoords::new(direction);
    let (stem_start_x, stem_start_y) = edge_exit_point(from, direction);
    let fanout_owner_id = format!("fanout:{}", from.id);
    let fanout_owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: fanout_owner_id.as_str(),
    };

    let portal_center = sg.bounds.x + sg.bounds.width / 2;
    let min_target_x = targets
        .iter()
        .map(|(x, _, _)| *x)
        .min()
        .unwrap_or(portal_center);
    let max_target_x = targets
        .iter()
        .map(|(x, _, _)| *x)
        .max()
        .unwrap_or(portal_center);
    let junction_x = portal_center.clamp(min_target_x, max_target_x);

    let portal_y = sg.bounds.y.saturating_add(1);
    let min_arrow_y = targets
        .iter()
        .map(|(_, _, t)| adjusted_edge_entry_point(t, direction, graph).1)
        .min()
        .unwrap_or(portal_y.saturating_add(2));

    if std::env::var("DEBUG_FANOUT").is_ok() {
        let target_xs: Vec<usize> = targets.iter().map(|(x, _, _)| *x).collect();
        eprintln!(
            "fanout stem=({stem_start_x}, {stem_start_y}) portal_y={portal_y} jx={junction_x} targets={target_xs:?}"
        );
    }

    // Leave a dedicated spine row before the split so the center column is visible,
    // and keep the split above the arrow row.
    let delta = min_arrow_y.saturating_sub(portal_y);
    let mut junction_y = portal_y.saturating_add(delta / 2);
    let min_split_y = portal_y.saturating_add(1);
    let max_split_y = min_arrow_y.saturating_sub(2).max(min_split_y);
    if junction_y < min_split_y {
        junction_y = min_split_y;
    } else if junction_y > max_split_y {
        junction_y = max_split_y;
    }

    // Align horizontally to the subgraph center before dropping in.
    if stem_start_x != junction_x {
        let (hx0, hx1) = if stem_start_x < junction_x {
            (stem_start_x, junction_x)
        } else {
            (junction_x, stem_start_x)
        };
        for x in hx0..=hx1 {
            set_route_edge_char(
                canvas,
                x,
                stem_start_y,
                style.edge_h,
                style,
                Some(fanout_owner),
            );
        }
        let corner = if junction_x > stem_start_x {
            style.corner_dr
        } else {
            style.corner_dl
        };
        set_route_edge_char(
            canvas,
            junction_x,
            stem_start_y,
            corner,
            style,
            Some(fanout_owner),
        );
    } else {
        set_route_edge_char(
            canvas,
            junction_x,
            stem_start_y,
            coords.primary_edge_char(style),
            style,
            Some(fanout_owner),
        );
    }

    // Vertical spine down into the subgraph (including the portal row)
    if stem_start_y < junction_y {
        for y in (stem_start_y + 1)..=junction_y {
            set_route_edge_char(
                canvas,
                junction_x,
                y,
                coords.primary_edge_char(style),
                style,
                Some(fanout_owner),
            );
        }
    }

    let mut sorted_targets = targets.to_vec();
    sorted_targets.sort_by_key(|(x, y, _)| coords.secondary_coord(*x, *y));

    let first_secondary = coords.secondary_coord(sorted_targets[0].0, sorted_targets[0].1);
    let last_secondary = coords.secondary_coord(
        sorted_targets[sorted_targets.len() - 1].0,
        sorted_targets[sorted_targets.len() - 1].1,
    );
    let junction_secondary = coords.secondary_coord(junction_x, junction_y);

    // Draw a visible spine row just above the junction.
    if junction_y > portal_y {
        let spine_y = junction_y.saturating_sub(1);
        set_route_edge_char(
            canvas,
            junction_x,
            spine_y,
            coords.primary_edge_char(style),
            style,
            Some(fanout_owner),
        );
    }

    let span_start = first_secondary.min(junction_secondary);
    let span_end = last_secondary.max(junction_secondary);

    for pos in span_start..=span_end {
        let (span_x, span_y) = coords.with_secondary(junction_x, junction_y, pos);
        let c = if pos == junction_secondary {
            match direction {
                Direction::TD | Direction::TB => style.junction_up,
                Direction::LR => style.junction_left,
                Direction::RL => style.junction_right,
                Direction::BT => style.junction_down,
            }
        } else if pos == span_start {
            match direction {
                Direction::TD | Direction::TB => style.corner_dl,
                Direction::LR => style.corner_dl,
                Direction::RL => style.corner_dr,
                Direction::BT => style.corner_ul,
            }
        } else if pos == span_end {
            match direction {
                Direction::TD | Direction::TB => style.corner_dr,
                Direction::LR => style.corner_ul,
                Direction::RL => style.corner_ur,
                Direction::BT => style.corner_ur,
            }
        } else {
            coords.secondary_edge_char(style)
        };
        set_route_edge_char(canvas, span_x, span_y, c, style, Some(fanout_owner));
    }

    // Ensure the split junction reads as an upward tee (trunk enters from above).
    if matches!(direction, Direction::TD | Direction::TB) {
        set_route_char(
            canvas,
            junction_x,
            junction_y,
            style.junction_up,
            Some(fanout_owner),
        );
    }

    for (target_x, target_y, target) in &sorted_targets {
        let branch_owner_id = edge_route_owner_id(graph, &from.id, &target.id);
        let branch_owner = RouteOwner {
            kind: CellOwnerKind::EdgeSegment,
            id: branch_owner_id.as_str(),
        };
        let target_secondary = coords.secondary_coord(*target_x, *target_y);
        let (arrow_x, arrow_y) = adjusted_edge_entry_point(target, direction, graph);
        let (drop_x, drop_y) = coords.with_secondary(junction_x, junction_y, target_secondary);
        let (drop_start_x, drop_start_y) = coords.advance(drop_x, drop_y, 1);
        if drop_start_x != arrow_x || drop_start_y != arrow_y {
            draw_line_primary(
                drop_start_x,
                drop_start_y,
                arrow_x,
                arrow_y,
                &coords,
                canvas,
                style,
                None,
                Some(branch_owner),
            );
        }

        set_route_char(
            canvas,
            arrow_x,
            arrow_y,
            coords.arrow_end(style),
            Some(branch_owner),
        );
    }
}
