//! TUI frame model and presenters.
//!
//! These modules provide the retained-frame, diff presenter, and viewport
//! helpers used by the live-preview CLI mode.

pub mod frame;
pub mod live;
pub mod presenter;

pub use frame::{FrameCell, FrameChange, FrameDelta, TerminalFrame};
pub use live::{
    build_inline_frame, build_preview_frame, clamp_viewport, initial_viewport, Viewport,
};
pub use presenter::{AnsiDiffPresenter, InlinePresenter, TerminalPresenter};
