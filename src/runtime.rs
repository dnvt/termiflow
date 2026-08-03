//! Process-bound inputs captured once at a rendering boundary.
//!
//! The renderer historically read environment variables from several layout
//! and render helpers.  This module keeps the compatibility behavior while
//! making those inputs explicit and stable for one render attempt. Public entry
//! points install a scoped context; direct low-level calls use a current capture
//! when no boundary has been installed.

use std::cell::RefCell;

/// Immutable process-bound inputs for one rendering operation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RuntimeContext {
    pub(crate) compatibility: CompatibilityOverrides,
    pub(crate) diagnostics: DiagnosticContext,
    pub(crate) terminal: TerminalDimensions,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CompatibilityOverrides {
    /// `TERMIFLOW_OPTIMIZE_RENDER` enables optimization when present.
    pub(crate) optimize_render: bool,
    /// `TERMIFLOW_DISABLE_PORTALS` disables portal carving when present.
    pub(crate) disable_portals: bool,
    /// An invalid repair-pass value is ignored, matching the legacy fallback.
    pub(crate) render_repair_passes: Option<usize>,
    /// An invalid repair-pass value is ignored, matching the legacy fallback.
    pub(crate) layout_repair_passes: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DiagnosticContext {
    pub(crate) timing: bool,
    pub(crate) routes: bool,
    pub(crate) fan_in: bool,
    pub(crate) fan_out: bool,
    pub(crate) cross: bool,
    pub(crate) crossing: bool,
    pub(crate) critic: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TerminalDimensions {
    pub(crate) columns: Option<usize>,
    pub(crate) lines: Option<usize>,
}

impl RuntimeContext {
    pub(crate) fn capture() -> Self {
        Self {
            compatibility: CompatibilityOverrides {
                optimize_render: present("TERMIFLOW_OPTIMIZE_RENDER"),
                disable_portals: present("TERMIFLOW_DISABLE_PORTALS"),
                render_repair_passes: parsed_usize("TERMIFLOW_RENDER_REPAIR_PASSES")
                    .map(|value| value.max(1)),
                layout_repair_passes: parsed_usize("TERMIFLOW_LAYOUT_REPAIR_PASSES")
                    .map(|value| value.max(1)),
            },
            diagnostics: DiagnosticContext {
                timing: present("TERMIFLOW_DEBUG_TIMING"),
                routes: present("TERMIFLOW_DEBUG_ROUTES"),
                fan_in: present("DEBUG_FANIN"),
                fan_out: present("DEBUG_FANOUT"),
                cross: present("DEBUG_CROSS"),
                crossing: present("TERMIFLOW_DEBUG_CROSSING"),
                critic: present("TERMIFLOW_DEBUG_CRITIC"),
            },
            terminal: TerminalDimensions {
                columns: parsed_usize("COLUMNS"),
                lines: parsed_usize("LINES"),
            },
        }
    }
}

fn present(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

fn parsed_usize(name: &str) -> Option<usize> {
    std::env::var(name).ok()?.parse::<usize>().ok()
}

thread_local! {
    static CURRENT_CONTEXT: RefCell<Vec<RuntimeContext>> = const { RefCell::new(Vec::new()) };
}

struct ContextGuard;

impl Drop for ContextGuard {
    fn drop(&mut self) {
        CURRENT_CONTEXT.with(|stack| {
            let _ = stack.borrow_mut().pop();
        });
    }
}

pub(crate) fn with_context<T>(context: RuntimeContext, operation: impl FnOnce() -> T) -> T {
    CURRENT_CONTEXT.with(|stack| stack.borrow_mut().push(context));
    let _guard = ContextGuard;
    operation()
}

pub(crate) fn with_captured<T>(operation: impl FnOnce() -> T) -> T {
    let already_captured = CURRENT_CONTEXT.with(|stack| !stack.borrow().is_empty());
    if already_captured {
        operation()
    } else {
        with_context(RuntimeContext::capture(), operation)
    }
}

pub(crate) fn current() -> RuntimeContext {
    CURRENT_CONTEXT
        .with(|stack| stack.borrow().last().cloned())
        .unwrap_or_else(RuntimeContext::capture)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_context_is_restored_after_nested_operation() {
        let outer = RuntimeContext {
            compatibility: CompatibilityOverrides {
                optimize_render: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let inner = RuntimeContext {
            diagnostics: DiagnosticContext {
                timing: true,
                ..Default::default()
            },
            ..Default::default()
        };

        with_context(outer.clone(), || {
            assert_eq!(current(), outer);
            with_context(inner.clone(), || assert_eq!(current(), inner));
            assert_eq!(current(), outer);
        });

        assert_eq!(current(), RuntimeContext::capture());
    }

    #[test]
    fn repair_overrides_normalize_zero_values() {
        let context = RuntimeContext::capture();
        assert!(context
            .compatibility
            .render_repair_passes
            .is_none_or(|passes| passes >= 1));
        assert!(context
            .compatibility
            .layout_repair_passes
            .is_none_or(|passes| passes >= 1));
    }
}
