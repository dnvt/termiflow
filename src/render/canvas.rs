//! Canvas - 2D character grid for diagram rendering.
//!
//! Provides the core `Canvas` type and character classification utilities
//! for detecting line types, junctions, and resolving overlapping characters.

use crate::graph::Node;
use crate::style::{StyleChars, BOX_HEIGHT};

// ============================================================================
// Character Classification
// ============================================================================

/// Horizontal line characters across all supported styles
pub fn is_horizontal(c: char, _style: &StyleChars) -> bool {
    matches!(c, '-' | '─' | '═' | '━' | '█')
}

/// Vertical line characters across all supported styles
pub fn is_vertical(c: char, _style: &StyleChars) -> bool {
    matches!(c, '|' | ':' | '│' | '║' | '┃' | '█')
}

/// Arrow characters (endpoints - never overwritten)
pub fn is_arrow(c: char) -> bool {
    matches!(c, 'v' | '^' | '<' | '>' | '▼' | '▲' | '◀' | '▶')
}

/// Corner characters for the given style
pub fn is_corner(c: char, s: &StyleChars) -> bool {
    c == s.corner_dr || c == s.corner_dl || c == s.corner_ur || c == s.corner_ul
}

/// Junction characters (T-junctions and crosses - preserved once created)
pub fn is_junction(c: char, s: &StyleChars) -> bool {
    c == s.junction_down
        || c == s.junction_up
        || c == s.junction_left
        || c == s.junction_right
        || c == s.cross
}

/// Box label content (alphanumeric + punctuation)
pub fn is_box_char(c: char, _style: &StyleChars) -> bool {
    c.is_alphanumeric()
        || matches!(
            c,
            '(' | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '_'
                | '.'
                | ','
                | ':'
                | ';'
                | '!'
                | '?'
                | '\''
                | '"'
                | '`'
                | '@'
                | '#'
                | '$'
                | '%'
                | '&'
                | '*'
                | '='
                | '+'
                | '/'
                | '\\'
                | '-'
        )
}

// Corner direction helpers (which way does the corner "open"?)
pub fn is_corner_up(c: char, s: &StyleChars) -> bool {
    c == s.corner_ul || c == s.corner_ur
}
pub fn is_corner_down(c: char, s: &StyleChars) -> bool {
    c == s.corner_dl || c == s.corner_dr
}
pub fn is_corner_left(c: char, s: &StyleChars) -> bool {
    c == s.corner_dl || c == s.corner_ul
}
pub fn is_corner_right(c: char, s: &StyleChars) -> bool {
    c == s.corner_dr || c == s.corner_ur
}

// ============================================================================
// Overlap Resolution
// ============================================================================

/// Resolve what character to draw when two characters overlap.
/// Creates junctions/crosses where appropriate, preserves sacred characters.
pub fn resolve_overlap(existing: char, new: char, s: &StyleChars) -> char {
    // Empty space - just use new character
    if existing == ' ' || existing == '\0' {
        return new;
    }

    // Sacred characters that must never be overwritten
    if is_arrow(existing) || is_box_char(existing, s) || is_junction(existing, s) {
        return existing;
    }

    // Corner + line = junction
    if is_corner(existing, s) {
        if is_vertical(new, s) {
            return if is_corner_left(existing, s) {
                s.junction_right // ├
            } else if is_corner_right(existing, s) {
                s.junction_left // ┤
            } else {
                s.cross
            };
        }
        if is_horizontal(new, s) {
            return if is_corner_up(existing, s) {
                s.junction_up // ┴
            } else if is_corner_down(existing, s) {
                s.junction_down // ┬
            } else {
                s.cross
            };
        }
        // Two corners = junction (edges converging)
        if is_corner(new, s) {
            let both_down = is_corner_down(existing, s) && is_corner_down(new, s);
            let left_right = is_corner_left(existing, s) && is_corner_right(new, s);
            let right_left = is_corner_right(existing, s) && is_corner_left(new, s);
            if both_down || left_right || right_left {
                return s.junction_down; // ┬
            }
            if is_corner_up(existing, s) && is_corner_up(new, s) {
                return s.junction_up; // ┴
            }
            return s.cross;
        }
    }

    // Perpendicular lines crossing = cross
    if (is_horizontal(existing, s) && is_vertical(new, s))
        || (is_vertical(existing, s) && is_horizontal(new, s))
    {
        return s.cross;
    }

    // Default: new character wins
    new
}

// ============================================================================
// Canvas Structure
// ============================================================================

/// 2D character canvas for rendering diagrams.
///
/// The canvas is a grid of characters that can be drawn to and then
/// converted to a string for display.
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    grid: Vec<Vec<char>>,
}

impl Canvas {
    /// Create a new canvas filled with spaces.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            grid: vec![vec![' '; width]; height],
        }
    }

    /// Set a character at position (x, y).
    pub fn set(&mut self, x: usize, y: usize, c: char) {
        if x < self.width && y < self.height {
            self.grid[y][x] = c;
        }
    }

    /// Get character at position (x, y).
    pub fn get(&self, x: usize, y: usize) -> char {
        if x < self.width && y < self.height {
            self.grid[y][x]
        } else {
            ' '
        }
    }

    /// Set edge character with smart crossing/junction detection.
    ///
    /// Priority (highest first):
    /// 1. Preserve arrows and box content
    /// 2. Preserve existing junctions
    /// 3. Create junctions when lines/corners overlap
    /// 4. New character wins for empty space
    pub fn set_edge_char(&mut self, x: usize, y: usize, new_char: char, s: &StyleChars) {
        let existing = self.get(x, y);
        let final_char = resolve_overlap(existing, new_char, s);
        self.set(x, y, final_char);
    }

    /// Check if a node is within visible canvas bounds.
    pub fn is_visible(&self, node: &Node) -> bool {
        node.x + node.width <= self.width && node.y + BOX_HEIGHT <= self.height
    }
}

impl std::fmt::Display for Canvas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let output = self
            .grid
            .iter()
            .map(|row| row.iter().collect::<String>().trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        write!(f, "{}", output)
    }
}

