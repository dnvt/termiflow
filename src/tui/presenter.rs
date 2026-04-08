//! ANSI terminal presenters.
//!
//! Two presenters are provided:
//! - `AnsiDiffPresenter` — diff-based rendering for alternate-screen TUI mode.
//! - `InlinePresenter` — diff-based redraw on the primary screen, for `--watch`
//!   mode without taking over the terminal.

use std::io::{self, Write};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    style::Print,
    terminal::{Clear, ClearType},
    QueueableCommand,
};

use super::frame::{FrameDelta, TerminalFrame};

pub trait TerminalPresenter {
    fn present(&mut self, next: &TerminalFrame) -> io::Result<()>;
}

pub struct AnsiDiffPresenter<W: Write> {
    writer: W,
    previous: Option<TerminalFrame>,
}

impl<W: Write> AnsiDiffPresenter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            previous: None,
        }
    }

    pub fn into_inner(self) -> W {
        self.writer
    }

    pub fn previous_frame(&self) -> Option<&TerminalFrame> {
        self.previous.as_ref()
    }
}

impl<W: Write> TerminalPresenter for AnsiDiffPresenter<W> {
    fn present(&mut self, next: &TerminalFrame) -> io::Result<()> {
        let delta = FrameDelta::between(self.previous.as_ref(), next);

        self.writer.queue(Hide)?;
        if delta.full_redraw {
            self.writer.queue(MoveTo(0, 0))?;
            self.writer.queue(Clear(ClearType::All))?;
        }
        for change in &delta.changes {
            self.writer.queue(MoveTo(change.x, change.y))?;
            self.writer.queue(Print(change.cell.ch))?;
        }
        self.writer.queue(Show)?;
        self.writer.flush()?;

        self.previous = Some(next.clone());
        Ok(())
    }
}

/// Inline primary-screen presenter.
///
/// This presenter keeps the preview in the primary screen buffer and diff-renders
/// subsequent frames relative to the top of the previously printed region. It
/// preserves normal scrollback while avoiding the full-string rewrite flicker of
/// the original watch-mode implementation.
pub struct InlinePresenter<W: Write> {
    writer: W,
    previous: Option<TerminalFrame>,
}

impl<W: Write> InlinePresenter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            previous: None,
        }
    }

    /// Render `content` in-place. Overwrites the previous render if any.
    pub fn render_string(&mut self, content: &str) -> io::Result<()> {
        let lines: Vec<&str> = content.lines().collect();
        let frame = TerminalFrame::from_lines(&lines);
        self.present(&frame)
    }

    pub fn into_inner(self) -> W {
        self.writer
    }

    pub fn previous_frame(&self) -> Option<&TerminalFrame> {
        self.previous.as_ref()
    }

    fn move_to_top_of_previous_region(&mut self) -> io::Result<()> {
        let previous_height = self.previous.as_ref().map_or(0, |frame| frame.height);
        if previous_height > 0 {
            write!(self.writer, "\x1b[{}A", previous_height)?;
        }
        Ok(())
    }

    fn render_full_frame(&mut self, next: &TerminalFrame) -> io::Result<()> {
        let previous_height = self.previous.as_ref().map_or(0, |frame| frame.height);

        for y in 0..next.height {
            let line = frame_line(next, y);
            write!(self.writer, "\r{}\x1b[K\n", line)?;
        }

        for _ in next.height..previous_height {
            write!(self.writer, "\r\x1b[K\n")?;
        }

        let surplus = previous_height.saturating_sub(next.height);
        if surplus > 0 {
            write!(self.writer, "\x1b[{}A", surplus)?;
        }

        Ok(())
    }

    fn render_diff(&mut self, next: &TerminalFrame, delta: &FrameDelta) -> io::Result<()> {
        let mut current_row = 0u16;
        let mut current_col = 0u16;

        for segment in diff_segments(delta) {
            move_inline_cursor(
                &mut self.writer,
                current_row,
                current_col,
                segment.y,
                segment.x,
            )?;
            write!(self.writer, "{}", segment.text)?;
            current_row = segment.y;
            current_col = segment.x + segment.text.chars().count() as u16;
        }

        write!(self.writer, "\r")?;
        let move_down = next.height.saturating_sub(current_row);
        if move_down > 0 {
            write!(self.writer, "\x1b[{}B", move_down)?;
        }

        Ok(())
    }
}

impl<W: Write> TerminalPresenter for InlinePresenter<W> {
    fn present(&mut self, next: &TerminalFrame) -> io::Result<()> {
        let delta = FrameDelta::between(self.previous.as_ref(), next);
        if !delta.full_redraw && delta.changes.is_empty() {
            return Ok(());
        }

        self.move_to_top_of_previous_region()?;

        if delta.full_redraw {
            self.render_full_frame(next)?;
        } else {
            self.render_diff(next, &delta)?;
        }

        self.writer.flush()?;
        self.previous = Some(next.clone());
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiffSegment {
    x: u16,
    y: u16,
    text: String,
}

fn frame_line(frame: &TerminalFrame, y: u16) -> String {
    let mut line = String::new();
    for x in 0..frame.width {
        line.push(frame.get(x, y).map(|cell| cell.ch).unwrap_or(' '));
    }
    line.trim_end_matches(' ').to_string()
}

fn diff_segments(delta: &FrameDelta) -> Vec<DiffSegment> {
    let mut segments = Vec::new();
    let mut iter = delta.changes.iter().peekable();

    while let Some(change) = iter.next() {
        let mut text = String::new();
        text.push(change.cell.ch);
        let x = change.x;
        let y = change.y;
        let mut next_x = change.x + 1;

        while let Some(candidate) = iter.peek() {
            if candidate.y != y || candidate.x != next_x {
                break;
            }
            text.push(candidate.cell.ch);
            next_x += 1;
            iter.next();
        }

        segments.push(DiffSegment { x, y, text });
    }

    segments
}

fn move_inline_cursor<W: Write>(
    writer: &mut W,
    from_row: u16,
    from_col: u16,
    to_row: u16,
    to_col: u16,
) -> io::Result<()> {
    if to_row > from_row {
        write!(writer, "\x1b[{}B", to_row - from_row)?;
    } else if to_row < from_row {
        write!(writer, "\x1b[{}A", from_row - to_row)?;
    }

    if to_row != from_row {
        write!(writer, "\r")?;
        if to_col > 0 {
            write!(writer, "\x1b[{}C", to_col)?;
        }
        return Ok(());
    }

    if to_col < from_col {
        write!(writer, "\r")?;
        if to_col > 0 {
            write!(writer, "\x1b[{}C", to_col)?;
        }
    } else if to_col > from_col {
        write!(writer, "\x1b[{}C", to_col - from_col)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::frame::TerminalFrame;

    #[test]
    fn presenter_updates_previous_frame() {
        let mut presenter = AnsiDiffPresenter::new(Vec::<u8>::new());
        let frame = TerminalFrame::from_lines(&["abc"]);
        presenter.present(&frame).expect("present");
        assert_eq!(presenter.previous_frame(), Some(&frame));
    }

    #[test]
    fn presenter_emits_output_bytes() {
        let mut presenter = AnsiDiffPresenter::new(Vec::<u8>::new());
        let frame = TerminalFrame::from_lines(&["abc"]);
        presenter.present(&frame).expect("present");
        let bytes = presenter.into_inner();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn inline_presenter_first_render_writes_content() {
        let mut presenter = InlinePresenter::new(Vec::<u8>::new());
        presenter
            .render_string("line one\nline two\n")
            .expect("render");
        let output = String::from_utf8(presenter.into_inner()).unwrap();
        assert!(output.contains("line one"));
        assert!(output.contains("line two"));
    }

    #[test]
    fn inline_presenter_second_render_moves_cursor_up() {
        let mut presenter = InlinePresenter::new(Vec::<u8>::new());
        presenter.render_string("aaa\nbbb").expect("first render");
        presenter.render_string("ccc\nddd").expect("second render");
        let output = String::from_utf8(presenter.into_inner()).unwrap();
        // Cursor-up escape sequence must appear before the second render
        assert!(
            output.contains("\x1b[2A"),
            "expected cursor-up in: {output:?}"
        );
    }

    #[test]
    fn inline_presenter_surplus_lines_are_cleared() {
        let mut presenter = InlinePresenter::new(Vec::<u8>::new());
        presenter.render_string("a\nb\nc\nd").expect("tall render");
        presenter.render_string("x").expect("short render");
        let output = String::from_utf8(presenter.into_inner()).unwrap();
        // Must have emitted at least 3 clear-to-EOL sequences for surplus lines
        let clear_count = output.matches("\x1b[K").count();
        assert!(clear_count >= 3, "expected ≥3 clears, got {clear_count}");
    }

    #[test]
    fn inline_presenter_skips_identical_frame_redraw() {
        let mut presenter = InlinePresenter::new(Vec::<u8>::new());
        presenter.render_string("same").expect("first render");
        let first_len = presenter.writer.len();
        presenter.render_string("same").expect("second render");
        assert_eq!(presenter.writer.len(), first_len);
    }

    #[test]
    fn inline_presenter_only_writes_changed_segments() {
        let mut presenter = InlinePresenter::new(Vec::<u8>::new());
        presenter.render_string("abc\ndef").expect("first render");
        let first_len = presenter.writer.len();
        presenter.render_string("axc\ndef").expect("second render");
        let second_pass = String::from_utf8(presenter.writer[first_len..].to_vec()).unwrap();

        assert!(second_pass.contains('x'));
        assert!(!second_pass.contains("def"));
    }

    #[test]
    fn inline_presenter_updates_previous_frame() {
        let mut presenter = InlinePresenter::new(Vec::<u8>::new());
        let frame = TerminalFrame::from_lines(&["abc"]);
        presenter.present(&frame).expect("present");
        assert_eq!(presenter.previous_frame(), Some(&frame));
    }

    #[test]
    fn diff_segments_group_contiguous_changes() {
        let delta = FrameDelta {
            full_redraw: false,
            changes: vec![
                crate::tui::frame::FrameChange {
                    x: 1,
                    y: 2,
                    cell: crate::tui::frame::FrameCell { ch: 'a' },
                },
                crate::tui::frame::FrameChange {
                    x: 2,
                    y: 2,
                    cell: crate::tui::frame::FrameCell { ch: 'b' },
                },
                crate::tui::frame::FrameChange {
                    x: 4,
                    y: 2,
                    cell: crate::tui::frame::FrameCell { ch: 'c' },
                },
            ],
        };

        let segments = diff_segments(&delta);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].x, 1);
        assert_eq!(segments[0].y, 2);
        assert_eq!(segments[0].text, "ab");
        assert_eq!(segments[1].x, 4);
        assert_eq!(segments[1].text, "c");
    }
}
