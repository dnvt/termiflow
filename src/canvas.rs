//! Canvas rendering - 2D character grid for diagram output.
//!
//! This module handles the final rendering phase:
//! - Box drawing for nodes with labels
//! - Edge routing (straight, L-shaped, back-edges)
//! - Junction/crossing detection for overlapping paths
//!
//! Rendering order: edges first, then boxes (so boxes overwrite edge lines).

use anyhow::Result;
use crate::graph::{Graph, Node};
use crate::style::{
    truncate_label, BorderStyle, StyleChars, BOX_HEIGHT, COL_SPACING, MAX_CANVAS_HEIGHT,
    MAX_CANVAS_WIDTH, RIGHT_GUTTER, ROW_SPACING,
};

// ============================================================================
// Character Classification
// ============================================================================

/// Horizontal line characters across all supported styles
fn is_horizontal(c: char, _style: &StyleChars) -> bool {
    matches!(c, '-' | '─' | '═' | '━' | '█')
}

/// Vertical line characters across all supported styles
fn is_vertical(c: char, _style: &StyleChars) -> bool {
    matches!(c, '|' | ':' | '│' | '║' | '┃' | '█')
}

/// Arrow characters (endpoints - never overwritten)
fn is_arrow(c: char) -> bool {
    matches!(c, 'v' | '^' | '<' | '>' | '▼' | '▲' | '◀' | '▶')
}

/// Corner characters for the given style
fn is_corner(c: char, s: &StyleChars) -> bool {
    c == s.corner_dr || c == s.corner_dl || c == s.corner_ur || c == s.corner_ul
}

/// Junction characters (T-junctions and crosses - preserved once created)
fn is_junction(c: char, s: &StyleChars) -> bool {
    c == s.junction_down || c == s.junction_up ||
    c == s.junction_left || c == s.junction_right || c == s.cross
}

/// Box label content (alphanumeric + punctuation)
fn is_box_char(c: char, _style: &StyleChars) -> bool {
    c.is_alphanumeric() || matches!(c,
        '(' | ')' | '[' | ']' | '{' | '}' | '_' | '.' | ',' | ':' | ';' |
        '!' | '?' | '\'' | '"' | '`' | '@' | '#' | '$' | '%' | '&' | '*' |
        '=' | '+' | '/' | '\\' | '-'
    )
}

// Corner direction helpers (which way does the corner "open"?)
fn is_corner_up(c: char, s: &StyleChars) -> bool { c == s.corner_ul || c == s.corner_ur }
fn is_corner_down(c: char, s: &StyleChars) -> bool { c == s.corner_dl || c == s.corner_dr }
fn is_corner_left(c: char, s: &StyleChars) -> bool { c == s.corner_dl || c == s.corner_ul }
fn is_corner_right(c: char, s: &StyleChars) -> bool { c == s.corner_dr || c == s.corner_ur }

/// Resolve what character to draw when two characters overlap.
/// Creates junctions/crosses where appropriate, preserves sacred characters.
fn resolve_overlap(existing: char, new: char, s: &StyleChars) -> char {
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
            return if is_corner_left(existing, s) { s.junction_right }  // ├
                   else if is_corner_right(existing, s) { s.junction_left }  // ┤
                   else { s.cross };
        }
        if is_horizontal(new, s) {
            return if is_corner_up(existing, s) { s.junction_up }  // ┴
                   else if is_corner_down(existing, s) { s.junction_down }  // ┬
                   else { s.cross };
        }
        // Two corners = junction (edges converging)
        if is_corner(new, s) {
            let both_down = is_corner_down(existing, s) && is_corner_down(new, s);
            let left_right = is_corner_left(existing, s) && is_corner_right(new, s);
            let right_left = is_corner_right(existing, s) && is_corner_left(new, s);
            if both_down || left_right || right_left {
                return s.junction_down;  // ┬
            }
            if is_corner_up(existing, s) && is_corner_up(new, s) {
                return s.junction_up;  // ┴
            }
            return s.cross;
        }
    }

    // Perpendicular lines crossing = cross
    if (is_horizontal(existing, s) && is_vertical(new, s)) ||
       (is_vertical(existing, s) && is_horizontal(new, s)) {
        return s.cross;
    }

    // Default: new character wins
    new
}

// ============================================================================
// Canvas Structure
// ============================================================================

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


    /// Check if a node is within visible canvas bounds
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

// ============================================================================
// Rendering
// ============================================================================

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

    // Get visible nodes
    let visible_nodes: Vec<&Node> = graph
        .nodes
        .iter()
        .filter(|n| canvas.is_visible(n))
        .collect();

    // Sort edges: straight edges first, then L-shaped edges
    // This allows L-shaped edges to merge with existing straight paths
    let mut edge_indices: Vec<(usize, bool)> = graph
        .edges
        .iter()
        .enumerate()
        .filter_map(|(i, edge)| {
            if edge.is_back_edge {
                return Some((i, false)); // Back edges last
            }
            let from = graph.get_node(&edge.from)?;
            let to = graph.get_node(&edge.to)?;
            if !canvas.is_visible(from) || !canvas.is_visible(to) {
                return None;
            }
            // Check if edge needs L-shaped routing (has intervening nodes)
            let start_x = center_x(from);
            let end_x = center_x(to);
            let start_y = from.y + BOX_HEIGHT;
            let end_y = to.y;
            let x_diff = (start_x as isize - end_x as isize).unsigned_abs();
            let needs_lshape = x_diff > 1 || visible_nodes.iter().any(|n| {
                if n.id == from.id || n.id == to.id {
                    return false;
                }
                let node_bottom = n.y + BOX_HEIGHT;
                if node_bottom <= start_y || n.y >= end_y {
                    return false;
                }
                end_x >= n.x && end_x < n.x + n.width
            });
            Some((i, needs_lshape))
        })
        .collect();

    // Sort: straight edges first (false < true)
    edge_indices.sort_by_key(|(_, needs_lshape)| *needs_lshape);

    // Draw edges FIRST (so boxes can overwrite them)
    for (i, _) in edge_indices {
        let edge = &graph.edges[i];
        let Some(from) = graph.get_node(&edge.from) else { continue };
        let Some(to) = graph.get_node(&edge.to) else { continue };

        if edge.is_back_edge {
            route_back_edge(from, to, &mut canvas, &chars);
        } else {
            route_edge(from, to, i, &mut canvas, &chars, &visible_nodes);
        }
    }

    // Draw boxes AFTER edges (boxes overwrite any edges passing through them)
    for node in &visible_nodes {
        let label = truncate_label(&node.label, config.max_label_width.min(node.width.saturating_sub(4)));
        draw_box(&mut canvas, node.x, node.y, node.width, &label, &chars);
    }

    Ok(canvas.to_string())
}

// ============================================================================
// Drawing Primitives
// ============================================================================

/// Draw a box at position (x, y) with the given label
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

// ============================================================================
// Edge Routing
// ============================================================================

/// Route an edge from source to target node.
///
/// Handles three cases:
/// 1. Straight vertical line (nodes nearly aligned)
/// 2. Reuse existing path (when blocked by intervening node)
/// 3. L-shaped routing (horizontal then vertical)
fn route_edge(
    from: &crate::graph::Node,
    to: &crate::graph::Node,
    edge_index: usize,
    canvas: &mut Canvas,
    style: &StyleChars,
    all_nodes: &[&Node],
) {
    if !canvas.is_visible(from) || !canvas.is_visible(to) {
        return;
    }

    let start_x = center_x(from);
    let start_y = from.y + BOX_HEIGHT;
    let end_x = center_x(to);
    let end_y = to.y.saturating_sub(1);

    // Check if vertical line at x would pass through an intervening node
    let is_blocked = |x: usize| {
        all_nodes.iter().any(|n| {
            n.id != from.id
                && n.id != to.id
                && n.y + BOX_HEIGHT > start_y
                && n.y < end_y
                && x >= n.x
                && x < n.x + n.width
        })
    };

    let x_diff = (start_x as isize - end_x as isize).unsigned_abs();

    // Case 1: Nearly aligned with clear path - draw straight vertical
    if x_diff <= 1 && !is_blocked(end_x) {
        for y in start_y..end_y {
            canvas.set_edge_char(end_x, y, style.edge_v, style);
        }
        canvas.set(end_x, end_y, style.arrow_down);
        return;
    }

    // Case 2: Blocked - check if existing path reaches target (edge sorting ensures this)
    if is_blocked(end_x) {
        let existing = canvas.get(end_x, end_y.saturating_sub(1));
        if is_vertical(existing, style) || is_arrow(existing) {
            // Path exists - just add arrow
            canvas.set(end_x, end_y, style.arrow_down);
            return;
        }
    }

    // Case 3: L-shaped routing needed
    let mid_y = calculate_mid_y(start_y, end_y, edge_index);

    // Merge with adjacent vertical line if exists
    let use_x = [start_x.saturating_sub(1), start_x + 1, start_x]
        .into_iter()
        .find(|&x| {
            let c = canvas.get(x, start_y);
            x != start_x && (is_vertical(c, style) || is_arrow(c))
        })
        .unwrap_or(start_x);

    // Draw vertical line down from source to mid_y
    for y in start_y..mid_y {
        canvas.set_edge_char(use_x, y, style.edge_v, style);
    }

    // Draw horizontal line at mid_y with corners
    let (left, right) = if use_x < end_x {
        (use_x, end_x)
    } else {
        (end_x, use_x)
    };
    for x in left..=right {
        canvas.set_edge_char(x, mid_y, style.edge_h, style);
    }

    // Place corners
    canvas.set_edge_char(use_x, mid_y, corner_char(use_x, end_x, true, style), style);
    canvas.set_edge_char(end_x, mid_y, corner_char(end_x, use_x, false, style), style);

    // Draw vertical line down to target
    for y in (mid_y + 1)..end_y {
        canvas.set_edge_char(end_x, y, style.edge_v, style);
    }


    // Place arrow at target
    if mid_y < end_y {
        canvas.set(end_x, end_y, style.arrow_down);
    }
}

/// Calculate the visual center x-coordinate of a node.
/// Uses (width-1)/2 for proper centering of odd-width boxes.
#[inline]
fn center_x(node: &Node) -> usize {
    node.x + (node.width.saturating_sub(1)) / 2
}

/// Route a back-edge (cycle) through the right gutter.
/// Back-edges go: right from source → down/up in gutter → left to target
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

/// Calculate the y-coordinate for the horizontal segment of an L-shaped edge.
/// Spreads multiple edges slightly to avoid perfect overlap.
fn calculate_mid_y(start_y: usize, end_y: usize, edge_index: usize) -> usize {
    let base_mid = start_y + (end_y.saturating_sub(start_y)) / 2;
    let offset = (edge_index % 3) as isize - 1;  // -1, 0, or 1
    let mid_y = (base_mid as isize + offset).max(start_y as isize + 1) as usize;
    mid_y.min(end_y.saturating_sub(1))  // Leave room for vertical to target
}

/// Select the appropriate corner character for an L-shaped edge.
///
/// - `from_x`: position where we're placing the corner
/// - `to_x`: position we're connecting to
/// - `is_source`: true for source corner (vertical→horizontal), false for target (horizontal→vertical)
fn corner_char(from_x: usize, to_x: usize, is_source: bool, s: &StyleChars) -> char {
    match (from_x < to_x, is_source) {
        (true, true)   => s.corner_ul,  // Source going right: └
        (false, true)  => s.corner_ur,  // Source going left:  ┘
        (false, false) => s.corner_dr,  // Target from left:   ┐
        (true, false)  => s.corner_dl,  // Target from right:  ┌
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
