//! Independent raw-frame route-clarity evidence for visual-audit packets.
//!
//! This module deliberately does not consume semantic frames, GeometryTrace,
//! fallback route plans, provenance, critic findings, or review decisions. It
//! derives a conservative risk result from the input graph, measured node
//! geometry, and the characters in the emitted frame. A risk is evidence that
//! keeps a row in the one-frame queue; it is not perceptual approval.

use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use super::common;
#[cfg(test)]
use termiflow::RenderOptions;
use termiflow::{
    layout_and_render_with_feedback, measure, parse, BaseStyle, CompositeStyle, Config,
};

pub const SCHEMA: &str = "termiflow.route_clarity.v1";

const ROUTE_NONE: u8 = 4;
const STATUSES: &[&str] = &["risk", "inconclusive", "clean", "not_applicable"];
const SEVERITIES: &[&str] = &["P0", "P1", "P2", "P3"];

pub fn analyze(input: &[u8], frame: &[u8], style: &str, mode: &str) -> Result<Value> {
    let source = std::str::from_utf8(input).context("route-clarity input is not UTF-8")?;
    let frame = std::str::from_utf8(frame).context("route-clarity frame is not UTF-8")?;
    let base_style = style
        .parse::<BaseStyle>()
        .map_err(|_| anyhow!("unsupported route-clarity style: {style}"))?;
    let optimized = match mode {
        "default" => false,
        "optimized" => true,
        other => return Err(anyhow!("unsupported route-clarity mode: {other}")),
    };

    let parsed = parse(source, false).context("parse route-clarity input")?;
    let mut graph = parsed.graph;
    // The packet renderer honors in-file directives (for example, intentional
    // label wrapping). Resolve the same directive layer before measuring so
    // the independent label check compares like with like.
    let mut config = Config::from_parse_config(&parsed.config);
    config.optimize_render = optimized;
    config.composite_style = CompositeStyle::from_base(base_style);
    config.spacing = config.spacing.for_direction(graph.direction);
    measure::measure_graph(&mut graph, &config);
    let (graph, _) = layout_and_render_with_feedback(graph, config)
        .context("measure route-clarity graph geometry")?;

    let mut findings = Vec::new();
    if frame.trim().is_empty() {
        findings.push(finding(
            "raw_frame_empty",
            "P1",
            "the emitted frame is empty",
            Vec::new(),
        ));
    }
    for node in &graph.nodes {
        if node.label.is_empty() {
            continue;
        }

        // Compare the frame with the renderer's measured visual lines rather
        // than the unwrapped source label. A wrapped label is intentionally
        // non-contiguous in the frame, and a bounded label intentionally ends
        // in an ellipsis; neither is a missing human-visible label.
        let rendered_lines: Vec<&str> = if node.label_lines.is_empty() {
            vec![node.label.as_str()]
        } else {
            node.label_lines
                .iter()
                .map(String::as_str)
                .filter(|line| !line.is_empty())
                .collect()
        };
        if rendered_lines.iter().any(|line| !frame.contains(line)) {
            findings.push(finding(
                "raw_label_missing",
                "P1",
                format!(
                    "rendered label lines for node {:?} are absent from the emitted frame",
                    node.id
                ),
                Vec::new(),
            ));
        }
    }

    if dual_junction_identity_family(&graph) {
        findings.extend(dual_junction_findings(&graph, frame));
    } else if dedicated_fan_in_identity_family(&graph) {
        findings.extend(dedicated_fan_in_findings(&graph, frame));
    } else if nonterminal_vertical_fan_in_identity_family(&graph) {
        findings.extend(nonterminal_vertical_fan_in_findings(&graph, frame));
    } else if ordinary_fan_in_identity_family(&graph) {
        findings.extend(ordinary_fan_in_findings(&graph, frame));
    } else {
        findings.extend(dense_route_findings(&graph, frame));
    }
    findings.extend(subgraph_portal_findings(&graph, frame, base_style));
    findings.extend(bt_title_boundary_hook_findings(
        &graph,
        frame,
        origin(&graph),
    ));
    findings.extend(bt_boundary_rail_findings(&graph, frame, origin(&graph)));
    findings.sort_by(|left, right| {
        left["code"]
            .as_str()
            .cmp(&right["code"].as_str())
            .then_with(|| left["message"].as_str().cmp(&right["message"].as_str()))
    });

    let dense = dense_crossing_input(&graph);
    let dedicated_fan_in = dedicated_fan_in_identity_family(&graph);
    let dual_junction = dual_junction_identity_family(&graph);
    let nonterminal_vertical_fan_in = nonterminal_vertical_fan_in_identity_family(&graph);
    let ordinary_fan_in = ordinary_fan_in_identity_family(&graph);
    let has_subgraphs = !graph.subgraphs.is_empty();
    let status = if findings.is_empty() {
        if dense
            || dedicated_fan_in
            || dual_junction
            || nonterminal_vertical_fan_in
            || ordinary_fan_in
            || has_subgraphs
        {
            "clean"
        } else {
            "not_applicable"
        }
    } else if findings.iter().any(|item| {
        matches!(
            item["code"].as_str(),
            Some("raw_frame_empty")
                | Some("raw_label_missing")
                | Some("declared_edge_missing")
                | Some("undeclared_target_reachable")
                | Some("subgraph_portal_missing_raw_contact")
        )
    }) {
        "risk"
    } else {
        "inconclusive"
    };

    Ok(json!({
        "schema": SCHEMA,
        "status": status,
        "source_sha256": common::sha256_bytes(input),
        "frame_sha256": common::sha256_bytes(frame.as_bytes()),
        "style": style,
        "mode": mode,
        "topology": {
            "nodes": graph.nodes.len(),
            "edges": graph.edges.len(),
            "subgraphs": graph.subgraphs.len(),
            "dense_crossing_candidate": dense,
        },
        "findings": findings,
    }))
}

/// Validate the report-to-row join without consulting any renderer-owned
/// evidence. The caller supplies the exact input and emitted frame bytes, so
/// a copied, stale, or hand-edited report cannot silently travel with a row.
pub fn validate_report(
    report: &Value,
    input: &[u8],
    frame: &[u8],
    style: &str,
    mode: &str,
    label: &str,
) -> Result<()> {
    if report.get("schema").and_then(Value::as_str) != Some(SCHEMA) {
        bail!("{label}: route-clarity schema must be {SCHEMA}");
    }
    let status = report
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("{label}: route-clarity status is missing"))?;
    if !STATUSES.contains(&status) {
        bail!("{label}: unsupported route-clarity status: {status}");
    }
    if report.get("source_sha256").and_then(Value::as_str)
        != Some(common::sha256_bytes(input).as_str())
    {
        bail!("{label}: route-clarity source hash is stale");
    }
    if report.get("frame_sha256").and_then(Value::as_str)
        != Some(common::sha256_bytes(frame).as_str())
    {
        bail!("{label}: route-clarity frame hash is stale");
    }
    if report.get("style").and_then(Value::as_str) != Some(style) {
        bail!("{label}: route-clarity style does not match the row");
    }
    if report.get("mode").and_then(Value::as_str) != Some(mode) {
        bail!("{label}: route-clarity mode does not match the row");
    }

    let topology = report
        .get("topology")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("{label}: route-clarity topology must be an object"))?;
    for field in ["nodes", "edges", "subgraphs"] {
        if !topology.get(field).is_some_and(Value::is_u64) {
            bail!("{label}: route-clarity topology.{field} must be an integer");
        }
    }
    if !topology
        .get("dense_crossing_candidate")
        .is_some_and(Value::is_boolean)
    {
        bail!("{label}: route-clarity topology.dense_crossing_candidate must be boolean");
    }

    let findings = report
        .get("findings")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("{label}: route-clarity findings must be an array"))?;
    for finding in findings {
        let finding = finding
            .as_object()
            .ok_or_else(|| anyhow!("{label}: route-clarity finding must be an object"))?;
        for field in ["code", "severity", "message"] {
            if finding
                .get(field)
                .and_then(Value::as_str)
                .is_none_or(|value| value.trim().is_empty())
            {
                bail!("{label}: route-clarity finding.{field} must be non-empty");
            }
        }
        if !SEVERITIES.contains(&finding["severity"].as_str().unwrap_or_default()) {
            bail!(
                "{label}: unsupported route-clarity finding severity: {}",
                finding["severity"]
            );
        }
        let cells = finding
            .get("cells")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("{label}: route-clarity finding.cells must be an array"))?;
        for cell in cells {
            if !cell.is_object()
                || !cell.get("x").is_some_and(Value::is_u64)
                || !cell.get("y").is_some_and(Value::is_u64)
            {
                bail!("{label}: route-clarity finding cell must contain integer x and y");
            }
        }
    }
    if matches!(status, "clean" | "not_applicable") && !findings.is_empty() {
        bail!("{label}: route-clarity {status} report contains findings");
    }
    if matches!(status, "risk" | "inconclusive") && findings.is_empty() {
        bail!("{label}: route-clarity {status} report has no findings");
    }
    Ok(())
}

fn finding(code: &str, severity: &str, message: impl Into<String>, cells: Vec<Value>) -> Value {
    json!({
        "code": code,
        "severity": severity,
        "message": message.into(),
        "cells": cells,
    })
}

fn cell(x: usize, y: usize) -> Value {
    json!({"x": x, "y": y})
}

fn dense_crossing_input(graph: &termiflow::Graph) -> bool {
    if !graph.subgraphs.is_empty()
        || graph
            .nodes
            .iter()
            .any(|node| node.shape != termiflow::graph::NodeShape::Rectangle)
        || graph.edges.iter().any(|edge| {
            edge.is_back_edge
                || edge.label.is_some()
                || edge.kind != termiflow::graph::EdgeKind::Arrow
        })
    {
        return false;
    }

    let source_count = graph
        .nodes
        .iter()
        .filter(|node| {
            graph
                .edges
                .iter()
                .any(|edge| !edge.is_back_edge && edge.from == node.id)
        })
        .count();
    let target_count = graph
        .nodes
        .iter()
        .filter(|node| {
            graph
                .edges
                .iter()
                .any(|edge| !edge.is_back_edge && edge.to == node.id)
        })
        .count();
    let fanout_count = graph
        .nodes
        .iter()
        .filter(|node| {
            graph
                .edges
                .iter()
                .filter(|edge| !edge.is_back_edge && edge.from == node.id)
                .count()
                >= 2
        })
        .count();
    let fanin_count = graph
        .nodes
        .iter()
        .filter(|node| {
            graph
                .edges
                .iter()
                .filter(|edge| !edge.is_back_edge && edge.to == node.id)
                .count()
                >= 2
        })
        .count();

    source_count >= 3 && target_count >= 3 && fanout_count >= 2 && fanin_count >= 2
}

fn dense_route_findings(graph: &termiflow::Graph, frame: &str) -> Vec<Value> {
    if !dense_crossing_input(graph) {
        return Vec::new();
    }

    let origin_x = graph.nodes.iter().map(|node| node.x).min().unwrap_or(0);
    let origin_y = graph.nodes.iter().map(|node| node.y).min().unwrap_or(0);
    let mut findings = Vec::new();

    for source in &graph.nodes {
        let mut outgoing: Vec<_> = graph
            .edges
            .iter()
            .filter(|edge| !edge.is_back_edge && edge.from == source.id)
            .collect();
        let Some(first_target) = outgoing.first().and_then(|edge| graph.get_node(&edge.to)) else {
            continue;
        };
        outgoing.sort_by_key(|edge| {
            graph
                .get_node(&edge.to)
                .map(|node| match graph.direction {
                    termiflow::graph::Direction::TD
                    | termiflow::graph::Direction::TB
                    | termiflow::graph::Direction::BT => node.center_x(),
                    termiflow::graph::Direction::LR | termiflow::graph::Direction::RL => {
                        node.center_y()
                    }
                })
                .unwrap_or_default()
        });
        let source_ports = dense_route_source_ports(source, graph.direction, outgoing.len());
        if source_ports.len() != outgoing.len() {
            findings.push(finding(
                "route_corridor_unmeasurable",
                "P2",
                format!(
                    "dense source {} has {} outgoing edges but {} raw ports",
                    source.id,
                    outgoing.len(),
                    source_ports.len()
                ),
                Vec::new(),
            ));
            continue;
        }
        let candidates: Vec<_> = graph
            .nodes
            .iter()
            .filter(|node| node.rank == first_target.rank)
            .collect();
        let Some(primary_range) =
            route_identity_primary_range(source, first_target, graph.direction)
        else {
            findings.push(finding(
                "route_corridor_unmeasurable",
                "P2",
                format!("route corridor for {} has no measurable span", source.id),
                Vec::new(),
            ));
            continue;
        };
        let declared_targets: HashSet<&str> =
            outgoing.iter().map(|edge| edge.to.as_str()).collect();
        let mut reachable_targets = HashSet::new();
        for source_secondary in source_ports {
            let source_start = dense_route_source_attachment(
                source,
                graph.direction,
                source_secondary,
                origin_x,
                origin_y,
            );
            reachable_targets.extend(route_identity_reachable_targets(
                frame,
                graph,
                source_start,
                &candidates,
                primary_range,
                origin_x,
                origin_y,
            ));
        }
        for target in candidates {
            let is_declared = declared_targets.contains(target.id.as_str());
            let is_reachable = reachable_targets.contains(&target.id);
            let target_point =
                dense_route_target_attachments(target, graph.direction, origin_x, origin_y)
                    .first()
                    .map(|(point, _)| *point)
                    .unwrap_or((0, 0));
            if is_declared && !is_reachable {
                findings.push(finding(
                    "declared_edge_missing",
                    "P2",
                    format!(
                        "declared edge {} -> {} has no raw-frame continuation",
                        source.id, target.id
                    ),
                    vec![cell(target_point.0, target_point.1)],
                ));
            } else if !is_declared && is_reachable {
                findings.push(finding(
                    "undeclared_target_reachable",
                    "P2",
                    format!(
                        "source {} can physically continue to undeclared target {} through the same raw corridor",
                        source.id, target.id
                    ),
                    vec![cell(target_point.0, target_point.1)],
                ));
            }
        }
    }
    findings
}

/// Check the target-side ports of the narrow fan-in families independently of
/// renderer ownership metadata. A clean route report must show one directional
/// arrow at every structurally required target port; otherwise the row stays a
/// P1 visual-review failure even when the underlying edge trace is complete.
fn dedicated_fan_in_findings(graph: &termiflow::Graph, frame: &str) -> Vec<Value> {
    if !dedicated_fan_in_identity_family(graph) {
        return Vec::new();
    }

    let (origin_x, origin_y) = origin(graph);
    let lines: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
    let mut findings = Vec::new();
    for target in &graph.nodes {
        let count = dedicated_target_port_count(graph, &target.id);
        if count < 2 {
            continue;
        }
        // Shape-owned contours can reserve a bridge cell between the visible
        // border and the actual arrowhead.  Probe the same entry contract as
        // the renderer so a database/cylinder arrow is not falsely reported
        // missing at its contour-adjacent shaft cell.
        let clearance = target_entry_clearance(target.shape, graph.direction);
        let rows = (0..count)
            .map(|index| target.y.saturating_add(1 + index.saturating_mul(2)))
            .collect::<Vec<_>>();
        let missing: Vec<(usize, usize)> = rows
            .into_iter()
            .filter_map(|row| {
                let (x, expected) = match graph.direction {
                    termiflow::graph::Direction::LR => (
                        target.x.saturating_sub(1 + clearance),
                        ['>', '→', '▶'].as_slice(),
                    ),
                    termiflow::graph::Direction::RL => (
                        target
                            .x
                            .saturating_add(target.width)
                            .saturating_add(clearance),
                        ['<', '←', '◀'].as_slice(),
                    ),
                    _ => return None,
                };
                let frame_x = x.saturating_sub(origin_x);
                let frame_y = row.saturating_sub(origin_y);
                let actual = lines
                    .get(frame_y)
                    .and_then(|line| line.get(frame_x))
                    .copied();
                (!actual.is_some_and(|glyph| expected.contains(&glyph)))
                    .then_some((frame_x, frame_y))
            })
            .collect();
        if !missing.is_empty() {
            findings.push(finding(
                "declared_edge_missing",
                "P1",
                format!(
                    "dedicated fan-in target {} is missing {} of {} visible incoming arrowheads",
                    target.id,
                    missing.len(),
                    count
                ),
                missing.into_iter().map(|(x, y)| cell(x, y)).collect(),
            ));
        }
    }
    findings
}

/// Check the separated top/bottom ports of the exact nonterminal vertical
/// fan-in scene. This remains independent of renderer ownership metadata so a
/// visually collapsed pair of arrows cannot be hidden by a complete trace.
fn nonterminal_vertical_fan_in_findings(graph: &termiflow::Graph, frame: &str) -> Vec<Value> {
    if !nonterminal_vertical_fan_in_identity_family(graph) {
        return Vec::new();
    }

    let Some(target) = graph.nodes.iter().find(|target| {
        graph
            .edges
            .iter()
            .filter(|edge| !edge.is_back_edge && edge.to == target.id)
            .count()
            == 2
    }) else {
        return Vec::new();
    };
    let (origin_x, origin_y) = origin(graph);
    let lines: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
    let center = target.x + target.width / 2;
    let ports = [center.saturating_sub(1), center.saturating_add(1)];
    let (row, expected) = match graph.direction {
        termiflow::graph::Direction::TD => (target.y.saturating_sub(1), ['v', '↓', '▼'].as_slice()),
        termiflow::graph::Direction::BT => (target.bottom_y(), ['^', '↑', '▲'].as_slice()),
        _ => return Vec::new(),
    };
    let missing: Vec<(usize, usize)> = ports
        .into_iter()
        .filter_map(|x| {
            let frame_x = x.saturating_sub(origin_x);
            let frame_y = row.saturating_sub(origin_y);
            let actual = lines
                .get(frame_y)
                .and_then(|line| line.get(frame_x))
                .copied();
            (!actual.is_some_and(|glyph| expected.contains(&glyph))).then_some((frame_x, frame_y))
        })
        .collect();
    if missing.is_empty() {
        Vec::new()
    } else {
        vec![finding(
            "declared_edge_missing",
            "P1",
            format!(
                "nonterminal vertical fan-in target {} is missing {} of 2 visible incoming arrowheads",
                target.id,
                missing.len()
            ),
            missing.into_iter().map(|(x, y)| cell(x, y)).collect(),
        )]
    }
}

/// Check every target-side port in the ordinary identity fan-in family. This
/// is intentionally raw-frame-only: a complete semantic edge trace cannot
/// excuse a collapsed target arrowhead.
fn ordinary_fan_in_findings(graph: &termiflow::Graph, frame: &str) -> Vec<Value> {
    if !ordinary_fan_in_identity_family(graph) {
        return Vec::new();
    }

    let (origin_x, origin_y) = origin(graph);
    let lines: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
    let mut findings = Vec::new();
    for target in &graph.nodes {
        let count = ordinary_target_port_count(graph, &target.id);
        if count < 2 {
            continue;
        }

        let (ports, expected): (Vec<(usize, usize)>, &[char]) = match graph.direction {
            termiflow::graph::Direction::TD | termiflow::graph::Direction::TB => {
                let center = target.center_x();
                let start = center.saturating_sub(count.saturating_sub(1));
                (
                    (0..count)
                        .map(|index| {
                            (
                                start.saturating_add(index.saturating_mul(2)),
                                target.y.saturating_sub(1),
                            )
                        })
                        .collect(),
                    &['v', '↓', '▼'],
                )
            }
            termiflow::graph::Direction::BT => {
                let center = target.center_x();
                let start = center.saturating_sub(count.saturating_sub(1));
                (
                    (0..count)
                        .map(|index| {
                            (
                                start.saturating_add(index.saturating_mul(2)),
                                target.bottom_y(),
                            )
                        })
                        .collect(),
                    &['^', '↑', '▲'],
                )
            }
            termiflow::graph::Direction::LR => (
                (0..count)
                    .map(|index| {
                        (
                            target.x.saturating_sub(1),
                            target.y.saturating_add(1 + index.saturating_mul(2)),
                        )
                    })
                    .collect(),
                &['>', '→', '▶'],
            ),
            termiflow::graph::Direction::RL => (
                (0..count)
                    .map(|index| {
                        (
                            target.x.saturating_add(target.width),
                            target.y.saturating_add(1 + index.saturating_mul(2)),
                        )
                    })
                    .collect(),
                &['<', '←', '◀'],
            ),
        };

        let missing: Vec<(usize, usize)> = ports
            .into_iter()
            .filter_map(|(x, y)| {
                let frame_x = x.saturating_sub(origin_x);
                let frame_y = y.saturating_sub(origin_y);
                let actual = lines
                    .get(frame_y)
                    .and_then(|line| line.get(frame_x))
                    .copied();
                (!actual.is_some_and(|glyph| expected.contains(&glyph)))
                    .then_some((frame_x, frame_y))
            })
            .collect();
        if !missing.is_empty() {
            findings.push(finding(
                "declared_edge_missing",
                "P1",
                format!(
                    "ordinary fan-in target {} is missing {} of {} visible incoming arrowheads",
                    target.id,
                    missing.len(),
                    count
                ),
                missing.into_iter().map(|(x, y)| cell(x, y)).collect(),
            ));
        }
    }
    findings
}

/// Check both incoming target ports and the total arrow count for the exact
/// two-in/two-out dual-junction family. This is independent of renderer
/// ownership metadata so a shared collector cannot hide a missing edge.
fn dual_junction_findings(graph: &termiflow::Graph, frame: &str) -> Vec<Value> {
    let Some(target) = dual_junction_target(graph) else {
        return Vec::new();
    };

    let (origin_x, origin_y) = origin(graph);
    let lines: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
    let (ports, expected): (Vec<(usize, usize)>, &[char]) = match graph.direction {
        termiflow::graph::Direction::TD => (
            vec![
                (
                    target.center_x().saturating_sub(1),
                    target.y.saturating_sub(1),
                ),
                (
                    target.center_x().saturating_add(1),
                    target.y.saturating_sub(1),
                ),
            ],
            &['v', '↓', '▼'],
        ),
        termiflow::graph::Direction::BT => (
            vec![
                (target.center_x().saturating_sub(1), target.bottom_y()),
                (target.center_x().saturating_add(1), target.bottom_y()),
            ],
            &['^', '↑', '▲'],
        ),
        termiflow::graph::Direction::LR => (
            vec![
                (target.x.saturating_sub(1), target.y.saturating_add(1)),
                (target.x.saturating_sub(1), target.y.saturating_add(3)),
            ],
            &['>', '→', '▶'],
        ),
        termiflow::graph::Direction::RL => (
            vec![
                (
                    target.x.saturating_add(target.width),
                    target.y.saturating_add(1),
                ),
                (
                    target.x.saturating_add(target.width),
                    target.y.saturating_add(3),
                ),
            ],
            &['<', '←', '◀'],
        ),
        _ => return Vec::new(),
    };

    let missing: Vec<(usize, usize)> = ports
        .iter()
        .filter_map(|(x, y)| {
            let frame_x = x.saturating_sub(origin_x);
            let frame_y = y.saturating_sub(origin_y);
            let actual = lines
                .get(frame_y)
                .and_then(|line| line.get(frame_x))
                .copied();
            (!actual.is_some_and(|glyph| expected.contains(&glyph))).then_some((frame_x, frame_y))
        })
        .collect();

    let arrowheads = lines
        .iter()
        .flat_map(|line| line.iter())
        .filter(|glyph| {
            matches!(
                glyph,
                '>' | '<' | '^' | 'v' | '→' | '←' | '↑' | '↓' | '▶' | '◀' | '▲' | '▼'
            )
        })
        .count();

    let mut findings = Vec::new();
    if !missing.is_empty() {
        findings.push(finding(
            "declared_edge_missing",
            "P1",
            format!(
                "dual-junction target {} is missing {} of 2 visible incoming arrowheads",
                target.id,
                missing.len()
            ),
            missing.into_iter().map(|(x, y)| cell(x, y)).collect(),
        ));
    }
    if arrowheads != graph.edges.len() {
        findings.push(finding(
            "dual_junction_arrow_count_mismatch",
            "P1",
            format!(
                "dual-junction frame has {arrowheads} visible arrowheads for {} semantic edges",
                graph.edges.len()
            ),
            Vec::new(),
        ));
    }
    findings
}

/// Independent structural mirror of the renderer's exact dual-junction
/// policy. It intentionally does not call the renderer policy module.
fn dual_junction_identity_family(graph: &termiflow::Graph) -> bool {
    dual_junction_target(graph).is_some()
}

fn dual_junction_target(graph: &termiflow::Graph) -> Option<&termiflow::graph::Node> {
    if !matches!(
        graph.direction,
        termiflow::graph::Direction::TD
            | termiflow::graph::Direction::BT
            | termiflow::graph::Direction::LR
            | termiflow::graph::Direction::RL
    ) || graph.nodes.len() != 5
        || graph.edges.len() != 4
        || !graph.subgraphs.is_empty()
        || graph
            .nodes
            .iter()
            .any(|node| node.shape != termiflow::graph::NodeShape::Rectangle)
        || graph.edges.iter().any(|edge| {
            edge.is_back_edge
                || edge.label.is_some()
                || edge.kind != termiflow::graph::EdgeKind::Arrow
        })
    {
        return None;
    }

    let node_ids: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    if node_ids.len() != graph.nodes.len() {
        return None;
    }
    let target = graph.nodes.iter().find(|target| {
        graph
            .edges
            .iter()
            .filter(|edge| edge.to == target.id)
            .count()
            == 2
            && graph
                .edges
                .iter()
                .filter(|edge| edge.from == target.id)
                .count()
                == 2
    })?;
    let incoming: Vec<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.to == target.id)
        .map(|edge| edge.from.as_str())
        .collect();
    let outgoing: Vec<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.from == target.id)
        .map(|edge| edge.to.as_str())
        .collect();
    let incoming_ids: HashSet<&str> = incoming.iter().copied().collect();
    let outgoing_ids: HashSet<&str> = outgoing.iter().copied().collect();
    if incoming_ids.len() != 2
        || outgoing_ids.len() != 2
        || incoming_ids.iter().any(|id| outgoing_ids.contains(id))
        || incoming_ids.iter().any(|id| !node_ids.contains(id))
        || outgoing_ids.iter().any(|id| !node_ids.contains(id))
    {
        return None;
    }
    let expected_ids: HashSet<&str> = incoming_ids
        .iter()
        .copied()
        .chain(std::iter::once(target.id.as_str()))
        .chain(outgoing_ids.iter().copied())
        .collect();
    (expected_ids == node_ids).then_some(target)
}

/// Independent structural mirror of the ordinary fan-in identity policy.
/// Fixture names and coordinates are deliberately absent from this gate.
fn ordinary_fan_in_identity_family(graph: &termiflow::Graph) -> bool {
    if !matches!(
        graph.direction,
        termiflow::graph::Direction::TD
            | termiflow::graph::Direction::TB
            | termiflow::graph::Direction::BT
            | termiflow::graph::Direction::LR
            | termiflow::graph::Direction::RL
    ) || graph.nodes.is_empty()
        || !graph.subgraphs.is_empty()
        || dense_crossing_scene_family(graph)
        || graph.has_cycles()
        || graph
            .nodes
            .iter()
            .any(|node| node.shape != termiflow::graph::NodeShape::Rectangle)
        || graph.edges.iter().any(|edge| {
            edge.is_back_edge
                || edge.kind != termiflow::graph::EdgeKind::Arrow
                || edge.label.is_some()
                || edge.from == edge.to
        })
    {
        return false;
    }

    graph
        .nodes
        .iter()
        .any(|target| ordinary_target_port_count(graph, &target.id) >= 2)
}

fn ordinary_target_port_count(graph: &termiflow::Graph, target_id: &str) -> usize {
    if !ordinary_policy_prerequisites(graph) {
        return 0;
    }
    let Some(target) = graph.nodes.iter().find(|node| node.id == target_id) else {
        return 0;
    };
    let incoming: Vec<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.to == target.id)
        .map(|edge| edge.from.as_str())
        .collect();
    let source_ids: HashSet<&str> = incoming.iter().copied().collect();
    let node_ids: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    if !(2..=4).contains(&incoming.len())
        || source_ids.len() != incoming.len()
        || source_ids.contains(target.id.as_str())
        || source_ids.iter().any(|source| !node_ids.contains(source))
    {
        0
    } else {
        incoming.len()
    }
}

fn ordinary_policy_prerequisites(graph: &termiflow::Graph) -> bool {
    matches!(
        graph.direction,
        termiflow::graph::Direction::TD
            | termiflow::graph::Direction::TB
            | termiflow::graph::Direction::BT
            | termiflow::graph::Direction::LR
            | termiflow::graph::Direction::RL
    ) && !graph.nodes.is_empty()
        && graph.subgraphs.is_empty()
        && !graph.has_cycles()
        && graph
            .nodes
            .iter()
            .all(|node| node.shape == termiflow::graph::NodeShape::Rectangle)
        && graph.edges.iter().all(|edge| {
            !edge.is_back_edge
                && edge.kind == termiflow::graph::EdgeKind::Arrow
                && edge.label.is_none()
                && edge.from != edge.to
        })
}

fn dedicated_fan_in_identity_family(graph: &termiflow::Graph) -> bool {
    if !matches!(
        graph.direction,
        termiflow::graph::Direction::LR | termiflow::graph::Direction::RL
    ) || !graph.subgraphs.is_empty()
        || graph.nodes.is_empty()
        || dense_crossing_scene_family(graph)
        || graph.edges.iter().any(|edge| {
            edge.is_back_edge
                || edge.label.is_some()
                || edge.kind != termiflow::graph::EdgeKind::Arrow
        })
    {
        return false;
    }

    let incoming_count =
        |node_id: &str| graph.edges.iter().filter(|edge| edge.to == node_id).count();
    let fan_in_target_count = graph
        .nodes
        .iter()
        .filter(|node| incoming_count(&node.id) >= 2)
        .count();
    if graph
        .nodes
        .iter()
        .all(|node| node.shape == termiflow::graph::NodeShape::Rectangle)
        && fan_in_target_count >= 4
    {
        return true;
    }

    graph.nodes.len() == 3
        && graph.edges.len() == 3
        && graph.nodes.iter().any(|target| {
            target.shape == termiflow::graph::NodeShape::Database
                && incoming_count(&target.id) == 2
                && graph.edges.iter().any(|edge| {
                    edge.to == target.id
                        && graph
                            .edges
                            .iter()
                            .filter(|outgoing| outgoing.from == edge.from)
                            .count()
                            >= 2
                        && graph.edges.iter().any(|other| {
                            other.from == edge.from
                                && other.to != target.id
                                && graph
                                    .edges
                                    .iter()
                                    .any(|via| via.from == other.to && via.to == target.id)
                        })
                })
        })
}

fn nonterminal_vertical_fan_in_identity_family(graph: &termiflow::Graph) -> bool {
    if !matches!(
        graph.direction,
        termiflow::graph::Direction::TD | termiflow::graph::Direction::BT
    ) || graph.nodes.len() != 4
        || graph.edges.len() != 3
        || !graph.subgraphs.is_empty()
        || graph
            .nodes
            .iter()
            .any(|node| node.shape != termiflow::graph::NodeShape::Rectangle)
        || graph.edges.iter().any(|edge| {
            edge.is_back_edge
                || edge.label.is_some()
                || edge.kind != termiflow::graph::EdgeKind::Arrow
        })
    {
        return false;
    }

    let Some(target) = graph.nodes.iter().find(|target| {
        graph
            .edges
            .iter()
            .filter(|edge| edge.to == target.id)
            .count()
            == 2
            && graph
                .edges
                .iter()
                .filter(|edge| edge.from == target.id)
                .count()
                == 1
    }) else {
        return false;
    };
    let incoming: Vec<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.to == target.id)
        .map(|edge| edge.from.as_str())
        .collect();
    let Some(downstream) = graph
        .edges
        .iter()
        .find(|edge| edge.from == target.id)
        .map(|edge| edge.to.as_str())
    else {
        return false;
    };
    incoming.len() == 2
        && incoming[0] != incoming[1]
        && incoming.iter().all(|source| *source != downstream)
        && downstream != target.id
        && graph
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<HashSet<_>>()
            == incoming
                .iter()
                .copied()
                .chain(std::iter::once(target.id.as_str()))
                .chain(std::iter::once(downstream))
                .collect::<HashSet<_>>()
}

fn dense_crossing_scene_family(graph: &termiflow::Graph) -> bool {
    if graph.nodes.len() != 9 || graph.edges.iter().filter(|edge| !edge.is_back_edge).count() != 12
    {
        return false;
    }
    let role_counts = graph
        .nodes
        .iter()
        .map(|node| {
            let incoming = graph
                .edges
                .iter()
                .filter(|edge| !edge.is_back_edge && edge.to == node.id)
                .count();
            let outgoing = graph
                .edges
                .iter()
                .filter(|edge| !edge.is_back_edge && edge.from == node.id)
                .count();
            (incoming, outgoing)
        })
        .collect::<Vec<_>>();
    role_counts.iter().filter(|role| **role == (0, 2)).count() == 3
        && role_counts.iter().filter(|role| **role == (2, 2)).count() == 3
        && role_counts.iter().filter(|role| **role == (2, 0)).count() == 3
}

fn dedicated_target_port_count(graph: &termiflow::Graph, target_id: &str) -> usize {
    let incoming_count = graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge && edge.to == target_id)
        .count();
    if incoming_count < 2 {
        return 0;
    }
    let fan_in_target_count = graph
        .nodes
        .iter()
        .filter(|node| {
            graph
                .edges
                .iter()
                .filter(|edge| !edge.is_back_edge && edge.to == node.id)
                .count()
                >= 2
        })
        .count();
    if graph
        .nodes
        .iter()
        .all(|node| node.shape == termiflow::graph::NodeShape::Rectangle)
        && fan_in_target_count >= 4
    {
        return incoming_count;
    }
    if graph
        .get_node(target_id)
        .is_some_and(|node| node.shape == termiflow::graph::NodeShape::Database)
    {
        incoming_count
    } else {
        0
    }
}

/// Independent mirror of the renderer's shape-owned incoming-edge clearance.
/// Keep this policy local to the oracle: `termiflow-qa` is a separate binary
/// crate and must not consume private renderer implementation state.
fn target_entry_clearance(
    shape: termiflow::graph::NodeShape,
    direction: termiflow::graph::Direction,
) -> usize {
    usize::from(
        shape == termiflow::graph::NodeShape::Diamond
            || (shape == termiflow::graph::NodeShape::Asymmetric
                && direction == termiflow::graph::Direction::LR),
    )
}

fn subgraph_portal_findings(graph: &termiflow::Graph, frame: &str, style: BaseStyle) -> Vec<Value> {
    let chars = CompositeStyle::from_base(style).to_style_chars(style);
    let origin_x = graph
        .nodes
        .iter()
        .map(|node| node.x)
        .chain(graph.subgraphs.iter().map(|subgraph| subgraph.bounds.x))
        .min()
        .unwrap_or(0);
    let origin_y = graph
        .nodes
        .iter()
        .map(|node| node.y)
        .chain(graph.subgraphs.iter().map(|subgraph| subgraph.bounds.y))
        .min()
        .unwrap_or(0);
    let mut findings = Vec::new();

    for subgraph in &graph.subgraphs {
        let mut pair_counts = HashMap::<(String, String), usize>::new();
        for edge in graph.edges.iter().filter(|edge| !edge.is_back_edge) {
            let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
            if exits.len() == 1
                && enters.len() == 1
                && (exits[0] == subgraph.id || enters[0] == subgraph.id)
            {
                *pair_counts
                    .entry((exits[0].to_owned(), enters[0].to_owned()))
                    .or_default() += 1;
            }
        }
        if pair_counts.is_empty() {
            continue;
        }

        let contacts =
            subgraph_border_contacts(graph, frame, &subgraph.bounds, chars, origin_x, origin_y);
        if contacts.is_empty() {
            findings.push(finding(
                "subgraph_portal_missing_raw_contact",
                "P1",
                format!(
                    "subgraph {} has declared boundary edges but no visible raw portal contact",
                    subgraph.id
                ),
                Vec::new(),
            ));
            continue;
        }

        let parallel = pair_counts.values().copied().max().unwrap_or(0);
        if graph.direction == termiflow::graph::Direction::BT && parallel >= 2 {
            let has_junction_like_contact = contacts.iter().any(|contact| {
                let x = contact["x"].as_u64().map(|value| value as usize);
                let y = contact["y"].as_u64().map(|value| value as usize);
                match (x, y) {
                    (Some(x), Some(y)) => rendered_char(
                        frame,
                        x.saturating_add(origin_x),
                        y.saturating_add(origin_y),
                        origin_x,
                        origin_y,
                    )
                    .is_some_and(is_bt_boundary_junction_glyph),
                    _ => false,
                }
            });
            if !has_junction_like_contact {
                continue;
            }
        }
        let code = if parallel >= 2 {
            "parallel_portal_identity_requires_human_review"
        } else {
            "subgraph_portal_ownership_requires_human_review"
        };
        findings.push(finding(
            code,
            "P2",
            format!(
                "subgraph {} has {} boundary edge group(s) crossing visible border cells; raw ownership needs one-frame review",
                subgraph.id,
                pair_counts.len()
            ),
            contacts,
        ));
    }

    findings
}

fn origin(graph: &termiflow::Graph) -> (usize, usize) {
    (
        graph
            .nodes
            .iter()
            .map(|node| node.x)
            .chain(graph.subgraphs.iter().map(|subgraph| subgraph.bounds.x))
            .min()
            .unwrap_or(0),
        graph
            .nodes
            .iter()
            .map(|node| node.y)
            .chain(graph.subgraphs.iter().map(|subgraph| subgraph.bounds.y))
            .min()
            .unwrap_or(0),
    )
}

/// Queue title-adjacent BT elbows for visual review.
///
/// A title-safe portal can be semantically connected while still producing a
/// conspicuous `└─┐`/`┌─┘` (or `+-+`) detour beside the title row. This is a
/// human-legibility signal, not a structural failure: the reviewer must decide
/// whether the elbow is an intentional title-avoidance channel or an accidental
/// route kink before a code hypothesis is promoted.
fn bt_title_boundary_hook_findings(
    graph: &termiflow::Graph,
    frame: &str,
    (origin_x, origin_y): (usize, usize),
) -> Vec<Value> {
    if graph.direction != termiflow::graph::Direction::BT {
        return Vec::new();
    }

    let mut findings = Vec::new();
    for subgraph in &graph.subgraphs {
        let Some(title) = subgraph.title.as_deref() else {
            continue;
        };
        let has_boundary_edge = graph.edges.iter().any(|edge| {
            if edge.is_back_edge {
                return false;
            }
            let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
            exits
                .iter()
                .chain(enters.iter())
                .any(|id| *id == subgraph.id)
        });
        if !has_boundary_edge || !subgraph.bounds.is_valid() {
            continue;
        }

        let title_y = termiflow::graph::subgraph_title_row(
            subgraph.bounds.y,
            subgraph.bounds.height,
            graph.direction,
        );
        let first_row = title_y.saturating_sub(2);
        let last_row = title_y.saturating_add(2);
        let first_x = subgraph.bounds.x.saturating_sub(origin_x);
        let last_x = subgraph
            .bounds
            .x
            .saturating_add(subgraph.bounds.width)
            .saturating_sub(origin_x);
        let lines: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
        let mut cells = Vec::new();

        for graph_y in first_row..=last_row {
            let Some(row) = lines.get(graph_y.saturating_sub(origin_y)) else {
                continue;
            };
            let start = first_x.min(row.len());
            let end = last_x.min(row.len());
            if start >= end {
                continue;
            }
            for (left, right) in horizontal_elbow_pairs(&row[start..end]) {
                let left_x = left.saturating_add(start);
                let right_x = right.saturating_add(start);
                let has_interior_endpoint = [left_x, right_x]
                    .into_iter()
                    .any(|x| x > first_x && x < last_x);
                let row_index = graph_y.saturating_sub(origin_y);
                let has_adjacent_vertical_route = [left_x, right_x].into_iter().any(|x| {
                    [row_index.saturating_sub(1), row_index.saturating_add(1)]
                        .into_iter()
                        .filter_map(|neighbor_y| {
                            lines.get(neighbor_y).and_then(|neighbor| neighbor.get(x))
                        })
                        .copied()
                        .any(is_vertical_route_glyph)
                });
                let is_directional_unicode_elbow = matches!(
                    (row[start + left], row[start + right]),
                    ('└', '┐') | ('┌', '┘') | ('╚', '╗') | ('╔', '╝')
                );
                let is_unicode_boundary_elbow = matches!(
                    (row[start + left], row[start + right]),
                    ('└', '┘') | ('┌', '┐') | ('╚', '╝') | ('╔', '╗')
                ) && has_interior_endpoint
                    && has_adjacent_vertical_route;
                let is_ascii_route_elbow = row[start + left] == '+'
                    && row[start + right] == '+'
                    && (right.saturating_sub(left) <= 3
                        || (has_interior_endpoint && has_adjacent_vertical_route));
                if !is_directional_unicode_elbow
                    && !is_unicode_boundary_elbow
                    && !is_ascii_route_elbow
                {
                    continue;
                }
                cells.push(cell(left_x, graph_y.saturating_sub(origin_y)));
                cells.push(cell(right_x, graph_y.saturating_sub(origin_y)));
            }
        }

        cells.sort_by_key(|value| {
            (
                value["y"].as_u64().unwrap_or_default(),
                value["x"].as_u64().unwrap_or_default(),
            )
        });
        cells.dedup();
        if !cells.is_empty() {
            findings.push(finding(
                "bt_title_boundary_hook_requires_human_review",
                "P2",
                format!(
                    "subgraph {} ({title:?}) has title-adjacent horizontal route elbows; one-frame review must distinguish title clearance from an accidental kink",
                    subgraph.id
                ),
                cells,
            ));
        }
    }
    findings
}

/// Queue BT rails whose straightness hides boundary ownership.
///
/// A connected shaft is not automatically a readable portal. When the same
/// column pierces multiple titled subgraph boundaries, or when several
/// parallel crossings are rendered as repeated four-way border junctions,
/// the raw frame can read as one trunk or bus. Keep those rows in the
/// one-frame queue even when the semantic and geometry reports are clean.
fn bt_boundary_rail_findings(
    graph: &termiflow::Graph,
    frame: &str,
    (origin_x, origin_y): (usize, usize),
) -> Vec<Value> {
    if graph.direction != termiflow::graph::Direction::BT {
        return Vec::new();
    }

    let lines: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
    let mut contacts_by_column = HashMap::<usize, Vec<(String, usize, char)>>::new();
    let mut columns_by_subgraph = HashMap::<String, Vec<usize>>::new();

    for subgraph in &graph.subgraphs {
        if subgraph.title.is_none() || !subgraph.bounds.is_valid() {
            continue;
        }
        let has_boundary_edge = graph.edges.iter().any(|edge| {
            if edge.is_back_edge {
                return false;
            }
            let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
            exits
                .iter()
                .chain(enters.iter())
                .any(|id| *id == subgraph.id)
        });
        if !has_boundary_edge {
            continue;
        }

        let last_x = subgraph
            .bounds
            .x
            .saturating_add(subgraph.bounds.width.saturating_sub(1));
        let last_y = subgraph
            .bounds
            .y
            .saturating_add(subgraph.bounds.height.saturating_sub(1));
        for y in [subgraph.bounds.y, last_y] {
            for x in subgraph.bounds.x.saturating_add(1)..last_x {
                let frame_x = x.saturating_sub(origin_x);
                let frame_y = y.saturating_sub(origin_y);
                let Some(glyph) = lines.get(frame_y).and_then(|row| row.get(frame_x)) else {
                    continue;
                };
                if !is_bt_boundary_rail_glyph(*glyph) {
                    continue;
                }
                let above = frame_y
                    .checked_sub(1)
                    .and_then(|neighbor_y| lines.get(neighbor_y))
                    .and_then(|row| row.get(frame_x))
                    .copied();
                let below = lines
                    .get(frame_y.saturating_add(1))
                    .and_then(|row| row.get(frame_x))
                    .copied();
                if !above.is_some_and(is_vertical_route_glyph)
                    || !below.is_some_and(is_vertical_route_glyph)
                {
                    continue;
                }

                contacts_by_column.entry(frame_x).or_default().push((
                    subgraph.id.clone(),
                    frame_y,
                    *glyph,
                ));
                columns_by_subgraph
                    .entry(subgraph.id.clone())
                    .or_default()
                    .push(frame_x);
            }
        }
    }

    for columns in columns_by_subgraph.values_mut() {
        columns.sort_unstable();
        columns.dedup();
    }

    let mut findings = Vec::new();
    let mut shared_columns = Vec::new();
    let mut subgraph_ids: Vec<String> = columns_by_subgraph.keys().cloned().collect();
    subgraph_ids.sort();
    for (index, left_id) in subgraph_ids.iter().enumerate() {
        let Some(left_columns) = columns_by_subgraph.get(left_id) else {
            continue;
        };
        for right_id in subgraph_ids.iter().skip(index + 1) {
            let Some(right_columns) = columns_by_subgraph.get(right_id) else {
                continue;
            };
            let common: Vec<usize> = left_columns
                .iter()
                .copied()
                .filter(|column| right_columns.contains(column))
                .collect();
            if !common.is_empty() {
                shared_columns.push((left_id.clone(), right_id.clone(), common));
            }
        }
    }

    for (left_id, right_id, columns) in shared_columns {
        let cells: Vec<Value> = columns
            .iter()
            .flat_map(|column| {
                contacts_by_column
                    .get(column)
                    .into_iter()
                    .flat_map(|contacts| contacts.iter())
                    .filter(|(subgraph_id, _, _)| {
                        subgraph_id == &left_id || subgraph_id == &right_id
                    })
                    .map(|(_, y, _)| cell(*column, *y))
            })
            .collect();
        let mut cells = cells;
        cells.sort_by_key(|value| {
            (
                value["y"].as_u64().unwrap_or_default(),
                value["x"].as_u64().unwrap_or_default(),
            )
        });
        cells.dedup();

        if columns.len() >= 3 {
            let has_repeated_junctions = columns.iter().any(|column| {
                contacts_by_column.get(column).is_some_and(|contacts| {
                    contacts.iter().any(|(subgraph_id, _, glyph)| {
                        (subgraph_id == &left_id || subgraph_id == &right_id)
                            && is_bt_boundary_junction_glyph(*glyph)
                    })
                })
            });
            if has_repeated_junctions {
                findings.push(finding(
                    "bt_parallel_portal_junction_requires_human_review",
                    "P2",
                    format!(
                        "BT subgraphs {left_id:?} and {right_id:?} share {} parallel border rails rendered as junction-like portals; one-frame review must distinguish independent ownership from a shared trunk",
                        columns.len()
                    ),
                    cells,
                ));
                continue;
            }
        }

        if columns.len() == 1 {
            findings.push(finding(
                "bt_sibling_boundary_rail_requires_human_review",
                "P2",
                format!(
                    "BT titled subgraphs {left_id:?} and {right_id:?} share one straight border rail; one-frame review must distinguish sibling transition ownership from a shared trunk"
                ),
                cells,
            ));
        }
    }

    findings
}

fn is_bt_boundary_rail_glyph(glyph: char) -> bool {
    is_vertical_route_glyph(glyph) || is_bt_boundary_junction_glyph(glyph)
}

fn is_bt_boundary_junction_glyph(glyph: char) -> bool {
    matches!(glyph, '+' | '┼' | '╬' | '╋')
}

fn is_vertical_route_glyph(glyph: char) -> bool {
    matches!(glyph, '|' | ':' | '│' | '║' | '┃' | '╎')
}

fn horizontal_elbow_pairs(row: &[char]) -> Vec<(usize, usize)> {
    let horizontal = |glyph: char| matches!(glyph, '-' | '=' | '─' | '═' | '━');
    let mut pairs = Vec::new();
    for left in 0..row.len() {
        let is_start = matches!(row[left], '+' | '└' | '┌' | '╚' | '╔');
        if !is_start {
            continue;
        }
        let mut right = left.saturating_add(1);
        while right < row.len() && horizontal(row[right]) {
            right += 1;
        }
        if right > left.saturating_add(1) && right < row.len() {
            let is_directional_unicode_elbow = matches!(
                (row[left], row[right]),
                ('└', '┐') | ('┌', '┘') | ('╚', '╗') | ('╔', '╝')
            );
            let is_unicode_corner_pair = matches!(
                (row[left], row[right]),
                ('└', '┘') | ('┌', '┐') | ('╚', '╝') | ('╔', '╗')
            );
            if is_directional_unicode_elbow
                || is_unicode_corner_pair
                || (row[left] == '+' && row[right] == '+')
            {
                pairs.push((left, right));
            }
        }
    }
    pairs
}

fn subgraph_border_contacts(
    graph: &termiflow::Graph,
    frame: &str,
    bounds: &termiflow::graph::Rectangle,
    chars: termiflow::style::StyleChars,
    origin_x: usize,
    origin_y: usize,
) -> Vec<Value> {
    let mut cells = Vec::new();
    let vertical_flow = matches!(
        graph.direction,
        termiflow::graph::Direction::TD
            | termiflow::graph::Direction::TB
            | termiflow::graph::Direction::BT
    );
    if vertical_flow {
        for y in [bounds.y, bounds.y + bounds.height.saturating_sub(1)] {
            for x in bounds.x + 1..bounds.x + bounds.width.saturating_sub(1) {
                let Some(glyph) = rendered_char(frame, x, y, origin_x, origin_y) else {
                    continue;
                };
                if is_border_contact(glyph, chars) {
                    cells.push(cell(x.saturating_sub(origin_x), y.saturating_sub(origin_y)));
                }
            }
        }
    } else {
        for x in [bounds.x, bounds.x + bounds.width.saturating_sub(1)] {
            for y in bounds.y + 1..bounds.y + bounds.height.saturating_sub(1) {
                let Some(glyph) = rendered_char(frame, x, y, origin_x, origin_y) else {
                    continue;
                };
                if is_border_contact(glyph, chars) {
                    cells.push(cell(x.saturating_sub(origin_x), y.saturating_sub(origin_y)));
                }
            }
        }
    }
    cells.sort_by_key(|value| {
        (
            value["y"].as_u64().unwrap_or_default(),
            value["x"].as_u64().unwrap_or_default(),
        )
    });
    cells.dedup();
    cells
}

fn is_border_contact(glyph: char, chars: termiflow::style::StyleChars) -> bool {
    glyph == chars.edge_v
        || glyph == chars.edge_h
        || glyph == chars.cross
        || glyph == chars.portal_pierce
        || matches!(
            glyph,
            '+' | '┼'
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
                | '┃'
                | '║'
                | '│'
                | '|'
                | '─'
                | '═'
                | '━'
                | '-'
        )
}

fn rendered_char(
    frame: &str,
    x: usize,
    y: usize,
    origin_x: usize,
    origin_y: usize,
) -> Option<char> {
    frame
        .lines()
        .nth(y.saturating_sub(origin_y))?
        .chars()
        .nth(x.saturating_sub(origin_x))
}

fn route_identity_mask(ch: char) -> u8 {
    match ch {
        '-' | '=' | '─' | '═' | '━' => 0b1010,
        '|' | ':' | '│' | '║' | '┃' | '╎' => 0b0101,
        '+' | '┼' | '╬' | '╋' => 0b1111,
        '┌' | '╔' => 0b0110,
        '┐' | '╗' => 0b1100,
        '└' | '╚' => 0b0011,
        '┘' | '╝' => 0b1001,
        '├' | '╠' => 0b0111,
        '┤' | '╣' => 0b1101,
        '┬' | '╦' => 0b1110,
        '┴' | '╩' => 0b1011,
        'x' | '✕' => 0b1111,
        '>' | '→' | '▶' => 0b1000,
        '<' | '←' | '◀' => 0b0010,
        'v' | '↓' | '▼' => 0b0001,
        '^' | '↑' | '▲' => 0b0100,
        _ => 0,
    }
}

fn route_identity_context_mask(lines: &[Vec<char>], x: usize, y: usize) -> u8 {
    let Some(row) = lines.get(y) else {
        return 0;
    };
    let Some(&ch) = row.get(x) else {
        return 0;
    };
    if ch != '+' {
        return route_identity_mask(ch);
    }

    // ASCII uses `+` for both an elbow and a junction. Infer the actual arms
    // from neighboring route glyphs so a two-arm turn does not masquerade as
    // a shared bus in the raw-frame oracle.
    let mut mask = 0;
    for step in 0..4u8 {
        let (dx, dy) = route_identity_step(termiflow::graph::Direction::TD, step);
        let nx = x as isize + dx;
        let ny = y as isize + dy;
        if nx < 0 || ny < 0 {
            continue;
        }
        let (nx, ny) = (nx as usize, ny as usize);
        let neighbor = lines
            .get(ny)
            .and_then(|neighbor_row| neighbor_row.get(nx))
            .copied()
            .unwrap_or(' ');
        if route_identity_mask(neighbor) & (1 << route_identity_opposite(step)) != 0 {
            mask |= 1 << step;
        }
    }
    mask
}

fn route_identity_step(_direction: termiflow::graph::Direction, index: u8) -> (isize, isize) {
    match index {
        0 => (0, -1),
        1 => (1, 0),
        2 => (0, 1),
        3 => (-1, 0),
        _ => unreachable!("route clarity has only four cell directions"),
    }
}

fn route_identity_opposite(index: u8) -> u8 {
    match index {
        0 => 2,
        1 => 3,
        2 => 0,
        3 => 1,
        _ => unreachable!("route clarity has only four cell directions"),
    }
}

#[cfg(test)]
mod bt_boundary_rail_tests {
    use super::*;

    fn report_for_fixture(path: &str, style: BaseStyle, optimized: bool) -> Value {
        let input = include_str!("../../tests/fixtures/inputs/collision_parallel_edges_bt.md");
        let input = if path.ends_with("collision_sibling_triple_bt.md") {
            include_str!("../../tests/fixtures/inputs/collision_sibling_triple_bt.md")
        } else {
            input
        };
        let output = termiflow::render_with_feedback(
            input,
            RenderOptions::new()
                .with_style(style)
                .with_optimize_render(optimized),
        )
        .expect("render route-clarity fixture");
        analyze(
            input.as_bytes(),
            output.output.as_bytes(),
            if matches!(style, BaseStyle::Ascii) {
                "ascii"
            } else {
                "unicode"
            },
            if optimized { "optimized" } else { "default" },
        )
        .expect("analyze route-clarity fixture")
    }

    #[test]
    fn bt_parallel_portals_remove_machine_junction_finding_when_projected_as_local_seams() {
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let report = report_for_fixture("collision_parallel_edges_bt.md", style, optimized);
                assert!(!report["findings"].as_array().is_some_and(|findings| {
                    findings.iter().any(|finding| {
                        matches!(
                            finding["code"].as_str(),
                            Some(
                                "bt_parallel_portal_junction_requires_human_review"
                                    | "parallel_portal_identity_requires_human_review"
                            )
                        )
                    })
                }));
            }
        }
    }

    #[test]
    fn bt_sibling_rails_are_not_machine_clean_when_one_trunk_crosses_boundaries() {
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let report = report_for_fixture("collision_sibling_triple_bt.md", style, optimized);
                assert_eq!(report["status"], "inconclusive");
                assert!(report["findings"].as_array().is_some_and(|findings| {
                    findings.iter().any(|finding| {
                        matches!(
                            finding["code"].as_str(),
                            Some(
                                "bt_sibling_boundary_rail_requires_human_review"
                                    | "bt_title_boundary_hook_requires_human_review"
                                    | "subgraph_portal_ownership_requires_human_review"
                                    | "parallel_portal_identity_requires_human_review"
                            )
                        )
                    })
                }));
            }
        }
    }
}

fn route_identity_progress_allowed(
    direction: termiflow::graph::Direction,
    x: usize,
    y: usize,
    nx: usize,
    ny: usize,
) -> bool {
    match direction {
        termiflow::graph::Direction::TD | termiflow::graph::Direction::TB => ny >= y,
        termiflow::graph::Direction::BT => ny <= y,
        termiflow::graph::Direction::LR => nx >= x,
        termiflow::graph::Direction::RL => nx <= x,
    }
}

fn dense_route_source_ports(
    node: &termiflow::graph::Node,
    direction: termiflow::graph::Direction,
    count: usize,
) -> Vec<usize> {
    match direction {
        termiflow::graph::Direction::TD
        | termiflow::graph::Direction::TB
        | termiflow::graph::Direction::BT => {
            let center = node.center_x();
            let offset = if node.width >= 7 { 2 } else { 1 };
            vec![center.saturating_sub(offset), center.saturating_add(offset)]
        }
        termiflow::graph::Direction::LR | termiflow::graph::Direction::RL => {
            let height = node.height.max(9);
            vec![
                node.y.saturating_add(1),
                node.y.saturating_add(height).saturating_sub(2),
            ]
        }
    }
    .into_iter()
    .take(count)
    .collect()
}

fn dense_route_target_ports(
    node: &termiflow::graph::Node,
    direction: termiflow::graph::Direction,
    count: usize,
) -> Vec<usize> {
    match direction {
        termiflow::graph::Direction::TD
        | termiflow::graph::Direction::TB
        | termiflow::graph::Direction::BT => {
            let center = node.center_x();
            let offset = if node.width >= 11 {
                4
            } else if node.width >= 7 {
                2
            } else {
                1
            };
            vec![center.saturating_sub(offset), center.saturating_add(offset)]
        }
        termiflow::graph::Direction::LR | termiflow::graph::Direction::RL => {
            let height = node.height.max(9);
            vec![
                node.y.saturating_add(3),
                node.y.saturating_add(height).saturating_sub(4),
            ]
        }
    }
    .into_iter()
    .take(count)
    .collect()
}

fn dense_route_source_attachment(
    node: &termiflow::graph::Node,
    direction: termiflow::graph::Direction,
    secondary: usize,
    origin_x: usize,
    origin_y: usize,
) -> (usize, usize) {
    let point = match direction {
        termiflow::graph::Direction::TD | termiflow::graph::Direction::TB => {
            (secondary, node.bottom_y().saturating_add(1))
        }
        termiflow::graph::Direction::BT => (secondary, node.y.saturating_sub(1)),
        termiflow::graph::Direction::LR => (node.x.saturating_add(node.width), secondary),
        termiflow::graph::Direction::RL => (node.x.saturating_sub(1), secondary),
    };
    (
        point.0.saturating_sub(origin_x),
        point.1.saturating_sub(origin_y),
    )
}

fn dense_route_target_attachments(
    node: &termiflow::graph::Node,
    direction: termiflow::graph::Direction,
    origin_x: usize,
    origin_y: usize,
) -> Vec<((usize, usize), String)> {
    dense_route_target_ports(node, direction, 2)
        .into_iter()
        .map(|secondary| {
            let point = match direction {
                termiflow::graph::Direction::TD | termiflow::graph::Direction::TB => {
                    (secondary, node.y.saturating_sub(1))
                }
                termiflow::graph::Direction::BT => (secondary, node.bottom_y().saturating_add(1)),
                termiflow::graph::Direction::LR => (node.x.saturating_sub(1), secondary),
                termiflow::graph::Direction::RL => (node.x.saturating_add(node.width), secondary),
            };
            (
                (
                    point.0.saturating_sub(origin_x),
                    point.1.saturating_sub(origin_y),
                ),
                node.id.clone(),
            )
        })
        .collect()
}

fn route_identity_primary_range(
    source: &termiflow::graph::Node,
    target: &termiflow::graph::Node,
    direction: termiflow::graph::Direction,
) -> Option<(usize, usize)> {
    let (start, end) = match direction {
        termiflow::graph::Direction::TD | termiflow::graph::Direction::TB => (
            source.bottom_y().saturating_add(1),
            target.y.saturating_sub(1),
        ),
        termiflow::graph::Direction::BT => (
            target.bottom_y().saturating_add(1),
            source.y.saturating_sub(1),
        ),
        termiflow::graph::Direction::LR => (
            source.x.saturating_add(source.width),
            target.x.saturating_sub(1),
        ),
        termiflow::graph::Direction::RL => (
            target.x.saturating_add(target.width),
            source.x.saturating_sub(1),
        ),
    };
    (start <= end).then_some((start, end))
}

fn route_identity_reachable_targets(
    frame: &str,
    graph: &termiflow::Graph,
    source_start: (usize, usize),
    target_nodes: &[&termiflow::graph::Node],
    primary_range: (usize, usize),
    origin_x: usize,
    origin_y: usize,
) -> HashSet<String> {
    let lines: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
    let width = lines.iter().map(Vec::len).max().unwrap_or(0);
    let target_points: HashMap<(usize, usize), String> = target_nodes
        .iter()
        .flat_map(|node| dense_route_target_attachments(node, graph.direction, origin_x, origin_y))
        .collect();
    let in_primary_range = |x: usize, y: usize| match graph.direction {
        termiflow::graph::Direction::TD
        | termiflow::graph::Direction::TB
        | termiflow::graph::Direction::BT => y >= primary_range.0 && y <= primary_range.1,
        termiflow::graph::Direction::LR | termiflow::graph::Direction::RL => {
            x >= primary_range.0 && x <= primary_range.1
        }
    };
    let char_at = |x: usize, y: usize| {
        lines
            .get(y)
            .and_then(|row| row.get(x))
            .copied()
            .unwrap_or(' ')
    };
    let mask_at = |x: usize, y: usize| route_identity_context_mask(&lines, x, y);
    let is_route = |x: usize, y: usize| mask_at(x, y) != 0;
    if source_start.0 >= width
        || source_start.1 >= lines.len()
        || !is_route(source_start.0, source_start.1)
    {
        return HashSet::new();
    }

    let mut queue = VecDeque::from([(source_start.0, source_start.1, ROUTE_NONE)]);
    let mut visited = HashSet::from([(source_start.0, source_start.1, ROUTE_NONE)]);
    let mut reachable = HashSet::new();
    while let Some((x, y, back)) = queue.pop_front() {
        if let Some(node_id) = target_points.get(&(x, y)) {
            reachable.insert(node_id.clone());
        }
        let glyph = char_at(x, y);
        let mask = mask_at(x, y);
        for step in 0..4u8 {
            if mask & (1 << step) == 0 {
                continue;
            }
            if matches!(glyph, 'x' | '✕') && back != ROUTE_NONE {
                let previous_is_vertical = matches!(back, 0 | 2);
                let next_is_vertical = matches!(step, 0 | 2);
                if previous_is_vertical != next_is_vertical || step == back {
                    continue;
                }
            }
            let (dx, dy) = route_identity_step(graph.direction, step);
            let nx = x as isize + dx;
            let ny = y as isize + dy;
            if nx < 0 || ny < 0 {
                continue;
            }
            let (nx, ny) = (nx as usize, ny as usize);
            if !route_identity_progress_allowed(graph.direction, x, y, nx, ny)
                || !in_primary_range(nx, ny)
                || !is_route(nx, ny)
                || mask_at(nx, ny) & (1 << route_identity_opposite(step)) == 0
            {
                continue;
            }
            let next = (nx, ny, route_identity_opposite(step));
            if visited.insert(next) {
                queue.push_back(next);
            }
        }
    }
    reachable
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_fixture(input: &[u8], style: BaseStyle, optimized: bool) -> String {
        render_fixture_with_graph(input, style, optimized).1
    }

    fn render_fixture_with_graph(
        input: &[u8],
        style: BaseStyle,
        optimized: bool,
    ) -> (termiflow::Graph, String) {
        let source = std::str::from_utf8(input).expect("fixture UTF-8");
        let parsed = parse(source, false).expect("parse fixture");
        let mut config = Config::from_parse_config(&parsed.config);
        let mut graph = parsed.graph;
        config.optimize_render = optimized;
        config.composite_style = CompositeStyle::from_base(style);
        config.spacing = config.spacing.for_direction(graph.direction);
        measure::measure_graph(&mut graph, &config);
        let (graph, outcome) =
            layout_and_render_with_feedback(graph, config).expect("render fixture");
        (graph, outcome.output)
    }

    fn replace_frame_cell(frame: &str, x: usize, y: usize, replacement: char) -> String {
        let trailing_newline = frame.ends_with('\n');
        let mut rows: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
        rows.get_mut(y)
            .and_then(|row| row.get_mut(x))
            .map(|cell| *cell = replacement)
            .expect("replacement cell must be in rendered frame");
        let mut mutated = rows
            .into_iter()
            .map(|row| row.into_iter().collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        if trailing_newline {
            mutated.push('\n');
        }
        mutated
    }

    #[test]
    fn dense_baseline_is_clean_after_route_identity_fix() {
        let input = std::fs::read("tests/fixtures/inputs/crossing_grid_td.md")
            .expect("read dense crossing input");
        let frame = render_fixture(&input, BaseStyle::Ascii, false);
        let report = analyze(&input, frame.as_bytes(), "ascii", "default").expect("analyze frame");
        assert_eq!(report["status"], "clean");
        assert!(report["findings"]
            .as_array()
            .is_some_and(|items| items.is_empty()));
    }

    #[test]
    fn raw_frame_mutation_is_detected_without_route_metadata() {
        let input = std::fs::read("tests/fixtures/inputs/crossing_grid_td.md")
            .expect("read dense crossing input");
        let frame = render_fixture(&input, BaseStyle::Ascii, false);
        let baseline =
            analyze(&input, frame.as_bytes(), "ascii", "default").expect("analyze baseline");
        let mutated: String = frame
            .chars()
            .map(|glyph| {
                matches!(glyph, '+' | '-' | '|' | 'x' | '>' | '<' | 'v' | '^')
                    .then_some(' ')
                    .unwrap_or(glyph)
            })
            .collect();
        let report =
            analyze(&input, mutated.as_bytes(), "ascii", "default").expect("analyze mutation");
        assert_ne!(report["frame_sha256"], baseline["frame_sha256"]);
        assert_ne!(report["findings"], baseline["findings"]);
        assert!(report["findings"].as_array().is_some_and(|items| {
            items
                .iter()
                .any(|item| item["code"] == "declared_edge_missing")
        }));
    }

    #[test]
    fn raw_label_oracle_accepts_wrapped_and_bounded_visual_lines() {
        for fixture in [
            "tests/fixtures/inputs/label_wrap_td.md",
            "tests/fixtures/inputs/label_long_td.md",
        ] {
            let input = std::fs::read(fixture).expect("read label fixture");
            let frame = render_fixture(&input, BaseStyle::Ascii, false);
            let report =
                analyze(&input, frame.as_bytes(), "ascii", "default").expect("analyze frame");
            assert!(
                report["findings"].as_array().is_some_and(|items| {
                    !items.iter().any(|item| item["code"] == "raw_label_missing")
                }),
                "raw-label oracle rejected the measured visual lines for {fixture}: {report}"
            );
        }
    }

    #[test]
    fn database_fan_in_route_probe_uses_shape_clearance_matrix() {
        assert_eq!(
            target_entry_clearance(
                termiflow::graph::NodeShape::Database,
                termiflow::graph::Direction::LR
            ),
            0,
            "database receiver entries use the generic one-cell terminal entry"
        );
        for direction in ["lr", "rl"] {
            let input = std::fs::read(format!(
                "tests/fixtures/inputs/shape_database_{direction}.md"
            ))
            .expect("read database fan-in fixture");
            for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
                let style_name = match style {
                    BaseStyle::Ascii => "ascii",
                    BaseStyle::Unicode => "unicode",
                    _ => unreachable!(),
                };
                for optimized in [false, true] {
                    let frame = render_fixture(&input, style, optimized);
                    let report = analyze(
                        &input,
                        frame.as_bytes(),
                        style_name,
                        if optimized { "optimized" } else { "default" },
                    )
                    .expect("analyze database fan-in frame");
                    assert!(
                        report["findings"].as_array().is_some_and(|items| {
                            !items.iter().any(|item| {
                                item["code"] == "declared_edge_missing"
                                    && item["severity"] == "P1"
                            })
                        }),
                        "database fan-in route probe falsely reports a P1 for {direction} {style_name} optimized={optimized}: {report}"
                    );
                }
            }
        }
    }

    #[test]
    fn database_fan_in_route_probe_rejects_missing_clearance_arrow() {
        let input = std::fs::read("tests/fixtures/inputs/shape_database_rl.md")
            .expect("read RL database fan-in fixture");
        let (graph, frame) = render_fixture_with_graph(&input, BaseStyle::Unicode, true);
        let target = graph.get_node("DB").expect("database fan-in target");
        let clearance = target_entry_clearance(target.shape, graph.direction);
        let arrow_x = target
            .x
            .saturating_add(target.width)
            .saturating_add(clearance);
        let arrow_y = target.y.saturating_add(1);
        let origin_x = graph.nodes.iter().map(|node| node.x).min().unwrap_or(0);
        let origin_y = graph.nodes.iter().map(|node| node.y).min().unwrap_or(0);
        let frame_x = arrow_x.saturating_sub(origin_x);
        let frame_y = arrow_y.saturating_sub(origin_y);
        let chars =
            CompositeStyle::from_base(BaseStyle::Unicode).to_style_chars(BaseStyle::Unicode);
        let actual = frame
            .lines()
            .nth(frame_y)
            .and_then(|line| line.chars().nth(frame_x));
        assert_eq!(actual, Some(chars.arrow_left));

        let mutated = replace_frame_cell(&frame, frame_x, frame_y, ' ');
        let report = analyze(&input, mutated.as_bytes(), "unicode", "optimized")
            .expect("analyze missing database arrow mutation");
        assert!(
            report["findings"].as_array().is_some_and(|items| {
                items
                    .iter()
                    .any(|item| item["code"] == "declared_edge_missing" && item["severity"] == "P1")
            }),
            "missing clearance-adjusted database arrow was not rejected: {report}"
        );
    }

    #[test]
    fn report_validation_rejects_stale_frame_and_missing_findings() {
        let input = std::fs::read("tests/fixtures/inputs/crossing_grid_td.md")
            .expect("read dense crossing input");
        let frame = render_fixture(&input, BaseStyle::Ascii, false);
        let report = analyze(&input, frame.as_bytes(), "ascii", "default").expect("report");
        validate_report(
            &report,
            &input,
            frame.as_bytes(),
            "ascii",
            "default",
            "test",
        )
        .expect("valid route-clarity report");

        let mut stale = report.clone();
        stale["frame_sha256"] = Value::String("0".repeat(64));
        assert!(
            validate_report(&stale, &input, frame.as_bytes(), "ascii", "default", "test",).is_err()
        );

        let mut inconsistent = report;
        inconsistent["status"] = Value::String("risk".to_owned());
        inconsistent["findings"] = Value::Array(Vec::new());
        assert!(validate_report(
            &inconsistent,
            &input,
            frame.as_bytes(),
            "ascii",
            "default",
            "test",
        )
        .is_err());
    }

    #[test]
    fn bt_title_boundary_hooks_are_queued_for_visual_review() {
        for (fixture, style) in [
            (
                "tests/fixtures/inputs/collision_parallel_edges_bt.md",
                BaseStyle::Ascii,
            ),
            (
                "tests/fixtures/inputs/collision_sibling_triple_bt.md",
                BaseStyle::Unicode,
            ),
        ] {
            let input = std::fs::read(fixture).expect("read BT title-hook fixture");
            let frame = render_fixture(&input, style, true);
            let report = analyze(
                &input,
                frame.as_bytes(),
                match style {
                    BaseStyle::Ascii => "ascii",
                    BaseStyle::Unicode => "unicode",
                    _ => unreachable!(),
                },
                "optimized",
            )
            .expect("analyze BT title-hook frame");
            assert_eq!(report["status"], "inconclusive");
            assert!(report["findings"].as_array().is_some_and(|items| {
                items
                    .iter()
                    .any(|item| item["code"] == "bt_title_boundary_hook_requires_human_review")
            }));
        }
    }
}
