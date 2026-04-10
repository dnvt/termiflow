//! Terminal frame model and diffing.

use crate::display_profile::{display_width, graphemes};

pub(crate) const CONTINUATION_CELL: char = '\0';

/// Single rendered terminal cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameCell {
    pub ch: char,
    text: String,
}

impl FrameCell {
    pub fn from_char(ch: char) -> Self {
        if is_continuation_cell(ch) {
            return Self::continuation();
        }

        Self {
            ch,
            text: ch.to_string(),
        }
    }

    pub fn from_text(text: &str) -> Self {
        let ch = text.chars().next().unwrap_or(' ');
        Self {
            ch,
            text: text.to_string(),
        }
    }

    pub fn continuation() -> Self {
        Self {
            ch: CONTINUATION_CELL,
            text: String::new(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

impl Default for FrameCell {
    fn default() -> Self {
        Self::from_char(' ')
    }
}

/// Full retained terminal frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TerminalFrame {
    pub width: u16,
    pub height: u16,
    pub cells: Vec<FrameCell>,
}

/// Single cell change between two frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameChange {
    pub x: u16,
    pub y: u16,
    pub cell: FrameCell,
}

/// Diff between frames.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FrameDelta {
    pub full_redraw: bool,
    pub changes: Vec<FrameChange>,
}

impl TerminalFrame {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            cells: vec![FrameCell::default(); usize::from(width) * usize::from(height)],
        }
    }

    pub fn from_lines(lines: &[&str]) -> Self {
        let height = lines.len() as u16;
        let width = lines
            .iter()
            .map(|line| line_display_width(line))
            .max()
            .unwrap_or(0);
        let mut frame = Self::new(width, height);
        for (y, line) in lines.iter().enumerate() {
            write_line_slice(&mut frame, y as u16, line, 0, width);
        }
        frame
    }

    pub fn get(&self, x: u16, y: u16) -> Option<&FrameCell> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.cells.get(self.index(x, y))
    }

    pub fn set(&mut self, x: u16, y: u16, ch: char) {
        if x < self.width && y < self.height {
            let idx = self.index(x, y);
            self.cells[idx] = FrameCell::from_char(ch);
        }
    }

    pub fn set_text(&mut self, x: u16, y: u16, text: &str) {
        if x < self.width && y < self.height {
            let idx = self.index(x, y);
            self.cells[idx] = FrameCell::from_text(text);
        }
    }

    fn append_to_cell(&mut self, x: u16, y: u16, text: &str) {
        if x < self.width && y < self.height {
            let idx = self.index(x, y);
            if !is_continuation_cell(self.cells[idx].ch) {
                self.cells[idx].text.push_str(text);
            }
        }
    }

    fn index(&self, x: u16, y: u16) -> usize {
        usize::from(y) * usize::from(self.width) + usize::from(x)
    }
}

pub(crate) fn is_continuation_cell(ch: char) -> bool {
    ch == CONTINUATION_CELL
}

pub(crate) fn line_display_width(line: &str) -> u16 {
    display_width(line).min(usize::from(u16::MAX)) as u16
}

pub(crate) fn write_line_slice(
    frame: &mut TerminalFrame,
    y: u16,
    line: &str,
    offset_x: u16,
    max_width: u16,
) {
    if y >= frame.height || max_width == 0 {
        return;
    }

    let viewport_start = u32::from(offset_x);
    let viewport_end = viewport_start + u32::from(max_width);
    let mut source_x = 0u32;
    let mut last_visible_cell = None;

    for grapheme in graphemes(line) {
        let width = display_width(grapheme).min(usize::from(u16::MAX)) as u32;
        if width == 0 {
            if let Some((x, y)) = last_visible_cell {
                frame.append_to_cell(x, y, grapheme);
            }
            continue;
        }

        let start = source_x;
        let end = start + width;
        source_x = end;

        if end <= viewport_start {
            continue;
        }
        if start >= viewport_end {
            break;
        }

        // Skip partially visible wide glyphs rather than drawing half-cells.
        if start < viewport_start || end > viewport_end {
            continue;
        }

        let target_x = (start - viewport_start) as u16;
        frame.set_text(target_x, y, grapheme);
        last_visible_cell = Some((target_x, y));
        for extra in 1..width {
            frame.set(target_x + extra as u16, y, CONTINUATION_CELL);
        }
    }
}

impl FrameDelta {
    pub fn between(previous: Option<&TerminalFrame>, next: &TerminalFrame) -> Self {
        let Some(previous) = previous else {
            return Self::full(next);
        };

        if previous.width != next.width || previous.height != next.height {
            return Self::full(next);
        }

        let mut changes = Vec::new();
        for y in 0..next.height {
            for x in 0..next.width {
                let prev = previous.get(x, y).cloned().unwrap_or_default();
                let next_cell = next.get(x, y).cloned().unwrap_or_default();
                if prev != next_cell {
                    changes.push(FrameChange {
                        x,
                        y,
                        cell: next_cell,
                    });
                }
            }
        }

        Self {
            full_redraw: false,
            changes,
        }
    }

    pub fn full(next: &TerminalFrame) -> Self {
        let mut changes = Vec::new();
        for y in 0..next.height {
            for x in 0..next.width {
                if let Some(cell) = next.get(x, y) {
                    changes.push(FrameChange {
                        x,
                        y,
                        cell: cell.clone(),
                    });
                }
            }
        }
        Self {
            full_redraw: true,
            changes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_is_empty_for_identical_frames() {
        let frame = TerminalFrame::from_lines(&["abc", "def"]);
        let delta = FrameDelta::between(Some(&frame), &frame);
        assert!(!delta.full_redraw);
        assert!(delta.changes.is_empty());
    }

    #[test]
    fn diff_detects_single_cell_change() {
        let previous = TerminalFrame::from_lines(&["abc"]);
        let next = TerminalFrame::from_lines(&["axc"]);
        let delta = FrameDelta::between(Some(&previous), &next);
        assert_eq!(delta.changes.len(), 1);
        assert_eq!(delta.changes[0].x, 1);
        assert_eq!(delta.changes[0].y, 0);
        assert_eq!(delta.changes[0].cell.ch, 'x');
    }

    #[test]
    fn from_lines_tracks_wide_glyph_columns() {
        let frame = TerminalFrame::from_lines(&["界a"]);

        assert_eq!(frame.width, 3);
        assert_eq!(frame.get(0, 0).map(|cell| cell.ch), Some('界'));
        assert_eq!(frame.get(1, 0).map(|cell| cell.ch), Some(CONTINUATION_CELL));
        assert_eq!(frame.get(2, 0).map(|cell| cell.ch), Some('a'));
    }

    #[test]
    fn from_lines_preserves_combining_graphemes() {
        let frame = TerminalFrame::from_lines(&["e\u{301}x"]);

        assert_eq!(frame.width, 2);
        assert_eq!(frame.get(0, 0).map(|cell| cell.text()), Some("e\u{301}"));
        assert_eq!(frame.get(1, 0).map(|cell| cell.text()), Some("x"));
    }
}
