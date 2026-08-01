//! Branch-balance critic rules.
//!
//! These rules inspect graph geometry before cell-level projection and are
//! kept separate from topology, labels, and border-artifact rules.

use super::{
    node_primary_center, node_secondary_center, node_secondary_end, node_secondary_start,
    CriticFinding, FindingCode, FindingSeverity,
};
use crate::graph::{Direction, Graph};
use std::collections::HashMap;

type BranchSets<'a> = (
    HashMap<&'a str, Vec<&'a str>>,
    HashMap<&'a str, Vec<&'a str>>,
);

pub(super) fn find_route_symmetry_imbalances(
    graph: &Graph,
    direction: Direction,
) -> Vec<CriticFinding> {
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

pub(super) fn find_branch_spacing_imbalances(
    graph: &Graph,
    direction: Direction,
) -> Vec<CriticFinding> {
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

pub(super) fn find_branch_crowding(graph: &Graph, direction: Direction) -> Vec<CriticFinding> {
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
    if other_ids.iter().any(|node_id| {
        if anchor_is_source {
            graph.edge_crosses_subgraph_boundary(anchor_id, node_id)
        } else {
            graph.edge_crosses_subgraph_boundary(node_id, anchor_id)
        }
    }) {
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
