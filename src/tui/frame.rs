//! Terminal frame model and diffing.

/// Single rendered terminal cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameCell {
    pub ch: char,
}

impl Default for FrameCell {
    fn default() -> Self {
        Self { ch: ' ' }
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
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0) as u16;
        let mut frame = Self::new(width, height);
        for (y, line) in lines.iter().enumerate() {
            for (x, ch) in line.chars().enumerate() {
                frame.set(x as u16, y as u16, ch);
            }
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
            self.cells[idx] = FrameCell { ch };
        }
    }

    fn index(&self, x: u16, y: u16) -> usize {
        usize::from(y) * usize::from(self.width) + usize::from(x)
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
                let prev = previous.get(x, y).map(|cell| cell.ch).unwrap_or(' ');
                let next_ch = next.get(x, y).map(|cell| cell.ch).unwrap_or(' ');
                if prev != next_ch {
                    changes.push(FrameChange {
                        x,
                        y,
                        cell: FrameCell { ch: next_ch },
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
}
