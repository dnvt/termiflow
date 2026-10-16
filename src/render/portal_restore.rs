use std::collections::{HashMap, HashSet};

use crate::graph::{Direction, Graph};
use crate::portals::{PortalSlots, TitleGutter};
use crate::style::StyleChars;

use super::portal_projection::{
    clamp_horizontal, clamp_vertical, is_node_owned_cell, is_textual, should_restore_corner,
    should_restore_horizontal_border, should_restore_vertical_border, stamp_portal_opening,
    subgraph_title_y, title_span, PortalAxis,
};
use super::{canvas, provenance, semantic, topology, Canvas};

/// The edge owner id of a routed edge segment at `(x, y)` when that edge
/// crosses `subgraph_id`'s boundary.
fn routed_crossing_owner<'a>(
    canvas: &'a Canvas,
    graph: &Graph,
    subgraph_id: &str,
    x: usize,
    y: usize,
) -> Option<&'a str> {
    let meta = canvas.get_meta(x, y)?;
    if meta.owner_kind != semantic::CellOwnerKind::EdgeSegment {
        return None;
    }
    let owner_id = meta.owner_id.as_deref()?;
    // A shared fan-out trunk is owned by `fanout:<source>`, a shared fan-in
    // trunk by `fanin:<target>`; either crosses this subgraph when any edge
    // out of that source, or into that target, does.
    let fanout_source = owner_id.strip_prefix("fanout:");
    let fanin_target = owner_id.strip_prefix("fanin:");
    let crosses = graph.edges.iter().enumerate().any(|(index, edge)| {
        let owned = match (fanout_source, fanin_target) {
            (Some(source), _) => edge.from == source,
            (None, Some(target)) => edge.to == target,
            (None, None) => provenance::edge_owner_id(index, edge) == owner_id,
        };
        if !owned {
            return false;
        }
        let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
        exits.contains(&subgraph_id) || enters.contains(&subgraph_id)
    });
    crosses.then_some(owner_id)
}

fn routed_owner_is(canvas: &Canvas, x: usize, y: usize, owner_id: &str) -> bool {
    canvas
        .get_meta(x, y)
        .is_some_and(|meta| meta.owner_id.as_deref() == Some(owner_id))
}

/// A shared BT fan-out trunk may keep its border pierce only when it climbs to
/// a cell that branches down (the bus junction, not label text) and no other
/// vertical line crosses directly beside it, where the two would read as one.
fn fanout_trunk_is_clear(canvas: &Canvas, chars: &StyleChars, x: usize, border_y: usize) -> bool {
    let mut y = border_y - 1;
    while y > 0 && canvas::is_vertical(canvas.get(x, y), chars) {
        y -= 1;
    }
    let reaches_junction = !canvas::is_vertical(canvas.get(x, y), chars)
        && topology::char_connects_down(canvas.get(x, y))
        && !canvas::is_arrow(canvas.get(x, y));
    let beside_vertical =
        |nx: usize| nx < canvas.width && canvas::is_vertical(canvas.get(nx, border_y + 1), chars);
    reaches_junction && !beside_vertical(x.wrapping_sub(1)) && !beside_vertical(x + 1)
}

pub(super) fn restore_subgraph_borders(
    canvas: &mut Canvas,
    graph: &Graph,
    slots: &HashMap<String, PortalSlots>,
    direction: Direction,
    chars: &StyleChars,
    subgraph_chars: &StyleChars,
) {
    let is_horizontalish = |c: char| {
        canvas::is_horizontal(c, chars)
            || canvas::is_junction(c, chars)
            || canvas::is_junction(c, subgraph_chars)
            || canvas::is_arrow(c)
    };
    let is_verticalish = |c: char| {
        canvas::is_vertical(c, chars)
            || canvas::is_junction(c, chars)
            || canvas::is_junction(c, subgraph_chars)
            || canvas::is_arrow(c)
    };
    // A line beside a side wall at `side_x` reaching it. A tee or corner there
    // (ASCII draws both as `+`, box corners included) may belong to a bus
    // running beside the wall or turning away from it: it reaches the wall
    // only when the line also carries on past it.
    let meets_wall = |canvas: &Canvas, side_x: usize, beyond_x: Option<usize>, y: usize| {
        let cell = canvas.get(side_x, y);
        let ambiguous = canvas::is_junction(cell, chars)
            || canvas::is_corner(cell, chars)
            || chars.corner_dr == chars.corner_ur && cell == chars.corner_dr;
        is_horizontalish(cell)
            && (!ambiguous
                || beyond_x
                    .is_some_and(|bx| bx < canvas.width && is_horizontalish(canvas.get(bx, y))))
    };
    for subgraph in &graph.subgraphs {
        let bounds = &subgraph.bounds;
        if !bounds.is_valid() {
            continue;
        }

        let left_x = bounds.x;
        let right_x = bounds.x + bounds.width.saturating_sub(1);
        let top_y = bounds.y;
        let bottom_y = bounds.y + bounds.height.saturating_sub(1);

        let portal_slots = slots.get(&subgraph.id);
        let top_slots: HashSet<usize> = portal_slots
            .map(|slots| {
                slots
                    .top
                    .iter()
                    .map(|x| clamp_horizontal(bounds, *x))
                    .collect()
            })
            .unwrap_or_default();
        let bottom_slots: HashSet<usize> = portal_slots
            .map(|slots| {
                slots
                    .bottom
                    .iter()
                    .map(|x| clamp_horizontal(bounds, *x))
                    .collect()
            })
            .unwrap_or_default();
        let left_slots: HashSet<usize> = portal_slots
            .map(|slots| {
                slots
                    .left
                    .iter()
                    .map(|y| clamp_vertical(bounds, *y))
                    .collect()
            })
            .unwrap_or_default();
        let right_slots: HashSet<usize> = portal_slots
            .map(|slots| {
                slots
                    .right
                    .iter()
                    .map(|y| clamp_vertical(bounds, *y))
                    .collect()
            })
            .unwrap_or_default();

        if left_x < canvas.width
            && top_y < canvas.height
            && !is_node_owned_cell(canvas, left_x, top_y)
            && should_restore_corner(canvas.get(left_x, top_y), subgraph_chars.tl)
        {
            canvas.set(left_x, top_y, subgraph_chars.tl);
        }
        if right_x < canvas.width
            && top_y < canvas.height
            && !is_node_owned_cell(canvas, right_x, top_y)
            && should_restore_corner(canvas.get(right_x, top_y), subgraph_chars.tr)
        {
            canvas.set(right_x, top_y, subgraph_chars.tr);
        }
        if left_x < canvas.width
            && bottom_y < canvas.height
            && !is_node_owned_cell(canvas, left_x, bottom_y)
            && should_restore_corner(canvas.get(left_x, bottom_y), subgraph_chars.bl)
        {
            canvas.set(left_x, bottom_y, subgraph_chars.bl);
        }
        if right_x < canvas.width
            && bottom_y < canvas.height
            && !is_node_owned_cell(canvas, right_x, bottom_y)
            && should_restore_corner(canvas.get(right_x, bottom_y), subgraph_chars.br)
        {
            canvas.set(right_x, bottom_y, subgraph_chars.br);
        }

        for x in left_x.saturating_add(1)..right_x {
            if x >= canvas.width {
                continue;
            }
            let fallback_top_claim =
                canvas.fallback_route_claims_boundary(&subgraph.id, "top", x, top_y);
            let top_slot_is_used = top_slots.contains(&x)
                && ((top_y > 0 && is_verticalish(canvas.get(x, top_y - 1)))
                    || (top_y + 1 < canvas.height && is_verticalish(canvas.get(x, top_y + 1))));
            if fallback_top_claim || top_slot_is_used {
                continue;
            }
            if top_y < canvas.height
                && !is_node_owned_cell(canvas, x, top_y)
                && should_restore_horizontal_border(canvas.get(x, top_y), subgraph_chars)
            {
                canvas.set(x, top_y, subgraph_chars.h);
            }
            let fallback_bottom_claim =
                canvas.fallback_route_claims_boundary(&subgraph.id, "bottom", x, bottom_y);
            let bottom_slot_is_used = bottom_slots.contains(&x)
                && ((bottom_y > 0 && is_verticalish(canvas.get(x, bottom_y - 1)))
                    || (bottom_y + 1 < canvas.height
                        && is_verticalish(canvas.get(x, bottom_y + 1))));
            if fallback_bottom_claim || bottom_slot_is_used {
                continue;
            }
            // A BT entry without a reserved bottom slot is still a real
            // crossing when the routed edge that crosses this subgraph owns
            // the border cell and continues straight through it. Keep it as a
            // clean pierce; the BT title-row restore below would otherwise
            // erase it and leave the route detached at the container wall.
            if matches!(direction, Direction::BT)
                && bottom_y > 0
                && bottom_y + 1 < canvas.height
                && routed_crossing_owner(canvas, graph, &subgraph.id, x, bottom_y).is_some_and(
                    |edge_id| {
                        routed_owner_is(canvas, x, bottom_y - 1, edge_id)
                            && routed_owner_is(canvas, x, bottom_y + 1, edge_id)
                            && is_verticalish(canvas.get(x, bottom_y - 1))
                            && is_verticalish(canvas.get(x, bottom_y + 1))
                            && (!edge_id.starts_with("fanout:")
                                || fanout_trunk_is_clear(canvas, chars, x, bottom_y))
                    },
                )
            {
                canvas.set(x, bottom_y, chars.edge_v);
                continue;
            }
            let bottom_existing = canvas.get(x, bottom_y);
            let can_restore_bt_title_row = matches!(direction, Direction::BT)
                && subgraph.title.is_some()
                && !is_textual(bottom_existing)
                && !canvas::is_arrow(bottom_existing);
            if bottom_y < canvas.height
                && !is_node_owned_cell(canvas, x, bottom_y)
                && (can_restore_bt_title_row
                    || should_restore_horizontal_border(bottom_existing, subgraph_chars))
            {
                canvas.set(x, bottom_y, subgraph_chars.h);
            }
        }

        for y in top_y.saturating_add(1)..bottom_y {
            if y >= canvas.height {
                continue;
            }
            if matches!(direction, Direction::LR | Direction::RL)
                && !is_node_owned_cell(canvas, left_x, y)
            {
                let current = canvas.get(left_x, y);
                if canvas::is_horizontal(current, chars)
                    || canvas::is_junction(current, chars)
                    || canvas::is_arrow(current)
                    || left_x
                        .checked_sub(1)
                        .is_some_and(|lx| meets_wall(canvas, lx, Some(left_x + 1), y))
                    || (left_x + 1 < canvas.width
                        && meets_wall(canvas, left_x + 1, left_x.checked_sub(1), y))
                {
                    stamp_portal_opening(
                        canvas,
                        left_x,
                        y,
                        chars,
                        PortalAxis::Horizontal,
                        "side_portal_band",
                        4,
                    );
                    continue;
                }
            }
            if !left_slots.contains(&y)
                && !is_node_owned_cell(canvas, left_x, y)
                && (should_restore_vertical_border(canvas.get(left_x, y), subgraph_chars)
                    || (matches!(direction, Direction::LR | Direction::RL)
                        && (canvas::is_horizontal(canvas.get(left_x, y), chars)
                            || canvas::is_junction(canvas.get(left_x, y), chars)
                            || canvas::is_junction(canvas.get(left_x, y), subgraph_chars)
                            || canvas::is_arrow(canvas.get(left_x, y)))))
            {
                canvas.set(left_x, y, subgraph_chars.v);
            }
            if matches!(direction, Direction::LR | Direction::RL)
                && !is_node_owned_cell(canvas, right_x, y)
            {
                let current = canvas.get(right_x, y);
                if canvas::is_horizontal(current, chars)
                    || canvas::is_junction(current, chars)
                    || canvas::is_arrow(current)
                    || right_x
                        .checked_sub(1)
                        .is_some_and(|lx| meets_wall(canvas, lx, Some(right_x + 1), y))
                    || (right_x + 1 < canvas.width
                        && meets_wall(canvas, right_x + 1, right_x.checked_sub(1), y))
                {
                    stamp_portal_opening(
                        canvas,
                        right_x,
                        y,
                        chars,
                        PortalAxis::Horizontal,
                        "side_portal_band",
                        4,
                    );
                    continue;
                }
            }
            if !right_slots.contains(&y)
                && !is_node_owned_cell(canvas, right_x, y)
                && (should_restore_vertical_border(canvas.get(right_x, y), subgraph_chars)
                    || (matches!(direction, Direction::LR | Direction::RL)
                        && (canvas::is_horizontal(canvas.get(right_x, y), chars)
                            || canvas::is_junction(canvas.get(right_x, y), chars)
                            || canvas::is_junction(canvas.get(right_x, y), subgraph_chars)
                            || canvas::is_arrow(canvas.get(right_x, y)))))
            {
                canvas.set(right_x, y, subgraph_chars.v);
            }
        }
    }
}

/// Whether some edge's line runs straight through a box it does not end at,
/// entering one side and leaving the opposite one. Such a line reads as
/// touching that box, so reconnecting gaps elsewhere could complete a path
/// through it.
pub(super) fn route_runs_through_box(canvas: &Canvas, graph: &Graph) -> bool {
    let route_owner = |x: usize, y: usize| {
        canvas
            .get_meta(x, y)
            .filter(|meta| {
                matches!(
                    meta.owner_kind,
                    semantic::CellOwnerKind::EdgeSegment | semantic::CellOwnerKind::ArrowHead
                )
            })
            .and_then(|meta| meta.owner_id.clone())
    };
    let ends: HashMap<String, (&str, &str)> = graph
        .edges
        .iter()
        .enumerate()
        .map(|(index, edge)| {
            (
                provenance::edge_owner_id(index, edge),
                (edge.from.as_str(), edge.to.as_str()),
            )
        })
        .collect();
    let foreign = |owner: Option<String>, node: &str| {
        owner
            .and_then(|owner| ends.get(&owner).copied())
            .is_some_and(|(from, to)| from != node && to != node)
    };
    graph.nodes.iter().any(|node| {
        let (left, top) = (node.x, node.y);
        let (right, bottom) = (
            node.x + node.width.saturating_sub(1),
            node.y + node.height.max(crate::style::BOX_HEIGHT).saturating_sub(1),
        );
        let across_rows = top > 0
            && (left + 1..right).any(|x| {
                let above = route_owner(x, top - 1);
                above.is_some() && above == route_owner(x, bottom + 1) && foreign(above, &node.id)
            });
        let across_columns = left > 0
            && (top + 1..bottom).any(|y| {
                let before = route_owner(left - 1, y);
                before.is_some() && before == route_owner(right + 1, y) && foreign(before, &node.id)
            });
        across_rows || across_columns
    })
}

pub(super) fn draw_subgraph_title(
    canvas: &mut Canvas,
    subgraph_id: &str,
    rect: &crate::graph::Rectangle,
    title: Option<&str>,
    direction: Direction,
    title_gutter: TitleGutter,
    reconnect_lines: bool,
) {
    let Some(t) = title else {
        return;
    };
    if !rect.is_valid() {
        return;
    }
    let title_fmt = crate::graph::subgraph_title_text_with_padding_sides(
        t,
        title_gutter.leading_extra_padding,
        title_gutter.trailing_extra_padding,
    );
    let Some((start_x, _)) = crate::graph::subgraph_title_span_with_padding_sides(
        rect.x,
        rect.width,
        t,
        direction,
        title_gutter.leading_extra_padding,
        title_gutter.trailing_extra_padding,
    ) else {
        return;
    };
    let title_y = subgraph_title_y(rect, direction);
    if title_y >= canvas.height {
        return;
    }
    let holds_route = |canvas: &Canvas, x: usize| {
        canvas.get_meta(x, title_y).is_some_and(|meta| {
            meta.z_index > 0
                && matches!(
                    meta.owner_kind,
                    crate::render::semantic::CellOwnerKind::EdgeSegment
                        | crate::render::semantic::CellOwnerKind::ArrowHead
                        | crate::render::semantic::CellOwnerKind::Junction
                        | crate::render::semantic::CellOwnerKind::CycleEdge
                        | crate::render::semantic::CellOwnerKind::PortalOpening
                )
        })
    };
    // A line crossing the title row in the wrapper cell after the title
    // would touch the last letter; move the title one cell left, into the
    // free cell before it, so a blank stays between text and line.
    let cells = crate::graph::title_cells(&title_fmt);
    let trailing_x = start_x + cells.len().saturating_sub(1);
    let shift_left = title_gutter.leading_extra_padding == 0
        && title_gutter.trailing_extra_padding == 0
        && holds_route(canvas, trailing_x)
        && start_x > rect.x + 1
        && !holds_route(canvas, start_x - 1)
        && canvas.get(start_x - 1, title_y) == ' ';
    if shift_left {
        // The cell the title leaves behind is its old first wrapper space.
        let start_x = start_x - 1;
        for (i, &c) in cells.iter().enumerate() {
            let x = start_x + i;
            if x < canvas.width && !holds_route(canvas, x) {
                canvas.set(x, title_y, c);
            }
        }
        return;
    }
    // A vertical line through the title band (an edge's run above the title
    // and beyond the border next to it) was left unrouted where the title
    // sat, so a title letter would read as part of it. Slide the title,
    // nearest first, to a stretch of the row clear of lines with a blank cell
    // on each side. (The gap in the line stays: closing it can complete a path
    // that other routing left ambiguous.) When none fits, write it in place.
    let width = cells.len();
    let border_y = if title_y == rect.y + 1 {
        Some((rect.y, rect.y.checked_sub(1), title_y + 1))
    } else if title_y + 2 == rect.y + rect.height {
        Some((title_y + 1, Some(title_y + 2), title_y.wrapping_sub(1)))
    } else {
        None
    };
    let passes_through = |canvas: &Canvas, x: usize| {
        border_y.is_some_and(|(border, outer, inner)| {
            // A plain vertical run cell an edge owns: not a corner or an ASCII
            // `+`, which need not continue through, nor a box side.
            let toward = |y: usize| {
                matches!(
                    canvas.get(x, y),
                    '│' | '┃' | '║' | '╎' | '┆' | '┊' | '┋' | '╏' | '|' | ':'
                ) && canvas.get_meta(x, y).is_some_and(|meta| {
                    matches!(
                        meta.owner_kind,
                        crate::render::semantic::CellOwnerKind::EdgeSegment
                            | crate::render::semantic::CellOwnerKind::CycleEdge
                    )
                })
            };
            outer.is_some_and(|outer| outer < canvas.height && toward(outer))
                && inner < canvas.height
                && toward(inner)
                && !topology::char_connects_up(canvas.get(x, border))
        })
    };
    // One edge's plain vertical run on both sides of the title band: that
    // edge's line goes through the band, whatever the border cell shows.
    let one_route_through = |canvas: &Canvas, x: usize| {
        border_y.and_then(|(_, outer, inner)| {
            let run_owner = |y: usize| {
                canvas
                    .get_meta(x, y)
                    .filter(|meta| {
                        matches!(canvas.get(x, y), '│' | '┃' | '║' | '|')
                            && meta.owner_kind
                                == crate::render::semantic::CellOwnerKind::EdgeSegment
                    })
                    .and_then(|meta| meta.owner_id.clone())
            };
            let outer = outer.filter(|&outer| outer < canvas.height)?;
            let owner = run_owner(outer)?;
            (inner < canvas.height && run_owner(inner).as_ref() == Some(&owner))
                .then(|| (canvas.get(x, inner), owner))
        })
    };
    let crossed = |canvas: &Canvas, x: usize| {
        holds_route(canvas, x)
            || passes_through(canvas, x)
            || one_route_through(canvas, x).is_some()
    };
    let clear_at = |canvas: &Canvas, start: usize| {
        start > rect.x
            && start + width < rect.x + rect.width
            && (start.saturating_sub(1)..=start + width).all(|x| !crossed(canvas, x))
            && (start..start + width)
                .all(|x| canvas.get(x, title_y) == ' ' || x < start_x + width && x >= start_x)
    };
    if title_gutter.leading_extra_padding == 0
        && title_gutter.trailing_extra_padding == 0
        && (start_x..start_x + width).any(|x| {
            passes_through(canvas, x)
                || one_route_through(canvas, x).is_some()
                || holds_route(canvas, x)
                    && matches!(canvas.get(x, title_y), '│' | '┃' | '║' | '╎' | '|' | ':')
        })
    {
        let slot = (1..rect.width).find_map(|distance| {
            [start_x.checked_sub(distance), Some(start_x + distance)]
                .into_iter()
                .flatten()
                .find(|&start| clear_at(canvas, start))
        });
        if let Some(start) = slot {
            // Lift the title drawn earlier, leaving any route cell in place.
            for x in start_x..start_x + width {
                if !holds_route(canvas, x) {
                    canvas.set(x, title_y, ' ');
                }
            }
            for (i, &c) in cells.iter().enumerate() {
                canvas.set(start + i, title_y, c);
            }
            canvas.record_moved_title(subgraph_id, start);
            // Give a line the title sat on back its title-band cells.
            let edge = crate::render::semantic::CellOwnerKind::EdgeSegment;
            let uncovered = (start_x..start_x + width).filter(|&x| x < start || x >= start + width);
            for x in uncovered.filter(|_| reconnect_lines) {
                let (Some((run, owner)), Some((border, _, _))) =
                    (one_route_through(canvas, x), border_y)
                else {
                    continue;
                };
                if canvas.get(x, title_y) == ' ' {
                    canvas.set_owned(x, title_y, run, edge, &owner, 5);
                }
                if matches!(canvas.get(x, border), '─' | '━' | '═' | '-') {
                    canvas.set_owned(x, border, run, edge, &owner, 5);
                }
            }
            return;
        }
    }
    let visible_title_span = crate::graph::subgraph_title_text_span_with_padding_sides(
        rect.x,
        rect.width,
        t,
        direction,
        title_gutter.leading_extra_padding,
        title_gutter.trailing_extra_padding,
    );
    for (i, &c) in cells.iter().enumerate() {
        if start_x + i < canvas.width {
            let x = start_x + i;
            // Extra topology-owned gutter cells are part of the title
            // envelope but not part of the visible title. Preserve a route
            // there just as we preserve the ordinary one-cell wrappers; a
            // sibling portal may intentionally use the second padded cell
            // to keep its rail continuous through the title row.
            let is_title_gutter = visible_title_span
                .is_some_and(|(visible_start, visible_end)| x < visible_start || x > visible_end);
            let preserves_route = is_title_gutter
                && canvas.get_meta(x, title_y).is_some_and(|meta| {
                    meta.z_index > 0
                        && matches!(
                            meta.owner_kind,
                            crate::render::semantic::CellOwnerKind::EdgeSegment
                                | crate::render::semantic::CellOwnerKind::ArrowHead
                                | crate::render::semantic::CellOwnerKind::Junction
                                | crate::render::semantic::CellOwnerKind::CycleEdge
                                | crate::render::semantic::CellOwnerKind::PortalOpening
                        )
                });
            if preserves_route {
                continue;
            }
            canvas.set(x, title_y, c);
        }
    }
}

pub(super) fn cleanup_bt_title_rows(
    canvas: &mut Canvas,
    graph: &Graph,
    portal_slots: &HashMap<String, PortalSlots>,
    chars: &StyleChars,
) {
    for subgraph in &graph.subgraphs {
        let Some(title) = subgraph.title.as_deref() else {
            continue;
        };
        if !subgraph.bounds.is_valid() || subgraph.bounds.height <= 2 {
            continue;
        }

        let title_y = subgraph_title_y(&subgraph.bounds, Direction::BT);
        let bottom_y = subgraph.bounds.y + subgraph.bounds.height.saturating_sub(1);
        if title_y >= canvas.height {
            continue;
        }
        let Some((title_start, title_end)) = title_span(&subgraph.bounds, title, Direction::BT)
        else {
            continue;
        };
        // A title the final title pass slid off a line sits at its new start.
        let (title_start, title_end) = match canvas.moved_title_start(&subgraph.id) {
            Some(start) => (start, start + (title_end - title_start)),
            None => (title_start, title_end),
        };
        let inner_left = subgraph.bounds.x.saturating_add(1);
        let inner_right = subgraph.bounds.x + subgraph.bounds.width.saturating_sub(2);
        let bottom_slots: HashSet<usize> = portal_slots
            .get(&subgraph.id)
            .map(|slots| {
                slots
                    .bottom
                    .iter()
                    .map(|x| clamp_horizontal(&subgraph.bounds, *x))
                    .collect()
            })
            .unwrap_or_default();
        let has_exact_sibling_entry =
            graph
                .edges
                .iter()
                .filter(|edge| !edge.is_back_edge)
                .any(|edge| {
                    let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
                    exits.len() == 1 && enters.len() == 1 && enters[0] == subgraph.id
                });

        for x in inner_left..=inner_right {
            if x >= title_start && x <= title_end {
                continue;
            }

            if canvas.fallback_route_claims_cell(x, title_y) {
                continue;
            }

            let current = canvas.get(x, title_y);
            let has_vertical_above =
                title_y > 0 && topology::char_connects_down(canvas.get(x, title_y - 1));
            let has_vertical_below = title_y + 1 < canvas.height
                && topology::char_connects_up(canvas.get(x, title_y + 1));

            // Title redraw intentionally restores a trailing padding cell to a
            // space. If that cell is also a declared BT bottom portal, retain
            // the clean vertical pierce instead of losing continuity merely
            // because the title pass ran after routing.
            if has_exact_sibling_entry
                && (bottom_slots.contains(&x) || has_vertical_below)
                && (has_vertical_above || has_vertical_below)
            {
                canvas.set(x, title_y, chars.edge_v);
                continue;
            }
            if current == ' ' || is_textual(current) {
                continue;
            }

            if title_y == bottom_y {
                if bottom_slots.contains(&x) && (has_vertical_above || has_vertical_below) {
                    canvas.set(x, title_y, chars.edge_v);
                } else {
                    canvas.set(x, title_y, chars.edge_h);
                }
            } else if has_vertical_above && has_vertical_below {
                canvas.set(x, title_y, chars.edge_v);
            } else {
                canvas.set(x, title_y, ' ');
            }
        }
    }
}
