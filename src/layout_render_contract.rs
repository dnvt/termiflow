//! Private layout/render contracts shared by the final layout and scene stages.
//!
//! This module deliberately does not add metadata to the public `Graph`.  A
//! contract is an immutable sidecar owned by the layout-and-render orchestration
//! and is absent for callers that render a manually constructed graph directly.

use std::collections::{HashMap, HashSet};

use sha2::{Digest, Sha256};

use crate::geom::Rect;
use crate::graph::{Direction, EdgeKind, Graph, NodeShape};
use crate::portals::{
    nudge_portal_x_from_corners, title_safe_portal_x, PortalColumnPreference,
    BT_SIBLING_CHAIN_TITLE_MARGIN,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BtSiblingTransition {
    pub(crate) edge_index: usize,
    pub(crate) edge_id: String,
    pub(crate) source_subgraph_id: String,
    pub(crate) target_subgraph_id: String,
    pub(crate) source_node_id: String,
    pub(crate) target_node_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BtSiblingEndpoint {
    pub(crate) edge_index: usize,
    pub(crate) edge_id: String,
    pub(crate) source_subgraph_id: String,
    pub(crate) target_subgraph_id: String,
    pub(crate) source_node_id: String,
    pub(crate) target_node_id: String,
    pub(crate) source_lane: usize,
    pub(crate) target_lane: usize,
    pub(crate) source_boundary_row: usize,
    pub(crate) target_boundary_row: usize,
    pub(crate) corridor: Rect,
    pub(crate) title_clearance_proven: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BtSiblingEndpointContract {
    pub(crate) transitions: Vec<BtSiblingEndpoint>,
    pub(crate) digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BtSiblingLanePlan {
    pub(crate) content_lanes: Vec<(String, usize)>,
    pub(crate) reference_centers: HashMap<String, usize>,
}

const BT_SIBLING_MIN_RAIL_GAP: usize = 3;

impl BtSiblingEndpointContract {
    pub(crate) fn for_edge(&self, edge_index: usize) -> Option<&BtSiblingEndpoint> {
        self.transitions
            .iter()
            .find(|transition| transition.edge_index == edge_index)
    }
}

/// Return the complete strict BT sibling-chain crossing order, from the
/// lowest source subgraph toward the highest target subgraph.
pub(crate) fn strict_bt_transitions(
    graph: &Graph,
    bounds: &HashMap<String, Rect>,
) -> Option<Vec<BtSiblingTransition>> {
    if graph.direction != Direction::BT
        || graph.subgraphs.len() < 3
        || graph.has_cycles()
        || graph.edges.iter().any(|edge| edge.is_back_edge)
    {
        return None;
    }

    let parent_id = graph.subgraphs.first()?.parent_id.clone();
    let mut chain: Vec<_> = graph
        .subgraphs
        .iter()
        .filter(|subgraph| {
            subgraph.parent_id == parent_id
                && subgraph.child_ids.is_empty()
                && subgraph.title.is_some()
                && subgraph.node_ids.len() == 2
                && bounds
                    .get(&subgraph.id)
                    .is_some_and(|rect| !rect.is_empty())
        })
        .collect();
    if chain.len() != graph.subgraphs.len() {
        return None;
    }

    chain.sort_by(|left, right| {
        let left_bounds = bounds.get(&left.id).expect("strict chain bound");
        let right_bounds = bounds.get(&right.id).expect("strict chain bound");
        right_bounds
            .y
            .cmp(&left_bounds.y)
            .then_with(|| left_bounds.x.cmp(&right_bounds.x))
            .then_with(|| left.id.cmp(&right.id))
    });
    if chain.windows(2).any(|pair| {
        let lower = bounds.get(&pair[0].id).expect("strict chain bound");
        let upper = bounds.get(&pair[1].id).expect("strict chain bound");
        lower.y <= upper.y || upper.bottom() > lower.y
    }) {
        return None;
    }

    let mut node_to_subgraph = HashMap::new();
    for subgraph in &chain {
        for node_id in &subgraph.node_ids {
            let node = graph.get_node(node_id)?;
            if node.shape != NodeShape::Rectangle {
                return None;
            }
            node_to_subgraph.insert(node_id.as_str(), subgraph.id.as_str());
        }
    }
    if node_to_subgraph.len() != graph.nodes.len() {
        return None;
    }

    let ordinary_edges: Vec<_> = graph
        .edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| !edge.is_back_edge)
        .collect();
    if ordinary_edges.len() != chain.len() * 2 - 1
        || ordinary_edges.iter().any(|(_, edge)| {
            edge.kind != EdgeKind::Arrow
                || edge.label.is_some()
                || !node_to_subgraph.contains_key(edge.from.as_str())
                || !node_to_subgraph.contains_key(edge.to.as_str())
        })
    {
        return None;
    }

    for subgraph in &chain {
        let internal_count = ordinary_edges
            .iter()
            .filter(|(_, edge)| {
                node_to_subgraph.get(edge.from.as_str()) == Some(&subgraph.id.as_str())
                    && node_to_subgraph.get(edge.to.as_str()) == Some(&subgraph.id.as_str())
            })
            .count();
        if internal_count != 1 {
            return None;
        }
    }

    let expected_pairs: HashSet<(&str, &str)> = chain
        .windows(2)
        .map(|pair| (pair[0].id.as_str(), pair[1].id.as_str()))
        .collect();
    let mut crossing_counts: HashMap<(&str, &str), usize> = HashMap::new();
    for (_, edge) in &ordinary_edges {
        let from_sg = *node_to_subgraph.get(edge.from.as_str())?;
        let to_sg = *node_to_subgraph.get(edge.to.as_str())?;
        if from_sg == to_sg {
            continue;
        }
        let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
        if exits.len() != 1 || enters.len() != 1 {
            return None;
        }
        *crossing_counts.entry((exits[0], enters[0])).or_default() += 1;
    }
    if crossing_counts.len() != expected_pairs.len()
        || crossing_counts.values().any(|count| *count != 1)
        || crossing_counts
            .keys()
            .any(|pair| !expected_pairs.contains(pair))
    {
        return None;
    }

    let mut transitions = Vec::with_capacity(chain.len().saturating_sub(1));
    for pair in chain.windows(2) {
        let source_subgraph_id = pair[0].id.clone();
        let target_subgraph_id = pair[1].id.clone();
        let candidates: Vec<_> = ordinary_edges
            .iter()
            .filter(|(_, edge)| {
                let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
                exits == vec![source_subgraph_id.as_str()]
                    && enters == vec![target_subgraph_id.as_str()]
                    && node_to_subgraph.get(edge.from.as_str())
                        == Some(&source_subgraph_id.as_str())
                    && node_to_subgraph.get(edge.to.as_str()) == Some(&target_subgraph_id.as_str())
            })
            .collect();
        if candidates.len() != 1 {
            return None;
        }
        let (edge_index, edge) = *candidates[0];
        transitions.push(BtSiblingTransition {
            edge_index,
            edge_id: stable_edge_id(edge_index, edge),
            source_subgraph_id,
            target_subgraph_id,
            source_node_id: edge.from.clone(),
            target_node_id: edge.to.clone(),
        });
    }
    Some(transitions)
}

/// Allocate content lanes so each subgraph's internal edge remains vertical,
/// while adjacent external transitions use different rails. The target rail is
/// allowed to enter anywhere on the target node's bottom edge; it does not
/// need to be the node center. This is the key role split that the previous
/// one-center alignment could not represent.
pub(crate) fn plan_bt_sibling_content_lanes(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    bounds: &HashMap<String, Rect>,
) -> Option<BtSiblingLanePlan> {
    let transitions = strict_bt_transitions(graph, bounds)?;
    let mut chain_ids = Vec::with_capacity(transitions.len() + 1);
    chain_ids.push(transitions.first()?.source_subgraph_id.clone());
    chain_ids.extend(
        transitions
            .iter()
            .map(|transition| transition.target_subgraph_id.clone()),
    );

    let mut reference_centers = HashMap::new();
    let mut source_node_by_subgraph = HashMap::new();
    for transition in &transitions {
        source_node_by_subgraph.insert(
            transition.source_subgraph_id.clone(),
            transition.source_node_id.clone(),
        );
    }
    for (index, subgraph_id) in chain_ids.iter().enumerate() {
        let node_id = source_node_by_subgraph
            .get(subgraph_id)
            .cloned()
            .or_else(|| {
                transitions
                    .last()
                    .map(|transition| transition.target_node_id.clone())
            })?;
        let rect = node_rects.get(&node_id).copied()?;
        let center = rect_center_x(rect);
        reference_centers.insert(subgraph_id.clone(), center);

        let subgraph = graph.get_subgraph(subgraph_id)?;
        let internal_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|edge| {
                !edge.is_back_edge
                    && graph.get_node_subgraph(&edge.from) == Some(subgraph_id.as_str())
                    && graph.get_node_subgraph(&edge.to) == Some(subgraph_id.as_str())
            })
            .collect();
        if internal_edges.len() != 1 {
            return None;
        }
        let source = node_rects.get(&internal_edges[0].from)?;
        let target = node_rects.get(&internal_edges[0].to)?;
        if rect_center_x(*source) != rect_center_x(*target) {
            return None;
        }
        if index == chain_ids.len().saturating_sub(1) && subgraph.node_ids.is_empty() {
            return None;
        }
    }

    let mut chosen = Vec::with_capacity(chain_ids.len());
    let mut search = LanePlanSearch {
        chain_ids: &chain_ids,
        transitions: &transitions,
        graph,
        node_rects,
        bounds,
        reference_centers: &reference_centers,
        chosen: &mut chosen,
    };
    if !search_lane_plan(&mut search, 0) {
        return None;
    }

    Some(BtSiblingLanePlan {
        content_lanes: chain_ids.into_iter().zip(chosen).collect(),
        reference_centers,
    })
}

struct LanePlanSearch<'a> {
    chain_ids: &'a [String],
    transitions: &'a [BtSiblingTransition],
    graph: &'a Graph,
    node_rects: &'a HashMap<String, Rect>,
    bounds: &'a HashMap<String, Rect>,
    reference_centers: &'a HashMap<String, usize>,
    chosen: &'a mut Vec<usize>,
}

fn search_lane_plan(search: &mut LanePlanSearch<'_>, index: usize) -> bool {
    if index == search.chain_ids.len() {
        return true;
    }
    let subgraph_id = &search.chain_ids[index];
    let Some(original_bounds) = search.bounds.get(subgraph_id).copied() else {
        return false;
    };
    let Some(&reference_center) = search.reference_centers.get(subgraph_id) else {
        return false;
    };
    let min_lane = original_bounds.x.saturating_add(1);
    let max_lane = original_bounds.right().saturating_sub(2);
    if min_lane > max_lane {
        return false;
    }
    let mut candidates: Vec<_> = (min_lane..=max_lane)
        .filter(|lane| *lane >= reference_center)
        .collect();
    candidates.sort_by_key(|lane| (lane.abs_diff(reference_center), *lane));

    for lane in candidates {
        if search
            .chosen
            .last()
            .is_some_and(|previous| previous.abs_diff(lane) < BT_SIBLING_MIN_RAIL_GAP)
        {
            continue;
        }
        let delta = lane as isize - reference_center as isize;
        let shifted_bounds = shifted_subgraph_bounds(
            search.graph,
            subgraph_id,
            original_bounds,
            delta,
            search.node_rects,
        );
        if index < search.chain_ids.len().saturating_sub(1)
            && !lane_is_title_safe(search.graph, subgraph_id, shifted_bounds, lane)
        {
            continue;
        }

        if index > 0 {
            let previous_lane = search.chosen[index - 1];
            let transition = &search.transitions[index - 1];
            let Some(target_rect) = search.node_rects.get(&transition.target_node_id).copied()
            else {
                continue;
            };
            let shifted_target = translate_rect(target_rect, delta);
            if !node_accepts_bt_entry(shifted_target, previous_lane)
                || !lane_is_title_safe(
                    search.graph,
                    &transition.target_subgraph_id,
                    shifted_bounds,
                    previous_lane,
                )
            {
                continue;
            }
        }

        search.chosen.push(lane);
        if search_lane_plan(search, index + 1) {
            return true;
        }
        search.chosen.pop();
    }
    false
}

fn lane_is_title_safe(graph: &Graph, subgraph_id: &str, bounds: Rect, lane: usize) -> bool {
    let Some(subgraph) = graph.get_subgraph(subgraph_id) else {
        return false;
    };
    let title_safe = title_safe_portal_x(
        bounds.x,
        bounds.width,
        subgraph.title.as_deref(),
        lane,
        Direction::BT,
        BT_SIBLING_CHAIN_TITLE_MARGIN,
        PortalColumnPreference::Nearest,
    );
    nudge_portal_x_from_corners(
        bounds.x,
        bounds.width,
        subgraph.title.as_deref(),
        Direction::BT,
        title_safe,
    ) == lane
}

fn node_accepts_bt_entry(rect: Rect, lane: usize) -> bool {
    let min_lane = rect.x.saturating_add(1);
    let max_lane = rect.right().saturating_sub(2);
    min_lane <= max_lane && lane >= min_lane && lane <= max_lane
}

/// Pick the receiver-owned lane for a strict BT sibling transition.
///
/// A source lane is useful for preserving the edge's external identity, but
/// choosing it first lets a long rail enter a receiver away from its center
/// and makes adjacent sibling transitions read as one trunk. Prefer the
/// nearest title-safe lane to the target node center; when the receiver is a
/// middle sibling, keep that lane away from the next outgoing source lane so
/// the two boundary roles cannot collapse into one rail. Fall back toward the
/// source lane only when the receiver center is not title-safe or no separated
/// candidate exists. The strict endpoint contract then records the lateral
/// turn explicitly in the open corridor instead of hiding it at the receiver
/// border.
fn bt_sibling_target_lane(
    target: Rect,
    target_bounds: Rect,
    title: Option<&str>,
    desired: usize,
    next_source_lane: Option<usize>,
    used_target_lanes: &[usize],
) -> Option<usize> {
    let min_lane = target.x.saturating_add(1);
    let max_lane = target.right().saturating_sub(2);
    if min_lane > max_lane {
        return None;
    }

    let candidates = (min_lane..=max_lane).collect::<Vec<_>>();
    let separated = candidates
        .iter()
        .copied()
        .filter(|lane| {
            next_source_lane
                .is_none_or(|next_lane| lane.abs_diff(next_lane) >= BT_SIBLING_MIN_RAIL_GAP)
                && used_target_lanes
                    .iter()
                    .all(|used_lane| lane.abs_diff(*used_lane) >= BT_SIBLING_MIN_RAIL_GAP)
        })
        .collect::<Vec<_>>();
    // When the receiver has enough width, avoid making the whole transition
    // collinear with its source. A short, explicit corridor turn makes the
    // boundary ownership visible; narrow receivers fall back to the proven
    // source-aligned lane below rather than inventing an unsafe hook.
    let non_collinear = separated
        .iter()
        .copied()
        .filter(|lane| *lane != desired)
        .collect::<Vec<_>>();
    let relaxed_non_collinear = candidates
        .iter()
        .copied()
        .filter(|lane| {
            *lane != desired
                && next_source_lane.is_none_or(|next_lane| *lane != next_lane)
                && used_target_lanes
                    .iter()
                    .all(|used_lane| lane.abs_diff(*used_lane) >= BT_SIBLING_MIN_RAIL_GAP)
        })
        .collect::<Vec<_>>();
    let prefer_relaxed_turn = non_collinear.is_empty() && !relaxed_non_collinear.is_empty();
    let separated = if !non_collinear.is_empty() {
        non_collinear
    } else if !relaxed_non_collinear.is_empty() {
        relaxed_non_collinear
    } else {
        separated
    };
    let separated = if separated.is_empty() {
        candidates
            .iter()
            .copied()
            .filter(|lane| {
                used_target_lanes
                    .iter()
                    .all(|used_lane| lane.abs_diff(*used_lane) >= BT_SIBLING_MIN_RAIL_GAP)
            })
            .collect::<Vec<_>>()
    } else {
        separated
    };
    let separated = if separated.is_empty() {
        candidates
    } else {
        separated
    };
    let mut candidates = separated;
    if prefer_relaxed_turn {
        candidates.sort_by_key(|lane| {
            (
                usize::MAX - lane.abs_diff(desired),
                lane.abs_diff(target.x + target.width / 2),
                *lane,
            )
        });
    } else {
        candidates.sort_by_key(|lane| {
            (
                lane.abs_diff(target.x + target.width / 2),
                lane.abs_diff(desired),
                *lane,
            )
        });
    }
    candidates.into_iter().find(|lane| {
        let title_safe = title_safe_portal_x(
            target_bounds.x,
            target_bounds.width,
            title,
            *lane,
            Direction::BT,
            BT_SIBLING_CHAIN_TITLE_MARGIN,
            PortalColumnPreference::Nearest,
        );
        node_accepts_bt_entry(target, *lane)
            && nudge_portal_x_from_corners(
                target_bounds.x,
                target_bounds.width,
                title,
                Direction::BT,
                title_safe,
            ) == *lane
    })
}

fn translate_rect(rect: Rect, delta: isize) -> Rect {
    let x = if delta.is_negative() {
        rect.x.saturating_sub(delta.unsigned_abs())
    } else {
        rect.x.saturating_add(delta as usize)
    };
    Rect::new(x, rect.y, rect.width, rect.height)
}

fn shifted_subgraph_bounds(
    graph: &Graph,
    subgraph_id: &str,
    original: Rect,
    delta: isize,
    node_rects: &HashMap<String, Rect>,
) -> Rect {
    let Some(subgraph) = graph.get_subgraph(subgraph_id) else {
        return translate_rect(original, delta);
    };
    let mut min_x = usize::MAX;
    let mut max_right = 0;
    for node_id in &subgraph.node_ids {
        let Some(rect) = node_rects.get(node_id).copied() else {
            return translate_rect(original, delta);
        };
        let shifted = translate_rect(rect, delta);
        min_x = min_x.min(shifted.x);
        max_right = max_right.max(shifted.right());
    }
    if min_x == usize::MAX {
        return translate_rect(original, delta);
    }
    let left = original.x.min(min_x);
    let right = original.right().max(max_right);
    Rect::new(
        left,
        original.y,
        right.saturating_sub(left),
        original.height,
    )
}

/// Build the immutable contract from final graph geometry. A direct render
/// caller can use this only when its graph already contains final bounds; the
/// normal layout path supplies this result as the authoritative sidecar.
pub(crate) fn build_bt_sibling_endpoint_contract(
    graph: &Graph,
) -> Option<BtSiblingEndpointContract> {
    let node_rects = graph
        .nodes
        .iter()
        .map(|node| {
            (
                node.id.clone(),
                Rect::new(node.x, node.y, node.width, node.height),
            )
        })
        .collect::<HashMap<_, _>>();
    let bounds = graph
        .subgraphs
        .iter()
        .map(|subgraph| {
            (
                subgraph.id.clone(),
                Rect::new(
                    subgraph.bounds.x,
                    subgraph.bounds.y,
                    subgraph.bounds.width,
                    subgraph.bounds.height,
                ),
            )
        })
        .collect::<HashMap<_, _>>();
    let transitions = strict_bt_transitions(graph, &bounds)?;
    let mut records = Vec::with_capacity(transitions.len());
    let mut used_target_lanes = Vec::with_capacity(transitions.len());

    for (transition_index, transition) in transitions.iter().enumerate() {
        let source = node_rects.get(&transition.source_node_id).copied()?;
        let target = node_rects.get(&transition.target_node_id).copied()?;
        let source_bounds = bounds.get(&transition.source_subgraph_id).copied()?;
        let target_bounds = bounds.get(&transition.target_subgraph_id).copied()?;
        let source_lane = rect_center_x(source);
        let target_lane = bt_sibling_target_lane(
            target,
            target_bounds,
            graph
                .get_subgraph(&transition.target_subgraph_id)
                .and_then(|subgraph| subgraph.title.as_deref()),
            source_lane,
            transitions
                .get(transition_index.saturating_add(1))
                .and_then(|next| node_rects.get(&next.source_node_id))
                .map(|rect| rect_center_x(*rect)),
            &used_target_lanes,
        )?;
        used_target_lanes.push(target_lane);
        if !lane_is_title_safe(
            graph,
            &transition.target_subgraph_id,
            target_bounds,
            target_lane,
        ) || source_lane < source_bounds.x.saturating_add(1)
            || source_lane > source_bounds.right().saturating_sub(2)
            || target_lane < target_bounds.x.saturating_add(1)
            || target_lane > target_bounds.right().saturating_sub(2)
        {
            return None;
        }

        let source_boundary_row = source_bounds.y;
        let target_boundary_row = target_bounds.bottom().saturating_sub(1);
        let corridor_height = source_boundary_row
            .saturating_sub(target_boundary_row)
            .saturating_sub(1);
        if corridor_height < 4 {
            return None;
        }
        let corridor = Rect::new(
            source_lane.min(target_lane),
            target_boundary_row.saturating_add(1),
            source_lane.abs_diff(target_lane).saturating_add(1),
            corridor_height,
        );
        records.push(BtSiblingEndpoint {
            edge_index: transition.edge_index,
            edge_id: transition.edge_id.clone(),
            source_subgraph_id: transition.source_subgraph_id.clone(),
            target_subgraph_id: transition.target_subgraph_id.clone(),
            source_node_id: transition.source_node_id.clone(),
            target_node_id: transition.target_node_id.clone(),
            source_lane,
            target_lane,
            source_boundary_row,
            target_boundary_row,
            corridor,
            title_clearance_proven: true,
        });
    }

    if records
        .windows(2)
        .any(|pair| pair[0].source_lane == pair[1].source_lane)
    {
        return None;
    }
    let digest = contract_digest(&records);
    Some(BtSiblingEndpointContract {
        transitions: records,
        digest,
    })
}

fn contract_digest(records: &[BtSiblingEndpoint]) -> String {
    let mut hasher = Sha256::new();
    for record in records {
        hasher.update(
            format!(
                "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{};",
                record.edge_index,
                record.edge_id,
                record.source_subgraph_id,
                record.target_subgraph_id,
                record.source_node_id,
                record.target_node_id,
                record.source_lane,
                record.target_lane,
                record.source_boundary_row,
                record.target_boundary_row,
                record.corridor.x,
                record.corridor.y,
                record.corridor.width,
                record.corridor.height,
            )
            .as_bytes(),
        );
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn stable_edge_id(edge_index: usize, edge: &crate::graph::Edge) -> String {
    format!("edge:{edge_index}:{}->{}", edge.from, edge.to)
}

fn rect_center_x(rect: Rect) -> usize {
    rect.x + rect.width / 2
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "maintainer-fixtures")]
    use super::*;

    #[cfg(feature = "maintainer-fixtures")]
    fn laid_out_fixture(path: &str) -> (Graph, BtSiblingEndpointContract) {
        let input = std::fs::read_to_string(path).expect("fixture");
        let parsed = crate::parser::parse(&input, false).expect("parse");
        let (graph, contract) = crate::layout::apply_coarse_layout_with_contract(
            parsed.graph,
            None,
            crate::layout::CoarseLayoutConfig::default(),
        )
        .expect("layout");
        (graph, contract.expect("strict BT endpoint contract"))
    }

    #[test]
    #[cfg(feature = "maintainer-fixtures")]
    fn strict_bt_contract_is_deterministic_and_keeps_shared_frame_columns() {
        for fixture in [
            "tests/fixtures/inputs/collision_sibling_triple_bt.md",
            "tests/fixtures/inputs/subgraph_chain_bt.md",
        ] {
            let (graph, contract) = laid_out_fixture(fixture);
            assert_eq!(graph.direction, Direction::BT);
            assert_eq!(contract.transitions.len(), graph.subgraphs.len() - 1);
            assert!(contract
                .transitions
                .windows(2)
                .all(|pair| pair[0].source_lane != pair[1].source_lane));
            assert!(contract.transitions.iter().all(|transition| {
                transition.title_clearance_proven
                    && transition.corridor.width
                        == transition.source_lane.abs_diff(transition.target_lane) + 1
                    && transition.corridor.height >= 4
            }));

            let frame_spans: HashSet<(usize, usize)> = graph
                .subgraphs
                .iter()
                .map(|subgraph| {
                    (
                        subgraph.bounds.x,
                        subgraph.bounds.x.saturating_add(subgraph.bounds.width),
                    )
                })
                .collect();
            assert_eq!(
                frame_spans.len(),
                1,
                "strict BT sibling frames must retain one visible column: {fixture}"
            );

            let (_, repeated_contract) = laid_out_fixture(fixture);
            assert_eq!(contract, repeated_contract);
            assert!(!contract.digest.is_empty());
        }
    }
}
