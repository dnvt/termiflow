//! Canvas rendering - 2D character grid
//!
//! Handles:
//! - Box drawing for nodes
//! - Edge routing with mid-y spreading
//! - Crossing detection
//! - Back-edge gutter rendering
//!
//! See SPEC §3 for rendering details

use anyhow::Result;
use crate::graph::{Graph, Node};
use crate::style::{
    truncate_label, BorderStyle, StyleChars, BOX_HEIGHT, COL_SPACING, MAX_CANVAS_HEIGHT,
    MAX_CANVAS_WIDTH, RIGHT_GUTTER, ROW_SPACING,
};

/// 2D character canvas for rendering
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    grid: Vec<Vec<char>>,
}

impl Canvas {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            grid: vec![vec![' '; width]; height],
        }
    }

    /// Set a character at position (x, y)
    pub fn set(&mut self, x: usize, y: usize, c: char) {
        if x < self.width && y < self.height {
            self.grid[y][x] = c;
        }
    }

    /// Get character at position (x, y)
    pub fn get(&self, x: usize, y: usize) -> char {
        if x < self.width && y < self.height {
            self.grid[y][x]
        } else {
            ' '
        }
    }

    /// Set edge character with crossing/junction detection
    pub fn set_edge_char(&mut self, x: usize, y: usize, new_char: char, style: &StyleChars) {
        let existing = self.get(x, y);

        let final_char = match (existing, new_char) {
            (' ' | '\0', c) => c,
            // Preserve arrows
            (c, _) if is_arrow(c) => c,
            // Cross when perpendicular lines meet
            (c, n) if is_horizontal(c, style) && is_vertical(n, style) => style.cross,
            (c, n) if is_vertical(c, style) && is_horizontal(n, style) => style.cross,
            // Preserve corners when lines try to overwrite them
            // (corners already have the line connectivity built in)
            (c, n) if is_corner(c, style) && (is_horizontal(n, style) || is_vertical(n, style)) => c,
            // Junction: when corner meets another corner going same direction
            (c, n) if is_corner(c, style) && is_corner(n, style) => {
                // Two corners meeting - create junction based on their directions
                if is_corner_down(c, style) && is_corner_down(n, style) {
                    style.junction_down  // ┬ - both going down
                } else if !is_corner_down(c, style) && !is_corner_down(n, style) {
                    style.junction_up    // ┴ - both going up
                } else {
                    // Mixed directions - use a cross or keep one corner
                    style.cross
                }
            },
            _ => new_char,
        };

        self.set(x, y, final_char);
    }

    /// Convert canvas to string
    pub fn to_string(&self) -> String {
        self.grid
            .iter()
            .map(|row| row.iter().collect::<String>().trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Check if a node is within visible canvas bounds
    pub fn is_visible(&self, node: &Node) -> bool {
        node.x + node.width <= self.width && node.y + BOX_HEIGHT <= self.height
    }
}

fn is_horizontal(c: char, _style: &StyleChars) -> bool {
    matches!(c, 
        // ASCII
        '-' | 
        // Unicode
        '─' | 
        // Double
        '═' | 
        // Heavy
        '━' | 
        // Blocks
        '█'
    )
}

fn is_vertical(c: char, _style: &StyleChars) -> bool {
    matches!(c, 
        // ASCII
        '|' | ':' |
        // Unicode
        '│' | 
        // Double
        '║' | 
        // Heavy
        '┃' | 
        // Blocks
        '█'
    )
}

fn is_arrow(c: char) -> bool {
    matches!(c, 'v' | '^' | '<' | '>' | '▼' | '▲' | '◀' | '▶')
}

fn is_corner(c: char, style: &StyleChars) -> bool {
    c == style.corner_dr || c == style.corner_dl || c == style.corner_ur || c == style.corner_ul
}

fn is_corner_down(c: char, style: &StyleChars) -> bool {
    c == style.corner_dr || c == style.corner_dl
}

#[allow(dead_code)]
fn is_corner_right(c: char, style: &StyleChars) -> bool {
    c == style.corner_dr || c == style.corner_ur
}

/// Render a graph to a string
pub fn render(graph: &Graph, config: &crate::config::Config) -> Result<String> {
    if graph.nodes.is_empty() {
        return Ok(String::new());
    }

    // Calculate canvas size from laid-out nodes
    let max_right = graph
        .nodes
        .iter()
        .map(|n| n.x + n.width)
        .max()
        .unwrap_or(0);
    let max_bottom = graph
        .nodes
        .iter()
        .map(|n| n.y + BOX_HEIGHT)
        .max()
        .unwrap_or(0);
    let gutter = if graph.has_cycles() { RIGHT_GUTTER } else { 0 };

    let mut width = max_right + COL_SPACING + gutter;
    if width > MAX_CANVAS_WIDTH {
        width = MAX_CANVAS_WIDTH;
        eprintln!(
            "termiflow: warning: Graph too wide ({} chars), clipping to {}",
            max_right + COL_SPACING + gutter,
            MAX_CANVAS_WIDTH
        );
    }
    // Ensure coverage of content but do not exceed max
    width = width
        .max(max_right.saturating_add(1).min(MAX_CANVAS_WIDTH))
        .max(1);

    let mut height = max_bottom + ROW_SPACING;
    if height > MAX_CANVAS_HEIGHT {
        height = MAX_CANVAS_HEIGHT;
        eprintln!(
            "termiflow: warning: Graph too tall ({} rows), clipping to {}",
            max_bottom + ROW_SPACING,
            MAX_CANVAS_HEIGHT
        );
    }
    height = height
        .max(max_bottom.saturating_add(1).min(MAX_CANVAS_HEIGHT))
        .max(1);

    if graph.has_cycles() && width <= RIGHT_GUTTER {
        eprintln!("termiflow: warning: Back-edges skipped (gutter clipped)");
    }

    let mut canvas = Canvas::new(width, height);
    // Mix component styles with unicode as default
    let chars = config.composite_style.to_style_chars(BorderStyle::default());

    // Draw visible nodes
    let visible_nodes: Vec<&Node> = graph
        .nodes
        .iter()
        .filter(|n| canvas.is_visible(n))
        .collect();
    for node in &visible_nodes {
        let label = truncate_label(&node.label, config.max_label_width.min(node.width.saturating_sub(4)));
        draw_box(&mut canvas, node.x, node.y, node.width, &label, &chars);
    }

    // Draw edges (forward/back)
    for (i, edge) in graph.edges.iter().enumerate() {
        let Some(from) = graph.get_node(&edge.from) else { continue };
        let Some(to) = graph.get_node(&edge.to) else { continue };

        if !canvas.is_visible(from) || !canvas.is_visible(to) {
            continue;
        }

        if edge.is_back_edge {
            route_back_edge(from, to, &mut canvas, &chars);
        } else {
            route_edge(from, to, i, &mut canvas, &chars);
        }
    }

    Ok(canvas.to_string())
}

/// Draw a box at position (x, y)
fn draw_box(canvas: &mut Canvas, x: usize, y: usize, width: usize, label: &str, style: &StyleChars) {
    // Top border
    canvas.set(x, y, style.tl);
    for i in 1..width - 1 {
        canvas.set(x + i, y, style.h);
    }
    canvas.set(x + width - 1, y, style.tr);

    // Middle row with label
    canvas.set(x, y + 1, style.v);
    let padded_label = format!(" {:^width$} ", label, width = width - 4);
    for (i, c) in padded_label.chars().take(width - 2).enumerate() {
        canvas.set(x + 1 + i, y + 1, c);
    }
    canvas.set(x + width - 1, y + 1, style.v);

    // Bottom border
    canvas.set(x, y + 2, style.bl);
    for i in 1..width - 1 {
        canvas.set(x + i, y + 2, style.h);
    }
    canvas.set(x + width - 1, y + 2, style.br);
}

fn route_edge(
    from: &crate::graph::Node,
    to: &crate::graph::Node,
    edge_index: usize,
    canvas: &mut Canvas,
    style: &StyleChars,
) {
    if !canvas.is_visible(from) || !canvas.is_visible(to) {
        return;
    }

    let start_x = from.x + from.width / 2;
    let start_y = from.y + BOX_HEIGHT;

    let end_x = to.x + to.width / 2;
    let end_y = to.y.saturating_sub(1);

    let mid_y = calculate_mid_y(start_y, end_y, edge_index);

    for y in start_y..mid_y {
        canvas.set_edge_char(start_x, y, style.edge_v, style);
    }

    if start_x != end_x {
        let (left, right) = if start_x < end_x {
            (start_x, end_x)
        } else {
            (end_x, start_x)
        };
        for x in left..=right {
            canvas.set_edge_char(x, mid_y, style.edge_h, style);
        }
        canvas.set_edge_char(
            start_x,
            mid_y,
            corner_char(start_x, end_x, true, style),
            style,
        );
        canvas.set_edge_char(
            end_x,
            mid_y,
            corner_char(end_x, start_x, false, style),
            style,
        );
    }

    // Draw vertical line down to target (if needed)
    for y in (mid_y + 1)..end_y {
        canvas.set_edge_char(end_x, y, style.edge_v, style);
    }
    
    // Only place arrow if there's actually a vertical segment leading to it
    // i.e., mid_y < end_y, which means we drew at least one vertical line
    if mid_y < end_y {
        canvas.set(end_x, end_y, style.arrow_down);
    }
}

fn route_back_edge(
    from: &crate::graph::Node,
    to: &crate::graph::Node,
    canvas: &mut Canvas,
    style: &StyleChars,
) {
    if !canvas.is_visible(from) || !canvas.is_visible(to) {
        return;
    }

    if canvas.width <= RIGHT_GUTTER {
        return;
    }

    let gutter_x = canvas.width - 2;

    let from_y = from.y + BOX_HEIGHT / 2;
    let to_y = to.y + BOX_HEIGHT / 2;

    for x in (from.x + from.width)..gutter_x {
        canvas.set_edge_char(x, from_y, style.back_h, style);
    }

    let (top, bottom) = if from_y < to_y {
        (from_y, to_y)
    } else {
        (to_y, from_y)
    };
    for y in top..=bottom {
        canvas.set_edge_char(gutter_x, y, style.back_v, style);
    }

    for x in (to.x + to.width)..gutter_x {
        canvas.set_edge_char(x, to_y, style.back_h, style);
    }

    canvas.set(to.x + to.width, to_y, style.arrow_left);
}

fn calculate_mid_y(start_y: usize, end_y: usize, edge_index: usize) -> usize {
    // Ensure there's space for both vertical segments
    let base_mid = start_y + (end_y.saturating_sub(start_y)) / 2;
    let offset = (edge_index % 3) as isize - 1;
    let mid_y = (base_mid as isize + offset).max(start_y as isize + 1) as usize;
    // Ensure mid_y leaves room for a vertical segment to target
    mid_y.min(end_y.saturating_sub(1))
}

fn corner_char(from_x: usize, to_x: usize, is_source: bool, style: &StyleChars) -> char {
    // Source corner: vertical from above turns horizontal (UP + direction)
    // Target corner: horizontal turns vertical downward (direction + DOWN)
    // Note: target call uses (end_x, start_x), inverting the comparison
    match (from_x < to_x, is_source) {
        (true, true) => style.corner_ul,   // Source going right: └ (UP+RIGHT)
        (true, false) => style.corner_dl,  // Target, came from right: ┌ (DOWN+RIGHT)
        (false, true) => style.corner_ur,  // Source going left: ┘ (UP+LEFT)
        (false, false) => style.corner_dr, // Target, came from left: ┐ (DOWN+LEFT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_new() {
        let canvas = Canvas::new(10, 5);
        assert_eq!(canvas.width, 10);
        assert_eq!(canvas.height, 5);
    }

    #[test]
    fn test_canvas_set_get() {
        let mut canvas = Canvas::new(10, 5);
        canvas.set(2, 3, 'X');
        assert_eq!(canvas.get(2, 3), 'X');
        assert_eq!(canvas.get(0, 0), ' ');
    }
}
