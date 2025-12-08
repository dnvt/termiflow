//! Border styles and character sets
//!
//! See SPEC §4 for complete character definitions

use unicode_width::UnicodeWidthStr;

/// Grid constants (SPEC §2.1)
pub const BOX_HEIGHT: usize = 3;
pub const BOX_MIN_WIDTH: usize = 5;
pub const BOX_PADDING: usize = 2;
pub const ROW_SPACING: usize = 2;
pub const COL_SPACING: usize = 3;
pub const EDGE_VERTICAL_GAP: usize = 1;
pub const MAX_LABEL_WIDTH: usize = 20;

pub const MAX_CANVAS_WIDTH: usize = 500;
pub const MAX_CANVAS_HEIGHT: usize = 200;
pub const MAX_NODES: usize = 100;

/// Back-edge gutter (reserved right margin for cycle rendering)
pub const RIGHT_GUTTER: usize = 4;

/// Border style variants
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum BorderStyle {
    #[default]
    Ascii,
    Unicode,
    Double,
    Rounded,
    Heavy,
}

/// Character set for a border style
#[derive(Debug, Clone, Copy)]
pub struct StyleChars {
    // Box corners
    pub tl: char, // top-left
    pub tr: char, // top-right
    pub bl: char, // bottom-left
    pub br: char, // bottom-right
    pub h: char,  // horizontal
    pub v: char,  // vertical

    // Arrows
    pub arrow_down: char,
    pub arrow_up: char,
    pub arrow_left: char,
    pub arrow_right: char,

    // Edges
    pub edge_h: char,
    pub edge_v: char,
    pub corner_dr: char, // down-right
    pub corner_dl: char, // down-left
    pub corner_ur: char, // up-right
    pub corner_ul: char, // up-left
    pub cross: char,

    // Back-edges
    pub back_h: char,
    pub back_v: char,
}

impl BorderStyle {
    pub fn chars(&self) -> &'static StyleChars {
        match self {
            BorderStyle::Ascii => &ASCII_CHARS,
            BorderStyle::Unicode => &UNICODE_CHARS,
            BorderStyle::Double => &DOUBLE_CHARS,
            BorderStyle::Rounded => &ROUNDED_CHARS,
            BorderStyle::Heavy => &HEAVY_CHARS,
        }
    }
}

pub static ASCII_CHARS: StyleChars = StyleChars {
    tl: '+',
    tr: '+',
    bl: '+',
    br: '+',
    h: '-',
    v: '|',
    arrow_down: 'v',
    arrow_up: '^',
    arrow_left: '<',
    arrow_right: '>',
    edge_h: '-',
    edge_v: '|',
    corner_dr: '+',
    corner_dl: '+',
    corner_ur: '+',
    corner_ul: '+',
    cross: '+',
    back_h: '-',
    back_v: ':',
};

pub static UNICODE_CHARS: StyleChars = StyleChars {
    tl: '┌',
    tr: '┐',
    bl: '└',
    br: '┘',
    h: '─',
    v: '│',
    arrow_down: '▼',
    arrow_up: '▲',
    arrow_left: '◀',
    arrow_right: '▶',
    edge_h: '─',
    edge_v: '│',
    corner_dr: '┐',
    corner_dl: '┌',
    corner_ur: '┘',
    corner_ul: '└',
    cross: '┼',
    back_h: '┄',
    back_v: '┆',
};

pub static DOUBLE_CHARS: StyleChars = StyleChars {
    tl: '╔',
    tr: '╗',
    bl: '╚',
    br: '╝',
    h: '═',
    v: '║',
    arrow_down: '▼',
    arrow_up: '▲',
    arrow_left: '◀',
    arrow_right: '▶',
    edge_h: '═',
    edge_v: '║',
    corner_dr: '╗',
    corner_dl: '╔',
    corner_ur: '╝',
    corner_ul: '╚',
    cross: '╬',
    back_h: '┄',
    back_v: '┊',
};

pub static ROUNDED_CHARS: StyleChars = StyleChars {
    tl: '╭',
    tr: '╮',
    bl: '╰',
    br: '╯',
    h: '─',
    v: '│',
    arrow_down: '▼',
    arrow_up: '▲',
    arrow_left: '◀',
    arrow_right: '▶',
    edge_h: '─',
    edge_v: '│',
    corner_dr: '╮',
    corner_dl: '╭',
    corner_ur: '╯',
    corner_ul: '╰',
    cross: '┼',
    back_h: '┄',
    back_v: '┆',
};

pub static HEAVY_CHARS: StyleChars = StyleChars {
    tl: '┏',
    tr: '┓',
    bl: '┗',
    br: '┛',
    h: '━',
    v: '┃',
    arrow_down: '▼',
    arrow_up: '▲',
    arrow_left: '◀',
    arrow_right: '▶',
    edge_h: '━',
    edge_v: '┃',
    corner_dr: '┓',
    corner_dl: '┏',
    corner_ur: '┛',
    corner_ul: '┗',
    cross: '╋',
    back_h: '┅',
    back_v: '╏',
};

/// Calculate display width of a string (handles CJK, emoji, etc.)
pub fn display_width(s: &str) -> usize {
    s.width()
}

/// Truncate label to fit within max display columns
pub fn truncate_label(label: &str, max_width: usize) -> String {
    let current_width = display_width(label);
    if current_width <= max_width {
        return label.to_string();
    }

    let mut result = String::new();
    let mut width = 0;
    let ellipsis_width = 3; // "..."

    for c in label.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
        if width + cw + ellipsis_width > max_width {
            result.push_str("...");
            break;
        }
        result.push(c);
        width += cw;
    }
    result
}

/// Calculate box width from label
pub fn box_width(label: &str) -> usize {
    let label_width = display_width(label).min(MAX_LABEL_WIDTH);
    (label_width + BOX_PADDING * 2 + 2).max(BOX_MIN_WIDTH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_box_width_simple() {
        assert_eq!(box_width("A"), 7); // 1 + 4 + 2 = 7, but min is 5... actually 1+4+2=7
        assert_eq!(box_width("Gateway"), 13); // 7 + 4 + 2 = 13
    }

    #[test]
    fn test_truncate_label() {
        assert_eq!(truncate_label("Short", 10), "Short");
        assert_eq!(truncate_label("VeryLongLabel", 10), "VeryLo...");
    }

    #[test]
    fn test_display_width() {
        assert_eq!(display_width("ABC"), 3);
        assert_eq!(display_width("日本語"), 6); // CJK = 2 width each
    }
}
