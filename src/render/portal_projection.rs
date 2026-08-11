use std::collections::{HashMap, HashSet};

use crate::graph::{Direction, Graph, Node};
use crate::layout_snapshot::LayoutSnapshot;
use crate::portals::{PortalSlots, TitleGutter};
use crate::style::{vertical_portal_seam, StyleChars, BOX_HEIGHT};

use super::provenance::edge_owner_id;
use super::{canvas, semantic, topology, Canvas};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PortalAxis {
    Horizontal,
    Vertical,
}

impl PortalAxis {
    fn glyph(self, chars: &StyleChars) -> char {
        match self {
            Self::Horizontal => chars.edge_h,
            Self::Vertical => chars.edge_v,
        }
    }

    fn from_direction(direction: Direction) -> Self {
        match direction {
            Direction::LR | Direction::RL => Self::Horizontal,
            Direction::TD | Direction::TB | Direction::BT => Self::Vertical,
        }
    }
}

pub(crate) fn stamp_portal_opening(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    chars: &StyleChars,
    axis: PortalAxis,
    owner_id: &str,
    z_index: u8,
) {
    if x >= canvas.width || y >= canvas.height || is_textual(canvas.get(x, y)) {
        return;
    }
    if is_node_owned_cell(canvas, x, y) {
        return;
    }
    canvas.set_owned(
        x,
        y,
        axis.glyph(chars),
        semantic::CellOwnerKind::PortalOpening,
        owner_id,
        z_index,
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PortalSide {
    Top,
    Bottom,
    Left,
    Right,
}

fn portal_side_for_cell(graph: &Graph, x: usize, y: usize) -> Option<PortalSide> {
    graph
        .subgraphs
        .iter()
        .filter_map(|subgraph| {
            let bounds = &subgraph.bounds;
            if !bounds.is_valid() {
                return None;
            }
            let right = bounds.x + bounds.width.saturating_sub(1);
            let bottom = bounds.y + bounds.height.saturating_sub(1);
            let side = if y == bounds.y && x > bounds.x && x < right {
                Some(PortalSide::Top)
            } else if y == bottom && x > bounds.x && x < right {
                Some(PortalSide::Bottom)
            } else if x == bounds.x && y > bounds.y && y < bottom {
                Some(PortalSide::Left)
            } else if x == right && y > bounds.y && y < bottom {
                Some(PortalSide::Right)
            } else {
                None
            }?;
            Some((bounds.width.saturating_mul(bounds.height), side))
        })
        .min_by_key(|(area, _)| *area)
        .map(|(_, side)| side)
}

fn stamp_side_aware_portal_opening(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    chars: &StyleChars,
    side: PortalSide,
    owner_id: &str,
    z_index: u8,
) {
    if x >= canvas.width || y >= canvas.height || is_textual(canvas.get(x, y)) {
        return;
    }
    if is_node_owned_cell(canvas, x, y) {
        return;
    }

    // A wall crossing is not a four-way junction. Keep the semantic portal owner,
    // but project the route shaft that is perpendicular to the crossed border.
    let axis = match side {
        PortalSide::Top | PortalSide::Bottom => PortalAxis::Vertical,
        PortalSide::Left | PortalSide::Right => PortalAxis::Horizontal,
    };
    canvas.set_owned(
        x,
        y,
        axis.glyph(chars),
        semantic::CellOwnerKind::PortalOpening,
        owner_id,
        z_index,
    );
}

pub(super) fn annotate_subgraph_region(
    canvas: &mut Canvas,
    subgraph: &crate::graph::Subgraph,
    direction: Direction,
    title_gutter: TitleGutter,
) {
    let bounds = &subgraph.bounds;
    if !bounds.is_valid() {
        return;
    }

    let x0 = bounds.x;
    let x1 = bounds.x + bounds.width.saturating_sub(1);
    let y0 = bounds.y;
    let y1 = bounds.y + bounds.height.saturating_sub(1);

    for x in x0..=x1 {
        canvas.set_meta_only(
            x,
            y0,
            semantic::CellOwnerKind::SubgraphBorder,
            Some(&subgraph.id),
            semantic::CellRole::Border,
            1,
        );
        canvas.set_meta_only(
            x,
            y1,
            semantic::CellOwnerKind::SubgraphBorder,
            Some(&subgraph.id),
            semantic::CellRole::Border,
            1,
        );
    }
    for y in y0..=y1 {
        canvas.set_meta_only(
            x0,
            y,
            semantic::CellOwnerKind::SubgraphBorder,
            Some(&subgraph.id),
            semantic::CellRole::Border,
            1,
        );
        canvas.set_meta_only(
            x1,
            y,
            semantic::CellOwnerKind::SubgraphBorder,
            Some(&subgraph.id),
            semantic::CellRole::Border,
            1,
        );
    }

    if let Some(title) = subgraph.title.as_deref() {
        let title_fmt = crate::graph::subgraph_title_text_with_padding_sides(
            title,
            title_gutter.leading_extra_padding,
            title_gutter.trailing_extra_padding,
        );
        let Some((start_x, _)) = crate::graph::subgraph_title_span_with_padding_sides(
            bounds.x,
            bounds.width,
            title,
            direction,
            title_gutter.leading_extra_padding,
            title_gutter.trailing_extra_padding,
        ) else {
            return;
        };
        let title_y = subgraph_title_y(bounds, direction);
        for (i, _) in title_fmt.chars().enumerate() {
            let x = start_x + i;
            if x < canvas.width {
                canvas.set_meta_only(
                    x,
                    title_y,
                    semantic::CellOwnerKind::SubgraphTitle,
                    Some(&subgraph.id),
                    semantic::CellRole::Text,
                    2,
                );
            }
        }
    }
}

pub(super) fn annotate_node_region(canvas: &mut Canvas, node: &Node, chars: &StyleChars) {
    for y in node.y..node.y + node.height.max(BOX_HEIGHT) {
        for x in node.x..node.x + node.width {
            if x >= canvas.width || y >= canvas.height {
                continue;
            }
            if matches!(
                canvas.get_meta(x, y).map(|meta| meta.owner_kind),
                Some(
                    semantic::CellOwnerKind::SubgraphBorder
                        | semantic::CellOwnerKind::SubgraphTitle
                        | semantic::CellOwnerKind::PortalOpening
                )
            ) {
                continue;
            }
            let ch = canvas.get(x, y);
            let (owner_kind, role) = if ch == ' ' {
                (semantic::CellOwnerKind::NodeFill, semantic::CellRole::Fill)
            } else if canvas::is_horizontal(ch, chars)
                || canvas::is_vertical(ch, chars)
                || canvas::is_junction(ch, chars)
                || canvas::is_corner(ch, chars)
                || matches!(ch, '(' | ')' | '<' | '>' | '/' | '\\')
            {
                (
                    semantic::CellOwnerKind::NodeBorder,
                    semantic::CellRole::Border,
                )
            } else {
                (semantic::CellOwnerKind::NodeLabel, semantic::CellRole::Text)
            };
            canvas.set_meta_only(x, y, owner_kind, Some(&node.id), role, 3);
        }
    }
}

pub(super) fn carve_subgraph_portals_on_canvas(
    canvas: &mut Canvas,
    graph: &Graph,
    slots: &HashMap<String, PortalSlots>,
    direction: Direction,
) {
    let mut sg_ids: Vec<&str> = slots.keys().map(|id| id.as_str()).collect();
    sg_ids.sort_unstable();

    for sg_id in sg_ids {
        let Some(portals) = slots.get(sg_id) else {
            continue;
        };
        let Some(sg) = graph.get_subgraph(sg_id) else {
            continue;
        };
        let bounds = &sg.bounds;
        if !bounds.is_valid() {
            continue;
        }

        let top_y = bounds.y;
        let bottom_y = bounds.y + bounds.height.saturating_sub(1);
        let left_x = bounds.x;
        let right_x = bounds.x + bounds.width.saturating_sub(1);

        for x in sorted_slot_positions(&portals.top) {
            let px = clamp_horizontal(bounds, x);
            let top_candidates = if matches!(direction, Direction::BT) {
                vec![top_y]
            } else {
                vec![top_y, top_y.saturating_add(1)]
            };
            carve_vertical_slot(canvas, px, &top_candidates);
        }
        for x in sorted_slot_positions(&portals.bottom) {
            let px = clamp_horizontal(bounds, x);
            carve_vertical_slot(
                canvas,
                px,
                &[
                    bottom_y,
                    bottom_y.saturating_sub(1),
                    bottom_y.saturating_sub(2),
                ],
            );
        }
        for y in sorted_slot_positions(&portals.left) {
            let py = clamp_vertical(bounds, y);
            carve_horizontal_slot(canvas, py, &[left_x.saturating_add(1), left_x]);
        }
        for y in sorted_slot_positions(&portals.right) {
            let py = clamp_vertical(bounds, y);
            carve_horizontal_slot(canvas, py, &[right_x.saturating_sub(1), right_x]);
        }
    }
}

pub(super) fn reinforce_subgraph_portals(
    canvas: &mut Canvas,
    graph: &Graph,
    slots: &HashMap<String, PortalSlots>,
    direction: Direction,
    chars: &StyleChars,
    subgraph_chars: &StyleChars,
) {
    fn is_verticalish(c: char, chars: &StyleChars, subgraph_chars: &StyleChars) -> bool {
        canvas::is_vertical(c, chars)
            || canvas::is_junction(c, chars)
            || canvas::is_junction(c, subgraph_chars)
            || canvas::is_arrow(c)
    }
    fn is_horizontalish(c: char, chars: &StyleChars, subgraph_chars: &StyleChars) -> bool {
        canvas::is_horizontal(c, chars)
            || canvas::is_junction(c, chars)
            || canvas::is_junction(c, subgraph_chars)
            || canvas::is_arrow(c)
    }

    let mut sg_ids: Vec<&str> = slots.keys().map(|id| id.as_str()).collect();
    sg_ids.sort_unstable();
    let td_parallel_seam_subgraph = graph
        .td_parallel_external_attachment_ids()
        .map(|(subgraph_id, ..)| subgraph_id);

    for sg_id in sg_ids {
        let Some(portals) = slots.get(sg_id) else {
            continue;
        };
        let Some(sg) = graph.get_subgraph(sg_id) else {
            continue;
        };
        let bounds = &sg.bounds;
        if !bounds.is_valid() {
            continue;
        }
        let title_span = sg
            .title
            .as_deref()
            .and_then(|t| title_span(bounds, t, direction));

        let top_y = bounds.y;
        let bottom_y = bounds.y + bounds.height.saturating_sub(1);
        let left_x = bounds.x;
        let right_x = bounds.x + bounds.width.saturating_sub(1);
        let td_parallel_seam = (td_parallel_seam_subgraph.as_deref() == Some(sg_id)
            && portals.top.len() == 1
            && portals.bottom.len() == 1)
            .then(|| vertical_portal_seam(chars, subgraph_chars))
            .flatten();

        match direction {
            Direction::TD | Direction::TB => {
                let top_slots: Vec<usize> = portals.top.iter().copied().collect();
                let bottom_slots: Vec<usize> = portals.bottom.iter().copied().collect();

                for x in top_slots {
                    let px = clamp_horizontal(bounds, x);
                    let ty = top_y;
                    let above = if ty > 0 { canvas.get(px, ty - 1) } else { ' ' };
                    let below = if ty + 1 < canvas.height {
                        canvas.get(px, ty + 1)
                    } else {
                        ' '
                    };
                    let used = is_verticalish(above, chars, subgraph_chars)
                        || is_verticalish(below, chars, subgraph_chars);
                    let existing = canvas.get(px, ty);
                    if used
                        && ty < canvas.height
                        && !is_textual(existing)
                        && !canvas::is_arrow(existing)
                    {
                        if let Some((top_seam, _)) = td_parallel_seam {
                            canvas.set_owned(
                                px,
                                ty,
                                top_seam,
                                semantic::CellOwnerKind::PortalOpening,
                                sg_id,
                                4,
                            );
                        } else {
                            canvas.set(px, ty, chars.edge_v);
                        }
                    }
                }
                for x in bottom_slots {
                    let px = clamp_horizontal(bounds, x);
                    let above = if bottom_y > 0 {
                        canvas.get(px, bottom_y - 1)
                    } else {
                        ' '
                    };
                    let below = if bottom_y + 1 < canvas.height {
                        canvas.get(px, bottom_y + 1)
                    } else {
                        ' '
                    };
                    let used = is_verticalish(above, chars, subgraph_chars)
                        || is_verticalish(below, chars, subgraph_chars);
                    if used && bottom_y < canvas.height && !is_textual(canvas.get(px, bottom_y)) {
                        // The exact parallel TD scene gets a topology-owned seam;
                        // every other portal remains a clean one-cell hole.
                        if let Some((_, bottom_seam)) = td_parallel_seam {
                            canvas.set_owned(
                                px,
                                bottom_y,
                                bottom_seam,
                                semantic::CellOwnerKind::PortalOpening,
                                sg_id,
                                4,
                            );
                        } else {
                            canvas.set(px, bottom_y, chars.edge_v);
                        }
                    }
                }
            }
            Direction::BT => {
                // BT titles now live on the bottom interior row. Keep portal holes on the
                // physical borders, but nudge them out of corners/title-safe spans so routing
                // enters cleanly without punching through label text.
                let inner_min_x = left_x.saturating_add(1);
                let inner_max_x = right_x.saturating_sub(1).max(inner_min_x);
                let is_in_title_text = |x: usize| -> bool {
                    let Some((s, e)) = title_span else {
                        return false;
                    };
                    x >= s && x <= e
                };
                let nudge_from_corners = |mut x: usize| -> usize {
                    if inner_max_x <= inner_min_x {
                        return x;
                    }
                    if x == inner_min_x {
                        let candidate = inner_min_x.saturating_add(1);
                        if !is_in_title_text(candidate) && candidate <= inner_max_x {
                            x = candidate;
                        }
                    } else if x == inner_max_x {
                        let candidate = inner_max_x.saturating_sub(1);
                        if !is_in_title_text(candidate) && candidate >= inner_min_x {
                            x = candidate;
                        }
                    }
                    x
                };
                for x in sorted_slot_positions(&portals.top) {
                    let mut px = clamp_horizontal(bounds, x);
                    px = nudge_from_corners(px);
                    let existing = canvas.get(px, top_y);
                    if top_y < canvas.height && !is_textual(existing) && !canvas::is_arrow(existing)
                    {
                        let above = if top_y > 0 {
                            canvas.get(px, top_y - 1)
                        } else {
                            ' '
                        };
                        let below = if top_y + 1 < canvas.height {
                            canvas.get(px, top_y + 1)
                        } else {
                            ' '
                        };
                        let has_above = is_verticalish(above, chars, subgraph_chars);
                        let has_below = is_verticalish(below, chars, subgraph_chars);
                        let used = has_above || has_below;
                        if used {
                            // For BT top border, always use a clean vertical portal hole.
                            // Do NOT place junction characters on the border - they corrupt
                            // the visual appearance. Junctions belong inside the subgraph.
                            canvas.set(px, top_y, chars.edge_v);
                        } else {
                            canvas.set(px, top_y, subgraph_chars.h);
                        }
                    }
                }
                for x in sorted_slot_positions(&portals.bottom) {
                    let mut px = clamp_horizontal(bounds, x);
                    px = nudge_from_corners(px);
                    let existing = canvas.get(px, bottom_y);
                    if bottom_y < canvas.height
                        && !is_textual(existing)
                        && !canvas::is_arrow(existing)
                    {
                        let above = if bottom_y > 0 {
                            canvas.get(px, bottom_y - 1)
                        } else {
                            ' '
                        };
                        let below = if bottom_y + 1 < canvas.height {
                            canvas.get(px, bottom_y + 1)
                        } else {
                            ' '
                        };
                        let has_above = is_verticalish(above, chars, subgraph_chars);
                        let has_below = is_verticalish(below, chars, subgraph_chars);
                        let used = has_above || has_below;
                        if used {
                            // Treat BT bottom-border pierces as clean vertical holes so
                            // junctions stay off the bottom edge.
                            canvas.set(px, bottom_y, chars.edge_v);
                        } else {
                            canvas.set(px, bottom_y, subgraph_chars.h);
                        }
                    }
                }
            }
            Direction::LR | Direction::RL => {
                for y in sorted_slot_positions(&portals.left) {
                    let py = clamp_vertical(bounds, y);
                    let existing = canvas.get(left_x, py);
                    if left_x < canvas.width && !is_textual(existing) && !canvas::is_arrow(existing)
                    {
                        let left = if left_x > 0 {
                            canvas.get(left_x - 1, py)
                        } else {
                            ' '
                        };
                        let right = if left_x + 1 < canvas.width {
                            canvas.get(left_x + 1, py)
                        } else {
                            ' '
                        };
                        let has_left = is_horizontalish(left, chars, subgraph_chars);
                        let has_right = is_horizontalish(right, chars, subgraph_chars);
                        let glyph = if has_left || has_right {
                            chars.edge_h
                        } else {
                            subgraph_chars.v
                        };
                        canvas.set(left_x, py, glyph);
                    }
                }
                for y in sorted_slot_positions(&portals.right) {
                    let py = clamp_vertical(bounds, y);
                    let existing = canvas.get(right_x, py);
                    if right_x < canvas.width
                        && !is_textual(existing)
                        && !canvas::is_arrow(existing)
                    {
                        let left = if right_x > 0 {
                            canvas.get(right_x - 1, py)
                        } else {
                            ' '
                        };
                        let right = if right_x + 1 < canvas.width {
                            canvas.get(right_x + 1, py)
                        } else {
                            ' '
                        };
                        let has_left = is_horizontalish(left, chars, subgraph_chars);
                        let has_right = is_horizontalish(right, chars, subgraph_chars);
                        let glyph = if has_left || has_right {
                            chars.edge_h
                        } else {
                            subgraph_chars.v
                        };
                        canvas.set(right_x, py, glyph);
                    }
                }
            }
        }

        // Repair any carved portal holes that ended up unused (e.g. nested subgraphs where
        // edges don't actually cross the outer border). Only applies to the bottom border
        // since titles live on the top border.
        if bottom_y < canvas.height && right_x > left_x.saturating_add(2) {
            let mut fill: Option<char> = None;
            for x in (left_x + 1)..right_x {
                let ch = canvas.get(x, bottom_y);
                if ch != ' '
                    && !canvas::is_vertical(ch, chars)
                    && !canvas::is_junction(ch, chars)
                    && !canvas::is_arrow(ch)
                    && !is_textual(ch)
                {
                    fill = Some(ch);
                    break;
                }
            }
            if let Some(fill_ch) = fill {
                for x in (left_x + 1)..right_x {
                    let ch = canvas.get(x, bottom_y);
                    if ch == ' ' {
                        canvas.set(x, bottom_y, fill_ch);
                    }
                }

                // Also undo any portal reinforcement that picked a slot no edge actually uses.
                if matches!(direction, Direction::TD | Direction::TB) {
                    for x in sorted_slot_positions(&portals.bottom) {
                        let px = clamp_horizontal(bounds, x);
                        let above = if bottom_y > 0 {
                            canvas.get(px, bottom_y - 1)
                        } else {
                            ' '
                        };
                        let below = if bottom_y + 1 < canvas.height {
                            canvas.get(px, bottom_y + 1)
                        } else {
                            ' '
                        };
                        let used = is_verticalish(above, chars, subgraph_chars)
                            || is_verticalish(below, chars, subgraph_chars);
                        if !used && canvas.get(px, bottom_y) == chars.edge_v {
                            canvas.set(px, bottom_y, fill_ch);
                        }
                    }
                }
            }
        }
    }
}

/// Re-apply the topology-owned TD seam after final provenance and repair
/// passes. Those passes intentionally normalize edge-owned junctions, so this
/// explicit portal projection must be the last glyph owner before the semantic
/// frame is captured.
pub(super) fn finalize_td_parallel_portal_seams(
    canvas: &mut Canvas,
    graph: &Graph,
    slots: &HashMap<String, PortalSlots>,
    chars: &StyleChars,
    subgraph_chars: &StyleChars,
) {
    fn is_verticalish(c: char, chars: &StyleChars, subgraph_chars: &StyleChars) -> bool {
        canvas::is_vertical(c, chars)
            || canvas::is_junction(c, chars)
            || canvas::is_junction(c, subgraph_chars)
            || canvas::is_arrow(c)
    }

    let Some((subgraph_id, ..)) = graph.td_parallel_external_attachment_ids() else {
        return;
    };
    let Some(portals) = slots.get(&subgraph_id) else {
        return;
    };
    if portals.top.len() != 1 || portals.bottom.len() != 1 {
        return;
    }
    let Some((top_seam, bottom_seam)) = vertical_portal_seam(chars, subgraph_chars) else {
        return;
    };
    let Some(subgraph) = graph.get_subgraph(&subgraph_id) else {
        return;
    };
    let bounds = &subgraph.bounds;
    if !bounds.is_valid() {
        return;
    }

    let top_y = bounds.y;
    let bottom_y = bounds.y + bounds.height.saturating_sub(1);
    for (slot, y, seam) in [
        (portals.top.iter().copied().next(), top_y, top_seam),
        (portals.bottom.iter().copied().next(), bottom_y, bottom_seam),
    ] {
        let Some(slot) = slot else {
            continue;
        };
        let px = clamp_horizontal(bounds, slot);
        if y >= canvas.height {
            continue;
        }
        let above = if y > 0 { canvas.get(px, y - 1) } else { ' ' };
        let below = if y + 1 < canvas.height {
            canvas.get(px, y + 1)
        } else {
            ' '
        };
        let current = canvas.get(px, y);
        if (is_verticalish(above, chars, subgraph_chars)
            || is_verticalish(below, chars, subgraph_chars))
            && !is_textual(current)
            && !canvas::is_arrow(current)
        {
            canvas.set_owned(
                px,
                y,
                seam,
                semantic::CellOwnerKind::PortalOpening,
                &subgraph_id,
                4,
            );
        }
    }
}

fn sorted_slot_positions(slots: &HashSet<usize>) -> Vec<usize> {
    let mut ordered: Vec<usize> = slots.iter().copied().collect();
    ordered.sort_unstable();
    ordered
}

pub(super) fn finalize_horizontal_side_portals(
    canvas: &mut Canvas,
    graph: &Graph,
    layout_snapshot: &LayoutSnapshot,
    slots: &HashMap<String, PortalSlots>,
    direction: Direction,
    chars: &StyleChars,
    subgraph_chars: &StyleChars,
) {
    if !matches!(direction, Direction::LR | Direction::RL) {
        return;
    }

    let stamp_side_portal = |canvas: &mut Canvas, x: usize, y: usize| {
        if x >= canvas.width || y >= canvas.height || is_node_owned_cell(canvas, x, y) {
            return;
        }
        match portal_side_for_cell(graph, x, y) {
            Some(side @ (PortalSide::Left | PortalSide::Right)) => {
                stamp_side_aware_portal_opening(canvas, x, y, chars, side, "final_side_portal", 4);
            }
            Some(PortalSide::Top | PortalSide::Bottom) | None => {
                stamp_portal_opening(
                    canvas,
                    x,
                    y,
                    chars,
                    PortalAxis::Horizontal,
                    "final_side_portal",
                    4,
                );
            }
        }
    };

    let is_horizontalish = |c: char| {
        canvas::is_horizontal(c, chars)
            || canvas::is_junction(c, chars)
            || canvas::is_junction(c, subgraph_chars)
            || canvas::is_arrow(c)
    };

    for subgraph in &graph.subgraphs {
        let Some(portals) = slots.get(&subgraph.id) else {
            continue;
        };
        let bounds = &subgraph.bounds;
        if !bounds.is_valid() {
            continue;
        }

        let left_x = bounds.x;
        let right_x = bounds.x + bounds.width.saturating_sub(1);

        for y in sorted_slot_positions(&portals.left) {
            let py = clamp_vertical(bounds, y);
            let left = if left_x > 0 {
                canvas.get(left_x - 1, py)
            } else {
                ' '
            };
            let right = if left_x + 1 < canvas.width {
                canvas.get(left_x + 1, py)
            } else {
                ' '
            };
            // A side portal is a wall crossing, not merely a route that runs
            // alongside the wall. Require horizontal route evidence on both
            // sides of the boundary; otherwise an interior vertical collector
            // or junction can incorrectly turn an entire wall band into
            // horizontal portal markers.
            if is_horizontalish(left) && is_horizontalish(right) {
                stamp_side_portal(canvas, left_x, py);
            }
        }

        for y in sorted_slot_positions(&portals.right) {
            let py = clamp_vertical(bounds, y);
            let left = if right_x > 0 {
                canvas.get(right_x - 1, py)
            } else {
                ' '
            };
            let right = if right_x + 1 < canvas.width {
                canvas.get(right_x + 1, py)
            } else {
                ' '
            };
            if is_horizontalish(left) && is_horizontalish(right) {
                stamp_side_portal(canvas, right_x, py);
            }
        }
    }

    // The selective LR/RL pilot can intentionally route through a visually
    // containing subgraph wall that is not a semantic boundary crossing. Scan
    // the final routed geometry itself so those extra visual pierces are
    // stamped as clean portal openings too.
    for subgraph in &graph.subgraphs {
        let bounds = &subgraph.bounds;
        if !bounds.is_valid() || bounds.height < 3 {
            continue;
        }
        let left_x = bounds.x;
        let right_x = bounds.x + bounds.width.saturating_sub(1);
        let min_y = bounds.y.saturating_add(1);
        let max_y = bounds.y + bounds.height.saturating_sub(2);

        for edge_id in layout_snapshot.route_ids() {
            let covered_by_fallback = graph.edges.get(edge_id.index()).is_some_and(|edge| {
                canvas.fallback_route_covers_edge(&edge_owner_id(edge_id.index(), edge))
            });
            if covered_by_fallback {
                continue;
            }
            let Some(route) = layout_snapshot.route(edge_id) else {
                continue;
            };
            for segment in &route.segments {
                if segment.from.y != segment.to.y {
                    continue;
                }
                let y = segment.from.y;
                if y < min_y || y > max_y {
                    continue;
                }
                let (min_x, max_x) = if segment.from.x <= segment.to.x {
                    (segment.from.x, segment.to.x)
                } else {
                    (segment.to.x, segment.from.x)
                };
                if left_x >= min_x && left_x <= max_x {
                    stamp_side_portal(canvas, left_x, y);
                }
                if right_x >= min_x && right_x <= max_x {
                    stamp_side_portal(canvas, right_x, y);
                }
            }
        }
    }
}

pub(super) fn finalize_dedicated_portal_markers(
    canvas: &mut Canvas,
    graph: &Graph,
    slots: &HashMap<String, PortalSlots>,
    chars: &StyleChars,
) {
    let is_route_neighbor = |x: usize, y: usize, canvas: &Canvas| {
        canvas.get_meta(x, y).is_some_and(|meta| {
            matches!(
                meta.owner_kind,
                semantic::CellOwnerKind::EdgeSegment
                    | semantic::CellOwnerKind::CycleEdge
                    | semantic::CellOwnerKind::ArrowHead
                    | semantic::CellOwnerKind::Junction
                    | semantic::CellOwnerKind::PortalOpening
            )
        }) || canvas::is_arrow(canvas.get(x, y))
    };

    let border_cell_is_route = |x: usize, y: usize, canvas: &Canvas| {
        canvas.get_meta(x, y).is_some_and(|meta| {
            matches!(
                meta.owner_kind,
                semantic::CellOwnerKind::EdgeSegment
                    | semantic::CellOwnerKind::CycleEdge
                    | semantic::CellOwnerKind::ArrowHead
                    | semantic::CellOwnerKind::Junction
                    | semantic::CellOwnerKind::PortalOpening
            )
        }) || canvas::is_arrow(canvas.get(x, y))
    };

    let slot_has_route_neighbor = |x: usize, y: usize, canvas: &Canvas| {
        y.checked_sub(1)
            .is_some_and(|yy| is_route_neighbor(x, yy, canvas))
            || (y + 1 < canvas.height && is_route_neighbor(x, y + 1, canvas))
            || x.checked_sub(1)
                .is_some_and(|xx| is_route_neighbor(xx, y, canvas))
            || (x + 1 < canvas.width && is_route_neighbor(x + 1, y, canvas))
    };

    let horizontal_border_is_used = |x: usize, y: usize, canvas: &Canvas| {
        let up = y
            .checked_sub(1)
            .is_some_and(|yy| is_route_neighbor(x, yy, canvas));
        let down = y + 1 < canvas.height && is_route_neighbor(x, y + 1, canvas);
        let left = x
            .checked_sub(1)
            .is_some_and(|xx| is_route_neighbor(xx, y, canvas));
        let right = x + 1 < canvas.width && is_route_neighbor(x + 1, y, canvas);

        (up || down) && ((up && down) || left || right || border_cell_is_route(x, y, canvas))
    };

    fn push_unique_marker(
        markers: &mut Vec<(usize, usize, String)>,
        x: usize,
        y: usize,
        owner_id: &str,
    ) {
        if markers.iter().any(|(mx, my, _)| *mx == x && *my == y) {
            return;
        }
        markers.push((x, y, owner_id.to_string()));
    }

    let mut markers: Vec<(usize, usize, String)> = Vec::new();

    // Three or more direct sibling BT crossings repeatedly overwrite a
    // horizontal border with a bare vertical shaft. Preserve the portal
    // ownership, but compose both physical arms at the final border cell so
    // the opening reads locally as a border junction rather than a fractured
    // container wall. This is topology-derived and intentionally does not
    // alter route geometry or exactly-two legacy sibling crossings.
    let is_bt_parallel_border_junction = |x: usize, y: usize, owner_id: &str| {
        graph.subgraphs.iter().any(|subgraph| {
            if subgraph.id != owner_id {
                return false;
            }
            if !subgraph.bounds.is_valid() {
                return false;
            }
            let right_x = subgraph.bounds.x + subgraph.bounds.width.saturating_sub(1);
            let bottom_y = subgraph.bounds.y + subgraph.bounds.height.saturating_sub(1);
            if x <= subgraph.bounds.x || x >= right_x || (y != subgraph.bounds.y && y != bottom_y) {
                return false;
            }
            topology::has_bt_parallel_boundary_junction(graph, &subgraph.id)
        })
    };

    for y in 0..canvas.height {
        for x in 0..canvas.width {
            let Some(meta) = canvas.get_meta(x, y) else {
                continue;
            };
            if meta.owner_kind == semantic::CellOwnerKind::PortalOpening
                && slot_has_route_neighbor(x, y, canvas)
            {
                push_unique_marker(
                    &mut markers,
                    x,
                    y,
                    meta.owner_id.as_deref().unwrap_or("portal"),
                );
            }
        }
    }

    for subgraph in &graph.subgraphs {
        let bounds = &subgraph.bounds;
        if !bounds.is_valid() {
            continue;
        }
        let title_y = subgraph_title_y(bounds, graph.direction);
        let title_span = subgraph
            .title
            .as_deref()
            .and_then(|title| title_span(bounds, title, graph.direction));
        let is_title_protected_cell = |x: usize, y: usize| {
            y == title_y && title_span.is_some_and(|(start, end)| x >= start && x < end)
        };

        if let Some(portals) = slots.get(&subgraph.id) {
            for &x in &portals.top {
                let px = clamp_horizontal(bounds, x);
                if !is_title_protected_cell(px, bounds.y)
                    && slot_has_route_neighbor(px, bounds.y, canvas)
                {
                    push_unique_marker(&mut markers, px, bounds.y, &subgraph.id);
                }
            }
            let bottom_y = bounds.y + bounds.height.saturating_sub(1);
            for &x in &portals.bottom {
                let px = clamp_horizontal(bounds, x);
                if !is_title_protected_cell(px, bottom_y)
                    && slot_has_route_neighbor(px, bottom_y, canvas)
                {
                    push_unique_marker(&mut markers, px, bottom_y, &subgraph.id);
                }
            }
            for &y in &portals.left {
                let py = clamp_vertical(bounds, y);
                if slot_has_route_neighbor(bounds.x, py, canvas) {
                    push_unique_marker(&mut markers, bounds.x, py, &subgraph.id);
                }
            }
            let right_x = bounds.x + bounds.width.saturating_sub(1);
            for &y in &portals.right {
                let py = clamp_vertical(bounds, y);
                if slot_has_route_neighbor(right_x, py, canvas) {
                    push_unique_marker(&mut markers, right_x, py, &subgraph.id);
                }
            }

            if bounds.width >= 3 {
                let top_scan_y = bounds.y;
                let bottom_scan_y = bounds.y + bounds.height.saturating_sub(1);
                for (scan_y, preferred_slots, side) in [
                    (top_scan_y, &portals.top, "top"),
                    (bottom_scan_y, &portals.bottom, "bottom"),
                ] {
                    let mut x = bounds.x + 1;
                    let scan_end = bounds.x + bounds.width.saturating_sub(1);
                    while x < scan_end {
                        if is_title_protected_cell(x, scan_y)
                            || !horizontal_border_is_used(x, scan_y, canvas)
                        {
                            x += 1;
                            continue;
                        }

                        let run_start = x;
                        let mut run_end = x;
                        while run_end + 1 < scan_end
                            && !is_title_protected_cell(run_end + 1, scan_y)
                            && horizontal_border_is_used(run_end + 1, scan_y, canvas)
                        {
                            run_end += 1;
                        }

                        let has_marker_in_run = markers
                            .iter()
                            .any(|(mx, my, _)| *my == scan_y && *mx >= run_start && *mx <= run_end);
                        let has_fallback_claim_in_run = (run_start..=run_end).any(|x| {
                            canvas.fallback_route_claims_boundary(&subgraph.id, side, x, scan_y)
                        });
                        if !has_marker_in_run && !has_fallback_claim_in_run {
                            let midpoint = run_start + (run_end - run_start) / 2;
                            let marker_x = preferred_slots
                                .iter()
                                .copied()
                                .find(|slot_x| {
                                    let px = clamp_horizontal(bounds, *slot_x);
                                    px >= run_start && px <= run_end
                                })
                                .map(|slot_x| clamp_horizontal(bounds, slot_x))
                                .unwrap_or(midpoint);
                            push_unique_marker(&mut markers, marker_x, scan_y, &subgraph.id);
                        }

                        x = run_end + 1;
                    }
                }
            }
        }

        if matches!(graph.direction, Direction::LR | Direction::RL)
            && bounds.height >= 3
            && !canvas.fallback_route_has_scene_boundary(&subgraph.id)
        {
            let bottom_y = bounds.y + bounds.height.saturating_sub(1);
            for y in bounds.y.saturating_add(1)..bottom_y {
                for border_x in [bounds.x, bounds.x + bounds.width.saturating_sub(1)] {
                    if border_x >= canvas.width
                        || y >= canvas.height
                        || is_node_owned_cell(canvas, border_x, y)
                    {
                        continue;
                    }
                    let left_route = border_x
                        .checked_sub(1)
                        .is_some_and(|xx| is_route_neighbor(xx, y, canvas));
                    let right_route =
                        border_x + 1 < canvas.width && is_route_neighbor(border_x + 1, y, canvas);
                    if left_route || right_route {
                        push_unique_marker(&mut markers, border_x, y, &subgraph.id);
                    }
                }
            }
        }
    }

    for (x, y, owner_id) in markers {
        if is_bt_parallel_border_junction(x, y, &owner_id)
            && x < canvas.width
            && y < canvas.height
            && !is_node_owned_cell(canvas, x, y)
            && !is_textual(canvas.get(x, y))
        {
            canvas.set_owned(
                x,
                y,
                chars.edge_v,
                semantic::CellOwnerKind::PortalOpening,
                &owner_id,
                4,
            );
            continue;
        }
        match portal_side_for_cell(graph, x, y) {
            Some(side) => stamp_side_aware_portal_opening(canvas, x, y, chars, side, &owner_id, 4),
            None => stamp_portal_opening(
                canvas,
                x,
                y,
                chars,
                PortalAxis::from_direction(graph.direction),
                &owner_id,
                4,
            ),
        }
    }

    canvas.finalize_fallback_route_claims();
}

pub(super) fn is_node_owned_cell(canvas: &Canvas, x: usize, y: usize) -> bool {
    matches!(
        canvas.get_meta(x, y).map(|meta| meta.owner_kind),
        Some(
            semantic::CellOwnerKind::NodeBorder
                | semantic::CellOwnerKind::NodeFill
                | semantic::CellOwnerKind::NodeLabel
        )
    )
}

pub(super) fn should_restore_horizontal_border(
    existing: char,
    subgraph_chars: &StyleChars,
) -> bool {
    if is_textual(existing) {
        return false;
    }

    existing == ' '
        || canvas::is_horizontal(existing, subgraph_chars)
        || existing == subgraph_chars.h
}

pub(super) fn should_restore_vertical_border(existing: char, subgraph_chars: &StyleChars) -> bool {
    if is_textual(existing) {
        return false;
    }

    existing == ' ' || canvas::is_vertical(existing, subgraph_chars) || existing == subgraph_chars.v
}

pub(super) fn should_restore_corner(existing: char, target: char) -> bool {
    existing == ' ' || existing == target
}

pub(super) fn clamp_horizontal(bounds: &crate::graph::Rectangle, x: usize) -> usize {
    let min = bounds.x.saturating_add(1);
    let max = bounds.x.saturating_add(bounds.width.saturating_sub(2));
    if max < min {
        min
    } else {
        x.clamp(min, max)
    }
}

pub(super) fn clamp_vertical(bounds: &crate::graph::Rectangle, y: usize) -> usize {
    let min = bounds.y.saturating_add(1);
    let max = bounds.y.saturating_add(bounds.height.saturating_sub(2));
    if max < min {
        min
    } else {
        y.clamp(min, max)
    }
}

fn carve_vertical_slot(canvas: &mut Canvas, x: usize, candidates: &[usize]) {
    for &y in candidates {
        if x < canvas.width && y < canvas.height {
            let existing = canvas.get(x, y);
            if !is_textual(existing) {
                canvas.set(x, y, ' ');
                return;
            }
        }
    }
}

fn carve_horizontal_slot(canvas: &mut Canvas, y: usize, candidates: &[usize]) {
    for &x in candidates {
        if x < canvas.width && y < canvas.height {
            let existing = canvas.get(x, y);
            if !is_textual(existing) {
                canvas.set(x, y, ' ');
                return;
            }
        }
    }
}

pub(super) fn is_textual(c: char) -> bool {
    c.is_alphanumeric() || c == '[' || c == ']'
}

pub(super) fn subgraph_title_y(bounds: &crate::graph::Rectangle, direction: Direction) -> usize {
    crate::graph::subgraph_title_row(bounds.y, bounds.height, direction)
}

pub(crate) fn title_span(
    bounds: &crate::graph::Rectangle,
    title: &str,
    direction: Direction,
) -> Option<(usize, usize)> {
    crate::graph::subgraph_title_span(bounds.x, bounds.width, title, direction)
}

#[cfg(test)]
mod tests {
    use super::{stamp_portal_opening, stamp_side_aware_portal_opening, PortalAxis, PortalSide};
    use crate::render::semantic::CellOwnerKind;
    use crate::render::Canvas;
    use crate::style::{ASCII_CHARS, UNICODE_CHARS};

    #[test]
    fn generic_portals_use_the_explicit_route_axis() {
        let mut canvas = Canvas::new(5, 3);

        stamp_portal_opening(
            &mut canvas,
            1,
            1,
            &ASCII_CHARS,
            PortalAxis::Horizontal,
            "horizontal",
            4,
        );
        stamp_portal_opening(
            &mut canvas,
            3,
            1,
            &UNICODE_CHARS,
            PortalAxis::Vertical,
            "vertical",
            4,
        );

        assert_eq!(canvas.get(1, 1), ASCII_CHARS.edge_h);
        assert_eq!(canvas.get(3, 1), UNICODE_CHARS.edge_v);
        assert_eq!(
            canvas.get_meta(1, 1).map(|meta| meta.owner_kind),
            Some(CellOwnerKind::PortalOpening)
        );
        assert_eq!(
            canvas.get_meta(3, 1).map(|meta| meta.owner_kind),
            Some(CellOwnerKind::PortalOpening)
        );
    }

    #[test]
    fn graph_direction_maps_ambiguous_portals_to_the_route_axis() {
        assert_eq!(
            PortalAxis::from_direction(crate::graph::Direction::TD),
            PortalAxis::Vertical
        );
        assert_eq!(
            PortalAxis::from_direction(crate::graph::Direction::TB),
            PortalAxis::Vertical
        );
        assert_eq!(
            PortalAxis::from_direction(crate::graph::Direction::BT),
            PortalAxis::Vertical
        );
        assert_eq!(
            PortalAxis::from_direction(crate::graph::Direction::LR),
            PortalAxis::Horizontal
        );
        assert_eq!(
            PortalAxis::from_direction(crate::graph::Direction::RL),
            PortalAxis::Horizontal
        );
    }

    #[test]
    fn portals_use_directional_route_glyphs_and_semantic_ownership() {
        let mut canvas = Canvas::new(5, 3);

        stamp_side_aware_portal_opening(&mut canvas, 2, 0, &ASCII_CHARS, PortalSide::Top, "top", 4);
        stamp_side_aware_portal_opening(
            &mut canvas,
            2,
            2,
            &UNICODE_CHARS,
            PortalSide::Bottom,
            "bottom",
            4,
        );
        stamp_side_aware_portal_opening(
            &mut canvas,
            0,
            1,
            &ASCII_CHARS,
            PortalSide::Left,
            "left",
            4,
        );
        stamp_side_aware_portal_opening(
            &mut canvas,
            4,
            1,
            &UNICODE_CHARS,
            PortalSide::Right,
            "right",
            4,
        );

        assert_eq!(canvas.get(2, 0), ASCII_CHARS.edge_v);
        assert_eq!(canvas.get(2, 2), UNICODE_CHARS.edge_v);
        assert_eq!(canvas.get(0, 1), ASCII_CHARS.portal_pierce);
        assert_eq!(canvas.get(4, 1), UNICODE_CHARS.portal_pierce);
        assert_eq!(
            canvas.get_meta(2, 0).map(|meta| meta.owner_kind),
            Some(CellOwnerKind::PortalOpening)
        );
        assert_eq!(
            canvas.get_meta(2, 2).map(|meta| meta.owner_kind),
            Some(CellOwnerKind::PortalOpening)
        );
        assert_eq!(
            canvas.get_meta(0, 1).map(|meta| meta.owner_kind),
            Some(CellOwnerKind::PortalOpening)
        );
        assert_eq!(
            canvas.get_meta(4, 1).map(|meta| meta.owner_kind),
            Some(CellOwnerKind::PortalOpening)
        );
    }
}
