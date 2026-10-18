//! Edge-label placement and provenance policy.

use crate::config::Config;
use crate::geom::{EdgeRoute, Segment};
use crate::graph::{Direction, Graph, Node};
use crate::style::{display_char_width, display_width, truncate_to_width, StyleChars};

use super::canvas;
use super::canvas::Canvas;
use super::portal_projection::{is_textual, subgraph_title_y};
use super::precomputed::is_subgraph_title_cell;
use super::provenance::{edge_owner_id, EdgeLabelPlacement};
use super::semantic::CellOwnerKind;

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
) -> Option<EdgeLabelPlacement> {
    let kind_style = edge_label_style(style, edge);
    let style = &kind_style;
    use super::cycle::{center_x, center_y};

    // Use slightly shorter limit for convergent labels to avoid crowding at merge points
    let convergent_limit = config.max_edge_label_width.saturating_sub(2).max(8);
    let display_label = format_edge_label_with_limit(label, convergent_limit.min(canvas.width));
    let label_width = display_width(&display_label);
    let owner_id = edge_owner_id(edge_idx, edge);
    let mut cells = Vec::new();

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
                convergent_turn_row(canvas, src_x, stem_start_y, false, to)
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
                }
            }

            let mut x_pos = label_start_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height {
                    set_label_char(canvas, &mut cells, x_pos, label_y, c);
                }
                x_pos += display_char_width(c);
            }
        }
        Direction::BT => {
            let src_x = center_x(from);
            let target_x = center_x(to);
            let stem_start_y = from.y.saturating_sub(1);
            let mut label_y = stem_start_y.saturating_sub(1);

            let mut label_start_x =
                convergent_vertical_label_start(src_x, target_x, label_width, canvas.width);
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
            if let Some(traced) = convergent_turn_row(canvas, src_x, stem_start_y, true, to) {
                turn_y = traced;
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
            let mut x_pos = label_start_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height {
                    set_label_char(canvas, &mut cells, x_pos, label_y, c);
                }
                x_pos += display_char_width(c);
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

    build_label_placement(owner_id, cells)
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
    !(left > 0 && merges(left - 1)) && !merges(right + 1)
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
