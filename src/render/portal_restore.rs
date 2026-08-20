use std::collections::{HashMap, HashSet};

use crate::graph::{Direction, Graph};
use crate::portals::{PortalSlots, TitleGutter};
use crate::style::StyleChars;

use super::portal_projection::{
    clamp_horizontal, clamp_vertical, is_node_owned_cell, is_textual, should_restore_corner,
    should_restore_horizontal_border, should_restore_vertical_border, stamp_portal_opening,
    subgraph_title_y, title_span, PortalAxis,
};
use super::{canvas, topology, Canvas};

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
                let left = if left_x > 0 {
                    canvas.get(left_x - 1, y)
                } else {
                    ' '
                };
                let right = if left_x + 1 < canvas.width {
                    canvas.get(left_x + 1, y)
                } else {
                    ' '
                };
                if canvas::is_horizontal(current, chars)
                    || canvas::is_junction(current, chars)
                    || canvas::is_arrow(current)
                    || is_horizontalish(left)
                    || is_horizontalish(right)
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
                let left = if right_x > 0 {
                    canvas.get(right_x - 1, y)
                } else {
                    ' '
                };
                let right = if right_x + 1 < canvas.width {
                    canvas.get(right_x + 1, y)
                } else {
                    ' '
                };
                if canvas::is_horizontal(current, chars)
                    || canvas::is_junction(current, chars)
                    || canvas::is_arrow(current)
                    || is_horizontalish(left)
                    || is_horizontalish(right)
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

pub(super) fn draw_subgraph_title(
    canvas: &mut Canvas,
    rect: &crate::graph::Rectangle,
    title: Option<&str>,
    direction: Direction,
    title_gutter: TitleGutter,
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
    let visible_title_span = crate::graph::subgraph_title_text_span_with_padding_sides(
        rect.x,
        rect.width,
        t,
        direction,
        title_gutter.leading_extra_padding,
        title_gutter.trailing_extra_padding,
    );
    for (i, c) in title_fmt.chars().enumerate() {
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
