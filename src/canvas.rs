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
use crate::graph::Graph;
use crate::style::{BorderStyle, StyleChars, BOX_HEIGHT, MAX_CANVAS_WIDTH, MAX_CANVAS_HEIGHT};

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

    /// Set edge character with crossing detection
    pub fn set_edge_char(&mut self, x: usize, y: usize, new_char: char, style: &StyleChars) {
        let existing = self.get(x, y);

        let final_char = match existing {
            ' ' | '\0' => new_char,
            c if is_horizontal(c, style) && is_vertical(new_char, style) => style.cross,
            c if is_vertical(c, style) && is_horizontal(new_char, style) => style.cross,
            c if is_arrow(c) => c, // Preserve arrows (Decision 41)
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
}

fn is_horizontal(c: char, _style: &StyleChars) -> bool {
    matches!(c, '-' | '─' | '═' | '━' | '┄' | '┅')
}

fn is_vertical(c: char, _style: &StyleChars) -> bool {
    matches!(c, '|' | '│' | '║' | '┃' | ':' | '┆' | '┊' | '╏')
}

fn is_arrow(c: char) -> bool {
    matches!(c, 'v' | '^' | '<' | '>' | '▼' | '▲' | '◀' | '▶')
}

/// Render a graph to a string
pub fn render(graph: &Graph, style: &BorderStyle, max_label_width: usize) -> Result<String> {
    let _ = max_label_width; // TODO: use for label truncation

    // TODO: Implement full rendering (Day 3)

    if graph.nodes.is_empty() {
        return Ok(String::new());
    }

    // Calculate canvas size
    let width = MAX_CANVAS_WIDTH.min(80);
    let height = MAX_CANVAS_HEIGHT.min(graph.nodes.len() * (BOX_HEIGHT + 2));

    let mut canvas = Canvas::new(width, height);
    let chars = style.chars();

    // Draw placeholder boxes
    for node in &graph.nodes {
        draw_box(&mut canvas, node.x, node.y, node.width, &node.label, chars);
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
