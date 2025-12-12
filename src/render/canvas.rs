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
    matches!(
        c,
        'v' | '^' | '<' | '>'           // ASCII
        | '↓' | '↑' | '←' | '→'         // Unicode thin arrows
        | '▼' | '▲' | '◀' | '▶' // Unicode filled arrows
    )
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

    // Arrows are endpoints - never overwrite
    if is_arrow(existing) {
        return existing;
    }

    // Junctions are already merged - preserve them
    if is_junction(existing, s) {
        return existing;
    }

    // Corner + line = junction (existing corner)
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

    // Line + corner = junction (existing line, new corner)
    if is_horizontal(existing, s) && is_corner(new, s) {
        return if is_corner_up(new, s) {
            s.junction_up // ┴
        } else if is_corner_down(new, s) {
            s.junction_down // ┬
        } else {
            s.cross
        };
    }
    if is_vertical(existing, s) && is_corner(new, s) {
        return if is_corner_left(new, s) {
            s.junction_right // ├
        } else if is_corner_right(new, s) {
            s.junction_left // ┤
        } else {
            s.cross
        };
    }

    // Perpendicular lines crossing = cross
    if (is_horizontal(existing, s) && is_vertical(new, s))
        || (is_vertical(existing, s) && is_horizontal(new, s))
    {
        return s.cross;
    }

    // Box content (labels) - preserve
    if is_box_char(existing, s) {
        return existing;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{BaseStyle, CompositeStyle};

    fn unicode_chars() -> StyleChars {
        CompositeStyle::default().to_style_chars(BaseStyle::Unicode)
    }

    fn ascii_chars() -> StyleChars {
        CompositeStyle::default().to_style_chars(BaseStyle::Ascii)
    }

    // ==========================================================================
    // Character Classification Tests
    // ==========================================================================

    #[test]
    fn test_is_horizontal_unicode() {
        let s = unicode_chars();
        assert!(is_horizontal('─', &s));
        assert!(is_horizontal('═', &s));
        assert!(is_horizontal('━', &s));
        assert!(!is_horizontal('│', &s));
        assert!(!is_horizontal('a', &s));
    }

    #[test]
    fn test_is_vertical_unicode() {
        let s = unicode_chars();
        assert!(is_vertical('│', &s));
        assert!(is_vertical('║', &s));
        assert!(is_vertical('┃', &s));
        assert!(!is_vertical('─', &s));
        assert!(!is_vertical('a', &s));
    }

    #[test]
    fn test_is_arrow() {
        assert!(is_arrow('v'));
        assert!(is_arrow('^'));
        assert!(is_arrow('<'));
        assert!(is_arrow('>'));
        assert!(is_arrow('▼'));
        assert!(is_arrow('↓'));
        assert!(!is_arrow('─'));
        assert!(!is_arrow('a'));
    }

    #[test]
    fn test_is_corner() {
        let s = unicode_chars();
        assert!(is_corner('┌', &s)); // corner_dl
        assert!(is_corner('┐', &s)); // corner_dr
        assert!(is_corner('└', &s)); // corner_ul
        assert!(is_corner('┘', &s)); // corner_ur
        assert!(!is_corner('─', &s));
        assert!(!is_corner('│', &s));
    }

    #[test]
    fn test_is_junction() {
        let s = unicode_chars();
        assert!(is_junction('┬', &s)); // junction_down
        assert!(is_junction('┴', &s)); // junction_up
        assert!(is_junction('┼', &s)); // cross
        assert!(!is_junction('─', &s));
        assert!(!is_junction('└', &s));
    }

    // ==========================================================================
    // Overlap Resolution Tests
    // ==========================================================================

    #[test]
    fn test_overlap_empty_space_takes_new() {
        let s = unicode_chars();
        assert_eq!(resolve_overlap(' ', '│', &s), '│');
        assert_eq!(resolve_overlap(' ', '─', &s), '─');
        assert_eq!(resolve_overlap('\0', '┌', &s), '┌');
    }

    #[test]
    fn test_overlap_arrows_never_overwritten() {
        let s = unicode_chars();
        assert_eq!(resolve_overlap('↓', '│', &s), '↓');
        assert_eq!(resolve_overlap('▼', '─', &s), '▼');
        assert_eq!(resolve_overlap('v', '|', &s), 'v');
    }

    #[test]
    fn test_overlap_junctions_preserved() {
        let s = unicode_chars();
        assert_eq!(resolve_overlap('┬', '│', &s), '┬');
        assert_eq!(resolve_overlap('┴', '─', &s), '┴');
        assert_eq!(resolve_overlap('┼', '│', &s), '┼');
    }

    #[test]
    fn test_overlap_corner_plus_vertical_creates_junction() {
        let s = unicode_chars();
        // Left-opening corner + vertical = right-pointing junction (├)
        assert_eq!(resolve_overlap('└', '│', &s), '├');
        assert_eq!(resolve_overlap('┌', '│', &s), '├');
        // Right-opening corner + vertical = left-pointing junction (┤)
        assert_eq!(resolve_overlap('┘', '│', &s), '┤');
        assert_eq!(resolve_overlap('┐', '│', &s), '┤');
    }

    #[test]
    fn test_overlap_corner_plus_horizontal_creates_junction() {
        let s = unicode_chars();
        // Up-opening corner + horizontal = up-pointing junction (┴)
        assert_eq!(resolve_overlap('└', '─', &s), '┴');
        assert_eq!(resolve_overlap('┘', '─', &s), '┴');
        // Down-opening corner + horizontal = down-pointing junction (┬)
        assert_eq!(resolve_overlap('┌', '─', &s), '┬');
        assert_eq!(resolve_overlap('┐', '─', &s), '┬');
    }

    #[test]
    fn test_overlap_perpendicular_lines_create_cross() {
        let s = unicode_chars();
        assert_eq!(resolve_overlap('│', '─', &s), '┼');
        assert_eq!(resolve_overlap('─', '│', &s), '┼');
    }

    #[test]
    fn test_overlap_box_content_preserved() {
        let s = unicode_chars();
        assert_eq!(resolve_overlap('A', '│', &s), 'A');
        assert_eq!(resolve_overlap('1', '─', &s), '1');
        assert_eq!(resolve_overlap('_', '┌', &s), '_');
    }

    // ==========================================================================
    // Canvas Operations Tests
    // ==========================================================================

    #[test]
    fn test_canvas_new_filled_with_spaces() {
        let canvas = Canvas::new(10, 5);
        assert_eq!(canvas.width, 10);
        assert_eq!(canvas.height, 5);
        assert_eq!(canvas.get(0, 0), ' ');
        assert_eq!(canvas.get(9, 4), ' ');
    }

    #[test]
    fn test_canvas_set_get() {
        let mut canvas = Canvas::new(10, 5);
        canvas.set(3, 2, 'X');
        assert_eq!(canvas.get(3, 2), 'X');
        assert_eq!(canvas.get(0, 0), ' ');
    }

    #[test]
    fn test_canvas_out_of_bounds_returns_space() {
        let canvas = Canvas::new(10, 5);
        assert_eq!(canvas.get(100, 100), ' ');
    }

    #[test]
    fn test_canvas_set_out_of_bounds_ignored() {
        let mut canvas = Canvas::new(10, 5);
        canvas.set(100, 100, 'X'); // Should not panic
        assert_eq!(canvas.get(100, 100), ' ');
    }

    #[test]
    fn test_canvas_set_edge_char_with_overlap_resolution() {
        let mut canvas = Canvas::new(10, 5);
        let s = unicode_chars();

        // First edge: vertical line
        canvas.set_edge_char(5, 2, '│', &s);
        assert_eq!(canvas.get(5, 2), '│');

        // Second edge: horizontal line crossing -> creates cross
        canvas.set_edge_char(5, 2, '─', &s);
        assert_eq!(canvas.get(5, 2), '┼');
    }

    #[test]
    fn test_canvas_is_visible() {
        let canvas = Canvas::new(80, 40);

        let visible_node = Node {
            id: "A".into(),
            label: "Test".into(),
            shape: crate::graph::NodeShape::Rectangle,
            click_target: None,
            x: 0,
            y: 0,
            width: 10,
            rank: 0,
        };
        assert!(canvas.is_visible(&visible_node));

        let clipped_node = Node {
            id: "B".into(),
            label: "Clipped".into(),
            shape: crate::graph::NodeShape::Rectangle,
            click_target: None,
            x: 75,
            y: 0,
            width: 10, // x + width = 85 > 80
            rank: 0,
        };
        assert!(!canvas.is_visible(&clipped_node));
    }

    #[test]
    fn test_canvas_display_trims_trailing_spaces() {
        let mut canvas = Canvas::new(10, 3);
        canvas.set(0, 0, 'A');
        canvas.set(2, 1, 'B');

        let output = format!("{}", canvas);
        let lines: Vec<&str> = output.lines().collect();

        assert_eq!(lines[0], "A"); // Trimmed from "A         "
        assert_eq!(lines[1], "  B"); // Trimmed from "  B       "
    }

    // ==========================================================================
    // ASCII Style Tests
    // ==========================================================================

    #[test]
    fn test_ascii_overlap_resolution() {
        let s = ascii_chars();

        // ASCII uses different characters
        assert_eq!(resolve_overlap(' ', '|', &s), '|');
        assert_eq!(resolve_overlap(' ', '-', &s), '-');

        // Perpendicular creates cross
        assert_eq!(resolve_overlap('|', '-', &s), '+');
        assert_eq!(resolve_overlap('-', '|', &s), '+');
    }
}
