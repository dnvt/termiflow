//! Typed scene intents and deterministic cell resolution.
//!
//! This is the first projection seam in the modular renderer. Existing route
//! families can continue to draw through `Canvas`; selected families may queue
//! typed intents and let this resolver apply the same Canvas ownership and
//! overlap rules. Keeping the resolver thin makes byte-for-byte comparison
//! against the legacy path possible.

use super::canvas;
use super::semantic::{CellOwnerKind, CellRole};
use super::Canvas;
use crate::style::StyleChars;

/// How an intent interacts with an occupied cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum SceneIntentMode {
    ReplaceOwned,
    ResolveEdgeOverlap,
}

/// A typed request to project one semantic scene cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SceneIntent {
    pub(crate) x: usize,
    pub(crate) y: usize,
    pub(crate) glyph: char,
    pub(crate) owner_kind: CellOwnerKind,
    pub(crate) owner_id: String,
    pub(crate) role: CellRole,
    pub(crate) z_index: u8,
    mode: SceneIntentMode,
    sequence: usize,
}

impl SceneIntent {
    /// Create an owned marker that should retain the legacy direct-write
    /// behavior while still being represented as a typed intent.
    pub(crate) fn owned(
        x: usize,
        y: usize,
        glyph: char,
        owner_kind: CellOwnerKind,
        owner_id: impl Into<String>,
        role: CellRole,
        z_index: u8,
    ) -> Self {
        Self {
            x,
            y,
            glyph,
            owner_kind,
            owner_id: owner_id.into(),
            role,
            z_index,
            mode: SceneIntentMode::ReplaceOwned,
            sequence: 0,
        }
    }

    /// Create an edge intent that uses Canvas's canonical overlap resolver.
    #[allow(dead_code)]
    pub(crate) fn edge(
        x: usize,
        y: usize,
        glyph: char,
        owner_id: impl Into<String>,
        role: CellRole,
        z_index: u8,
    ) -> Self {
        Self {
            x,
            y,
            glyph,
            owner_kind: CellOwnerKind::EdgeSegment,
            owner_id: owner_id.into(),
            role,
            z_index,
            mode: SceneIntentMode::ResolveEdgeOverlap,
            sequence: 0,
        }
    }
}

/// Deterministic collection of scene intents for one projection stage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Scene {
    intents: Vec<SceneIntent>,
}

impl Scene {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn push(&mut self, mut intent: SceneIntent) {
        intent.sequence = self.intents.len();
        self.intents.push(intent);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.intents.is_empty()
    }

    /// Resolve intents in stable priority/position/owner order.
    pub(crate) fn resolve(
        &mut self,
        canvas: &mut Canvas,
        chars: &StyleChars,
    ) -> SceneResolveReport {
        self.intents.sort_by(|left, right| {
            left.z_index
                .cmp(&right.z_index)
                .then_with(|| left.y.cmp(&right.y))
                .then_with(|| left.x.cmp(&right.x))
                .then_with(|| left.owner_id.cmp(&right.owner_id))
                .then_with(|| left.sequence.cmp(&right.sequence))
        });

        let mut report = SceneResolveReport::default();
        for intent in &self.intents {
            let existing = canvas.get(intent.x, intent.y);
            let existing_is_arrow = canvas::is_arrow(existing);
            let conflicting_arrow = existing_is_arrow && existing != intent.glyph;
            if conflicting_arrow {
                report.skipped += 1;
                continue;
            }

            match intent.mode {
                SceneIntentMode::ReplaceOwned => canvas.set_owned(
                    intent.x,
                    intent.y,
                    intent.glyph,
                    intent.owner_kind,
                    &intent.owner_id,
                    intent.z_index,
                ),
                SceneIntentMode::ResolveEdgeOverlap => canvas.set_edge_char_owned(
                    intent.x,
                    intent.y,
                    intent.glyph,
                    chars,
                    intent.owner_kind,
                    &intent.owner_id,
                    intent.z_index,
                ),
            }
            report.applied += 1;
        }
        report
    }
}

/// Counts from one deterministic resolution pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SceneResolveReport {
    pub(crate) applied: usize,
    pub(crate) skipped: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{BaseStyle, StyleChars};

    fn chars() -> StyleChars {
        *BaseStyle::Unicode.chars()
    }

    #[test]
    fn scene_resolution_is_deterministic_and_uses_canvas_overlap() {
        let mut first = Scene::new();
        first.push(SceneIntent::edge(
            1,
            1,
            '─',
            "edge:b",
            CellRole::Horizontal,
            5,
        ));
        first.push(SceneIntent::edge(
            1,
            1,
            '│',
            "edge:a",
            CellRole::Vertical,
            5,
        ));
        let mut second = first.clone();
        let mut first_canvas = Canvas::new(3, 3);
        let mut second_canvas = Canvas::new(3, 3);

        let first_report = first.resolve(&mut first_canvas, &chars());
        let second_report = second.resolve(&mut second_canvas, &chars());

        assert_eq!(first_report, second_report);
        assert_eq!(first_canvas.get(1, 1), second_canvas.get(1, 1));
        assert_eq!(first_canvas.get(1, 1), '┼');
        assert_eq!(first_report.applied, 2);
    }

    #[test]
    fn scene_preserves_existing_arrowheads() {
        let style = chars();
        let mut canvas = Canvas::new(3, 1);
        canvas.set_owned(
            1,
            0,
            style.arrow_right,
            CellOwnerKind::ArrowHead,
            "edge:old",
            6,
        );
        let mut scene = Scene::new();
        scene.push(SceneIntent::edge(
            1,
            0,
            style.edge_v,
            "edge:new",
            CellRole::Vertical,
            5,
        ));

        let report = scene.resolve(&mut canvas, &style);

        assert_eq!(canvas.get(1, 0), style.arrow_right);
        assert_eq!(report.applied, 0);
        assert_eq!(report.skipped, 1);
    }

    #[test]
    fn owned_intents_keep_explicit_provenance() {
        let style = chars();
        let mut canvas = Canvas::new(2, 1);
        let mut scene = Scene::new();
        scene.push(SceneIntent::owned(
            0,
            0,
            style.arrow_down,
            CellOwnerKind::ArrowHead,
            "edge:0",
            CellRole::ArrowTip,
            6,
        ));

        let report = scene.resolve(&mut canvas, &style);
        let meta = canvas.get_meta(0, 0).expect("intent writes metadata");
        assert_eq!(report.applied, 1);
        assert_eq!(meta.owner_id.as_deref(), Some("edge:0"));
        assert_eq!(meta.role, CellRole::ArrowTip);
    }
}
