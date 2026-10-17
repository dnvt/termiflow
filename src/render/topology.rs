//! Shared routing-topology helpers for criticism and repair.

use crate::graph::{Direction, Graph};
use crate::style::StyleChars;

use super::canvas::Canvas;
use super::semantic::{CellOwnerKind, SemanticFrame};

/// Return whether a subgraph participates in a direct BT boundary pair with
/// three or more non-back edges. The predicate identifies the topology that
/// needs an explicit local portal seam; projection policy decides its glyph.
pub(crate) fn has_bt_parallel_boundary_junction(graph: &Graph, subgraph_id: &str) -> bool {
    if graph.direction != Direction::BT {
        return false;
    }

    let mut pair_counts = std::collections::HashMap::<(&str, &str), usize>::new();
    for edge in &graph.edges {
        if edge.is_back_edge {
            continue;
        }
        let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
        if exits.len() != 1
            || enters.len() != 1
            || (exits[0] != subgraph_id && enters[0] != subgraph_id)
        {
            continue;
        }
        *pair_counts.entry((exits[0], enters[0])).or_default() += 1;
    }

    pair_counts.values().any(|count| *count >= 3)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Connections {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
}

impl Connections {
    pub fn arm_count(self) -> usize {
        [self.up, self.down, self.left, self.right]
            .into_iter()
            .filter(|connected| *connected)
            .count()
    }
}

pub fn frame_connections(frame: &SemanticFrame, x: usize, y: usize) -> Connections {
    Connections {
        up: frame_connects_up(frame, x, y),
        down: frame_connects_down(frame, x, y),
        left: frame_connects_left(frame, x, y),
        right: frame_connects_right(frame, x, y),
    }
}

pub fn canvas_connections(canvas: &Canvas, x: usize, y: usize) -> Connections {
    Connections {
        up: canvas_connects_up(canvas, x, y),
        down: canvas_connects_down(canvas, x, y),
        left: canvas_connects_left(canvas, x, y),
        right: canvas_connects_right(canvas, x, y),
    }
}

/// Read access to a rendered grid, the canvas while rendering or the semantic
/// frame the critic reads, so both judge ASCII corners by the same rules.
trait GlyphGrid {
    fn glyph(&self, x: usize, y: usize) -> char;
    fn owner(&self, x: usize, y: usize) -> Option<CellOwnerKind>;
}

impl GlyphGrid for Canvas {
    fn glyph(&self, x: usize, y: usize) -> char {
        self.get(x, y)
    }

    fn owner(&self, x: usize, y: usize) -> Option<CellOwnerKind> {
        self.get_meta(x, y).map(|meta| meta.owner_kind)
    }
}

impl GlyphGrid for SemanticFrame {
    fn glyph(&self, x: usize, y: usize) -> char {
        self.get(x, y).map_or(' ', |cell| cell.ch)
    }

    fn owner(&self, x: usize, y: usize) -> Option<CellOwnerKind> {
        self.get(x, y).map(|cell| cell.owner_kind)
    }
}

fn frame_connects_up(frame: &SemanticFrame, x: usize, y: usize) -> bool {
    if y == 0 {
        return false;
    }
    if is_ascii_frame_corner(frame, x, y - 1, is_ascii_plain_h(frame.glyph(x, y)))
        || (is_ascii_plain_h(frame.glyph(x, y))
            && ascii_corner_is_complete(frame, x, y - 1, (0, 1)))
    {
        return false;
    }

    let Some(above) = frame.get(x, y - 1) else {
        return false;
    };
    match above.owner_kind {
        CellOwnerKind::EdgeLabel => {
            if y >= 2 {
                return char_connects_down(frame.get(x, y - 2).map(|cell| cell.ch).unwrap_or(' '))
                    && !(is_ascii_plain_h(frame.glyph(x, y))
                        && ascii_corner_is_complete(frame, x, y - 2, (0, 1)));
            }
            return false;
        }
        CellOwnerKind::NodeLabel | CellOwnerKind::SubgraphTitle => return false,
        _ => {}
    }

    char_connects_down(above.ch)
}

fn frame_connects_down(frame: &SemanticFrame, x: usize, y: usize) -> bool {
    if y + 1 >= frame.height {
        return false;
    }
    if is_ascii_frame_corner(frame, x, y + 1, is_ascii_plain_h(frame.glyph(x, y)))
        || (is_ascii_plain_h(frame.glyph(x, y))
            && ascii_corner_is_complete(frame, x, y + 1, (0, -1)))
    {
        return false;
    }

    let Some(below) = frame.get(x, y + 1) else {
        return false;
    };
    match below.owner_kind {
        CellOwnerKind::EdgeLabel => {
            if y + 2 < frame.height {
                return char_connects_up(frame.get(x, y + 2).map(|cell| cell.ch).unwrap_or(' '))
                    && !(is_ascii_plain_h(frame.glyph(x, y))
                        && ascii_corner_is_complete(frame, x, y + 2, (0, -1)));
            }
            return false;
        }
        CellOwnerKind::NodeLabel | CellOwnerKind::SubgraphTitle => return false,
        _ => {}
    }

    char_connects_up(below.ch)
}

fn frame_connects_left(frame: &SemanticFrame, x: usize, y: usize) -> bool {
    if x == 0 {
        return false;
    }
    if is_ascii_frame_corner(frame, x - 1, y, is_ascii_plain_v(frame.glyph(x, y)))
        || (is_ascii_plain_v(frame.glyph(x, y))
            && ascii_corner_is_complete(frame, x - 1, y, (1, 0)))
    {
        return false;
    }

    let Some(left) = frame.get(x - 1, y) else {
        return false;
    };
    match left.owner_kind {
        CellOwnerKind::EdgeLabel => {
            if x >= 2 {
                return char_connects_right(frame.get(x - 2, y).map(|cell| cell.ch).unwrap_or(' '))
                    && !(is_ascii_plain_v(frame.glyph(x, y))
                        && ascii_corner_is_complete(frame, x - 2, y, (1, 0)));
            }
            return false;
        }
        CellOwnerKind::NodeLabel | CellOwnerKind::SubgraphTitle => return false,
        _ => {}
    }

    char_connects_right(left.ch)
}

fn frame_connects_right(frame: &SemanticFrame, x: usize, y: usize) -> bool {
    if x + 1 >= frame.width {
        return false;
    }
    if is_ascii_frame_corner(frame, x + 1, y, is_ascii_plain_v(frame.glyph(x, y)))
        || (is_ascii_plain_v(frame.glyph(x, y))
            && ascii_corner_is_complete(frame, x + 1, y, (-1, 0)))
    {
        return false;
    }

    let Some(right) = frame.get(x + 1, y) else {
        return false;
    };
    match right.owner_kind {
        CellOwnerKind::EdgeLabel => {
            if x + 2 < frame.width {
                return char_connects_left(frame.get(x + 2, y).map(|cell| cell.ch).unwrap_or(' '))
                    && !(is_ascii_plain_v(frame.glyph(x, y))
                        && ascii_corner_is_complete(frame, x + 2, y, (-1, 0)));
            }
            return false;
        }
        CellOwnerKind::NodeLabel | CellOwnerKind::SubgraphTitle => return false,
        _ => {}
    }

    char_connects_left(right.ch)
}

/// Whether the ASCII frame or box corner at `(x, y)` should not count as an arm
/// of the asking cell, when that cell is a plain straight edge (`plain_run`: `-` with
/// the corner above or below it, `|` with the corner beside it). An ASCII
/// corner is a `+`, which joins every direction; read as a route glyph it
/// would turn an edge merely passing the corner into a junction. Turns and
/// junctions keep the arm, so routes that already bend at a corner are left
/// as they are.
fn is_ascii_frame_corner(grid: &impl GlyphGrid, x: usize, y: usize, plain_run: bool) -> bool {
    let owner = |x: usize| grid.owner(x, y);
    plain_run
        && grid.glyph(x, y) == '+'
        && match owner(x) {
            Some(CellOwnerKind::SubgraphBorder) => true,
            // A box corner has its border on one side only; a `+` with border
            // on both sides is a tee where an edge leaves the box.
            Some(CellOwnerKind::NodeBorder) => {
                let left = x > 0 && owner(x - 1) == Some(CellOwnerKind::NodeBorder);
                let right = owner(x + 1) == Some(CellOwnerKind::NodeBorder);
                left != right
            }
            _ => false,
        }
}

/// A plain ASCII horizontal run cell: solid, dotted or thick.
fn is_ascii_plain_h(ch: char) -> bool {
    matches!(ch, '-' | '.' | '=')
}

/// A plain ASCII vertical run cell: solid or dotted.
fn is_ascii_plain_v(ch: char) -> bool {
    matches!(ch, '|' | ':')
}

/// Whether the ASCII `+` at `(x, y)` already turns or passes through with two
/// of its other sides, so the plain run on `side` (the asking cell, relative
/// to the `+`) is not one of its arms: a `-` cannot take a vertical arm, nor
/// a `|` a horizontal one.
fn ascii_corner_is_complete(
    grid: &impl GlyphGrid,
    x: usize,
    y: usize,
    side: (isize, isize),
) -> bool {
    if grid.glyph(x, y) != '+' {
        return false;
    }
    // Box and frame borders have their own rules; this one is for edge turns.
    let edge_owned = matches!(
        grid.owner(x, y),
        Some(CellOwnerKind::EdgeSegment | CellOwnerKind::Junction | CellOwnerKind::CycleEdge)
    );
    if !edge_owned {
        return false;
    }
    let arms = [(0, -1), (0, 1), (-1, 0), (1, 0)]
        .into_iter()
        .filter(|&step| step != side)
        .filter(|&(dx, dy)| {
            let (Some(nx), Some(ny)) = (x.checked_add_signed(dx), y.checked_add_signed(dy)) else {
                return false;
            };
            let ch = grid.glyph(nx, ny);
            match (dx, dy) {
                (0, -1) => char_connects_down(ch),
                (0, 1) => char_connects_up(ch),
                (-1, 0) => char_connects_right(ch),
                _ => char_connects_left(ch),
            }
        })
        .count();
    arms >= 2
}

fn canvas_connects_up(canvas: &Canvas, x: usize, y: usize) -> bool {
    if y == 0 {
        return false;
    }
    if is_ascii_frame_corner(canvas, x, y - 1, is_ascii_plain_h(canvas.get(x, y)))
        || (is_ascii_plain_h(canvas.get(x, y))
            && ascii_corner_is_complete(canvas, x, y - 1, (0, 1)))
    {
        return false;
    }

    match canvas.get_meta(x, y - 1).map(|meta| meta.owner_kind) {
        Some(CellOwnerKind::EdgeLabel) => {
            if y >= 2 {
                return char_connects_down(canvas.get(x, y - 2))
                    && !(is_ascii_plain_h(canvas.get(x, y))
                        && ascii_corner_is_complete(canvas, x, y - 2, (0, 1)));
            }
            return false;
        }
        Some(CellOwnerKind::NodeLabel | CellOwnerKind::SubgraphTitle) => return false,
        _ => {}
    }

    char_connects_down(canvas.get(x, y - 1))
}

fn canvas_connects_down(canvas: &Canvas, x: usize, y: usize) -> bool {
    if y + 1 >= canvas.height {
        return false;
    }
    if is_ascii_frame_corner(canvas, x, y + 1, is_ascii_plain_h(canvas.get(x, y)))
        || (is_ascii_plain_h(canvas.get(x, y))
            && ascii_corner_is_complete(canvas, x, y + 1, (0, -1)))
    {
        return false;
    }

    match canvas.get_meta(x, y + 1).map(|meta| meta.owner_kind) {
        Some(CellOwnerKind::EdgeLabel) => {
            if y + 2 < canvas.height {
                return char_connects_up(canvas.get(x, y + 2))
                    && !(is_ascii_plain_h(canvas.get(x, y))
                        && ascii_corner_is_complete(canvas, x, y + 2, (0, -1)));
            }
            return false;
        }
        Some(CellOwnerKind::NodeLabel | CellOwnerKind::SubgraphTitle) => return false,
        _ => {}
    }

    char_connects_up(canvas.get(x, y + 1))
}

fn canvas_connects_left(canvas: &Canvas, x: usize, y: usize) -> bool {
    if x == 0 {
        return false;
    }
    if is_ascii_frame_corner(canvas, x - 1, y, is_ascii_plain_v(canvas.get(x, y)))
        || (is_ascii_plain_v(canvas.get(x, y))
            && ascii_corner_is_complete(canvas, x - 1, y, (1, 0)))
    {
        return false;
    }

    match canvas.get_meta(x - 1, y).map(|meta| meta.owner_kind) {
        Some(CellOwnerKind::EdgeLabel) => {
            if x >= 2 {
                return char_connects_right(canvas.get(x - 2, y))
                    && !(is_ascii_plain_v(canvas.get(x, y))
                        && ascii_corner_is_complete(canvas, x - 2, y, (1, 0)));
            }
            return false;
        }
        Some(CellOwnerKind::NodeLabel | CellOwnerKind::SubgraphTitle) => return false,
        _ => {}
    }

    char_connects_right(canvas.get(x - 1, y))
}

fn canvas_connects_right(canvas: &Canvas, x: usize, y: usize) -> bool {
    if x + 1 >= canvas.width {
        return false;
    }
    if is_ascii_frame_corner(canvas, x + 1, y, is_ascii_plain_v(canvas.get(x, y)))
        || (is_ascii_plain_v(canvas.get(x, y))
            && ascii_corner_is_complete(canvas, x + 1, y, (-1, 0)))
    {
        return false;
    }

    match canvas.get_meta(x + 1, y).map(|meta| meta.owner_kind) {
        Some(CellOwnerKind::EdgeLabel) => {
            if x + 2 < canvas.width {
                return char_connects_left(canvas.get(x + 2, y))
                    && !(is_ascii_plain_v(canvas.get(x, y))
                        && ascii_corner_is_complete(canvas, x + 2, y, (-1, 0)));
            }
            return false;
        }
        Some(CellOwnerKind::NodeLabel | CellOwnerKind::SubgraphTitle) => return false,
        _ => {}
    }

    char_connects_left(canvas.get(x + 1, y))
}

pub fn canonical_routing_glyph(
    connections: Connections,
    chars: &StyleChars,
    owner_kind: CellOwnerKind,
) -> Option<char> {
    let horizontal = if owner_kind == CellOwnerKind::CycleEdge {
        chars.back_h
    } else {
        chars.edge_h
    };
    let vertical = if owner_kind == CellOwnerKind::CycleEdge {
        chars.back_v
    } else {
        chars.edge_v
    };

    match connections.arm_count() {
        4 => Some(chars.cross),
        3 => {
            if !connections.up {
                Some(chars.junction_down)
            } else if !connections.down {
                Some(chars.junction_up)
            } else if !connections.left {
                Some(chars.junction_right)
            } else {
                Some(chars.junction_left)
            }
        }
        2 => {
            if connections.up && connections.down {
                Some(vertical)
            } else if connections.left && connections.right {
                Some(horizontal)
            } else if connections.down && connections.right {
                Some(chars.corner_dl)
            } else if connections.down && connections.left {
                Some(chars.corner_dr)
            } else if connections.up && connections.right {
                Some(chars.corner_ul)
            } else if connections.up && connections.left {
                Some(chars.corner_ur)
            } else {
                None
            }
        }
        1 => {
            if connections.up || connections.down {
                Some(vertical)
            } else if connections.left || connections.right {
                Some(horizontal)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(crate) fn char_connects_up(ch: char) -> bool {
    matches!(
        ch,
        '|' | ':'
            | '│'
            | '║'
            | '┃'
            | '╎'
            | '┆'
            | '┊'
            | '┋'
            | '╏'
            | '█'
            | '+'
            | '┼'
            | '╬'
            | '╋'
            | '├'
            | '┤'
            | '┴'
            | '╠'
            | '╣'
            | '╩'
            | '┣'
            | '┫'
            | '┻'
            | '└'
            | '┘'
            | '╚'
            | '╝'
            | '╰'
            | '╯'
            | 'v'
            | '↓'
            | '▼'
            | 'o'
            | '○'
            | 'x'
            | '✕'
    )
}

pub(crate) fn char_connects_down(ch: char) -> bool {
    matches!(
        ch,
        '|' | ':'
            | '│'
            | '║'
            | '┃'
            | '╎'
            | '┆'
            | '┊'
            | '┋'
            | '╏'
            | '█'
            | '+'
            | '┼'
            | '╬'
            | '╋'
            | '├'
            | '┤'
            | '┬'
            | '╠'
            | '╣'
            | '╦'
            | '┣'
            | '┫'
            | '┳'
            | '┌'
            | '┐'
            | '╔'
            | '╗'
            | '╭'
            | '╮'
            | '^'
            | '↑'
            | '▲'
            | 'o'
            | '○'
            | 'x'
            | '✕'
    )
}

pub(crate) fn char_connects_left(ch: char) -> bool {
    matches!(
        ch,
        '-' | '='
            | '.'
            | '─'
            | '═'
            | '━'
            | '╌'
            | '╴'
            | '╶'
            | '╸'
            | '╺'
            | '█'
            | '+'
            | '┼'
            | '╬'
            | '╋'
            | '┤'
            | '┬'
            | '┴'
            | '╣'
            | '╦'
            | '╩'
            | '┫'
            | '┳'
            | '┻'
            | '┐'
            | '┘'
            | '╗'
            | '╝'
            | '╮'
            | '╯'
            | '>'
            | '→'
            | '▶'
            | 'o'
            | '○'
            | 'x'
            | '✕'
    )
}

pub(crate) fn char_connects_right(ch: char) -> bool {
    matches!(
        ch,
        '-' | '='
            | '.'
            | '─'
            | '═'
            | '━'
            | '╌'
            | '╴'
            | '╶'
            | '╸'
            | '╺'
            | '█'
            | '+'
            | '┼'
            | '╬'
            | '╋'
            | '├'
            | '┬'
            | '┴'
            | '╠'
            | '╦'
            | '╩'
            | '┣'
            | '┳'
            | '┻'
            | '┌'
            | '└'
            | '╔'
            | '╚'
            | '╭'
            | '╰'
            | '<'
            | '←'
            | '◀'
            | 'o'
            | '○'
            | 'x'
            | '✕'
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::semantic::{CellMeta, CellRole};
    use crate::style::{BaseStyle, CompositeStyle};

    fn unicode_chars() -> StyleChars {
        CompositeStyle::default().to_style_chars(BaseStyle::Unicode)
    }

    #[test]
    fn canonical_routing_glyph_prefers_cycle_verticals_for_cycle_edges() {
        let chars = unicode_chars();
        let glyph = canonical_routing_glyph(
            Connections {
                up: true,
                down: false,
                left: false,
                right: false,
            },
            &chars,
            CellOwnerKind::CycleEdge,
        );

        assert_eq!(glyph, Some(chars.back_v));
    }

    #[test]
    fn frame_connections_detects_corner_turns() {
        let mut cells = vec![CellMeta::default(); 9];
        cells[4] = CellMeta {
            ch: '┐',
            owner_kind: CellOwnerKind::EdgeSegment,
            owner_id: Some("edge:0:A->B".to_string()),
            role: CellRole::Corner,
            z_index: 5,
        };
        cells[3] = CellMeta {
            ch: '─',
            owner_kind: CellOwnerKind::EdgeSegment,
            owner_id: Some("edge:0:A->B".to_string()),
            role: CellRole::Horizontal,
            z_index: 5,
        };
        cells[7] = CellMeta {
            ch: '│',
            owner_kind: CellOwnerKind::EdgeSegment,
            owner_id: Some("edge:0:A->B".to_string()),
            role: CellRole::Vertical,
            z_index: 5,
        };
        let frame = SemanticFrame {
            width: 3,
            height: 3,
            cells,
        };

        assert_eq!(
            frame_connections(&frame, 1, 1),
            Connections {
                up: false,
                down: true,
                left: true,
                right: false,
            }
        );
    }

    #[test]
    fn topology_connections_recognize_styled_route_shafts() {
        assert!(char_connects_left('.'));
        assert!(char_connects_right('='));
        assert!(char_connects_up('╎'));
        assert!(char_connects_down('╎'));
    }
}
