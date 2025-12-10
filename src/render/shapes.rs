//! Box drawing primitives for node rendering.
//!
//! Supports 9 node shapes with direction-aware junction placement.

use crate::graph::{Direction, NodeShape};
use crate::style::StyleChars;

use super::canvas::{is_vertical, Canvas};

/// Draw a subgraph bounding box with optional title.
pub fn draw_subgraph(
    canvas: &mut Canvas,
    rect: &crate::graph::Rectangle,
    title: Option<&str>,
    style: &StyleChars,
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
        if title_fmt.len() <= width.saturating_sub(2) {
            let start_x = x + (width - title_fmt.len()) / 2;
            for (i, c) in title_fmt.chars().enumerate() {
                if start_x + i < canvas.width {
                    canvas.set(start_x + i, y, c);
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
    label: &str,
    shape: NodeShape,
    style: &StyleChars,
    direction: Direction,
) {
    match shape {
        NodeShape::Rectangle => draw_rectangle(canvas, x, y, width, label, style, direction),
        NodeShape::Rounded => draw_rounded(canvas, x, y, width, label, style, direction),
        NodeShape::Diamond => draw_diamond(canvas, x, y, width, label, style),
        NodeShape::Circle => draw_circle(canvas, x, y, width, label, style),
        NodeShape::Stadium => draw_stadium(canvas, x, y, width, label, style, direction),
        NodeShape::Hexagon => draw_hexagon(canvas, x, y, width, label, style, direction),
        NodeShape::Database => draw_database(canvas, x, y, width, label, style, direction),
        NodeShape::Subroutine => draw_subroutine(canvas, x, y, width, label, style, direction),
        NodeShape::Asymmetric => draw_asymmetric(canvas, x, y, width, label, style, direction),
        // Parallelogram and trapezoid fall back to rectangle
        NodeShape::Parallelogram
        | NodeShape::ParallelogramAlt
        | NodeShape::Trapezoid
        | NodeShape::TrapezoidAlt => draw_rectangle(canvas, x, y, width, label, style, direction),
    }
}

/// Draw a rectangle box.
fn draw_rectangle(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label: &str,
    style: &StyleChars,
    direction: Direction,
) {
    // Top border - check for edge exits above (BT direction only)
    canvas.set(x, y, style.tl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if direction == Direction::BT {
            let above = if y > 0 { canvas.get(pos_x, y - 1) } else { ' ' };
            if is_vertical(above, style) {
                style.junction_up
            } else {
                style.h
            }
        } else {
            style.h
        };
        canvas.set(pos_x, y, c);
    }
    canvas.set(x + width - 1, y, style.tr);

    // Middle row with label
    canvas.set(x, y + 1, style.v);
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, style.v);

    // Bottom border - check for edge exits below (TD/TB direction only)
    canvas.set(x, y + 2, style.bl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if matches!(direction, Direction::TD | Direction::TB) {
            let below = canvas.get(pos_x, y + 3);
            if is_vertical(below, style) {
                style.junction_down
            } else {
                style.h
            }
        } else {
            style.h
        };
        canvas.set(pos_x, y + 2, c);
    }
    canvas.set(x + width - 1, y + 2, style.br);
}

/// Draw a rounded box (uses round corner characters).
fn draw_rounded(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label: &str,
    style: &StyleChars,
    direction: Direction,
) {
    let (tl, tr, bl, br) = if style.tl == '┌' {
        ('╭', '╮', '╰', '╯')
    } else {
        ('(', ')', '(', ')')
    };

    canvas.set(x, y, tl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if direction == Direction::BT {
            let above = if y > 0 { canvas.get(pos_x, y - 1) } else { ' ' };
            if is_vertical(above, style) {
                style.junction_up
            } else {
                style.h
            }
        } else {
            style.h
        };
        canvas.set(pos_x, y, c);
    }
    canvas.set(x + width - 1, y, tr);

    canvas.set(x, y + 1, style.v);
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, style.v);

    canvas.set(x, y + 2, bl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if matches!(direction, Direction::TD | Direction::TB) {
            let below = canvas.get(pos_x, y + 3);
            if is_vertical(below, style) {
                style.junction_down
            } else {
                style.h
            }
        } else {
            style.h
        };
        canvas.set(pos_x, y + 2, c);
    }
    canvas.set(x + width - 1, y + 2, br);
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
    let bottom_char = if is_vertical(below, style) {
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
        let c = if is_vertical(below, style) {
            style.junction_down
        } else {
            h
        };
        canvas.set(pos_x, y + 2, c);
    }
    canvas.set(x + width - 1, y + 2, br);
}

/// Draw a stadium/pill shape.
fn draw_stadium(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label: &str,
    style: &StyleChars,
    direction: Direction,
) {
    canvas.set(x, y, style.tl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if direction == Direction::BT {
            let above = if y > 0 { canvas.get(pos_x, y - 1) } else { ' ' };
            if is_vertical(above, style) { style.junction_up } else { style.h }
        } else {
            style.h
        };
        canvas.set(pos_x, y, c);
    }
    canvas.set(x + width - 1, y, style.tr);

    canvas.set(x, y + 1, '(');
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, ')');

    canvas.set(x, y + 2, style.bl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if matches!(direction, Direction::TD | Direction::TB) {
            let below = canvas.get(pos_x, y + 3);
            if is_vertical(below, style) { style.junction_down } else { style.h }
        } else {
            style.h
        };
        canvas.set(pos_x, y + 2, c);
    }
    canvas.set(x + width - 1, y + 2, style.br);
}

/// Draw a hexagon shape.
fn draw_hexagon(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label: &str,
    style: &StyleChars,
    direction: Direction,
) {
    canvas.set(x, y, '/');
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if direction == Direction::BT {
            let above = if y > 0 { canvas.get(pos_x, y - 1) } else { ' ' };
            if is_vertical(above, style) { style.junction_up } else { style.h }
        } else {
            style.h
        };
        canvas.set(pos_x, y, c);
    }
    canvas.set(x + width - 1, y, '\\');

    canvas.set(x, y + 1, '<');
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, '>');

    canvas.set(x, y + 2, '\\');
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if matches!(direction, Direction::TD | Direction::TB) {
            let below = canvas.get(pos_x, y + 3);
            if is_vertical(below, style) { style.junction_down } else { style.h }
        } else {
            style.h
        };
        canvas.set(pos_x, y + 2, c);
    }
    canvas.set(x + width - 1, y + 2, '/');
}

/// Draw a database/cylinder shape.
fn draw_database(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label: &str,
    style: &StyleChars,
    direction: Direction,
) {
    let is_unicode = style.tl == '┌';
    let h = if is_unicode { '─' } else { '-' };

    canvas.set(x, y, '/');
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if direction == Direction::BT {
            let above = if y > 0 { canvas.get(pos_x, y - 1) } else { ' ' };
            if is_vertical(above, style) { style.junction_up } else { h }
        } else {
            h
        };
        canvas.set(pos_x, y, c);
    }
    canvas.set(x + width - 1, y, '\\');

    canvas.set(x, y + 1, style.v);
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, style.v);

    canvas.set(x, y + 2, '\\');
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if matches!(direction, Direction::TD | Direction::TB) {
            let below = canvas.get(pos_x, y + 3);
            if is_vertical(below, style) { style.junction_down } else { h }
        } else {
            h
        };
        canvas.set(pos_x, y + 2, c);
    }
    canvas.set(x + width - 1, y + 2, '/');
}

/// Draw a subroutine box (double vertical lines on sides).
fn draw_subroutine(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label: &str,
    style: &StyleChars,
    direction: Direction,
) {
    let dv = if style.tl == '┌' { '║' } else { '|' };

    canvas.set(x, y, style.tl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if direction == Direction::BT {
            let above = if y > 0 { canvas.get(pos_x, y - 1) } else { ' ' };
            if is_vertical(above, style) { style.junction_up } else { style.h }
        } else {
            style.h
        };
        canvas.set(pos_x, y, c);
    }
    canvas.set(x + width - 1, y, style.tr);

    canvas.set(x, y + 1, dv);
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, dv);

    canvas.set(x, y + 2, style.bl);
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if matches!(direction, Direction::TD | Direction::TB) {
            let below = canvas.get(pos_x, y + 3);
            if is_vertical(below, style) { style.junction_down } else { style.h }
        } else {
            style.h
        };
        canvas.set(pos_x, y + 2, c);
    }
    canvas.set(x + width - 1, y + 2, style.br);
}

/// Draw an asymmetric/flag shape.
fn draw_asymmetric(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    label: &str,
    style: &StyleChars,
    direction: Direction,
) {
    canvas.set(x, y, '>');
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if direction == Direction::BT {
            let above = if y > 0 { canvas.get(pos_x, y - 1) } else { ' ' };
            if is_vertical(above, style) { style.junction_up } else { style.h }
        } else {
            style.h
        };
        canvas.set(pos_x, y, c);
    }
    canvas.set(x + width - 1, y, style.tr);

    canvas.set(x, y + 1, ' ');
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, style.v);

    canvas.set(x, y + 2, '>');
    for i in 1..width - 1 {
        let pos_x = x + i;
        let c = if matches!(direction, Direction::TD | Direction::TB) {
            let below = canvas.get(pos_x, y + 3);
            if is_vertical(below, style) { style.junction_down } else { style.h }
        } else {
            style.h
        };
        canvas.set(pos_x, y + 2, c);
    }
    canvas.set(x + width - 1, y + 2, style.br);
}
