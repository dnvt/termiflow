//! Oracles that derive expectations from parser output and raw text only.
//!
//! These deliberately do not consume `SemanticFrame`, provenance, or critic
//! findings. Geometry checks consume the normalized trace as a separate input.

use std::collections::{HashMap, HashSet};
use std::fs;
use termiflow::{
    layout_and_render_with_feedback, measure, parse, BaseStyle, CompositeStyle, Config,
    GeometryTrace, RectTrace, RenderOptions,
};

fn secondary_center_for(x: usize, y: usize, width: usize, height: usize, direction: &str) -> usize {
    match direction {
        "TD" | "BT" => x + width / 2,
        "LR" | "RL" => y + height / 2,
        other => panic!("unsupported junction direction {other}"),
    }
}

fn raw_frame_errors(input: &str, frame: &str) -> Vec<String> {
    let parsed = parse(input, false).expect("parse oracle input");
    let mut errors = Vec::new();

    if frame.trim().is_empty() {
        errors.push("rendered frame is empty".to_string());
    }
    for node in &parsed.graph.nodes {
        if !node.label.is_empty() && !frame.contains(&node.label) {
            errors.push(format!(
                "node label {:?} is absent from raw frame",
                node.label
            ));
        }
    }

    errors.extend(raw_topology_errors(frame, parsed.graph.edges.len()));
    errors
}

fn raw_char_at(frame: &str, x: usize, y: usize) -> Option<char> {
    frame.lines().nth(y)?.chars().nth(x)
}

fn diamond_entry_clearance_errors(input: &str, style: BaseStyle, optimized: bool) -> Vec<String> {
    let mut graph = parse(input, false)
        .expect("parse diamond-entry fixture")
        .graph;
    let mut config = Config::default();
    config.optimize_render = optimized;
    config.composite_style = CompositeStyle::from_base(style);
    config.spacing = config.spacing.for_direction(graph.direction);
    measure::measure_graph(&mut graph, &config);
    let (graph, outcome) =
        layout_and_render_with_feedback(graph, config).expect("render diamond-entry fixture");
    let target = graph
        .get_node("Check")
        .expect("diamond-entry fixture target node");

    let (entry, gap) = match graph.direction {
        termiflow::graph::Direction::TD | termiflow::graph::Direction::TB => (
            (target.center_x(), target.y.saturating_sub(2)),
            (target.center_x(), target.y.saturating_sub(1)),
        ),
        termiflow::graph::Direction::BT => (
            (target.center_x(), target.bottom_y().saturating_add(1)),
            (target.center_x(), target.bottom_y()),
        ),
        termiflow::graph::Direction::LR => (
            (target.x.saturating_sub(2), target.center_y()),
            (target.x.saturating_sub(1), target.center_y()),
        ),
        termiflow::graph::Direction::RL => (
            (
                target.x.saturating_add(target.width).saturating_add(1),
                target.center_y(),
            ),
            (target.x.saturating_add(target.width), target.center_y()),
        ),
    };

    let trace = GeometryTrace::from_graph(&graph);
    let mut errors = Vec::new();
    if let Some(edge) = trace
        .edges
        .iter()
        .find(|edge| edge.from == "Start" && edge.to == "Check")
    {
        if let Some(last_segment) = edge.segments.last() {
            if (last_segment.to.x, last_segment.to.y) != entry {
                errors.push(format!(
                    "geometry route ends at ({}, {}) instead of diamond entry ({}, {})",
                    last_segment.to.x, last_segment.to.y, entry.0, entry.1
                ));
            }
        } else {
            errors.push(
                "zero-length geometry route leaves the Start -> Check arrow visually disconnected"
                    .to_string(),
            );
        }
    } else {
        errors.push("Start -> Check geometry edge is missing".to_string());
    }

    let Some(entry_meta) = outcome.semantic_frame.get(entry.0, entry.1) else {
        errors.push(format!(
            "semantic frame has no entry cell at ({}, {})",
            entry.0, entry.1
        ));
        return errors;
    };
    if entry_meta.role != termiflow::render::semantic::CellRole::ArrowTip {
        errors.push(format!(
            "semantic entry cell at ({}, {}) is {:?} instead of ArrowTip",
            entry.0, entry.1, entry_meta.role
        ));
    }
    let Some(gap_meta) = outcome.semantic_frame.get(gap.0, gap.1) else {
        errors.push(format!(
            "semantic frame has no gap cell at ({}, {})",
            gap.0, gap.1
        ));
        return errors;
    };
    if gap_meta.ch != ' ' || gap_meta.role != termiflow::render::semantic::CellRole::Empty {
        errors.push(format!(
            "semantic gap cell at ({}, {}) is {:?}/{:?}, not empty",
            gap.0, gap.1, gap_meta.ch, gap_meta.role
        ));
    }

    let min_x = graph
        .nodes
        .iter()
        .map(|node| node.x)
        .chain(graph.subgraphs.iter().map(|subgraph| subgraph.bounds.x))
        .min()
        .unwrap_or(0);
    let min_y = graph
        .nodes
        .iter()
        .map(|node| node.y)
        .chain(graph.subgraphs.iter().map(|subgraph| subgraph.bounds.y))
        .min()
        .unwrap_or(0);
    if raw_char_at(
        &outcome.output,
        entry.0.saturating_sub(min_x),
        entry.1.saturating_sub(min_y),
    ) != Some(entry_meta.ch)
    {
        errors.push("cropped raw frame disagrees with semantic entry glyph".to_string());
    }
    if raw_char_at(
        &outcome.output,
        gap.0.saturating_sub(min_x),
        gap.1.saturating_sub(min_y),
    ) != Some(' ')
    {
        errors.push("cropped raw frame lost the one-cell diamond entry gap".to_string());
    }

    errors
}

fn raw_portal_marker_errors(input: &str, style: BaseStyle, optimized: bool) -> Vec<String> {
    let mut graph = parse(input, false)
        .expect("parse portal oracle fixture")
        .graph;
    let mut config = Config::default();
    config.optimize_render = optimized;
    config.composite_style = CompositeStyle::from_base(style);
    config.spacing = config.spacing.for_direction(graph.direction);
    measure::measure_graph(&mut graph, &config);
    let (graph, outcome) =
        layout_and_render_with_feedback(graph, config).expect("render portal oracle fixture");

    raw_portal_marker_errors_for_frame(&graph, &outcome.output, style)
}

fn raw_portal_marker_errors_for_frame(
    graph: &termiflow::Graph,
    frame: &str,
    style: BaseStyle,
) -> Vec<String> {
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
    let rendered_char = |x: usize, y: usize| {
        raw_char_at(
            frame,
            x.saturating_sub(origin_x),
            y.saturating_sub(origin_y),
        )
    };
    let mut errors = Vec::new();

    for subgraph in &graph.subgraphs {
        let left_x = subgraph.bounds.x;
        let right_x = subgraph.bounds.x + subgraph.bounds.width.saturating_sub(1);
        let top_y = subgraph.bounds.y;
        let bottom_y = subgraph.bounds.y + subgraph.bounds.height.saturating_sub(1);
        let invalid_side_glyphs = [
            chars.cross,
            chars.junction_down,
            chars.junction_up,
            chars.junction_left,
            chars.junction_right,
        ];
        if matches!(
            graph.direction,
            termiflow::graph::Direction::TD | termiflow::graph::Direction::BT
        ) {
            let direct_parallel_crossings = if graph.direction == termiflow::graph::Direction::BT {
                let mut pair_counts = HashMap::<(&str, &str), usize>::new();
                for edge in &graph.edges {
                    if edge.is_back_edge {
                        continue;
                    }
                    let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
                    if exits.len() == 1
                        && enters.len() == 1
                        && (exits[0] == subgraph.id || enters[0] == subgraph.id)
                    {
                        *pair_counts.entry((exits[0], enters[0])).or_default() += 1;
                    }
                }
                pair_counts.values().copied().max().unwrap_or(0)
            } else {
                0
            };
            let top_markers = (left_x + 1..right_x)
                .filter(|x| rendered_char(*x, top_y) == Some(chars.edge_v))
                .count();
            let bottom_markers = (left_x + 1..right_x)
                .filter(|x| rendered_char(*x, bottom_y) == Some(chars.edge_v))
                .count();
            let top_junctions = (left_x + 1..right_x)
                .filter(|x| rendered_char(*x, top_y) == Some(chars.cross))
                .count();
            let bottom_junctions = (left_x + 1..right_x)
                .filter(|x| rendered_char(*x, bottom_y) == Some(chars.cross))
                .count();
            if direct_parallel_crossings >= 3 {
                if top_junctions + bottom_junctions > 0 {
                    errors.push(format!(
                        "{} has {} generic BT portal junctions; expected explicit BT portal seams",
                        subgraph.id,
                        top_junctions + bottom_junctions
                    ));
                }
                if top_markers + bottom_markers < direct_parallel_crossings {
                    errors.push(format!(
                        "{} has {} explicit BT portal seams for {} direct parallel crossings",
                        subgraph.id,
                        top_markers + bottom_markers,
                        direct_parallel_crossings
                    ));
                }
                for y in [top_y, bottom_y] {
                    for x in left_x + 1..right_x {
                        if rendered_char(x, y) != Some(chars.edge_v) {
                            continue;
                        }
                        let shaft_above = y > 0 && rendered_char(x, y - 1) == Some(chars.edge_v);
                        let shaft_below =
                            rendered_char(x, y.saturating_add(1)) == Some(chars.edge_v);
                        if !shaft_above && !shaft_below {
                            errors.push(format!(
                                "{} BT portal seam at ({x},{y}) has no adjacent vertical shaft",
                                subgraph.id
                            ));
                        }
                    }
                }
            } else if top_markers + bottom_markers == 0 {
                errors.push(format!(
                    "{} has no route-perpendicular top/bottom portal shaft",
                    subgraph.id
                ));
            }
        } else {
            for (side, x) in [("left", left_x), ("right", right_x)] {
                let mut markers = Vec::new();
                for y in top_y + 1..bottom_y {
                    let actual = rendered_char(x, y);
                    if actual == Some(chars.portal_pierce) {
                        markers.push(y);
                    }
                    if actual.is_some_and(|glyph| invalid_side_glyphs.contains(&glyph)) {
                        errors.push(format!(
                            "{} {side} border at ({x},{y}) rendered junction-like glyph {:?}",
                            subgraph.id, actual
                        ));
                    }
                }
                if !markers.is_empty() {
                    break;
                }
                if side == "right" {
                    errors.push(format!(
                        "{} has no dedicated left/right portal marker {:?}",
                        subgraph.id, chars.portal_pierce
                    ));
                }
            }
        }
    }

    errors
}

fn replace_raw_char(frame: &str, x: usize, y: usize, replacement: char) -> String {
    let mut lines: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
    if let Some(line) = lines.get_mut(y) {
        if let Some(cell) = line.get_mut(x) {
            *cell = replacement;
        }
    }
    lines
        .into_iter()
        .map(|line| line.into_iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn shared_fan_in_raw_frame_errors(input: &str, frame: &str) -> Vec<String> {
    let parsed = parse(input, false).expect("parse fan-in oracle input");
    let mut errors = Vec::new();
    if frame.trim().is_empty() {
        errors.push("rendered frame is empty".to_string());
    }
    for node in &parsed.graph.nodes {
        if !node.label.is_empty() && !frame.contains(&node.label) {
            errors.push(format!(
                "node label {:?} is absent from raw frame",
                node.label
            ));
        }
    }

    let expected_arrowheads = if dedicated_fan_in_identity_family(&parsed.graph)
        || wide_terminal_fan_in_identity_family(&parsed.graph)
        || dual_junction_identity_family(&parsed.graph)
        || nonterminal_vertical_fan_in_identity_family(&parsed.graph)
        || labeled_terminal_fan_in_identity_family(&parsed.graph)
        || internal_subgraph_nonterminal_fan_in_identity_family(&parsed.graph)
    {
        parsed
            .graph
            .edges
            .iter()
            .filter(|edge| !edge.is_back_edge)
            .count()
    } else if ordinary_fan_in_identity_family(&parsed.graph) {
        parsed
            .graph
            .nodes
            .iter()
            .map(|target| {
                let incoming = parsed
                    .graph
                    .edges
                    .iter()
                    .filter(|edge| !edge.is_back_edge && edge.to == target.id)
                    .count();
                ordinary_target_port_count(&parsed.graph, &target.id).max(incoming.min(1))
            })
            .sum()
    } else {
        parsed
            .graph
            .nodes
            .iter()
            .filter(|node| {
                parsed
                    .graph
                    .edges
                    .iter()
                    .any(|edge| !edge.is_back_edge && edge.to == node.id)
            })
            .count()
    };
    errors.extend(raw_topology_errors(frame, expected_arrowheads));
    errors
}

/// Independent mirror of the exact labeled terminal identity selector. This
/// deliberately does not call renderer policy code: the raw-frame oracle must
/// fail if a shared target arrow makes two labeled edges look like one.
fn labeled_terminal_fan_in_identity_family(graph: &termiflow::Graph) -> bool {
    if !matches!(
        graph.direction,
        termiflow::graph::Direction::TD
            | termiflow::graph::Direction::TB
            | termiflow::graph::Direction::BT
            | termiflow::graph::Direction::LR
            | termiflow::graph::Direction::RL
    ) || graph.nodes.len() != 3
        || graph.edges.len() != 2
        || !graph.subgraphs.is_empty()
        || graph.has_cycles()
        || graph
            .nodes
            .iter()
            .any(|node| node.shape != termiflow::graph::NodeShape::Rectangle)
    {
        return false;
    }

    let Some(target) = graph.nodes.iter().find(|node| {
        graph.edges.iter().filter(|edge| edge.to == node.id).count() == 2
            && graph
                .edges
                .iter()
                .filter(|edge| edge.from == node.id)
                .count()
                == 0
    }) else {
        return false;
    };

    let incoming: Vec<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.to == target.id)
        .map(|edge| edge.from.as_str())
        .collect();
    let source_ids: HashSet<&str> = incoming.iter().copied().collect();
    let node_ids: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    incoming.len() == 2
        && source_ids.len() == incoming.len()
        && source_ids.iter().all(|source| {
            node_ids.contains(source)
                && graph
                    .nodes
                    .iter()
                    .find(|node| node.id == *source)
                    .is_some_and(|node| node.shape == termiflow::graph::NodeShape::Rectangle)
        })
        && graph.edges.iter().all(|edge| {
            edge.to == target.id
                && edge.from != target.id
                && edge.kind == termiflow::graph::EdgeKind::Arrow
                && edge.label.is_some()
                && !edge.is_back_edge
        })
}

/// Independent mirror of the internal titled-subgraph nonterminal selector.
fn internal_subgraph_nonterminal_fan_in_identity_family(graph: &termiflow::Graph) -> bool {
    if !matches!(
        graph.direction,
        termiflow::graph::Direction::TD
            | termiflow::graph::Direction::TB
            | termiflow::graph::Direction::BT
            | termiflow::graph::Direction::LR
            | termiflow::graph::Direction::RL
    ) || graph.subgraphs.len() != 1
        || graph.has_cycles()
    {
        return false;
    }

    let subgraph = &graph.subgraphs[0];
    if subgraph.title.is_none() || subgraph.parent_id.is_some() || !subgraph.child_ids.is_empty() {
        return false;
    }

    graph.nodes.iter().any(|target| {
        if target.shape != termiflow::graph::NodeShape::Rectangle
            || graph.get_node_subgraph(&target.id) != Some(subgraph.id.as_str())
        {
            return false;
        }
        let incoming: Vec<_> = graph
            .edges
            .iter()
            .filter(|edge| edge.to == target.id)
            .collect();
        if incoming.len() != 2
            || incoming.iter().any(|edge| {
                edge.is_back_edge
                    || edge.kind != termiflow::graph::EdgeKind::Arrow
                    || edge.label.is_some()
            })
        {
            return false;
        }
        let source_ids: HashSet<&str> = incoming.iter().map(|edge| edge.from.as_str()).collect();
        if source_ids.len() != incoming.len()
            || source_ids.iter().any(|source| {
                graph.get_node_subgraph(source) != Some(subgraph.id.as_str())
                    || graph
                        .nodes
                        .iter()
                        .find(|node| node.id == *source)
                        .is_none_or(|node| node.shape != termiflow::graph::NodeShape::Rectangle)
                    || graph
                        .edges
                        .iter()
                        .filter(|edge| edge.from == *source)
                        .count()
                        != 1
            })
        {
            return false;
        }

        let outgoing: Vec<_> = graph
            .edges
            .iter()
            .filter(|edge| edge.from == target.id)
            .collect();
        outgoing.len() == 1
            && outgoing[0].to != target.id
            && !source_ids.contains(outgoing[0].to.as_str())
            && graph
                .get_node(&outgoing[0].to)
                .is_some_and(|node| node.shape == termiflow::graph::NodeShape::Rectangle)
            && outgoing[0].kind == termiflow::graph::EdgeKind::Arrow
            && outgoing[0].label.is_none()
            && !outgoing[0].is_back_edge
    })
}

/// Independent structural mirror of the bounded ordinary fan-in identity
/// policy. Keep this separate from renderer-owned policy code so a shared
/// target arrow cannot make the raw-frame oracle pass accidentally.
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

    let node_ids: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    graph.nodes.iter().any(|target| {
        let incoming: Vec<&str> = graph
            .edges
            .iter()
            .filter(|edge| edge.to == target.id)
            .map(|edge| edge.from.as_str())
            .collect();
        let source_ids: HashSet<&str> = incoming.iter().copied().collect();
        (2..=4).contains(&incoming.len())
            && source_ids.len() == incoming.len()
            && !source_ids.contains(target.id.as_str())
            && source_ids.iter().all(|source| node_ids.contains(source))
    })
}

fn ordinary_target_port_count(graph: &termiflow::Graph, target_id: &str) -> usize {
    let Some(target) = graph.nodes.iter().find(|node| node.id == target_id) else {
        return 0;
    };
    let incoming: Vec<&str> = graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge && edge.to == target.id)
        .map(|edge| edge.from.as_str())
        .collect();
    let source_ids: HashSet<&str> = incoming.iter().copied().collect();
    let node_ids: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    if (2..=4).contains(&incoming.len())
        && source_ids.len() == incoming.len()
        && !source_ids.contains(target.id.as_str())
        && source_ids.iter().all(|source| node_ids.contains(source))
    {
        incoming.len()
    } else {
        0
    }
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

    if horizontal_branch_rejoin_identity_family(graph)
        || horizontal_mixed_junction_identity_family(graph)
    {
        return true;
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

    if graph.nodes.len() != 3 || graph.edges.len() != 3 {
        return false;
    }
    graph.nodes.iter().any(|target| {
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

fn horizontal_branch_rejoin_identity_family(graph: &termiflow::Graph) -> bool {
    if !matches!(
        graph.direction,
        termiflow::graph::Direction::LR | termiflow::graph::Direction::RL
    ) || graph.nodes.len() != 4
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
        return false;
    }

    let node_ids: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    if node_ids.len() != graph.nodes.len() {
        return false;
    }

    let incoming_count =
        |node_id: &str| graph.edges.iter().filter(|edge| edge.to == node_id).count();
    let outgoing_count = |node_id: &str| {
        graph
            .edges
            .iter()
            .filter(|edge| edge.from == node_id)
            .count()
    };
    let targets: Vec<&str> = graph
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .filter(|node_id| incoming_count(node_id) == 2 && outgoing_count(node_id) == 0)
        .collect();
    let sources: Vec<&str> = graph
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .filter(|node_id| incoming_count(node_id) == 0 && outgoing_count(node_id) == 2)
        .collect();
    if targets.len() != 1 || sources.len() != 1 {
        return false;
    }

    let target_id = targets[0];
    let source_id = sources[0];
    let incoming_ids: HashSet<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.to == target_id)
        .map(|edge| edge.from.as_str())
        .collect();
    let branch_ids: HashSet<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.from == source_id)
        .map(|edge| edge.to.as_str())
        .collect();
    if incoming_ids.len() != 2
        || branch_ids != incoming_ids
        || branch_ids.contains(source_id)
        || branch_ids.contains(target_id)
    {
        return false;
    }

    if branch_ids.iter().any(|branch_id| {
        incoming_count(branch_id) != 1
            || graph
                .edges
                .iter()
                .filter(|edge| edge.from == *branch_id && edge.to == target_id)
                .count()
                != 1
    }) {
        return false;
    }

    let expected_ids: HashSet<&str> = branch_ids
        .iter()
        .copied()
        .chain(std::iter::once(source_id))
        .chain(std::iter::once(target_id))
        .collect();
    expected_ids == node_ids
}

fn horizontal_mixed_junction_identity_family(graph: &termiflow::Graph) -> bool {
    if !matches!(
        graph.direction,
        termiflow::graph::Direction::LR | termiflow::graph::Direction::RL
    ) || graph.nodes.len() != 5
        || graph.edges.len() != 6
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

    let node_ids: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    if node_ids.len() != graph.nodes.len() {
        return false;
    }

    let incoming_count =
        |node_id: &str| graph.edges.iter().filter(|edge| edge.to == node_id).count();
    let outgoing_count = |node_id: &str| {
        graph
            .edges
            .iter()
            .filter(|edge| edge.from == node_id)
            .count()
    };
    let targets: Vec<&str> = graph
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .filter(|node_id| incoming_count(node_id) == 3 && outgoing_count(node_id) == 0)
        .collect();
    let sources: Vec<&str> = graph
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .filter(|node_id| incoming_count(node_id) == 0 && outgoing_count(node_id) == 3)
        .collect();
    if targets.len() != 1 || sources.len() != 1 {
        return false;
    }

    let target_id = targets[0];
    let source_id = sources[0];
    let incoming_ids: HashSet<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.to == target_id)
        .map(|edge| edge.from.as_str())
        .collect();
    let branch_ids: HashSet<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.from == source_id)
        .map(|edge| edge.to.as_str())
        .collect();
    if incoming_ids.len() != 3
        || branch_ids != incoming_ids
        || branch_ids.contains(source_id)
        || branch_ids.contains(target_id)
    {
        return false;
    }

    if branch_ids.iter().any(|branch_id| {
        incoming_count(branch_id) != 1
            || graph
                .edges
                .iter()
                .filter(|edge| edge.from == *branch_id && edge.to == target_id)
                .count()
                != 1
    }) {
        return false;
    }

    let expected_ids: HashSet<&str> = branch_ids
        .iter()
        .copied()
        .chain(std::iter::once(source_id))
        .chain(std::iter::once(target_id))
        .collect();
    expected_ids == node_ids
}

fn wide_terminal_fan_in_identity_family(graph: &termiflow::Graph) -> bool {
    if !matches!(
        graph.direction,
        termiflow::graph::Direction::TD
            | termiflow::graph::Direction::BT
            | termiflow::graph::Direction::LR
            | termiflow::graph::Direction::RL
    ) || !graph.subgraphs.is_empty()
        || !(5..=9).contains(&graph.nodes.len())
        || !(4..=8).contains(&graph.edges.len())
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

    let incoming_count =
        |node_id: &str| graph.edges.iter().filter(|edge| edge.to == node_id).count();
    let outgoing_count = |node_id: &str| {
        graph
            .edges
            .iter()
            .filter(|edge| edge.from == node_id)
            .count()
    };
    let targets: Vec<_> = graph
        .nodes
        .iter()
        .filter(|node| {
            let count = incoming_count(&node.id);
            (4..=8).contains(&count) && outgoing_count(&node.id) == 0
        })
        .collect();
    let Some(target) = targets.first() else {
        return false;
    };
    if targets.len() != 1 || incoming_count(&target.id) != graph.edges.len() {
        return false;
    }

    let source_ids: HashSet<&str> = graph.edges.iter().map(|edge| edge.from.as_str()).collect();
    source_ids.len() == graph.edges.len()
        && source_ids.len() + 1 == graph.nodes.len()
        && source_ids.iter().all(|source| *source != target.id)
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

    let node_ids: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
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
    if incoming.len() != 2
        || incoming[0] == incoming[1]
        || incoming.contains(&downstream)
        || downstream == target.id
    {
        return false;
    }

    incoming
        .iter()
        .copied()
        .chain(std::iter::once(target.id.as_str()))
        .chain(std::iter::once(downstream))
        .collect::<HashSet<_>>()
        == node_ids
}

fn dual_junction_identity_family(graph: &termiflow::Graph) -> bool {
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
        return false;
    }
    let node_ids: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
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
                == 2
    }) else {
        return false;
    };
    let incoming: HashSet<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.to == target.id)
        .map(|edge| edge.from.as_str())
        .collect();
    let outgoing: HashSet<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.from == target.id)
        .map(|edge| edge.to.as_str())
        .collect();
    let expected: HashSet<&str> = incoming
        .iter()
        .copied()
        .chain(std::iter::once(target.id.as_str()))
        .chain(outgoing.iter().copied())
        .collect();
    incoming.len() == 2
        && outgoing.len() == 2
        && incoming.is_disjoint(&outgoing)
        && expected == node_ids
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

fn char_index_of(line: &str, needle: &str) -> Option<usize> {
    let needle_chars: Vec<char> = needle.chars().collect();
    if needle_chars.is_empty() {
        return Some(0);
    }
    let chars: Vec<char> = line.chars().collect();
    chars
        .windows(needle_chars.len())
        .position(|window| window == needle_chars.as_slice())
}

fn crossing_grid_raw_frame_errors(input: &str, frame: &str) -> Vec<String> {
    let parsed = parse(input, false).expect("parse crossing-grid oracle input");
    let lines: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
    let mut errors = Vec::new();

    if frame.trim().is_empty() {
        errors.push("crossing-grid frame is empty".to_string());
        return errors;
    }
    for node in &parsed.graph.nodes {
        if !frame.contains(&node.label) {
            errors.push(format!(
                "crossing-grid node label {:?} is absent from raw frame",
                node.label
            ));
        }
    }

    let horizontal = |ch: char| matches!(ch, '-' | '=' | '─' | '═' | '━' | '╌');
    let vertical = |ch: char| matches!(ch, '|' | ':' | '│' | '║' | '┃' | '╎');

    match parsed.graph.direction {
        termiflow::graph::Direction::TD
        | termiflow::graph::Direction::TB
        | termiflow::graph::Direction::BT => {
            let mut label_rows = ["Node A1", "Node B3", "Node C1"]
                .iter()
                .filter_map(|needle| {
                    lines.iter().position(|line| {
                        line.windows(needle.chars().count())
                            .any(|window| window == needle.chars().collect::<Vec<_>>())
                    })
                })
                .collect::<Vec<_>>();
            label_rows.sort_unstable();
            if label_rows.len() != 3 {
                errors.push(format!(
                    "crossing-grid expected three layer label rows, found {}",
                    label_rows.len()
                ));
            } else {
                for pair in label_rows.windows(2) {
                    let start = pair[0].saturating_add(2);
                    let end = pair[1].saturating_sub(2);
                    let bands = if start <= end {
                        (start..=end)
                            .filter(|row| {
                                lines[*row].iter().filter(|ch| horizontal(**ch)).count() >= 4
                            })
                            .count()
                    } else {
                        0
                    };
                    if bands < 2 {
                        errors.push(format!(
                            "crossing-grid layer corridor {}..{} has {bands} distinct horizontal merge bands; expected at least 2",
                            pair[0], pair[1]
                        ));
                    }
                }
            }
        }
        termiflow::graph::Direction::LR | termiflow::graph::Direction::RL => {
            let mut label_columns = ["Node A1", "Node B3", "Node C1"]
                .iter()
                .filter_map(|needle| {
                    lines
                        .iter()
                        .find_map(|line| char_index_of(&line.iter().collect::<String>(), needle))
                })
                .collect::<Vec<_>>();
            label_columns.sort_unstable();
            if label_columns.len() != 3 {
                errors.push(format!(
                    "crossing-grid expected three layer label columns, found {}",
                    label_columns.len()
                ));
            } else {
                for pair in label_columns.windows(2) {
                    let start = pair[0].saturating_add(10);
                    let end = pair[1].saturating_sub(3);
                    let bands = if start <= end {
                        (start..=end)
                            .filter(|column| {
                                lines
                                    .iter()
                                    .filter_map(|line| line.get(*column))
                                    .filter(|ch| vertical(**ch))
                                    .count()
                                    >= 2
                            })
                            .count()
                    } else {
                        0
                    };
                    if bands < 2 {
                        errors.push(format!(
                            "crossing-grid layer corridor {}..{} has {bands} distinct vertical merge bands; expected at least 2",
                            pair[0], pair[1]
                        ));
                    }
                }
            }
        }
    }
    errors
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

fn explicit_crossing_marker_raw_frame_errors(
    input: &str,
    frame: &str,
    style: BaseStyle,
) -> Vec<String> {
    let parsed = parse(input, false).expect("parse explicit-crossing oracle input");
    if !dense_crossing_input(&parsed.graph) {
        return Vec::new();
    }

    let marker = match style {
        BaseStyle::Ascii => 'x',
        BaseStyle::Unicode => '✕',
        _ => unreachable!("explicit-crossing oracle only exercises ASCII and Unicode"),
    };
    let lines: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
    let positions = lines
        .iter()
        .enumerate()
        .flat_map(|(y, row)| {
            row.iter()
                .enumerate()
                .filter_map(move |(x, ch)| (*ch == marker).then_some((x, y)))
        })
        .collect::<Vec<_>>();
    let mut errors = Vec::new();
    if positions.is_empty() {
        errors.push(format!(
            "dense crossing frame has no explicit {marker:?} marker"
        ));
        return errors;
    }

    let route_neighbor = |ch: char| {
        matches!(
            ch,
            '-' | '='
                | '|'
                | ':'
                | '─'
                | '│'
                | '═'
                | '║'
                | '━'
                | '┃'
                | '╌'
                | '╎'
                | '+'
                | '┼'
                | '├'
                | '┤'
                | '┬'
                | '┴'
                | '╠'
                | '╣'
                | '╦'
                | '╩'
                | '╬'
                | '╋'
                | '┌'
                | '┐'
                | '└'
                | '┘'
                | '╔'
                | '╗'
                | '╚'
                | '╝'
        )
    };
    let arrow = |ch: char| {
        matches!(
            ch,
            '>' | '<' | '^' | 'v' | '→' | '←' | '↑' | '↓' | '▶' | '◀' | '▲' | '▼'
        )
    };
    let at = |x: usize, y: usize| {
        lines
            .get(y)
            .and_then(|row| row.get(x))
            .copied()
            .unwrap_or(' ')
    };

    for (x, y) in positions {
        let left = at(x.saturating_sub(1), y);
        let right = at(x.saturating_add(1), y);
        let up = at(x, y.saturating_sub(1));
        let down = at(x, y.saturating_add(1));
        if [left, right, up, down].iter().any(|ch| arrow(*ch)) {
            errors.push(format!(
                "explicit crossing marker at ({x},{y}) touches an arrow endpoint"
            ));
        }
        if !route_neighbor(left)
            || !route_neighbor(right)
            || !route_neighbor(up)
            || !route_neighbor(down)
        {
            errors.push(format!(
                "explicit crossing marker at ({x},{y}) lacks four visible route arms"
            ));
        }

        for node in &parsed.graph.nodes {
            let label_chars = node.label.chars().collect::<Vec<_>>();
            if label_chars.is_empty() {
                continue;
            }
            let inside_label = lines.iter().enumerate().any(|(label_y, row)| {
                row.windows(label_chars.len())
                    .enumerate()
                    .any(|(label_x, window)| {
                        window == label_chars.as_slice()
                            && label_y == y
                            && x >= label_x
                            && x < label_x + label_chars.len()
                    })
            });
            if inside_label {
                errors.push(format!(
                    "explicit crossing marker at ({x},{y}) is inside node label {:?}",
                    node.label
                ));
            }
        }
    }
    errors
}

const ROUTE_NONE: u8 = 4;

fn route_identity_mask(ch: char) -> u8 {
    match ch {
        '-' | '─' | '═' | '━' => 0b1010,
        '|' | '│' | '║' | '┃' => 0b0101,
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

    // ASCII uses `+` for both a corner and a junction. Infer the actual arms
    // from neighboring route glyphs so a two-arm elbow does not become a
    // false four-way branch in the independent raw-frame oracle.
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

fn route_identity_step(direction: termiflow::graph::Direction, index: u8) -> (isize, isize) {
    match (direction, index) {
        (_, 0) => (0, -1),
        (_, 1) => (1, 0),
        (_, 2) => (0, 1),
        (_, 3) => (-1, 0),
        _ => unreachable!("route identity has only four cell directions"),
    }
}

fn route_identity_opposite(index: u8) -> u8 {
    match index {
        0 => 2,
        1 => 3,
        2 => 0,
        3 => 1,
        _ => unreachable!("route identity has only four cell directions"),
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
        || mask_at(source_start.0, source_start.1) == 0
    {
        return HashSet::new();
    }

    // State includes the direction back to the previous cell.  This is what
    // keeps an explicit x/✕ crossing as two straight-through channels instead
    // of silently turning it into a four-way junction.
    let mut queue =
        std::collections::VecDeque::from([(source_start.0, source_start.1, ROUTE_NONE)]);
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

fn crossing_grid_route_identity_errors(
    input: &str,
    frame: &str,
    style: BaseStyle,
    optimized: bool,
) -> Vec<String> {
    let mut graph = parse(input, false)
        .expect("parse route-identity oracle input")
        .graph;
    let mut config = Config::default();
    config.optimize_render = optimized;
    config.composite_style = CompositeStyle::from_base(style);
    config.spacing = config.spacing.for_direction(graph.direction);
    measure::measure_graph(&mut graph, &config);
    let (graph, _) =
        layout_and_render_with_feedback(graph, config).expect("layout route-identity oracle input");
    if !dense_crossing_input(&graph) {
        return Vec::new();
    }
    let origin_x = graph.nodes.iter().map(|node| node.x).min().unwrap_or(0);
    let origin_y = graph.nodes.iter().map(|node| node.y).min().unwrap_or(0);
    let mut errors = Vec::new();

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
            errors.push(format!(
                "dense source {} has {} outgoing edges but {} raw ports",
                source.id,
                outgoing.len(),
                source_ports.len()
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
            errors.push(format!(
                "route-identity corridor for {} has no measurable span",
                source.id
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
            let reachable = route_identity_reachable_targets(
                frame,
                &graph,
                source_start,
                &candidates,
                primary_range,
                origin_x,
                origin_y,
            );
            reachable_targets.extend(reachable);
        }
        for target in &candidates {
            let is_declared = declared_targets.contains(target.id.as_str());
            let is_reachable = reachable_targets.contains(&target.id);
            if is_declared && !is_reachable {
                errors.push(format!(
                    "declared edge {} -> {} has no raw-frame continuation",
                    source.id, target.id
                ));
            } else if !is_declared && is_reachable {
                errors.push(format!(
                    "source {} can physically continue to undeclared target {} through the same raw corridor",
                    source.id, target.id
                ));
            }
        }
    }
    errors
}

fn junction_quad_raw_frame_errors(input: &str, frame: &str) -> Vec<String> {
    let parsed = parse(input, false).expect("parse junction quad oracle input");
    let (_, _, _, expected_arrows) = dual_junction_shape(&parsed.graph);
    let mut errors = Vec::new();
    for node in &parsed.graph.nodes {
        if !node.label.is_empty() && !frame.contains(&node.label) {
            errors.push(format!(
                "node label {:?} is absent from raw frame",
                node.label
            ));
        }
    }
    // The dual-junction policy requires one visible arrowhead per semantic
    // edge. The independent geometry trace separately requires every edge
    // record to survive alongside the raw-frame shaft/arrow checks.
    errors.extend(raw_topology_errors(frame, expected_arrows));
    let arrows = frame
        .chars()
        .filter(|ch| {
            matches!(
                ch,
                '>' | '<' | '^' | 'v' | '→' | '←' | '↑' | '↓' | '▶' | '◀' | '▲' | '▼'
            )
        })
        .count();
    if arrows != expected_arrows {
        errors.push(format!(
            "dual-junction raw frame has {arrows} arrowheads for {expected_arrows} semantic edges"
        ));
    }
    errors
}

fn dual_junction_shape(graph: &termiflow::Graph) -> (String, Vec<String>, usize, usize) {
    let candidates: Vec<_> = graph
        .nodes
        .iter()
        .filter_map(|node| {
            let incoming: HashSet<&str> = graph
                .edges
                .iter()
                .filter(|edge| edge.to == node.id && edge.from != node.id)
                .map(|edge| edge.from.as_str())
                .collect();
            let outgoing: HashSet<&str> = graph
                .edges
                .iter()
                .filter(|edge| edge.from == node.id && edge.to != node.id)
                .map(|edge| edge.to.as_str())
                .collect();
            (incoming.len() >= 2 && outgoing.len() >= 2).then(|| {
                let mut targets: Vec<String> = outgoing.into_iter().map(str::to_owned).collect();
                targets.sort_unstable();
                (node.id.clone(), targets, incoming.len())
            })
        })
        .collect();
    assert_eq!(
        candidates.len(),
        1,
        "expected exactly one dual-junction anchor"
    );
    let (anchor, targets, _incoming_count) = candidates
        .into_iter()
        .next()
        .expect("dual-junction candidate");
    let semantic_edges = graph.edges.len();
    let expected_arrows = semantic_edges;
    (anchor, targets, semantic_edges, expected_arrows)
}

fn pure_fan_in_shape(graph: &termiflow::Graph) -> (String, Vec<String>, usize) {
    let candidates: Vec<_> = graph
        .nodes
        .iter()
        .filter_map(|node| {
            let incoming: HashSet<&str> = graph
                .edges
                .iter()
                .filter(|edge| !edge.is_back_edge && edge.to == node.id && edge.from != node.id)
                .map(|edge| edge.from.as_str())
                .collect();
            let outgoing = graph
                .edges
                .iter()
                .any(|edge| !edge.is_back_edge && edge.from == node.id && edge.to != node.id);
            if incoming.len() < 2 || outgoing {
                return None;
            }

            let mut sources: Vec<String> = incoming.into_iter().map(str::to_owned).collect();
            sources.sort_unstable();
            Some((node.id.clone(), sources))
        })
        .collect();
    assert_eq!(
        candidates.len(),
        1,
        "expected exactly one pure fan-in target"
    );
    let (anchor, sources) = candidates
        .into_iter()
        .next()
        .expect("pure fan-in candidate");
    (anchor, sources, graph.edges.len())
}

fn raw_topology_errors(frame: &str, expected_edges: usize) -> Vec<String> {
    let mut errors = Vec::new();
    let cells: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
    let arrows = cells
        .iter()
        .flat_map(|row| row.iter())
        .filter(|ch| {
            matches!(
                ch,
                '>' | '<' | '^' | 'v' | '→' | '←' | '↑' | '↓' | '▶' | '◀' | '▲' | '▼'
            )
        })
        .count();
    if arrows < expected_edges {
        errors.push(format!(
            "raw frame has {arrows} arrowheads for {expected_edges} directed edges",
        ));
    }

    for (y, row) in cells.iter().enumerate() {
        for (x, ch) in row.iter().enumerate() {
            let predecessor = match ch {
                'v' | '↓' | '▼' => y
                    .checked_sub(1)
                    .and_then(|py| cells.get(py).and_then(|r| r.get(x))),
                '^' | '↑' | '▲' => cells.get(y + 1).and_then(|r| r.get(x)),
                '>' | '→' | '▶' => x.checked_sub(1).and_then(|px| row.get(px)),
                '<' | '←' | '◀' => row.get(x + 1),
                _ => continue,
            };
            if !predecessor.is_some_and(|glyph| is_route_glyph(*glyph)) {
                errors.push(format!(
                    "arrowhead at ({x},{y}) lacks a visible raw-frame shaft"
                ));
            }
        }
    }
    errors
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
            | '╋'
    )
}

fn node_geometry_errors(trace: &GeometryTrace) -> Vec<String> {
    let mut errors = Vec::new();
    for node in &trace.nodes {
        if node.width == 0 || node.height == 0 {
            errors.push(format!("node {} has an empty rectangle", node.id));
        }
    }
    for (index, left) in trace.nodes.iter().enumerate() {
        for right in &trace.nodes[index + 1..] {
            if rectangles_overlap(
                &RectTrace {
                    x: left.x,
                    y: left.y,
                    width: left.width,
                    height: left.height,
                },
                &RectTrace {
                    x: right.x,
                    y: right.y,
                    width: right.width,
                    height: right.height,
                },
            ) {
                errors.push(format!(
                    "node rectangles {} and {} overlap",
                    left.id, right.id
                ));
            }
        }
    }
    errors
}

fn geometry_errors(trace: &GeometryTrace) -> Vec<String> {
    let mut errors = node_geometry_errors(trace);
    for edge in &trace.edges {
        if !trace.nodes.iter().any(|node| node.id == edge.from)
            || !trace.nodes.iter().any(|node| node.id == edge.to)
        {
            errors.push(format!("edge {} has an unknown endpoint", edge.owner_id));
        }
        if edge.segments.is_empty() {
            errors.push(format!("edge {} has no geometry segments", edge.owner_id));
        }
    }
    errors
}

fn rectangles_overlap(left: &RectTrace, right: &RectTrace) -> bool {
    left.x < right.x.saturating_add(right.width)
        && right.x < left.x.saturating_add(left.width)
        && left.y < right.y.saturating_add(right.height)
        && right.y < left.y.saturating_add(left.height)
}

#[test]
fn raw_frame_oracle_covers_ascii_unicode_and_all_directions() {
    for direction in ["TD", "LR", "BT", "RL"] {
        let input = format!("graph {direction}\nA[Alpha] --> B[Beta]");
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            let output = termiflow::render(&input, RenderOptions::new().with_style(style))
                .expect("render oracle fixture");
            assert!(
                raw_frame_errors(&input, &output).is_empty(),
                "{direction} {style:?}:\n{output}"
            );
        }
    }
}

#[test]
fn raw_frame_oracle_rejects_label_and_shaft_mutations() {
    let input = "graph TD\nA[Alpha] --> B[Beta]";
    let output = termiflow::render(input, RenderOptions::new().with_style(BaseStyle::Ascii))
        .expect("render mutation fixture");
    assert!(raw_frame_errors(input, &output.replace("Alpha", "     "))
        .iter()
        .any(|error| error.contains("Alpha")));

    let shaftless = output.replace(['|', '-', '+'], " ");
    assert!(raw_frame_errors(input, &shaftless)
        .iter()
        .any(|error| error.contains("lacks a visible")));
}

#[test]
fn td_single_subgraph_route_transaction_has_clean_portal_attachments() {
    let input = fs::read_to_string("tests/fixtures/inputs/subgraph_single_td.md")
        .expect("read TD single-subgraph route fixture");

    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        let (title_portal, bottom_portal) = match style {
            BaseStyle::Ascii => ("Container   |", "+----------|----------+"),
            BaseStyle::Unicode => ("Container   │", "┗━━━━━━━━━━│━━━━━━━━━━┛"),
            _ => unreachable!("focused oracle only exercises ASCII and Unicode"),
        };
        for optimized in [false, true] {
            let output = termiflow::render(
                &input,
                RenderOptions::new()
                    .with_style(style)
                    .with_optimize_render(optimized),
            )
            .expect("render TD single-subgraph route fixture");

            let vertical = match style {
                BaseStyle::Ascii => '|',
                BaseStyle::Unicode => '│',
                _ => unreachable!("focused oracle only exercises ASCII and Unicode"),
            };
            let lines: Vec<&str> = output.lines().collect();
            let start_row = lines
                .iter()
                .position(|line| line.contains("Start"))
                .expect("single-subgraph source label");
            let approach_row = lines
                .get(start_row.saturating_add(2))
                .copied()
                .expect("single-subgraph source approach row");
            assert_eq!(
                approach_row.chars().filter(|cell| *cell == vertical).count(),
                1,
                "source should have one vertical approach shaft for {style:?} optimized={optimized}:\n{output}"
            );
            assert!(
                approach_row
                    .chars()
                    .all(|cell| cell == ' ' || cell == vertical),
                "source approach row should not contain a horizontal elbow for {style:?} optimized={optimized}:\n{output}"
            );

            assert!(
                output.contains(title_portal),
                "title/portal attachment changed for {style:?} optimized={optimized}:\n{output}"
            );
            assert!(
                output.contains(bottom_portal),
                "bottom portal attachment changed for {style:?} optimized={optimized}:\n{output}"
            );

            let forbidden = match style {
                BaseStyle::Ascii => vec!["+-+", "++", "||", "|v", "v|"],
                BaseStyle::Unicode => vec!["│↓", "↓│"],
                _ => unreachable!("focused oracle only exercises ASCII and Unicode"),
            };
            for artifact in forbidden {
                assert!(
                    !output.contains(artifact),
                    "route artifact {artifact:?} remained for {style:?} optimized={optimized}:\n{output}"
                );
            }
        }
    }
}

#[test]
fn td_single_entry_alignment_rejects_multi_entry_subgraph() {
    let input = "graph TD
Before1[First]
Before2[Second]
subgraph SG1 [Container]
Solo[Single Node]
end
After[End]
Before1 --> Solo
Before2 --> Solo
Solo --> After
";

    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let mut graph = parse(input, false)
                .expect("parse multi-entry negative control")
                .graph;
            let mut config = Config::default();
            config.optimize_render = optimized;
            config.composite_style = CompositeStyle::from_base(style);
            config.spacing = config.spacing.for_direction(graph.direction);
            measure::measure_graph(&mut graph, &config);

            let (graph, outcome) = layout_and_render_with_feedback(graph, config)
                .expect("render multi-entry negative control");
            let first = graph
                .get_node("Before1")
                .expect("first external source after layout");
            let second = graph
                .get_node("Before2")
                .expect("second external source after layout");
            assert_ne!(
                first.center_x(),
                second.center_x(),
                "ambiguous multi-entry scene collapsed source lanes for {style:?} optimized={optimized}:\n{}",
                outcome.output
            );
            assert!(
                outcome.output.contains("First") && outcome.output.contains("Second"),
                "multi-entry negative control lost a source label for {style:?} optimized={optimized}:\n{}",
                outcome.output
            );
        }
    }
}

#[test]
fn td_parallel_subgraph_portals_use_topology_owned_seams() {
    let input = fs::read_to_string("tests/fixtures/inputs/subgraph_parallel_td.md")
        .expect("read TD parallel-subgraph seam fixture");

    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let output = termiflow::render(
                &input,
                RenderOptions::new()
                    .with_composite_style(CompositeStyle::from_base(style))
                    .with_optimize_render(optimized),
            )
            .expect("render TD parallel-subgraph seam fixture");

            match style {
                BaseStyle::Ascii => {
                    assert!(
                        output.lines().any(|line| {
                            line.starts_with('+')
                                && line.ends_with('+')
                                && line.matches('+').count() >= 3
                        }),
                        "ASCII top portal seam missing optimized={optimized}:\n{output}"
                    );
                    assert!(
                        output.lines().rev().any(|line| {
                            line.starts_with('+')
                                && line.ends_with('+')
                                && line.matches('+').count() >= 3
                        }),
                        "ASCII bottom portal seam missing optimized={optimized}:\n{output}"
                    );
                }
                BaseStyle::Unicode => {
                    assert!(
                        output.lines().any(|line| {
                            line.starts_with('┌') && line.ends_with('┐') && line.contains('┬')
                        }),
                        "Unicode top portal seam missing optimized={optimized}:\n{output}"
                    );
                    assert!(
                        output.lines().rev().any(|line| {
                            line.starts_with('└') && line.ends_with('┘') && line.contains('┴')
                        }),
                        "Unicode bottom portal seam missing optimized={optimized}:\n{output}"
                    );
                }
                _ => unreachable!("focused oracle only exercises ASCII and Unicode"),
            }
        }
    }
}

#[test]
fn collision_parallel_portal_oracle_covers_direction_style_and_mode_matrix() {
    for direction in ["TD", "BT", "LR", "RL"] {
        let input = fs::read_to_string(format!(
            "tests/fixtures/inputs/collision_parallel_edges_{}.md",
            direction.to_lowercase()
        ))
        .expect("read collision parallel portal fixture");

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let errors = raw_portal_marker_errors(&input, style, optimized);
                assert!(
                    errors.is_empty(),
                    "independent portal oracle failed for {direction} {style:?} optimized={optimized}: {errors:?}"
                );
                let outcome = termiflow::render_with_feedback(
                    &input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("render critic portal fixture");
                assert!(
                    outcome.critic_report.findings.is_empty(),
                    "critic retained findings for {direction} {style:?} optimized={optimized}: {:?}",
                    outcome.critic_report.findings
                );
            }
        }
    }
}

#[test]
fn bt_parallel_portal_oracle_rejects_bare_border_pipe_mutation() {
    let input = fs::read_to_string("tests/fixtures/inputs/collision_parallel_edges_bt.md")
        .expect("read BT parallel portal fixture");

    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let mut graph = parse(&input, false)
                .expect("parse BT parallel portal fixture")
                .graph;
            let mut config = Config::default();
            config.optimize_render = optimized;
            config.composite_style = CompositeStyle::from_base(style);
            config.spacing = config.spacing.for_direction(graph.direction);
            measure::measure_graph(&mut graph, &config);
            let (graph, outcome) = layout_and_render_with_feedback(graph, config)
                .expect("render BT parallel portal fixture");
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
            let portal = graph.subgraphs.iter().find_map(|subgraph| {
                let left_x = subgraph.bounds.x;
                let right_x = subgraph
                    .bounds
                    .x
                    .saturating_add(subgraph.bounds.width.saturating_sub(1));
                let bottom_y = subgraph
                    .bounds
                    .y
                    .saturating_add(subgraph.bounds.height.saturating_sub(1));
                [subgraph.bounds.y, bottom_y].into_iter().find_map(|y| {
                    (left_x + 1..right_x).find_map(|x| {
                        (raw_char_at(
                            &outcome.output,
                            x.saturating_sub(origin_x),
                            y.saturating_sub(origin_y),
                        ) == Some(chars.edge_v))
                        .then_some((x.saturating_sub(origin_x), y.saturating_sub(origin_y)))
                    })
                })
            });
            let (portal_x, portal_y) = portal.unwrap_or_else(|| {
                panic!(
                    "expected an explicit BT portal seam for {style:?} optimized={optimized}\n{}",
                    outcome.output
                )
            });
            let mutated = replace_raw_char(&outcome.output, portal_x, portal_y, chars.cross);
            let errors = raw_portal_marker_errors_for_frame(&graph, &mutated, style);
            assert!(
                errors
                    .iter()
                    .any(|error| error.contains("explicit BT portal seams")),
                "generic BT portal junction mutation was not rejected for {style:?} optimized={optimized}: {errors:?}\n{mutated}"
            );
        }
    }
}

#[test]
fn crossing_grid_edge_identity_oracle_covers_direction_style_and_mode_matrix() {
    for direction in ["td", "bt", "lr", "rl"] {
        let input = fs::read_to_string(format!(
            "tests/fixtures/inputs/crossing_grid_{direction}.md"
        ))
        .expect("read crossing-grid fixture");

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let mut graph = parse(&input, false)
                    .expect("parse crossing-grid fixture")
                    .graph;
                let mut config = Config::default();
                config.optimize_render = optimized;
                config.composite_style = CompositeStyle::from_base(style);
                config.spacing = config.spacing.for_direction(graph.direction);
                measure::measure_graph(&mut graph, &config);
                let (_, outcome) = layout_and_render_with_feedback(graph, config)
                    .expect("render crossing-grid fixture");
                let errors = crossing_grid_raw_frame_errors(&input, &outcome.output);
                assert!(
                    errors.is_empty(),
                    "independent crossing-grid edge-identity oracle failed for {direction} {style:?} optimized={optimized}: {errors:?}\n{}",
                    outcome.output
                );
            }
        }
    }
}

#[test]
fn crossing_grid_explicit_marker_oracle_requires_interior_pass_throughs() {
    for direction in ["td", "bt", "lr", "rl"] {
        let input = fs::read_to_string(format!(
            "tests/fixtures/inputs/crossing_grid_{direction}.md"
        ))
        .expect("read crossing-grid fixture");

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            let marker = match style {
                BaseStyle::Ascii => 'x',
                BaseStyle::Unicode => '✕',
                _ => unreachable!("explicit-crossing oracle only exercises ASCII and Unicode"),
            };
            let replacement = CompositeStyle::from_base(style).to_style_chars(style).cross;

            for optimized in [false, true] {
                let output = termiflow::render(
                    &input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("render crossing-grid explicit-marker fixture");
                let errors = explicit_crossing_marker_raw_frame_errors(&input, &output, style);
                assert!(
                    errors.is_empty(),
                    "independent explicit-crossing oracle failed for {direction} {style:?} optimized={optimized}: {errors:?}\n{output}"
                );

                let mutated = output.replace(marker, &replacement.to_string());
                let mutation_errors =
                    explicit_crossing_marker_raw_frame_errors(&input, &mutated, style);
                assert!(
                    mutation_errors
                        .iter()
                        .any(|error| error.contains("no explicit")),
                    "replacing explicit crossing markers with the legacy glyph was not rejected for {direction} {style:?} optimized={optimized}: {mutation_errors:?}\n{mutated}"
                );
            }
        }
    }
}

#[test]
fn crossing_grid_route_identity_oracle_accepts_dedicated_ports() {
    for direction in ["td", "bt", "lr", "rl"] {
        let input = fs::read_to_string(format!(
            "tests/fixtures/inputs/crossing_grid_{direction}.md"
        ))
        .expect("read crossing-grid route-identity fixture");

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let output = termiflow::render(
                    &input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("render crossing-grid route-identity fixture");
                let errors = crossing_grid_route_identity_errors(&input, &output, style, optimized);
                assert!(
                    errors.is_empty(),
                    "the dedicated-port route-identity oracle rejected {direction} {style:?} optimized={optimized}: {errors:?}\n{output}"
                );

                let marker = match style {
                    BaseStyle::Ascii => 'x',
                    BaseStyle::Unicode => '✕',
                    _ => unreachable!(),
                };
                let legacy_cross = CompositeStyle::from_base(style).to_style_chars(style).cross;
                let marker_position = output
                    .lines()
                    .enumerate()
                    .find_map(|(y, line)| line.chars().position(|ch| ch == marker).map(|x| (x, y)));
                if let Some((marker_x, marker_y)) = marker_position {
                    let marker_replaced =
                        replace_raw_char(&output, marker_x, marker_y, legacy_cross);
                    assert!(
                        !crossing_grid_route_identity_errors(
                            &input,
                            &marker_replaced,
                            style,
                            optimized,
                        )
                        .is_empty(),
                        "replacing a straight-through marker was not rejected by the route-identity oracle for {direction} {style:?} optimized={optimized}\n{marker_replaced}"
                    );
                }
            }
        }
    }
}

#[test]
fn bt_sibling_chain_scene_clearance_oracle_covers_full_focused_matrix() {
    let input = fs::read_to_string("tests/fixtures/inputs/collision_sibling_triple_bt.md")
        .expect("read BT sibling-chain scene-clearance fixture");

    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        let (vertical, horizontal) = match style {
            BaseStyle::Ascii => ('|', '-'),
            BaseStyle::Unicode => ('│', '─'),
            _ => unreachable!("focused oracle only exercises ASCII and Unicode"),
        };

        for optimized in [false, true] {
            let options = RenderOptions::new()
                .with_style(style)
                .with_optimize_render(optimized);
            let first = termiflow::render_with_feedback(&input, options.clone())
                .expect("render BT sibling-chain scene-clearance fixture");
            let second = termiflow::render_with_feedback(&input, options)
                .expect("rerender BT sibling-chain scene-clearance fixture");
            assert_eq!(
                first.output, second.output,
                "BT sibling-chain scene-clearance frame is non-deterministic for {style:?} optimized={optimized}"
            );

            let raw_errors = raw_frame_errors(&input, &first.output);
            assert!(
                raw_errors.is_empty(),
                "BT sibling-chain scene-clearance raw oracle failed for {style:?} optimized={optimized}: {raw_errors:?}\n{}",
                first.output
            );
            assert!(
                first.output.chars().filter(|glyph| *glyph == vertical).count() >= 12,
                "BT sibling-chain scene-clearance lost visible vertical route cells for {style:?} optimized={optimized}:\n{}",
                first.output
            );
            assert!(
                first.critic_report.findings.is_empty(),
                "BT sibling-chain scene-clearance retained critic findings for {style:?} optimized={optimized}: {:?}\n{}",
                first.critic_report.findings,
                first.output
            );

            let lines: Vec<Vec<char>> = first
                .output
                .lines()
                .map(|line| line.chars().collect())
                .collect();
            for title in ["Group 2", "Group 3"] {
                let title_row = lines
                    .iter()
                    .position(|line| line.iter().collect::<String>().contains(title))
                    .unwrap_or_else(|| {
                        panic!("missing {title} title in {style:?}:\n{}", first.output)
                    });
                assert!(
                    title_row >= 2,
                    "{title} has no turn/clearance rows for {style:?} optimized={optimized}:\n{}",
                    first.output
                );
                let clearance = &lines[title_row - 1];
                let turn = &lines[title_row - 2];
                assert!(
                    clearance.contains(&vertical) && !clearance.contains(&horizontal),
                    "{title} lacks a dedicated vertical clearance row for {style:?} optimized={optimized}:\n{}",
                    first.output
                );
                // When the source and target portal columns are aligned, the
                // safest title-aware route is a straight continuation and no
                // horizontal jog should be invented. A jog remains valid for
                // offset columns, but both cases must retain a visible route
                // cell immediately before the clearance row.
                let aligned_continuation = turn.contains(&vertical) && !turn.contains(&horizontal);
                assert!(
                    turn.contains(&horizontal) || aligned_continuation,
                    "{title} target turn or aligned continuation disappeared before its clearance row for {style:?} optimized={optimized}:\n{}",
                    first.output
                );
            }
        }
    }
}

#[test]
fn diamond_entry_clearance_oracle_covers_direction_style_and_mode_matrix() {
    for direction in ["td", "bt", "lr", "rl"] {
        let input = fs::read_to_string(format!(
            "tests/fixtures/inputs/subgraph_shapes_{direction}.md"
        ))
        .expect("read diamond-entry fixture");

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let errors = diamond_entry_clearance_errors(&input, style, optimized);
                assert!(
                    errors.is_empty(),
                    "diamond entry-clearance oracle failed for {direction} {style:?} optimized={optimized}: {errors:?}"
                );
            }
        }
    }
}

#[test]
fn shape_all_horizontal_diamond_contours_follow_flow_direction() {
    for direction in ["lr", "rl"] {
        let input = fs::read_to_string(format!("tests/fixtures/inputs/shape_all_{direction}.md"))
            .expect("read all-shapes horizontal fixture");

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let parsed = parse(&input, false).expect("parse all-shapes horizontal fixture");
                let labels: Vec<String> = parsed
                    .graph
                    .nodes
                    .iter()
                    .map(|node| node.label.clone())
                    .collect();
                let mut graph = parsed.graph;
                let mut config = Config::default();
                config.optimize_render = optimized;
                config.composite_style = CompositeStyle::from_base(style);
                config.spacing = config.spacing.for_direction(graph.direction);
                measure::measure_graph(&mut graph, &config);
                let (_, outcome) = layout_and_render_with_feedback(graph, config)
                    .expect("render all-shapes horizontal fixture");

                for label in labels {
                    assert!(
                        outcome.output.contains(&label),
                        "horizontal all-shapes raw frame lost label {label:?} for {direction} {style:?} optimized={optimized}:\n{}",
                        outcome.output
                    );
                }
                match style {
                    BaseStyle::Ascii => {
                        assert!(
                            !outcome.output.contains('^') && !outcome.output.contains('v'),
                            "horizontal ASCII Decision retained a vertical point marker for {direction} optimized={optimized}:\n{}",
                            outcome.output
                        );
                    }
                    BaseStyle::Unicode => {
                        assert!(
                            !outcome.output.contains('◇'),
                            "horizontal Unicode Decision retained a vertical point marker for {direction} optimized={optimized}:\n{}",
                            outcome.output
                        );
                    }
                    _ => unreachable!("shape contour oracle only exercises ASCII and Unicode"),
                }
            }
        }
    }
}

#[test]
fn shape_all_diamond_contours_are_closed_across_direction_style_and_mode() {
    for direction in ["td", "bt", "lr", "rl"] {
        let input = fs::read_to_string(format!("tests/fixtures/inputs/shape_all_{direction}.md"))
            .expect("read all-shapes Decision fixture");

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let parsed = parse(&input, false).expect("parse all-shapes Decision fixture");
                let mut graph = parsed.graph;
                let mut config = Config::default();
                config.optimize_render = optimized;
                config.composite_style = CompositeStyle::from_base(style);
                config.spacing = config.spacing.for_direction(graph.direction);
                measure::measure_graph(&mut graph, &config);
                let (graph, outcome) = layout_and_render_with_feedback(graph, config)
                    .expect("render all-shapes Decision fixture");
                let decision = graph
                    .get_node("A")
                    .expect("Decision node in all-shapes fixture");
                let chars = style.chars();
                let cell = |x: usize, y: usize| {
                    outcome
                        .semantic_frame
                        .get(x, y)
                        .unwrap_or_else(|| panic!("missing Decision contour cell ({x}, {y})"))
                        .ch
                };
                let (top_left, top_right, bottom_left, bottom_right) =
                    if style == BaseStyle::Unicode {
                        ('╱', '╲', '╲', '╱')
                    } else {
                        ('/', '\\', '\\', '/')
                    };

                assert_eq!(
                    cell(decision.x, decision.y),
                    top_left,
                    "Decision top-left contour is not closed for {direction} {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
                assert_eq!(
                    cell(decision.x + decision.width - 1, decision.y),
                    top_right,
                    "Decision top-right contour is not closed for {direction} {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
                assert_eq!(
                    cell(decision.x, decision.y + 2),
                    bottom_left,
                    "Decision bottom-left contour is not closed for {direction} {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
                assert_eq!(
                    cell(decision.x + decision.width - 1, decision.y + 2),
                    bottom_right,
                    "Decision bottom-right contour is not closed for {direction} {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
                assert_eq!(
                    cell(decision.center_x(), decision.y),
                    chars.h,
                    "Decision top contour retained a point marker for {direction} {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
                assert_eq!(
                    cell(decision.center_x(), decision.y + 2),
                    chars.h,
                    "Decision bottom contour retained a point marker for {direction} {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
                assert!(
                    outcome.output.contains("Decision"),
                    "Decision label disappeared for {direction} {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
            }
        }
    }
}

#[test]
fn shape_all_flag_contours_preserve_point_and_shoulders_across_matrix() {
    for direction in ["td", "bt", "lr", "rl"] {
        let input = fs::read_to_string(format!("tests/fixtures/inputs/shape_all_{direction}.md"))
            .expect("read all-shapes Flag fixture");

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let parsed = parse(&input, false).expect("parse all-shapes Flag fixture");
                let mut graph = parsed.graph;
                let mut config = Config::default();
                config.optimize_render = optimized;
                config.composite_style = CompositeStyle::from_base(style);
                config.spacing = config.spacing.for_direction(graph.direction);
                measure::measure_graph(&mut graph, &config);
                let (graph, outcome) = layout_and_render_with_feedback(graph, config)
                    .expect("render all-shapes Flag fixture");
                let flag = graph
                    .get_node("H")
                    .expect("Flag node in all-shapes fixture");
                let chars = style.chars();
                let center_x = flag.center_x();
                let center_y = flag.center_y();
                let bottom_y = flag.bottom_y().saturating_sub(1);
                let cell = |x: usize, y: usize| {
                    outcome
                        .semantic_frame
                        .get(x, y)
                        .unwrap_or_else(|| panic!("missing Flag contour cell ({x}, {y})"))
                        .ch
                };
                let (upper_left, lower_left) = if style == BaseStyle::Unicode {
                    ('╱', '╲')
                } else {
                    ('/', '\\')
                };

                assert_eq!(
                    cell(flag.x, flag.y),
                    ' ',
                    "Flag top shoulder is not empty for {direction} {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
                assert_eq!(
                    cell(flag.x + 1, flag.y),
                    upper_left,
                    "Flag upper shoulder is wrong for {direction} {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
                assert_eq!(
                    cell(flag.x, center_y),
                    '<',
                    "Flag point is missing for {direction} {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
                assert_eq!(
                    cell(flag.x + 1, bottom_y),
                    lower_left,
                    "Flag lower shoulder is wrong for {direction} {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
                assert_eq!(
                    cell(flag.x + flag.width - 1, center_y),
                    chars.v,
                    "Flag right wall was replaced by a junction for {direction} {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
                assert_eq!(
                    cell(center_x, flag.y),
                    chars.h,
                    "Flag top contour was replaced by a junction for {direction} {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
                assert_eq!(
                    cell(center_x, bottom_y),
                    chars.h,
                    "Flag bottom contour was replaced by a junction for {direction} {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
                for label in [
                    "Decision",
                    "Rounded",
                    "Circle",
                    "Database",
                    "Subroutine",
                    "Stadium",
                    "Hexagon",
                    "Flag",
                    "Rectangle",
                ] {
                    assert!(
                        outcome.output.contains(label),
                        "all-shapes frame lost label {label:?} for {direction} {style:?} optimized={optimized}:\n{}",
                        outcome.output
                    );
                }
            }
        }
    }
}

#[test]
fn shape_all_lr_flag_attachment_keeps_arrow_and_point_visibly_separate() {
    let input = fs::read_to_string("tests/fixtures/inputs/shape_all_lr.md")
        .expect("read all-shapes LR Flag fixture");

    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let parsed = parse(&input, false).expect("parse all-shapes LR Flag fixture");
            let mut graph = parsed.graph;
            let mut config = Config::default();
            config.optimize_render = optimized;
            config.composite_style = CompositeStyle::from_base(style);
            config.spacing = config.spacing.for_direction(graph.direction);
            measure::measure_graph(&mut graph, &config);
            let (graph, outcome) = layout_and_render_with_feedback(graph, config)
                .expect("render all-shapes LR Flag fixture");
            let flag = graph
                .get_node("H")
                .expect("Flag node in all-shapes LR fixture");
            let chars = style.chars();
            let y = flag.center_y();
            let point_x = flag.x;
            let arrow_x = point_x.saturating_sub(2);
            let separator_x = point_x.saturating_sub(1);
            let cell = |x: usize, y: usize| {
                outcome
                    .semantic_frame
                    .get(x, y)
                    .unwrap_or_else(|| panic!("missing LR Flag attachment cell ({x}, {y})"))
                    .ch
            };

            assert_eq!(
                cell(arrow_x, y),
                chars.arrow_right,
                "LR Flag lost its incoming arrowhead for {style:?} optimized={optimized}:\n{}",
                outcome.output
            );
            assert_ne!(
                cell(separator_x, y),
                chars.arrow_right,
                "LR Flag arrowhead still touches the point for {style:?} optimized={optimized}:\n{}",
                outcome.output
            );
            assert_eq!(
                cell(point_x, y),
                '<',
                "LR Flag point was overwritten for {style:?} optimized={optimized}:\n{}",
                outcome.output
            );
            assert!(
                outcome.output.contains("Flag"),
                "LR Flag label disappeared for {style:?} optimized={optimized}:\n{}",
                outcome.output
            );
        }
    }
}

#[test]
fn geometry_oracle_rejects_an_overlapping_trace_mutation() {
    let input = "graph TD\nA[Alpha] --> B[Beta]";
    let mut graph = parse(input, false).expect("parse geometry fixture").graph;
    let config = Config::default();
    measure::measure_graph(&mut graph, &config);
    let (graph, _) =
        layout_and_render_with_feedback(graph, config).expect("layout geometry fixture");
    let trace = GeometryTrace::from_graph(&graph);
    assert!(geometry_errors(&trace).is_empty());

    let mut corrupted = trace.clone();
    corrupted.nodes[1].x = corrupted.nodes[0].x;
    corrupted.nodes[1].y = corrupted.nodes[0].y;
    assert!(geometry_errors(&corrupted)
        .iter()
        .any(|error| error.contains("overlap")));
}

fn geometry_trace_for(input: &str) -> GeometryTrace {
    let mut graph = parse(input, false).expect("parse geometry fixture").graph;
    let mut config = Config::default();
    config.spacing = config.spacing.for_direction(graph.direction);
    measure::measure_graph(&mut graph, &config);
    let (graph, _) =
        layout_and_render_with_feedback(graph, config).expect("layout geometry fixture");
    GeometryTrace::from_graph(&graph)
}

#[test]
fn full_primary_corpus_raw_topology_reports_are_deterministic() {
    let mut inputs: Vec<_> = fs::read_dir("tests/fixtures/inputs")
        .expect("read fixture directory")
        .map(|entry| entry.expect("read fixture entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .filter(|path| {
            !path
                .file_stem()
                .is_some_and(|stem| stem.to_string_lossy().starts_with("error_"))
        })
        .collect();
    inputs.sort();

    for path in inputs {
        let input = fs::read_to_string(&path).expect("read fixture");
        let expected_edges = parse(&input, false)
            .expect("parse fixture")
            .graph
            .edges
            .len();
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let first = termiflow::render(
                    &input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("render fixture");
                let second = termiflow::render(
                    &input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("repeat render fixture");
                assert_eq!(
                    raw_topology_errors(&first, expected_edges),
                    raw_topology_errors(&second, expected_edges),
                    "non-deterministic raw-topology report for {} {style:?} optimized={optimized}",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn diamond_shortcut_raw_oracle_keeps_all_three_edges_visible() {
    for direction in ["td", "bt", "lr", "rl"] {
        let input = fs::read_to_string(format!(
            "tests/fixtures/inputs/shape_database_{direction}.md"
        ))
        .expect("read diamond shortcut fixture");
        let parsed = parse(&input, false).expect("parse diamond shortcut fixture");
        let direction_name = direction.to_ascii_uppercase();

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            let chars = CompositeStyle::from_base(style).to_style_chars(style);
            let arrow = match direction_name.as_str() {
                "TD" => chars.arrow_down,
                "BT" => chars.arrow_up,
                "LR" => chars.arrow_right,
                "RL" => chars.arrow_left,
                _ => unreachable!(),
            };

            for optimized in [false, true] {
                let output = termiflow::render(
                    &input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("render diamond shortcut fixture");
                let arrow_count = output.chars().filter(|glyph| *glyph == arrow).count();

                // Two target nodes exist in this scene, so a convergent pair
                // may legitimately share the final physical target arrow.
                // The direct shortcut is proven independently by its outer
                // lane; this avoids treating a shared port as a missing edge.
                assert!(
                    arrow_count >= 2,
                    "raw diamond frame must expose arrows for both distinct targets for {direction_name} {style:?} optimized={optimized}:\n{output}"
                );
                for label in ["REST API", "Redis", "PostgreSQL"] {
                    assert!(
                        output.contains(label),
                        "raw diamond frame lost node label {label:?} for {direction_name} {style:?} optimized={optimized}:\n{output}"
                    );
                }

                let border_runs: HashSet<usize> = parsed
                    .graph
                    .nodes
                    .iter()
                    .map(|node| node.width.saturating_sub(2))
                    .collect();
                let has_outer_lane = output.lines().any(|line| {
                    if ["REST API", "Redis", "PostgreSQL"]
                        .iter()
                        .any(|label| line.contains(label))
                    {
                        return false;
                    }
                    let markers = [
                        chars.corner_dr,
                        chars.corner_dl,
                        chars.corner_ur,
                        chars.corner_ul,
                        chars.cross,
                        chars.junction_down,
                        chars.junction_up,
                        chars.junction_right,
                        chars.junction_left,
                    ];
                    let mut run = 0usize;
                    let mut found = false;
                    for glyph in line.chars().chain(std::iter::once(' ')) {
                        if glyph == chars.edge_h {
                            run = run.saturating_add(1);
                        } else {
                            if run >= 4
                                && !border_runs.contains(&run)
                                && line.chars().any(|marker| markers.contains(&marker))
                            {
                                found = true;
                            }
                            run = 0;
                        }
                    }
                    found
                });
                assert!(
                    has_outer_lane,
                    "raw diamond frame lost the direct shortcut's outside lane for {direction_name} {style:?} optimized={optimized}:\n{output}"
                );
            }
        }
    }
}

#[test]
fn full_fixture_corpus_geometry_traces_are_deterministic() {
    let mut inputs: Vec<_> = fs::read_dir("tests/fixtures/inputs")
        .expect("read fixture directory")
        .map(|entry| entry.expect("read fixture entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .filter(|path| {
            !path
                .file_stem()
                .is_some_and(|stem| stem.to_string_lossy().starts_with("error_"))
        })
        .collect();
    inputs.sort();

    for path in inputs {
        let input = fs::read_to_string(&path).expect("read fixture");
        let first = geometry_trace_for(&input);
        let second = geometry_trace_for(&input);
        assert_eq!(
            first,
            second,
            "non-deterministic geometry trace for {}",
            path.display()
        );
    }
}

#[test]
fn junction_quad_independent_oracle_covers_the_full_review_matrix() {
    for prefix in ["junction_quad", "junction_quad_holdout"] {
        for direction in ["TD", "BT", "LR", "RL"] {
            let direction_file = direction.to_ascii_lowercase();
            let input = fs::read_to_string(if prefix == "junction_quad" {
                format!("tests/fixtures/inputs/{prefix}_{direction_file}.md")
            } else {
                format!("tests/fixtures/holdouts/inputs/{prefix}_{direction_file}.md")
            })
            .expect("read junction quad fixture");
            let (anchor_id, target_ids, semantic_edges, _) =
                dual_junction_shape(&parse(&input, false).expect("parse junction quad").graph);

            for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
                for optimized in [false, true] {
                    let render_case = || {
                        let mut graph = parse(&input, false)
                            .expect("parse junction quad fixture")
                            .graph;
                        let mut config = Config::default();
                        config.optimize_render = optimized;
                        config.composite_style = CompositeStyle::from_base(style);
                        config.spacing = config.spacing.for_direction(graph.direction);
                        measure::measure_graph(&mut graph, &config);
                        let (graph, outcome) = layout_and_render_with_feedback(graph, config)
                            .expect("render junction quad fixture");
                        (outcome.output, GeometryTrace::from_graph(&graph))
                    };

                    let (first_output, first_trace) = render_case();
                    let (second_output, second_trace) = render_case();
                    assert_eq!(
                        first_output, second_output,
                        "non-deterministic raw frame for {prefix} {direction} {style:?} optimized={optimized}"
                    );
                    assert_eq!(
                        first_trace, second_trace,
                        "non-deterministic geometry for {prefix} {direction} {style:?} optimized={optimized}"
                    );

                    let raw_errors = junction_quad_raw_frame_errors(&input, &first_output);
                    assert!(
                        raw_errors.is_empty(),
                        "raw-frame oracle failed for {prefix} {direction} {style:?} optimized={optimized}: {raw_errors:?}\n{first_output}"
                    );
                    let node_errors = node_geometry_errors(&first_trace);
                    assert!(
                        node_errors.is_empty(),
                        "geometry oracle failed for {prefix} {direction} {style:?} optimized={optimized}: {node_errors:?}"
                    );
                    assert_eq!(
                        first_trace.edges.len(),
                        semantic_edges,
                        "junction quad lost a semantic edge trace for {prefix} {direction} {style:?} optimized={optimized}"
                    );

                    let center = |id: &str| {
                        let node = first_trace
                            .nodes
                            .iter()
                            .find(|node| node.id == id)
                            .unwrap_or_else(|| panic!("missing node {id}"));
                        secondary_center_for(node.x, node.y, node.width, node.height, direction)
                    };
                    let target_midpoint =
                        (center(&target_ids[0]) + center(target_ids.last().expect("target"))) / 2;
                    assert!(
                        center(&anchor_id).abs_diff(target_midpoint) <= 1,
                        "dual-junction midpoint drift for {prefix} {direction} {style:?} optimized={optimized}: anchor={} target_midpoint={target_midpoint}",
                        center(&anchor_id),
                    );
                }
            }
        }
    }
}

#[test]
fn pure_fan_in_independent_oracle_covers_the_full_scale_matrix() {
    for direction in ["TD", "BT", "LR", "RL"] {
        let input = fs::read_to_string(format!(
            "tests/fixtures/inputs/scale_dense_{}.md",
            direction.to_lowercase()
        ))
        .expect("read scale dense fixture");
        let parsed = parse(&input, false).expect("parse scale dense fixture");
        let (anchor_id, source_ids, semantic_edges) = pure_fan_in_shape(&parsed.graph);

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let render_case = || {
                    let mut graph = parse(&input, false)
                        .expect("parse scale dense fixture")
                        .graph;
                    let mut config = Config::default();
                    config.optimize_render = optimized;
                    config.composite_style = CompositeStyle::from_base(style);
                    config.spacing = config.spacing.for_direction(graph.direction);
                    measure::measure_graph(&mut graph, &config);
                    let (graph, outcome) = layout_and_render_with_feedback(graph, config)
                        .expect("render scale dense fixture");
                    (
                        outcome.output.clone(),
                        GeometryTrace::from_graph(&graph),
                        outcome,
                    )
                };

                let (first_output, first_trace, first_outcome) = render_case();
                let (second_output, second_trace, _) = render_case();
                assert_eq!(
                    first_output, second_output,
                    "non-deterministic raw frame for scale_dense {direction} {style:?} optimized={optimized}"
                );
                assert_eq!(
                    first_trace, second_trace,
                    "non-deterministic geometry for scale_dense {direction} {style:?} optimized={optimized}"
                );

                assert!(
                    shared_fan_in_raw_frame_errors(&input, &first_output).is_empty(),
                    "raw-frame oracle failed for scale_dense {direction} {style:?} optimized={optimized}: {:?}\n{first_output}",
                    shared_fan_in_raw_frame_errors(&input, &first_output)
                );
                assert!(
                    node_geometry_errors(&first_trace).is_empty(),
                    "node geometry oracle failed for scale_dense {direction} {style:?} optimized={optimized}: {:?}",
                    node_geometry_errors(&first_trace)
                );
                assert_eq!(
                    first_trace.edges.len(),
                    semantic_edges,
                    "scale dense lost a semantic edge trace for {direction} {style:?} optimized={optimized}"
                );
                assert!(
                    first_outcome
                        .critic_report
                        .findings
                        .iter()
                        .all(|finding| finding.code != termiflow::FindingCode::RouteSymmetryImbalance),
                    "pure fan-in critic finding remained for scale_dense {direction} {style:?} optimized={optimized}: {:?}",
                    first_outcome.critic_report.findings
                );

                let center = |id: &str| {
                    let node = first_trace
                        .nodes
                        .iter()
                        .find(|node| node.id == id)
                        .unwrap_or_else(|| panic!("missing node {id}"));
                    secondary_center_for(node.x, node.y, node.width, node.height, direction)
                };
                let source_centers: Vec<usize> = source_ids.iter().map(|id| center(id)).collect();
                let source_midpoint =
                    (source_centers.iter().min().copied().expect("source center")
                        + source_centers.iter().max().copied().expect("source center"))
                        / 2;
                assert!(
                    center(&anchor_id).abs_diff(source_midpoint) <= 1,
                    "pure fan-in midpoint drift for scale_dense {direction} {style:?} optimized={optimized}: anchor={} midpoint={source_midpoint}",
                    center(&anchor_id)
                );
            }
        }
    }
}

fn database_terminal_entry_raw_frame_errors(
    input: &str,
    style: BaseStyle,
    optimized: bool,
) -> Vec<String> {
    let mut graph = parse(input, false)
        .expect("parse database entry-clearance fixture")
        .graph;
    let mut config = Config::default();
    config.optimize_render = optimized;
    config.composite_style = CompositeStyle::from_base(style);
    config.spacing = config.spacing.for_direction(graph.direction);
    measure::measure_graph(&mut graph, &config);
    let (graph, outcome) = layout_and_render_with_feedback(graph, config)
        .expect("render database entry-clearance fixture");
    let target = graph
        .get_node("DB")
        .expect("database entry-clearance fixture target node");
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
    let rendered_char = |x: usize, y: usize| {
        raw_char_at(
            &outcome.output,
            x.saturating_sub(origin_x),
            y.saturating_sub(origin_y),
        )
    };

    let incoming_count = graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge && edge.to == target.id)
        .count();
    let mut arrow_cells = Vec::new();
    let mut errors = Vec::new();

    match graph.direction {
        termiflow::graph::Direction::TD | termiflow::graph::Direction::TB => {
            let arrow_y = target.y.saturating_sub(1);
            let contour_y = target.y;
            let start_x = target.x.saturating_add(1);
            let end_x = target.x.saturating_add(target.width.saturating_sub(1));
            for x in start_x..end_x {
                if rendered_char(x, arrow_y) == Some(chars.arrow_down) {
                    arrow_cells.push((x, arrow_y));
                    if rendered_char(x, contour_y) != Some(chars.h) {
                        errors.push(format!(
                            "TD database contour at ({x},{contour_y}) is not intact"
                        ));
                    }
                }
            }
        }
        termiflow::graph::Direction::BT => {
            let arrow_y = target.bottom_y();
            let contour_y = target.bottom_y().saturating_sub(1);
            let start_x = target.x.saturating_add(1);
            let end_x = target.x.saturating_add(target.width.saturating_sub(1));
            for x in start_x..end_x {
                if rendered_char(x, arrow_y) == Some(chars.arrow_up) {
                    arrow_cells.push((x, arrow_y));
                    if rendered_char(x, contour_y) != Some(chars.h) {
                        errors.push(format!(
                            "BT database contour at ({x},{contour_y}) is not intact"
                        ));
                    }
                }
            }
        }
        termiflow::graph::Direction::LR => {
            let arrow_x = target.x.saturating_sub(1);
            let contour_x = target.x;
            let start_y = target.y.saturating_add(1);
            let end_y = target.y.saturating_add(target.height.saturating_sub(1));
            for y in start_y..end_y {
                if rendered_char(arrow_x, y) == Some(chars.arrow_right) {
                    arrow_cells.push((arrow_x, y));
                    if !matches!(
                        rendered_char(contour_x, y),
                        Some(glyph) if glyph == chars.v || glyph == chars.junction_left
                    ) {
                        errors.push(format!(
                            "LR database contour at ({contour_x},{y}) is not intact"
                        ));
                    }
                }
            }
        }
        termiflow::graph::Direction::RL => {
            let arrow_x = target.x.saturating_add(target.width);
            let contour_x = target.x.saturating_add(target.width).saturating_sub(1);
            let start_y = target.y.saturating_add(1);
            let end_y = target.y.saturating_add(target.height.saturating_sub(1));
            for y in start_y..end_y {
                if rendered_char(arrow_x, y) == Some(chars.arrow_left) {
                    arrow_cells.push((arrow_x, y));
                    if !matches!(
                        rendered_char(contour_x, y),
                        Some(glyph) if glyph == chars.v || glyph == chars.junction_right
                    ) {
                        errors.push(format!(
                            "RL database contour at ({contour_x},{y}) is not intact"
                        ));
                    }
                }
            }
        }
    }

    if arrow_cells.len() < incoming_count {
        errors.push(format!(
            "database target exposes {} arrowheads for {incoming_count} incoming edges: {arrow_cells:?}",
            arrow_cells.len()
        ));
    }
    errors
}

#[test]
fn database_terminal_entry_raw_oracle_covers_direction_style_and_mode_matrix() {
    for direction in ["td", "bt", "lr", "rl"] {
        let input = fs::read_to_string(format!(
            "tests/fixtures/inputs/shape_database_{direction}.md"
        ))
        .expect("read database entry-clearance fixture");
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let errors = database_terminal_entry_raw_frame_errors(&input, style, optimized);
                assert!(
                    errors.is_empty(),
                    "database entry-clearance raw oracle failed for {direction} {style:?} optimized={optimized}: {errors:?}"
                );
            }
        }
    }
}

fn intermediate_database_entry_raw_frame_errors(
    input: &str,
    style: BaseStyle,
    optimized: bool,
) -> Vec<String> {
    let mut graph = parse(input, false)
        .expect("parse intermediate database fixture")
        .graph;
    let mut config = Config::default();
    config.optimize_render = optimized;
    config.composite_style = CompositeStyle::from_base(style);
    config.spacing = config.spacing.for_direction(graph.direction);
    measure::measure_graph(&mut graph, &config);
    let (graph, outcome) = layout_and_render_with_feedback(graph, config)
        .expect("render intermediate database fixture");
    let intermediate = graph
        .get_node("Cache")
        .expect("intermediate database fixture cache node");
    let chars = CompositeStyle::from_base(style).to_style_chars(style);
    let origin_x = graph.nodes.iter().map(|node| node.x).min().unwrap_or(0);
    let origin_y = graph.nodes.iter().map(|node| node.y).min().unwrap_or(0);
    let rendered_char = |x: usize, y: usize| {
        raw_char_at(
            &outcome.output,
            x.saturating_sub(origin_x),
            y.saturating_sub(origin_y),
        )
    };

    let (arrow, entry, shaft, side_neighbors) = match graph.direction {
        termiflow::graph::Direction::TD | termiflow::graph::Direction::TB => (
            chars.arrow_down,
            (intermediate.center_x(), intermediate.y.saturating_sub(1)),
            (intermediate.center_x(), intermediate.y.saturating_sub(2)),
            [
                (
                    intermediate.center_x().saturating_sub(1),
                    intermediate.y.saturating_sub(2),
                ),
                (
                    intermediate.center_x().saturating_add(1),
                    intermediate.y.saturating_sub(2),
                ),
            ],
        ),
        termiflow::graph::Direction::BT => (
            chars.arrow_up,
            (intermediate.center_x(), intermediate.bottom_y()),
            (
                intermediate.center_x(),
                intermediate.bottom_y().saturating_add(1),
            ),
            [
                (
                    intermediate.center_x().saturating_sub(1),
                    intermediate.bottom_y().saturating_add(1),
                ),
                (
                    intermediate.center_x().saturating_add(1),
                    intermediate.bottom_y().saturating_add(1),
                ),
            ],
        ),
        termiflow::graph::Direction::LR => (
            chars.arrow_right,
            (intermediate.x.saturating_sub(1), intermediate.center_y()),
            (intermediate.x.saturating_sub(2), intermediate.center_y()),
            [
                (
                    intermediate.x.saturating_sub(2),
                    intermediate.center_y().saturating_sub(1),
                ),
                (
                    intermediate.x.saturating_sub(2),
                    intermediate.center_y().saturating_add(1),
                ),
            ],
        ),
        termiflow::graph::Direction::RL => (
            chars.arrow_left,
            (
                intermediate.x.saturating_add(intermediate.width),
                intermediate.center_y(),
            ),
            (
                intermediate
                    .x
                    .saturating_add(intermediate.width)
                    .saturating_add(1),
                intermediate.center_y(),
            ),
            [
                (
                    intermediate
                        .x
                        .saturating_add(intermediate.width)
                        .saturating_add(1),
                    intermediate.center_y().saturating_sub(1),
                ),
                (
                    intermediate
                        .x
                        .saturating_add(intermediate.width)
                        .saturating_add(1),
                    intermediate.center_y().saturating_add(1),
                ),
            ],
        ),
    };

    let mut errors = Vec::new();
    if rendered_char(entry.0, entry.1) != Some(arrow) {
        errors.push(format!(
            "intermediate database entry at {entry:?} is missing its arrowhead"
        ));
    }
    let expected_shaft = match graph.direction {
        termiflow::graph::Direction::TD
        | termiflow::graph::Direction::TB
        | termiflow::graph::Direction::BT => chars.edge_v,
        termiflow::graph::Direction::LR | termiflow::graph::Direction::RL => chars.edge_h,
    };
    if rendered_char(shaft.0, shaft.1) != Some(expected_shaft) {
        errors.push(format!(
            "intermediate database entry at {entry:?} lacks its primary-axis shaft"
        ));
    }
    for neighbor in side_neighbors {
        if rendered_char(neighbor.0, neighbor.1).is_some_and(|glyph| glyph != ' ') {
            errors.push(format!(
                "intermediate database arrow at {entry:?} has a continuing side-axis route at {neighbor:?}"
            ));
        }
    }
    errors
}

#[test]
fn intermediate_database_entry_is_terminal_across_direction_style_and_mode_matrix() {
    for direction in ["td", "bt", "lr", "rl"] {
        let input = fs::read_to_string(format!(
            "tests/fixtures/inputs/shape_database_{direction}.md"
        ))
        .expect("read intermediate database fixture");
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let errors = intermediate_database_entry_raw_frame_errors(&input, style, optimized);
                assert!(
                    errors.is_empty(),
                    "intermediate database raw oracle failed for {direction} {style:?} optimized={optimized}: {errors:?}"
                );
            }
        }
    }
}

#[test]
fn database_dual_junction_independent_oracle_requires_both_arrivals() {
    for direction in ["LR", "RL"] {
        let input = fs::read_to_string(format!(
            "tests/fixtures/inputs/shape_database_{}.md",
            direction.to_lowercase()
        ))
        .expect("read database dual-junction fixture");
        let parsed = parse(&input, false).expect("parse database dual-junction fixture");
        assert!(dedicated_fan_in_identity_family(&parsed.graph));

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let mut graph = parsed.graph.clone();
                let mut config = Config::default();
                config.optimize_render = optimized;
                config.composite_style = CompositeStyle::from_base(style);
                config.spacing = config.spacing.for_direction(graph.direction);
                measure::measure_graph(&mut graph, &config);
                let (_, outcome) = layout_and_render_with_feedback(graph, config)
                    .expect("render database dual-junction fixture");
                let errors = shared_fan_in_raw_frame_errors(&input, &outcome.output);
                assert!(
                    errors.is_empty(),
                    "database dual-junction raw oracle failed for {direction} {style:?} optimized={optimized}: {errors:?}\n{}",
                    outcome.output
                );
            }
        }
    }
}

#[test]
fn horizontal_branch_rejoin_independent_oracle_covers_full_matrix() {
    for direction in ["lr", "rl"] {
        let input = fs::read_to_string(format!("tests/fixtures/inputs/flow_branch_{direction}.md"))
            .expect("read horizontal branch/rejoin fixture");
        let parsed = parse(&input, false).expect("parse horizontal branch/rejoin fixture");
        assert!(horizontal_branch_rejoin_identity_family(&parsed.graph));

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let render = || {
                    termiflow::render_with_feedback(
                        &input,
                        RenderOptions::new()
                            .with_style(style)
                            .with_optimize_render(optimized),
                    )
                    .expect("render horizontal branch/rejoin fixture")
                };
                let first = render();
                let second = render();
                assert_eq!(
                    first.output, second.output,
                    "non-deterministic raw frame for flow_branch_{direction} {style:?} optimized={optimized}"
                );
                assert_eq!(
                    first.semantic_frame, second.semantic_frame,
                    "non-deterministic semantic frame for flow_branch_{direction} {style:?} optimized={optimized}"
                );

                let errors = shared_fan_in_raw_frame_errors(&input, &first.output);
                assert!(
                    errors.is_empty(),
                    "horizontal branch/rejoin raw oracle failed for {direction} {style:?} optimized={optimized}: {errors:?}\n{}",
                    first.output
                );
                let arrow = match (direction, style) {
                    ("lr", BaseStyle::Ascii) => '>',
                    ("lr", BaseStyle::Unicode) => '→',
                    ("rl", BaseStyle::Ascii) => '<',
                    ("rl", BaseStyle::Unicode) => '←',
                    _ => unreachable!("horizontal branch/rejoin oracle only uses LR/RL"),
                };
                assert_eq!(
                    first.output.matches(arrow).count(),
                    4,
                    "horizontal branch/rejoin must expose all four directed edges for {direction} {style:?} optimized={optimized}:\n{}",
                    first.output
                );
                let arrow_owners: HashSet<String> = first
                    .semantic_frame
                    .cells
                    .iter()
                    .filter(|cell| cell.role == termiflow::render::semantic::CellRole::ArrowTip)
                    .filter_map(|cell| cell.owner_id.clone())
                    .collect();
                assert_eq!(
                    arrow_owners.len(),
                    4,
                    "horizontal branch/rejoin must preserve one owner per arrow for {direction} {style:?} optimized={optimized}: {arrow_owners:?}"
                );
            }
        }
    }
}

#[test]
fn horizontal_mixed_junction_independent_oracle_covers_full_matrix() {
    for direction in ["lr", "rl"] {
        let input = fs::read_to_string(format!(
            "tests/fixtures/inputs/junction_mixed_{direction}.md"
        ))
        .expect("read horizontal mixed-junction fixture");
        let parsed = parse(&input, false).expect("parse horizontal mixed-junction fixture");
        assert!(horizontal_mixed_junction_identity_family(&parsed.graph));

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let render = || {
                    termiflow::render_with_feedback(
                        &input,
                        RenderOptions::new()
                            .with_style(style)
                            .with_optimize_render(optimized),
                    )
                    .expect("render horizontal mixed-junction fixture")
                };
                let first = render();
                let second = render();
                assert_eq!(
                    first.output, second.output,
                    "non-deterministic raw frame for junction_mixed_{direction} {style:?} optimized={optimized}"
                );
                assert_eq!(
                    first.semantic_frame, second.semantic_frame,
                    "non-deterministic semantic frame for junction_mixed_{direction} {style:?} optimized={optimized}"
                );

                let errors = shared_fan_in_raw_frame_errors(&input, &first.output);
                assert!(
                    errors.is_empty(),
                    "horizontal mixed-junction raw oracle failed for {direction} {style:?} optimized={optimized}: {errors:?}\n{}",
                    first.output
                );
                let arrow = match (direction, style) {
                    ("lr", BaseStyle::Ascii) => '>',
                    ("lr", BaseStyle::Unicode) => '→',
                    ("rl", BaseStyle::Ascii) => '<',
                    ("rl", BaseStyle::Unicode) => '←',
                    _ => unreachable!("horizontal mixed-junction oracle only uses LR/RL"),
                };
                assert_eq!(
                    first.output.matches(arrow).count(),
                    6,
                    "horizontal mixed-junction must expose all six directed edges for {direction} {style:?} optimized={optimized}:\n{}",
                    first.output
                );
                let arrow_owners: HashSet<String> = first
                    .semantic_frame
                    .cells
                    .iter()
                    .filter(|cell| cell.role == termiflow::render::semantic::CellRole::ArrowTip)
                    .filter_map(|cell| cell.owner_id.clone())
                    .collect();
                assert_eq!(
                    arrow_owners.len(),
                    6,
                    "horizontal mixed-junction must preserve one owner per arrow for {direction} {style:?} optimized={optimized}: {arrow_owners:?}"
                );
            }
        }
    }
}

#[test]
fn mixed_edge_kind_fanout_preserves_source_enclosure_and_edge_identity() {
    let input = fs::read_to_string("tests/fixtures/inputs/edge_kinds_rl.md")
        .expect("read mixed edge-kind fanout fixture");
    let parsed = parse(&input, false).expect("parse mixed edge-kind fanout fixture");
    assert_eq!(parsed.graph.direction, termiflow::graph::Direction::RL);
    assert_eq!(parsed.graph.edges.len(), 6);

    for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
        for optimized in [false, true] {
            let render = || {
                termiflow::render_with_feedback(
                    &input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("render mixed edge-kind fanout fixture")
            };
            let first = render();
            let second = render();
            assert_eq!(
                first.output, second.output,
                "non-deterministic raw frame for edge_kinds_rl {style:?} optimized={optimized}"
            );
            assert_eq!(
                first.semantic_frame, second.semantic_frame,
                "non-deterministic semantic frame for edge_kinds_rl {style:?} optimized={optimized}"
            );

            let hub_line = first
                .output
                .lines()
                .find(|line| line.contains("Hub"))
                .expect("Hub row in mixed edge-kind fanout frame");
            let expected_hub = match style {
                BaseStyle::Ascii => "|  Hub  |",
                BaseStyle::Unicode => "│  Hub  │",
                _ => unreachable!("oracle only covers ASCII and Unicode styles"),
            };
            assert!(
                hub_line.contains(expected_hub),
                "mixed edge-kind fanout must retain a complete Hub enclosure for {style:?} optimized={optimized}:\n{}",
                first.output
            );

            for label in ["Arrow", "Open", "Thick", "Dotted", "Circle", "Cross"] {
                assert!(
                    first.output.contains(label),
                    "mixed edge-kind fanout lost {label} label for {style:?} optimized={optimized}:\n{}",
                    first.output
                );
            }
            let required_glyphs = match style {
                BaseStyle::Ascii => ['<', '=', '.', 'o', 'x'],
                BaseStyle::Unicode => ['←', '━', '╌', '○', '✕'],
                _ => unreachable!("oracle only covers ASCII and Unicode styles"),
            };
            for glyph in required_glyphs {
                assert!(
                    first.output.contains(glyph),
                    "mixed edge-kind fanout lost edge glyph {glyph:?} for {style:?} optimized={optimized}:\n{}",
                    first.output
                );
            }
        }
    }
}

#[test]
fn mixed_edge_kind_endpoint_markers_preserve_direction_style_and_mode_identity() {
    for direction in ["td", "bt", "lr", "rl"] {
        let input = fs::read_to_string(format!("tests/fixtures/inputs/edge_kinds_{direction}.md"))
            .expect("read mixed edge-kind endpoint fixture");
        let parsed = parse(&input, false).expect("parse mixed edge-kind endpoint fixture");
        let circle_index = parsed
            .graph
            .edges
            .iter()
            .position(|edge| edge.from == "A" && edge.to == "F")
            .expect("CircleEnd edge in mixed edge-kind fixture");
        let cross_index = parsed
            .graph
            .edges
            .iter()
            .position(|edge| edge.from == "A" && edge.to == "G")
            .expect("CrossEnd edge in mixed edge-kind fixture");
        let arrow_index = parsed
            .graph
            .edges
            .iter()
            .position(|edge| edge.from == "A" && edge.to == "B")
            .expect("Arrow edge in mixed edge-kind fixture");
        let circle_owner = format!(
            "edge:{circle_index}:{}->{}",
            parsed.graph.edges[circle_index].from, parsed.graph.edges[circle_index].to
        );
        let cross_owner = format!(
            "edge:{cross_index}:{}->{}",
            parsed.graph.edges[cross_index].from, parsed.graph.edges[cross_index].to
        );
        let arrow_owner = format!(
            "edge:{arrow_index}:{}->{}",
            parsed.graph.edges[arrow_index].from, parsed.graph.edges[arrow_index].to
        );

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let outcome = termiflow::render_with_feedback(
                    &input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("render mixed edge-kind endpoint fixture");
                let marker_cells: Vec<_> = outcome
                    .semantic_frame
                    .cells
                    .iter()
                    .filter(|cell| {
                        cell.role == termiflow::render::semantic::CellRole::EndpointMarker
                    })
                    .collect();
                assert_eq!(
                    marker_cells.len(),
                    2,
                    "mixed edge-kind endpoint count changed for {direction} {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );

                let circle = marker_cells
                    .iter()
                    .find(|cell| cell.owner_id.as_deref() == Some(circle_owner.as_str()))
                    .expect("CircleEnd endpoint marker owner");
                let cross = marker_cells
                    .iter()
                    .find(|cell| cell.owner_id.as_deref() == Some(cross_owner.as_str()))
                    .expect("CrossEnd endpoint marker owner");
                let (circle_glyph, cross_glyph) = match style {
                    BaseStyle::Ascii => ('o', 'x'),
                    BaseStyle::Unicode => ('○', '✕'),
                    _ => unreachable!("oracle only covers ASCII and Unicode styles"),
                };
                assert_eq!(circle.ch, circle_glyph);
                assert_eq!(cross.ch, cross_glyph);

                let directed_arrows: Vec<_> = outcome
                    .semantic_frame
                    .cells
                    .iter()
                    .filter(|cell| {
                        cell.role == termiflow::render::semantic::CellRole::ArrowTip
                            && cell.owner_id.as_deref() == Some(arrow_owner.as_str())
                    })
                    .collect();
                assert_eq!(directed_arrows.len(), 1);
            }
        }
    }
}

#[test]
fn wide_terminal_fan_in_independent_oracle_covers_direction_style_and_mode_matrix() {
    for direction in ["bt", "td", "lr", "rl"] {
        let input = fs::read_to_string(format!(
            "tests/fixtures/inputs/converge_deep_{direction}.md"
        ))
        .expect("read wide terminal fan-in fixture");
        let parsed = parse(&input, false).expect("parse wide terminal fan-in fixture");
        assert!(wide_terminal_fan_in_identity_family(&parsed.graph));

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let outcome = termiflow::render_with_feedback(
                    &input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("render wide terminal fan-in fixture");
                let errors = shared_fan_in_raw_frame_errors(&input, &outcome.output);
                assert!(
                    errors.is_empty(),
                    "wide terminal fan-in raw oracle failed for {direction} {style:?} optimized={optimized}: {errors:?}\n{}",
                    outcome.output
                );
                let arrow = match (direction, style) {
                    ("bt", BaseStyle::Ascii) => '^',
                    ("bt", BaseStyle::Unicode) => '↑',
                    ("td", BaseStyle::Ascii) => 'v',
                    ("td", BaseStyle::Unicode) => '↓',
                    ("lr", BaseStyle::Ascii) => '>',
                    ("lr", BaseStyle::Unicode) => '→',
                    ("rl", BaseStyle::Ascii) => '<',
                    ("rl", BaseStyle::Unicode) => '←',
                    _ => unreachable!("unexpected wide fan-in direction/style"),
                };
                assert_eq!(
                    outcome.output.matches(arrow).count(),
                    8,
                    "wide terminal fan-in lost a target arrow for {direction} {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
                let arrow_owners: HashSet<String> = outcome
                    .semantic_frame
                    .cells
                    .iter()
                    .filter(|cell| cell.role == termiflow::render::semantic::CellRole::ArrowTip)
                    .filter_map(|cell| cell.owner_id.clone())
                    .collect();
                assert_eq!(
                    arrow_owners.len(),
                    8,
                    "wide terminal fan-in did not preserve one edge owner per arrow for {direction} {style:?} optimized={optimized}: {arrow_owners:?}"
                );
            }
        }
    }
}

#[test]
fn nonterminal_vertical_fan_in_independent_oracle_covers_homolog_matrix() {
    for fixture in [
        "flow_chain_td",
        "flow_chain_bt",
        "junction_triple_td",
        "junction_triple_bt",
    ] {
        let input = fs::read_to_string(format!("tests/fixtures/inputs/{fixture}.md"))
            .expect("read nonterminal vertical fan-in fixture");
        let parsed = parse(&input, false).expect("parse nonterminal vertical fan-in fixture");
        assert!(nonterminal_vertical_fan_in_identity_family(&parsed.graph));

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let first = termiflow::render_with_feedback(
                    &input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("render nonterminal vertical fan-in fixture");
                let second = termiflow::render_with_feedback(
                    &input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("repeat render nonterminal vertical fan-in fixture");
                assert_eq!(
                    first.output, second.output,
                    "non-deterministic raw frame for {fixture} {style:?} optimized={optimized}"
                );
                assert_eq!(
                    first.semantic_frame, second.semantic_frame,
                    "non-deterministic semantic frame for {fixture} {style:?} optimized={optimized}"
                );

                let errors = shared_fan_in_raw_frame_errors(&input, &first.output);
                assert!(
                    errors.is_empty(),
                    "nonterminal vertical fan-in raw oracle failed for {fixture} {style:?} optimized={optimized}: {errors:?}\n{}",
                    first.output
                );
                let arrow = match (parsed.graph.direction, style) {
                    (termiflow::graph::Direction::TD, BaseStyle::Ascii) => 'v',
                    (termiflow::graph::Direction::TD, BaseStyle::Unicode) => '↓',
                    (termiflow::graph::Direction::BT, BaseStyle::Ascii) => '^',
                    (termiflow::graph::Direction::BT, BaseStyle::Unicode) => '↑',
                    _ => unreachable!("focused nonterminal fan-in matrix only uses TD/BT"),
                };
                assert_eq!(
                    first.output.matches(arrow).count(),
                    3,
                    "nonterminal vertical fan-in must show two incoming and one downstream arrow for {fixture} {style:?} optimized={optimized}:\n{}",
                    first.output
                );
                let arrow_owners: HashSet<String> = first
                    .semantic_frame
                    .cells
                    .iter()
                    .filter(|cell| cell.role == termiflow::render::semantic::CellRole::ArrowTip)
                    .filter_map(|cell| cell.owner_id.clone())
                    .collect();
                assert_eq!(
                    arrow_owners.len(),
                    3,
                    "nonterminal vertical fan-in must preserve one owner per arrow for {fixture} {style:?} optimized={optimized}: {arrow_owners:?}"
                );
                assert_eq!(
                    first.critic_report.findings.len(),
                    0,
                    "nonterminal vertical fan-in critic findings for {fixture} {style:?} optimized={optimized}: {:?}",
                    first.critic_report.findings
                );
            }
        }
    }
}

#[test]
fn subgraph_fan_in_remains_owned_by_boundary_layout() {
    for direction in ["TD", "BT", "LR", "RL"] {
        let input = fs::read_to_string(format!(
            "tests/fixtures/inputs/subgraph_fanin_{}.md",
            direction.to_lowercase()
        ))
        .expect("read subgraph fan-in fixture");
        let parsed = parse(&input, false).expect("parse subgraph fan-in fixture");
        let (anchor_id, source_ids, _) = pure_fan_in_shape(&parsed.graph);
        assert!(
            source_ids.iter().any(|source_id| {
                parsed
                    .graph
                    .edge_crosses_subgraph_boundary(source_id, &anchor_id)
            }),
            "fixture must cross a declared boundary for {direction}"
        );

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            let outcome =
                termiflow::render_with_feedback(&input, RenderOptions::new().with_style(style))
                    .expect("render subgraph fan-in fixture");
            assert!(
                outcome
                    .critic_report
                    .findings
                    .iter()
                    .all(|finding| finding.code != termiflow::FindingCode::RouteSymmetryImbalance),
                "boundary-owned fan-in should not be claimed by the pure normalizer for {direction} {style:?}: {:?}",
                outcome.critic_report.findings
            );
        }
    }
}

#[test]
fn strict_subgraph_fan_in_preserves_boundary_lanes_and_target_ports() {
    for direction in ["td", "bt", "lr", "rl"] {
        let input = fs::read_to_string(format!(
            "tests/fixtures/inputs/subgraph_fanin_{direction}.md"
        ))
        .expect("read strict subgraph fan-in fixture");
        let parsed = parse(&input, false).expect("parse strict subgraph fan-in fixture");

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let mut graph = parsed.graph.clone();
                let mut config = Config::default();
                config.optimize_render = optimized;
                config.crop = false;
                config.composite_style = CompositeStyle::from_base(style);
                config.spacing = config.spacing.for_direction(graph.direction);
                measure::measure_graph(&mut graph, &config);
                let (graph, outcome) = layout_and_render_with_feedback(graph, config)
                    .expect("render strict subgraph fan-in fixture");
                let (target_id, source_ids, _) = pure_fan_in_shape(&graph);
                let target = graph
                    .get_node(&target_id)
                    .expect("strict fan-in target after layout");
                let subgraph = graph
                    .subgraphs
                    .first()
                    .expect("strict fan-in source subgraph after layout");
                let mut lanes: Vec<usize> = source_ids
                    .iter()
                    .map(|source_id| {
                        let source = graph
                            .get_node(source_id)
                            .expect("strict fan-in source after layout");
                        match graph.direction {
                            termiflow::graph::Direction::TD
                            | termiflow::graph::Direction::TB
                            | termiflow::graph::Direction::BT => source.center_x(),
                            termiflow::graph::Direction::LR | termiflow::graph::Direction::RL => {
                                source.center_y()
                            }
                        }
                    })
                    .collect();
                lanes.sort_unstable();
                lanes.dedup();
                assert_eq!(
                    lanes.len(),
                    source_ids.len(),
                    "strict fan-in source lanes collapsed for {direction} {style:?} optimized={optimized}"
                );

                let boundary = match graph.direction {
                    termiflow::graph::Direction::TD | termiflow::graph::Direction::TB => lanes
                        .iter()
                        .map(|lane| (*lane, subgraph.bounds.y + subgraph.bounds.height - 1))
                        .collect::<Vec<_>>(),
                    termiflow::graph::Direction::BT => lanes
                        .iter()
                        .map(|lane| (*lane, subgraph.bounds.y))
                        .collect::<Vec<_>>(),
                    termiflow::graph::Direction::LR => lanes
                        .iter()
                        .map(|lane| (subgraph.bounds.x + subgraph.bounds.width - 1, *lane))
                        .collect::<Vec<_>>(),
                    termiflow::graph::Direction::RL => lanes
                        .iter()
                        .map(|lane| (subgraph.bounds.x, *lane))
                        .collect::<Vec<_>>(),
                };
                let chars = CompositeStyle::from_base(style).to_style_chars(style);
                let expected_portal = match graph.direction {
                    termiflow::graph::Direction::TD
                    | termiflow::graph::Direction::TB
                    | termiflow::graph::Direction::BT => chars.edge_v,
                    termiflow::graph::Direction::LR | termiflow::graph::Direction::RL => {
                        chars.edge_h
                    }
                };
                for (x, y) in boundary {
                    assert_eq!(
                        raw_char_at(&outcome.output, x, y),
                        Some(expected_portal),
                        "strict fan-in boundary lane missing at ({x},{y}) for {direction} {style:?} optimized={optimized}:\n{}",
                        outcome.output
                    );
                }

                let arrow = match graph.direction {
                    termiflow::graph::Direction::TD | termiflow::graph::Direction::TB => {
                        chars.arrow_down
                    }
                    termiflow::graph::Direction::BT => chars.arrow_up,
                    termiflow::graph::Direction::LR => chars.arrow_right,
                    termiflow::graph::Direction::RL => chars.arrow_left,
                };
                assert_eq!(
                    outcome.output.matches(arrow).count(),
                    source_ids.len(),
                    "strict fan-in must keep one target arrow per source for {direction} {style:?} optimized={optimized}:\n{}",
                    outcome.output
                );
                let target_ports: Vec<(usize, usize)> = match graph.direction {
                    termiflow::graph::Direction::TD
                    | termiflow::graph::Direction::TB
                    | termiflow::graph::Direction::BT => {
                        let center = target.x + target.width / 2;
                        let start = center.saturating_sub(source_ids.len().saturating_sub(1));
                        let y = if graph.direction == termiflow::graph::Direction::BT {
                            target.bottom_y()
                        } else {
                            target.y.saturating_sub(1)
                        };
                        (0..source_ids.len())
                            .map(|index| (start + index * 2, y))
                            .collect()
                    }
                    termiflow::graph::Direction::LR | termiflow::graph::Direction::RL => {
                        let center = target.y + target.height / 2;
                        let start = center.saturating_sub(source_ids.len().saturating_sub(1));
                        let x = if graph.direction == termiflow::graph::Direction::LR {
                            target.x.saturating_sub(1)
                        } else {
                            target.x + target.width
                        };
                        (0..source_ids.len())
                            .map(|index| (x, start + index * 2))
                            .collect()
                    }
                };
                let expected_owners: HashSet<String> = graph
                    .edges
                    .iter()
                    .enumerate()
                    .map(|(index, edge)| format!("edge:{index}:{}->{}", edge.from, edge.to))
                    .collect();
                let arrow_cells: Vec<_> = outcome
                    .semantic_frame
                    .cells
                    .iter()
                    .filter(|cell| cell.role == termiflow::render::semantic::CellRole::ArrowTip)
                    .collect();
                assert_eq!(
                    arrow_cells.len(),
                    source_ids.len(),
                    "strict fan-in semantic arrow count mismatch for {direction} {style:?} optimized={optimized}: {arrow_cells:?}"
                );
                let arrow_owners: HashSet<String> = arrow_cells
                    .iter()
                    .filter_map(|cell| cell.owner_id.clone())
                    .collect();
                assert_eq!(
                    arrow_owners, expected_owners,
                    "strict fan-in target arrows must retain one edge owner per source for {direction} {style:?} optimized={optimized}: {arrow_owners:?}"
                );
                for (x, y) in target_ports {
                    assert_eq!(
                        raw_char_at(&outcome.output, x, y),
                        Some(arrow),
                        "strict fan-in target port missing at ({x},{y}) for {direction} {style:?} optimized={optimized}:\n{}",
                        outcome.output
                    );
                }
                assert!(
                    outcome
                        .critic_report
                        .findings
                        .iter()
                        .all(|finding| finding.code != termiflow::FindingCode::RouteSymmetryImbalance),
                    "strict fan-in route symmetry finding for {direction} {style:?} optimized={optimized}: {:?}",
                    outcome.critic_report.findings
                );
                assert_eq!(
                    target.id, target_id,
                    "pure fan-in target changed identity during strict boundary routing"
                );
            }
        }
    }
}
