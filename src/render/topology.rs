//! Shared routing-topology helpers for criticism and repair.

use crate::style::StyleChars;

use super::canvas::Canvas;
use super::semantic::{CellOwnerKind, SemanticFrame};

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

fn frame_connects_up(frame: &SemanticFrame, x: usize, y: usize) -> bool {
    if y == 0 {
        return false;
    }

    let Some(above) = frame.get(x, y - 1) else {
        return false;
    };
    if above.owner_kind == CellOwnerKind::EdgeLabel {
        if y >= 2 {
            return char_connects_down(frame.get(x, y - 2).map(|cell| cell.ch).unwrap_or(' '));
        }
        return false;
    }

    char_connects_down(above.ch)
}

fn frame_connects_down(frame: &SemanticFrame, x: usize, y: usize) -> bool {
    if y + 1 >= frame.height {
        return false;
    }

    let Some(below) = frame.get(x, y + 1) else {
        return false;
    };
    if below.owner_kind == CellOwnerKind::EdgeLabel {
        if y + 2 < frame.height {
            return char_connects_up(frame.get(x, y + 2).map(|cell| cell.ch).unwrap_or(' '));
        }
        return false;
    }

    char_connects_up(below.ch)
}

fn frame_connects_left(frame: &SemanticFrame, x: usize, y: usize) -> bool {
    if x == 0 {
        return false;
    }

    let Some(left) = frame.get(x - 1, y) else {
        return false;
    };
    if left.owner_kind == CellOwnerKind::EdgeLabel {
        if x >= 2 {
            return char_connects_right(frame.get(x - 2, y).map(|cell| cell.ch).unwrap_or(' '));
        }
        return false;
    }

    char_connects_right(left.ch)
}

fn frame_connects_right(frame: &SemanticFrame, x: usize, y: usize) -> bool {
    if x + 1 >= frame.width {
        return false;
    }

    let Some(right) = frame.get(x + 1, y) else {
        return false;
    };
    if right.owner_kind == CellOwnerKind::EdgeLabel {
        if x + 2 < frame.width {
            return char_connects_left(frame.get(x + 2, y).map(|cell| cell.ch).unwrap_or(' '));
        }
        return false;
    }

    char_connects_left(right.ch)
}

fn canvas_connects_up(canvas: &Canvas, x: usize, y: usize) -> bool {
    if y == 0 {
        return false;
    }

    if matches!(
        canvas.get_meta(x, y - 1).map(|meta| meta.owner_kind),
        Some(CellOwnerKind::EdgeLabel)
    ) {
        if y >= 2 {
            return char_connects_down(canvas.get(x, y - 2));
        }
        return false;
    }

    char_connects_down(canvas.get(x, y - 1))
}

fn canvas_connects_down(canvas: &Canvas, x: usize, y: usize) -> bool {
    if y + 1 >= canvas.height {
        return false;
    }

    if matches!(
        canvas.get_meta(x, y + 1).map(|meta| meta.owner_kind),
        Some(CellOwnerKind::EdgeLabel)
    ) {
        if y + 2 < canvas.height {
            return char_connects_up(canvas.get(x, y + 2));
        }
        return false;
    }

    char_connects_up(canvas.get(x, y + 1))
}

fn canvas_connects_left(canvas: &Canvas, x: usize, y: usize) -> bool {
    if x == 0 {
        return false;
    }

    if matches!(
        canvas.get_meta(x - 1, y).map(|meta| meta.owner_kind),
        Some(CellOwnerKind::EdgeLabel)
    ) {
        if x >= 2 {
            return char_connects_right(canvas.get(x - 2, y));
        }
        return false;
    }

    char_connects_right(canvas.get(x - 1, y))
}

fn canvas_connects_right(canvas: &Canvas, x: usize, y: usize) -> bool {
    if x + 1 >= canvas.width {
        return false;
    }

    if matches!(
        canvas.get_meta(x + 1, y).map(|meta| meta.owner_kind),
        Some(CellOwnerKind::EdgeLabel)
    ) {
        if x + 2 < canvas.width {
            return char_connects_left(canvas.get(x + 2, y));
        }
        return false;
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
        '-' | '─'
            | '═'
            | '━'
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
        '-' | '─'
            | '═'
            | '━'
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
}
