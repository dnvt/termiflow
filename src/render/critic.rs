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

#[path = "critic_report.rs"]
mod report;

#[path = "critic_branches.rs"]
mod branches;

pub use report::{
    AuditSummary, AuditVerdict, CriticFinding, CriticReport, FindingCode, FindingSeverity,
};

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
    findings.extend(branches::find_route_symmetry_imbalances(graph, direction));
    findings.extend(branches::find_branch_spacing_imbalances(graph, direction));
    findings.extend(branches::find_branch_crowding(graph, direction));
    findings.extend(find_unused_portal_openings(graph, frame));
    findings.extend(find_arrow_without_visible_shaft(frame));
    findings.extend(find_arrow_touching_node_borders(graph, frame));
    findings.extend(find_arrow_touching_subgraph_borders(graph, frame));
    findings.extend(find_subgraph_border_portal_artifacts(
        graph, frame, direction, chars,
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
            if is_clean_horizontal_side_portal(frame, x, y, cell.ch, chars) {
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

fn is_clean_horizontal_side_portal(
    frame: &SemanticFrame,
    x: usize,
    y: usize,
    ch: char,
    chars: &StyleChars,
) -> bool {
    // LR/RL side-wall portals are only clean if the border cell itself stays a
    // horizontal opening. Junction glyphs still imply topology living on the
    // wall, which is exactly the artifact this oracle is meant to catch.
    if !super::canvas::is_horizontal(ch, chars) {
        return false;
    }

    let left = x
        .checked_sub(1)
        .and_then(|xx| frame.get(xx, y))
        .is_some_and(|cell| char_connects_right(cell.ch));
    let right = frame
        .get(x.saturating_add(1), y)
        .is_some_and(|cell| char_connects_left(cell.ch));
    if !left && !right {
        return false;
    }

    let up = y
        .checked_sub(1)
        .and_then(|yy| frame.get(x, yy))
        .filter(|cell| char_connects_down(cell.ch));
    let down = frame
        .get(x, y.saturating_add(1))
        .filter(|cell| char_connects_up(cell.ch));

    let vertical_neighbors_are_only_subgraph_borders = [up, down]
        .into_iter()
        .flatten()
        .all(|cell| cell.owner_kind == CellOwnerKind::SubgraphBorder);

    vertical_neighbors_are_only_subgraph_borders && (up.is_some() || down.is_some())
}

fn is_compact_horizontal_portal_arrow(
    frame: &SemanticFrame,
    x: usize,
    y: usize,
    ch: char,
    subgraph: &crate::graph::Subgraph,
) -> bool {
    if !matches!(ch, '>' | '→' | '▶' | '<' | '←' | '◀') {
        return false;
    }
    if x != subgraph.bounds.x && x != subgraph.bounds.x + subgraph.bounds.width.saturating_sub(1) {
        return false;
    }

    let horizontal_neighbor = match ch {
        '>' | '→' | '▶' => x
            .checked_sub(1)
            .and_then(|xx| frame.get(xx, y))
            .is_some_and(|cell| char_connects_right(cell.ch)),
        '<' | '←' | '◀' => frame
            .get(x.saturating_add(1), y)
            .is_some_and(|cell| char_connects_left(cell.ch)),
        _ => false,
    };
    if !horizontal_neighbor {
        return false;
    }

    let up = y
        .checked_sub(1)
        .and_then(|yy| frame.get(x, yy))
        .filter(|cell| char_connects_down(cell.ch));
    let down = frame
        .get(x, y.saturating_add(1))
        .filter(|cell| char_connects_up(cell.ch));

    [up, down]
        .into_iter()
        .flatten()
        .all(|cell| cell.owner_kind == CellOwnerKind::SubgraphBorder)
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
    if cell.owner_kind == CellOwnerKind::PortalOpening {
        return true;
    }
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
                if on_border
                    && !is_compact_horizontal_portal_arrow(frame, x, y, cell.ch, subgraph)
                    && !arrow_uses_subgraph_border_pierce(frame, x, y, cell.ch)
                {
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

        let title_fmt = crate::graph::subgraph_title_text(title);
        let Some(start_x) = crate::graph::subgraph_title_start_x(
            subgraph.bounds.x,
            subgraph.bounds.width,
            title,
            direction,
        ) else {
            continue;
        };
        let title_len = title_fmt.chars().count();
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
    chars: &StyleChars,
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
                if is_clean_horizontal_side_portal(frame, x, y, cell.ch, chars) {
                    continue;
                }
                if cell.owner_kind == CellOwnerKind::PortalOpening {
                    continue;
                }

                let has_horizontal = char_connects_left(cell.ch) || char_connects_right(cell.ch);
                let has_vertical = char_connects_up(cell.ch) || char_connects_down(cell.ch);
                if has_horizontal && has_vertical {
                    // In LR/RL, a side-wall portal must be a clean horizontal
                    // opening. Any glyph that still advertises vertical
                    // topology on the border column implies a merge/junction
                    // living on the wall instead of inside or outside it.
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

    // A plain route segment touching the end of an inline label is expected:
    // labels are drawn on the edge itself. Reserve the finding for stronger
    // topology pressure such as a foreign corner, junction, or arrowhead.
    if foreign_line_neighbors
        .iter()
        .all(|(_, _, role)| matches!(role, CellRole::Horizontal | CellRole::Vertical))
    {
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
        let from_right = from.x.saturating_add(from.width);
        let to_right = to.x.saturating_add(to.width);
        let gap = if from_right <= to.x {
            to.x.saturating_sub(from_right)
        } else if to_right <= from.x {
            from.x.saturating_sub(to_right)
        } else {
            0
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

pub(super) fn node_secondary_center(node: &crate::graph::Node, direction: Direction) -> usize {
    match direction {
        Direction::TD | Direction::TB | Direction::BT => node.center_x(),
        Direction::LR | Direction::RL => node.center_y(),
    }
}

pub(super) fn node_secondary_start(node: &crate::graph::Node, direction: Direction) -> usize {
    match direction {
        Direction::TD | Direction::TB | Direction::BT => node.x,
        Direction::LR | Direction::RL => node.y,
    }
}

pub(super) fn node_secondary_end(node: &crate::graph::Node, direction: Direction) -> usize {
    match direction {
        Direction::TD | Direction::TB | Direction::BT => node.x + node.width,
        Direction::LR | Direction::RL => node.y + node.height.max(crate::style::BOX_HEIGHT),
    }
}

pub(super) fn node_primary_center(node: &crate::graph::Node, direction: Direction) -> usize {
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
mod tests;
