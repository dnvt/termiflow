//! Canvas - 2D character grid for diagram rendering.
//!
//! Provides the core `Canvas` type and character classification utilities
//! for detecting line types, junctions, and resolving overlapping characters.

use crate::graph::Node;
use crate::style::StyleChars;

use super::semantic::{CellMeta, CellOwnerKind, CellRole};

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
///
/// Note: Parallel edges (both horizontal or both vertical) do NOT create
/// crossing indicators. This is intentional - they are visually distinguishable
/// by separation, and crosses would create ambiguity about edge connectivity.
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

    // Identical characters - no change needed
    if existing == new {
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
        // Use arm-counting to determine the correct junction type based on
        // which directions both corners open toward.
        //
        // Note: is_corner_left/right indicate which SIDE the corner is on, not
        // which direction the arm points. Corners on the left side have their
        // horizontal arm pointing RIGHT, and vice versa.
        if is_corner(new, s) {
            // Count all directional arms from both corners
            let has_up_arm = is_corner_up(existing, s) || is_corner_up(new, s);
            let has_down_arm = is_corner_down(existing, s) || is_corner_down(new, s);
            // Corners on right side (is_corner_right) have arm going LEFT
            let has_left_arm = is_corner_right(existing, s) || is_corner_right(new, s);
            // Corners on left side (is_corner_left) have arm going RIGHT
            let has_right_arm = is_corner_left(existing, s) || is_corner_left(new, s);

            let arm_count = [has_up_arm, has_down_arm, has_left_arm, has_right_arm]
                .iter()
                .filter(|&&b| b)
                .count();

            if arm_count >= 4 {
                return s.cross; // ┼ - all four directions
            }
            if arm_count == 3 {
                // Three-way junction - determine which direction is missing
                if !has_up_arm {
                    return s.junction_down; // ┬ - no up arm
                }
                if !has_down_arm {
                    return s.junction_up; // ┴ - no down arm
                }
                if !has_left_arm {
                    return s.junction_right; // ├ - no left arm
                }
                if !has_right_arm {
                    return s.junction_left; // ┤ - no right arm
                }
            }
            // Two arms - this is actually a corner situation (shouldn't happen
            // for two overlapping corners, but fall through to cross as safety)
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
    meta_grid: Vec<Vec<CellMeta>>,
}

impl Canvas {
    /// Create a new canvas filled with spaces.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            grid: vec![vec![' '; width]; height],
            meta_grid: vec![vec![CellMeta::default(); width]; height],
        }
    }

    /// Set a character at position (x, y).
    pub fn set(&mut self, x: usize, y: usize, c: char) {
        self.set_inferred(x, y, c);
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
        self.set_inferred(x, y, final_char);
    }

    /// Set a character and infer a generic semantic classification from the glyph.
    pub fn set_inferred(&mut self, x: usize, y: usize, c: char) {
        if x < self.width && y < self.height {
            self.grid[y][x] = c;
            self.meta_grid[y][x] = infer_meta(c);
        }
    }

    /// Set a character with explicit semantic ownership.
    pub fn set_owned(
        &mut self,
        x: usize,
        y: usize,
        c: char,
        owner_kind: CellOwnerKind,
        owner_id: &str,
        z_index: u8,
    ) {
        if x < self.width && y < self.height {
            self.grid[y][x] = c;
            self.meta_grid[y][x] = infer_owned_meta(c, owner_kind, owner_id, z_index);
        }
    }

    /// Set an edge character with overlap resolution and explicit ownership.
    #[allow(clippy::too_many_arguments)]
    pub fn set_edge_char_owned(
        &mut self,
        x: usize,
        y: usize,
        new_char: char,
        s: &StyleChars,
        owner_kind: CellOwnerKind,
        owner_id: &str,
        z_index: u8,
    ) {
        let existing = self.get(x, y);
        let final_char = resolve_overlap(existing, new_char, s);
        if x < self.width && y < self.height {
            self.grid[y][x] = final_char;
            let existing_meta = &self.meta_grid[y][x];
            let final_role = infer_role(final_char);
            let should_preserve_existing = final_char == existing
                && !matches!(
                    final_role,
                    CellRole::Horizontal
                        | CellRole::Vertical
                        | CellRole::Corner
                        | CellRole::Junction
                        | CellRole::ArrowTip
                );

            if should_preserve_existing {
                return;
            }

            let meta = infer_owned_meta(final_char, owner_kind, owner_id, z_index);
            if meta.z_index >= existing_meta.z_index {
                self.meta_grid[y][x] = meta;
            }
        }
    }

    /// Rebuild metadata for every cell from the current visible glyph grid.
    pub fn refresh_inferred_meta(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.meta_grid[y][x] = infer_meta(self.grid[y][x]);
            }
        }
    }

    /// Update semantic metadata without changing the visible character.
    pub fn set_meta_only(
        &mut self,
        x: usize,
        y: usize,
        owner_kind: CellOwnerKind,
        owner_id: Option<&str>,
        role: CellRole,
        z_index: u8,
    ) {
        if x < self.width && y < self.height {
            let ch = self.grid[y][x];
            if z_index >= self.meta_grid[y][x].z_index {
                self.meta_grid[y][x] = CellMeta {
                    ch,
                    owner_kind,
                    owner_id: owner_id.map(ToOwned::to_owned),
                    role,
                    z_index,
                };
            }
        }
    }

    /// Get semantic metadata at position (x, y).
    pub fn get_meta(&self, x: usize, y: usize) -> Option<&CellMeta> {
        if x < self.width && y < self.height {
            Some(&self.meta_grid[y][x])
        } else {
            None
        }
    }

    /// Capture explicit edge-related metadata that should survive a metadata refresh.
    pub fn explicit_edge_meta(&self) -> Vec<(usize, usize, CellMeta)> {
        let mut preserved = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                let meta = &self.meta_grid[y][x];
                if meta.owner_id.is_some()
                    && meta.z_index > 0
                    && matches!(
                        meta.role,
                        CellRole::Horizontal
                            | CellRole::Vertical
                            | CellRole::Corner
                            | CellRole::Junction
                            | CellRole::ArrowTip
                    )
                {
                    preserved.push((x, y, meta.clone()));
                }
            }
        }
        preserved
    }

    /// Check if a node is within visible canvas bounds.
    pub fn is_visible(&self, node: &Node) -> bool {
        node.x + node.width <= self.width && node.y + node.height <= self.height
    }

    /// Convert the canvas to a string, cropping empty margins and optionally padding.
    ///
    /// Cropping trims any fully-empty rows/columns (spaces only) around the content.
    /// Padding adds blank rows and left/right spaces around every line.
    pub fn to_string_cropped(&self, pad: usize) -> String {
        if self.width == 0 || self.height == 0 {
            return String::new();
        }

        let mut found = false;
        let mut min_x = self.width;
        let mut max_x = 0usize;
        let mut min_y = self.height;
        let mut max_y = 0usize;

        for (y, row) in self.grid.iter().enumerate() {
            for (x, c) in row.iter().enumerate() {
                if *c != ' ' {
                    found = true;
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                }
            }
        }

        if !found {
            return String::new();
        }

        let mut lines: Vec<String> = Vec::with_capacity(max_y.saturating_sub(min_y) + 1);
        for y in min_y..=max_y {
            let slice = &self.grid[y][min_x..=max_x];
            let line = slice.iter().collect::<String>().trim_end().to_string();
            lines.push(line);
        }

        pad_lines(&lines, pad)
    }
}

fn infer_meta(c: char) -> CellMeta {
    let role = infer_role(c);

    let owner_kind = match role {
        CellRole::Empty => CellOwnerKind::Empty,
        CellRole::ArrowTip => CellOwnerKind::ArrowHead,
        CellRole::Junction => CellOwnerKind::Junction,
        CellRole::Horizontal | CellRole::Vertical | CellRole::Corner => CellOwnerKind::EdgeSegment,
        CellRole::Text
        | CellRole::Unknown
        | CellRole::Fill
        | CellRole::Border
        | CellRole::Portal => CellOwnerKind::Unknown,
    };

    CellMeta {
        ch: c,
        owner_kind,
        owner_id: None,
        role,
        z_index: 0,
    }
}

fn infer_role(c: char) -> CellRole {
    if c == ' ' {
        CellRole::Empty
    } else if is_arrow(c) {
        CellRole::ArrowTip
    } else if matches!(
        c,
        '┌' | '┐' | '└' | '┘' | '╔' | '╗' | '╚' | '╝' | '╭' | '╮' | '╰' | '╯'
    ) {
        CellRole::Corner
    } else if matches!(c, '-' | '─' | '═' | '━' | '█') {
        CellRole::Horizontal
    } else if matches!(c, '|' | ':' | '│' | '║' | '┃') {
        CellRole::Vertical
    } else if matches!(
        c,
        '+' | '┼'
            | '╬'
            | '╋'
            | '├'
            | '┤'
            | '┬'
            | '┴'
            | '╠'
            | '╣'
            | '╦'
            | '╩'
            | '┣'
            | '┫'
            | '┳'
            | '┻'
    ) {
        CellRole::Junction
    } else {
        CellRole::Text
    }
}

fn infer_owned_meta(c: char, owner_kind: CellOwnerKind, owner_id: &str, z_index: u8) -> CellMeta {
    let role = infer_role(c);
    let final_owner_kind = match (owner_kind, role) {
        (CellOwnerKind::CycleEdge, CellRole::ArrowTip) => CellOwnerKind::CycleEdge,
        (_, CellRole::ArrowTip) => CellOwnerKind::ArrowHead,
        _ => owner_kind,
    };

    CellMeta {
        ch: c,
        owner_kind: final_owner_kind,
        owner_id: Some(owner_id.to_string()),
        role,
        z_index,
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

fn pad_lines(lines: &[String], pad: usize) -> String {
    if pad == 0 {
        return lines.join("\n");
    }

    let prefix = " ".repeat(pad);
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + pad * 2);

    for _ in 0..pad {
        out.push(String::new());
    }
    for line in lines {
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

    #[test]
    fn test_overlap_two_corners_creates_junction() {
        let s = unicode_chars();
        // Two up-opening corners (└ and ┘) combine to junction_up (┴)
        // └ = up+right, ┘ = up+left → combined: up+right+left = ┴
        assert_eq!(resolve_overlap('└', '┘', &s), '┴');
        assert_eq!(resolve_overlap('┘', '└', &s), '┴');

        // Two down-opening corners (┌ and ┐) combine to junction_down (┬)
        // ┌ = down+right, ┐ = down+left → combined: down+right+left = ┬
        assert_eq!(resolve_overlap('┌', '┐', &s), '┬');
        assert_eq!(resolve_overlap('┐', '┌', &s), '┬');

        // Opposite corners (└ and ┐) combine to cross or specific junction
        // └ = up+right, ┐ = down+left → combined: all 4 = cross
        assert_eq!(resolve_overlap('└', '┐', &s), '┼');
        assert_eq!(resolve_overlap('┐', '└', &s), '┼');

        // Same-side corners combine to appropriate junction
        // └ = up+right, ┌ = down+right → combined: up+down+right = ├
        assert_eq!(resolve_overlap('└', '┌', &s), '├');
        assert_eq!(resolve_overlap('┌', '└', &s), '├');

        // ┘ = up+left, ┐ = down+left → combined: up+down+left = ┤
        assert_eq!(resolve_overlap('┘', '┐', &s), '┤');
        assert_eq!(resolve_overlap('┐', '┘', &s), '┤');
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
            label_lines: Vec::new(),
            shape: crate::graph::NodeShape::Rectangle,
            click_target: None,
            x: 0,
            y: 0,
            width: 10,
            height: crate::style::BOX_HEIGHT,
            rank: 0,
        };
        assert!(canvas.is_visible(&visible_node));

        let clipped_node = Node {
            id: "B".into(),
            label: "Clipped".into(),
            label_lines: Vec::new(),
            shape: crate::graph::NodeShape::Rectangle,
            click_target: None,
            x: 75,
            y: 0,
            width: 10, // x + width = 85 > 80
            height: crate::style::BOX_HEIGHT,
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
