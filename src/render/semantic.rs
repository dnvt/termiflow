//! Semantic render frame scaffolding.
//!
//! This module introduces a richer framebuffer representation that can evolve
//! into the canonical source of truth for render criticism and repair. The
//! current implementation is intentionally conservative: it snapshots the
//! existing `Canvas` without changing visible output behavior.

use super::canvas::Canvas;

/// High-level owner category for a rendered cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellOwnerKind {
    #[default]
    Unknown,
    Empty,
    NodeBorder,
    NodeFill,
    NodeLabel,
    EdgeSegment,
    ArrowHead,
    Junction,
    SubgraphBorder,
    SubgraphTitle,
    CycleEdge,
    PortalOpening,
    EdgeLabel,
}

/// Semantic role for a rendered cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellRole {
    #[default]
    Unknown,
    Empty,
    Text,
    Horizontal,
    Vertical,
    Junction,
    ArrowTip,
    Corner,
    Fill,
    Border,
    Portal,
}

/// Metadata captured for a single rendered cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellMeta {
    pub ch: char,
    pub owner_kind: CellOwnerKind,
    pub owner_id: Option<String>,
    pub role: CellRole,
    pub z_index: u8,
}

impl Default for CellMeta {
    fn default() -> Self {
        Self {
            ch: ' ',
            owner_kind: CellOwnerKind::Empty,
            owner_id: None,
            role: CellRole::Empty,
            z_index: 0,
        }
    }
}

/// Snapshot of a rendered frame with room for semantic provenance.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SemanticFrame {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<CellMeta>,
}

impl SemanticFrame {
    /// Create a semantic frame by snapshotting the current `Canvas`.
    ///
    /// This is a Phase 6.0 foundation only:
    /// - ownership is not yet propagated from the renderer
    /// - roles are inferred from final glyphs
    /// - the resulting frame is suitable for debug reporting and future critic work
    pub fn from_canvas(canvas: &Canvas) -> Self {
        let mut cells = Vec::with_capacity(canvas.width.saturating_mul(canvas.height));

        for y in 0..canvas.height {
            for x in 0..canvas.width {
                cells.push(canvas.get_meta(x, y).cloned().unwrap_or_default());
            }
        }

        Self {
            width: canvas.width,
            height: canvas.height,
            cells,
        }
    }

    pub fn get(&self, x: usize, y: usize) -> Option<&CellMeta> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.cells.get(y * self.width + x)
    }

    pub fn crop_and_pad(&self, crop: bool, pad: usize) -> Self {
        if self.width == 0 || self.height == 0 {
            return Self::default();
        }

        let (min_x, max_x, min_y, max_y) = if crop {
            let mut found = false;
            let mut min_x = self.width;
            let mut max_x = 0usize;
            let mut min_y = self.height;
            let mut max_y = 0usize;

            for y in 0..self.height {
                for x in 0..self.width {
                    let Some(cell) = self.get(x, y) else {
                        continue;
                    };
                    if cell.ch == ' ' {
                        continue;
                    }
                    found = true;
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                }
            }

            if !found {
                return Self::default();
            }

            (min_x, max_x, min_y, max_y)
        } else {
            (
                0,
                self.width.saturating_sub(1),
                0,
                self.height.saturating_sub(1),
            )
        };

        let source_width = max_x.saturating_sub(min_x).saturating_add(1);
        let source_height = max_y.saturating_sub(min_y).saturating_add(1);
        let target_width = source_width.saturating_add(pad);
        let target_height = source_height.saturating_add(pad.saturating_mul(2));
        let mut cells = vec![CellMeta::default(); target_width.saturating_mul(target_height)];

        for source_y in min_y..=max_y {
            for source_x in min_x..=max_x {
                let Some(cell) = self.get(source_x, source_y).cloned() else {
                    continue;
                };
                let target_x = source_x.saturating_sub(min_x).saturating_add(pad);
                let target_y = source_y.saturating_sub(min_y).saturating_add(pad);
                let idx = target_y * target_width + target_x;
                if let Some(slot) = cells.get_mut(idx) {
                    *slot = cell;
                }
            }
        }

        Self {
            width: target_width,
            height: target_height,
            cells,
        }
    }

    pub fn non_space_cell_count(&self) -> usize {
        self.cells.iter().filter(|cell| cell.ch != ' ').count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_frame_snapshots_canvas_cells() {
        let mut canvas = Canvas::new(3, 2);
        canvas.set(0, 0, 'A');
        canvas.set(1, 0, '─');
        canvas.set(2, 0, '→');

        let frame = SemanticFrame::from_canvas(&canvas);

        assert_eq!(frame.width, 3);
        assert_eq!(frame.height, 2);
        assert_eq!(frame.non_space_cell_count(), 3);
        assert_eq!(frame.get(0, 0).map(|cell| cell.role), Some(CellRole::Text));
        assert_eq!(
            frame.get(1, 0).map(|cell| cell.role),
            Some(CellRole::Horizontal)
        );
        assert_eq!(
            frame.get(2, 0).map(|cell| cell.role),
            Some(CellRole::ArrowTip)
        );
    }
}
