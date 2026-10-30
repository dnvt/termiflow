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
                    && !is_ascii_frame_corner(
                        frame,
                        x,
                        y - 2,
                        is_ascii_plain_h(frame.glyph(x, y)),
                    )
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
                    && !is_ascii_frame_corner(
                        frame,
                        x,
                        y + 2,
                        is_ascii_plain_h(frame.glyph(x, y)),
                    )
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
                    && !is_ascii_frame_corner(
                        frame,
                        x - 2,
                        y,
                        is_ascii_plain_v(frame.glyph(x, y)),
                    )
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
                    && !is_ascii_frame_corner(
                        frame,
                        x + 2,
                        y,
                        is_ascii_plain_v(frame.glyph(x, y)),
                    )
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
                    && !is_ascii_frame_corner(
                        canvas,
                        x,
                        y - 2,
                        is_ascii_plain_h(canvas.get(x, y)),
                    )
                    && !(is_ascii_plain_h(canvas.get(x, y))
                        && ascii_corner_is_complete(canvas, x, y - 2, (0, 1)));
            }
            return false;
        }
        Some(CellOwnerKind::NodeLabel | CellOwnerKind::SubgraphTitle) => return false,
        _ => {}
    }

    char_connects_down(route_glyph(canvas, x, y - 1))
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
                    && !is_ascii_frame_corner(
                        canvas,
                        x,
                        y + 2,
                        is_ascii_plain_h(canvas.get(x, y)),
                    )
                    && !(is_ascii_plain_h(canvas.get(x, y))
                        && ascii_corner_is_complete(canvas, x, y + 2, (0, -1)));
            }
            return false;
        }
        Some(CellOwnerKind::NodeLabel | CellOwnerKind::SubgraphTitle) => return false,
        _ => {}
    }

    char_connects_up(route_glyph(canvas, x, y + 1))
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
                    && !is_ascii_frame_corner(
                        canvas,
                        x - 2,
                        y,
                        is_ascii_plain_v(canvas.get(x, y)),
                    )
                    && !(is_ascii_plain_v(canvas.get(x, y))
                        && ascii_corner_is_complete(canvas, x - 2, y, (1, 0)));
            }
            return false;
        }
        Some(CellOwnerKind::NodeLabel | CellOwnerKind::SubgraphTitle) => return false,
        _ => {}
    }

    char_connects_right(route_glyph(canvas, x - 1, y))
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
                    && !is_ascii_frame_corner(
                        canvas,
                        x + 2,
                        y,
                        is_ascii_plain_v(canvas.get(x, y)),
                    )
                    && !(is_ascii_plain_v(canvas.get(x, y))
                        && ascii_corner_is_complete(canvas, x + 2, y, (-1, 0)));
            }
            return false;
        }
        Some(CellOwnerKind::NodeLabel | CellOwnerKind::SubgraphTitle) => return false,
        _ => {}
    }

    char_connects_left(route_glyph(canvas, x + 1, y))
}

/// The heavy corners stay out of the glyph tables, which would otherwise
/// read every frame corner as joining the line beside it; a heavy corner a
/// route owns (the heavy style) reads as its light twin and joins like any
/// other (gen_d_203: Hub's self-loop corner beside A -> H's crossing).
fn route_glyph(canvas: &Canvas, x: usize, y: usize) -> char {
    let ch = canvas.get(x, y);
    let routed = canvas.get_meta(x, y).is_some_and(|meta| {
        matches!(
            meta.owner_kind,
            CellOwnerKind::EdgeSegment | CellOwnerKind::CycleEdge | CellOwnerKind::Junction
        )
    });
    match ch {
        '┏' if routed => '┌',
        '┓' if routed => '┐',
        '┗' if routed => '└',
        '┛' if routed => '┘',
        _ => ch,
    }
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

    /// A heavy corner a route owns joins its two neighbours; a frame's
    /// heavy corner joins none.
    #[test]
    fn heavy_route_corners_join_their_neighbours_and_frame_corners_do_not() {
        let corner = |ch: char, owner: CellOwnerKind| {
            let mut canvas = Canvas::new(3, 3);
            canvas.set_owned(1, 1, ch, owner, "edge:0:A->B", 1);
            [
                canvas_connections(&canvas, 1, 0).down,
                canvas_connections(&canvas, 1, 2).up,
                canvas_connections(&canvas, 0, 1).right,
                canvas_connections(&canvas, 2, 1).left,
            ]
        };
        let edge = CellOwnerKind::EdgeSegment;
        assert_eq!(corner('┏', edge), [false, true, false, true]);
        assert_eq!(corner('┓', edge), [false, true, true, false]);
        assert_eq!(corner('┗', edge), [true, false, false, true]);
        assert_eq!(corner('┛', edge), [true, false, true, false]);
        assert_eq!(
            corner('┏', CellOwnerKind::SubgraphBorder),
            [false, false, false, false]
        );
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

    // TEST-ONLY PROPOSAL: append inside existing render::topology::tests.
    // Base topology SHA5e39dbed...; no production edits, parsing, compile or execution.
    // Three #[test] functions. All helper names are private to this test appendix.
    type TcPoint = (usize, usize);

    #[derive(Clone, Copy, Debug)]
    enum TcLook {
        Up,
        Down,
        Left,
        Right,
    }
    impl TcLook {
        fn step(self) -> (isize, isize) {
            match self {
                Self::Up => (0, -1),
                Self::Down => (0, 1),
                Self::Left => (-1, 0),
                Self::Right => (1, 0),
            }
        }
        fn horizontal(self) -> bool {
            matches!(self, Self::Left | Self::Right)
        }
        fn plain(self) -> char {
            if self.horizontal() {
                '|'
            } else {
                '-'
            }
        }
        fn aligned(self) -> char {
            if self.horizontal() {
                '-'
            } else {
                '|'
            }
        }
        fn arm(self, c: Connections) -> bool {
            match self {
                Self::Up => c.up,
                Self::Down => c.down,
                Self::Left => c.left,
                Self::Right => c.right,
            }
        }
    }
    fn tc_off(p: (usize, usize), d: (isize, isize)) -> (usize, usize) {
        (
            p.0.checked_add_signed(d.0).unwrap(),
            p.1.checked_add_signed(d.1).unwrap(),
        )
    }
    fn tc_put(c: &mut Canvas, p: (usize, usize), ch: char, owner: CellOwnerKind, role: CellRole) {
        let (id, z) = match owner {
            CellOwnerKind::NodeBorder | CellOwnerKind::NodeLabel => ("box", 3),
            CellOwnerKind::SubgraphBorder | CellOwnerKind::SubgraphTitle => ("frame", 2),
            CellOwnerKind::EdgeLabel => ("edge:0:A->B", 6),
            _ => ("edge:0:A->B", 5),
        };
        c.set_owned_with_role(p.0, p.1, ch, owner, id, role, z);
    }
    fn tc_profiles() -> Vec<StyleChars> {
        let ascii = CompositeStyle::from_base(BaseStyle::Ascii);
        let mut mixed = ascii.clone();
        mixed.border = Some(BaseStyle::Unicode);
        mixed.junction = Some(BaseStyle::Unicode);
        let mut double_border = ascii.clone();
        double_border.border = Some(BaseStyle::Double);
        double_border.junction = Some(BaseStyle::Heavy);
        [ascii, mixed, double_border]
            .into_iter()
            .map(|s| s.to_style_chars(BaseStyle::Unicode))
            .collect()
    }
    fn tc_base(look: TcLook, asking: char) -> (Canvas, TcPoint, TcPoint, TcPoint) {
        let mut c = Canvas::new(11, 11);
        let q = (5, 5);
        let m = tc_off(q, look.step());
        let f = tc_off(m, look.step());
        let vertical = matches!(asking, '|' | ':') || asking == '+' && look.horizontal();
        let axis = if vertical { (0, 1) } else { (1, 0) };
        let role = if asking == '+' {
            CellRole::Corner
        } else if vertical {
            CellRole::Vertical
        } else {
            CellRole::Horizontal
        };
        tc_put(&mut c, q, asking, CellOwnerKind::EdgeSegment, role);
        for sign in [-1, 1] {
            let p = tc_off(q, (axis.0 * sign, axis.1 * sign));
            tc_put(
                &mut c,
                p,
                if vertical { '|' } else { '-' },
                CellOwnerKind::EdgeSegment,
                if vertical {
                    CellRole::Vertical
                } else {
                    CellRole::Horizontal
                },
            );
        }
        tc_put(&mut c, m, 'e', CellOwnerKind::EdgeLabel, CellRole::Text);
        (c, q, m, f)
    }
    fn tc_corner(
        c: &mut Canvas,
        f: (usize, usize),
        look: TcLook,
        owner: CellOwnerKind,
        chars: &StyleChars,
    ) {
        let border = matches!(
            owner,
            CellOwnerKind::NodeBorder | CellOwnerKind::SubgraphBorder
        );
        tc_put(
            c,
            f,
            '+',
            owner,
            if border {
                CellRole::Border
            } else {
                CellRole::Corner
            },
        );
        let h = if matches!(look, TcLook::Right) {
            (1, 0)
        } else {
            (-1, 0)
        };
        let v = if matches!(look, TcLook::Down) {
            (0, 1)
        } else {
            (0, -1)
        };
        tc_put(
            c,
            tc_off(f, h),
            if border { chars.h } else { '-' },
            owner,
            if border {
                CellRole::Border
            } else {
                CellRole::Horizontal
            },
        );
        tc_put(
            c,
            tc_off(f, v),
            if border { chars.v } else { '|' },
            owner,
            if border {
                CellRole::Border
            } else {
                CellRole::Vertical
            },
        );
    }
    fn tc_views(c: &Canvas, q: (usize, usize), look: TcLook, expected: bool, case: &str) {
        let frame = SemanticFrame::from_canvas(c);
        let a = canvas_connections(c, q.0, q.1);
        assert_eq!(look.arm(a), expected, "tc-mask canvas {look:?} {case}");
        let b = frame_connections(&frame, q.0, q.1);
        assert_eq!(look.arm(b), expected, "tc-mask frame {look:?} {case}");
        assert_eq!(
            [a.up, a.down, a.left, a.right],
            [b.up, b.down, b.left, b.right],
            "grid parity {look:?} {case}"
        );
        assert_eq!(
            SemanticFrame::from_canvas(c),
            frame,
            "queries preserve cells/metadata"
        );
    }
    #[test]
    fn topology_edge_label_reuses_boundary_corner_mask() {
        // First parent-red case: Up / '-' / NodeBorder / pure ASCII / Canvas arm.
        for look in [TcLook::Up, TcLook::Down, TcLook::Left, TcLook::Right] {
            let glyphs = if look.horizontal() {
                vec!['|', ':']
            } else {
                vec!['-', '.', '=']
            };
            for asking in glyphs {
                for owner in [CellOwnerKind::NodeBorder, CellOwnerKind::SubgraphBorder] {
                    for (profile, chars) in tc_profiles().into_iter().enumerate() {
                        let (mut c, q, m, f) = tc_base(look, asking);
                        tc_corner(&mut c, f, look, owner, &chars);
                        assert_eq!(
                            c.get_meta(m.0, m.1).unwrap().owner_kind,
                            CellOwnerKind::EdgeLabel
                        );
                        let plain = if look.horizontal() {
                            is_ascii_plain_v(asking)
                        } else {
                            is_ascii_plain_h(asking)
                        };
                        assert!(
                            is_ascii_frame_corner(&c, f.0, f.1, plain),
                            "existing boundary mask setup"
                        );
                        let case =
                            format!("boundary={owner:?} asking={asking:?} profile={profile}");
                        tc_views(&c, q, look, false, &case);
                        if matches!(look, TcLook::Up)
                            && asking == '-'
                            && matches!(owner, CellOwnerKind::NodeBorder)
                            && profile == 0
                        {
                            let mut repaired = c.clone();
                            crate::render::repair::stabilize_degree_mismatches(
                                &mut repaired,
                                &chars,
                            );
                            assert_eq!(
                                repaired.get(q.0, q.1),
                                '-',
                                "degree repair must not add the distant box-corner arm"
                            );
                            crate::render::repair::stabilize_routing_topology_keeping(
                                &mut repaired,
                                &chars,
                                &std::collections::HashSet::new(),
                            );
                            assert_eq!(
                                repaired.get(q.0, q.1),
                                '-',
                                "later topology repair preserves the same mask"
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn topology_edge_label_mask_preserves_real_ports_and_opaque_neighbors() {
        for look in [TcLook::Up, TcLook::Down, TcLook::Left, TcLook::Right] {
            for chars in tc_profiles() {
                for (case, expected) in [
                    ("node-tee", true),
                    ("route-shaft", true),
                    ("route-corner", true),
                    ("route-tee", true),
                    ("complete-edge-corner", false),
                    ("explicit-turn", true),
                    ("node-label", false),
                    ("title", false),
                ] {
                    let asking = match case {
                        "node-tee" | "complete-edge-corner" => look.plain(),
                        "explicit-turn" => '+',
                        _ => look.aligned(),
                    };
                    let (mut c, q, m, f) = tc_base(look, asking);
                    match case {
                        "node-tee" => {
                            tc_put(&mut c, f, '+', CellOwnerKind::NodeBorder, CellRole::Border);
                            let axis = if look.horizontal() { (0, 1) } else { (1, 0) };
                            for sign in [-1, 1] {
                                tc_put(
                                    &mut c,
                                    tc_off(f, (axis.0 * sign, axis.1 * sign)),
                                    if look.horizontal() { chars.v } else { chars.h },
                                    CellOwnerKind::NodeBorder,
                                    CellRole::Border,
                                );
                            }
                            assert!(
                                !is_ascii_frame_corner(&c, f.0, f.1, true),
                                "two-sided real box tee setup"
                            );
                        }
                        "complete-edge-corner" => {
                            tc_corner(&mut c, f, look, CellOwnerKind::EdgeSegment, &chars)
                        }
                        "explicit-turn" => {
                            tc_corner(&mut c, f, look, CellOwnerKind::NodeBorder, &chars);
                            let clear = if look.horizontal() {
                                (q.0, q.1 + 1)
                            } else {
                                (q.0 + 1, q.1)
                            };
                            c.set(clear.0, clear.1, ' ');
                        }
                        "route-corner" | "route-tee" => {
                            tc_put(
                                &mut c,
                                f,
                                '+',
                                CellOwnerKind::EdgeSegment,
                                if case == "route-corner" {
                                    CellRole::Corner
                                } else {
                                    CellRole::Junction
                                },
                            );
                            let axis = if look.horizontal() { (0, 1) } else { (1, 0) };
                            let signs: &[isize] =
                                if case == "route-tee" { &[-1, 1] } else { &[-1] };
                            for &sign in signs {
                                tc_put(
                                    &mut c,
                                    tc_off(f, (axis.0 * sign, axis.1 * sign)),
                                    if look.horizontal() { '|' } else { '-' },
                                    CellOwnerKind::EdgeSegment,
                                    if look.horizontal() {
                                        CellRole::Vertical
                                    } else {
                                        CellRole::Horizontal
                                    },
                                );
                            }
                        }
                        _ => {
                            let role = if look.horizontal() {
                                CellRole::Horizontal
                            } else {
                                CellRole::Vertical
                            };
                            tc_put(&mut c, f, look.aligned(), CellOwnerKind::EdgeSegment, role);
                            tc_put(
                                &mut c,
                                tc_off(f, look.step()),
                                look.aligned(),
                                CellOwnerKind::EdgeSegment,
                                role,
                            );
                            if case == "node-label" || case == "title" {
                                // Only the IMMEDIATE cell is opaque. No new far-text ownership rule.
                                tc_put(
                                    &mut c,
                                    m,
                                    look.aligned(),
                                    if case == "node-label" {
                                        CellOwnerKind::NodeLabel
                                    } else {
                                        CellOwnerKind::SubgraphTitle
                                    },
                                    CellRole::Text,
                                );
                            }
                        }
                    }
                    tc_views(&c, q, look, expected, case);
                }
            }
            let (q, m) = match look {
                TcLook::Up => ((0, 1), (0, 0)),
                TcLook::Down => ((0, 0), (0, 1)),
                TcLook::Left => ((1, 0), (0, 0)),
                TcLook::Right => ((0, 0), (1, 0)),
            };
            let mut tiny = Canvas::new(2, 2);
            tc_put(
                &mut tiny,
                q,
                look.plain(),
                CellOwnerKind::EdgeSegment,
                if look.horizontal() {
                    CellRole::Vertical
                } else {
                    CellRole::Horizontal
                },
            );
            tc_put(&mut tiny, m, 'e', CellOwnerKind::EdgeLabel, CellRole::Text);
            tc_views(&tiny, q, look, false, "two-hop outside physical bounds");
        }
    }
    #[test]
    fn topology_td_caption_does_not_grow_a_distant_box_corner_arm() {
        // Actual fixture, public API, styles × default/optimized Config lanes.
        // This is not CLI/native/canonical/authored policy equivalence evidence.
        let input = include_str!("../../tests/fixtures/inputs/label_junction_td.md");
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let outcome = crate::render_with_feedback(
                    input,
                    crate::RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized)
                        .with_crop(false)
                        .with_pad(0),
                )
                .expect("actual TD fixture render");
                let frame = &outcome.output;
                let rows: Vec<Vec<char>> = frame.lines().map(|r| r.chars().collect()).collect();
                let at = |x: usize, y: usize| rows.get(y).and_then(|r| r.get(x)).copied();
                let ascii = style == BaseStyle::Ascii;
                for label in ["Source", "Other", "Target", "label 1", "label 2"] {
                    assert_eq!(
                        frame.matches(label).count(),
                        1,
                        "fixture text setup {label}:\n{frame}"
                    );
                }
                assert_eq!(
                    frame
                        .chars()
                        .filter(|&c| c == if ascii { 'v' } else { '↓' })
                        .count(),
                    2,
                    "two distinct heads setup:\n{frame}"
                );
                assert_eq!(
                    at(6, 4),
                    Some(if ascii { '+' } else { '└' }),
                    "restored Source turn setup:\n{frame}"
                );
                assert_eq!(
                    at(21, 4),
                    Some(if ascii { '|' } else { '│' }),
                    "Other lane setup:\n{frame}"
                );
                assert_eq!(
                    at(11, 2),
                    Some(if ascii { '+' } else { '┘' }),
                    "Source corner setup:\n{frame}"
                );
                assert_eq!(
                    at(11, 3),
                    Some('e'),
                    "caption between corner and run setup:\n{frame}"
                );
                assert_eq!(at(11,4),Some(if ascii { '-' } else { '─' }),"TD caption must not invent a box-corner arm at (11,4): {style:?} optimized={optimized}\n{frame}");
            }
        }
    }
}
