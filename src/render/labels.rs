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
    use super::cycle::{center_x, center_y};

    let display_label =
        format_edge_label_with_limit(label, config.max_edge_label_width.min(canvas.width));
    let label_width = display_width(&display_label);
    let owner_id = edge_owner_id(edge_idx, edge);
    let mut cells = Vec::new();

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
                    canvas.set(x_pos, label_y, c);
                    record_label_cell(&mut cells, x_pos, label_y);
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

            let label_start_x =
                pick_bt_vertical_label_start(canvas, &[from, to], edge_x, label_y, label_width);
            let mut x_pos = label_start_x;
            for c in display_label.chars() {
                if x_pos < canvas.width
                    && label_y < canvas.height
                    && !is_textual(canvas.get(x_pos, label_y))
                {
                    canvas.set(x_pos, label_y, c);
                    record_label_cell(&mut cells, x_pos, label_y);
                }
                x_pos += display_char_width(c);
            }
        }
        Direction::LR => {
            let edge_y = center_y(to);
            let stem_start_x = from.x + from.width;
            let arrow_x = to.x.saturating_sub(1);
            let span_width = arrow_x.saturating_sub(stem_start_x);
            let outside_row =
                pick_outside_horizontal_label_row(edge_y, canvas.height, &[from, to], graph);
            let can_fit_full_inline = label_width + 3 <= span_width;

            if can_fit_full_inline {
                let label_start_x = stem_start_x + (span_width - (label_width + 3)) / 2;

                for x in stem_start_x..label_start_x {
                    canvas.set(x, edge_y, style.edge_h);
                }

                canvas.set(label_start_x, edge_y, ' ');

                let mut x_pos = label_start_x + 1;
                for c in display_label.chars() {
                    if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                        canvas.set(x_pos, edge_y, c);
                        record_label_cell(&mut cells, x_pos, edge_y);
                    }
                    x_pos += display_char_width(c);
                }

                if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                    canvas.set(x_pos, edge_y, ' ');
                }
                x_pos += 1;

                for x in x_pos..arrow_x {
                    if x < canvas.width {
                        canvas.set(x, edge_y, style.edge_h);
                    }
                }
            } else if let Some(label_row) = outside_row {
                let label_x = stem_start_x + span_width / 2;
                let max_label_start = canvas.width.saturating_sub(label_width);
                let mut label_start_x =
                    label_x.saturating_sub(label_width / 2).min(max_label_start);
                label_start_x = adjust_horizontal_label_slot(
                    label_start_x,
                    0,
                    canvas.width,
                    label_row,
                    label_width,
                    &[from, to],
                    graph,
                );

                let mut x_pos = label_start_x;
                for c in display_label.chars() {
                    if x_pos < canvas.width && label_row < canvas.height {
                        canvas.set(x_pos, label_row, c);
                        record_label_cell(&mut cells, x_pos, label_row);
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
                    canvas.set(x, edge_y, style.edge_h);
                }

                canvas.set(label_start_x, edge_y, ' ');

                let mut x_pos = label_start_x + 1;
                for c in inline_label.chars() {
                    if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                        canvas.set(x_pos, edge_y, c);
                        record_label_cell(&mut cells, x_pos, edge_y);
                    }
                    x_pos += display_char_width(c);
                }

                if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                    canvas.set(x_pos, edge_y, ' ');
                }
                x_pos += 1;

                for x in x_pos..arrow_x {
                    if x < canvas.width {
                        canvas.set(x, edge_y, style.edge_h);
                    }
                }
            }
        }
        Direction::RL => {
            let edge_y = center_y(to);
            let arrow_x = to.x + to.width; // Arrow is after target box
            let stem_end_x = from.x; // Edge ends at left side of source box
            let gap_start_x = arrow_x.saturating_add(1);
            let span_width = stem_end_x.saturating_sub(gap_start_x);
            let outside_row =
                pick_outside_horizontal_label_row(edge_y, canvas.height, &[from, to], graph);
            let can_fit_full_inline = label_width + 4 <= span_width;

            if can_fit_full_inline {
                let label_start_x = gap_start_x + 1 + (span_width - (label_width + 4)) / 2;

                for x in gap_start_x..label_start_x {
                    if x < canvas.width {
                        canvas.set(x, edge_y, style.edge_h);
                    }
                }

                if label_start_x < canvas.width {
                    canvas.set(label_start_x, edge_y, ' ');
                }

                let mut x_pos = label_start_x + 1;
                for c in display_label.chars() {
                    if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                        canvas.set(x_pos, edge_y, c);
                        record_label_cell(&mut cells, x_pos, edge_y);
                    }
                    x_pos += display_char_width(c);
                }

                if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                    canvas.set(x_pos, edge_y, ' ');
                }
                x_pos += 1;

                for x in x_pos..stem_end_x {
                    if x < canvas.width {
                        canvas.set(x, edge_y, style.edge_h);
                    }
                }
            } else if let Some(label_row) = outside_row {
                let label_x = gap_start_x + span_width / 2;
                let max_label_start = canvas.width.saturating_sub(label_width);
                let mut label_start_x =
                    label_x.saturating_sub(label_width / 2).min(max_label_start);
                label_start_x = adjust_horizontal_label_slot(
                    label_start_x,
                    0,
                    canvas.width,
                    label_row,
                    label_width,
                    &[from, to],
                    graph,
                );

                let mut x_pos = label_start_x;
                for c in display_label.chars() {
                    if x_pos < canvas.width && label_row < canvas.height {
                        canvas.set(x_pos, label_row, c);
                        record_label_cell(&mut cells, x_pos, label_row);
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
                        canvas.set(x, edge_y, style.edge_h);
                    }
                }

                if label_start_x < canvas.width {
                    canvas.set(label_start_x, edge_y, ' ');
                }

                let mut x_pos = label_start_x + 1;
                for c in inline_label.chars() {
                    if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                        canvas.set(x_pos, edge_y, c);
                        record_label_cell(&mut cells, x_pos, edge_y);
                    }
                    x_pos += display_char_width(c);
                }

                if x_pos < canvas.width && !is_textual(canvas.get(x_pos, edge_y)) {
                    canvas.set(x_pos, edge_y, ' ');
                }
                x_pos += 1;

                for x in x_pos..stem_end_x {
                    if x < canvas.width {
                        canvas.set(x, edge_y, style.edge_h);
                    }
                }
            }
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
    if route.segments.is_empty() {
        return None;
    }

    let display_label =
        format_edge_label_with_limit(label, config.max_edge_label_width.min(canvas.width));
    let label_width = display_width(&display_label);
    let owner_id = edge_owner_id(edge_idx, edge);
    let mut cells = Vec::new();

    let nodes: Vec<&Node> = graph.nodes.iter().collect();
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
        let gap_start_x = min_x;
        let gap_end_x = max_x.saturating_add(1);
        let gap_width = gap_end_x.saturating_sub(gap_start_x);
        let mid_x = gap_start_x + gap_width / 2;
        let centered_start_x = mid_x.saturating_sub(label_width / 2);
        let outside_row = pick_outside_horizontal_label_row(y, canvas.height, &nodes, graph);
        let reserve_leading_shaft = graph.direction == Direction::RL;
        let inline_margin = if reserve_leading_shaft { 4 } else { 3 };
        let inline_collides = overlaps_node(&nodes, centered_start_x, y, label_width);
        let can_fit_full_inline = !inline_collides && label_width + inline_margin <= gap_width;

        if can_fit_full_inline {
            let start_x = gap_start_x
                + usize::from(reserve_leading_shaft)
                + (gap_width - (label_width + inline_margin)) / 2;
            for x in gap_start_x..start_x {
                if y < canvas.height && x < canvas.width {
                    canvas.set(x, y, style.edge_h);
                }
            }

            if start_x < canvas.width && y < canvas.height {
                canvas.set(start_x, y, ' ');
            }

            let mut x_pos = start_x + 1;
            for c in display_label.chars() {
                if y < canvas.height && x_pos < canvas.width {
                    canvas.set(x_pos, y, c);
                    record_label_cell(&mut cells, x_pos, y);
                }
                x_pos += display_char_width(c);
            }

            if x_pos < canvas.width && y < canvas.height {
                canvas.set(x_pos, y, ' ');
            }
            x_pos += 1;

            for x in x_pos..gap_end_x {
                if y < canvas.height && x < canvas.width {
                    canvas.set(x, y, style.edge_h);
                }
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
                    canvas.set(x_pos, label_row, c);
                    record_label_cell(&mut cells, x_pos, label_row);
                }
                x_pos += display_char_width(c);
            }
        } else {
            let inline_limit = config
                .max_edge_label_width
                .min(gap_width.saturating_sub(inline_margin).max(1));
            let inline_label = format_edge_label_with_limit(label, inline_limit);
            let inline_width = display_width(&inline_label);
            let start_x = gap_start_x
                + usize::from(reserve_leading_shaft)
                + (gap_width.saturating_sub(inline_width + inline_margin)) / 2;

            for x in gap_start_x..start_x {
                if y < canvas.height && x < canvas.width {
                    canvas.set(x, y, style.edge_h);
                }
            }

            if start_x < canvas.width && y < canvas.height {
                canvas.set(start_x, y, ' ');
            }

            let mut x_pos = start_x + 1;
            for c in inline_label.chars() {
                if y < canvas.height && x_pos < canvas.width {
                    canvas.set(x_pos, y, c);
                    record_label_cell(&mut cells, x_pos, y);
                }
                x_pos += display_char_width(c);
            }

            if x_pos < canvas.width && y < canvas.height {
                canvas.set(x_pos, y, ' ');
            }
            x_pos += 1;

            for x in x_pos..gap_end_x {
                if y < canvas.height && x < canvas.width {
                    canvas.set(x, y, style.edge_h);
                }
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
                canvas.set(x_pos, mid_y, c);
                record_label_cell(&mut cells, x_pos, mid_y);
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
        let distance = start.abs_diff(centered);
        let score = usize::from(covers_edge) * 1000
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
    edge_y: usize,
    canvas_height: usize,
    nodes: &[&Node],
    graph: &Graph,
) -> Option<usize> {
    let mut candidates = Vec::new();

    for delta in [2usize, 3usize] {
        if let Some(row) = edge_y.checked_sub(delta) {
            candidates.push(row);
        }
        let row = edge_y.saturating_add(delta);
        if row < canvas_height {
            candidates.push(row);
        }
    }

    candidates.into_iter().find(|row| {
        let intersects_node = nodes
            .iter()
            .any(|node| *row >= node.y && *row < node.bottom_y());
        !intersects_node && !is_reserved_subgraph_label_row(graph, *row)
    })
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
    config: &Config,
    edge_idx: usize,
    edge: &crate::graph::Edge,
) -> Option<EdgeLabelPlacement> {
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
            let label_y = stem_start_y + 1;

            // Move the label away from the shared merge corridor when the source
            // approaches the target from the left or right.
            let label_start_x = if src_x + 1 < target_x {
                src_x.saturating_sub(label_width)
            } else if src_x > target_x + 1 {
                src_x.saturating_add(2)
            } else {
                src_x.saturating_sub(label_width / 2)
            };

            let mut x_pos = label_start_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height {
                    canvas.set(x_pos, label_y, c);
                    record_label_cell(&mut cells, x_pos, label_y);
                }
                x_pos += display_char_width(c);
            }
        }
        Direction::BT => {
            let src_x = center_x(from);
            let stem_start_y = from.y.saturating_sub(1);
            let label_y = stem_start_y.saturating_sub(1);

            let label_start_x = src_x.saturating_sub(label_width / 2);
            let mut x_pos = label_start_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height {
                    canvas.set(x_pos, label_y, c);
                    record_label_cell(&mut cells, x_pos, label_y);
                }
                x_pos += display_char_width(c);
            }
        }
        Direction::LR => {
            // Place label on horizontal line from source, before merge
            let src_y = center_y(from);
            let stem_start_x = from.x + from.width;
            let label_x = stem_start_x + 1;
            // Place label above the edge line
            let label_y = src_y.saturating_sub(1);

            let mut x_pos = label_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height {
                    canvas.set(x_pos, label_y, c);
                    record_label_cell(&mut cells, x_pos, label_y);
                }
                x_pos += display_char_width(c);
            }
        }
        Direction::RL => {
            let src_y = center_y(from);
            let stem_start_x = from.x.saturating_sub(1);
            let label_x = stem_start_x.saturating_sub(label_width);
            let label_y = src_y.saturating_sub(1);

            let mut x_pos = label_x;
            for c in display_label.chars() {
                if x_pos < canvas.width && label_y < canvas.height {
                    canvas.set(x_pos, label_y, c);
                    record_label_cell(&mut cells, x_pos, label_y);
                }
                x_pos += display_char_width(c);
            }
        }
    }

    build_label_placement(owner_id, cells)
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
