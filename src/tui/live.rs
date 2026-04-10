//! Helpers for low-flicker live preview frames.

use super::frame::{line_display_width, write_line_slice, TerminalFrame};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Viewport {
    pub offset_x: u16,
    pub offset_y: u16,
}

pub fn initial_viewport(content: &str, size: (u16, u16)) -> Viewport {
    let (width, _) = size;
    let content_width = content.lines().map(line_display_width).max().unwrap_or(0);
    let offset_x = content_width.saturating_sub(width) / 2;

    let mut viewport = Viewport {
        offset_x,
        offset_y: 0,
    };
    clamp_viewport(&mut viewport, content, size);
    viewport
}

pub fn clamp_viewport(viewport: &mut Viewport, content: &str, size: (u16, u16)) {
    let (width, height) = size;
    let content_height = content.lines().count() as u16;
    let content_width = content.lines().map(line_display_width).max().unwrap_or(0);
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
        write_line_slice(&mut frame, row as u16, line, viewport.offset_x, width);
    }

    let status_y = height.saturating_sub(1);
    write_line_slice(&mut frame, status_y, status, 0, width);

    frame
}

pub fn build_inline_frame(content: &str, status: &str) -> TerminalFrame {
    let content_lines: Vec<&str> = content.lines().collect();
    let content_height = content_lines.len() as u16;
    let content_width = content_lines
        .iter()
        .map(|line| line_display_width(line))
        .max()
        .unwrap_or(0);
    let status_width = line_display_width(status);
    let width = content_width.max(status_width);
    let height = content_height.saturating_add(1);

    let mut frame = TerminalFrame::new(width, height);
    for (row, line) in content_lines.iter().enumerate() {
        write_line_slice(&mut frame, row as u16, line, 0, width);
    }

    let status_y = height.saturating_sub(1);
    write_line_slice(&mut frame, status_y, status, 0, width);

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

    #[test]
    fn clamp_viewport_uses_display_columns() {
        let mut viewport = Viewport {
            offset_x: 20,
            offset_y: 0,
        };

        clamp_viewport(&mut viewport, "界ab", (2, 2));

        assert_eq!(viewport.offset_x, 2);
    }

    #[test]
    fn build_inline_frame_tracks_wide_glyph_width() {
        let frame = build_inline_frame("界a", "ok");

        assert_eq!(frame.width, 3);
        assert_eq!(frame.get(0, 0).map(|cell| cell.ch), Some('界'));
        assert_eq!(frame.get(2, 0).map(|cell| cell.ch), Some('a'));
    }

    #[test]
    fn initial_viewport_centers_wide_content_horizontally() {
        let viewport = initial_viewport("0123456789", (4, 2));

        assert_eq!(
            viewport,
            Viewport {
                offset_x: 3,
                offset_y: 0,
            }
        );
    }

    #[test]
    fn initial_viewport_keeps_narrow_content_left_aligned() {
        let viewport = initial_viewport("abc", (10, 2));

        assert_eq!(viewport, Viewport::default());
    }
}
