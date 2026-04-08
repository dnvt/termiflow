//! Render critic rules and reporting.
//!
//! The critic turns a semantic frame into concrete findings that can drive
//! local repairs. The current rule set is intentionally bounded and only
//! targets defects that can be fixed on the rendered canvas without relayout.

use super::semantic::{CellOwnerKind, CellRole, SemanticFrame};
use super::subgraph_title_y;
use super::topology::{
    canonical_routing_glyph, char_connects_down, char_connects_left, char_connects_right,
    char_connects_up, frame_connections,
};
use crate::graph::{Direction, Graph};
use crate::style::StyleChars;
use std::collections::HashMap;

type BranchSets<'a> = (
    HashMap<&'a str, Vec<&'a str>>,
    HashMap<&'a str, Vec<&'a str>>,
);

/// Stable code for a critic finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingCode {
    EmptyRenderedFrame,
    JunctionTopologyMismatch,
    RouteTopologyMismatch,
    RouteSymmetryImbalance,
    BranchSpacingImbalance,
    BranchCrowding,
    UnusedPortalOpening,
    ArrowWithoutVisibleShaft,
    ChainTooCrampedLR,
    ArrowTouchesNodeBorder,
    ArrowTouchesSubgraphBorder,
    RouteCrossesNodeInterior,
    SubgraphTitleCorrupted,
    CrowdedEdgeLabel,
    CanvasClipped,
    EdgeLabelCollidesWithNode,
}

/// Severity level for a critic finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingSeverity {
    Info,
    Warning,
    Error,
}

/// Single critic finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriticFinding {
    pub code: FindingCode,
    pub severity: FindingSeverity,
    pub penalty: i32,
    pub message: String,
    pub cells: Vec<(usize, usize)>,
    pub owner_ids: Vec<String>,
}

/// Aggregate critic report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CriticReport {
    pub score: i32,
    pub findings: Vec<CriticFinding>,
    pub notes: Vec<String>,
}

/// High-level quality verdict derived from a critic report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditVerdict {
    Clean,
    NeedsReview,
    Broken,
}

/// Human-facing audit summary for a rendered diagram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditSummary {
    pub verdict: AuditVerdict,
    pub score: i32,
    pub info_count: usize,
    pub warning_count: usize,
    pub error_count: usize,
    pub highlights: Vec<String>,
}

impl AuditSummary {
    pub fn is_clean(&self) -> bool {
        self.verdict == AuditVerdict::Clean
    }
}

impl CriticReport {
    pub fn audit_summary(&self) -> AuditSummary {
        let info_count = self
            .findings
            .iter()
            .filter(|finding| finding.severity == FindingSeverity::Info)
            .count();
        let warning_count = self
            .findings
            .iter()
            .filter(|finding| finding.severity == FindingSeverity::Warning)
            .count();
        let error_count = self
            .findings
            .iter()
            .filter(|finding| finding.severity == FindingSeverity::Error)
            .count();

        let verdict = if error_count > 0 {
            AuditVerdict::Broken
        } else if self.findings.is_empty() {
            AuditVerdict::Clean
        } else {
            AuditVerdict::NeedsReview
        };

        let mut ordered: Vec<&CriticFinding> = self.findings.iter().collect();
        ordered.sort_by_key(|finding| (severity_rank(finding.severity), finding.penalty));
        ordered.reverse();
        let highlights = ordered
            .into_iter()
            .take(5)
            .map(|finding| {
                format!(
                    "{:?} {:?}: {}",
                    finding.severity, finding.code, finding.message
                )
            })
            .collect();

        AuditSummary {
            verdict,
            score: self.score,
            info_count,
            warning_count,
            error_count,
            highlights,
        }
    }
}

/// Analyze a semantic frame and generate actionable findings.
pub fn analyze(
    graph: &Graph,
    frame: &SemanticFrame,
    direction: Direction,
    chars: &StyleChars,
) -> CriticReport {
    let mut findings = Vec::new();

    if frame.non_space_cell_count() == 0 && !graph.nodes.is_empty() {
        findings.push(CriticFinding {
            code: FindingCode::EmptyRenderedFrame,
            severity: FindingSeverity::Warning,
            penalty: 100,
            message: "rendered frame is empty despite non-empty graph".to_string(),
            cells: Vec::new(),
            owner_ids: Vec::new(),
        });
    }

    findings.extend(find_junction_topology_mismatches(frame, chars));
    findings.extend(find_route_topology_mismatches(frame, chars));
    findings.extend(find_route_symmetry_imbalances(graph, direction));
    findings.extend(find_branch_spacing_imbalances(graph, direction));
    findings.extend(find_branch_crowding(graph, direction));
    findings.extend(find_unused_portal_openings(graph, frame));
    findings.extend(find_arrow_without_visible_shaft(frame));
    findings.extend(find_arrow_touching_node_borders(graph, frame));
    findings.extend(find_arrow_touching_subgraph_borders(graph, frame));
    findings.extend(find_subgraph_border_portal_artifacts(
        graph, frame, direction,
    ));
    findings.extend(find_route_crossing_node_interiors(graph, frame));
    findings.extend(find_subgraph_title_corruption(graph, frame, direction));
    findings.extend(find_crowded_edge_labels(frame));
    findings.extend(find_edge_label_collisions_with_nodes(graph, frame));
    findings.extend(find_canvas_clipping(graph, frame));
    if matches!(direction, Direction::LR | Direction::RL) {
        findings.extend(find_chain_too_cramped_lr(graph, chars));
    }

    let score: i32 = findings.iter().map(|finding| finding.penalty).sum();
    let notes = vec![
        format!("nodes={}", graph.nodes.len()),
        format!("edges={}", graph.edges.len()),
        format!("subgraphs={}", graph.subgraphs.len()),
        format!("frame={}x{}", frame.width, frame.height),
        format!("non_space_cells={}", frame.non_space_cell_count()),
    ];

    CriticReport {
        score,
        findings,
        notes,
    }
}

/// Compatibility shim for the initial Phase 6.0 debug path.
pub fn baseline_report(graph: &Graph, frame: &SemanticFrame) -> CriticReport {
    let chars =
        crate::style::CompositeStyle::default().to_style_chars(crate::style::BaseStyle::Unicode);
    analyze(graph, frame, graph.direction, &chars)
}

/// Emit a compact debug report to stderr.
pub fn emit_debug_report(report: &CriticReport) {
    eprintln!("termiflow: critic score={}", report.score);
    for note in &report.notes {
        eprintln!("termiflow: critic note: {note}");
    }
    for finding in &report.findings {
        eprintln!(
            "termiflow: critic finding: {:?} {:?} penalty={} {}",
            finding.severity, finding.code, finding.penalty, finding.message
        );
    }
}

fn severity_rank(severity: FindingSeverity) -> u8 {
    match severity {
        FindingSeverity::Info => 0,
        FindingSeverity::Warning => 1,
        FindingSeverity::Error => 2,
    }
}

fn glyph_is_ambiguous_topology(ch: char, chars: &StyleChars) -> bool {
    let variants = [
        chars.cross,
        chars.junction_down,
        chars.junction_up,
        chars.junction_right,
        chars.junction_left,
        chars.corner_dl,
        chars.corner_dr,
        chars.corner_ul,
        chars.corner_ur,
    ];

    variants.iter().filter(|glyph| **glyph == ch).count() > 1
}

fn find_junction_topology_mismatches(
    frame: &SemanticFrame,
    chars: &StyleChars,
) -> Vec<CriticFinding> {
    let mut findings = Vec::new();

    for y in 0..frame.height {
        for x in 0..frame.width {
            let Some(cell) = frame.get(x, y) else {
                continue;
            };
            if cell.role == CellRole::Junction {
                let connections = frame_connections(frame, x, y);
                let arms = connections.arm_count();
                if arms < 3 && !(arms >= 2 && glyph_is_ambiguous_topology(cell.ch, chars)) {
                    findings.push(CriticFinding {
                        code: FindingCode::JunctionTopologyMismatch,
                        severity: FindingSeverity::Warning,
                        penalty: 15,
                        message: format!("junction at ({x},{y}) has only {arms} connected arm(s)"),
                        cells: vec![(x, y)],
                        owner_ids: cell.owner_id.clone().into_iter().collect(),
                    });
                } else if let Some(expected) =
                    canonical_routing_glyph(connections, chars, cell.owner_kind)
                {
                    if cell.ch != expected {
                        findings.push(CriticFinding {
                            code: FindingCode::JunctionTopologyMismatch,
                            severity: FindingSeverity::Warning,
                            penalty: 15,
                            message: format!(
                                "junction at ({x},{y}) implies '{expected}' but rendered '{}'",
                                cell.ch
                            ),
                            cells: vec![(x, y)],
                            owner_ids: cell.owner_id.clone().into_iter().collect(),
                        });
                    }
                }
            }
        }
    }

    findings
}

fn find_route_topology_mismatches(frame: &SemanticFrame, chars: &StyleChars) -> Vec<CriticFinding> {
    let mut findings = Vec::new();

    for y in 0..frame.height {
        for x in 0..frame.width {
            let Some(cell) = frame.get(x, y) else {
                continue;
            };
            if !matches!(
                cell.owner_kind,
                CellOwnerKind::EdgeSegment
                    | CellOwnerKind::CycleEdge
                    | CellOwnerKind::ArrowHead
                    | CellOwnerKind::Junction
            ) {
                continue;
            }
            if !matches!(
                cell.role,
                CellRole::Horizontal | CellRole::Vertical | CellRole::Corner
            ) {
                continue;
            }

            let connections = frame_connections(frame, x, y);
            let Some(expected) = canonical_routing_glyph(connections, chars, cell.owner_kind)
            else {
                continue;
            };

            if cell.ch != expected {
                findings.push(CriticFinding {
                    code: FindingCode::RouteTopologyMismatch,
                    severity: FindingSeverity::Warning,
                    penalty: 10,
                    message: format!(
                        "routing glyph at ({x},{y}) implies '{expected}' but rendered '{}'",
                        cell.ch
                    ),
                    cells: vec![(x, y)],
                    owner_ids: cell.owner_id.clone().into_iter().collect(),
                });
            }
        }
    }

    findings
}

fn find_route_symmetry_imbalances(graph: &Graph, direction: Direction) -> Vec<CriticFinding> {
    let (fanout, fanin) = collect_branch_sets(graph);

    let mut findings = Vec::new();
    for (anchor, others) in fanout {
        if others.len() < 2 {
            continue;
        }
        if let Some(finding) =
            branch_symmetry_finding(graph, direction, anchor, &others, "fan-out", true)
        {
            findings.push(finding);
        }
    }
    for (anchor, others) in fanin {
        if others.len() < 2 {
            continue;
        }
        if let Some(finding) =
            branch_symmetry_finding(graph, direction, anchor, &others, "fan-in", false)
        {
            findings.push(finding);
        }
    }

    findings
}

fn find_branch_spacing_imbalances(graph: &Graph, direction: Direction) -> Vec<CriticFinding> {
    let (fanout, fanin) = collect_branch_sets(graph);

    let mut findings = Vec::new();
    for (anchor, others) in fanout {
        if others.len() < 3 {
            continue;
        }
        if let Some(finding) = branch_spacing_finding(graph, direction, anchor, &others, "fan-out")
        {
            findings.push(finding);
        }
    }
    for (anchor, others) in fanin {
        if others.len() < 3 {
            continue;
        }
        if let Some(finding) = branch_spacing_finding(graph, direction, anchor, &others, "fan-in") {
            findings.push(finding);
        }
    }

    findings
}

fn find_branch_crowding(graph: &Graph, direction: Direction) -> Vec<CriticFinding> {
    let (fanout, fanin) = collect_branch_sets(graph);

    let mut findings = Vec::new();
    for (anchor, others) in fanout {
        if others.len() < 2 {
            continue;
        }
        if let Some(finding) = branch_crowding_finding(graph, direction, anchor, &others, "fan-out")
        {
            findings.push(finding);
        }
    }
    for (anchor, others) in fanin {
        if others.len() < 2 {
            continue;
        }
        if let Some(finding) = branch_crowding_finding(graph, direction, anchor, &others, "fan-in")
        {
            findings.push(finding);
        }
    }

    findings
}

fn collect_branch_sets(graph: &Graph) -> BranchSets<'_> {
    let mut fanout: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut fanin: HashMap<&str, Vec<&str>> = HashMap::new();

    for edge in &graph.edges {
        if edge.is_back_edge {
            continue;
        }
        if graph.get_node(&edge.from).is_none() || graph.get_node(&edge.to).is_none() {
            continue;
        }
        fanout.entry(&edge.from).or_default().push(&edge.to);
        fanin.entry(&edge.to).or_default().push(&edge.from);
    }

    (fanout, fanin)
}

fn branch_symmetry_finding(
    graph: &Graph,
    direction: Direction,
    anchor_id: &str,
    other_ids: &[&str],
    kind: &str,
    anchor_is_source: bool,
) -> Option<CriticFinding> {
    let anchor = graph.get_node(anchor_id)?;
    let anchor_secondary = node_secondary_center(anchor, direction);

    let mut secondaries: Vec<usize> = other_ids
        .iter()
        .filter_map(|node_id| graph.get_node(node_id))
        .map(|node| node_secondary_center(node, direction))
        .collect();
    secondaries.sort_unstable();
    let (Some(min_secondary), Some(max_secondary)) =
        (secondaries.first().copied(), secondaries.last().copied())
    else {
        return None;
    };
    if max_secondary <= min_secondary {
        return None;
    }

    let midpoint = (min_secondary + max_secondary) / 2;
    let offset = anchor_secondary.abs_diff(midpoint);
    if offset <= 1 {
        return None;
    }
    if is_balanced_crossing_permutation(
        graph,
        direction,
        anchor_id,
        other_ids.len(),
        anchor_is_source,
    ) {
        return None;
    }

    let mut owner_ids = Vec::with_capacity(other_ids.len() + 1);
    owner_ids.push(anchor_id.to_string());
    owner_ids.extend(other_ids.iter().map(|node_id| (*node_id).to_string()));

    let relation = if anchor_is_source {
        "targets"
    } else {
        "sources"
    };
    Some(CriticFinding {
        code: FindingCode::RouteSymmetryImbalance,
        severity: FindingSeverity::Info,
        penalty: 6,
        message: format!(
            "{kind} at {anchor_id} is off-center from its {relation} by {offset} cell(s)"
        ),
        cells: Vec::new(),
        owner_ids,
    })
}

fn is_balanced_crossing_permutation(
    graph: &Graph,
    direction: Direction,
    anchor_id: &str,
    branch_degree: usize,
    anchor_is_source: bool,
) -> bool {
    if branch_degree < 2 {
        return false;
    }

    let Some(anchor) = graph.get_node(anchor_id) else {
        return false;
    };
    let anchor_primary = node_primary_center(anchor, direction);

    let mut peer_branches: Vec<(&crate::graph::Node, Vec<String>)> = graph
        .nodes
        .iter()
        .filter(|node| node_primary_center(node, direction).abs_diff(anchor_primary) <= 1)
        .filter_map(|node| {
            let relation_ids = collect_relation_ids(graph, &node.id, anchor_is_source);
            (relation_ids.len() == branch_degree).then_some((node, relation_ids))
        })
        .collect();
    if peer_branches.len() < 3 {
        return false;
    }

    peer_branches.sort_unstable_by_key(|(node, _)| node_secondary_center(node, direction));
    let peer_centers: Vec<usize> = peer_branches
        .iter()
        .map(|(node, _)| node_secondary_center(node, direction))
        .collect();
    if !centers_are_evenly_spaced(&peer_centers) {
        return false;
    }

    let mut relation_frequency: HashMap<String, usize> = HashMap::new();
    for (_, relation_ids) in &peer_branches {
        for relation_id in relation_ids {
            *relation_frequency.entry(relation_id.clone()).or_insert(0) += 1;
        }
    }
    if relation_frequency.len() != peer_branches.len() {
        return false;
    }
    if relation_frequency.values().any(|count| *count < 2) {
        return false;
    }
    if branch_degree >= relation_frequency.len() {
        return false;
    }

    let mut relation_nodes: Vec<&crate::graph::Node> = relation_frequency
        .keys()
        .filter_map(|node_id| graph.get_node(node_id))
        .collect();
    if relation_nodes.len() != relation_frequency.len() {
        return false;
    }
    relation_nodes.sort_unstable_by_key(|node| node_secondary_center(node, direction));

    let relation_primaries: Vec<usize> = relation_nodes
        .iter()
        .map(|node| node_primary_center(node, direction))
        .collect();
    let (Some(min_relation_primary), Some(max_relation_primary)) = (
        relation_primaries.iter().min().copied(),
        relation_primaries.iter().max().copied(),
    ) else {
        return false;
    };
    if max_relation_primary.saturating_sub(min_relation_primary) > 2 {
        return false;
    }

    let relation_centers: Vec<usize> = relation_nodes
        .iter()
        .map(|node| node_secondary_center(node, direction))
        .collect();
    if !centers_are_evenly_spaced(&relation_centers) {
        return false;
    }

    let peer_span = peer_centers
        .last()
        .copied()
        .unwrap_or(0)
        .saturating_sub(peer_centers.first().copied().unwrap_or(0));
    let relation_span = relation_centers
        .last()
        .copied()
        .unwrap_or(0)
        .saturating_sub(relation_centers.first().copied().unwrap_or(0));

    peer_span.abs_diff(relation_span) <= 2
}

fn collect_relation_ids(graph: &Graph, anchor_id: &str, anchor_is_source: bool) -> Vec<String> {
    let mut relation_ids = Vec::new();

    for edge in &graph.edges {
        if edge.is_back_edge {
            continue;
        }

        let relation_id = if anchor_is_source {
            (edge.from == anchor_id).then_some(edge.to.as_str())
        } else {
            (edge.to == anchor_id).then_some(edge.from.as_str())
        };

        if let Some(relation_id) = relation_id {
            if !relation_ids.iter().any(|existing| existing == relation_id) {
                relation_ids.push(relation_id.to_string());
            }
        }
    }

    relation_ids
}

fn centers_are_evenly_spaced(centers: &[usize]) -> bool {
    if centers.len() < 3 {
        return false;
    }

    let gaps: Vec<usize> = centers
        .windows(2)
        .map(|pair| pair[1].saturating_sub(pair[0]))
        .collect();
    let (Some(min_gap), Some(max_gap)) = (gaps.iter().min().copied(), gaps.iter().max().copied())
    else {
        return false;
    };

    min_gap > 0 && max_gap.saturating_sub(min_gap) <= 2
}

fn branch_spacing_finding(
    graph: &Graph,
    direction: Direction,
    anchor_id: &str,
    other_ids: &[&str],
    kind: &str,
) -> Option<CriticFinding> {
    let mut branches: Vec<(&str, usize)> = other_ids
        .iter()
        .filter_map(|node_id| {
            graph
                .get_node(node_id)
                .map(|node| (*node_id, node_secondary_center(node, direction)))
        })
        .collect();
    if branches.len() < 3 {
        return None;
    }

    branches.sort_unstable_by_key(|(_, secondary)| *secondary);
    let gaps: Vec<usize> = branches
        .windows(2)
        .map(|pair| pair[1].1.saturating_sub(pair[0].1))
        .collect();
    let (Some(min_gap), Some(max_gap)) = (gaps.iter().min().copied(), gaps.iter().max().copied())
    else {
        return None;
    };

    let imbalance = max_gap.saturating_sub(min_gap);
    if imbalance < 5 {
        return None;
    }

    let ratio = max_gap as f32 / min_gap.max(1) as f32;
    if ratio < 1.6 {
        return None;
    }

    let mut owner_ids = Vec::with_capacity(branches.len() + 1);
    owner_ids.push(anchor_id.to_string());
    owner_ids.extend(branches.iter().map(|(node_id, _)| (*node_id).to_string()));

    Some(CriticFinding {
        code: FindingCode::BranchSpacingImbalance,
        severity: FindingSeverity::Info,
        penalty: 5,
        message: format!(
            "{kind} at {anchor_id} has uneven branch spacing (gaps {min_gap}..{max_gap})"
        ),
        cells: Vec::new(),
        owner_ids,
    })
}

fn branch_crowding_finding(
    graph: &Graph,
    direction: Direction,
    anchor_id: &str,
    other_ids: &[&str],
    kind: &str,
) -> Option<CriticFinding> {
    let mut branches: Vec<(&str, usize, usize, usize)> = other_ids
        .iter()
        .filter_map(|node_id| {
            graph.get_node(node_id).map(|node| {
                (
                    *node_id,
                    node_secondary_start(node, direction),
                    node_secondary_end(node, direction),
                    node_primary_center(node, direction),
                )
            })
        })
        .collect();
    if branches.len() < 2 {
        return None;
    }

    let (Some(min_primary), Some(max_primary)) = (
        branches.iter().map(|(_, _, _, primary)| *primary).min(),
        branches.iter().map(|(_, _, _, primary)| *primary).max(),
    ) else {
        return None;
    };
    if max_primary.saturating_sub(min_primary) > 2 {
        return None;
    }

    branches.sort_unstable_by_key(|(_, start, _, _)| *start);
    let min_gap = branches
        .windows(2)
        .map(|pair| pair[1].1.saturating_sub(pair[0].2))
        .min()
        .unwrap_or(usize::MAX);
    let desired_gap = desired_branch_gap(direction);
    if min_gap >= desired_gap {
        return None;
    }

    let mut owner_ids = Vec::with_capacity(branches.len() + 1);
    owner_ids.push(anchor_id.to_string());
    owner_ids.extend(
        branches
            .iter()
            .map(|(node_id, _, _, _)| (*node_id).to_string()),
    );

    Some(CriticFinding {
        code: FindingCode::BranchCrowding,
        severity: FindingSeverity::Info,
        penalty: 6,
        message: format!(
            "{kind} at {anchor_id} has cramped sibling gaps (min {min_gap}, target {desired_gap})"
        ),
        cells: Vec::new(),
        owner_ids,
    })
}

fn desired_branch_gap(direction: Direction) -> usize {
    match direction {
        Direction::TD | Direction::TB | Direction::BT => 3,
        Direction::LR | Direction::RL => 1,
    }
}

fn find_unused_portal_openings(graph: &Graph, frame: &SemanticFrame) -> Vec<CriticFinding> {
    let mut findings = Vec::new();

    for sg in &graph.subgraphs {
        let x0 = sg.bounds.x;
        let x1 = sg.bounds.x + sg.bounds.width.saturating_sub(1);
        let y0 = sg.bounds.y;
        let y1 = sg.bounds.y + sg.bounds.height.saturating_sub(1);

        for x in x0..=x1 {
            maybe_push_unused_portal(frame, x, y0, &mut findings);
            maybe_push_unused_portal(frame, x, y1, &mut findings);
        }
        for y in y0..=y1 {
            maybe_push_unused_portal(frame, x0, y, &mut findings);
            maybe_push_unused_portal(frame, x1, y, &mut findings);
        }
    }

    findings
}

fn maybe_push_unused_portal(
    frame: &SemanticFrame,
    x: usize,
    y: usize,
    findings: &mut Vec<CriticFinding>,
) {
    let Some(cell) = frame.get(x, y) else {
        return;
    };
    if cell.owner_kind != CellOwnerKind::PortalOpening {
        return;
    }

    let neighbors = [
        frame
            .get(x, y.saturating_sub(1))
            .map(|cell| cell.ch)
            .unwrap_or(' '),
        if y + 1 < frame.height {
            frame.get(x, y + 1).map(|cell| cell.ch).unwrap_or(' ')
        } else {
            ' '
        },
        frame
            .get(x.saturating_sub(1), y)
            .map(|cell| cell.ch)
            .unwrap_or(' '),
        if x + 1 < frame.width {
            frame.get(x + 1, y).map(|cell| cell.ch).unwrap_or(' ')
        } else {
            ' '
        },
    ];

    if neighbors.iter().all(|ch| !is_line_like(*ch)) {
        findings.push(CriticFinding {
            code: FindingCode::UnusedPortalOpening,
            severity: FindingSeverity::Info,
            penalty: 5,
            message: format!("unused portal opening at ({x},{y})"),
            cells: vec![(x, y)],
            owner_ids: Vec::new(),
        });
    }
}

fn find_arrow_without_visible_shaft(frame: &SemanticFrame) -> Vec<CriticFinding> {
    let mut findings = Vec::new();

    for y in 0..frame.height {
        for x in 0..frame.width {
            let Some(cell) = frame.get(x, y) else {
                continue;
            };
            if cell.role != CellRole::ArrowTip {
                continue;
            }

            let connections = frame_connections(frame, x, y);
            let has_shaft = match cell.ch {
                '>' | '→' | '▶' => connections.left,
                '<' | '←' | '◀' => connections.right,
                '^' | '↑' | '▲' => connections.down,
                'v' | '↓' | '▼' => connections.up,
                _ => false,
            };

            if !has_shaft && !arrow_uses_subgraph_border_pierce(frame, x, y, cell.ch) {
                findings.push(CriticFinding {
                    code: FindingCode::ArrowWithoutVisibleShaft,
                    severity: FindingSeverity::Warning,
                    penalty: 10,
                    message: format!("arrow at ({x},{y}) has no visible shaft"),
                    cells: vec![(x, y)],
                    owner_ids: cell.owner_id.clone().into_iter().collect(),
                });
            }
        }
    }

    findings
}

fn arrow_uses_subgraph_border_pierce(
    frame: &SemanticFrame,
    x: usize,
    y: usize,
    arrow: char,
) -> bool {
    let behind = match arrow {
        '>' | '→' | '▶' => x.checked_sub(1).and_then(|xx| frame.get(xx, y)),
        '<' | '←' | '◀' => frame.get(x + 1, y),
        '^' | '↑' | '▲' => frame.get(x, y + 1),
        'v' | '↓' | '▼' => y.checked_sub(1).and_then(|yy| frame.get(x, yy)),
        _ => None,
    };

    let Some(cell) = behind else {
        return false;
    };
    if cell.owner_kind != CellOwnerKind::SubgraphBorder {
        return false;
    }

    // Check that the border cell has an arm pointing back toward the edge source
    // (opposite to the arrow direction), confirming the shaft runs through it.
    match arrow {
        '^' | '↑' | '▲' => char_connects_down(cell.ch),
        'v' | '↓' | '▼' => char_connects_up(cell.ch),
        '>' | '→' | '▶' => char_connects_left(cell.ch),
        '<' | '←' | '◀' => char_connects_right(cell.ch),
        _ => false,
    }
}

fn find_arrow_touching_node_borders(graph: &Graph, frame: &SemanticFrame) -> Vec<CriticFinding> {
    let mut findings = Vec::new();

    for y in 0..frame.height {
        for x in 0..frame.width {
            let Some(cell) = frame.get(x, y) else {
                continue;
            };
            if cell.role != CellRole::ArrowTip {
                continue;
            }

            for node in &graph.nodes {
                let max_y = node.y + node.height.max(crate::style::BOX_HEIGHT).saturating_sub(1);
                let max_x = node.x + node.width.saturating_sub(1);
                if x < node.x || x > max_x || y < node.y || y > max_y {
                    continue;
                }
                let on_border = x == node.x || x == max_x || y == node.y || y == max_y;
                if on_border {
                    findings.push(CriticFinding {
                        code: FindingCode::ArrowTouchesNodeBorder,
                        severity: FindingSeverity::Warning,
                        penalty: 12,
                        message: format!("arrow at ({x},{y}) lands on node border {}", node.id),
                        cells: vec![(x, y)],
                        owner_ids: vec![node.id.clone()],
                    });
                }
            }
        }
    }

    findings
}

fn find_arrow_touching_subgraph_borders(
    graph: &Graph,
    frame: &SemanticFrame,
) -> Vec<CriticFinding> {
    let mut findings = Vec::new();

    for y in 0..frame.height {
        for x in 0..frame.width {
            let Some(cell) = frame.get(x, y) else {
                continue;
            };
            if cell.role != CellRole::ArrowTip {
                continue;
            }

            for subgraph in &graph.subgraphs {
                let max_y = subgraph.bounds.y + subgraph.bounds.height.saturating_sub(1);
                let max_x = subgraph.bounds.x + subgraph.bounds.width.saturating_sub(1);
                if x < subgraph.bounds.x || x > max_x || y < subgraph.bounds.y || y > max_y {
                    continue;
                }
                let on_border =
                    x == subgraph.bounds.x || x == max_x || y == subgraph.bounds.y || y == max_y;
                if on_border {
                    findings.push(CriticFinding {
                        code: FindingCode::ArrowTouchesSubgraphBorder,
                        severity: FindingSeverity::Warning,
                        penalty: 10,
                        message: format!(
                            "arrow at ({x},{y}) lands on subgraph border {}",
                            subgraph.id
                        ),
                        cells: vec![(x, y)],
                        owner_ids: vec![subgraph.id.clone()],
                    });
                }
            }
        }
    }

    findings
}

fn find_crowded_edge_labels(frame: &SemanticFrame) -> Vec<CriticFinding> {
    let mut by_owner: std::collections::HashMap<String, Vec<(usize, usize)>> =
        std::collections::HashMap::new();

    for y in 0..frame.height {
        for x in 0..frame.width {
            let Some(cell) = frame.get(x, y) else {
                continue;
            };
            if cell.owner_kind != CellOwnerKind::EdgeLabel {
                continue;
            }
            let Some(owner_id) = cell.owner_id.clone() else {
                continue;
            };
            if has_crowding_neighbor(frame, x, y, &owner_id) {
                by_owner.entry(owner_id).or_default().push((x, y));
            }
        }
    }

    by_owner
        .into_iter()
        .map(|(owner_id, cells)| CriticFinding {
            code: FindingCode::CrowdedEdgeLabel,
            severity: FindingSeverity::Info,
            penalty: 8,
            message: format!("edge label {owner_id} is crowded by nearby routing"),
            cells,
            owner_ids: vec![owner_id],
        })
        .collect()
}

fn find_route_crossing_node_interiors(graph: &Graph, frame: &SemanticFrame) -> Vec<CriticFinding> {
    let mut findings = Vec::new();

    for node in &graph.nodes {
        let max_y = node.y + node.height.max(crate::style::BOX_HEIGHT).saturating_sub(1);
        let max_x = node.x + node.width.saturating_sub(1);
        if max_x <= node.x + 1 || max_y <= node.y + 1 {
            continue;
        }

        let mut cells = Vec::new();
        for y in (node.y + 1)..max_y {
            for x in (node.x + 1)..max_x {
                let Some(cell) = frame.get(x, y) else {
                    continue;
                };
                if matches!(
                    cell.owner_kind,
                    CellOwnerKind::EdgeSegment
                        | CellOwnerKind::CycleEdge
                        | CellOwnerKind::ArrowHead
                        | CellOwnerKind::Junction
                        | CellOwnerKind::EdgeLabel
                ) {
                    cells.push((x, y));
                }
            }
        }

        if !cells.is_empty() {
            findings.push(CriticFinding {
                code: FindingCode::RouteCrossesNodeInterior,
                severity: FindingSeverity::Warning,
                penalty: 12,
                message: format!("routing intrudes into node interior {}", node.id),
                cells,
                owner_ids: vec![node.id.clone()],
            });
        }
    }

    findings
}

fn find_subgraph_title_corruption(
    graph: &Graph,
    frame: &SemanticFrame,
    direction: Direction,
) -> Vec<CriticFinding> {
    let mut findings = Vec::new();

    for subgraph in &graph.subgraphs {
        let Some(title) = subgraph.title.as_deref() else {
            continue;
        };
        if !subgraph.bounds.is_valid() {
            continue;
        }

        let title_fmt = format!("[  {}  ]", title);
        let title_len = title_fmt.chars().count();
        if title_len > subgraph.bounds.width.saturating_sub(2) {
            continue;
        }

        let start_x = subgraph.bounds.x + (subgraph.bounds.width - title_len) / 2;
        let title_y = subgraph_title_y(&subgraph.bounds, direction);

        let mut cells = Vec::new();
        for (offset, expected_ch) in title_fmt.chars().enumerate() {
            let x = start_x + offset;
            let Some(cell) = frame.get(x, title_y) else {
                continue;
            };
            if cell.ch != expected_ch {
                cells.push((x, title_y));
            }
        }

        if matches!(direction, Direction::BT) && title_y != subgraph.bounds.y {
            let inner_left = subgraph.bounds.x.saturating_add(1);
            let inner_right = subgraph.bounds.x + subgraph.bounds.width.saturating_sub(2);
            let title_end = start_x + title_len;
            let bottom_y = subgraph.bounds.y + subgraph.bounds.height.saturating_sub(1);
            let protected_left = start_x.saturating_sub(2).max(inner_left);
            let protected_right = title_end.saturating_add(1).min(inner_right);
            for x in inner_left..=inner_right {
                if x >= start_x && x < title_end {
                    continue;
                }
                let Some(cell) = frame.get(x, title_y) else {
                    continue;
                };
                let horizontal_only = char_connects_left(cell.ch)
                    && char_connects_right(cell.ch)
                    && !char_connects_up(cell.ch)
                    && !char_connects_down(cell.ch);
                let vertical_only = char_connects_up(cell.ch)
                    && char_connects_down(cell.ch)
                    && !char_connects_left(cell.ch)
                    && !char_connects_right(cell.ch);
                let continues_from_below = title_y + 1 < frame.height
                    && frame
                        .get(x, title_y + 1)
                        .is_some_and(|below| char_connects_up(below.ch));
                let clean_row_glyph = if title_y == bottom_y {
                    horizontal_only
                        || (vertical_only
                            && (!continues_from_below || x < protected_left || x > protected_right))
                } else {
                    vertical_only
                };
                if is_line_like(cell.ch) && !clean_row_glyph {
                    cells.push((x, title_y));
                }
            }
        }
        cells.sort_unstable();
        cells.dedup();

        if !cells.is_empty() {
            findings.push(CriticFinding {
                code: FindingCode::SubgraphTitleCorrupted,
                severity: FindingSeverity::Warning,
                penalty: 12,
                message: format!(
                    "subgraph title {} is corrupted by border or routing",
                    subgraph.id
                ),
                cells,
                owner_ids: vec![subgraph.id.clone()],
            });
        }
    }

    findings
}

fn find_subgraph_border_portal_artifacts(
    graph: &Graph,
    frame: &SemanticFrame,
    direction: Direction,
) -> Vec<CriticFinding> {
    if !matches!(direction, Direction::LR | Direction::RL) {
        return Vec::new();
    }

    let mut findings = Vec::new();

    for subgraph in &graph.subgraphs {
        if !subgraph.bounds.is_valid() || subgraph.bounds.height < 3 {
            continue;
        }

        let left_x = subgraph.bounds.x;
        let right_x = subgraph.bounds.x + subgraph.bounds.width.saturating_sub(1);
        let min_y = subgraph.bounds.y.saturating_add(1);
        let max_y = subgraph.bounds.y + subgraph.bounds.height.saturating_sub(2);
        let mut cells = Vec::new();

        for y in min_y..=max_y {
            for x in [left_x, right_x] {
                let Some(cell) = frame.get(x, y) else {
                    continue;
                };
                if !is_line_like(cell.ch) {
                    continue;
                }

                let has_horizontal = char_connects_left(cell.ch) || char_connects_right(cell.ch);
                let has_vertical = char_connects_up(cell.ch) || char_connects_down(cell.ch);
                if has_horizontal && has_vertical {
                    cells.push((x, y));
                }
            }
        }

        cells.sort_unstable();
        cells.dedup();
        if !cells.is_empty() {
            findings.push(CriticFinding {
                code: FindingCode::RouteTopologyMismatch,
                severity: FindingSeverity::Warning,
                penalty: 10,
                message: format!(
                    "subgraph border {} uses junction-like side pierces instead of clean portal holes",
                    subgraph.id
                ),
                cells,
                owner_ids: vec![subgraph.id.clone()],
            });
        }
    }

    findings
}

fn has_crowding_neighbor(frame: &SemanticFrame, x: usize, y: usize, owner_id: &str) -> bool {
    let min_y = y.saturating_sub(1);
    let max_y = (y + 1).min(frame.height.saturating_sub(1));
    let min_x = x.saturating_sub(1);
    let max_x = (x + 1).min(frame.width.saturating_sub(1));
    let mut foreign_line_neighbors = Vec::new();

    for yy in min_y..=max_y {
        for xx in min_x..=max_x {
            if xx == x && yy == y {
                continue;
            }
            let Some(neighbor) = frame.get(xx, yy) else {
                continue;
            };
            if neighbor.owner_id.as_deref() == Some(owner_id) {
                continue;
            }
            if matches!(
                neighbor.role,
                CellRole::Horizontal
                    | CellRole::Vertical
                    | CellRole::Corner
                    | CellRole::Junction
                    | CellRole::ArrowTip
            ) {
                foreign_line_neighbors.push((xx, yy, neighbor.role));
            }
        }
    }

    if foreign_line_neighbors.is_empty() {
        return false;
    }

    // A label stacked cleanly above/below nearby routing is often readable and
    // intentional, even if the adjacent route includes corners or a junction.
    // Reserve the crowded-label finding for same-row pressure near the label.
    if foreign_line_neighbors.iter().all(|(_, yy, _)| *yy != y) {
        return false;
    }

    true
}

fn find_canvas_clipping(graph: &Graph, frame: &SemanticFrame) -> Vec<CriticFinding> {
    let max_graph_x = graph
        .nodes
        .iter()
        .map(|node| node.x + node.width)
        .chain(
            graph
                .subgraphs
                .iter()
                .map(|subgraph| subgraph.bounds.x + subgraph.bounds.width),
        )
        .max()
        .unwrap_or(0);
    let max_graph_y = graph
        .nodes
        .iter()
        .map(|node| node.y + node.height.max(crate::style::BOX_HEIGHT))
        .chain(
            graph
                .subgraphs
                .iter()
                .map(|subgraph| subgraph.bounds.y + subgraph.bounds.height),
        )
        .max()
        .unwrap_or(0);

    let mut findings = Vec::new();
    if max_graph_x > frame.width || max_graph_y > frame.height {
        findings.push(CriticFinding {
            code: FindingCode::CanvasClipped,
            severity: FindingSeverity::Warning,
            penalty: 20,
            message: format!(
                "graph bounds {}x{} exceed rendered frame {}x{}",
                max_graph_x, max_graph_y, frame.width, frame.height
            ),
            cells: Vec::new(),
            owner_ids: Vec::new(),
        });
    }

    findings
}

fn find_chain_too_cramped_lr(graph: &Graph, chars: &StyleChars) -> Vec<CriticFinding> {
    let mut findings = Vec::new();
    let min_gap = chars.arrow_right.len_utf8();

    for edge in &graph.edges {
        if edge.is_back_edge {
            continue;
        }
        let Some(from) = graph.get_node(&edge.from) else {
            continue;
        };
        let Some(to) = graph.get_node(&edge.to) else {
            continue;
        };
        let gap = if graph.direction == Direction::LR {
            to.x.saturating_sub(from.x + from.width)
        } else {
            from.x.saturating_sub(to.x + to.width)
        };
        if gap < min_gap {
            findings.push(CriticFinding {
                code: FindingCode::ChainTooCrampedLR,
                severity: FindingSeverity::Info,
                penalty: 5,
                message: format!(
                    "horizontal gap between {} and {} is cramped ({gap})",
                    from.id, to.id
                ),
                cells: Vec::new(),
                owner_ids: vec![from.id.clone(), to.id.clone()],
            });
        }
    }

    findings
}

fn node_secondary_center(node: &crate::graph::Node, direction: Direction) -> usize {
    match direction {
        Direction::TD | Direction::TB | Direction::BT => node.center_x(),
        Direction::LR | Direction::RL => node.center_y(),
    }
}

fn node_secondary_start(node: &crate::graph::Node, direction: Direction) -> usize {
    match direction {
        Direction::TD | Direction::TB | Direction::BT => node.x,
        Direction::LR | Direction::RL => node.y,
    }
}

fn node_secondary_end(node: &crate::graph::Node, direction: Direction) -> usize {
    match direction {
        Direction::TD | Direction::TB | Direction::BT => node.x + node.width,
        Direction::LR | Direction::RL => node.y + node.height.max(crate::style::BOX_HEIGHT),
    }
}

fn node_primary_center(node: &crate::graph::Node, direction: Direction) -> usize {
    match direction {
        Direction::TD | Direction::TB | Direction::BT => node.center_y(),
        Direction::LR | Direction::RL => node.center_x(),
    }
}

/// Detect edge label cells that overlap a node's bounding box.
///
/// An edge label is placed along its parent edge's routing path. If layout
/// geometry puts the route too close to a node, the label text may end up
/// on top of the node's border or interior characters. This finding drives
/// layout repair to push the affected edge further from the node.
fn find_edge_label_collisions_with_nodes(
    graph: &Graph,
    frame: &SemanticFrame,
) -> Vec<CriticFinding> {
    use std::collections::HashMap;

    let mut by_owner: HashMap<String, Vec<(usize, usize)>> = HashMap::new();

    for y in 0..frame.height {
        for x in 0..frame.width {
            let Some(cell) = frame.get(x, y) else {
                continue;
            };
            if cell.owner_kind != CellOwnerKind::EdgeLabel {
                continue;
            }
            let Some(ref owner_id) = cell.owner_id else {
                continue;
            };

            for node in &graph.nodes {
                let node_max_y = node.y + node.height.max(crate::style::BOX_HEIGHT);
                let node_max_x = node.x + node.width;
                if x >= node.x && x < node_max_x && y >= node.y && y < node_max_y {
                    by_owner.entry(owner_id.clone()).or_default().push((x, y));
                    break;
                }
            }
        }
    }

    by_owner
        .into_iter()
        .map(|(owner_id, cells)| CriticFinding {
            code: FindingCode::EdgeLabelCollidesWithNode,
            severity: FindingSeverity::Warning,
            penalty: 18,
            message: format!(
                "edge label {owner_id} overlaps a node bounding box ({} cell(s))",
                cells.len()
            ),
            cells,
            owner_ids: vec![owner_id],
        })
        .collect()
}

fn is_line_like(ch: char) -> bool {
    matches!(
        ch,
        '-' | '─'
            | '═'
            | '━'
            | '█'
            | '|'
            | ':'
            | '│'
            | '║'
            | '┃'
            | '+'
            | '┼'
            | '╬'
            | '╋'
            | '├'
            | '┤'
            | '┬'
            | '┴'
            | '╠'
            | '╣'
            | '╦'
            | '╩'
            | '┣'
            | '┫'
            | '┳'
            | '┻'
            | '┌'
            | '┐'
            | '└'
            | '┘'
            | '╔'
            | '╗'
            | '╚'
            | '╝'
            | '╭'
            | '╮'
            | '╰'
            | '╯'
            | '<'
            | '>'
            | '^'
            | 'v'
            | '→'
            | '←'
            | '↑'
            | '↓'
            | '▶'
            | '◀'
            | '▲'
            | '▼'
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Direction, Graph, Node, Rectangle, Subgraph};
    use crate::render::semantic::{CellMeta, CellOwnerKind, SemanticFrame};
    use crate::style::{BaseStyle, CompositeStyle};

    fn unicode_chars() -> StyleChars {
        CompositeStyle::default().to_style_chars(BaseStyle::Unicode)
    }

    #[test]
    fn baseline_report_adds_empty_frame_finding_for_non_empty_graph() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;
        graph.add_node(Node::new("A", "A"));

        let frame = SemanticFrame {
            width: 4,
            height: 2,
            cells: vec![Default::default(); 8],
        };

        let report = analyze(&graph, &frame, Direction::TD, &unicode_chars());

        assert!(report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::EmptyRenderedFrame));
        assert!(report.score >= 100);
    }

    #[test]
    fn analyze_reports_arrow_without_shaft() {
        let frame = SemanticFrame {
            width: 2,
            height: 1,
            cells: vec![
                Default::default(),
                CellMeta {
                    ch: '>',
                    owner_kind: CellOwnerKind::ArrowHead,
                    owner_id: None,
                    role: CellRole::ArrowTip,
                    z_index: 0,
                },
            ],
        };

        let report = analyze(&Graph::new(), &frame, Direction::LR, &unicode_chars());
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::ArrowWithoutVisibleShaft));
    }

    #[test]
    fn analyze_ignores_arrow_with_label_occluding_stem() {
        let frame = SemanticFrame {
            width: 1,
            height: 3,
            cells: vec![
                CellMeta {
                    ch: '^',
                    owner_kind: CellOwnerKind::ArrowHead,
                    owner_id: Some("edge:0:A->B".to_string()),
                    role: CellRole::ArrowTip,
                    z_index: 5,
                },
                CellMeta {
                    ch: 'L',
                    owner_kind: CellOwnerKind::EdgeLabel,
                    owner_id: Some("edge:0:A->B".to_string()),
                    role: CellRole::Text,
                    z_index: 6,
                },
                CellMeta {
                    ch: '│',
                    owner_kind: CellOwnerKind::EdgeSegment,
                    owner_id: Some("edge:0:A->B".to_string()),
                    role: CellRole::Vertical,
                    z_index: 5,
                },
            ],
        };

        let report = analyze(&Graph::new(), &frame, Direction::BT, &unicode_chars());
        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::ArrowWithoutVisibleShaft));
    }

    #[test]
    fn analyze_ignores_arrow_using_subgraph_border_pierce() {
        let frame = SemanticFrame {
            width: 1,
            height: 2,
            cells: vec![
                CellMeta {
                    ch: '↑',
                    owner_kind: CellOwnerKind::ArrowHead,
                    owner_id: Some("edge:0:A->B".to_string()),
                    role: CellRole::ArrowTip,
                    z_index: 5,
                },
                CellMeta {
                    ch: '┬',
                    owner_kind: CellOwnerKind::SubgraphBorder,
                    owner_id: Some("SG".to_string()),
                    role: CellRole::Border,
                    z_index: 1,
                },
            ],
        };

        let report = analyze(&Graph::new(), &frame, Direction::BT, &unicode_chars());
        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::ArrowWithoutVisibleShaft));
    }

    #[test]
    fn analyze_reports_crowded_edge_label() {
        let frame = SemanticFrame {
            width: 3,
            height: 2,
            cells: vec![
                CellMeta {
                    ch: 'L',
                    owner_kind: CellOwnerKind::EdgeLabel,
                    owner_id: Some("edge:0:A->B".to_string()),
                    role: CellRole::Text,
                    z_index: 6,
                },
                CellMeta {
                    ch: '┼',
                    owner_kind: CellOwnerKind::Junction,
                    owner_id: Some("edge:1:C->D".to_string()),
                    role: CellRole::Junction,
                    z_index: 5,
                },
                Default::default(),
                Default::default(),
                Default::default(),
                Default::default(),
            ],
        };

        let report = analyze(&Graph::new(), &frame, Direction::LR, &unicode_chars());
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::CrowdedEdgeLabel));
    }

    #[test]
    fn analyze_reports_route_crossing_node_interior() {
        let mut graph = Graph::new();
        let mut node = Node::new("A", "A");
        node.x = 0;
        node.y = 0;
        node.width = 5;
        graph.add_node(node);

        let mut cells = vec![CellMeta::default(); 15];
        cells[7] = CellMeta {
            ch: '│',
            owner_kind: CellOwnerKind::EdgeSegment,
            owner_id: Some("edge:0:X->A".to_string()),
            role: CellRole::Vertical,
            z_index: 5,
        };
        let frame = SemanticFrame {
            width: 5,
            height: 3,
            cells,
        };

        let report = analyze(&graph, &frame, Direction::TD, &unicode_chars());
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::RouteCrossesNodeInterior));
    }

    #[test]
    fn analyze_reports_subgraph_title_corruption() {
        let mut graph = Graph::new();
        let mut subgraph = Subgraph::new("sg", Some("Svc".to_string()));
        subgraph.bounds = Rectangle {
            x: 0,
            y: 0,
            width: 11,
            height: 3,
        };
        graph.add_subgraph(subgraph);

        let frame = SemanticFrame {
            width: 11,
            height: 3,
            cells: "[  ─vc  ]  "
                .chars()
                .map(|ch| CellMeta {
                    ch,
                    owner_kind: CellOwnerKind::SubgraphTitle,
                    owner_id: Some("sg".to_string()),
                    role: CellRole::Text,
                    z_index: 2,
                })
                .chain(std::iter::repeat_n(CellMeta::default(), 22))
                .collect(),
        };

        let report = analyze(&graph, &frame, Direction::TD, &unicode_chars());
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::SubgraphTitleCorrupted));
    }

    #[test]
    fn analyze_does_not_report_subgraph_title_corruption_for_title_text_with_v() {
        let mut graph = Graph::new();
        let mut subgraph = Subgraph::new("sg", Some("Service".to_string()));
        let title_fmt = "[  Service  ]";
        subgraph.bounds = Rectangle {
            x: 0,
            y: 0,
            width: title_fmt.chars().count().max(3),
            height: 3,
        };
        graph.add_subgraph(subgraph);

        let title_row: Vec<CellMeta> = title_fmt
            .chars()
            .map(|ch| CellMeta {
                ch,
                owner_kind: CellOwnerKind::SubgraphTitle,
                owner_id: Some("sg".to_string()),
                role: CellRole::Text,
                z_index: 2,
            })
            .collect();
        let frame = SemanticFrame {
            width: title_fmt.chars().count(),
            height: 3,
            cells: title_row
                .into_iter()
                .chain(std::iter::repeat_n(
                    CellMeta::default(),
                    title_fmt.chars().count() * 2,
                ))
                .collect(),
        };

        let report = analyze(&graph, &frame, Direction::TD, &unicode_chars());
        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::SubgraphTitleCorrupted));
    }

    #[test]
    fn analyze_reports_route_topology_mismatch_for_wrong_corner() {
        let mut cells = vec![CellMeta::default(); 9];
        cells[4] = CellMeta {
            ch: '┘',
            owner_kind: CellOwnerKind::CycleEdge,
            owner_id: Some("edge:0:A->A".to_string()),
            role: CellRole::Corner,
            z_index: 5,
        };
        cells[5] = CellMeta {
            ch: '─',
            owner_kind: CellOwnerKind::CycleEdge,
            owner_id: Some("edge:0:A->A".to_string()),
            role: CellRole::Horizontal,
            z_index: 5,
        };
        cells[7] = CellMeta {
            ch: '│',
            owner_kind: CellOwnerKind::CycleEdge,
            owner_id: Some("edge:0:A->A".to_string()),
            role: CellRole::Vertical,
            z_index: 5,
        };
        let frame = SemanticFrame {
            width: 3,
            height: 3,
            cells,
        };

        let report = analyze(&Graph::new(), &frame, Direction::LR, &unicode_chars());
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::RouteTopologyMismatch));
    }

    #[test]
    fn analyze_reports_edge_label_collision_with_node() {
        let mut graph = Graph::new();
        let mut node = Node::new("A", "A");
        node.x = 0;
        node.y = 0;
        node.width = 7;
        node.height = 3;
        graph.add_node(node);

        // Place an edge label cell inside the node bounding box (x=3, y=1).
        let mut cells = vec![CellMeta::default(); 7 * 3];
        cells[1 * 7 + 3] = CellMeta {
            ch: 'X',
            owner_kind: CellOwnerKind::EdgeLabel,
            owner_id: Some("edge:0:A->B".to_string()),
            role: CellRole::Text,
            z_index: 6,
        };
        let frame = SemanticFrame {
            width: 7,
            height: 3,
            cells,
        };

        let report = analyze(&graph, &frame, Direction::TD, &unicode_chars());
        assert!(report
            .findings
            .iter()
            .any(|f| f.code == FindingCode::EdgeLabelCollidesWithNode));
        let collision = report
            .findings
            .iter()
            .find(|f| f.code == FindingCode::EdgeLabelCollidesWithNode)
            .unwrap();
        assert_eq!(collision.owner_ids, vec!["edge:0:A->B"]);
        assert_eq!(collision.cells, vec![(3, 1)]);
    }

    #[test]
    fn audit_summary_marks_empty_report_clean() {
        let report = CriticReport {
            score: 0,
            findings: Vec::new(),
            notes: Vec::new(),
        };

        let summary = report.audit_summary();
        assert_eq!(summary.verdict, AuditVerdict::Clean);
        assert!(summary.is_clean());
        assert_eq!(summary.highlights.len(), 0);
    }

    #[test]
    fn ascii_plus_corner_is_not_flagged_as_junction_mismatch() {
        let chars = CompositeStyle::default().to_style_chars(BaseStyle::Ascii);
        let frame = SemanticFrame {
            width: 2,
            height: 2,
            cells: vec![
                CellMeta {
                    ch: '+',
                    owner_kind: CellOwnerKind::Junction,
                    owner_id: Some("edge:0:A->B".to_string()),
                    role: CellRole::Junction,
                    z_index: 5,
                },
                CellMeta {
                    ch: '-',
                    owner_kind: CellOwnerKind::EdgeSegment,
                    owner_id: Some("edge:0:A->B".to_string()),
                    role: CellRole::Horizontal,
                    z_index: 5,
                },
                CellMeta {
                    ch: '|',
                    owner_kind: CellOwnerKind::EdgeSegment,
                    owner_id: Some("edge:0:A->B".to_string()),
                    role: CellRole::Vertical,
                    z_index: 5,
                },
                CellMeta::default(),
            ],
        };

        let report = analyze(&Graph::new(), &frame, Direction::TD, &chars);
        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::JunctionTopologyMismatch));
    }

    #[test]
    fn analyze_reports_route_symmetry_imbalance_for_skewed_fanout() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;

        let mut a = Node::new("A", "A");
        a.x = 8;
        a.y = 0;
        a.width = 5;
        let mut b = Node::new("B", "B");
        b.x = 0;
        b.y = 8;
        b.width = 5;
        let mut c = Node::new("C", "C");
        c.x = 20;
        c.y = 8;
        c.width = 5;

        graph.add_node(a);
        graph.add_node(b);
        graph.add_node(c);
        graph.add_edge(crate::graph::Edge::new("A", "B"));
        graph.add_edge(crate::graph::Edge::new("A", "C"));

        let report = analyze(
            &graph,
            &SemanticFrame::default(),
            Direction::TD,
            &unicode_chars(),
        );

        assert!(report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::RouteSymmetryImbalance));
    }

    #[test]
    fn analyze_ignores_balanced_crossing_permutation_rows() {
        let mut graph = Graph::new();
        graph.direction = Direction::BT;

        let mut a1 = Node::new("A1", "Node A1");
        a1.x = 8;
        a1.y = 14;
        a1.width = 13;
        let mut a2 = Node::new("A2", "Node A2");
        a2.x = 25;
        a2.y = 14;
        a2.width = 13;
        let mut a3 = Node::new("A3", "Node A3");
        a3.x = 42;
        a3.y = 14;
        a3.width = 13;

        let mut b3 = Node::new("B3", "Node B3");
        b3.x = 0;
        b3.y = 7;
        b3.width = 13;
        let mut b2 = Node::new("B2", "Node B2");
        b2.x = 17;
        b2.y = 7;
        b2.width = 13;
        let mut b1 = Node::new("B1", "Node B1");
        b1.x = 34;
        b1.y = 7;
        b1.width = 13;

        let mut c1 = Node::new("C1", "Node C1");
        c1.x = 8;
        c1.y = 0;
        c1.width = 13;
        let mut c2 = Node::new("C2", "Node C2");
        c2.x = 25;
        c2.y = 0;
        c2.width = 13;
        let mut c3 = Node::new("C3", "Node C3");
        c3.x = 42;
        c3.y = 0;
        c3.width = 13;

        graph.add_node(a1);
        graph.add_node(a2);
        graph.add_node(a3);
        graph.add_node(b3);
        graph.add_node(b2);
        graph.add_node(b1);
        graph.add_node(c1);
        graph.add_node(c2);
        graph.add_node(c3);

        graph.add_edge(crate::graph::Edge::new("A1", "B2"));
        graph.add_edge(crate::graph::Edge::new("A1", "B3"));
        graph.add_edge(crate::graph::Edge::new("A2", "B1"));
        graph.add_edge(crate::graph::Edge::new("A2", "B3"));
        graph.add_edge(crate::graph::Edge::new("A3", "B1"));
        graph.add_edge(crate::graph::Edge::new("A3", "B2"));

        graph.add_edge(crate::graph::Edge::new("B1", "C2"));
        graph.add_edge(crate::graph::Edge::new("B1", "C3"));
        graph.add_edge(crate::graph::Edge::new("B2", "C1"));
        graph.add_edge(crate::graph::Edge::new("B2", "C3"));
        graph.add_edge(crate::graph::Edge::new("B3", "C1"));
        graph.add_edge(crate::graph::Edge::new("B3", "C2"));

        let report = analyze(
            &graph,
            &SemanticFrame::default(),
            Direction::BT,
            &unicode_chars(),
        );

        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::RouteSymmetryImbalance));
    }

    #[test]
    fn analyze_reports_branch_spacing_imbalance_for_uneven_fanout() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;

        let mut a = Node::new("A", "A");
        a.x = 20;
        a.y = 0;
        a.width = 9;

        let mut b = Node::new("B", "B");
        b.x = 0;
        b.y = 8;
        b.width = 7;

        let mut c = Node::new("C", "C");
        c.x = 12;
        c.y = 8;
        c.width = 7;

        let mut d = Node::new("D", "D");
        d.x = 42;
        d.y = 8;
        d.width = 7;

        graph.add_node(a);
        graph.add_node(b);
        graph.add_node(c);
        graph.add_node(d);
        graph.add_edge(crate::graph::Edge::new("A", "B"));
        graph.add_edge(crate::graph::Edge::new("A", "C"));
        graph.add_edge(crate::graph::Edge::new("A", "D"));

        let report = analyze(
            &graph,
            &SemanticFrame::default(),
            Direction::TD,
            &unicode_chars(),
        );

        assert!(report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::BranchSpacingImbalance));
        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::RouteSymmetryImbalance));
    }

    #[test]
    fn analyze_does_not_report_branch_spacing_imbalance_for_even_fanout() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;

        let mut a = Node::new("A", "A");
        a.x = 20;
        a.y = 0;
        a.width = 9;

        let mut b = Node::new("B", "B");
        b.x = 0;
        b.y = 8;
        b.width = 7;

        let mut c = Node::new("C", "C");
        c.x = 21;
        c.y = 8;
        c.width = 7;

        let mut d = Node::new("D", "D");
        d.x = 42;
        d.y = 8;
        d.width = 7;

        graph.add_node(a);
        graph.add_node(b);
        graph.add_node(c);
        graph.add_node(d);
        graph.add_edge(crate::graph::Edge::new("A", "B"));
        graph.add_edge(crate::graph::Edge::new("A", "C"));
        graph.add_edge(crate::graph::Edge::new("A", "D"));

        let report = analyze(
            &graph,
            &SemanticFrame::default(),
            Direction::TD,
            &unicode_chars(),
        );

        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::BranchSpacingImbalance));
    }

    #[test]
    fn analyze_reports_branch_crowding_for_dense_fanout() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;

        let mut a = Node::new("A", "A");
        a.x = 12;
        a.y = 0;
        a.width = 9;

        let mut b = Node::new("B", "B");
        b.x = 4;
        b.y = 8;
        b.width = 7;

        let mut c = Node::new("C", "C");
        c.x = 11;
        c.y = 8;
        c.width = 7;

        let mut d = Node::new("D", "D");
        d.x = 18;
        d.y = 8;
        d.width = 7;

        graph.add_node(a);
        graph.add_node(b);
        graph.add_node(c);
        graph.add_node(d);
        graph.add_edge(crate::graph::Edge::new("A", "B"));
        graph.add_edge(crate::graph::Edge::new("A", "C"));
        graph.add_edge(crate::graph::Edge::new("A", "D"));

        let report = analyze(
            &graph,
            &SemanticFrame::default(),
            Direction::TD,
            &unicode_chars(),
        );

        assert!(report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::BranchCrowding));
        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::BranchSpacingImbalance));
    }

    #[test]
    fn analyze_does_not_report_branch_crowding_for_roomy_fanout() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;

        let mut a = Node::new("A", "A");
        a.x = 20;
        a.y = 0;
        a.width = 9;

        let mut b = Node::new("B", "B");
        b.x = 0;
        b.y = 8;
        b.width = 7;

        let mut c = Node::new("C", "C");
        c.x = 16;
        c.y = 8;
        c.width = 7;

        let mut d = Node::new("D", "D");
        d.x = 32;
        d.y = 8;
        d.width = 7;

        graph.add_node(a);
        graph.add_node(b);
        graph.add_node(c);
        graph.add_node(d);
        graph.add_edge(crate::graph::Edge::new("A", "B"));
        graph.add_edge(crate::graph::Edge::new("A", "C"));
        graph.add_edge(crate::graph::Edge::new("A", "D"));

        let report = analyze(
            &graph,
            &SemanticFrame::default(),
            Direction::TD,
            &unicode_chars(),
        );

        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::BranchCrowding));
    }
}
