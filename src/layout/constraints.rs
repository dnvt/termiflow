//! Subgraph containment, route-pressure, and rebalancing constraints.

use std::collections::{HashMap, HashSet};

use crate::geom::{Point, Rect};
use crate::graph::{Direction, Graph};
use crate::portals::{
    compute_envelopes, horizontal_sibling_chain_requires_extra_corridor, SubgraphEnvelope,
    HORIZONTAL_SIBLING_CHAIN_MIN_INTER_GAP,
};

use super::CoarseLayoutConfig;

pub(super) fn rect_fully_inside(outer: Rect, inner: Rect) -> bool {
    if outer.is_empty() || inner.is_empty() {
        return false;
    }
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.right() <= outer.right()
        && inner.bottom() <= outer.bottom()
}

pub(super) fn rects_overlap_vertically(a: Rect, b: Rect) -> bool {
    a.y < b.bottom() && b.y < a.bottom()
}

pub(super) fn rects_overlap_horizontally(a: Rect, b: Rect) -> bool {
    a.x < b.right() && b.x < a.right()
}

pub(super) fn rect_center_x(rect: Rect) -> usize {
    rect.x + rect.width / 2
}

pub(super) fn subgraph_tree_rank_range(
    graph: &Graph,
    ranks: &HashMap<String, usize>,
    subgraph_id: &str,
) -> Option<(usize, usize)> {
    if let Some(subgraph) = graph.get_subgraph(subgraph_id) {
        let mut direct_min: Option<usize> = None;
        let mut direct_max: Option<usize> = None;
        for node_id in &subgraph.node_ids {
            let Some(rank) = ranks.get(node_id) else {
                continue;
            };
            direct_min = Some(direct_min.map_or(*rank, |current| current.min(*rank)));
            direct_max = Some(direct_max.map_or(*rank, |current| current.max(*rank)));
        }
        if let (Some(min_rank), Some(max_rank)) = (direct_min, direct_max) {
            return Some((min_rank, max_rank));
        }
    }

    let mut min_rank: Option<usize> = None;
    let mut max_rank: Option<usize> = None;

    for (node_id, rank) in ranks {
        if !graph.is_node_in_subgraph_tree(node_id, subgraph_id) {
            continue;
        }
        min_rank = Some(min_rank.map_or(*rank, |current| current.min(*rank)));
        max_rank = Some(max_rank.map_or(*rank, |current| current.max(*rank)));
    }

    Some((min_rank?, max_rank?))
}

pub(super) fn subgraphs_have_declared_hierarchy(
    graph: &Graph,
    left_id: &str,
    right_id: &str,
) -> bool {
    graph.is_subgraph_ancestor(left_id, right_id) || graph.is_subgraph_ancestor(right_id, left_id)
}

pub(super) fn is_vertical_flow(direction: Direction) -> bool {
    matches!(direction, Direction::TD | Direction::TB)
}

pub(super) fn route_budgeted_subgraphs(graph: &Graph) -> Vec<String> {
    let mut subgraph_ids: Vec<String> = graph
        .subgraphs
        .iter()
        .filter(|subgraph| subgraph.parent_id.is_some())
        .map(|subgraph| subgraph.id.clone())
        .collect();
    subgraph_ids.sort();
    subgraph_ids.dedup();
    subgraph_ids
}

pub(super) fn top_level_subgraph_id(graph: &Graph, subgraph_id: &str) -> String {
    let mut current = subgraph_id.to_string();
    while let Some(parent_id) = graph
        .get_subgraph(&current)
        .and_then(|subgraph| subgraph.parent_id.as_deref())
    {
        current = parent_id.to_string();
    }
    current
}

pub(super) fn top_level_subgraph_components(graph: &Graph) -> Vec<Vec<String>> {
    let top_level_ids: Vec<String> = graph
        .subgraphs
        .iter()
        .filter(|subgraph| subgraph.parent_id.is_none())
        .map(|subgraph| subgraph.id.clone())
        .collect();
    if top_level_ids.len() < 2 {
        return Vec::new();
    }

    let top_level_set: HashSet<String> = top_level_ids.iter().cloned().collect();
    let mut adjacency: HashMap<String, HashSet<String>> = top_level_ids
        .iter()
        .cloned()
        .map(|id| (id, HashSet::new()))
        .collect();

    for edge in graph.edges.iter().filter(|edge| !edge.is_back_edge) {
        let Some(from_sg) = graph.get_node_subgraph(&edge.from) else {
            continue;
        };
        let Some(to_sg) = graph.get_node_subgraph(&edge.to) else {
            continue;
        };

        let from_top = top_level_subgraph_id(graph, from_sg);
        let to_top = top_level_subgraph_id(graph, to_sg);
        if from_top == to_top
            || !top_level_set.contains(&from_top)
            || !top_level_set.contains(&to_top)
        {
            continue;
        }

        adjacency
            .entry(from_top.clone())
            .or_default()
            .insert(to_top.clone());
        adjacency.entry(to_top).or_default().insert(from_top);
    }

    let mut visited: HashSet<String> = HashSet::new();
    let mut components = Vec::new();
    for start_id in top_level_ids {
        if !visited.insert(start_id.clone()) {
            continue;
        }

        let mut stack = vec![start_id];
        let mut component = Vec::new();
        while let Some(current) = stack.pop() {
            component.push(current.clone());
            if let Some(neighbors) = adjacency.get(&current) {
                for next in neighbors {
                    if visited.insert(next.clone()) {
                        stack.push(next.clone());
                    }
                }
            }
        }

        if component.len() > 1 {
            components.push(component);
        }
    }

    components
}

pub(super) fn compact_stacked_vertical_top_level_sibling_subgraphs(
    graph: &Graph,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    gutter: usize,
    canvas_height: &mut usize,
) {
    if !matches!(graph.direction, Direction::TD | Direction::TB) || graph.subgraphs.is_empty() {
        return;
    }

    let components = top_level_subgraph_components(graph);
    if components.is_empty() {
        return;
    }

    const TARGET_BORDER_GAP: usize = 4;
    const MIN_BORDER_GAP: usize = 1;

    for _ in 0..8 {
        let envelopes = compute_envelopes(graph, node_rects, gutter);
        let mut best_shift: Option<(String, usize)> = None;

        for component in &components {
            let mut ordered: Vec<(&str, Rect)> = component
                .iter()
                .filter_map(|subgraph_id| {
                    envelopes
                        .get(subgraph_id)
                        .map(|env| (subgraph_id.as_str(), env.outer))
                })
                .collect();
            if ordered.len() < 2 {
                continue;
            }

            ordered.sort_by_key(|(_, outer)| outer.y);
            for pair in ordered.windows(2) {
                let (_upper_id, upper_outer) = pair[0];
                let (lower_id, lower_outer) = pair[1];

                if !rects_overlap_horizontally(upper_outer, lower_outer)
                    || rect_fully_inside(upper_outer, lower_outer)
                    || rect_fully_inside(lower_outer, upper_outer)
                {
                    continue;
                }

                let current_gap = lower_outer.y.saturating_sub(upper_outer.bottom());
                if current_gap <= TARGET_BORDER_GAP {
                    continue;
                }

                let mut allowed_shift = current_gap.saturating_sub(TARGET_BORDER_GAP);
                allowed_shift = allowed_shift.min(
                    lower_outer
                        .y
                        .saturating_sub(upper_outer.bottom().saturating_add(MIN_BORDER_GAP)),
                );
                if allowed_shift == 0 {
                    continue;
                }

                let mut incoming_count_by_source: HashMap<&str, usize> = HashMap::new();
                for edge in graph.edges.iter().filter(|edge| !edge.is_back_edge) {
                    if graph.is_node_in_subgraph_tree(&edge.from, lower_id)
                        || !graph.is_node_in_subgraph_tree(&edge.to, lower_id)
                    {
                        continue;
                    }
                    *incoming_count_by_source
                        .entry(edge.from.as_str())
                        .or_default() += 1;
                }

                for edge in graph.edges.iter().filter(|edge| !edge.is_back_edge) {
                    if graph.is_node_in_subgraph_tree(&edge.from, lower_id)
                        || !graph.is_node_in_subgraph_tree(&edge.to, lower_id)
                    {
                        continue;
                    }
                    let Some(source_rect) = node_rects.get(&edge.from) else {
                        continue;
                    };
                    let clearance = if incoming_count_by_source
                        .get(edge.from.as_str())
                        .copied()
                        .unwrap_or(1)
                        > 1
                    {
                        2
                    } else {
                        1
                    };
                    let required_outer_y = source_rect.bottom().saturating_add(clearance);
                    allowed_shift =
                        allowed_shift.min(lower_outer.y.saturating_sub(required_outer_y));
                    if allowed_shift == 0 {
                        break;
                    }
                }
                if allowed_shift == 0 {
                    continue;
                }

                let candidate_outer = Rect::new(
                    lower_outer.x,
                    lower_outer.y.saturating_sub(allowed_shift),
                    lower_outer.width,
                    lower_outer.height,
                );
                let overlaps_foreign_node = node_rects.iter().any(|(node_id, rect)| {
                    !graph.is_node_in_subgraph_tree(node_id, lower_id)
                        && rects_overlap_horizontally(*rect, candidate_outer)
                        && rects_overlap_vertically(*rect, candidate_outer)
                });
                if overlaps_foreign_node {
                    continue;
                }

                let candidate_gap = candidate_outer.y.saturating_sub(upper_outer.bottom());
                if candidate_gap < MIN_BORDER_GAP {
                    continue;
                }

                if best_shift
                    .as_ref()
                    .is_none_or(|(_, best_delta)| allowed_shift > *best_delta)
                {
                    best_shift = Some((lower_id.to_string(), allowed_shift));
                }
            }
        }

        let Some((subgraph_id, delta_y)) = best_shift else {
            break;
        };

        shift_nodes_in_subgraph_tree_y_signed(
            graph,
            positions,
            node_rects,
            &subgraph_id,
            -(delta_y as isize),
        );
        *canvas_height = node_rects
            .values()
            .map(|rect| rect.bottom())
            .max()
            .unwrap_or(*canvas_height);
    }
}

pub(super) fn enforce_declared_nested_envelopes(
    graph: &Graph,
    subgraph_envelopes: &mut HashMap<String, SubgraphEnvelope>,
) {
    let mut nested_subgraphs: Vec<_> = graph
        .subgraphs
        .iter()
        .filter(|subgraph| subgraph.parent_id.is_some())
        .collect();
    nested_subgraphs.sort_by_key(|subgraph| {
        let mut depth = 0usize;
        let mut current = subgraph.parent_id.as_deref();
        while let Some(parent_id) = current {
            depth += 1;
            current = graph
                .get_subgraph(parent_id)
                .and_then(|parent| parent.parent_id.as_deref());
        }
        std::cmp::Reverse(depth)
    });

    for subgraph in nested_subgraphs {
        let Some(parent_id) = subgraph.parent_id.as_deref() else {
            continue;
        };
        let Some(child_env) = subgraph_envelopes.get(&subgraph.id).cloned() else {
            continue;
        };
        let Some(parent_env) = subgraph_envelopes.get_mut(parent_id) else {
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

pub(super) fn shift_nodes_from_rank_td(
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    ranks: &HashMap<String, usize>,
    min_rank: usize,
    delta_y: usize,
) {
    if delta_y == 0 {
        return;
    }
    for (id, p) in positions.iter_mut() {
        let Some(rank) = ranks.get(id) else {
            continue;
        };
        if *rank < min_rank {
            continue;
        }
        p.y += delta_y;
        if let Some(r) = node_rects.get_mut(id) {
            r.y += delta_y;
        }
    }
}

pub(super) fn shift_nodes_in_subgraph(
    graph: &Graph,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    subgraph_id: &str,
    delta_x: usize,
) {
    if delta_x == 0 {
        return;
    }
    let Some(sg) = graph.get_subgraph(subgraph_id) else {
        return;
    };
    for node_id in &sg.node_ids {
        if let Some(p) = positions.get_mut(node_id) {
            p.x += delta_x;
        }
        if let Some(r) = node_rects.get_mut(node_id) {
            r.x += delta_x;
        }
    }
}

pub(super) fn shift_nodes_in_subgraph_y(
    graph: &Graph,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    subgraph_id: &str,
    delta_y: usize,
) {
    if delta_y == 0 {
        return;
    }
    let Some(sg) = graph.get_subgraph(subgraph_id) else {
        return;
    };
    for node_id in &sg.node_ids {
        if let Some(p) = positions.get_mut(node_id) {
            p.y += delta_y;
        }
        if let Some(r) = node_rects.get_mut(node_id) {
            r.y += delta_y;
        }
    }
}

pub(super) fn shift_nodes_in_subgraph_tree_y_signed(
    graph: &Graph,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    subgraph_id: &str,
    delta_y: isize,
) {
    if delta_y == 0 {
        return;
    }
    for (node_id, p) in positions.iter_mut() {
        if !graph.is_node_in_subgraph_tree(node_id, subgraph_id) {
            continue;
        }
        let next_y = if delta_y.is_negative() {
            p.y.saturating_sub(delta_y.unsigned_abs())
        } else {
            p.y.saturating_add(delta_y as usize)
        };
        p.y = next_y;
        if let Some(r) = node_rects.get_mut(node_id) {
            r.y = next_y;
        }
    }
}

pub(super) fn reserve_nested_horizontal_subgraph_headroom(
    graph: &Graph,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    gutter: usize,
    canvas_height: &mut usize,
) {
    if !matches!(graph.direction, Direction::LR | Direction::RL) || graph.subgraphs.is_empty() {
        return;
    }

    if !graph
        .subgraphs
        .iter()
        .any(|subgraph| subgraph.parent_id.is_some())
    {
        return;
    }

    // Declared nested LR/RL stacks can saturate at y=0 before successive titled
    // envelopes have enough room to staircase their title rows. Recompute
    // envelopes and add only the minimum extra headroom until every parent/child
    // pair occupies a distinct top row.
    for _ in 0..16 {
        let envelopes = compute_envelopes(graph, node_rects, gutter);
        let needs_shift = graph.subgraphs.iter().any(|subgraph| {
            let Some(parent_id) = subgraph.parent_id.as_deref() else {
                return false;
            };
            let (Some(parent_env), Some(child_env)) =
                (envelopes.get(parent_id), envelopes.get(&subgraph.id))
            else {
                return false;
            };
            child_env.outer.y <= parent_env.outer.y
        });
        if !needs_shift {
            break;
        }

        for point in positions.values_mut() {
            point.y += 1;
        }
        for rect in node_rects.values_mut() {
            rect.y += 1;
        }
        *canvas_height = canvas_height.saturating_add(1);
    }
}

pub(super) fn reserve_titled_horizontal_subgraph_headroom(
    graph: &Graph,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    gutter: usize,
    canvas_height: &mut usize,
) {
    if !matches!(graph.direction, Direction::LR | Direction::RL) || graph.subgraphs.is_empty() {
        return;
    }

    let required_shift = compute_envelopes(graph, node_rects, gutter)
        .into_iter()
        .filter_map(|(subgraph_id, env)| {
            let subgraph = graph.get_subgraph(&subgraph_id)?;
            if subgraph.parent_id.is_some() || subgraph.title.is_none() {
                return None;
            }
            let actual_top_pad = env.inner.y.saturating_sub(env.outer.y);
            let desired_top_pad = 3usize;
            Some(desired_top_pad.saturating_sub(actual_top_pad))
        })
        .max()
        .unwrap_or(0);

    if required_shift == 0 {
        return;
    }

    for point in positions.values_mut() {
        point.y += required_shift;
    }
    for rect in node_rects.values_mut() {
        rect.y += required_shift;
    }
    *canvas_height = canvas_height.saturating_add(required_shift);
}

pub(super) fn shift_nodes_in_subgraph_tree_x_signed(
    graph: &Graph,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    subgraph_id: &str,
    delta_x: isize,
) {
    if delta_x == 0 {
        return;
    }
    for (node_id, p) in positions.iter_mut() {
        if !graph.is_node_in_subgraph_tree(node_id, subgraph_id) {
            continue;
        }
        let next_x = if delta_x.is_negative() {
            p.x.saturating_sub(delta_x.unsigned_abs())
        } else {
            p.x.saturating_add(delta_x as usize)
        };
        p.x = next_x;
        if let Some(r) = node_rects.get_mut(node_id) {
            r.x = next_x;
        }
    }
}

pub(super) fn subgraph_depth_in_graph(graph: &Graph, subgraph_id: &str) -> usize {
    let mut depth = 0usize;
    let mut current = graph
        .get_subgraph(subgraph_id)
        .and_then(|subgraph| subgraph.parent_id.as_deref());
    while let Some(parent_id) = current {
        depth += 1;
        current = graph
            .get_subgraph(parent_id)
            .and_then(|parent| parent.parent_id.as_deref());
    }
    depth
}

pub(super) fn subgraph_has_cross_boundary_edges(graph: &Graph, subgraph_id: &str) -> bool {
    graph.edges.iter().any(|edge| {
        let from_in = graph.is_node_in_subgraph_tree(&edge.from, subgraph_id);
        let to_in = graph.is_node_in_subgraph_tree(&edge.to, subgraph_id);
        (from_in || to_in) && from_in != to_in
    })
}

pub(super) fn subgraph_is_top_level_leaf(graph: &Graph, subgraph_id: &str) -> bool {
    let Some(subgraph) = graph.get_subgraph(subgraph_id) else {
        return false;
    };
    subgraph.parent_id.is_none()
        && !graph
            .subgraphs
            .iter()
            .any(|candidate| candidate.parent_id.as_deref() == Some(subgraph_id))
}

pub(super) fn subgraph_has_overlapping_foreign_nodes(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    subgraph_id: &str,
    outer: Rect,
) -> bool {
    node_rects.iter().any(|(node_id, rect)| {
        !graph.is_node_in_subgraph_tree(node_id, subgraph_id)
            && rects_overlap_vertically(*rect, outer)
            && rect.right() > outer.x
            && rect.x < outer.right()
    })
}

pub(super) fn subgraph_can_rebalance_horizontal_content(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    subgraph_id: &str,
    outer: Rect,
) -> bool {
    if !subgraph_has_cross_boundary_edges(graph, subgraph_id) {
        return true;
    }

    subgraph_is_top_level_leaf(graph, subgraph_id)
        && !subgraph_has_overlapping_foreign_nodes(graph, node_rects, subgraph_id, outer)
}

pub(super) fn rebalance_titled_vertical_subgraph_content_x(
    graph: &Graph,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    gutter: usize,
    canvas_width: &mut usize,
) {
    if !matches!(
        graph.direction,
        Direction::TD | Direction::TB | Direction::BT
    ) || graph.subgraphs.is_empty()
    {
        return;
    }

    let mut titled_subgraph_ids: Vec<String> = graph
        .subgraphs
        .iter()
        .filter(|subgraph| subgraph.title.is_some())
        .map(|subgraph| subgraph.id.clone())
        .collect();
    titled_subgraph_ids
        .sort_by_key(|subgraph_id| std::cmp::Reverse(subgraph_depth_in_graph(graph, subgraph_id)));

    for _ in 0..16 {
        let envelopes = compute_envelopes(graph, node_rects, gutter);
        let Some((subgraph_id, delta_x)) = titled_subgraph_ids.iter().find_map(|subgraph_id| {
            let env = envelopes.get(subgraph_id)?;
            if !subgraph_can_rebalance_horizontal_content(graph, node_rects, subgraph_id, env.outer)
            {
                return None;
            }
            if env.outer.width <= 2 || env.inner.width >= env.outer.width.saturating_sub(2) {
                return None;
            }

            let left_pad = env.inner.x.saturating_sub(env.outer.x.saturating_add(1));
            let right_pad = env
                .outer
                .right()
                .saturating_sub(env.inner.right().saturating_add(1));
            if left_pad.abs_diff(right_pad) <= 1 {
                return None;
            }

            let available_inner_width = env.outer.width.saturating_sub(2);
            let target_inner_x = env
                .outer
                .x
                .saturating_add(1)
                .saturating_add((available_inner_width.saturating_sub(env.inner.width)) / 2);
            let delta_x = target_inner_x as isize - env.inner.x as isize;
            (delta_x != 0).then(|| (subgraph_id.clone(), delta_x))
        }) else {
            break;
        };

        shift_nodes_in_subgraph_tree_x_signed(graph, positions, node_rects, &subgraph_id, delta_x);

        let max_right = node_rects
            .values()
            .map(|rect| rect.right())
            .max()
            .unwrap_or(*canvas_width);
        *canvas_width = (*canvas_width).max(max_right);
    }
}

pub(super) fn rebalance_titled_vertical_subgraph_content_y(
    graph: &Graph,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    gutter: usize,
    canvas_height: &mut usize,
) {
    if !matches!(
        graph.direction,
        Direction::TD | Direction::TB | Direction::BT
    ) || graph.subgraphs.is_empty()
    {
        return;
    }

    let mut titled_subgraph_ids: Vec<String> = graph
        .subgraphs
        .iter()
        .filter(|subgraph| subgraph.title.is_some())
        .map(|subgraph| subgraph.id.clone())
        .collect();
    titled_subgraph_ids
        .sort_by_key(|subgraph_id| std::cmp::Reverse(subgraph_depth_in_graph(graph, subgraph_id)));

    for _ in 0..16 {
        let envelopes = compute_envelopes(graph, node_rects, gutter);
        let Some((subgraph_id, delta_y)) = titled_subgraph_ids.iter().find_map(|subgraph_id| {
            if subgraph_has_cross_boundary_edges(graph, subgraph_id) {
                return None;
            }

            let env = envelopes.get(subgraph_id)?;
            if env.outer.height <= 2 || env.inner.height >= env.outer.height.saturating_sub(2) {
                return None;
            }

            let top_pad = env.inner.y.saturating_sub(env.outer.y);
            let bottom_pad = env.outer.bottom().saturating_sub(env.inner.bottom());
            if top_pad.abs_diff(bottom_pad) <= 1 {
                return None;
            }

            let target_inner_y = env
                .outer
                .y
                .saturating_add((env.outer.height.saturating_sub(env.inner.height)) / 2);
            let delta_y = target_inner_y as isize - env.inner.y as isize;
            (delta_y != 0).then(|| (subgraph_id.clone(), delta_y))
        }) else {
            break;
        };

        shift_nodes_in_subgraph_tree_y_signed(graph, positions, node_rects, &subgraph_id, delta_y);

        let max_bottom = node_rects
            .values()
            .map(|rect| rect.bottom())
            .max()
            .unwrap_or(*canvas_height);
        *canvas_height = (*canvas_height).max(max_bottom);
    }
}

pub(super) fn shift_nodes_by_id_y(
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    node_shifts: &HashMap<String, usize>,
) {
    for (node_id, delta_y) in node_shifts {
        if *delta_y == 0 {
            continue;
        }
        if let Some(point) = positions.get_mut(node_id) {
            point.y += *delta_y;
        }
        if let Some(rect) = node_rects.get_mut(node_id) {
            rect.y += *delta_y;
        }
    }
}

pub(super) fn shift_nodes_from_x(
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    min_x: usize,
    delta_x: usize,
) {
    if delta_x == 0 {
        return;
    }
    for (id, rect) in node_rects.iter_mut() {
        if rect.x < min_x {
            continue;
        }
        rect.x += delta_x;
        if let Some(point) = positions.get_mut(id) {
            point.x += delta_x;
        }
    }
}

pub(super) fn shift_nodes_by_id_x(
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    node_shifts: &HashMap<String, usize>,
) {
    for (node_id, delta_x) in node_shifts {
        if *delta_x == 0 {
            continue;
        }
        if let Some(point) = positions.get_mut(node_id) {
            point.x += *delta_x;
        }
        if let Some(rect) = node_rects.get_mut(node_id) {
            rect.x += *delta_x;
        }
    }
}

pub(super) fn candidate_introduces_foreign_node_overlap_for_subgraph(
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

pub(super) fn flow_exit_gap_to_external_node(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    component_ids: &HashSet<String>,
    subgraph_id: &str,
    outer: Rect,
) -> Option<usize> {
    let mut best_gap: Option<usize> = None;

    for edge in graph.edges.iter().filter(|edge| !edge.is_back_edge) {
        let from_inside = graph.is_node_in_subgraph_tree(&edge.from, subgraph_id);
        let to_inside = graph.is_node_in_subgraph_tree(&edge.to, subgraph_id);
        if from_inside == to_inside {
            continue;
        }

        let external_node_id = if from_inside { &edge.to } else { &edge.from };
        if let Some(external_subgraph_id) = graph.get_node_subgraph(external_node_id) {
            let external_top_level_id = top_level_subgraph_id(graph, external_subgraph_id);
            if component_ids.contains(&external_top_level_id) {
                continue;
            }
        }

        let Some(external_rect) = node_rects.get(external_node_id).copied() else {
            continue;
        };

        let candidate_gap = match graph.direction {
            Direction::LR => {
                if external_rect.x < outer.right() {
                    continue;
                }
                external_rect.x.saturating_sub(outer.right())
            }
            Direction::RL => {
                if external_rect.right() > outer.x {
                    continue;
                }
                outer.x.saturating_sub(external_rect.right())
            }
            _ => continue,
        };

        best_gap = Some(best_gap.map_or(candidate_gap, |current| current.min(candidate_gap)));
    }

    best_gap
}

pub(super) fn rebalance_side_by_side_horizontal_top_level_sibling_gaps(
    graph: &Graph,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    gutter: usize,
    canvas_width: &mut usize,
) {
    if !matches!(graph.direction, Direction::LR | Direction::RL) || graph.subgraphs.is_empty() {
        return;
    }

    const MIN_VISUAL_GAP: usize = 2;
    const MIN_INTER_GAP: usize = 6;
    const IMBALANCE_TOLERANCE: usize = 2;

    for _ in 0..8 {
        let envelopes = compute_envelopes(graph, node_rects, gutter);
        let mut best_shift: Option<(String, isize, usize)> = None;

        let min_visual_gap = if horizontal_sibling_chain_requires_extra_corridor(graph) {
            HORIZONTAL_SIBLING_CHAIN_MIN_INTER_GAP
        } else {
            MIN_VISUAL_GAP
        };

        for component in top_level_subgraph_components(graph) {
            let component_ids: HashSet<String> = component.iter().cloned().collect();
            let mut ordered: Vec<(String, Rect)> = component
                .iter()
                .filter_map(|subgraph_id| {
                    envelopes
                        .get(subgraph_id)
                        .map(|env| (subgraph_id.clone(), env.outer))
                })
                .collect();
            if ordered.len() < 2 {
                continue;
            }

            ordered.sort_by_key(|(_, outer)| outer.x);
            let is_side_by_side_row = ordered.windows(2).all(|pair| {
                let left_outer = pair[0].1;
                let right_outer = pair[1].1;
                rects_overlap_vertically(left_outer, right_outer) && left_outer.x <= right_outer.x
            });
            if !is_side_by_side_row {
                continue;
            }

            for pair in ordered.windows(2) {
                let (_left_id, left_outer) = (&pair[0].0, pair[0].1);
                let (right_id, right_outer) = (&pair[1].0, pair[1].1);
                let required_right_x = left_outer.right().saturating_add(min_visual_gap);
                let delta_to_minimum = required_right_x.saturating_sub(right_outer.x);

                if delta_to_minimum > 0 {
                    let delta_x = delta_to_minimum;
                    let candidate = Rect::new(
                        right_outer.x.saturating_add(delta_x),
                        right_outer.y,
                        right_outer.width,
                        right_outer.height,
                    );
                    if candidate_introduces_foreign_node_overlap_for_subgraph(
                        graph,
                        node_rects,
                        right_id,
                        right_outer,
                        candidate,
                    ) {
                        continue;
                    }

                    if best_shift
                        .as_ref()
                        .is_none_or(|(_, _, best_delta)| delta_x > *best_delta)
                    {
                        best_shift = Some((right_id.clone(), delta_x as isize, delta_x));
                    }
                    continue;
                }

                let inter_gap = right_outer.x.saturating_sub(left_outer.right());
                if inter_gap <= MIN_INTER_GAP {
                    continue;
                }

                let Some(exit_gap) = flow_exit_gap_to_external_node(
                    graph,
                    node_rects,
                    &component_ids,
                    right_id,
                    right_outer,
                ) else {
                    continue;
                };
                if inter_gap <= exit_gap.saturating_add(IMBALANCE_TOLERANCE) {
                    continue;
                }

                let shift = inter_gap.saturating_sub(exit_gap).div_ceil(2);
                let max_shift = inter_gap.saturating_sub(MIN_INTER_GAP);
                let delta_x = shift.min(max_shift);
                if delta_x == 0 {
                    continue;
                }

                let candidate = Rect::new(
                    right_outer.x.saturating_sub(delta_x),
                    right_outer.y,
                    right_outer.width,
                    right_outer.height,
                );
                if candidate_introduces_foreign_node_overlap_for_subgraph(
                    graph,
                    node_rects,
                    right_id,
                    right_outer,
                    candidate,
                ) {
                    continue;
                };

                let moving_id = right_id.clone();
                let current_outer = right_outer;
                let delta_x = -(delta_x as isize);
                let shift_magnitude = current_outer.x.abs_diff(if delta_x.is_negative() {
                    current_outer.x.saturating_sub(delta_x.unsigned_abs())
                } else {
                    current_outer.x.saturating_add(delta_x as usize)
                });
                if best_shift
                    .as_ref()
                    .is_none_or(|(_, _, best_delta)| shift_magnitude > *best_delta)
                {
                    best_shift = Some((moving_id, delta_x, shift_magnitude));
                }
            }
        }

        let Some((subgraph_id, delta_x, _)) = best_shift else {
            break;
        };

        shift_nodes_in_subgraph_tree_x_signed(graph, positions, node_rects, &subgraph_id, delta_x);
        *canvas_width = node_rects
            .values()
            .map(|rect| rect.right())
            .max()
            .unwrap_or(*canvas_width);
    }
}

pub(super) fn preferred_subgraph_center_x(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    subgraph_id: &str,
    current_center_x: usize,
) -> usize {
    let mut external_centers = Vec::new();
    for edge in graph.edges.iter().filter(|edge| !edge.is_back_edge) {
        let from_inside = graph.is_node_in_subgraph_tree(&edge.from, subgraph_id);
        let to_inside = graph.is_node_in_subgraph_tree(&edge.to, subgraph_id);
        if from_inside == to_inside {
            continue;
        }
        let external_node_id = if from_inside { &edge.to } else { &edge.from };
        let Some(rect) = node_rects.get(external_node_id).copied() else {
            continue;
        };
        external_centers.push(rect_center_x(rect));
    }

    if external_centers.is_empty() {
        current_center_x
    } else {
        let sum: usize = external_centers.iter().sum();
        (sum + current_center_x) / (external_centers.len() + 1)
    }
}

pub(super) fn nested_horizontal_follow_gap(config: &CoarseLayoutConfig) -> usize {
    config.subgraph_gutter.saturating_add(2)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn preferred_declared_nested_horizontal_left(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    parent_id: &str,
    child_id: &str,
    parent_env: &SubgraphEnvelope,
    child_env: &SubgraphEnvelope,
    direction: Direction,
    gap: usize,
) -> Option<usize> {
    if !matches!(direction, Direction::LR | Direction::RL) {
        return None;
    }

    let parent_min_left = parent_env.outer.x.saturating_add(1);
    let parent_max_left = parent_env
        .outer
        .right()
        .saturating_sub(child_env.outer.width.saturating_add(1));
    if parent_max_left < parent_min_left {
        return None;
    }

    let mut min_left = parent_min_left;
    let mut max_left = parent_max_left;
    let child_center_x = rect_center_x(child_env.outer);

    for edge in graph.edges.iter().filter(|edge| !edge.is_back_edge) {
        let from_inside = graph.is_node_in_subgraph_tree(&edge.from, child_id);
        let to_inside = graph.is_node_in_subgraph_tree(&edge.to, child_id);
        if from_inside == to_inside {
            continue;
        }

        let external_node_id = if from_inside { &edge.to } else { &edge.from };
        let Some(external_rect) = node_rects.get(external_node_id).copied() else {
            continue;
        };

        if external_rect.right() <= child_env.outer.x {
            min_left = min_left.max(external_rect.right().saturating_add(gap));
        } else if external_rect.x >= child_env.outer.right() {
            max_left = max_left.min(
                external_rect
                    .x
                    .saturating_sub(child_env.outer.width.saturating_add(gap)),
            );
        }
    }

    for (node_id, node_rect) in node_rects {
        if graph.is_node_in_subgraph_tree(node_id, child_id) {
            continue;
        }
        if !graph.is_node_in_subgraph_tree(node_id, parent_id) {
            continue;
        }
        if !rects_overlap_vertically(*node_rect, child_env.outer) {
            continue;
        }

        if node_rect.right() <= child_env.outer.x {
            min_left = min_left.max(node_rect.right().saturating_add(gap));
        } else if node_rect.x >= child_env.outer.right() {
            max_left = max_left.min(
                node_rect
                    .x
                    .saturating_sub(child_env.outer.width.saturating_add(gap)),
            );
        } else if rect_center_x(*node_rect) <= child_center_x {
            min_left = min_left.max(node_rect.right().saturating_add(gap));
        } else {
            max_left = max_left.min(
                node_rect
                    .x
                    .saturating_sub(child_env.outer.width.saturating_add(gap)),
            );
        }
    }

    if max_left < min_left {
        return None;
    }

    Some(match direction {
        Direction::LR => min_left,
        Direction::RL => max_left,
        _ => unreachable!(),
    })
}

pub(super) fn outgoing_route_pressure_shift_x(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    subgraph_id: &str,
) -> usize {
    let mut source_centers = Vec::new();
    let mut target_centers = Vec::new();

    for edge in graph.edges.iter().filter(|edge| !edge.is_back_edge) {
        if !graph.is_node_in_subgraph_tree(&edge.from, subgraph_id)
            || graph.is_node_in_subgraph_tree(&edge.to, subgraph_id)
        {
            continue;
        }
        let Some(source_rect) = node_rects.get(&edge.from).copied() else {
            continue;
        };
        let Some(target_rect) = node_rects.get(&edge.to).copied() else {
            continue;
        };
        source_centers.push(rect_center_x(source_rect));
        target_centers.push(rect_center_x(target_rect));
    }

    if source_centers.len() < 2 || target_centers.is_empty() {
        return 0;
    }

    let span_start = source_centers.iter().copied().min().unwrap_or(0);
    let span_end = source_centers.iter().copied().max().unwrap_or(span_start);
    let source_span_center = (span_start + span_end) / 2;
    let target_center = target_centers.iter().sum::<usize>() / target_centers.len();

    if source_span_center > target_center {
        source_span_center.saturating_sub(target_center).div_ceil(6)
    } else {
        0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InternalRouteSpanBudget {
    pub(super) pivot_center: usize,
    pub(super) shift_x: usize,
}

pub(super) fn internal_route_span_budget_x(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    subgraph_id: &str,
    min_horizontal_spacing: usize,
) -> Option<InternalRouteSpanBudget> {
    let subgraph = graph.get_subgraph(subgraph_id)?;
    if !subgraph.has_parent() {
        return None;
    }

    let mut outgoing_by_target: HashMap<&str, HashSet<&str>> = HashMap::new();
    let mut incoming_by_source: HashMap<&str, HashSet<&str>> = HashMap::new();

    for edge in graph.edges.iter().filter(|edge| !edge.is_back_edge) {
        let from_inside = graph.is_node_in_subgraph_tree(&edge.from, subgraph_id);
        let to_inside = graph.is_node_in_subgraph_tree(&edge.to, subgraph_id);
        if from_inside == to_inside {
            continue;
        }
        if from_inside {
            outgoing_by_target
                .entry(edge.to.as_str())
                .or_default()
                .insert(edge.from.as_str());
        } else {
            incoming_by_source
                .entry(edge.from.as_str())
                .or_default()
                .insert(edge.to.as_str());
        }
    }

    let mut best_budget: Option<InternalRouteSpanBudget> = None;
    let desired_lane_gap = min_horizontal_spacing.saturating_add(4);

    for (external_node_id, inside_node_ids) in
        outgoing_by_target.iter().chain(incoming_by_source.iter())
    {
        if inside_node_ids.len() < 2 {
            continue;
        }
        let Some(external_rect) = node_rects.get(*external_node_id).copied() else {
            continue;
        };
        let external_center = rect_center_x(external_rect);

        let mut centers = Vec::new();
        for node_id in inside_node_ids {
            let Some(node_rect) = node_rects.get(*node_id).copied() else {
                continue;
            };
            centers.push(rect_center_x(node_rect));
        }
        if centers.len() < 2 {
            continue;
        }

        let span_start = centers.iter().copied().min().unwrap_or(0);
        let span_end = centers.iter().copied().max().unwrap_or(span_start);
        if external_center < span_start || external_center > span_end {
            continue;
        }

        let current_span = span_end.saturating_sub(span_start);
        let desired_span = inside_node_ids
            .len()
            .saturating_sub(1)
            .saturating_mul(desired_lane_gap);
        let shift_x = desired_span.saturating_sub(current_span);
        if shift_x == 0 {
            continue;
        }

        let candidate = InternalRouteSpanBudget {
            pivot_center: (span_start + span_end) / 2,
            shift_x,
        };
        if best_budget.is_none_or(|existing| candidate.shift_x > existing.shift_x) {
            best_budget = Some(candidate);
        }
    }

    best_budget
}

pub(super) fn widen_subgraph_for_internal_route_span(
    graph: &Graph,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    subgraph_id: &str,
    min_horizontal_spacing: usize,
) -> usize {
    let Some(budget) =
        internal_route_span_budget_x(graph, node_rects, subgraph_id, min_horizontal_spacing)
    else {
        return 0;
    };

    let mut shifted_any = false;
    for node in &graph.nodes {
        if !graph.is_node_in_subgraph_tree(&node.id, subgraph_id) {
            continue;
        }
        let Some(node_rect) = node_rects.get(&node.id).copied() else {
            continue;
        };
        if rect_center_x(node_rect) < budget.pivot_center {
            continue;
        }
        if let Some(position) = positions.get_mut(&node.id) {
            position.x += budget.shift_x;
        }
        if let Some(node_rect) = node_rects.get_mut(&node.id) {
            node_rect.x += budget.shift_x;
        }
        shifted_any = true;
    }

    if shifted_any {
        budget.shift_x
    } else {
        0
    }
}

pub(super) fn widen_subgraph_for_outgoing_route_pressure(
    graph: &Graph,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    subgraph_id: &str,
) -> usize {
    let mut source_node_ids: HashSet<String> = HashSet::new();
    let mut source_centers = Vec::new();
    let mut target_centers = Vec::new();

    for node in &graph.nodes {
        if !graph.is_node_in_subgraph_tree(&node.id, subgraph_id) {
            continue;
        }
        let Some(source_rect) = node_rects.get(&node.id).copied() else {
            continue;
        };

        let mut has_external_outgoing = false;
        for edge in graph
            .edges
            .iter()
            .filter(|edge| !edge.is_back_edge && edge.from == node.id)
        {
            if graph.is_node_in_subgraph_tree(&edge.to, subgraph_id) {
                continue;
            }
            let Some(target_rect) = node_rects.get(&edge.to).copied() else {
                continue;
            };
            has_external_outgoing = true;
            target_centers.push(rect_center_x(target_rect));
        }

        if has_external_outgoing {
            source_node_ids.insert(node.id.clone());
            source_centers.push(rect_center_x(source_rect));
        }
    }

    if source_centers.len() < 2 || target_centers.is_empty() {
        return 0;
    }

    let span_start = source_centers.iter().copied().min().unwrap_or(0);
    let span_end = source_centers.iter().copied().max().unwrap_or(span_start);
    let target_center = target_centers.iter().sum::<usize>() / target_centers.len();
    let current_span = span_end.saturating_sub(span_start);

    if span_start <= target_center {
        return 0;
    }

    let source_span_center = (span_start + span_end) / 2;
    let desired_span = span_start
        .saturating_sub(target_center)
        .div_ceil(3)
        .saturating_add(4)
        .clamp(8, 12);
    if current_span >= desired_span {
        return 0;
    }

    let shift_x = desired_span - current_span;
    if shift_x == 0 {
        return 0;
    }

    let mut shifted_any = false;
    for node in &graph.nodes {
        if !graph.is_node_in_subgraph_tree(&node.id, subgraph_id) {
            continue;
        }
        let Some(node_rect) = node_rects.get(&node.id).copied() else {
            continue;
        };
        let center_x = rect_center_x(node_rect);
        let should_shift = center_x >= source_span_center
            || (source_node_ids.contains(&node.id) && center_x == source_span_center);
        if !should_shift {
            continue;
        }
        if let Some(position) = positions.get_mut(&node.id) {
            position.x += shift_x;
        }
        if let Some(node_rect) = node_rects.get_mut(&node.id) {
            node_rect.x += shift_x;
        }
        shifted_any = true;
    }

    if shifted_any {
        shift_x
    } else {
        0
    }
}

#[allow(dead_code)]
pub(super) fn shift_nodes_up_to_rank_bt(
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    ranks: &HashMap<String, usize>,
    max_rank: usize,
    delta_y: usize,
) {
    if delta_y == 0 {
        return;
    }
    for (id, p) in positions.iter_mut() {
        let Some(rank) = ranks.get(id) else {
            continue;
        };
        if *rank > max_rank {
            continue;
        }
        p.y += delta_y;
        if let Some(r) = node_rects.get_mut(id) {
            r.y += delta_y;
        }
    }
}
