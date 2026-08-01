//! Candidate policy for bounded layout repair.
//!
//! This module owns critic-driven candidate generation and ranking. The public
//! render API remains in the crate root; keeping this policy private makes the
//! expensive optimization boundary explicit and testable.

use crate::config::Config;
use crate::geom;
use crate::graph::{self, Graph, Node};
use crate::orientation;
use crate::render::RenderOutcome;
use crate::spacing::SpacingConfig;

/// Maximum number of unique layout candidates evaluated in one repair pass.
///
/// Candidate order is deterministic and prioritizes targeted repairs before
/// broad spacing fallbacks. The cap prevents pathological critic output from
/// multiplying full layout/render passes without affecting ordinary fixtures.
pub(crate) const MAX_LAYOUT_REPAIR_CANDIDATES: usize = 32;

#[derive(Debug, Clone)]
pub(crate) struct LayoutRepairCandidateBatch {
    pub(crate) candidates: Vec<LayoutRepairCandidate>,
    pub(crate) omitted: usize,
}

pub(crate) fn node_positions(graph: &Graph) -> std::collections::HashMap<String, geom::Point> {
    graph
        .nodes
        .iter()
        .map(|node| (node.id.clone(), geom::Point::new(node.x, node.y)))
        .collect()
}

pub(crate) fn build_layout_repair_candidates(
    graph: &Graph,
    config: &Config,
    outcome: &RenderOutcome,
) -> LayoutRepairCandidateBatch {
    use crate::render::critic::FindingCode;

    let mut candidates = Vec::new();
    let base_positions = node_positions(graph);
    let secondary_delta = secondary_nudge_delta(graph.direction, &config.spacing);

    let mut has_primary_spacing_pressure = false;
    let mut has_secondary_spacing_pressure = false;
    let mut has_label_pressure = false;
    let mut has_canvas_clipping = false;

    for finding in &outcome.critic_report.findings {
        match finding.code {
            FindingCode::ChainTooCrampedLR
            | FindingCode::ArrowTouchesNodeBorder
            | FindingCode::ArrowTouchesSubgraphBorder
            | FindingCode::EdgeLabelCollidesWithNode => has_primary_spacing_pressure = true,
            FindingCode::CrowdedEdgeLabel => has_label_pressure = true,
            FindingCode::CanvasClipped => has_canvas_clipping = true,
            FindingCode::RouteCrossesNodeInterior => {
                for owner_id in &finding.owner_ids {
                    if graph.get_node(owner_id).is_some() {
                        push_targeted_node_nudge_candidates(
                            &mut candidates,
                            graph.direction,
                            &config.spacing,
                            &base_positions,
                            std::slice::from_ref(owner_id),
                            secondary_delta,
                        );
                    }
                }
            }
            FindingCode::SubgraphTitleCorrupted => {
                for owner_id in &finding.owner_ids {
                    if graph.get_subgraph(owner_id).is_some() {
                        push_targeted_subgraph_nudge_candidates(
                            &mut candidates,
                            graph,
                            &config.spacing,
                            &base_positions,
                            owner_id,
                            secondary_delta,
                        );
                    }
                }
            }
            FindingCode::RouteSymmetryImbalance => {
                push_branch_recenter_candidate(
                    &mut candidates,
                    graph,
                    &config.spacing,
                    &base_positions,
                    &finding.owner_ids,
                );
            }
            FindingCode::BranchSpacingImbalance => {
                push_branch_spacing_candidate(
                    &mut candidates,
                    graph,
                    &config.spacing,
                    &base_positions,
                    &finding.owner_ids,
                );
            }
            FindingCode::BranchCrowding => {
                has_secondary_spacing_pressure = true;
                push_branch_spread_candidate(
                    &mut candidates,
                    graph,
                    &config.spacing,
                    &base_positions,
                    &finding.owner_ids,
                );
            }
            _ => {}
        }

        if matches!(
            finding.code,
            FindingCode::CrowdedEdgeLabel | FindingCode::EdgeLabelCollidesWithNode
        ) {
            for owner_id in &finding.owner_ids {
                push_edge_label_nudge_candidates(
                    &mut candidates,
                    graph,
                    &config.spacing,
                    &base_positions,
                    owner_id,
                    secondary_delta,
                );
            }
        }

        if finding.code == FindingCode::ArrowTouchesNodeBorder {
            for owner_id in &finding.owner_ids {
                if graph.get_node(owner_id).is_some() {
                    push_targeted_node_nudge_candidates(
                        &mut candidates,
                        graph.direction,
                        &config.spacing,
                        &base_positions,
                        std::slice::from_ref(owner_id),
                        secondary_delta,
                    );
                }
            }
        }

        if finding.code == FindingCode::ArrowTouchesSubgraphBorder {
            for owner_id in &finding.owner_ids {
                if graph.get_subgraph(owner_id).is_some() {
                    push_targeted_subgraph_nudge_candidates(
                        &mut candidates,
                        graph,
                        &config.spacing,
                        &base_positions,
                        owner_id,
                        secondary_delta,
                    );
                }
            }
        }
    }

    if has_primary_spacing_pressure {
        let mut spacing = config.spacing.clone();
        if matches!(graph.direction, graph::Direction::LR | graph::Direction::RL) {
            spacing.col_spacing += 2;
            spacing.stem_length_horizontal += 2;
        } else {
            spacing.row_spacing += 1;
            spacing.stem_length_vertical += 1;
        }
        push_spacing_candidate(&mut candidates, spacing);
    }

    if has_label_pressure {
        let mut spacing = config.spacing.clone();
        spacing.row_spacing += 1;
        if matches!(graph.direction, graph::Direction::LR | graph::Direction::RL) {
            spacing.col_spacing += 1;
        }
        push_spacing_candidate(&mut candidates, spacing);
    }

    if has_secondary_spacing_pressure {
        let mut spacing = config.spacing.clone();
        match graph.direction {
            graph::Direction::TD | graph::Direction::TB | graph::Direction::BT => {
                spacing.col_spacing += 2;
            }
            graph::Direction::LR | graph::Direction::RL => {
                spacing.row_spacing += 1;
            }
        }
        push_spacing_candidate(&mut candidates, spacing);
    }

    if has_canvas_clipping {
        let mut spacing = config.spacing.clone();
        spacing.max_canvas_width = spacing.max_canvas_width.saturating_mul(2);
        spacing.max_canvas_height = spacing.max_canvas_height.saturating_mul(2);
        push_spacing_candidate(&mut candidates, spacing);
    }

    if candidates.is_empty() && config.optimize_render && outcome.critic_report.score > 0 {
        let mut spacing = config.spacing.clone();
        spacing.col_spacing += 1;
        spacing.row_spacing += 1;
        push_spacing_candidate(&mut candidates, spacing);
    }

    let omitted = cap_candidates(&mut candidates);
    LayoutRepairCandidateBatch {
        candidates,
        omitted,
    }
}

fn cap_candidates(candidates: &mut Vec<LayoutRepairCandidate>) -> usize {
    let omitted = candidates
        .len()
        .saturating_sub(MAX_LAYOUT_REPAIR_CANDIDATES);
    candidates.truncate(MAX_LAYOUT_REPAIR_CANDIDATES);
    omitted
}

pub(crate) fn budget_warning(omitted: usize) -> String {
    format!(
        "layout repair candidate budget capped at {MAX_LAYOUT_REPAIR_CANDIDATES}; omitted {omitted} candidate(s)"
    )
}

#[derive(Debug, Clone)]
pub(crate) struct LayoutRepairCandidate {
    pub(crate) spacing: SpacingConfig,
    pub(crate) prior_positions: Option<std::collections::HashMap<String, geom::Point>>,
}

pub(crate) fn is_better_outcome(candidate: &RenderOutcome, baseline: &RenderOutcome) -> bool {
    (
        candidate.critic_report.score,
        candidate.critic_report.findings.len(),
        candidate
            .semantic_frame
            .width
            .saturating_mul(candidate.semantic_frame.height),
    ) < (
        baseline.critic_report.score,
        baseline.critic_report.findings.len(),
        baseline
            .semantic_frame
            .width
            .saturating_mul(baseline.semantic_frame.height),
    )
}

fn push_spacing_candidate(candidates: &mut Vec<LayoutRepairCandidate>, spacing: SpacingConfig) {
    push_unique_layout_candidate(
        candidates,
        LayoutRepairCandidate {
            spacing,
            prior_positions: None,
        },
    );
}

fn push_unique_layout_candidate(
    candidates: &mut Vec<LayoutRepairCandidate>,
    candidate: LayoutRepairCandidate,
) {
    if !candidates
        .iter()
        .any(|existing| layout_candidate_eq(existing, &candidate))
    {
        candidates.push(candidate);
    }
}

fn spacing_eq(a: &SpacingConfig, b: &SpacingConfig) -> bool {
    a.box_height == b.box_height
        && a.box_min_width == b.box_min_width
        && a.box_padding == b.box_padding
        && a.row_spacing == b.row_spacing
        && a.col_spacing == b.col_spacing
        && a.node_margin == b.node_margin
        && a.subgraph_gutter == b.subgraph_gutter
        && a.stem_length_vertical == b.stem_length_vertical
        && a.stem_length_horizontal == b.stem_length_horizontal
        && a.max_label_width == b.max_label_width
        && a.max_canvas_width == b.max_canvas_width
        && a.max_canvas_height == b.max_canvas_height
        && a.cycle_gutter == b.cycle_gutter
}

fn layout_candidate_eq(a: &LayoutRepairCandidate, b: &LayoutRepairCandidate) -> bool {
    spacing_eq(&a.spacing, &b.spacing) && a.prior_positions == b.prior_positions
}

fn secondary_nudge_delta(direction: graph::Direction, spacing: &SpacingConfig) -> usize {
    match direction {
        graph::Direction::TD | graph::Direction::TB | graph::Direction::BT => {
            (spacing.col_spacing / 2).max(1)
        }
        graph::Direction::LR | graph::Direction::RL => (spacing.row_spacing / 2).max(1),
    }
}

fn push_edge_label_nudge_candidates(
    candidates: &mut Vec<LayoutRepairCandidate>,
    graph: &Graph,
    spacing: &SpacingConfig,
    base_positions: &std::collections::HashMap<String, geom::Point>,
    owner_id: &str,
    delta: usize,
) {
    let Some(edge) = graph.edges.iter().enumerate().find_map(|(idx, edge)| {
        (crate::render::provenance::edge_owner_id(idx, edge) == owner_id).then_some(edge)
    }) else {
        return;
    };

    let outward = build_secondary_nudged_positions(
        base_positions,
        graph.direction,
        &[edge.from.as_str()],
        &[edge.to.as_str()],
        delta,
    );
    push_unique_layout_candidate(
        candidates,
        LayoutRepairCandidate {
            spacing: spacing.clone(),
            prior_positions: Some(outward),
        },
    );

    let inward = build_secondary_nudged_positions(
        base_positions,
        graph.direction,
        &[edge.to.as_str()],
        &[edge.from.as_str()],
        delta,
    );
    push_unique_layout_candidate(
        candidates,
        LayoutRepairCandidate {
            spacing: spacing.clone(),
            prior_positions: Some(inward),
        },
    );
}

fn push_targeted_node_nudge_candidates(
    candidates: &mut Vec<LayoutRepairCandidate>,
    direction: graph::Direction,
    spacing: &SpacingConfig,
    base_positions: &std::collections::HashMap<String, geom::Point>,
    node_ids: &[String],
    delta: usize,
) {
    let refs: Vec<&str> = node_ids.iter().map(String::as_str).collect();
    push_secondary_nudge_candidate(
        candidates,
        direction,
        spacing,
        base_positions,
        &refs,
        delta,
        true,
    );
    push_secondary_nudge_candidate(
        candidates,
        direction,
        spacing,
        base_positions,
        &refs,
        delta,
        false,
    );
}

fn push_targeted_subgraph_nudge_candidates(
    candidates: &mut Vec<LayoutRepairCandidate>,
    graph: &Graph,
    spacing: &SpacingConfig,
    base_positions: &std::collections::HashMap<String, geom::Point>,
    subgraph_id: &str,
    delta: usize,
) {
    let Some(subgraph) = graph.get_subgraph(subgraph_id) else {
        return;
    };
    let node_ids: Vec<&str> = subgraph.node_ids.iter().map(String::as_str).collect();
    push_secondary_nudge_candidate(
        candidates,
        graph.direction,
        spacing,
        base_positions,
        &node_ids,
        delta,
        true,
    );
    push_secondary_nudge_candidate(
        candidates,
        graph.direction,
        spacing,
        base_positions,
        &node_ids,
        delta,
        false,
    );
}

fn push_secondary_nudge_candidate(
    candidates: &mut Vec<LayoutRepairCandidate>,
    direction: graph::Direction,
    spacing: &SpacingConfig,
    base_positions: &std::collections::HashMap<String, geom::Point>,
    node_ids: &[&str],
    delta: usize,
    positive: bool,
) {
    let nudged = build_secondary_nudged_positions(
        base_positions,
        direction,
        if positive { &[] } else { node_ids },
        if positive { node_ids } else { &[] },
        delta,
    );
    push_unique_layout_candidate(
        candidates,
        LayoutRepairCandidate {
            spacing: spacing.clone(),
            prior_positions: Some(nudged),
        },
    );
}

fn push_branch_recenter_candidate(
    candidates: &mut Vec<LayoutRepairCandidate>,
    graph: &Graph,
    spacing: &SpacingConfig,
    base_positions: &std::collections::HashMap<String, geom::Point>,
    owner_ids: &[String],
) {
    let Some((anchor_id, branch_ids)) = owner_ids.split_first() else {
        return;
    };
    if branch_ids.is_empty() {
        return;
    }

    let Some(anchor) = graph.get_node(anchor_id) else {
        return;
    };
    let mut secondaries: Vec<usize> = branch_ids
        .iter()
        .filter_map(|node_id| graph.get_node(node_id))
        .map(|node| secondary_center(node, graph.direction))
        .collect();
    if secondaries.len() < 2 {
        return;
    }
    secondaries.sort_unstable();
    let min_secondary = secondaries[0];
    let max_secondary = secondaries[secondaries.len() - 1];
    if max_secondary <= min_secondary {
        return;
    }

    let anchor_secondary = secondary_center(anchor, graph.direction);
    let midpoint = (min_secondary + max_secondary) / 2;
    if anchor_secondary != midpoint {
        // Try moving the anchor toward the branch midpoint. This is the most
        // direct repair when the anchor has a stable rank and its parents do
        // not leave enough room to move every branch in the opposite direction.
        let anchor_delta = signed_delta(midpoint, anchor_secondary);
        let anchor_refs = [anchor_id.as_str()];
        let nudged_anchor = build_signed_secondary_shift_positions(
            base_positions,
            graph.direction,
            &anchor_refs,
            anchor_delta,
        );
        push_unique_layout_candidate(
            candidates,
            LayoutRepairCandidate {
                spacing: spacing.clone(),
                prior_positions: Some(nudged_anchor),
            },
        );

        // Also retain the branch-shift alternative for cases where the anchor
        // is constrained by a surrounding layout or subgraph.
        let branch_delta = signed_delta(anchor_secondary, midpoint);
        let refs: Vec<&str> = branch_ids.iter().map(String::as_str).collect();
        let nudged_branches = build_signed_secondary_shift_positions(
            base_positions,
            graph.direction,
            &refs,
            branch_delta,
        );
        push_unique_layout_candidate(
            candidates,
            LayoutRepairCandidate {
                spacing: spacing.clone(),
                prior_positions: Some(nudged_branches),
            },
        );
    }
}

fn push_branch_spacing_candidate(
    candidates: &mut Vec<LayoutRepairCandidate>,
    graph: &Graph,
    spacing: &SpacingConfig,
    base_positions: &std::collections::HashMap<String, geom::Point>,
    owner_ids: &[String],
) {
    let Some((_anchor_id, branch_ids)) = owner_ids.split_first() else {
        return;
    };
    if branch_ids.len() < 3 {
        return;
    }

    let mut branches: Vec<(&str, usize)> = branch_ids
        .iter()
        .filter_map(|node_id| {
            graph
                .get_node(node_id)
                .map(|node| (node_id.as_str(), secondary_center(node, graph.direction)))
        })
        .collect();
    if branches.len() < 3 {
        return;
    }

    branches.sort_unstable_by_key(|(_, secondary)| *secondary);
    let min_secondary = branches[0].1;
    let max_secondary = branches[branches.len() - 1].1;
    if max_secondary <= min_secondary {
        return;
    }

    let span = max_secondary - min_secondary;
    let denominator = branches.len() - 1;
    let coords = orientation::OrientedCoords::new(graph.direction);
    let mut positions = base_positions.clone();
    let mut changed = false;

    for (index, (node_id, current_secondary)) in branches.iter().enumerate() {
        let target_secondary = min_secondary + ((span * index) + (denominator / 2)) / denominator;
        let delta = signed_delta(target_secondary, *current_secondary);
        if delta == 0 {
            continue;
        }

        let Some(point) = positions.get_mut(*node_id) else {
            continue;
        };
        match coords.secondary {
            orientation::Axis::Horizontal => point.x = apply_signed_delta(point.x, delta),
            orientation::Axis::Vertical => point.y = apply_signed_delta(point.y, delta),
        }
        changed = true;
    }

    if changed {
        push_unique_layout_candidate(
            candidates,
            LayoutRepairCandidate {
                spacing: spacing.clone(),
                prior_positions: Some(positions),
            },
        );
    }
}

fn push_branch_spread_candidate(
    candidates: &mut Vec<LayoutRepairCandidate>,
    graph: &Graph,
    spacing: &SpacingConfig,
    base_positions: &std::collections::HashMap<String, geom::Point>,
    owner_ids: &[String],
) {
    let Some((_anchor_id, branch_ids)) = owner_ids.split_first() else {
        return;
    };
    if branch_ids.len() < 2 {
        return;
    }

    let mut branches: Vec<(&str, usize, usize)> = branch_ids
        .iter()
        .filter_map(|node_id| {
            graph.get_node(node_id).map(|node| {
                (
                    node_id.as_str(),
                    secondary_start(node, graph.direction),
                    secondary_end(node, graph.direction),
                )
            })
        })
        .collect();
    if branches.len() < 2 {
        return;
    }

    branches.sort_unstable_by_key(|(_, start, _)| *start);
    let desired_gap = desired_branch_spread_gap(graph.direction);
    let original_min = branches[0].1;
    let original_max = branches[branches.len() - 1].2;

    let mut adjusted_starts = Vec::with_capacity(branches.len());
    let mut prev_end = None;
    for (_, start, end) in &branches {
        let span = end.saturating_sub(*start);
        let adjusted_start =
            prev_end.map_or(*start, |end_bound| (*start).max(end_bound + desired_gap));
        adjusted_starts.push(adjusted_start);
        prev_end = Some(adjusted_start + span);
    }

    let Some(&last_start) = adjusted_starts.last() else {
        return;
    };
    let last_span = branches[branches.len() - 1]
        .2
        .saturating_sub(branches[branches.len() - 1].1);
    let adjusted_min = adjusted_starts[0];
    let adjusted_max = last_start + last_span;
    let recenter_delta = signed_delta(
        (original_min + original_max) / 2,
        (adjusted_min + adjusted_max) / 2,
    );

    let coords = orientation::OrientedCoords::new(graph.direction);
    let mut positions = base_positions.clone();
    let mut changed = false;

    for ((node_id, start, _), adjusted_start) in branches.iter().zip(adjusted_starts) {
        let final_start = apply_signed_delta(adjusted_start, recenter_delta);
        let delta = signed_delta(final_start, *start);
        if delta == 0 {
            continue;
        }

        let Some(point) = positions.get_mut(*node_id) else {
            continue;
        };
        match coords.secondary {
            orientation::Axis::Horizontal => point.x = apply_signed_delta(point.x, delta),
            orientation::Axis::Vertical => point.y = apply_signed_delta(point.y, delta),
        }
        changed = true;
    }

    if changed {
        push_unique_layout_candidate(
            candidates,
            LayoutRepairCandidate {
                spacing: spacing.clone(),
                prior_positions: Some(positions),
            },
        );
    }
}

fn build_secondary_nudged_positions(
    base_positions: &std::collections::HashMap<String, geom::Point>,
    direction: graph::Direction,
    negative_ids: &[&str],
    positive_ids: &[&str],
    delta: usize,
) -> std::collections::HashMap<String, geom::Point> {
    let mut positions = base_positions.clone();
    let coords = orientation::OrientedCoords::new(direction);

    for node_id in negative_ids {
        if let Some(point) = positions.get_mut(*node_id) {
            match coords.secondary {
                orientation::Axis::Horizontal => point.x = point.x.saturating_sub(delta),
                orientation::Axis::Vertical => point.y = point.y.saturating_sub(delta),
            }
        }
    }

    for node_id in positive_ids {
        if let Some(point) = positions.get_mut(*node_id) {
            match coords.secondary {
                orientation::Axis::Horizontal => point.x += delta,
                orientation::Axis::Vertical => point.y += delta,
            }
        }
    }

    positions
}

fn build_signed_secondary_shift_positions(
    base_positions: &std::collections::HashMap<String, geom::Point>,
    direction: graph::Direction,
    node_ids: &[&str],
    delta: isize,
) -> std::collections::HashMap<String, geom::Point> {
    let mut positions = base_positions.clone();
    let coords = orientation::OrientedCoords::new(direction);

    for node_id in node_ids {
        if let Some(point) = positions.get_mut(*node_id) {
            match coords.secondary {
                orientation::Axis::Horizontal => point.x = apply_signed_delta(point.x, delta),
                orientation::Axis::Vertical => point.y = apply_signed_delta(point.y, delta),
            }
        }
    }

    positions
}

fn secondary_center(node: &Node, direction: graph::Direction) -> usize {
    match direction {
        graph::Direction::TD | graph::Direction::TB | graph::Direction::BT => node.center_x(),
        graph::Direction::LR | graph::Direction::RL => node.center_y(),
    }
}

fn secondary_start(node: &Node, direction: graph::Direction) -> usize {
    match direction {
        graph::Direction::TD | graph::Direction::TB | graph::Direction::BT => node.x,
        graph::Direction::LR | graph::Direction::RL => node.y,
    }
}

fn secondary_end(node: &Node, direction: graph::Direction) -> usize {
    match direction {
        graph::Direction::TD | graph::Direction::TB | graph::Direction::BT => node.x + node.width,
        graph::Direction::LR | graph::Direction::RL => {
            node.y + node.height.max(crate::style::BOX_HEIGHT)
        }
    }
}

fn desired_branch_spread_gap(direction: graph::Direction) -> usize {
    match direction {
        graph::Direction::TD | graph::Direction::TB | graph::Direction::BT => 3,
        graph::Direction::LR | graph::Direction::RL => 1,
    }
}

fn signed_delta(anchor: usize, midpoint: usize) -> isize {
    if anchor >= midpoint {
        (anchor - midpoint) as isize
    } else {
        -((midpoint - anchor) as isize)
    }
}

fn apply_signed_delta(value: usize, delta: isize) -> usize {
    if delta >= 0 {
        value.saturating_add(delta as usize)
    } else {
        value.saturating_sub((-delta) as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_cap_is_deterministic() {
        let mut candidates = Vec::new();
        for col_spacing in 0..(MAX_LAYOUT_REPAIR_CANDIDATES + 3) {
            candidates.push(LayoutRepairCandidate {
                spacing: SpacingConfig {
                    col_spacing,
                    ..SpacingConfig::default()
                },
                prior_positions: None,
            });
        }

        let omitted = cap_candidates(&mut candidates);

        assert_eq!(candidates.len(), MAX_LAYOUT_REPAIR_CANDIDATES);
        assert_eq!(omitted, 3);
        assert_eq!(candidates[0].spacing.col_spacing, 0);
        assert_eq!(
            candidates[MAX_LAYOUT_REPAIR_CANDIDATES - 1]
                .spacing
                .col_spacing,
            MAX_LAYOUT_REPAIR_CANDIDATES - 1
        );
        assert_eq!(
            budget_warning(omitted),
            "layout repair candidate budget capped at 32; omitted 3 candidate(s)"
        );
    }
}
