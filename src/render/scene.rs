//! Typed scene intents and deterministic cell resolution.
//!
//! This is the first projection seam in the modular renderer. Existing route
//! families can continue to draw through `Canvas`; selected families may queue
//! typed intents and let this resolver apply the same Canvas ownership and
//! overlap rules. Keeping the resolver thin makes byte-for-byte comparison
//! against the legacy path possible.

use std::collections::BTreeMap;

use serde::Serialize;

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
    ResolveEdgeOverlapInferred,
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
        Self::edge_owned(
            x,
            y,
            glyph,
            CellOwnerKind::EdgeSegment,
            owner_id,
            role,
            z_index,
        )
    }

    /// Create an edge-style intent with an explicit semantic owner.
    pub(crate) fn edge_owned(
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
            mode: SceneIntentMode::ResolveEdgeOverlap,
            sequence: 0,
        }
    }

    /// Create an edge intent that delegates both overlap and metadata
    /// inference to Canvas's legacy edge-write path.
    pub(crate) fn edge_inferred(x: usize, y: usize, glyph: char) -> Self {
        Self {
            x,
            y,
            glyph,
            owner_kind: CellOwnerKind::EdgeSegment,
            owner_id: String::new(),
            role: CellRole::Unknown,
            z_index: 0,
            mode: SceneIntentMode::ResolveEdgeOverlapInferred,
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
        self.intents.sort_by(scene_intent_order);

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
                SceneIntentMode::ResolveEdgeOverlapInferred => {
                    canvas.set_edge_char(intent.x, intent.y, intent.glyph, chars)
                }
            }
            report.applied += 1;
        }
        report
    }

    pub(crate) fn resolve_with_recorder(
        &mut self,
        canvas: &mut Canvas,
        chars: &StyleChars,
        recorder: &mut SceneRecorder,
        stage: &str,
    ) -> SceneResolveReport {
        recorder.observe(stage, self, canvas.width, canvas.height);
        self.resolve(canvas, chars)
    }
}

fn scene_intent_order(left: &SceneIntent, right: &SceneIntent) -> std::cmp::Ordering {
    left.z_index
        .cmp(&right.z_index)
        .then_with(|| left.y.cmp(&right.y))
        .then_with(|| left.x.cmp(&right.x))
        .then_with(|| left.owner_id.cmp(&right.owner_id))
        .then_with(|| left.sequence.cmp(&right.sequence))
}

/// Counts from one deterministic resolution pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SceneResolveReport {
    pub(crate) applied: usize,
    pub(crate) skipped: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ScenePrimitiveRecord {
    stage: String,
    x: usize,
    y: usize,
    glyph: char,
    owner_kind: String,
    owner_id: Option<String>,
    role: String,
    z_index: u8,
    mode: String,
    sequence: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SceneRejectionRecord {
    stage: String,
    x: usize,
    y: usize,
    reason: String,
}

/// Private recording/null consumer for the scene pilot.
///
/// Recording does not participate in glyph lowering. It checks the primitive
/// vocabulary and collision policy while the legacy Canvas path remains the
/// production authority.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SceneRecorder {
    records: Vec<ScenePrimitiveRecord>,
    rejections: Vec<SceneRejectionRecord>,
}

impl SceneRecorder {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn observe(&mut self, stage: &str, scene: &Scene, width: usize, height: usize) {
        let mut intents = scene.intents.clone();
        intents.sort_by(scene_intent_order);
        let mut owners: BTreeMap<(usize, usize), (u8, String)> = BTreeMap::new();

        for intent in intents {
            let inferred = matches!(intent.mode, SceneIntentMode::ResolveEdgeOverlapInferred);
            let owner_id = (!intent.owner_id.is_empty()).then(|| intent.owner_id.clone());
            if intent.x >= width || intent.y >= height {
                self.rejections.push(SceneRejectionRecord {
                    stage: stage.to_string(),
                    x: intent.x,
                    y: intent.y,
                    reason: "out-of-bounds primitive".to_string(),
                });
            }
            if !inferred && owner_id.is_none() {
                self.rejections.push(SceneRejectionRecord {
                    stage: stage.to_string(),
                    x: intent.x,
                    y: intent.y,
                    reason: "explicit primitive is missing owner_id".to_string(),
                });
            }
            if let Some(owner_id) = owner_id.as_ref() {
                if let Some((existing_z, existing_owner)) = owners.get(&(intent.x, intent.y)) {
                    if *existing_z == intent.z_index
                        && existing_owner != owner_id
                        && matches!(intent.mode, SceneIntentMode::ReplaceOwned)
                    {
                        self.rejections.push(SceneRejectionRecord {
                            stage: stage.to_string(),
                            x: intent.x,
                            y: intent.y,
                            reason: format!(
                                "same-layer owner collision: {existing_owner} vs {owner_id}"
                            ),
                        });
                    }
                }
                let replace = owners
                    .get(&(intent.x, intent.y))
                    .is_none_or(|(existing_z, _)| intent.z_index >= *existing_z);
                if replace {
                    owners.insert((intent.x, intent.y), (intent.z_index, owner_id.clone()));
                }
            }
            self.records.push(ScenePrimitiveRecord {
                stage: stage.to_string(),
                x: intent.x,
                y: intent.y,
                glyph: intent.glyph,
                owner_kind: format!("{:?}", intent.owner_kind),
                owner_id,
                role: format!("{:?}", intent.role),
                z_index: intent.z_index,
                mode: format!("{:?}", intent.mode),
                sequence: intent.sequence,
            });
        }
    }

    #[allow(dead_code)]
    pub(crate) fn stable_json(&self) -> String {
        let mut records = self.records.clone();
        records.sort_by(|left, right| {
            left.stage
                .cmp(&right.stage)
                .then_with(|| left.z_index.cmp(&right.z_index))
                .then_with(|| left.y.cmp(&right.y))
                .then_with(|| left.x.cmp(&right.x))
                .then_with(|| left.owner_id.cmp(&right.owner_id))
                .then_with(|| left.sequence.cmp(&right.sequence))
        });
        let mut rejections = self.rejections.clone();
        rejections.sort_by(|left, right| {
            left.stage
                .cmp(&right.stage)
                .then_with(|| left.y.cmp(&right.y))
                .then_with(|| left.x.cmp(&right.x))
                .then_with(|| left.reason.cmp(&right.reason))
        });
        serde_json::json!({
            "schema": "termiflow.scene_recording.v1",
            "records": records,
            "rejections": rejections,
        })
        .to_string()
    }

    #[allow(dead_code)]
    pub(crate) fn record_count(&self) -> usize {
        self.records.len()
    }

    #[allow(dead_code)]
    pub(crate) fn rejection_count(&self) -> usize {
        self.rejections.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Direction, Graph};
    use crate::render::critic;
    use crate::render::semantic::SemanticFrame;
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

    #[test]
    fn edge_owned_preserves_overlap_and_records_junction_provenance() {
        let style = chars();
        let mut canvas = Canvas::new(3, 3);
        canvas.set_owned(
            1,
            1,
            style.edge_v,
            CellOwnerKind::EdgeSegment,
            "edge:old",
            5,
        );
        let mut scene = Scene::new();
        scene.push(SceneIntent::edge_owned(
            1,
            1,
            style.edge_h,
            CellOwnerKind::Junction,
            "junction:A",
            CellRole::Junction,
            5,
        ));

        let report = scene.resolve(&mut canvas, &style);
        let meta = canvas.get_meta(1, 1).expect("junction metadata");

        assert_eq!(report.applied, 1);
        assert_eq!(canvas.get(1, 1), style.cross);
        assert_eq!(meta.owner_kind, CellOwnerKind::Junction);
        assert_eq!(meta.owner_id.as_deref(), Some("junction:A"));
        assert_eq!(meta.z_index, 5);
    }

    #[test]
    fn empty_scene_is_a_noop() {
        let mut scene = Scene::new();
        let mut canvas = Canvas::new(1, 1);

        assert_eq!(
            scene.resolve(&mut canvas, &chars()),
            SceneResolveReport::default()
        );
        assert_eq!(canvas.get(0, 0), ' ');
    }

    #[test]
    fn inferred_edge_intents_keep_canvas_metadata_inferred() {
        let style = chars();
        let mut canvas = Canvas::new(3, 3);
        canvas.set_edge_char(1, 1, style.edge_v, &style);
        let mut scene = Scene::new();
        scene.push(SceneIntent::edge_inferred(1, 1, style.edge_h));

        let report = scene.resolve(&mut canvas, &style);
        let meta = canvas.get_meta(1, 1).expect("inferred metadata");

        assert_eq!(report.applied, 1);
        assert_eq!(canvas.get(1, 1), style.cross);
        assert_eq!(meta.owner_id, None);
        assert_eq!(meta.z_index, 0);
        assert_eq!(meta.owner_kind, CellOwnerKind::Junction);
    }

    #[test]
    fn recorder_rejects_same_layer_explicit_owner_collisions() {
        let mut scene = Scene::new();
        scene.push(SceneIntent::owned(
            1,
            1,
            'A',
            CellOwnerKind::NodeLabel,
            "node:a",
            CellRole::Text,
            3,
        ));
        scene.push(SceneIntent::owned(
            1,
            1,
            'B',
            CellOwnerKind::NodeLabel,
            "node:b",
            CellRole::Text,
            3,
        ));

        let mut recorder = SceneRecorder::new();
        recorder.observe("collision-test", &scene, 3, 3);

        assert_eq!(recorder.record_count(), 2);
        assert_eq!(recorder.rejection_count(), 1);
        assert!(recorder
            .stable_json()
            .contains("same-layer owner collision"));
    }

    #[test]
    fn scene_and_legacy_lowering_match_all_differential_surfaces() {
        let style = chars();
        let mut scene = Scene::new();
        scene.push(SceneIntent::edge_owned(
            1,
            1,
            style.edge_h,
            CellOwnerKind::EdgeSegment,
            "edge:a",
            CellRole::Horizontal,
            5,
        ));
        scene.push(SceneIntent::edge_owned(
            1,
            1,
            style.edge_v,
            CellOwnerKind::Junction,
            "junction:a",
            CellRole::Junction,
            5,
        ));

        let mut recorder = SceneRecorder::new();
        let mut scene_canvas = Canvas::new(3, 3);
        let outcome =
            scene.resolve_with_recorder(&mut scene_canvas, &style, &mut recorder, "differential");

        let mut legacy_canvas = Canvas::new(3, 3);
        legacy_canvas.set_edge_char_owned(
            1,
            1,
            style.edge_h,
            &style,
            CellOwnerKind::EdgeSegment,
            "edge:a",
            5,
        );
        legacy_canvas.set_edge_char_owned(
            1,
            1,
            style.edge_v,
            &style,
            CellOwnerKind::Junction,
            "junction:a",
            5,
        );

        assert_eq!(
            outcome,
            SceneResolveReport {
                applied: 2,
                skipped: 0
            }
        );
        assert_eq!(scene_canvas.to_string(), legacy_canvas.to_string());

        let scene_frame = SemanticFrame::from_canvas(&scene_canvas);
        let legacy_frame = SemanticFrame::from_canvas(&legacy_canvas);
        assert_eq!(scene_frame, legacy_frame);
        assert_eq!(
            scene_frame.crop_and_pad(true, 1),
            legacy_frame.crop_and_pad(true, 1)
        );

        let graph = Graph {
            direction: Direction::LR,
            ..Graph::default()
        };
        let scene_critic = critic::analyze(&graph, &scene_frame, graph.direction, &style);
        let legacy_critic = critic::analyze(&graph, &legacy_frame, graph.direction, &style);
        assert_eq!(scene_critic, legacy_critic);
        let scene_warnings: Vec<String> = Vec::new();
        let legacy_warnings: Vec<String> = Vec::new();
        assert_eq!(scene_warnings, legacy_warnings);
        let scene_repairs = (0usize, 0usize);
        let legacy_repairs = (0usize, 0usize);
        assert_eq!(scene_repairs, legacy_repairs);
        assert_eq!(recorder.rejection_count(), 0);
    }

    #[test]
    fn recorder_marks_empty_and_clipped_primitives_without_claiming_success() {
        let mut empty = Scene::new();
        let mut recorder = SceneRecorder::new();
        recorder.observe("empty", &empty, 2, 2);
        assert_eq!(recorder.record_count(), 0);

        empty.push(SceneIntent::owned(
            9,
            9,
            'x',
            CellOwnerKind::NodeLabel,
            "node:clipped",
            CellRole::Text,
            1,
        ));
        recorder.observe("clipped", &empty, 2, 2);
        assert_eq!(recorder.rejection_count(), 1);
        assert!(recorder.stable_json().contains("out-of-bounds primitive"));
    }
}
