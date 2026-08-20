//! Box drawing primitives for node rendering.
//!
//! Supports 9 node shapes with direction-aware junction placement.

use crate::graph::{Direction, NodeShape};
use crate::style::{display_char_width, display_width, StyleChars};

use super::canvas::{is_arrow, is_horizontal, is_junction, is_vertical, Canvas};
use super::subgraph_title_y;

/// Draw a label into a terminal-cell-sized content band.
///
/// The canvas is indexed by logical cells, while a wide Unicode glyph occupies
/// more than one terminal cell. A NUL continuation marker reserves those extra
/// cells in the canvas and is omitted when the final frame is serialized. This
/// keeps border coordinates, route coordinates, and terminal display columns
/// in agreement without making routing code Unicode-width-aware.
fn draw_centered_label(canvas: &mut Canvas, x: usize, y: usize, width: usize, label: &str) {
    if width == 0 {
        return;
    }

    let label_width = display_width(label);
    let padding = if width >= label_width.saturating_add(2) {
        1
    } else {
        0
    };
    let available = width.saturating_sub(padding * 2);
    let left = padding + available.saturating_sub(label_width) / 2;
    let mut cursor = x.saturating_add(left);

    for ch in label.chars() {
        let char_width = display_char_width(ch);
        if char_width == 0 {
            // Combining marks and variation selectors belong immediately after
            // the preceding glyph in the serialized string. A wide glyph has
            // a continuation cell available; otherwise keep the mark adjacent
            // in the next logical cell without changing the cell budget.
            if cursor > x {
                canvas.set_inferred(cursor.saturating_sub(1), y, ch);
            } else if cursor < x.saturating_add(width) {
                canvas.set_inferred(cursor, y, ch);
                cursor = cursor.saturating_add(1);
            }
            continue;
        }

        if cursor >= x.saturating_add(width) {
            break;
        }
        canvas.set_inferred(cursor, y, ch);
        cursor = cursor.saturating_add(1);
        for _ in 1..char_width {
            if cursor >= x.saturating_add(width) {
                break;
            }
            canvas.set_inferred(cursor, y, '\0');
            cursor = cursor.saturating_add(1);
        }
    }
}

/// Draw a subgraph bounding box with optional title.
pub fn draw_subgraph(
    canvas: &mut Canvas,
    rect: &crate::graph::Rectangle,
    title: Option<&str>,
    style: &StyleChars,
    direction: Direction,
) {
    if !rect.is_valid() {
        return;
    }

    let x = rect.x;
    let y = rect.y;
    let width = rect.width;
    let height = rect.height;

    // Use standard corners but maybe lighter or same style
    // For now, reuse standard style chars
    canvas.set(x, y, style.tl);
    for i in 1..width - 1 {
        canvas.set(x + i, y, style.h);
    }
    canvas.set(x + width - 1, y, style.tr);

    // Sides
    for j in 1..height - 1 {
        canvas.set(x, y + j, style.v);
        canvas.set(x + width - 1, y + j, style.v);
    }

    // Bottom
    canvas.set(x, y + height - 1, style.bl);
    for i in 1..width - 1 {
        canvas.set(x + i, y + height - 1, style.h);
    }
    canvas.set(x + width - 1, y + height - 1, style.br);

    if let Some(t) = title {
        let title_fmt = crate::graph::subgraph_title_text(t);
        if let Some(start_x) = crate::graph::subgraph_title_start_x(x, width, t, direction) {
            let title_y = subgraph_title_y(rect, direction);
            for (i, c) in title_fmt.chars().enumerate() {
                if start_x + i < canvas.width {
                    canvas.set(start_x + i, title_y, c);
                }
            }
        }
    }
}

/// Draw a node at position (x, y) with the given label and shape.
#[allow(clippy::too_many_arguments)]
pub fn draw_node(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    label_lines: &[String],
    shape: NodeShape,
    style: &StyleChars,
    direction: Direction,
) {
    let label = label_lines.first().map(|s| s.as_str()).unwrap_or_default();
    match shape {
        NodeShape::Rectangle => {
            draw_rectangle(canvas, x, y, width, height, label_lines, style, direction)
        }
        NodeShape::Rounded => {
            draw_rounded(canvas, x, y, width, height, label_lines, style, direction)
        }
        NodeShape::Diamond => draw_diamond(canvas, x, y, width, label, style, direction),
        NodeShape::Circle => draw_circle(canvas, x, y, width, label, style),
        NodeShape::DoubleCircle => draw_double_circle(canvas, x, y, width, label, style),
        NodeShape::Stadium => {
            draw_stadium(canvas, x, y, width, height, label_lines, style, direction)
        }
        NodeShape::Hexagon => {
            draw_hexagon(canvas, x, y, width, height, label_lines, style, direction)
        }
        NodeShape::Database => {
            draw_database(canvas, x, y, width, height, label_lines, style, direction)
        }
        NodeShape::Subroutine => {
            draw_subroutine(canvas, x, y, width, height, label_lines, style, direction)
        }
        NodeShape::Asymmetric => {
            draw_asymmetric(canvas, x, y, width, height, label_lines, style, direction)
        }
        NodeShape::Parallelogram => draw_parallelogram(
            canvas,
            x,
            y,
            width,
            height,
            label_lines,
            style,
            direction,
            true,
        ),
        NodeShape::ParallelogramAlt => draw_parallelogram(
            canvas,
            x,
            y,
            width,
            height,
            label_lines,
            style,
            direction,
            false,
        ),
        NodeShape::Trapezoid => draw_trapezoid(
            canvas,
            x,
            y,
            width,
            height,
            label_lines,
            style,
            direction,
            true,
        ),
        NodeShape::TrapezoidAlt => draw_trapezoid(
            canvas,
            x,
            y,
            width,
            height,
            label_lines,
            style,
            direction,
            false,
        ),
    }
}

/// Draw a node while preserving its horizontal source wall for a generic
/// multi-edge fanout.
///
/// `draw_boxlike` normally replaces a side wall with a junction when a route
/// is immediately outside the node. That is useful for a single visible port,
/// but it conflates the enclosure with the shared collector used by a
/// multi-edge fanout. The route remains connected through the adjacent cell;
/// restoring the shape-owned wall keeps the node boundary independently
/// legible.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_node_with_fanout_policy(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    label_lines: &[String],
    shape: NodeShape,
    style: &StyleChars,
    direction: Direction,
    preserve_horizontal_side_wall: bool,
) {
    draw_node(
        canvas,
        x,
        y,
        width,
        height,
        label_lines,
        shape,
        style,
        direction,
    );

    if !preserve_horizontal_side_wall || !matches!(direction, Direction::LR | Direction::RL) {
        return;
    }

    let Some((left_side, right_side)) = fanout_side_walls(shape, style) else {
        return;
    };
    let center_y = y + height.max(3) / 2;
    let (side_x, side_char) = match direction {
        Direction::LR => (x + width.saturating_sub(1), right_side),
        Direction::RL => (x, left_side),
        Direction::TD | Direction::TB | Direction::BT => return,
    };
    if side_x < canvas.width && center_y < canvas.height {
        canvas.set(side_x, center_y, side_char);
    }
}

pub(crate) fn supports_horizontal_fanout_wall(shape: NodeShape) -> bool {
    matches!(
        shape,
        NodeShape::Rectangle
            | NodeShape::Rounded
            | NodeShape::Stadium
            | NodeShape::Hexagon
            | NodeShape::Database
            | NodeShape::Subroutine
            | NodeShape::Parallelogram
            | NodeShape::ParallelogramAlt
            | NodeShape::Trapezoid
            | NodeShape::TrapezoidAlt
    )
}

fn fanout_side_walls(shape: NodeShape, style: &StyleChars) -> Option<(char, char)> {
    match shape {
        NodeShape::Rectangle
        | NodeShape::Rounded
        | NodeShape::Database
        | NodeShape::Parallelogram
        | NodeShape::ParallelogramAlt
        | NodeShape::Trapezoid
        | NodeShape::TrapezoidAlt => Some((style.v, style.v)),
        NodeShape::Stadium => Some(('(', ')')),
        NodeShape::Hexagon => Some(('<', '>')),
        NodeShape::Subroutine => {
            let double_vertical = if style.tl == '┌' { '║' } else { '|' };
            Some((double_vertical, double_vertical))
        }
        NodeShape::Diamond
        | NodeShape::Circle
        | NodeShape::DoubleCircle
        | NodeShape::Asymmetric => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_boxlike(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    label_lines: &[String],
    top_left: char,
    top_right: char,
    bottom_left: char,
    bottom_right: char,
    top_h: char,
    bottom_h: char,
    left_side: char,
    right_side: char,
    style: &StyleChars,
    direction: Direction,
) {
    let height = height.max(3);
    let bottom_y = y + height - 1;

    // Top border - check for edge exits above (BT direction only)
    let mut bt_preferred_down_arm: Option<usize> = None;
    if direction == Direction::BT {
        let center_x = x + width / 2;
        let mut candidates: Vec<usize> = Vec::new();
        for i in 1..width.saturating_sub(1) {
            let pos_x = x + i;
            let above = if y > 0 { canvas.get(pos_x, y - 1) } else { ' ' };
            let above2 = if y > 1 { canvas.get(pos_x, y - 2) } else { ' ' };
            let above_is_vertical = is_vertical(above, style) || is_arrow(above);
            let above_is_corner_down = above == style.corner_dr || above == style.corner_dl;
            let above_is_junction = is_junction(above, style);
            let above2_is_vertical =
                is_vertical(above2, style) || is_arrow(above2) || is_junction(above2, style);
            let has_down_arm = above_is_vertical
                || ((above_is_corner_down || above_is_junction) && above2_is_vertical);
            if has_down_arm {
                candidates.push(pos_x);
            }
        }
        if let Some(best) = candidates.into_iter().min_by_key(|pos| {
            let dist = (*pos).abs_diff(center_x);
            (dist, *pos)
        }) {
            bt_preferred_down_arm = Some(best);
        }
    }

    canvas.set(x, y, top_left);
    for i in 1..width.saturating_sub(1) {
        let pos_x = x + i;
        let c = if direction == Direction::BT {
            let above = if y > 0 { canvas.get(pos_x, y - 1) } else { ' ' };
            let above2 = if y > 1 { canvas.get(pos_x, y - 2) } else { ' ' };
            // Only treat junctions/corners as a down arm if a vertical continues above them.
            let above_is_vertical = is_vertical(above, style) || is_arrow(above);
            let above_is_corner_down = above == style.corner_dr || above == style.corner_dl;
            let above_is_junction = is_junction(above, style);
            let above2_is_vertical =
                is_vertical(above2, style) || is_arrow(above2) || is_junction(above2, style);
            let has_down_arm = above_is_vertical
                || ((above_is_corner_down || above_is_junction) && above2_is_vertical);
            if has_down_arm && (bt_preferred_down_arm == Some(pos_x) || above_is_vertical) {
                style.junction_up
            } else {
                top_h
            }
        } else {
            top_h
        };
        canvas.set(pos_x, y, c);
    }
    canvas.set(x + width.saturating_sub(1), y, top_right);

    // Interior rows
    let inner_height = height.saturating_sub(2);
    let label_start_y = y + 1 + inner_height.saturating_sub(label_lines.len()) / 2;
    let label_area_width = width.saturating_sub(4);

    for j in 0..inner_height {
        let row_y = y + 1 + j;
        let left = if matches!(direction, Direction::LR | Direction::RL) {
            let outside = x.saturating_sub(1);
            let outside_char = canvas.get(outside, row_y);
            if is_horizontal(outside_char, style) || is_arrow(outside_char) {
                style.junction_left
            } else {
                left_side
            }
        } else {
            left_side
        };
        let right = if matches!(direction, Direction::LR | Direction::RL) {
            let outside = x.saturating_add(width);
            let outside_char = canvas.get(outside, row_y);
            if is_horizontal(outside_char, style) || is_arrow(outside_char) {
                style.junction_right
            } else {
                right_side
            }
        } else {
            right_side
        };
        canvas.set(x, row_y, left);
        for i in 1..width.saturating_sub(1) {
            canvas.set(x + i, row_y, ' ');
        }
        canvas.set(x + width.saturating_sub(1), row_y, right);
    }

    for (idx, line) in label_lines.iter().enumerate() {
        let row_y = label_start_y + idx;
        if row_y < y + 1 || row_y >= bottom_y {
            continue;
        }
        draw_centered_label(canvas, x + 1, row_y, label_area_width + 2, line);
    }

    // Bottom border - check for edge exits below (TD/TB direction only)
    canvas.set(x, bottom_y, bottom_left);
    for i in 1..width.saturating_sub(1) {
        let pos_x = x + i;
        let c = if matches!(direction, Direction::TD | Direction::TB) {
            let below = canvas.get(pos_x, bottom_y + 1);
            // Check for vertical lines, junctions, or corners with upward component
            let has_up_arm = is_vertical(below, style)
                || is_junction(below, style)
                || below == style.corner_ur  // ┘ - up/left corner
                || below == style.corner_ul; // └ - up/right corner
            if has_up_arm {
                style.junction_down
            } else {
                bottom_h
            }
        } else {
            bottom_h
        };
        canvas.set(pos_x, bottom_y, c);
    }
    canvas.set(x + width.saturating_sub(1), bottom_y, bottom_right);
}

/// Draw a rectangle box.
#[allow(clippy::too_many_arguments)]
fn draw_rectangle(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    label_lines: &[String],
    style: &StyleChars,
    direction: Direction,
) {
    draw_boxlike(
        canvas,
        x,
        y,
        width,
        height,
        label_lines,
        style.tl,
        style.tr,
        style.bl,
        style.br,
        style.h,
        style.h,
        style.v,
        style.v,
        style,
        direction,
    );
}

/// Draw a rounded box (uses round corner characters).
#[allow(clippy::too_many_arguments)]
fn draw_rounded(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    label_lines: &[String],
    style: &StyleChars,
    direction: Direction,
) {
    let (tl, tr, bl, br) = if style.tl == '┌' {
        ('╭', '╮', '╰', '╯')
    } else {
        ('(', ')', '(', ')')
    };
    draw_boxlike(
        canvas,
        x,
        y,
        width,
        height,
        label_lines,
        tl,
        tr,
        bl,
        br,
        style.h,
        style.h,
        style.v,
        style.v,
        style,
        direction,
    );
}

/// Draw a diamond/rhombus shape.
fn draw_diamond(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label: &str,
    style: &StyleChars,
    _direction: Direction,
) {
    let is_unicode = style.tl == '┌';
    let (top_left, top_right, bottom_left, bottom_right) = if is_unicode {
        ('╱', '╲', '╲', '╱')
    } else {
        ('/', '\\', '\\', '/')
    };

    // Keep one closed, three-row rhombus in every flow direction. A vertical
    // point-only rendering (`^`/`v` in ASCII or isolated `◇` cells in Unicode)
    // is visually indistinguishable from detached edge markers. The closed
    // contour preserves the existing measured height and route clearances
    // while making the Decision boundary legible as one shape.
    canvas.set(x, y, top_left);
    for i in 1..width.saturating_sub(1) {
        canvas.set(x + i, y, style.h);
    }
    canvas.set(x + width.saturating_sub(1), y, top_right);

    canvas.set(x, y + 1, '<');
    draw_centered_label(canvas, x + 1, y + 1, width.saturating_sub(2), label);
    canvas.set(x + width.saturating_sub(1), y + 1, '>');

    canvas.set(x, y + 2, bottom_left);
    for i in 1..width.saturating_sub(1) {
        canvas.set(x + i, y + 2, style.h);
    }
    canvas.set(x + width.saturating_sub(1), y + 2, bottom_right);
}

/// Draw a circle shape (elliptical approximation).
fn draw_circle(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label: &str,
    style: &StyleChars,
) {
    let is_unicode = style.tl == '┌';
    let (tl, tr, bl, br, h) = if is_unicode {
        ('╭', '╮', '╰', '╯', '─')
    } else {
        ('/', '\\', '\\', '/', '-')
    };

    canvas.set(x, y, tl);
    for i in 1..width - 1 {
        canvas.set(x + i, y, h);
    }
    canvas.set(x + width - 1, y, tr);

    canvas.set(x, y + 1, '(');
    draw_centered_label(canvas, x + 1, y + 1, width.saturating_sub(2), label);
    canvas.set(x + width - 1, y + 1, ')');

    canvas.set(x, y + 2, bl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        let below = canvas.get(pos_x, y + 3);
        let c = if is_vertical(below, style) || is_junction(below, style) {
            style.junction_down
        } else {
            h
        };
        canvas.set(pos_x, y + 2, c);
    }
    canvas.set(x + width - 1, y + 2, br);
}

/// Draw a double-circle shape: `(((label)))`.
///
/// Uses `((` / `))` side markers on the label row, with the same curved arcs
/// as `draw_circle` on the top and bottom rows. Visually distinct from a single
/// circle at a glance.
fn draw_double_circle(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label: &str,
    style: &StyleChars,
) {
    let width = width.max(7); // need at least 7 to fit "(( x ))"
    let is_unicode = style.tl == '┌';
    let (tl, tr, bl, br, h) = if is_unicode {
        ('╭', '╮', '╰', '╯', '─')
    } else {
        ('/', '\\', '\\', '/', '-')
    };

    // Top arc
    canvas.set(x, y, tl);
    for i in 1..width - 1 {
        canvas.set(x + i, y, h);
    }
    canvas.set(x + width - 1, y, tr);

    // Middle row: (( label ))
    canvas.set(x, y + 1, '(');
    canvas.set(x + 1, y + 1, '(');
    // content area is width - 6: 2 for "((" and 2 for "))" and 1 space each side
    let content_width = width.saturating_sub(6);
    draw_centered_label(canvas, x + 2, y + 1, content_width.saturating_add(2), label);
    canvas.set(x + width - 2, y + 1, ')');
    canvas.set(x + width - 1, y + 1, ')');

    // Bottom arc
    canvas.set(x, y + 2, bl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        let below = canvas.get(pos_x, y + 3);
        let c = if is_vertical(below, style) || is_junction(below, style) {
            style.junction_down
        } else {
            h
        };
        canvas.set(pos_x, y + 2, c);
    }
    canvas.set(x + width - 1, y + 2, br);
}

/// Draw a stadium/pill shape.
#[allow(clippy::too_many_arguments)]
fn draw_stadium(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    label_lines: &[String],
    style: &StyleChars,
    direction: Direction,
) {
    draw_boxlike(
        canvas,
        x,
        y,
        width,
        height,
        label_lines,
        style.tl,
        style.tr,
        style.bl,
        style.br,
        style.h,
        style.h,
        '(',
        ')',
        style,
        direction,
    );
}

/// Draw a hexagon shape.
#[allow(clippy::too_many_arguments)]
fn draw_hexagon(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    label_lines: &[String],
    style: &StyleChars,
    direction: Direction,
) {
    draw_boxlike(
        canvas,
        x,
        y,
        width,
        height,
        label_lines,
        '/',
        '\\',
        '\\',
        '/',
        style.h,
        style.h,
        '<',
        '>',
        style,
        direction,
    );
}

/// Draw a database/cylinder shape.
#[allow(clippy::too_many_arguments)]
fn draw_database(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    label_lines: &[String],
    style: &StyleChars,
    direction: Direction,
) {
    let is_unicode = style.tl == '┌';
    let h = if is_unicode { '─' } else { '-' };
    draw_boxlike(
        canvas,
        x,
        y,
        width,
        height,
        label_lines,
        '/',
        '\\',
        '\\',
        '/',
        h,
        h,
        style.v,
        style.v,
        style,
        direction,
    );
}

/// Draw a subroutine box (double vertical lines on sides).
#[allow(clippy::too_many_arguments)]
fn draw_subroutine(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    label_lines: &[String],
    style: &StyleChars,
    direction: Direction,
) {
    let dv = if style.tl == '┌' { '║' } else { '|' };
    draw_boxlike(
        canvas,
        x,
        y,
        width,
        height,
        label_lines,
        style.tl,
        style.tr,
        style.bl,
        style.br,
        style.h,
        style.h,
        dv,
        dv,
        style,
        direction,
    );
}

/// Draw an asymmetric/flag shape.
#[allow(clippy::too_many_arguments)]
fn draw_asymmetric(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    label_lines: &[String],
    style: &StyleChars,
    _direction: Direction,
) {
    let height = height.max(3);
    let bottom_y = y + height - 1;
    let (upper_left, lower_left) = if style.tl == '┌' {
        ('╱', '╲')
    } else {
        ('/', '\\')
    };

    // Mermaid's asymmetric/Flag geometry is a left-center point with
    // diagonal shoulders and a flat right side. Keep these contour cells
    // separate from the rectangular junction policy used by draw_boxlike.
    canvas.set(x, y, ' ');
    if width > 1 {
        canvas.set(x + 1, y, upper_left);
    }
    for i in 2..width.saturating_sub(1) {
        canvas.set(x + i, y, style.h);
    }
    canvas.set(x + width.saturating_sub(1), y, style.tr);

    let inner_height = height.saturating_sub(2);
    let label_start_y = y + 1 + inner_height.saturating_sub(label_lines.len()) / 2;
    let label_area_width = width.saturating_sub(4);
    for j in 0..inner_height {
        let row_y = y + 1 + j;
        let left = if row_y == label_start_y { '<' } else { ' ' };
        canvas.set(x, row_y, left);
        for i in 1..width.saturating_sub(1) {
            canvas.set(x + i, row_y, ' ');
        }
        canvas.set(x + width.saturating_sub(1), row_y, style.v);
    }

    for (idx, line) in label_lines.iter().enumerate() {
        let row_y = label_start_y + idx;
        if row_y < y + 1 || row_y >= bottom_y {
            continue;
        }
        draw_centered_label(canvas, x + 1, row_y, label_area_width + 2, line);
    }

    canvas.set(x, bottom_y, ' ');
    if width > 1 {
        canvas.set(x + 1, bottom_y, lower_left);
    }
    for i in 2..width.saturating_sub(1) {
        canvas.set(x + i, bottom_y, style.h);
    }
    canvas.set(x + width.saturating_sub(1), bottom_y, style.br);
}

/// Draw a parallelogram node (lean-right `[/label/]` or lean-left `[\label\]`).
///
/// Both the left and right sides use the same diagonal character, giving
/// the illusion of a slanted box. Edge connectors still attach at the
/// rectangular bounding box edges, keeping routing unchanged.
#[allow(clippy::too_many_arguments)]
fn draw_parallelogram(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    label_lines: &[String],
    style: &StyleChars,
    direction: Direction,
    lean_right: bool,
) {
    let is_unicode = style.tl == '┌';
    let (fwd, back) = if is_unicode {
        ('╱', '╲')
    } else {
        ('/', '\\')
    };
    let corner = if lean_right { fwd } else { back };
    draw_boxlike(
        canvas,
        x,
        y,
        width,
        height,
        label_lines,
        corner,
        corner,
        corner,
        corner,
        style.h,
        style.h,
        style.v,
        style.v,
        style,
        direction,
    );
}

/// Draw a trapezoid node.
///
/// Wider-top (`[/label\]`): top corners `/─\`, bottom corners `\─/`.
/// Wider-bottom (`[\label/]`): top corners `\─/`, bottom corners `/─\`.
#[allow(clippy::too_many_arguments)]
fn draw_trapezoid(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    label_lines: &[String],
    style: &StyleChars,
    direction: Direction,
    wider_top: bool,
) {
    let is_unicode = style.tl == '┌';
    let (fwd, back) = if is_unicode {
        ('╱', '╲')
    } else {
        ('/', '\\')
    };
    let (tl, tr, bl, br) = if wider_top {
        // /─\ on top, \─/ on bottom  →  wider at the top
        (fwd, back, back, fwd)
    } else {
        // \─/ on top, /─\ on bottom  →  wider at the bottom
        (back, fwd, fwd, back)
    };
    draw_boxlike(
        canvas,
        x,
        y,
        width,
        height,
        label_lines,
        tl,
        tr,
        bl,
        br,
        style.h,
        style.h,
        style.v,
        style.v,
        style,
        direction,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Direction, NodeShape, Rectangle};
    use crate::render::canvas::Canvas;
    use crate::style::{ASCII_CHARS, UNICODE_CHARS};

    fn mk_canvas(w: usize, h: usize) -> Canvas {
        Canvas::new(w, h)
    }

    fn lines(s: &str) -> Vec<String> {
        vec![s.to_string()]
    }

    // =========================================================================
    // draw_subgraph
    // =========================================================================

    #[test]
    fn subgraph_draws_corners_ascii() {
        let mut c = mk_canvas(10, 6);
        let r = Rectangle::new(0, 0, 10, 6);
        draw_subgraph(&mut c, &r, None, &ASCII_CHARS, Direction::TD);
        assert_eq!(c.get(0, 0), '+');
        assert_eq!(c.get(9, 0), '+');
        assert_eq!(c.get(0, 5), '+');
        assert_eq!(c.get(9, 5), '+');
    }

    #[test]
    fn subgraph_draws_corners_unicode() {
        let mut c = mk_canvas(10, 6);
        let r = Rectangle::new(0, 0, 10, 6);
        draw_subgraph(&mut c, &r, None, &UNICODE_CHARS, Direction::TD);
        assert_eq!(c.get(0, 0), '┌');
        assert_eq!(c.get(9, 0), '┐');
        assert_eq!(c.get(0, 5), '└');
        assert_eq!(c.get(9, 5), '┘');
    }

    #[test]
    fn subgraph_invalid_rect_is_noop() {
        let mut c = mk_canvas(10, 6);
        let r = Rectangle::new(0, 0, 0, 6); // zero width — invalid
        draw_subgraph(&mut c, &r, None, &ASCII_CHARS, Direction::TD);
        assert_eq!(c.get(0, 0), ' ');
    }

    #[test]
    fn subgraph_title_appears_in_td() {
        let mut c = mk_canvas(20, 5);
        let r = Rectangle::new(0, 0, 20, 5);
        draw_subgraph(&mut c, &r, Some("Grp"), &ASCII_CHARS, Direction::TD);
        let row: String = (0..20).map(|x| c.get(x, 1)).collect();
        assert!(row.contains("Grp"), "title not found in row: {row:?}");
    }

    #[test]
    fn subgraph_title_too_long_is_skipped() {
        let mut c = mk_canvas(8, 4);
        let r = Rectangle::new(0, 0, 8, 4);
        draw_subgraph(
            &mut c,
            &r,
            Some("VeryLongTitle"),
            &ASCII_CHARS,
            Direction::TD,
        );
        assert_eq!(c.get(0, 0), '+');
        let row: String = (1..7).map(|x| c.get(x, 0)).collect();
        assert!(!row.contains('['), "unexpected title bracket in: {row:?}");
    }

    // =========================================================================
    // draw_node dispatch
    // =========================================================================

    #[test]
    fn draw_node_rectangle_corners_ascii() {
        let mut c = mk_canvas(12, 5);
        draw_node(
            &mut c,
            0,
            0,
            12,
            5,
            &lines("hi"),
            NodeShape::Rectangle,
            &ASCII_CHARS,
            Direction::TD,
        );
        assert_eq!(c.get(0, 0), '+');
        assert_eq!(c.get(11, 0), '+');
        assert_eq!(c.get(0, 4), '+');
        assert_eq!(c.get(11, 4), '+');
    }

    #[test]
    fn draw_node_rounded_corners_ascii() {
        let mut c = mk_canvas(12, 5);
        draw_node(
            &mut c,
            0,
            0,
            12,
            5,
            &lines("hi"),
            NodeShape::Rounded,
            &ASCII_CHARS,
            Direction::TD,
        );
        assert_eq!(c.get(0, 0), '(');
        assert_eq!(c.get(11, 0), ')');
        assert_eq!(c.get(0, 4), '(');
        assert_eq!(c.get(11, 4), ')');
    }

    #[test]
    fn draw_node_rectangle_label_written() {
        let mut c = mk_canvas(12, 3);
        draw_node(
            &mut c,
            0,
            0,
            12,
            3,
            &lines("hi"),
            NodeShape::Rectangle,
            &ASCII_CHARS,
            Direction::TD,
        );
        let row: String = (0..12).map(|x| c.get(x, 1)).collect();
        assert!(
            row.contains("hi"),
            "label not found in interior row: {row:?}"
        );
    }

    #[test]
    fn wide_cjk_label_preserves_terminal_box_width() {
        let mut c = mk_canvas(20, 3);
        draw_node(
            &mut c,
            0,
            0,
            12,
            3,
            &lines("日本語"),
            NodeShape::Rectangle,
            &UNICODE_CHARS,
            Direction::LR,
        );

        let output = c.to_string_cropped(0);
        let rows: Vec<_> = output.lines().collect();
        assert_eq!(rows.len(), 3);
        assert!(rows
            .iter()
            .all(|row| crate::style::display_width(row) == 12));
        assert!(rows[1].contains("日本語"), "wide label missing: {output:?}");
        assert_eq!(rows[0].chars().next(), Some('┌'));
        assert_eq!(rows[0].chars().last(), Some('┐'));
        assert_eq!(rows[1].chars().last(), Some('│'));
    }

    #[test]
    fn fanout_source_policy_preserves_horizontal_side_wall_in_both_directions_and_styles() {
        for style in [&ASCII_CHARS, &UNICODE_CHARS] {
            for direction in [Direction::LR, Direction::RL] {
                let mut preserved = mk_canvas(20, 5);
                let (outside_x, node_x, wall_x, expected_wall, expected_junction) = match direction
                {
                    Direction::LR => (12, 4, 11, style.v, style.junction_right),
                    Direction::RL => (3, 4, 4, style.v, style.junction_left),
                    Direction::TD | Direction::TB | Direction::BT => unreachable!(),
                };
                preserved.set(outside_x, 2, style.edge_h);
                draw_node_with_fanout_policy(
                    &mut preserved,
                    node_x,
                    1,
                    8,
                    3,
                    &lines("Hub"),
                    NodeShape::Rectangle,
                    style,
                    direction,
                    true,
                );
                assert_eq!(
                    preserved.get(wall_x, 2),
                    expected_wall,
                    "fanout source wall was not preserved for {style:?} {direction:?}"
                );

                let mut ordinary = mk_canvas(20, 5);
                ordinary.set(outside_x, 2, style.edge_h);
                draw_node(
                    &mut ordinary,
                    node_x,
                    1,
                    8,
                    3,
                    &lines("Hub"),
                    NodeShape::Rectangle,
                    style,
                    direction,
                );
                assert_eq!(
                    ordinary.get(wall_x, 2),
                    expected_junction,
                    "ordinary source port changed for {style:?} {direction:?}"
                );
            }
        }
    }

    #[test]
    fn draw_node_all_shapes_no_panic() {
        let shapes = [
            NodeShape::Rectangle,
            NodeShape::Rounded,
            NodeShape::Diamond,
            NodeShape::Circle,
            NodeShape::DoubleCircle,
            NodeShape::Stadium,
            NodeShape::Hexagon,
            NodeShape::Database,
            NodeShape::Subroutine,
            NodeShape::Asymmetric,
            NodeShape::Parallelogram,
            NodeShape::ParallelogramAlt,
            NodeShape::Trapezoid,
            NodeShape::TrapezoidAlt,
        ];
        for shape in shapes {
            let mut c = mk_canvas(20, 7);
            draw_node(
                &mut c,
                0,
                0,
                20,
                7,
                &lines("test"),
                shape,
                &UNICODE_CHARS,
                Direction::TD,
            );
            let non_space = (0..20).any(|x| (0..7).any(|y| c.get(x, y) != ' '));
            assert!(non_space, "shape {shape:?} produced blank canvas");
        }
    }

    #[test]
    fn diamond_closed_contour_preserves_corners_when_route_is_adjacent() {
        for style in [&ASCII_CHARS, &UNICODE_CHARS] {
            let mut c = mk_canvas(20, 7);
            let center = 10;
            c.set(center, 4, style.edge_v);
            draw_node(
                &mut c,
                0,
                1,
                20,
                3,
                &lines("Decision"),
                NodeShape::Diamond,
                style,
                Direction::TD,
            );

            let (top_left, top_right, bottom_left, bottom_right) = if style.tl == '┌' {
                ('╱', '╲', '╲', '╱')
            } else {
                ('/', '\\', '\\', '/')
            };
            assert_eq!(c.get(0, 1), top_left);
            assert_eq!(c.get(19, 1), top_right);
            assert_eq!(c.get(center, 1), style.h);
            assert_eq!(c.get(0, 3), bottom_left);
            assert_eq!(c.get(19, 3), bottom_right);
            assert_eq!(c.get(center, 3), style.h);
        }
    }

    #[test]
    fn diamond_horizontal_uses_flow_aligned_closed_contour() {
        for style in [&ASCII_CHARS, &UNICODE_CHARS] {
            let mut c = mk_canvas(20, 5);
            draw_node(
                &mut c,
                1,
                1,
                12,
                3,
                &lines("Decision"),
                NodeShape::Diamond,
                style,
                Direction::RL,
            );

            let (top_left, top_right, bottom_left, bottom_right) = if style.tl == '┌' {
                ('╱', '╲', '╲', '╱')
            } else {
                ('/', '\\', '\\', '/')
            };
            assert_eq!(c.get(1, 1), top_left);
            assert_eq!(c.get(12, 1), top_right);
            assert_eq!(c.get(1, 3), bottom_left);
            assert_eq!(c.get(12, 3), bottom_right);
            assert_ne!(c.get(12, 1), if style.tl == '┌' { '◇' } else { '^' });
            assert_ne!(c.get(12, 3), if style.tl == '┌' { '◇' } else { 'v' });
        }
    }

    #[test]
    fn asymmetric_flag_preserves_point_and_shoulders_when_routes_are_adjacent() {
        for style in [&ASCII_CHARS, &UNICODE_CHARS] {
            for direction in [
                Direction::TD,
                Direction::TB,
                Direction::BT,
                Direction::LR,
                Direction::RL,
            ] {
                let mut c = mk_canvas(32, 16);
                let x = 8;
                let y = 6;
                let width = 12;
                let height = 3;
                let center_x = x + width / 2;
                let center_y = y + height / 2;

                match direction {
                    Direction::TD | Direction::TB => c.set(center_x, y + height, style.edge_v),
                    Direction::BT => c.set(center_x, y - 1, style.edge_v),
                    Direction::LR | Direction::RL => {
                        c.set(x - 1, center_y, style.edge_h);
                        c.set(x + width, center_y, style.edge_h);
                    }
                }

                draw_node(
                    &mut c,
                    x,
                    y,
                    width,
                    height,
                    &lines("Flag"),
                    NodeShape::Asymmetric,
                    style,
                    direction,
                );

                let (upper_left, lower_left) = if style.tl == '┌' {
                    ('╱', '╲')
                } else {
                    ('/', '\\')
                };
                assert_eq!(c.get(x, y), ' ');
                assert_eq!(c.get(x + 1, y), upper_left);
                assert_eq!(c.get(x, center_y), '<');
                assert_eq!(c.get(x + 1, y + height - 1), lower_left);
                assert_eq!(c.get(x + width - 1, center_y), style.v);
                assert_eq!(c.get(center_x, y), style.h);
                assert_eq!(c.get(center_x, y + height - 1), style.h);
                assert_ne!(c.get(x, center_y), style.junction_left);
                assert_ne!(c.get(x + width - 1, center_y), style.junction_right);
            }
        }
    }

    // =========================================================================
    // draw_boxlike — TD junction at bottom border
    // =========================================================================

    #[test]
    fn boxlike_td_junction_placed_on_bottom_border() {
        let mut c = mk_canvas(14, 7);
        let style = &UNICODE_CHARS;
        c.set(7, 3, style.edge_v);
        draw_node(
            &mut c,
            0,
            0,
            14,
            3,
            &lines("A"),
            NodeShape::Rectangle,
            style,
            Direction::TD,
        );
        let ch = c.get(7, 2);
        assert_eq!(
            ch, style.junction_down,
            "expected junction_down at bottom border col 7, got {ch:?}"
        );
    }

    #[test]
    fn boxlike_td_no_junction_without_down_arm() {
        let mut c = mk_canvas(14, 3);
        let style = &UNICODE_CHARS;
        draw_node(
            &mut c,
            0,
            0,
            14,
            3,
            &lines("A"),
            NodeShape::Rectangle,
            style,
            Direction::TD,
        );
        let ch = c.get(7, 2);
        assert_eq!(ch, style.h, "expected plain h-line, got {ch:?}");
    }

    // =========================================================================
    // draw_boxlike — BT junction at top border
    // =========================================================================

    #[test]
    fn boxlike_bt_junction_placed_on_top_border() {
        let mut c = mk_canvas(14, 7);
        let style = &UNICODE_CHARS;
        c.set(7, 1, style.edge_v);
        draw_node(
            &mut c,
            0,
            2,
            14,
            3,
            &lines("A"),
            NodeShape::Rectangle,
            style,
            Direction::BT,
        );
        let ch = c.get(7, 2);
        assert_eq!(ch, style.junction_up, "expected junction_up, got {ch:?}");
    }

    #[test]
    fn boxlike_bt_no_junction_without_arm_above() {
        let mut c = mk_canvas(14, 7);
        let style = &UNICODE_CHARS;
        draw_node(
            &mut c,
            0,
            2,
            14,
            3,
            &lines("A"),
            NodeShape::Rectangle,
            style,
            Direction::BT,
        );
        let ch = c.get(7, 2);
        assert_eq!(ch, style.h, "expected plain h-line, got {ch:?}");
    }

    // =========================================================================
    // Trapezoid and Parallelogram orientation variants
    // =========================================================================

    #[test]
    fn trapezoid_variants_differ_at_top_left_corner() {
        let mut c1 = mk_canvas(16, 5);
        let mut c2 = mk_canvas(16, 5);
        draw_node(
            &mut c1,
            0,
            0,
            16,
            5,
            &lines("x"),
            NodeShape::Trapezoid,
            &ASCII_CHARS,
            Direction::TD,
        );
        draw_node(
            &mut c2,
            0,
            0,
            16,
            5,
            &lines("x"),
            NodeShape::TrapezoidAlt,
            &ASCII_CHARS,
            Direction::TD,
        );
        let top1 = c1.get(0, 0);
        let top2 = c2.get(0, 0);
        assert_ne!(
            top1, top2,
            "Trapezoid variants should differ at top-left corner"
        );
    }

    #[test]
    fn parallelogram_variants_differ_at_top_left_corner() {
        let mut c1 = mk_canvas(16, 5);
        let mut c2 = mk_canvas(16, 5);
        draw_node(
            &mut c1,
            0,
            0,
            16,
            5,
            &lines("x"),
            NodeShape::Parallelogram,
            &ASCII_CHARS,
            Direction::TD,
        );
        draw_node(
            &mut c2,
            0,
            0,
            16,
            5,
            &lines("x"),
            NodeShape::ParallelogramAlt,
            &ASCII_CHARS,
            Direction::TD,
        );
        let tl1 = c1.get(0, 0);
        let tl2 = c2.get(0, 0);
        assert_ne!(
            tl1, tl2,
            "Parallelogram variants should differ at top-left corner"
        );
    }
}
