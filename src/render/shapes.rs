//! Box drawing primitives for node rendering.
//!
//! Supports 9 node shapes with direction-aware junction placement.

use crate::graph::{Direction, NodeShape};
use crate::style::StyleChars;

use super::canvas::{is_arrow, is_junction, is_vertical, Canvas};

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

    // Draw title if present
    if let Some(t) = title {
        // Format: [  Title  ] centered on top edge
        let title_fmt = format!("[  {}  ]", t);
        let title_len = title_fmt.chars().count();
        if title_len <= width.saturating_sub(2) {
            let start_x = x + (width - title_len) / 2;
            let title_y = if matches!(direction, Direction::BT) && y + 1 < y + height - 1 {
                y + 1
            } else {
                y
            };
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
        NodeShape::Diamond => draw_diamond(canvas, x, y, width, label, style),
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
            if has_down_arm && bt_preferred_down_arm == Some(pos_x) {
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
        canvas.set(x, row_y, left_side);
        for i in 1..width.saturating_sub(1) {
            canvas.set(x + i, row_y, ' ');
        }
        canvas.set(x + width.saturating_sub(1), row_y, right_side);
    }

    for (idx, line) in label_lines.iter().enumerate() {
        let row_y = label_start_y + idx;
        if row_y < y + 1 || row_y >= bottom_y {
            continue;
        }
        let padded_label = format!(" {:^w$} ", line, w = label_area_width);
        for (i, c) in padded_label
            .chars()
            .take(width.saturating_sub(2))
            .enumerate()
        {
            canvas.set(x + 1 + i, row_y, c);
        }
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
) {
    let center = x + width / 2;
    let is_unicode = style.tl == '┌';
    let point_char = if is_unicode { '◇' } else { 'v' };

    canvas.set(center, y, if is_unicode { '◇' } else { '^' });

    canvas.set(x, y + 1, '<');
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, '>');

    let below = canvas.get(center, y + 3);
    let bottom_char = if is_vertical(below, style) || is_junction(below, style) {
        style.junction_down
    } else {
        point_char
    };
    canvas.set(center, y + 2, bottom_char);

    for i in 1..center - x {
        canvas.set(x + i, y + 2, ' ');
        canvas.set(center + i, y + 2, ' ');
    }
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
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
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
    let padded_label = format!(" {:^width$} ", label, width = content_width);
    for (i, c) in padded_label.chars().take(width - 4).enumerate() {
        canvas.set(x + 2 + i, y + 1, c);
    }
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
    direction: Direction,
) {
    draw_boxlike(
        canvas,
        x,
        y,
        width,
        height,
        label_lines,
        '>',
        style.tr,
        '>',
        style.br,
        style.h,
        style.h,
        ' ',
        style.v,
        style,
        direction,
    );
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
