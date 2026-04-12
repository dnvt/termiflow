//! Shared subgraph envelope + portal helpers for layout and render.
//!
//! Provides a single source of truth for:
//! - Subgraph inner/outer rectangles (with gutters)
//! - Portal slots per side derived from crossing edges
//! - Helpers to build node rects from a laid-out graph

use std::collections::{HashMap, HashSet};

use crate::geom::Rect;
use crate::graph::{Direction, Graph};

/// Portal coordinates along each side of a subgraph border.
#[derive(Debug, Clone, Default)]
pub struct PortalSlots {
    pub top: HashSet<usize>,
    pub bottom: HashSet<usize>,
    pub left: HashSet<usize>,
    pub right: HashSet<usize>,
}

/// Combined inner/outer bounds with portals.
#[derive(Debug, Clone)]
pub struct SubgraphEnvelope {
    pub outer: Rect,
    pub inner: Rect,
    pub portals: PortalSlots,
}

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

fn top_level_connected_subgraph_components<'a>(graph: &'a Graph) -> Vec<Vec<&'a str>> {
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

fn current_node_rect(
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

fn current_subgraph_bounds(
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

    let mut bottom_hard_pad = if title_on_bottom { 3 } else { 1 };
    if matches!(graph.direction, Direction::BT) && incoming_cross_count > 0 {
        bottom_hard_pad = bottom_hard_pad.max(if has_title { 4 } else { 2 });
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

    let horizontal_pad_target = 3usize.max(side_hard_pad);
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
    if let Some(max_bottom_pad) = bottom_max_pad {
        bottom_pad = bottom_pad.min(max_bottom_pad);
    }
    if top_pad > bottom_pad.saturating_add(1) {
        top_pad = bottom_pad.saturating_add(1);
    } else if bottom_pad > top_pad.saturating_add(1) {
        bottom_pad = top_pad.saturating_add(1);
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

/// Shared portal slot discovery (used by layout + render).
pub fn collect_portal_slots(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    direction: Direction,
) -> HashMap<String, PortalSlots> {
    collect_portal_slots_with_bounds(graph, node_rects, direction, None)
}

fn collect_portal_slots_with_bounds(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    direction: Direction,
    current_bounds: Option<&HashMap<String, Rect>>,
) -> HashMap<String, PortalSlots> {
    let mut slots: HashMap<String, PortalSlots> = HashMap::new();
    let mut shared_td_fanout_top_slots: HashMap<(String, String), usize> = HashMap::new();
    let mut shared_td_fanin_bottom_slots: HashMap<(String, String), usize> = HashMap::new();
    let mut shared_horizontal_fanin_side_slots: HashMap<(String, String), usize> = HashMap::new();

    let shift_x_out_of_title = |sg_id: &str, desired_x: usize| -> usize {
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
        let Some((start, end)) =
            crate::graph::subgraph_title_span(bounds.x, bounds.width, title, graph.direction)
        else {
            return desired_x;
        };
        let min_x = bounds.x.saturating_add(1);
        let max_x = bounds.x.saturating_add(bounds.width.saturating_sub(2));
        if max_x < min_x {
            return desired_x;
        }
        let protected_start = start.saturating_sub(2);
        let protected_end = end.saturating_add(2).min(max_x);
        let x = desired_x.clamp(min_x, max_x);
        if x < protected_start || x > protected_end {
            return x;
        }
        if graph.direction == Direction::BT {
            let left = (protected_start > min_x).then(|| protected_start.saturating_sub(1));
            let right = (protected_end < max_x).then(|| protected_end + 1);
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
        } else if protected_end < max_x {
            protected_end + 1
        } else if protected_start > min_x {
            protected_start.saturating_sub(1)
        } else {
            x
        }
    };

    let bt_nudge_from_corners = |sg_id: &str, x: usize| -> usize {
        let Some(sg) = graph.get_subgraph(sg_id) else {
            return x;
        };
        let Some(bounds) = current_subgraph_bounds(graph, current_bounds, sg_id) else {
            return x;
        };
        let Some(title) = sg.title.as_deref() else {
            return x;
        };
        if bounds.is_empty() {
            return x;
        }
        let min = bounds.x.saturating_add(1);
        let max = bounds.x.saturating_add(bounds.width.saturating_sub(2));
        if max <= min {
            return x;
        }
        let Some((start, end)) =
            crate::graph::subgraph_title_span(bounds.x, bounds.width, title, graph.direction)
        else {
            return x;
        };
        let in_title_text = |pos: usize| pos >= start && pos <= end;
        if x == min {
            let candidate = min.saturating_add(1);
            if candidate <= max && !in_title_text(candidate) {
                return candidate;
            }
        } else if x == max {
            let candidate = max.saturating_sub(1);
            if candidate >= min && !in_title_text(candidate) {
                return candidate;
            }
        }
        x
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
            let shared_x = target_center_x
                .clamp(min_source_x, max_source_x)
                .clamp(min_x, max_x.max(min_x));
            shared_td_fanin_bottom_slots.insert((to_id, sg_id), shared_x);
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
            let portal_y = ((min_source_y + max_source_y) / 2).clamp(
                bounds.y.saturating_add(1),
                bounds.y + bounds.height.saturating_sub(2),
            );
            shared_horizontal_fanin_side_slots.insert((to_id, sg_id), portal_y);
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

                    let mut x = node_center_x(node_rects, &edge.to, to);
                    if let Some(&shared_x) =
                        shared_td_fanout_top_slots.get(&(edge.from.clone(), id.to_string()))
                    {
                        x = shared_x;
                    } else {
                        x = shift_x_out_of_title(id, x);
                    }
                    slots.entry(id.to_string()).or_default().top.insert(x);
                }
                for id in exit_subgraphs {
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
                    let slot_x = shared_td_fanin_bottom_slots
                        .get(&(edge.to.clone(), id.to_string()))
                        .copied()
                        .unwrap_or_else(|| node_center_x(node_rects, &edge.from, from));
                    slots
                        .entry(id.to_string())
                        .or_default()
                        .bottom
                        .insert(slot_x);
                }
            }
            Direction::BT => {
                let nested_bt_entry = enter_subgraphs.len() > 1;
                for id in enter_subgraphs {
                    if nested_bt_entry {
                        continue;
                    }
                    let mut x = node_center_x(node_rects, &edge.to, to);
                    x = shift_x_out_of_title(id, x);
                    x = bt_nudge_from_corners(id, x);
                    slots.entry(id.to_string()).or_default().bottom.insert(x);
                }
                for id in exit_subgraphs {
                    let mut x = node_center_x(node_rects, &edge.from, from);
                    x = shift_x_out_of_title(id, x);
                    x = bt_nudge_from_corners(id, x);
                    slots.entry(id.to_string()).or_default().top.insert(x);
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
                    let slot_y = shared_horizontal_fanin_side_slots
                        .get(&(edge.to.clone(), id.to_string()))
                        .copied()
                        .unwrap_or_else(|| node_center_y(node_rects, &edge.from, from));
                    slots
                        .entry(id.to_string())
                        .or_default()
                        .right
                        .insert(slot_y);
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
                    let slot_y = shared_horizontal_fanin_side_slots
                        .get(&(edge.to.clone(), id.to_string()))
                        .copied()
                        .unwrap_or_else(|| node_center_y(node_rects, &edge.from, from));
                    slots.entry(id.to_string()).or_default().left.insert(slot_y);
                }
            }
        }
    }

    slots
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
    use crate::graph::{Edge, Node, Subgraph};

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
