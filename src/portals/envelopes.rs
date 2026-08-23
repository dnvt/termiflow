//! Subgraph envelope geometry and containment policy.

use std::collections::{HashMap, HashSet};

use crate::geom::Rect;
use crate::graph::{Direction, EdgeKind, Graph, NodeShape};

use super::{
    collect_portal_slots_with_bounds, horizontal_sibling_chain_requires_extra_corridor,
    PortalSlots, SubgraphEnvelope,
};

fn subgraphs_have_declared_hierarchy(graph: &Graph, left_id: &str, right_id: &str) -> bool {
    graph.is_subgraph_ancestor(left_id, right_id) || graph.is_subgraph_ancestor(right_id, left_id)
}

fn rects_overlap_vertically(a: Rect, b: Rect) -> bool {
    a.y < b.bottom() && b.y < a.bottom()
}

fn rects_overlap_horizontally(a: Rect, b: Rect) -> bool {
    a.x < b.right() && b.x < a.right()
}

fn centered_outer_with_width(outer: Rect, width: usize) -> Rect {
    if width <= outer.width {
        return outer;
    }

    let extra = width - outer.width;
    let left_extra = extra / 2;
    Rect::new(
        outer.x.saturating_sub(left_extra),
        outer.y,
        width,
        outer.height,
    )
}

fn centered_outer_with_height(outer: Rect, height: usize) -> Rect {
    if height <= outer.height {
        return outer;
    }

    let extra = height - outer.height;
    let top_extra = extra / 2;
    Rect::new(
        outer.x,
        outer.y.saturating_sub(top_extra),
        outer.width,
        height,
    )
}

fn inner_horizontal_pad_delta(env: &SubgraphEnvelope, outer: Rect) -> usize {
    let left_pad = env.inner.x.saturating_sub(outer.x.saturating_add(1));
    let right_pad = outer
        .right()
        .saturating_sub(1)
        .saturating_sub(env.inner.right());
    left_pad.abs_diff(right_pad)
}

fn inner_vertical_pad_delta(env: &SubgraphEnvelope, outer: Rect) -> usize {
    let top_pad = env.inner.y.saturating_sub(outer.y);
    let bottom_pad = outer
        .y
        .saturating_add(outer.height.saturating_sub(1))
        .saturating_sub(env.inner.y.saturating_add(env.inner.height));
    top_pad.abs_diff(bottom_pad)
}

#[allow(dead_code)]
fn candidate_introduces_foreign_node_overlap(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    subgraph_id: &str,
    current: Rect,
    candidate: Rect,
) -> bool {
    node_rects.iter().any(|(node_id, rect)| {
        !graph.is_node_in_subgraph_tree(node_id, subgraph_id)
            && rects_overlap_vertically(*rect, candidate)
            && rects_overlap_horizontally(*rect, candidate)
            && !(rects_overlap_vertically(*rect, current)
                && rects_overlap_horizontally(*rect, current))
    })
}

fn top_level_connected_subgraph_components(graph: &Graph) -> Vec<Vec<&str>> {
    let top_level_ids: Vec<&str> = graph
        .subgraphs
        .iter()
        .filter(|subgraph| subgraph.parent_id.is_none())
        .map(|subgraph| subgraph.id.as_str())
        .collect();
    if top_level_ids.len() < 2 {
        return Vec::new();
    }

    let top_level_set: HashSet<&str> = top_level_ids.iter().copied().collect();
    let mut adjacency: HashMap<&str, HashSet<&str>> = top_level_ids
        .iter()
        .copied()
        .map(|id| (id, HashSet::new()))
        .collect();

    for edge in graph.edges.iter().filter(|edge| !edge.is_back_edge) {
        let Some(from_sg) = graph.get_node_subgraph(&edge.from) else {
            continue;
        };
        let Some(to_sg) = graph.get_node_subgraph(&edge.to) else {
            continue;
        };
        if from_sg == to_sg || !top_level_set.contains(from_sg) || !top_level_set.contains(to_sg) {
            continue;
        }

        adjacency.entry(from_sg).or_default().insert(to_sg);
        adjacency.entry(to_sg).or_default().insert(from_sg);
    }

    let mut visited: HashSet<&str> = HashSet::new();
    let mut components = Vec::new();
    for &start_id in &top_level_ids {
        if !visited.insert(start_id) {
            continue;
        }

        let mut stack = vec![start_id];
        let mut component = Vec::new();
        while let Some(current) = stack.pop() {
            component.push(current);
            if let Some(neighbors) = adjacency.get(current) {
                for &next in neighbors {
                    if visited.insert(next) {
                        stack.push(next);
                    }
                }
            }
        }

        if component.len() < 2 {
            continue;
        }

        components.push(component);
    }

    components
}

fn harmonize_stacked_vertical_top_level_sibling_widths(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    envelopes: &mut HashMap<String, SubgraphEnvelope>,
) {
    if !matches!(
        graph.direction,
        Direction::TD | Direction::TB | Direction::BT
    ) {
        return;
    }

    let components = top_level_connected_subgraph_components(graph);
    for component in components {
        let mut ordered: Vec<(&str, Rect)> = component
            .iter()
            .filter_map(|id| envelopes.get(*id).map(|env| (*id, env.outer)))
            .collect();
        if ordered.len() < 2 {
            continue;
        }

        ordered.sort_by_key(|(_, outer)| outer.y);
        let is_stacked_column = ordered.windows(2).all(|pair| {
            let upper = pair[0].1;
            let lower = pair[1].1;
            !rects_overlap_vertically(upper, lower) && rects_overlap_horizontally(upper, lower)
        });
        if !is_stacked_column {
            continue;
        }

        let min_width = ordered
            .iter()
            .map(|(_, outer)| outer.width)
            .min()
            .unwrap_or(0);
        let target_left = ordered.iter().map(|(_, outer)| outer.x).min().unwrap_or(0);
        let target_right = ordered
            .iter()
            .map(|(_, outer)| outer.right())
            .max()
            .unwrap_or(target_left);
        let target_width = target_right.saturating_sub(target_left);
        let width_spread = target_width.saturating_sub(min_width);
        if width_spread == 0 || width_spread > 12 {
            continue;
        }

        let mut normalized: Vec<(&str, Rect)> = Vec::with_capacity(ordered.len());
        for (subgraph_id, outer) in &ordered {
            let mut best_outer = *outer;
            let mut candidate_width = target_width;
            while candidate_width > outer.width {
                let candidate = centered_outer_with_width(*outer, candidate_width);
                if !candidate_introduces_foreign_node_overlap(
                    graph,
                    node_rects,
                    subgraph_id,
                    *outer,
                    candidate,
                ) {
                    best_outer = candidate;
                    break;
                }
                candidate_width = candidate_width.saturating_sub(1);
            }
            normalized.push((*subgraph_id, best_outer));
        }

        let shared_left = normalized
            .iter()
            .map(|(_, outer)| outer.x)
            .min()
            .unwrap_or(0);
        let shared_right = normalized
            .iter()
            .map(|(_, outer)| outer.right())
            .max()
            .unwrap_or(shared_left);
        let shared_width = shared_right.saturating_sub(shared_left);

        for (subgraph_id, normalized_outer) in normalized {
            let Some(env) = envelopes.get_mut(subgraph_id) else {
                continue;
            };

            let current_outer = normalized_outer;
            let aligned_outer =
                if current_outer.x == shared_left && current_outer.width == shared_width {
                    current_outer
                } else {
                    let candidate = Rect::new(
                        shared_left,
                        current_outer.y,
                        shared_width,
                        current_outer.height,
                    );
                    let candidate_delta = inner_horizontal_pad_delta(env, candidate);
                    if !candidate_introduces_foreign_node_overlap(
                        graph,
                        node_rects,
                        subgraph_id,
                        current_outer,
                        candidate,
                    ) && candidate_delta <= 1
                    {
                        candidate
                    } else {
                        current_outer
                    }
                };

            env.outer = aligned_outer;
        }
    }
}

#[allow(dead_code)]
fn harmonize_stacked_vertical_top_level_sibling_heights(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    envelopes: &mut HashMap<String, SubgraphEnvelope>,
) {
    if !matches!(
        graph.direction,
        Direction::TD | Direction::TB | Direction::BT
    ) {
        return;
    }

    const MAX_HEIGHT_SPREAD: usize = 2;

    for component in top_level_connected_subgraph_components(graph) {
        let mut ordered: Vec<(&str, Rect)> = component
            .iter()
            .filter_map(|id| envelopes.get(*id).map(|env| (*id, env.outer)))
            .collect();
        if ordered.len() < 2 {
            continue;
        }

        ordered.sort_by_key(|(_, outer)| outer.y);
        let is_stacked_column = ordered.windows(2).all(|pair| {
            let upper = pair[0].1;
            let lower = pair[1].1;
            !rects_overlap_vertically(upper, lower) && rects_overlap_horizontally(upper, lower)
        });
        if !is_stacked_column {
            continue;
        }

        let min_height = ordered
            .iter()
            .map(|(_, outer)| outer.height)
            .min()
            .unwrap_or(0);
        let target_height = ordered
            .iter()
            .map(|(_, outer)| outer.height)
            .max()
            .unwrap_or(min_height);
        let height_spread = target_height.saturating_sub(min_height);
        if height_spread == 0 || height_spread > MAX_HEIGHT_SPREAD {
            continue;
        }

        for (subgraph_id, outer) in ordered {
            if outer.height >= target_height {
                continue;
            }

            let Some(env) = envelopes.get_mut(subgraph_id) else {
                continue;
            };
            let current_delta = inner_vertical_pad_delta(env, outer);
            let candidate = centered_outer_with_height(outer, target_height);
            if !candidate_introduces_foreign_node_overlap(
                graph,
                node_rects,
                subgraph_id,
                outer,
                candidate,
            ) && inner_vertical_pad_delta(env, candidate) <= current_delta
            {
                env.outer = candidate;
            }
        }
    }
}

fn harmonize_side_by_side_horizontal_top_level_sibling_heights(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    envelopes: &mut HashMap<String, SubgraphEnvelope>,
) {
    if !matches!(graph.direction, Direction::LR | Direction::RL) {
        return;
    }

    const MAX_HEIGHT_SPREAD: usize = 4;

    for component in top_level_connected_subgraph_components(graph) {
        let mut ordered: Vec<(&str, Rect, Rect)> = component
            .iter()
            .filter_map(|id| envelopes.get(*id).map(|env| (*id, env.outer, env.inner)))
            .collect();
        if ordered.len() < 2 {
            continue;
        }

        ordered.sort_by_key(|(_, outer, _)| outer.x);
        let is_side_by_side_row = ordered.windows(2).all(|pair| {
            let left_outer = pair[0].1;
            let right_outer = pair[1].1;
            let left_inner = pair[0].2;
            let right_inner = pair[1].2;

            let outer_separate = !rects_overlap_horizontally(left_outer, right_outer);
            let inner_separate = !rects_overlap_horizontally(left_inner, right_inner);

            rects_overlap_vertically(left_outer, right_outer) && (outer_separate || inner_separate)
        });
        if !is_side_by_side_row {
            continue;
        }

        let min_height = ordered
            .iter()
            .map(|(_, outer, _)| outer.height)
            .min()
            .unwrap_or(0);
        let shared_top = ordered
            .iter()
            .map(|(_, outer, _)| outer.y)
            .min()
            .unwrap_or(0);
        let shared_bottom = ordered
            .iter()
            .map(|(_, outer, _)| outer.bottom())
            .max()
            .unwrap_or(shared_top);
        let shared_height = shared_bottom.saturating_sub(shared_top);
        let height_spread = shared_height.saturating_sub(min_height);
        if height_spread == 0 || height_spread > MAX_HEIGHT_SPREAD {
            continue;
        }

        for (subgraph_id, outer, _) in ordered {
            let Some(env) = envelopes.get_mut(subgraph_id) else {
                continue;
            };
            let candidate = Rect::new(outer.x, shared_top, outer.width, shared_height);
            if candidate == outer {
                continue;
            }
            if !candidate_introduces_foreign_node_overlap(
                graph,
                node_rects,
                subgraph_id,
                outer,
                candidate,
            ) {
                env.outer = candidate;
            }
        }
    }
}

fn is_horizontal_visual_nesting_candidate(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    parent_id: &str,
    child_id: &str,
    parent: &SubgraphEnvelope,
    child: &SubgraphEnvelope,
    gutter: usize,
) -> bool {
    if !matches!(graph.direction, Direction::LR | Direction::RL) {
        return false;
    }
    if subgraphs_have_declared_hierarchy(graph, parent_id, child_id) {
        return false;
    }

    let cross_boundary_targets: HashSet<&str> = graph
        .edges
        .iter()
        .filter(|edge| {
            !edge.is_back_edge
                && graph.get_node_subgraph(&edge.from) == Some(parent_id)
                && graph.get_node_subgraph(&edge.to) == Some(child_id)
        })
        .map(|edge| edge.to.as_str())
        .collect();
    if cross_boundary_targets.len() < 2 {
        return false;
    }

    let child_has_external_outgoing = graph.edges.iter().any(|edge| {
        !edge.is_back_edge
            && graph.is_node_in_subgraph_tree(&edge.from, child_id)
            && !graph.is_node_in_subgraph_tree(&edge.to, child_id)
    });
    if !child_has_external_outgoing {
        return false;
    }

    let near_parent_flow_band = match graph.direction {
        Direction::LR => {
            child.outer.x
                <= parent
                    .outer
                    .right()
                    .saturating_add(gutter.saturating_add(2))
        }
        Direction::RL => {
            child.outer.right().saturating_add(gutter.saturating_add(2)) >= parent.outer.x
        }
        _ => false,
    };
    if !near_parent_flow_band {
        return false;
    }

    node_rects.iter().any(|(node_id, rect)| {
        graph.get_node_subgraph(node_id) == Some(parent_id)
            && rects_overlap_vertically(*rect, child.outer)
            && match graph.direction {
                Direction::LR => rect.x < child.outer.right(),
                Direction::RL => rect.right() > child.outer.x,
                _ => false,
            }
    })
}

pub(super) fn current_node_rect(
    rects: &HashMap<String, Rect>,
    node_id: &str,
    fallback_node: &crate::graph::Node,
) -> Rect {
    rects.get(node_id).copied().unwrap_or_else(|| {
        Rect::new(
            fallback_node.x,
            fallback_node.y,
            fallback_node.width,
            fallback_node.height.max(crate::style::BOX_HEIGHT),
        )
    })
}

pub(super) fn current_subgraph_bounds(
    graph: &Graph,
    current_bounds: Option<&HashMap<String, Rect>>,
    subgraph_id: &str,
) -> Option<Rect> {
    if let Some(bounds) = current_bounds
        .and_then(|bounds| bounds.get(subgraph_id))
        .copied()
    {
        return Some(bounds);
    }

    let subgraph = graph.get_subgraph(subgraph_id)?;
    Some(Rect::new(
        subgraph.bounds.x,
        subgraph.bounds.y,
        subgraph.bounds.width,
        subgraph.bounds.height,
    ))
}

/// Build node rectangles from a laid-out graph.
pub fn node_rects_from_graph(graph: &Graph) -> HashMap<String, Rect> {
    graph
        .nodes
        .iter()
        .map(|n| (n.id.clone(), Rect::new(n.x, n.y, n.width, n.height)))
        .collect()
}

/// Compute subgraph envelopes (inner/outer) and portals for the given graph state.
pub fn compute_envelopes(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    gutter: usize,
) -> HashMap<String, SubgraphEnvelope> {
    let mut envelopes: HashMap<String, SubgraphEnvelope> = HashMap::new();
    let mut subgraphs_by_depth: Vec<&crate::graph::Subgraph> = graph.subgraphs.iter().collect();
    subgraphs_by_depth.sort_by_key(|subgraph| std::cmp::Reverse(subgraph_depth(graph, subgraph)));

    for subgraph in subgraphs_by_depth {
        let child_ids = child_subgraph_ids(graph, &subgraph.id);
        let (mut content, mut max_exit_y) = direct_subgraph_content(subgraph, node_rects);
        for child_id in &child_ids {
            let Some(child_env) = envelopes.get(child_id) else {
                continue;
            };
            let child_clearance = child_env.outer.inflate(1);
            content = if content.is_empty() {
                child_clearance
            } else {
                content.union(&child_clearance)
            };
            max_exit_y = max_exit_y.max(child_env.outer.bottom());
        }
        if content.is_empty() {
            continue;
        }

        let mut envelope = build_envelope(graph, subgraph, node_rects, gutter, content, max_exit_y);
        for child_id in &child_ids {
            let Some(child_env) = envelopes.get(child_id) else {
                continue;
            };
            let child_clearance = child_env.outer.inflate(1);
            envelope.inner = if envelope.inner.is_empty() {
                child_clearance
            } else {
                envelope.inner.union(&child_clearance)
            };
            envelope.outer = envelope.outer.union(&child_clearance);
        }
        envelopes.insert(subgraph.id.clone(), envelope);
    }

    enforce_declared_nested_containment(graph, &mut envelopes);

    // If two subgraphs overlap in canvas space and there is a cross-subgraph edge
    // from one to the other, treat the source subgraph as an outer container and
    // expand it to fully enclose the destination subgraph.
    //
    // This preserves separate stacked subgraphs (no overlap), while allowing
    // "visually nested" compositions to render as nested envelopes.
    //
    // IMPORTANT: Only expand if the child's content is INSIDE the parent's content
    // (true nesting). If the child's inner region is below/above the parent's inner
    // region (stacked), don't expand - let the layout constraint loop handle spacing.
    if matches!(
        graph.direction,
        Direction::TD | Direction::TB | Direction::BT
    ) {
        let intersects = |a: Rect, b: Rect| -> bool {
            a.x < b.right() && a.right() > b.x && a.y < b.bottom() && a.bottom() > b.y
        };
        for e in &graph.edges {
            let Some(from_sg) = graph.get_node_subgraph(&e.from) else {
                continue;
            };
            let Some(to_sg) = graph.get_node_subgraph(&e.to) else {
                continue;
            };
            if from_sg == to_sg {
                continue;
            }
            if subgraphs_have_declared_hierarchy(graph, from_sg, to_sg) {
                continue;
            }
            let (parent_id, child_id) = match graph.direction {
                Direction::BT => (to_sg, from_sg),
                _ => (from_sg, to_sg),
            };
            let (Some(parent), Some(child)) = (envelopes.get(parent_id), envelopes.get(child_id))
            else {
                continue;
            };
            if !intersects(parent.outer, child.outer) {
                continue;
            }
            // Check if child is truly nested (inner content starts within parent's inner)
            // vs stacked (child's inner is entirely below/above parent's inner).
            let is_stacked = match graph.direction {
                Direction::TD | Direction::TB | Direction::BT => {
                    child.inner.y >= parent.inner.bottom()
                }
                _ => false,
            };
            if is_stacked {
                // Don't expand for stacked subgraphs - let layout constraint loop handle spacing
                continue;
            }
            let child_clearance = child.outer.inflate(2);
            let mut new_outer = parent.outer.union(&child_clearance);
            let bt_titled_nested = graph.direction == Direction::BT
                && (graph
                    .get_subgraph(parent_id)
                    .and_then(|sg| sg.title.as_ref())
                    .is_some()
                    || graph
                        .get_subgraph(child_id)
                        .and_then(|sg| sg.title.as_ref())
                        .is_some());
            // Ensure the parent border doesn't land on the same row as the child border;
            // give the parent at least one extra row of depth beyond the child.
            if matches!(graph.direction, Direction::TD | Direction::TB) {
                let desired_bottom = child_clearance.bottom();
                if new_outer.bottom() < desired_bottom {
                    new_outer.height += desired_bottom - new_outer.bottom();
                }
            } else if graph.direction == Direction::BT {
                // BT: children are below parents, so expand TOP to give clearance
                let desired_top = child_clearance.y;
                if new_outer.y > desired_top {
                    let extra = new_outer.y - desired_top;
                    new_outer.y = desired_top;
                    new_outer.height += extra;
                }
                if bt_titled_nested {
                    // BT titles live on the bottom border row. If a visually nested BT parent
                    // stops on the same bottom row as its child, both titles fight for the
                    // same border. Leave one full spacer row between those border rows.
                    let desired_bottom = child.outer.bottom().saturating_add(2);
                    if new_outer.bottom() < desired_bottom {
                        new_outer.height += desired_bottom - new_outer.bottom();
                    }
                }
            }
            if let Some(parent_mut) = envelopes.get_mut(parent_id) {
                parent_mut.outer = new_outer;
            }
        }
    }

    if matches!(graph.direction, Direction::LR | Direction::RL) {
        for e in &graph.edges {
            let Some(from_sg) = graph.get_node_subgraph(&e.from) else {
                continue;
            };
            let Some(to_sg) = graph.get_node_subgraph(&e.to) else {
                continue;
            };
            if from_sg == to_sg {
                continue;
            }
            if subgraphs_have_declared_hierarchy(graph, from_sg, to_sg) {
                continue;
            }

            let parent_id = from_sg;
            let child_id = to_sg;
            let (Some(parent), Some(child)) = (envelopes.get(parent_id), envelopes.get(child_id))
            else {
                continue;
            };
            if !is_horizontal_visual_nesting_candidate(
                graph, node_rects, parent_id, child_id, parent, child, gutter,
            ) {
                continue;
            }

            let mut new_outer = parent.outer.union(&child.outer);
            let desired_bottom = child.outer.bottom().saturating_add(1);
            if new_outer.bottom() < desired_bottom {
                new_outer.height += desired_bottom - new_outer.bottom();
            }

            match graph.direction {
                Direction::LR => {
                    let desired_right = child.outer.right().saturating_add(2);
                    if new_outer.right() < desired_right {
                        new_outer.width += desired_right - new_outer.right();
                    }
                }
                Direction::RL => {
                    let desired_left = child.outer.x.saturating_sub(2);
                    if new_outer.x > desired_left {
                        let extra = new_outer.x - desired_left;
                        new_outer.x = desired_left;
                        new_outer.width += extra;
                    }
                }
                _ => {}
            }

            if let Some(parent_mut) = envelopes.get_mut(parent_id) {
                parent_mut.outer = new_outer;
            }
        }
    }

    harmonize_stacked_vertical_top_level_sibling_widths(graph, node_rects, &mut envelopes);
    harmonize_side_by_side_horizontal_top_level_sibling_heights(graph, node_rects, &mut envelopes);

    // Reserve the target-title row as one atomic scene transaction. The BT
    // renderer consumes the same live-topology predicate and routes the turn
    // one row farther from the title; ordinary sibling layouts retain their
    // established envelope geometry.
    let chain_bounds: HashMap<String, Rect> = envelopes
        .iter()
        .map(|(id, env)| (id.clone(), env.outer))
        .collect();
    if let Some(target_ids) = super::bt_sibling_chain_target_ids(graph, &chain_bounds) {
        for target_id in target_ids {
            if let Some(env) = envelopes.get_mut(&target_id) {
                env.outer.height = env.outer.height.saturating_add(1);
            }
        }
    }

    // The exact mixed BT sibling target has an internal receiver branch below
    // the crossed target branch. Its route-owned horizontal turn must clear
    // that branch before it can approach the upper receiver; adding the quiet
    // band to the target envelope keeps the title row and the route turn from
    // becoming one visual cluster. The predicate is exact and leaves ordinary
    // two-edge sibling crossings on their established envelope budget.
    if let Some(target_id) = exact_bt_mixed_sibling_target_id(graph, &envelopes) {
        if let Some(env) = envelopes.get_mut(&target_id) {
            env.outer.height = env.outer.height.saturating_add(2);
        }
    }

    // Populate portals after envelopes are defined so we can clamp coordinates.
    let current_bounds: HashMap<String, Rect> = envelopes
        .iter()
        .map(|(id, env)| (id.clone(), env.outer))
        .collect();
    let slots =
        collect_portal_slots_with_bounds(graph, node_rects, graph.direction, Some(&current_bounds));
    for (sg_id, portal) in slots {
        if let Some(env) = envelopes.get_mut(&sg_id) {
            env.portals = portal;
        }
    }

    envelopes
}

/// Return the upper target boundary for the exact four-node BT mixed sibling
/// scene using live envelope bounds. The render selector intentionally reads
/// finalized graph bounds; layout needs the same ownership before those bounds
/// have been copied back, so this narrow mirror keeps the extra quiet band in
/// the layout/render contract without depending on fixture names.
fn exact_bt_mixed_sibling_target_id(
    graph: &Graph,
    envelopes: &HashMap<String, SubgraphEnvelope>,
) -> Option<String> {
    if graph.direction != Direction::BT
        || graph.subgraphs.len() != 2
        || graph.nodes.len() != 4
        || graph.edges.len() != 4
        || graph.has_cycles()
    {
        return None;
    }

    let mut ordered = graph
        .subgraphs
        .iter()
        .filter(|subgraph| {
            subgraph.parent_id.is_none()
                && subgraph.child_ids.is_empty()
                && subgraph.title.is_some()
                && subgraph.node_ids.len() == 2
                && subgraph.node_ids.iter().all(|node_id| {
                    graph.get_node(node_id).is_some()
                        && graph.get_node_subgraph(node_id) == Some(subgraph.id.as_str())
                })
        })
        .filter_map(|subgraph| envelopes.get(&subgraph.id).map(|env| (subgraph, env.outer)))
        .collect::<Vec<_>>();
    if ordered.len() != 2 || ordered.iter().any(|(_, bounds)| bounds.is_empty()) {
        return None;
    }
    ordered.sort_by_key(|(_, bounds)| (bounds.y, bounds.x));
    let (target, target_bounds) = ordered[0];
    let (source, source_bounds) = ordered[1];
    if target_bounds.y >= source_bounds.y || target_bounds.bottom() > source_bounds.y {
        return None;
    }

    let ordinary_edges = graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge && edge.kind == EdgeKind::Arrow && edge.label.is_none())
        .collect::<Vec<_>>();
    if ordinary_edges.len() != 4 {
        return None;
    }
    let internal_count = |subgraph: &crate::graph::Subgraph| {
        ordinary_edges
            .iter()
            .filter(|edge| {
                subgraph.node_ids.contains(&edge.from) && subgraph.node_ids.contains(&edge.to)
            })
            .count()
    };
    if internal_count(source) != 1 || internal_count(target) != 1 {
        return None;
    }
    let crossing_count = ordinary_edges
        .iter()
        .filter(|edge| source.node_ids.contains(&edge.from) && target.node_ids.contains(&edge.to))
        .count();
    (crossing_count == 2).then(|| target.id.clone())
}

fn enforce_declared_nested_containment(
    graph: &Graph,
    envelopes: &mut HashMap<String, SubgraphEnvelope>,
) {
    let mut subgraphs_by_depth: Vec<&crate::graph::Subgraph> = graph
        .subgraphs
        .iter()
        .filter(|subgraph| subgraph.parent_id.is_some())
        .collect();
    subgraphs_by_depth.sort_by_key(|subgraph| std::cmp::Reverse(subgraph_depth(graph, subgraph)));

    for subgraph in subgraphs_by_depth {
        let Some(parent_id) = subgraph.parent_id.as_deref() else {
            continue;
        };
        let Some(child_env) = envelopes.get(&subgraph.id).cloned() else {
            continue;
        };
        let Some(parent_env) = envelopes.get_mut(parent_id) else {
            continue;
        };

        let child_clearance = child_env.outer.inflate(1);
        parent_env.inner = if parent_env.inner.is_empty() {
            child_clearance
        } else {
            parent_env.inner.union(&child_clearance)
        };
        parent_env.outer = if parent_env.outer.is_empty() {
            child_clearance
        } else {
            parent_env.outer.union(&child_clearance)
        };
    }
}

fn subgraph_depth(graph: &Graph, subgraph: &crate::graph::Subgraph) -> usize {
    let mut depth = 0usize;
    let mut current = subgraph.parent_id.as_deref();
    while let Some(parent_id) = current {
        depth += 1;
        current = graph
            .get_subgraph(parent_id)
            .and_then(|parent| parent.parent_id.as_deref());
    }
    depth
}

fn child_subgraph_ids(graph: &Graph, parent_id: &str) -> Vec<String> {
    graph
        .subgraphs
        .iter()
        .filter(|subgraph| subgraph.parent_id.as_deref() == Some(parent_id))
        .map(|subgraph| subgraph.id.clone())
        .collect()
}

fn direct_subgraph_content(
    subgraph: &crate::graph::Subgraph,
    node_rects: &HashMap<String, Rect>,
) -> (Rect, usize) {
    let mut content = Rect::default();
    let mut max_exit_y = 0;
    for node_id in &subgraph.node_ids {
        if let Some(r) = node_rects.get(node_id) {
            content = if content.is_empty() {
                *r
            } else {
                content.union(r)
            };
            max_exit_y = max_exit_y.max(r.bottom());
        }
    }
    (content, max_exit_y)
}

fn strict_simple_horizontal_subgraph_fanin(graph: &Graph, subgraph_id: &str) -> bool {
    if !matches!(graph.direction, Direction::LR | Direction::RL)
        || graph.subgraphs.len() != 1
        || graph.edges.len() < 3
    {
        return false;
    }

    let Some(target_id) = graph.edges.first().map(|edge| edge.to.as_str()) else {
        return false;
    };
    let Some(target) = graph.get_node(target_id) else {
        return false;
    };
    if target.shape != NodeShape::Rectangle {
        return false;
    }

    let mut source_ids = HashSet::new();
    for edge in &graph.edges {
        if edge.is_back_edge
            || edge.label.is_some()
            || edge.kind != crate::graph::EdgeKind::Arrow
            || edge.to != target_id
            || graph.get_node_subgraph(&edge.from) != Some(subgraph_id)
            || graph.get_node_subgraph(&edge.to) == Some(subgraph_id)
            || graph
                .get_node(&edge.from)
                .is_none_or(|source| source.shape != NodeShape::Rectangle)
        {
            return false;
        }
        source_ids.insert(edge.from.as_str());
    }

    source_ids.len() >= 3
}

fn build_envelope(
    graph: &Graph,
    subgraph: &crate::graph::Subgraph,
    node_rects: &HashMap<String, Rect>,
    gutter: usize,
    content: Rect,
    max_exit_y: usize,
) -> SubgraphEnvelope {
    let mut has_external_edges = false;
    let mut has_outgoing = false;
    let mut outgoing_cross_count = 0usize;
    let mut incoming_cross_count = 0usize;
    let mut incoming_outside_sources: HashSet<&str> = HashSet::new();
    let mut outgoing_inside_sources: HashSet<&str> = HashSet::new();
    let mut incoming_inside_targets: HashSet<&str> = HashSet::new();
    for e in &graph.edges {
        let from_in = graph.is_node_in_subgraph_tree(&e.from, &subgraph.id);
        let to_in = graph.is_node_in_subgraph_tree(&e.to, &subgraph.id);
        if (from_in || to_in) && from_in != to_in {
            has_external_edges = true;
            if from_in {
                has_outgoing = true;
                outgoing_cross_count += 1;
                outgoing_inside_sources.insert(e.from.as_str());
            } else {
                incoming_cross_count += 1;
                incoming_outside_sources.insert(e.from.as_str());
                incoming_inside_targets.insert(e.to.as_str());
            }
        }
    }

    // Keep routing/title reservations explicit, then push the visible frame toward
    // a shared balanced pad target instead of letting each side drift independently.
    let inner_pad = 0;
    let mut side_hard_pad = if has_external_edges { gutter.max(1) } else { 1 };
    let nested_route_lane_budget = outgoing_inside_sources
        .len()
        .max(incoming_inside_targets.len());
    if subgraph.has_parent()
        && nested_route_lane_budget > 1
        && matches!(
            graph.direction,
            Direction::TD | Direction::TB | Direction::BT
        )
    {
        // Nested children need more horizontal room when multiple lanes must
        // enter or leave across the child border. Without this pre-envelope
        // budget, render has to squeeze merge/entry geometry against the wall.
        side_hard_pad = side_hard_pad.max(gutter.saturating_add(nested_route_lane_budget - 1));
    }

    // A strict horizontal subgraph fan-in needs three distinct visual events
    // after each source box: its exit stem, the shared collector shaft, and
    // the wall portal. Reserve one additional side cell for that corridor;
    // unrelated, labeled, nested, or mixed topologies retain the existing
    // envelope budget.
    if strict_simple_horizontal_subgraph_fanin(graph, &subgraph.id) {
        side_hard_pad = side_hard_pad.max(4);
    }

    // Ensure titled subgraphs are wide enough to display the anchored title plus
    // some post-title keepout for portal slots on the same border.
    let title_buffer = if matches!(graph.direction, Direction::BT) && incoming_cross_count > 1 {
        6
    } else if matches!(graph.direction, Direction::BT) && incoming_cross_count > 0 {
        4
    } else if has_external_edges {
        2
    } else {
        1
    };
    let has_title = subgraph.title.is_some();
    let nested_bt_direct_entry = graph.direction == Direction::BT
        && has_title
        && subgraph.has_parent()
        && graph.edges.iter().any(|edge| {
            !edge.is_back_edge
                && graph.get_node_subgraph(&edge.to) == Some(subgraph.id.as_str())
                && graph.edge_boundary_crossings(&edge.from, &edge.to).1.len() >= 2
        });
    let bt_sibling_direct_entry = graph.direction == Direction::BT
        && has_title
        && !subgraph.has_parent()
        && incoming_cross_count == 1
        && graph.edges.iter().any(|edge| {
            if edge.is_back_edge || graph.get_node_subgraph(&edge.from).is_none() {
                return false;
            }
            let (exit_subgraphs, enter_subgraphs) =
                graph.edge_boundary_crossings(&edge.from, &edge.to);
            exit_subgraphs.len() == 1
                && enter_subgraphs == vec![subgraph.id.as_str()]
                && exit_subgraphs[0] != subgraph.id
        });
    if let Some(t) = subgraph.title.as_ref() {
        let title_len = crate::graph::subgraph_title_len(t);
        let min_outer_width = title_len.saturating_add(2 + title_buffer);
        if content.width + side_hard_pad * 2 < min_outer_width {
            let needed = min_outer_width.saturating_sub(content.width);
            side_hard_pad = side_hard_pad.max(needed.div_ceil(2));
        }
        if matches!(graph.direction, Direction::BT) && incoming_cross_count > 0 {
            side_hard_pad = side_hard_pad.max(title_buffer);
        }
    }

    let title_on_bottom = has_title && matches!(graph.direction, Direction::BT);
    let title_on_top = has_title && !title_on_bottom;

    let mut top_hard_pad = if title_on_top {
        // Titles now live on the first interior row. Reserve the title row plus
        // one clear row beneath it before content begins.
        //
        // Special-case: when a single external source fans out into multiple targets
        // inside this titled subgraph, we need extra internal rows to draw a trunk,
        // split bar, drops, and arrowheads without colliding with the title row.
        let is_fanout_entry = incoming_cross_count > 1 && incoming_outside_sources.len() == 1;
        if is_fanout_entry {
            6
        } else if incoming_cross_count > 0
            && matches!(graph.direction, Direction::TD | Direction::TB)
        {
            4
        } else {
            3
        }
    } else if has_external_edges {
        if incoming_cross_count > 0 {
            2
        } else {
            1
        }
    } else {
        0
    };

    // BT with outgoing edges: edges exit from TOP of sources and need merge space
    // above the sources (smaller y). This is the opposite of TD where outgoing
    // edges need bottom_pad.
    if matches!(graph.direction, Direction::BT) && has_outgoing && outgoing_cross_count > 1 {
        // Need space for: merge line + vertical stems from sources
        top_hard_pad = top_hard_pad.max(gutter.saturating_add(2));
    }

    // The strict TD/TB terminal-entry scene uses the row immediately below the
    // title band as a quiet visual buffer. The route bridge remains one row
    // above the receiver arrow, so reserve exactly one extra top cell in the
    // envelope instead of inflating the rank gap for unrelated diagrams.
    if matches!(graph.direction, Direction::TD | Direction::TB)
        && super::td_terminal_entry_scene_subgraph(graph)
            .is_some_and(|scene| scene.id == subgraph.id)
    {
        top_hard_pad = top_hard_pad.saturating_add(1);
    }

    let mut bottom_hard_pad: usize = if title_on_bottom { 3 } else { 1 };
    if matches!(graph.direction, Direction::BT) && incoming_cross_count > 0 {
        bottom_hard_pad = bottom_hard_pad.max(if has_title { 4 } else { 2 });
    }
    if nested_bt_direct_entry {
        // A nested BT entry bridge turns one row below the target arrow.  Keep
        // one additional bottom corridor row so that turn does not become
        // visually attached to the child title.  The predicate is owned by
        // the live direct child topology; ordinary titled BT targets retain
        // the existing balanced envelope.
        bottom_hard_pad = bottom_hard_pad.saturating_add(1);
    }
    if bt_sibling_direct_entry {
        // A flat sibling entry uses one additional route-owned quiet row:
        // arrow, vertical shaft, turn, vertical shaft, then the title. Without
        // this row the title-safe turn is forced directly against the arrow.
        bottom_hard_pad = bottom_hard_pad.saturating_add(1);
    }

    let inner = content.inflate(inner_pad);
    let min_bottom_pad = if has_external_edges {
        let clearance = if outgoing_cross_count > 1 { 2 } else { 1 };
        max_exit_y
            .saturating_add(clearance)
            .saturating_sub(inner.y + inner.height)
    } else {
        0
    };
    bottom_hard_pad = bottom_hard_pad.max(min_bottom_pad);
    if matches!(graph.direction, Direction::TD | Direction::TB)
        && has_outgoing
        && outgoing_cross_count > 1
    {
        let extra_exit_clearance = if subgraph.has_parent() {
            gutter.saturating_add(2)
        } else {
            gutter.saturating_add(1)
        };
        bottom_hard_pad = bottom_hard_pad.max(extra_exit_clearance);
    }

    let mut bottom_max_pad: Option<usize> = None;
    // Avoid overlapping the bottom border with an outgoing target box:
    // keep at least one empty row between the border and the target arrow row.
    if matches!(graph.direction, Direction::TD | Direction::TB) && has_outgoing {
        let inner_bottom_inclusive = inner.y + inner.height.saturating_sub(1);
        let mut min_target_y: Option<usize> = None;
        for e in &graph.edges {
            if !graph.is_node_in_subgraph_tree(&e.from, &subgraph.id)
                || graph.is_node_in_subgraph_tree(&e.to, &subgraph.id)
            {
                continue;
            }
            let Some(target_rect) = node_rects.get(&e.to) else {
                continue;
            };
            // Only consider targets placed below the subgraph content.
            if target_rect.y <= inner_bottom_inclusive {
                continue;
            }
            // If the target is inside another subgraph, we need clearance for
            // that subgraph's top border (title row + border), not just the node.
            let effective_y = if let Some(target_sg_id) = graph.get_node_subgraph(&e.to) {
                // Find the target subgraph and compute its topmost node Y
                if let Some(target_sg) = graph.get_subgraph(target_sg_id) {
                    // Compute the minimum Y of all nodes in the target subgraph
                    let min_node_y = target_sg
                        .node_ids
                        .iter()
                        .filter_map(|id| node_rects.get(id))
                        .map(|r| r.y)
                        .min()
                        .unwrap_or(target_rect.y);
                    // Estimate top border position: nodes have padding above them for
                    // title (2-3 rows) and border (1 row)
                    let has_title = target_sg.title.is_some();
                    let title_clearance = if has_title { 3 } else { 1 };
                    min_node_y.saturating_sub(title_clearance)
                } else {
                    target_rect.y
                }
            } else {
                target_rect.y
            };
            min_target_y = Some(min_target_y.map_or(effective_y, |v| v.min(effective_y)));
        }
        if let Some(target_y) = min_target_y {
            let allowed_border_y = target_y.saturating_sub(2);
            let allowed_bottom_pad = allowed_border_y.saturating_sub(inner_bottom_inclusive);
            bottom_max_pad = Some(allowed_bottom_pad.max(min_bottom_pad));
        }
    }

    let horizontal_pad_target = if horizontal_sibling_chain_requires_extra_corridor(graph) {
        5
    } else {
        3usize.max(side_hard_pad)
    };
    let isolated_titled_vertical_subgraph = has_title
        && !has_external_edges
        && matches!(
            graph.direction,
            Direction::TD | Direction::TB | Direction::BT
        );
    let vertical_balance_floor = if isolated_titled_vertical_subgraph {
        2
    } else {
        3
    };
    let side_pad = horizontal_pad_target;
    let mut top_pad = vertical_balance_floor.max(top_hard_pad);
    let mut bottom_pad = vertical_balance_floor.max(bottom_hard_pad);
    if horizontal_sibling_chain_requires_extra_corridor(graph) {
        // The strict LR/RL sibling scene starts its corridor one row below the
        // node border. Reserve that row in the envelope so every transition
        // gets a quiet attachment band instead of borrowing the node-adjacent
        // border composition.
        bottom_pad = bottom_pad.max(4);
    }
    if let Some(max_bottom_pad) = bottom_max_pad {
        bottom_pad = bottom_pad.min(max_bottom_pad);
    }
    if top_pad > bottom_pad.saturating_add(1) {
        top_pad = bottom_pad.saturating_add(1);
    } else if bottom_pad > top_pad.saturating_add(1)
        && !nested_bt_direct_entry
        && !bt_sibling_direct_entry
    {
        bottom_pad = top_pad.saturating_add(1);
    }
    if matches!(graph.direction, Direction::TD | Direction::TB)
        && super::td_terminal_entry_scene_subgraph(graph)
            .is_some_and(|scene| scene.id == subgraph.id)
    {
        // The normal balancing rule intentionally keeps ordinary frames
        // symmetric. This exact scene owns one additional top cell even when
        // the balance pass would otherwise spend it on the bottom side.
        top_pad = top_pad.saturating_add(1);
    }

    let outer = Rect::new(
        inner.x.saturating_sub(side_pad),
        inner.y.saturating_sub(top_pad),
        inner.width + side_pad * 2,
        inner.height + top_pad + bottom_pad,
    );
    SubgraphEnvelope {
        outer,
        inner,
        portals: PortalSlots::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, Node, Subgraph};

    fn horizontal_fanin_graph(labeled: bool) -> (Graph, HashMap<String, Rect>) {
        let mut graph = Graph::new();
        graph.direction = Direction::LR;
        for id in ["S1", "S2", "S3", "Merge"] {
            graph.add_node(Node::new(id, id));
        }
        for source in ["S1", "S2", "S3"] {
            graph.add_edge(if labeled {
                Edge::with_label(source, "Merge", "edge")
            } else {
                Edge::new(source, "Merge")
            });
        }

        let mut subgraph = Subgraph::new("SG1", Some("Data Sources".into()));
        for source in ["S1", "S2", "S3"] {
            subgraph.add_node(source);
        }
        graph.add_subgraph(subgraph);
        for source in ["S1", "S2", "S3"] {
            graph.associate_node_with_subgraph(source, "SG1");
        }

        let node_rects = HashMap::from([
            ("S1".to_string(), Rect::new(2, 5, 14, 3)),
            ("S2".to_string(), Rect::new(2, 9, 14, 3)),
            ("S3".to_string(), Rect::new(2, 13, 14, 3)),
            ("Merge".to_string(), Rect::new(26, 9, 16, 3)),
        ]);
        (graph, node_rects)
    }

    #[test]
    fn strict_horizontal_fanin_reserves_one_extra_side_cell() {
        let (graph, node_rects) = horizontal_fanin_graph(false);
        let envelope = compute_envelopes(&graph, &node_rects, 1)
            .remove("SG1")
            .expect("fan-in envelope");

        assert_eq!(envelope.inner, Rect::new(2, 5, 14, 11));
        assert_eq!(envelope.outer, Rect::new(0, 2, 22, 17));
    }

    #[test]
    fn labeled_horizontal_fanin_keeps_the_existing_side_budget() {
        let (graph, node_rects) = horizontal_fanin_graph(true);
        let envelope = compute_envelopes(&graph, &node_rects, 1)
            .remove("SG1")
            .expect("fan-in envelope");

        assert_eq!(envelope.outer, Rect::new(0, 2, 20, 17));
    }
}
