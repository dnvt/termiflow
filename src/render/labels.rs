//! Edge-label placement and provenance policy.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use crate::config::Config;
use crate::geom::{EdgeRoute, Segment};
use crate::graph::{Direction, Graph, Node};
use crate::style::{display_char_width, display_width, truncate_to_width, StyleChars};

use super::canvas;
use super::canvas::Canvas;
use super::portal_projection::{is_textual, subgraph_title_y};
use super::precomputed::is_subgraph_title_cell;
use super::provenance::{edge_owner_id, EdgeLabelPlacement};
use super::semantic::{CellMeta, CellOwnerKind, CellRole};

pub(super) fn pad_string(input: &str, pad: usize) -> String {
    if pad == 0 {
        return input.to_string();
    }

    let prefix = " ".repeat(pad);
    let mut out: Vec<String> = Vec::new();

    for _ in 0..pad {
        out.push(String::new());
    }
    for line in input.lines() {
        if line.is_empty() {
            out.push(String::new());
        } else {
            out.push(format!("{prefix}{line}"));
        }
    }
    for _ in 0..pad {
        out.push(String::new());
    }

    out.join("\n")
}

fn set_routed_label_shaft(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    glyph: char,
    style: &StyleChars,
) {
    let existing = canvas.get(x, y);
    if x < canvas.width
        && y < canvas.height
        && (existing == ' ' || canvas::is_horizontal(existing, style))
    {
        canvas.set(x, y, glyph);
    }
}

/// A routed label may replace a horizontal shaft cell, but never a turn or a
/// vertical/semantic cell. The previous predicate protected only arrows,
/// which let a label erase its route's corner and produce a visually broken
/// path (for example `←─label` where the next vertical turn disappeared).
fn routed_label_cell_is_writable(canvas: &Canvas, x: usize, y: usize, style: &StyleChars) -> bool {
    if x >= canvas.width || y >= canvas.height {
        return false;
    }
    let existing = canvas.get(x, y);
    if existing == ' ' {
        return true;
    }
    if canvas::is_arrow(existing) || !canvas::is_horizontal(existing, style) {
        return false;
    }
    canvas.get_meta(x, y).is_some_and(|meta| {
        matches!(
            meta.owner_kind,
            CellOwnerKind::EdgeSegment | CellOwnerKind::CycleEdge
        )
    })
}

fn routed_label_slot_is_writable(
    canvas: &Canvas,
    start_x: usize,
    y: usize,
    width: usize,
    style: &StyleChars,
) -> bool {
    let boundary_is_clear = |x: usize| {
        let existing = canvas.get(x, y);
        existing == ' ' || canvas::is_horizontal(existing, style)
    };
    width > 0
        && start_x.saturating_add(width) <= canvas.width
        && y < canvas.height
        && (start_x..start_x + width).all(|x| routed_label_cell_is_writable(canvas, x, y, style))
        && (start_x == 0 || boundary_is_clear(start_x - 1))
        && (start_x.saturating_add(width) >= canvas.width || boundary_is_clear(start_x + width))
}

/// Find a deterministic slot nearest the preferred center that does not erase
/// a turn or another semantic owner. The range is the chosen route segment;
/// labels are allowed to replace only its horizontal shaft cells.
fn nearest_routed_label_slot(
    canvas: &Canvas,
    range_start: usize,
    range_end: usize,
    preferred: usize,
    width: usize,
    y: usize,
    style: &StyleChars,
) -> Option<usize> {
    if width == 0 || range_end <= range_start || width > range_end - range_start {
        return None;
    }
    let max_start = range_end - width;
    let preferred = preferred.clamp(range_start, max_start);
    for distance in 0..=max_start.saturating_sub(range_start) {
        if let Some(candidate) = preferred.checked_sub(distance) {
            if candidate >= range_start
                && routed_label_slot_is_writable(canvas, candidate, y, width, style)
            {
                return Some(candidate);
            }
        }
        if distance > 0 {
            let candidate = preferred.saturating_add(distance);
            if candidate <= max_start
                && routed_label_slot_is_writable(canvas, candidate, y, width, style)
            {
                return Some(candidate);
            }
        }
    }
    None
}

// ============================================================================
// Edge Label Drawing
// ============================================================================

/// Draw an edge label on the appropriate segment between two nodes.
/// For TD/BT: labels go on vertical segments
/// For LR/RL: labels go on horizontal segments
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_edge_label(
    canvas: &mut Canvas,
    from: &Node,
    to: &Node,
    label: &str,
    direction: Direction,
    style: &StyleChars,
    config: &Config,
    edge_idx: usize,
    edge: &crate::graph::Edge,
    graph: &Graph,
) -> Option<EdgeLabelPlacement> {
    // Shaft cells redrawn around the label keep the edge kind (thick/dotted).
    let base_style = style;
    let kind_style = edge_label_style(style, edge);
    let style = &kind_style;
    use super::cycle::{center_x, center_y};

    let display_label =
        format_edge_label_with_limit(label, config.max_edge_label_width.min(canvas.width));
    let label_width = display_width(&display_label);
    let owner_id = edge_owner_id(edge_idx, edge);
    let mut cells = Vec::new();
    // (turn cell, label row, run side step) of a horizontal run that starts
    // at a fan-out bus turn rather than at the source box.
    let mut run_turn: Option<(usize, usize, isize)> = None;

    match direction {
        Direction::TD | Direction::TB => {
            // Vertical layout: place label on vertical segment
            let edge_x = center_x(to);
            let stem_start_y = from.bottom_y();
            let arrow_y = to.y.saturating_sub(1);
            let lower_bound = stem_start_y.saturating_add(1);
            let upper_bound = arrow_y.saturating_sub(1);
            let mut label_y = arrow_y.saturating_sub(1);

            if lower_bound <= upper_bound {
                label_y = label_y.max(lower_bound).min(upper_bound);

                let mut found = None;
                let mut probe_y = label_y;
                loop {
                    if !is_textual(canvas.get(edge_x, probe_y)) {
                        found = Some(probe_y);
                        break;
                    }

                    if probe_y == lower_bound {
                        break;
                    }
                    probe_y = probe_y.saturating_sub(1);
                }

                if let Some(y) = found {
                    label_y = y;
                }
            } else {
                label_y = label_y.min(arrow_y.saturating_sub(1));
            }

            // A line that passes a frame title in a lane turns back toward the
            // target on the row above its arrow; a label there hides the turn
            // (gen_c_097). It goes instead on the nearest row above where the
            // line runs straight down, clear of boxes and of frame borders and
            // titles, over blanks and the line itself.
            // So does one that, centred there, would cover a frame's wall or title
            // (gen_d_032).
            let mut edge_x = edge_x;
            let own = |x: usize, y: usize| {
                canvas
                    .get_meta(x, y)
                    .is_some_and(|meta| meta.owner_id.as_deref() == Some(owner_id.as_str()))
            };
            let on_frame = overlaps_reserved_subgraph_cells(
                graph,
                edge_x
                    .saturating_sub(label_width / 2)
                    .min(canvas.width.saturating_sub(label_width)),
                label_y,
                label_width,
            );
            if on_frame
                || (own(edge_x, label_y)
                    && !canvas::is_vertical(canvas.get(edge_x, label_y), base_style))
            {
                let nodes: Vec<&Node> = graph.nodes.iter().collect();
                let straight = (lower_bound..label_y).rev().find_map(|y| {
                    let x = (0..canvas.width).find(|&x| {
                        own(x, y) && canvas::is_vertical(canvas.get(x, y), base_style)
                    })?;
                    let start = x.saturating_sub(label_width / 2);
                    let clear = !is_reserved_subgraph_label_row(graph, y)
                        && !overlaps_reserved_subgraph_cells(graph, start, y, label_width)
                        && !overlaps_node(&nodes, start, y, label_width)
                        && (start..start + label_width)
                            .all(|cx| canvas.get(cx, y) == ' ' || own(cx, y));
                    // Nor right beside a frame border the line crosses into
                    // the frame's title: with text on both sides of it, the
                    // crossing would be lost.
                    let beside_border = [
                        (y.checked_sub(1), y.checked_sub(2)),
                        (Some(y + 1), Some(y + 2)),
                    ]
                    .into_iter()
                    .any(|(row, beyond)| {
                        let (Some(row), Some(beyond)) = (row, beyond) else {
                            return false;
                        };
                        graph.subgraphs.iter().any(|sg| {
                            let b = &sg.bounds;
                            b.is_valid()
                                && (row == b.y || row + 1 == b.y + b.height)
                                && x > b.x
                                && x + 1 < b.x + b.width
                        }) && super::edge::is_subgraph_title_cell(graph, x, beyond)
                    });
                    let clear = clear && !beside_border;
                    clear.then_some((x, y))
                });
                if let Some((x, y)) = straight {
                    edge_x = x;
                    label_y = y;
                }
            }

            // Center the label around the edge position
            let max_label_start = canvas.width.saturating_sub(label_width);
            let mut label_start_x = edge_x.saturating_sub(label_width / 2).min(max_label_start);
            if overlaps_node(&[from, to], label_start_x, label_y, label_width)
                && label_start_x + label_width + 1 < canvas.width
            {
                label_start_x += 1;
            }

            // Draw the label characters
            let mut x_pos = label_start_x;
            for c in display_label.chars() {
                if x_pos < canvas.width
                    && label_y < canvas.height
                    && !is_textual(canvas.get(x_pos, label_y))
                {
                    set_label_char(canvas, &mut cells, x_pos, label_y, c);
                }
                x_pos += display_char_width(c);
            }
        }
        Direction::BT => {
            // Bottom-to-top: similar to TD but arrows point up
            let edge_x = center_x(to);
            let stem_start_y = from.y.saturating_sub(1);
            let arrow_y = to.bottom_y();
            let lower_bound = arrow_y.saturating_add(1);
            let upper_bound = stem_start_y.saturating_sub(1);
            let mut label_y = lower_bound;

            if lower_bound <= upper_bound {
                let mut found = None;
                let mut probe_y = label_y;
                while probe_y <= upper_bound && probe_y < canvas.height {
                    if !is_textual(canvas.get(edge_x, probe_y)) {
                        found = Some(probe_y);
                        break;
                    }
                    probe_y += 1;
                }
                if let Some(y) = found {
                    label_y = y;
                }
            } else {
                label_y = lower_bound.min(stem_start_y);
            }

            let mut label_start_x = pick_bt_vertical_label_start(
                canvas,
                base_style,
                &[from, to],
                edge_x,
                label_y,
                label_width,
            );
            // A label over a frame's border or wall breaks the frame (gen_c_031's
            // "result" on System's top border). It goes instead on the nearest
            // row further along the line clear of frames, boxes and other lines.
            if overlaps_reserved_subgraph_cells(graph, label_start_x, label_y, label_width) {
                let nodes: Vec<&Node> = graph.nodes.iter().collect();
                let own = |x: usize, y: usize| {
                    canvas
                        .get_meta(x, y)
                        .is_some_and(|meta| meta.owner_id.as_deref() == Some(owner_id.as_str()))
                };
                let clear_row = (label_y + 1..=upper_bound.min(canvas.height.saturating_sub(1)))
                    .find_map(|y| {
                        let x = (0..canvas.width).find(|&x| {
                            own(x, y) && canvas::is_vertical(canvas.get(x, y), base_style)
                        })?;
                        let start = pick_bt_vertical_label_start(
                            canvas,
                            base_style,
                            &[from, to],
                            x,
                            y,
                            label_width,
                        );
                        let clear = !overlaps_reserved_subgraph_cells(graph, start, y, label_width)
                            && !overlaps_node(&nodes, start, y, label_width)
                            && label_neighbour_rows_clear(canvas, start, label_width, y)
                            && (start..start + label_width)
                                .all(|cx| canvas.get(cx, y) == ' ' || own(cx, y));
                        clear.then_some((y, start))
                    });
                if let Some((y, start)) = clear_row {
                    label_y = y;
                    label_start_x = start;
                }
            }
            let mut x_pos = label_start_x;
            for c in display_label.chars() {
                if x_pos < canvas.width
                    && label_y < canvas.height
                    && !is_textual(canvas.get(x_pos, label_y))
                {
                    set_label_char(canvas, &mut cells, x_pos, label_y, c);
                }
                x_pos += display_char_width(c);
            }
        }
        Direction::LR => {
            let edge_y = center_y(to);
            let arrow_x = to.x.saturating_sub(1);
            // Label only the final straight run after a fan-out bus turn.
            let turn = source_fans_out(graph, from)
                .then(|| {
                    final_run_turn(
                        canvas,
                        graph,
                        edge_y,
                        (from.x + from.width..arrow_x).rev(),
                        base_style,
                    )
                })
                .flatten();
            run_turn = turn.map(|x| (x, edge_y, 1));
            let stem_start_x = turn.map_or(from.x + from.width, |turn| turn + 1);
            let (stem_start_x, run_end_x) =
                wall_free_run(graph, edge_y, stem_start_x, arrow_x, label_width);
            let span_width = run_end_x.saturating_sub(stem_start_x);
            let outside_row = pick_outside_horizontal_label_row(
                canvas,
                edge_y,
                stem_start_x + span_width / 2,
                label_width,
                &[from, to],
                graph,
            );
            // Full placement pads the label with a blank on each side and keeps a
            // shaft cell before the first blank, as the RL placement does.
            let can_fit_full_inline = label_width + 4 <= span_width;
            // Tight placement keeps one shaft cell on each side: before the
            // arrowhead and after the source or bus turn.
            let can_fit_tight_inline = label_width + 2 <= span_width;

            if can_fit_full_inline {
                let label_start_x = stem_start_x + 1 + (span_width - (label_width + 4)) / 2;

                for x in stem_start_x..label_start_x {
                    set_label_shaft(canvas, x, edge_y, style, base_style);
                }

                canvas.set(label_start_x, edge_y, ' ');

                let mut x_pos = label_start_x + 1;
                for c in display_label.chars() {
                    if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                        set_label_char(canvas, &mut cells, x_pos, edge_y, c);
                    }
                    x_pos += display_char_width(c);
                }

                if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                    canvas.set(x_pos, edge_y, ' ');
                }
                x_pos += 1;

                for x in x_pos..run_end_x {
                    if x < canvas.width {
                        set_label_shaft(canvas, x, edge_y, style, base_style);
                    }
                }
            } else if can_fit_tight_inline {
                let label_start_x = stem_start_x + 1 + (span_width - label_width - 2) / 2;
                for x in stem_start_x..label_start_x {
                    set_label_shaft(canvas, x, edge_y, style, base_style);
                }

                let mut x_pos = label_start_x;
                for c in display_label.chars() {
                    if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                        set_label_char(canvas, &mut cells, x_pos, edge_y, c);
                    }
                    x_pos += display_char_width(c);
                }

                for x in x_pos..run_end_x {
                    if x < canvas.width {
                        set_label_shaft(canvas, x, edge_y, style, base_style);
                    }
                }
            } else if let Some(label_row) = outside_row {
                let (label_row, label_start_x) = outside_label_slot(
                    canvas,
                    edge_y,
                    label_row,
                    stem_start_x + span_width / 2,
                    label_width,
                    &[from, to],
                    graph,
                    base_style,
                );

                let mut x_pos = label_start_x;
                for c in display_label.chars() {
                    if x_pos < canvas.width && label_row < canvas.height {
                        set_label_char(canvas, &mut cells, x_pos, label_row, c);
                    }
                    x_pos += display_char_width(c);
                }
            } else {
                let inline_limit = config
                    .max_edge_label_width
                    .min(span_width.saturating_sub(3).max(1));
                let inline_label = format_edge_label_with_limit(label, inline_limit);
                let inline_width = display_width(&inline_label);
                let label_start_x =
                    stem_start_x + (span_width.saturating_sub(inline_width + 3)) / 2;

                for x in stem_start_x..label_start_x {
                    set_label_shaft(canvas, x, edge_y, style, base_style);
                }

                canvas.set(label_start_x, edge_y, ' ');

                let mut x_pos = label_start_x + 1;
                for c in inline_label.chars() {
                    if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                        set_label_char(canvas, &mut cells, x_pos, edge_y, c);
                    }
                    x_pos += display_char_width(c);
                }

                if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                    canvas.set(x_pos, edge_y, ' ');
                }
                x_pos += 1;

                for x in x_pos..run_end_x {
                    if x < canvas.width {
                        set_label_shaft(canvas, x, edge_y, style, base_style);
                    }
                }
            }
        }
        Direction::RL => {
            let edge_y = center_y(to);
            let arrow_x = to.x + to.width; // Arrow is after target box
            let gap_start_x = arrow_x.saturating_add(1);
            // Label only the final straight run after a fan-out bus turn.
            let turn = source_fans_out(graph, from)
                .then(|| final_run_turn(canvas, graph, edge_y, gap_start_x..from.x, base_style))
                .flatten();
            run_turn = turn.map(|x| (x, edge_y, -1));
            let stem_end_x = turn.unwrap_or(from.x);
            let (gap_start_x, stem_end_x) =
                wall_free_run(graph, edge_y, gap_start_x, stem_end_x, label_width);
            let span_width = stem_end_x.saturating_sub(gap_start_x);
            let outside_row = pick_outside_horizontal_label_row(
                canvas,
                edge_y,
                gap_start_x + span_width / 2,
                label_width,
                &[from, to],
                graph,
            );
            let can_fit_full_inline = label_width + 4 <= span_width;
            // Tight placement keeps one shaft cell on each side: before the
            // arrowhead and after the source or bus turn.
            let can_fit_tight_inline = label_width + 2 <= span_width;

            if can_fit_full_inline {
                let label_start_x = gap_start_x + 1 + (span_width - (label_width + 4)) / 2;

                for x in gap_start_x..label_start_x {
                    if x < canvas.width {
                        set_label_shaft(canvas, x, edge_y, style, base_style);
                    }
                }

                if label_start_x < canvas.width {
                    canvas.set(label_start_x, edge_y, ' ');
                }

                let mut x_pos = label_start_x + 1;
                for c in display_label.chars() {
                    if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                        set_label_char(canvas, &mut cells, x_pos, edge_y, c);
                    }
                    x_pos += display_char_width(c);
                }

                if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                    canvas.set(x_pos, edge_y, ' ');
                }
                x_pos += 1;

                for x in x_pos..stem_end_x {
                    if x < canvas.width {
                        set_label_shaft(canvas, x, edge_y, style, base_style);
                    }
                }
            } else if can_fit_tight_inline {
                let label_start_x = gap_start_x + 1 + (span_width - label_width - 2) / 2;
                for x in gap_start_x..label_start_x {
                    if x < canvas.width {
                        set_label_shaft(canvas, x, edge_y, style, base_style);
                    }
                }

                let mut x_pos = label_start_x;
                for c in display_label.chars() {
                    if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                        set_label_char(canvas, &mut cells, x_pos, edge_y, c);
                    }
                    x_pos += display_char_width(c);
                }

                for x in x_pos..stem_end_x {
                    if x < canvas.width {
                        set_label_shaft(canvas, x, edge_y, style, base_style);
                    }
                }
            } else if let Some(label_row) = outside_row {
                let (label_row, label_start_x) = outside_label_slot(
                    canvas,
                    edge_y,
                    label_row,
                    gap_start_x + span_width / 2,
                    label_width,
                    &[from, to],
                    graph,
                    base_style,
                );

                let mut x_pos = label_start_x;
                for c in display_label.chars() {
                    if x_pos < canvas.width && label_row < canvas.height {
                        set_label_char(canvas, &mut cells, x_pos, label_row, c);
                    }
                    x_pos += display_char_width(c);
                }
            } else {
                let inline_limit = config
                    .max_edge_label_width
                    .min(span_width.saturating_sub(4).max(1));
                let inline_label = format_edge_label_with_limit(label, inline_limit);
                let inline_width = display_width(&inline_label);
                let label_start_x =
                    gap_start_x + 1 + (span_width.saturating_sub(inline_width + 4)) / 2;

                for x in gap_start_x..label_start_x {
                    if x < canvas.width {
                        set_label_shaft(canvas, x, edge_y, style, base_style);
                    }
                }

                if label_start_x < canvas.width {
                    canvas.set(label_start_x, edge_y, ' ');
                }

                let mut x_pos = label_start_x + 1;
                for c in inline_label.chars() {
                    if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                        set_label_char(canvas, &mut cells, x_pos, edge_y, c);
                    }
                    x_pos += display_char_width(c);
                }

                if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                    canvas.set(x_pos, edge_y, ' ');
                }
                x_pos += 1;

                for x in x_pos..stem_end_x {
                    if x < canvas.width {
                        set_label_shaft(canvas, x, edge_y, style, base_style);
                    }
                }
            }
        }
    }

    // A label padding blank must not sit against the bus turn: the turn would
    // lose its arm toward the run and be normalized into a straight shaft.
    if let Some((turn_x, turn_y, side)) = run_turn {
        let next_x = turn_x as isize + side;
        if next_x >= 0 && canvas.get(next_x as usize, turn_y) == ' ' {
            canvas.set(next_x as usize, turn_y, style.edge_h);
        }
    }

    build_label_placement(owner_id, cells)
}

/// Draw an edge label using a precomputed Manhattan route. Picks the longest
/// segment (preferring horizontal) and centers the label along it.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_routed_edge_label(
    canvas: &mut Canvas,
    route: &EdgeRoute,
    label: &str,
    style: &StyleChars,
    graph: &Graph,
    config: &Config,
    edge_idx: usize,
    edge: &crate::graph::Edge,
) -> Option<EdgeLabelPlacement> {
    let kind_style = edge_label_style(style, edge);
    let style = &kind_style;
    if route.segments.is_empty() {
        return None;
    }

    let display_label =
        format_edge_label_with_limit(label, config.max_edge_label_width.min(canvas.width));
    let label_width = display_width(&display_label);
    let owner_id = edge_owner_id(edge_idx, edge);
    let mut cells = Vec::new();

    let nodes: Vec<&Node> = graph.nodes.iter().collect();
    let from_node = graph
        .nodes
        .iter()
        .find(|node| node.id.as_str() == edge.from.as_str());
    let to_node = graph
        .nodes
        .iter()
        .find(|node| node.id.as_str() == edge.to.as_str());
    let border_spans: Vec<crate::graph::Rectangle> = graph
        .subgraphs
        .iter()
        .map(|sg| sg.bounds.clone())
        .filter(|b| b.is_valid())
        .collect();

    // Choose longest segment, prefer horizontal for readability, avoid subgraph borders when possible.
    let mut best: Option<(&Segment, usize, bool)> = None; // (segment, length, is_horizontal)
    for seg in &route.segments {
        let is_horizontal = seg.from.y == seg.to.y;
        let length = if is_horizontal {
            seg.from.x.abs_diff(seg.to.x)
        } else {
            seg.from.y.abs_diff(seg.to.y)
        };
        let on_border = border_spans.iter().any(|b| segment_on_border(seg, b));

        match best {
            None => best = Some((seg, length, is_horizontal)),
            Some((prev_seg, best_len, best_horizontal)) => {
                let prev_on_border = border_spans.iter().any(|b| segment_on_border(prev_seg, b));
                let prefer_current = match (prev_on_border, on_border) {
                    (true, false) => true,
                    (false, true) => false,
                    _ => {
                        (is_horizontal && !best_horizontal)
                            || (is_horizontal == best_horizontal && length > best_len)
                    }
                };
                if prefer_current {
                    best = Some((seg, length, is_horizontal));
                }
            }
        }
    }

    let (seg, _, is_horizontal) = best?;

    if is_horizontal {
        let mut y = seg.from.y;
        if border_spans
            .iter()
            .any(|b| y == b.y || y == b.y + b.height.saturating_sub(1))
        {
            if y + 1 < canvas.height {
                y += 1;
            } else if y > 0 {
                y = y.saturating_sub(1);
            }
        }
        let (min_x, max_x) = if seg.from.x <= seg.to.x {
            (seg.from.x, seg.to.x)
        } else {
            (seg.to.x, seg.from.x)
        };
        let (gap_start_x, gap_end_x) =
            wall_free_run(graph, y, min_x, max_x.saturating_add(1), label_width);
        let gap_width = gap_end_x.saturating_sub(gap_start_x);
        let mid_x = gap_start_x + gap_width / 2;
        let centered_start_x = mid_x.saturating_sub(label_width / 2);
        let outside_row =
            pick_outside_horizontal_label_row(canvas, y, mid_x, label_width, &nodes, graph);
        let reserve_leading_shaft = graph.direction == Direction::RL;
        let inline_margin = if reserve_leading_shaft { 4 } else { 3 };
        let inline_collides = overlaps_node(&nodes, centered_start_x, y, label_width)
            || overlaps_reserved_subgraph_cells(graph, centered_start_x, y, label_width);
        let full_inline_start = if !inline_collides && label_width + inline_margin <= gap_width {
            let preferred = gap_start_x
                + usize::from(reserve_leading_shaft)
                + (gap_width - (label_width + inline_margin)) / 2;
            nearest_routed_label_slot(
                canvas,
                gap_start_x,
                gap_end_x,
                preferred,
                label_width,
                y,
                style,
            )
        } else {
            None
        };
        let tight_inline_start = if !inline_collides && label_width <= gap_width {
            let preferred = gap_start_x + (gap_width.saturating_sub(label_width)) / 2;
            nearest_routed_label_slot(
                canvas,
                gap_start_x,
                gap_end_x,
                preferred,
                label_width,
                y,
                style,
            )
        } else {
            None
        };
        let (node_gap_start_x, node_gap_end_x) = match (graph.direction, from_node, to_node) {
            (Direction::LR, Some(from), Some(to)) => (from.x + from.width, to.x.saturating_sub(1)),
            (Direction::RL, Some(from), Some(to)) => (to.x + to.width, from.x),
            _ => (gap_start_x, gap_end_x),
        };
        let node_gap_width = node_gap_end_x.saturating_sub(node_gap_start_x);
        let node_gap_center = node_gap_start_x + node_gap_width / 2;
        let node_gap_start = node_gap_center.saturating_sub(label_width / 2);
        let node_gap_label_start = if matches!(graph.direction, Direction::LR | Direction::RL)
            && label_width <= node_gap_width
            && !overlaps_node(&nodes, node_gap_start, y, label_width)
            && !overlaps_reserved_subgraph_cells(graph, node_gap_start, y, label_width)
        {
            Some(
                nearest_routed_label_slot(
                    canvas,
                    node_gap_start_x,
                    node_gap_end_x,
                    node_gap_start,
                    label_width,
                    y,
                    style,
                )
                .or_else(|| {
                    if node_gap_start != node_gap_center {
                        nearest_routed_label_slot(
                            canvas,
                            node_gap_start_x,
                            node_gap_end_x,
                            node_gap_center,
                            label_width,
                            y,
                            style,
                        )
                    } else {
                        None
                    }
                }),
            )
            .flatten()
        } else {
            None
        };

        // Beyond a frame wall the segment keeps the edge's own glyph too.
        for x in (min_x..gap_start_x).chain(gap_end_x..max_x.saturating_add(1)) {
            set_routed_label_shaft(canvas, x, y, style.edge_h, style);
        }

        if let Some(start_x) = full_inline_start {
            for x in gap_start_x..start_x {
                set_routed_label_shaft(canvas, x, y, style.edge_h, style);
            }

            if routed_label_cell_is_writable(canvas, start_x, y, style) {
                canvas.set(start_x, y, ' ');
            }

            let mut x_pos = start_x + 1;
            for c in display_label.chars() {
                if routed_label_cell_is_writable(canvas, x_pos, y, style) {
                    set_label_char(canvas, &mut cells, x_pos, y, c);
                }
                x_pos += display_char_width(c);
            }

            if routed_label_cell_is_writable(canvas, x_pos, y, style) {
                canvas.set(x_pos, y, ' ');
            }
            x_pos += 1;

            for x in x_pos..gap_end_x {
                set_routed_label_shaft(canvas, x, y, style.edge_h, style);
            }
        } else if let Some(start_x) = tight_inline_start {
            for x in gap_start_x..start_x {
                set_routed_label_shaft(canvas, x, y, style.edge_h, style);
            }

            let mut x_pos = start_x;
            for c in display_label.chars() {
                if routed_label_cell_is_writable(canvas, x_pos, y, style) {
                    set_label_char(canvas, &mut cells, x_pos, y, c);
                }
                x_pos += display_char_width(c);
            }

            for x in x_pos..gap_end_x {
                set_routed_label_shaft(canvas, x, y, style.edge_h, style);
            }
        } else if let Some(start_x) = node_gap_label_start {
            for x in node_gap_start_x..start_x {
                set_routed_label_shaft(canvas, x, y, style.edge_h, style);
            }

            let mut x_pos = start_x;
            for c in display_label.chars() {
                if routed_label_cell_is_writable(canvas, x_pos, y, style) {
                    set_label_char(canvas, &mut cells, x_pos, y, c);
                }
                x_pos += display_char_width(c);
            }

            for x in x_pos..node_gap_end_x {
                set_routed_label_shaft(canvas, x, y, style.edge_h, style);
            }
        } else if let Some(label_row) = outside_row {
            let max_label_start = canvas.width.saturating_sub(label_width);
            let mut start_x = centered_start_x.min(max_label_start);
            start_x = adjust_horizontal_label_slot(
                start_x,
                0,
                canvas.width,
                label_row,
                label_width,
                &nodes,
                graph,
            );

            let mut x_pos = start_x;
            for c in display_label.chars() {
                if label_row < canvas.height && x_pos < canvas.width {
                    set_label_char(canvas, &mut cells, x_pos, label_row, c);
                }
                x_pos += display_char_width(c);
            }
        } else {
            let inline_limit = config
                .max_edge_label_width
                .min(gap_width.saturating_sub(inline_margin).max(1));
            let inline_label = format_edge_label_with_limit(label, inline_limit);
            let inline_width = display_width(&inline_label);
            let preferred_start_x = gap_start_x
                + usize::from(reserve_leading_shaft)
                + (gap_width.saturating_sub(inline_width + inline_margin)) / 2;
            let start_x = nearest_routed_label_slot(
                canvas,
                gap_start_x,
                gap_end_x,
                preferred_start_x,
                inline_width,
                y,
                style,
            )?;

            for x in gap_start_x..start_x {
                set_routed_label_shaft(canvas, x, y, style.edge_h, style);
            }

            if routed_label_cell_is_writable(canvas, start_x, y, style) {
                canvas.set(start_x, y, ' ');
            }

            let mut x_pos = start_x + 1;
            for c in inline_label.chars() {
                if routed_label_cell_is_writable(canvas, x_pos, y, style) {
                    set_label_char(canvas, &mut cells, x_pos, y, c);
                }
                x_pos += display_char_width(c);
            }

            if routed_label_cell_is_writable(canvas, x_pos, y, style) {
                canvas.set(x_pos, y, ' ');
            }
            x_pos += 1;

            for x in x_pos..gap_end_x {
                set_routed_label_shaft(canvas, x, y, style.edge_h, style);
            }
        }
    } else {
        let x = seg.from.x;
        let (min_y, max_y) = if seg.from.y <= seg.to.y {
            (seg.from.y, seg.to.y)
        } else {
            (seg.to.y, seg.from.y)
        };
        let mut mid_y = if canvas::is_arrow(canvas.get(x, min_y)) && min_y < max_y {
            min_y + 1
        } else if canvas::is_arrow(canvas.get(x, max_y)) && max_y > min_y {
            max_y.saturating_sub(1)
        } else {
            min_y + (max_y.saturating_sub(min_y)) / 2
        };
        if border_spans
            .iter()
            .any(|b| mid_y == b.y || mid_y == b.y + b.height.saturating_sub(1))
        {
            if canvas::is_arrow(canvas.get(x, min_y)) && mid_y < max_y && mid_y + 1 < canvas.height
            {
                mid_y += 1;
            } else if mid_y > min_y {
                mid_y = mid_y.saturating_sub(1);
            }
        }
        let mut start_x = x.saturating_sub(label_width / 2);
        if start_x + label_width > canvas.width {
            start_x = canvas.width.saturating_sub(label_width);
        }

        // Avoid drawing over node interiors if possible.
        let mut x_pos = start_x;
        for c in display_label.chars() {
            if mid_y < canvas.height && x_pos < canvas.width {
                set_label_char(canvas, &mut cells, x_pos, mid_y, c);
            }
            x_pos += display_char_width(c);
        }
    }

    build_label_placement(owner_id, cells)
}

/// Truncate and format edge label to the specified maximum width.
pub(super) fn format_edge_label_with_limit(label: &str, max_len: usize) -> String {
    if display_width(label) <= max_len {
        return label.to_string();
    }
    let ellipsis = "…";
    let ellipsis_width = display_width(ellipsis);
    if max_len <= ellipsis_width {
        return truncate_to_width(ellipsis, max_len);
    }

    let prefix = truncate_to_width(label, max_len.saturating_sub(ellipsis_width));
    format!("{prefix}{ellipsis}")
}

fn segment_on_border(seg: &Segment, bounds: &crate::graph::Rectangle) -> bool {
    if !bounds.is_valid() {
        return false;
    }
    // Horizontal along top/bottom
    if seg.from.y == seg.to.y {
        let y = seg.from.y;
        if y == bounds.y || y == bounds.y + bounds.height.saturating_sub(1) {
            let (min_x, max_x) = if seg.from.x <= seg.to.x {
                (seg.from.x, seg.to.x)
            } else {
                (seg.to.x, seg.from.x)
            };
            let span_left = bounds.x;
            let span_right = bounds.x + bounds.width.saturating_sub(1);
            return max_x >= span_left && min_x <= span_right;
        }
    } else if seg.from.x == seg.to.x {
        let x = seg.from.x;
        if x == bounds.x || x == bounds.x + bounds.width.saturating_sub(1) {
            let (min_y, max_y) = if seg.from.y <= seg.to.y {
                (seg.from.y, seg.to.y)
            } else {
                (seg.to.y, seg.from.y)
            };
            let span_top = bounds.y;
            let span_bottom = bounds.y + bounds.height.saturating_sub(1);
            return max_y >= span_top && min_y <= span_bottom;
        }
    }
    false
}

fn overlaps_node(nodes: &[&Node], x: usize, y: usize, width: usize) -> bool {
    for n in nodes {
        if y >= n.y && y < n.bottom_y() {
            let nx0 = n.x;
            let nx1 = n.x + n.width;
            if x < nx1 && x + width > nx0 {
                return true;
            }
        }
    }
    false
}

fn pick_bt_vertical_label_start(
    canvas: &Canvas,
    style: &StyleChars,
    nodes: &[&Node],
    edge_x: usize,
    y: usize,
    width: usize,
) -> usize {
    if width == 0 {
        return edge_x;
    }

    let max_start = canvas.width.saturating_sub(width);
    let centered = edge_x.saturating_sub(width / 2).min(max_start);
    let centered_covers_edge = centered <= edge_x && edge_x < centered.saturating_add(width);

    if (!centered_covers_edge || canvas.get(edge_x, y) == ' ')
        && !overlaps_node(nodes, centered, y, width)
    {
        return centered;
    }

    let candidates = [
        edge_x
            .saturating_sub(width.saturating_add(1))
            .min(max_start),
        edge_x.saturating_add(2).min(max_start),
        centered,
    ];
    let mut best = centered;
    let mut best_score = usize::MAX;

    for start in candidates {
        if start + width > canvas.width {
            continue;
        }

        let covers_edge = start <= edge_x && edge_x < start.saturating_add(width);
        let overlaps = overlaps_node(nodes, start, y, width);
        let occupied = (start..start + width)
            .filter(|x| canvas.get(*x, y) != ' ')
            .count();
        // A label over another line's junction or corner cuts its branches
        // off (gen_a_038: "metrics" over the trunk joining the bus).
        let breaks_line = (start..start + width).any(|x| {
            let cell = canvas.get(x, y);
            canvas::is_junction(cell, style) || canvas::is_corner(cell, style)
        });
        let distance = start.abs_diff(centered);
        let score = usize::from(covers_edge) * 1000
            + usize::from(breaks_line) * 1000
            + usize::from(overlaps) * 100
            + occupied * 10
            + distance;

        if score < best_score {
            best_score = score;
            best = start;
        }
    }

    best
}

fn adjust_horizontal_label_slot(
    start_x: usize,
    min_x: usize,
    max_x: usize,
    y: usize,
    width: usize,
    nodes: &[&Node],
    graph: &Graph,
) -> usize {
    let candidate = start_x;
    if !overlaps_node(nodes, candidate, y, width)
        && !overlaps_reserved_subgraph_cells(graph, candidate, y, width)
    {
        return candidate;
    }

    // Try small shifts within segment bounds.
    for delta in 1..=4 {
        if candidate >= delta
            && !overlaps_node(nodes, candidate - delta, y, width)
            && !overlaps_reserved_subgraph_cells(graph, candidate - delta, y, width)
            && candidate - delta >= min_x
        {
            return candidate - delta;
        }
        if candidate + width + delta <= max_x
            && !overlaps_node(nodes, candidate + delta, y, width)
            && !overlaps_reserved_subgraph_cells(graph, candidate + delta, y, width)
        {
            return candidate + delta;
        }
    }
    candidate
}

fn pick_outside_horizontal_label_row(
    canvas: &Canvas,
    edge_y: usize,
    label_center_x: usize,
    label_width: usize,
    nodes: &[&Node],
    graph: &Graph,
) -> Option<usize> {
    if label_width == 0 || label_width > canvas.width {
        return None;
    }

    let max_start = canvas.width.saturating_sub(label_width);
    let label_start_x = label_center_x
        .saturating_sub(label_width / 2)
        .min(max_start);
    let mut candidates = Vec::with_capacity(canvas.height.saturating_sub(1));
    for delta in 1..=canvas.height {
        if let Some(row) = edge_y.checked_sub(delta) {
            candidates.push(row);
        }
        let row = edge_y.saturating_add(delta);
        if row < canvas.height {
            candidates.push(row);
        }
    }

    candidates.into_iter().find(|&row| {
        outside_label_row_is_free(canvas, row, label_start_x, label_width, nodes, graph)
    })
}

/// Whether a label at `label_start_x` on `row` would sit on blank cells, off
/// every box and off a frame's title or bottom row.
fn outside_label_row_is_free(
    canvas: &Canvas,
    row: usize,
    label_start_x: usize,
    label_width: usize,
    nodes: &[&Node],
    graph: &Graph,
) -> bool {
    let intersects_node = nodes.iter().any(|node| {
        row >= node.y
            && row < node.bottom_y()
            && label_start_x < node.x + node.width
            && label_start_x + label_width > node.x
    });
    let occupied = (label_start_x..label_start_x + label_width).any(|x| canvas.get(x, row) != ' ');
    !intersects_node && !occupied && !is_reserved_subgraph_label_row(graph, row)
}

/// The row and start of a label beside the horizontal run on `edge_y`: on
/// `label_row`, nudged off any route beside it, or, when no slot there is,
/// on the row as far on the other side of the run if one there is.
#[allow(clippy::too_many_arguments)]
fn outside_label_slot(
    canvas: &Canvas,
    edge_y: usize,
    label_row: usize,
    label_x: usize,
    label_width: usize,
    nodes: &[&Node],
    graph: &Graph,
    style: &StyleChars,
) -> (usize, usize) {
    let max_label_start = canvas.width.saturating_sub(label_width);
    let centred = label_x.saturating_sub(label_width / 2).min(max_label_start);
    let slot = |row: usize| {
        let start =
            adjust_horizontal_label_slot(centred, 0, canvas.width, row, label_width, nodes, graph);
        keep_label_off_routes(canvas, start, row, label_width, style, nodes)
    };
    let start = match slot(label_row) {
        Ok(start) => return (label_row, start),
        Err(start) => start,
    };
    let mirror = if label_row < edge_y {
        Some(edge_y + (edge_y - label_row))
    } else {
        edge_y.checked_sub(label_row - edge_y)
    };
    mirror
        .filter(|&row| {
            row < canvas.height
                && outside_label_row_is_free(canvas, row, centred, label_width, nodes, graph)
        })
        .and_then(|row| slot(row).ok().map(|start| (row, start)))
        .unwrap_or((label_row, start))
}

fn is_reserved_subgraph_label_row(graph: &Graph, y: usize) -> bool {
    graph.subgraphs.iter().any(|sg| {
        if !sg.bounds.is_valid() {
            return false;
        }

        let bottom_y = sg.bounds.y + sg.bounds.height.saturating_sub(1);
        if y == sg.bounds.y || y == bottom_y {
            return true;
        }

        sg.title.is_some() && y == subgraph_title_y(&sg.bounds, graph.direction)
    })
}

fn overlaps_reserved_subgraph_cells(graph: &Graph, start_x: usize, y: usize, width: usize) -> bool {
    let end_x = start_x.saturating_add(width);

    graph.subgraphs.iter().any(|sg| {
        if !sg.bounds.is_valid() {
            return false;
        }

        let left = sg.bounds.x;
        let right = sg.bounds.x + sg.bounds.width.saturating_sub(1);
        let top = sg.bounds.y;
        let bottom = sg.bounds.y + sg.bounds.height.saturating_sub(1);

        (start_x..end_x).any(|x| {
            let on_horizontal_border = (y == top || y == bottom) && x >= left && x <= right;
            let on_vertical_border = (x == left || x == right) && y >= top && y <= bottom;
            let on_vertical_border_gutter = y >= top
                && y <= bottom
                && (x == left.saturating_sub(1) || x == right.saturating_add(1));
            on_horizontal_border
                || on_vertical_border
                || on_vertical_border_gutter
                || is_subgraph_title_cell(graph, x, y)
        })
    })
}

/// Whether the rows just above and below the `width` cells from `start_x` on
/// row `y` hold no text: stacked labels read as one, and the line checks read
/// through a label to the row past it.
fn label_neighbour_rows_clear(canvas: &Canvas, start_x: usize, width: usize, y: usize) -> bool {
    [y.checked_sub(1), Some(y + 1)]
        .into_iter()
        .flatten()
        .all(|row| {
            (start_x.saturating_sub(1)..start_x + width + 1)
                .all(|x| !is_textual(canvas.get(x, row)))
        })
}

/// Whether a line other than `owner_id`'s already holds one of the `width`
/// cells from `start_x` on row `y`.
fn cells_held_by_another_line(
    canvas: &Canvas,
    start_x: usize,
    width: usize,
    y: usize,
    owner_id: &str,
) -> bool {
    (start_x..start_x + width).any(|x| {
        canvas.get(x, y) != ' '
            && canvas
                .get_meta(x, y)
                .is_none_or(|meta| meta.owner_id.as_deref() != Some(owner_id))
    })
}

/// The row where a convergent edge's stem turns toward its target. Walking
/// the source's stem from `start_y` (upward when `up`), the first row whose
/// sideways run toward the target meets a line ending in an arrowhead at the
/// target box. Tees where branches to other targets leave the stem are
/// passed. `None` when the stem has no such run.
#[cfg(test)]
fn convergent_turn_row(
    canvas: &Canvas,
    src_x: usize,
    start_y: usize,
    up: bool,
    to: &Node,
) -> Option<usize> {
    use super::topology::{
        char_connects_down, char_connects_left, char_connects_right, char_connects_up,
    };
    let target_x = super::cycle::center_x(to);
    if target_x == src_x {
        return None;
    }
    let left = target_x < src_x;
    let next_y = |y: usize| {
        if up {
            y.checked_sub(1)
        } else {
            Some(y + 1).filter(|&y| y < canvas.height)
        }
    };
    let next_x = |x: usize| {
        if left {
            x.checked_sub(1)
        } else {
            Some(x + 1).filter(|&x| x < canvas.width)
        }
    };
    // Both cells agree they join: a corner or tee with blank beyond it does not.
    let joins_v = |x: usize, y: usize| {
        next_y(y).is_some_and(|n| {
            let (here, there) = (canvas.get(x, y), canvas.get(x, n));
            if up {
                char_connects_up(here) && char_connects_down(there)
            } else {
                char_connects_down(here) && char_connects_up(there)
            }
        })
    };
    let joins_h = |x: usize, y: usize| {
        next_x(x).is_some_and(|n| {
            let (here, there) = (canvas.get(x, y), canvas.get(n, y));
            if left {
                char_connects_left(here) && char_connects_right(there)
            } else {
                char_connects_right(here) && char_connects_left(there)
            }
        })
    };
    let reaches_target = |x: usize, mut y: usize| {
        while joins_v(x, y) {
            y = next_y(y).expect("joined");
        }
        canvas::is_arrow(canvas.get(x, y))
            && next_y(y).is_some_and(|n| {
                (to.x..to.x + to.width).contains(&x) && (to.y..to.bottom_y()).contains(&n)
            })
    };
    let mut y = start_y;
    loop {
        let mut x = src_x;
        while joins_h(x, y) {
            x = next_x(x).expect("joined");
            if joins_v(x, y) && reaches_target(x, y) {
                return Some(y);
            }
        }
        if !joins_v(src_x, y) {
            return None;
        }
        y = next_y(y)?;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConvergentCellWitness {
    x: usize,
    y: usize,
    raw: char,
    meta: CellMeta,
    stage: String,
}

impl ConvergentCellWitness {
    fn capture(canvas: &Canvas, x: usize, y: usize) -> Option<Self> {
        Some(Self {
            x,
            y,
            raw: canvas.get(x, y),
            meta: canvas.get_meta(x, y)?.clone(),
            stage: canvas.write_stage_at(x, y)?.to_owned(),
        })
    }

    fn is_current(&self, canvas: &Canvas) -> bool {
        Self::capture(canvas, self.x, self.y).as_ref() == Some(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConvergentPathProof {
    turn_y: usize,
    source_x: usize,
    stem_start: usize,
    target_x: usize,
    target_end: usize,
    up: bool,
    reads: Vec<ConvergentCellWitness>,
    dependencies: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VerifiedConvergentLabel {
    sequence: usize,
    epoch: usize,
    eligible: bool,
    owner_id: String,
    from_id: String,
    target_id: String,
    direction: Direction,
    label: String,
    start: usize,
    end: usize,
    y: usize,
    post: Vec<ConvergentCellWitness>,
    underlay: Vec<ConvergentCellWitness>,
    proof: ConvergentPathProof,
}

/// Render-local observations of completed convergent transactions. Reservations
/// survive loss of proof eligibility; legacy coordinates never become proofs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ConvergentLabelContext {
    graph_key: Option<String>,
    format_key: Option<String>,
    limit: usize,
    epoch: usize,
    receipts: Vec<VerifiedConvergentLabel>,
    reservations: Vec<Vec<(usize, usize)>>,
}

fn convergent_graph_key(graph: &Graph) -> String {
    format!(
        "{:?}|{:?}|{:?}|{:?}",
        graph.direction, graph.nodes, graph.edges, graph.subgraphs
    )
}

fn convergent_format_key(style: &StyleChars, limit: usize) -> String {
    format!("{style:?}|{limit}")
}

// Canvas's Display includes its combining stream and omits continuation cells.
// Extract by terminal columns, rejecting split wide glyphs or unsupported
// scalar/grapheme width combinations. Missing trailing display spaces are real
// blank Canvas cells, not missing label text.
fn convergent_positioned_text(
    canvas: &Canvas,
    start: usize,
    end: usize,
    y: usize,
) -> Option<String> {
    if start >= end || end > canvas.width || y >= canvas.height {
        return None;
    }
    let output = format!("{canvas}");
    let row = output.split('\n').nth(y)?;
    if row.chars().map(display_char_width).sum::<usize>() != display_width(row) {
        return None;
    }
    let mut column = 0usize;
    let mut text = String::new();
    for ch in row.chars() {
        let width = display_char_width(ch);
        if width == 0 {
            if start < column && column <= end {
                text.push(ch);
            }
            continue;
        }
        let next = column.checked_add(width)?;
        if (column < start && next > start) || (column < end && next > end) {
            return None;
        }
        if column >= start && next <= end {
            text.push(ch);
        }
        column = next;
    }
    if column < end {
        text.extend(std::iter::repeat_n(' ', end - column.max(start)));
    }
    Some(text)
}

struct ConvergentProofView<'a> {
    canvas: &'a Canvas,
    overlay: BTreeMap<(usize, usize), (usize, ConvergentCellWitness)>,
    reads: RefCell<BTreeMap<(usize, usize), ConvergentCellWitness>>,
    dependencies: RefCell<BTreeSet<usize>>,
}

impl<'a> ConvergentProofView<'a> {
    fn new(
        canvas: &'a Canvas,
        receipts: impl IntoIterator<Item = &'a VerifiedConvergentLabel>,
    ) -> Self {
        let mut overlay = BTreeMap::new();
        for receipt in receipts {
            for cell in &receipt.underlay {
                overlay.insert((cell.x, cell.y), (receipt.sequence, cell.clone()));
            }
        }
        Self {
            canvas,
            overlay,
            reads: RefCell::new(BTreeMap::new()),
            dependencies: RefCell::new(BTreeSet::new()),
        }
    }

    fn peek(&self, x: usize, y: usize) -> Option<ConvergentCellWitness> {
        self.overlay
            .get(&(x, y))
            .map(|(_, witness)| witness.clone())
            .or_else(|| ConvergentCellWitness::capture(self.canvas, x, y))
    }

    fn read(&self, x: usize, y: usize) -> Option<ConvergentCellWitness> {
        let cell = self.peek(x, y)?;
        if let Some((sequence, _)) = self.overlay.get(&(x, y)) {
            self.dependencies.borrow_mut().insert(*sequence);
        }
        self.reads.borrow_mut().insert((x, y), cell.clone());
        Some(cell)
    }
}

impl ConvergentLabelContext {
    fn prepare(&mut self, graph: &Graph, style: &StyleChars, limit: usize) {
        let graph_key = convergent_graph_key(graph);
        let format_key = convergent_format_key(style, limit);
        if self.graph_key.as_ref() != Some(&graph_key)
            || self.format_key.as_ref() != Some(&format_key)
        {
            for receipt in &mut self.receipts {
                receipt.eligible = false;
            }
            self.epoch = self.epoch.saturating_add(1);
            self.graph_key = Some(graph_key);
            self.format_key = Some(format_key);
        }
        self.limit = limit;
    }

    fn intrinsic_indices(
        &self,
        canvas: &Canvas,
        graph: &Graph,
        base_style: &StyleChars,
    ) -> Vec<usize> {
        if self.graph_key.as_ref() != Some(&convergent_graph_key(graph))
            || self.format_key.as_ref() != Some(&convergent_format_key(base_style, self.limit))
        {
            return Vec::new();
        }
        self.receipts
            .iter()
            .enumerate()
            .filter_map(|(index, receipt)| {
                let edge = graph.edges.iter().enumerate().find_map(|(i, edge)| {
                    (edge_owner_id(i, edge) == receipt.owner_id).then_some(edge)
                })?;
                let expected = format_edge_label_with_limit(
                    edge.label.as_deref()?,
                    self.limit.min(canvas.width),
                );
                (receipt.eligible
                    && receipt.epoch == self.epoch
                    && receipt.sequence == index
                    && !receipt.proof.reads.is_empty()
                    && receipt
                        .proof
                        .dependencies
                        .iter()
                        .all(|&prior| prior < index)
                    && edge.from == receipt.from_id
                    && edge.to == receipt.target_id
                    && graph.direction == receipt.direction
                    && expected == receipt.label
                    && convergent_label_supported_width(&receipt.label)
                        == Some(receipt.end.checked_sub(receipt.start)?)
                    && receipt.post.len() == receipt.end - receipt.start
                    && receipt.post.iter().enumerate().all(|(offset, cell)| {
                        cell.x == receipt.start + offset
                            && cell.y == receipt.y
                            && cell.is_current(canvas)
                    })
                    && convergent_positioned_text(canvas, receipt.start, receipt.end, receipt.y)
                        .as_deref()
                        == Some(receipt.label.as_str())
                    && receipt.underlay.iter().all(|cell| {
                        cell.y == receipt.y
                            && cell.x >= receipt.start
                            && cell.x < receipt.end
                            && cell.meta.role == CellRole::Horizontal
                            && cell.meta.ch == cell.raw
                            && cell.meta.owner_kind == CellOwnerKind::EdgeSegment
                            && canvas::is_horizontal(cell.raw, base_style)
                            && (cell.meta.owner_id.as_deref() == Some(receipt.owner_id.as_str())
                                || cell.meta.owner_id.as_deref()
                                    == Some(format!("fanin:{}", receipt.target_id).as_str()))
                    }))
                .then_some(index)
            })
            .collect()
    }

    fn valid_indices(&self, canvas: &Canvas, graph: &Graph, base_style: &StyleChars) -> Vec<usize> {
        let mut indices = self.intrinsic_indices(canvas, graph, base_style);
        // This is a finite removal pass over already witnessed publications,
        // never recursive construction or a new certificate for a future label.
        loop {
            let view = ConvergentProofView::new(canvas, indices.iter().map(|&i| &self.receipts[i]));
            let next: Vec<_> = indices
                .iter()
                .copied()
                .filter(|&i| {
                    self.receipts[i].proof.reads.iter().all(|expected| {
                        view.peek(expected.x, expected.y).as_ref() == Some(expected)
                    })
                })
                .collect();
            if next == indices {
                return next;
            }
            indices = next;
        }
    }

    fn reserves(&self, start: usize, end: usize, y: usize) -> bool {
        self.reservations
            .iter()
            .flatten()
            .any(|&(x, row)| row == y && x >= start && x < end)
    }

    /// Called after EVERY legacy writer, including a failed/partial None.
    pub(super) fn observe_legacy(&mut self, placement: Option<&EdgeLabelPlacement>) {
        if let Some(placement) = placement {
            for receipt in &mut self.receipts {
                if placement
                    .cells
                    .iter()
                    .any(|&(x, y)| y == receipt.y && x >= receipt.start && x < receipt.end)
                {
                    receipt.eligible = false;
                }
            }
            self.reservations.push(placement.cells.clone());
        } else {
            for receipt in &mut self.receipts {
                receipt.eligible = false;
            }
        }
    }
}

fn convergent_node_in_bounds(canvas: &Canvas, node: &Node) -> bool {
    node.width > 0
        && node
            .x
            .checked_add(node.width)
            .is_some_and(|end| end <= canvas.width)
        && node
            .y
            .checked_add(node.height.max(crate::style::BOX_HEIGHT))
            .is_some_and(|end| end <= canvas.height)
        && canvas.is_visible(node)
}

fn convergent_source_cell_owned(cell: &ConvergentCellWitness, graph: &Graph, from: &Node) -> bool {
    cell.meta.ch == cell.raw
        && graph.edges.iter().enumerate().any(|(index, edge)| {
            edge.from == from.id
                && cell.meta.owner_id.as_deref() == Some(edge_owner_id(index, edge).as_str())
                && cell.meta.owner_kind
                    == if edge.is_back_edge {
                        CellOwnerKind::CycleEdge
                    } else {
                        CellOwnerKind::EdgeSegment
                    }
        })
}

fn convergent_path_cell_usable(cell: &ConvergentCellWitness, style: &StyleChars) -> bool {
    let route_owner = matches!(
        cell.meta.owner_kind,
        CellOwnerKind::EdgeSegment
            | CellOwnerKind::CycleEdge
            | CellOwnerKind::ArrowHead
            | CellOwnerKind::Junction
            | CellOwnerKind::PortalOpening
    );
    let horizontal = canvas::is_horizontal(cell.raw, style);
    let vertical = canvas::is_vertical(cell.raw, style);
    let role_matches = match cell.meta.role {
        CellRole::Horizontal => horizontal,
        CellRole::Vertical => vertical,
        CellRole::Corner => {
            canvas::is_corner(cell.raw, style)
                || canvas::is_corner(cell.raw, &crate::style::ASCII_CHARS)
                || canvas::is_corner(cell.raw, &crate::style::UNICODE_CHARS)
        }
        CellRole::Junction => {
            matches!(cell.raw, 'x' | '✕')
                || canvas::is_junction(cell.raw, style)
                || canvas::is_junction(cell.raw, &crate::style::ASCII_CHARS)
                || canvas::is_junction(cell.raw, &crate::style::UNICODE_CHARS)
        }
        CellRole::ArrowTip => canvas::is_arrow(cell.raw),
        CellRole::Portal => horizontal || vertical,
        // Owned dotted shafts have legacy inferred Text roles. Their exact
        // route glyph and owner still distinguish them from caption text.
        CellRole::Text => horizontal || vertical,
        _ => false,
    };
    cell.meta.ch == cell.raw && cell.meta.owner_id.is_some() && route_owner && role_matches
}

fn convergent_path_cell_can_bend(cell: &ConvergentCellWitness, style: &StyleChars) -> bool {
    convergent_path_cell_usable(cell, style)
        && matches!(cell.meta.role, CellRole::Corner | CellRole::Junction)
        && !(matches!(cell.raw, 'x' | '✕') && cell.meta.role == CellRole::Junction)
}

fn convergent_path_connections(
    cell: &ConvergentCellWitness,
    style: &StyleChars,
) -> super::topology::Connections {
    use super::topology::{
        char_connects_down, char_connects_left, char_connects_right, char_connects_up, Connections,
    };
    if !convergent_path_cell_usable(cell, style) {
        return Connections::default();
    }
    let ch = cell.raw;
    let mut arms = Connections {
        up: char_connects_up(ch),
        down: char_connects_down(ch),
        left: char_connects_left(ch),
        right: char_connects_right(ch),
    };
    if cell.meta.role == CellRole::Corner {
        arms.up |= ch == style.corner_ur || ch == style.corner_ul;
        arms.down |= ch == style.corner_dr || ch == style.corner_dl;
        arms.left |= ch == style.corner_dr || ch == style.corner_ur;
        arms.right |= ch == style.corner_dl || ch == style.corner_ul;
    } else if cell.meta.role == CellRole::Junction && !matches!(ch, 'x' | '✕') {
        arms.up |= ch == style.junction_up
            || ch == style.junction_left
            || ch == style.junction_right
            || ch == style.cross;
        arms.down |= ch == style.junction_down
            || ch == style.junction_left
            || ch == style.junction_right
            || ch == style.cross;
        arms.left |= ch == style.junction_up
            || ch == style.junction_down
            || ch == style.junction_left
            || ch == style.cross;
        arms.right |= ch == style.junction_up
            || ch == style.junction_down
            || ch == style.junction_right
            || ch == style.cross;
    }
    // A block shaft has raw arms in both axes. Its typed role still permits
    // only straight transit; only an actual Corner/Junction may turn.
    if cell.meta.role == CellRole::Horizontal {
        arms.up = false;
        arms.down = false;
    } else if cell.meta.role == CellRole::Vertical {
        arms.left = false;
        arms.right = false;
    }
    arms
}

fn convergent_vertical_join(
    view: &ConvergentProofView<'_>,
    x: usize,
    y: usize,
    next_y: usize,
    up: bool,
    style: &StyleChars,
) -> bool {
    let (Some(here), Some(there)) = (view.read(x, y), view.read(x, next_y)) else {
        return false;
    };
    let here = convergent_path_connections(&here, style);
    let there = convergent_path_connections(&there, style);
    if up {
        here.up && there.down
    } else {
        here.down && there.up
    }
}

fn convergent_horizontal_join(
    view: &ConvergentProofView<'_>,
    x: usize,
    next_x: usize,
    y: usize,
    left: bool,
    style: &StyleChars,
) -> bool {
    let (Some(here), Some(there)) = (view.read(x, y), view.read(next_x, y)) else {
        return false;
    };
    let here = convergent_path_connections(&here, style);
    let there = convergent_path_connections(&there, style);
    if left {
        here.left && there.right
    } else {
        here.right && there.left
    }
}

fn convergent_target_head(
    view: &ConvergentProofView<'_>,
    graph: &Graph,
    to: &Node,
    x: usize,
    y: usize,
    up: bool,
) -> bool {
    let Some(cell) = view.read(x, y) else {
        return false;
    };
    let correct_direction = if up {
        matches!(cell.raw, '^' | '↑' | '▲')
    } else {
        matches!(cell.raw, 'v' | '↓' | '▼')
    };
    correct_direction
        && cell.meta.ch == cell.raw
        && cell.meta.role == CellRole::ArrowTip
        && (cell.meta.owner_id.as_deref() == Some(format!("fanin:{}", to.id).as_str())
            || graph.edges.iter().enumerate().any(|(i, edge)| {
                edge.to == to.id
                    && cell.meta.owner_id.as_deref() == Some(edge_owner_id(i, edge).as_str())
            }))
}

#[allow(clippy::too_many_arguments)]
fn prove_convergent_path(
    canvas: &Canvas,
    graph: &Graph,
    from: &Node,
    to: &Node,
    direction: Direction,
    style: &StyleChars,
    owner_id: &str,
    context: &ConvergentLabelContext,
) -> Option<ConvergentPathProof> {
    if !matches!(direction, Direction::TD | Direction::TB | Direction::BT)
        || graph.direction != direction
        || !convergent_node_in_bounds(canvas, from)
        || !convergent_node_in_bounds(canvas, to)
        || graph.get_node(&from.id).map(|n| format!("{n:?}")) != Some(format!("{from:?}"))
        || graph.get_node(&to.id).map(|n| format!("{n:?}")) != Some(format!("{to:?}"))
        || !graph.edges.iter().enumerate().any(|(i, edge)| {
            edge_owner_id(i, edge) == owner_id && edge.from == from.id && edge.to == to.id
        })
    {
        return None;
    }
    let source_x = from.x.checked_add(from.width / 2)?;
    let target_right = to.x.checked_add(to.width)?;
    let up = direction == Direction::BT;
    let stem_start = if up {
        from.y.checked_sub(1)?
    } else {
        from.y
            .checked_add(from.height.max(crate::style::BOX_HEIGHT))?
    };
    let target_end = if up {
        to.y.checked_add(to.height.max(crate::style::BOX_HEIGHT))?
    } else {
        to.y.checked_sub(1)?
    };
    if stem_start >= canvas.height
        || target_end >= canvas.height
        || (up && target_end > stem_start)
        || (!up && target_end < stem_start)
    {
        return None;
    }
    let indices = context.valid_indices(canvas, graph, style);
    let view = ConvergentProofView::new(canvas, indices.iter().map(|&i| &context.receipts[i]));
    let entry = view.read(source_x, stem_start)?;
    if !convergent_source_cell_owned(&entry, graph, from)
        || !(if up {
            convergent_path_connections(&entry, style).down
        } else {
            convergent_path_connections(&entry, style).up
        })
    {
        return None;
    }
    let next_y = |y: usize| {
        if up {
            y.checked_sub(1)
        } else {
            y.checked_add(1).filter(|&n| n < canvas.height)
        }
    };
    let reaches_target = |x: usize, mut y: usize| {
        if !(to.x..target_right).contains(&x) {
            return false;
        }
        while y != target_end {
            let Some(next) = next_y(y) else {
                return false;
            };
            if (up && next < target_end)
                || (!up && next > target_end)
                || !convergent_vertical_join(&view, x, y, next, up, style)
            {
                return false;
            }
            y = next;
        }
        convergent_target_head(&view, graph, to, x, y, up)
    };
    let mut y = stem_start;
    let (turn_y, target_x) = 'source: loop {
        if reaches_target(source_x, y) {
            break (target_end, source_x);
        }
        let source = view.read(source_x, y)?;
        if convergent_path_cell_can_bend(&source, style) {
            // Actual target ports can lie beside its center. Walk each
            // source-facing run only as far as the target's declared extent.
            for left in [true, false] {
                if (left && source_x <= to.x) || (!left && source_x >= target_right - 1) {
                    continue;
                }
                let mut x = source_x;
                loop {
                    let next = if left {
                        x.checked_sub(1)
                    } else {
                        x.checked_add(1)
                    };
                    let Some(next) = next else { break };
                    if (left && next < to.x)
                        || (!left && next >= target_right)
                        || !convergent_horizontal_join(&view, x, next, y, left, style)
                    {
                        break;
                    }
                    x = next;
                    if view
                        .read(x, y)
                        .is_some_and(|cell| convergent_path_cell_can_bend(&cell, style))
                        && reaches_target(x, y)
                    {
                        break 'source (y, x);
                    }
                }
            }
        }
        let next = next_y(y)?;
        if (up && next < target_end)
            || (!up && next > target_end)
            || !convergent_vertical_join(&view, source_x, y, next, up, style)
        {
            return None;
        }
        y = next;
    };
    let reads = view.reads.into_inner().into_values().collect();
    let dependencies = view.dependencies.into_inner().into_iter().collect();
    Some(ConvergentPathProof {
        turn_y,
        source_x,
        stem_start,
        target_x,
        target_end,
        up,
        reads,
        dependencies,
    })
}

struct ConvergentLabelSite<'a> {
    graph: &'a Graph,
    from: &'a Node,
    to: &'a Node,
    direction: Direction,
    style: &'a StyleChars,
    owner_id: &'a str,
    label: &'a str,
    context: &'a ConvergentLabelContext,
    proof: Option<ConvergentPathProof>,
    valid: Vec<usize>,
}

impl ConvergentLabelSite<'_> {
    fn annotation_run_applies(&self, start: usize, end: usize, y: usize) -> bool {
        self.proof.as_ref().is_some_and(|proof| {
            y == proof.turn_y
                && proof.source_x != proof.target_x
                && start > proof.source_x.min(proof.target_x)
                && end <= proof.source_x.max(proof.target_x)
        })
    }

    fn collector_applies(&self, canvas: &Canvas, start: usize, end: usize, y: usize) -> bool {
        self.graph.edges.iter().enumerate().any(|(i, edge)| {
            edge_owner_id(i, edge) == self.owner_id
                && !edge.is_back_edge
                && edge.from == self.from.id
                && edge.to == self.to.id
        }) && self.annotation_run_applies(start, end, y)
            && self
                .graph
                .edges
                .iter()
                .filter(|edge| {
                    !edge.is_back_edge
                        && edge.to == self.to.id
                        && self
                            .graph
                            .get_node(&edge.from)
                            .is_some_and(|node| convergent_node_in_bounds(canvas, node))
                })
                .count()
                >= 2
    }

    fn neighbor_reservation(&self, start: usize, end: usize, y: usize) -> bool {
        self.context
            .reservations
            .iter()
            .flatten()
            .any(|&(x, row)| row.abs_diff(y) == 1 && x >= start && x < end)
    }

    fn shaft_at(&self, canvas: &Canvas, node: &Node, x: usize, y: usize) -> bool {
        ConvergentCellWitness::capture(canvas, x, y).is_some_and(|cell| {
            matches!(cell.meta.role, CellRole::Vertical | CellRole::Text)
                && canvas::is_vertical(cell.raw, self.style)
                && convergent_source_cell_owned(&cell, self.graph, node)
        })
    }

    fn own_gap(
        &self,
        canvas: &Canvas,
        start: usize,
        end: usize,
        y: usize,
        stem: usize,
    ) -> Option<usize> {
        let (gap_start, gap_end, distance) = if end <= stem {
            (end, stem, stem.checked_sub(end.checked_sub(1)?)?)
        } else if stem < start {
            (stem.checked_add(1)?, start, start - stem)
        } else {
            return None;
        };
        if gap_start >= gap_end || convergent_label_reserved(self.graph, gap_start, gap_end, y) {
            return None;
        }
        (gap_start..gap_end)
            .all(|x| {
                canvas.get(x, y) == ' '
                    && canvas.get_meta(x, y).is_some_and(|meta| {
                        meta.ch == ' '
                            && meta.owner_kind == CellOwnerKind::Empty
                            && meta.role == CellRole::Empty
                            && meta.owner_id.is_none()
                            && meta.z_index == 0
                    })
            })
            .then_some(distance)
    }

    fn span_distance(start: usize, end: usize, stem: usize) -> usize {
        if stem < start {
            start - stem
        } else if stem >= end {
            stem - (end - 1)
        } else {
            0
        }
    }

    fn structured_peer(
        &self,
        canvas: &Canvas,
        start: usize,
        end: usize,
        y: usize,
    ) -> Option<usize> {
        let proof = self.proof.as_ref()?;
        if self
            .graph
            .edges
            .iter()
            .enumerate()
            .any(|(i, edge)| edge_owner_id(i, edge) == self.owner_id && edge.is_back_edge)
            || y < proof.stem_start.min(proof.turn_y)
            || y > proof.stem_start.max(proof.turn_y)
        {
            return None;
        }
        let margin_start = start.saturating_sub(1);
        let margin_end = end.checked_add(1)?.min(canvas.width);
        let mut blocking = BTreeSet::new();
        for row in [y.checked_sub(1), y.checked_add(1)].into_iter().flatten() {
            if row >= canvas.height {
                continue;
            }
            for x in margin_start..margin_end {
                if is_textual(canvas.get(x, row)) {
                    blocking.insert((x, row));
                }
            }
        }
        for &(x, row) in self.context.reservations.iter().flatten() {
            if row.abs_diff(y) == 1 && x >= margin_start && x < margin_end {
                blocking.insert((x, row));
            }
        }
        if blocking.is_empty() {
            return None;
        }
        let mut peers = self.valid.iter().copied().filter(|&i| {
            let peer = &self.context.receipts[i];
            if peer.direction != self.direction
                || peer.target_id != self.to.id
                || peer.from_id == self.from.id
                || peer.y.abs_diff(y) != 1
                || !blocking
                    .iter()
                    .all(|&(x, row)| row == peer.y && x >= peer.start && x < peer.end)
            {
                return false;
            }
            let Some(peer_edge) =
                self.graph.edges.iter().enumerate().find_map(|(i, edge)| {
                    (edge_owner_id(i, edge) == peer.owner_id).then_some(edge)
                })
            else {
                return false;
            };
            let Some(peer_from) = self.graph.get_node(&peer.from_id) else {
                return false;
            };
            if peer_edge.is_back_edge || peer.proof.source_x == proof.source_x {
                return false;
            }
            let (own_stem, peer_stem) = (proof.source_x, peer.proof.source_x);
            for row in [y, peer.y] {
                if !self.shaft_at(canvas, self.from, own_stem, row)
                    || !self.shaft_at(canvas, peer_from, peer_stem, row)
                {
                    return false;
                }
            }
            let Some(own_distance) = self.own_gap(canvas, start, end, y, own_stem) else {
                return false;
            };
            let Some(peer_distance) = self.own_gap(canvas, peer.start, peer.end, peer.y, peer_stem)
            else {
                return false;
            };
            own_distance < Self::span_distance(start, end, peer_stem)
                && peer_distance < Self::span_distance(peer.start, peer.end, own_stem)
        });
        let first = peers.next()?;
        peers.next().is_none().then_some(first)
    }

    fn writable(
        &self,
        canvas: &Canvas,
        start: usize,
        y: usize,
        annotation: bool,
        structured: bool,
    ) -> bool {
        convergent_label_slot_in_context(
            canvas,
            self.graph,
            start,
            y,
            self.label,
            self.style,
            self.owner_id,
            annotation,
            Some(self),
            structured,
        )
    }
}

struct ConvergentPublication {
    cells: Vec<(usize, usize)>,
    start: usize,
    end: usize,
    y: usize,
    proof: Option<ConvergentPathProof>,
    underlay: Vec<ConvergentCellWitness>,
}

fn commit_convergent_label(
    canvas: &mut Canvas,
    start: usize,
    y: usize,
    label: &str,
) -> Vec<(usize, usize)> {
    let mut cells = Vec::new();
    let mut cursor = start;
    let mut base = None;
    for ch in label.chars() {
        let advance = display_char_width(ch);
        if advance == 0 {
            canvas.set(base.expect("preflight visible label base"), y, ch);
            continue;
        }
        canvas.set(cursor, y, ch);
        cells.push((cursor, y));
        for offset in 1..advance {
            canvas.set(cursor + offset, y, '\0');
            cells.push((cursor + offset, y));
        }
        base = (ch != ' ').then_some(cursor);
        cursor += advance;
    }
    cells
}

fn commit_convergent_site(
    canvas: &mut Canvas,
    site: &ConvergentLabelSite<'_>,
    start: usize,
    y: usize,
    annotation: bool,
    structured: bool,
) -> Option<ConvergentPublication> {
    if !site.writable(canvas, start, y, annotation, structured) {
        return None;
    }
    let end = start.checked_add(convergent_label_supported_width(site.label)?)?;
    let underlay: Vec<_> = (start..end)
        .filter_map(|x| {
            let cell = ConvergentCellWitness::capture(canvas, x, y)?;
            (annotation
                && cell.meta.ch == cell.raw
                && cell.meta.role == CellRole::Horizontal
                && cell.meta.owner_kind == CellOwnerKind::EdgeSegment
                && canvas::is_horizontal(cell.raw, site.style))
            .then_some(cell)
        })
        .collect();
    let proof = site
        .proof
        .as_ref()
        .filter(|proof| {
            if site
                .graph
                .edges
                .iter()
                .enumerate()
                .any(|(i, edge)| edge_owner_id(i, edge) == site.owner_id && edge.is_back_edge)
            {
                return false;
            }
            let on_run = annotation
                && site.annotation_run_applies(
                    start.saturating_sub(1),
                    end.saturating_add(1).min(canvas.width),
                    y,
                );
            on_run
                || (y >= proof.stem_start.min(proof.turn_y)
                    && y <= proof.stem_start.max(proof.turn_y)
                    && site.shaft_at(canvas, site.from, proof.source_x, y))
        })
        .cloned();
    let cells = commit_convergent_label(canvas, start, y, site.label);
    Some(ConvergentPublication {
        cells,
        start,
        end,
        y,
        proof,
        underlay,
    })
}

impl ConvergentLabelContext {
    #[allow(clippy::too_many_arguments)]
    fn record_publication(
        &mut self,
        canvas: &Canvas,
        from: &Node,
        to: &Node,
        direction: Direction,
        owner_id: String,
        label: String,
        publication: ConvergentPublication,
    ) {
        self.reservations.push(publication.cells.clone());
        let Some(proof) = publication.proof else {
            return;
        };
        let sequence = self.receipts.len();
        if proof.dependencies.iter().any(|&prior| prior >= sequence)
            || convergent_positioned_text(canvas, publication.start, publication.end, publication.y)
                .as_deref()
                != Some(label.as_str())
        {
            return;
        }
        let post: Option<Vec<_>> = publication
            .cells
            .iter()
            .map(|&(x, y)| ConvergentCellWitness::capture(canvas, x, y))
            .collect();
        let Some(post) = post else {
            return;
        };
        self.receipts.push(VerifiedConvergentLabel {
            sequence,
            epoch: self.epoch,
            eligible: true,
            owner_id,
            from_id: from.id.clone(),
            target_id: to.id.clone(),
            direction,
            label,
            start: publication.start,
            end: publication.end,
            y: publication.y,
            post,
            underlay: publication.underlay,
            proof,
        });
    }
}

fn convergent_label_span_in_bounds(
    canvas: &Canvas,
    start_x: usize,
    y: usize,
    width: usize,
) -> bool {
    width > 0
        && y < canvas.height
        && start_x
            .checked_add(width)
            .is_some_and(|end| end <= canvas.width)
}

// Canvas stores one base glyph per cell. Keep unsupported grapheme-width
// combinations out of this transaction rather than publishing a partial span.
fn convergent_label_supported_width(label: &str) -> Option<usize> {
    let mut width = 0usize;
    let mut has_base = false;
    for ch in label.chars() {
        if ch.is_control() {
            return None;
        }
        let advance = display_char_width(ch);
        if advance == 0 {
            if !has_base {
                return None;
            }
        } else {
            width = width.checked_add(advance)?;
            has_base = ch != ' ';
        }
    }
    (width > 0 && width == display_width(label)).then_some(width)
}

fn convergent_label_reserved(graph: &Graph, start: usize, end: usize, y: usize) -> bool {
    if graph.nodes.iter().any(|node| {
        let (Some(right), Some(bottom)) = (
            node.x.checked_add(node.width),
            node.y.checked_add(node.height),
        ) else {
            return true;
        };
        y >= node.y && y < bottom && start < right && end > node.x
    }) {
        return true;
    }
    graph.subgraphs.iter().any(|sg| {
        if !sg.bounds.is_valid() {
            return false;
        }
        let bounds = &sg.bounds;
        let (Some(right), Some(bottom)) = (
            bounds.x.checked_add(bounds.width),
            bounds.y.checked_add(bounds.height),
        ) else {
            return true;
        };
        let intersects = start < right && end > bounds.x;
        let horizontal_border = (y == bounds.y || y == bottom - 1) && intersects;
        let title_band =
            sg.title.is_some() && y == subgraph_title_y(bounds, graph.direction) && intersects;
        let side_gutter = y >= bounds.y
            && y < bottom
            && [bounds.x.saturating_sub(1), bounds.x, right - 1, right]
                .into_iter()
                .any(|x| x >= start && x < end);
        horizontal_border || title_band || side_gutter
    })
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
fn convergent_label_slot_is_writable(
    canvas: &Canvas,
    graph: &Graph,
    start_x: usize,
    y: usize,
    label: &str,
    style: &StyleChars,
    owner_id: &str,
    allow_horizontal_annotation: bool,
) -> bool {
    convergent_label_slot_in_context(
        canvas,
        graph,
        start_x,
        y,
        label,
        style,
        owner_id,
        allow_horizontal_annotation,
        None,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn convergent_label_slot_in_context(
    canvas: &Canvas,
    graph: &Graph,
    start_x: usize,
    y: usize,
    label: &str,
    style: &StyleChars,
    owner_id: &str,
    allow_horizontal_annotation: bool,
    site: Option<&ConvergentLabelSite<'_>>,
    structured: bool,
) -> bool {
    let Some(width) = convergent_label_supported_width(label) else {
        return false;
    };
    if !convergent_label_span_in_bounds(canvas, start_x, y, width) {
        return false;
    }
    let Some(owner_edge) = graph
        .edges
        .iter()
        .enumerate()
        .find_map(|(index, edge)| (edge_owner_id(index, edge) == owner_id).then_some(edge))
    else {
        return false;
    };
    let end = start_x + width;
    let margin_start = start_x.saturating_sub(1);
    let margin_end = end.checked_add(1).unwrap_or(end).min(canvas.width);
    if convergent_label_reserved(graph, margin_start, margin_end, y)
        || site.is_some_and(|site| site.context.reserves(margin_start, margin_end, y))
    {
        return false;
    }
    let neighbors_clear = label_neighbour_rows_clear(canvas, start_x, width, y)
        && site.is_none_or(|site| !site.neighbor_reservation(margin_start, margin_end, y));
    if !neighbors_clear
        && !(structured
            && site.is_some_and(|site| site.structured_peer(canvas, start_x, end, y).is_some()))
    {
        return false;
    }
    (margin_start..margin_end).all(|x| {
        let ch = canvas.get(x, y);
        let Some(meta) = canvas.get_meta(x, y) else {
            return false;
        };
        if ch == ' ' {
            return meta.ch == ch
                && meta.owner_kind == CellOwnerKind::Empty
                && meta.role == CellRole::Empty
                && meta.owner_id.is_none()
                && meta.z_index == 0;
        }
        allow_horizontal_annotation
            && canvas::is_horizontal(ch, style)
            && meta.ch == ch
            && meta.role == CellRole::Horizontal
            && ((meta.owner_id.as_deref() == Some(owner_id)
                && site
                    .is_none_or(|site| site.annotation_run_applies(margin_start, margin_end, y))
                && (if owner_edge.is_back_edge {
                    meta.owner_kind == CellOwnerKind::CycleEdge
                } else {
                    meta.owner_kind == CellOwnerKind::EdgeSegment
                }))
                || (meta.owner_kind == CellOwnerKind::EdgeSegment
                    && site.is_some_and(|site| {
                        meta.owner_id.as_deref() == Some(format!("fanin:{}", site.to.id).as_str())
                            && site.collector_applies(canvas, margin_start, margin_end, y)
                    })))
    })
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
fn try_publish_convergent_label(
    canvas: &mut Canvas,
    graph: &Graph,
    start_x: usize,
    y: usize,
    label: &str,
    style: &StyleChars,
    owner_id: &str,
    allow_horizontal_annotation: bool,
) -> Option<Vec<(usize, usize)>> {
    if !convergent_label_slot_is_writable(
        canvas,
        graph,
        start_x,
        y,
        label,
        style,
        owner_id,
        allow_horizontal_annotation,
    ) {
        return None;
    }
    Some(commit_convergent_label(canvas, start_x, y, label))
}

// The turn tracer deliberately returns None for aligned endpoints. Prove
// that special case by walking mutual raw joins all the way to the target's
// direction-correct arrow, without jumping across a gap or another column.
#[cfg(test)]
fn convergent_straight_end(
    canvas: &Canvas,
    source_x: usize,
    stem_start: usize,
    up: bool,
    to: &Node,
    style: &StyleChars,
) -> Option<usize> {
    use super::topology::{char_connects_down, char_connects_up};
    if source_x != super::cycle::center_x(to) {
        return None;
    }
    let end = if up {
        to.y.checked_add(to.height)?
    } else {
        to.y.checked_sub(1)?
    };
    if source_x >= canvas.width
        || stem_start >= canvas.height
        || end >= canvas.height
        || (up && end > stem_start)
        || (!up && end < stem_start)
    {
        return None;
    }
    let mut y = stem_start;
    while y != end {
        let next = if up { y - 1 } else { y + 1 };
        let (here, there) = (canvas.get(source_x, y), canvas.get(source_x, next));
        if !(if up {
            char_connects_up(here) && char_connects_down(there)
        } else {
            char_connects_down(here) && char_connects_up(there)
        }) {
            return None;
        }
        y = next;
    }
    (canvas.get(source_x, end) == if up { style.arrow_up } else { style.arrow_down }).then_some(end)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConvergentSourcePrefix {
    source_x: usize,
    stem_start: usize,
    rows: Vec<usize>,
}

// This is placement evidence only. It never supplies a target-path witness.
fn convergent_prefix_owner(
    canvas: &Canvas,
    site: &ConvergentLabelSite<'_>,
    cell: &ConvergentCellWitness,
) -> bool {
    cell.meta.ch == cell.raw
        && cell.meta.owner_kind == CellOwnerKind::EdgeSegment
        && site.graph.edges.iter().enumerate().any(|(index, edge)| {
            if edge.is_back_edge
                || cell.meta.owner_id.as_deref() != Some(edge_owner_id(index, edge).as_str())
                || (cell.meta.role == CellRole::Text
                    && (edge.kind != crate::graph::EdgeKind::Dotted
                        || cell.raw != site.style.dotted_v))
            {
                return false;
            }
            let (Some(peer), Some(target)) = (
                site.graph.get_node(&edge.from),
                site.graph.get_node(&edge.to),
            ) else {
                return false;
            };
            if !convergent_node_in_bounds(canvas, peer)
                || !convergent_node_in_bounds(canvas, target)
            {
                return false;
            }
            if peer.id == site.from.id {
                return true;
            }
            if edge.to != site.to.id
                || peer.x.checked_add(peer.width / 2)
                    != site.from.x.checked_add(site.from.width / 2)
            {
                return false;
            }
            if site.direction == Direction::BT {
                site.from
                    .y
                    .checked_add(site.from.height.max(crate::style::BOX_HEIGHT))
                    .is_some_and(|bottom| peer.y >= bottom)
            } else {
                peer.y
                    .checked_add(peer.height.max(crate::style::BOX_HEIGHT))
                    .is_some_and(|bottom| bottom <= site.from.y)
            }
        })
}

fn convergent_prefix_reserved(graph: &Graph, start: usize, end: usize, y: usize) -> bool {
    convergent_label_reserved(graph, start, end, y)
        || graph.nodes.iter().any(|node| {
            let (Some(right), Some(bottom)) = (
                node.x.checked_add(node.width),
                node.y
                    .checked_add(node.height.max(crate::style::BOX_HEIGHT)),
            ) else {
                return true;
            };
            y >= node.y && y < bottom && start < right && end > node.x
        })
}

fn convergent_source_prefix(
    canvas: &Canvas,
    site: &ConvergentLabelSite<'_>,
) -> Option<ConvergentSourcePrefix> {
    if !matches!(
        site.direction,
        Direction::TD | Direction::TB | Direction::BT
    ) || site.graph.direction != site.direction
        || !convergent_node_in_bounds(canvas, site.from)
        || !convergent_node_in_bounds(canvas, site.to)
        || site
            .graph
            .get_node(&site.from.id)
            .map(|node| format!("{node:?}"))
            != Some(format!("{:?}", site.from))
        || site
            .graph
            .get_node(&site.to.id)
            .map(|node| format!("{node:?}"))
            != Some(format!("{:?}", site.to))
        || !site.graph.edges.iter().enumerate().any(|(index, edge)| {
            !edge.is_back_edge
                && edge_owner_id(index, edge) == site.owner_id
                && edge.from == site.from.id
                && edge.to == site.to.id
        })
        || site
            .graph
            .edges
            .iter()
            .filter(|edge| {
                !edge.is_back_edge
                    && edge.to == site.to.id
                    && site
                        .graph
                        .get_node(&edge.from)
                        .is_some_and(|node| convergent_node_in_bounds(canvas, node))
            })
            .count()
            < 2
    {
        return None;
    }
    let source_x = site.from.x.checked_add(site.from.width / 2)?;
    let up = site.direction == Direction::BT;
    let stem_start = if up {
        site.from.y.checked_sub(1)?
    } else {
        site.from
            .y
            .checked_add(site.from.height.max(crate::style::BOX_HEIGHT))?
    };
    let terminal = if up {
        site.to
            .y
            .checked_add(site.to.height.max(crate::style::BOX_HEIGHT))?
    } else {
        site.to.y.checked_sub(1)?
    };
    if stem_start >= canvas.height
        || terminal >= canvas.height
        || (up && terminal > stem_start)
        || (!up && terminal < stem_start)
    {
        return None;
    }
    let end_x = source_x.checked_add(1)?;
    let mut rows = Vec::new();
    let mut y = stem_start;
    loop {
        if convergent_prefix_reserved(site.graph, source_x, end_x, y)
            || site.context.reserves(source_x, end_x, y)
        {
            break;
        }
        let cell = ConvergentCellWitness::capture(canvas, source_x, y)?;
        let role_ok = matches!(cell.meta.role, CellRole::Vertical | CellRole::Text)
            && canvas::is_vertical(cell.raw, site.style)
            || (y != stem_start
                && cell.meta.role == CellRole::Junction
                && !matches!(cell.raw, 'x' | '✕'));
        let arms = convergent_path_connections(&cell, site.style);
        if !role_ok || !convergent_prefix_owner(canvas, site, &cell) || !arms.up || !arms.down {
            break;
        }
        // Both arms on every retained raw cell establish mutual straight joins.
        rows.push(y);
        if y == terminal {
            break;
        }
        y = if up {
            y.checked_sub(1)?
        } else {
            y.checked_add(1)?
        };
    }
    (!rows.is_empty()).then_some(ConvergentSourcePrefix {
        source_x,
        stem_start,
        rows,
    })
}

fn convergent_prefix_gap_clear(
    canvas: &Canvas,
    site: &ConvergentLabelSite<'_>,
    start: usize,
    end: usize,
    y: usize,
    stem: usize,
) -> bool {
    let (low, high) = if end <= stem {
        (end, stem)
    } else if start > stem {
        let Some(low) = stem.checked_add(1) else {
            return false;
        };
        (low, start)
    } else {
        return false;
    };
    low < high
        && !convergent_prefix_reserved(site.graph, low, high, y)
        && !site.context.reserves(low, high, y)
        && (low..high).all(|x| {
            canvas.get(x, y) == ' '
                && canvas.get_meta(x, y).is_some_and(|meta| {
                    meta.ch == ' '
                        && meta.owner_kind == CellOwnerKind::Empty
                        && meta.role == CellRole::Empty
                        && meta.owner_id.is_none()
                        && meta.z_index == 0
                })
        })
}

fn commit_convergent_source_prefix(
    canvas: &mut Canvas,
    site: &ConvergentLabelSite<'_>,
    start: usize,
    y: usize,
    max_canvas_width: usize,
) -> Option<ConvergentPublication> {
    let prefix = convergent_source_prefix(canvas, site)?;
    let width = convergent_label_supported_width(site.label)?;
    let left = width
        .checked_add(2)
        .and_then(|gap| prefix.source_x.checked_sub(gap));
    let right = prefix.source_x.checked_add(2);
    if !prefix.rows.contains(&y) || (Some(start) != left && Some(start) != right) {
        return None;
    }
    let end = start.checked_add(width)?;
    let commit = |target: &mut Canvas| {
        if convergent_prefix_reserved(
            site.graph,
            start.saturating_sub(1),
            end.checked_add(1).unwrap_or(end).min(target.width),
            y,
        ) || !convergent_prefix_gap_clear(target, site, start, end, y, prefix.source_x)
        {
            return None;
        }
        let mut publication = commit_convergent_site(target, site, start, y, false, false)?;
        publication.proof = None;
        publication.underlay.clear();
        Some(publication)
    };
    if end <= canvas.width {
        return commit(canvas);
    }
    if Some(start) != right || end > max_canvas_width {
        return None;
    }
    let mut expanded = canvas.clone();
    expanded.ensure_width(end);
    let publication = commit(&mut expanded)?;
    *canvas = expanded;
    Some(publication)
}

fn publish_convergent_source_prefix(
    canvas: &mut Canvas,
    site: &ConvergentLabelSite<'_>,
    preferred_y: usize,
    max_canvas_width: usize,
) -> Option<ConvergentPublication> {
    let mut prefix = convergent_source_prefix(canvas, site)?;
    let width = convergent_label_supported_width(site.label)?;
    let target_x = site.to.x.checked_add(site.to.width / 2)?;
    let left = width
        .checked_add(2)
        .and_then(|gap| prefix.source_x.checked_sub(gap));
    let right = prefix.source_x.checked_add(2);
    let sides = if prefix.source_x <= target_x {
        [left, right]
    } else {
        [right, left]
    };
    prefix
        .rows
        .sort_unstable_by_key(|&y| (y.abs_diff(preferred_y), y.abs_diff(prefix.stem_start), y));
    for grow in [false, true] {
        for &y in &prefix.rows {
            for start in sides.into_iter().flatten() {
                let Some(end) = start.checked_add(width) else {
                    continue;
                };
                if (end > canvas.width) != grow {
                    continue;
                }
                if let Some(publication) =
                    commit_convergent_source_prefix(canvas, site, start, y, max_canvas_width)
                {
                    return Some(publication);
                }
            }
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn publish_vertical_convergent_label(
    canvas: &mut Canvas,
    graph: &Graph,
    from: &Node,
    to: &Node,
    direction: Direction,
    preferred_x: usize,
    preferred_y: usize,
    label: &str,
    style: &StyleChars,
    owner_id: &str,
    annotation: bool,
    base_style: &StyleChars,
    context: &ConvergentLabelContext,
    max_canvas_width: usize,
) -> Option<ConvergentPublication> {
    let proof = prove_convergent_path(
        canvas, graph, from, to, direction, base_style, owner_id, context,
    );
    let valid = context.valid_indices(canvas, graph, base_style);
    let site = ConvergentLabelSite {
        graph,
        from,
        to,
        direction,
        style,
        owner_id,
        label,
        context,
        proof,
        valid,
    };
    if let Some(publication) =
        commit_convergent_site(canvas, &site, preferred_x, preferred_y, annotation, false)
    {
        return Some(publication);
    }
    let width = convergent_label_supported_width(label)?;
    let Some(proof) = site.proof.as_ref() else {
        return publish_convergent_source_prefix(canvas, &site, preferred_y, max_canvas_width);
    };
    let (source_x, target_x, turn) = (proof.source_x, proof.target_x, proof.turn_y);

    // A rejected old nearest candidate does not make the whole proved run
    // unavailable. Test every bounded same-row alternative with the SAME
    // complete transaction guard before moving text beside a stem.
    let low_x = source_x.min(target_x).checked_add(1)?;
    let high_x = source_x.max(target_x);
    if low_x < high_x && width <= high_x - low_x {
        let last = high_x - width;
        let desired = preferred_x.clamp(low_x, last);
        let mut starts: Vec<_> = (low_x..=last).collect();
        starts.sort_unstable_by_key(|&x| (x.abs_diff(desired), x));
        for start in starts {
            if let Some(publication) =
                commit_convergent_site(canvas, &site, start, turn, true, false)
            {
                return Some(publication);
            }
        }
    }
    let mut rows: Vec<_> = (proof.stem_start.min(turn)..=proof.stem_start.max(turn)).collect();
    rows.sort_unstable_by_key(|&y| (y.abs_diff(preferred_y), y.abs_diff(proof.stem_start), y));
    let left = width
        .checked_add(2)
        .and_then(|gap| source_x.checked_sub(gap));
    let right = source_x.checked_add(2);
    let sides = if source_x <= target_x {
        [left, right]
    } else {
        [right, left]
    };

    // Exhaust ALL ordinary candidates, including the raw reserve-only prefix,
    // before the named full-proof adjacency exception.
    for structured in [false, true] {
        if structured {
            if let Some(publication) =
                publish_convergent_source_prefix(canvas, &site, preferred_y, max_canvas_width)
            {
                return Some(publication);
            }
        }
        for &y in &rows {
            if !site.shaft_at(canvas, from, source_x, y) {
                continue;
            }
            for start in sides.into_iter().flatten() {
                if let Some(publication) =
                    commit_convergent_site(canvas, &site, start, y, false, structured)
                {
                    return Some(publication);
                }
            }
        }
    }
    None
}

/// Draw an edge label for convergent edges (multiple sources to one target).
/// Labels are placed on the branch's outer side before the merge point so they
/// do not crowd the shared junction corridor.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_convergent_edge_label(
    canvas: &mut Canvas,
    from: &Node,
    to: &Node,
    label: &str,
    direction: Direction,
    style: &StyleChars,
    config: &Config,
    edge_idx: usize,
    edge: &crate::graph::Edge,
    graph: &Graph,
    context: &mut ConvergentLabelContext,
) -> Option<EdgeLabelPlacement> {
    let vertical = matches!(direction, Direction::TD | Direction::TB | Direction::BT);
    if vertical
        && (!convergent_node_in_bounds(canvas, from) || !convergent_node_in_bounds(canvas, to))
    {
        return None;
    }
    let base_style = *style;
    let kind_style = edge_label_style(style, edge);
    let style = &kind_style;
    use super::cycle::{center_x, center_y};

    // Use slightly shorter limit for convergent labels to avoid crowding at merge points
    let convergent_limit = config.max_edge_label_width.saturating_sub(2).max(8);
    let display_label = format_edge_label_with_limit(label, convergent_limit.min(canvas.width));
    let label_width = display_width(&display_label);
    let owner_id = edge_owner_id(edge_idx, edge);
    let mut cells = Vec::new();
    let mut publication = None;
    let mut working_context = context.clone();
    if vertical {
        working_context.prepare(graph, &base_style, convergent_limit.min(canvas.width));
    }

    match direction {
        Direction::TD | Direction::TB => {
            // Place label on vertical line from source, before merge point
            let src_x = center_x(from);
            let target_x = center_x(to);
            let stem_start_y = from.bottom_y();
            // Place label just below the source box on the vertical stem
            let mut label_y = stem_start_y + 1;

            // Move the label away from the shared merge corridor when the source
            // approaches the target from the left or right.
            let mut label_start_x =
                convergent_vertical_label_start(src_x, target_x, label_width, canvas.width);
            let mut horizontal_annotation = false;
            // When another line already runs there (a branch to another target,
            // gen_a_045), the label goes where the source's stem turns toward
            // its target, on a plain stretch of that run, as in BT.
            let turn_y = if cells_held_by_another_line(
                canvas,
                label_start_x,
                label_width,
                label_y,
                &owner_id,
            ) {
                prove_convergent_path(
                    canvas,
                    graph,
                    from,
                    to,
                    direction,
                    &base_style,
                    &owner_id,
                    &working_context,
                )
                .filter(|proof| proof.source_x != proof.target_x)
                .map(|proof| proof.turn_y)
                .filter(|&turn_y| turn_y > label_y)
            } else {
                None
            };
            if let Some(turn_y) = turn_y {
                let (run_start, run_end) = if src_x <= target_x {
                    (src_x + 1, target_x)
                } else {
                    (target_x + 1, src_x)
                };
                let preferred = run_start + run_end.saturating_sub(run_start + label_width) / 2;
                if let Some(start) = nearest_routed_label_slot(
                    canvas,
                    run_start,
                    run_end,
                    preferred,
                    label_width,
                    turn_y,
                    style,
                ) {
                    label_y = turn_y;
                    label_start_x = start;
                    horizontal_annotation = true;
                } else if let Some(y) = (label_y + 1..turn_y).find(|&y| {
                    // A run too short for the label (gen_a_011): beside the
                    // stem on a free row above the turn.
                    canvas::is_vertical(canvas.get(src_x, y), style)
                        && (label_start_x..label_start_x + label_width)
                            .all(|x| canvas.get(x, y) == ' ')
                }) {
                    label_y = y;
                }
            }
            // A label over a frame's wall breaks the frame (gen_a_165's "ILM
            // rollover" on Data Nodes' left wall). It goes beside the stem on the
            // other side, or further down the stem, clear of frames, boxes and
            // other lines.
            if overlaps_reserved_subgraph_cells(graph, label_start_x, label_y, label_width) {
                let nodes: Vec<&Node> = graph.nodes.iter().collect();
                let sides = [
                    src_x.saturating_add(2),
                    src_x.saturating_sub(label_width.saturating_add(1)),
                ];
                // The row just below the source box first after its own, then on
                // down the stem.
                let clear_slot = std::iter::once(label_y)
                    .chain(
                        (stem_start_y..canvas.height)
                            .filter(|&y| y != label_y)
                            .take_while(|&y| canvas::is_vertical(canvas.get(src_x, y), style)),
                    )
                    .find_map(|y| {
                        sides.into_iter().find_map(|start| {
                            let clear = start + label_width <= canvas.width
                                && !overlaps_reserved_subgraph_cells(graph, start, y, label_width)
                                && !overlaps_node(&nodes, start, y, label_width)
                                && label_neighbour_rows_clear(canvas, start, label_width, y)
                                && (start..start + label_width).all(|x| canvas.get(x, y) == ' ');
                            clear.then_some((y, start))
                        })
                    });
                if let Some((y, start)) = clear_slot {
                    label_y = y;
                    label_start_x = start;
                    horizontal_annotation = false;
                }
            }
            if let Some(published) = publish_vertical_convergent_label(
                canvas,
                graph,
                from,
                to,
                direction,
                label_start_x,
                label_y,
                &display_label,
                style,
                &owner_id,
                horizontal_annotation,
                &base_style,
                &working_context,
                config.spacing.max_canvas_width,
            ) {
                cells = published.cells.clone();
                publication = Some(published);
            }
        }
        Direction::BT => {
            let src_x = center_x(from);
            let target_x = center_x(to);
            let stem_start_y = from.y.saturating_sub(1);
            let mut label_y = stem_start_y.saturating_sub(1);

            let mut label_start_x =
                convergent_vertical_label_start(src_x, target_x, label_width, canvas.width);
            let mut horizontal_annotation = false;
            // The label belongs where the source's stem turns toward the target.
            // When that turn lies higher up than the row just above the source
            // (the merge bus sits near the target), put the label on a plain
            // stretch of that run instead of in empty space beside the stem.
            let mut turn_y = stem_start_y;
            while turn_y > 0 && canvas::is_vertical(canvas.get(src_x, turn_y), style) {
                turn_y -= 1;
            }
            // Past tees where other branches leave the stem (gen_a_073), up to
            // the run that reaches the target.
            if let Some(proof) = prove_convergent_path(
                canvas,
                graph,
                from,
                to,
                direction,
                &base_style,
                &owner_id,
                &working_context,
            ) {
                if proof.source_x != proof.target_x {
                    turn_y = proof.turn_y;
                }
            }
            // A real turn: a corner or tee leading sideways, below the target
            // (not its arrowhead or border, where a straight stem ends).
            let turn_cell = canvas.get(src_x, turn_y);
            let turns = turn_y > to.bottom_y()
                && !canvas::is_arrow(turn_cell)
                && (super::topology::char_connects_left(turn_cell)
                    || super::topology::char_connects_right(turn_cell));
            if turns && turn_y + 1 < label_y {
                let (run_start, run_end) = if src_x <= target_x {
                    (src_x + 1, target_x)
                } else {
                    (target_x + 1, src_x)
                };
                let preferred = run_start + run_end.saturating_sub(run_start + label_width) / 2;
                if let Some(start) = nearest_routed_label_slot(
                    canvas,
                    run_start,
                    run_end,
                    preferred,
                    label_width,
                    turn_y,
                    style,
                ) {
                    label_y = turn_y;
                    label_start_x = start;
                    horizontal_annotation = true;
                } else if cells_held_by_another_line(
                    canvas,
                    label_start_x,
                    label_width,
                    label_y,
                    &owner_id,
                ) {
                    // The run is too short for the label and another branch
                    // crosses the row above the source (gen_a_043): beside the
                    // stem on a free row below the turn.
                    if let Some(y) = (turn_y + 1..label_y).rev().find(|&y| {
                        canvas::is_vertical(canvas.get(src_x, y), style)
                            && (label_start_x..label_start_x + label_width)
                                .all(|x| canvas.get(x, y) == ' ')
                    }) {
                        label_y = y;
                    }
                }
            }
            if let Some(published) = publish_vertical_convergent_label(
                canvas,
                graph,
                from,
                to,
                direction,
                label_start_x,
                label_y,
                &display_label,
                style,
                &owner_id,
                horizontal_annotation,
                &base_style,
                &working_context,
                config.spacing.max_canvas_width,
            ) {
                cells = published.cells.clone();
                publication = Some(published);
            }
        }
        Direction::LR => {
            // Place label on horizontal line from source, before merge
            let src_y = center_y(from);
            let stem_start_x = from.x + from.width;
            // Place label above the edge line
            let label_y = src_y.saturating_sub(1);
            let gap_start = stem_start_x.saturating_add(1);
            let gap_end = to.x.saturating_sub(1);
            let preferred =
                convergent_horizontal_label_start(gap_start, gap_end, label_width, canvas.width);
            let label_start = nearest_routed_label_slot(
                canvas,
                gap_start,
                gap_end,
                preferred,
                label_width,
                label_y,
                style,
            );
            if let Some(label_start) = label_start {
                let mut x_pos = label_start;
                for c in display_label.chars() {
                    if routed_label_cell_is_writable(canvas, x_pos, label_y, style) {
                        set_label_char(canvas, &mut cells, x_pos, label_y, c);
                    }
                    x_pos += display_char_width(c);
                }
            }
        }
        Direction::RL => {
            let src_y = center_y(from);
            let label_y = src_y.saturating_sub(1);
            let gap_start = to.x.saturating_add(to.width).saturating_add(1);
            let gap_end = from.x.saturating_sub(1);
            let preferred =
                convergent_horizontal_label_start(gap_start, gap_end, label_width, canvas.width);
            let label_start = nearest_routed_label_slot(
                canvas,
                gap_start,
                gap_end,
                preferred,
                label_width,
                label_y,
                style,
            );
            if let Some(label_start) = label_start {
                let mut x_pos = label_start;
                for c in display_label.chars() {
                    if routed_label_cell_is_writable(canvas, x_pos, label_y, style) {
                        set_label_char(canvas, &mut cells, x_pos, label_y, c);
                    }
                    x_pos += display_char_width(c);
                }
            }
        }
    }

    let placement = build_label_placement(owner_id.clone(), cells)?;
    if let Some(publication) = publication {
        working_context.record_publication(
            canvas,
            from,
            to,
            direction,
            owner_id,
            display_label,
            publication,
        );
        *context = working_context;
    }
    Some(placement)
}

/// Prefer the horizontal turn corridor between a convergent source and target.
/// If that corridor is too narrow for the label, place the label on the source
/// side so it remains local to the branch instead of floating at a frame edge.
fn convergent_vertical_label_start(
    source_x: usize,
    target_x: usize,
    label_width: usize,
    canvas_width: usize,
) -> usize {
    let (left, right) = if source_x <= target_x {
        (source_x.saturating_add(2), target_x.saturating_sub(1))
    } else {
        (target_x.saturating_add(1), source_x.saturating_sub(2))
    };
    if right >= left {
        let span = right.saturating_sub(left).saturating_add(1);
        if span >= label_width {
            return left + (span - label_width) / 2;
        }
    }

    let outer = if source_x <= target_x {
        source_x.saturating_sub(label_width.saturating_add(2))
    } else {
        source_x.saturating_add(2)
    };
    outer.min(canvas_width.saturating_sub(label_width))
}

/// Center a convergent horizontal label in the open gap between the source
/// and target boxes. Tight branches use the nearest bounded source-local slot.
fn convergent_horizontal_label_start(
    gap_start: usize,
    gap_end_exclusive: usize,
    label_width: usize,
    canvas_width: usize,
) -> usize {
    if gap_end_exclusive > gap_start {
        let span = gap_end_exclusive - gap_start;
        if span >= label_width {
            return gap_start + (span - label_width) / 2;
        }
    }
    gap_start.min(canvas_width.saturating_sub(label_width))
}

/// Write and record one label glyph. The extra cells of a wide glyph become
/// recorded continuations, so a line drawn earlier cannot show through
/// between glyphs and routes read past them as label text.
fn set_label_char(
    canvas: &mut Canvas,
    cells: &mut Vec<(usize, usize)>,
    x: usize,
    y: usize,
    c: char,
) {
    canvas.set(x, y, c);
    record_label_cell(cells, x, y);
    for offset in 1..display_char_width(c) {
        if x + offset < canvas.width {
            canvas.set(x + offset, y, '\0');
            record_label_cell(cells, x + offset, y);
        }
    }
}

fn record_label_cell(cells: &mut Vec<(usize, usize)>, x: usize, y: usize) {
    cells.push((x, y));
}

fn build_label_placement(
    owner_id: String,
    cells: Vec<(usize, usize)>,
) -> Option<EdgeLabelPlacement> {
    if cells.is_empty() {
        None
    } else {
        Some(EdgeLabelPlacement { owner_id, cells })
    }
}

/// Style for shaft cells drawn around an edge label: the edge's own kind
/// (thick/dotted) for forward edges; back edges keep the cycle styling.
fn edge_label_style(style: &StyleChars, edge: &crate::graph::Edge) -> StyleChars {
    if edge.is_back_edge {
        *style
    } else {
        super::edge::style_for_edge_kind(style, edge.kind)
    }
}

/// Write one horizontal shaft cell next to a label. Free or shaft cells take
/// the edge-kind glyph; any other cell (another route's vertical or junction)
/// gets the base glyph so the canvas merge still composes a crossing.
fn set_label_shaft(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    kind_style: &StyleChars,
    base_style: &StyleChars,
) {
    let existing = canvas.get(x, y);
    if existing == ' ' || canvas::is_horizontal(existing, base_style) {
        canvas.set(x, y, kind_style.edge_h);
    } else {
        canvas.set(x, y, base_style.edge_h);
    }
}

/// The far end of the final straight run that reaches a horizontal arrow.
///
/// Scans the label row from the arrow back toward the source (`cells` in
/// that order) and returns the first cell where the route turns or branches,
/// so a fan-out branch label lands on its own branch instead of before the
/// shared bus.
fn final_run_turn(
    canvas: &Canvas,
    graph: &Graph,
    y: usize,
    cells: impl Iterator<Item = usize>,
    style: &StyleChars,
) -> Option<usize> {
    // A subgraph side wall crossed by the run is a portal, not a turn.
    let on_side_wall = |x: usize| {
        graph.subgraphs.iter().any(|subgraph| {
            let bounds = &subgraph.bounds;
            bounds.is_valid()
                && (x == bounds.x || x + 1 == bounds.x + bounds.width)
                && y > bounds.y
                && y + 1 < bounds.y + bounds.height
        })
    };
    for x in cells {
        let ch = canvas.get(x, y);
        if ch == ' ' || canvas::is_horizontal(ch, style) || canvas::is_arrow(ch) || on_side_wall(x)
        {
            continue;
        }
        // Any turn, tee, or four-arm glyph ends the run: on a fan-out bus a
        // four-arm cell is the branch junction itself.
        return Some(x);
    }
    None
}

/// Whether `node` has more than one forward outgoing edge (a fan-out bus).
fn source_fans_out(graph: &Graph, node: &Node) -> bool {
    graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge && edge.from == node.id)
        .count()
        > 1
}

/// Nudge an outside-row label so neither neighbour cell is a route glyph: a
/// label glued to a vertical bus makes the topology stabilizer grow a tee arm
/// toward the text (`├no`). A nudged slot stays off the edge's own boxes,
/// which are drawn later over any text there. Returns the original slot as
/// an error when no nearby slot fits.
fn keep_label_off_routes(
    canvas: &Canvas,
    start_x: usize,
    y: usize,
    width: usize,
    style: &StyleChars,
    nodes: &[&Node],
) -> Result<usize, usize> {
    let is_route = |x: usize| {
        let ch = canvas.get(x, y);
        ch != ' '
            && !is_textual(ch)
            && (canvas::is_vertical(ch, style) || canvas::is_junction(ch, style))
    };
    let fits = |start: usize| {
        start + width < canvas.width
            && (start..start + width).all(|x| canvas.get(x, y) == ' ')
            && (start == 0 || !is_route(start - 1))
            && !is_route(start + width)
            && !overlaps_node(nodes, start, y, width)
    };
    if fits(start_x) || !(start_x..start_x + width).all(|x| canvas.get(x, y) == ' ') {
        return Ok(start_x);
    }
    for delta in 1..=2 {
        if fits(start_x + delta) {
            return Ok(start_x + delta);
        }
        if start_x >= delta && fits(start_x - delta) {
            return Ok(start_x - delta);
        }
    }
    Err(start_x)
}

/// Draw a back edge's label on the longest straight horizontal run of its
/// drawn cycle route (in TD/BT the leg out to the right gutter, in LR/RL the
/// bottom gutter run), padded `─ label ─`. A run too short for that keeps the
/// label on a free row just below it.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_cycle_edge_label(
    canvas: &mut Canvas,
    from: &Node,
    graph: &Graph,
    label: &str,
    direction: Direction,
    style: &StyleChars,
    config: &Config,
    edge_idx: usize,
    edge: &crate::graph::Edge,
) -> Option<EdgeLabelPlacement> {
    let owner_id = edge_owner_id(edge_idx, edge);
    let display_label =
        format_edge_label_with_limit(label, config.max_edge_label_width.min(canvas.width));
    let label_width = display_width(&display_label);
    // Boxes are drawn after routes and labels, so a cell under a box is lost.
    let under_box = |x: usize, y: usize| {
        graph.nodes.iter().any(|node| {
            x >= node.x && x < node.x + node.width && y >= node.y && y < node.y + node.height
        })
    };
    let is_run_cell = |canvas: &Canvas, x: usize, y: usize| {
        let ch = canvas.get(x, y);
        (ch == style.back_h || canvas::is_horizontal(ch, style))
            && !under_box(x, y)
            && canvas
                .get_meta(x, y)
                .is_some_and(|meta| meta.owner_id.as_deref() == Some(owner_id.as_str()))
    };

    // Longest runs of plain horizontal cells owned by this edge (in TD/BT a tie
    // goes to the source leg): one the route has to itself, and any run.
    let source_row = super::cycle::center_y(from);
    let prefers_source = !matches!(direction, Direction::LR | Direction::RL);
    let rank = |(left, right, row): (usize, usize, usize)| {
        (right - left, prefers_source && row == source_row)
    };
    let mut best_own: Option<(usize, usize, usize)> = None;
    let mut best_any: Option<(usize, usize, usize)> = None;
    for y in 0..canvas.height {
        let mut x = 0;
        while x < canvas.width {
            if !is_run_cell(canvas, x, y) {
                x += 1;
                continue;
            }
            let start = x;
            while x < canvas.width && is_run_cell(canvas, x, y) {
                x += 1;
            }
            let run = (start, x - 1, y);
            if !run_on_frame_border(graph, run)
                && best_any.is_none_or(|current| rank(run) > rank(current))
            {
                best_any = Some(run);
            }
            if cycle_label_run_is_own(canvas, graph, run)
                && best_own.is_none_or(|current| rank(run) > rank(current))
            {
                best_own = Some(run);
            }
        }
    }
    let fits = |(left, right, _): (usize, usize, usize)| right - left + 1 >= label_width + 4;
    let mut cells = Vec::new();

    // Inline on the route's own run; else on a free row beside it; else, as a
    // last resort, inline on a run it shares with another line.
    if let Some(run) = best_own.filter(|&run| fits(run)) {
        write_inline_cycle_label(canvas, &mut cells, run, &display_label, label_width);
    } else if let Some((left, right, row)) = best_own {
        let centre = (left + right) / 2;
        let start = centre
            .saturating_sub(label_width / 2)
            .min(canvas.width.saturating_sub(label_width));
        // A blank on each side keeps the label clear of nearby lines.
        let free = |canvas: &Canvas, y: usize| {
            (start.saturating_sub(1)..start + label_width + 1).all(|x| canvas.get(x, y) == ' ')
        };
        if let Some(label_row) = [row + 1, row.saturating_sub(1)]
            .into_iter()
            .find(|&y| y != row && y < canvas.height && free(canvas, y))
        {
            let mut x_pos = start;
            for c in display_label.chars() {
                set_label_char(canvas, &mut cells, x_pos, label_row, c);
                x_pos += display_char_width(c);
            }
        }
    }
    if cells.is_empty() {
        if let Some(run) = best_any.filter(|&run| fits(run)) {
            write_inline_cycle_label(canvas, &mut cells, run, &display_label, label_width);
        }
    }

    build_label_placement(owner_id, cells)
}

/// Write `─ label ─` centred on a horizontal run that holds it with a shaft
/// cell and a blank on each side.
fn write_inline_cycle_label(
    canvas: &mut Canvas,
    cells: &mut Vec<(usize, usize)>,
    (left, right, row): (usize, usize, usize),
    display_label: &str,
    label_width: usize,
) {
    let run_width = right - left + 1;
    let blank = left + 1 + (run_width - (label_width + 4)) / 2;
    canvas.set(blank, row, ' ');
    let mut x_pos = blank + 1;
    for c in display_label.chars() {
        set_label_char(canvas, cells, x_pos, row, c);
        x_pos += display_char_width(c);
    }
    canvas.set(x_pos, row, ' ');
}

/// Whether a horizontal run of a cycle route belongs to that route alone: it
/// does not lie on a subgraph border row, and neither end meets a junction
/// (a tee) where it would merge with another edge's line. Box sides and
/// arrowheads at an end are the route's own endpoints; a crossing, a frame
/// wall or a spine shared by back edges is not another edge.
fn cycle_label_run_is_own(
    canvas: &Canvas,
    graph: &Graph,
    (left, right, row): (usize, usize, usize),
) -> bool {
    use super::topology::{
        char_connects_down, char_connects_left, char_connects_right, char_connects_up,
    };

    if run_on_frame_border(graph, (left, right, row)) {
        return false;
    }
    let arms = |x: usize, y: usize| {
        let mut count = 0;
        if y > 0 && char_connects_down(canvas.get(x, y - 1)) {
            count += 1;
        }
        if char_connects_up(canvas.get(x, y + 1)) {
            count += 1;
        }
        if x > 0 && char_connects_right(canvas.get(x - 1, y)) {
            count += 1;
        }
        if char_connects_left(canvas.get(x + 1, y)) {
            count += 1;
        }
        count
    };
    let on_side_wall = |x: usize| {
        graph.subgraphs.iter().any(|subgraph| {
            let bounds = &subgraph.bounds;
            bounds.is_valid()
                && (x == bounds.x || x + 1 == bounds.x + bounds.width)
                && row > bounds.y
                && row + 1 < bounds.y + bounds.height
        })
    };
    let merges = |x: usize| {
        if on_side_wall(x) {
            return false;
        }
        let ch = canvas.get(x, row);
        let endpoint = canvas::is_arrow(ch)
            || canvas.get_meta(x, row).is_some_and(|meta| {
                matches!(
                    meta.owner_kind,
                    CellOwnerKind::NodeBorder | CellOwnerKind::ArrowHead
                )
            });
        // A tee joins another line; a crossing lets the run pass, and a tee on a
        // column drawn by back edges is their shared spine.
        let cycle_cell = |y: usize| {
            canvas
                .get_meta(x, y)
                .is_some_and(|meta| meta.owner_kind == CellOwnerKind::CycleEdge)
        };
        let spine = (row > 0 && cycle_cell(row - 1)) || cycle_cell(row + 1);
        !endpoint && !spine && arms(x, row) == 3
    };
    !(left > 0 && merges(left - 1) || merges(right + 1))
}

/// Whether a horizontal run lies on a subgraph's top or bottom border row,
/// where the border is redrawn over anything written into it.
fn run_on_frame_border(graph: &Graph, (left, right, row): (usize, usize, usize)) -> bool {
    graph.subgraphs.iter().any(|subgraph| {
        let bounds = &subgraph.bounds;
        bounds.is_valid()
            && (row == bounds.y || row + 1 == bounds.y + bounds.height)
            && left < bounds.x + bounds.width
            && right >= bounds.x
    })
}

/// The stretch of a horizontal label run `[start, end)` on row `y` to write a
/// label in. When a label centred on the run would cover a subgraph side wall,
/// the longest stretch between walls that holds the padded label, so the
/// label never overwrites a wall; otherwise the whole run.
fn wall_free_run(
    graph: &Graph,
    y: usize,
    start: usize,
    end: usize,
    label_width: usize,
) -> (usize, usize) {
    let mut walls: Vec<usize> = graph
        .subgraphs
        .iter()
        .filter(|subgraph| {
            let bounds = &subgraph.bounds;
            bounds.is_valid() && y > bounds.y && y + 1 < bounds.y + bounds.height
        })
        .flat_map(|subgraph| {
            let bounds = &subgraph.bounds;
            [bounds.x, bounds.x + bounds.width - 1]
        })
        .filter(|&x| x >= start && x < end)
        .collect();
    if walls.is_empty() {
        return (start, end);
    }
    // Only narrow when a label centred on the whole run would cover a wall.
    let centred = start + end.saturating_sub(start).saturating_sub(label_width) / 2;
    if !walls
        .iter()
        .any(|&wall| wall + 1 >= centred && wall <= centred + label_width)
    {
        return (start, end);
    }
    walls.sort_unstable();
    walls.dedup();
    let mut stretches = Vec::new();
    let mut from = start;
    for wall in walls {
        stretches.push((from, wall));
        from = wall + 1;
    }
    stretches.push((from, end));
    stretches
        .into_iter()
        .filter(|&(left, right)| right.saturating_sub(left) >= label_width + 4)
        .max_by_key(|&(left, right)| (right - left, std::cmp::Reverse(left)))
        .unwrap_or((start, end))
}

/// A detoured edge's label goes on its lane. A horizontal lane takes it as
/// any routed edge's run does. Beside a vertical lane it goes level with the
/// lane's middle, on the side away from the boxes first, wherever it covers
/// no line, text or frame wall; failing that, as on any routed run.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_detour_edge_label(
    canvas: &mut Canvas,
    lane: &EdgeRoute,
    label: &str,
    style: &StyleChars,
    graph: &Graph,
    config: &Config,
    edge_idx: usize,
    edge: &crate::graph::Edge,
) -> Option<EdgeLabelPlacement> {
    let segment = lane.segments.first()?;
    if segment.from.x != segment.to.x {
        return draw_routed_edge_label(canvas, lane, label, style, graph, config, edge_idx, edge);
    }
    let display_label = format_edge_label_with_limit(label, config.max_edge_label_width);
    let width = display_width(&display_label);
    let x = segment.from.x;
    let (top, bottom) = (
        segment.from.y.min(segment.to.y),
        segment.from.y.max(segment.to.y),
    );
    let outer_right = graph.nodes.iter().all(|node| node.x + node.width <= x);
    let right = x + 2;
    let left = x.checked_sub(width + 1);
    let sides: Vec<usize> = if outer_right {
        [Some(right), left].into_iter().flatten().collect()
    } else {
        [left, Some(right)].into_iter().flatten().collect()
    };
    let middle = (top + bottom) / 2;
    let rows = (0..=bottom - top).flat_map(|offset| {
        [middle.checked_sub(offset), Some(middle + offset)]
            .into_iter()
            .flatten()
            .filter(|&y| y > top && y < bottom)
    });
    let mut slot = None;
    'search: for y in rows {
        for &start in &sides {
            let end = start + width;
            if end > config.spacing.max_canvas_width {
                continue;
            }
            let free = (start..end).all(|cx| cx >= canvas.width || canvas.get(cx, y) == ' ')
                && !overlaps_reserved_subgraph_cells(graph, start, y, width)
                && label_neighbour_rows_clear(canvas, start, width, y);
            if free {
                slot = Some((start, y));
                break 'search;
            }
        }
    }
    let Some((start, y)) = slot else {
        return draw_routed_edge_label(canvas, lane, label, style, graph, config, edge_idx, edge);
    };
    canvas.ensure_width(start + width);
    let mut cells = Vec::new();
    let mut cx = start;
    for c in display_label.chars() {
        set_label_char(canvas, &mut cells, cx, y, c);
        cx += display_char_width(c).max(1);
    }
    build_label_placement(edge_owner_id(edge_idx, edge), cells)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, EdgeKind, Rectangle, Subgraph};
    use crate::render::semantic::{CellRole, SemanticFrame};
    use crate::style::BaseStyle;

    const LABEL: &str = "1234567";

    // Include public text output: raw chars alone cannot observe combining loss.
    #[derive(Debug, PartialEq, Eq)]
    struct Snapshot {
        output: String,
        frame: SemanticFrame,
        stages: Vec<String>,
        explicit_edges: Vec<(usize, usize, crate::render::semantic::CellMeta)>,
    }

    fn snapshot(canvas: &Canvas) -> Snapshot {
        let stages = (0..canvas.height)
            .flat_map(|y| {
                (0..canvas.width).map(move |x| {
                    canvas
                        .write_stage_at(x, y)
                        .unwrap_or("<missing>")
                        .to_owned()
                })
            })
            .collect();
        Snapshot {
            output: format!("{canvas}"),
            frame: SemanticFrame::from_canvas(canvas),
            stages,
            explicit_edges: canvas.explicit_edge_meta(),
        }
    }

    fn graph(direction: Direction) -> (Graph, String) {
        graph_with_back_edge(direction, false)
    }

    fn graph_with_back_edge(direction: Direction, is_back_edge: bool) -> (Graph, String) {
        let mut graph = Graph::new();
        graph.direction = direction;
        let mut edge = Edge::with_label("Origin", "Finish", LABEL);
        edge.is_back_edge = is_back_edge;
        let owner = edge_owner_id(0, &edge);
        graph.add_edge(edge);
        (graph, owner)
    }

    fn node(id: &str, x: usize, y: usize, width: usize) -> Node {
        let mut node = Node::new(id, id);
        node.x = x;
        node.y = y;
        node.width = width;
        node.height = 3;
        node
    }

    fn route_cell(
        canvas: &mut Canvas,
        x: usize,
        y: usize,
        ch: char,
        owner: &str,
        kind: CellOwnerKind,
        role: CellRole,
    ) {
        canvas.set_owned_with_role(x, y, ch, kind, owner, role, 4);
    }

    fn horizontal(
        canvas: &mut Canvas,
        start: usize,
        end: usize,
        y: usize,
        style: &StyleChars,
        owner: &str,
        kind: CellOwnerKind,
    ) {
        for x in start..end {
            route_cell(
                canvas,
                x,
                y,
                style.edge_h,
                owner,
                kind,
                CellRole::Horizontal,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn rejected(
        canvas: &mut Canvas,
        graph: &Graph,
        start: usize,
        y: usize,
        label: &str,
        style: &StyleChars,
        owner: &str,
        annotation: bool,
        witness: &str,
    ) {
        let before = snapshot(canvas);
        assert!(
            !convergent_label_slot_is_writable(
                canvas, graph, start, y, label, style, owner, annotation
            ),
            "guard unexpectedly accepted {witness}"
        );
        assert_eq!(
            try_publish_convergent_label(canvas, graph, start, y, label, style, owner, annotation),
            None,
            "publisher unexpectedly accepted {witness}"
        );
        assert_eq!(
            snapshot(canvas),
            before,
            "failed transaction mutated {witness}"
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn published(
        canvas: &mut Canvas,
        graph: &Graph,
        start: usize,
        y: usize,
        label: &str,
        style: &StyleChars,
        owner: &str,
        annotation: bool,
        width: usize,
    ) -> Vec<(usize, usize)> {
        assert!(convergent_label_slot_is_writable(
            canvas, graph, start, y, label, style, owner, annotation
        ));
        let cells =
            try_publish_convergent_label(canvas, graph, start, y, label, style, owner, annotation)
                .expect("complete safe slot must remain feasible");
        let mut covered = cells.clone();
        covered.sort_unstable();
        covered.dedup();
        assert_eq!(
            covered,
            (start..start + width).map(|x| (x, y)).collect::<Vec<_>>()
        );
        assert!(
            format!("{canvas}").lines().nth(y).unwrap().contains(label),
            "complete supported label missing: {label:?}"
        );
        cells
    }

    fn styles() -> impl Iterator<Item = StyleChars> {
        [BaseStyle::Ascii, BaseStyle::Unicode]
            .into_iter()
            .flat_map(|base| {
                [EdgeKind::Arrow, EdgeKind::Thick, EdgeKind::Dotted]
                    .into_iter()
                    .map(move |kind| super::super::edge::style_for_edge_kind(base.chars(), kind))
            })
    }

    #[test]
    fn label15_transaction_protects_owned_route_cells() {
        for direction in [Direction::TD, Direction::BT] {
            for style in styles() {
                let (graph, owner) = graph(direction);
                for (glyph, kind, role) in [
                    (style.edge_v, CellOwnerKind::EdgeSegment, CellRole::Vertical),
                    (
                        style.corner_ul,
                        CellOwnerKind::EdgeSegment,
                        CellRole::Corner,
                    ),
                    (
                        style.junction_down,
                        CellOwnerKind::Junction,
                        CellRole::Junction,
                    ),
                    (style.cross, CellOwnerKind::Junction, CellRole::Junction),
                    (
                        style.arrow_down,
                        CellOwnerKind::ArrowHead,
                        CellRole::ArrowTip,
                    ),
                    (
                        style.circle_end,
                        CellOwnerKind::EdgeSegment,
                        CellRole::EndpointMarker,
                    ),
                    (
                        style.cross_end,
                        CellOwnerKind::EdgeSegment,
                        CellRole::EndpointMarker,
                    ),
                ] {
                    let mut canvas = Canvas::new(30, 9);
                    route_cell(&mut canvas, 6, 4, glyph, &owner, kind, role);
                    rejected(
                        &mut canvas,
                        &graph,
                        3,
                        4,
                        LABEL,
                        &style,
                        &owner,
                        true,
                        "owned protected route",
                    );
                    assert_eq!(canvas.get(6, 4), glyph);
                    published(&mut canvas, &graph, 17, 4, LABEL, &style, &owner, false, 7);
                    assert_eq!(canvas.get(6, 4), glyph);
                }
            }
        }
    }

    #[test]
    fn label15_horizontal_annotation_requires_actual_owner() {
        let style = BaseStyle::Unicode.chars();
        let (graph, owner) = graph(Direction::TD);
        for is_back_edge in [false, true] {
            let (graph, owner) = graph_with_back_edge(Direction::TD, is_back_edge);
            let kind = if is_back_edge {
                CellOwnerKind::CycleEdge
            } else {
                CellOwnerKind::EdgeSegment
            };
            let mut own = Canvas::new(24, 7);
            horizontal(&mut own, 4, 11, 3, style, &owner, kind);
            let mut not_proven = own.clone();
            rejected(
                &mut not_proven,
                &graph,
                4,
                3,
                LABEL,
                style,
                &owner,
                false,
                "unproven annotation",
            );
            published(&mut own, &graph, 4, 3, LABEL, style, &owner, true, 7);
            let wrong_kind = if is_back_edge {
                CellOwnerKind::EdgeSegment
            } else {
                CellOwnerKind::CycleEdge
            };
            let mut mismatched = Canvas::new(24, 7);
            horizontal(&mut mismatched, 4, 11, 3, style, &owner, wrong_kind);
            rejected(
                &mut mismatched,
                &graph,
                4,
                3,
                LABEL,
                style,
                &owner,
                true,
                "horizontal metadata kind disagrees with canonical edge backness",
            );
        }
        for (id, kind, role) in [
            (
                Some("edge:1:Peer->Finish"),
                CellOwnerKind::EdgeSegment,
                CellRole::Horizontal,
            ),
            (None, CellOwnerKind::EdgeSegment, CellRole::Horizontal),
            (
                Some("not-the-canonical-owner"),
                CellOwnerKind::CycleEdge,
                CellRole::Horizontal,
            ),
            (
                Some(owner.as_str()),
                CellOwnerKind::EdgeSegment,
                CellRole::Text,
            ),
            (
                Some(owner.as_str()),
                CellOwnerKind::Unknown,
                CellRole::Horizontal,
            ),
        ] {
            let mut canvas = Canvas::new(24, 7);
            canvas.set(7, 3, style.edge_h);
            canvas.set_meta_only(7, 3, kind, id, role, 4);
            rejected(
                &mut canvas,
                &graph,
                4,
                3,
                LABEL,
                style,
                &owner,
                true,
                "foreign/missing/wrong semantic owner",
            );
        }
        // Insert this block immediately before the existing final brace of
        // label15_horizontal_annotation_requires_actual_owner; retain all old assertions.
        {
            for direction in [Direction::TD, Direction::TB, Direction::BT] {
                for base in [BaseStyle::Ascii, BaseStyle::Unicode] {
                    let mut f = h2_typed_source_crossing_fixture(direction, base, false, '┼');
                    let collector = format!("fanin:{}", f.to.id);
                    let row = f.point(7, 7).1;
                    assert_eq!(
                        f.graph
                            .edges
                            .iter()
                            .filter(|e| !e.is_back_edge && e.to == f.to.id)
                            .count(),
                        2
                    );
                    for x in 9..18 {
                        let meta = f.canvas.get_meta(x, row).unwrap();
                        assert_eq!(f.canvas.get(x, row), f.style.edge_h);
                        assert_eq!(meta.owner_kind, CellOwnerKind::EdgeSegment);
                        assert_eq!(meta.role, CellRole::Horizontal);
                        assert_eq!(meta.owner_id.as_deref(), Some(collector.as_str()));
                    }
                    h2_c_collector_commit(
                        &mut f,
                        10,
                        row,
                        LABEL,
                        true,
                        None,
                        true,
                        true,
                        "forward exact-target actual collector",
                    )
                    .unwrap();

                    let mut mixed = h2_typed_source_crossing_fixture(direction, base, false, '┼');
                    let owner = mixed.owner.clone();
                    for x in [10, 12, 14, 16] {
                        let glyph = mixed.style.edge_h;
                        route_cell(
                            &mut mixed.canvas,
                            x,
                            row,
                            glyph,
                            &owner,
                            CellOwnerKind::EdgeSegment,
                            CellRole::Horizontal,
                        );
                        assert_eq!(
                            mixed.canvas.get_meta(x, row).unwrap().owner_id.as_deref(),
                            Some(owner.as_str())
                        );
                    }
                    h2_c_collector_commit(
                        &mut mixed,
                        10,
                        row,
                        LABEL,
                        true,
                        None,
                        true,
                        true,
                        "mixed exact collector and canonical current edge",
                    )
                    .unwrap();
                }
            }

            for invalid in [
                "wrong-token",
                "missing-target",
                "stale-target",
                "one-incoming",
                "back-sibling",
                "invisible-sibling",
                "missing-sibling",
                "disconnected-source",
                "wrong-canonical-edge",
                "false-annotation",
            ] {
                let mut f =
                    h2_typed_source_crossing_fixture(Direction::TD, BaseStyle::Unicode, false, '┼');
                let mut owner_override = None;
                let mut annotation = true;
                let mut expected_proof = true;
                match invalid {
                    "wrong-token" => {
                        f.raw(12, 7, '─', "fanin:Other", CellRole::Horizontal);
                        assert_eq!(
                            f.canvas.get_meta(12, 7).unwrap().owner_id.as_deref(),
                            Some("fanin:Other")
                        );
                    }
                    "missing-target" => {
                        f.graph.nodes.retain(|n| n.id != f.to.id);
                        assert!(f.graph.get_node(&f.to.id).is_none());
                        expected_proof = false;
                    }
                    "stale-target" => {
                        f.to.x += 1;
                        assert_ne!(f.graph.get_node(&f.to.id).unwrap().x, f.to.x);
                        expected_proof = false;
                    }
                    "one-incoming" => {
                        f.graph.edges.remove(1);
                        assert_eq!(
                            f.graph
                                .edges
                                .iter()
                                .filter(|e| !e.is_back_edge && e.to == f.to.id)
                                .count(),
                            1
                        );
                    }
                    "back-sibling" => {
                        f.graph.edges[1].is_back_edge = true;
                        assert!(f.graph.edges[1].is_back_edge);
                    }
                    "invisible-sibling" => {
                        f.graph.get_node_mut("Peer").unwrap().x = f.canvas.width;
                        assert!(!convergent_node_in_bounds(
                            &f.canvas,
                            f.graph.get_node("Peer").unwrap()
                        ));
                    }
                    "missing-sibling" => {
                        f.graph.nodes.retain(|n| n.id != "Peer");
                        assert!(f.graph.get_node("Peer").is_none());
                    }
                    "disconnected-source" => {
                        f.canvas.set(7, 5, ' ');
                        assert_eq!(f.canvas.get(7, 5), ' ');
                        expected_proof = false;
                    }
                    "wrong-canonical-edge" => {
                        owner_override = Some(edge_owner_id(1, &f.graph.edges[1]));
                        assert_ne!(owner_override.as_deref(), Some(f.owner.as_str()));
                        expected_proof = false;
                    }
                    "false-annotation" => annotation = false,
                    _ => unreachable!(),
                }
                h2_c_collector_commit(
                    &mut f,
                    10,
                    7,
                    LABEL,
                    annotation,
                    owner_override.as_deref(),
                    expected_proof,
                    false,
                    invalid,
                );
            }

            // A current back edge is never granted forward collector permission, even
            // with a genuinely provable CycleEdge source entry and actual target head.
            let mut back =
                h2_typed_source_crossing_fixture(Direction::TD, BaseStyle::Unicode, false, '┼');
            back.graph.edges[0].is_back_edge = true;
            let owner = back.owner.clone();
            for y in 4..7 {
                route_cell(
                    &mut back.canvas,
                    7,
                    y,
                    '│',
                    &owner,
                    CellOwnerKind::CycleEdge,
                    CellRole::Vertical,
                );
            }
            h2_c_collector_commit(
                &mut back,
                10,
                7,
                LABEL,
                true,
                None,
                true,
                false,
                "current back edge cannot consume forward collector",
            );

            // This same-token horizontal island is wholly outside the actual reflected
            // source→target run. All raw cell/margin predicates pass; padded-run proof
            // is the refusal, rather than a corner, endpoint, missing label room or glyph.
            let mut outside =
                h2_typed_source_crossing_fixture(Direction::TD, BaseStyle::Unicode, true, '┼');
            let collector = format!("fanin:{}", outside.to.id);
            for x in 1..10 {
                route_cell(
                    &mut outside.canvas,
                    x,
                    7,
                    '─',
                    &collector,
                    CellOwnerKind::EdgeSegment,
                    CellRole::Horizontal,
                );
            }
            assert!(routed_label_slot_is_writable(
                &outside.canvas,
                2,
                7,
                7,
                &outside.style
            ));
            assert!(label_neighbour_rows_clear(&outside.canvas, 2, 7, 7));
            h2_c_collector_commit(
                &mut outside,
                2,
                7,
                LABEL,
                true,
                None,
                true,
                false,
                "outside actual proved padded run",
            );

            // Rejections for exact collector cells preserve all Canvas channels. The
            // path remains intact where the deliberate role/kind still describes a
            // traversable raw shaft/junction; heads and non-route kinds destroy proof.
            for (glyph, kind, role, expected_proof, witness) in [
                (
                    '─',
                    CellOwnerKind::CycleEdge,
                    CellRole::Horizontal,
                    true,
                    "collector wrong edge kind",
                ),
                (
                    '─',
                    CellOwnerKind::Unknown,
                    CellRole::Horizontal,
                    false,
                    "collector unknown kind",
                ),
                (
                    '─',
                    CellOwnerKind::EdgeLabel,
                    CellRole::Horizontal,
                    false,
                    "collector is caption owner",
                ),
                (
                    '─',
                    CellOwnerKind::NodeLabel,
                    CellRole::Horizontal,
                    false,
                    "collector is node text owner",
                ),
                (
                    '─',
                    CellOwnerKind::EdgeSegment,
                    CellRole::Text,
                    true,
                    "collector wrong Text role",
                ),
                (
                    '─',
                    CellOwnerKind::EdgeSegment,
                    CellRole::Vertical,
                    false,
                    "collector wrong vertical role",
                ),
                (
                    '+',
                    CellOwnerKind::EdgeSegment,
                    CellRole::Corner,
                    true,
                    "collector actual corner",
                ),
                (
                    '┼',
                    CellOwnerKind::Junction,
                    CellRole::Junction,
                    true,
                    "collector actual junction",
                ),
                (
                    '↓',
                    CellOwnerKind::ArrowHead,
                    CellRole::ArrowTip,
                    false,
                    "collector actual head",
                ),
                (
                    '○',
                    CellOwnerKind::EdgeSegment,
                    CellRole::EndpointMarker,
                    false,
                    "collector actual endpoint",
                ),
            ] {
                let mut f =
                    h2_typed_source_crossing_fixture(Direction::TD, BaseStyle::Unicode, false, '┼');
                let collector = format!("fanin:{}", f.to.id);
                route_cell(&mut f.canvas, 12, 7, glyph, &collector, kind, role);
                assert_eq!(f.canvas.get(12, 7), glyph);
                let meta = f.canvas.get_meta(12, 7).unwrap();
                assert_eq!(meta.owner_id.as_deref(), Some(collector.as_str()));
                assert_eq!(meta.owner_kind, kind);
                assert_eq!(meta.role, role);
                h2_c_collector_commit(
                    &mut f,
                    10,
                    7,
                    LABEL,
                    true,
                    None,
                    expected_proof,
                    false,
                    witness,
                );
            }
            for foreign in ["edge:99:Missing->Finish", "not-a-canonical-owner"] {
                let mut f =
                    h2_typed_source_crossing_fixture(Direction::TD, BaseStyle::Unicode, false, '┼');
                f.raw(12, 7, '─', foreign, CellRole::Horizontal);
                assert_eq!(
                    f.canvas.get_meta(12, 7).unwrap().owner_id.as_deref(),
                    Some(foreign)
                );
                h2_c_collector_commit(
                    &mut f,
                    10,
                    7,
                    LABEL,
                    true,
                    None,
                    true,
                    false,
                    "mixed collector and foreign horizontal",
                );
            }
            let mut peer =
                h2_typed_source_crossing_fixture(Direction::TD, BaseStyle::Unicode, false, '┼');
            let peer_owner = edge_owner_id(1, &peer.graph.edges[1]);
            peer.raw(12, 7, '─', &peer_owner, CellRole::Horizontal);
            h2_c_collector_commit(
                &mut peer,
                10,
                7,
                LABEL,
                true,
                None,
                true,
                false,
                "canonical sibling is still foreign to collector caption",
            );
        }
    }

    #[test]
    fn label15_rejects_blank_node_regions() {
        let style = BaseStyle::Unicode.chars();
        let (mut graph, owner) = graph(Direction::TD);
        graph.add_node(node("BlankNode", 5, 3, 6));
        for (x, y) in [(5, 3), (7, 4), (10, 5)] {
            let mut canvas = Canvas::new(30, 10);
            assert_eq!(canvas.get(x, y), ' ');
            rejected(
                &mut canvas,
                &graph,
                x,
                y,
                "a",
                style,
                &owner,
                false,
                "blank graph node cap/interior",
            );
            published(&mut canvas, &graph, 17, 7, "abc", style, &owner, false, 3);
        }
        let (empty_graph, owner) = self::graph(Direction::TD);
        let mut canvas = Canvas::new(24, 9);
        canvas.set_meta_only(
            8,
            4,
            CellOwnerKind::NodeFill,
            Some("unpainted"),
            CellRole::Fill,
            4,
        );
        rejected(
            &mut canvas,
            &empty_graph,
            6,
            4,
            "abcde",
            style,
            &owner,
            false,
            "blank typed NodeFill",
        );
    }

    #[test]
    fn label15_rejects_reserved_title_and_frame_regions() {
        let style = BaseStyle::Unicode.chars();
        for direction in [Direction::TD, Direction::BT] {
            let (mut graph, owner) = graph(direction);
            let mut frame = Subgraph::new("Frame", Some("Title".to_owned()));
            frame.bounds = Rectangle::new(5, 1, 18, 9);
            frame.inner_bounds = Rectangle::new(7, 3, 14, 5);
            graph.add_subgraph(frame);
            let title_y = if direction == Direction::BT { 8 } else { 2 };
            for (x, y) in [
                (7, title_y),
                (18, title_y),
                (10, 1),
                (10, 9),
                (5, 5),
                (22, 5),
                (4, 5),
                (23, 5),
            ] {
                let mut canvas = Canvas::new(40, 13);
                rejected(
                    &mut canvas,
                    &graph,
                    x,
                    y,
                    "a",
                    style,
                    &owner,
                    false,
                    "blank frame/title/gutter",
                );
                published(&mut canvas, &graph, 29, 6, "abc", style, &owner, false, 3);
            }
        }
        for (kind, role) in [
            (CellOwnerKind::SubgraphTitle, CellRole::Text),
            (CellOwnerKind::EdgeLabel, CellRole::Text),
        ] {
            let (graph, owner) = graph(Direction::TD);
            let mut canvas = Canvas::new(24, 8);
            canvas.set_meta_only(8, 4, kind, Some("foreign"), role, 4);
            rejected(
                &mut canvas,
                &graph,
                6,
                4,
                "abcde",
                style,
                &owner,
                false,
                "blank reserved text metadata",
            );
        }
    }

    #[test]
    fn label15_checks_complete_wide_footprint() {
        let style = BaseStyle::Unicode.chars();
        let (graph, owner) = graph(Direction::TD);
        let mut blocked = Canvas::new(16, 7);
        route_cell(
            &mut blocked,
            5,
            3,
            style.edge_v,
            "edge:9:X->Y",
            CellOwnerKind::EdgeSegment,
            CellRole::Vertical,
        );
        rejected(
            &mut blocked,
            &graph,
            4,
            3,
            "界a",
            style,
            &owner,
            false,
            "CJK continuation conflict",
        );
        let mut exact = Canvas::new(7, 5);
        published(&mut exact, &graph, 4, 2, "界a", style, &owner, false, 3);
        assert_eq!(exact.get(4, 2), '界');
        assert_eq!(exact.get(5, 2), '\0');
        assert_eq!(exact.get(6, 2), 'a');
        let mut narrow = Canvas::new(8, 5);
        rejected(
            &mut narrow,
            &graph,
            0,
            2,
            "界界界界a",
            style,
            &owner,
            false,
            "whole span wider than canvas",
        );
    }

    #[test]
    fn label15_combining_cursor_stays_with_owned_base() {
        let style = BaseStyle::Unicode.chars();
        let (graph, owner) = graph(Direction::TD);
        let mut plain = Canvas::new(14, 6);
        published(
            &mut plain,
            &graph,
            3,
            3,
            "e\u{301}abc",
            style,
            &owner,
            false,
            4,
        );
        assert!(format!("{plain}").contains("e\u{301}abc"));
        let mut occupied_cursor = Canvas::new(14, 6);
        horizontal(
            &mut occupied_cursor,
            3,
            7,
            3,
            style,
            &owner,
            CellOwnerKind::EdgeSegment,
        );
        published(
            &mut occupied_cursor,
            &graph,
            3,
            3,
            "e\u{301}abc",
            style,
            &owner,
            true,
            4,
        );
        assert!(format!("{occupied_cursor}").contains("e\u{301}abc"));
        let mut foreign_cursor = Canvas::new(14, 6);
        horizontal(
            &mut foreign_cursor,
            3,
            7,
            3,
            style,
            &owner,
            CellOwnerKind::EdgeSegment,
        );
        foreign_cursor.set_owned_with_role(
            4,
            3,
            'q',
            CellOwnerKind::EdgeLabel,
            "edge:9:X->Y",
            CellRole::Text,
            5,
        );
        foreign_cursor.set(4, 3, '\u{308}');
        rejected(
            &mut foreign_cursor,
            &graph,
            3,
            3,
            "e\u{301}abc",
            style,
            &owner,
            true,
            "foreign occupied combining cursor retains its own base/mark",
        );
        let mut boundary = Canvas::new(8, 5);
        published(
            &mut boundary,
            &graph,
            7,
            2,
            "e\u{301}",
            style,
            &owner,
            false,
            1,
        );
        assert!(format!("{boundary}").contains("e\u{301}"));
        let mut leading = Canvas::new(12, 5);
        leading.set(2, 2, 'q');
        leading.set(3, 2, '\u{308}');
        rejected(
            &mut leading,
            &graph,
            4,
            2,
            "\u{301}abc",
            style,
            &owner,
            false,
            "leading mark has no newly owned base",
        );
        // This is a refusal/atomicity calibration, NOT full ZWJ support.
        // Scalar advance and grapheme display width differ in the current model.
        let zwj = "👩\u{200d}💻";
        assert_ne!(
            zwj.chars().map(display_char_width).sum::<usize>(),
            display_width(zwj)
        );
        let mut unsupported = Canvas::new(16, 6);
        route_cell(
            &mut unsupported,
            6,
            3,
            style.edge_v,
            "edge:9:X->Y",
            CellOwnerKind::EdgeSegment,
            CellRole::Vertical,
        );
        rejected(
            &mut unsupported,
            &graph,
            4,
            3,
            zwj,
            style,
            &owner,
            false,
            "unsupported ZWJ scalar-advance footprint",
        );
    }

    #[test]
    fn label15_late_conflict_causes_no_partial_write() {
        let style = BaseStyle::Unicode.chars();
        let (graph, owner) = graph(Direction::TD);
        for conflict in [6, 8] {
            let mut canvas = Canvas::new(22, 8);
            canvas.set(0, 0, 'q');
            canvas.set(1, 0, '\u{308}');
            canvas.set_write_stage("before-label-transaction");
            route_cell(
                &mut canvas,
                conflict,
                4,
                style.edge_v,
                "edge:9:X->Y",
                CellOwnerKind::EdgeSegment,
                CellRole::Vertical,
            );
            rejected(
                &mut canvas,
                &graph,
                2,
                4,
                "e\u{301}ab cde",
                style,
                &owner,
                false,
                "late/middle ASCII+combining conflict",
            );
        }
        let mut wide = Canvas::new(22, 8);
        wide.set(0, 0, 'q');
        wide.set(1, 0, '\u{308}');
        route_cell(
            &mut wide,
            7,
            4,
            style.edge_v,
            "edge:9:X->Y",
            CellOwnerKind::EdgeSegment,
            CellRole::Vertical,
        );
        rejected(
            &mut wide,
            &graph,
            2,
            4,
            "ab界界a",
            style,
            &owner,
            false,
            "late CJK continuation conflict",
        );
        // Insert this block immediately before the existing final brace of
        // label15_late_conflict_causes_no_partial_write; retain all old assertions.
        // This reaches production commit_convergent_site, beyond the cfg(test) primitive.
        {
            for (label, late, witness) in [
                (
                    "1234567",
                    16,
                    "actual Site late final payload owner conflict",
                ),
                ("队abc", 11, "actual Site wide continuation owner conflict"),
            ] {
                let mut f = h2_shared_publication_fixture(BaseStyle::Unicode, label, "other");
                let width = display_width(label);
                assert_eq!(f.proof(0).unwrap().turn_y, 7);
                assert!(convergent_positioned_text(&f.canvas, 10, 10 + width, 7)
                    .unwrap()
                    .chars()
                    .all(|c| c == '─'));
                let mut valid_control = f.clone();
                let complete = valid_control
                    .commit_at(0, 10, 7, true, false, None)
                    .unwrap();
                assert_eq!(
                    complete,
                    (10..10 + width).map(|x| (x, 7)).collect::<Vec<_>>()
                );
                assert_eq!(
                    convergent_positioned_text(&valid_control.canvas, 10, 10 + width, 7).as_deref(),
                    Some(label)
                );
                valid_control.assert_receipt(0, 0);

                // Keep the actual raw horizontal route and a valid canonical OTHER
                // edge owner. Only complete current-edge/collector preflight rejects it.
                let foreign = f.owner(1);
                route_cell(
                    &mut f.canvas,
                    late,
                    7,
                    '─',
                    &foreign,
                    CellOwnerKind::EdgeSegment,
                    CellRole::Horizontal,
                );
                assert_eq!(f.canvas.get(late, 7), '─');
                let meta = f.canvas.get_meta(late, 7).unwrap();
                assert_eq!(meta.owner_id.as_deref(), Some(foreign.as_str()));
                assert_eq!(meta.owner_kind, CellOwnerKind::EdgeSegment);
                assert_eq!(meta.role, CellRole::Horizontal);
                assert_eq!(
                    f.proof(0).unwrap().turn_y,
                    7,
                    "{witness}: real path remains intact"
                );
                assert_eq!(f.canvas.get(10, 7), '─', "prefix starts unmodified");
                f.canvas
                    .set_owned(40, 3, 'q', CellOwnerKind::NodeLabel, "protected-mark", 9);
                f.canvas.set_inferred(40, 3, '\u{308}');
                assert_eq!(
                    convergent_positioned_text(&f.canvas, 40, 41, 3).as_deref(),
                    Some("q\u{308}")
                );
                f.canvas
                    .set_write_stage("h2-before-actual-site-late-conflict");
                let before = snapshot(&f.canvas);
                let before_context = f.context.clone();
                assert!(
                    f.commit_at(0, 10, 7, true, false, None).is_none(),
                    "{witness}"
                );
                assert_eq!(
                    snapshot(&f.canvas),
                    before,
                    "{witness}: no prefix/continuation/mark/metadata/stage write"
                );
                assert_eq!(
                    f.context, before_context,
                    "{witness}: no receipt or reservation mutation"
                );
            }
        }
    }

    #[test]
    fn label15_checked_bounds_reject_overflow() {
        let style = BaseStyle::Unicode.chars();
        let (graph, owner) = graph(Direction::TD);
        let mut canvas = Canvas::new(10, 4);
        for (start, y, width) in [
            (usize::MAX, 1, 1),
            (1, 1, usize::MAX),
            (usize::MAX - 1, 1, 4),
            (0, usize::MAX, 1),
            (10, 1, 1),
            (0, 4, 1),
            (0, 1, 0),
        ] {
            assert!(
                !convergent_label_span_in_bounds(&canvas, start, y, width),
                "{start}/{y}/{width}"
            );
        }
        for (start, y) in [
            (usize::MAX, 1),
            (usize::MAX - 1, 1),
            (10, 1),
            (0, usize::MAX),
            (0, 4),
        ] {
            rejected(
                &mut canvas,
                &graph,
                start,
                y,
                "abc",
                style,
                &owner,
                false,
                "checked invalid bounds",
            );
        }
        for y in [0, 3] {
            assert!(convergent_label_span_in_bounds(&canvas, 0, y, 10));
            let mut boundary = Canvas::new(10, 4);
            published(
                &mut boundary,
                &graph,
                0,
                y,
                "abcdefghij",
                style,
                &owner,
                false,
                10,
            );
        }
        let mut margin = Canvas::new(12, 5);
        route_cell(
            &mut margin,
            9,
            2,
            style.edge_v,
            &owner,
            CellOwnerKind::EdgeSegment,
            CellRole::Vertical,
        );
        rejected(
            &mut margin,
            &graph,
            2,
            2,
            LABEL,
            style,
            &owner,
            false,
            "protected right margin",
        );
        let mut left_margin = Canvas::new(12, 5);
        route_cell(
            &mut left_margin,
            1,
            2,
            style.edge_v,
            &owner,
            CellOwnerKind::EdgeSegment,
            CellRole::Vertical,
        );
        rejected(
            &mut left_margin,
            &graph,
            2,
            2,
            LABEL,
            style,
            &owner,
            false,
            "protected left margin",
        );
    }

    #[test]
    fn label15_threshold_mirror_translation_and_safe_zero() {
        let style = BaseStyle::Unicode.chars();
        for source in [9, 8, 6] {
            for (dx, dy) in [(0, 0), (11, 3)] {
                for mirror in [false, true] {
                    let (mut graph, owner) = graph(Direction::TD);
                    let width = 80;
                    let sx = if mirror {
                        width - 1 - (source + dx)
                    } else {
                        source + dx
                    };
                    let tx = if mirror {
                        width - 1 - (source + dx + 20)
                    } else {
                        source + dx + 20
                    };
                    graph.add_node(node("Origin", sx - 2, 1 + dy, 5));
                    graph.add_node(node("Finish", tx - 2, 10 + dy, 5));
                    let start = if mirror { width - dx - 7 } else { dx };
                    let y = 5 + dy;
                    let mut canvas = Canvas::new(width, 18);
                    route_cell(
                        &mut canvas,
                        sx,
                        y,
                        style.edge_v,
                        &owner,
                        CellOwnerKind::EdgeSegment,
                        CellRole::Vertical,
                    );
                    // At8 the old two-cell preference is short, but a complete
                    // span with one clear margin is safe. No source_x cutoff.
                    if source >= 8 {
                        published(
                            &mut canvas,
                            &graph,
                            start,
                            y,
                            LABEL,
                            style,
                            &owner,
                            false,
                            7,
                        );
                    } else {
                        rejected(
                            &mut canvas,
                            &graph,
                            start,
                            y,
                            LABEL,
                            style,
                            &owner,
                            false,
                            "translated/mirrored destructive preference",
                        );
                    }
                    assert_eq!(canvas.get(sx, y), style.edge_v);
                }
            }
        }
    }

    struct Entry {
        canvas: Canvas,
        graph: Graph,
        from: Node,
        to: Node,
        edge: Edge,
        owner: String,
        label_y: usize,
    }

    fn entry(direction: Direction, source_x: usize, target_x: usize) -> Entry {
        let style = BaseStyle::Unicode.chars();
        let up = direction == Direction::BT;
        let mut from = node("Origin", source_x - 2, if up { 10 } else { 1 }, 5);
        let mut to = node("Finish", target_x - 2, if up { 0 } else { 10 }, 5);
        from.rank = 0;
        to.rank = 1;
        let edge = Edge::with_label(&from.id, &to.id, LABEL);
        let owner = edge_owner_id(0, &edge);
        let mut graph = Graph::new();
        graph.direction = direction;
        graph.add_node(from.clone());
        graph.add_node(to.clone());
        graph.add_edge(edge.clone());
        let mut canvas = Canvas::new(44, 14);
        let turn_y = 7;
        let source_rows = if up { 8..10 } else { 4..7 };
        for y in source_rows {
            route_cell(
                &mut canvas,
                source_x,
                y,
                style.edge_v,
                &owner,
                CellOwnerKind::EdgeSegment,
                CellRole::Vertical,
            );
        }
        let left = source_x < target_x;
        let source_corner = match (up, left) {
            (false, true) => style.corner_ul,
            (false, false) => style.corner_ur,
            (true, true) => style.corner_dl,
            (true, false) => style.corner_dr,
        };
        let target_corner = match (up, left) {
            (false, true) => style.corner_dr,
            (false, false) => style.corner_dl,
            (true, true) => style.corner_ur,
            (true, false) => style.corner_ul,
        };
        route_cell(
            &mut canvas,
            source_x,
            turn_y,
            source_corner,
            &owner,
            CellOwnerKind::EdgeSegment,
            CellRole::Corner,
        );
        horizontal(
            &mut canvas,
            source_x.min(target_x) + 1,
            source_x.max(target_x),
            turn_y,
            style,
            &owner,
            CellOwnerKind::EdgeSegment,
        );
        route_cell(
            &mut canvas,
            target_x,
            turn_y,
            target_corner,
            &owner,
            CellOwnerKind::EdgeSegment,
            CellRole::Corner,
        );
        for y in if up { 4..7 } else { 8..9 } {
            route_cell(
                &mut canvas,
                target_x,
                y,
                style.edge_v,
                &owner,
                CellOwnerKind::EdgeSegment,
                CellRole::Vertical,
            );
        }
        route_cell(
            &mut canvas,
            target_x,
            if up { 3 } else { 9 },
            if up { style.arrow_up } else { style.arrow_down },
            &owner,
            CellOwnerKind::ArrowHead,
            CellRole::ArrowTip,
        );
        Entry {
            canvas,
            graph,
            from,
            to,
            edge,
            owner,
            label_y: if up { 8 } else { 5 },
        }
    }

    fn draw(entry: &mut Entry, label: &str) -> Option<EdgeLabelPlacement> {
        assert_entry_route(entry);
        invoke(entry, label)
    }

    fn invoke(entry: &mut Entry, label: &str) -> Option<EdgeLabelPlacement> {
        draw_convergent_edge_label(
            &mut entry.canvas,
            &entry.from,
            &entry.to,
            label,
            entry.graph.direction,
            BaseStyle::Unicode.chars(),
            &Config::default(),
            0,
            &entry.edge,
            &entry.graph,
            &mut ConvergentLabelContext::default(),
        )
    }

    fn assert_entry_route(entry: &Entry) {
        use super::super::topology::{
            char_connects_down, char_connects_left, char_connects_right, char_connects_up,
        };
        let style = BaseStyle::Unicode.chars();
        let source_x = entry.from.center_x();
        let target_x = entry.to.center_x();
        let up = entry.graph.direction == Direction::BT;
        let left_to_right = source_x < target_x;
        let source_corner = entry.canvas.get(source_x, 7);
        let target_corner = entry.canvas.get(target_x, 7);
        let expected = match (up, left_to_right) {
            (false, true) => ('└', '┐'),
            (false, false) => ('┘', '┌'),
            (true, true) => ('┌', '┘'),
            (true, false) => ('┐', '└'),
        };
        assert_eq!(
            (source_corner, target_corner),
            expected,
            "actual fixture corner glyphs"
        );
        assert!(if up {
            char_connects_down(source_corner)
        } else {
            char_connects_up(source_corner)
        });
        assert!(if up {
            char_connects_up(target_corner)
        } else {
            char_connects_down(target_corner)
        });
        assert!(if left_to_right {
            char_connects_right(source_corner) && char_connects_left(target_corner)
        } else {
            char_connects_left(source_corner) && char_connects_right(target_corner)
        });
        let stem_start = if up {
            entry.from.y - 1
        } else {
            entry.from.bottom_y()
        };
        assert_eq!(
            convergent_turn_row(&entry.canvas, source_x, stem_start, up, &entry.to),
            Some(7),
            "entry fixture must actually trace to the declared target before any label draw"
        );
        let arrow_y = if up { 3 } else { 9 };
        assert_eq!(
            entry.canvas.get(target_x, arrow_y),
            if up { style.arrow_up } else { style.arrow_down }
        );
        assert_eq!(
            entry
                .canvas
                .get_meta(target_x, arrow_y)
                .unwrap()
                .owner_id
                .as_deref(),
            Some(entry.owner.as_str())
        );
    }

    fn assert_complete_entry(entry: &Entry, placement: &EdgeLabelPlacement) {
        assert_eq!(placement.owner_id, entry.owner);
        let mut cells = placement.cells.clone();
        cells.sort_unstable();
        cells.dedup();
        assert_eq!(cells.len(), 7);
        let y = cells[0].1;
        assert!(cells.iter().all(|cell| cell.1 == y));
        assert_eq!(
            cells
                .iter()
                .map(|&(x, y)| entry.canvas.get(x, y))
                .collect::<String>(),
            LABEL
        );
    }

    #[derive(Clone, Copy)]
    enum StraightOwner {
        Own,
        SharedSource,
        BackEdge,
    }

    fn straight_entry(direction: Direction, route_owner: StraightOwner) -> Entry {
        let style = BaseStyle::Unicode.chars();
        let up = direction == Direction::BT;
        let from = node("Origin", 7, if up { 10 } else { 1 }, 5);
        let to = node("Finish", 7, if up { 0 } else { 10 }, 5);
        let mut edge = Edge::with_label(&from.id, &to.id, LABEL);
        edge.is_back_edge = matches!(route_owner, StraightOwner::BackEdge);
        let owner = edge_owner_id(0, &edge);
        let mut graph = Graph::new();
        graph.direction = direction;
        graph.add_node(from.clone());
        graph.add_node(to.clone());
        graph.add_edge(edge.clone());
        let shaft_owner = if matches!(route_owner, StraightOwner::SharedSource) {
            graph.add_node(node("PeerGoal", 34, 5, 5));
            let shared = Edge::new("Origin", "PeerGoal");
            let shared_owner = edge_owner_id(1, &shared);
            graph.add_edge(shared);
            shared_owner
        } else {
            owner.clone()
        };
        let kind = if edge.is_back_edge {
            CellOwnerKind::CycleEdge
        } else {
            CellOwnerKind::EdgeSegment
        };
        let mut canvas = Canvas::new(44, 14);
        for y in if up { 4..10 } else { 4..9 } {
            route_cell(
                &mut canvas,
                9,
                y,
                style.edge_v,
                &shaft_owner,
                kind,
                CellRole::Vertical,
            );
        }
        route_cell(
            &mut canvas,
            9,
            if up { 3 } else { 9 },
            if up { style.arrow_up } else { style.arrow_down },
            &owner,
            CellOwnerKind::ArrowHead,
            CellRole::ArrowTip,
        );
        let label_y = if up { 8 } else { 5 };
        // Aligned endpoints otherwise have a safe old outer preference0.
        // Make it genuinely unsafe so the connectivity proof is exercised.
        route_cell(
            &mut canvas,
            0,
            label_y,
            style.edge_v,
            "edge:99:Other->Missing",
            CellOwnerKind::EdgeSegment,
            CellRole::Vertical,
        );
        Entry {
            canvas,
            graph,
            from,
            to,
            edge,
            owner,
            label_y,
        }
    }

    fn assert_straight_raw(entry: &Entry) {
        let style = BaseStyle::Unicode.chars();
        let up = entry.graph.direction == Direction::BT;
        assert_eq!(entry.from.center_x(), 9);
        assert_eq!(entry.to.center_x(), 9);
        let stem_start = if up {
            entry.from.y - 1
        } else {
            entry.from.bottom_y()
        };
        assert_eq!(
            convergent_turn_row(&entry.canvas, 9, stem_start, up, &entry.to),
            None,
            "aligned routes intentionally have no sideways turn"
        );
        for y in if up { 4..10 } else { 4..9 } {
            assert_eq!(
                entry.canvas.get(9, y),
                style.edge_v,
                "mutually joined straight shaft at{y}"
            );
        }
        let head_y = if up {
            entry.to.bottom_y()
        } else {
            entry.to.y - 1
        };
        assert_eq!(head_y, if up { 3 } else { 9 });
        assert_eq!(
            convergent_straight_end(&entry.canvas, 9, stem_start, up, &entry.to, style),
            Some(head_y),
            "intact aligned baseline must prove the actual declared target boundary"
        );
        assert_eq!(
            entry.canvas.get(9, head_y),
            if up { style.arrow_up } else { style.arrow_down }
        );
        assert_eq!(
            entry
                .canvas
                .get_meta(9, head_y)
                .unwrap()
                .owner_id
                .as_deref(),
            Some(entry.owner.as_str())
        );
        assert!(
            !convergent_label_slot_is_writable(
                &entry.canvas,
                &entry.graph,
                0,
                entry.label_y,
                LABEL,
                style,
                &entry.owner,
                false
            ),
            "unsafe preference is required to reach fallback proof"
        );
    }

    fn isolate_far_straight_candidate(entry: &mut Entry) -> usize {
        let style = BaseStyle::Unicode.chars();
        let up = entry.graph.direction == Direction::BT;
        let far = if up { 4 } else { 8 };
        for y in if up { 4..10 } else { 4..9 } {
            route_cell(
                &mut entry.canvas,
                0,
                y,
                style.edge_v,
                "edge:99:Other->Missing",
                CellOwnerKind::EdgeSegment,
                CellRole::Vertical,
            );
            if y != far {
                route_cell(
                    &mut entry.canvas,
                    11,
                    y,
                    style.edge_v,
                    "edge:99:Other->Missing",
                    CellOwnerKind::EdgeSegment,
                    CellRole::Vertical,
                );
            }
        }
        for y in if up { 4..10 } else { 4..9 } {
            assert!(!convergent_label_slot_is_writable(
                &entry.canvas,
                &entry.graph,
                0,
                y,
                LABEL,
                style,
                &entry.owner,
                false
            ));
            assert_eq!(
                convergent_label_slot_is_writable(
                    &entry.canvas,
                    &entry.graph,
                    11,
                    y,
                    LABEL,
                    style,
                    &entry.owner,
                    false
                ),
                y == far,
                "only one later row has complete safe text room"
            );
        }
        far
    }

    #[test]
    fn label15_fallback_is_local_deterministic_and_complete() {
        let style = BaseStyle::Unicode.chars();
        for direction in [Direction::TD, Direction::BT] {
            for route_owner in [
                StraightOwner::Own,
                StraightOwner::SharedSource,
                StraightOwner::BackEdge,
            ] {
                let mut aligned = straight_entry(direction, route_owner);
                assert_straight_raw(&aligned);
                assert!(convergent_label_slot_is_writable(
                    &aligned.canvas,
                    &aligned.graph,
                    11,
                    aligned.label_y,
                    LABEL,
                    style,
                    &aligned.owner,
                    false
                ));
                let row = aligned.label_y;
                let placement = invoke(&mut aligned, LABEL)
                    .expect("connected aligned source must retain complete safe fallback");
                assert_complete_entry(&aligned, &placement);
                assert_eq!(
                    placement.cells,
                    (11..18).map(|x| (x, row)).collect::<Vec<_>>()
                );
                assert_straight_raw(&aligned);
            }
        }
        for direction in [Direction::TD, Direction::BT] {
            let mut preferred = entry(direction, 11, 16);
            let row = preferred.label_y;
            let placement = draw(&mut preferred, LABEL).expect("safe preference retained");
            assert_complete_entry(&preferred, &placement);
            assert_eq!(placement.cells.iter().min().copied(), Some((2, row)));

            let mut opposite = entry(direction, 7, 12);
            let before_route = opposite.canvas.explicit_edge_meta();
            let row = opposite.label_y;
            let placement = draw(&mut opposite, LABEL).expect("complete opposite-side alternative");
            assert_complete_entry(&opposite, &placement);
            assert!(placement
                .cells
                .iter()
                .all(|&(x, y)| y == row && (9..16).contains(&x)));
            for (x, y, meta) in before_route {
                assert_eq!(
                    opposite.canvas.get(x, y),
                    meta.ch,
                    "fallback lost protected route {x}/{y}"
                );
            }
        }
        // Both row4 and row6 are clear equally near alternatives; occupied
        // preferred row5 uses nontext route glyphs so adjacency is independent.
        let mut nearby = entry(Direction::TD, 7, 12);
        for x in [0, 9] {
            route_cell(
                &mut nearby.canvas,
                x,
                5,
                style.edge_v,
                "edge:9:X->Y",
                CellOwnerKind::EdgeSegment,
                CellRole::Vertical,
            );
        }
        assert!(convergent_label_slot_is_writable(
            &nearby.canvas,
            &nearby.graph,
            9,
            4,
            LABEL,
            style,
            &nearby.owner,
            false
        ));
        assert!(convergent_label_slot_is_writable(
            &nearby.canvas,
            &nearby.graph,
            9,
            6,
            LABEL,
            style,
            &nearby.owner,
            false
        ));
        let mut repeat = Entry {
            canvas: nearby.canvas.clone(),
            graph: nearby.graph.clone(),
            from: nearby.from.clone(),
            to: nearby.to.clone(),
            edge: nearby.edge.clone(),
            owner: nearby.owner.clone(),
            label_y: nearby.label_y,
        };
        let first = draw(&mut nearby, LABEL).expect("nearby source-corridor slot");
        let second = draw(&mut repeat, LABEL).expect("repeat same choice");
        assert_eq!(first, second);
        assert_eq!(snapshot(&nearby.canvas), snapshot(&repeat.canvas));
        assert_complete_entry(&nearby, &first);
        assert!(first.cells.iter().all(|&(_, y)| [4, 6].contains(&y)));

        let mut neighbour = entry(Direction::TD, 7, 12);
        for x in [0, 9] {
            route_cell(
                &mut neighbour.canvas,
                x,
                5,
                style.edge_v,
                "edge:9:X->Y",
                CellOwnerKind::EdgeSegment,
                CellRole::Vertical,
            );
        }
        neighbour.canvas.set(10, 3, 'z');
        rejected(
            &mut neighbour.canvas,
            &neighbour.graph,
            9,
            4,
            LABEL,
            style,
            &neighbour.owner,
            false,
            "row beside foreign label text",
        );
        let placement =
            draw(&mut neighbour, LABEL).expect("row6 remains clear when row4 abuts text");
        assert_complete_entry(&neighbour, &placement);
        assert!(
            placement.cells.iter().all(|&(_, y)| y == 6),
            "do not stack label against neighboring text"
        );
        assert_eq!(neighbour.canvas.get(10, 3), 'z');
        // Insert this block immediately before the existing final brace of
        // label15_fallback_is_local_deterministic_and_complete; retain old assertions.
        {
            let mut f =
                h2_typed_source_crossing_fixture(Direction::TD, BaseStyle::Unicode, false, '┼');
            let foreign = edge_owner_id(1, &f.graph.edges[1]);
            f.raw(9, 7, '─', &foreign, CellRole::Horizontal);
            let meta = f.canvas.get_meta(9, 7).unwrap();
            assert_eq!(meta.owner_id.as_deref(), Some(foreign.as_str()));
            assert_eq!(meta.owner_kind, CellOwnerKind::EdgeSegment);
            assert_eq!(meta.role, CellRole::Horizontal);
            f.check(
                7,
                Some(7),
                "full-predicate rescan retains actual current path",
            );
            assert!(routed_label_slot_is_writable(&f.canvas, 10, 7, 7, &f.style));
            assert!(label_neighbour_rows_clear(&f.canvas, 10, 7, 7));
            assert_eq!(
                nearest_routed_label_slot(&f.canvas, 8, 20, 10, 7, 7, &f.style),
                Some(10),
                "old raw nearest accepts the foreign horizontal padded margin"
            );
            h2_c_collector_commit(
                &mut f,
                10,
                7,
                LABEL,
                true,
                None,
                true,
                false,
                "preferred full guard rejects foreign padded margin",
            );
            h2_c_collector_commit(
                &mut f,
                9,
                7,
                LABEL,
                true,
                None,
                true,
                false,
                "equally-near lower start still includes foreign cell",
            );
            let mut next = f.clone();
            h2_c_collector_commit(
                &mut next,
                11,
                7,
                LABEL,
                true,
                None,
                true,
                true,
                "next same-row candidate satisfies complete Site predicate",
            )
            .unwrap();

            let context = ConvergentLabelContext::default();
            let before_context = context.clone();
            let protected = f.canvas.explicit_edge_meta();
            let publication = publish_vertical_convergent_label(
                &mut f.canvas,
                &f.graph,
                &f.from,
                &f.to,
                f.direction,
                10,
                7,
                LABEL,
                &f.style,
                &f.owner,
                true,
                &f.style,
                &context,
                Config::default().spacing.max_canvas_width,
            )
            .expect("full-predicate rescan must publish the next safe same-row candidate");
            assert_eq!(
                publication.cells,
                (11..18).map(|x| (x, 7)).collect::<Vec<_>>(),
                "full-predicate rescan chooses the next safe same-row candidate"
            );
            assert_eq!(publication.start, 11);
            assert_eq!(publication.y, 7);
            assert_eq!(publication.proof.as_ref().unwrap().turn_y, 7);
            assert_eq!(publication.underlay.len(), 7);
            assert_eq!(
                convergent_positioned_text(&f.canvas, 11, 18, 7).as_deref(),
                Some(LABEL)
            );
            assert_eq!(context, before_context);
            for (x, y, meta) in protected {
                if !publication.cells.contains(&(x, y)) {
                    assert_eq!(f.canvas.get(x, y), meta.ch);
                    assert_eq!(f.canvas.get_meta(x, y), Some(&meta));
                }
            }

            // The public mTLS regression has a Thick current edge and a thin fanin
            // collector. Test that exact style/owner mechanism through the real writer,
            // with BT source/target heads and a real visible second incoming branch.
            for base in [BaseStyle::Ascii, BaseStyle::Unicode] {
                let mut mixed = h2_c_mtls_fixture(base);
                assert_eq!(mixed.graph.direction, Direction::BT);
                assert_eq!(mixed.graph.edges[0].from, "FEp");
                assert_eq!(mixed.graph.edges[0].to, "Catp");
                assert_eq!(mixed.graph.edges[0].label.as_deref(), Some("mTLS"));
                assert_eq!(mixed.graph.edges[0].kind, EdgeKind::Thick);
                assert_eq!(mixed.graph.edges[1].from, "Istiod");
                assert_eq!(mixed.graph.edges[1].to, "Catp");
                assert_eq!(mixed.graph.edges[1].kind, EdgeKind::Dotted);
                assert!(mixed
                    .graph
                    .nodes
                    .iter()
                    .all(|n| convergent_node_in_bounds(&mixed.canvas, n)));
                let branch = edge_label_style(&mixed.style, &mixed.graph.edges[0]);
                assert_ne!(
                    branch.edge_h, mixed.style.edge_h,
                    "actual current edge is not thin"
                );
                let (stem_x, stem_y) = mixed.point(7, 5);
                assert_eq!(mixed.canvas.get(stem_x, stem_y), branch.edge_v);
                assert_eq!(
                    mixed
                        .canvas
                        .get_meta(stem_x, stem_y)
                        .unwrap()
                        .owner_id
                        .as_deref(),
                    Some(mixed.owner.as_str())
                );
                let protected = mixed.canvas.explicit_edge_meta();
                let row = mixed.point(7, 7).1;
                let mut context = ConvergentLabelContext::default();
                let edge = mixed.graph.edges[0].clone();
                let placement = draw_convergent_edge_label(
                    &mut mixed.canvas,
                    &mixed.from,
                    &mixed.to,
                    "mTLS",
                    Direction::BT,
                    &mixed.style,
                    &Config::default(),
                    0,
                    &edge,
                    &mixed.graph,
                    &mut context,
                )
                .expect("mTLS on the actual thin collector must survive Thick kind styling");
                assert_eq!(placement.owner_id, mixed.owner);
                assert_eq!(
                    placement.cells,
                    (12..16).map(|x| (x, row)).collect::<Vec<_>>()
                );
                assert_eq!(
                    convergent_positioned_text(&mixed.canvas, 12, 16, row).as_deref(),
                    Some("mTLS")
                );
                assert_eq!(
                    convergent_positioned_text(&mixed.canvas, 11, 17, row).as_deref(),
                    Some(if base == BaseStyle::Ascii {
                        "-mTLS-"
                    } else {
                        "─mTLS─"
                    })
                );
                assert_eq!(context.receipts.len(), 1);
                assert_eq!(context.reservations, vec![placement.cells.clone()]);
                assert_eq!(
                    context.valid_indices(&mixed.canvas, &mixed.graph, &mixed.style),
                    vec![0]
                );
                for (x, y, meta) in protected {
                    if !placement.cells.contains(&(x, y)) {
                        assert_eq!(
                            mixed.canvas.get(x, y),
                            meta.ch,
                            "mTLS protected route {x}/{y}"
                        );
                        assert_eq!(mixed.canvas.get_meta(x, y), Some(&meta));
                    }
                }
            }
        }
    }

    #[test]
    fn label15_no_slot_and_empty_label_are_atomic() {
        let style = BaseStyle::Unicode.chars();
        for direction in [Direction::TD, Direction::BT] {
            // Raw aligned joins remain intact, but only the later candidate
            // has space. First-stem and candidate-anchor ownership are
            // independent requirements; do not constrain ownership of every
            // cell along a legitimate merged target path.
            for at_first_stem in [false, true] {
                for invalid_owner in ["stale", "noncanonical", "other-source", "wrong-kind"] {
                    let mut aligned = straight_entry(direction, StraightOwner::Own);
                    let far = isolate_far_straight_candidate(&mut aligned);
                    assert_straight_raw(&aligned);
                    let bad_y = if at_first_stem {
                        if direction == Direction::BT {
                            9
                        } else {
                            4
                        }
                    } else {
                        far
                    };
                    let bad_owner = match invalid_owner {
                        "stale" => "edge:99:Origin->Missing".to_owned(),
                        "noncanonical" => "Origin->Finish".to_owned(),
                        "other-source" => {
                            aligned.graph.add_node(node("Other", 34, 1, 5));
                            aligned.graph.add_node(node("PeerGoal", 34, 9, 5));
                            let foreign = Edge::new("Other", "PeerGoal");
                            let id = edge_owner_id(1, &foreign);
                            aligned.graph.add_edge(foreign);
                            id
                        }
                        _ => aligned.owner.clone(),
                    };
                    let kind = if invalid_owner == "wrong-kind" {
                        CellOwnerKind::CycleEdge
                    } else {
                        CellOwnerKind::EdgeSegment
                    };
                    route_cell(
                        &mut aligned.canvas,
                        9,
                        bad_y,
                        style.edge_v,
                        &bad_owner,
                        kind,
                        CellRole::Vertical,
                    );
                    assert_straight_raw(&aligned);
                    assert!(convergent_label_slot_is_writable(
                        &aligned.canvas,
                        &aligned.graph,
                        11,
                        far,
                        LABEL,
                        style,
                        &aligned.owner,
                        false
                    ));
                    let before = snapshot(&aligned.canvas);
                    assert_eq!(invoke(&mut aligned, LABEL), None, "reject {invalid_owner} at first={at_first_stem} without borrowing a later shaft");
                    assert_eq!(snapshot(&aligned.canvas), before);
                }
            }
            let mut broken = straight_entry(direction, StraightOwner::Own);
            let far = isolate_far_straight_candidate(&mut broken);
            assert_straight_raw(&broken);
            broken.canvas.set(9, 6, ' ');
            assert_eq!(broken.canvas.get(9, 6), ' ');
            let up = direction == Direction::BT;
            let stem_start = if up {
                broken.from.y - 1
            } else {
                broken.from.bottom_y()
            };
            assert_eq!(
                convergent_straight_end(&broken.canvas, 9, stem_start, up, &broken.to, style),
                None,
                "actual raw gap must destroy the aligned target proof"
            );
            assert!(convergent_label_slot_is_writable(
                &broken.canvas,
                &broken.graph,
                11,
                far,
                LABEL,
                style,
                &broken.owner,
                false
            ));
            let before = snapshot(&broken.canvas);
            assert_eq!(
                invoke(&mut broken, LABEL),
                None,
                "an aligned gap cannot be jumped to a farther own shaft"
            );
            assert_eq!(snapshot(&broken.canvas), before);

            for foreign_later_anchor in [false, true] {
                let mut disconnected = entry(direction, 7, 12);
                assert_entry_route(&disconnected);
                let up = direction == Direction::BT;
                let (gap, later, near_rows) = if up { (8, 5, [8, 9]) } else { (6, 8, [4, 5]) };
                disconnected.canvas.set(7, gap, ' ');
                for x in 9..16 {
                    disconnected.canvas.set(x, later, ' ');
                }
                for y in near_rows {
                    route_cell(
                        &mut disconnected.canvas,
                        9,
                        y,
                        style.edge_v,
                        "edge:99:Other->Missing",
                        CellOwnerKind::EdgeSegment,
                        CellRole::Vertical,
                    );
                }
                route_cell(
                    &mut disconnected.canvas,
                    0,
                    disconnected.label_y,
                    style.edge_v,
                    "edge:99:Other->Missing",
                    CellOwnerKind::EdgeSegment,
                    CellRole::Vertical,
                );
                let anchor_owner = if foreign_later_anchor {
                    disconnected.graph.add_node(node("Other", 34, 1, 5));
                    disconnected.graph.add_node(node("PeerGoal", 34, 9, 5));
                    let foreign = Edge::new("Other", "PeerGoal");
                    let id = edge_owner_id(1, &foreign);
                    disconnected.graph.add_edge(foreign);
                    id
                } else {
                    disconnected.owner.clone()
                };
                route_cell(
                    &mut disconnected.canvas,
                    7,
                    later,
                    style.edge_v,
                    &anchor_owner,
                    CellOwnerKind::EdgeSegment,
                    CellRole::Vertical,
                );
                let stem_start = if up {
                    disconnected.from.y - 1
                } else {
                    disconnected.from.bottom_y()
                };
                assert_ne!(disconnected.from.center_x(), disconnected.to.center_x());
                assert_eq!(
                    convergent_turn_row(&disconnected.canvas, 7, stem_start, up, &disconnected.to),
                    None
                );
                assert!(
                    convergent_label_slot_is_writable(
                        &disconnected.canvas,
                        &disconnected.graph,
                        9,
                        later,
                        LABEL,
                        style,
                        &disconnected.owner,
                        false
                    ),
                    "later disconnected anchor has genuine complete text room"
                );
                assert!(!convergent_label_slot_is_writable(
                    &disconnected.canvas,
                    &disconnected.graph,
                    0,
                    disconnected.label_y,
                    LABEL,
                    style,
                    &disconnected.owner,
                    false
                ));
                for y in near_rows {
                    assert!(!convergent_label_slot_is_writable(
                        &disconnected.canvas,
                        &disconnected.graph,
                        9,
                        y,
                        LABEL,
                        style,
                        &disconnected.owner,
                        false
                    ));
                }
                let before = snapshot(&disconnected.canvas);
                assert_eq!(invoke(&mut disconnected, LABEL), None, "nonaligned missing target trace cannot use disconnected later anchor; foreign={foreign_later_anchor}");
                assert_eq!(snapshot(&disconnected.canvas), before);
            }
        }
        for direction in [Direction::TD, Direction::BT] {
            let mut blocked = entry(direction, 7, 12);
            for y in 0..blocked.canvas.height {
                for x in 0..blocked.canvas.width {
                    if blocked.canvas.get(x, y) == ' ' {
                        blocked.canvas.set_owned_with_role(
                            x,
                            y,
                            '?',
                            CellOwnerKind::EdgeLabel,
                            "foreign",
                            CellRole::Text,
                            5,
                        );
                    }
                }
            }
            let before = snapshot(&blocked.canvas);
            assert_eq!(draw(&mut blocked, LABEL), None);
            assert_eq!(snapshot(&blocked.canvas), before);

            let mut empty = entry(direction, 7, 24);
            let before = snapshot(&empty.canvas);
            assert_eq!(draw(&mut empty, ""), None);
            assert_eq!(snapshot(&empty.canvas), before);
        }
        let (graph, owner) = graph(Direction::TD);
        let mut canvas = Canvas::new(12, 5);
        rejected(
            &mut canvas,
            &graph,
            2,
            2,
            "",
            style,
            &owner,
            false,
            "empty direct transaction",
        );
        // Insert this block immediately before the existing final brace of
        // label15_no_slot_and_empty_label_are_atomic; retain all original assertions.
        {
            let mut seeded =
                h2_shared_publication_fixture(BaseStyle::Unicode, "a               b", "later");
            let published_a = seeded.commit_at(0, 9, 7, true, false, None).unwrap();
            seeded.assert_receipt(0, 0);
            assert_eq!(published_a, (9..26).map(|x| (x, 7)).collect::<Vec<_>>());
            assert_eq!(seeded.context.receipts.len(), 1);
            assert_eq!(seeded.context.reservations, vec![published_a.clone()]);
            assert_eq!(
                convergent_positioned_text(&seeded.canvas, 9, 26, 7).as_deref(),
                Some("a               b")
            );

            // The preferred blank side is blocked far outside A's captured path reads.
            // Every B annotation meets A's persistent footprint; every B side candidate
            // intersects A's real stem/junction. A remains a current complete receipt.
            let mut blocked = seeded.clone();
            route_cell(
                &mut blocked.canvas,
                14,
                5,
                '│',
                "edge:99:Other->Missing",
                CellOwnerKind::EdgeSegment,
                CellRole::Vertical,
            );
            blocked.assert_receipt(0, 0);
            assert_eq!(
                blocked.proof(1).unwrap().turn_y,
                7,
                "B genuinely reaches Target using A's current recorded underlay"
            );
            assert_eq!(
                convergent_vertical_label_start(2, 30, 5, blocked.canvas.width),
                14
            );
            assert!(cells_held_by_another_line(
                &blocked.canvas,
                14,
                5,
                5,
                &blocked.owner(1)
            ));
            let before = snapshot(&blocked.canvas);
            let before_context = blocked.context.clone();
            for start in 3..=25 {
                for structured in [false, true] {
                    assert!(blocked.commit_at(1, start, 7, true, structured, None).is_none(), "every bounded horizontal candidate is invalid: start={start}, structured={structured}");
                }
            }
            for row in 4..=7 {
                for structured in [false, true] {
                    assert!(blocked.commit_at(1, 4, row, false, structured, None).is_none(), "all available stem-side candidates are invalid: row={row}, structured={structured}");
                }
            }
            assert_eq!(snapshot(&blocked.canvas), before);
            assert_eq!(blocked.context, before_context);
            assert!(
                blocked.draw_edge(1).is_none(),
                "actual writer exhausts every strict and structured slot"
            );
            assert_eq!(snapshot(&blocked.canvas), before);
            assert_eq!(blocked.context, before_context);
            blocked.assert_receipt(0, 0);

            // Empty-label failures use a nonempty valid context without changing Graph
            // label identity. Both the actual writer and direct Site transaction refuse.
            let mut empty = seeded.clone();
            let before = snapshot(&empty.canvas);
            let before_context = empty.context.clone();
            assert!(h2_c_draw_override(&mut empty, 1, "").is_none());
            assert_eq!(snapshot(&empty.canvas), before);
            assert_eq!(empty.context, before_context);
            assert!(empty.commit_at(1, 15, 7, false, false, Some("")).is_none());
            assert_eq!(snapshot(&empty.canvas), before);
            assert_eq!(empty.context, before_context);
            empty.assert_receipt(0, 0);

            // The tiny candidate and its padded margins fit wholly inside the prior
            // long word gap. Raw blank/neighbor guards accept it; persistent footprint
            // reservations reject it both before and after real legacy revocation.
            for legacy_none in [false, true] {
                let mut gap = seeded.clone();
                for x in 14..17 {
                    assert_eq!(gap.canvas.get(x, 7), ' ');
                    let meta = gap.canvas.get_meta(x, 7).unwrap();
                    assert_eq!(meta.owner_kind, CellOwnerKind::Empty);
                    assert_eq!(meta.role, CellRole::Empty);
                    assert_eq!(meta.owner_id, None);
                }
                assert!(convergent_label_slot_is_writable(
                    &gap.canvas,
                    &gap.graph,
                    15,
                    7,
                    "x",
                    &gap.style,
                    &gap.owner(1),
                    false
                ));
                assert!(gap.commit_at(1, 15, 7, false, false, Some("x")).is_none());
                let receipts_before = gap.context.receipts.len();
                let reservations_before = gap.context.reservations.clone();
                if legacy_none {
                    // Actual generic empty writer returns None. Its notification must
                    // revoke eligibility without adding or clearing any reservations.
                    let edge = gap.graph.edges[1].clone();
                    let from = gap.graph.get_node(&edge.from).unwrap().clone();
                    let to = gap.graph.get_node(&edge.to).unwrap().clone();
                    let legacy = draw_edge_label(
                        &mut gap.canvas,
                        &from,
                        &to,
                        "",
                        gap.graph.direction,
                        &gap.style,
                        &Config::default(),
                        1,
                        &edge,
                        &gap.graph,
                    );
                    assert!(legacy.is_none());
                    gap.context.observe_legacy(legacy.as_ref());
                    assert_eq!(gap.context.reservations, reservations_before);
                } else {
                    // Actual same-text legacy output overlaps A's first letter, but
                    // does not touch the tested interior word gap.
                    let legacy = gap.legacy_text("legacy:wordmark", 9, 7, "a");
                    assert_eq!(legacy.cells, vec![(9, 7)]);
                    gap.context.observe_legacy(Some(&legacy));
                    assert_eq!(
                        gap.context.reservations.len(),
                        reservations_before.len() + 1
                    );
                    assert_eq!(
                        gap.context.reservations[..reservations_before.len()],
                        reservations_before
                    );
                }
                assert_eq!(
                    gap.context.receipts.len(),
                    receipts_before,
                    "generic result never promotes a receipt"
                );
                assert!(gap.context.receipts.iter().all(|r| !r.eligible));
                assert!(gap
                    .context
                    .valid_indices(&gap.canvas, &gap.graph, &gap.style)
                    .is_empty());
                assert!(gap.context.reserves(14, 17, 7));
                assert!(convergent_label_slot_is_writable(
                    &gap.canvas,
                    &gap.graph,
                    15,
                    7,
                    "x",
                    &gap.style,
                    &gap.owner(1),
                    false
                ));
                let before = snapshot(&gap.canvas);
                let before_context = gap.context.clone();
                assert!(gap.commit_at(1, 15, 7, false, false, Some("x")).is_none());
                assert_eq!(snapshot(&gap.canvas), before);
                assert_eq!(gap.context, before_context);
                assert_eq!(
                    convergent_positioned_text(&gap.canvas, 14, 17, 7).as_deref(),
                    Some("   ")
                );
            }
        }
    }

    #[test]
    fn label15_preserves_proven_horizontal_annotation_spaces() {
        for style in styles() {
            for is_back_edge in [false, true] {
                let (graph, owner) = graph_with_back_edge(Direction::BT, is_back_edge);
                let kind = if is_back_edge {
                    CellOwnerKind::CycleEdge
                } else {
                    CellOwnerKind::EdgeSegment
                };
                let mut canvas = Canvas::new(26, 7);
                horizontal(&mut canvas, 3, 14, 3, &style, &owner, kind);
                let corner = style.corner_ul;
                route_cell(&mut canvas, 2, 3, corner, &owner, kind, CellRole::Corner);
                let cells = published(
                    &mut canvas,
                    &graph,
                    4,
                    3,
                    "abc def",
                    &style,
                    &owner,
                    true,
                    7,
                );
                assert_eq!(canvas.get(7, 3), ' ', "word gap must erase its own shaft");
                assert!(
                    cells.contains(&(7, 3)),
                    "word gap remains in provenance placement"
                );
                assert_eq!(canvas.get(2, 3), corner);
                assert_eq!(canvas.get(3, 3), style.edge_h);
                assert_eq!(canvas.get(11, 3), style.edge_h);
            }
        }
    }

    #[test]
    fn label15_transaction_scope_keeps_horizontal_paths_unchanged() {
        for direction in [Direction::LR, Direction::RL] {
            let style = BaseStyle::Unicode.chars();
            let (from_x, to_x) = if direction == Direction::LR {
                (1, 24)
            } else {
                (24, 1)
            };
            let from = node("Origin", from_x, 4, 5);
            let to = node("Finish", to_x, 4, 5);
            let edge = Edge::with_label("Origin", "Finish", LABEL);
            let owner = edge_owner_id(0, &edge);
            let mut graph = Graph::new();
            graph.direction = direction;
            graph.add_node(from.clone());
            graph.add_node(to.clone());
            graph.add_edge(edge.clone());
            let mut canvas = Canvas::new(36, 11);
            horizontal(
                &mut canvas,
                6,
                24,
                5,
                style,
                &owner,
                CellOwnerKind::EdgeSegment,
            );
            let before_route = canvas.explicit_edge_meta();
            let placement = draw_convergent_edge_label(
                &mut canvas,
                &from,
                &to,
                LABEL,
                direction,
                style,
                &Config::default(),
                0,
                &edge,
                &graph,
                &mut ConvergentLabelContext::default(),
            )
            .expect("existing horizontal slot remains feasible");
            assert_eq!(
                placement.cells,
                (11..18).map(|x| (x, 4)).collect::<Vec<_>>()
            );
            assert_eq!(
                (11..18).map(|x| canvas.get(x, 4)).collect::<String>(),
                LABEL
            );
            for (x, y, meta) in before_route {
                assert_eq!(canvas.get(x, y), meta.ch);
                assert_eq!(canvas.get_meta(x, y), Some(&meta));
            }
        }
        let mut td = entry(Direction::TD, 7, 12);
        let mut tb = entry(Direction::TB, 7, 12);
        let td_placement = draw(&mut td, LABEL);
        let tb_placement = draw(&mut tb, LABEL);
        assert!(td_placement.is_some());
        assert_eq!(td_placement, tb_placement);
        assert_eq!(snapshot(&td.canvas), snapshot(&tb.canvas));
    }

    // Append inside the existing labels.rs #[cfg(test)] mod tests.
    // Reuses node(), route_cell(), snapshot(), LABEL and its existing imports.
    // Requires ConvergentLabelContext: Default + Debug, but no private fields.

    const H2_U16_WIDTH: usize = 34;
    const H2_U16_HEIGHT: usize = 18;

    #[derive(Clone)]
    struct H2TypedPathFixture {
        canvas: Canvas,
        graph: Graph,
        from: Node,
        to: Node,
        owner: String,
        style: StyleChars,
        direction: Direction,
        mirror: bool,
    }

    impl H2TypedPathFixture {
        fn point(&self, x: usize, y: usize) -> (usize, usize) {
            (
                if self.mirror { H2_U16_WIDTH - 1 - x } else { x },
                if self.direction == Direction::BT {
                    H2_U16_HEIGHT - 1 - y
                } else {
                    y
                },
            )
        }

        fn positioned_node(&self, id: &str, center_x: usize, y: usize) -> Node {
            let x = center_x - 2;
            node(
                id,
                if self.mirror { H2_U16_WIDTH - x - 5 } else { x },
                if self.direction == Direction::BT {
                    H2_U16_HEIGHT - y - 3
                } else {
                    y
                },
                5,
            )
        }

        // Semantic Unicode reference glyphs are reflected first, then mapped to
        // the actual public StyleChars fields. Their field names are deliberately
        // not interpreted as conventional top-left/bottom-right abbreviations.
        fn glyph(&self, reference: char) -> char {
            let vertical = if self.direction == Direction::BT {
                match reference {
                    '└' => '┌',
                    '┘' => '┐',
                    '┌' => '└',
                    '┐' => '┘',
                    '┬' => '┴',
                    '┴' => '┬',
                    '↓' => '↑',
                    other => other,
                }
            } else {
                reference
            };
            let oriented = if self.mirror {
                match vertical {
                    '└' => '┘',
                    '┘' => '└',
                    '┌' => '┐',
                    '┐' => '┌',
                    '├' => '┤',
                    '┤' => '├',
                    other => other,
                }
            } else {
                vertical
            };
            match oriented {
                '│' => self.style.edge_v,
                '─' => self.style.edge_h,
                '└' => self.style.corner_ul,
                '┘' => self.style.corner_ur,
                '┌' => self.style.corner_dl,
                '┐' => self.style.corner_dr,
                '┬' => self.style.junction_down,
                '┴' => self.style.junction_up,
                '├' => self.style.junction_right,
                '┤' => self.style.junction_left,
                '┼' => self.style.cross,
                '✕' => self.style.cross_end,
                '↓' => self.style.arrow_down,
                '↑' => self.style.arrow_up,
                other => panic!("unexpected U16 reference glyph {other:?}"),
            }
        }

        fn raw(&mut self, x: usize, y: usize, glyph: char, owner: &str, role: CellRole) {
            let (x, y) = self.point(x, y);
            let kind = if role == CellRole::ArrowTip {
                CellOwnerKind::ArrowHead
            } else {
                CellOwnerKind::EdgeSegment
            };
            route_cell(&mut self.canvas, x, y, glyph, owner, kind, role);
        }

        fn put(&mut self, x: usize, y: usize, reference: char, owner: &str, role: CellRole) {
            let glyph = self.glyph(reference);
            self.raw(x, y, glyph, owner, role);
        }

        fn add_edge(&mut self, from: &str, to: &str) -> String {
            let edge = Edge::new(from, to);
            let owner = edge_owner_id(self.graph.edges.len(), &edge);
            self.graph.add_edge(edge);
            owner
        }

        fn stem_start(&self) -> usize {
            if self.direction == Direction::BT {
                self.from.y - 1
            } else {
                self.from.bottom_y()
            }
        }

        fn source_faces_node(&self) -> bool {
            let ch = self.canvas.get(self.from.center_x(), self.stem_start());
            if self.direction == Direction::BT {
                super::super::topology::char_connects_down(ch)
            } else {
                super::super::topology::char_connects_up(ch)
            }
        }

        fn check(&self, legacy_turn: usize, wanted_turn: Option<usize>, witness: &str) {
            let (sx, first) = self.point(7, 4);
            let (tx, head_y) = self.point(20, 9);
            assert_eq!(self.from.center_x(), sx, "{witness}: source geometry");
            assert_eq!(self.to.center_x(), tx, "{witness}: target geometry");
            assert_eq!(self.stem_start(), first, "{witness}: actual source entry");
            assert_eq!(self.graph.nodes[0].id, self.from.id);
            assert_eq!(self.graph.nodes[1].id, self.to.id);
            assert_eq!(self.graph.direction, self.direction);
            assert_eq!(self.graph.edges[0].from, self.from.id);
            assert_eq!(self.graph.edges[0].to, self.to.id);
            assert_eq!(edge_owner_id(0, &self.graph.edges[0]), self.owner);
            assert_eq!(
                self.canvas.get_meta(sx, first).unwrap().owner_id.as_deref(),
                Some(self.owner.as_str()),
                "{witness}: first stem is canonical even in deliberate negatives"
            );
            assert_eq!(
                head_y,
                if self.direction == Direction::BT {
                    self.to.bottom_y()
                } else {
                    self.to.y - 1
                },
                "{witness}: actual target-adjacent head"
            );
            assert_eq!(
                self.canvas.get(tx, head_y),
                self.glyph('↓'),
                "{witness}: direction-correct actual head"
            );
            assert_eq!(
                self.canvas.get_meta(tx, head_y).unwrap().role,
                CellRole::ArrowTip
            );
            assert_eq!(
                convergent_turn_row(
                    &self.canvas,
                    sx,
                    first,
                    self.direction == Direction::BT,
                    &self.to,
                ),
                Some(self.point(7, legacy_turn).1),
                "{witness}: legacy raw trace must actually reach the declared target"
            );

            let context = ConvergentLabelContext::default();
            let before_canvas = snapshot(&self.canvas);
            let before_context = format!("{context:?}");
            let proof = prove_convergent_path(
                &self.canvas,
                &self.graph,
                &self.from,
                &self.to,
                self.direction,
                &self.style,
                &self.owner,
                &context,
            );
            match wanted_turn {
                Some(y) => {
                    let proof = proof.unwrap_or_else(|| {
                        panic!("{witness}: intact typed own path must remain provable")
                    });
                    assert_eq!(
                        proof.turn_y,
                        self.point(7, y).1,
                        "{witness}: retain the actual own turn"
                    );
                }
                None => assert!(
                    proof.is_none(),
                    "{witness}: typed semantics must reject this raw-only path"
                ),
            }
            assert_eq!(
                snapshot(&self.canvas),
                before_canvas,
                "{witness}: pure probe"
            );
            assert_eq!(
                format!("{context:?}"),
                before_context,
                "{witness}: pure context"
            );
        }
    }

    fn h2_typed_path_fixture(
        direction: Direction,
        base: BaseStyle,
        mirror: bool,
        turn: usize,
    ) -> H2TypedPathFixture {
        let mut fixture = H2TypedPathFixture {
            canvas: Canvas::new(H2_U16_WIDTH, H2_U16_HEIGHT),
            graph: Graph::new(),
            from: node("Origin", 5, 1, 5),
            to: node("Finish", 18, 10, 5),
            owner: String::new(),
            style: *base.chars(),
            direction,
            mirror,
        };
        fixture.from = fixture.positioned_node("Origin", 7, 1);
        fixture.to = fixture.positioned_node("Finish", 20, 10);
        fixture.graph.direction = direction;
        fixture.graph.add_node(fixture.from.clone());
        fixture.graph.add_node(fixture.to.clone());
        let edge = Edge::with_label("Origin", "Finish", LABEL);
        fixture.owner = edge_owner_id(0, &edge);
        fixture.graph.add_edge(edge);
        let owner = fixture.owner.clone();
        for y in 4..turn {
            fixture.put(7, y, '│', &owner, CellRole::Vertical);
        }
        fixture.put(7, turn, '└', &owner, CellRole::Corner);
        for x in 8..20 {
            fixture.put(x, turn, '─', &owner, CellRole::Horizontal);
        }
        fixture.put(20, turn, '┐', &owner, CellRole::Corner);
        for y in turn + 1..9 {
            fixture.put(20, y, '│', &owner, CellRole::Vertical);
        }
        fixture.put(20, 9, '↓', &owner, CellRole::ArrowTip);
        fixture
    }

    fn h2_typed_source_crossing_fixture(
        direction: Direction,
        base: BaseStyle,
        mirror: bool,
        junction_reference: char,
    ) -> H2TypedPathFixture {
        let mut fixture = h2_typed_path_fixture(direction, base, mirror, 7);
        let peer_node = fixture.positioned_node("Peer", 2, 1);
        fixture.graph.add_node(peer_node);
        let peer = fixture.add_edge("Peer", "Finish");
        let other_node = fixture.positioned_node("Other", 7, 13);
        fixture.graph.add_node(other_node);
        let other = fixture.add_edge("Origin", "Other");
        let collector = format!("fanin:{}", fixture.to.id);
        for y in 4..7 {
            fixture.put(2, y, '│', &peer, CellRole::Vertical);
        }
        fixture.put(2, 7, '└', &peer, CellRole::Corner);
        for x in 3..20 {
            fixture.put(x, 7, '─', &collector, CellRole::Horizontal);
        }
        for y in 8..12 {
            fixture.put(7, y, '│', &other, CellRole::Vertical);
        }
        fixture.put(7, 12, '↓', &other, CellRole::ArrowTip);
        fixture.put(7, 7, junction_reference, &collector, CellRole::Junction);
        fixture
    }

    fn h2_typed_target_crossing_fixture(
        direction: Direction,
        base: BaseStyle,
        mirror: bool,
        junction_reference: char,
    ) -> H2TypedPathFixture {
        let mut fixture = h2_typed_path_fixture(direction, base, mirror, 7);
        let peer_node = fixture.positioned_node("Peer", 20, 1);
        fixture.graph.add_node(peer_node);
        let peer = fixture.add_edge("Peer", "Finish");
        let other_node = fixture.positioned_node("Other", 27, 10);
        fixture.graph.add_node(other_node);
        let other = fixture.add_edge("Origin", "Other");
        for y in 4..9 {
            fixture.put(20, y, '│', &peer, CellRole::Vertical);
        }
        fixture.put(20, 9, '↓', &peer, CellRole::ArrowTip);
        let owner = fixture.owner.clone();
        for x in 21..27 {
            fixture.put(x, 7, '─', &owner, CellRole::Horizontal);
        }
        fixture.put(27, 7, '┐', &other, CellRole::Corner);
        fixture.put(27, 8, '│', &other, CellRole::Vertical);
        fixture.put(27, 9, '↓', &other, CellRole::ArrowTip);
        fixture.put(20, 7, junction_reference, &peer, CellRole::Junction);
        fixture
    }

    #[test]
    fn label15_proven_run_preserves_explicit_crossing_axes() {
        for direction in [Direction::TD, Direction::TB, Direction::BT] {
            for base in [BaseStyle::Ascii, BaseStyle::Unicode] {
                for mirror in [false, true] {
                    let baseline = h2_typed_path_fixture(direction, base, mirror, 7);
                    assert_eq!(baseline.graph.edges.len(), 1);
                    assert!(baseline.source_faces_node());
                    baseline.check(7, Some(7), "one-edge canonical generic path");

                    let phantom = h2_typed_source_crossing_fixture(direction, base, mirror, '✕');
                    assert!(phantom.source_faces_node());
                    let (x, y) = phantom.point(7, 7);
                    assert_eq!(phantom.canvas.get(x, y), phantom.style.cross_end);
                    assert_eq!(
                        phantom.canvas.get_meta(x, y).unwrap().role,
                        CellRole::Junction
                    );
                    assert_eq!(phantom.canvas.get(x, y - 1), phantom.style.edge_v);
                    assert_eq!(phantom.canvas.get(x, y + 1), phantom.style.edge_v);
                    phantom.check(7, None, "explicit source crossing is not a collector bend");

                    let real_source =
                        h2_typed_source_crossing_fixture(direction, base, mirror, '┼');
                    let (x, y) = real_source.point(7, 7);
                    assert_eq!(real_source.canvas.get(x, y), real_source.style.cross);
                    assert_eq!(
                        real_source.canvas.get_meta(x, y).unwrap().role,
                        CellRole::Junction
                    );
                    real_source.check(7, Some(7), "ordinary source junction really joins");

                    let target = h2_typed_target_crossing_fixture(direction, base, mirror, '✕');
                    assert!(target.source_faces_node());
                    let (x, y) = target.point(20, 7);
                    assert_eq!(target.canvas.get(x, y), target.style.cross_end);
                    assert_eq!(
                        target.canvas.get_meta(x, y).unwrap().role,
                        CellRole::Junction
                    );
                    target.check(
                        7,
                        None,
                        "explicit target crossing cannot turn toward the head",
                    );

                    let real_target =
                        h2_typed_target_crossing_fixture(direction, base, mirror, '┼');
                    let (x, y) = real_target.point(20, 7);
                    assert_eq!(real_target.canvas.get(x, y), real_target.style.cross);
                    assert_eq!(
                        real_target.canvas.get_meta(x, y).unwrap().role,
                        CellRole::Junction
                    );
                    real_target.check(7, Some(7), "ordinary target junction really joins");

                    // The earlier foreign collector genuinely reaches Finish, but
                    // Origin crosses it without joining; its own real turn is7.
                    let mut through_source = h2_typed_path_fixture(direction, base, mirror, 7);
                    let peer_node = through_source.positioned_node("Peer", 2, 1);
                    through_source.graph.add_node(peer_node);
                    let peer = through_source.add_edge("Peer", "Finish");
                    let collector = format!("fanin:{}", through_source.to.id);
                    for y in 4..5 {
                        through_source.put(2, y, '│', &peer, CellRole::Vertical);
                    }
                    through_source.put(2, 5, '└', &peer, CellRole::Corner);
                    for x in 3..20 {
                        through_source.put(x, 5, '─', &collector, CellRole::Horizontal);
                    }
                    through_source.put(20, 5, '┐', &collector, CellRole::Corner);
                    through_source.put(20, 6, '│', &collector, CellRole::Vertical);
                    through_source.put(20, 7, '┤', &collector, CellRole::Junction);
                    through_source.put(7, 5, '✕', &collector, CellRole::Junction);
                    through_source.check(
                        5,
                        Some(7),
                        "straight source transit reaches its later real turn",
                    );

                    let mut through_horizontal = h2_typed_path_fixture(direction, base, mirror, 7);
                    let peer_node = through_horizontal.positioned_node("Peer", 14, 1);
                    let other_node = through_horizontal.positioned_node("Other", 14, 13);
                    through_horizontal.graph.add_node(peer_node);
                    through_horizontal.graph.add_node(other_node);
                    let foreign = through_horizontal.add_edge("Peer", "Other");
                    for y in 4..12 {
                        through_horizontal.put(14, y, '│', &foreign, CellRole::Vertical);
                    }
                    through_horizontal.put(14, 12, '↓', &foreign, CellRole::ArrowTip);
                    through_horizontal.put(14, 7, '✕', &foreign, CellRole::Junction);
                    through_horizontal.check(
                        7,
                        Some(7),
                        "straight horizontal transit preserves the real target bend",
                    );

                    // All four marker spellings have raw four-arm connectivity.
                    // Their EndpointMarker role still terminates proof, including
                    // at the very first stem or the apparent targetward bend.
                    for endpoint in ['o', '○', 'x', '✕'] {
                        for (x, y) in [(7, 4), (7, 5), (7, 7), (20, 7), (20, 8)] {
                            let mut ended = h2_typed_path_fixture(direction, base, mirror, 7);
                            let owner = ended.owner.clone();
                            ended.raw(x, y, endpoint, &owner, CellRole::EndpointMarker);
                            let (actual_x, actual_y) = ended.point(x, y);
                            assert_eq!(ended.canvas.get(actual_x, actual_y), endpoint);
                            assert_eq!(
                                ended.canvas.get_meta(actual_x, actual_y).unwrap().role,
                                CellRole::EndpointMarker
                            );
                            ended.check(
                                7,
                                None,
                                "true endpoint cannot be crossed or used as a bend",
                            );
                        }
                    }

                    let proper = h2_typed_path_fixture(direction, base, mirror, 4);
                    assert!(proper.source_faces_node());
                    let (first_x, first_y) = proper.point(7, 4);
                    assert_eq!(proper.canvas.get(first_x, first_y), proper.glyph('└'));
                    assert_eq!(
                        proper.canvas.get_meta(first_x, first_y).unwrap().role,
                        CellRole::Corner
                    );
                    proper.check(
                        4,
                        Some(4),
                        "source-adjacent correctly facing corner remains valid",
                    );

                    let mut wrong = proper.clone();
                    let owner = wrong.owner.clone();
                    if base == BaseStyle::Unicode {
                        wrong.put(7, 4, '┌', &owner, CellRole::Corner);
                    } else {
                        // ASCII '+' has all four arms; it cannot encode a
                        // wrong-facing corner. A canonical horizontal entry
                        // witnesses the same missing-source-facing precondition.
                        wrong.put(7, 4, '─', &owner, CellRole::Horizontal);
                    }
                    assert!(!wrong.source_faces_node());
                    wrong.check(
                        4,
                        None,
                        "canonical identity does not replace source-facing connectivity",
                    );
                }
            }
        }

        // Actual alternate-style route glyphs must not be filtered by a thin-only
        // role classifier. Heavy corners are not recognized by the unchanged raw
        // topology tracer, so Heavy's real supported junctions provide its turns.
        for direction in [Direction::TD, Direction::TB, Direction::BT] {
            for base in [BaseStyle::Double, BaseStyle::Rounded, BaseStyle::Heavy] {
                for mirror in [false, true] {
                    let mut alternate = h2_typed_path_fixture(direction, base, mirror, 7);
                    let owner = alternate.owner.clone();
                    let expected_role = if base == BaseStyle::Heavy {
                        for x in [7, 20] {
                            alternate.put(x, 7, '┼', &owner, CellRole::Junction);
                        }
                        CellRole::Junction
                    } else {
                        CellRole::Corner
                    };
                    for (x, reference) in [(7, '└'), (20, '┐')] {
                        let (actual_x, actual_y) = alternate.point(x, 7);
                        let expected = if base == BaseStyle::Heavy {
                            alternate.style.cross
                        } else {
                            alternate.glyph(reference)
                        };
                        assert_eq!(alternate.canvas.get(actual_x, actual_y), expected);
                        let meta = alternate.canvas.get_meta(actual_x, actual_y).unwrap();
                        assert_eq!(meta.ch, expected);
                        assert_eq!(meta.role, expected_role);
                        assert_eq!(meta.owner_kind, CellOwnerKind::EdgeSegment);
                        assert_eq!(meta.owner_id.as_deref(), Some(owner.as_str()));
                    }
                    assert!(alternate.source_faces_node());
                    alternate.check(7, Some(7), "active alternate-style canonical route");

                    // These are fresh ordinary junction constructions, never
                    // attempts to overwrite a protected explicit x/✕ crossing.
                    let mut source_join =
                        h2_typed_source_crossing_fixture(direction, base, mirror, '┼');
                    if base == BaseStyle::Heavy {
                        let owner = source_join.owner.clone();
                        source_join.put(20, 7, '┼', &owner, CellRole::Junction);
                    }
                    let (x, y) = source_join.point(7, 7);
                    let collector = format!("fanin:{}", source_join.to.id);
                    assert_eq!(source_join.canvas.get(x, y), source_join.style.cross);
                    let meta = source_join.canvas.get_meta(x, y).unwrap();
                    assert_eq!(meta.role, CellRole::Junction);
                    assert_eq!(meta.owner_kind, CellOwnerKind::EdgeSegment);
                    assert_eq!(meta.owner_id.as_deref(), Some(collector.as_str()));
                    source_join.check(7, Some(7), "actual alternate-style collector junction");

                    let mut target_join =
                        h2_typed_target_crossing_fixture(direction, base, mirror, '┼');
                    if base == BaseStyle::Heavy {
                        let owner = target_join.owner.clone();
                        target_join.put(7, 7, '┼', &owner, CellRole::Junction);
                    }
                    let (x, y) = target_join.point(20, 7);
                    assert_eq!(target_join.canvas.get(x, y), target_join.style.cross);
                    let meta = target_join.canvas.get_meta(x, y).unwrap();
                    assert_eq!(meta.role, CellRole::Junction);
                    assert_eq!(meta.owner_kind, CellOwnerKind::EdgeSegment);
                    assert_eq!(
                        meta.owner_id.as_deref(),
                        Some(edge_owner_id(1, &target_join.graph.edges[1]).as_str())
                    );
                    target_join.check(7, Some(7), "actual alternate-style target junction");
                }
            }
        }

        // The raw block glyph has four arms. A shaft role cannot manufacture an
        // axis change even with a real collector, graph and target-adjacent head.
        // Conversely, an active custom style may intentionally use that glyph as
        // an actual Corner; typed Corner role distinguishes that valid bend.
        for direction in [Direction::TD, Direction::TB, Direction::BT] {
            for mirror in [false, true] {
                for role in [CellRole::Vertical, CellRole::Horizontal] {
                    let mut source_shaft = h2_typed_source_crossing_fixture(
                        direction,
                        BaseStyle::Unicode,
                        mirror,
                        '┼',
                    );
                    let collector = format!("fanin:{}", source_shaft.to.id);
                    source_shaft.raw(7, 7, '█', &collector, role);
                    let (x, y) = source_shaft.point(7, 7);
                    assert_eq!(source_shaft.canvas.get(x, y), '█');
                    let meta = source_shaft.canvas.get_meta(x, y).unwrap();
                    assert_eq!(meta.role, role);
                    assert_eq!(meta.owner_kind, CellOwnerKind::EdgeSegment);
                    assert_eq!(meta.owner_id.as_deref(), Some(collector.as_str()));
                    source_shaft.check(7, None, "collector-owned block shaft cannot turn");

                    let mut target_shaft = h2_typed_target_crossing_fixture(
                        direction,
                        BaseStyle::Unicode,
                        mirror,
                        '┼',
                    );
                    let peer = edge_owner_id(1, &target_shaft.graph.edges[1]);
                    target_shaft.raw(20, 7, '█', &peer, role);
                    let (x, y) = target_shaft.point(20, 7);
                    assert_eq!(target_shaft.canvas.get(x, y), '█');
                    let meta = target_shaft.canvas.get_meta(x, y).unwrap();
                    assert_eq!(meta.role, role);
                    assert_eq!(meta.owner_kind, CellOwnerKind::EdgeSegment);
                    assert_eq!(meta.owner_id.as_deref(), Some(peer.as_str()));
                    target_shaft.check(7, None, "target-owned block shaft cannot turn");
                }

                let mut block_corners =
                    h2_typed_path_fixture(direction, BaseStyle::Unicode, mirror, 7);
                block_corners.style.corner_dl = '█';
                block_corners.style.corner_dr = '█';
                block_corners.style.corner_ul = '█';
                block_corners.style.corner_ur = '█';
                let owner = block_corners.owner.clone();
                for x in [7, 20] {
                    block_corners.put(
                        x,
                        7,
                        if x == 7 { '└' } else { '┐' },
                        &owner,
                        CellRole::Corner,
                    );
                    let (actual_x, actual_y) = block_corners.point(x, 7);
                    assert_eq!(block_corners.canvas.get(actual_x, actual_y), '█');
                    let meta = block_corners.canvas.get_meta(actual_x, actual_y).unwrap();
                    assert_eq!(meta.role, CellRole::Corner);
                    assert_eq!(meta.owner_kind, CellOwnerKind::EdgeSegment);
                    assert_eq!(meta.owner_id.as_deref(), Some(owner.as_str()));
                }
                block_corners.check(7, Some(7), "active block Corner role really turns");
                for role in [CellRole::Vertical, CellRole::Horizontal] {
                    for x in [7, 20] {
                        let mut same_glyph_shaft = block_corners.clone();
                        same_glyph_shaft.raw(x, 7, '█', &owner, role);
                        let (actual_x, actual_y) = same_glyph_shaft.point(x, 7);
                        assert_eq!(same_glyph_shaft.canvas.get(actual_x, actual_y), '█');
                        let meta = same_glyph_shaft
                            .canvas
                            .get_meta(actual_x, actual_y)
                            .unwrap();
                        assert_eq!(meta.role, role);
                        assert_eq!(meta.owner_kind, CellOwnerKind::EdgeSegment);
                        assert_eq!(meta.owner_id.as_deref(), Some(owner.as_str()));
                        same_glyph_shaft.check(
                            7,
                            None,
                            "active corner glyph with shaft role still cannot turn",
                        );
                    }
                }

                let mut straight_blocks =
                    h2_typed_path_fixture(direction, BaseStyle::Unicode, mirror, 7);
                let owner = straight_blocks.owner.clone();
                for (x, y, role) in [(7, 5, CellRole::Vertical), (14, 7, CellRole::Horizontal)] {
                    straight_blocks.raw(x, y, '█', &owner, role);
                    let (actual_x, actual_y) = straight_blocks.point(x, y);
                    assert_eq!(straight_blocks.canvas.get(actual_x, actual_y), '█');
                    let meta = straight_blocks.canvas.get_meta(actual_x, actual_y).unwrap();
                    assert_eq!(meta.role, role);
                    assert_eq!(meta.owner_kind, CellOwnerKind::EdgeSegment);
                    assert_eq!(meta.owner_id.as_deref(), Some(owner.as_str()));
                }
                straight_blocks.check(
                    7,
                    Some(7),
                    "block shafts retain their straight-axis transit",
                );
            }
        }

        // Exercise the same typed-path policy through actual label publication:
        // genuine Unicode dotted shafts infer Text, retain canonical route owners,
        // and must qualify the opposite-side fallback once the preference is unsafe.
        for direction in [Direction::TD, Direction::TB, Direction::BT] {
            let style = BaseStyle::Unicode.chars();
            let mut dotted = entry(direction, 7, 12);
            dotted.edge.kind = EdgeKind::Dotted;
            dotted.graph.edges[0] = dotted.edge.clone();
            assert_eq!(edge_owner_id(0, &dotted.graph.edges[0]), dotted.owner);
            let source_rows = if direction == Direction::BT {
                8..10
            } else {
                4..7
            };
            for y in source_rows {
                dotted.canvas.set_owned(
                    7,
                    y,
                    style.dotted_v,
                    CellOwnerKind::EdgeSegment,
                    &dotted.owner,
                    4,
                );
                assert_eq!(dotted.canvas.get(7, y), '╎');
                let meta = dotted.canvas.get_meta(7, y).unwrap();
                assert_eq!(meta.ch, style.dotted_v);
                assert_eq!(meta.owner_kind, CellOwnerKind::EdgeSegment);
                assert_eq!(
                    meta.role,
                    CellRole::Text,
                    "inferred by actual Canvas writer"
                );
                assert_eq!(meta.owner_id.as_deref(), Some(dotted.owner.as_str()));
            }
            assert_entry_route(&dotted);
            let row = dotted.label_y;
            assert!(!convergent_label_slot_is_writable(
                &dotted.canvas,
                &dotted.graph,
                0,
                row,
                LABEL,
                style,
                &dotted.owner,
                false,
            ));
            dotted.canvas.set_owned(
                30,
                row,
                'W',
                CellOwnerKind::NodeLabel,
                "untouched-wordmark",
                9,
            );
            dotted.canvas.set_inferred(30, row, '\u{301}');
            let wordmark_meta = dotted.canvas.get_meta(30, row).unwrap().clone();
            let wordmark = convergent_positioned_text(&dotted.canvas, 30, 31, row).unwrap();
            assert_eq!(wordmark, "W\u{301}");
            let mut context = ConvergentLabelContext::default();
            let before_probe = snapshot(&dotted.canvas);
            let before_context = format!("{context:?}");
            let proof = prove_convergent_path(
                &dotted.canvas,
                &dotted.graph,
                &dotted.from,
                &dotted.to,
                direction,
                style,
                &dotted.owner,
                &context,
            )
            .expect("actual owned dotted Text shaft must prove its real target before publication");
            assert_eq!(proof.turn_y, 7);
            assert_eq!(snapshot(&dotted.canvas), before_probe);
            assert_eq!(format!("{context:?}"), before_context);
            let before_route = dotted.canvas.explicit_edge_meta();
            let placement = draw_convergent_edge_label(
                &mut dotted.canvas,
                &dotted.from,
                &dotted.to,
                LABEL,
                direction,
                style,
                &Config::default(),
                0,
                &dotted.edge,
                &dotted.graph,
                &mut context,
            )
            .expect("proved dotted stem must publish its complete safe opposite-side fallback");
            assert_complete_entry(&dotted, &placement);
            assert_eq!(
                placement.cells,
                (9..16).map(|x| (x, row)).collect::<Vec<_>>()
            );
            assert_eq!(
                convergent_positioned_text(&dotted.canvas, 9, 16, row).as_deref(),
                Some(LABEL)
            );
            for (x, y, meta) in before_route {
                assert_eq!(dotted.canvas.get(x, y), meta.ch, "protected route {x}/{y}");
                assert_eq!(dotted.canvas.get_meta(x, y), Some(&meta));
            }
            assert_eq!(dotted.canvas.get_meta(30, row), Some(&wordmark_meta));
            assert_eq!(
                convergent_positioned_text(&dotted.canvas, 30, 31, row).as_deref(),
                Some(wordmark.as_str())
            );
        }
    }

    // Append these helpers and u14.rs/u15.rs inside the existing labels tests.
    // Reuses node(), route_cell(), horizontal(), snapshot(), and existing imports.
    // No receipt baseline is manually constructed: commit + record or draw creates it.

    // Append inside the existing labels tests, alongside admitted U16 and F's
    // H2PublicationFixture helpers. No new test function is introduced here.

    #[allow(clippy::too_many_arguments)]
    fn h2_c_collector_commit(
        f: &mut H2TypedPathFixture,
        start: usize,
        y: usize,
        label: &str,
        annotation: bool,
        owner_override: Option<&str>,
        expected_proof: bool,
        expected_success: bool,
        witness: &str,
    ) -> Option<Vec<(usize, usize)>> {
        let owner = owner_override.unwrap_or(&f.owner).to_owned();
        let context = ConvergentLabelContext::default();
        let before_context = context.clone();
        let before = snapshot(&f.canvas);
        let proof = prove_convergent_path(
            &f.canvas,
            &f.graph,
            &f.from,
            &f.to,
            f.direction,
            &f.style,
            &owner,
            &context,
        );
        assert_eq!(
            proof.is_some(),
            expected_proof,
            "{witness}: path prerequisite"
        );
        if let Some(proof) = &proof {
            assert_eq!(proof.turn_y, f.point(7, 7).1, "{witness}: actual turn");
        }
        assert_eq!(snapshot(&f.canvas), before, "{witness}: pure proof");
        assert_eq!(context, before_context, "{witness}: pure proof context");
        let site = ConvergentLabelSite {
            graph: &f.graph,
            from: &f.from,
            to: &f.to,
            direction: f.direction,
            style: &f.style,
            owner_id: &owner,
            label,
            context: &context,
            proof,
            valid: context.valid_indices(&f.canvas, &f.graph, &f.style),
        };
        let result = commit_convergent_site(&mut f.canvas, &site, start, y, annotation, false);
        assert_eq!(
            result.is_some(),
            expected_success,
            "{witness}: actual Site commit"
        );
        assert_eq!(context, before_context, "{witness}: immutable Site context");
        let Some(publication) = result else {
            assert_eq!(
                snapshot(&f.canvas),
                before,
                "{witness}: full None atomicity"
            );
            return None;
        };
        let width = convergent_label_supported_width(label).unwrap();
        assert_eq!(publication.start, start);
        assert_eq!(publication.end, start + width);
        assert_eq!(publication.y, y);
        assert_eq!(
            publication.cells,
            (start..start + width).map(|x| (x, y)).collect::<Vec<_>>()
        );
        assert_eq!(
            convergent_positioned_text(&f.canvas, start, start + width, y).as_deref(),
            Some(label)
        );
        assert!(
            publication.proof.is_some(),
            "{witness}: publication carries the proved run"
        );
        assert_eq!(
            publication.underlay.len(),
            width,
            "{witness}: actual horizontal underlay"
        );
        Some(publication.cells)
    }

    // A bounded unit fixture for the observed mTLS seam: BT FEp→Catp is Thick,
    // Istiod→Catp is Dotted, while their actual shared collector remains thin.
    // It reproduces that producer/consumer style mixture, not the whole mesh layout.
    fn h2_c_mtls_fixture(base: BaseStyle) -> H2TypedPathFixture {
        let mut f = h2_typed_path_fixture(Direction::BT, base, false, 7);
        let old_owner = f.owner.clone();
        f.from.id = "FEp".to_owned();
        f.to.id = "Catp".to_owned();
        f.graph.nodes[0] = f.from.clone();
        f.graph.nodes[1] = f.to.clone();
        f.graph.edges[0] = Edge::with_label("FEp", "Catp", "mTLS");
        f.graph.edges[0].kind = EdgeKind::Thick;
        f.owner = edge_owner_id(0, &f.graph.edges[0]);
        let owner = f.owner.clone();
        for (x, y, meta) in f.canvas.explicit_edge_meta() {
            assert_eq!(meta.owner_id.as_deref(), Some(old_owner.as_str()));
            route_cell(
                &mut f.canvas,
                x,
                y,
                meta.ch,
                &owner,
                meta.owner_kind,
                meta.role,
            );
        }
        let branch_style = edge_label_style(&f.style, &f.graph.edges[0]);
        for y in 4..7 {
            f.raw(7, y, branch_style.edge_v, &owner, CellRole::Vertical);
        }
        f.raw(20, 8, branch_style.edge_v, &owner, CellRole::Vertical);
        let peer_node = f.positioned_node("Istiod", 18, 1);
        f.graph.add_node(peer_node);
        let mut peer_edge = Edge::new("Istiod", "Catp");
        peer_edge.kind = EdgeKind::Dotted;
        let peer = edge_owner_id(1, &peer_edge);
        f.graph.add_edge(peer_edge);
        for y in 4..7 {
            let (x, y) = f.point(18, y);
            f.canvas
                .set_owned(x, y, f.style.dotted_v, CellOwnerKind::EdgeSegment, &peer, 4);
        }
        let collector = "fanin:Catp";
        for x in 8..20 {
            f.put(x, 7, '─', collector, CellRole::Horizontal);
        }
        f.put(18, 7, '┴', collector, CellRole::Junction);
        for x in 11..17 {
            let (x, y) = f.point(x, 7);
            assert_eq!(f.canvas.get(x, y), f.style.edge_h);
            let meta = f.canvas.get_meta(x, y).unwrap();
            assert_eq!(meta.role, CellRole::Horizontal);
            assert_eq!(meta.owner_kind, CellOwnerKind::EdgeSegment);
            assert_eq!(meta.owner_id.as_deref(), Some(collector));
        }
        f.check(
            7,
            Some(7),
            "mTLS mixed-style path reaches its actual Catp head",
        );
        f
    }

    fn h2_c_draw_override(
        f: &mut H2PublicationFixture,
        index: usize,
        label: &str,
    ) -> Option<EdgeLabelPlacement> {
        let edge = f.graph.edges[index].clone();
        let from = f.graph.get_node(&edge.from).unwrap().clone();
        let to = f.graph.get_node(&edge.to).unwrap().clone();
        let config = Config {
            max_edge_label_width: f.context.limit + 2,
            ..Config::default()
        };
        let before = snapshot(&f.canvas);
        let before_context = f.context.clone();
        let result = draw_convergent_edge_label(
            &mut f.canvas,
            &from,
            &to,
            label,
            f.graph.direction,
            &f.style,
            &config,
            index,
            &edge,
            &f.graph,
            &mut f.context,
        );
        if result.is_none() {
            assert_eq!(
                snapshot(&f.canvas),
                before,
                "actual draw None changed full Canvas"
            );
            assert_eq!(
                f.context, before_context,
                "actual draw None changed full context"
            );
        }
        result
    }

    #[derive(Clone)]
    struct H2PublicationFixture {
        canvas: Canvas,
        graph: Graph,
        style: StyleChars,
        context: ConvergentLabelContext,
        base_width: usize,
        base_height: usize,
        mirror: bool,
        dx: usize,
        dy: usize,
        frame: Vec<ConvergentCellWitness>,
    }

    impl H2PublicationFixture {
        fn point(&self, x: usize, y: usize) -> (usize, usize) {
            (
                self.dx
                    + if self.mirror {
                        self.base_width - 1 - x
                    } else {
                        x
                    },
                self.dy
                    + if self.graph.direction == Direction::BT {
                        self.base_height - 1 - y
                    } else {
                        y
                    },
            )
        }

        fn make_node(&self, id: &str, x: usize, y: usize, width: usize) -> Node {
            // Even-width Node::center_x is right-biased. Reflect the actual port,
            // then position the unchanged-width rectangle around that port.
            let center = self.dx
                + if self.mirror {
                    self.base_width - 1 - (x + width / 2)
                } else {
                    x + width / 2
                };
            node(
                id,
                center - width / 2,
                self.dy
                    + if self.graph.direction == Direction::BT {
                        self.base_height - y - 3
                    } else {
                        y
                    },
                width,
            )
        }

        fn glyph(&self, reference: char) -> char {
            let vertical = if self.graph.direction == Direction::BT {
                match reference {
                    '└' => '┌',
                    '┘' => '┐',
                    '┌' => '└',
                    '┐' => '┘',
                    '┬' => '┴',
                    '┴' => '┬',
                    '↓' => '↑',
                    other => other,
                }
            } else {
                reference
            };
            let reflected = if self.mirror {
                match vertical {
                    '└' => '┘',
                    '┘' => '└',
                    '┌' => '┐',
                    '┐' => '┌',
                    '├' => '┤',
                    '┤' => '├',
                    other => other,
                }
            } else {
                vertical
            };
            match reflected {
                '│' => self.style.edge_v,
                '─' => self.style.edge_h,
                // Public StyleChars uses corner_ul=└, corner_dl=┌.
                '└' => self.style.corner_ul,
                '┘' => self.style.corner_ur,
                '┌' => self.style.corner_dl,
                '┐' => self.style.corner_dr,
                '┬' => self.style.junction_down,
                '┴' => self.style.junction_up,
                '├' => self.style.junction_right,
                '┤' => self.style.junction_left,
                '↓' => self.style.arrow_down,
                '↑' => self.style.arrow_up,
                other => panic!("unexpected H2 publication reference {other:?}"),
            }
        }

        fn owner(&self, edge: usize) -> String {
            edge_owner_id(edge, &self.graph.edges[edge])
        }

        fn put(&mut self, x: usize, y: usize, reference: char, owner: &str, role: CellRole) {
            let (x, y) = self.point(x, y);
            let glyph = self.glyph(reference);
            route_cell(
                &mut self.canvas,
                x,
                y,
                glyph,
                owner,
                if role == CellRole::ArrowTip {
                    CellOwnerKind::ArrowHead
                } else {
                    CellOwnerKind::EdgeSegment
                },
                role,
            );
        }

        fn proof(&self, index: usize) -> Option<ConvergentPathProof> {
            let edge = &self.graph.edges[index];
            prove_convergent_path(
                &self.canvas,
                &self.graph,
                self.graph.get_node(&edge.from).unwrap(),
                self.graph.get_node(&edge.to).unwrap(),
                self.graph.direction,
                &self.style,
                &self.owner(index),
                &self.context,
            )
        }

        // Every actual None is checked for Canvas AND context atomicity.
        fn commit_at(
            &mut self,
            index: usize,
            start: usize,
            y: usize,
            annotation: bool,
            structured: bool,
            override_label: Option<&str>,
        ) -> Option<Vec<(usize, usize)>> {
            let edge = self.graph.edges[index].clone();
            let from = self.graph.get_node(&edge.from).unwrap().clone();
            let to = self.graph.get_node(&edge.to).unwrap().clone();
            let owner = self.owner(index);
            let label = override_label.map(str::to_owned).unwrap_or_else(|| {
                format_edge_label_with_limit(edge.label.as_deref().unwrap(), self.context.limit)
            });
            let proof = self.proof(index);
            let valid = self
                .context
                .valid_indices(&self.canvas, &self.graph, &self.style);
            let before = snapshot(&self.canvas);
            let before_context = self.context.clone();
            let site = ConvergentLabelSite {
                graph: &self.graph,
                from: &from,
                to: &to,
                direction: self.graph.direction,
                style: &self.style,
                owner_id: &owner,
                label: &label,
                context: &self.context,
                proof,
                valid,
            };
            let publication =
                commit_convergent_site(&mut self.canvas, &site, start, y, annotation, structured);
            let Some(publication) = publication else {
                assert_eq!(snapshot(&self.canvas), before, "None changed Canvas");
                assert_eq!(self.context, before_context, "None changed receipt context");
                return None;
            };
            let cells = publication.cells.clone();
            self.context.record_publication(
                &self.canvas,
                &from,
                &to,
                self.graph.direction,
                owner,
                label,
                publication,
            );
            Some(cells)
        }

        fn draw_edge(&mut self, index: usize) -> Option<EdgeLabelPlacement> {
            let edge = self.graph.edges[index].clone();
            let from = self.graph.get_node(&edge.from).unwrap().clone();
            let to = self.graph.get_node(&edge.to).unwrap().clone();
            let config = Config {
                max_edge_label_width: self.context.limit + 2,
                ..Config::default()
            };
            let before = snapshot(&self.canvas);
            let before_context = self.context.clone();
            let result = draw_convergent_edge_label(
                &mut self.canvas,
                &from,
                &to,
                edge.label.as_deref().unwrap(),
                self.graph.direction,
                &self.style,
                &config,
                index,
                &edge,
                &self.graph,
                &mut self.context,
            );
            if result.is_none() {
                assert_eq!(snapshot(&self.canvas), before, "draw None changed Canvas");
                assert_eq!(self.context, before_context, "draw None changed context");
            }
            result
        }

        fn assert_receipt(&self, sequence: usize, edge: usize) {
            let receipt = &self.context.receipts[sequence];
            assert_eq!(receipt.sequence, sequence);
            assert_eq!(receipt.owner_id, self.owner(edge));
            assert_eq!(receipt.from_id, self.graph.edges[edge].from);
            assert_eq!(receipt.target_id, self.graph.edges[edge].to);
            assert!(!receipt.proof.reads.is_empty());
            assert!(receipt.proof.dependencies.iter().all(|&p| p < sequence));
            assert_eq!(
                convergent_positioned_text(&self.canvas, receipt.start, receipt.end, receipt.y),
                Some(receipt.label.clone()),
            );
            assert!(self
                .context
                .valid_indices(&self.canvas, &self.graph, &self.style)
                .contains(&sequence));
        }

        fn assert_frame(&self) {
            assert!(!self.frame.is_empty());
            assert!(self.frame.iter().all(|cell| cell.is_current(&self.canvas)));
        }

        fn legacy_text(
            &mut self,
            owner: &str,
            start: usize,
            y: usize,
            label: &str,
        ) -> EdgeLabelPlacement {
            let mut cells = Vec::new();
            let mut cursor = start;
            for ch in label.chars() {
                assert!(
                    display_char_width(ch) > 0,
                    "legacy witness uses simple complete text"
                );
                set_label_char(&mut self.canvas, &mut cells, cursor, y, ch);
                cursor += display_char_width(ch);
            }
            build_label_placement(owner.to_owned(), cells).unwrap()
        }
    }

    fn h2_shared_publication_fixture(
        base: BaseStyle,
        label_a: &str,
        label_b: &str,
    ) -> H2PublicationFixture {
        let mut f = H2PublicationFixture {
            canvas: Canvas::new(48, 18),
            graph: Graph::new(),
            style: *base.chars(),
            context: ConvergentLabelContext::default(),
            base_width: 48,
            base_height: 18,
            mirror: false,
            dx: 0,
            dy: 0,
            frame: Vec::new(),
        };
        f.graph.direction = Direction::TD;
        for n in [
            node("A", 5, 1, 5),
            node("B", 0, 1, 5),
            node("Target", 28, 10, 5),
            node("Future", 32, 12, 5),
        ] {
            f.graph.add_node(n);
        }
        // Both future producers and the future reserved node exist before A publishes.
        f.graph.add_edge(Edge::with_label("A", "Target", label_a));
        f.graph.add_edge(Edge::with_label("B", "Target", label_b));
        let a = f.owner(0);
        let b = f.owner(1);
        let collector = "fanin:Target";
        for y in 4..7 {
            f.put(7, y, '│', &a, CellRole::Vertical);
            f.put(2, y, '│', &b, CellRole::Vertical);
        }
        f.put(2, 7, '└', &b, CellRole::Corner);
        for x in 3..30 {
            f.put(x, 7, '─', collector, CellRole::Horizontal);
        }
        f.put(7, 7, '┴', &a, CellRole::Junction);
        f.put(30, 7, '┐', collector, CellRole::Corner);
        f.put(30, 8, '│', collector, CellRole::Vertical);
        f.put(30, 9, '↓', collector, CellRole::ArrowTip);
        f.context.prepare(&f.graph, &f.style, 40);
        assert_eq!(f.proof(0).unwrap().turn_y, 7);
        assert_eq!(f.proof(1).unwrap().turn_y, 7);
        assert_eq!(f.canvas.get(7, 7), f.style.junction_up);
        assert_eq!(f.canvas.get_meta(7, 7).unwrap().role, CellRole::Junction);
        f
    }

    #[derive(Clone, Copy, Debug)]
    enum H2SiblingVariant {
        Normal,
        SameSource,
        WrongTarget,
        Back,
        Third,
    }

    #[allow(clippy::too_many_arguments)]
    fn h2_sibling_fixture(
        direction: Direction,
        base: BaseStyle,
        mirror: bool,
        dx: usize,
        dy: usize,
        source_y: usize,
        label_b: &str,
        variant: H2SiblingVariant,
    ) -> H2PublicationFixture {
        let mut f = H2PublicationFixture {
            canvas: Canvas::new(81 + 2 * dx, 34 + 2 * dy),
            graph: Graph::new(),
            style: *base.chars(),
            context: ConvergentLabelContext::default(),
            base_width: 81,
            base_height: 34,
            mirror,
            dx,
            dy,
            frame: Vec::new(),
        };
        f.graph.direction = direction;
        for n in [
            f.make_node("A", 34, source_y, 16),
            f.make_node("B", 54, source_y, 16),
            f.make_node("Target", 44, 25, 16),
        ] {
            f.graph.add_node(n);
        }
        f.graph
            .add_edge(Edge::with_label("A", "Target", "ILM rollover"));
        let mut b = Edge::with_label(
            if matches!(variant, H2SiblingVariant::SameSource) {
                "A"
            } else {
                "B"
            },
            if matches!(variant, H2SiblingVariant::WrongTarget) {
                "Other"
            } else {
                "Target"
            },
            label_b,
        );
        b.is_back_edge = matches!(variant, H2SiblingVariant::Back);
        f.graph.add_edge(b);
        if matches!(variant, H2SiblingVariant::WrongTarget) {
            f.graph.add_node(f.make_node("Other", 45, 25, 16));
        }
        if matches!(variant, H2SiblingVariant::Third) {
            // Independent producer identity, declared before either publication.
            f.graph.add_edge(Edge::with_label("A", "Target", "x"));
        }
        let a = f.owner(0);
        let b = f.owner(1);
        for y in source_y + 3..21 {
            f.put(42, y, '│', &a, CellRole::Vertical);
            f.put(62, y, '│', &b, CellRole::Vertical);
            if matches!(variant, H2SiblingVariant::Back) {
                let (x, y) = f.point(62, y);
                route_cell(
                    &mut f.canvas,
                    x,
                    y,
                    f.style.edge_v,
                    &b,
                    CellOwnerKind::CycleEdge,
                    CellRole::Vertical,
                );
            }
        }
        f.put(42, 21, '└', &a, CellRole::Corner);
        for x in 43..62 {
            f.put(x, 21, '─', "fanin:Target", CellRole::Horizontal);
        }
        f.put(62, 21, '┘', &b, CellRole::Corner);
        f.put(52, 21, '┬', "fanin:Target", CellRole::Junction);
        for y in 22..24 {
            f.put(52, y, '│', &a, CellRole::Vertical);
        }
        f.put(52, 24, '↓', &a, CellRole::ArrowTip);
        if matches!(variant, H2SiblingVariant::WrongTarget) {
            // Independent actual headed target port. Target centers need not be ports.
            f.put(53, 21, '┌', &b, CellRole::Corner);
            for y in 22..24 {
                f.put(53, y, '│', &b, CellRole::Vertical);
            }
            f.put(53, 24, '↓', &b, CellRole::ArrowTip);
        }
        let mut frame = Subgraph::new("DataNodes", Some("Data Nodes".to_owned()));
        frame.bounds = Rectangle::new(
            dx + if mirror { 81 - 31 - 42 } else { 31 },
            dy + if direction == Direction::BT {
                34 - 10 - 20
            } else {
                10
            },
            42,
            20,
        );
        for y in 10..30 {
            for x in [31, 72] {
                let (x, y) = f.point(x, y);
                route_cell(
                    &mut f.canvas,
                    x,
                    y,
                    f.style.edge_v,
                    "DataNodes",
                    CellOwnerKind::SubgraphBorder,
                    CellRole::Border,
                );
            }
        }
        for x in 31..73 {
            for y in [10, 29] {
                let (x, y) = f.point(x, y);
                route_cell(
                    &mut f.canvas,
                    x,
                    y,
                    f.style.edge_h,
                    "DataNodes",
                    CellOwnerKind::SubgraphBorder,
                    CellRole::Border,
                );
            }
        }
        f.graph.add_subgraph(frame);
        for y in 10..30 {
            for x in 31..73 {
                if x == 31 || x == 72 || y == 10 || y == 29 {
                    let (x, y) = f.point(x, y);
                    f.frame
                        .push(ConvergentCellWitness::capture(&f.canvas, x, y).unwrap());
                }
            }
        }
        f.context.prepare(&f.graph, &f.style, 24);
        assert_eq!(f.proof(0).unwrap().turn_y, f.point(42, 21).1);
        assert!(
            f.proof(1).is_some(),
            "negative peer variants still have an actual candidate path"
        );
        f.assert_frame();
        f
    }

    #[test]
    fn label15_receipts_require_current_complete_publications() {
        for base in [BaseStyle::Ascii, BaseStyle::Unicode] {
            // A and B are declared before either publication. B's later physical
            // annotation lies on A's original path but has a disjoint footprint.
            let mut pair = h2_shared_publication_fixture(base, "队 e\u{301} x", "later");
            let initial_a = pair.proof(0).unwrap();
            assert!(initial_a.dependencies.is_empty());
            assert!(pair.commit_at(0, 10, 7, true, false, None).is_some());
            pair.assert_receipt(0, 0);
            let a = pair.context.receipts[0].clone();
            assert_eq!(a.label, "队 e\u{301} x");
            assert_eq!(a.end - a.start, 6);
            assert_eq!(a.post.len(), 6);
            assert_eq!(pair.canvas.get(11, 7), '\0');
            assert_eq!(pair.canvas.get(12, 7), ' ');
            assert_eq!(pair.canvas.get(14, 7), ' ');
            assert_eq!(a.underlay.len(), 6);
            assert!(a.underlay.iter().all(|c| c.raw == pair.style.edge_h
                && c.meta.role == CellRole::Horizontal
                && c.meta.owner_kind == CellOwnerKind::EdgeSegment
                && c.meta.owner_id.as_deref() == Some("fanin:Target")));
            assert!(a.underlay.iter().all(|c| c.x >= 10 && c.x < 16 && c.y == 7));
            let view = ConvergentProofView::new(&pair.canvas, [&pair.context.receipts[0]]);
            assert_eq!(view.peek(13, 7).unwrap().raw, pair.style.edge_h);
            assert!(
                !view.overlay.contains_key(&(9, 7)),
                "padding was never captured"
            );
            assert_eq!(
                view.peek(10, 5).unwrap().raw,
                ' ',
                "blank space is not a line"
            );
            let b_proof = pair
                .proof(1)
                .expect("B crosses only A's certified underlay");
            assert_eq!(b_proof.dependencies, vec![0]);
            let before_b = pair.canvas.clone();
            assert!(pair.commit_at(1, 22, 7, true, false, None).is_some());
            pair.assert_receipt(1, 1);
            pair.assert_receipt(0, 0);
            assert_eq!(
                pair.context
                    .valid_indices(&pair.canvas, &pair.graph, &pair.style),
                vec![0, 1]
            );
            let b = &pair.context.receipts[1];
            assert_eq!(b.underlay.len(), 5);
            assert!(a.end < b.start - 1, "full padded footprints are disjoint");
            assert!(
                b.underlay.iter().all(|c| c.is_current(&before_b)),
                "capture is real raw Canvas"
            );
            assert!(a
                .proof
                .reads
                .iter()
                .any(|c| c.x >= b.start && c.x < b.end && c.y == b.y));
            assert!(
                a.proof.reads.iter().any(|c| !c.is_current(&pair.canvas)),
                "later B really changes A's physical path reads"
            );

            // The same captured later annotation cannot excuse its own corruption.
            let mut invalid_b = pair.clone();
            invalid_b.canvas.set(23, 7, '?');
            assert!(invalid_b
                .context
                .valid_indices(&invalid_b.canvas, &invalid_b.graph, &invalid_b.style)
                .is_empty());

            // Complete current positioned identity, not matching substrings elsewhere.
            let mut single = h2_shared_publication_fixture(base, "队 e\u{301} x", "later");
            single.commit_at(0, 10, 7, true, false, None).unwrap();
            single.assert_receipt(0, 0);
            for (x, replacement) in [(10, '?'), (11, ' '), (12, '?'), (14, '?')] {
                let mut changed = single.clone();
                changed.canvas.set(x, 7, replacement);
                assert!(changed
                    .context
                    .valid_indices(&changed.canvas, &changed.graph, &changed.style)
                    .is_empty());
                assert!(
                    changed.proof(1).is_none(),
                    "invalid text must not virtualize underlay"
                );
            }
            let mut combining = single.clone();
            combining.canvas.set(13, 7, '\u{308}');
            assert_eq!(
                combining.canvas.get(13, 7),
                'e',
                "raw base stayed identical"
            );
            assert!(combining
                .context
                .valid_indices(&combining.canvas, &combining.graph, &combining.style)
                .is_empty());
            let mut marks = single.clone();
            marks.canvas.set_meta_only(
                13,
                7,
                CellOwnerKind::EdgeLabel,
                Some("legacy same positioned text"),
                CellRole::Text,
                4,
            );
            assert_eq!(
                convergent_positioned_text(&marks.canvas, 10, 16, 7).as_deref(),
                Some("队 e\u{301} x")
            );
            assert!(marks
                .context
                .valid_indices(&marks.canvas, &marks.graph, &marks.style)
                .is_empty());
            let mut relocated = single.clone();
            commit_convergent_label(&mut relocated.canvas, 10, 5, "队 e\u{301} x");
            relocated.canvas.set(10, 7, '?');
            assert_eq!(
                convergent_positioned_text(&relocated.canvas, 10, 16, 5).as_deref(),
                Some("队 e\u{301} x")
            );
            assert!(relocated
                .context
                .valid_indices(&relocated.canvas, &relocated.graph, &relocated.style)
                .is_empty());

            // Every graph/format mutation is independent of current label bytes.
            for mutation in 0..7 {
                let mut changed = single.clone();
                let mut style = changed.style;
                let mut limit = changed.context.limit;
                match mutation {
                    0 => changed.graph.nodes[0].x += 1,
                    1 => changed.graph.edges[0].to = "Future".to_owned(),
                    2 => changed.graph.edges[0].kind = EdgeKind::Thick,
                    3 => changed.graph.edges[0].label = Some("different".to_owned()),
                    4 => changed.graph.direction = Direction::BT,
                    5 => style.edge_h = '=',
                    6 => limit -= 1,
                    _ => unreachable!(),
                }
                let reservations = changed.context.reservations.clone();
                changed.context.prepare(&changed.graph, &style, limit);
                assert!(changed
                    .context
                    .valid_indices(&changed.canvas, &changed.graph, &style)
                    .is_empty());
                assert_eq!(changed.context.reservations, reservations);
            }

            // Currentness cannot introduce self/future or cyclic proof dependency.
            for illegal_dependency in [0, 1] {
                let mut attempt = h2_shared_publication_fixture(base, "queue", "later");
                let from = attempt.graph.get_node("A").unwrap().clone();
                let to = attempt.graph.get_node("Target").unwrap().clone();
                let owner = attempt.owner(0);
                let mut proof = attempt.proof(0).unwrap();
                assert!(proof.dependencies.is_empty());
                // Negative mutation of a REAL prepublication proof, never a
                // fabricated successful receipt or an accepted baseline.
                proof.dependencies.push(illegal_dependency);
                let site = ConvergentLabelSite {
                    graph: &attempt.graph,
                    from: &from,
                    to: &to,
                    direction: Direction::TD,
                    style: &attempt.style,
                    owner_id: &owner,
                    label: "queue",
                    context: &attempt.context,
                    proof: Some(proof),
                    valid: Vec::new(),
                };
                let publication =
                    commit_convergent_site(&mut attempt.canvas, &site, 10, 7, true, false).unwrap();
                let cells = publication.cells.clone();
                attempt.context.record_publication(
                    &attempt.canvas,
                    &from,
                    &to,
                    Direction::TD,
                    owner,
                    "queue".to_owned(),
                    publication,
                );
                assert!(
                    attempt.context.receipts.is_empty(),
                    "constructor refuses self/future dependencies"
                );
                assert_eq!(
                    attempt.context.reservations,
                    vec![cells],
                    "already successful output is reserved without becoming eligible"
                );
                assert!(attempt.proof(1).is_none());
            }
            for target in [0, 2] {
                let mut circular = single.clone();
                circular.context.receipts[0].proof.dependencies.push(target);
                assert!(circular
                    .context
                    .intrinsic_indices(&circular.canvas, &circular.graph, &circular.style)
                    .is_empty());
                assert!(circular.proof(1).is_none());
            }
            let mut cyclic = pair.clone();
            cyclic.context.receipts[0].proof.dependencies.push(1);
            assert!(cyclic
                .context
                .valid_indices(&cyclic.canvas, &cyclic.graph, &cyclic.style)
                .is_empty());
            let mut unpublished = h2_shared_publication_fixture(base, "队 e\u{301} x", "later");
            // Successful old writer output has no certified producer receipt.
            let unpublished_owner = unpublished.owner(0);
            horizontal(
                &mut unpublished.canvas,
                9,
                17,
                7,
                &unpublished.style,
                &unpublished_owner,
                CellOwnerKind::EdgeSegment,
            );
            assert!(unpublished.proof(1).is_some());
            let cells = try_publish_convergent_label(
                &mut unpublished.canvas,
                &unpublished.graph,
                10,
                7,
                "队 e\u{301} x",
                &unpublished.style,
                &unpublished_owner,
                true,
            )
            .unwrap();
            let generic = build_label_placement(unpublished.owner(0), cells).unwrap();
            unpublished.context.observe_legacy(Some(&generic));
            assert!(unpublished.context.receipts.is_empty());
            assert!(
                unpublished.proof(1).is_none(),
                "unpublished/generic text cannot construct B proof"
            );

            for mutation in 0..3 {
                let mut forged_underlay = single.clone();
                let cell = &mut forged_underlay.context.receipts[0].underlay[0];
                match mutation {
                    0 => cell.raw = ' ',
                    1 => cell.meta.role = CellRole::Vertical,
                    2 => cell.meta.owner_id = Some("foreign".to_owned()),
                    _ => unreachable!(),
                }
                assert!(forged_underlay
                    .context
                    .valid_indices(
                        &forged_underlay.canvas,
                        &forged_underlay.graph,
                        &forged_underlay.style
                    )
                    .is_empty());
                assert!(forged_underlay.proof(1).is_none());
            }

            // Blank-side publication may reserve/prove attribution but supplies no line.
            let mut side = h2_shared_publication_fixture(base, "队 e\u{301} x", "later");
            side.commit_at(0, 10, 5, false, false, None).unwrap();
            side.assert_receipt(0, 0);
            assert!(side.context.receipts[0].underlay.is_empty());
            let view = ConvergentProofView::new(&side.canvas, [&side.context.receipts[0]]);
            assert!(view.overlay.is_empty());
            assert_eq!(view.peek(10, 5).unwrap().raw, '队');
            side.canvas.set(12, 7, ' ');
            assert!(
                side.proof(1).is_none(),
                "blank-side receipt must not bridge a real route gap"
            );

            let mut refused_legacy = h2_shared_publication_fixture(base, "queue", "later");
            route_cell(
                &mut refused_legacy.canvas,
                11,
                5,
                refused_legacy.style.cross_end,
                "foreign crossing",
                CellOwnerKind::EdgeSegment,
                CellRole::Junction,
            );
            let owner_b = refused_legacy.owner(1);
            let partial_placement = refused_legacy.legacy_text(&owner_b, 10, 5, "abc");
            assert_eq!(
                refused_legacy.canvas.get(11, 5),
                refused_legacy.style.cross_end
            );
            assert!(
                partial_placement.cells.contains(&(11, 5)),
                "generic legacy placement records even the refused crossing cell"
            );
            refused_legacy
                .context
                .observe_legacy(Some(&partial_placement));
            assert!(
                refused_legacy.context.receipts.is_empty(),
                "refused/partial generic output is never a certified receipt"
            );

            // Safe output with no canonical source proof reserves but is not eligible.
            let mut unproved = h2_shared_publication_fixture(base, "queue", "later");
            unproved.canvas.set(7, 4, ' ');
            assert!(unproved.proof(0).is_none());
            unproved.commit_at(0, 10, 5, false, false, None).unwrap();
            assert!(unproved.context.receipts.is_empty());
            assert!(!unproved.context.reservations.is_empty());
            assert!(
                unproved
                    .commit_at(0, 33, 12, false, false, Some("z"))
                    .is_none(),
                "future node is already reserved"
            );

            // Same visible bytes, raw metadata and marks still do not preserve A's
            // ownership after a real interleaved legacy B writer claims them.
            let mut reused = h2_shared_publication_fixture(base, "route note", "later");
            reused.commit_at(0, 10, 7, true, false, None).unwrap();
            reused.assert_receipt(0, 0);
            let before_reuse = snapshot(&reused.canvas);
            let reservations = reused.context.reservations.clone();
            let owner_b = reused.owner(1);
            let placement = reused.legacy_text(&owner_b, 10, 7, "route note");
            assert_eq!(
                snapshot(&reused.canvas),
                before_reuse,
                "same-text overwrite is physically indistinguishable"
            );
            reused.context.observe_legacy(Some(&placement));
            assert!(reused
                .context
                .valid_indices(&reused.canvas, &reused.graph, &reused.style)
                .is_empty());
            assert_eq!(
                &reused.context.reservations[..reservations.len()],
                reservations.as_slice()
            );
            assert!(reused.proof(1).is_none());

            // Nonoverlap Some retains A only while ALL proof reads remain current.
            for mutation in 0..5 {
                let mut legacy = single.clone();
                let placement = legacy.legacy_text("legacy:B", 38, 5, "old");
                legacy.context.observe_legacy(Some(&placement));
                legacy.assert_receipt(0, 0);
                assert!(legacy.context.receipts[0]
                    .proof
                    .reads
                    .iter()
                    .any(|c| c.x == 7 && c.y == 5));
                assert!(!placement.cells.contains(&(7, 5)));
                let a_owner = legacy.owner(0);
                match mutation {
                    0 => legacy.canvas.set(7, 5, ' '),
                    1 => route_cell(
                        &mut legacy.canvas,
                        7,
                        5,
                        legacy.style.edge_v,
                        "foreign",
                        CellOwnerKind::EdgeSegment,
                        CellRole::Vertical,
                    ),
                    2 => route_cell(
                        &mut legacy.canvas,
                        7,
                        5,
                        legacy.style.edge_v,
                        &a_owner,
                        CellOwnerKind::EdgeSegment,
                        CellRole::Horizontal,
                    ),
                    3 => {
                        legacy.canvas.set_write_stage("legacy hidden shaft");
                        route_cell(
                            &mut legacy.canvas,
                            7,
                            5,
                            legacy.style.edge_v,
                            &a_owner,
                            CellOwnerKind::EdgeSegment,
                            CellRole::Vertical,
                        );
                    }
                    4 => legacy.canvas.set_meta_only(
                        7,
                        5,
                        CellOwnerKind::CycleEdge,
                        Some(&a_owner),
                        CellRole::Vertical,
                        4,
                    ),
                    _ => unreachable!(),
                }
                legacy.context.observe_legacy(Some(&placement));
                assert!(legacy
                    .context
                    .valid_indices(&legacy.canvas, &legacy.graph, &legacy.style)
                    .is_empty());
            }
            let mut partial = single.clone();
            let reserved = partial.context.reservations.clone();
            partial.canvas.set(40, 5, 'p'); // real partial old-writer mutation, no placement
            partial.context.observe_legacy(None);
            assert!(partial
                .context
                .valid_indices(&partial.canvas, &partial.graph, &partial.style)
                .is_empty());
            assert_eq!(partial.context.reservations, reserved);

            // Every word-space cell is protected even after receipt revocation.
            let long_label = "a               b";
            let mut spaces = h2_shared_publication_fixture(base, long_label, "later");
            spaces.commit_at(0, 9, 7, true, false, None).unwrap();
            spaces.assert_receipt(0, 0);
            assert!((16..19).all(|x| spaces.canvas.get(x, 7) == ' '
                && spaces.canvas.get_meta(x, 7).unwrap().owner_kind == CellOwnerKind::Empty));
            let mut without_reservation = spaces.clone();
            without_reservation.context.reservations.clear();
            assert!(
                without_reservation
                    .commit_at(0, 17, 7, false, false, Some("z"))
                    .is_some(),
                "positive isolation control: raw guards alone accept this internal blank gap"
            );
            assert!(spaces
                .commit_at(0, 17, 7, false, false, Some("z"))
                .is_none());
            spaces.context.observe_legacy(None);
            assert!(spaces
                .context
                .valid_indices(&spaces.canvas, &spaces.graph, &spaces.style)
                .is_empty());
            assert!(
                spaces
                    .commit_at(0, 17, 7, false, false, Some("z"))
                    .is_none(),
                "revocation retains complete word-space reservations"
            );
        }
    }

    #[test]
    fn label15_structured_sibling_adjacency_is_last_resort() {
        for direction in [Direction::TD, Direction::TB, Direction::BT] {
            for base in [BaseStyle::Ascii, BaseStyle::Unicode] {
                for mirror in [false, true] {
                    for (dx, dy) in [(0, 0), (3, 2)] {
                        for label_b in ["ILM rollover", "move archive", "trace events"] {
                            let mut pair = h2_sibling_fixture(
                                direction,
                                base,
                                mirror,
                                dx,
                                dy,
                                16,
                                label_b,
                                H2SiblingVariant::Normal,
                            );
                            pair.draw_edge(0).expect("A has a strict safe slot");
                            pair.assert_receipt(0, 0);
                            let a = pair.context.receipts[0].clone();
                            let proof_b = pair.proof(1).unwrap();
                            assert!(
                                proof_b.source_x.abs_diff(proof_b.target_x) - 1 < 12,
                                "actual shared run is too short for the whole annotation"
                            );
                            for y in proof_b.stem_start.min(proof_b.turn_y)
                                ..=proof_b.stem_start.max(proof_b.turn_y)
                            {
                                for start in [
                                    proof_b.source_x.checked_sub(14),
                                    proof_b.source_x.checked_add(2),
                                ]
                                .into_iter()
                                .flatten()
                                {
                                    let mut strict_probe = pair.clone();
                                    assert!(
                                        strict_probe
                                            .commit_at(1, start, y, false, false, None)
                                            .is_none(),
                                        "all bounded stem-side strict candidates are exhausted"
                                    );
                                }
                            }
                            pair.draw_edge(1).expect(
                                "B must not be omitted when its named sibling slot is feasible",
                            );
                            pair.assert_receipt(1, 1);
                            pair.assert_receipt(0, 0);
                            pair.assert_frame();
                            let b = &pair.context.receipts[1];
                            assert_eq!(a.label, "ILM rollover");
                            assert_eq!(b.label, label_b);
                            assert_eq!(a.end - a.start, 12);
                            assert_eq!(b.end - b.start, 12);
                            assert_eq!(a.y.abs_diff(b.y), 1);
                            assert_ne!(a.from_id, b.from_id);
                            assert_eq!(a.target_id, b.target_id);
                            assert_ne!(a.proof.source_x, b.proof.source_x);
                            let a_gap = ConvergentLabelSite::span_distance(
                                a.start,
                                a.end,
                                a.proof.source_x,
                            );
                            let b_gap = ConvergentLabelSite::span_distance(
                                b.start,
                                b.end,
                                b.proof.source_x,
                            );
                            assert!(
                                a_gap
                                    < ConvergentLabelSite::span_distance(
                                        a.start,
                                        a.end,
                                        b.proof.source_x
                                    )
                            );
                            assert!(
                                b_gap
                                    < ConvergentLabelSite::span_distance(
                                        b.start,
                                        b.end,
                                        a.proof.source_x
                                    )
                            );
                            for row in [a.y, b.y] {
                                for receipt in [&a, b] {
                                    let stem = receipt.proof.source_x;
                                    assert_eq!(
                                        pair.canvas.get_meta(stem, row).unwrap().role,
                                        CellRole::Vertical
                                    );
                                    assert_eq!(pair.canvas.get(stem, row), pair.style.edge_v);
                                }
                            }
                            assert!(pair.proof(0).is_some());
                            assert!(pair.proof(1).is_some());
                            let overlap = a.end.min(b.end).saturating_sub(a.start.max(b.start));
                            assert!(overlap > 0, "this is the intended adjacent overlap case");
                            if direction != Direction::BT && !mirror && dx == 0 && dy == 0 {
                                assert_eq!((a.start, a.end, a.y), (44, 56, 20));
                                assert_eq!((b.start, b.end, b.y), (48, 60, 19));
                                assert_eq!(overlap, 8);
                                assert_eq!(pair.canvas.get(72, 19), pair.style.edge_v);
                                assert_eq!(pair.canvas.get(72, 20), pair.style.edge_v);
                            }
                            // Structural unit evidence only. The full adjacent pair
                            // still needs its separately admitted fresh readability review.
                        }

                        // An available strict row wins despite a closer structured row.
                        let mut strict = h2_sibling_fixture(
                            direction,
                            base,
                            mirror,
                            dx,
                            dy,
                            14,
                            "move archive",
                            H2SiblingVariant::Normal,
                        );
                        strict.draw_edge(0).unwrap();
                        strict.assert_receipt(0, 0);
                        let a = strict.context.receipts[0].clone();
                        assert_eq!(a.y, strict.point(42, 18).1);
                        strict.draw_edge(1).unwrap();
                        strict.assert_receipt(1, 1);
                        let b = &strict.context.receipts[1];
                        assert_eq!(b.y, strict.point(62, 20).1);
                        assert_eq!(
                            a.y.abs_diff(b.y),
                            2,
                            "strict row20 wins before adjacent17/19"
                        );
                        strict.assert_frame();
                    }
                }
            }
        }

        // Isolated slot probes start with an actual complete A transaction and an
        // independently proved B path. Each returned None checks complete atomics.
        for base in [BaseStyle::Ascii, BaseStyle::Unicode] {
            let mut baseline = h2_sibling_fixture(
                Direction::TD,
                base,
                false,
                0,
                0,
                16,
                "move archive",
                H2SiblingVariant::Normal,
            );
            baseline.draw_edge(0).unwrap();
            baseline.assert_receipt(0, 0);
            assert!(baseline.proof(1).is_some());
            let mut positive = baseline.clone();
            assert!(positive.commit_at(1, 48, 19, false, true, None).is_some());
            positive.assert_receipt(1, 1);
            assert!(
                baseline.commit_at(1, 48, 19, false, false, None).is_none(),
                "otherwise identical strict probe is genuinely blocked only by adjacent A"
            );

            let mut unknown = baseline.clone();
            unknown.context.receipts.clear();
            assert!(
                unknown.proof(1).is_some(),
                "raw candidate route is unchanged"
            );
            assert!(unknown.commit_at(1, 48, 19, false, true, None).is_none());

            let mut partial = baseline.clone();
            let a = &partial.context.receipts[0];
            partial.canvas.set(a.start, a.y, '?');
            assert!(
                partial.proof(1).is_some(),
                "partial peer does not damage B's independent path"
            );
            assert!(partial.commit_at(1, 48, 19, false, true, None).is_none());

            let mut mixed = baseline.clone();
            mixed.canvas.set(58, 20, 'X');
            mixed.assert_receipt(0, 0);
            assert!(mixed.proof(1).is_some());
            assert!(
                mixed.commit_at(1, 48, 19, false, true, None).is_none(),
                "one complete peer cannot excuse additional unknown adjacent text"
            );

            let mut collision = baseline.clone();
            assert!(
                collision.commit_at(1, 45, 20, false, true, None).is_none(),
                "prior full footprint, including spaces, remains reserved"
            );

            let mut closer_other = baseline.clone();
            assert_eq!(ConvergentLabelSite::span_distance(45, 57, 62), 6);
            assert_eq!(ConvergentLabelSite::span_distance(45, 57, 42), 3);
            assert!(closer_other
                .commit_at(1, 45, 19, false, true, None)
                .is_none());
            let mut tied = h2_sibling_fixture(
                Direction::TD,
                base,
                false,
                0,
                0,
                16,
                "trace events!",
                H2SiblingVariant::Normal,
            );
            tied.draw_edge(0).unwrap();
            tied.assert_receipt(0, 0);
            assert!(tied.proof(1).is_some());
            assert_eq!(ConvergentLabelSite::span_distance(46, 59, 62), 4);
            assert_eq!(ConvergentLabelSite::span_distance(46, 59, 42), 4);
            let mut tie_control = tied.clone();
            assert!(
                tie_control
                    .commit_at(1, 48, 19, false, true, None)
                    .is_some(),
                "same complete 13-column setup has an untied feasible candidate"
            );
            assert!(
                tied.commit_at(1, 46, 19, false, true, None).is_none(),
                "exact endpoint-distance ties reject"
            );

            for (x, y) in [(62, 20), (42, 19)] {
                let mut missing_stem = baseline.clone();
                missing_stem.canvas.set(x, y, ' ');
                assert_eq!(missing_stem.canvas.get(x, y), ' ');
                assert!(
                    missing_stem
                        .commit_at(1, 48, 19, false, true, None)
                        .is_none(),
                    "both actual stems are required at both adjacent rows"
                );
            }

            for variant in [
                H2SiblingVariant::SameSource,
                H2SiblingVariant::WrongTarget,
                H2SiblingVariant::Back,
            ] {
                let mut invalid_peer = h2_sibling_fixture(
                    Direction::TD,
                    base,
                    false,
                    0,
                    0,
                    16,
                    "move archive",
                    variant,
                );
                // Variant graph is fixed BEFORE the real A receipt is created.
                invalid_peer.draw_edge(0).unwrap();
                invalid_peer.assert_receipt(0, 0);
                assert!(
                    invalid_peer.proof(1).is_some(),
                    "negative has complete actual candidate path"
                );
                assert!(
                    invalid_peer
                        .commit_at(1, 48, 19, false, true, None)
                        .is_none(),
                    "structured peer guard refuses {variant:?}"
                );
            }

            let mut multiple = h2_sibling_fixture(
                Direction::TD,
                base,
                false,
                0,
                0,
                16,
                "move archive",
                H2SiblingVariant::Third,
            );
            multiple.draw_edge(0).unwrap();
            multiple.assert_receipt(0, 0);
            assert!(multiple.commit_at(2, 58, 20, false, false, None).is_some());
            multiple.assert_receipt(1, 2);
            multiple.assert_receipt(0, 0);
            assert!(multiple.proof(1).is_some());
            assert!(
                multiple.commit_at(1, 48, 19, false, true, None).is_none(),
                "two independently certified neighbors cannot be treated as exactly one peer"
            );

            let mut stale_direction = baseline.clone();
            stale_direction.context.receipts[0].direction = Direction::BT;
            assert!(stale_direction
                .commit_at(1, 48, 19, false, true, None)
                .is_none());
            let mut word_space = baseline.clone();
            word_space.canvas.set(47, 20, '?'); // A's original ILM<space>rollover gap
            assert!(word_space.proof(1).is_some());
            assert!(word_space.commit_at(1, 48, 19, false, true, None).is_none());

            let mut wide_peer = h2_sibling_fixture(
                Direction::TD,
                base,
                false,
                0,
                0,
                16,
                "move archive",
                H2SiblingVariant::Normal,
            );
            wide_peer.graph.edges[0].label = Some("队 a b cdefg".to_owned());
            wide_peer
                .context
                .prepare(&wide_peer.graph, &wide_peer.style, 24);
            wide_peer.draw_edge(0).unwrap();
            wide_peer.assert_receipt(0, 0);
            assert_eq!(
                wide_peer.context.receipts[0].end - wide_peer.context.receipts[0].start,
                12
            );
            assert_eq!(wide_peer.canvas.get(45, 20), '\0');
            let mut wide_positive = wide_peer.clone();
            assert!(wide_positive
                .commit_at(1, 48, 19, false, true, None)
                .is_some());
            wide_peer.canvas.set(45, 20, ' ');
            assert!(wide_peer.proof(1).is_some());
            assert!(
                wide_peer.commit_at(1, 48, 19, false, true, None).is_none(),
                "a damaged continuation outside the overlapping text still stales the whole peer"
            );

            let mut graph_drift = baseline.clone();
            graph_drift.graph.nodes[0].x += 1;
            graph_drift
                .context
                .prepare(&graph_drift.graph, &graph_drift.style, 24);
            assert!(graph_drift.proof(1).is_some());
            assert!(graph_drift
                .commit_at(1, 48, 19, false, true, None)
                .is_none());
            let mut format_drift = baseline.clone();
            format_drift
                .context
                .prepare(&format_drift.graph, &format_drift.style, 23);
            assert!(format_drift.proof(1).is_some());
            assert!(format_drift
                .commit_at(1, 48, 19, false, true, None)
                .is_none());
        }
    }
    // Append inside the existing labels.rs #[cfg(test)] mod tests.
    // Reuses H2PublicationFixture, route_cell, node and actual receipt constructor.
    // No product runtime has been executed for this draft.

    fn h3_prefix_fixture(
        direction: Direction,
        base: BaseStyle,
        mirror: bool,
        width: usize,
        label: &str,
        peer_owned: bool,
    ) -> H2PublicationFixture {
        h3_prefix_fixture_at(direction, base, mirror, width, label, peer_owned, 0, 0)
    }

    #[allow(clippy::too_many_arguments)]
    fn h3_prefix_fixture_at(
        direction: Direction,
        base: BaseStyle,
        mirror: bool,
        width: usize,
        label: &str,
        peer_owned: bool,
        dx: usize,
        dy: usize,
    ) -> H2PublicationFixture {
        let mut f = H2PublicationFixture {
            canvas: Canvas::new(width + 2 * dx, 20 + 2 * dy),
            graph: Graph::new(),
            style: *base.chars(),
            context: ConvergentLabelContext::default(),
            base_width: 20,
            base_height: 20,
            mirror,
            dx,
            dy,
            frame: Vec::new(),
        };
        f.graph.direction = direction;
        for n in [
            f.make_node("Current", 1, 5, 5),
            f.make_node("Upstream", 1, 1, 5),
            f.make_node("Target", 1, 14, 5),
        ] {
            f.graph.add_node(n);
        }
        f.graph
            .add_edge(Edge::with_label("Current", "Target", label));
        f.graph
            .add_edge(Edge::with_label("Upstream", "Target", "peer"));
        let owner = f.owner(usize::from(peer_owned));
        for y in 8..13 {
            f.put(3, y, '│', &owner, CellRole::Vertical);
        }
        let current = f.owner(0);
        f.put(3, 13, '↓', &current, CellRole::ArrowTip);
        f.context.prepare(&f.graph, &f.style, 18.min(width));
        f
    }

    fn h3_with_site<R>(
        f: &mut H2PublicationFixture,
        edge_index: usize,
        operation: impl FnOnce(&mut Canvas, &ConvergentLabelSite<'_>) -> R,
    ) -> R {
        h3_with_site_override(f, edge_index, |_, _, _, _| {}, operation)
    }

    fn h3_with_site_override<R>(
        f: &mut H2PublicationFixture,
        edge_index: usize,
        override_site: impl FnOnce(&mut Node, &mut Node, &mut Direction, &mut String),
        operation: impl FnOnce(&mut Canvas, &ConvergentLabelSite<'_>) -> R,
    ) -> R {
        let edge = f.graph.edges[edge_index].clone();
        let mut from = f.graph.get_node(&edge.from).unwrap().clone();
        let mut to = f.graph.get_node(&edge.to).unwrap().clone();
        let mut owner = f.owner(edge_index);
        let mut direction = f.graph.direction;
        override_site(&mut from, &mut to, &mut direction, &mut owner);
        let label = format_edge_label_with_limit(
            edge.label.as_deref().unwrap(),
            f.context.limit.min(f.canvas.width),
        );
        let proof = prove_convergent_path(
            &f.canvas, &f.graph, &from, &to, direction, &f.style, &owner, &f.context,
        );
        let valid = f.context.valid_indices(&f.canvas, &f.graph, &f.style);
        let site = ConvergentLabelSite {
            graph: &f.graph,
            from: &from,
            to: &to,
            direction,
            style: &f.style,
            owner_id: &owner,
            label: &label,
            context: &f.context,
            proof,
            valid,
        };
        operation(&mut f.canvas, &site)
    }

    fn h3_prefix(f: &mut H2PublicationFixture) -> Option<ConvergentSourcePrefix> {
        let before = f.canvas.clone();
        let context = f.context.clone();
        let result = h3_with_site(f, 0, |canvas, site| convergent_source_prefix(canvas, site));
        assert_eq!(f.canvas, before, "prefix probe changed complete Canvas");
        assert_eq!(f.context, context, "prefix probe changed context");
        result
    }

    fn h3_commit(
        f: &mut H2PublicationFixture,
        index: usize,
        start: usize,
        y: usize,
        cap: usize,
    ) -> Option<ConvergentPublication> {
        let before = f.canvas.clone();
        let context = f.context.clone();
        let result = h3_with_site(f, index, |canvas, site| {
            commit_convergent_source_prefix(canvas, site, start, y, cap)
        });
        assert_eq!(
            f.context, context,
            "private commit changed borrowed context"
        );
        if result.is_none() {
            assert_eq!(f.canvas, before, "weak None changed complete Canvas");
        }
        result
    }

    fn h3_publish(
        f: &mut H2PublicationFixture,
        index: usize,
        preferred_x: usize,
        preferred_y: usize,
        cap: usize,
    ) -> Option<ConvergentPublication> {
        h3_publish_override(f, index, preferred_x, preferred_y, cap, |_, _, _, _| {})
    }

    #[allow(clippy::too_many_arguments)]
    fn h3_publish_override(
        f: &mut H2PublicationFixture,
        index: usize,
        preferred_x: usize,
        preferred_y: usize,
        cap: usize,
        override_site: impl FnOnce(&mut Node, &mut Node, &mut Direction, &mut String),
    ) -> Option<ConvergentPublication> {
        let before = f.canvas.clone();
        let context = f.context.clone();
        let result = h3_with_site_override(f, index, override_site, |canvas, site| {
            assert!(
                !site.writable(canvas, preferred_x, preferred_y, false, false),
                "old preferred must be unavailable in a weak-branch witness"
            );
            publish_vertical_convergent_label(
                canvas,
                site.graph,
                site.from,
                site.to,
                site.direction,
                preferred_x,
                preferred_y,
                site.label,
                site.style,
                site.owner_id,
                false,
                site.style,
                site.context,
                cap,
            )
        });
        assert_eq!(f.context, context, "publisher changed borrowed context");
        if result.is_none() {
            assert_eq!(f.canvas, before, "publisher None changed complete Canvas");
        }
        result
    }

    fn h3_record(f: &mut H2PublicationFixture, index: usize, publication: ConvergentPublication) {
        let edge = f.graph.edges[index].clone();
        let from = f.graph.get_node(&edge.from).unwrap().clone();
        let to = f.graph.get_node(&edge.to).unwrap().clone();
        let owner = f.owner(index);
        let label = format_edge_label_with_limit(edge.label.as_deref().unwrap(), f.context.limit);
        assert_eq!(
            convergent_label_supported_width(&label),
            Some(publication.end - publication.start),
            "record helper must retain the exact already-published display width",
        );
        assert_eq!(
            convergent_positioned_text(
                &f.canvas,
                publication.start,
                publication.end,
                publication.y
            )
            .as_deref(),
            Some(label.as_str()),
            "record helper may not hide a within-attempt reformat/truncation mismatch",
        );
        f.context.record_publication(
            &f.canvas,
            &from,
            &to,
            f.graph.direction,
            owner,
            label,
            publication,
        );
    }

    fn h3_raw(
        f: &mut H2PublicationFixture,
        x: usize,
        y: usize,
        ch: char,
        owner: &str,
        kind: CellOwnerKind,
        role: CellRole,
    ) {
        let (x, y) = f.point(x, y);
        route_cell(&mut f.canvas, x, y, ch, owner, kind, role);
    }

    #[test]
    fn label15_source_prefix_stops_at_first_invalid_cell() {
        // Translation changes both the real Graph and raw cells before probing;
        // no hard-coded base-coordinate lookup may manufacture a later anchor.
        for direction in [Direction::TD, Direction::TB, Direction::BT] {
            for base in [BaseStyle::Ascii, BaseStyle::Unicode] {
                for mirror in [false, true] {
                    for (dx, dy) in [(4, 3), (9, 2)] {
                        let mut shifted =
                            h3_prefix_fixture_at(direction, base, mirror, 20, LABEL, false, dx, dy);
                        let expected: Vec<_> = (8..13).map(|y| shifted.point(3, y).1).collect();
                        assert!(shifted.proof(0).is_some());
                        let prefix = h3_prefix(&mut shifted).unwrap();
                        assert_eq!(prefix.rows, expected);
                        let (source, row) = shifted.point(3, 9);
                        assert_eq!(prefix.source_x, source);
                        let gap = shifted.point(3, 11);
                        shifted.canvas.set(gap.0, gap.1, ' ');
                        assert!(shifted.proof(0).is_none());
                        let cap = shifted.canvas.width;
                        let publication = h3_publish(&mut shifted, 0, source, row, cap).expect(
                            "translated reachable blank source-local slot remains feasible",
                        );
                        let expected_start = source
                            .checked_sub(display_width(LABEL) + 2)
                            .unwrap_or(source + 2);
                        assert_eq!((publication.start, publication.y), (expected_start, row));
                        assert!(publication.proof.is_none() && publication.underlay.is_empty());
                        assert_eq!(
                            convergent_positioned_text(
                                &shifted.canvas,
                                publication.start,
                                publication.end,
                                row,
                            )
                            .as_deref(),
                            Some(LABEL),
                        );
                    }
                }
            }
        }
        for direction in [Direction::TD, Direction::TB, Direction::BT] {
            for base in [BaseStyle::Ascii, BaseStyle::Unicode] {
                for mirror in [false, true] {
                    let mut baseline = h3_prefix_fixture(direction, base, mirror, 20, LABEL, false);
                    assert!(
                        baseline.proof(0).is_some(),
                        "baseline really reaches Target"
                    );
                    let expected: Vec<_> = (8..13).map(|y| baseline.point(3, y).1).collect();
                    let prefix = h3_prefix(&mut baseline).unwrap();
                    assert_eq!(prefix.source_x, baseline.point(3, 8).0);
                    assert_eq!(prefix.stem_start, baseline.point(3, 8).1);
                    assert_eq!(prefix.rows, expected);
                    let owner = baseline.owner(0);
                    let mut transit = baseline.clone();
                    transit.put(3, 9, '├', &owner, CellRole::Junction);
                    assert_eq!(
                        h3_prefix(&mut transit).unwrap().rows,
                        expected,
                        "known straight junction can transit"
                    );
                    let mut first_junction = baseline.clone();
                    first_junction.put(3, 8, '├', &owner, CellRole::Junction);
                    assert!(
                        h3_prefix(&mut first_junction).is_none(),
                        "a firststem junction is not a geometric port anchor"
                    );
                    for (ch, role, name) in [
                        (' ', CellRole::Empty, "raw gap"),
                        ('q', CellRole::Text, "unrecorded legacy caption"),
                        ('x', CellRole::Junction, "explicit ASCII crossing"),
                        ('✕', CellRole::Junction, "explicit Unicode crossing"),
                        ('o', CellRole::EndpointMarker, "endpoint"),
                    ] {
                        let mut cut = baseline.clone();
                        h3_raw(&mut cut, 3, 9, ch, &owner, CellOwnerKind::EdgeSegment, role);
                        assert_eq!(
                            h3_prefix(&mut cut).unwrap().rows,
                            vec![cut.point(3, 8).1],
                            "prefix stops before {name}"
                        );
                    }
                    let mut wrong_axis = baseline.clone();
                    wrong_axis.put(3, 8, '─', &owner, CellRole::Horizontal);
                    assert!(
                        h3_prefix(&mut wrong_axis).is_none(),
                        "source-facing vertical arms required"
                    );
                    let mut dotted = baseline.clone();
                    dotted.graph.edges[0].kind = EdgeKind::Dotted;
                    let dotted_glyph = dotted.style.dotted_v;
                    h3_raw(
                        &mut dotted,
                        3,
                        8,
                        dotted_glyph,
                        &owner,
                        CellOwnerKind::EdgeSegment,
                        CellRole::Text,
                    );
                    assert_eq!(
                        h3_prefix(&mut dotted).unwrap().rows,
                        expected,
                        "owned dotted Text role is a route shaft"
                    );
                    for reservation in ["node", "frame", "context"] {
                        let mut blocked = baseline.clone();
                        match reservation {
                            "node" => {
                                let n = blocked.make_node("Cut", 1, 9, 5);
                                blocked.graph.add_node(n);
                            }
                            "frame" => {
                                let (x, y) = blocked.point(2, 9);
                                let mut frame =
                                    Subgraph::new("CutFrame", Some("reserved title".to_owned()));
                                frame.bounds = Rectangle::new(x.saturating_sub(1), y, 4, 1);
                                blocked.graph.add_subgraph(frame);
                            }
                            _ => {
                                let point = blocked.point(3, 9);
                                blocked.context.reservations.push(vec![point]);
                            }
                        }
                        assert_eq!(
                            h3_prefix(&mut blocked).unwrap().rows,
                            vec![blocked.point(3, 8).1],
                            "stem reservation {reservation} stops before row evaluation"
                        );
                    }
                }
            }
        }
        // The Text exception is ONLY an owned dotted shaft, never arbitrary
        // solid route-looking caption text. Force the actual weak branch.
        for solid in ['│', '|', '█'] {
            let mut text =
                h3_prefix_fixture(Direction::TD, BaseStyle::Unicode, false, 20, LABEL, false);
            text.canvas.set(3, 11, ' ');
            let owner = text.owner(0);
            h3_raw(
                &mut text,
                3,
                8,
                solid,
                &owner,
                CellOwnerKind::EdgeSegment,
                CellRole::Text,
            );
            assert!(text.proof(0).is_none());
            assert!(
                h3_prefix(&mut text).is_none(),
                "solid Text glyph {solid:?} is not an owned dotted shaft"
            );
            assert!(
                h3_publish(&mut text, 0, 0, 9, 20).is_none(),
                "no older path rescues Text-role solid {solid:?}"
            );
        }
        // Foreign Graph nodes reserve their PHYSICAL minimum-three-row bodies,
        // even when a manually positioned public Graph has height0 or1.
        for height in [0, 1] {
            for placement in ["stem", "gap", "caption"] {
                let mut physical =
                    h3_prefix_fixture(Direction::TD, BaseStyle::Unicode, false, 20, LABEL, false);
                physical.canvas.set(3, 11, ' ');
                let mut foreign = match placement {
                    "stem" => node("Physical", 1, 7, 5),
                    "gap" => node("Physical", 4, 8, 1),
                    _ => node("Physical", 6, 8, 1),
                };
                foreign.height = height;
                physical.graph.add_node(foreign);
                assert!(physical.proof(0).is_none());
                if placement == "stem" {
                    assert!(h3_prefix(&mut physical).is_none());
                } else {
                    assert!(h3_prefix(&mut physical).unwrap().rows.contains(&9));
                    assert!(
                        h3_with_site(&mut physical, 0, |canvas, site| site
                            .writable(canvas, 5, 9, false, false)),
                        "old raw-height guard does not cover this physical {placement} row"
                    );
                    assert!(h3_commit(&mut physical, 0, 5, 9, 20).is_none());
                }
                assert!(
                    h3_publish(&mut physical, 0, 0, 9, 20).is_none(),
                    "all weak rows intersect physical {placement} height{height}"
                );
            }
        }
        // M22: old preferred is blocked, full proof is broken, near weak row is
        // blocked, and a later complete slot is real but beyond the first gap.
        for direction in [Direction::TD, Direction::TB, Direction::BT] {
            let mut cut = h3_prefix_fixture(direction, BaseStyle::Unicode, false, 20, LABEL, false);
            let (sx, gap) = cut.point(3, 9);
            cut.canvas.set(sx, gap, ' ');
            let glyph = cut.style.edge_v;
            h3_raw(
                &mut cut,
                0,
                9,
                glyph,
                "edge:99:foreign",
                CellOwnerKind::EdgeSegment,
                CellRole::Vertical,
            );
            h3_raw(
                &mut cut,
                5,
                8,
                glyph,
                "edge:99:foreign",
                CellOwnerKind::EdgeSegment,
                CellRole::Vertical,
            );
            assert!(cut.proof(0).is_none());
            let later = cut.point(3, 10).1;
            assert!(
                h3_with_site(&mut cut, 0, |canvas, site| site
                    .writable(canvas, 5, later, false, false)),
                "later disconnected caption has complete room"
            );
            assert_eq!(h3_prefix(&mut cut).unwrap().rows, vec![cut.point(3, 8).1]);
            assert!(
                h3_publish(&mut cut, 0, 0, gap, 20).is_none(),
                "raw gap must not borrow the later safe shaft"
            );
        }
        // A genuine eligible earlier full receipt overlays an actual horizontal
        // run, but neither it nor its raw caption may bridge a weak vertical prefix.
        // Reservation and raw-axis guards both apply; no isolated overlay kill is
        // claimed, since a legitimate captured underlay is Horizontal, not Vertical.
        let mut overlay = h2_shared_publication_fixture(BaseStyle::Unicode, "a 読 b", "peer");
        overlay.graph.add_node(node("OverlayCurrent", 10, 1, 3));
        overlay
            .graph
            .add_edge(Edge::with_label("OverlayCurrent", "Target", LABEL));
        let current = overlay.owner(2);
        for y in [4, 5, 6, 8, 9] {
            overlay.put(11, y, '│', &current, CellRole::Vertical);
        }
        overlay.context.prepare(&overlay.graph, &overlay.style, 48);
        overlay
            .commit_at(0, 9, 7, true, false, None)
            .expect("real earlier full annotation, not a constructed receipt");
        overlay.assert_receipt(0, 0);
        assert_eq!(overlay.canvas.get(11, 7), '読');
        assert!(overlay.context.reserves(11, 12, 7));
        {
            let view = ConvergentProofView::new(&overlay.canvas, [&overlay.context.receipts[0]]);
            let underlay = view.peek(11, 7).unwrap();
            assert_eq!(underlay.raw, overlay.style.edge_h);
            assert_eq!(underlay.meta.role, CellRole::Horizontal);
        }
        for y in 4..7 {
            route_cell(
                &mut overlay.canvas,
                13,
                y,
                '│',
                "edge:99:foreign",
                CellOwnerKind::EdgeSegment,
                CellRole::Vertical,
            );
        }
        overlay.assert_receipt(0, 0);
        assert!(overlay.proof(2).is_none());
        let before = overlay.canvas.clone();
        let context = overlay.context.clone();
        let prefix = h3_with_site(&mut overlay, 2, |canvas, site| {
            convergent_source_prefix(canvas, site)
        })
        .unwrap();
        assert_eq!(prefix.rows, vec![4, 5, 6]);
        assert_eq!(overlay.canvas, before);
        assert_eq!(overlay.context, context);
        assert!(
            h3_with_site(&mut overlay, 2, |canvas, site| {
                site.writable(canvas, 13, 9, false, false)
            }),
            "a complete later slot exists beyond the real annotation"
        );
        assert!(
            h3_publish(&mut overlay, 2, 0, 5, 48).is_none(),
            "raw-only weak search cannot use receipt transparency or borrow a later shaft"
        );
        overlay.assert_receipt(0, 0);
    }

    #[test]
    fn label15_source_prefix_requires_exact_peer_port() {
        // Snapshot mismatches use an otherwise feasible forced weak branch.
        // Source geometry changes preserve the exact source port, avoiding an
        // unrelated blank-entry refusal as the only reason the case rejects.
        for direction in [Direction::TD, Direction::TB, Direction::BT] {
            for base in [BaseStyle::Ascii, BaseStyle::Unicode] {
                let mut valid = h3_prefix_fixture(direction, base, false, 20, LABEL, false);
                let gap = valid.point(3, 11);
                valid.canvas.set(gap.0, gap.1, ' ');
                assert!(valid.proof(0).is_none());
                assert!(h3_prefix(&mut valid).is_some());
                let row = valid.point(3, 9).1;
                let mut positive = valid.clone();
                assert!(h3_publish(&mut positive, 0, 0, row, 20).is_some());
                for variant in ["from-snapshot", "to-snapshot", "direction-mismatch"] {
                    let mut bad = valid.clone();
                    let result = h3_publish_override(
                        &mut bad,
                        0,
                        0,
                        row,
                        20,
                        |from, to, supplied_direction, _| match variant {
                            "from-snapshot" => {
                                let port = from.x + from.width / 2;
                                from.x += 1;
                                from.width -= 2;
                                assert_eq!(from.x + from.width / 2, port);
                            }
                            "to-snapshot" => to.x += 1,
                            _ => {
                                *supplied_direction = if direction == Direction::TD {
                                    Direction::TB
                                } else {
                                    Direction::TD
                                };
                            }
                        },
                    );
                    assert!(
                        result.is_none(),
                        "exact current Graph snapshot rejects {variant}"
                    );
                }
            }
        }
        for direction in [Direction::TD, Direction::TB, Direction::BT] {
            for base in [BaseStyle::Ascii, BaseStyle::Unicode] {
                let mut peer = h3_prefix_fixture(direction, base, false, 20, LABEL, true);
                assert!(
                    peer.proof(0).is_none(),
                    "other-source raw ownership is not a full path receipt"
                );
                assert_eq!(h3_prefix(&mut peer).unwrap().rows.len(), 5);
                let row = peer.point(3, 9).1;
                let publication = h3_publish(&mut peer, 0, 0, row, 20).expect(
                    "aligned upstream same-target source port permits blank ordinary caption",
                );
                assert_eq!((publication.start, publication.y), (5, row));
                assert!(publication.proof.is_none() && publication.underlay.is_empty());
                h3_record(&mut peer, 0, publication);
                assert!(peer.context.receipts.is_empty());
                assert_eq!(peer.context.reservations.len(), 1);

                for variant in [
                    "stale",
                    "noncanonical",
                    "wrong-kind",
                    "back",
                    "wrong-target",
                    "shifted",
                    "overlap",
                    "invisible",
                    "current-back",
                    "single-incoming",
                ] {
                    let mut bad = h3_prefix_fixture(direction, base, false, 20, LABEL, true);
                    // A real independent third source keeps >=2 forward incoming
                    // edges even when the peer target/backness/visibility changes.
                    let third = bad.make_node("Third", 10, 1, 5);
                    bad.graph.add_node(third);
                    bad.graph.add_edge(Edge::new("Third", "Target"));
                    match variant {
                        "back" => bad.graph.edges[1].is_back_edge = true,
                        "wrong-target" => {
                            let other = bad.make_node("Other", 10, 14, 5);
                            bad.graph.add_node(other);
                            bad.graph.edges[1].to = "Other".to_owned();
                        }
                        "shifted" => {
                            bad.graph
                                .nodes
                                .iter_mut()
                                .find(|n| n.id == "Upstream")
                                .unwrap()
                                .x += 1
                        }
                        "overlap" => {
                            let replacement = bad.make_node("Upstream", 1, 5, 5);
                            *bad.graph
                                .nodes
                                .iter_mut()
                                .find(|n| n.id == "Upstream")
                                .unwrap() = replacement;
                        }
                        "invisible" => {
                            bad.graph
                                .nodes
                                .iter_mut()
                                .find(|n| n.id == "Upstream")
                                .unwrap()
                                .x = usize::MAX
                        }
                        "current-back" => bad.graph.edges[0].is_back_edge = true,
                        "single-incoming" => {
                            bad.graph.edges.truncate(1);
                        }
                        _ => {}
                    }
                    let owner = match variant {
                        "stale" => "edge:99:Upstream->Target".to_owned(),
                        "noncanonical" => "Upstream->Target".to_owned(),
                        "single-incoming" => bad.owner(0),
                        _ => bad.owner(1),
                    };
                    let glyph = bad.style.edge_v;
                    let kind = if variant == "wrong-kind" {
                        CellOwnerKind::CycleEdge
                    } else {
                        CellOwnerKind::EdgeSegment
                    };
                    for y in 8..13 {
                        h3_raw(&mut bad, 3, y, glyph, &owner, kind, CellRole::Vertical);
                    }
                    assert!(
                        h3_prefix(&mut bad).is_none(),
                        "dedicated raw owner/Graph guard refuses {variant}"
                    );
                    let row = bad.point(3, 9).1;
                    assert!(
                        h3_commit(&mut bad, 0, 5, row, 20).is_none(),
                        "peer refusal must reach actual weak commit: {variant}"
                    );
                }
            }
        }
        // Both actual gap cells left of a stem must remain empty. The nearest
        // one lies beyond the ordinary caption's one-cell right padding.
        let mut left = h3_prefix_fixture(Direction::TD, BaseStyle::Unicode, true, 20, LABEL, false);
        let source = left.point(3, 9).0;
        let start = source - (display_width(LABEL) + 2);
        left.canvas.set_owned_with_role(
            source - 1,
            9,
            '│',
            CellOwnerKind::EdgeSegment,
            "edge:99:foreign",
            CellRole::Vertical,
            5,
        );
        assert!(
            h3_with_site(&mut left, 0, |canvas, site| site
                .writable(canvas, start, 9, false, false)),
            "ordinary caption margin does not include nearest gap cell"
        );
        assert!(
            h3_commit(&mut left, 0, start, 9, 20).is_none(),
            "complete gap rejects hidden foreign route"
        );
        left.canvas.set(source - 1, 9, ' ');
        left.context.reservations.push(vec![(source - 1, 9)]);
        assert!(
            h3_commit(&mut left, 0, start, 9, 20).is_none(),
            "complete gap rejects an internal blank reservation"
        );
    }

    #[test]
    fn label15_source_prefix_extension_is_atomic_and_reserve_only() {
        // Exact gen_d_023 B caption boundary:18cells at5, bodyend23.
        for (width, cap, expected_width, succeeds) in
            [(21, 22, 21, false), (21, 23, 23, true), (23, 24, 23, true)]
        {
            let text = "another long wrap…";
            assert_eq!(display_width(text), 18);
            let mut f =
                h3_prefix_fixture(Direction::TD, BaseStyle::Unicode, false, width, text, false);
            f.canvas.set(3, 11, ' ');
            assert!(f.proof(0).is_none());
            let publication = h3_publish(&mut f, 0, 0, 9, cap);
            assert_eq!(
                publication.is_some(),
                succeeds,
                "exact B boundary width={width} cap={cap}"
            );
            assert_eq!(f.canvas.width, expected_width);
            if let Some(publication) = publication {
                assert_eq!(
                    (publication.start, publication.end, publication.y),
                    (5, 23, 9)
                );
                assert_eq!(
                    convergent_positioned_text(&f.canvas, 5, 23, 9).as_deref(),
                    Some(text)
                );
                assert!(publication.proof.is_none() && publication.underlay.is_empty());
            }
        }
        for (width, cap, expected_width, succeeds) in [
            (10, 11, 10, false),
            (10, 12, 12, true),
            (12, 13, 12, true),
            (20, 10, 20, true),
        ] {
            let mut f = h3_prefix_fixture(
                Direction::TD,
                BaseStyle::Unicode,
                false,
                width,
                LABEL,
                false,
            );
            f.canvas.set(3, 11, ' ');
            assert!(f.proof(0).is_none(), "strong corridor unavailable");
            let publication = h3_publish(&mut f, 0, 0, 9, cap);
            assert_eq!(
                publication.is_some(),
                succeeds,
                "body-end cap case width={width} cap={cap}"
            );
            assert_eq!(f.canvas.width, expected_width);
            if let Some(publication) = publication {
                assert_eq!(publication.end, 12);
                assert!(publication.proof.is_none() && publication.underlay.is_empty());
                assert_eq!(
                    convergent_positioned_text(&f.canvas, 5, 12, publication.y).as_deref(),
                    Some(LABEL)
                );
                h3_record(&mut f, 0, publication);
                assert!(f.context.receipts.is_empty());
                assert!(f.context.reserves(5, 12, 9));
            }
        }
        // M24: source/prefix/geometry/gap/cap pass; growth is required. The
        // complete slot guard on the expanded clone rejects a protected body cell.
        let mut refused =
            h3_prefix_fixture(Direction::TD, BaseStyle::Unicode, false, 10, LABEL, false);
        refused.canvas.set(3, 11, ' ');
        refused.canvas.set_owned_with_role(
            6,
            9,
            '│',
            CellOwnerKind::EdgeSegment,
            "edge:99:foreign",
            CellRole::Vertical,
            5,
        );
        assert!(h3_prefix(&mut refused).unwrap().rows.contains(&9));
        assert_eq!(5 + display_width(LABEL), 12);
        assert!(h3_with_site(&mut refused, 0, |canvas, site| {
            assert!(convergent_prefix_gap_clear(canvas, site, 5, 12, 9, 3));
            let mut expanded = canvas.clone();
            expanded.ensure_width(12);
            !site.writable(&expanded, 5, 9, false, false)
        }));
        let before = refused.canvas.clone();
        assert!(h3_commit(&mut refused, 0, 5, 9, 12).is_none());
        assert_eq!(
            refused.canvas, before,
            "failed expanded complete guard must retain every Canvas field and dimension"
        );
        assert!(h3_commit(&mut refused, 0, usize::MAX, 9, usize::MAX).is_none(), "checked canonical candidate bounds refuse oversized coordinate; not an isolated arithmetic mutation witness");

        let text = "読み a\u{301} b";
        let mut wide = h3_prefix_fixture(Direction::TD, BaseStyle::Unicode, false, 10, text, false);
        wide.canvas.set(3, 11, ' ');
        let width = display_width(text);
        assert_eq!(width, 8);
        assert_eq!(
            format_edge_label_with_limit(text, wide.context.limit.min(wide.canvas.width)),
            text,
            "wide/combining setup must not silently truncate its own expected text",
        );
        let publication = h3_publish(&mut wide, 0, 0, 9, 5 + width)
            .expect("complete supported wide+combining caption fits exact body end");
        assert_eq!(
            publication.cells,
            (5..5 + width).map(|x| (x, 9)).collect::<Vec<_>>()
        );
        assert_eq!(
            convergent_positioned_text(&wide.canvas, 5, 5 + width, 9).as_deref(),
            Some(text)
        );
        assert_eq!(wide.canvas.get(6, 9), '\0');
        assert_eq!(wide.canvas.get(8, 9), '\0');
        assert!(publication.proof.is_none() && publication.underlay.is_empty());
        h3_record(&mut wide, 0, publication);
        assert!(wide.context.receipts.is_empty());
        for x in 5..5 + width {
            assert!(
                wide.context.reserves(x, x + 1, 9),
                "wide/SPACE cell {x} reserved"
            );
        }

        // M25 and formatting lifecycle: this is a REAL pre-existing full receipt,
        // with actual horizontal underlay and SPACE/CJK post cells, not a forged Rc.
        let mut pair = h2_shared_publication_fixture(BaseStyle::Unicode, "a 読 b", &"Z".repeat(90));
        pair.graph.nodes.iter_mut().find(|n| n.id == "B").unwrap().x = 33;
        let b = pair.owner(1);
        for y in 4..7 {
            pair.put(35, y, '│', &b, CellRole::Vertical);
        }
        pair.put(35, 7, '┘', &b, CellRole::Corner);
        for x in 31..35 {
            pair.put(x, 7, '─', "fanin:Target", CellRole::Horizontal);
        }
        pair.put(30, 7, '┬', "fanin:Target", CellRole::Junction);
        pair.canvas.set(45, 15, 'e');
        pair.canvas.set(45, 15, '\u{301}');
        pair.context.prepare(&pair.graph, &pair.style, 48);
        pair.commit_at(0, 9, 7, true, false, None)
            .expect("real A annotation");
        pair.assert_receipt(0, 0);
        assert_eq!(pair.canvas.get(12, 7), '\0');
        let a_cells = pair.context.reservations[0].clone();
        assert!(
            pair.proof(1).is_some(),
            "B has independent full proof before weak publication"
        );
        let before = pair.canvas.clone();
        assert!(
            pair.canvas.width < 80,
            "initial canvas clips the configured next-call format limit"
        );
        let fixed_b =
            format_edge_label_with_limit(pair.graph.edges[1].label.as_deref().unwrap(), 48);
        let publication = h3_publish(&mut pair, 1, 0, 5, 85)
            .expect("all strong slots too narrow; exact weak extension survives");
        assert_eq!(
            (publication.start, publication.end, publication.y),
            (37, 85, 5)
        );
        assert_eq!(pair.canvas.width, 85);
        // Full-state allowlist: only exact-width empty-column extension and the
        // already verified complete caption writer may differ. This equality
        // covers Canvas's private crossing/route/title/current-stage state too.
        let mut allowed = before.clone();
        allowed.ensure_width(85);
        commit_convergent_label(&mut allowed, 37, 5, &fixed_b);
        assert_eq!(
            pair.canvas, allowed,
            "successful weak commit changes only allowed caption/width state"
        );
        assert!(
            publication.proof.is_none(),
            "weak publication must never promote the site's available full proof"
        );
        assert!(
            publication.underlay.is_empty(),
            "weak blank publication supplies no virtual line"
        );
        for y in 0..before.height {
            for x in 0..before.width {
                if !(y == 5 && (37..85).contains(&x)) {
                    assert_eq!(pair.canvas.get(x, y), before.get(x, y));
                    assert_eq!(pair.canvas.get_meta(x, y), before.get_meta(x, y));
                    assert_eq!(
                        pair.canvas.write_stage_at(x, y),
                        before.write_stage_at(x, y)
                    );
                }
            }
        }
        assert_eq!(
            convergent_positioned_text(&pair.canvas, 9, 15, 7),
            Some("a 読 b".to_owned())
        );
        assert_eq!(
            convergent_positioned_text(&pair.canvas, 45, 46, 15).as_deref(),
            Some("e\u{301}"),
            "unrelated combining stream is preserved"
        );
        h3_record(&mut pair, 1, publication);
        assert_eq!(
            pair.context.receipts.len(),
            1,
            "weak successful publication reserves only"
        );
        pair.assert_receipt(0, 0);
        assert_eq!(
            pair.context.limit, 48,
            "current attempt formatting never changes during growth"
        );
        pair.context
            .prepare(&pair.graph, &pair.style, 80.min(pair.canvas.width));
        assert!(
            !pair.context.receipts[0].eligible,
            "next call's changed effective format limit revokes old FULL receipt"
        );
        assert!(pair
            .context
            .valid_indices(&pair.canvas, &pair.graph, &pair.style)
            .is_empty());
        for (x, y) in a_cells {
            assert!(
                pair.context.reserves(x, x + 1, y),
                "old SPACE/CJK/full footprint survives revocation"
            );
        }
        assert!(
            pair.context.reserves(37, 85, 5),
            "weak footprint also survives"
        );

        // A feasible full-proof structured slot competes with an ordinary weak
        // extension. Reuse the genuine sibling fixture, remove its frame before
        // publication, and move only B's test geometry/route ten columns right.
        // No receipt is transplanted across a changed Graph or fabricated.
        for direction in [Direction::TD, Direction::TB, Direction::BT] {
            for base in [BaseStyle::Ascii, BaseStyle::Unicode] {
                let mut ordered = h2_sibling_fixture(
                    direction,
                    base,
                    false,
                    0,
                    0,
                    16,
                    "BBBBBBBBBBBB",
                    H2SiblingVariant::Normal,
                );
                for cell in ordered.frame.drain(..) {
                    ordered.canvas.set(cell.x, cell.y, ' ');
                }
                ordered.graph.subgraphs.clear();
                ordered.graph.node_subgraph.clear();
                ordered.graph.edges[0].label = Some("A".repeat(20));
                ordered
                    .graph
                    .nodes
                    .iter_mut()
                    .find(|node| node.id == "B")
                    .unwrap()
                    .x += 10;
                let b = ordered.owner(1);
                for y in 19..21 {
                    let old = ordered.point(62, y);
                    ordered.canvas.set(old.0, old.1, ' ');
                    ordered.put(72, y, '│', &b, CellRole::Vertical);
                }
                for x in 53..72 {
                    ordered.put(x, 21, '─', "fanin:Target", CellRole::Horizontal);
                }
                ordered.put(72, 21, '┘', &b, CellRole::Corner);
                ordered.context.prepare(&ordered.graph, &ordered.style, 24);
                let a_row = ordered.point(42, 20).1;
                ordered
                    .commit_at(0, 44, a_row, false, false, None)
                    .expect("complete earlier ordinary FULL receipt");
                ordered.assert_receipt(0, 0);
                assert!(ordered.proof(1).is_some());
                let row = ordered.point(72, 19).1;
                let turn = ordered.point(72, 21).1;
                h3_with_site(&mut ordered, 1, |canvas, site| {
                    assert!(site.structured_peer(canvas, 58, 70, row).is_some());
                    assert!(!site.writable(canvas, 58, row, false, false));
                    assert!(site.writable(canvas, 58, row, false, true));
                    for start in 53..=60 {
                        assert!(
                            !site.writable(canvas, start, turn, true, false),
                            "every ordinary proved-run alternative is unavailable"
                        );
                    }
                    let prefix = convergent_source_prefix(canvas, site).unwrap();
                    assert_eq!(prefix.source_x, 72);
                    for &candidate_row in &prefix.rows {
                        assert!(!site.writable(canvas, 58, candidate_row, false, false));
                        assert!(!site.writable(canvas, 74, candidate_row, false, false));
                    }
                });
                let mut structured = ordered.clone();
                structured
                    .commit_at(1, 58, row, false, true, None)
                    .expect("actual competing structured publication must be feasible");
                structured.assert_receipt(1, 1);
                let publication = h3_publish(&mut ordered, 1, 58, row, 86).expect(
                    "all strict weak alternatives precede the feasible structured fallback",
                );
                assert_eq!(
                    (
                        publication.start,
                        publication.end,
                        publication.y,
                        ordered.canvas.width
                    ),
                    (74, 86, row, 86),
                );
                assert!(publication.proof.is_none() && publication.underlay.is_empty());
                assert_eq!(
                    convergent_positioned_text(&ordered.canvas, 74, 86, row).as_deref(),
                    Some("BBBBBBBBBBBB"),
                );
                h3_record(&mut ordered, 1, publication);
                assert_eq!(ordered.context.receipts.len(), 1);
                ordered.assert_receipt(0, 0);
            }
        }
    }
}
