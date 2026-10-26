//! Fan-out routing into a shared subgraph.

use crate::graph::{Direction, EdgeKind, Graph, Node, NodeShape};
use crate::orientation::{is_before, OrientedCoords};
use crate::spacing::SpacingConfig;
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::edge_policy::{
    straight_entry_x, td_single_incoming_route_x, td_single_outgoing_route_x, title_safe_td_entry_x,
};
use super::super::fallback_route::FallbackRoutePlan;
use super::super::semantic::{CellOwnerKind, CellRole};
use super::edge_primitives::{
    adjusted_edge_entry_point, draw_line_primary, draw_line_secondary, edge_entry_candidates,
    edge_exit_point, fan_out_target_uses_left_port, get_node_center, hits_foreign_subgraph_border,
    is_subgraph_title_text_cell,
};
use super::subgraph::{
    lower_td_fallback_plan, route_cross_subgraph_bt, route_cross_subgraph_td,
    route_divergent_into_subgraph_bt, route_divergent_into_subgraph_horizontal,
    route_divergent_into_subgraph_td, td_complex_title_portal_entry_x, BtRouteOutcome,
};
use super::{
    edge_route_owner_id, set_route_char, set_route_edge_char, set_route_endpoint_char,
    style_for_edge_kind, RouteOwner,
};

pub(super) const MIXED_SIBLING_DETOUR: &str = "td-mixed-sibling-side-exit";

struct MixedSiblingDetour {
    plan: FallbackRoutePlan,
    crossing_owner: String,
    crossing: (usize, usize),
}

/// The remaining branch of an already separated fan-in source cannot use
/// its center stem when a peer box lies below it. Leave through the frame's
/// right wall, then approach the receiver on a title-safe vertical lane.
fn td_mixed_sibling_detour(
    from: &Node,
    target: &Node,
    canvas: &Canvas,
    style: &StyleChars,
    spacing: &SpacingConfig,
    graph: &Graph,
) -> Option<MixedSiblingDetour> {
    if !matches!(graph.direction, Direction::TD | Direction::TB)
        || from.shape != NodeShape::Rectangle
        || !target.shape.has_flat_vertical_entry()
    {
        return None;
    }
    let outgoing: Vec<_> = graph.edges.iter().filter(|e| e.from == from.id).collect();
    if outgoing.len() != 2
        || outgoing
            .iter()
            .any(|e| e.is_back_edge || e.label.is_some() || e.kind != EdgeKind::Arrow)
        || graph.edges.iter().filter(|e| e.to == target.id).count() != 1
    {
        return None;
    }
    let other = outgoing.iter().find(|e| e.to != target.id)?;
    if !canvas.fallback_route_covers_edge(&edge_route_owner_id(graph, &from.id, &other.to)) {
        return None;
    }
    let incoming: Vec<_> = graph.edges.iter().filter(|e| e.to == other.to).collect();
    if incoming.len() != 2 {
        return None;
    }
    let peer = graph.get_node(&incoming.iter().find(|e| e.from != from.id)?.from)?;
    let peer_outgoing: Vec<_> = graph.edges.iter().filter(|e| e.from == peer.id).collect();
    if peer_outgoing.len() != 2
        || peer_outgoing
            .iter()
            .any(|e| e.is_back_edge || e.label.is_some() || e.kind != EdgeKind::Arrow)
    {
        return None;
    }
    let crossing_target = graph.get_node(&peer_outgoing.iter().find(|e| e.to != other.to)?.to)?;
    if !crossing_target.shape.has_flat_vertical_entry()
        || crossing_target.y <= target.bottom_y() + 1
        || graph
            .edges
            .iter()
            .filter(|e| e.to == crossing_target.id)
            .count()
            != 1
    {
        return None;
    }
    let (exits, enters) = graph.edge_boundary_crossings(&from.id, &target.id);
    let ([exit], [entry]) = (exits.as_slice(), enters.as_slice()) else {
        return None;
    };
    let a = graph.get_subgraph(exit)?;
    let b = graph.get_subgraph(entry)?;
    let (ab, bb) = (&a.bounds, &b.bounds);
    if a.parent_id.is_some()
        || b.parent_id.is_some()
        || !ab.is_valid()
        || !bb.is_valid()
        || graph.get_node_subgraph(&peer.id) != Some(*exit)
        || graph.get_node_subgraph(&crossing_target.id) != Some(*entry)
        || from.y <= ab.y
        || peer.y < from.bottom_y()
        || peer.bottom_y() >= ab.y + ab.height - 1
        || !(peer.x..peer.x + peer.width).contains(&from.center_x())
    {
        return None;
    }
    let turn_y = from.bottom_y();
    let exit_x = ab.x + ab.width - 1;
    let outside_x = exit_x + 2;
    // Keep a blank row between this shoulder and a later fan-out shoulder
    // immediately above the receiver frame; adjacent ASCII bends fuse.
    let jog_y = bb.y.checked_sub(3)?;
    let arrow_y = target.y.checked_sub(1)?;
    if from.x <= ab.x
        || from.x + from.width >= exit_x
        || jog_y < ab.y + ab.height + 1
        || arrow_y <= bb.y + 2
        || turn_y >= peer.y
        || outside_x >= canvas.width
    {
        return None;
    }
    let owner_id = edge_route_owner_id(graph, &from.id, &target.id);
    let gutter = crate::portals::td_sibling_title_gutter(graph, &b.id);
    if canvas
        .get_meta(from.center_x(), turn_y - 1)
        .is_some_and(|m| {
            m.z_index > 0
                && !(m.owner_kind == CellOwnerKind::NodeBorder
                    && m.owner_id.as_deref() == Some(&from.id))
        })
        || b.title.as_deref().is_some_and(|title| {
            crate::graph::subgraph_title_span_with_padding_sides(
                bb.x,
                bb.width,
                title,
                graph.direction,
                gutter.leading_extra_padding,
                gutter.trailing_extra_padding,
            )
            .is_none()
        })
    {
        return None;
    }
    let foreign_route = |x, y| {
        canvas.get_meta(x, y).is_some_and(|m| {
            m.z_index > 0
                && matches!(
                    m.owner_kind,
                    CellOwnerKind::EdgeSegment
                        | CellOwnerKind::ArrowHead
                        | CellOwnerKind::Junction
                        | CellOwnerKind::CycleEdge
                        | CellOwnerKind::PortalOpening
                )
        })
    };
    for (lane, y) in edge_entry_candidates(target, graph.direction) {
        if y != arrow_y
            || lane + 2 >= exit_x
            || lane <= bb.x + 1
            || lane + 2 >= bb.x + bb.width
            || lane <= peer.center_x()
            || lane + 1 >= crossing_target.center_x()
        {
            continue;
        }
        let mut plan = FallbackRoutePlan::new(&owner_id, MIXED_SIBLING_DETOUR);
        plan.set_scene_coverage([owner_id.clone()]);
        plan.push_horizontal(turn_y, from.center_x(), outside_x, style.edge_h);
        plan.push_vertical(outside_x, turn_y, jog_y, style.edge_v);
        plan.push_horizontal(jog_y, outside_x, lane, style.edge_h);
        plan.push_vertical(lane, jog_y, arrow_y, style.edge_v);
        plan.push_corner(from.center_x(), turn_y, style.corner_ul);
        plan.push_corner(outside_x, turn_y, style.corner_dr);
        plan.push_corner(outside_x, jog_y, style.corner_ur);
        plan.push_corner(lane, jog_y, style.corner_dl);
        plan.push_paint(from.center_x(), turn_y - 1, style.junction_down);
        plan.push_paint(lane, arrow_y, style.arrow_down);
        plan.claim_boundary(*exit, "right", exit_x, turn_y, style.edge_h);
        plan.claim_boundary(*entry, "top", lane, bb.y, style.edge_v);
        if plan.validation_error(canvas.width, canvas.height).is_some()
            || canvas.fallback_route_cell_owned_by_other(
                from.center_x(),
                turn_y - 1,
                Some(&owner_id),
            )
        {
            continue;
        }
        let clear = plan.segments.iter().all(|segment| {
            let xs = segment.from.x.min(segment.to.x)..=segment.from.x.max(segment.to.x);
            let ys = segment.from.y.min(segment.to.y)..=segment.from.y.max(segment.to.y);
            xs.into_iter().all(|x| {
                ys.clone().all(|y| {
                    let wall = (x == exit_x && y == turn_y) || (x == lane && y == bb.y);
                    !graph
                        .nodes
                        .iter()
                        .any(|n| x >= n.x && x < n.x + n.width && y >= n.y && y < n.bottom_y())
                        && !canvas.fallback_route_cell_owned_by_other(x, y, Some(&owner_id))
                        && !canvas.get_meta(x, y).is_some_and(|m| {
                            m.z_index > 0
                                && matches!(
                                    m.owner_kind,
                                    CellOwnerKind::EdgeSegment
                                        | CellOwnerKind::ArrowHead
                                        | CellOwnerKind::Junction
                                        | CellOwnerKind::CycleEdge
                                        | CellOwnerKind::PortalOpening
                                        | CellOwnerKind::NodeLabel
                                        | CellOwnerKind::EdgeLabel
                                )
                        })
                        && ![
                            (
                                x - 1,
                                y,
                                super::super::topology::char_connects_right(canvas.get(x - 1, y)),
                            ),
                            (
                                x + 1,
                                y,
                                super::super::topology::char_connects_left(canvas.get(x + 1, y)),
                            ),
                            (
                                x,
                                y - 1,
                                super::super::topology::char_connects_down(canvas.get(x, y - 1)),
                            ),
                            (
                                x,
                                y + 1,
                                super::super::topology::char_connects_up(canvas.get(x, y + 1)),
                            ),
                        ]
                        .iter()
                        .any(|&(nx, ny, aimed)| aimed && foreign_route(nx, ny))
                        && !graph.subgraphs.iter().any(|sg| {
                            let r = &sg.bounds;
                            r.contains(x, y)
                                && (x == r.x
                                    || x + 1 == r.x + r.width
                                    || y == r.y
                                    || y + 1 == r.y + r.height)
                                && !((sg.id == *exit && x == exit_x && y == turn_y)
                                    || (sg.id == *entry && x == lane && y == bb.y))
                        })
                        && (canvas.get(x, y) == ' '
                            || (wall
                                && canvas.get_meta(x, y).is_some_and(|m| {
                                    m.owner_kind == CellOwnerKind::SubgraphBorder
                                })
                                && if y == turn_y {
                                    super::super::canvas::is_vertical(canvas.get(x, y), style)
                                } else {
                                    super::super::canvas::is_horizontal(canvas.get(x, y), style)
                                })
                            || (y == bb.y + 1 && is_subgraph_title_text_cell(graph, x, y)))
                })
            })
        });
        if !clear {
            continue;
        }
        let mut probe = canvas.clone();
        lower_td_fallback_plan(
            plan.clone(),
            &mut probe,
            style,
            Some(RouteOwner {
                kind: CellOwnerKind::EdgeSegment,
                id: &owner_id,
            }),
        );
        // The peer is routed later. Prove its actual title/portal policy
        // crosses this straight run, rather than predicting from its center.
        // Reversing the peer pair cannot admit another detour: its peer is
        // above it, so the recursive probe uses the ordinary route policy.
        let crossing_owner = edge_route_owner_id(graph, &peer.id, &crossing_target.id);
        let crossing = (lane, bb.y - 1);
        probe.allow_fallback_route_crossing(&owner_id, &crossing_owner, crossing.0, crossing.1);
        route_divergent_edges(
            peer,
            &[crossing_target],
            &mut probe,
            style,
            spacing,
            graph.direction,
            graph,
        );
        probe.finalize_explicit_crossings(style);
        if probe.get(crossing.0, crossing.1) != style.cross_end
            || probe
                .refused_route_writes()
                .iter()
                .any(|(holder, _, _, _)| holder == &owner_id)
        {
            continue;
        }
        super::super::portal_restore::draw_subgraph_title(
            &mut probe,
            &b.id,
            bb,
            b.title.as_deref(),
            graph.direction,
            gutter,
            &std::collections::HashSet::new(),
        );
        if probe.get(lane, bb.y + 1) == style.edge_v {
            return Some(MixedSiblingDetour {
                plan,
                crossing_owner,
                crossing,
            });
        }
    }
    None
}

/// Draw the end of the forward edge `from -> to` at its target: an arrowhead,
/// a circle or cross end marker, or for an open link the plain shaft.
#[allow(clippy::too_many_arguments)]
fn set_target_end(
    canvas: &mut Canvas,
    coords: &OrientedCoords,
    style: &StyleChars,
    graph: &Graph,
    from: &Node,
    to: &Node,
    (x, y): (usize, usize),
    owner: RouteOwner<'_>,
) {
    let kind = graph
        .edges
        .iter()
        .find(|edge| edge.from == from.id && edge.to == to.id && !edge.is_back_edge)
        .map_or(EdgeKind::Arrow, |edge| edge.kind);
    match kind {
        EdgeKind::CircleEnd => set_route_endpoint_char(canvas, x, y, style.circle_end, owner),
        EdgeKind::CrossEnd => set_route_endpoint_char(canvas, x, y, style.cross_end, owner),
        EdgeKind::Open => {
            let shaft = coords.primary_edge_char(&style_for_edge_kind(style, kind));
            set_route_char(canvas, x, y, shaft, Some(owner));
        }
        _ => set_route_char(canvas, x, y, coords.arrow_end(style), Some(owner)),
    }
}

/// A remaining branch of a fan-in source must not bend along its frame's
/// bottom wall. If the boxes overlap, give it a separate, clear downward
/// port instead of joining the already drawn fan-in stem (gen_a_151).
fn td_independent_frame_exit(
    from: &Node,
    target: &Node,
    junction_y: usize,
    canvas: &Canvas,
    style: &StyleChars,
    graph: &Graph,
) -> Option<(usize, FallbackRoutePlan)> {
    if !matches!(graph.direction, Direction::TD | Direction::TB)
        || !matches!(
            from.shape,
            NodeShape::Rectangle | NodeShape::Rounded | NodeShape::Stadium | NodeShape::Subroutine
        )
        || !target.shape.has_flat_vertical_entry()
        || graph
            .edges
            .iter()
            .filter(|edge| !edge.is_back_edge && edge.from == from.id)
            .count()
            < 2
        || graph
            .edges
            .iter()
            .filter(|edge| edge.to == target.id)
            .count()
            != 1
        || graph
            .edges
            .iter()
            .any(|edge| edge.from == from.id && edge.to == target.id && edge.label.is_some())
    {
        return None;
    }
    let (exits, enters) = graph.edge_boundary_crossings(&from.id, &target.id);
    let [exit] = exits.as_slice() else {
        return None;
    };
    let bounds = &graph.get_subgraph(exit)?.bounds;
    let start_y = from.bottom_y();
    let arrow_y = target.y.checked_sub(1)?;
    if !enters.is_empty()
        || junction_y + 1 != bounds.y + bounds.height
        || start_y <= bounds.y
        || start_y > junction_y
        || junction_y >= arrow_y
    {
        return None;
    }
    let owner_id = edge_route_owner_id(graph, &from.id, &target.id);
    let (lane, arrow_y) = edge_entry_candidates(target, graph.direction)
        .into_iter()
        .find(|&(x, arrow_y)| {
            x >= from.x + 2
                && x + 3 <= from.x + from.width
                && x.abs_diff(from.center_x()) >= 2
                && x > bounds.x
                && x + 1 < bounds.x + bounds.width
                && !canvas.fallback_route_cell_owned_by_other(x, start_y - 1, Some(&owner_id))
                && (start_y..=arrow_y).all(|y| {
                    let blocked_node = graph.nodes.iter().any(|node| {
                        node.id != from.id
                            && node.id != target.id
                            && x >= node.x
                            && x < node.x + node.width
                            && y >= node.y
                            && y < node.bottom_y()
                    });
                    let blocked_frame = graph.subgraphs.iter().any(|sg| {
                        let b = &sg.bounds;
                        sg.id != *exit
                            && b.contains(x, y)
                            && (x == b.x
                                || x + 1 == b.x + b.width
                                || y == b.y
                                || y + 1 == b.y + b.height)
                    });
                    !blocked_node
                        && !blocked_frame
                        && !canvas.fallback_route_cell_owned_by_other(x, y, Some(&owner_id))
                        && (canvas.get(x, y) == ' '
                            || (y == junction_y
                                && canvas.get_meta(x, y).is_some_and(|meta| {
                                    meta.owner_kind == CellOwnerKind::SubgraphBorder
                                })
                                && super::super::canvas::is_horizontal(canvas.get(x, y), style)))
                })
        })?;
    let mut plan = FallbackRoutePlan::new(owner_id, "td-independent-frame-exit");
    plan.push_vertical(lane, start_y, arrow_y, style.edge_v);
    plan.push_paint(lane, start_y - 1, style.junction_down);
    plan.claim_boundary(*exit, "bottom", lane, junction_y, style.edge_v);
    if plan.validation_error(canvas.width, canvas.height).is_some() {
        return None;
    }
    Some((lane, plan))
}

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

    if crate::runtime::current().diagnostics.fan_out {
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

        set_target_end(
            canvas,
            &coords,
            style,
            graph,
            from,
            target,
            (arrow_x, arrow_y),
            branch_owner,
        );
    }
}

/// Route edges from a single source to multiple targets (divergence)
/// Works for all orientations using the abstraction layer
pub fn route_divergent_edges(
    from: &Node,
    to_nodes: &[&Node],
    canvas: &mut Canvas,
    style: &StyleChars,
    spacing: &SpacingConfig,
    direction: Direction,
    graph: &Graph,
) {
    if to_nodes.is_empty() || !canvas.is_visible(from) {
        return;
    }

    let coords = OrientedCoords::new(direction);
    let context = crate::runtime::current();
    let debug_timing = context.diagnostics.timing;
    if debug_timing {
        let targets: Vec<&str> = to_nodes.iter().map(|n| n.id.as_str()).collect();
        eprintln!("render: route from {} to {:?}", from.id, targets);
    }

    // Filter to visible targets only
    let visible_targets: Vec<&Node> = to_nodes
        .iter()
        .filter(|n| canvas.is_visible(n))
        .copied()
        .collect();
    if visible_targets.is_empty() {
        return;
    }

    // Calculate source center based on orientation
    let (src_x, src_y) = get_node_center(from);
    // Decided before any branch is drawn, from the exits already on the canvas.
    let left_ports: Vec<&str> = visible_targets
        .iter()
        .filter(|target| fan_out_target_uses_left_port(from, target, canvas, style, graph))
        .map(|target| target.id.as_str())
        .collect();

    // Calculate stem start position (edge of source box on primary axis)
    let (stem_start_x, stem_start_y) = edge_exit_point(from, direction);

    // Calculate junction position (stem length away from source)
    let stem_length = match direction {
        Direction::LR | Direction::RL => spacing.stem_length_horizontal,
        _ => spacing.stem_length_vertical,
    };

    let (mut junction_x, mut junction_y) = coords.advance(stem_start_x, stem_start_y, stem_length);

    // Get target centers and sort them on secondary axis
    let mut target_positions: Vec<(usize, usize, &Node)> = visible_targets
        .iter()
        .map(|&n| {
            let (tx, ty) = get_node_center(n);
            (tx, ty, n)
        })
        .collect();

    target_positions.sort_by_key(|(x, y, _)| coords.secondary_coord(*x, *y));

    // If all targets share the same subgraph (different from the source), branch
    // inside that subgraph to keep the junction aligned with its interior.
    if matches!(direction, Direction::TD | Direction::TB | Direction::BT)
        && target_positions.len() > 1
    {
        if let Some(target_sg) = visible_targets
            .first()
            .and_then(|n| graph.get_node_subgraph(&n.id))
        {
            let all_same = visible_targets
                .iter()
                .all(|n| graph.get_node_subgraph(&n.id) == Some(target_sg));
            let source_sg = graph.get_node_subgraph(&from.id);
            if all_same && source_sg != Some(target_sg) {
                if let Some(sg) = graph.get_subgraph(target_sg) {
                    match direction {
                        Direction::TD | Direction::TB => route_divergent_into_subgraph_td(
                            from,
                            &visible_targets,
                            canvas,
                            style,
                            sg,
                            direction,
                            graph,
                        ),
                        Direction::BT => route_divergent_into_subgraph_bt(
                            from,
                            &visible_targets,
                            canvas,
                            style,
                            sg,
                            graph,
                        ),
                        _ => unreachable!(),
                    }
                    return;
                }
            }
        }
    }

    // For horizontal fan-outs, nudge the junction away from the targets so we keep
    // visible elbows/dashes before the arrows.
    if matches!(direction, Direction::LR | Direction::RL) && target_positions.len() > 1 {
        let arrow_primaries: Vec<usize> = target_positions
            .iter()
            .map(|(_, _, n)| {
                let (ax, ay) = adjusted_edge_entry_point(n, direction, graph);
                coords.primary_coord(ax, ay)
            })
            .collect();
        if let Some(closest_arrow) = match direction {
            Direction::LR => arrow_primaries.iter().min(),
            Direction::RL => arrow_primaries.iter().max(),
            _ => None,
        } {
            let stem_start_primary = coords.primary_coord(stem_start_x, stem_start_y);
            let current_primary = coords.primary_coord(junction_x, junction_y);
            let desired_primary = match direction {
                // Keep at least two visible dashes before the arrow when possible.
                Direction::LR => closest_arrow.saturating_sub(3),
                Direction::RL => closest_arrow.saturating_add(3),
                _ => current_primary,
            };
            let adjusted_primary = match direction {
                Direction::LR => desired_primary
                    .min(current_primary)
                    .max(stem_start_primary + 1),
                Direction::RL => desired_primary
                    .max(current_primary)
                    .min(stem_start_primary.saturating_sub(1)),
                _ => current_primary,
            };
            if adjusted_primary != current_primary {
                coords.set_primary(&mut junction_x, &mut junction_y, adjusted_primary);
            }
        }
    }

    // Ensure some horizontal breathing room between junction and nearest target arrow for LR/RL.
    if matches!(direction, Direction::LR | Direction::RL) && target_positions.len() > 1 {
        let stem_start_primary = coords.primary_coord(stem_start_x, stem_start_y);
        let junction_primary = coords.primary_coord(junction_x, junction_y);
        let nearest_arrow_primary = target_positions
            .iter()
            .map(|(_, _, n)| {
                let (ax, ay) = adjusted_edge_entry_point(n, direction, graph);
                coords.primary_coord(ax, ay)
            })
            .min_by_key(|p| junction_primary.abs_diff(*p));
        if let Some(arrow_primary) = nearest_arrow_primary {
            let gap = junction_primary.abs_diff(arrow_primary);
            // With `drop_start = junction + 1`, `gap=3` yields two dashes before the arrow.
            let min_gap = 3;
            if gap < min_gap {
                let adjust = min_gap - gap;
                let mut adjusted_primary = junction_primary;
                match direction {
                    Direction::LR => {
                        adjusted_primary = adjusted_primary.saturating_sub(adjust);
                        adjusted_primary = adjusted_primary.max(stem_start_primary + 1);
                    }
                    Direction::RL => {
                        adjusted_primary = adjusted_primary.saturating_add(adjust);
                        adjusted_primary = adjusted_primary
                            .min(stem_start_primary.saturating_sub(1).max(adjusted_primary));
                    }
                    _ => {}
                }
                coords.set_primary(&mut junction_x, &mut junction_y, adjusted_primary);
            }
        }
    }

    // Targets inside frames the source is not in: the bus (or a single
    // target's turn) stays outside the outermost of them, a blank column clear
    // of its border, instead of running down the border itself.
    // A single target's turn moves only when it lands on a frame's side
    // border, where the border hides it; elsewhere it keeps its column.
    let on_frame_side = {
        let x = junction_x;
        graph.subgraphs.iter().any(|sg| {
            sg.bounds.is_valid()
                && (x == sg.bounds.x || x + 1 == sg.bounds.x + sg.bounds.width)
                && sg.bounds.y <= junction_y
                && junction_y < sg.bounds.y + sg.bounds.height
        })
    };
    // A single target's new turn column must not already carry another
    // vertical line between the source and target rows: the turn would run
    // along it and merge the two (gen_a_174's Publish onto a fan-in bus).
    let turn_row = junction_y;
    let free_turn = |column: usize| {
        if target_positions.len() > 1 {
            return true;
        }
        let rows = target_positions
            .iter()
            .map(|(_, _, target)| adjusted_edge_entry_point(target, direction, graph).1)
            .chain([turn_row]);
        let (low, high) = rows.fold((usize::MAX, 0), |(low, high), y| (low.min(y), high.max(y)));
        (low..=high).all(|y| {
            let cell = canvas.get(column, y);
            !super::super::canvas::is_vertical(cell, style)
                && !super::super::canvas::is_junction(cell, style)
                && !super::super::canvas::is_corner(cell, style)
        })
    };
    if matches!(direction, Direction::LR | Direction::RL)
        && (target_positions.len() > 1 || (target_positions.len() == 1 && on_frame_side))
    {
        let entered: Vec<&crate::graph::Subgraph> = graph
            .subgraphs
            .iter()
            .filter(|sg| sg.bounds.is_valid())
            .filter(|sg| !graph.is_node_in_subgraph_tree(&from.id, &sg.id))
            .filter(|sg| {
                target_positions
                    .iter()
                    .all(|(_, _, target)| graph.is_node_in_subgraph_tree(&target.id, &sg.id))
            })
            .collect();
        let stem_start_primary = coords.primary_coord(stem_start_x, stem_start_y);
        let junction_primary = coords.primary_coord(junction_x, junction_y);
        let clear = match direction {
            Direction::RL => entered
                .iter()
                .map(|sg| sg.bounds.x + sg.bounds.width + 1)
                .max()
                .filter(|&clear| clear > junction_primary && clear < stem_start_primary),
            _ => entered
                .iter()
                .filter_map(|sg| sg.bounds.x.checked_sub(2))
                .min()
                .filter(|&clear| clear < junction_primary && clear > stem_start_primary),
        };
        if let Some(clear) = clear.filter(|&clear| free_turn(clear)) {
            coords.set_primary(&mut junction_x, &mut junction_y, clear);
        }
        // Likewise for frames the source leaves that hold none of the targets:
        // the bus starts a blank column outside the outermost of them.
        let left: Vec<&crate::graph::Subgraph> = graph
            .subgraphs
            .iter()
            .filter(|sg| sg.bounds.is_valid())
            .filter(|sg| graph.is_node_in_subgraph_tree(&from.id, &sg.id))
            .filter(|sg| {
                target_positions
                    .iter()
                    .all(|(_, _, target)| !graph.is_node_in_subgraph_tree(&target.id, &sg.id))
            })
            .collect();
        let junction_primary = coords.primary_coord(junction_x, junction_y);
        let arrows = target_positions.iter().map(|(_, _, target)| {
            let (ax, ay) = adjusted_edge_entry_point(target, direction, graph);
            coords.primary_coord(ax, ay)
        });
        let clear = match direction {
            Direction::RL => {
                let nearest_arrow = arrows.max().unwrap_or(0);
                left.iter()
                    .filter_map(|sg| sg.bounds.x.checked_sub(2))
                    .min()
                    .filter(|&clear| clear < junction_primary && clear > nearest_arrow + 1)
            }
            _ => {
                let nearest_arrow = arrows.min().unwrap_or(usize::MAX);
                left.iter()
                    .map(|sg| sg.bounds.x + sg.bounds.width + 1)
                    .max()
                    .filter(|&clear| clear > junction_primary && clear + 1 < nearest_arrow)
            }
        };
        if let Some(clear) = clear.filter(|&clear| free_turn(clear)) {
            coords.set_primary(&mut junction_x, &mut junction_y, clear);
        }
    }

    // Likewise in TD/BT for frames the source leaves that hold none of the
    // targets: the bus runs outside the outermost of them, a blank row clear
    // of its wall where the targets leave room, instead of along the wall
    // (which hides it) or right inside it.  Only for targets in open space
    // (or in frames the source is in too): a branch into another frame turns
    // up to its own portal, which a bus beside that frame's wall would cut.
    if matches!(direction, Direction::TD | Direction::TB | Direction::BT)
        && target_positions.len() > 1
        && target_positions.iter().all(|(_, _, target)| {
            graph
                .get_node_subgraph(&target.id)
                .is_none_or(|sg| graph.is_node_in_subgraph_tree(&from.id, sg))
        })
    {
        let left: Vec<&crate::graph::Subgraph> = graph
            .subgraphs
            .iter()
            .filter(|sg| sg.bounds.is_valid())
            .filter(|sg| graph.is_node_in_subgraph_tree(&from.id, &sg.id))
            .filter(|sg| {
                target_positions
                    .iter()
                    .all(|(_, _, target)| !graph.is_node_in_subgraph_tree(&target.id, &sg.id))
            })
            .collect();
        let arrows = target_positions
            .iter()
            .map(|(_, _, target)| adjusted_edge_entry_point(target, direction, graph).1);
        // A labelled branch keeps a row between the bus and its head for the
        // label.
        let label_room = usize::from(target_positions.iter().any(|(_, _, target)| {
            graph.edges.iter().any(|edge| {
                !edge.is_back_edge
                    && edge.from == from.id
                    && edge.to == target.id
                    && edge.label.is_some()
            })
        }));
        let clear = if direction == Direction::BT {
            let nearest_arrow = arrows.max().unwrap_or(0);
            left.iter().map(|sg| sg.bounds.y).min().and_then(|wall| {
                [wall.checked_sub(2), wall.checked_sub(1)]
                    .into_iter()
                    .flatten()
                    .find(|&clear| clear < junction_y && clear > nearest_arrow + label_room)
            })
        } else {
            let nearest_arrow = arrows.min().unwrap_or(usize::MAX);
            left.iter()
                .map(|sg| sg.bounds.y + sg.bounds.height.saturating_sub(1))
                .max()
                .and_then(|wall| {
                    [wall + 2, wall + 1]
                        .into_iter()
                        .find(|&clear| clear > junction_y && clear + label_room < nearest_arrow)
                })
        };
        if let Some(clear) = clear {
            coords.set_primary(&mut junction_x, &mut junction_y, clear);
        }
    }

    // Labeled horizontal fan-out: each branch label is drawn on its branch run
    // (bus -> arrow). Move the bus toward the source until the shortest run
    // fits the widest branch label with a shaft cell on each side, so labels
    // neither float before the bus nor overwrite its corners.
    // A target one row off the source row exits through its own adjacent
    // port; pulling the junction back would merge it into a new bus.
    let source_row = coords.secondary_coord(stem_start_x, stem_start_y);
    let has_adjacent_row_target = target_positions.iter().any(|(_, _, n)| {
        let (ax, ay) = adjusted_edge_entry_point(n, direction, graph);
        coords.secondary_coord(ax, ay).abs_diff(source_row) == 1
    });
    if matches!(direction, Direction::LR | Direction::RL)
        && target_positions.len() > 1
        && !has_adjacent_row_target
    {
        let widest_label = target_positions
            .iter()
            .filter_map(|(_, _, target)| {
                graph
                    .edges
                    .iter()
                    .find(|edge| !edge.is_back_edge && edge.from == from.id && edge.to == target.id)
                    .and_then(|edge| edge.label.as_deref())
            })
            .map(|label| crate::display_profile::display_width(label).min(20))
            .max();
        if let Some(widest_label) = widest_label {
            let stem_start_primary = coords.primary_coord(stem_start_x, stem_start_y);
            let junction_primary = coords.primary_coord(junction_x, junction_y);
            let room = widest_label + 3;
            let nearest_arrow_primary = target_positions
                .iter()
                .map(|(_, _, n)| {
                    let (ax, ay) = adjusted_edge_entry_point(n, direction, graph);
                    coords.primary_coord(ax, ay)
                })
                .min_by_key(|p| junction_primary.abs_diff(*p));
            if let Some(arrow_primary) = nearest_arrow_primary {
                let adjusted_primary = match direction {
                    Direction::LR => arrow_primary
                        .saturating_sub(room)
                        .min(junction_primary)
                        .max(stem_start_primary + 1),
                    _ => arrow_primary
                        .saturating_add(room)
                        .max(junction_primary)
                        .min(stem_start_primary.saturating_sub(1)),
                };
                if adjusted_primary != junction_primary {
                    coords.set_primary(&mut junction_x, &mut junction_y, adjusted_primary);
                }
            }
        }
    }

    // A source can contribute to a downstream fan-in while also branching to
    // another target.  Convergence is rendered before this pass, so keep the
    // later LR/RL spine away from an already-owned fan-in junction when the
    // complete measured candidate route has room.  This is deliberately
    // topology-gated; ordinary single-target L-routes retain their existing
    // geometry.
    if target_positions.len() == 1 && matches!(direction, Direction::LR | Direction::RL) {
        let target = target_positions[0].2;
        let request = MixedBranchRequest {
            from,
            target,
            stem_start: (stem_start_x, stem_start_y),
            junction: (junction_x, junction_y),
            canvas,
            direction,
            graph,
        };
        if let Some((candidate_x, candidate_y)) = mixed_branch_junction(request) {
            junction_x = candidate_x;
            junction_y = candidate_y;
        }
    }

    // A mixed fan-out needs one writable target-facing shaft cell for Thick and
    // Dotted branches.  With the compact vertical layout the shared span can
    // otherwise land immediately beside the target arrow row, leaving those
    // edge kinds with no branch cell to carry their kind-specific shaft glyph.
    // Move only this topology family one cell toward the source when the
    // candidate corridor is blank; ordinary fan-outs retain their geometry.
    if target_positions.len() > 1
        && matches!(direction, Direction::TD | Direction::TB | Direction::BT)
        && target_positions.iter().any(|(_, _, target)| {
            graph
                .edges
                .iter()
                .find(|edge| !edge.is_back_edge && edge.from == from.id && edge.to == target.id)
                .is_some_and(|edge| matches!(edge.kind, EdgeKind::Thick | EdgeKind::Dotted))
        })
    {
        if let Some((candidate_x, candidate_y)) = mixed_vertical_fanout_clearance(
            &target_positions,
            &left_ports,
            junction_x,
            junction_y,
            stem_start_x,
            stem_start_y,
            direction,
            graph,
            canvas,
        ) {
            junction_x = candidate_x;
            junction_y = candidate_y;
        }
    }

    // Single target: direct route
    if target_positions.len() == 1 {
        let (mut target_x, target_y, target) = (
            target_positions[0].0,
            target_positions[0].1,
            target_positions[0].2,
        );
        let route_owner_id = edge_route_owner_id(graph, &from.id, &target.id);
        let route_owner = RouteOwner {
            kind: CellOwnerKind::EdgeSegment,
            id: route_owner_id.as_str(),
        };
        // A dotted shaft stays dotted, as on a fan-out branch; it was drawn
        // plain (`B3 -.-> Drain` out of a frame). A thick one stays plain:
        // its heavy shaft would read as the wall of a frame (drawn heavy).
        let dotted = graph.edges.iter().any(|edge| {
            edge.from == from.id
                && edge.to == target.id
                && !edge.is_back_edge
                && edge.kind == EdgeKind::Dotted
        });
        let edge_style = if dotted {
            style_for_edge_kind(style, EdgeKind::Dotted)
        } else {
            *style
        };
        let style = &edge_style;

        let (mut arrow_x, arrow_y) = adjusted_edge_entry_point(target, direction, graph);
        // Nested boundary chains pick their own portal lanes, so only routes that
        // cross at most one subgraph border can promise a straight shaft here.
        let (exits, enters) = graph.edge_boundary_crossings(&from.id, &target.id);
        if let Some(lane_x) = (exits.len() + enters.len() <= 1)
            .then(|| straight_entry_x(from, target, stem_start_x, arrow_x, arrow_y, graph))
            .flatten()
        {
            arrow_x = lane_x;
            target_x = lane_x;
        }

        if matches!(direction, Direction::TD | Direction::TB) {
            if let Some(detour) =
                td_mixed_sibling_detour(from, target, canvas, style, spacing, graph)
            {
                canvas.allow_fallback_route_crossing(
                    &detour.plan.owner_id,
                    &detour.crossing_owner,
                    detour.crossing.0,
                    detour.crossing.1,
                );
                lower_td_fallback_plan(detour.plan, canvas, style, Some(route_owner));
                return;
            }
            let from_sg = graph.get_node_subgraph(&from.id);
            let to_sg = graph.get_node_subgraph(&target.id);
            if from_sg.is_some() && to_sg.is_none() {
                if let Some(transaction_x) =
                    td_single_outgoing_route_x(from, target, stem_start_x, arrow_y, graph)
                {
                    arrow_x = transaction_x;
                    target_x = transaction_x;
                }
            }
            let mut cross_arrow_x = if from_sg != to_sg {
                td_complex_title_portal_entry_x(from, target, stem_start_x, arrow_y, graph)
                    .or_else(|| td_single_incoming_route_x(from, target, arrow_x, arrow_y, graph))
                    .unwrap_or_else(|| {
                        title_safe_td_entry_x(target, arrow_x, arrow_y, stem_start_y, graph)
                    })
            } else {
                arrow_x
            };
            if context.diagnostics.cross {
                eprintln!(
                    "single-edge cross? {}({:?}) -> {}({:?})",
                    from.id, from_sg, target.id, to_sg
                );
            }
            if from_sg != to_sg
                && route_cross_subgraph_td(
                    from,
                    target,
                    stem_start_x,
                    stem_start_y,
                    &mut cross_arrow_x,
                    arrow_y,
                    canvas,
                    style,
                    graph,
                    Some(route_owner),
                )
            {
                set_target_end(
                    canvas,
                    &coords,
                    style,
                    graph,
                    from,
                    target,
                    (cross_arrow_x, arrow_y),
                    route_owner,
                );
                return;
            }
            if let Some((lane, plan)) =
                td_independent_frame_exit(from, target, junction_y, canvas, style, graph)
            {
                lower_td_fallback_plan(plan, canvas, style, Some(route_owner));
                set_target_end(
                    canvas,
                    &coords,
                    style,
                    graph,
                    from,
                    target,
                    (lane, arrow_y),
                    route_owner,
                );
                return;
            }
        } else if direction == Direction::BT {
            let from_sg = graph.get_node_subgraph(&from.id);
            let to_sg = graph.get_node_subgraph(&target.id);
            if from_sg != to_sg {
                match route_cross_subgraph_bt(
                    from,
                    target,
                    stem_start_x,
                    stem_start_y,
                    arrow_x,
                    arrow_y,
                    canvas,
                    style,
                    graph,
                    Some(route_owner),
                    Some(&mut arrow_x),
                ) {
                    BtRouteOutcome::Handled | BtRouteOutcome::Rejected => {
                        set_target_end(
                            canvas,
                            &coords,
                            style,
                            graph,
                            from,
                            target,
                            (arrow_x, arrow_y),
                            route_owner,
                        );
                        return;
                    }
                    BtRouteOutcome::NotApplicable => {}
                }
            }
        }

        if debug_timing {
            eprintln!("  single target centers ({src_x},{src_y}) -> ({arrow_x},{arrow_y})");
        }

        let src_secondary = coords.secondary_coord(src_x, src_y);
        let target_secondary = coords.secondary_coord(target_x, target_y);

        if src_secondary == target_secondary {
            // Aligned: straight line on primary axis
            draw_line_primary(
                stem_start_x,
                stem_start_y,
                arrow_x,
                arrow_y,
                &coords,
                canvas,
                style,
                Some(graph),
                Some(route_owner),
            );
            if matches!(direction, Direction::TD | Direction::TB) {
                if let (Some(from_sg), Some(to_sg)) = (
                    graph.get_node_subgraph(&from.id),
                    graph.get_node_subgraph(&target.id),
                ) {
                    if from_sg != to_sg {
                        if let Some(sg) = graph.get_subgraph(to_sg) {
                            let border_y = sg.bounds.y;
                            if arrow_x < canvas.width && border_y < canvas.height && !sg.has_title()
                            {
                                set_route_edge_char(
                                    canvas,
                                    arrow_x,
                                    border_y,
                                    style.junction_down,
                                    style,
                                    Some(route_owner),
                                );
                            }
                        }
                    }
                }
            }
        } else {
            // L-shaped route
            let going_before = is_before(src_secondary, target_secondary);

            // Horizontal layouts look cleaner when we turn toward the row first, then
            // travel horizontally to the target.
            if matches!(direction, Direction::LR | Direction::RL) {
                // If the target sits above/below the source, go vertical first so the
                // corner hugs the target row instead of floating off the junction stem.
                if target_secondary != src_secondary {
                    let spine_x = junction_x;

                    // Horizontal run out of the source before turning.
                    draw_line_primary(
                        stem_start_x,
                        stem_start_y,
                        spine_x,
                        stem_start_y,
                        &coords,
                        canvas,
                        style,
                        Some(graph),
                        Some(route_owner),
                    );

                    // Turn onto the vertical spine at the source row.
                    let going_up = target_secondary < src_secondary;
                    let corner1 = match direction {
                        Direction::LR => {
                            if going_up {
                                style.corner_ur
                            } else {
                                style.corner_dr
                            }
                        }
                        Direction::RL => {
                            if going_up {
                                style.corner_ul
                            } else {
                                style.corner_dl
                            }
                        }
                        _ => unreachable!(),
                    };
                    set_route_edge_char(
                        canvas,
                        spine_x,
                        stem_start_y,
                        corner1,
                        style,
                        Some(route_owner),
                    );

                    // Vertical segment to target row.
                    let (bend_x, bend_y) =
                        coords.with_secondary(spine_x, stem_start_y, target_secondary);
                    draw_line_secondary(
                        spine_x,
                        stem_start_y,
                        bend_x,
                        bend_y,
                        &coords,
                        canvas,
                        style,
                        Some(graph),
                        Some(route_owner),
                    );

                    // Turn toward the target column.
                    let corner2 = match direction {
                        Direction::LR => {
                            if going_up {
                                style.corner_dl
                            } else {
                                style.corner_ul
                            }
                        }
                        Direction::RL => {
                            if going_up {
                                style.corner_dr
                            } else {
                                style.corner_ur
                            }
                        }
                        _ => unreachable!(),
                    };
                    set_route_edge_char(canvas, bend_x, bend_y, corner2, style, Some(route_owner));

                    // Final horizontal run to the arrow.
                    let (seg_start_x, seg_start_y) = coords.advance(bend_x, bend_y, 1);
                    draw_line_primary(
                        seg_start_x,
                        seg_start_y,
                        arrow_x,
                        arrow_y,
                        &coords,
                        canvas,
                        style,
                        Some(graph),
                        Some(route_owner),
                    );
                    set_target_end(
                        canvas,
                        &coords,
                        style,
                        graph,
                        from,
                        target,
                        (arrow_x, arrow_y),
                        route_owner,
                    );
                    return;
                }

                let (bend_x, bend_y) =
                    coords.with_secondary(stem_start_x, stem_start_y, target_secondary);

                // Move vertically first
                draw_line_secondary(
                    stem_start_x,
                    stem_start_y,
                    bend_x,
                    bend_y,
                    &coords,
                    canvas,
                    style,
                    Some(graph),
                    Some(route_owner),
                );

                // Turn toward the target column
                let corner = coords.corner_secondary_to_end(going_before, style);
                set_route_edge_char(canvas, bend_x, bend_y, corner, style, Some(route_owner));

                // Final horizontal run to the arrow
                let (seg_start_x, seg_start_y) = coords.advance(bend_x, bend_y, 1);
                draw_line_primary(
                    seg_start_x,
                    seg_start_y,
                    arrow_x,
                    arrow_y,
                    &coords,
                    canvas,
                    style,
                    Some(graph),
                    Some(route_owner),
                );
            } else {
                // For BT fan-outs, the elbow row can overlap a previously rendered
                // convergence bar (e.g. a sibling fan-in into another target).
                // If so, prefer placing the elbow on the stem start row.
                if matches!(direction, Direction::BT) {
                    let (bend_x, _bend_y) =
                        coords.with_secondary(junction_x, junction_y, target_secondary);
                    let (x0, x1) = if junction_x <= bend_x {
                        (junction_x, bend_x)
                    } else {
                        (bend_x, junction_x)
                    };
                    let span_conflicts = if x1 > x0 + 1 {
                        ((x0 + 1)..x1).any(|x| canvas.get(x, junction_y) != ' ')
                    } else {
                        false
                    };
                    let junction_cell = canvas.get(junction_x, junction_y);
                    let junction_conflicts =
                        junction_cell != ' ' && !super::canvas::is_vertical(junction_cell, style);
                    if span_conflicts {
                        let (cand_x, cand_y) = coords.retreat(junction_x, junction_y, 1);
                        let stem_start_primary = coords.primary_coord(stem_start_x, stem_start_y);
                        let cand_primary = coords.primary_coord(cand_x, cand_y);
                        if cand_primary <= stem_start_primary {
                            let (cand_bx, _) =
                                coords.with_secondary(cand_x, cand_y, target_secondary);
                            let (cx0, cx1) = if cand_x <= cand_bx {
                                (cand_x, cand_bx)
                            } else {
                                (cand_bx, cand_x)
                            };
                            let cand_conflicts = if cx1 > cx0 + 1 {
                                ((cx0 + 1)..cx1).any(|x| canvas.get(x, cand_y) != ' ')
                            } else {
                                false
                            };
                            if !cand_conflicts {
                                junction_x = cand_x;
                                junction_y = cand_y;
                            }
                        }
                    } else if junction_conflicts {
                        // If we would immediately intersect an existing horizontal bar,
                        // prefer shifting the elbow down onto the stem row.
                        let (cand_x, cand_y) = coords.retreat(junction_x, junction_y, 1);
                        let stem_start_primary = coords.primary_coord(stem_start_x, stem_start_y);
                        if coords.primary_coord(cand_x, cand_y) <= stem_start_primary {
                            junction_x = cand_x;
                            junction_y = cand_y;
                        }
                    }
                }

                // 1. Stem from source
                draw_line_primary(
                    stem_start_x,
                    stem_start_y,
                    junction_x,
                    junction_y,
                    &coords,
                    canvas,
                    style,
                    Some(graph),
                    Some(route_owner),
                );

                // 2. Turn at junction
                let corner = coords.corner_start_to_secondary(going_before, style);
                set_route_edge_char(
                    canvas,
                    junction_x,
                    junction_y,
                    corner,
                    style,
                    Some(route_owner),
                );

                // 3. Secondary span to target column
                let (bend_x, bend_y) =
                    coords.with_secondary(junction_x, junction_y, target_secondary);
                draw_line_secondary(
                    junction_x,
                    junction_y,
                    bend_x,
                    bend_y,
                    &coords,
                    canvas,
                    style,
                    Some(graph),
                    Some(route_owner),
                );

                // 4. Turn to target
                let corner2 = coords.corner_secondary_to_end(going_before, style);
                set_route_edge_char(canvas, bend_x, bend_y, corner2, style, Some(route_owner));

                // 5. Final segment to arrow
                let (seg_start_x, seg_start_y) = coords.advance(bend_x, bend_y, 1);
                draw_line_primary(
                    seg_start_x,
                    seg_start_y,
                    arrow_x,
                    arrow_y,
                    &coords,
                    canvas,
                    style,
                    Some(graph),
                    Some(route_owner),
                );
            }
        }

        // Arrow at target
        set_target_end(
            canvas,
            &coords,
            style,
            graph,
            from,
            target,
            (arrow_x, arrow_y),
            route_owner,
        );

        // If the edge exits a subgraph boundary (TD/TB), stamp a junction on the border
        // so the vertical line visually pierces the container instead of leaving a gap.
        // A target beside the frame is reached through a side wall: no stamp.
        if matches!(direction, Direction::TD | Direction::TB) {
            if let Some(from_sg) = graph.get_node_subgraph(&from.id) {
                if graph.get_node_subgraph(&target.id) != Some(from_sg) {
                    if let Some(sg) = graph.get_subgraph(from_sg) {
                        let border_y = sg.bounds.y + sg.bounds.height.saturating_sub(1);
                        let below =
                            sg.bounds.x < arrow_x && arrow_x + 1 < sg.bounds.x + sg.bounds.width;
                        if below && arrow_x < canvas.width && border_y < canvas.height {
                            set_route_edge_char(
                                canvas,
                                arrow_x,
                                border_y,
                                style.junction_down,
                                style,
                                Some(route_owner),
                            );
                        }
                    }
                }
            }
        }
        return;
    }

    // Fan-out into a single subgraph: route to the subgraph interior before splitting
    // so junctions sit inside the container rather than on its border.
    if matches!(direction, Direction::TD | Direction::TB) {
        if let Some(target_sg_id) = target_positions
            .first()
            .and_then(|(_, _, n)| graph.get_node_subgraph(&n.id))
        {
            let source_sg = graph.get_node_subgraph(&from.id);
            let all_targets_same_sg = target_positions
                .iter()
                .all(|(_, _, n)| graph.get_node_subgraph(&n.id) == Some(target_sg_id));

            if all_targets_same_sg && source_sg != Some(target_sg_id) {
                if let Some(sg) = graph.get_subgraph(target_sg_id) {
                    if sg.bounds.is_valid() {
                        route_fanout_into_subgraph_td(
                            from,
                            &target_positions,
                            canvas,
                            style,
                            sg,
                            direction,
                            graph,
                        );
                        return;
                    }
                }
            }
        }
    }

    // Horizontal fan-out into one subgraph is one boundary scene: enter through
    // a single owned portal, keep a collector rail inside the boundary, and
    // branch to each target from that rail.  The generic fan-out below routes
    // each target independently and can leave the off-axis branches as
    // detached stubs when the source is outside a titled subgraph.
    if matches!(direction, Direction::LR | Direction::RL) && target_positions.len() > 1 {
        if let Some(target_sg_id) = target_positions
            .first()
            .and_then(|(_, _, n)| graph.get_node_subgraph(&n.id))
        {
            let source_sg = graph.get_node_subgraph(&from.id);
            let all_targets_same_sg = target_positions
                .iter()
                .all(|(_, _, n)| graph.get_node_subgraph(&n.id) == Some(target_sg_id));

            if all_targets_same_sg && source_sg != Some(target_sg_id) {
                if let Some(sg) = graph.get_subgraph(target_sg_id) {
                    if route_divergent_into_subgraph_horizontal(
                        from,
                        &visible_targets,
                        canvas,
                        style,
                        direction,
                        sg,
                        graph,
                    ) {
                        return;
                    }
                }
            }
        }
    }

    // Multiple targets: draw branching structure
    let fanout_owner_id = format!("fanout:{}", from.id);
    let fanout_owner = RouteOwner {
        kind: CellOwnerKind::EdgeSegment,
        id: fanout_owner_id.as_str(),
    };

    // 1. Draw stem from source to junction (not including junction)
    let stem_length = {
        let start_primary = coords.primary_coord(stem_start_x, stem_start_y);
        let junction_primary = coords.primary_coord(junction_x, junction_y);
        match direction {
            Direction::LR | Direction::TD | Direction::TB => {
                junction_primary.saturating_sub(start_primary)
            }
            Direction::RL | Direction::BT => start_primary.saturating_sub(junction_primary),
        }
    };
    for i in 0..stem_length {
        let (px, py) = coords.advance(stem_start_x, stem_start_y, i);
        set_route_edge_char(
            canvas,
            px,
            py,
            coords.primary_edge_char(style),
            style,
            Some(fanout_owner),
        );
    }

    // A TD drop into a frame through its title text, where the title has no
    // room to slide off it, leaves the bus clear of the title instead and
    // turns back under it (gen_a_060's Events -> HDFS through "Batch
    // Layer"): (drop column, row under the title).
    let title_step = |target: &Node, arrow_x: usize| -> Option<(usize, usize)> {
        if !matches!(direction, Direction::TD | Direction::TB) {
            return None;
        }
        let (entry_x, jog_y, under_y) =
            super::convergence::title_safe_frame_entry(graph, target, arrow_x)?;
        (junction_y <= jog_y).then_some((entry_x, under_y))
    };

    // 2. Calculate span on secondary axis
    let src_secondary = coords.secondary_coord(src_x, src_y);
    let target_secondaries: Vec<usize> = target_positions
        .iter()
        .map(|(_, _, target)| {
            let (arrow_x, arrow_y) =
                fanout_entry_point(target, src_x, &left_ports, junction_y, direction, graph);
            title_step(target, arrow_x).map_or_else(
                || coords.secondary_coord(arrow_x, arrow_y),
                |(entry_x, _)| entry_x,
            )
        })
        .collect();
    let first_secondary = target_secondaries
        .iter()
        .copied()
        .min()
        .unwrap_or(src_secondary);
    let last_secondary = target_secondaries
        .iter()
        .copied()
        .max()
        .unwrap_or(src_secondary);

    // A source beyond every target (a subgraph can push its exit past them)
    // still reaches the span: it extends to the source, which turns into it
    // on a corner.  Only across blank cells, so it never runs along another
    // route sharing that line, or across a frame's wall: targets dropped under
    // the source's frame take the bus through it (gen_c_122).
    let extension = if src_secondary < first_secondary {
        Some(src_secondary..first_secondary)
    } else if src_secondary > last_secondary {
        Some(last_secondary + 1..src_secondary + 1)
    } else {
        None
    };
    let source_beyond_targets = extension.is_some_and(|mut extension| {
        extension.all(|pos| {
            let (x, y) = coords.with_secondary(junction_x, junction_y, pos);
            canvas.get(x, y) == ' '
                || canvas
                    .get_meta(x, y)
                    .is_some_and(|meta| meta.owner_kind == CellOwnerKind::SubgraphBorder)
        })
    });
    let (span_start, span_end) = if source_beyond_targets {
        (
            first_secondary.min(src_secondary),
            last_secondary.max(src_secondary),
        )
    } else {
        (first_secondary, last_secondary)
    };

    // If the source column sits on the edge of the span, nudge the junction inward
    // so the bar keeps corners at both ends.
    let mut junction_secondary = src_secondary;
    if span_end > span_start && !source_beyond_targets {
        if junction_secondary == span_start {
            junction_secondary = span_start + 1;
        } else if junction_secondary == span_end {
            junction_secondary = span_end - 1;
        }
    }

    // 3. Draw horizontal junction span with corners and junction
    let (start_corner, end_corner) = match direction {
        Direction::TD | Direction::TB => (style.corner_dl, style.corner_dr),
        Direction::BT => (style.corner_ul, style.corner_ur),
        Direction::LR => (style.corner_dl, style.corner_ul),
        Direction::RL => (style.corner_dr, style.corner_ur),
    };
    let has_target_at_junction = target_positions
        .iter()
        .any(|(x, y, _)| coords.secondary_coord(*x, *y) == junction_secondary);

    // Collect target drop positions for junction placement
    for pos in span_start..=span_end {
        let (span_x, span_y) = coords.with_secondary(junction_x, junction_y, pos);

        // Check if this position is a target drop point (not the source junction)
        let is_target_drop = pos != junction_secondary && target_secondaries.contains(&pos);

        let c = if pos == junction_secondary && !source_beyond_targets {
            // Junction at source position - stem meets vertical span
            match direction {
                Direction::TD | Direction::TB => style.junction_up, // ┴
                Direction::LR => {
                    if has_target_at_junction {
                        style.junction_right // ├ (branch right on this row)
                    } else {
                        style.junction_left // ┤ (no right branch on this row)
                    }
                }
                Direction::RL => {
                    if has_target_at_junction {
                        style.junction_left // ┤ (branch left on this row)
                    } else {
                        style.junction_right // ├ (no left branch on this row)
                    }
                }
                Direction::BT => style.junction_down, // ┬ (stem below, branches above)
            }
        } else if pos == span_start {
            // Corner at start of span - corners already have correct shape for drops
            // e.g., ┌ (corner_dl) connects right and down, which is exactly what we need
            start_corner
        } else if pos == span_end {
            // Corner at end of span - corners already have correct shape for drops
            // e.g., ┐ (corner_dr) connects left and down, which is exactly what we need
            end_corner
        } else if is_target_drop {
            // Target drop in middle of span - needs T-junction
            match direction {
                Direction::TD | Direction::TB => style.junction_down, // ┬ (horizontal span, drop below)
                Direction::BT => style.junction_up, // ┴ (horizontal span, drop above)
                Direction::LR => style.junction_right, // ├ (vertical span, drop right)
                Direction::RL => style.junction_left, // ┤ (vertical span, drop left)
            }
        } else {
            coords.secondary_edge_char(style)
        };
        set_route_edge_char(canvas, span_x, span_y, c, style, Some(fanout_owner));
    }

    // Connect the source column to the adjusted junction column if we nudged it.
    if junction_secondary != src_secondary {
        let (sx, sy) = coords.with_secondary(junction_x, junction_y, src_secondary);
        let (jx, jy) = coords.with_secondary(junction_x, junction_y, junction_secondary);
        draw_line_secondary(
            sx,
            sy,
            jx,
            jy,
            &coords,
            canvas,
            style,
            Some(graph),
            Some(fanout_owner),
        );
    }

    // 4. Draw drops and arrows for each target
    for (_, _, target) in &target_positions {
        let branch_owner_id = edge_route_owner_id(graph, &from.id, &target.id);
        let branch_owner = RouteOwner {
            kind: CellOwnerKind::EdgeSegment,
            id: branch_owner_id.as_str(),
        };
        let edge_kind = graph
            .edges
            .iter()
            .find(|e| e.from == from.id && e.to == target.id && !e.is_back_edge)
            .map(|e| e.kind)
            .unwrap_or(EdgeKind::Arrow);
        let branch_style = style_for_edge_kind(style, edge_kind);
        let (arrow_x, arrow_y) =
            fanout_entry_point(target, src_x, &left_ports, junction_y, direction, graph);
        let step = title_step(target, arrow_x);
        let target_secondary = step.map_or_else(
            || coords.secondary_coord(arrow_x, arrow_y),
            |(entry_x, _)| entry_x,
        );

        // Draw vertical drop from junction+1 to arrow
        let (drop_x, drop_y) = coords.with_secondary(junction_x, junction_y, target_secondary);
        let (mut drop_start_x, mut drop_start_y) = coords.advance(drop_x, drop_y, 1);
        if let Some((entry_x, under_y)) = step {
            draw_line_primary(
                entry_x,
                drop_start_y,
                entry_x,
                under_y - 1,
                &coords,
                canvas,
                &branch_style,
                Some(graph),
                Some(branch_owner),
            );
            super::convergence::jog_row(canvas, style, branch_owner, (entry_x, arrow_x), under_y);
            (drop_start_x, drop_start_y) = (arrow_x, under_y + 1);
        }

        // Only draw if there's actually a drop to draw
        if drop_start_x != arrow_x || drop_start_y != arrow_y {
            draw_line_primary(
                drop_start_x,
                drop_start_y,
                arrow_x,
                arrow_y,
                &coords,
                canvas,
                &branch_style,
                Some(graph),
                Some(branch_owner),
            );
        }

        set_target_end(
            canvas,
            &coords,
            style,
            graph,
            from,
            target,
            (arrow_x, arrow_y),
            branch_owner,
        );

        // A custom branch shaft can otherwise downgrade the shared fan-out
        // junction while overlap resolution sees only the branch style. Restore
        // the base-style tee after drawing the branch so edge-kind emphasis does
        // not erase topology at the merge point.
        let (junction_cell_x, junction_cell_y) =
            coords.with_secondary(junction_x, junction_y, target_secondary);
        // A drop at either end of the span is an elbow, not a tee.  The
        // branch still terminates at the target arrow, but there is no arm
        // continuing past the endpoint.  Keeping the endpoint corner here
        // prevents the later branch write from turning a truthful `┌`/`┐`
        // (or its ASCII equivalent) into an ambiguous three-way junction.
        let junction = if target_secondary == span_start && span_start != junction_secondary {
            start_corner
        } else if target_secondary == span_end && span_end != junction_secondary {
            end_corner
        } else {
            match direction {
                Direction::TD | Direction::TB => style.junction_down,
                Direction::BT => style.junction_up,
                Direction::LR => style.junction_right,
                Direction::RL => style.junction_left,
            }
        };
        set_route_char(
            canvas,
            junction_cell_x,
            junction_cell_y,
            junction,
            Some(fanout_owner),
        );
    }

    // Reinforce clean corners at the ends of the span so drops don't turn them into tees.
    // Only override if the existing character is a primary edge (vertical/horizontal line
    // from our drops) - if it's a subgraph border or other structure, use normal overlap
    // resolution to create proper junctions.
    let (start_pos_x, start_pos_y) = coords.with_secondary(junction_x, junction_y, span_start);
    let (end_pos_x, end_pos_y) = coords.with_secondary(junction_x, junction_y, span_end);
    let primary_edge = coords.primary_edge_char(style);

    if span_start != junction_secondary {
        let existing = canvas.get(start_pos_x, start_pos_y);
        if existing == primary_edge || existing == ' ' {
            set_route_char(
                canvas,
                start_pos_x,
                start_pos_y,
                start_corner,
                Some(fanout_owner),
            );
        } else {
            set_route_edge_char(
                canvas,
                start_pos_x,
                start_pos_y,
                start_corner,
                style,
                Some(fanout_owner),
            );
        }
    }
    if span_end != junction_secondary {
        let existing = canvas.get(end_pos_x, end_pos_y);
        if existing == primary_edge || existing == ' ' {
            set_route_char(canvas, end_pos_x, end_pos_y, end_corner, Some(fanout_owner));
        } else {
            set_route_edge_char(
                canvas,
                end_pos_x,
                end_pos_y,
                end_corner,
                style,
                Some(fanout_owner),
            );
        }
    }

    // The generic horizontal fanout is still a renderer-owned fallback route,
    // even though its visible geometry is assembled from shared and branch
    // owners above. Record the exact primitive coverage after lowering so the
    // final evidence can validate every edge against the post-repair canvas.
    // This is intentionally limited to LR/RL here; vertical fallback scenes
    // have their own specialized route-plan seams.
    let mut edge_kinds = Vec::new();
    for (_, _, target) in &target_positions {
        if let Some(edge) = graph
            .edges
            .iter()
            .find(|edge| !edge.is_back_edge && edge.from == from.id && edge.to == target.id)
        {
            if !edge_kinds.contains(&edge.kind) {
                edge_kinds.push(edge.kind);
            }
        }
    }
    let can_record_fallback_plan = edge_kinds.len() >= 2
        && graph.subgraphs.is_empty()
        && matches!(direction, Direction::LR | Direction::RL)
        && target_positions.iter().all(|(_, _, target)| {
            graph
                .edges
                .iter()
                .find(|edge| !edge.is_back_edge && edge.from == from.id && edge.to == target.id)
                .is_some_and(|edge| edge.label.is_none())
        });
    if can_record_fallback_plan {
        let mut plan = FallbackRoutePlan::new(fanout_owner_id.clone(), "generic-horizontal-fanout");
        plan.set_scene_coverage(
            target_positions
                .iter()
                .map(|(_, _, target)| edge_route_owner_id(graph, &from.id, &target.id))
                .collect::<Vec<_>>(),
        );

        if stem_length > 0 {
            let (stem_end_x, _) = coords.advance(stem_start_x, stem_start_y, stem_length - 1);
            plan.push_horizontal(
                stem_start_y,
                stem_start_x,
                stem_end_x,
                coords.primary_edge_char(style),
            );
            if stem_length == 1 {
                plan.push_paint(
                    stem_start_x,
                    stem_start_y,
                    canvas.get(stem_start_x, stem_start_y),
                );
            }
        }

        plan.push_vertical(
            junction_x,
            span_start,
            span_end,
            coords.secondary_edge_char(style),
        );
        if span_start == span_end {
            let (span_x, span_y) = coords.with_secondary(junction_x, junction_y, span_start);
            plan.push_paint(span_x, span_y, canvas.get(span_x, span_y));
        }

        if junction_secondary != src_secondary {
            plan.push_vertical(
                junction_x,
                src_secondary,
                junction_secondary,
                coords.secondary_edge_char(style),
            );
        }

        for (_, _, target) in &target_positions {
            let edge_kind = graph
                .edges
                .iter()
                .find(|e| e.from == from.id && e.to == target.id && !e.is_back_edge)
                .map(|e| e.kind)
                .unwrap_or(EdgeKind::Arrow);
            let branch_style = style_for_edge_kind(style, edge_kind);
            let (arrow_x, arrow_y) =
                fanout_entry_point(target, src_x, &left_ports, junction_y, direction, graph);
            let target_secondary = coords.secondary_coord(arrow_x, arrow_y);
            let (drop_x, drop_y) = coords.with_secondary(junction_x, junction_y, target_secondary);
            let (drop_start_x, drop_start_y) = coords.advance(drop_x, drop_y, 1);
            if drop_start_x != arrow_x || drop_start_y != arrow_y {
                plan.push_horizontal(drop_start_y, drop_start_x, arrow_x, branch_style.edge_h);
            }
            plan.push_paint(arrow_x, arrow_y, canvas.get(arrow_x, arrow_y));
        }

        canvas.record_fallback_route_plan(plan);
    }

    // Generic vertical fan-outs use the same shared collector/edge-branch
    // ownership model as the horizontal fallback above. Record that split
    // explicitly for evidence, but keep the admission narrower: labeled and
    // subgraph routes have separate attachment semantics and must not be
    // claimed by this generic contract.
    let source_subgraph = graph.get_node_subgraph(&from.id);
    let intra_subgraph_fanout = source_subgraph.is_some()
        && target_positions
            .iter()
            .all(|(_, _, target)| graph.get_node_subgraph(&target.id) == source_subgraph);
    let can_record_vertical_fallback_plan = (graph.subgraphs.is_empty() || intra_subgraph_fanout)
        && matches!(direction, Direction::TD | Direction::TB | Direction::BT)
        && target_positions.iter().all(|(_, _, target)| {
            graph
                .edges
                .iter()
                .find(|edge| !edge.is_back_edge && edge.from == from.id && edge.to == target.id)
                .is_some_and(|edge| edge.label.is_none())
        });
    if can_record_vertical_fallback_plan {
        let mut plan = FallbackRoutePlan::new(fanout_owner_id, "generic-vertical-fanout");
        plan.set_scene_coverage(
            target_positions
                .iter()
                .map(|(_, _, target)| edge_route_owner_id(graph, &from.id, &target.id))
                .collect::<Vec<_>>(),
        );

        let primary_glyph = coords.primary_edge_char(style);
        if stem_length > 0 {
            let stem_end = coords.advance(stem_start_x, stem_start_y, stem_length - 1);
            push_axis_aligned_plan_segment(
                &mut plan,
                (stem_start_x, stem_start_y),
                stem_end,
                primary_glyph,
            );
        }

        let span_start_point = coords.with_secondary(junction_x, junction_y, span_start);
        let span_end_point = coords.with_secondary(junction_x, junction_y, span_end);
        push_axis_aligned_plan_segment(
            &mut plan,
            span_start_point,
            span_end_point,
            coords.secondary_edge_char(style),
        );

        if junction_secondary != src_secondary {
            let source_point = coords.with_secondary(junction_x, junction_y, src_secondary);
            let junction_point = coords.with_secondary(junction_x, junction_y, junction_secondary);
            push_axis_aligned_plan_segment(
                &mut plan,
                source_point,
                junction_point,
                coords.secondary_edge_char(style),
            );
        }

        for (_, _, target) in &target_positions {
            let edge_kind = graph
                .edges
                .iter()
                .find(|edge| !edge.is_back_edge && edge.from == from.id && edge.to == target.id)
                .map(|edge| edge.kind)
                .unwrap_or(EdgeKind::Arrow);
            let branch_style = style_for_edge_kind(style, edge_kind);
            let (arrow_x, arrow_y) =
                fanout_entry_point(target, src_x, &left_ports, junction_y, direction, graph);
            let target_secondary = coords.secondary_coord(arrow_x, arrow_y);
            let drop = coords.with_secondary(junction_x, junction_y, target_secondary);
            let drop_start = coords.advance(drop.0, drop.1, 1);
            if drop_start != (arrow_x, arrow_y) {
                push_axis_aligned_plan_segment(
                    &mut plan,
                    drop_start,
                    (arrow_x, arrow_y),
                    coords.primary_edge_char(&branch_style),
                );
            }
            plan.push_paint(arrow_x, arrow_y, canvas.get(arrow_x, arrow_y));
        }

        canvas.record_fallback_route_evidence(plan);
    }
}

/// Where a vertical fan-out branch enters its target.
///
/// A target in `left_ports` (see [`fan_out_target_uses_left_port`]) takes a
/// trunk coming down its left centre column straight into its branch (`┼`)
/// instead of landing beside it (`┴┬`).
fn fanout_entry_point(
    target: &Node,
    trunk_x: usize,
    left_ports: &[&str],
    bus_y: usize,
    direction: Direction,
    graph: &Graph,
) -> (usize, usize) {
    let entry = adjusted_edge_entry_point(target, direction, graph);
    if entry.0 == target.center_x()
        && trunk_x + 1 == entry.0
        && left_ports.contains(&target.id.as_str())
        && !hits_foreign_subgraph_border(target, trunk_x, entry.1, graph)
    {
        return (trunk_x, entry.1);
    }
    // A branch dropping down a frame's side wall, through another box or
    // right beside it would read as part of that wall or box; enter the target
    // a few columns over instead, keeping a blank column clear. The drop runs
    // from the bus row (which can itself lie on a frame border) to the arrow.
    let drop_rows = match direction {
        Direction::TD | Direction::TB => Some((bus_y, entry.1)),
        Direction::BT => Some((entry.1 + 1, bus_y + 1)),
        Direction::LR | Direction::RL => None,
    };
    if let Some((top, bottom)) = drop_rows {
        let on_side_wall = |x: usize| {
            graph.subgraphs.iter().any(|sg| {
                let b = &sg.bounds;
                b.is_valid()
                    && (x == b.x || x + 1 == b.x + b.width)
                    && b.y < bottom
                    && top < b.y + b.height
            })
        };
        // A self-loop's return leg runs two columns right of its box, so a
        // self-looping box claims those columns too.
        let hugs_box = |x: usize| {
            graph.nodes.iter().any(|node| {
                let loop_cols = if graph
                    .edges
                    .iter()
                    .any(|e| e.from == node.id && e.to == node.id)
                {
                    2
                } else {
                    0
                };
                node.id != target.id
                    && node.y < bottom
                    && top < node.y + node.height
                    && x + 1 >= node.x
                    && x <= node.x + node.width + loop_cols
            })
        };
        let blocked = |x: usize| on_side_wall(x) || hugs_box(x);
        // A centred drop one column beside the trunk would sit next to the
        // stem on the bus (`┴┬`); a flat contour spanning the trunk column
        // takes it there instead, so stem and drop meet in one junction.  Only
        // for a target past the bus along the flow: one behind it is reached
        // by a route that leaves the bus, which a shared column would merge.
        // And only for a target with no edge of its own leaving it: its exit
        // stays centred, and entry and exit must share one column.
        let past_bus = match direction {
            Direction::BT => entry.1 < bus_y,
            _ => entry.1 > bus_y,
        };
        let leaves_target = graph
            .edges
            .iter()
            .any(|edge| !edge.is_back_edge && edge.from == target.id);
        if past_bus
            && !leaves_target
            && entry.0 == target.center_x()
            && entry.0.abs_diff(trunk_x) == 1
            && target.shape.has_flat_vertical_entry()
            && trunk_x > target.x
            && trunk_x + 1 < target.x + target.width
            && !blocked(trunk_x)
            && !hits_foreign_subgraph_border(target, trunk_x, entry.1, graph)
        {
            return (trunk_x, entry.1);
        }
        if blocked(entry.0) {
            let inside_target = |x: usize| x > target.x && x + 1 < target.x + target.width;
            if let Some(x) = [
                entry.0.checked_sub(2),
                Some(entry.0 + 2),
                entry.0.checked_sub(1),
                Some(entry.0 + 1),
                entry.0.checked_sub(3),
                Some(entry.0 + 3),
                entry.0.checked_sub(4),
                Some(entry.0 + 4),
            ]
            .into_iter()
            .flatten()
            .find(|&x| inside_target(x) && !blocked(x))
            {
                return (x, entry.1);
            }
        }
    }
    entry
}

fn push_axis_aligned_plan_segment(
    plan: &mut FallbackRoutePlan,
    from: (usize, usize),
    to: (usize, usize),
    glyph: char,
) {
    if from.0 == to.0 {
        plan.push_vertical(from.0, from.1, to.1, glyph);
    } else if from.1 == to.1 {
        plan.push_horizontal(from.1, from.0, to.0, glyph);
    }
}

// These are intentionally separate geometry inputs: collapsing them into a
// context object would obscure the oriented source/junction/target contract
// this narrow fallback checks.
#[allow(clippy::too_many_arguments)]
fn mixed_vertical_fanout_clearance(
    target_positions: &[(usize, usize, &Node)],
    left_ports: &[&str],
    junction_x: usize,
    junction_y: usize,
    stem_start_x: usize,
    stem_start_y: usize,
    direction: Direction,
    graph: &Graph,
    canvas: &Canvas,
) -> Option<(usize, usize)> {
    let coords = OrientedCoords::new(direction);
    let candidate = coords.retreat(junction_x, junction_y, 1);
    let source_primary = coords.primary_coord(stem_start_x, stem_start_y);
    let candidate_primary = coords.primary_coord(candidate.0, candidate.1);

    // Do not move the junction through the source exit or outside the canvas.
    let candidate_is_between_source_and_targets = match direction {
        Direction::TD | Direction::TB => candidate_primary > source_primary,
        Direction::BT => candidate_primary < source_primary,
        Direction::LR | Direction::RL => false,
    };
    if !candidate_is_between_source_and_targets
        || candidate.0 >= canvas.width
        || candidate.1 >= canvas.height
    {
        return None;
    }

    let target_secondaries: Vec<usize> = target_positions
        .iter()
        .map(|(_, _, target)| {
            let (arrow_x, arrow_y) = fanout_entry_point(
                target,
                stem_start_x,
                left_ports,
                junction_y,
                direction,
                graph,
            );
            coords.secondary_coord(arrow_x, arrow_y)
        })
        .collect();
    let span_start = target_secondaries.iter().copied().min()?;
    let span_end = target_secondaries.iter().copied().max()?;

    // The candidate span and the one-cell target-facing drops must be free
    // before this route is lowered.  A crowded corridor should keep the old
    // geometry and remain visible to the perceptual review queue.
    for secondary in span_start..=span_end {
        let (x, y) = coords.with_secondary(candidate.0, candidate.1, secondary);
        if canvas.get(x, y) != ' ' {
            return None;
        }
    }
    for secondary in target_secondaries {
        let (drop_x, drop_y) = coords.with_secondary(candidate.0, candidate.1, secondary);
        let (shaft_x, shaft_y) = coords.advance(drop_x, drop_y, 1);
        if canvas.get(shaft_x, shaft_y) != ' ' {
            return None;
        }
    }

    Some(candidate)
}

const MIXED_BRANCH_JUNCTION_SEPARATION: usize = 3;
const MIXED_BRANCH_TARGET_CLEARANCE: usize = 3;

struct MixedBranchRequest<'a> {
    from: &'a Node,
    target: &'a Node,
    stem_start: (usize, usize),
    junction: (usize, usize),
    canvas: &'a Canvas,
    direction: Direction,
    graph: &'a Graph,
}

struct MixedBranchGeometry<'a> {
    stem_start: (usize, usize),
    arrow: (usize, usize),
    source_secondary: usize,
    target_secondary: usize,
    source_primary: usize,
    direction: Direction,
    canvas: &'a Canvas,
}

/// Select a measured LR/RL fan-out spine when the source also contributes to
/// another fan-in.  The fan-in pass runs first, so a candidate can be rejected
/// from the current canvas without teaching generic routes about a particular
/// fixture.  Returning `None` is the fail-closed path: the caller keeps its
/// ordinary L-route and does not claim a partial scene.
fn mixed_branch_junction(request: MixedBranchRequest<'_>) -> Option<(usize, usize)> {
    if !mixed_branch_topology_supported(request.from, request.target, request.graph)
        || !matches!(request.direction, Direction::LR | Direction::RL)
    {
        return None;
    }

    let coords = OrientedCoords::new(request.direction);
    let (source_x, source_y) = get_node_center(request.from);
    let (target_x, target_y) = get_node_center(request.target);
    let source_secondary = coords.secondary_coord(source_x, source_y);
    let target_secondary = coords.secondary_coord(target_x, target_y);
    if source_secondary == target_secondary {
        return None;
    }

    let arrow = adjusted_edge_entry_point(request.target, request.direction, request.graph);
    let source_primary = coords.primary_coord(request.stem_start.0, request.stem_start.1);
    let current_primary = coords.primary_coord(request.junction.0, request.junction.1);
    let arrow_primary = coords.primary_coord(arrow.0, arrow.1);
    let target_limit = match request.direction {
        Direction::LR => arrow_primary.saturating_sub(MIXED_BRANCH_TARGET_CLEARANCE),
        Direction::RL => arrow_primary.saturating_add(MIXED_BRANCH_TARGET_CLEARANCE),
        _ => return None,
    };

    let current_in_range = match request.direction {
        Direction::LR => current_primary > source_primary && current_primary <= target_limit,
        Direction::RL => current_primary < source_primary && current_primary >= target_limit,
        _ => false,
    };
    if !current_in_range {
        return None;
    }

    let primary_span = match request.direction {
        Direction::LR => target_limit.saturating_sub(current_primary),
        Direction::RL => current_primary.saturating_sub(target_limit),
        _ => return None,
    };

    for delta in 0..=primary_span {
        let candidate_primary = match request.direction {
            Direction::LR => current_primary.saturating_add(delta),
            Direction::RL => current_primary.saturating_sub(delta),
            _ => return None,
        };
        let mut candidate = request.junction;
        coords.set_primary(&mut candidate.0, &mut candidate.1, candidate_primary);

        let geometry = MixedBranchGeometry {
            stem_start: request.stem_start,
            arrow,
            source_secondary,
            target_secondary,
            source_primary,
            direction: request.direction,
            canvas: request.canvas,
        };
        if mixed_branch_candidate_is_clear(candidate, &geometry) {
            return Some(candidate);
        }
    }

    None
}

/// The candidate is intentionally narrower than a generic graph-family
/// detector.  Unsupported visual semantics remain on their existing route
/// owners instead of being partially claimed by this helper.
fn mixed_branch_topology_supported(from: &Node, target: &Node, graph: &Graph) -> bool {
    if !graph.subgraphs.is_empty()
        || graph.nodes.iter().any(|node| {
            !matches!(
                node.shape,
                crate::graph::NodeShape::Rectangle | crate::graph::NodeShape::Database
            )
        })
        || graph
            .edges
            .iter()
            .any(|edge| edge.is_back_edge || edge.label.is_some() || edge.kind != EdgeKind::Arrow)
    {
        return false;
    }

    let outgoing: Vec<&str> = graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge && edge.from == from.id)
        .map(|edge| edge.to.as_str())
        .collect();
    if outgoing.len() < 2 || !outgoing.iter().any(|id| *id == target.id) {
        return false;
    }

    // The selected branch is mixed only when another outgoing target has a
    // real fan-in.  The current target itself is the remaining divergence
    // edge after the convergence pass, so it normally has one incoming edge.
    outgoing.iter().any(|id| {
        *id != target.id.as_str()
            && graph
                .edges
                .iter()
                .filter(|edge| !edge.is_back_edge && edge.to == *id)
                .count()
                > 1
    })
}

fn mixed_branch_candidate_is_clear(
    candidate: (usize, usize),
    geometry: &MixedBranchGeometry<'_>,
) -> bool {
    let coords = OrientedCoords::new(geometry.direction);
    let candidate_primary = coords.primary_coord(candidate.0, candidate.1);

    // A foreign fan-in junction may remain on the shared source shaft, but a
    // new fan-out corner cannot be adjacent to it: that is the exact visual
    // ambiguity this candidate is meant to remove.
    let source_end_primary = coords.primary_coord(geometry.arrow.0, geometry.stem_start.1);
    let source_span_start = geometry.source_primary.min(source_end_primary);
    let source_span_end = geometry.source_primary.max(source_end_primary);
    for primary in source_span_start..=source_span_end {
        let mut point = geometry.stem_start;
        coords.set_primary(&mut point.0, &mut point.1, primary);
        if geometry
            .canvas
            .get_meta(point.0, point.1)
            .is_some_and(|meta| matches!(meta.role, CellRole::Junction | CellRole::Corner))
            && point.0.abs_diff(candidate.0) + point.1.abs_diff(candidate.1)
                < MIXED_BRANCH_JUNCTION_SEPARATION
        {
            return false;
        }
    }

    // The candidate spine and its target-row bend must be empty.  The source
    // row is excluded here because the source shaft can legitimately share a
    // horizontal fan-in corridor; the junction-separation check above keeps
    // that sharing visually attributable.
    let secondary_start = geometry.source_secondary.min(geometry.target_secondary);
    let secondary_end = geometry.source_secondary.max(geometry.target_secondary);
    for secondary in secondary_start..=secondary_end {
        let point = coords.with_secondary(candidate.0, candidate.1, secondary);
        if secondary != geometry.source_secondary && geometry.canvas.get(point.0, point.1) != ' ' {
            return false;
        }
    }

    // Keep the final target-row approach clear through the arrow.  The arrow
    // cell itself is still empty at this stage, but including it here makes
    // the capacity proof explicit and direction-neutral.
    let final_start = match geometry.direction {
        Direction::LR => candidate_primary.saturating_add(1),
        Direction::RL => candidate_primary.saturating_sub(1),
        _ => return false,
    };
    let final_end = coords.primary_coord(geometry.arrow.0, geometry.arrow.1);
    let (low, high) = (final_start.min(final_end), final_start.max(final_end));
    for primary in low..=high {
        let mut point = (candidate.0, geometry.arrow.1);
        coords.set_primary(&mut point.0, &mut point.1, primary);
        if geometry.canvas.get(point.0, point.1) != ' ' {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod mixed_branch_tests {
    use super::{mixed_branch_junction, mixed_branch_topology_supported, MixedBranchRequest};
    use crate::graph::Direction;
    use crate::graph::{Edge, Graph, Node, NodeShape};
    use crate::render::canvas::Canvas;
    use crate::render::semantic::CellOwnerKind;

    fn mixed_graph() -> Graph {
        let mut graph = Graph::new();
        graph.direction = Direction::LR;

        let mut gateway = Node::new("Gateway", "API Gateway");
        gateway.x = 0;
        gateway.y = 4;
        gateway.width = 17;
        gateway.height = 3;
        let mut auth = Node::new("Auth", "Auth Service");
        auth.x = 27;
        auth.y = 6;
        auth.width = 18;
        auth.height = 3;
        let mut api = Node::new("API", "Main API");
        api.x = 27;
        api.y = 2;
        api.width = 14;
        api.height = 3;
        let mut cache = Node::new("Cache", "Redis Cache");
        cache.x = 55;
        cache.y = 0;
        cache.width = 17;
        cache.height = 3;
        let mut database = Node::with_shape("DB", "Database", NodeShape::Database);
        database.x = 55;
        database.y = 4;
        database.width = 14;
        database.height = 3;

        for node in [gateway, auth, api, cache, database] {
            graph.add_node(node);
        }
        graph.add_edge(Edge::new("Gateway", "Auth"));
        graph.add_edge(Edge::new("Gateway", "API"));
        graph.add_edge(Edge::new("Auth", "DB"));
        graph.add_edge(Edge::new("API", "DB"));
        graph.add_edge(Edge::new("API", "Cache"));
        graph
    }

    #[test]
    fn mixed_role_predicate_accepts_database_target_family() {
        let graph = mixed_graph();
        let api = graph.get_node("API").expect("API node");
        let cache = graph.get_node("Cache").expect("Cache node");
        assert!(mixed_branch_topology_supported(api, cache, &graph));
    }

    #[test]
    fn mixed_role_predicate_rejects_labeled_variants() {
        let mut graph = mixed_graph();
        graph.edges[4].label = Some("cache".to_owned());
        let api = graph.get_node("API").expect("API node");
        let cache = graph.get_node("Cache").expect("Cache node");
        assert!(!mixed_branch_topology_supported(api, cache, &graph));
    }

    #[test]
    fn mixed_role_candidate_leaves_two_cells_between_foreign_junctions() {
        let graph = mixed_graph();
        let api = graph.get_node("API").expect("API node");
        let cache = graph.get_node("Cache").expect("Cache node");
        let mut canvas = Canvas::new(80, 10);
        canvas.set_owned(48, 3, '+', CellOwnerKind::EdgeSegment, "fanin:DB", 5);

        assert_eq!(
            mixed_branch_junction(MixedBranchRequest {
                from: api,
                target: cache,
                stem_start: (41, 3),
                junction: (47, 3),
                canvas: &canvas,
                direction: Direction::LR,
                graph: &graph,
            }),
            Some((51, 3))
        );
    }
}

#[cfg(test)]
mod generic_fanout_tests {
    use super::{push_axis_aligned_plan_segment, route_divergent_edges, td_independent_frame_exit};
    use crate::graph::{Direction, Edge, Graph, Node, NodeShape, Rectangle, Subgraph};
    use crate::render::canvas::Canvas;
    use crate::render::fallback_route::FallbackRoutePlan;
    use crate::render::semantic::CellOwnerKind;
    use crate::spacing::SpacingConfig;
    use crate::style::ASCII_CHARS;

    /// Only x=21 overlaps the two flat contours, so a blocked lane cannot
    /// silently fall back to a different column in the collision controls.
    fn independent_exit_scene() -> (Graph, Canvas) {
        let mut graph = Graph::new();
        let mut source = Node::new("S", "Source");
        (source.x, source.y, source.width, source.height) = (10, 2, 14, 3);
        let mut target = Node::new("T", "Target");
        (target.x, target.y, target.width, target.height) = (20, 9, 9, 3);
        graph.add_node(source);
        graph.add_node(target);
        graph.add_edge(Edge::new("S", "T"));
        graph.add_edge(Edge::new("S", "Lake"));
        let mut frame = Subgraph::new("Frame", None);
        frame.bounds = Rectangle::new(8, 0, 18, 8);
        graph.add_subgraph(frame);
        graph.associate_node_with_subgraph("S", "Frame");
        let mut canvas = Canvas::new(40, 16);
        for x in 9..25 {
            canvas.set_owned(x, 7, '-', CellOwnerKind::SubgraphBorder, "Frame", 3);
        }
        (graph, canvas)
    }

    fn independent_exit(graph: &Graph, canvas: &Canvas) -> Option<(usize, FallbackRoutePlan)> {
        td_independent_frame_exit(
            graph.get_node("S")?,
            graph.get_node("T")?,
            7,
            canvas,
            &ASCII_CHARS,
            graph,
        )
    }

    #[test]
    fn an_independent_exit_claims_only_its_clear_downward_wall_crossing() {
        let (graph, canvas) = independent_exit_scene();
        let (lane, plan) = independent_exit(&graph, &canvas).expect("clear drop");
        assert_eq!(lane, 21);
        assert_eq!(plan.boundary_claims.len(), 1);
        assert_eq!(
            (plan.boundary_claims[0].x, plan.boundary_claims[0].y),
            (21, 7)
        );
        assert!(plan.claims_cell(21, 4) && plan.claims_cell(21, 8));

        let controls: &[(&str, fn(&mut Graph))] = &[
            ("opposite direction", |g| g.direction = Direction::BT),
            ("non-flat source", |g| g.nodes[0].shape = NodeShape::Circle),
            ("non-flat target", |g| g.nodes[1].shape = NodeShape::Diamond),
            ("one source exit", |g| {
                g.edges.pop();
            }),
            ("shared target entry", |g| {
                g.edges.push(Edge::new("Other", "T"))
            }),
            ("labelled branch", |g| {
                g.edges[0].label = Some("label".into())
            }),
            ("entry left of source contour", |g| {
                g.nodes[1].x = 10;
                g.nodes[1].width = 3;
            }),
            ("entry right of source contour", |g| {
                g.nodes[1].x = 24;
                g.nodes[1].width = 3;
            }),
            ("entry on the shared source stem", |g| {
                g.nodes[1].x = 16;
                g.nodes[1].width = 3;
            }),
            ("wall above source", |g| g.nodes[0].y = 6),
            ("source outside frame top", |g| {
                g.subgraphs[0].bounds.y = 5;
                g.subgraphs[0].bounds.height = 3;
            }),
            ("wall below arrow", |g| g.nodes[1].y = 7),
            ("turn off wall", |g| g.subgraphs[0].bounds.height = 7),
            ("invalid frame", |g| g.subgraphs[0].bounds.height = 0),
            ("zero-height frame at the turn", |g| {
                g.subgraphs[0].bounds.y = 8;
                g.subgraphs[0].bounds.height = 0;
            }),
            ("lane on left frame side", |g| {
                g.subgraphs[0].bounds.x = 21;
            }),
            ("lane on frame side", |g| g.subgraphs[0].bounds.width = 14),
            ("nested exit", |g| {
                let mut outer = Subgraph::new("Outer", None);
                outer.bounds = Rectangle::new(6, 0, 24, 12);
                g.subgraphs[0].parent_id = Some("Outer".into());
                g.add_subgraph(outer);
            }),
            ("entry into another frame", |g| {
                let mut target_frame = Subgraph::new("TargetFrame", None);
                target_frame.bounds = Rectangle::new(0, 0, 40, 16);
                g.add_subgraph(target_frame);
                g.associate_node_with_subgraph("T", "TargetFrame");
            }),
            ("node in the lane before box painting", |g| {
                let mut obstacle = Node::new("Obstacle", "Obstacle");
                (obstacle.x, obstacle.y, obstacle.width, obstacle.height) = (20, 5, 3, 2);
                g.add_node(obstacle);
            }),
            ("unrelated frame in the lane", |g| {
                let mut obstacle = Subgraph::new("Obstacle", None);
                obstacle.bounds = Rectangle::new(20, 5, 3, 2);
                g.add_subgraph(obstacle);
            }),
        ];
        for &(name, change) in controls {
            let mut changed = graph.clone();
            change(&mut changed);
            assert!(independent_exit(&changed, &canvas).is_none(), "{name}");
        }
    }

    #[test]
    fn an_independent_exit_does_not_overwrite_another_route() {
        let (graph, canvas) = independent_exit_scene();
        for (x, y, glyph) in [(21, 5, '|'), (21, 7, '-')] {
            let mut blocked = canvas.clone();
            blocked.set_owned(x, y, glyph, CellOwnerKind::EdgeSegment, "other", 5);
            assert!(
                independent_exit(&graph, &blocked).is_none(),
                "occupied ({x},{y})"
            );
        }
        let mut stray_wall = canvas.clone();
        stray_wall.set_owned(21, 5, '-', CellOwnerKind::SubgraphBorder, "Frame", 3);
        assert!(
            independent_exit(&graph, &stray_wall).is_none(),
            "a wall stroke off the crossed border"
        );
        let mut broken_wall = canvas.clone();
        broken_wall.set_owned(21, 7, '|', CellOwnerKind::SubgraphBorder, "Frame", 3);
        assert!(
            independent_exit(&graph, &broken_wall).is_none(),
            "a non-horizontal wall cell"
        );
        for y in [4, 5, 8] {
            let mut reserved = canvas.clone();
            let mut plan = FallbackRoutePlan::new("other", "reserved-drop");
            plan.push_paint(21, y, '|');
            reserved.record_fallback_route_plan(plan);
            assert!(
                independent_exit(&graph, &reserved).is_none(),
                "reserved (21,{y})"
            );
        }
        assert!(
            independent_exit(&graph, &Canvas::new(20, 16)).is_none(),
            "out of canvas"
        );
    }

    #[test]
    fn plan_helper_records_vertical_and_horizontal_segments() {
        let mut plan = FallbackRoutePlan::new("fanout:R", "test");
        push_axis_aligned_plan_segment(&mut plan, (2, 1), (2, 3), '|');
        push_axis_aligned_plan_segment(&mut plan, (2, 3), (5, 3), '-');

        let cells = plan.planned_cells();
        assert_eq!(plan.segments.len(), 2);
        assert!(cells.contains(&(2, 1)));
        assert!(cells.contains(&(2, 3)));
        assert!(cells.contains(&(5, 3)));
    }

    #[test]
    fn generic_vertical_fanout_records_a_clean_shared_route_plan() {
        let mut graph = Graph::new();
        graph.direction = Direction::BT;

        let mut source = Node::new("R", "R");
        source.x = 12;
        source.y = 12;
        let mut targets = Vec::new();
        for (id, x) in [("S1", 0), ("S2", 12), ("S3", 24)] {
            let mut target = Node::new(id, id);
            target.x = x;
            target.y = 0;
            targets.push(target);
        }
        graph.add_node(source);
        for target in targets {
            graph.add_node(target);
        }
        for target in ["S1", "S2", "S3"] {
            graph.add_edge(Edge::new("R", target));
        }

        let source = graph.get_node("R").expect("source node");
        let target_refs = ["S1", "S2", "S3"]
            .into_iter()
            .map(|id| graph.get_node(id).expect("target node"))
            .collect::<Vec<_>>();
        let mut canvas = Canvas::new(40, 20);
        route_divergent_edges(
            source,
            &target_refs,
            &mut canvas,
            &ASCII_CHARS,
            &SpacingConfig::default(),
            Direction::BT,
            &graph,
        );

        let trace = canvas
            .fallback_route_traces()
            .into_iter()
            .find(|trace| trace.strategy == "generic-vertical-fanout")
            .expect("generic vertical fanout trace");
        assert_eq!(trace.covered_edge_ids.len(), 3);
        assert!(trace.mismatches.is_empty(), "{:?}", trace.mismatches);
    }

    fn mixed_sibling_detour_test_graph() -> Graph {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;
        for (id, x, y, width) in [
            ("S", 5, 16, 15),
            ("Peer", 10, 29, 13),
            ("PG", 3, 57, 14),
            ("T", 16, 43, 3),
            ("Store", 22, 57, 8),
        ] {
            let mut node = Node::new(id, id);
            (node.x, node.y, node.width, node.height) = (x, y, width, 3);
            graph.add_node(node);
        }
        for (from, to) in [("S", "PG"), ("S", "T"), ("Peer", "PG"), ("Peer", "Store")] {
            graph.add_edge(Edge::new(from, to));
        }
        for (id, title, bounds) in [
            ("A", "API Tier", Rectangle::new(2, 12, 24, 23)),
            ("B", "Data Stores", Rectangle::new(0, 39, 32, 24)),
        ] {
            let mut frame = Subgraph::new(id, Some(title.into()));
            frame.bounds = bounds;
            graph.add_subgraph(frame);
        }
        for id in ["S", "Peer"] {
            graph.associate_node_with_subgraph(id, "A");
        }
        for id in ["PG", "T", "Store"] {
            graph.associate_node_with_subgraph(id, "B");
        }
        graph
    }

    fn mixed_sibling_detour_test_canvas(graph: &Graph, style: &crate::style::StyleChars) -> Canvas {
        let mut canvas = Canvas::new(36, 66);
        let owner = super::edge_route_owner_id(graph, "S", "PG");
        let mut earlier = FallbackRoutePlan::new(&owner, "test-prior-independent-pg");
        earlier.set_scene_coverage([owner]);
        earlier.push_vertical(8, 18, 56, style.edge_v);
        canvas.record_fallback_route_plan(earlier);
        // Repaint from the mutated graph in topology/geometry controls, so a
        // stale wall or changed edge index cannot accidentally kill the control.
        let a = &graph.get_subgraph("A").expect("source frame").bounds;
        if a.width > 0 {
            for y in a.y..a.y + a.height {
                canvas.set_owned(
                    a.x + a.width - 1,
                    y,
                    style.edge_v,
                    CellOwnerKind::SubgraphBorder,
                    "A",
                    3,
                );
            }
        }
        let b = &graph.get_subgraph("B").expect("target frame").bounds;
        for x in b.x..b.x + b.width {
            canvas.set_owned(x, b.y, style.edge_h, CellOwnerKind::SubgraphBorder, "B", 3);
        }
        canvas
    }

    fn mixed_sibling_detour_test_plan(
        graph: &Graph,
        canvas: &Canvas,
        style: &crate::style::StyleChars,
    ) -> Option<super::MixedSiblingDetour> {
        super::td_mixed_sibling_detour(
            graph.get_node("S")?,
            graph.get_node("T")?,
            canvas,
            style,
            &SpacingConfig::default(),
            graph,
        )
    }

    #[test]
    fn a_mixed_sibling_detour_has_owned_wall_claims_and_one_future_crossing() {
        for style in [&crate::style::ASCII_CHARS, &crate::style::UNICODE_CHARS] {
            for direction in [Direction::TD, Direction::TB] {
                let mut graph = mixed_sibling_detour_test_graph();
                graph.direction = direction;
                let canvas = mixed_sibling_detour_test_canvas(&graph, style);
                let detour = mixed_sibling_detour_test_plan(&graph, &canvas, style)
                    .expect("clear right-frame exit with one future crossing");
                let owner = super::edge_route_owner_id(&graph, "S", "T");
                let crossing_owner = super::edge_route_owner_id(&graph, "Peer", "Store");
                assert_eq!(detour.plan.owner_id, owner);
                assert_eq!(detour.plan.strategy, super::MIXED_SIBLING_DETOUR);
                assert_eq!(detour.plan.covered_edge_ids, vec![owner.clone()]);
                assert_eq!(detour.crossing_owner, crossing_owner);
                assert_eq!(detour.crossing, (17, 38));
                assert_eq!(detour.plan.boundary_claims.len(), 2);
                assert!(detour.plan.claims_boundary("A", "right", 25, 19));
                assert!(detour.plan.claims_boundary("B", "top", 17, 39));
                assert!(detour.plan.validation_error(36, 66).is_none());
                for (x, y) in [
                    (12, 18),
                    (12, 19),
                    (25, 19),
                    (27, 19),
                    (27, 25),
                    (27, 36),
                    (17, 36),
                    (17, 38),
                    (17, 39),
                    (17, 42),
                ] {
                    assert!(detour.plan.claims_cell(x, y), "missing ({x},{y})");
                }
                assert!(!detour.plan.claims_cell(8, 36), "PG shaft remains separate");
                assert!(
                    !detour.plan.claims_cell(16, 29),
                    "peer box remains untouched"
                );
                let mut lowered = canvas.clone();
                assert!(!lowered.planned_fallback_crossing_allowed(17, 38, &crossing_owner));
                lowered.allow_fallback_route_crossing(&owner, &crossing_owner, 17, 38);
                super::lower_td_fallback_plan(
                    detour.plan,
                    &mut lowered,
                    style,
                    Some(super::RouteOwner {
                        kind: CellOwnerKind::EdgeSegment,
                        id: &owner,
                    }),
                );
                assert!(lowered.planned_fallback_crossing_allowed(17, 38, &crossing_owner));
                assert!(!lowered.planned_fallback_crossing_allowed(17, 37, &crossing_owner));
                assert!(!lowered.planned_fallback_crossing_allowed(17, 38, "foreign"));
                assert_eq!(lowered.get(25, 19), style.edge_h);
                assert_eq!(lowered.get(17, 39), style.edge_v);
                assert_eq!(lowered.get(12, 18), style.junction_down);
                assert_eq!(lowered.get(17, 42), style.arrow_down);
            }
        }
    }

    #[test]
    fn a_mixed_sibling_detour_rejects_unsupported_topology_and_geometry() {
        let graph = mixed_sibling_detour_test_graph();
        let controls: &[(&str, fn(&mut Graph))] = &[
            ("BT", |g| g.direction = Direction::BT),
            ("LR", |g| g.direction = Direction::LR),
            ("RL", |g| g.direction = Direction::RL),
            ("curved source", |g| g.nodes[0].shape = NodeShape::Circle),
            ("curved target", |g| g.nodes[3].shape = NodeShape::Diamond),
            ("curved future crossing target", |g| {
                g.nodes[4].shape = NodeShape::Diamond
            }),
            ("only one source exit", |g| {
                g.edges.remove(1);
            }),
            ("third source exit", |g| {
                g.add_node(Node::new("Extra", "Extra"));
                g.edges.push(Edge::new("S", "Extra"));
            }),
            ("two target entries", |g| {
                g.edges.push(Edge::new("Other", "T"))
            }),
            ("three collector entries", |g| {
                g.edges.push(Edge::new("Other", "PG"))
            }),
            ("only one peer exit", |g| {
                g.edges.pop();
            }),
            ("third peer exit", |g| {
                g.add_node(Node::new("Extra", "Extra"));
                g.edges.push(Edge::new("Peer", "Extra"));
            }),
            ("two future crossing-target entries", |g| {
                g.edges.push(Edge::new("Other", "Store"))
            }),
            ("source edge label", |g| {
                g.edges[1].label = Some("label".into())
            }),
            ("source back edge", |g| g.edges[1].is_back_edge = true),
            ("source open edge", |g| {
                g.edges[1].kind = crate::graph::EdgeKind::Open
            }),
            ("peer edge label", |g| {
                g.edges[3].label = Some("label".into())
            }),
            ("peer back edge", |g| g.edges[3].is_back_edge = true),
            ("peer open edge", |g| {
                g.edges[3].kind = crate::graph::EdgeKind::Open
            }),
            ("peer in target frame", |g| {
                g.associate_node_with_subgraph("Peer", "B")
            }),
            ("future crossing target in source frame", |g| {
                g.associate_node_with_subgraph("Store", "A")
            }),
            ("source touches frame top", |g| g.nodes[0].y = 12),
            ("source touches frame side", |g| g.nodes[0].x = 2),
            ("source reaches exit wall", |g| g.nodes[0].width = 21),
            ("peer overlaps source", |g| g.nodes[1].y = 18),
            ("peer reaches source bottom wall", |g| g.nodes[1].y = 31),
            ("no obstructing peer", |g| g.nodes[1].x = 20),
            ("target lane left of peer center", |g| g.nodes[3].x = 13),
            ("target lane against exit shoulder", |g| g.nodes[3].x = 22),
            ("future crossing target lacks horizontal clearance", |g| {
                g.nodes[4].x = 14
            }),
            ("future crossing target too close vertically", |g| {
                g.nodes[4].y = 47
            }),
            ("no gap between frames", |g| g.subgraphs[1].bounds.y = 38),
            ("target arrow touches title band", |g| g.nodes[3].y = 42),
            ("invalid source frame", |g| g.subgraphs[0].bounds.width = 0),
            ("invalid target frame", |g| g.subgraphs[1].bounds.height = 0),
            ("full title does not fit", |g| {
                g.subgraphs[1].title =
                    Some("Data Stores title that cannot fit this frame at all".into())
            }),
            ("nested exit", |g| {
                let mut outer = Subgraph::new("Outer", None);
                outer.bounds = Rectangle::new(0, 0, 36, 66);
                g.add_subgraph(outer);
                g.subgraphs[0].parent_id = Some("Outer".into());
            }),
            ("nested entry", |g| {
                let mut outer = Subgraph::new("Outer", None);
                outer.bounds = Rectangle::new(0, 0, 36, 66);
                g.add_subgraph(outer);
                g.subgraphs[1].parent_id = Some("Outer".into());
            }),
            ("foreign node on outside shaft", |g| {
                let mut node = Node::new("Block", "Block");
                (node.x, node.y, node.width, node.height) = (26, 25, 3, 3);
                g.add_node(node);
            }),
            ("foreign frame on outside shaft", |g| {
                let mut frame = Subgraph::new("Block", None);
                frame.bounds = Rectangle::new(26, 25, 3, 3);
                g.add_subgraph(frame);
            }),
        ];
        for style in [&crate::style::ASCII_CHARS, &crate::style::UNICODE_CHARS] {
            for &(name, mutate) in controls {
                let mut changed = graph.clone();
                mutate(&mut changed);
                let canvas = mixed_sibling_detour_test_canvas(&changed, style);
                assert!(
                    mixed_sibling_detour_test_plan(&changed, &canvas, style).is_none(),
                    "{name}"
                );
            }
            let canvas = mixed_sibling_detour_test_canvas(&graph, style);
            assert!(
                mixed_sibling_detour_test_plan(&graph, &Canvas::new(36, 66), style).is_none(),
                "prior independent branch must be reserved"
            );
            // Preserve the prior coverage so each clipped-canvas control tests bounds.
            for (width, height) in [(27, 66), (36, 42)] {
                let mut clipped = Canvas::new(width, height);
                let owner = super::edge_route_owner_id(&graph, "S", "PG");
                let mut prior = FallbackRoutePlan::new(&owner, "test-clipped-prior");
                prior.set_scene_coverage([owner]);
                prior.push_vertical(8, 18, 56, style.edge_v);
                clipped.record_fallback_route_plan(prior);
                assert!(
                    mixed_sibling_detour_test_plan(&graph, &clipped, style).is_none(),
                    "canvas {width}x{height}"
                );
            }
            assert!(
                mixed_sibling_detour_test_plan(&graph, &canvas, style).is_some(),
                "negative controls must preserve a live positive fixture"
            );
        }
    }

    #[test]
    fn a_mixed_sibling_detour_preserves_reserved_owned_and_aimed_cells() {
        let graph = mixed_sibling_detour_test_graph();
        for style in [&crate::style::ASCII_CHARS, &crate::style::UNICODE_CHARS] {
            let canvas = mixed_sibling_detour_test_canvas(&graph, style);
            for (x, y) in [
                (12, 18),
                (12, 19),
                (25, 19),
                (27, 25),
                (17, 38),
                (17, 39),
                (17, 41),
                (17, 42),
            ] {
                let mut reserved = canvas.clone();
                let mut prior = FallbackRoutePlan::new("foreign", "test-reservation");
                prior.push_paint(x, y, style.edge_v);
                reserved.record_fallback_route_plan(prior);
                assert!(
                    mixed_sibling_detour_test_plan(&graph, &reserved, style).is_none(),
                    "reserved ({x},{y})"
                );
                for (glyph, kind) in [
                    (' ', CellOwnerKind::EdgeSegment),
                    (' ', CellOwnerKind::NodeLabel),
                    (' ', CellOwnerKind::EdgeLabel),
                    (style.edge_h, CellOwnerKind::EdgeSegment),
                    ('X', CellOwnerKind::NodeLabel),
                    ('X', CellOwnerKind::EdgeLabel),
                ] {
                    let mut owned = canvas.clone();
                    owned.set_owned(x, y, glyph, kind, "foreign", 5);
                    assert!(
                        mixed_sibling_detour_test_plan(&graph, &owned, style).is_none(),
                        "owned {kind:?} ({x},{y})"
                    );
                }
            }
            for (x, y, glyph) in [
                (11, 19, style.edge_h),
                (28, 19, style.edge_h),
                (28, 36, style.edge_h),
                (16, 36, style.edge_h),
                (28, 25, style.edge_h),
                (16, 41, style.edge_h),
            ] {
                let mut aimed = canvas.clone();
                aimed.set_owned(x, y, glyph, CellOwnerKind::EdgeSegment, "foreign", 5);
                assert!(
                    mixed_sibling_detour_test_plan(&graph, &aimed, style).is_none(),
                    "foreign stroke aimed into route from ({x},{y})"
                );
            }
            for (x, y, glyph) in [(25, 19, style.edge_v), (17, 39, style.edge_h)] {
                // Those are the two legitimate wall orientations. The helper
                // must reject the opposite orientation at either claimed wall.
                let mut wrong_wall = canvas.clone();
                wrong_wall.set_owned(
                    x,
                    y,
                    glyph,
                    CellOwnerKind::SubgraphBorder,
                    if x == 25 { "A" } else { "B" },
                    3,
                );
                // Here the changed glyph is the wall's original orientation;
                // the crossing stays legal. This is a live orientation control.
                assert!(
                    mixed_sibling_detour_test_plan(&graph, &wrong_wall, style).is_some(),
                    "original wall ({x},{y})"
                );
                let opposite = if x == 25 { style.edge_h } else { style.edge_v };
                wrong_wall.set_owned(
                    x,
                    y,
                    opposite,
                    CellOwnerKind::SubgraphBorder,
                    if x == 25 { "A" } else { "B" },
                    3,
                );
                assert!(
                    mixed_sibling_detour_test_plan(&graph, &wrong_wall, style).is_none(),
                    "wrong wall orientation ({x},{y})"
                );
            }
            let mut stray_wall = canvas.clone();
            stray_wall.set_owned(
                27,
                25,
                style.edge_h,
                CellOwnerKind::SubgraphBorder,
                "Other",
                3,
            );
            assert!(
                mixed_sibling_detour_test_plan(&graph, &stray_wall, style).is_none(),
                "unclaimed wall stroke"
            );
        }
    }

    #[test]
    fn a_mixed_sibling_detour_rejects_when_every_target_port_is_blocked() {
        let mut graph = mixed_sibling_detour_test_graph();
        graph.nodes[3].width = 7;
        for style in [&crate::style::ASCII_CHARS, &crate::style::UNICODE_CHARS] {
            let canvas = mixed_sibling_detour_test_canvas(&graph, style);
            assert!(
                mixed_sibling_detour_test_plan(&graph, &canvas, style).is_some(),
                "wide receiver is a positive control before all ports are blocked"
            );
            let mut blocked = canvas.clone();
            let target = graph.get_node("T").expect("receiver");
            let ports = super::edge_entry_candidates(target, graph.direction);
            assert_eq!(ports.len(), 5);
            for (x, _) in ports {
                blocked.set_owned(x, 41, ' ', CellOwnerKind::EdgeSegment, "foreign", 5);
            }
            assert!(
                mixed_sibling_detour_test_plan(&graph, &blocked, style).is_none(),
                "all five entry candidates must reject, including the last alternative"
            );
        }
    }

    fn mixed_sibling_crossing_scene(
        style: &crate::style::StyleChars,
        horizontal_holder: bool,
    ) -> (Canvas, FallbackRoutePlan) {
        let mut canvas = Canvas::new(9, 9);
        let mut plan = FallbackRoutePlan::new("holder", super::MIXED_SIBLING_DETOUR);
        plan.set_scene_coverage(["holder".to_owned()]);
        if horizontal_holder {
            plan.push_horizontal(4, 1, 7, style.edge_h);
        } else {
            plan.push_vertical(4, 1, 7, style.edge_v);
        }
        for offset in 1..=7 {
            let (x, y, ch) = if horizontal_holder {
                (offset, 4, style.edge_h)
            } else {
                (4, offset, style.edge_v)
            };
            canvas.set_owned(x, y, ch, CellOwnerKind::EdgeSegment, "holder", 5);
        }
        (canvas, plan)
    }

    fn mixed_sibling_crossing_write(
        canvas: &mut Canvas,
        style: &crate::style::StyleChars,
        x: usize,
        y: usize,
        ch: char,
        writer: Option<&str>,
    ) {
        super::super::set_route_edge_char(
            canvas,
            x,
            y,
            ch,
            style,
            writer.map(|id| super::super::RouteOwner {
                kind: CellOwnerKind::EdgeSegment,
                id,
            }),
        );
    }

    // Bits are up, right, down, left. Inspect the visible neighbor's reciprocal
    // connection, so an explicit marker cannot make its own arms appear complete.
    fn mixed_sibling_crossing_arm_mask(canvas: &Canvas) -> u8 {
        use crate::render::topology::{
            char_connects_down, char_connects_left, char_connects_right, char_connects_up,
        };
        u8::from(char_connects_down(canvas.get(4, 3)))
            | (u8::from(char_connects_left(canvas.get(5, 4))) << 1)
            | (u8::from(char_connects_up(canvas.get(4, 5))) << 2)
            | (u8::from(char_connects_right(canvas.get(3, 4))) << 3)
    }

    fn mixed_sibling_crossing_marker_mask(ch: char) -> u8 {
        use crate::render::topology::{
            char_connects_down, char_connects_left, char_connects_right, char_connects_up,
        };
        u8::from(char_connects_up(ch))
            | (u8::from(char_connects_right(ch)) << 1)
            | (u8::from(char_connects_down(ch)) << 2)
            | (u8::from(char_connects_left(ch)) << 3)
    }

    fn mixed_sibling_crossing_lower(
        style: &crate::style::StyleChars,
        horizontal_holder: bool,
    ) -> Canvas {
        let (mut canvas, plan) = mixed_sibling_crossing_scene(style, horizontal_holder);
        canvas.record_fallback_route_plan(plan);
        canvas.allow_fallback_route_crossing("holder", "writer", 4, 4);
        for offset in 1..=7 {
            let (x, y, ch) = if horizontal_holder {
                (4, offset, style.edge_v)
            } else {
                (offset, 4, style.edge_h)
            };
            mixed_sibling_crossing_write(&mut canvas, style, x, y, ch, Some("writer"));
        }
        canvas
    }

    #[test]
    fn a_planned_mixed_sibling_crossing_keeps_both_straight_channels() {
        use crate::render::semantic::CellRole;
        for style in [&ASCII_CHARS, &crate::style::UNICODE_CHARS] {
            for horizontal_holder in [false, true] {
                let mut canvas = mixed_sibling_crossing_lower(style, horizontal_holder);
                assert_eq!(canvas.get(4, 4), style.cross_end);
                canvas.finalize_explicit_crossings(style);
                assert_eq!(canvas.get(4, 4), style.cross_end);
                assert_eq!(mixed_sibling_crossing_marker_mask(canvas.get(4, 4)), 0b1111);
                assert_eq!(mixed_sibling_crossing_arm_mask(&canvas), 0b1111);
                let meta = canvas.get_meta(4, 4).expect("crossing metadata");
                assert_eq!(meta.owner_kind, CellOwnerKind::EdgeSegment);
                assert_eq!(meta.owner_id.as_deref(), Some("writer"));
                assert_eq!(meta.role, CellRole::Junction);
                assert!(canvas.refused_route_writes().is_empty());
                let traces = canvas.fallback_route_traces();
                let trace = traces
                    .iter()
                    .find(|trace| trace.owner_id == "holder")
                    .unwrap();
                assert_eq!(trace.shared_cells.len(), 1);
                assert_eq!((trace.shared_cells[0].x, trace.shared_cells[0].y), (4, 4));
                assert!(trace.mismatches.is_empty(), "{:?}", trace.mismatches);
            }
        }
    }

    #[test]
    fn an_unapproved_mixed_sibling_crossing_preserves_the_reserved_cell() {
        let controls = [
            "wrong writer",
            "unlisted cell",
            "unlisted holder",
            "parallel stroke",
            "run endpoint",
            "corner",
            "paint",
            "wall",
            "multiple holders",
            "no owner",
            "incoming arrow",
            "existing arrow",
        ];
        for style in [&ASCII_CHARS, &crate::style::UNICODE_CHARS] {
            for control in controls {
                let (mut canvas, mut plan) = mixed_sibling_crossing_scene(style, true);
                let (mut x, y) = (4, 4);
                let mut ch = style.edge_v;
                let mut writer = Some("writer");
                let mut allowed_holder = "holder";
                let mut allowed_cell = (4, 4);
                match control {
                    "wrong writer" => writer = Some("intruder"),
                    "unlisted cell" => allowed_cell = (5, 4),
                    "unlisted holder" => allowed_holder = "unrelated",
                    "parallel stroke" => ch = style.edge_h,
                    "run endpoint" => {
                        x = 1;
                        allowed_cell = (1, 4);
                    }
                    "corner" => {
                        plan.push_corner(4, 4, style.corner_dl);
                        canvas.set_owned(
                            4,
                            4,
                            style.corner_dl,
                            CellOwnerKind::EdgeSegment,
                            "holder",
                            5,
                        );
                    }
                    "paint" => plan.push_paint(4, 4, style.edge_h),
                    "wall" => {
                        plan.claim_boundary("Frame", "top", 4, 4, style.edge_h);
                        canvas.set_owned(
                            4,
                            4,
                            style.edge_h,
                            CellOwnerKind::SubgraphBorder,
                            "Frame",
                            5,
                        );
                    }
                    "multiple holders" => {
                        let mut other = FallbackRoutePlan::new("other", "other-reservation");
                        other.push_paint(4, 4, style.edge_h);
                        canvas.record_fallback_route_plan(other);
                    }
                    "no owner" => writer = None,
                    "incoming arrow" => ch = style.arrow_down,
                    "existing arrow" => canvas.set_owned(
                        4,
                        4,
                        style.arrow_down,
                        CellOwnerKind::ArrowHead,
                        "holder",
                        5,
                    ),
                    _ => unreachable!(),
                }
                canvas.record_fallback_route_plan(plan);
                canvas.allow_fallback_route_crossing(
                    allowed_holder,
                    "writer",
                    allowed_cell.0,
                    allowed_cell.1,
                );
                let glyph_before = canvas.get(x, y);
                let meta_before = canvas.get_meta(x, y).cloned();
                mixed_sibling_crossing_write(&mut canvas, style, x, y, ch, writer);
                canvas.finalize_explicit_crossings(style);
                assert_eq!(canvas.get(x, y), glyph_before, "{control}");
                assert_eq!(canvas.get_meta(x, y).cloned(), meta_before, "{control}");
                assert!(
                    canvas.refused_route_writes().iter().any(|refusal| {
                        refusal.0 == "holder"
                            && refusal.1 == x
                            && refusal.2 == y
                            && refusal.3.as_deref() == writer
                    }),
                    "{control}: missing refusal for {writer:?}"
                );
                assert!(
                    canvas
                        .fallback_route_traces()
                        .iter()
                        .all(|trace| trace.shared_cells.is_empty()),
                    "{control}: a refused write must not create sharing evidence"
                );
            }
        }
    }

    #[test]
    fn mixed_sibling_crossing_finalization_requires_all_four_visible_arms() {
        for style in [&ASCII_CHARS, &crate::style::UNICODE_CHARS] {
            for mask in 0u8..16 {
                let mut canvas = mixed_sibling_crossing_lower(style, true);
                for (bit, (x, y)) in [(4, 3), (5, 4), (4, 5), (3, 4)].into_iter().enumerate() {
                    if mask & (1 << bit) == 0 {
                        canvas.set(x, y, ' ');
                    }
                }
                assert_eq!(mixed_sibling_crossing_arm_mask(&canvas), mask);
                canvas.finalize_explicit_crossings(style);
                let expected = if mask == 0b1111 {
                    style.cross_end
                } else {
                    style.cross
                };
                assert_eq!(canvas.get(4, 4), expected, "arm mask {mask:04b}");
            }
        }
    }

    #[test]
    fn a_mixed_sibling_crossing_next_to_an_arrow_finalizes_as_an_attachment() {
        for style in [&ASCII_CHARS, &crate::style::UNICODE_CHARS] {
            let mut canvas = mixed_sibling_crossing_lower(style, true);
            canvas.set_owned(
                5,
                4,
                style.arrow_right,
                CellOwnerKind::ArrowHead,
                "writer",
                5,
            );
            // This arrow has a reciprocal arm but still terminates a route.
            assert_eq!(mixed_sibling_crossing_arm_mask(&canvas), 0b1111);
            canvas.finalize_explicit_crossings(style);
            assert_eq!(canvas.get(4, 4), style.cross);
        }
    }
}
