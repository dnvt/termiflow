//! Versioned evidence records for deterministic visual-audit packets.
//!
//! This module deliberately consumes the final graph and render outcome from
//! one render invocation. It keeps machine-readable evidence beside the
//! existing human-facing output without changing the normal CLI contract.

use super::critic::CriticReport;
use super::semantic::{CellOwnerKind, CellRole, SemanticFrame};
use super::trace::{GeometryTrace, PortalTrace};
use super::RenderOutcome;
use crate::graph::Graph;
use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

pub const EVIDENCE_SCHEMA: &str = "termiflow.render_evidence.v1";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RenderEvidence {
    pub schema: &'static str,
    pub canvas: FrameDimensions,
    pub display: FrameDimensions,
    pub semantic: SemanticSummary,
    pub raw: RawFrameReport,
    pub geometry: GeometryReport,
    pub geometry_trace: GeometryTrace,
    pub portal_trace: PortalTrace,
    pub critic: CriticReport,
    pub warnings: Vec<String>,
    pub optimized: bool,
    pub repair_passes: usize,
    pub layout_attempts: usize,
    pub layout_repairs_applied: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FrameDimensions {
    pub width: usize,
    pub height: usize,
    pub non_space_cells: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct SemanticSummary {
    pub owner_counts: BTreeMap<String, usize>,
    pub role_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CellCoordinate {
    pub x: usize,
    pub y: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct RawFrameReport {
    pub errors: Vec<String>,
    pub arrowheads: usize,
    pub shaftless_arrowheads: Vec<CellCoordinate>,
    pub visible_label_cells: usize,
    pub missing_node_labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct GeometryReport {
    pub errors: Vec<String>,
    pub node_count: usize,
    pub edge_count: usize,
    pub subgraph_count: usize,
    pub traced_edges: usize,
    pub untraced_fallback_edges: Vec<String>,
    pub segment_count: usize,
}

pub fn build(graph: &Graph, outcome: &RenderOutcome) -> RenderEvidence {
    let geometry_trace = GeometryTrace::from_graph(graph);
    let portal_trace = outcome.portal_trace.clone();
    let mut geometry = geometry_report(graph, &geometry_trace);
    apply_fallback_geometry_trace(&mut geometry, &portal_trace);
    RenderEvidence {
        schema: EVIDENCE_SCHEMA,
        canvas: frame_dimensions(&outcome.semantic_frame),
        display: frame_dimensions(&outcome.display_semantic_frame),
        semantic: semantic_summary(&outcome.semantic_frame),
        raw: raw_frame_report(graph, &outcome.semantic_frame),
        geometry,
        geometry_trace,
        portal_trace,
        critic: canonical_critic_report(&outcome.critic_report),
        warnings: outcome.warnings.clone(),
        optimized: outcome.optimized,
        repair_passes: outcome.repair_passes,
        layout_attempts: outcome.layout_attempts,
        layout_repairs_applied: outcome.layout_repairs_applied,
        policy: None,
    }
}

fn apply_fallback_geometry_trace(geometry: &mut GeometryReport, portal_trace: &PortalTrace) {
    for rejection in &portal_trace.fallback_route_rejections {
        geometry.errors.push(format!(
            "fallback {} rejected by {}: {}",
            rejection.owner_id, rejection.strategy, rejection.reason
        ));
    }
    for fallback in &portal_trace.fallback_routes {
        if fallback.mismatches.is_empty() {
            let covered_edge_ids = if fallback.covered_edge_ids.is_empty() {
                vec![fallback.owner_id.clone()]
            } else {
                fallback.covered_edge_ids.clone()
            };
            for edge_id in covered_edge_ids {
                if let Some(index) = geometry
                    .untraced_fallback_edges
                    .iter()
                    .position(|owner_id| owner_id == &edge_id)
                {
                    geometry.untraced_fallback_edges.remove(index);
                    geometry.traced_edges += 1;
                }
            }
            geometry.segment_count += fallback.planned_segments.len() + fallback.paints.len();
        } else {
            for mismatch in &fallback.mismatches {
                geometry
                    .errors
                    .push(format!("fallback {}: {mismatch}", fallback.owner_id));
            }
        }
    }
}

/// Write evidence atomically next to the requested destination.
pub fn write_json(path: &Path, graph: &Graph, outcome: &RenderOutcome) -> Result<()> {
    write_json_with_policy(path, graph, outcome, None)
}

/// Write evidence with the resolved CLI/runtime policy used for the render.
pub fn write_json_with_policy(
    path: &Path,
    graph: &Graph,
    outcome: &RenderOutcome,
    policy: Option<&serde_json::Value>,
) -> Result<()> {
    let mut evidence = build(graph, outcome);
    evidence.policy = policy.cloned();
    let bytes = serde_json::to_vec_pretty(&evidence).context("serialize render evidence")?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("audit evidence path must have a UTF-8 file name")?;
    let temporary = path.with_file_name(format!(".{file_name}.tmp-{}", std::process::id()));

    if let Err(error) = fs::write(&temporary, bytes) {
        return Err(error).with_context(|| {
            format!(
                "write temporary render evidence file {}",
                temporary.display()
            )
        });
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error)
            .with_context(|| format!("publish render evidence file {}", path.display()));
    }
    Ok(())
}

fn frame_dimensions(frame: &SemanticFrame) -> FrameDimensions {
    FrameDimensions {
        width: frame.width,
        height: frame.height,
        non_space_cells: frame.non_space_cell_count(),
    }
}

fn semantic_summary(frame: &SemanticFrame) -> SemanticSummary {
    let mut summary = SemanticSummary::default();
    for cell in &frame.cells {
        increment(&mut summary.owner_counts, format!("{:?}", cell.owner_kind));
        increment(&mut summary.role_counts, format!("{:?}", cell.role));
    }
    summary
}

fn increment(counts: &mut BTreeMap<String, usize>, key: String) {
    *counts.entry(key).or_default() += 1;
}

fn canonical_critic_report(report: &CriticReport) -> CriticReport {
    let mut report = report.clone();
    report.findings.sort_by_key(|finding| {
        (
            format!("{:?}", finding.code),
            format!("{:?}", finding.severity),
            finding.penalty,
            finding.message.clone(),
            finding.cells.clone(),
            finding.owner_ids.clone(),
        )
    });
    report
}

fn raw_frame_report(graph: &Graph, frame: &SemanticFrame) -> RawFrameReport {
    let mut report = RawFrameReport::default();
    let mut visible_label_owners = HashSet::new();

    for y in 0..frame.height {
        for x in 0..frame.width {
            let Some(cell) = frame.get(x, y) else {
                continue;
            };
            if cell.owner_kind == CellOwnerKind::NodeLabel && cell.ch != ' ' {
                report.visible_label_cells += 1;
                if let Some(owner_id) = &cell.owner_id {
                    visible_label_owners.insert(owner_id.clone());
                }
            }
            if cell.role != CellRole::ArrowTip {
                continue;
            }

            report.arrowheads += 1;
            if !has_visible_raw_shaft(frame, x, y, cell) {
                report.shaftless_arrowheads.push(CellCoordinate { x, y });
            }
        }
    }

    for coordinate in &report.shaftless_arrowheads {
        report.errors.push(format!(
            "arrowhead at ({},{}) lacks a visible raw-frame shaft",
            coordinate.x, coordinate.y
        ));
    }

    for node in &graph.nodes {
        if !node.label.is_empty() && !visible_label_owners.contains(&node.id) {
            report.missing_node_labels.push(node.id.clone());
            report.errors.push(format!(
                "node label {:?} is absent from raw frame",
                node.label
            ));
        }
    }

    if frame.non_space_cell_count() == 0 && !graph.nodes.is_empty() {
        report.errors.push("rendered frame is empty".to_string());
    }
    report
}

fn arrow_predecessor(
    frame: &SemanticFrame,
    x: usize,
    y: usize,
    arrow: char,
) -> Option<&super::semantic::CellMeta> {
    match arrow {
        'v' | '↓' | '▼' => y.checked_sub(1).and_then(|py| frame.get(x, py)),
        '^' | '↑' | '▲' => frame.get(x, y + 1),
        '>' | '→' | '▶' => x.checked_sub(1).and_then(|px| frame.get(px, y)),
        '<' | '←' | '◀' => frame.get(x + 1, y),
        _ => None,
    }
}

fn has_visible_raw_shaft(
    frame: &SemanticFrame,
    x: usize,
    y: usize,
    arrow: &super::semantic::CellMeta,
) -> bool {
    let Some(predecessor) = arrow_predecessor(frame, x, y, arrow.ch) else {
        return false;
    };
    if is_route_glyph(predecessor.ch) {
        return true;
    }

    // Edge labels intentionally occupy the final stem cell for vertical
    // labeled routes. Keep this independent raw check strict by accepting only
    // a label that belongs to the same edge as the arrowhead.
    predecessor.owner_kind == CellOwnerKind::EdgeLabel
        && arrow.owner_id.is_some()
        && predecessor.owner_id == arrow.owner_id
}

fn is_route_glyph(ch: char) -> bool {
    matches!(
        ch,
        '-' | '|'
            | '+'
            | '='
            | ':'
            | '.'
            | '─'
            | '│'
            | '┌'
            | '┐'
            | '└'
            | '┘'
            | '├'
            | '┤'
            | '┬'
            | '┴'
            | '┯'
            | '┷'
            | '┰'
            | '┸'
            | '┼'
            | '═'
            | '║'
            | '╔'
            | '╗'
            | '╚'
            | '╝'
            | '╠'
            | '╣'
            | '╦'
            | '╩'
            | '╤'
            | '╧'
            | '╥'
            | '╨'
            | '╬'
            | '━'
            | '┃'
            | '╌'
            | '╎'
            | '┄'
            | '┈'
            | '┅'
            | '┉'
            | '┆'
            | '┊'
            | '┋'
            | '╏'
            | '╋'
    )
}

fn geometry_report(graph: &Graph, trace: &GeometryTrace) -> GeometryReport {
    let mut report = GeometryReport {
        node_count: trace.nodes.len(),
        edge_count: trace.edges.len(),
        subgraph_count: trace.subgraphs.len(),
        ..GeometryReport::default()
    };

    for (index, left) in trace.nodes.iter().enumerate() {
        if left.width == 0 || left.height == 0 {
            report
                .errors
                .push(format!("node {} has an empty rectangle", left.id));
        }
        for right in &trace.nodes[index + 1..] {
            if rectangles_overlap(left, right) {
                report.errors.push(format!(
                    "node rectangles {} and {} overlap",
                    left.id, right.id
                ));
            }
        }
    }

    let node_ids: HashSet<&str> = trace.nodes.iter().map(|node| node.id.as_str()).collect();
    for edge in &trace.edges {
        if !node_ids.contains(edge.from.as_str()) || !node_ids.contains(edge.to.as_str()) {
            report
                .errors
                .push(format!("edge {} has an unknown endpoint", edge.owner_id));
        }
        if edge.segments.is_empty() {
            report.untraced_fallback_edges.push(edge.owner_id.clone());
        } else {
            report.traced_edges += 1;
            report.segment_count += edge.segments.len();
        }
    }

    // The trace intentionally does not claim coverage for fallback routes.
    // Keep the graph argument in the signature so the distinction remains an
    // explicit seam when route coverage is expanded in a later phase.
    let _ = graph;
    report
}

fn rectangles_overlap(left: &super::trace::NodeTrace, right: &super::trace::NodeTrace) -> bool {
    left.x < right.x.saturating_add(right.width)
        && right.x < left.x.saturating_add(left.width)
        && left.y < right.y.saturating_add(right.height)
        && right.y < left.y.saturating_add(left.height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Direction, Edge, Graph, Node};

    fn cell(
        ch: char,
        role: CellRole,
        owner_kind: CellOwnerKind,
    ) -> super::super::semantic::CellMeta {
        cell_with_owner(ch, role, owner_kind, "edge:0:A->B")
    }

    fn cell_with_owner(
        ch: char,
        role: CellRole,
        owner_kind: CellOwnerKind,
        owner_id: &str,
    ) -> super::super::semantic::CellMeta {
        super::super::semantic::CellMeta {
            ch,
            owner_kind,
            owner_id: Some(owner_id.to_string()),
            role,
            z_index: 1,
        }
    }

    #[test]
    fn raw_report_catches_shaftless_arrow() {
        let frame = SemanticFrame {
            width: 2,
            height: 1,
            cells: vec![
                cell(' ', CellRole::Empty, CellOwnerKind::Empty),
                cell('>', CellRole::ArrowTip, CellOwnerKind::ArrowHead),
            ],
        };
        let mut graph = Graph::new();
        graph.add_node(Node::new("A", "A"));

        let report = raw_frame_report(&graph, &frame);
        assert_eq!(report.arrowheads, 1);
        assert_eq!(report.shaftless_arrowheads.len(), 1);
        assert!(report.errors[0].contains("shaft"));
    }

    #[test]
    fn raw_report_accepts_same_edge_label_before_arrow() {
        let frame = SemanticFrame {
            width: 1,
            height: 3,
            cells: vec![
                cell('|', CellRole::Vertical, CellOwnerKind::EdgeSegment),
                cell('L', CellRole::Text, CellOwnerKind::EdgeLabel),
                cell('v', CellRole::ArrowTip, CellOwnerKind::ArrowHead),
            ],
        };

        let report = raw_frame_report(&Graph::new(), &frame);
        assert!(report.shaftless_arrowheads.is_empty());
        assert!(report.errors.is_empty());
    }

    #[test]
    fn raw_report_rejects_different_edge_label_before_arrow() {
        let frame = SemanticFrame {
            width: 1,
            height: 3,
            cells: vec![
                cell('|', CellRole::Vertical, CellOwnerKind::EdgeSegment),
                cell_with_owner('L', CellRole::Text, CellOwnerKind::EdgeLabel, "edge:1:X->Y"),
                cell('v', CellRole::ArrowTip, CellOwnerKind::ArrowHead),
            ],
        };

        let report = raw_frame_report(&Graph::new(), &frame);
        assert_eq!(report.shaftless_arrowheads.len(), 1);
        assert!(report.errors[0].contains("shaft"));
    }

    #[test]
    fn raw_report_accepts_unicode_dotted_shaft_before_arrow() {
        let frame = SemanticFrame {
            width: 2,
            height: 1,
            cells: vec![
                cell('╌', CellRole::Horizontal, CellOwnerKind::EdgeSegment),
                cell('→', CellRole::ArrowTip, CellOwnerKind::ArrowHead),
            ],
        };

        let report = raw_frame_report(&Graph::new(), &frame);
        assert!(report.shaftless_arrowheads.is_empty());
        assert!(report.errors.is_empty());
    }

    #[test]
    fn geometry_report_distinguishes_untraced_fallback_edges() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;
        let mut source = Node::new("A", "A");
        source.width = 5;
        let mut target = Node::new("B", "B");
        target.x = 8;
        target.y = 8;
        target.width = 5;
        graph.add_node(source);
        graph.add_node(target);
        graph.add_edge(Edge::new("A", "B"));

        let trace = GeometryTrace::from_graph(&graph);
        let report = geometry_report(&graph, &trace);
        assert_eq!(report.errors, Vec::<String>::new());
        assert_eq!(report.traced_edges, 0);
        assert_eq!(report.untraced_fallback_edges.len(), 1);
    }

    #[test]
    fn evidence_serializes_with_versioned_schema() {
        let outcome = RenderOutcome {
            output: "A".to_string(),
            semantic_frame: SemanticFrame::default(),
            display_semantic_frame: SemanticFrame::default(),
            critic_report: CriticReport::default(),
            warnings: Vec::new(),
            optimized: false,
            repair_passes: 0,
            layout_attempts: 1,
            layout_repairs_applied: 0,
            portal_trace: PortalTrace::default(),
        };
        let evidence = build(&Graph::new(), &outcome);
        let json = serde_json::to_value(evidence).expect("serialize evidence");
        assert_eq!(json["schema"], EVIDENCE_SCHEMA);
        assert!(json.get("critic").is_some());
        assert!(json.get("geometry_trace").is_some());
        assert!(json.get("portal_trace").is_some());
    }
}
