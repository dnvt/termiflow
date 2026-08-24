//! Shared subgraph envelope + portal helpers for layout and render.
//!
//! Provides a single source of truth for:
//! - Subgraph inner/outer rectangles (with gutters)
//! - Portal slots per side derived from crossing edges
//! - Helpers to build node rects from a laid-out graph

use std::collections::{HashMap, HashSet};

use crate::geom::Rect;
use crate::graph::{Direction, EdgeKind, Graph, NodeShape};

mod envelopes;

pub use envelopes::{compute_envelopes, node_rects_from_graph};
use envelopes::{current_node_rect, current_subgraph_bounds};

/// Portal coordinates along each side of a subgraph border.
#[derive(Debug, Clone, Default)]
pub struct PortalSlots {
    pub top: HashSet<usize>,
    pub bottom: HashSet<usize>,
    pub left: HashSet<usize>,
    pub right: HashSet<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PortalColumnPreference {
    Directional,
    Nearest,
}

/// Return the coordinated title margin for the current routing direction.
///
/// BT sibling routes use their topology-derived edge policy below; the shared
/// renderer/layout default stays direct so ordinary titled BT portals remain
/// continuous through the title row.
pub(crate) fn title_margin_for_direction(direction: Direction) -> usize {
    if direction == Direction::BT {
        0
    } else {
        2
    }
}

/// Return the target members of a strict BT sibling chain that can reserve one
/// additional title-clearance row as a scene transaction. A two-group chain is
/// valid as well: it is the smallest topology that still has a titled source,
/// a titled target, and one inter-group corridor to own.
///
/// The same live-topology predicate is consumed by layout and rendering. It
/// deliberately accepts no fixture names or labels, and rejects any scene
/// whose members cannot be proven to be one complete, simple chain.
pub(crate) fn bt_sibling_chain_target_ids(
    graph: &Graph,
    bounds: &HashMap<String, Rect>,
) -> Option<HashSet<String>> {
    if graph.direction != Direction::BT
        || graph.has_cycles()
        || graph.subgraphs.len() < 2
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

    if chain.len() != graph.subgraphs.len() || chain.len() < 2 {
        return None;
    }

    chain.sort_by_key(|subgraph| {
        let rect = bounds
            .get(&subgraph.id)
            .expect("eligible BT sibling subgraph has live bounds");
        (rect.y, rect.x, subgraph.id.as_str())
    });

    if chain.windows(2).any(|pair| {
        let upper = bounds
            .get(&pair[0].id)
            .expect("eligible BT sibling subgraph has live bounds");
        let lower = bounds
            .get(&pair[1].id)
            .expect("eligible BT sibling subgraph has live bounds");
        upper.y >= lower.y || upper.bottom() > lower.y
    }) {
        return None;
    }

    let mut node_to_subgraph = HashMap::new();
    for subgraph in &chain {
        for node_id in &subgraph.node_ids {
            let node = graph.nodes.iter().find(|node| node.id == *node_id)?;
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
        .filter(|edge| !edge.is_back_edge)
        .collect();
    if ordinary_edges.len() != chain.len() * 2 - 1
        || ordinary_edges.iter().any(|edge| {
            edge.kind != EdgeKind::Arrow
                || edge.label.is_some()
                || !node_to_subgraph.contains_key(edge.from.as_str())
                || !node_to_subgraph.contains_key(edge.to.as_str())
        })
    {
        return None;
    }

    let mut internal_counts: HashMap<&str, usize> = HashMap::new();
    for edge in &ordinary_edges {
        let from_sg = *node_to_subgraph.get(edge.from.as_str())?;
        let to_sg = *node_to_subgraph.get(edge.to.as_str())?;
        if from_sg == to_sg {
            *internal_counts.entry(from_sg).or_default() += 1;
        }
    }
    if chain
        .iter()
        .any(|subgraph| internal_counts.get(subgraph.id.as_str()).copied() != Some(1))
    {
        return None;
    }

    let mut crossing_pairs: HashMap<(&str, &str), usize> = HashMap::new();
    for edge in &ordinary_edges {
        let from_sg = *node_to_subgraph.get(edge.from.as_str())?;
        let to_sg = *node_to_subgraph.get(edge.to.as_str())?;
        if from_sg == to_sg {
            continue;
        }
        let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
        if exits.len() != 1 || enters.len() != 1 {
            return None;
        }
        *crossing_pairs.entry((exits[0], enters[0])).or_default() += 1;
    }

    let expected_pairs: HashSet<(&str, &str)> = chain
        .windows(2)
        .map(|pair| (pair[1].id.as_str(), pair[0].id.as_str()))
        .collect();
    if crossing_pairs.len() != expected_pairs.len()
        || crossing_pairs.values().any(|count| *count != 1)
        || crossing_pairs
            .keys()
            .any(|pair| !expected_pairs.contains(pair))
    {
        return None;
    }

    Some(
        chain
            .into_iter()
            .take(expected_pairs.len())
            .map(|subgraph| subgraph.id.clone())
            .collect(),
    )
}

/// Return whether a flat horizontal sibling chain needs an extra side corridor.
///
/// The strict scene renderer turns each cross-sibling transition through a
/// quiet row.  With the ordinary three-cell side pad, the turn is only one
/// cell away from a node and reads as a box-attached corner in terminal output.
/// This predicate is deliberately structural so the envelope can reserve the
/// extra side clearance for the same narrow topology without widening ordinary
/// subgraphs.
pub(crate) fn horizontal_sibling_chain_requires_extra_corridor(graph: &Graph) -> bool {
    if !matches!(graph.direction, Direction::LR | Direction::RL)
        || graph.subgraphs.len() < 3
        || graph.has_cycles()
        || graph.edges.iter().any(|edge| edge.is_back_edge)
        || graph.nodes.len() != graph.subgraphs.len().saturating_mul(2)
        || graph.edges.len() != graph.subgraphs.len().saturating_mul(2).saturating_sub(1)
    {
        return false;
    }

    let mut node_to_subgraph = HashMap::new();
    for subgraph in &graph.subgraphs {
        if subgraph.parent_id.is_some()
            || !subgraph.child_ids.is_empty()
            || subgraph.title.is_none()
            || subgraph.node_ids.len() != 2
        {
            return false;
        }
        for node_id in &subgraph.node_ids {
            let Some(node) = graph.get_node(node_id) else {
                return false;
            };
            if node.shape != NodeShape::Rectangle
                || graph.get_node_subgraph(node_id) != Some(subgraph.id.as_str())
            {
                return false;
            }
            node_to_subgraph.insert(node_id.as_str(), subgraph.id.as_str());
        }
    }

    let mut internal_counts: HashMap<&str, usize> = HashMap::new();
    let mut crossing_count = 0usize;
    for edge in &graph.edges {
        if edge.kind != EdgeKind::Arrow || edge.label.is_some() {
            return false;
        }
        let Some(from_subgraph) = node_to_subgraph.get(edge.from.as_str()).copied() else {
            return false;
        };
        let Some(to_subgraph) = node_to_subgraph.get(edge.to.as_str()).copied() else {
            return false;
        };
        if from_subgraph == to_subgraph {
            *internal_counts.entry(from_subgraph).or_default() += 1;
        } else {
            let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
            if exits.len() != 1 || enters.len() != 1 {
                return false;
            }
            crossing_count += 1;
        }
    }

    crossing_count == graph.subgraphs.len().saturating_sub(1)
        && graph
            .subgraphs
            .iter()
            .all(|subgraph| internal_counts.get(subgraph.id.as_str()) == Some(&1))
}

/// Keep a title-safe portal away from the two interior border corners.
pub(crate) fn nudge_portal_x_from_corners(
    left_x: usize,
    width: usize,
    title: Option<&str>,
    direction: Direction,
    mut x: usize,
) -> usize {
    let Some(title) = title else {
        return x;
    };
    let min = left_x.saturating_add(1);
    let max = left_x.saturating_add(width.saturating_sub(2));
    if max <= min {
        return x;
    }
    let Some((start, end)) = crate::graph::subgraph_title_span(left_x, width, title, direction)
    else {
        return x;
    };
    let in_title_text = |pos: usize| pos >= start && pos <= end;
    if x == min {
        let candidate = min.saturating_add(1);
        if candidate <= max && !in_title_text(candidate) {
            x = candidate;
        }
    } else if x == max {
        let candidate = max.saturating_sub(1);
        if candidate >= min && !in_title_text(candidate) {
            x = candidate;
        }
    }
    x
}

/// Select a title-safe horizontal portal column shared by layout and rendering.
///
/// `Directional` preserves the established policy: it chooses the nearest
/// side of the protected title band for BT while other directions prefer the
/// next column after the band. `Nearest` is used by nested route owners that
/// must preserve their incoming column as closely as possible. Keeping the
/// protected-band calculation here prevents the portal-slot map and the
/// renderer from silently selecting different columns.
pub(crate) fn title_safe_portal_x(
    left_x: usize,
    width: usize,
    title: Option<&str>,
    desired: usize,
    direction: Direction,
    title_margin: usize,
    preference: PortalColumnPreference,
) -> usize {
    let title_span =
        title.and_then(|title| crate::graph::subgraph_title_span(left_x, width, title, direction));
    title_safe_portal_x_for_span(
        left_x,
        width,
        desired,
        direction,
        title_margin,
        preference,
        title_span,
    )
}

/// Select the receiver lane used by the bounded BT sibling scene whose
/// external side reservation needs a quiet title margin. Layout stages the
/// receiver node on this lane; route lowering then sees the same lane through
/// the receiver center and does not need a title-adjacent jog.
pub(crate) fn bt_external_side_receiver_lane(
    left_x: usize,
    width: usize,
    title: Option<&str>,
    desired: usize,
) -> usize {
    title_safe_portal_x(
        left_x,
        width,
        title,
        desired,
        Direction::BT,
        BT_EXTERNAL_SIDE_RECEIVER_TITLE_MARGIN,
        PortalColumnPreference::Directional,
    )
}

/// Select a title-safe column using independent leading/trailing title gutter.
// The side-aware variant intentionally mirrors the existing scalar helper while
// keeping both gutter dimensions explicit at this policy boundary.
#[allow(clippy::too_many_arguments)]
pub(crate) fn title_safe_portal_x_with_text_padding_sides(
    left_x: usize,
    width: usize,
    title: Option<&str>,
    desired: usize,
    direction: Direction,
    leading_extra_padding: usize,
    trailing_extra_padding: usize,
    title_margin: usize,
    preference: PortalColumnPreference,
) -> usize {
    let title_span = title.and_then(|title| {
        crate::graph::subgraph_title_text_span_with_padding_sides(
            left_x,
            width,
            title,
            direction,
            leading_extra_padding,
            trailing_extra_padding,
        )
    });
    title_safe_portal_x_for_span(
        left_x,
        width,
        desired,
        direction,
        title_margin,
        preference,
        title_span,
    )
}

fn title_safe_portal_x_for_span(
    left_x: usize,
    width: usize,
    desired: usize,
    direction: Direction,
    title_margin: usize,
    preference: PortalColumnPreference,
    title_span: Option<(usize, usize)>,
) -> usize {
    let min = left_x.saturating_add(1);
    let max = left_x + width.saturating_sub(2);
    let x = desired.clamp(min, max);
    let Some((start, end)) = title_span else {
        return x;
    };

    let protected_start = start.saturating_sub(title_margin);
    let protected_end = end.saturating_add(title_margin).min(max);
    if x < protected_start || x > protected_end {
        return x;
    }

    let choose_nearest =
        preference == PortalColumnPreference::Nearest || direction == Direction::BT;
    let selected = if choose_nearest {
        let left = (protected_start > min).then(|| protected_start.saturating_sub(1));
        let right = (protected_end < max).then(|| protected_end + 1);
        match (left, right) {
            (Some(left), Some(right)) => {
                let left_distance = x.abs_diff(left);
                let right_distance = x.abs_diff(right);
                if left_distance < right_distance {
                    left
                } else if right_distance < left_distance {
                    right
                } else if x <= (protected_start + protected_end) / 2 {
                    left
                } else {
                    right
                }
            }
            (Some(left), None) => left,
            (None, Some(right)) => right,
            (None, None) => x,
        }
    } else if protected_end < max {
        protected_end + 1
    } else if protected_start > min {
        protected_start.saturating_sub(1)
    } else {
        x
    };

    // A vertical cross-subgraph route can approach a target node one column
    // outside the literal title text. Keeping that already-safe column avoids
    // creating two adjacent one-cell elbows where the portal rejoins the node
    // centerline. Preserve the wider margin when the desired column is inside
    // the title text; otherwise both policies retain a safe one-cell alignment.
    if matches!(direction, Direction::TD | Direction::TB)
        && x.abs_diff(selected) == 1
        && (x < start || x > end)
    {
        x
    } else {
        selected
    }
}

/// Select a BT target portal while preserving a quiet turn from both the
/// source stem and receiving arrow. The exact flat one-entry BT scene may
/// additionally allow the source-centered lane after layout has staged the
/// source there; all other callers retain the original fail-closed selector.
#[allow(clippy::too_many_arguments)]
pub(crate) fn bt_target_portal_x_avoiding_single_cell_turn_with_source_center(
    left_x: usize,
    width: usize,
    title: Option<&str>,
    desired: usize,
    source_x: usize,
    source_node_bounds: Option<(usize, usize)>,
    arrow_x: usize,
    title_margin: usize,
    allow_source_center: bool,
) -> usize {
    let initial = title_safe_portal_x(
        left_x,
        width,
        title,
        desired,
        Direction::BT,
        title_margin,
        PortalColumnPreference::Directional,
    );
    let initial_is_quiet = initial.abs_diff(source_x) >= 3 && initial.abs_diff(arrow_x) >= 3;
    if initial_is_quiet {
        return initial;
    }

    let min = left_x.saturating_add(1);
    let max = left_x + width.saturating_sub(2);
    let mut candidates = Vec::new();
    for candidate in [
        initial.saturating_sub(1),
        initial.saturating_add(1),
        initial.saturating_sub(2),
        initial.saturating_add(2),
        initial.saturating_sub(3),
        initial.saturating_add(3),
        initial.saturating_sub(5),
        initial.saturating_add(5),
        arrow_x.saturating_sub(3),
        arrow_x.saturating_add(3),
        arrow_x.saturating_sub(5),
        arrow_x.saturating_add(5),
        source_x.saturating_sub(3),
        source_x.saturating_add(3),
        source_x.saturating_sub(5),
        source_x.saturating_add(5),
    ] {
        if candidate < min
            || candidate > max
            || candidate.abs_diff(source_x) < 3
            || candidate.abs_diff(arrow_x) < 3
            || source_node_bounds.is_some_and(|(left, right)| {
                if candidate == source_x {
                    !allow_source_center
                } else if candidate > source_x {
                    candidate <= right.saturating_add(2)
                } else if candidate < source_x {
                    candidate.saturating_add(2) >= left
                } else {
                    true
                }
            })
            || title_safe_portal_x(
                left_x,
                width,
                title,
                candidate,
                Direction::BT,
                title_margin,
                PortalColumnPreference::Directional,
            ) != candidate
        {
            continue;
        }
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }

    if allow_source_center
        && source_x >= min
        && source_x <= max
        && source_x.abs_diff(arrow_x) >= 3
        && title_safe_portal_x(
            left_x,
            width,
            title,
            source_x,
            Direction::BT,
            title_margin,
            PortalColumnPreference::Directional,
        ) == source_x
        && !candidates.contains(&source_x)
    {
        candidates.push(source_x);
    }

    candidates
        .into_iter()
        .min_by_key(|candidate| (candidate.abs_diff(initial), candidate.abs_diff(arrow_x)))
        .unwrap_or(initial)
}

/// Return the nearest title-safe lane that can be owned by the source stem
/// while retaining a quiet three-cell separation from the receiving arrow.
/// Layout uses this only for a flat, single external BT entry; the route
/// selector receives the same topology permission so both stages agree.
pub(crate) fn bt_external_entry_source_center_lane(
    left_x: usize,
    width: usize,
    title: Option<&str>,
    desired: usize,
    source_center: usize,
    arrow_x: usize,
    title_margin: usize,
) -> Option<usize> {
    let min = left_x.saturating_add(1);
    let max = left_x + width.saturating_sub(2);
    (min..=max)
        .filter(|candidate| {
            candidate.abs_diff(arrow_x) >= 3
                && title_safe_portal_x(
                    left_x,
                    width,
                    title,
                    *candidate,
                    Direction::BT,
                    title_margin,
                    PortalColumnPreference::Directional,
                ) == *candidate
        })
        .min_by_key(|candidate| {
            (
                candidate.abs_diff(desired),
                candidate.abs_diff(source_center),
                *candidate,
            )
        })
}

/// Identify the only BT topology allowed to make the source stem itself the
/// receiving portal lane. Shared fan-in/fan-out, sibling, nested, labeled, and
/// parallel entries retain the conservative quiet-turn selector.
pub(crate) fn bt_single_external_entry_source_center_allowed(
    graph: &Graph,
    from_id: &str,
    to_id: &str,
    subgraph_id: &str,
) -> bool {
    if graph.direction != Direction::BT
        || graph.subgraphs.len() != 1
        || graph.get_node_subgraph(from_id).is_some()
        || graph.get_node_subgraph(to_id) != Some(subgraph_id)
    {
        return false;
    }
    let Some(subgraph) = graph.get_subgraph(subgraph_id) else {
        return false;
    };
    if subgraph.parent_id.is_some() || !subgraph.child_ids.is_empty() || !subgraph.has_title() {
        return false;
    }

    let direct_entries: Vec<_> = graph
        .edges
        .iter()
        .filter(|edge| {
            !edge.is_back_edge
                && edge.kind == EdgeKind::Arrow
                && edge.label.is_none()
                && graph.get_node_subgraph(&edge.from).is_none()
                && graph.get_node_subgraph(&edge.to) == Some(subgraph_id)
                && graph
                    .edge_boundary_crossings(&edge.from, &edge.to)
                    .0
                    .is_empty()
                && graph.edge_boundary_crossings(&edge.from, &edge.to).1 == vec![subgraph_id]
        })
        .collect();
    if direct_entries.len() != 1 {
        return false;
    }
    let edge = direct_entries[0];
    edge.from == from_id && edge.to == to_id
}

/// Identify the narrow TD/TB topology allowed to use the visible-title
/// gutter as its portal margin: one flat titled subgraph with exactly one
/// unlabeled external entry. A single source/target shaft can then stay on the
/// target's legal entry column without painting a title-adjacent elbow; all
/// shared, nested, fan-in, fan-out, and labeled routes retain the wider
/// title-clearance policy.
pub(crate) fn td_single_external_entry_uses_literal_gutter_lane(
    graph: &Graph,
    from_id: &str,
    to_id: &str,
    subgraph_id: &str,
) -> bool {
    if !matches!(graph.direction, Direction::TD | Direction::TB)
        || graph.get_node_subgraph(from_id).is_some()
        || graph.get_node_subgraph(to_id) != Some(subgraph_id)
    {
        return false;
    }
    let Some(subgraph) = graph.get_subgraph(subgraph_id) else {
        return false;
    };
    if subgraph.parent_id.is_some() || !subgraph.child_ids.is_empty() || !subgraph.has_title() {
        return false;
    }

    let direct_entries: Vec<_> = graph
        .edges
        .iter()
        .filter(|edge| {
            !edge.is_back_edge
                && edge.kind == EdgeKind::Arrow
                && edge.label.is_none()
                && graph.get_node_subgraph(&edge.from).is_none()
                && graph.get_node_subgraph(&edge.to) == Some(subgraph_id)
                && graph
                    .edge_boundary_crossings(&edge.from, &edge.to)
                    .0
                    .is_empty()
                && graph.edge_boundary_crossings(&edge.from, &edge.to).1 == vec![subgraph_id]
        })
        .collect();
    direct_entries.len() == 1 && direct_entries[0].from == from_id && direct_entries[0].to == to_id
}

/// Preserve a wider BT title gutter for sibling crossings whose shared target
/// title would otherwise read as a border seam. Exact-two crossings retain the
/// established two-cell policy because their two lanes can otherwise read as a
/// single widened route. Three-or-more aligned crossings have enough visual
/// separation to keep each rail on its literal title-safe column; applying the
/// two-cell margin to the first rail would create a needless boundary hook.
/// A proven strict sibling chain receives a separate one-cell quiet-title
/// margin. This is topology-derived and independent of fixture names.
pub(crate) fn bt_title_margin_for_edge(
    graph: &Graph,
    from_id: &str,
    to_id: &str,
    boundary_id: &str,
) -> usize {
    let (exit_subgraphs, enter_subgraphs) = graph.edge_boundary_crossings(from_id, to_id);
    if exit_subgraphs.len() != 1 || enter_subgraphs.len() != 1 || enter_subgraphs[0] != boundary_id
    {
        return 0;
    }
    let source_id = exit_subgraphs[0];
    let target_id = enter_subgraphs[0];
    let parallel_edges = graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge)
        .filter(|edge| {
            let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
            exits.len() == 1 && enters.len() == 1 && exits[0] == source_id && enters[0] == target_id
        })
        .count();
    if parallel_edges == 1 {
        let bounds: HashMap<String, Rect> = graph
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
            .collect();
        if bt_sibling_chain_target_ids(graph, &bounds)
            .is_some_and(|target_ids| target_ids.contains(boundary_id))
        {
            return BT_SIBLING_CHAIN_TITLE_MARGIN;
        }
    }
    match parallel_edges {
        0 | 1 => 0,
        2 if graph.bt_sibling_target_entry_scene().is_some() => 0,
        2 => 2,
        _ => 0,
    }
}

const TD_SIBLING_LANE_OFFSET: usize = 2;
const TD_SIBLING_EXTRA_TITLE_PADDING: usize = 1;
/// Keep a proven BT sibling-chain rail one quiet cell away from its target title.
/// Generic BT portals and exact-two parallel-edge policies retain their
/// established behavior.
pub(crate) const BT_SIBLING_CHAIN_TITLE_MARGIN: usize = 1;

/// Minimum inter-border gap required by the strict BT sibling route's
/// two-cell target/source offsets to leave one usable corridor row.
pub(crate) const BT_SIBLING_CHAIN_MIN_CORRIDOR_GAP: usize = 3;

/// Shared policy for the strict three-edge BT parallel-sibling scene. Layout
/// and route lowering must agree on this margin or a title-safe boundary lane
/// can diverge from the lane occupied by its node pair.
pub(crate) const BT_PARALLEL_TITLE_MARGIN: usize = 1;
pub(crate) const BT_PARALLEL_MIN_LANE_GAP: usize = 4;

/// At the final fixed-envelope transaction, prefer moving the first paired
/// rail one cell away from the visible BT title when that lane is already
/// title-safe and keeps the complete three-rail assignment legal. This is a
/// scene-local preference, not a generic title-margin change.
pub(crate) const BT_PARALLEL_FIRST_RAIL_SHIFT: usize = 1;

/// Keep the receiver portal outside the complete BT title token while the
/// layout owner aligns the receiver node to that same lane.
pub(crate) const BT_EXTERNAL_SIDE_RECEIVER_TITLE_MARGIN: usize = 2;

/// Keep a strict horizontal sibling-chain bridge long enough to read as a
/// deliberate cross-group transition after its quiet-corridor turn.  A
/// shorter gap makes the two corner cells look like a tiny second box.
pub(crate) const HORIZONTAL_SIBLING_CHAIN_MIN_INTER_GAP: usize = 16;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TitleGutter {
    pub leading_extra_padding: usize,
    pub trailing_extra_padding: usize,
}

fn td_sibling_pair_is_eligible(graph: &Graph, source_id: &str, target_id: &str) -> bool {
    let Some(source) = graph.get_subgraph(source_id) else {
        return false;
    };
    let Some(target) = graph.get_subgraph(target_id) else {
        return false;
    };
    source.parent_id.as_deref() == target.parent_id.as_deref()
        && source.bounds.width > 0
        && source.bounds.height > 0
        && target.bounds.width > 0
        && target.bounds.height > 0
        && source.bounds.y < target.bounds.y
        && source.bounds.y.saturating_add(source.bounds.height) <= target.bounds.y
        && td_sibling_edge_is_unique(graph, source_id, target_id)
}

#[derive(Debug, Clone)]
struct TdSiblingCrossing {
    from: String,
    to: String,
    source_id: String,
    target_id: String,
    source_y: usize,
    target_y: usize,
}

fn td_sibling_eligible_crossings(graph: &Graph) -> Vec<TdSiblingCrossing> {
    let mut eligible = graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge)
        .filter_map(|edge| {
            let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
            if exits.len() != 1
                || enters.len() != 1
                || !td_sibling_pair_is_eligible(graph, exits[0], enters[0])
            {
                return None;
            }
            let source = graph.get_subgraph(exits[0])?;
            let target = graph.get_subgraph(enters[0])?;
            Some(TdSiblingCrossing {
                from: edge.from.clone(),
                to: edge.to.clone(),
                source_id: exits[0].to_owned(),
                target_id: enters[0].to_owned(),
                source_y: source.bounds.y,
                target_y: target.bounds.y,
            })
        })
        .collect::<Vec<_>>();

    eligible.sort_by(|left, right| {
        (
            left.target_y,
            left.source_y,
            &left.target_id,
            &left.source_id,
            &left.from,
            &left.to,
        )
            .cmp(&(
                right.target_y,
                right.source_y,
                &right.target_id,
                &right.source_id,
                &right.from,
                &right.to,
            ))
    });
    eligible
}

/// Return the topology-owned title gutter for an eligible target.
///
/// A linear chain alternates leading and trailing gutters with its ordered
/// crossings so repeated transitions do not reuse one apparent trunk. Shared
/// targets retain the first/next lane policy: the first ordered sibling lane
/// receives leading padding and the next lane receives trailing padding. This
/// avoids spending the narrow target envelope's last interior cell on the
/// opposite side of the selected route. A target with multiple eligible
/// entries gets symmetric padding only when that token fits.
pub(crate) fn td_sibling_title_gutter(graph: &Graph, target_id: &str) -> TitleGutter {
    if !matches!(graph.direction, Direction::TD | Direction::TB) {
        return TitleGutter::default();
    }
    let Some(target) = graph.get_subgraph(target_id) else {
        return TitleGutter::default();
    };
    let Some(title) = target.title.as_deref() else {
        return TitleGutter::default();
    };

    let eligible = td_sibling_eligible_crossings(graph);
    if eligible.len() < 2 {
        return TitleGutter::default();
    }
    if !eligible
        .iter()
        .any(|crossing| crossing.target_id == target_id)
    {
        return TitleGutter::default();
    }
    if td_sibling_crossings_form_linear_chain(&eligible) {
        let ordinal = eligible
            .iter()
            .position(|crossing| crossing.target_id == target_id)
            .unwrap_or(0);
        let gutter = if ordinal % 2 == 0 {
            TitleGutter {
                leading_extra_padding: TD_SIBLING_EXTRA_TITLE_PADDING,
                trailing_extra_padding: 0,
            }
        } else {
            TitleGutter {
                leading_extra_padding: 0,
                trailing_extra_padding: TD_SIBLING_EXTRA_TITLE_PADDING + 1,
            }
        };
        return if crate::graph::subgraph_title_span_with_padding_sides(
            target.bounds.x,
            target.bounds.width,
            title,
            graph.direction,
            gutter.leading_extra_padding,
            gutter.trailing_extra_padding,
        )
        .is_some()
        {
            gutter
        } else {
            TitleGutter::default()
        };
    }
    let Some(ordinal) = eligible
        .iter()
        .position(|crossing| crossing.target_id == target_id)
    else {
        return TitleGutter::default();
    };
    let entries_for_target = eligible
        .iter()
        .filter(|crossing| crossing.target_id == target_id)
        .count();
    let gutter = if entries_for_target > 1 {
        TitleGutter {
            leading_extra_padding: TD_SIBLING_EXTRA_TITLE_PADDING,
            trailing_extra_padding: TD_SIBLING_EXTRA_TITLE_PADDING,
        }
    } else if ordinal % 2 == 0 {
        TitleGutter {
            leading_extra_padding: TD_SIBLING_EXTRA_TITLE_PADDING,
            trailing_extra_padding: 0,
        }
    } else {
        TitleGutter {
            leading_extra_padding: 0,
            trailing_extra_padding: TD_SIBLING_EXTRA_TITLE_PADDING,
        }
    };

    if crate::graph::subgraph_title_span_with_padding_sides(
        target.bounds.x,
        target.bounds.width,
        title,
        graph.direction,
        gutter.leading_extra_padding,
        gutter.trailing_extra_padding,
    )
    .is_some()
    {
        gutter
    } else {
        TitleGutter::default()
    }
}

fn td_sibling_crossings_form_linear_chain(eligible: &[TdSiblingCrossing]) -> bool {
    if eligible.len() < 2 {
        return false;
    }

    let mut degrees: HashMap<&str, (usize, usize)> = HashMap::new();
    for crossing in eligible {
        let source = degrees.entry(crossing.source_id.as_str()).or_default();
        source.0 += 1;
        let target = degrees.entry(crossing.target_id.as_str()).or_default();
        target.1 += 1;
    }

    let starts = degrees
        .values()
        .filter(|(outgoing, incoming)| *outgoing == 1 && *incoming == 0)
        .count();
    let ends = degrees
        .values()
        .filter(|(outgoing, incoming)| *outgoing == 0 && *incoming == 1)
        .count();
    degrees
        .values()
        .all(|(outgoing, incoming)| *outgoing <= 1 && *incoming <= 1)
        && degrees.len() == eligible.len() + 1
        && starts == 1
        && ends == 1
}

/// Return a topology-owned target lane for a direct, vertically stacked TD/TB
/// sibling crossing.
///
/// A repeated center column through several sibling borders reads as one
/// fused route even when every individual border opening is technically valid.
/// Only a small, unambiguous class receives the lane policy: unique direct
/// boundary pairs, siblings with the same parent, and at least two eligible
/// crossings in the same vertical family. Parallel edges and shared
/// fan-in/fan-out remain on their existing coordinated slots.
pub(crate) fn td_sibling_portal_x(
    graph: &Graph,
    from_id: &str,
    to_id: &str,
    desired_x: usize,
    direction: Direction,
) -> Option<usize> {
    let source_id = graph.get_node_subgraph(from_id)?;
    let target_id = graph.get_node_subgraph(to_id)?;
    let source = graph.get_subgraph(source_id)?;
    let target = graph.get_subgraph(target_id)?;
    let result = td_sibling_portal_x_with_bounds(
        graph,
        from_id,
        to_id,
        desired_x,
        direction,
        Rect::new(
            source.bounds.x,
            source.bounds.y,
            source.bounds.width,
            source.bounds.height,
        ),
        Rect::new(
            target.bounds.x,
            target.bounds.y,
            target.bounds.width,
            target.bounds.height,
        ),
        graph.get_node(from_id)?.center_x(),
    );
    result
}

/// Select a shared TD/TB sibling lane for the small mixed fan-out where two
/// distinct source nodes cross the same pair of titled subgraphs. The normal
/// sibling policy intentionally rejects that non-unique boundary pair; this
/// narrower policy only intervenes when its selected title-safe lane is one
/// cell from the active source stem, which would compose into adjacent
/// corners beside the other crossing's vertical shaft.
#[allow(clippy::too_many_arguments)]
pub(crate) fn td_mixed_sibling_clearance_lane(
    graph: &Graph,
    from_id: &str,
    to_id: &str,
    desired_x: usize,
    direction: Direction,
    source_bounds: Rect,
    target_bounds: Rect,
    source_lane: usize,
) -> Option<usize> {
    if !matches!(direction, Direction::TD | Direction::TB)
        || source_bounds.width == 0
        || source_bounds.height == 0
        || target_bounds.width == 0
        || target_bounds.height == 0
        || source_bounds.y >= target_bounds.y
        || source_bounds.bottom() > target_bounds.y
    {
        return None;
    }
    let (exit_subgraphs, enter_subgraphs) = graph.edge_boundary_crossings(from_id, to_id);
    if exit_subgraphs.len() != 1 || enter_subgraphs.len() != 1 {
        return None;
    }
    let source_id = exit_subgraphs[0];
    let target_id = enter_subgraphs[0];
    let source = graph.get_subgraph(source_id)?;
    let target = graph.get_subgraph(target_id)?;
    if source.parent_id.as_deref() != target.parent_id.as_deref()
        || target.title.is_none()
        || source_id == target_id
    {
        return None;
    }

    let mut endpoint_pairs = HashSet::new();
    let crossing_count = graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge)
        .filter(|edge| edge.kind == EdgeKind::Arrow && edge.label.is_none())
        .filter(|edge| {
            let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
            exits.len() == 1 && enters.len() == 1 && exits[0] == source_id && enters[0] == target_id
        })
        .inspect(|edge| {
            endpoint_pairs.insert((edge.from.clone(), edge.to.clone()));
        })
        .count();
    if crossing_count != 2 || endpoint_pairs.len() != crossing_count {
        return None;
    }

    let min_x = target_bounds.x.saturating_add(1);
    let max_x = target_bounds
        .x
        .saturating_add(target_bounds.width.saturating_sub(2));
    if min_x > max_x {
        return None;
    }
    let title_margin = title_margin_for_direction(direction);
    let initial = title_safe_portal_x(
        target_bounds.x,
        target_bounds.width,
        target.title.as_deref(),
        desired_x,
        direction,
        title_margin,
        PortalColumnPreference::Directional,
    );
    if initial.abs_diff(source_lane) != 1 {
        return None;
    }

    (min_x..=max_x)
        .filter(|candidate| candidate.abs_diff(source_lane) >= 3)
        .map(|candidate| {
            (
                candidate,
                title_safe_portal_x(
                    target_bounds.x,
                    target_bounds.width,
                    target.title.as_deref(),
                    candidate,
                    direction,
                    title_margin,
                    PortalColumnPreference::Nearest,
                ),
            )
        })
        .filter(|(candidate, resolved)| candidate == resolved)
        .map(|(candidate, _)| candidate)
        .min_by_key(|candidate| {
            (
                candidate.abs_diff(initial),
                candidate.abs_diff(desired_x),
                *candidate,
            )
        })
}

/// Select the one vertical lane shared by a nested TD/TB boundary chain.
///
/// A route entering several declared ancestors must not let each boundary
/// independently title-shift the same stem. That creates two adjacent portal
/// shafts at one border: one belongs to the route and one belongs only to the
/// slot projection. Resolve the chain from outermost to innermost with the
/// same title-safe policy used by the renderer, then reuse the final lane for
/// every crossed boundary.
pub(crate) fn td_nested_boundary_lane(
    graph: &Graph,
    boundary_ids: &[&str],
    desired_x: usize,
) -> Option<usize> {
    td_nested_boundary_lane_with_bounds(graph, boundary_ids, desired_x, None)
}

/// Bounds-aware form of [`td_nested_boundary_lane`] for layout stages that
/// are evaluating a candidate envelope before copying it into `Graph`.
pub(crate) fn td_nested_boundary_lane_with_bounds(
    graph: &Graph,
    boundary_ids: &[&str],
    desired_x: usize,
    current_bounds: Option<&HashMap<String, Rect>>,
) -> Option<usize> {
    if !matches!(graph.direction, Direction::TD | Direction::TB) || boundary_ids.len() < 2 {
        return None;
    }

    let mut lane = desired_x;
    let mut resolved = false;
    for boundary_id in boundary_ids.iter().rev() {
        let Some(subgraph) = graph.get_subgraph(boundary_id) else {
            continue;
        };
        let Some(bounds) = current_subgraph_bounds(graph, current_bounds, boundary_id) else {
            continue;
        };
        if bounds.is_empty() {
            continue;
        }
        lane = title_safe_portal_x(
            bounds.x,
            bounds.width,
            subgraph.title.as_deref(),
            lane,
            graph.direction,
            title_margin_for_direction(graph.direction),
            PortalColumnPreference::Nearest,
        );
        resolved = true;
    }
    resolved.then_some(lane)
}

/// Select one BT lane outside every title span in a nested boundary chain.
/// Independent title shifts at each bottom border create a visible staircase;
/// a common safe lane lets routing and border restoration share one ownership
/// decision for the whole chain.
pub(crate) fn bt_nested_boundary_lane_with_bounds(
    graph: &Graph,
    boundary_ids: &[&str],
    desired_x: usize,
    current_bounds: Option<&HashMap<String, Rect>>,
) -> Option<usize> {
    let candidates =
        bt_nested_boundary_lane_candidates_with_bounds(graph, boundary_ids, current_bounds)?;
    let min_x = candidates.iter().copied().min()?;
    let max_x = candidates.iter().copied().max()?;
    let preferred = desired_x.clamp(min_x, max_x);
    candidates
        .into_iter()
        .min_by_key(|candidate| (candidate.abs_diff(preferred), *candidate))
}

/// Select a common BT lane for a nested entry, while rejecting a one-cell
/// source-to-target turn whenever the chain has a title-safe alternative.
///
/// Nested entries still use one lane through the boundary chain as the
/// conservative ownership control.  The extra spacing rule is shared by
/// layout and rendering so the collector cannot reserve the close lane that
/// the lowerer later turns into a visually dense `+-+`/`└┐` shoulder.
pub(crate) fn bt_nested_boundary_lane_with_quiet_turn(
    graph: &Graph,
    boundary_ids: &[&str],
    desired_x: usize,
    source_x: usize,
    arrow_x: usize,
    current_bounds: Option<&HashMap<String, Rect>>,
) -> Option<usize> {
    let common =
        bt_nested_boundary_lane_with_bounds(graph, boundary_ids, desired_x, current_bounds)?;
    if common.abs_diff(source_x) >= 3 && common.abs_diff(arrow_x) >= 3 {
        return Some(common);
    }

    let candidates =
        bt_nested_boundary_lane_candidates_with_bounds(graph, boundary_ids, current_bounds)?;
    let min_x = candidates.iter().copied().min()?;
    let max_x = candidates.iter().copied().max()?;
    let preferred = desired_x.clamp(min_x, max_x);
    candidates
        .into_iter()
        .filter(|candidate| candidate.abs_diff(source_x) >= 3 && candidate.abs_diff(arrow_x) >= 3)
        .min_by_key(|candidate| {
            (
                candidate.abs_diff(preferred),
                candidate.abs_diff(arrow_x),
                *candidate,
            )
        })
        .or(Some(common))
}

fn bt_nested_boundary_lane_candidates_with_bounds(
    graph: &Graph,
    boundary_ids: &[&str],
    current_bounds: Option<&HashMap<String, Rect>>,
) -> Option<Vec<usize>> {
    if graph.direction != Direction::BT || boundary_ids.len() < 2 {
        return None;
    }

    let mut min_x = 0usize;
    let mut max_x = usize::MAX;
    let mut title_spans = Vec::new();
    for boundary_id in boundary_ids {
        let subgraph = graph.get_subgraph(boundary_id)?;
        let bounds = current_subgraph_bounds(graph, current_bounds, boundary_id)?;
        if bounds.is_empty() {
            return None;
        }
        min_x = min_x.max(bounds.x.saturating_add(1));
        max_x = max_x.min(bounds.x.saturating_add(bounds.width.saturating_sub(2)));
        if let Some(title) = subgraph.title.as_deref() {
            if let Some(span) =
                crate::graph::subgraph_title_span(bounds.x, bounds.width, title, graph.direction)
            {
                title_spans.push(span);
            }
        }
    }
    if min_x > max_x {
        return None;
    }

    let candidates = (min_x..=max_x)
        .filter(|candidate| {
            title_spans
                .iter()
                .all(|(start, end)| candidate < start || candidate > end)
        })
        .collect::<Vec<_>>();
    (!candidates.is_empty()).then_some(candidates)
}

/// Layout-aware form of [`td_sibling_portal_x`] that accepts live envelope
/// bounds while retaining the graph's topology and title metadata.
#[allow(clippy::too_many_arguments)]
pub(crate) fn td_sibling_portal_x_with_bounds(
    graph: &Graph,
    from_id: &str,
    to_id: &str,
    desired_x: usize,
    direction: Direction,
    source_bounds: Rect,
    target_bounds: Rect,
    source_lane: usize,
) -> Option<usize> {
    if !matches!(direction, Direction::TD | Direction::TB)
        || source_bounds.width == 0
        || source_bounds.height == 0
        || target_bounds.width == 0
        || target_bounds.height == 0
        || source_bounds.y >= target_bounds.y
        || source_bounds.bottom() > target_bounds.y
    {
        return None;
    }

    let (exit_subgraphs, enter_subgraphs) = graph.edge_boundary_crossings(from_id, to_id);
    if exit_subgraphs.len() != 1 || enter_subgraphs.len() != 1 {
        return None;
    }
    let source_id = exit_subgraphs[0];
    let target_id = enter_subgraphs[0];
    if source_id == target_id {
        return None;
    }

    let target = graph.get_subgraph(target_id)?;
    if !td_sibling_pair_is_eligible(graph, source_id, target_id) {
        return None;
    }

    let eligible = td_sibling_eligible_crossings(graph);

    if eligible.len() < 2 {
        return None;
    }

    let ordinal = eligible.iter().position(|candidate| {
        candidate.from == from_id
            && candidate.to == to_id
            && candidate.source_id == source_id
            && candidate.target_id == target_id
    })?;

    let is_linear_chain = td_sibling_crossings_form_linear_chain(&eligible);

    let min_x = target_bounds.x.saturating_add(1);
    let max_x = target_bounds
        .x
        .saturating_add(target_bounds.width.saturating_sub(2));
    if min_x > max_x {
        return None;
    }

    let desired_x = desired_x.clamp(min_x, max_x);
    // A linear chain still needs distinct boundary ownership. Reusing one
    // side portal for every crossing creates a single shaft through all
    // sibling frames; alternate the title-safe side so each transition gets
    // a visible turn in its own corridor. Shared targets, fan-in/fan-out, and
    // parallel scenes retain the same alternating policy.
    let offset_lane = if ordinal % 2 == 0 {
        desired_x.saturating_sub(TD_SIBLING_LANE_OFFSET)
    } else {
        desired_x.saturating_add(TD_SIBLING_LANE_OFFSET)
    }
    .clamp(min_x, max_x);
    if offset_lane.abs_diff(desired_x) < TD_SIBLING_LANE_OFFSET {
        return None;
    }

    let gutter = if is_linear_chain {
        if ordinal % 2 == 0 {
            TitleGutter {
                leading_extra_padding: TD_SIBLING_EXTRA_TITLE_PADDING,
                trailing_extra_padding: 0,
            }
        } else {
            TitleGutter {
                leading_extra_padding: 0,
                // The right-hand portal sits after the literal title span;
                // reserve one additional quiet cell so the rail cannot read
                // as a title suffix.
                trailing_extra_padding: TD_SIBLING_EXTRA_TITLE_PADDING + 1,
            }
        }
    } else {
        td_sibling_title_gutter(graph, target_id)
    };
    let title_margin = if gutter.leading_extra_padding > 0 || gutter.trailing_extra_padding > 0 {
        // Keep one quiet cell outside the visible title text while
        // preserving the topology-owned interior lane. A wider margin
        // collapses the left lane onto the border in narrow sibling
        // envelopes and produces a visually worse route.
        1
    } else {
        0
    };
    let mut lane = title_safe_portal_x_with_text_padding_sides(
        target_bounds.x,
        target_bounds.width,
        target.title.as_deref(),
        offset_lane,
        direction,
        gutter.leading_extra_padding,
        gutter.trailing_extra_padding,
        title_margin,
        PortalColumnPreference::Nearest,
    );

    // A mixed fan-out can place one sibling lane immediately beside the
    // source stem while another route already owns the neighboring vertical
    // cell. The two corner glyphs then compose into `++`/`└┐` even though the
    // route is connected. Move only this topology-owned lane to the nearest
    // title-safe alternative with a real shaft cell between the turns. The
    // source lane is supplied by both layout and rendering so portal slots
    // and final lowering share the same proof.
    if lane.abs_diff(source_lane) == 1 {
        let mut candidates = [
            lane.saturating_sub(2),
            lane.saturating_add(2),
            desired_x.saturating_sub(2),
            desired_x.saturating_add(2),
        ]
        .into_iter()
        .filter(|candidate| {
            *candidate >= min_x && *candidate <= max_x && candidate.abs_diff(source_lane) >= 2
        })
        .map(|candidate| {
            title_safe_portal_x_with_text_padding_sides(
                target_bounds.x,
                target_bounds.width,
                target.title.as_deref(),
                candidate,
                direction,
                gutter.leading_extra_padding,
                gutter.trailing_extra_padding,
                title_margin,
                PortalColumnPreference::Nearest,
            )
        })
        .filter(|candidate| {
            *candidate >= min_x && *candidate <= max_x && candidate.abs_diff(source_lane) >= 2
        })
        .collect::<Vec<_>>();
        candidates.sort_by_key(|candidate| {
            (
                candidate.abs_diff(lane),
                candidate.abs_diff(desired_x),
                *candidate,
            )
        });
        if let Some(candidate) = candidates.into_iter().next() {
            lane = candidate;
        }
    }
    (lane.abs_diff(desired_x) >= TD_SIBLING_LANE_OFFSET).then_some(lane)
}

fn td_sibling_edge_is_unique(graph: &Graph, source_id: &str, target_id: &str) -> bool {
    graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge)
        .filter(|edge| {
            let (exits, enters) = graph.edge_boundary_crossings(&edge.from, &edge.to);
            exits.len() == 1 && enters.len() == 1 && exits[0] == source_id && enters[0] == target_id
        })
        .count()
        == 1
}

/// Combined inner/outer bounds with portals.
#[derive(Debug, Clone)]
pub struct SubgraphEnvelope {
    pub outer: Rect,
    pub inner: Rect,
    pub portals: PortalSlots,
}

/// Shared portal slot discovery (used by layout + render).
pub fn collect_portal_slots(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    direction: Direction,
) -> HashMap<String, PortalSlots> {
    collect_portal_slots_with_contract(graph, node_rects, direction, None)
}

/// Collect portal slots and apply the immutable layout-owned BT endpoint
/// contract when one is available. Direct graph callers pass `None` and keep
/// the existing conservative collector; the normal layout path therefore has
/// exactly one authoritative source for strict-chain lanes.
pub(crate) fn collect_portal_slots_with_contract(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    direction: Direction,
    contract: Option<&crate::layout_render_contract::BtSiblingEndpointContract>,
) -> HashMap<String, PortalSlots> {
    let mut slots = collect_portal_slots_with_bounds(graph, node_rects, direction, None);
    if let Some(contract) = contract {
        for transition in &contract.transitions {
            let source_slots = slots
                .entry(transition.source_subgraph_id.clone())
                .or_default();
            source_slots.top.clear();
            source_slots.top.insert(transition.source_lane);

            let target_slots = slots
                .entry(transition.target_subgraph_id.clone())
                .or_default();
            target_slots.bottom.clear();
            target_slots.bottom.insert(transition.target_lane);
        }
    }
    slots
}

/// Return one boundary coordinate per edge for the narrowly scoped subgraph
/// fan-in scene.  The route planner and portal projection both consume this
/// structural proof so a shared median slot cannot be reintroduced by a later
/// stage.
pub(crate) fn strict_simple_subgraph_fanin_lanes(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    target_id: &str,
    subgraph_id: &str,
    direction: Direction,
) -> Option<Vec<usize>> {
    if graph.subgraphs.len() != 1 || graph.edges.len() < 3 {
        return None;
    }
    let target = graph.get_node(target_id)?;
    if target.shape != NodeShape::Rectangle
        || graph.get_node_subgraph(target_id) == Some(subgraph_id)
    {
        return None;
    }
    let source_edges: Vec<_> = graph
        .edges
        .iter()
        .filter(|edge| {
            edge.to == target_id && graph.get_node_subgraph(&edge.from) == Some(subgraph_id)
        })
        .collect();
    if source_edges.len() != graph.edges.len() || source_edges.len() < 3 {
        return None;
    }
    if source_edges.iter().any(|edge| {
        edge.is_back_edge
            || edge.label.is_some()
            || edge.kind != EdgeKind::Arrow
            || graph
                .get_node(&edge.from)
                .is_none_or(|source| source.shape != NodeShape::Rectangle)
    }) {
        return None;
    }

    let mut source_lanes: Vec<(usize, String)> = source_edges
        .iter()
        .filter_map(|edge| graph.get_node(&edge.from))
        .map(|source| {
            (
                if matches!(direction, Direction::LR | Direction::RL) {
                    node_center_y(node_rects, &source.id, source)
                } else {
                    node_center_x(node_rects, &source.id, source)
                },
                source.id.clone(),
            )
        })
        .collect();
    if source_lanes.len() != source_edges.len()
        || source_lanes
            .iter()
            .map(|(_, id)| id.as_str())
            .collect::<HashSet<_>>()
            .len()
            != source_lanes.len()
    {
        return None;
    }
    let subgraph = graph.get_subgraph(subgraph_id)?;
    let (min_lane, max_lane) = if matches!(direction, Direction::LR | Direction::RL) {
        (
            subgraph.bounds.y.saturating_add(1),
            subgraph
                .bounds
                .y
                .saturating_add(subgraph.bounds.height.saturating_sub(2)),
        )
    } else {
        (
            subgraph.bounds.x.saturating_add(1),
            subgraph
                .bounds
                .x
                .saturating_add(subgraph.bounds.width.saturating_sub(2)),
        )
    };
    source_lanes.sort_by_key(|(lane, id)| (*lane, id.clone()));
    if min_lane > max_lane
        || source_lanes
            .iter()
            .any(|(lane, _)| *lane < min_lane || *lane > max_lane)
        || source_lanes
            .windows(2)
            .any(|lanes| lanes[0].0 == lanes[1].0)
    {
        return None;
    }
    Some(source_lanes.into_iter().map(|(lane, _)| lane).collect())
}

/// Return the target center for the narrow direct TD/TB entry scene that may
/// use a literal target-centered portal. The target must be terminal, every
/// direct entry into the same titled subgraph must be one-to-one, and the
/// center must stay outside the literal title token. This policy is shared by
/// portal-slot discovery and the renderer so the two stages cannot disagree
/// about a one-cell hook repair.
pub(crate) fn td_terminal_entry_target_center(
    graph: &Graph,
    from_id: &str,
    to_id: &str,
    subgraph_id: &str,
    target_bounds: Rect,
    direction: Direction,
    target_center: usize,
) -> Option<usize> {
    if !matches!(direction, Direction::TD | Direction::TB)
        || graph.get_node_subgraph(from_id).is_some()
        || graph.get_node_subgraph(to_id) != Some(subgraph_id)
        || !graph.edges.iter().any(|edge| {
            edge.from == from_id
                && edge.to == to_id
                && !edge.is_back_edge
                && edge.kind == EdgeKind::Arrow
                && edge.label.is_none()
        })
    {
        return None;
    }

    let direct_entries: Vec<_> = graph
        .edges
        .iter()
        .filter(|edge| {
            !edge.is_back_edge
                && edge.kind == EdgeKind::Arrow
                && edge.label.is_none()
                && graph.get_node_subgraph(&edge.from).is_none()
                && graph.get_node_subgraph(&edge.to) == Some(subgraph_id)
                && graph
                    .edge_boundary_crossings(&edge.from, &edge.to)
                    .0
                    .is_empty()
                && graph.edge_boundary_crossings(&edge.from, &edge.to).1 == vec![subgraph_id]
        })
        .collect();
    if direct_entries.len() < 2 {
        return None;
    }
    let source_ids: HashSet<&str> = direct_entries
        .iter()
        .map(|edge| edge.from.as_str())
        .collect();
    let target_ids: HashSet<&str> = direct_entries.iter().map(|edge| edge.to.as_str()).collect();
    if source_ids.len() != direct_entries.len() || target_ids.len() != direct_entries.len() {
        return None;
    }
    if direct_entries.iter().any(|edge| {
        graph
            .edges
            .iter()
            .any(|candidate| !candidate.is_back_edge && candidate.from == edge.to)
    }) {
        return None;
    }

    let title_span = graph
        .get_subgraph(subgraph_id)
        .and_then(|subgraph| subgraph.title.as_deref())
        .and_then(|title| {
            crate::graph::subgraph_title_span(
                target_bounds.x,
                target_bounds.width,
                title,
                direction,
            )
        });
    if title_span.is_some_and(|(start, end)| target_center >= start && target_center <= end) {
        return None;
    }
    Some(target_center)
}

/// Return the one flat titled TD/TB subgraph whose complete edge set is a
/// one-to-one set of unlabeled external entries into terminal rectangle nodes.
///
/// This is intentionally stricter than the older target-center helper.  The
/// quiet-band route is a scene transaction, so accepting a partial edge family
/// would let an unrelated edge borrow the same title/receiver corridor and
/// recreate the very ownership ambiguity the transaction is meant to remove.
pub(crate) fn td_terminal_entry_scene_subgraph(graph: &Graph) -> Option<&crate::graph::Subgraph> {
    if !matches!(graph.direction, Direction::TD | Direction::TB)
        || graph.subgraphs.len() != 1
        || graph.edges.iter().any(|edge| edge.is_back_edge)
    {
        return None;
    }

    let subgraph = graph.subgraphs.first()?;
    if subgraph.parent_id.is_some() || !subgraph.child_ids.is_empty() || !subgraph.has_title() {
        return None;
    }

    let entries: Vec<_> = graph
        .edges
        .iter()
        .filter(|edge| {
            edge.kind == EdgeKind::Arrow
                && edge.label.is_none()
                && graph.get_node_subgraph(&edge.from).is_none()
                && graph.get_node_subgraph(&edge.to) == Some(subgraph.id.as_str())
                && subgraph.node_ids.contains(&edge.to)
                && graph
                    .edge_boundary_crossings(&edge.from, &edge.to)
                    .0
                    .is_empty()
                && graph.edge_boundary_crossings(&edge.from, &edge.to).1
                    == vec![subgraph.id.as_str()]
        })
        .collect();
    if entries.len() < 2 || entries.len() != graph.edges.len() {
        return None;
    }

    let source_ids: HashSet<&str> = entries.iter().map(|edge| edge.from.as_str()).collect();
    let target_ids: HashSet<&str> = entries.iter().map(|edge| edge.to.as_str()).collect();
    let pair_ids: HashSet<(&str, &str)> = entries
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect();
    if source_ids.len() != entries.len()
        || target_ids.len() != entries.len()
        || pair_ids.len() != entries.len()
        || target_ids.len() != subgraph.node_ids.len()
        || subgraph
            .node_ids
            .iter()
            .any(|node_id| !target_ids.contains(node_id.as_str()))
        || graph.nodes.iter().any(|node| {
            !source_ids.contains(node.id.as_str()) && !target_ids.contains(node.id.as_str())
        })
    {
        return None;
    }

    if graph
        .nodes
        .iter()
        .any(|node| target_ids.contains(node.id.as_str()) && node.shape != NodeShape::Rectangle)
    {
        return None;
    }

    Some(subgraph)
}

/// Select title-safe interior lanes for a strict terminal-entry scene.
///
/// Each target is assigned independently in target-center order while keeping
/// a readable gap between adjacent portal shafts. The helper is intentionally
/// shared by layout, portal projection, and rendering so the three stages do
/// not invent different receiver lanes.
const TD_TERMINAL_ENTRY_MIN_LANE_GAP: usize = 4;

pub(crate) fn td_terminal_entry_portal_lanes(
    graph: &Graph,
    subgraph_id: &str,
    target_bounds: Rect,
    direction: Direction,
    target_centers: &HashMap<String, usize>,
) -> Option<HashMap<String, usize>> {
    let subgraph = td_terminal_entry_scene_subgraph(graph)?;
    if subgraph.id != subgraph_id || target_bounds.is_empty() {
        return None;
    }

    let min_lane = target_bounds.x.saturating_add(2);
    let max_lane = target_bounds
        .x
        .saturating_add(target_bounds.width.saturating_sub(3));
    if min_lane > max_lane
        || subgraph
            .node_ids
            .iter()
            .any(|node_id| !target_centers.contains_key(node_id))
    {
        return None;
    }

    let mut target_ids: Vec<&str> = subgraph.node_ids.iter().map(String::as_str).collect();
    target_ids.sort_unstable_by_key(|target_id| {
        (
            target_centers
                .get(*target_id)
                .copied()
                .unwrap_or(usize::MAX),
            *target_id,
        )
    });

    let mut lanes = HashMap::new();
    let mut used = Vec::new();
    for target_id in target_ids {
        let target_center = *target_centers.get(target_id)?;
        let mut candidates = (min_lane..=max_lane)
            .filter(|candidate| {
                title_safe_portal_x(
                    target_bounds.x,
                    target_bounds.width,
                    subgraph.title.as_deref(),
                    *candidate,
                    direction,
                    0,
                    PortalColumnPreference::Directional,
                ) == *candidate
            })
            .collect::<Vec<_>>();
        candidates
            .sort_unstable_by_key(|candidate| (candidate.abs_diff(target_center), *candidate));
        let lane = candidates
            .iter()
            .copied()
            .find(|candidate| {
                used.iter()
                    .all(|prior| candidate.abs_diff(*prior) >= TD_TERMINAL_ENTRY_MIN_LANE_GAP)
            })
            .or_else(|| {
                candidates
                    .iter()
                    .copied()
                    .find(|candidate| !used.contains(candidate))
            })?;
        used.push(lane);
        lanes.insert(target_id.to_owned(), lane);
    }
    Some(lanes)
}

fn collect_portal_slots_with_bounds(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    direction: Direction,
    current_bounds: Option<&HashMap<String, Rect>>,
) -> HashMap<String, PortalSlots> {
    let mut slots: HashMap<String, PortalSlots> = HashMap::new();
    let mut shared_td_fanout_top_slots: HashMap<(String, String), usize> = HashMap::new();
    let mut shared_td_fanin_bottom_slots: HashMap<(String, String), Vec<usize>> = HashMap::new();
    let mut shared_horizontal_fanin_side_slots: HashMap<(String, String), Vec<usize>> =
        HashMap::new();

    let strict_td_entry_lanes = if matches!(direction, Direction::TD | Direction::TB) {
        td_terminal_entry_scene_subgraph(graph).and_then(|scene| {
            let bounds = current_subgraph_bounds(graph, current_bounds, &scene.id)?;
            let target_centers = scene
                .node_ids
                .iter()
                .filter_map(|node_id| {
                    let node = graph.get_node(node_id)?;
                    Some((node_id.clone(), node_center_x(node_rects, node_id, node)))
                })
                .collect::<HashMap<_, _>>();
            td_terminal_entry_portal_lanes(graph, &scene.id, bounds, direction, &target_centers)
        })
    } else {
        None
    };

    let shift_x_out_of_title = |sg_id: &str, desired_x: usize, margin: Option<usize>| -> usize {
        let Some(sg) = graph.get_subgraph(sg_id) else {
            return desired_x;
        };
        let Some(bounds) = current_subgraph_bounds(graph, current_bounds, sg_id) else {
            return desired_x;
        };
        let Some(title) = sg.title.as_deref() else {
            return desired_x;
        };
        if bounds.is_empty() {
            return desired_x;
        }
        title_safe_portal_x(
            bounds.x,
            bounds.width,
            Some(title),
            desired_x,
            graph.direction,
            margin.unwrap_or_else(|| title_margin_for_direction(graph.direction)),
            PortalColumnPreference::Directional,
        )
    };

    if matches!(direction, Direction::TD | Direction::TB) {
        let mut grouped_targets: HashMap<(String, String), Vec<usize>> = HashMap::new();
        let mut grouped_sources: HashMap<(String, String), Vec<usize>> = HashMap::new();
        for edge in &graph.edges {
            let Some(from) = graph.get_node(&edge.from) else {
                continue;
            };
            let Some(to) = graph.get_node(&edge.to) else {
                continue;
            };
            let (_, enter_subgraphs) = graph.edge_boundary_crossings(&edge.from, &edge.to);
            for target_sg_id in enter_subgraphs {
                grouped_targets
                    .entry((edge.from.clone(), target_sg_id.to_string()))
                    .or_default()
                    .push(node_center_x(node_rects, &edge.to, to));
            }

            let (exit_subgraphs, _) = graph.edge_boundary_crossings(&edge.from, &edge.to);
            for source_sg_id in exit_subgraphs {
                grouped_sources
                    .entry((edge.to.clone(), source_sg_id.to_string()))
                    .or_default()
                    .push(node_center_x(node_rects, &edge.from, from));
            }
        }

        for ((from_id, sg_id), target_xs) in grouped_targets {
            if target_xs.len() < 2 {
                continue;
            }
            let Some(bounds) = current_subgraph_bounds(graph, current_bounds, &sg_id) else {
                continue;
            };
            if bounds.is_empty() {
                continue;
            }

            let portal_center = bounds.x + bounds.width / 2;
            let min_target_x = target_xs.iter().copied().min().unwrap_or(portal_center);
            let max_target_x = target_xs.iter().copied().max().unwrap_or(portal_center);
            shared_td_fanout_top_slots.insert(
                (from_id, sg_id),
                portal_center.clamp(min_target_x, max_target_x),
            );
        }

        for ((to_id, sg_id), source_xs) in grouped_sources {
            if source_xs.len() < 2 {
                continue;
            }
            let Some(bounds) = current_subgraph_bounds(graph, current_bounds, &sg_id) else {
                continue;
            };
            if bounds.is_empty() {
                continue;
            }
            let Some(target) = graph.get_node(&to_id) else {
                continue;
            };

            let min_source_x = source_xs.iter().copied().min().unwrap_or(bounds.x);
            let max_source_x = source_xs.iter().copied().max().unwrap_or(bounds.x);
            let target_center_x = node_center_x(node_rects, &to_id, target);
            let inset = if bounds.width >= 9 { 1 } else { 0 };
            let min_x = bounds.x.saturating_add(inset);
            let max_x = bounds
                .x
                .saturating_add(bounds.width.saturating_sub(inset + 1));
            let default_shared_x = target_center_x
                .clamp(min_source_x, max_source_x)
                .clamp(min_x, max_x.max(min_x));
            let shared_xs = if let Some(lanes) =
                strict_simple_subgraph_fanin_lanes(graph, node_rects, &to_id, &sg_id, direction)
            {
                if lanes
                    .iter()
                    .all(|candidate| *candidate >= min_x && *candidate <= max_x.max(min_x))
                {
                    lanes
                } else {
                    vec![default_shared_x]
                }
            } else {
                vec![default_shared_x]
            };
            let shared_xs = if shared_xs.len() == 1
                && strict_simple_subgraph_fanin_lanes(graph, node_rects, &to_id, &sg_id, direction)
                    .is_none()
            {
                vec![shift_x_out_of_title(&sg_id, shared_xs[0], None)]
            } else {
                shared_xs
            };
            shared_td_fanin_bottom_slots.insert((to_id, sg_id), shared_xs);
        }
    } else if matches!(direction, Direction::LR | Direction::RL) {
        let mut grouped_sources: HashMap<(String, String), Vec<usize>> = HashMap::new();
        for edge in &graph.edges {
            let Some(from) = graph.get_node(&edge.from) else {
                continue;
            };
            let (exit_subgraphs, _) = graph.edge_boundary_crossings(&edge.from, &edge.to);
            for source_sg_id in exit_subgraphs {
                grouped_sources
                    .entry((edge.to.clone(), source_sg_id.to_string()))
                    .or_default()
                    .push(node_center_y(node_rects, &edge.from, from));
            }
        }

        for ((to_id, sg_id), source_ys) in grouped_sources {
            if source_ys.len() < 2 {
                continue;
            }
            let Some(bounds) = current_subgraph_bounds(graph, current_bounds, &sg_id) else {
                continue;
            };
            if bounds.is_empty() {
                continue;
            }

            let min_source_y = source_ys.iter().copied().min().unwrap_or(bounds.y);
            let max_source_y = source_ys.iter().copied().max().unwrap_or(bounds.y);
            let shared_ys =
                strict_simple_subgraph_fanin_lanes(graph, node_rects, &to_id, &sg_id, direction)
                    .filter(|lanes| {
                        lanes.iter().all(|lane| {
                            *lane >= bounds.y.saturating_add(1)
                                && *lane <= bounds.y + bounds.height.saturating_sub(2)
                        })
                    })
                    .unwrap_or_else(|| {
                        let portal_y = ((min_source_y + max_source_y) / 2).clamp(
                            bounds.y.saturating_add(1),
                            bounds.y + bounds.height.saturating_sub(2),
                        );
                        vec![portal_y]
                    });
            shared_horizontal_fanin_side_slots.insert((to_id, sg_id), shared_ys);
        }
    }

    for edge in &graph.edges {
        let Some(from) = graph.get_node(&edge.from) else {
            continue;
        };
        let Some(to) = graph.get_node(&edge.to) else {
            continue;
        };

        let (exit_subgraphs, enter_subgraphs) = graph.edge_boundary_crossings(&edge.from, &edge.to);
        if exit_subgraphs.is_empty() && enter_subgraphs.is_empty() {
            continue;
        }

        match direction {
            Direction::TD | Direction::TB => {
                let nested_entry_lane = (enter_subgraphs.len() > 1)
                    .then(|| {
                        td_nested_boundary_lane_with_bounds(
                            graph,
                            &enter_subgraphs,
                            node_center_x(node_rects, &edge.to, to),
                            current_bounds,
                        )
                    })
                    .flatten();
                for &id in &enter_subgraphs {
                    let Some(target_bounds) = current_subgraph_bounds(graph, current_bounds, id)
                    else {
                        continue;
                    };

                    let source_x = node_center_x(node_rects, &edge.from, from);
                    let source_exit_y = node_exit_y(node_rects, &edge.from, from);
                    let source_rect = current_node_rect(node_rects, &edge.from, from);
                    let target_top_interior = target_bounds.y.saturating_add(1);
                    let target_bottom_interior = target_bounds
                        .y
                        .saturating_add(target_bounds.height.saturating_sub(2));
                    let visually_nested_parent = graph.subgraphs.iter().any(|candidate| {
                        let Some(candidate_bounds) =
                            current_subgraph_bounds(graph, current_bounds, &candidate.id)
                        else {
                            return false;
                        };
                        if candidate.id == id || candidate_bounds.is_empty() {
                            return false;
                        }
                        let child_right = target_bounds.x + target_bounds.width;
                        let child_bottom = target_bounds.y + target_bounds.height;
                        let source_right = source_rect.right();
                        let source_bottom = source_rect.bottom();
                        target_bounds.x >= candidate_bounds.x
                            && target_bounds.y >= candidate_bounds.y
                            && child_right <= candidate_bounds.x + candidate_bounds.width
                            && child_bottom <= candidate_bounds.y + candidate_bounds.height
                            && source_rect.x >= candidate_bounds.x
                            && source_rect.y >= candidate_bounds.y
                            && source_right <= candidate_bounds.x + candidate_bounds.width
                            && source_bottom <= candidate_bounds.y + candidate_bounds.height
                    });

                    let can_side_enter = visually_nested_parent
                        && !target_bounds.is_empty()
                        && source_exit_y >= target_top_interior
                        && source_exit_y <= target_bottom_interior;

                    if can_side_enter && source_x < target_bounds.x {
                        slots
                            .entry(id.to_string())
                            .or_default()
                            .left
                            .insert(source_exit_y);
                        continue;
                    }
                    if can_side_enter
                        && source_x
                            > target_bounds
                                .x
                                .saturating_add(target_bounds.width.saturating_sub(1))
                    {
                        slots
                            .entry(id.to_string())
                            .or_default()
                            .right
                            .insert(source_exit_y);
                        continue;
                    }

                    let target_center_x = node_center_x(node_rects, &edge.to, to);
                    let x = if let Some(nested_lane) = nested_entry_lane {
                        nested_lane
                    } else if let Some(&shared_x) =
                        shared_td_fanout_top_slots.get(&(edge.from.clone(), id.to_string()))
                    {
                        shared_x
                    } else if let Some(strict_lane) = strict_td_entry_lanes
                        .as_ref()
                        .and_then(|lanes| lanes.get(&edge.to).copied())
                    {
                        strict_lane
                    } else if let Some(target_center) = td_terminal_entry_target_center(
                        graph,
                        &edge.from,
                        &edge.to,
                        id,
                        target_bounds,
                        direction,
                        target_center_x,
                    ) {
                        target_center
                    } else {
                        let sibling_lane = exit_subgraphs.first().and_then(|source_id| {
                            let source_bounds =
                                current_subgraph_bounds(graph, current_bounds, source_id)?;
                            let source_lane = node_center_x(node_rects, &edge.from, from);
                            td_mixed_sibling_clearance_lane(
                                graph,
                                &edge.from,
                                &edge.to,
                                target_center_x,
                                direction,
                                source_bounds,
                                target_bounds,
                                source_lane,
                            )
                            .or_else(|| {
                                td_sibling_portal_x_with_bounds(
                                    graph,
                                    &edge.from,
                                    &edge.to,
                                    target_center_x,
                                    direction,
                                    source_bounds,
                                    target_bounds,
                                    source_lane,
                                )
                            })
                        });
                        sibling_lane.unwrap_or_else(|| {
                            let margin = td_single_external_entry_uses_literal_gutter_lane(
                                graph, &edge.from, &edge.to, id,
                            )
                            .then_some(0);
                            shift_x_out_of_title(id, target_center_x, margin)
                        })
                    };
                    slots.entry(id.to_string()).or_default().top.insert(x);
                }
                for id in &exit_subgraphs {
                    let Some(exit_bounds) = current_subgraph_bounds(graph, current_bounds, id)
                    else {
                        continue;
                    };
                    let source_rect = current_node_rect(node_rects, &edge.from, from);
                    let suppress_exit = enter_subgraphs.iter().any(|target_id| {
                        let Some(target_bounds) =
                            current_subgraph_bounds(graph, current_bounds, target_id)
                        else {
                            return false;
                        };
                        let target_right = target_bounds.x + target_bounds.width;
                        let target_bottom = target_bounds.y + target_bounds.height;
                        let source_right = source_rect.right();
                        let source_bottom = source_rect.bottom();
                        !target_bounds.is_empty()
                            && !exit_bounds.is_empty()
                            && target_bounds.x >= exit_bounds.x
                            && target_bounds.y >= exit_bounds.y
                            && target_right <= exit_bounds.x + exit_bounds.width
                            && target_bottom <= exit_bounds.y + exit_bounds.height
                            && source_rect.x >= exit_bounds.x
                            && source_rect.y >= exit_bounds.y
                            && source_right <= exit_bounds.x + exit_bounds.width
                            && source_bottom <= exit_bounds.y + exit_bounds.height
                    });
                    if suppress_exit {
                        continue;
                    }
                    let slot_xs = if exit_subgraphs.len() > 1 {
                        td_nested_boundary_lane_with_bounds(
                            graph,
                            &exit_subgraphs,
                            node_center_x(node_rects, &edge.from, from),
                            current_bounds,
                        )
                        .map(|lane| vec![lane])
                        .unwrap_or_else(|| vec![node_center_x(node_rects, &edge.from, from)])
                    } else {
                        shared_td_fanin_bottom_slots
                            .get(&(edge.to.clone(), id.to_string()))
                            .cloned()
                            .unwrap_or_else(|| vec![node_center_x(node_rects, &edge.from, from)])
                    };
                    slots
                        .entry(id.to_string())
                        .or_default()
                        .bottom
                        .extend(slot_xs);
                }
            }
            Direction::BT => {
                let nested_entry_lane = (enter_subgraphs.len() > 1)
                    .then(|| {
                        bt_nested_boundary_lane_with_quiet_turn(
                            graph,
                            &enter_subgraphs,
                            node_center_x(node_rects, &edge.to, to),
                            node_center_x(node_rects, &edge.from, from),
                            node_center_x(node_rects, &edge.to, to),
                            current_bounds,
                        )
                    })
                    .flatten();
                for id in enter_subgraphs {
                    let x = nested_entry_lane.unwrap_or_else(|| {
                        let mut lane = node_center_x(node_rects, &edge.to, to);
                        let title_margin = (graph.direction == Direction::BT)
                            .then(|| bt_title_margin_for_edge(graph, &edge.from, &edge.to, id));
                        lane = shift_x_out_of_title(id, lane, title_margin);
                        let Some(bounds) = current_subgraph_bounds(graph, current_bounds, id)
                        else {
                            return lane;
                        };
                        let title = graph
                            .get_subgraph(id)
                            .and_then(|subgraph| subgraph.title.as_deref());
                        let allow_source_center = bt_single_external_entry_source_center_allowed(
                            graph, &edge.from, &edge.to, id,
                        );
                        lane = bt_target_portal_x_avoiding_single_cell_turn_with_source_center(
                            bounds.x,
                            bounds.width,
                            title,
                            lane,
                            node_center_x(node_rects, &edge.from, from),
                            graph.get_node_subgraph(&edge.from).is_none().then(|| {
                                let source = current_node_rect(node_rects, &edge.from, from);
                                (source.x, source.right().saturating_sub(1))
                            }),
                            node_center_x(node_rects, &edge.to, to),
                            title_margin.unwrap_or(0),
                            allow_source_center,
                        );
                        nudge_portal_x_from_corners(
                            bounds.x,
                            bounds.width,
                            title,
                            graph.direction,
                            lane,
                        )
                    });
                    slots.entry(id.to_string()).or_default().bottom.insert(x);
                }
                for id in exit_subgraphs {
                    // BT titles are on the bottom interior row; exits cross the
                    // top border, so the source lane is already title-safe and
                    // should not be shifted into a one-cell elbow.
                    if let Some(lanes) = strict_simple_subgraph_fanin_lanes(
                        graph,
                        node_rects,
                        &edge.to,
                        id,
                        Direction::BT,
                    ) {
                        slots.entry(id.to_string()).or_default().top.extend(lanes);
                    } else {
                        slots
                            .entry(id.to_string())
                            .or_default()
                            .top
                            .insert(node_center_x(node_rects, &edge.from, from));
                    }
                }
            }
            Direction::LR => {
                for id in enter_subgraphs {
                    slots
                        .entry(id.to_string())
                        .or_default()
                        .left
                        .insert(node_center_y(node_rects, &edge.to, to));
                }
                for id in exit_subgraphs {
                    let slot_ys = shared_horizontal_fanin_side_slots
                        .get(&(edge.to.clone(), id.to_string()))
                        .cloned()
                        .unwrap_or_else(|| vec![node_center_y(node_rects, &edge.from, from)]);
                    slots
                        .entry(id.to_string())
                        .or_default()
                        .right
                        .extend(slot_ys);
                }
            }
            Direction::RL => {
                for id in enter_subgraphs {
                    slots
                        .entry(id.to_string())
                        .or_default()
                        .right
                        .insert(node_center_y(node_rects, &edge.to, to));
                }
                for id in exit_subgraphs {
                    let slot_ys = shared_horizontal_fanin_side_slots
                        .get(&(edge.to.clone(), id.to_string()))
                        .cloned()
                        .unwrap_or_else(|| vec![node_center_y(node_rects, &edge.from, from)]);
                    slots
                        .entry(id.to_string())
                        .or_default()
                        .left
                        .extend(slot_ys);
                }
            }
        }
    }

    ensure_td_mixed_sibling_target_portals(graph, node_rects, current_bounds, &mut slots);

    slots
}

/// Reserve two distinct, title-safe top lanes for the exact TD mixed sibling
/// target scene. Generic collection sees two target crossings but both title
/// policies can converge on the same safe column; the scene lowerer then has
/// no way to keep the internal and cross-subgraph arrivals distinct.
///
/// This runs in the shared collector consumed by both layout and rendering.
/// The topology selector is exact and the pair is chosen from live node and
/// envelope coordinates, so unrelated fan-in, nested, labelled, and crowded
/// scenes retain the generic policy.
fn ensure_td_mixed_sibling_target_portals(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    current_bounds: Option<&HashMap<String, Rect>>,
    slots: &mut HashMap<String, PortalSlots>,
) {
    if graph.direction != Direction::TD || graph.subgraphs.len() != 2 {
        return;
    }

    let mut ordered = graph
        .subgraphs
        .iter()
        .filter_map(|subgraph| {
            current_subgraph_bounds(graph, current_bounds, &subgraph.id)
                .map(|bounds| (subgraph, bounds))
        })
        .collect::<Vec<_>>();
    if ordered.len() != 2 {
        return;
    }
    ordered.sort_by_key(|(_, bounds)| (bounds.y, bounds.x));
    let source_id = ordered[0].0.id.as_str();
    let target_id = ordered[1].0.id.as_str();
    let Some(scene) = crate::render::sibling_target_entry_identity::td_scene_for_layout(
        graph, source_id, target_id,
    ) else {
        return;
    };
    let Some(target_bounds) = current_subgraph_bounds(graph, current_bounds, target_id) else {
        return;
    };
    let Some(target_subgraph) = graph.get_subgraph(target_id) else {
        return;
    };
    let Some(target_start) = graph.get_node(&scene.target_start_node_id) else {
        return;
    };
    let Some(target_end) = graph.get_node(&scene.target_end_node_id) else {
        return;
    };

    let min_x = target_bounds.x.saturating_add(1);
    let max_x = target_bounds.right().saturating_sub(2);
    if min_x > max_x {
        return;
    }

    // One quiet cell after the title is sufficient for the paired scene and
    // leaves room for a second lane. A wider generic margin is what caused
    // the two otherwise-valid lanes to collapse onto one column.
    const MIXED_TARGET_TITLE_MARGIN: usize = 1;
    let safe_columns = (min_x..=max_x)
        .filter(|x| {
            title_safe_portal_x(
                target_bounds.x,
                target_bounds.width,
                target_subgraph.title.as_deref(),
                *x,
                Direction::TD,
                MIXED_TARGET_TITLE_MARGIN,
                PortalColumnPreference::Nearest,
            ) == *x
        })
        .collect::<Vec<_>>();
    if safe_columns.len() < 2 {
        return;
    }

    let start_x = node_center_x(node_rects, &target_start.id, target_start);
    let end_x = node_center_x(node_rects, &target_end.id, target_end);
    let nearest = |left: usize, right: usize, desired: usize| {
        if left.abs_diff(desired) <= right.abs_diff(desired) {
            left
        } else {
            right
        }
    };

    let mut selected = None;
    for minimum_gap in [2, 1] {
        for (left_index, left) in safe_columns.iter().enumerate() {
            for right in safe_columns.iter().skip(left_index + 1) {
                if right.saturating_sub(*left) < minimum_gap {
                    continue;
                }
                let start_lane = nearest(*left, *right, start_x);
                let end_lane = nearest(*left, *right, end_x);
                if start_lane == end_lane {
                    continue;
                }
                let score = (
                    start_lane.abs_diff(start_x) + end_lane.abs_diff(end_x),
                    right.saturating_sub(*left),
                    *left,
                    *right,
                );
                if selected
                    .as_ref()
                    .is_none_or(|(best_score, _)| score < *best_score)
                {
                    selected = Some((score, (*left, *right)));
                }
            }
        }
        if selected.is_some() {
            break;
        }
    }

    let Some((_, (left, right))) = selected else {
        return;
    };
    let target_slots = slots.entry(target_id.to_owned()).or_default();
    target_slots.top.clear();
    target_slots.top.insert(left);
    target_slots.top.insert(right);
}

fn node_center_x(
    rects: &HashMap<String, Rect>,
    node_id: &str,
    fallback_node: &crate::graph::Node,
) -> usize {
    rects
        .get(node_id)
        .map(|r| r.x + r.width / 2)
        .unwrap_or_else(|| fallback_node.center_x())
}

fn node_center_y(
    rects: &HashMap<String, Rect>,
    node_id: &str,
    fallback_node: &crate::graph::Node,
) -> usize {
    rects
        .get(node_id)
        .map(|r| r.y + r.height / 2)
        .unwrap_or_else(|| fallback_node.center_y())
}

fn node_exit_y(
    rects: &HashMap<String, Rect>,
    node_id: &str,
    fallback_node: &crate::graph::Node,
) -> usize {
    rects
        .get(node_id)
        .map(|r| r.y + r.height)
        .unwrap_or_else(|| fallback_node.bottom_y())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Direction, Edge, Node, Subgraph};

    #[test]
    fn title_safe_portal_policy_preserves_the_existing_margin_behavior() {
        let expected = 18;

        assert_eq!(
            title_safe_portal_x(
                10,
                16,
                Some("S2"),
                15,
                Direction::BT,
                2,
                PortalColumnPreference::Directional,
            ),
            expected
        );
        assert_eq!(
            title_safe_portal_x(
                10,
                16,
                Some("S2"),
                15,
                Direction::TD,
                2,
                PortalColumnPreference::Directional,
            ),
            expected
        );
        assert_eq!(
            title_safe_portal_x(
                10,
                16,
                Some("S2"),
                15,
                Direction::BT,
                2,
                PortalColumnPreference::Nearest,
            ),
            expected
        );
    }

    #[test]
    fn td_directional_portal_keeps_a_safe_one_cell_target_alignment() {
        assert_eq!(
            title_safe_portal_x(
                0,
                13,
                Some("X"),
                6,
                Direction::TD,
                2,
                PortalColumnPreference::Directional,
            ),
            6
        );
        assert_eq!(
            title_safe_portal_x(
                0,
                13,
                Some("X"),
                3,
                Direction::TD,
                2,
                PortalColumnPreference::Directional,
            ),
            7
        );
        assert_eq!(
            title_safe_portal_x(
                0,
                13,
                Some("X"),
                6,
                Direction::TD,
                2,
                PortalColumnPreference::Nearest,
            ),
            6
        );
    }

    #[test]
    fn title_safe_portal_policy_handles_missing_titles_and_margin_zero() {
        assert_eq!(
            title_safe_portal_x(
                10,
                16,
                None,
                15,
                Direction::BT,
                2,
                PortalColumnPreference::Directional,
            ),
            15
        );
        assert_eq!(
            title_safe_portal_x(
                10,
                16,
                Some("S2"),
                15,
                Direction::BT,
                0,
                PortalColumnPreference::Directional,
            ),
            16
        );
    }

    #[test]
    fn bt_title_margin_only_widens_exactly_two_parallel_edges() {
        fn graph_with_parallel_edges(count: usize) -> Graph {
            let mut graph = Graph::new();
            graph.direction = Direction::BT;
            graph.add_subgraph(Subgraph::new("source", Some("Source".into())));
            graph.add_subgraph(Subgraph::new("target", Some("Target".into())));

            for index in 0..count {
                let source_id = format!("source-{index}");
                let target_id = format!("target-{index}");
                graph.add_node(Node::new(&source_id, &source_id));
                graph.add_node(Node::new(&target_id, &target_id));
                graph.associate_node_with_subgraph(&source_id, "source");
                graph.associate_node_with_subgraph(&target_id, "target");
                graph.add_edge(Edge::new(&source_id, &target_id));
            }
            graph
        }

        for (count, expected) in [(1, 0), (2, 2), (3, 0), (4, 0)] {
            let graph = graph_with_parallel_edges(count);
            assert_eq!(
                bt_title_margin_for_edge(&graph, "source-0", "target-0", "target"),
                expected,
                "unexpected BT title margin for {count} parallel edges"
            );
        }
    }

    #[test]
    fn strict_bt_sibling_chain_gets_one_quiet_title_cell() {
        let mut graph = Graph::new();
        graph.direction = Direction::BT;
        for (id, title, y, first, second) in [
            ("g1", "Group 1", 0, "a1", "a2"),
            ("g2", "Group 2", 16, "b1", "b2"),
            ("g3", "Group 3", 32, "c1", "c2"),
        ] {
            let mut subgraph = Subgraph::new(id, Some(title.into()));
            subgraph.bounds = crate::graph::Rectangle::new(0, y, 17, 14);
            subgraph.add_node(first);
            subgraph.add_node(second);
            graph.add_subgraph(subgraph);
            graph.add_node(Node::new(first, first));
            graph.add_node(Node::new(second, second));
            graph.associate_node_with_subgraph(first, id);
            graph.associate_node_with_subgraph(second, id);
            graph.add_edge(Edge::new(first, second));
        }
        graph.add_edge(Edge::new("b2", "a1"));
        graph.add_edge(Edge::new("c2", "b1"));

        assert_eq!(
            bt_title_margin_for_edge(&graph, "b2", "a1", "g1"),
            BT_SIBLING_CHAIN_TITLE_MARGIN
        );
        assert_eq!(
            bt_title_margin_for_edge(&graph, "c2", "b1", "g2"),
            BT_SIBLING_CHAIN_TITLE_MARGIN
        );
    }

    #[test]
    fn td_sibling_title_padding_lanes_leave_a_wall_gutter() {
        assert_eq!(
            title_safe_portal_x_with_text_padding_sides(
                0,
                22,
                Some("Transform Stage"),
                9,
                Direction::TD,
                0,
                0,
                0,
                PortalColumnPreference::Nearest,
            ),
            2
        );
        assert_eq!(
            title_safe_portal_x_with_text_padding_sides(
                0,
                22,
                Some("Transform Stage"),
                13,
                Direction::TD,
                0,
                0,
                0,
                PortalColumnPreference::Nearest,
            ),
            18
        );
    }

    #[test]
    fn td_sibling_title_gutter_keeps_one_blank_cell_from_visible_text() {
        assert_eq!(
            title_safe_portal_x_with_text_padding_sides(
                0,
                22,
                Some("Transform Stage"),
                9,
                Direction::TD,
                1,
                1,
                1,
                PortalColumnPreference::Nearest,
            ),
            2
        );
        assert_eq!(
            title_safe_portal_x_with_text_padding_sides(
                0,
                22,
                Some("Transform Stage"),
                13,
                Direction::TD,
                1,
                1,
                1,
                PortalColumnPreference::Nearest,
            ),
            20
        );
    }

    #[test]
    fn right_sibling_title_gutter_keeps_a_wall_blank_in_narrow_bounds() {
        assert_eq!(
            title_safe_portal_x_with_text_padding_sides(
                0,
                14,
                Some("Group 3"),
                9,
                Direction::TD,
                0,
                1,
                1,
                PortalColumnPreference::Nearest,
            ),
            11
        );
    }

    #[test]
    fn title_margin_is_zero_only_for_bt_until_other_directions_are_reviewed() {
        assert_eq!(title_margin_for_direction(Direction::BT), 0);
        assert_eq!(title_margin_for_direction(Direction::TD), 2);
        assert_eq!(title_margin_for_direction(Direction::LR), 2);
        assert_eq!(title_margin_for_direction(Direction::RL), 2);
    }

    fn rect_inside(outer: Rect, inner: Rect) -> bool {
        inner.x >= outer.x
            && inner.y >= outer.y
            && inner.right() <= outer.right()
            && inner.bottom() <= outer.bottom()
    }

    #[test]
    fn portal_slots_cross_subgraph_td() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        g.nodes.push(Node::new("a", "A"));
        g.nodes.push(Node::new("b", "B"));
        g.nodes.push(Node::new("c", "C"));
        g.edges.push(Edge::new("a", "b"));
        g.edges.push(Edge::new("b", "c"));

        let mut sg = Subgraph::new("sg", Some("G".into()));
        sg.add_node("b");
        g.add_subgraph(sg);
        g.associate_node_with_subgraph("b", "sg");

        // Pretend layout already placed nodes.
        g.get_node_mut("a").unwrap().x = 0;
        g.get_node_mut("a").unwrap().y = 0;
        g.get_node_mut("b").unwrap().x = 4;
        g.get_node_mut("b").unwrap().y = 5;
        g.get_node_mut("c").unwrap().x = 8;
        g.get_node_mut("c").unwrap().y = 12;

        let node_rects = node_rects_from_graph(&g);
        let slots = collect_portal_slots(&g, &node_rects, g.direction);
        let portals = slots.get("sg").expect("slots for sg");
        assert!(
            !portals.top.is_empty(),
            "incoming edge should create top portal"
        );
        assert!(
            !portals.bottom.is_empty(),
            "outgoing edge should create bottom portal"
        );
    }

    #[test]
    fn td_sibling_portal_lanes_alternate_title_safe_sides_for_linear_chain() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        for (id, label) in [("a2", "A2"), ("b", "B"), ("b2", "B2"), ("c", "C")] {
            g.nodes.push(Node::new(id, label));
        }
        g.edges.push(Edge::new("a2", "b"));
        g.edges.push(Edge::new("b2", "c"));

        for (id, title, y, node_id) in [
            ("g1", "Group 1", 0, "a2"),
            ("g2", "Group 2", 16, "b"),
            ("g3", "Group 3", 33, "c"),
        ] {
            let mut subgraph = Subgraph::new(id, Some(title.into()));
            subgraph.bounds = crate::graph::Rectangle::new(0, y, 14, 14);
            subgraph.add_node(node_id);
            g.add_subgraph(subgraph);
            g.associate_node_with_subgraph(node_id, id);
        }
        {
            let middle = g.get_subgraph_mut("g2").expect("middle subgraph");
            middle.add_node("b2");
        }
        g.associate_node_with_subgraph("b2", "g2");

        let node_rects = HashMap::from([
            ("a2".to_string(), Rect::new(3, 7, 8, 3)),
            ("b".to_string(), Rect::new(3, 20, 8, 3)),
            ("b2".to_string(), Rect::new(3, 25, 8, 3)),
            ("c".to_string(), Rect::new(3, 37, 8, 3)),
        ]);
        let slots = collect_portal_slots(&g, &node_rects, g.direction);

        assert_eq!(
            td_sibling_title_gutter(&g, "g2"),
            TitleGutter {
                leading_extra_padding: 1,
                trailing_extra_padding: 0,
            }
        );
        assert_eq!(
            td_sibling_title_gutter(&g, "g3"),
            TitleGutter {
                leading_extra_padding: 0,
                trailing_extra_padding: 2,
            }
        );

        assert_eq!(
            slots.get("g2").expect("middle slots").top,
            HashSet::from([2]),
            "first sibling crossing should take the left padded title-gutter lane"
        );
        assert_eq!(
            slots.get("g3").expect("last slots").top,
            HashSet::from([11]),
            "second sibling crossing should take the right padded title-gutter lane"
        );
        assert_eq!(
            td_sibling_portal_x(&g, "a2", "b", 7, Direction::TD),
            Some(2)
        );
        assert_eq!(
            td_sibling_portal_x(&g, "b2", "c", 7, Direction::TD),
            Some(11)
        );
    }

    #[test]
    fn portal_slots_collapse_shared_td_fanout_entry_to_single_top_slot() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        g.nodes.push(Node::new("router", "Router"));
        g.nodes.push(Node::new("h1", "Handler 1"));
        g.nodes.push(Node::new("h2", "Handler 2"));
        g.nodes.push(Node::new("h3", "Handler 3"));
        g.edges.push(Edge::new("router", "h1"));
        g.edges.push(Edge::new("router", "h2"));
        g.edges.push(Edge::new("router", "h3"));

        let mut sg = Subgraph::new("sg", Some("Handler Group".into()));
        sg.add_node("h1");
        sg.add_node("h2");
        sg.add_node("h3");
        sg.bounds = crate::graph::Rectangle {
            x: 0,
            y: 5,
            width: 57,
            height: 9,
        };
        g.add_subgraph(sg);
        g.associate_node_with_subgraph("h1", "sg");
        g.associate_node_with_subgraph("h2", "sg");
        g.associate_node_with_subgraph("h3", "sg");

        g.get_node_mut("router").unwrap().x = 18;
        g.get_node_mut("router").unwrap().y = 0;
        g.get_node_mut("h1").unwrap().x = 2;
        g.get_node_mut("h1").unwrap().y = 10;
        g.get_node_mut("h2").unwrap().x = 21;
        g.get_node_mut("h2").unwrap().y = 10;
        g.get_node_mut("h3").unwrap().x = 40;
        g.get_node_mut("h3").unwrap().y = 10;

        let node_rects = node_rects_from_graph(&g);
        let slots = collect_portal_slots(&g, &node_rects, g.direction);
        let portals = slots.get("sg").expect("slots for sg");

        assert_eq!(
            portals.top.len(),
            1,
            "shared TD fanout should reserve one top entry slot, got {:?}",
            portals.top
        );
    }

    #[test]
    fn portal_slots_collapse_shared_td_fanin_exit_to_single_bottom_slot() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        g.nodes.push(Node::new("d1", "User DB"));
        g.nodes.push(Node::new("d2", "Order DB"));
        g.nodes.push(Node::new("rsp", "Response"));
        g.edges.push(Edge::new("d1", "rsp"));
        g.edges.push(Edge::new("d2", "rsp"));

        let mut sg = Subgraph::new("sg", Some("Data Layer".into()));
        sg.add_node("d1");
        sg.add_node("d2");
        sg.bounds = crate::graph::Rectangle::new(0, 10, 41, 15);
        g.add_subgraph(sg);
        g.associate_node_with_subgraph("d1", "sg");
        g.associate_node_with_subgraph("d2", "sg");

        g.get_node_mut("d1").unwrap().x = 24;
        g.get_node_mut("d1").unwrap().y = 14;
        g.get_node_mut("d1").unwrap().width = 13;
        g.get_node_mut("d1").unwrap().height = 3;
        g.get_node_mut("d2").unwrap().x = 4;
        g.get_node_mut("d2").unwrap().y = 20;
        g.get_node_mut("d2").unwrap().width = 14;
        g.get_node_mut("d2").unwrap().height = 3;
        g.get_node_mut("rsp").unwrap().x = 10;
        g.get_node_mut("rsp").unwrap().y = 28;
        g.get_node_mut("rsp").unwrap().width = 22;
        g.get_node_mut("rsp").unwrap().height = 3;

        let node_rects = node_rects_from_graph(&g);
        let slots = collect_portal_slots(&g, &node_rects, g.direction);
        let portals = slots.get("sg").expect("slots for sg");

        assert_eq!(
            portals.bottom.len(),
            1,
            "shared TD fanin should reserve one bottom exit slot, got {:?}",
            portals.bottom
        );
    }

    #[test]
    fn portal_slots_td_visually_nested_child_can_use_left_side_entry() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        g.nodes.push(Node::new("s2", "Order Service"));
        g.nodes.push(Node::new("d2", "Order DB"));
        g.edges.push(Edge::new("s2", "d2"));

        let mut outer = Subgraph::new("outer", Some("Service".into()));
        outer.bounds = crate::graph::Rectangle::new(0, 6, 47, 29);
        outer.add_node("s2");

        let mut inner = Subgraph::new("inner", Some("Data".into()));
        inner.bounds = crate::graph::Rectangle::new(22, 16, 23, 17);
        inner.add_node("d2");

        g.add_subgraph(outer);
        g.add_subgraph(inner);
        g.associate_node_with_subgraph("s2", "outer");
        g.associate_node_with_subgraph("d2", "inner");

        g.get_node_mut("s2").unwrap().x = 2;
        g.get_node_mut("s2").unwrap().y = 19;
        g.get_node_mut("s2").unwrap().width = 19;
        g.get_node_mut("s2").unwrap().height = 3;
        g.get_node_mut("d2").unwrap().x = 24;
        g.get_node_mut("d2").unwrap().y = 26;
        g.get_node_mut("d2").unwrap().width = 14;
        g.get_node_mut("d2").unwrap().height = 3;

        let node_rects = node_rects_from_graph(&g);
        let slots = collect_portal_slots(&g, &node_rects, g.direction);
        let portals = slots.get("inner").expect("slots for inner");

        assert!(
            portals.left.contains(&22),
            "expected the visually nested child to expose a left-side TD entry slot: {portals:?}"
        );
        assert!(
            portals.top.is_empty(),
            "expected side-entry routing to avoid a redundant top slot for this edge: {portals:?}"
        );
        assert!(
            slots.get("outer").is_none_or(|outer_portals| outer_portals.bottom.is_empty()),
            "expected the containing parent to avoid a redundant bottom exit slot when the edge stays visually inside it: {:?}",
            slots.get("outer")
        );
    }

    #[test]
    fn portal_slots_td_side_entry_uses_live_node_rects_not_stale_graph_coords() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        g.nodes.push(Node::new("s2", "Order Service"));
        g.nodes.push(Node::new("d2", "Order DB"));
        g.edges.push(Edge::new("s2", "d2"));

        let mut outer = Subgraph::new("outer", Some("Service".into()));
        outer.bounds = crate::graph::Rectangle::new(0, 6, 54, 29);
        outer.add_node("s2");

        let mut inner = Subgraph::new("inner", Some("Data".into()));
        inner.bounds = crate::graph::Rectangle::new(25, 16, 27, 17);
        inner.add_node("d2");

        g.add_subgraph(outer);
        g.add_subgraph(inner);
        g.get_subgraph_mut("inner").unwrap().parent_id = Some("outer".into());
        g.get_subgraph_mut("outer").unwrap().add_child("inner");
        g.associate_node_with_subgraph("s2", "outer");
        g.associate_node_with_subgraph("d2", "inner");

        // Simulate a layout loop where graph node positions are stale but node_rects
        // carry the live geometry that portal discovery must honor.
        g.get_node_mut("s2").unwrap().x = 0;
        g.get_node_mut("s2").unwrap().y = 0;
        g.get_node_mut("s2").unwrap().width = 19;
        g.get_node_mut("s2").unwrap().height = 3;
        g.get_node_mut("d2").unwrap().x = 0;
        g.get_node_mut("d2").unwrap().y = 0;
        g.get_node_mut("d2").unwrap().width = 14;
        g.get_node_mut("d2").unwrap().height = 3;

        let node_rects = HashMap::from([
            ("s2".to_string(), Rect::new(2, 19, 19, 3)),
            ("d2".to_string(), Rect::new(27, 26, 14, 3)),
        ]);

        let slots = collect_portal_slots(&g, &node_rects, g.direction);
        let portals = slots.get("inner").expect("slots for inner");

        assert!(
            portals.left.contains(&22),
            "expected the visually nested child to keep a left-side TD entry slot from live rects: {portals:?}"
        );
        assert!(
            slots.get("outer").is_none_or(|outer_portals| outer_portals.bottom.is_empty()),
            "expected the containing parent to suppress redundant bottom exits when live rects show the edge staying inside it: {:?}",
            slots.get("outer")
        );
    }

    #[test]
    fn portal_slots_external_to_nested_child_open_all_entered_ancestors() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        g.nodes.push(Node::new("src", "Source"));
        g.nodes.push(Node::new("dst", "Target"));
        g.edges.push(Edge::new("src", "dst"));

        g.add_subgraph(Subgraph::new("parent", Some("Parent".into())));
        g.add_subgraph(Subgraph::new("child", Some("Child".into())));
        g.get_subgraph_mut("child").unwrap().parent_id = Some("parent".into());
        g.get_subgraph_mut("parent").unwrap().add_child("child");
        g.associate_node_with_subgraph("dst", "child");

        g.get_node_mut("src").unwrap().x = 10;
        g.get_node_mut("src").unwrap().y = 0;
        g.get_node_mut("dst").unwrap().x = 12;
        g.get_node_mut("dst").unwrap().y = 10;

        let node_rects = node_rects_from_graph(&g);
        let slots = collect_portal_slots(&g, &node_rects, g.direction);

        assert!(!slots
            .get("parent")
            .expect("parent slots should exist")
            .top
            .is_empty());
        assert!(!slots
            .get("child")
            .expect("child slots should exist")
            .top
            .is_empty());
    }

    #[test]
    fn portal_slots_child_to_external_open_all_exited_ancestors() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        g.nodes.push(Node::new("src", "Source"));
        g.nodes.push(Node::new("dst", "Target"));
        g.edges.push(Edge::new("src", "dst"));

        g.add_subgraph(Subgraph::new("parent", Some("Parent".into())));
        g.add_subgraph(Subgraph::new("child", Some("Child".into())));
        g.get_subgraph_mut("child").unwrap().parent_id = Some("parent".into());
        g.get_subgraph_mut("parent").unwrap().add_child("child");
        g.associate_node_with_subgraph("src", "child");

        g.get_node_mut("src").unwrap().x = 12;
        g.get_node_mut("src").unwrap().y = 10;
        g.get_node_mut("dst").unwrap().x = 10;
        g.get_node_mut("dst").unwrap().y = 20;

        let node_rects = node_rects_from_graph(&g);
        let slots = collect_portal_slots(&g, &node_rects, g.direction);

        assert!(!slots
            .get("parent")
            .expect("parent slots should exist")
            .bottom
            .is_empty());
        assert!(!slots
            .get("child")
            .expect("child slots should exist")
            .bottom
            .is_empty());
    }

    #[test]
    fn compute_envelopes_builds_parent_from_child_when_parent_has_no_direct_nodes() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        g.add_subgraph(Subgraph::new("parent", Some("Parent".into())));
        g.add_subgraph(Subgraph::new("child", Some("Child".into())));
        g.get_subgraph_mut("child").unwrap().parent_id = Some("parent".into());
        g.get_subgraph_mut("parent").unwrap().add_child("child");

        g.add_node(Node::new("n1", "Inner"));
        g.get_node_mut("n1").unwrap().x = 10;
        g.get_node_mut("n1").unwrap().y = 8;
        g.associate_node_with_subgraph("n1", "child");

        let node_rects = node_rects_from_graph(&g);
        let envelopes = compute_envelopes(&g, &node_rects, 2);
        let parent = envelopes.get("parent").expect("parent envelope");
        let child = envelopes.get("child").expect("child envelope");

        assert!(rect_inside(parent.inner, child.outer.inflate(1)));
        assert!(rect_inside(parent.outer, child.outer));
    }

    #[test]
    fn compute_envelopes_counts_descendant_edges_as_parent_external_edges() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        g.add_subgraph(Subgraph::new("parent", Some("Parent".into())));
        g.add_subgraph(Subgraph::new("child", Some("Child".into())));
        g.get_subgraph_mut("child").unwrap().parent_id = Some("parent".into());
        g.get_subgraph_mut("parent").unwrap().add_child("child");

        g.add_node(Node::new("inside", "Inside"));
        g.add_node(Node::new("outside", "Outside"));
        g.get_node_mut("inside").unwrap().x = 12;
        g.get_node_mut("inside").unwrap().y = 8;
        g.get_node_mut("outside").unwrap().x = 35;
        g.get_node_mut("outside").unwrap().y = 16;
        g.associate_node_with_subgraph("inside", "child");
        g.add_edge(Edge::new("inside", "outside"));

        let node_rects = node_rects_from_graph(&g);
        let envelopes = compute_envelopes(&g, &node_rects, 3);
        let parent = envelopes.get("parent").expect("parent envelope");

        assert!(
            parent.outer.width > parent.inner.width + 4,
            "parent should reserve external-edge gutter for descendant crossings: outer={:?} inner={:?}",
            parent.outer,
            parent.inner
        );
    }

    #[test]
    fn compute_envelopes_keep_parent_visibly_outside_nested_child() {
        let mut g = Graph::new();
        g.direction = Direction::TD;
        g.add_subgraph(Subgraph::new("parent", Some("Parent".into())));
        g.add_subgraph(Subgraph::new("child", Some("Child".into())));
        g.get_subgraph_mut("child").unwrap().parent_id = Some("parent".into());
        g.get_subgraph_mut("parent").unwrap().add_child("child");

        g.add_node(Node::new("parent_node", "Parent Node"));
        g.add_node(Node::new("child_node", "Child Node"));
        g.add_node(Node::new("outside", "Outside"));
        g.get_node_mut("parent_node").unwrap().x = 2;
        g.get_node_mut("parent_node").unwrap().y = 6;
        g.get_node_mut("child_node").unwrap().x = 24;
        g.get_node_mut("child_node").unwrap().y = 12;
        g.get_node_mut("outside").unwrap().x = 12;
        g.get_node_mut("outside").unwrap().y = 20;
        g.associate_node_with_subgraph("parent_node", "parent");
        g.associate_node_with_subgraph("child_node", "child");
        g.add_edge(Edge::new("child_node", "outside"));

        let node_rects = node_rects_from_graph(&g);
        let envelopes = compute_envelopes(&g, &node_rects, 2);
        let parent = envelopes.get("parent").expect("parent envelope");
        let child = envelopes.get("child").expect("child envelope");

        assert!(
            parent.outer.y < child.outer.y || parent.outer.bottom() > child.outer.bottom(),
            "parent should stay visibly outside nested child: parent={:?} child={:?}",
            parent.outer,
            child.outer
        );
        assert!(rect_inside(parent.outer, child.outer));
    }
}
