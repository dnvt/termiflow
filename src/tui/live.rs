//! Helpers for low-flicker live preview frames.

use super::frame::TerminalFrame;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Viewport {
    pub offset_x: u16,
    pub offset_y: u16,
}

pub fn clamp_viewport(viewport: &mut Viewport, content: &str, size: (u16, u16)) {
    let (width, height) = size;
    let content_height = content.lines().count() as u16;
    let content_width = content
        .lines()
        .map(|line| line.chars().count() as u16)
        .max()
        .unwrap_or(0);
    let viewport_height = height.saturating_sub(1);

    viewport.offset_x = viewport.offset_x.min(content_width.saturating_sub(width));
    viewport.offset_y = viewport
        .offset_y
        .min(content_height.saturating_sub(viewport_height));
}

pub fn build_preview_frame(
    content: &str,
    status: &str,
    size: (u16, u16),
    viewport: Viewport,
) -> TerminalFrame {
    let (width, height) = size;
    let mut frame = TerminalFrame::new(width, height);
    if width == 0 || height == 0 {
        return frame;
    }

    let viewport_height = height.saturating_sub(1);
    for (row, line) in content
        .lines()
        .skip(usize::from(viewport.offset_y))
        .take(usize::from(viewport_height))
        .enumerate()
    {
        for (col, ch) in line
            .chars()
            .skip(usize::from(viewport.offset_x))
            .take(usize::from(width))
            .enumerate()
        {
            frame.set(col as u16, row as u16, ch);
        }
    }

    let status_y = height.saturating_sub(1);
    for (col, ch) in status.chars().take(usize::from(width)).enumerate() {
        frame.set(col as u16, status_y, ch);
    }

    frame
}

pub fn build_inline_frame(content: &str, status: &str) -> TerminalFrame {
    let content_lines: Vec<&str> = content.lines().collect();
    let content_height = content_lines.len() as u16;
    let content_width = content_lines
        .iter()
        .map(|line| line.chars().count() as u16)
        .max()
        .unwrap_or(0);
    let status_width = status.chars().count() as u16;
    let width = content_width.max(status_width);
    let height = content_height.saturating_add(1);

    let mut frame = TerminalFrame::new(width, height);
    for (row, line) in content_lines.iter().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            frame.set(col as u16, row as u16, ch);
        }
    }

    let status_y = height.saturating_sub(1);
    for (col, ch) in status.chars().enumerate() {
        frame.set(col as u16, status_y, ch);
    }

    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_preview_frame_applies_viewport_crop() {
        let frame = build_preview_frame(
            "012345\nabcdef\nXYZ",
            "status",
            (4, 3),
            Viewport {
                offset_x: 1,
                offset_y: 1,
            },
        );

        let first_row: String = (0..4)
            .map(|x| frame.get(x, 0).map(|cell| cell.ch).unwrap_or(' '))
            .collect();
        assert_eq!(first_row, "bcde");
    }

    #[test]
    fn clamp_viewport_limits_offsets_to_content_bounds() {
        let mut viewport = Viewport {
            offset_x: 20,
            offset_y: 20,
        };
        clamp_viewport(&mut viewport, "abc\n123", (4, 2));
        assert_eq!(viewport.offset_x, 0);
        assert_eq!(viewport.offset_y, 1);
    }

    #[test]
    fn build_inline_frame_appends_status_row() {
        let frame = build_inline_frame("abc\ndef", "status");

        assert_eq!(frame.width, 6);
        assert_eq!(frame.height, 3);

        let status_row: String = (0..6)
            .map(|x| frame.get(x, 2).map(|cell| cell.ch).unwrap_or(' '))
            .collect();
        assert_eq!(status_row, "status");
    }
}
