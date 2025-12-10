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

// Edge routing constants
pub const STEM_LENGTH_VERTICAL: usize = 1;   // Stem length for TD/BT layouts
pub const STEM_LENGTH_HORIZONTAL: usize = 3; // Stem length for LR/RL layouts
pub const EDGE_JUNCTION_HEIGHT: usize = 1;   // Junction row spacing
pub const EDGE_DROP_HEIGHT: usize = 1;       // Drop spacing for multi-target
pub const MAX_LABEL_WIDTH: usize = 20;

pub const MAX_CANVAS_WIDTH: usize = 500;
pub const MAX_CANVAS_HEIGHT: usize = 200;

/// Cycle edge gutter size (right margin for TD/BT, bottom for LR/RL)
pub const CYCLE_GUTTER: usize = 4;

/// Border style variants
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum BaseStyle {
    Ascii,
    #[default]
    Unicode,
    Double,
    Rounded,
    Heavy,
    Dots,   // • for corners
    Plus,   // + for corners
    Stars,  // * for corners
    Blocks, // █ for lines
}

/// Component-specific style configuration
///
/// Each component can be styled independently using any BorderStyle:
/// - `corner` - Box corners (┌┐└┘ for unicode, ╭╮╰╯ for rounded, ╔╗╚╝ for double, • for dots, * for stars, etc.)
/// - `border` - Box borders/lines (─│ for unicode, ═║ for double, ━┃ for heavy, etc.)
/// - `arrow` - Arrow heads (▼◀▶ for unicode, v<> for ascii)
/// - `edge` - Connection lines between boxes
/// - `junction` - T-junctions where edges meet (┬┴├┤ for unicode, ╦╩╠╣ for double, etc.)
/// - `back` - Back edges for cycles (dotted/dashed lines)
/// - `subgraph` - Subgraph container borders (defaults to ascii for visual distinction)
#[derive(Debug, Clone, Default)]
pub struct CompositeStyle {
    pub corner: Option<BaseStyle>,   // Box corners
    pub border: Option<BaseStyle>,   // Box borders (h/v lines)
    pub arrow: Option<BaseStyle>,    // Arrow heads
    pub edge: Option<BaseStyle>,     // Edge/connection lines
    pub junction: Option<BaseStyle>, // Junction characters
    pub back: Option<BaseStyle>,     // Back edges for cycles
    pub subgraph: Option<BaseStyle>, // Subgraph container borders
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

    // Arrows for all four directions
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

    // Junctions (T-shapes for edge branching/merging)
    pub junction_down: char,  // ┬ - stem above, branches below
    pub junction_up: char,    // ┴ - stem below, branches above
    pub junction_right: char, // ├ - stem left, branches right
    pub junction_left: char,  // ┤ - stem right, branches left

    // Back-edges
    pub back_h: char,
    pub back_v: char,
}

impl BaseStyle {
    pub fn chars(&self) -> &'static StyleChars {
        match self {
            BaseStyle::Ascii => &ASCII_CHARS,
            BaseStyle::Unicode => &UNICODE_CHARS,
            BaseStyle::Double => &DOUBLE_CHARS,
            BaseStyle::Rounded => &ROUNDED_CHARS,
            BaseStyle::Heavy => &HEAVY_CHARS,
            BaseStyle::Dots => &DOTS_CHARS,
            BaseStyle::Plus => &PLUS_CHARS,
            BaseStyle::Stars => &STARS_CHARS,
            BaseStyle::Blocks => &BLOCKS_CHARS,
        }
    }

    /// Parse a string into a BorderStyle (case-insensitive), returning None if invalid
    pub fn parse_name(s: &str) -> Option<Self> {
        s.parse().ok()
    }
}

impl std::str::FromStr for BaseStyle {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ascii" => Ok(BaseStyle::Ascii),
            "unicode" => Ok(BaseStyle::Unicode),
            "double" => Ok(BaseStyle::Double),
            "rounded" => Ok(BaseStyle::Rounded),
            "heavy" => Ok(BaseStyle::Heavy),
            "dots" | "dot" => Ok(BaseStyle::Dots),
            "plus" => Ok(BaseStyle::Plus),
            "stars" | "star" => Ok(BaseStyle::Stars),
            "blocks" | "block" => Ok(BaseStyle::Blocks),
            _ => Err(()),
        }
    }
}

impl CompositeStyle {
    /// Create a CompositeStyle with all components set to the given base style
    pub fn from_base(style: BaseStyle) -> Self {
        Self {
            corner: Some(style),
            border: Some(style),
            arrow: Some(style),
            edge: Some(style),
            junction: Some(style),
            back: Some(style),
            subgraph: Some(style),
        }
    }

    /// Parse a composite style string like "box:rounded,arrow:heavy,line:double"
    pub fn parse(s: &str) -> Self {
        let mut style = CompositeStyle::default();

        // Handle simple style (backward compatibility)
        if !s.contains(':') {
            if let Some(border_style) = BaseStyle::parse_name(s) {
                // Apply to all components for backward compatibility
                style.corner = Some(border_style);
                style.border = Some(border_style);
                style.arrow = Some(border_style);
                style.edge = Some(border_style);
                style.junction = Some(border_style);
                style.back = Some(border_style);
                style.subgraph = Some(border_style);
            } else if !s.is_empty() {
                // Invalid style name - warn and use default
                eprintln!(
                    "termiflow: warning: Unknown style '{}', using default (unicode)",
                    s
                );
            }
            return style;
        }

        // Parse component-specific styles
        for part in s.split(',') {
            let part = part.trim();
            if let Some((component, style_name)) = part.split_once(':') {
                let border_style = BaseStyle::parse_name(style_name.trim());
                match component.trim() {
                    "box" => {
                        // Legacy: "box" applies to both corners and borders
                        style.corner = border_style;
                        style.border = border_style;
                    }
                    "corner" => style.corner = border_style,
                    "border" => style.border = border_style,
                    "arrow" => style.arrow = border_style,
                    "edge" => style.edge = border_style,
                    "junction" => style.junction = border_style,
                    "back" => style.back = border_style,
                    "subgraph" => style.subgraph = border_style,
                    // Legacy aliases
                    "line" => style.edge = border_style, // "line" -> "edge"
                    "box_corner" => style.corner = border_style,
                    "box_line" | "box_border" => style.border = border_style,
                    "back_edge" => style.back = border_style,
                    unknown => {
                        eprintln!(
                            "termiflow: warning: Unknown style component '{}', ignoring",
                            unknown
                        );
                    }
                }
                // Also warn if the style name within the component is invalid
                if border_style.is_none() {
                    eprintln!(
                        "termiflow: warning: Unknown style name '{}' for component '{}', using default",
                        style_name.trim(),
                        component.trim()
                    );
                }
            }
        }

        style
    }

    /// Create a mixed StyleChars from component styles with a fallback
    pub fn to_style_chars(&self, fallback: BaseStyle) -> StyleChars {
        let corner_chars = self.corner.unwrap_or(fallback).chars();
        let border_chars = self.border.unwrap_or(fallback).chars();
        let arrow_chars = self.arrow.unwrap_or(fallback).chars();
        let edge_chars = self.edge.unwrap_or(fallback).chars();
        let junction_chars = self.junction.unwrap_or(fallback).chars();
        let back_chars = self.back.unwrap_or(fallback).chars();

        StyleChars {
            // Box corners (from corner style)
            tl: corner_chars.tl,
            tr: corner_chars.tr,
            bl: corner_chars.bl,
            br: corner_chars.br,

            // Box borders (from border style)
            h: border_chars.h,
            v: border_chars.v,

            // Arrow components
            arrow_down: arrow_chars.arrow_down,
            arrow_up: arrow_chars.arrow_up,
            arrow_left: arrow_chars.arrow_left,
            arrow_right: arrow_chars.arrow_right,

            // Edge components (connection lines)
            edge_h: edge_chars.edge_h,
            edge_v: edge_chars.edge_v,
            corner_dr: edge_chars.corner_dr,
            corner_dl: edge_chars.corner_dl,
            corner_ur: edge_chars.corner_ur,
            corner_ul: edge_chars.corner_ul,
            cross: edge_chars.cross,

            // Junction components
            junction_down: junction_chars.junction_down,
            junction_up: junction_chars.junction_up,
            junction_right: junction_chars.junction_right,
            junction_left: junction_chars.junction_left,

            // Back-edge components
            back_h: back_chars.back_h,
            back_v: back_chars.back_v,
        }
    }

    /// Get StyleChars for subgraph borders.
    ///
    /// Subgraphs default to ASCII style for visual distinction from node boxes.
    /// This can be overridden with `--style="subgraph:unicode"` etc.
    pub fn to_subgraph_chars(&self) -> &'static StyleChars {
        // Default to ASCII for subgraphs (visual distinction from nodes)
        self.subgraph.unwrap_or(BaseStyle::Ascii).chars()
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
    junction_down: '+',
    junction_up: '+',
    junction_right: '+',
    junction_left: '+',
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
    arrow_down: '↓',
    arrow_up: '↑',
    arrow_left: '←',
    arrow_right: '→',
    edge_h: '─',
    edge_v: '│',
    corner_dr: '┐',
    corner_dl: '┌',
    corner_ur: '┘',
    corner_ul: '└',
    cross: '┼',
    junction_down: '┬',
    junction_up: '┴',
    junction_right: '├',
    junction_left: '┤',
    back_h: '─',
    back_v: '│',
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
    junction_down: '╦',
    junction_up: '╩',
    junction_right: '╠',
    junction_left: '╣',
    back_h: '═',
    back_v: '║',
};

pub static ROUNDED_CHARS: StyleChars = StyleChars {
    tl: '╭',
    tr: '╮',
    bl: '╰',
    br: '╯',
    h: '─',
    v: '│',
    arrow_down: '↓',
    arrow_up: '↑',
    arrow_left: '←',
    arrow_right: '→',
    edge_h: '─',
    edge_v: '│',
    corner_dr: '╮',
    corner_dl: '╭',
    corner_ur: '╯',
    corner_ul: '╰',
    cross: '┼',
    junction_down: '┬',
    junction_up: '┴',
    junction_right: '├',
    junction_left: '┤',
    back_h: '─',
    back_v: '│',
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
    junction_down: '┳',
    junction_up: '┻',
    junction_right: '┣',
    junction_left: '┫',
    back_h: '━',
    back_v: '┃',
};

pub static DOTS_CHARS: StyleChars = StyleChars {
    tl: '•',
    tr: '•',
    bl: '•',
    br: '•',
    h: '─',
    v: '│',
    arrow_down: '↓',
    arrow_up: '↑',
    arrow_left: '←',
    arrow_right: '→',
    edge_h: '─',
    edge_v: '│',
    corner_dr: '┐',
    corner_dl: '┌',
    corner_ur: '┘',
    corner_ul: '└',
    cross: '┼',
    junction_down: '┬',
    junction_up: '┴',
    junction_right: '├',
    junction_left: '┤',
    back_h: '─',
    back_v: '│',
};

pub static PLUS_CHARS: StyleChars = StyleChars {
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
    junction_down: '+',
    junction_up: '+',
    junction_right: '+',
    junction_left: '+',
    back_h: '-',
    back_v: ':',
};

pub static STARS_CHARS: StyleChars = StyleChars {
    tl: '*',
    tr: '*',
    bl: '*',
    br: '*',
    h: '─',
    v: '│',
    arrow_down: '↓',
    arrow_up: '↑',
    arrow_left: '←',
    arrow_right: '→',
    edge_h: '─',
    edge_v: '│',
    corner_dr: '┐',
    corner_dl: '┌',
    corner_ur: '┘',
    corner_ul: '└',
    cross: '┼',
    junction_down: '┬',
    junction_up: '┴',
    junction_right: '├',
    junction_left: '┤',
    back_h: '─',
    back_v: '│',
};

pub static BLOCKS_CHARS: StyleChars = StyleChars {
    tl: '█',
    tr: '█',
    bl: '█',
    br: '█',
    h: '█',
    v: '█',
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
    junction_down: '┬',
    junction_up: '┴',
    junction_right: '├',
    junction_left: '┤',
    back_h: '█',
    back_v: '█',
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
        // max_width=10, ellipsis=3, so 7 chars fit + "..." = "VeryLon..."
        assert_eq!(truncate_label("VeryLongLabel", 10), "VeryLon...");
    }

    #[test]
    fn test_display_width() {
        assert_eq!(display_width("ABC"), 3);
        assert_eq!(display_width("日本語"), 6); // CJK = 2 width each
    }

    #[test]
    fn test_composite_style_parse_simple() {
        let style = CompositeStyle::parse("unicode");
        assert_eq!(style.corner, Some(BaseStyle::Unicode));
        assert_eq!(style.border, Some(BaseStyle::Unicode));
        assert_eq!(style.arrow, Some(BaseStyle::Unicode));
        assert_eq!(style.edge, Some(BaseStyle::Unicode));
        assert_eq!(style.junction, Some(BaseStyle::Unicode));
        assert_eq!(style.back, Some(BaseStyle::Unicode));
    }

    #[test]
    fn test_composite_style_parse_complex() {
        let style = CompositeStyle::parse("corner:rounded,border:heavy,arrow:unicode,edge:double");
        assert_eq!(style.corner, Some(BaseStyle::Rounded));
        assert_eq!(style.border, Some(BaseStyle::Heavy));
        assert_eq!(style.arrow, Some(BaseStyle::Unicode));
        assert_eq!(style.edge, Some(BaseStyle::Double));
        assert_eq!(style.junction, None);
        assert_eq!(style.back, None);
    }

    #[test]
    fn test_composite_style_parse_all_components() {
        let style = CompositeStyle::parse(
            "corner:dots,border:heavy,arrow:unicode,edge:double,junction:heavy,back:rounded",
        );
        assert_eq!(style.corner, Some(BaseStyle::Dots));
        assert_eq!(style.border, Some(BaseStyle::Heavy));
        assert_eq!(style.arrow, Some(BaseStyle::Unicode));
        assert_eq!(style.edge, Some(BaseStyle::Double));
        assert_eq!(style.junction, Some(BaseStyle::Heavy));
        assert_eq!(style.back, Some(BaseStyle::Rounded));
    }

    #[test]
    fn test_composite_style_to_style_chars() {
        let mut composite = CompositeStyle::default();
        composite.corner = Some(BaseStyle::Dots);
        composite.border = Some(BaseStyle::Heavy);
        composite.arrow = Some(BaseStyle::Heavy);

        let chars = composite.to_style_chars(BaseStyle::Unicode);

        // Box corners should be dots
        assert_eq!(chars.tl, '•');
        assert_eq!(chars.tr, '•');

        // Box lines should be heavy
        assert_eq!(chars.h, '━');
        assert_eq!(chars.v, '┃');

        // Arrows should be heavy (uses heavy set → filled down arrow)
        assert_eq!(chars.arrow_down, '▼');

        // Lines should fall back to unicode
        assert_eq!(chars.edge_h, '─');
    }

    #[test]
    fn test_new_styles() {
        // Test dots style
        assert_eq!(BaseStyle::parse_name("dots"), Some(BaseStyle::Dots));
        assert_eq!(BaseStyle::parse_name("dot"), Some(BaseStyle::Dots));

        // Test plus style
        assert_eq!(BaseStyle::parse_name("plus"), Some(BaseStyle::Plus));

        // Test stars style
        assert_eq!(BaseStyle::parse_name("stars"), Some(BaseStyle::Stars));
        assert_eq!(BaseStyle::parse_name("star"), Some(BaseStyle::Stars));

        // Test blocks style
        assert_eq!(BaseStyle::parse_name("blocks"), Some(BaseStyle::Blocks));
        assert_eq!(BaseStyle::parse_name("block"), Some(BaseStyle::Blocks));
    }

    #[test]
    fn test_legacy_compatibility() {
        // Test that legacy names still work
        let style = CompositeStyle::parse("box:rounded");
        assert_eq!(style.corner, Some(BaseStyle::Rounded));
        assert_eq!(style.border, Some(BaseStyle::Rounded));

        let style =
            CompositeStyle::parse("box_corner:dots,box_line:heavy,line:double,back_edge:ascii");
        assert_eq!(style.corner, Some(BaseStyle::Dots));
        assert_eq!(style.border, Some(BaseStyle::Heavy));
        assert_eq!(style.edge, Some(BaseStyle::Double));
        assert_eq!(style.back, Some(BaseStyle::Ascii));
    }
}
