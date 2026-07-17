//! Coarse layout + Manhattan routing pipeline (default engine).
//!
//! This module supersedes the legacy waterfall layout while keeping a legacy
//! implementation available in `layout_legacy` for compatibility. The coarse
//! engine provides:
//! - Direction-agnostic layered placement on a coarse grid
//! - Obstacle-aware Manhattan routing with simple detours
//! - Subgraph gutter metadata for future avoidance/bundling

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

use anyhow::Result;

use crate::crossing::CrossingMinimizer;
use crate::geom::{EdgeRoute, Point, Rect};
use crate::graph::{Direction, Graph};
use crate::orientation::{Axis, OrientedCoords};
use crate::portals::{compute_envelopes, SubgraphEnvelope};
use crate::spacing::SpacingConfig;
use crate::style::{box_width, BOX_HEIGHT, BOX_MIN_WIDTH};

fn rect_fully_inside(outer: Rect, inner: Rect) -> bool {
    if outer.is_empty() || inner.is_empty() {
        return false;
    }
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.right() <= outer.right()
        && inner.bottom() <= outer.bottom()
}

fn rects_overlap_vertically(a: Rect, b: Rect) -> bool {
    a.y < b.bottom() && b.y < a.bottom()
}

fn rects_overlap_horizontally(a: Rect, b: Rect) -> bool {
    a.x < b.right() && b.x < a.right()
}

fn rect_center_x(rect: Rect) -> usize {
    rect.x + rect.width / 2
}

fn subgraph_tree_rank_range(
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

fn subgraphs_have_declared_hierarchy(graph: &Graph, left_id: &str, right_id: &str) -> bool {
    graph.is_subgraph_ancestor(left_id, right_id) || graph.is_subgraph_ancestor(right_id, left_id)
}

fn is_vertical_flow(direction: Direction) -> bool {
    matches!(direction, Direction::TD | Direction::TB)
}

fn route_budgeted_subgraphs(graph: &Graph) -> Vec<String> {
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

fn top_level_subgraph_id(graph: &Graph, subgraph_id: &str) -> String {
    let mut current = subgraph_id.to_string();
    while let Some(parent_id) = graph
        .get_subgraph(&current)
        .and_then(|subgraph| subgraph.parent_id.as_deref())
    {
        current = parent_id.to_string();
    }
    current
}

fn top_level_subgraph_components(graph: &Graph) -> Vec<Vec<String>> {
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

fn compact_stacked_vertical_top_level_sibling_subgraphs(
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

fn enforce_declared_nested_envelopes(
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

fn shift_nodes_from_rank_td(
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

fn shift_nodes_in_subgraph(
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

fn shift_nodes_in_subgraph_y(
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

fn shift_nodes_in_subgraph_tree_y_signed(
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

fn reserve_nested_horizontal_subgraph_headroom(
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

fn reserve_titled_horizontal_subgraph_headroom(
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

fn shift_nodes_in_subgraph_tree_x_signed(
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

fn subgraph_depth_in_graph(graph: &Graph, subgraph_id: &str) -> usize {
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

fn subgraph_has_cross_boundary_edges(graph: &Graph, subgraph_id: &str) -> bool {
    graph.edges.iter().any(|edge| {
        let from_in = graph.is_node_in_subgraph_tree(&edge.from, subgraph_id);
        let to_in = graph.is_node_in_subgraph_tree(&edge.to, subgraph_id);
        (from_in || to_in) && from_in != to_in
    })
}

fn subgraph_is_top_level_leaf(graph: &Graph, subgraph_id: &str) -> bool {
    let Some(subgraph) = graph.get_subgraph(subgraph_id) else {
        return false;
    };
    subgraph.parent_id.is_none()
        && !graph
            .subgraphs
            .iter()
            .any(|candidate| candidate.parent_id.as_deref() == Some(subgraph_id))
}

fn subgraph_has_overlapping_foreign_nodes(
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

fn subgraph_can_rebalance_horizontal_content(
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

fn rebalance_titled_vertical_subgraph_content_x(
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

fn rebalance_titled_vertical_subgraph_content_y(
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

fn shift_nodes_by_id_y(
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

fn shift_nodes_from_x(
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

fn shift_nodes_by_id_x(
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

fn candidate_introduces_foreign_node_overlap_for_subgraph(
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

fn flow_exit_gap_to_external_node(
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

fn rebalance_side_by_side_horizontal_top_level_sibling_gaps(
    graph: &Graph,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    gutter: usize,
    canvas_width: &mut usize,
) {
    if graph.direction != Direction::LR || graph.subgraphs.is_empty() {
        return;
    }

    const MIN_INTER_GAP: usize = 6;
    const IMBALANCE_TOLERANCE: usize = 2;

    for _ in 0..8 {
        let envelopes = compute_envelopes(graph, node_rects, gutter);
        let mut best_shift: Option<(String, isize, usize)> = None;

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
                rects_overlap_vertically(left_outer, right_outer)
                    && !rects_overlap_horizontally(left_outer, right_outer)
            });
            if !is_side_by_side_row {
                continue;
            }

            for pair in ordered.windows(2) {
                let (_left_id, left_outer) = (&pair[0].0, pair[0].1);
                let (right_id, right_outer) = (&pair[1].0, pair[1].1);
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

fn preferred_subgraph_center_x(
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

fn nested_horizontal_follow_gap(config: &CoarseLayoutConfig) -> usize {
    config.subgraph_gutter.saturating_add(2)
}

#[allow(clippy::too_many_arguments)]
fn preferred_declared_nested_horizontal_left(
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

fn outgoing_route_pressure_shift_x(
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
struct InternalRouteSpanBudget {
    pivot_center: usize,
    shift_x: usize,
}

fn internal_route_span_budget_x(
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

fn widen_subgraph_for_internal_route_span(
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

fn widen_subgraph_for_outgoing_route_pressure(
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
fn shift_nodes_up_to_rank_bt(
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

/// Input for the experimental layout engine.
pub struct LayoutInput<'a> {
    pub graph: &'a Graph,
    pub prior_positions: Option<HashMap<String, Point>>,
}

/// Output of the experimental layout pipeline.
#[derive(Debug, Default)]
pub struct LayoutOutput {
    pub positions: HashMap<String, Point>,
    pub subgraph_envelopes: HashMap<String, SubgraphEnvelope>,
    pub routes: HashMap<usize, EdgeRoute>,
    pub canvas: Rect,
    pub warnings: Vec<String>,
    pub ranks: HashMap<String, usize>,
}

/// Tunable spacing controls.
#[derive(Debug, Clone)]
pub struct CoarseLayoutConfig {
    /// Padding around nodes when building the occupancy grid.
    pub node_padding: usize,
    /// Gutter around subgraphs (stored separately; optionally treated as obstacles).
    pub subgraph_gutter: usize,
    /// Minimum spacing along the horizontal axis.
    pub min_horizontal_spacing: usize,
    /// Minimum spacing along the vertical axis.
    pub min_vertical_spacing: usize,
    /// Allow carving through subgraph borders (portals).
    pub enable_portals: bool,
}

impl Default for CoarseLayoutConfig {
    fn default() -> Self {
        Self::from_spacing(&SpacingConfig::default_config())
    }
}

impl CoarseLayoutConfig {
    /// Tighter spacing defaults for terminal-friendly diagrams.
    ///
    /// This is intentionally conservative (still leaves room for elbows/arrows)
    /// but reduces the default "big gaps" between ranks/columns.
    pub fn compact() -> Self {
        Self::from_spacing(&SpacingConfig::compact())
    }

    pub fn from_spacing(spacing: &SpacingConfig) -> Self {
        Self {
            node_padding: spacing.node_margin,
            subgraph_gutter: spacing.subgraph_gutter,
            min_horizontal_spacing: spacing.col_spacing,
            min_vertical_spacing: spacing.row_spacing,
            enable_portals: true,
        }
    }
}

pub fn coarse_waterfall_with_config(graph: Graph, mut config: CoarseLayoutConfig) -> Result<Graph> {
    if std::env::var("TERMIFLOW_DISABLE_PORTALS").is_ok() {
        config.enable_portals = false;
    }
    apply_coarse_layout(graph, None, config)
}

/// Preferred entry point for the coarse layout engine.
pub fn coarse_waterfall(graph: Graph) -> Result<Graph> {
    coarse_waterfall_with_config(graph, CoarseLayoutConfig::default())
}

/// Backwards-compatible alias for callers expecting `waterfall`.
#[deprecated(note = "Use coarse_waterfall or layout_legacy::waterfall for the old engine")]
pub fn waterfall(graph: Graph) -> Result<Graph> {
    coarse_waterfall(graph)
}

/// Coarse layout engine entry point.
pub fn layout(input: LayoutInput, config: CoarseLayoutConfig) -> Result<LayoutOutput> {
    let coords = OrientedCoords::new(input.graph.direction);
    let debug_timing = std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok();

    // 1) Layer assignment (lenient Kahn) and ordering.
    let t_layers = std::time::Instant::now();
    let mut layers = assign_layers(input.graph);

    // 1.5) Optimize layer order to minimize crossings (adaptive algorithm with convergence)
    let minimizer = CrossingMinimizer::new();
    let final_crossings = minimizer.minimize(input.graph, &mut layers);
    if debug_timing {
        eprintln!(
            "termiflow: layers {:?} ({} layers, {} crossings)",
            t_layers.elapsed(),
            layers.len(),
            final_crossings
        );
    }

    // 2) Place nodes on coarse grid.
    let t_place = std::time::Instant::now();
    let mut placement = place_nodes(
        input.graph,
        &layers,
        &coords,
        &config,
        input.prior_positions.as_ref(),
    );
    if debug_timing {
        eprintln!(
            "termiflow: placement {:?} (canvas {}x{})",
            t_place.elapsed(),
            placement.canvas.width,
            placement.canvas.height
        );
    }

    // 2.25) Resolve horizontal subgraph overlaps for LR/RL before flipping coordinates.
    if matches!(input.graph.direction, Direction::LR | Direction::RL)
        && !input.graph.subgraphs.is_empty()
    {
        for _ in 0..8 {
            let mut required_shift_by_id: HashMap<String, usize> = HashMap::new();

            let mut subgraph_min_rank: HashMap<&str, usize> = HashMap::new();
            for sg in &input.graph.subgraphs {
                let min_rank = subgraph_tree_rank_range(input.graph, &placement.ranks, &sg.id)
                    .map(|(min_rank, _)| min_rank);
                if let Some(r) = min_rank {
                    subgraph_min_rank.insert(sg.id.as_str(), r);
                }
            }

            let envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);

            let mut sg_ids: Vec<&String> = envelopes.keys().collect();
            sg_ids.sort_unstable_by(|left, right| left.as_str().cmp(right.as_str()));
            for i in 0..sg_ids.len() {
                for j in (i + 1)..sg_ids.len() {
                    let env1 = &envelopes[sg_ids[i]];
                    let env2 = &envelopes[sg_ids[j]];
                    let intersects = env1.outer.x < env2.outer.right()
                        && env1.outer.right() > env2.outer.x
                        && env1.outer.y < env2.outer.bottom()
                        && env1.outer.bottom() > env2.outer.y;
                    if !intersects {
                        continue;
                    }
                    let nested = rect_fully_inside(env1.outer, env2.outer)
                        || rect_fully_inside(env2.outer, env1.outer);
                    if nested
                        && subgraphs_have_declared_hierarchy(
                            input.graph,
                            sg_ids[i].as_str(),
                            sg_ids[j].as_str(),
                        )
                    {
                        continue;
                    }

                    let r1 = subgraph_min_rank.get(sg_ids[i].as_str()).copied();
                    let r2 = subgraph_min_rank.get(sg_ids[j].as_str()).copied();
                    let (Some(rank1), Some(rank2)) = (r1, r2) else {
                        continue;
                    };
                    // Shift the later-ranked subgraph to the right until it clears the earlier one.
                    let (late_id, early_env, late_env) = if rank1 <= rank2 {
                        (sg_ids[j].as_str(), env1, env2)
                    } else {
                        (sg_ids[i].as_str(), env2, env1)
                    };

                    let required_left = early_env.outer.right().saturating_add(1);
                    if late_env.outer.x < required_left {
                        let delta = required_left - late_env.outer.x;
                        required_shift_by_id
                            .entry(late_id.to_string())
                            .and_modify(|d| *d = (*d).max(delta))
                            .or_insert(delta);
                    }
                }
            }

            let Some((late_id, delta_x)) = required_shift_by_id
                .iter()
                .max_by(|(left_id, left_delta), (right_id, right_delta)| {
                    left_delta
                        .cmp(right_delta)
                        .then_with(|| right_id.cmp(left_id))
                })
                .map(|(id, delta)| (id.clone(), *delta))
            else {
                break;
            };

            shift_nodes_in_subgraph(
                input.graph,
                &mut placement.positions,
                &mut placement.node_rects,
                &late_id,
                delta_x,
            );
        }
    }

    // 2.26) Resolve vertical subgraph overlaps for TD/BT
    if matches!(
        input.graph.direction,
        Direction::TD | Direction::TB | Direction::BT
    ) && !input.graph.subgraphs.is_empty()
    {
        for _ in 0..8 {
            let envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            let mut shifts: HashMap<String, usize> = HashMap::new();

            // Compute minimum rank for each subgraph to determine "earlier" vs "later"
            let mut subgraph_min_rank: HashMap<&str, usize> = HashMap::new();
            for sg in &input.graph.subgraphs {
                let min_rank = subgraph_tree_rank_range(input.graph, &placement.ranks, &sg.id)
                    .map(|(min_rank, _)| min_rank);
                if let Some(r) = min_rank {
                    subgraph_min_rank.insert(sg.id.as_str(), r);
                }
            }

            // Check all sibling pairs for vertical overlap
            let mut sg_ids: Vec<&String> = envelopes.keys().collect();
            sg_ids.sort_unstable_by(|left, right| left.as_str().cmp(right.as_str()));
            for i in 0..sg_ids.len() {
                for j in (i + 1)..sg_ids.len() {
                    let env1 = &envelopes[sg_ids[i]];
                    let env2 = &envelopes[sg_ids[j]];

                    // Must overlap horizontally to collide vertically
                    let h_overlap =
                        env1.outer.x < env2.outer.right() && env2.outer.x < env1.outer.right();
                    let v_overlap =
                        env1.outer.y < env2.outer.bottom() && env2.outer.y < env1.outer.bottom();

                    if !h_overlap || !v_overlap {
                        continue;
                    }

                    // Skip nested subgraphs
                    let nested = rect_fully_inside(env1.outer, env2.outer)
                        || rect_fully_inside(env2.outer, env1.outer);
                    if nested
                        && subgraphs_have_declared_hierarchy(
                            input.graph,
                            sg_ids[i].as_str(),
                            sg_ids[j].as_str(),
                        )
                    {
                        continue;
                    }

                    // Determine which subgraph is "later" (higher rank = drawn later)
                    let r1 = subgraph_min_rank.get(sg_ids[i].as_str()).copied();
                    let r2 = subgraph_min_rank.get(sg_ids[j].as_str()).copied();
                    let (Some(rank1), Some(rank2)) = (r1, r2) else {
                        continue;
                    };

                    // Shift the later-ranked subgraph down until it clears the earlier one
                    let (late_id, early_env, late_env) = if rank1 <= rank2 {
                        (sg_ids[j].as_str(), env1, env2)
                    } else {
                        (sg_ids[i].as_str(), env2, env1)
                    };

                    let required_top = early_env.outer.bottom().saturating_add(1);
                    if late_env.outer.y < required_top {
                        let delta = required_top - late_env.outer.y;
                        shifts
                            .entry(late_id.to_string())
                            .and_modify(|d| *d = (*d).max(delta))
                            .or_insert(delta);
                    }
                }
            }

            let Some((sg_id, delta)) = shifts
                .iter()
                .max_by(|(left_id, left_delta), (right_id, right_delta)| {
                    left_delta
                        .cmp(right_delta)
                        .then_with(|| right_id.cmp(left_id))
                })
                .map(|(id, d)| (id.clone(), *d))
            else {
                break;
            };

            // Shift all nodes in the subgraph down
            shift_nodes_in_subgraph_y(
                input.graph,
                &mut placement.positions,
                &mut placement.node_rects,
                &sg_id,
                delta,
            );
        }
    }

    // 2.5) Flip coordinates for BT/RL to match flow direction
    // Calculate strict content bounds
    let max_x = placement
        .node_rects
        .values()
        .map(|r| r.right())
        .max()
        .unwrap_or(0);
    let max_y = placement
        .node_rects
        .values()
        .map(|r| r.bottom())
        .max()
        .unwrap_or(0);

    if input.graph.direction == Direction::BT {
        for (id, p) in placement.positions.iter_mut() {
            let h = placement
                .node_rects
                .get(id)
                .map(|r| r.height)
                .unwrap_or(BOX_HEIGHT);
            p.y = max_y.saturating_sub(p.y).saturating_sub(h);
        }
        for r in placement.node_rects.values_mut() {
            r.y = max_y.saturating_sub(r.y).saturating_sub(r.height);
        }
    } else if input.graph.direction == Direction::RL {
        // Easier: Iterate keys of positions (node ids)
        for (id, p) in placement.positions.iter_mut() {
            if let Some(r) = placement.node_rects.get_mut(id) {
                let new_x = max_x.saturating_sub(r.x + r.width);
                p.x = new_x;
                r.x = new_x;
            }
        }
    }

    // Shift nodes to make room for subgraph gutters if any subgraphs exist
    if !input.graph.subgraphs.is_empty() {
        let shift = config.subgraph_gutter;
        for p in placement.positions.values_mut() {
            p.x += shift;
            p.y += shift;
        }
        for r in placement.node_rects.values_mut() {
            r.x += shift;
            r.y += shift;
        }
        // Canvas grows by the shift amount (padding on both sides)
        placement.canvas.width = max_x + shift * 2;
        placement.canvas.height = max_y + shift * 2;
    } else {
        // Tighten canvas to content if no subgraphs (optional, but cleaner)
        placement.canvas.width = max_x;
        placement.canvas.height = max_y;
    }

    reserve_nested_horizontal_subgraph_headroom(
        input.graph,
        &mut placement.positions,
        &mut placement.node_rects,
        config.subgraph_gutter,
        &mut placement.canvas.height,
    );

    if matches!(input.graph.direction, Direction::LR | Direction::RL)
        && !input.graph.subgraphs.is_empty()
    {
        for _ in 0..8 {
            let envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            let mut required_env_shift: Option<(usize, usize)> = None;
            let mut external_node_shifts: HashMap<String, usize> = HashMap::new();

            for (subgraph_id, env) in &envelopes {
                for edge in input.graph.edges.iter().filter(|edge| !edge.is_back_edge) {
                    let (Some(from_rect), Some(to_rect)) = (
                        placement.node_rects.get(&edge.from),
                        placement.node_rects.get(&edge.to),
                    ) else {
                        continue;
                    };

                    let from_inside_tree = input
                        .graph
                        .is_node_in_subgraph_tree(&edge.from, subgraph_id);
                    let to_inside_tree =
                        input.graph.is_node_in_subgraph_tree(&edge.to, subgraph_id);
                    if from_inside_tree == to_inside_tree {
                        continue;
                    }

                    let inside_rect = if from_inside_tree {
                        *from_rect
                    } else {
                        *to_rect
                    };
                    if !rect_fully_inside(env.outer, inside_rect) {
                        continue;
                    }

                    let external_rect = if from_inside_tree {
                        *to_rect
                    } else {
                        *from_rect
                    };
                    let external_is_subgraph = if from_inside_tree {
                        input.graph.get_node_subgraph(&edge.to).is_some()
                    } else {
                        input.graph.get_node_subgraph(&edge.from).is_some()
                    };
                    if external_is_subgraph {
                        continue;
                    }

                    if external_rect.x < env.outer.x {
                        let overlaps_left_wall = external_rect.right() > env.outer.x;
                        if overlaps_left_wall {
                            let required_env_x = external_rect.right().saturating_add(2);
                            let threshold_x = env.outer.x;
                            let delta_x = required_env_x - env.outer.x;
                            match required_env_shift {
                                Some((best_x, best_delta)) => {
                                    if threshold_x < best_x
                                        || (threshold_x == best_x && delta_x > best_delta)
                                    {
                                        required_env_shift = Some((threshold_x, delta_x));
                                    }
                                }
                                None => required_env_shift = Some((threshold_x, delta_x)),
                            }
                        }
                    } else {
                        let overlaps_right_wall = external_rect.x < env.outer.right();
                        if overlaps_right_wall {
                            let required_external_x = env.outer.right().saturating_add(2);
                            let external_node_id = if from_inside_tree {
                                edge.to.clone()
                            } else {
                                edge.from.clone()
                            };
                            let delta_x = required_external_x - external_rect.x;
                            external_node_shifts
                                .entry(external_node_id)
                                .and_modify(|existing| *existing = (*existing).max(delta_x))
                                .or_insert(delta_x);
                        }
                    }
                }
            }

            if required_env_shift.is_none() && external_node_shifts.is_empty() {
                break;
            }

            if let Some((threshold_x, delta_x)) = required_env_shift {
                shift_nodes_from_x(
                    &mut placement.positions,
                    &mut placement.node_rects,
                    threshold_x,
                    delta_x,
                );
            }
            if !external_node_shifts.is_empty() {
                shift_nodes_by_id_x(
                    &mut placement.positions,
                    &mut placement.node_rects,
                    &external_node_shifts,
                );
            }

            let max_right = placement
                .node_rects
                .values()
                .map(|rect| rect.right())
                .max()
                .unwrap_or(placement.canvas.right());
            placement.canvas.width = placement.canvas.width.max(max_right);
        }
    }

    rebalance_side_by_side_horizontal_top_level_sibling_gaps(
        input.graph,
        &mut placement.positions,
        &mut placement.node_rects,
        config.subgraph_gutter,
        &mut placement.canvas.width,
    );

    // 3) Subgraph bounds + gutters.
    let mut subgraph_envelopes =
        compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
    adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);

    if matches!(input.graph.direction, Direction::LR | Direction::RL)
        && !subgraph_envelopes.is_empty()
    {
        let gap = nested_horizontal_follow_gap(&config);
        for _ in 0..8 {
            let mut required_shift_by_id: HashMap<String, isize> = HashMap::new();

            for child_subgraph in input
                .graph
                .subgraphs
                .iter()
                .filter(|subgraph| subgraph.parent_id.is_some())
            {
                let Some(parent_id) = child_subgraph.parent_id.as_deref() else {
                    continue;
                };
                let (Some(parent_env), Some(child_env)) = (
                    subgraph_envelopes.get(parent_id),
                    subgraph_envelopes.get(&child_subgraph.id),
                ) else {
                    continue;
                };
                if !rect_fully_inside(parent_env.outer, child_env.outer) {
                    continue;
                }

                let Some(target_left) = preferred_declared_nested_horizontal_left(
                    input.graph,
                    &placement.node_rects,
                    parent_id,
                    &child_subgraph.id,
                    parent_env,
                    child_env,
                    input.graph.direction,
                    gap,
                ) else {
                    continue;
                };

                if target_left == child_env.outer.x {
                    continue;
                }

                let delta = target_left as isize - child_env.outer.x as isize;
                required_shift_by_id
                    .entry(child_subgraph.id.clone())
                    .and_modify(|existing| {
                        if delta.abs() > existing.abs() {
                            *existing = delta;
                        }
                    })
                    .or_insert(delta);
            }

            let Some((subgraph_id, delta_x)) = required_shift_by_id
                .iter()
                .max_by_key(|(_, delta)| delta.abs())
                .map(|(id, delta)| (id.clone(), *delta))
            else {
                break;
            };

            shift_nodes_in_subgraph_tree_x_signed(
                input.graph,
                &mut placement.positions,
                &mut placement.node_rects,
                &subgraph_id,
                delta_x,
            );

            let max_right = placement
                .node_rects
                .values()
                .map(|rect| rect.right())
                .max()
                .unwrap_or(placement.canvas.right());
            placement.canvas.width = placement.canvas.width.max(max_right);

            subgraph_envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
        }

        for _ in 0..8 {
            let envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            let mut required_env_shift: Option<(usize, usize)> = None;
            let mut external_node_shifts: HashMap<String, usize> = HashMap::new();

            for (subgraph_id, env) in &envelopes {
                for edge in input.graph.edges.iter().filter(|edge| !edge.is_back_edge) {
                    let (Some(from_rect), Some(to_rect)) = (
                        placement.node_rects.get(&edge.from),
                        placement.node_rects.get(&edge.to),
                    ) else {
                        continue;
                    };

                    let from_inside_tree = input
                        .graph
                        .is_node_in_subgraph_tree(&edge.from, subgraph_id);
                    let to_inside_tree =
                        input.graph.is_node_in_subgraph_tree(&edge.to, subgraph_id);
                    if from_inside_tree == to_inside_tree {
                        continue;
                    }

                    let inside_rect = if from_inside_tree {
                        *from_rect
                    } else {
                        *to_rect
                    };
                    if !rect_fully_inside(env.outer, inside_rect) {
                        continue;
                    }

                    let external_rect = if from_inside_tree {
                        *to_rect
                    } else {
                        *from_rect
                    };
                    let external_is_subgraph = if from_inside_tree {
                        input.graph.get_node_subgraph(&edge.to).is_some()
                    } else {
                        input.graph.get_node_subgraph(&edge.from).is_some()
                    };
                    if external_is_subgraph {
                        continue;
                    }

                    if external_rect.x < env.outer.x {
                        let overlaps_left_wall = external_rect.right() > env.outer.x;
                        if overlaps_left_wall {
                            let required_env_x = external_rect.right().saturating_add(2);
                            let threshold_x = env.outer.x;
                            let delta_x = required_env_x - env.outer.x;
                            match required_env_shift {
                                Some((best_x, best_delta)) => {
                                    if threshold_x < best_x
                                        || (threshold_x == best_x && delta_x > best_delta)
                                    {
                                        required_env_shift = Some((threshold_x, delta_x));
                                    }
                                }
                                None => required_env_shift = Some((threshold_x, delta_x)),
                            }
                        }
                    } else {
                        let overlaps_right_wall = external_rect.x < env.outer.right();
                        if overlaps_right_wall {
                            let required_external_x = env.outer.right().saturating_add(2);
                            let external_node_id = if from_inside_tree {
                                edge.to.clone()
                            } else {
                                edge.from.clone()
                            };
                            let delta_x = required_external_x - external_rect.x;
                            external_node_shifts
                                .entry(external_node_id)
                                .and_modify(|existing| *existing = (*existing).max(delta_x))
                                .or_insert(delta_x);
                        }
                    }
                }
            }

            if required_env_shift.is_none() && external_node_shifts.is_empty() {
                break;
            }

            if let Some((threshold_x, delta_x)) = required_env_shift {
                shift_nodes_from_x(
                    &mut placement.positions,
                    &mut placement.node_rects,
                    threshold_x,
                    delta_x,
                );
            }
            if !external_node_shifts.is_empty() {
                shift_nodes_by_id_x(
                    &mut placement.positions,
                    &mut placement.node_rects,
                    &external_node_shifts,
                );
            }

            let max_right = placement
                .node_rects
                .values()
                .map(|rect| rect.right())
                .max()
                .unwrap_or(placement.canvas.right());
            placement.canvas.width = placement.canvas.width.max(max_right);

            subgraph_envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
        }
    }

    if is_vertical_flow(input.graph.direction) && !subgraph_envelopes.is_empty() {
        let route_budgeted_subgraphs = route_budgeted_subgraphs(input.graph);
        for _ in 0..8 {
            let mut widened_any = false;
            for subgraph_id in &route_budgeted_subgraphs {
                if widen_subgraph_for_internal_route_span(
                    input.graph,
                    &mut placement.positions,
                    &mut placement.node_rects,
                    subgraph_id,
                    config.min_horizontal_spacing,
                ) > 0
                {
                    widened_any = true;
                }
                if widen_subgraph_for_outgoing_route_pressure(
                    input.graph,
                    &mut placement.positions,
                    &mut placement.node_rects,
                    subgraph_id,
                ) > 0
                {
                    widened_any = true;
                }
            }
            if widened_any {
                let max_right = placement
                    .node_rects
                    .values()
                    .map(|r| r.right())
                    .max()
                    .unwrap_or(placement.canvas.right());
                placement.canvas.width = placement.canvas.width.max(max_right);
                subgraph_envelopes =
                    compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
                adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
            }

            let mut required_shift_by_id: HashMap<String, isize> = HashMap::new();

            let sg_ids: Vec<&String> = subgraph_envelopes.keys().collect();
            for parent_id in &sg_ids {
                let Some(parent_env) = subgraph_envelopes.get(*parent_id) else {
                    continue;
                };
                for child_id in &sg_ids {
                    if parent_id == child_id {
                        continue;
                    }
                    let Some(child_env) = subgraph_envelopes.get(*child_id) else {
                        continue;
                    };
                    if !rect_fully_inside(parent_env.outer, child_env.outer) {
                        continue;
                    }
                    let child_has_external_outgoing = input.graph.edges.iter().any(|edge| {
                        !edge.is_back_edge
                            && input.graph.is_node_in_subgraph_tree(&edge.from, child_id)
                            && !input.graph.is_node_in_subgraph_tree(&edge.to, child_id)
                    });
                    if !child_has_external_outgoing {
                        continue;
                    }

                    let preferred_center_x = preferred_subgraph_center_x(
                        input.graph,
                        &placement.node_rects,
                        child_id,
                        rect_center_x(child_env.outer),
                    );
                    let route_pressure_shift = outgoing_route_pressure_shift_x(
                        input.graph,
                        &placement.node_rects,
                        child_id,
                    );
                    let preferred_left =
                        preferred_center_x.saturating_sub(child_env.outer.width / 2);

                    let mut min_left = 0usize;
                    let mut max_left: Option<usize> = None;

                    for (node_id, node_rect) in placement.node_rects.iter() {
                        if input.graph.is_node_in_subgraph_tree(node_id, child_id) {
                            continue;
                        }
                        if !rect_fully_inside(parent_env.outer, *node_rect)
                            || !rects_overlap_vertically(*node_rect, child_env.outer)
                        {
                            continue;
                        }

                        if node_rect.right() <= child_env.outer.x {
                            min_left = min_left.max(node_rect.right().saturating_add(1));
                        } else if node_rect.x >= child_env.outer.right() {
                            let candidate = node_rect
                                .x
                                .saturating_sub(child_env.outer.width.saturating_add(1));
                            max_left =
                                Some(max_left.map_or(candidate, |limit| limit.min(candidate)));
                        } else {
                            min_left = min_left.max(node_rect.right().saturating_add(1));
                        }
                    }

                    let unclamped_left = if let Some(limit) = max_left {
                        preferred_left.clamp(min_left, limit.max(min_left))
                    } else {
                        preferred_left.max(min_left)
                    };
                    let target_left = if let Some(limit) = max_left {
                        unclamped_left
                            .saturating_add(route_pressure_shift)
                            .min(limit.max(unclamped_left))
                    } else {
                        unclamped_left.saturating_add(route_pressure_shift)
                    };

                    if target_left != child_env.outer.x {
                        let delta = target_left as isize - child_env.outer.x as isize;
                        required_shift_by_id
                            .entry((**child_id).clone())
                            .and_modify(|existing| {
                                if delta.abs() > existing.abs() {
                                    *existing = delta;
                                }
                            })
                            .or_insert(delta);
                    }
                }
            }

            let Some((sg_id, delta_x)) = required_shift_by_id
                .iter()
                .max_by_key(|(_, delta)| delta.abs())
                .map(|(id, delta)| (id.clone(), *delta))
            else {
                if widened_any {
                    continue;
                }
                break;
            };

            shift_nodes_in_subgraph_tree_x_signed(
                input.graph,
                &mut placement.positions,
                &mut placement.node_rects,
                &sg_id,
                delta_x,
            );

            let max_right = placement
                .node_rects
                .values()
                .map(|r| r.right())
                .max()
                .unwrap_or(placement.canvas.right());
            placement.canvas.width = placement.canvas.width.max(max_right);

            subgraph_envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
        }
    }

    // Ensure we have at least one row between a subgraph bottom border and any
    // external target box below it. Otherwise the renderer's arrow would land on
    // the border row (missing the arrow at the target entry point).
    if matches!(input.graph.direction, Direction::TD | Direction::TB)
        && !subgraph_envelopes.is_empty()
    {
        for _ in 0..8 {
            let mut required_shift_by_rank: HashMap<usize, usize> = HashMap::new();

            let mut subgraph_min_rank: HashMap<&str, usize> = HashMap::new();
            for sg in &input.graph.subgraphs {
                let min_rank = subgraph_tree_rank_range(input.graph, &placement.ranks, &sg.id)
                    .map(|(min_rank, _)| min_rank);
                if let Some(r) = min_rank {
                    subgraph_min_rank.insert(sg.id.as_str(), r);
                }
            }

            let mut incoming_into_subgraph_from: HashMap<(String, String), usize> = HashMap::new();
            for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                let (_, enter_subgraphs) =
                    input.graph.edge_boundary_crossings(&edge.from, &edge.to);
                for to_sg in enter_subgraphs {
                    *incoming_into_subgraph_from
                        .entry((edge.from.clone(), to_sg.to_string()))
                        .or_default() += 1;
                }
            }

            // Ensure declared parents keep a visible title/border band above nested children.
            for child_subgraph in input
                .graph
                .subgraphs
                .iter()
                .filter(|subgraph| subgraph.parent_id.is_some())
            {
                let Some(parent_id) = child_subgraph.parent_id.as_deref() else {
                    continue;
                };
                let (Some(parent_env), Some(child_env)) = (
                    subgraph_envelopes.get(parent_id),
                    subgraph_envelopes.get(&child_subgraph.id),
                ) else {
                    continue;
                };
                let Some(&shift_rank) = subgraph_min_rank.get(child_subgraph.id.as_str()) else {
                    continue;
                };

                let parent_has_title = input
                    .graph
                    .get_subgraph(parent_id)
                    .and_then(|subgraph| subgraph.title.as_ref())
                    .is_some();
                let required_child_top =
                    parent_env
                        .outer
                        .y
                        .saturating_add(if parent_has_title { 3 } else { 2 });
                if child_env.outer.y >= required_child_top {
                    continue;
                }

                let delta = required_child_top - child_env.outer.y;
                required_shift_by_rank
                    .entry(shift_rank)
                    .and_modify(|existing| *existing = (*existing).max(delta))
                    .or_insert(delta);
            }

            // Ensure enough clearance above a subgraph top border for incoming edges.
            for (sg_id, env) in subgraph_envelopes.iter() {
                let Some(&shift_rank) = subgraph_min_rank.get(sg_id.as_str()) else {
                    continue;
                };
                for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                    let (_, enter_subgraphs) =
                        input.graph.edge_boundary_crossings(&edge.from, &edge.to);
                    if !enter_subgraphs.contains(&sg_id.as_str()) {
                        continue;
                    }
                    // Don't apply this spacing rule for edges whose source already sits inside
                    // another subgraph (nested compositions). Those are handled by internal
                    // subgraph padding and routing, and enforcing "outside" clearance here
                    // can cause runaway vertical expansion.
                    if input.graph.get_node_subgraph(&edge.from).is_some() {
                        continue;
                    }
                    let Some(from_rect) = placement.node_rects.get(&edge.from) else {
                        continue;
                    };
                    // Single incoming edge: one connector row is enough.
                    // Fan-out entry (same external source → multiple targets): keep two rows so
                    // the trunk can be visible before entering the subgraph.
                    let incoming_count = incoming_into_subgraph_from
                        .get(&(edge.from.clone(), sg_id.clone()))
                        .copied()
                        .unwrap_or(1);
                    let clearance = if incoming_count > 1 { 2 } else { 1 };
                    let required_border_y = from_rect.bottom().saturating_add(clearance);
                    if env.outer.y < required_border_y {
                        let delta = required_border_y - env.outer.y;
                        required_shift_by_rank
                            .entry(shift_rank)
                            .and_modify(|d| *d = (*d).max(delta))
                            .or_insert(delta);
                    }
                }
            }

            // Ensure at least one empty row between stacked subgraphs when an edge crosses
            // from one to the next (so the connector is visible outside both borders).
            for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                let (Some(from_sg), Some(to_sg)) = (
                    input.graph.get_node_subgraph(&edge.from),
                    input.graph.get_node_subgraph(&edge.to),
                ) else {
                    continue;
                };
                if from_sg == to_sg {
                    continue;
                }
                let (Some(from_env), Some(to_env)) = (
                    subgraph_envelopes.get(from_sg),
                    subgraph_envelopes.get(to_sg),
                ) else {
                    continue;
                };
                // Only skip if subgraphs are truly nested (one fully inside the other).
                // Overlapping-but-not-nested subgraphs need spacing applied.
                let is_nested = rect_fully_inside(from_env.outer, to_env.outer)
                    || rect_fully_inside(to_env.outer, from_env.outer);
                if is_nested {
                    continue;
                }
                let required_to_top = from_env.outer.bottom().saturating_add(1);
                if to_env.outer.y >= required_to_top {
                    continue;
                }
                let Some(&shift_rank) = subgraph_min_rank.get(to_sg) else {
                    continue;
                };
                let delta = required_to_top - to_env.outer.y;
                required_shift_by_rank
                    .entry(shift_rank)
                    .and_modify(|d| *d = (*d).max(delta))
                    .or_insert(delta);
            }

            for env in subgraph_envelopes.values() {
                for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                    let (Some(from_rect), Some(to_rect)) = (
                        placement.node_rects.get(&edge.from),
                        placement.node_rects.get(&edge.to),
                    ) else {
                        continue;
                    };

                    if !rect_fully_inside(env.outer, *from_rect) {
                        continue;
                    }
                    if rect_fully_inside(env.outer, *to_rect) {
                        continue;
                    }
                    // If the destination is inside another subgraph, let that subgraph's
                    // padding handle arrow/label clearance. This rule is specifically for
                    // edges that exit a subgraph into open (non-subgraph) space.
                    if input.graph.get_node_subgraph(&edge.to).is_some() {
                        continue;
                    }
                    let required_target_y = env.outer.bottom().saturating_add(1);
                    if to_rect.y >= required_target_y {
                        continue;
                    }
                    let Some(rank) = placement.ranks.get(&edge.to) else {
                        continue;
                    };
                    let delta = required_target_y - to_rect.y;
                    required_shift_by_rank
                        .entry(*rank)
                        .and_modify(|d| *d = (*d).max(delta))
                        .or_insert(delta);
                }
            }

            let Some((&min_rank, &delta_y)) = required_shift_by_rank.iter().min_by_key(|(r, _)| *r)
            else {
                break;
            };

            shift_nodes_from_rank_td(
                &mut placement.positions,
                &mut placement.node_rects,
                &placement.ranks,
                min_rank,
                delta_y,
            );

            let max_bottom = placement
                .node_rects
                .values()
                .map(|r| r.bottom())
                .max()
                .unwrap_or(placement.canvas.bottom());
            placement.canvas.height = placement.canvas.height.max(max_bottom);

            subgraph_envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
        }
    }

    compact_stacked_vertical_top_level_sibling_subgraphs(
        input.graph,
        &mut placement.positions,
        &mut placement.node_rects,
        config.subgraph_gutter,
        &mut placement.canvas.height,
    );
    subgraph_envelopes =
        compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
    adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);

    // BT: ensure clearance above subgraph top borders (for outgoing edges to external
    // targets above) and between stacked subgraphs (so connectors don't overwrite
    // titles/corners on adjacent borders).
    if input.graph.direction == Direction::BT && !subgraph_envelopes.is_empty() {
        for _ in 0..8 {
            let mut required_shift_by_rank: HashMap<usize, usize> = HashMap::new();

            let mut subgraph_max_rank: HashMap<&str, usize> = HashMap::new();
            for sg in &input.graph.subgraphs {
                let max_rank = subgraph_tree_rank_range(input.graph, &placement.ranks, &sg.id)
                    .map(|(_, max_rank)| max_rank);
                if let Some(r) = max_rank {
                    subgraph_max_rank.insert(sg.id.as_str(), r);
                }
            }

            // Keep at least one connector row between an external target box above and the
            // subgraph top border it is connected to.
            for (sg_id, env) in subgraph_envelopes.iter() {
                let Some(&shift_rank) = subgraph_max_rank.get(sg_id.as_str()) else {
                    continue;
                };
                for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                    let (exit_subgraphs, _) =
                        input.graph.edge_boundary_crossings(&edge.from, &edge.to);
                    if !exit_subgraphs.contains(&sg_id.as_str()) {
                        continue;
                    }
                    let Some(to_rect) = placement.node_rects.get(&edge.to) else {
                        continue;
                    };
                    // Only when the destination is above this envelope.
                    if to_rect.bottom() > env.outer.y.saturating_add(1) {
                        continue;
                    }
                    let required_border_y = to_rect.bottom().saturating_add(1);
                    if env.outer.y >= required_border_y {
                        continue;
                    }
                    let delta = required_border_y - env.outer.y;
                    required_shift_by_rank
                        .entry(shift_rank)
                        .and_modify(|d| *d = (*d).max(delta))
                        .or_insert(delta);
                }
            }

            // Ensure at least one connector row between a subgraph bottom border and any
            // external source node that feeds into content inside that envelope. In BT this
            // matters for both direct targets and visually nested parent envelopes; otherwise
            // an enlarged outer border can land on top of the lower source box.
            for (sg_id, env) in subgraph_envelopes.iter() {
                let Some(subgraph) = input.graph.get_subgraph(sg_id) else {
                    continue;
                };
                if subgraph.parent_id.is_none() && subgraph.child_ids.is_empty() {
                    continue;
                }
                for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                    let Some(from_rect) = placement.node_rects.get(&edge.from) else {
                        continue;
                    };
                    if input.graph.is_node_in_subgraph_tree(&edge.from, sg_id) {
                        continue;
                    }
                    if !input.graph.is_node_in_subgraph_tree(&edge.to, sg_id) {
                        continue;
                    }
                    if !rect_fully_inside(env.outer, *from_rect) {
                        continue;
                    }
                    // The source node must start at least one row below the outer envelope
                    // bottom so there is room for the routing connector between them.
                    let required_source_y = env.outer.bottom().saturating_add(1);
                    if from_rect.y >= required_source_y {
                        continue;
                    }
                    let Some(&rank) = placement.ranks.get(&edge.from) else {
                        continue;
                    };
                    let delta = required_source_y - from_rect.y;
                    required_shift_by_rank
                        .entry(rank)
                        .and_modify(|d| *d = (*d).max(delta))
                        .or_insert(delta);
                }
            }

            // Ensure at least one empty row between stacked subgraphs when an edge crosses
            // from the lower subgraph to the upper one (BT flows upward).
            for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                let (Some(from_sg), Some(to_sg)) = (
                    input.graph.get_node_subgraph(&edge.from),
                    input.graph.get_node_subgraph(&edge.to),
                ) else {
                    continue;
                };
                if from_sg == to_sg {
                    continue;
                }
                let (Some(from_env), Some(to_env)) = (
                    subgraph_envelopes.get(from_sg),
                    subgraph_envelopes.get(to_sg),
                ) else {
                    continue;
                };
                // In BT, `to_sg` is visually above `from_sg` (smaller y). Only skip if
                // subgraphs are truly nested (one fully inside the other).
                let is_nested = rect_fully_inside(from_env.outer, to_env.outer)
                    || rect_fully_inside(to_env.outer, from_env.outer);
                if is_nested {
                    continue;
                }
                let required_from_top = to_env.outer.bottom().saturating_add(1);
                if from_env.outer.y >= required_from_top {
                    continue;
                }
                let Some(&shift_rank) = subgraph_max_rank.get(from_sg) else {
                    continue;
                };
                let delta = required_from_top - from_env.outer.y;
                required_shift_by_rank
                    .entry(shift_rank)
                    .and_modify(|d| *d = (*d).max(delta))
                    .or_insert(delta);
            }

            let Some((&max_rank, &delta_y)) = required_shift_by_rank.iter().max_by_key(|(r, _)| *r)
            else {
                break;
            };

            shift_nodes_up_to_rank_bt(
                &mut placement.positions,
                &mut placement.node_rects,
                &placement.ranks,
                max_rank,
                delta_y,
            );

            let max_bottom = placement
                .node_rects
                .values()
                .map(|r| r.bottom())
                .max()
                .unwrap_or(placement.canvas.bottom());
            placement.canvas.height = placement.canvas.height.max(max_bottom);

            subgraph_envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
        }

        let mut incoming_into_subgraph_from: HashMap<(String, String), usize> = HashMap::new();
        for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
            let (_, enter_subgraphs) = input.graph.edge_boundary_crossings(&edge.from, &edge.to);
            for to_sg in enter_subgraphs {
                *incoming_into_subgraph_from
                    .entry((edge.from.clone(), to_sg.to_string()))
                    .or_default() += 1;
            }
        }

        let mut source_shifts: HashMap<String, usize> = HashMap::new();
        for (subgraph_id, env) in subgraph_envelopes.iter() {
            let has_title = input
                .graph
                .get_subgraph(subgraph_id)
                .and_then(|subgraph| subgraph.title.as_ref())
                .is_some();
            let contains_child_envelope = subgraph_envelopes.iter().any(|(other_id, other_env)| {
                other_id != subgraph_id && rect_fully_inside(env.outer, other_env.outer)
            });
            if !contains_child_envelope && !has_title {
                continue;
            }
            let required_source_y = env.outer.bottom().saturating_add(1);
            for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                let (Some(from_rect), Some(to_rect)) = (
                    placement.node_rects.get(&edge.from),
                    placement.node_rects.get(&edge.to),
                ) else {
                    continue;
                };
                if rect_fully_inside(env.outer, *from_rect)
                    || !rect_fully_inside(env.outer, *to_rect)
                {
                    continue;
                }
                if input.graph.get_node_subgraph(&edge.from).is_some() {
                    continue;
                }
                if !contains_child_envelope
                    && incoming_into_subgraph_from
                        .get(&(edge.from.clone(), subgraph_id.clone()))
                        .copied()
                        .unwrap_or(1)
                        <= 1
                {
                    continue;
                }
                let overlaps_envelope_horizontally =
                    from_rect.x < env.outer.right() && env.outer.x < from_rect.right();
                if !overlaps_envelope_horizontally || from_rect.y >= required_source_y {
                    continue;
                }

                let delta = required_source_y - from_rect.y;
                source_shifts
                    .entry(edge.from.clone())
                    .and_modify(|existing| *existing = (*existing).max(delta))
                    .or_insert(delta);
            }
        }

        if !source_shifts.is_empty() {
            shift_nodes_by_id_y(
                &mut placement.positions,
                &mut placement.node_rects,
                &source_shifts,
            );
            let max_bottom = placement
                .node_rects
                .values()
                .map(|r| r.bottom())
                .max()
                .unwrap_or(placement.canvas.bottom());
            placement.canvas.height = placement.canvas.height.max(max_bottom);
        }
    }

    // Warn about overlapping (but not nested) subgraphs that couldn't be resolved.
    if debug_timing && subgraph_envelopes.len() > 1 {
        let sg_ids: Vec<&String> = subgraph_envelopes.keys().collect();
        for i in 0..sg_ids.len() {
            for j in (i + 1)..sg_ids.len() {
                let env1 = &subgraph_envelopes[sg_ids[i]];
                let env2 = &subgraph_envelopes[sg_ids[j]];
                // Check if they intersect
                let intersects = env1.outer.x < env2.outer.right()
                    && env1.outer.right() > env2.outer.x
                    && env1.outer.y < env2.outer.bottom()
                    && env1.outer.bottom() > env2.outer.y;
                if intersects {
                    let nested = rect_fully_inside(env1.outer, env2.outer)
                        || rect_fully_inside(env2.outer, env1.outer);
                    if !nested {
                        eprintln!(
                            "termiflow: warning: subgraphs {} and {} overlap",
                            sg_ids[i], sg_ids[j]
                        );
                    }
                }
            }
        }
    }

    rebalance_titled_vertical_subgraph_content_x(
        input.graph,
        &mut placement.positions,
        &mut placement.node_rects,
        config.subgraph_gutter,
        &mut placement.canvas.width,
    );
    rebalance_titled_vertical_subgraph_content_y(
        input.graph,
        &mut placement.positions,
        &mut placement.node_rects,
        config.subgraph_gutter,
        &mut placement.canvas.height,
    );
    subgraph_envelopes =
        compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
    adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);

    enforce_declared_nested_envelopes(input.graph, &mut subgraph_envelopes);
    adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);

    // 4) Occupancy grid seeded with node padding and subgraph gutters (with carved portals).
    let t_grid = std::time::Instant::now();
    let mut grid = OccupancyGrid::new(
        placement.canvas.right()
            + config.min_horizontal_spacing
            + config.subgraph_gutter
            + config.min_horizontal_spacing,
        placement.canvas.bottom()
            + config.min_vertical_spacing
            + config.subgraph_gutter
            + config.min_vertical_spacing,
    );
    for rect in placement.node_rects.values() {
        grid.mark_rect(&rect.inflate(config.node_padding));
    }
    carve_node_portals(
        &mut grid,
        &placement.node_rects,
        &coords,
        config.node_padding,
        input.graph,
        &subgraph_envelopes,
    );
    // No additional carving for fan-outs; deterministic lanes are built during routing.
    mark_subgraph_rings(&mut grid, &subgraph_envelopes);
    if config.enable_portals {
        carve_subgraph_portals(&mut grid, &subgraph_envelopes, config.subgraph_gutter);
    }
    if debug_timing {
        eprintln!(
            "termiflow: grid {:?} ({}x{})",
            t_grid.elapsed(),
            grid.width,
            grid.height
        );
    }

    // 5) Route edges with Manhattan + obstacle avoidance.
    let mut routes: HashMap<usize, EdgeRoute> = HashMap::new();
    let warnings = Vec::new();
    let t_route = std::time::Instant::now();
    let mut outgoing_counts: HashMap<&str, usize> = HashMap::new();
    let mut incoming_counts: HashMap<&str, usize> = HashMap::new();
    for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
        *outgoing_counts.entry(edge.from.as_str()).or_default() += 1;
        *incoming_counts.entry(edge.to.as_str()).or_default() += 1;
    }
    route_selective_horizontal_cross_subgraph_fanin_groups(
        input.graph,
        &placement.node_rects,
        &subgraph_envelopes,
        &incoming_counts,
        &mut routes,
        &mut grid,
    );
    for (edge_idx, edge) in input.graph.edges.iter().enumerate() {
        if edge.is_back_edge {
            // Skip routing here; back-edges are handled by the cycle renderer.
            continue;
        }
        if routes.contains_key(&edge_idx) {
            continue;
        }

        if debug_timing {
            eprintln!("termiflow: route edge {} -> {}", edge.from, edge.to);
        }
        let from_rect = placement
            .node_rects
            .get(&edge.from)
            .cloned()
            .unwrap_or_default();
        let to_rect = placement
            .node_rects
            .get(&edge.to)
            .cloned()
            .unwrap_or_default();

        let out_degree = outgoing_counts
            .get(edge.from.as_str())
            .copied()
            .unwrap_or(0);
        let in_degree = incoming_counts.get(edge.to.as_str()).copied().unwrap_or(0);

        // Convergent edges (multiple sources into one target) render best when the renderer
        // owns the junction, so skip pre-routing here.
        if in_degree > 1 {
            if debug_timing {
                eprintln!("  skip edge {} due to convergent routing", edge_idx);
            }
            continue;
        }

        // Fan-outs look best when the renderer owns the shared junction.
        if out_degree > 1 {
            if debug_timing {
                eprintln!("  skip edge {} fan-out handled in renderer", edge_idx);
            }
            continue;
        }

        // Labeled fan-out / fan-in edges are better handled in the renderer so labels
        // can sit on clean junctions instead of fighting precomputed paths.
        if edge.label.is_some() && (out_degree > 1 || in_degree > 1) {
            if debug_timing {
                eprintln!("  skip edge {} labeled fan-out/fan-in", edge_idx);
            }
            continue;
        }

        let crosses_subgraph = input
            .graph
            .edge_crosses_subgraph_boundary(&edge.from, &edge.to);

        // Leave fan-out / fan-in edges that cross subgraph boundaries to the renderer so
        // they can share junctions cleanly instead of overlapping pre-routed lanes.
        if crosses_subgraph && (out_degree > 1 || in_degree > 1) {
            if debug_timing {
                eprintln!("  skip edge {} cross-subgraph fan routing", edge_idx);
            }
            continue;
        }

        // Any edge that crosses a subgraph boundary is rendered with portal-aware logic;
        // skip pre-routing to avoid stale paths that don't honor portals.
        if crosses_subgraph {
            continue;
        }

        // Compute avoid gutters (all subgraphs except those containing endpoints).
        let avoid_rects = gutters_to_avoid(
            input.graph,
            &subgraph_envelopes,
            edge_idx,
            &edge.from,
            &edge.to,
        );

        let from_sg = input.graph.get_node_subgraph(&edge.from);
        let to_sg = input.graph.get_node_subgraph(&edge.to);

        let start = edge_exit_point(from_rect, input.graph.direction);
        let end = edge_entry_point(to_rect, input.graph.direction);

        if debug_timing {
            eprintln!(
                "  start {:?} end {:?} avoid {}",
                start,
                end,
                avoid_rects.len()
            );
        }

        // Ensure endpoints are traversable even if padding or rings marked them as obstacles.
        grid.clear_point(start);
        grid.clear_point(end);

        // Deterministic fan-out / fan-in lanes for simple non-subgraph cases.
        if edge.label.is_none() {
            if let Some(route) = lane_route(
                start,
                end,
                from_rect,
                to_rect,
                input.graph.direction,
                out_degree,
                in_degree,
                config.node_padding.max(1),
            ) {
                grid.mark_path(&route);
                if debug_timing {
                    eprintln!("  lane route stored for edge {}", edge_idx);
                }
                routes.insert(edge_idx, route);
                continue;
            }
        }

        // Build waypoints: start → (portal exit?) → (portal enter?) → end.
        let mut checkpoints = vec![start];
        if config.enable_portals && from_sg != to_sg {
            if let Some(id) = from_sg {
                if let Some(env) = subgraph_envelopes.get(id) {
                    if let Some(p) = portal_point(env, PortalUse::Exit, input.graph.direction) {
                        checkpoints.push(p);
                        grid.clear_point(p);
                    }
                }
            }
            if let Some(id) = to_sg {
                if let Some(env) = subgraph_envelopes.get(id) {
                    if let Some(p) = portal_point(env, PortalUse::Enter, input.graph.direction) {
                        checkpoints.push(p);
                        grid.clear_point(p);
                    }
                }
            }
        }
        checkpoints.push(end);

        let mut combined = EdgeRoute::new();
        for pair in checkpoints.windows(2) {
            let (seg_start, seg_end) = (pair[0], pair[1]);
            if let Some(route) =
                route_with_obstacles_v2(seg_start, seg_end, &mut grid, &avoid_rects, &coords)
            {
                grid.mark_path(&route);
                for s in route.segments {
                    combined.push_segment(s.from, s.to);
                }
            } else {
                let route = fallback_manhattan_route(seg_start, seg_end, input.graph.direction);
                grid.mark_path(&route);
                for s in route.segments {
                    combined.push_segment(s.from, s.to);
                }
            }
        }

        if debug_timing {
            eprintln!(
                "  stored route {} with {} segments (checkpoints={})",
                edge_idx,
                combined.segments.len(),
                checkpoints.len()
            );
        }
        routes.insert(edge_idx, combined);
    }
    if debug_timing {
        eprintln!(
            "termiflow: routing {:?} ({} edges)",
            t_route.elapsed(),
            input.graph.edges.len()
        );
        eprintln!("termiflow: stored routes {}", routes.len());
    }

    Ok(LayoutOutput {
        positions: placement.positions,
        subgraph_envelopes,
        routes,
        canvas: placement.canvas,
        warnings,
        ranks: placement.ranks,
    })
}

/// Convenience helper: run the coarse layout and apply positions back to the graph.
pub fn apply_coarse_layout(
    mut graph: Graph,
    prior_positions: Option<HashMap<String, Point>>,
    config: CoarseLayoutConfig,
) -> Result<Graph> {
    let debug_timing = std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok();
    let t_start = std::time::Instant::now();

    // Ensure all nodes have valid dimensions before layout
    for node in graph.nodes.iter_mut() {
        if node.width == 0 {
            node.width = box_width(&node.label).max(BOX_MIN_WIDTH);
        }
    }

    // Detect cycles and mark back-edges so the renderer can add gutters.
    let has_cycles = mark_back_edges(&mut graph);
    if has_cycles {
        graph
            .warnings
            .push("termiflow: warning: Cycle detected, rendering back-edges in gutter".to_string());
    }

    let t_layout_start = std::time::Instant::now();
    let output = layout(
        LayoutInput {
            graph: &graph,
            prior_positions,
        },
        config,
    )?;
    if debug_timing {
        eprintln!("termiflow: layout core {:?}", t_layout_start.elapsed());
    }

    for node in graph.nodes.iter_mut() {
        if let Some(p) = output.positions.get(&node.id) {
            node.x = p.x;
            node.y = p.y;
        }
        if let Some(rank) = output.ranks.get(&node.id) {
            node.rank = *rank;
        }
    }

    for subgraph in graph.subgraphs.iter_mut() {
        if let Some(bounds) = output.subgraph_envelopes.get(&subgraph.id) {
            subgraph.bounds = crate::graph::Rectangle::new(
                bounds.outer.x,
                bounds.outer.y,
                bounds.outer.width,
                bounds.outer.height,
            );
            subgraph.inner_bounds = crate::graph::Rectangle::new(
                bounds.inner.x,
                bounds.inner.y,
                bounds.inner.width,
                bounds.inner.height,
            );
        }
    }

    if debug_timing {
        for (sg_id, bounds) in &output.subgraph_envelopes {
            eprintln!(
                "subgraph {} outer=({}, {}, {}x{}) inner=({}, {}, {}x{})",
                sg_id,
                bounds.outer.x,
                bounds.outer.y,
                bounds.outer.width,
                bounds.outer.height,
                bounds.inner.x,
                bounds.inner.y,
                bounds.inner.width,
                bounds.inner.height
            );
        }
        for node in &graph.nodes {
            eprintln!(
                "node {} @ ({}, {}) size {}x{}",
                node.id, node.x, node.y, node.width, node.height
            );
        }
    }

    graph.edge_routes = output.routes;

    for w in output.warnings {
        graph.warnings.push(w);
    }

    if debug_timing {
        for (idx, route) in &graph.edge_routes {
            eprintln!("termiflow: route {} segments {}", idx, route.segments.len());
            for (i, seg) in route.segments.iter().enumerate() {
                eprintln!(
                    "  seg[{}]: ({}, {}) -> ({}, {})",
                    i, seg.from.x, seg.from.y, seg.to.x, seg.to.y
                );
            }
        }
    }

    if debug_timing {
        eprintln!("termiflow: apply {:?}", t_start.elapsed());
    }

    Ok(graph)
}

fn adjust_portal_slots_for_title(envelopes: &mut HashMap<String, SubgraphEnvelope>, graph: &Graph) {
    // BT titles are drawn on the bottom border row. Any bottom-border portal slots
    // must stay out of that title span (including its surrounding spaces).
    if !matches!(graph.direction, Direction::BT) {
        return;
    }

    for sg in &graph.subgraphs {
        let Some(title) = sg.title.as_deref() else {
            continue;
        };
        let Some(env) = envelopes.get_mut(&sg.id) else {
            continue;
        };

        let Some((start, end)) =
            crate::graph::subgraph_title_span(env.outer.x, env.outer.width, title, graph.direction)
        else {
            continue;
        };
        let min_x = env.outer.x.saturating_add(1);
        let max_x = env.outer.right().saturating_sub(2);
        if max_x < min_x {
            continue;
        }

        let shift_out_of_span = |x: usize| -> usize {
            let protected_start = start.saturating_sub(2);
            let protected_end = end.saturating_add(2).min(max_x);
            if x < protected_start || x > protected_end {
                return x;
            }
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
        };

        if !env.portals.bottom.is_empty() {
            let mut shifted = HashSet::new();
            for &x in &env.portals.bottom {
                let cx = x.clamp(min_x, max_x);
                shifted.insert(shift_out_of_span(cx));
            }
            env.portals.bottom = shifted;
        }
    }
}

/// Backwards-compatible alias for callers using the previous spike API.
#[deprecated(note = "Use apply_coarse_layout instead")]
pub fn apply_spike_layout(
    graph: Graph,
    prior_positions: Option<HashMap<String, Point>>,
    config: CoarseLayoutConfig,
) -> Result<Graph> {
    apply_coarse_layout(graph, prior_positions, config)
}

// -----------------------------------------------------------------------------
// Placement
// -----------------------------------------------------------------------------

/// Row spacing for simple edges without labels (minimal: stem → arrow)
const SPACING_MINIMAL: usize = 2;
/// Row spacing for labeled edges (stem → label → arrow)
const SPACING_LABELED: usize = 3;
/// Row spacing for fan-in (convergent) edges without labels (stems → junction → arrow)
const SPACING_FANIN: usize = 3;
/// Row spacing for fan-out (divergent) edges without labels (stem → junction → drops → arrows)
const SPACING_FANOUT: usize = 1;
/// Row spacing for multi-target edges with labels (stem → junction → label → arrow)
const SPACING_MULTI_LABELED: usize = 4;

#[derive(Debug)]
struct Placement {
    positions: HashMap<String, Point>,
    node_rects: HashMap<String, Rect>,
    canvas: Rect,
    ranks: HashMap<String, usize>,
}

fn gap_for_axis(axis: Axis, cfg: &CoarseLayoutConfig) -> usize {
    match axis {
        Axis::Horizontal => cfg.min_horizontal_spacing,
        Axis::Vertical => cfg.min_vertical_spacing,
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct LayoutSpacingPolicy {
    gutter: usize,
    node_padding: usize,
    min_horizontal: usize,
    min_vertical: usize,
}

impl LayoutSpacingPolicy {
    fn new(gutter: usize, node_padding: usize, min_horizontal: usize, min_vertical: usize) -> Self {
        Self {
            gutter,
            node_padding,
            min_horizontal,
            min_vertical,
        }
    }

    fn spacing_for_layer(&self, graph: &Graph, layers: &[Vec<usize>], layer_idx: usize) -> usize {
        let layer = &layers[layer_idx];

        // Check fan-out: source (in this layer) has multiple targets
        let mut has_fan_out = false;
        for &idx in layer {
            let source_id = &graph.nodes[idx].id;
            let target_count = graph
                .edges
                .iter()
                .filter(|e| !e.is_back_edge && &e.from == source_id)
                .count();
            if target_count > 1 {
                has_fan_out = true;
                break;
            }
        }

        // Check fan-in: target (in next layer) has multiple sources from this layer
        let mut has_fan_in = false;
        if layer_idx + 1 < layers.len() {
            for &idx in &layers[layer_idx + 1] {
                let target_id = &graph.nodes[idx].id;
                let source_count = graph
                    .edges
                    .iter()
                    .filter(|e| {
                        !e.is_back_edge
                            && &e.to == target_id
                            && layer
                                .iter()
                                .any(|&src_idx| graph.nodes[src_idx].id == e.from)
                    })
                    .count();
                if source_count > 1 {
                    has_fan_in = true;
                    break;
                }
            }
        }

        // Check for labeled edges from this rank
        let has_labels = layer.iter().any(|&idx| {
            let source_id = &graph.nodes[idx].id;
            graph
                .edges
                .iter()
                .any(|e| !e.is_back_edge && &e.from == source_id && e.label.is_some())
        });

        // Detect fan-out that targets a single subgraph to allow tighter vertical spacing.
        let fanout_targets_same_subgraph = if has_fan_out {
            let mut subgraph_ids: HashSet<&str> = HashSet::new();
            for &idx in layer {
                let source_id = &graph.nodes[idx].id;
                for e in graph
                    .edges
                    .iter()
                    .filter(|e| !e.is_back_edge && &e.from == source_id)
                {
                    if let Some(sg) = graph.get_node_subgraph(&e.to) {
                        subgraph_ids.insert(sg);
                    } else {
                        subgraph_ids.insert("");
                    }
                }
            }
            subgraph_ids.len() == 1
        } else {
            false
        };

        let external_boundary_target_count = if has_fan_out && layer_idx + 1 < layers.len() {
            let mut targets: HashSet<&str> = HashSet::new();
            for &src_idx in layer {
                let source_id = &graph.nodes[src_idx].id;
                let source_sg = graph.get_node_subgraph(source_id);
                for &dst_idx in &layers[layer_idx + 1] {
                    let target_id = &graph.nodes[dst_idx].id;
                    let target_sg = graph.get_node_subgraph(target_id);
                    if source_sg == target_sg {
                        continue;
                    }
                    if graph.edges.iter().any(|edge| {
                        !edge.is_back_edge && edge.from == *source_id && edge.to == *target_id
                    }) {
                        targets.insert(target_id.as_str());
                    }
                }
            }
            targets.len()
        } else {
            0
        };

        // Base spacing by flow shape
        let mut spacing = if has_fan_out || has_fan_in {
            if has_labels {
                SPACING_MULTI_LABELED
            } else if has_fan_out {
                SPACING_FANOUT
            } else {
                SPACING_FANIN
            }
        } else if has_labels {
            SPACING_LABELED
        } else {
            SPACING_MINIMAL
        };

        // When a boundary simultaneously contains fan-out and fan-in (diamond-ish shapes),
        // keep extra rows/cols so merge/junction bars don't collide with boxes.
        if has_fan_out && has_fan_in && !has_labels {
            spacing = spacing.max(SPACING_FANIN + 1);
        }

        // Subgraph boundary inflation between this layer and the next
        let mut boundary_crosses_subgraph = false;
        let mut crossing_into_titled_subgraph = false;
        if !graph.subgraphs.is_empty() && layer_idx + 1 < layers.len() {
            for &src_idx in layer {
                let src_id = &graph.nodes[src_idx].id;
                let src_sg = graph.get_node_subgraph(src_id);
                for &dst_idx in &layers[layer_idx + 1] {
                    let dst_id = &graph.nodes[dst_idx].id;
                    let dst_sg = graph.get_node_subgraph(dst_id);
                    if src_sg != dst_sg {
                        boundary_crosses_subgraph = true;
                        if let Some(sg_id) = dst_sg {
                            if let Some(sg) = graph.get_subgraph(sg_id) {
                                if let Some(title) = sg.title.as_ref() {
                                    // Rough fit check: the title text should fit inside the widest node plus modest padding.
                                    let title_len = crate::graph::subgraph_title_len(title);
                                    let widest_node = graph
                                        .nodes
                                        .iter()
                                        .filter(|n| sg.contains_node(&n.id))
                                        .map(|n| n.width)
                                        .max()
                                        .unwrap_or(0);
                                    if title_len <= widest_node.saturating_add(6) {
                                        crossing_into_titled_subgraph = true;
                                    }
                                }
                            }
                        }
                        break;
                    }
                }
                if boundary_crosses_subgraph {
                    break;
                }
            }

            if boundary_crosses_subgraph {
                if !has_fan_out && !has_fan_in && !has_labels {
                    // Leave a visible connector row plus an arrow head before the next node.
                    spacing = if crossing_into_titled_subgraph {
                        if matches!(graph.direction, Direction::TD | Direction::TB) {
                            SPACING_MINIMAL + 1
                        } else {
                            SPACING_MINIMAL + 2
                        }
                    } else {
                        SPACING_MINIMAL + 1
                    };
                } else {
                    let extra = if fanout_targets_same_subgraph {
                        self.gutter.saturating_sub(1)
                    } else if has_fan_out && has_fan_in {
                        self.gutter
                    } else if has_fan_out {
                        self.gutter * 2
                    } else {
                        self.gutter
                    };
                    spacing += extra;
                    spacing = spacing.max(SPACING_MINIMAL + 2);
                }

                // Fan-outs into a single subgraph can be tighter because the subgraph
                // itself reserves internal rows for trunk/split/drop rendering.
                if has_fan_out {
                    if fanout_targets_same_subgraph {
                        spacing = spacing.max(SPACING_MINIMAL + 2);
                    } else if external_boundary_target_count <= 1
                        && matches!(graph.direction, Direction::TD | Direction::TB)
                    {
                        // Mixed fan-outs that only pierce one sibling boundary do not need the
                        // full oversized cross-subgraph gap. The destination subgraph already
                        // reserves its own entry rows, so keep this boundary compact.
                        spacing = spacing.max(SPACING_MINIMAL + 3);
                    } else {
                        spacing = spacing.max(SPACING_MINIMAL + 5);
                    }
                }
            }
        }

        if boundary_crosses_subgraph && has_labels && !has_fan_out && !has_fan_in {
            spacing = spacing.saturating_sub(2).max(SPACING_LABELED + 1);
        }

        if fanout_targets_same_subgraph {
            // Leave a modest cushion for the junction row while keeping fan-outs compact.
            spacing = spacing.max(SPACING_FANOUT + 2);
        }

        // Horizontal layouts need a bit more primary gap for fan-outs to give
        // elbows/dashes room before hitting the targets.
        if matches!(graph.direction, Direction::LR | Direction::RL) && has_fan_out {
            spacing = spacing.max(SPACING_FANOUT + 4);
        }

        // Aspect ratio compensation for LR/RL layouts.
        // Terminal characters are ~2:1 height:width ratio, so horizontal layouts
        // need proportionally more spacing along the primary (horizontal) axis.
        // For complex topologies (fan-out, fan-in, labels) we apply a 2x multiplier.
        // For simple chains we honour the configured minimum horizontal spacing, which
        // already encodes the 2x compensation via SpacingConfig::for_direction.
        if matches!(graph.direction, Direction::LR | Direction::RL) {
            if !has_fan_out && !has_fan_in && !has_labels {
                spacing = self.min_horizontal.max(spacing * 2);
            } else {
                spacing *= 2;
            }
        }

        spacing
    }
}

fn compute_primary_gaps(
    graph: &Graph,
    layers: &[Vec<usize>],
    _coords: &OrientedCoords,
    config: &CoarseLayoutConfig,
) -> Vec<usize> {
    let mut gaps = Vec::with_capacity(layers.len());
    let policy = LayoutSpacingPolicy::new(
        config.subgraph_gutter,
        config.node_padding,
        config.min_horizontal_spacing,
        config.min_vertical_spacing,
    );
    for r in 0..layers.len() {
        gaps.push(policy.spacing_for_layer(graph, layers, r));
    }
    gaps
}

fn place_nodes(
    graph: &Graph,
    layers: &[Vec<usize>],
    coords: &OrientedCoords,
    config: &CoarseLayoutConfig,
    prior_positions: Option<&HashMap<String, Point>>,
) -> Placement {
    let mut positions: HashMap<String, Point> = HashMap::new();
    let mut node_rects: HashMap<String, Rect> = HashMap::new();
    let mut ranks: HashMap<String, usize> = HashMap::new();

    // 1. Calculate Primary Positions (Ranks)
    let primary_gaps = compute_primary_gaps(graph, layers, coords, config);

    // Compute primary offsets per layer (cumulative max extent + gap)
    let mut primary_offsets: Vec<usize> = Vec::with_capacity(layers.len());
    let mut primary_cursor = 0usize;
    for (i, layer) in layers.iter().enumerate() {
        let max_extent = layer
            .iter()
            .map(|idx| node_extent_primary(&graph.nodes[*idx], coords))
            .max()
            .unwrap_or(BOX_HEIGHT);

        primary_offsets.push(primary_cursor);

        let gap = if i < primary_gaps.len() {
            primary_gaps[i]
        } else {
            config.min_vertical_spacing
        };
        primary_cursor = primary_cursor + max_extent + gap;
    }

    let secondary_gap = gap_for_axis(coords.secondary, config);

    // 2. Calculate Secondary Positions (Barycenter / Median Alignment)
    for (layer_idx, layer) in layers.iter().enumerate() {
        let primary_pos = primary_offsets[layer_idx];
        let mut secondary_cursor = 0usize;

        for &node_idx in layer {
            let node = &graph.nodes[node_idx];
            let extent_sec = node_extent_secondary(node, coords);

            // Calculate desired secondary position based on parents (barycenter)
            let mut parent_centers = Vec::new();
            for edge in &graph.edges {
                if !edge.is_back_edge && edge.to == node.id {
                    if let Some(parent_rect) = node_rects.get(&edge.from) {
                        let center = match coords.secondary {
                            Axis::Horizontal => parent_rect.x + parent_rect.width / 2,
                            Axis::Vertical => parent_rect.y + parent_rect.height / 2,
                        };
                        parent_centers.push(center);
                    }
                }
            }

            let has_incoming = graph
                .edges
                .iter()
                .any(|e| !e.is_back_edge && e.to == node.id);

            if std::env::var("DEBUG_FANIN").is_ok() && node.id == "Merge" {
                eprintln!(
                    "layout fanin node={} parents={:?} incoming_edges={}",
                    node.id, parent_centers, has_incoming
                );
            }

            let desired_center = if !parent_centers.is_empty() {
                let sum: usize = parent_centers.iter().sum();
                sum / parent_centers.len()
            } else if has_incoming && layer_idx > 0 {
                // Fall back to centering on the preceding layer when parents exist
                // but haven't been placed (e.g., subgraph portal alignment).
                let mut prev_centers = Vec::new();
                for &prev_idx in &layers[layer_idx - 1] {
                    if let Some(rect) = node_rects.get(&graph.nodes[prev_idx].id) {
                        let center = match coords.secondary {
                            Axis::Horizontal => rect.x + rect.width / 2,
                            Axis::Vertical => rect.y + rect.height / 2,
                        };
                        prev_centers.push(center);
                    }
                }

                if !prev_centers.is_empty() {
                    let sum: usize = prev_centers.iter().sum();
                    sum / prev_centers.len()
                } else if let Some(prior) = prior_positions.as_ref().and_then(|m| m.get(&node.id)) {
                    match coords.secondary {
                        Axis::Horizontal => prior.x + node.width / 2,
                        Axis::Vertical => prior.y + node.height / 2,
                    }
                } else {
                    0
                }
            } else if let Some(prior) = prior_positions.as_ref().and_then(|m| m.get(&node.id)) {
                match coords.secondary {
                    Axis::Horizontal => prior.x + node.width / 2,
                    Axis::Vertical => prior.y + node.height / 2,
                }
            } else {
                0
            };

            let desired_start = desired_center.saturating_sub(extent_sec / 2);
            let secondary_pos = desired_start.max(secondary_cursor);

            if std::env::var("DEBUG_FANIN").is_ok() && node.id == "Merge" {
                eprintln!(
                    "place {} desired_center={} extent={} start={} cursor={} -> pos={}",
                    node.id,
                    desired_center,
                    extent_sec,
                    desired_start,
                    secondary_cursor,
                    secondary_pos
                );
            }

            let mut x = 0usize;
            let mut y = 0usize;
            coords.set_primary(&mut x, &mut y, primary_pos);
            coords.set_secondary(&mut x, &mut y, secondary_pos);

            positions.insert(node.id.clone(), Point::new(x, y));
            node_rects.insert(node.id.clone(), Rect::new(x, y, node.width, node.height));
            ranks.insert(node.id.clone(), layer_idx);

            secondary_cursor = secondary_pos + extent_sec + secondary_gap;
        }
    }

    // 3. Balance Coordinates (Iterative refinement)
    balance_coordinates(
        graph,
        &mut positions,
        &mut node_rects,
        layers,
        coords,
        config,
    );

    if std::env::var("DEBUG_FANIN").is_ok() {
        if let Some(rect) = node_rects.get("Merge") {
            eprintln!("post-balance Merge rect {:?}", rect);
        }
        if let Some(rect) = node_rects.get("S1") {
            eprintln!("post-balance S1 rect {:?}", rect);
        }
    }

    // Normalize coordinates (shift everything so min_x/min_y is 0)
    let min_x = node_rects.values().map(|r| r.x).min().unwrap_or(0);
    let min_y = node_rects.values().map(|r| r.y).min().unwrap_or(0);

    if std::env::var("DEBUG_FANIN").is_ok() {
        eprintln!("normalize min_x={} min_y={}", min_x, min_y);
    }

    if min_x > 0 || min_y > 0 {
        for p in positions.values_mut() {
            p.x = p.x.saturating_sub(min_x);
            p.y = p.y.saturating_sub(min_y);
        }
        for r in node_rects.values_mut() {
            r.x = r.x.saturating_sub(min_x);
            r.y = r.y.saturating_sub(min_y);
        }
    }

    let mut post_normalize_canvas_height =
        node_rects.values().map(|r| r.bottom()).max().unwrap_or(0);

    reserve_titled_horizontal_subgraph_headroom(
        graph,
        &mut positions,
        &mut node_rects,
        config.subgraph_gutter,
        &mut post_normalize_canvas_height,
    );

    if std::env::var("DEBUG_FANIN").is_ok() {
        if let Some(rect) = node_rects.get("Merge") {
            eprintln!("post-normalize Merge rect {:?}", rect);
        }
        if let Some(rect) = node_rects.get("S1") {
            eprintln!("post-normalize S1 rect {:?}", rect);
        }
    }

    // Compute canvas bounds
    let max_x = node_rects
        .values()
        .map(|r| r.right() + config.min_horizontal_spacing)
        .max()
        .unwrap_or(0);
    let max_y = node_rects
        .values()
        .map(|r| r.bottom() + config.min_vertical_spacing)
        .max()
        .unwrap_or(0);

    let canvas = Rect::new(0, 0, max_x + 1, max_y + 1);

    Placement {
        positions,
        node_rects,
        canvas,
        ranks,
    }
}

fn assign_layers(graph: &Graph) -> Vec<Vec<usize>> {
    let mut index_map: HashMap<&str, usize> = HashMap::new();
    for (idx, node) in graph.nodes.iter().enumerate() {
        index_map.insert(&node.id, idx);
    }

    let mut indegree = vec![0usize; graph.nodes.len()];
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); graph.nodes.len()];
    for edge in &graph.edges {
        if edge.is_back_edge {
            continue;
        }
        if let (Some(&from_idx), Some(&to_idx)) = (
            index_map.get(edge.from.as_str()),
            index_map.get(edge.to.as_str()),
        ) {
            indegree[to_idx] += 1;
            adj[from_idx].push(to_idx);
        }
    }

    let mut queue: VecDeque<usize> = indegree
        .iter()
        .enumerate()
        .filter_map(|(idx, &deg)| if deg == 0 { Some(idx) } else { None })
        .collect();

    let mut order = Vec::new();
    let mut rank = vec![0usize; graph.nodes.len()];
    while let Some(u) = queue.pop_front() {
        order.push(u);
        for &v in &adj[u] {
            if indegree[v] > 0 {
                indegree[v] -= 1;
                if indegree[v] == 0 {
                    rank[v] = rank[u] + 1;
                    queue.push_back(v);
                }
            }
        }
    }

    // Any nodes not processed (cycles/disconnected) keep rank 0 but deterministic position
    for idx in 0..graph.nodes.len() {
        if !order.contains(&idx) {
            order.push(idx);
        }
    }

    promote_nested_child_root_ranks(graph, &index_map, &adj, &order, &mut rank);

    let mut by_rank: HashMap<usize, Vec<usize>> = HashMap::new();
    for (idx, r) in rank.iter().enumerate() {
        by_rank.entry(*r).or_default().push(idx);
    }

    let max_rank = *rank.iter().max().unwrap_or(&0);
    let mut layers: Vec<Vec<usize>> = Vec::with_capacity(max_rank + 1);
    for r in 0..=max_rank {
        let mut layer = by_rank.remove(&r).unwrap_or_default();
        layer.sort_by_key(|idx| graph.nodes[*idx].id.clone());
        layers.push(layer);
    }

    layers
}

fn promote_nested_child_root_ranks(
    graph: &Graph,
    index_map: &HashMap<&str, usize>,
    adj: &[Vec<usize>],
    order: &[usize],
    rank: &mut [usize],
) {
    let mut nested_subgraphs: Vec<_> = graph
        .subgraphs
        .iter()
        .filter(|subgraph| subgraph.parent_id.is_some())
        .collect();
    nested_subgraphs.sort_by_key(|subgraph| subgraph.parent_id.clone());

    let mut promoted = false;

    for subgraph in nested_subgraphs {
        let Some(parent_id) = subgraph.parent_id.as_deref() else {
            continue;
        };
        let Some(parent) = graph.get_subgraph(parent_id) else {
            continue;
        };

        let Some(parent_direct_max_rank) = parent
            .node_ids
            .iter()
            .filter_map(|node_id| index_map.get(node_id.as_str()).copied())
            .map(|idx| rank[idx])
            .max()
        else {
            continue;
        };

        let minimum_child_rank = parent_direct_max_rank.saturating_add(1);
        for child_root_idx in nested_child_root_indices(graph, index_map, &subgraph.id) {
            if rank[child_root_idx] < minimum_child_rank {
                rank[child_root_idx] = minimum_child_rank;
                promoted = true;
            }
        }
    }

    if !promoted {
        return;
    }

    for &from_idx in order {
        for &to_idx in &adj[from_idx] {
            let next_rank = rank[from_idx].saturating_add(1);
            if rank[to_idx] < next_rank {
                rank[to_idx] = next_rank;
            }
        }
    }
}

fn nested_child_root_indices(
    graph: &Graph,
    index_map: &HashMap<&str, usize>,
    subgraph_id: &str,
) -> Vec<usize> {
    graph
        .nodes
        .iter()
        .filter(|node| graph.is_node_in_subgraph_tree(&node.id, subgraph_id))
        .filter(|node| {
            !graph.edges.iter().any(|edge| {
                !edge.is_back_edge
                    && edge.to == node.id
                    && graph.is_node_in_subgraph_tree(&edge.from, subgraph_id)
            })
        })
        .filter_map(|node| index_map.get(node.id.as_str()).copied())
        .collect()
}

fn node_extent_primary(node: &crate::graph::Node, coords: &OrientedCoords) -> usize {
    match coords.primary {
        Axis::Vertical => node.height,
        Axis::Horizontal => node.width,
    }
}

fn node_extent_secondary(node: &crate::graph::Node, coords: &OrientedCoords) -> usize {
    match coords.secondary {
        Axis::Vertical => node.height,
        Axis::Horizontal => node.width,
    }
}

fn mark_back_edges(graph: &mut Graph) -> bool {
    if graph.nodes.is_empty() || graph.edges.is_empty() {
        return false;
    }

    let mut index_map: HashMap<&str, usize> = HashMap::new();
    for (idx, node) in graph.nodes.iter().enumerate() {
        index_map.insert(&node.id, idx);
    }

    // Build adjacency with edge indices for DFS
    let mut adj: Vec<Vec<(usize, usize)>> = vec![Vec::new(); graph.nodes.len()];
    for (edge_idx, edge) in graph.edges.iter().enumerate() {
        let (Some(&from_idx), Some(&to_idx)) = (
            index_map.get(edge.from.as_str()),
            index_map.get(edge.to.as_str()),
        ) else {
            continue;
        };
        adj[from_idx].push((to_idx, edge_idx));
    }

    let mut state = vec![0u8; graph.nodes.len()]; // 0=unvisited,1=visiting,2=done
    let mut has_cycle = false;
    let mut seen_edges: HashSet<usize> = HashSet::new();

    fn dfs(
        u: usize,
        state: &mut [u8],
        adj: &[Vec<(usize, usize)>],
        edges: &mut [crate::graph::Edge],
        has_cycle: &mut bool,
        seen_edges: &mut HashSet<usize>,
    ) {
        state[u] = 1;
        for &(v, edge_idx) in &adj[u] {
            match state[v] {
                0 => dfs(v, state, adj, edges, has_cycle, seen_edges),
                1 => {
                    *has_cycle = true;
                    if seen_edges.insert(edge_idx) {
                        if let Some(edge) = edges.get_mut(edge_idx) {
                            edge.is_back_edge = true;
                        }
                    }
                }
                _ => {}
            }
        }
        state[u] = 2;
    }

    for u in 0..graph.nodes.len() {
        if state[u] == 0 {
            dfs(
                u,
                &mut state,
                &adj,
                &mut graph.edges,
                &mut has_cycle,
                &mut seen_edges,
            );
        }
    }

    has_cycle
}

// -----------------------------------------------------------------------------
// Crossing Minimization (Legacy Barycenter)
// NOTE: This implementation is superseded by crate::crossing::CrossingMinimizer
// which provides adaptive convergence detection and median heuristic support.
// Keeping for reference and potential fallback scenarios.
// -----------------------------------------------------------------------------

#[allow(dead_code)]
#[deprecated(
    since = "0.2.0",
    note = "Use crate::crossing::CrossingMinimizer instead"
)]
fn optimize_layer_order(graph: &Graph, layers: &mut [Vec<usize>]) {
    // Run a few passes of barycenter minimization
    for _ in 0..4 {
        // Down sweep
        for i in 1..layers.len() {
            sort_layer(graph, layers, i, i - 1);
        }
        // Up sweep
        for i in (0..layers.len() - 1).rev() {
            sort_layer(graph, layers, i, i + 1);
        }
    }
}

#[allow(dead_code)]
fn sort_layer(graph: &Graph, layers: &mut [Vec<usize>], target_idx: usize, ref_idx: usize) {
    let ref_layer = layers[ref_idx].clone();
    let target_layer = &mut layers[target_idx];

    let barycenters = calculate_barycenters(graph, target_layer, &ref_layer);

    #[derive(Debug)]
    struct Cluster {
        nodes: Vec<usize>,
        avg_barycenter: f32,
    }

    let mut clusters: Vec<Cluster> = Vec::new();
    let mut subgraph_clusters: HashMap<String, usize> = HashMap::new();

    for &node_idx in target_layer.iter() {
        let node_id = &graph.nodes[node_idx].id;
        let sg_id = graph.get_node_subgraph(node_id);

        if let Some(sg) = sg_id {
            if let Some(&cluster_idx) = subgraph_clusters.get(sg) {
                clusters[cluster_idx].nodes.push(node_idx);
            } else {
                let idx = clusters.len();
                clusters.push(Cluster {
                    nodes: vec![node_idx],
                    avg_barycenter: 0.0,
                });
                subgraph_clusters.insert(sg.to_string(), idx);
            }
        } else {
            clusters.push(Cluster {
                nodes: vec![node_idx],
                avg_barycenter: 0.0,
            });
        }
    }

    for cluster in &mut clusters {
        let mut sum = 0.0;
        let mut count = 0.0;

        cluster.nodes.sort_by(|&a, &b| {
            let ba = barycenters.get(&a).copied().unwrap_or(f32::MAX);
            let bb = barycenters.get(&b).copied().unwrap_or(f32::MAX);
            ba.partial_cmp(&bb).unwrap_or(Ordering::Equal)
        });

        for &node_idx in &cluster.nodes {
            if let Some(&val) = barycenters.get(&node_idx) {
                sum += val;
                count += 1.0;
            }
        }

        cluster.avg_barycenter = if count > 0.0 { sum / count } else { f32::MAX };
    }

    clusters.sort_by(|a, b| {
        a.avg_barycenter
            .partial_cmp(&b.avg_barycenter)
            .unwrap_or(Ordering::Equal)
    });

    *target_layer = clusters.into_iter().flat_map(|c| c.nodes).collect();
}

#[allow(dead_code)]
fn calculate_barycenters(
    graph: &Graph,
    target_layer: &[usize],
    ref_layer: &[usize],
) -> HashMap<usize, f32> {
    let mut barycenters = HashMap::new();

    let ref_positions: HashMap<&str, usize> = ref_layer
        .iter()
        .enumerate()
        .map(|(i, &idx)| (graph.nodes[idx].id.as_str(), i))
        .collect();

    for &node_idx in target_layer {
        let node_id = &graph.nodes[node_idx].id;
        let mut sum = 0.0;
        let mut count = 0.0;

        for edge in &graph.edges {
            if edge.is_back_edge {
                continue;
            }

            let neighbor_id = if &edge.from == node_id {
                &edge.to
            } else if &edge.to == node_id {
                &edge.from
            } else {
                continue;
            };

            if let Some(&pos) = ref_positions.get(neighbor_id.as_str()) {
                sum += pos as f32;
                count += 1.0;
            }
        }

        if count > 0.0 {
            barycenters.insert(node_idx, sum / count);
        }
    }
    barycenters
}

// -----------------------------------------------------------------------------
// Coordinate Balancing
// -----------------------------------------------------------------------------

fn balance_coordinates(
    graph: &Graph,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    layers: &[Vec<usize>],
    coords: &OrientedCoords,
    config: &CoarseLayoutConfig,
) {
    for _ in 0..2 {
        for i in 1..layers.len() {
            apply_balance_pass(
                graph,
                positions,
                node_rects,
                &layers[i],
                &layers[0..i],
                coords,
                config,
                true,
            );
        }
        for i in (0..layers.len() - 1).rev() {
            apply_balance_pass(
                graph,
                positions,
                node_rects,
                &layers[i],
                &layers[i + 1..],
                coords,
                config,
                false,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_balance_pass(
    graph: &Graph,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    target_layer: &[usize],
    ref_layers: &[Vec<usize>],
    coords: &OrientedCoords,
    config: &CoarseLayoutConfig,
    is_down_sweep: bool,
) {
    let gap = gap_for_axis(coords.secondary, config);
    let mut min_pos = 0usize;

    for &node_idx in target_layer {
        let node_id = &graph.nodes[node_idx].id;
        let node_width = match coords.secondary {
            Axis::Horizontal => graph.nodes[node_idx].width,
            Axis::Vertical => graph.nodes[node_idx].height,
        };

        let mut sum_centers = 0.0;
        let mut count = 0.0;
        let current_pos = match coords.secondary {
            Axis::Horizontal => positions[node_id].x,
            Axis::Vertical => positions[node_id].y,
        };
        let incoming_count = graph
            .edges
            .iter()
            .filter(|e| !e.is_back_edge && &e.to == node_id)
            .count();
        let has_fan_out = graph
            .edges
            .iter()
            .filter(|e| !e.is_back_edge && &e.from == node_id)
            .count()
            > 1;
        let is_fanin_target = incoming_count > 1;
        let participates_in_fanin = graph
            .edges
            .iter()
            .filter(|e| !e.is_back_edge && &e.from == node_id)
            .any(|e| {
                graph
                    .edges
                    .iter()
                    .filter(|f| !f.is_back_edge && f.to == e.to)
                    .count()
                    > 1
            });

        for layer in ref_layers {
            for &ref_idx in layer {
                let ref_id = &graph.nodes[ref_idx].id;

                let connected = if is_down_sweep {
                    graph
                        .edges
                        .iter()
                        .any(|e| !e.is_back_edge && &e.from == ref_id && &e.to == node_id)
                } else {
                    graph
                        .edges
                        .iter()
                        .any(|e| !e.is_back_edge && &e.from == node_id && &e.to == ref_id)
                };

                if connected {
                    if let Some(rect) = node_rects.get(ref_id) {
                        let center = match coords.secondary {
                            Axis::Horizontal => rect.x + rect.width / 2,
                            Axis::Vertical => rect.y + rect.height / 2,
                        };
                        sum_centers += center as f32;
                        count += 1.0;
                    }
                }
            }
        }

        if count > 0.0 {
            let ideal_center = (sum_centers / count) as usize;
            let ideal_start = ideal_center.saturating_sub(node_width / 2);

            let proposed = ideal_start.max(min_pos);
            let clamp_for_fanin =
                !is_down_sweep && !has_fan_out && participates_in_fanin && !is_fanin_target;
            let new_pos = if !is_down_sweep && is_fanin_target {
                current_pos.max(min_pos)
            } else if clamp_for_fanin {
                proposed.min(current_pos).max(min_pos)
            } else {
                proposed
            };

            if let Some(p) = positions.get_mut(node_id) {
                match coords.secondary {
                    Axis::Horizontal => p.x = new_pos,
                    Axis::Vertical => p.y = new_pos,
                }
            }
            if let Some(r) = node_rects.get_mut(node_id) {
                match coords.secondary {
                    Axis::Horizontal => r.x = new_pos,
                    Axis::Vertical => r.y = new_pos,
                }
            }
            min_pos = new_pos + node_width + gap;
        } else {
            let current_pos = match coords.secondary {
                Axis::Horizontal => positions[node_id].x,
                Axis::Vertical => positions[node_id].y,
            };

            let new_pos = current_pos.max(min_pos);

            if new_pos != current_pos {
                if let Some(p) = positions.get_mut(node_id) {
                    match coords.secondary {
                        Axis::Horizontal => p.x = new_pos,
                        Axis::Vertical => p.y = new_pos,
                    }
                }
                if let Some(r) = node_rects.get_mut(node_id) {
                    match coords.secondary {
                        Axis::Horizontal => r.x = new_pos,
                        Axis::Vertical => r.y = new_pos,
                    }
                }
            }
            min_pos = new_pos + node_width + gap;
        }
    }
}

// -----------------------------------------------------------------------------
// Subgraphs
// -----------------------------------------------------------------------------

fn gutters_to_avoid(
    graph: &Graph,
    subgraph_envelopes: &HashMap<String, SubgraphEnvelope>,
    _edge_idx: usize,
    from: &str,
    to: &str,
) -> Vec<Rect> {
    // Skip gutters that contain either endpoint to avoid blocking exits.
    let mut avoid = Vec::new();
    for (sg_id, bounds) in subgraph_envelopes {
        let contains_endpoint = graph
            .node_subgraph
            .get(from)
            .map(|id| id == sg_id)
            .unwrap_or(false)
            || graph
                .node_subgraph
                .get(to)
                .map(|id| id == sg_id)
                .unwrap_or(false);
        if !contains_endpoint {
            avoid.push(bounds.outer);
        }
    }
    avoid
}

fn mark_subgraph_rings(grid: &mut OccupancyGrid, subgraphs: &HashMap<String, SubgraphEnvelope>) {
    for bounds in subgraphs.values() {
        let outer = bounds.outer;
        let inner = bounds.inner;
        if outer.is_empty() || inner.is_empty() {
            continue;
        }

        // Top band
        if inner.y > outer.y {
            grid.mark_rect(&Rect::new(
                outer.x,
                outer.y,
                outer.width,
                inner.y.saturating_sub(outer.y),
            ));
        }
        // Bottom band
        if outer.bottom() > inner.bottom() {
            grid.mark_rect(&Rect::new(
                outer.x,
                inner.bottom(),
                outer.width,
                outer.bottom().saturating_sub(inner.bottom()),
            ));
        }
        // Left band
        if inner.x > outer.x {
            grid.mark_rect(&Rect::new(
                outer.x,
                inner.y,
                inner.x.saturating_sub(outer.x),
                inner.height,
            ));
        }
        // Right band
        if outer.right() > inner.right() {
            grid.mark_rect(&Rect::new(
                inner.right(),
                inner.y,
                outer.right().saturating_sub(inner.right()),
                inner.height,
            ));
        }
    }
}

fn carve_node_portals(
    grid: &mut OccupancyGrid,
    node_rects: &HashMap<String, Rect>,
    coords: &OrientedCoords,
    padding: usize,
    graph: &Graph,
    subgraph_envelopes: &HashMap<String, SubgraphEnvelope>,
) {
    let ring_zones: Vec<&SubgraphEnvelope> = subgraph_envelopes.values().collect();

    for (node_id, rect) in node_rects {
        let entry = edge_entry_point(*rect, coords.direction);
        let exit = edge_exit_point(*rect, coords.direction);

        let (allowed_rect, in_subgraph) = graph
            .get_node_subgraph(node_id)
            .and_then(|sg_id| subgraph_envelopes.get(sg_id))
            .map(|b| (b.inner.inflate(padding.max(1)), true))
            .unwrap_or_else(|| (Rect::new(0, 0, grid.width, grid.height), false));

        // Determine clearing direction based on layout direction
        // Entry clears OUTWARDS from the box (opposite to flow into box)
        // Exit clears OUTWARDS from the box (with flow out of box)
        let (entry_dir, exit_dir) = match coords.direction {
            Direction::TD | Direction::TB => ((0, -1), (0, 1)),
            Direction::BT => ((0, 1), (0, -1)),
            Direction::LR => ((-1, 0), (1, 0)),
            Direction::RL => ((1, 0), (-1, 0)),
        };

        for i in 0..=padding {
            // Clear entry path
            if !in_subgraph {
                let ex = if entry_dir.0 < 0 {
                    entry.x.saturating_sub((-entry_dir.0 * i as isize) as usize)
                } else {
                    entry.x.saturating_add((entry_dir.0 * i as isize) as usize)
                };
                let ey = if entry_dir.1 < 0 {
                    entry.y.saturating_sub((-entry_dir.1 * i as isize) as usize)
                } else {
                    entry.y.saturating_add((entry_dir.1 * i as isize) as usize)
                };
                let entry_point = Point::new(ex, ey);
                let in_ring = ring_zones
                    .iter()
                    .any(|b| b.outer.contains(entry_point) && !b.inner.contains(entry_point));
                if allowed_rect.contains(entry_point) && !in_ring {
                    grid.clear_point(entry_point);
                }
            }

            // Clear exit path
            let xx = if exit_dir.0 < 0 {
                exit.x.saturating_sub((-exit_dir.0 * i as isize) as usize)
            } else {
                exit.x.saturating_add((exit_dir.0 * i as isize) as usize)
            };
            let xy = if exit_dir.1 < 0 {
                exit.y.saturating_sub((-exit_dir.1 * i as isize) as usize)
            } else {
                exit.y.saturating_add((exit_dir.1 * i as isize) as usize)
            };
            let exit_point = Point::new(xx, xy);
            let in_ring = ring_zones
                .iter()
                .any(|b| b.outer.contains(exit_point) && !b.inner.contains(exit_point));
            if allowed_rect.contains(exit_point) && !in_ring {
                grid.clear_point(exit_point);
            }
        }
    }
}

fn carve_subgraph_portals(
    grid: &mut OccupancyGrid,
    subgraphs: &HashMap<String, SubgraphEnvelope>,
    gutter: usize,
) {
    let debug_timing = std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok();

    let span = gutter.max(1) * 2 + 1;
    for (sg_id, bounds) in subgraphs {
        let portals = &bounds.portals;
        let clamp_h = |x: usize| {
            let min = bounds.outer.x.saturating_add(1);
            let max = bounds.outer.right().saturating_sub(2);
            x.clamp(min, max)
        };
        let clamp_v = |y: usize| {
            let min = bounds.outer.y.saturating_add(1);
            let max = bounds.outer.bottom().saturating_sub(2);
            y.clamp(min, max)
        };
        let half = span / 2;

        for &x in &portals.top {
            let cx = clamp_h(x);
            let start_x = cx.saturating_sub(half);
            let end_x = start_x + span;
            for y in bounds.outer.y..=bounds.inner.y {
                for xi in start_x..end_x {
                    grid.clear_point(Point::new(xi, y));
                }
            }
        }
        for &x in &portals.bottom {
            let cx = clamp_h(x);
            let start_x = cx.saturating_sub(half);
            let end_x = start_x + span;
            for y in bounds.inner.bottom()..=bounds.outer.bottom().saturating_sub(1) {
                for xi in start_x..end_x {
                    grid.clear_point(Point::new(xi, y));
                }
            }
        }
        for &y in &portals.left {
            let cy = clamp_v(y);
            let start_y = cy.saturating_sub(half);
            let end_y = start_y + span;
            for x in bounds.outer.x..=bounds.inner.x {
                for yi in start_y..end_y {
                    grid.clear_point(Point::new(x, yi));
                }
            }
        }
        for &y in &portals.right {
            let cy = clamp_v(y);
            let start_y = cy.saturating_sub(half);
            let end_y = start_y + span;
            for x in bounds.inner.right()..=bounds.outer.right().saturating_sub(1) {
                for yi in start_y..end_y {
                    grid.clear_point(Point::new(x, yi));
                }
            }
        }

        if debug_timing {
            eprintln!(
                "subgraph {} portals top={:?} bottom={:?} left={:?} right={:?}",
                sg_id, portals.top, portals.bottom, portals.left, portals.right
            );
        }
    }
}

enum PortalUse {
    Enter,
    Exit,
}

fn median_slot(slots: &HashSet<usize>, fallback: usize) -> usize {
    if slots.is_empty() {
        return fallback;
    }
    let mut vals: Vec<usize> = slots.iter().copied().collect();
    vals.sort_unstable();
    vals[vals.len() / 2]
}

fn portal_point(bounds: &SubgraphEnvelope, how: PortalUse, direction: Direction) -> Option<Point> {
    match (direction, how) {
        (Direction::TD | Direction::TB, PortalUse::Enter) => {
            let x = median_slot(&bounds.portals.top, bounds.outer.x + bounds.outer.width / 2);
            Some(Point::new(x, bounds.outer.y.saturating_add(1)))
        }
        (Direction::TD | Direction::TB, PortalUse::Exit) => {
            let x = median_slot(
                &bounds.portals.bottom,
                bounds.outer.x + bounds.outer.width / 2,
            );
            Some(Point::new(x, bounds.outer.bottom().saturating_sub(1)))
        }
        (Direction::BT, PortalUse::Enter) => {
            let x = median_slot(
                &bounds.portals.bottom,
                bounds.outer.x + bounds.outer.width / 2,
            );
            Some(Point::new(x, bounds.outer.bottom().saturating_sub(1)))
        }
        (Direction::BT, PortalUse::Exit) => {
            let x = median_slot(&bounds.portals.top, bounds.outer.x + bounds.outer.width / 2);
            Some(Point::new(x, bounds.outer.y))
        }
        (Direction::LR, PortalUse::Enter) => {
            let y = median_slot(
                &bounds.portals.left,
                bounds.outer.y + bounds.outer.height / 2,
            );
            Some(Point::new(bounds.outer.x, y))
        }
        (Direction::LR, PortalUse::Exit) => {
            let y = median_slot(
                &bounds.portals.right,
                bounds.outer.y + bounds.outer.height / 2,
            );
            Some(Point::new(bounds.outer.right().saturating_sub(1), y))
        }
        (Direction::RL, PortalUse::Enter) => {
            let y = median_slot(
                &bounds.portals.right,
                bounds.outer.y + bounds.outer.height / 2,
            );
            Some(Point::new(bounds.outer.right().saturating_sub(1), y))
        }
        (Direction::RL, PortalUse::Exit) => {
            let y = median_slot(
                &bounds.portals.left,
                bounds.outer.y + bounds.outer.height / 2,
            );
            Some(Point::new(bounds.outer.x, y))
        }
    }
}

fn push_route_leg(route: &mut EdgeRoute, from: Point, to: Point) {
    if from != to {
        route.push_segment(from, to);
    }
}

fn build_horizontal_cross_subgraph_fanin_route(
    start: Point,
    portal: Point,
    arrow: Point,
    direction: Direction,
    inner_lane_x: usize,
    outer_lane_x: usize,
) -> EdgeRoute {
    let mut route = EdgeRoute::new();
    let mut cursor = start;

    let source_lane = Point::new(inner_lane_x, cursor.y);
    push_route_leg(&mut route, cursor, source_lane);
    cursor = source_lane;

    let merge_lane = Point::new(inner_lane_x, portal.y);
    push_route_leg(&mut route, cursor, merge_lane);
    cursor = merge_lane;

    push_route_leg(&mut route, cursor, portal);
    cursor = portal;

    if portal.y == arrow.y {
        push_route_leg(&mut route, cursor, arrow);
        return route;
    }

    let outside_lane = Point::new(outer_lane_x, portal.y);
    push_route_leg(&mut route, cursor, outside_lane);

    let outside_turn = Point::new(outer_lane_x, arrow.y);
    push_route_leg(&mut route, outside_lane, outside_turn);

    match direction {
        Direction::LR | Direction::RL => push_route_leg(&mut route, outside_turn, arrow),
        _ => unreachable!(),
    }

    route
}

fn route_selective_horizontal_cross_subgraph_fanin_groups(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    subgraph_envelopes: &HashMap<String, SubgraphEnvelope>,
    incoming_counts: &HashMap<&str, usize>,
    routes: &mut HashMap<usize, EdgeRoute>,
    grid: &mut OccupancyGrid,
) {
    if !matches!(graph.direction, Direction::LR | Direction::RL) {
        return;
    }

    let mut grouped_by_target: HashMap<&str, Vec<usize>> = HashMap::new();
    for (edge_idx, edge) in graph.edges.iter().enumerate() {
        if edge.is_back_edge || edge.label.is_some() {
            continue;
        }
        if incoming_counts.get(edge.to.as_str()).copied().unwrap_or(0) < 2 {
            continue;
        }
        grouped_by_target
            .entry(edge.to.as_str())
            .or_default()
            .push(edge_idx);
    }

    for (target_id, edge_indices) in grouped_by_target {
        let Some(target) = graph.get_node(target_id) else {
            continue;
        };
        let Some(first_edge) = edge_indices
            .first()
            .and_then(|edge_idx| graph.edges.get(*edge_idx))
        else {
            continue;
        };
        let Some(source_sg_id) = graph.get_node_subgraph(&first_edge.from) else {
            continue;
        };
        if graph.get_node_subgraph(target_id) == Some(source_sg_id) {
            continue;
        }

        let Some(env) = subgraph_envelopes.get(source_sg_id) else {
            continue;
        };
        let Some(portal) = portal_point(env, PortalUse::Exit, graph.direction) else {
            continue;
        };

        let target_rect = node_rects
            .get(target_id)
            .copied()
            .unwrap_or_else(|| Rect::new(target.x, target.y, target.width, target.height));
        let arrow = edge_entry_point(target_rect, graph.direction);

        let mut starts = Vec::new();
        let mut all_from_same_subgraph = true;
        for edge_idx in &edge_indices {
            let Some(edge) = graph.edges.get(*edge_idx) else {
                all_from_same_subgraph = false;
                break;
            };
            if graph.get_node_subgraph(&edge.from) != Some(source_sg_id)
                || !graph.edge_crosses_subgraph_boundary(&edge.from, &edge.to)
            {
                all_from_same_subgraph = false;
                break;
            }
            let Some(source) = graph.get_node(&edge.from) else {
                all_from_same_subgraph = false;
                break;
            };
            let source_rect = node_rects
                .get(edge.from.as_str())
                .copied()
                .unwrap_or_else(|| Rect::new(source.x, source.y, source.width, source.height));
            starts.push((*edge_idx, edge_exit_point(source_rect, graph.direction)));
        }
        if !all_from_same_subgraph || starts.len() < 2 {
            continue;
        }

        let min_source_y = starts
            .iter()
            .map(|(_, start)| start.y)
            .min()
            .unwrap_or(portal.y);
        let max_source_y = starts
            .iter()
            .map(|(_, start)| start.y)
            .max()
            .unwrap_or(portal.y);
        if portal.y <= min_source_y || portal.y >= max_source_y {
            continue;
        }

        let Some((inner_lane_x, outer_lane_x)) = (match graph.direction {
            Direction::LR => {
                let max_exit_x = starts
                    .iter()
                    .map(|(_, start)| start.x)
                    .max()
                    .unwrap_or(portal.x);
                let desired_inner_lane_x = max_exit_x.saturating_add(1);
                let inner_lane_x = desired_inner_lane_x
                    .min(portal.x.saturating_sub(2))
                    .max(max_exit_x);
                let outer_lane_x = arrow.x.saturating_sub(1);
                (inner_lane_x < portal.x && outer_lane_x > portal.x)
                    .then_some((inner_lane_x, outer_lane_x))
            }
            Direction::RL => {
                let min_exit_x = starts
                    .iter()
                    .map(|(_, start)| start.x)
                    .min()
                    .unwrap_or(portal.x);
                let desired_inner_lane_x = min_exit_x.saturating_sub(1);
                let inner_lane_x = desired_inner_lane_x
                    .max(portal.x.saturating_add(2))
                    .min(min_exit_x);
                let outer_lane_x = arrow.x.saturating_add(1);
                (inner_lane_x > portal.x && outer_lane_x < portal.x)
                    .then_some((inner_lane_x, outer_lane_x))
            }
            _ => None,
        }) else {
            continue;
        };

        for (edge_idx, start) in starts {
            let route = build_horizontal_cross_subgraph_fanin_route(
                start,
                portal,
                arrow,
                graph.direction,
                inner_lane_x,
                outer_lane_x,
            );
            if route.segments.is_empty() {
                continue;
            }
            grid.mark_path(&route);
            routes.insert(edge_idx, route);
        }
    }
}

// -----------------------------------------------------------------------------
// Routing
// -----------------------------------------------------------------------------

const WEIGHT_FREE: u8 = 1;
const WEIGHT_EDGE: u8 = 10;
const WEIGHT_OBSTACLE: u8 = 255;
const COST_BEND: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    fn from_vec(dx: isize, dy: isize) -> Option<Self> {
        match (dx, dy) {
            (0, -1) => Some(Dir::Up),
            (0, 1) => Some(Dir::Down),
            (-1, 0) => Some(Dir::Left),
            (1, 0) => Some(Dir::Right),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
struct OccupancyGrid {
    width: usize,
    height: usize,
    weights: Vec<u8>,
}

impl OccupancyGrid {
    fn new(width: usize, height: usize) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            width,
            height,
            weights: vec![WEIGHT_FREE; width * height],
        }
    }

    fn in_bounds(&self, p: Point) -> bool {
        p.x < self.width && p.y < self.height
    }

    fn idx(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }

    fn mark_rect(&mut self, rect: &Rect) {
        if rect.is_empty() {
            return;
        }
        let x_end = rect.right().min(self.width);
        let y_end = rect.bottom().min(self.height);
        let x_start = rect.x.min(self.width);
        let y_start = rect.y.min(self.height);

        for y in y_start..y_end {
            let row_offset = y * self.width;
            for x in x_start..x_end {
                self.weights[row_offset + x] = WEIGHT_OBSTACLE;
            }
        }
    }

    fn clear_point(&mut self, p: Point) {
        if self.in_bounds(p) {
            let idx = self.idx(p.x, p.y);
            self.weights[idx] = WEIGHT_FREE;
        }
    }

    fn cost_at(&self, p: Point) -> u8 {
        if !self.in_bounds(p) {
            return WEIGHT_OBSTACLE;
        }
        self.weights[self.idx(p.x, p.y)]
    }

    fn mark_path(&mut self, route: &EdgeRoute) {
        for seg in &route.segments {
            // Determine direction and range
            if seg.from.x == seg.to.x {
                // Vertical
                let (min_y, max_y) = if seg.from.y < seg.to.y {
                    (seg.from.y, seg.to.y)
                } else {
                    (seg.to.y, seg.from.y)
                };
                for y in min_y..=max_y {
                    if y < self.height {
                        let idx = self.idx(seg.from.x, y);
                        // Don't overwrite hard obstacles, but do overwrite free/edge
                        if self.weights[idx] != WEIGHT_OBSTACLE {
                            self.weights[idx] = WEIGHT_EDGE;
                        }
                    }
                }
            } else {
                // Horizontal
                let (min_x, max_x) = if seg.from.x < seg.to.x {
                    (seg.from.x, seg.to.x)
                } else {
                    (seg.to.x, seg.from.x)
                };
                for x in min_x..=max_x {
                    if x < self.width {
                        let idx = self.idx(x, seg.from.y);
                        if self.weights[idx] != WEIGHT_OBSTACLE {
                            self.weights[idx] = WEIGHT_EDGE;
                        }
                    }
                }
            }
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
struct PathNode {
    cost: usize,
    estimate: usize,
    point: Point,
    arrival_dir: Option<Dir>,
}

impl Ord for PathNode {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap behavior using BinaryHeap
        (other.cost + other.estimate).cmp(&(self.cost + self.estimate))
    }
}

impl PartialOrd for PathNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn manhattan(a: Point, b: Point) -> usize {
    a.x.abs_diff(b.x) + a.y.abs_diff(b.y)
}

fn add_manhattan_segment(route: &mut EdgeRoute, from: Point, to: Point, direction: Direction) {
    if from == to {
        return;
    }
    if from.x == to.x || from.y == to.y {
        route.push_segment(from, to);
        return;
    }

    let mid = match direction {
        Direction::TD | Direction::TB | Direction::BT => Point::new(to.x, from.y),
        Direction::LR | Direction::RL => Point::new(from.x, to.y),
    };
    route.push_segment(from, mid);
    route.push_segment(mid, to);
}

#[allow(clippy::too_many_arguments)]
fn lane_route(
    start: Point,
    end: Point,
    from_rect: Rect,
    to_rect: Rect,
    direction: Direction,
    out_count: usize,
    in_count: usize,
    pad: usize,
) -> Option<EdgeRoute> {
    if out_count < 2 && in_count < 2 {
        return None;
    }

    let mut route = EdgeRoute::new();
    match direction {
        Direction::TD | Direction::TB => {
            if out_count > 1 {
                let lane_y = from_rect.bottom().saturating_add(pad);
                let mid_a = Point::new(start.x, lane_y);
                let mid_b = Point::new(end.x, lane_y);
                route.push_segment(start, mid_a);
                route.push_segment(mid_a, mid_b);
                route.push_segment(mid_b, end);
                return Some(route);
            }
            if in_count > 1 {
                let lane_y = to_rect.y.saturating_sub(pad);
                let mid_a = Point::new(start.x, lane_y);
                let mid_b = Point::new(end.x, lane_y);
                route.push_segment(start, mid_a);
                route.push_segment(mid_a, mid_b);
                route.push_segment(mid_b, end);
                return Some(route);
            }
        }
        Direction::BT => {
            if out_count > 1 {
                let lane_y = from_rect.y.saturating_sub(pad);
                let mid_a = Point::new(start.x, lane_y);
                let mid_b = Point::new(end.x, lane_y);
                route.push_segment(start, mid_a);
                route.push_segment(mid_a, mid_b);
                route.push_segment(mid_b, end);
                return Some(route);
            }
            if in_count > 1 {
                let lane_y = to_rect.bottom().saturating_add(pad);
                let mid_a = Point::new(start.x, lane_y);
                let mid_b = Point::new(end.x, lane_y);
                route.push_segment(start, mid_a);
                route.push_segment(mid_a, mid_b);
                route.push_segment(mid_b, end);
                return Some(route);
            }
        }
        Direction::LR => {
            if out_count > 1 {
                let lane_x = from_rect.right().saturating_add(pad);
                let mid_a = Point::new(lane_x, start.y);
                let mid_b = Point::new(lane_x, end.y);
                route.push_segment(start, mid_a);
                route.push_segment(mid_a, mid_b);
                route.push_segment(mid_b, end);
                return Some(route);
            }
            if in_count > 1 {
                let lane_x = to_rect.x.saturating_sub(pad);
                let mid_a = Point::new(lane_x, start.y);
                let mid_b = Point::new(lane_x, end.y);
                route.push_segment(start, mid_a);
                route.push_segment(mid_a, mid_b);
                route.push_segment(mid_b, end);
                return Some(route);
            }
        }
        Direction::RL => {
            if out_count > 1 {
                let lane_x = from_rect.x.saturating_sub(pad);
                let mid_a = Point::new(lane_x, start.y);
                let mid_b = Point::new(lane_x, end.y);
                route.push_segment(start, mid_a);
                route.push_segment(mid_a, mid_b);
                route.push_segment(mid_b, end);
                return Some(route);
            }
            if in_count > 1 {
                let lane_x = to_rect.right().saturating_add(pad);
                let mid_a = Point::new(lane_x, start.y);
                let mid_b = Point::new(lane_x, end.y);
                route.push_segment(start, mid_a);
                route.push_segment(mid_a, mid_b);
                route.push_segment(mid_b, end);
                return Some(route);
            }
        }
    }

    None
}

fn fallback_manhattan_route(start: Point, end: Point, direction: Direction) -> EdgeRoute {
    let mut route = EdgeRoute::new();
    add_manhattan_segment(&mut route, start, end, direction);
    route
}

fn route_with_obstacles(
    start: Point,
    end: Point,
    grid: &mut OccupancyGrid,
    avoid_rects: &[Rect],
    coords: &OrientedCoords,
) -> Option<EdgeRoute> {
    let debug_timing = std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok();
    if start == end {
        let mut route = EdgeRoute::new();
        route.push_segment(start, end);
        return Some(route);
    }

    let mut came_from: HashMap<Point, Point> = HashMap::new();
    let mut best_cost: HashMap<(Point, Option<Dir>), usize> = HashMap::new();
    // Track overall best cost to each point (regardless of direction) for came_from updates
    let mut best_cost_to_point: HashMap<Point, usize> = HashMap::new();
    let mut open = BinaryHeap::new();

    open.push(PathNode {
        cost: 0,
        estimate: manhattan(start, end),
        point: start,
        arrival_dir: None,
    });

    // Initial cost for start point (any direction)
    best_cost.insert((start, None), 0);
    best_cost_to_point.insert(start, 0);

    let mut found_end = false;
    let mut steps: usize = 0;
    let max_steps = grid
        .width
        .saturating_mul(grid.height)
        .saturating_mul(10)
        .max(10_000);

    while let Some(current) = open.pop() {
        steps += 1;
        if steps > max_steps {
            eprintln!(
                "termiflow: warning: routing aborted after {} steps ({:?} -> {:?})",
                steps, start, end
            );
            break;
        }
        if debug_timing && steps.is_multiple_of(500) {
            eprintln!(
                "    routing step {} at {:?} (open={})",
                steps,
                current.point,
                open.len()
            );
        }
        if current.point == end {
            found_end = true;
            break;
        }

        let neighbors = ordered_neighbors(current.point, end, coords);
        if debug_timing && steps <= 1 {
            for next in &neighbors {
                let cost = grid.cost_at(*next);
                let blocked = avoid_rects.iter().any(|r| r.contains(*next));
                eprintln!(
                    "    neighbor {:?} cost={} blocked_by_rect={}",
                    next, cost, blocked
                );
            }
        }
        for next in neighbors {
            // Check hard obstacles (rects)
            if avoid_rects.iter().any(|r| r.contains(next)) && next != end {
                continue;
            }

            // Check grid cost
            let cell_cost = grid.cost_at(next);
            if cell_cost == WEIGHT_OBSTACLE && next != end {
                continue;
            }

            // Calculate movement direction
            let dx = next.x as isize - current.point.x as isize;
            let dy = next.y as isize - current.point.y as isize;
            let move_dir = Dir::from_vec(dx, dy);

            // Calculate new cost
            let mut new_cost = current.cost + cell_cost as usize;

            // Add bend penalty
            if let (Some(prev), Some(curr)) = (current.arrival_dir, move_dir) {
                if prev != curr {
                    new_cost += COST_BEND;
                }
            }

            let key = (next, move_dir);
            let known = best_cost.get(&key).copied().unwrap_or(usize::MAX);

            if new_cost < known {
                best_cost.insert(key, new_cost);
                // Only update came_from if this is the best overall path to this point
                let best_to_next = best_cost_to_point.get(&next).copied().unwrap_or(usize::MAX);
                if new_cost < best_to_next {
                    best_cost_to_point.insert(next, new_cost);
                    came_from.insert(next, current.point);
                }
                open.push(PathNode {
                    cost: new_cost,
                    estimate: manhattan(next, end),
                    point: next,
                    arrival_dir: move_dir,
                });
            }
        }
    }

    if !found_end {
        if debug_timing {
            eprintln!("    routing failed after {} steps", steps);
        }
        return None;
    }

    if debug_timing {
        eprintln!("    routing succeeded after {} steps", steps);
    }

    let mut path: Vec<Point> = Vec::new();
    let mut current = end;
    path.push(current);
    let mut visited: HashSet<Point> = HashSet::new();
    visited.insert(current);
    while let Some(prev) = came_from.get(&current) {
        if !visited.insert(*prev) {
            break;
        }
        current = *prev;
        path.push(current);
        if current == start {
            break;
        }
    }
    path.reverse();

    let route = compress_path(&path);

    // Mark the successful route on the grid to repel future edges
    grid.mark_path(&route);

    Some(route)
}

fn route_with_obstacles_v2(
    start: Point,
    end: Point,
    grid: &mut OccupancyGrid,
    avoid_rects: &[Rect],
    coords: &OrientedCoords,
) -> Option<EdgeRoute> {
    if let Some(route) = route_with_obstacles(start, end, grid, avoid_rects, coords) {
        return Some(route);
    }
    route_with_detours(start, end, grid, avoid_rects, coords)
}

fn route_with_detours(
    start: Point,
    end: Point,
    grid: &mut OccupancyGrid,
    avoid_rects: &[Rect],
    coords: &OrientedCoords,
) -> Option<EdgeRoute> {
    if start == end {
        return Some(EdgeRoute::new());
    }

    let in_avoid = |p: Point| -> bool { avoid_rects.iter().any(|r| r.contains(p)) };
    let in_bounds = |p: Point| -> bool { p.x < grid.width && p.y < grid.height };

    let (start_primary, end_primary) = match coords.primary {
        Axis::Horizontal => (start.x, end.x),
        Axis::Vertical => (start.y, end.y),
    };
    let (p_min, p_max) = if start_primary <= end_primary {
        (start_primary, end_primary)
    } else {
        (end_primary, start_primary)
    };

    // Try a small set of primary-axis "dogleg" rows/cols near the midpoint and endpoints.
    let mid = p_min + (p_max.saturating_sub(p_min) / 2);
    let mut candidates: Vec<usize> = vec![
        mid,
        mid.saturating_add(1),
        mid.saturating_sub(1),
        mid.saturating_add(2),
        mid.saturating_sub(2),
        p_min.saturating_add(1),
        p_max.saturating_sub(1),
    ];
    candidates.sort_unstable();
    candidates.dedup();

    for primary in candidates {
        let (p1, p2) = match coords.primary {
            Axis::Vertical => (Point::new(start.x, primary), Point::new(end.x, primary)),
            Axis::Horizontal => (Point::new(primary, start.y), Point::new(primary, end.y)),
        };
        if !in_bounds(p1) || !in_bounds(p2) {
            continue;
        }
        if (p1 != start && p1 != end && in_avoid(p1)) || (p2 != start && p2 != end && in_avoid(p2))
        {
            continue;
        }

        // Use a cloned grid so failed attempts don't "burn in" partial routes.
        let mut trial = grid.clone();
        trial.clear_point(p1);
        trial.clear_point(p2);

        let mut combined = EdgeRoute::new();
        let legs = [(start, p1), (p1, p2), (p2, end)];
        let mut ok = true;
        for (a, b) in legs {
            if a == b {
                continue;
            }
            let Some(route) = route_with_obstacles(a, b, &mut trial, avoid_rects, coords) else {
                ok = false;
                break;
            };
            for s in route.segments {
                combined.push_segment(s.from, s.to);
            }
        }

        if ok && !combined.segments.is_empty() {
            return Some(combined);
        }
    }

    None
}

fn ordered_neighbors(current: Point, goal: Point, coords: &OrientedCoords) -> Vec<Point> {
    let dx = goal.x as isize - current.x as isize;
    let dy = goal.y as isize - current.y as isize;

    let primary_first = if coords.primary == Axis::Horizontal {
        vec![
            (dx.signum(), 0),
            (0, dy.signum()),
            (-dx.signum(), 0),
            (0, -dy.signum()),
        ]
    } else {
        vec![
            (0, dy.signum()),
            (dx.signum(), 0),
            (0, -dy.signum()),
            (-dx.signum(), 0),
        ]
    };

    let mut neighbors = Vec::new();
    for (sx, sy) in primary_first {
        if sx == 0 && sy == 0 {
            continue;
        }
        let nx = if sx.is_negative() {
            current.x.saturating_sub(sx.unsigned_abs())
        } else {
            current.x.saturating_add(sx as usize)
        };
        let ny = if sy.is_negative() {
            current.y.saturating_sub(sy.unsigned_abs())
        } else {
            current.y.saturating_add(sy as usize)
        };
        let next = Point::new(nx, ny);
        if next != current {
            neighbors.push(next);
        }
    }
    neighbors
}

fn compress_path(points: &[Point]) -> EdgeRoute {
    let mut route = EdgeRoute::new();
    if points.is_empty() {
        return route;
    }
    if points.len() == 1 {
        route.push_segment(points[0], points[0]);
        return route;
    }

    let mut seg_start = points[0];
    let mut last_dir = (0isize, 0isize);
    for window in points.windows(2) {
        let a = window[0];
        let b = window[1];
        let dir = (b.x as isize - a.x as isize, b.y as isize - a.y as isize);
        let norm = (dir.0.signum(), dir.1.signum());
        if last_dir != norm && last_dir != (0, 0) {
            route.push_segment(seg_start, a);
            seg_start = a;
        }
        last_dir = norm;
    }
    route.push_segment(seg_start, *points.last().unwrap());
    route
}

fn edge_exit_point(rect: Rect, direction: Direction) -> Point {
    match direction {
        Direction::TD | Direction::TB => Point::new(rect.x + rect.width / 2, rect.y + rect.height),
        Direction::BT => Point::new(rect.x + rect.width / 2, rect.y.saturating_sub(1)),
        Direction::LR => Point::new(rect.x + rect.width, rect.y + rect.height / 2),
        Direction::RL => Point::new(rect.x.saturating_sub(1), rect.y + rect.height / 2),
    }
}

fn edge_entry_point(rect: Rect, direction: Direction) -> Point {
    match direction {
        Direction::TD | Direction::TB => {
            Point::new(rect.x + rect.width / 2, rect.y.saturating_sub(1))
        }
        Direction::BT => Point::new(rect.x + rect.width / 2, rect.y + rect.height),
        Direction::LR => Point::new(rect.x.saturating_sub(1), rect.y + rect.height / 2),
        Direction::RL => Point::new(rect.x + rect.width, rect.y + rect.height / 2),
    }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, Node, Subgraph};
    use crate::parser::parse;

    fn simple_graph(direction: Direction) -> Graph {
        let mut g = Graph::new();
        g.direction = direction;
        g.nodes.push(Node::new("A", "A"));
        g.nodes.push(Node::new("B", "B"));
        g.edges.push(Edge::new("A", "B"));
        g
    }

    #[test]
    fn routes_around_obstacle() {
        let graph = simple_graph(Direction::TD);
        let input = LayoutInput {
            graph: &graph,
            prior_positions: None,
        };
        let cfg = CoarseLayoutConfig::default();
        let output = layout(input, cfg).expect("layout");
        let route = output.routes.get(&0).expect("route");
        assert!(!route.segments.is_empty());
    }

    #[test]
    fn gutter_avoids_external_edges() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;
        graph.nodes.push(Node::new("A", "A"));
        graph.nodes.push(Node::new("B", "B"));
        graph.nodes.push(Node::new("C", "C"));
        graph.edges.push(Edge::new("A", "B"));
        graph.edges.push(Edge::new("B", "C"));

        let mut sg = crate::graph::Subgraph::new("sg1", Some("Group".into()));
        sg.add_node("B");
        graph.add_subgraph(sg);
        graph.associate_node_with_subgraph("B", "sg1");

        let input = LayoutInput {
            graph: &graph,
            prior_positions: None,
        };
        let output = layout(input, CoarseLayoutConfig::default()).expect("layout");
        assert!(output.subgraph_envelopes.contains_key("sg1"));
        // Routing may be deferred to the renderer for some shapes; layout should still succeed.
    }

    #[test]
    fn inner_bounds_persist_on_graph() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;
        graph.nodes.push(Node::new("A", "A"));
        graph.nodes.push(Node::new("B", "B"));
        graph.edges.push(Edge::new("A", "B"));

        let mut sg = crate::graph::Subgraph::new("sg", Some("Group".into()));
        sg.add_node("A");
        sg.add_node("B");
        graph.add_subgraph(sg);
        graph.associate_node_with_subgraph("A", "sg");
        graph.associate_node_with_subgraph("B", "sg");

        let laid_out =
            apply_coarse_layout(graph, None, CoarseLayoutConfig::default()).expect("layout");
        let sg = laid_out.get_subgraph("sg").expect("subgraph exists");
        assert!(
            sg.inner_bounds.is_valid(),
            "inner bounds should be populated from layout"
        );
        assert!(
            sg.bounds.width >= sg.inner_bounds.width && sg.bounds.height >= sg.inner_bounds.height
        );
    }

    #[test]
    fn routes_cross_subgraph_boundaries() {
        let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_outside_td.md")
            .expect("read fixture");
        let parsed = parse(&input, false).expect("parse");
        let graph =
            apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

        if let Some(sg) = graph.subgraphs.first() {
            let _ = sg; // keep test quiet
        }

        // Edge routes for cross-subgraph edges may be provided by layout or deferred to the
        // renderer; if present, they should be non-empty.
        for edge_idx in [1usize, 2usize] {
            if let Some(route) = graph.edge_routes.get(&edge_idx) {
                assert!(
                    !route.segments.is_empty(),
                    "route {} should have segments",
                    edge_idx
                );
            }
        }
    }

    #[test]
    fn nested_service_data_sample_populates_envelopes_and_portals() {
        let input = "graph TD\nA[API Gateway] --> B[User Service]\nsubgraph SL[Service Layer]\nB\nsubgraph DL[Data Layer]\nC[Order Service] --> D[(Order DB)]\nE[(User DB)]\nend\nB --> E\nD --> F[Response Builder]\nE --> F\nend";
        let parsed = parse(input, false).expect("parse");
        let output = layout(
            LayoutInput {
                graph: &parsed.graph,
                prior_positions: None,
            },
            CoarseLayoutConfig::default(),
        )
        .expect("layout");

        let service = output
            .subgraph_envelopes
            .get("SL")
            .expect("service envelope");
        let data = output.subgraph_envelopes.get("DL").expect("data envelope");

        assert!(
            !service.inner.is_empty() && !service.outer.is_empty(),
            "service envelope should be populated"
        );
        assert!(
            !data.inner.is_empty() && !data.outer.is_empty(),
            "data envelope should be populated"
        );
        assert!(
            !service.portals.top.is_empty(),
            "service envelope should expose a top portal for A -> B"
        );
        assert!(
            !data.portals.top.is_empty(),
            "data envelope should expose a top portal for B -> E"
        );
        assert!(
            !data.portals.bottom.is_empty(),
            "data envelope should expose bottom portals for D/E -> F"
        );
    }

    #[test]
    fn explicit_nested_child_roots_follow_parent_direct_rank() {
        let input = "graph TD\nA[API Gateway] --> B[User Service]\nsubgraph SL[Service Layer]\nB\nsubgraph DL[Data Layer]\nC[Order Service] --> D[(Order DB)]\nE[(User DB)]\nend\nB --> E\nD --> F[Response Builder]\nE --> F\nend";
        let parsed = parse(input, false).expect("parse");
        let graph =
            apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

        let service = graph.get_subgraph("SL").expect("service layer");
        let data = graph.get_subgraph("DL").expect("data layer");
        let user_service = graph.get_node("B").expect("user service");
        let order_service = graph.get_node("C").expect("order service");
        let response_builder = graph.get_node("F").expect("response builder");

        assert!(
            order_service.rank > user_service.rank,
            "expected true nested child roots to be promoted after the parent's direct node: child_rank={} parent_rank={}",
            order_service.rank,
            user_service.rank
        );
        assert!(
            service.bounds.contains(data.bounds.x, data.bounds.y)
                && service.bounds.contains(
                    data.bounds.x + data.bounds.width.saturating_sub(1),
                    data.bounds.y + data.bounds.height.saturating_sub(1)
                ),
            "expected the declared parent envelope to fully contain the nested child envelope: parent={:?} child={:?}",
            service.bounds,
            data.bounds
        );
        assert!(
            order_service.y > user_service.y + user_service.height,
            "expected the true nested child content to start below the parent's direct node content: order_service=({}, {}, {}x{}) user_service=({}, {}, {}x{}) data={:?}",
            order_service.x,
            order_service.y,
            order_service.width,
            order_service.height,
            user_service.x,
            user_service.y,
            user_service.width,
            user_service.height,
            data.bounds
        );
        assert!(
            data.bounds.y > user_service.y + user_service.height,
            "expected the nested child border/title band to stay below the parent's direct node band: data={:?} user_service=({}, {}, {}x{})",
            data.bounds,
            user_service.x,
            user_service.y,
            user_service.width,
            user_service.height,
        );
        assert!(
            !data.bounds.contains(response_builder.x, response_builder.y)
                && !data.bounds.contains(
                    response_builder.x + response_builder.width.saturating_sub(1),
                    response_builder.y + response_builder.height.saturating_sub(1)
                ),
            "expected the nested child envelope to exclude the parent's direct response node: data={:?} response_builder=({}, {}, {}x{})",
            data.bounds,
            response_builder.x,
            response_builder.y,
            response_builder.width,
            response_builder.height,
        );
    }

    #[test]
    fn explicit_nested_horizontal_children_stay_contained_and_ordered_by_flow() {
        for (direction, data_precedes_response) in [(Direction::LR, true), (Direction::RL, false)] {
            let input = format!(
                "graph {direction:?}\nA[API Gateway] --> B[User Service]\nsubgraph SL[Service Layer]\nB\nsubgraph DL[Data Layer]\nC[Order Service] --> D[(Order DB)]\nE[(User DB)]\nend\nB --> E\nD --> F[Response Builder]\nE --> F\nend"
            );
            let parsed = parse(&input, false).expect("parse");
            let graph = apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default())
                .expect("layout");

            let service = graph.get_subgraph("SL").expect("service layer");
            let data = graph.get_subgraph("DL").expect("data layer");
            let user_service = graph.get_node("B").expect("user service");
            let response_builder = graph.get_node("F").expect("response builder");

            assert!(
                service.bounds.contains(data.bounds.x, data.bounds.y)
                    && service.bounds.contains(
                        data.bounds.x + data.bounds.width.saturating_sub(1),
                        data.bounds.y + data.bounds.height.saturating_sub(1)
                    ),
                "expected the declared parent envelope to fully contain the nested child envelope in {direction:?}: parent={:?} child={:?}",
                service.bounds,
                data.bounds,
            );
            assert!(
                data.bounds.y > service.bounds.y,
                "expected the nested child title row to staircase below the parent title row in {direction:?}: parent={:?} child={:?}",
                service.bounds,
                data.bounds,
            );
            assert!(
                !data.bounds.contains(response_builder.x, response_builder.y)
                    && !data.bounds.contains(
                        response_builder.x + response_builder.width.saturating_sub(1),
                        response_builder.y + response_builder.height.saturating_sub(1)
                    ),
                "expected the nested child envelope to exclude the parent-only response node in {direction:?}: child={:?} response_builder=({}, {}, {}x{})",
                data.bounds,
                response_builder.x,
                response_builder.y,
                response_builder.width,
                response_builder.height,
            );

            if data_precedes_response {
                let gap_to_user_service = data
                    .bounds
                    .x
                    .saturating_sub(user_service.x.saturating_add(user_service.width));
                let gap_to_response = response_builder
                    .x
                    .saturating_sub(data.bounds.x.saturating_add(data.bounds.width));
                assert!(
                    data.bounds.x > user_service.x + user_service.width,
                    "expected the nested child to remain after the parent's direct node along LR flow: child={:?} user_service=({}, {}, {}x{})",
                    data.bounds,
                    user_service.x,
                    user_service.y,
                    user_service.width,
                    user_service.height,
                );
                assert!(
                    response_builder.x > data.bounds.x + data.bounds.width,
                    "expected the parent-only response node to remain after the nested child along LR flow: child={:?} response_builder=({}, {}, {}x{})",
                    data.bounds,
                    response_builder.x,
                    response_builder.y,
                    response_builder.width,
                    response_builder.height,
                );
                assert!(
                    gap_to_user_service <= gap_to_response.saturating_add(2),
                    "expected the nested child to stay at least as close to the upstream parent-direct node as to the downstream parent-only response node in LR: child={:?} user_service=({}, {}, {}x{}) response_builder=({}, {}, {}x{}) gaps=({}, {})",
                    data.bounds,
                    user_service.x,
                    user_service.y,
                    user_service.width,
                    user_service.height,
                    response_builder.x,
                    response_builder.y,
                    response_builder.width,
                    response_builder.height,
                    gap_to_user_service,
                    gap_to_response,
                );
            } else {
                let gap_to_user_service = user_service
                    .x
                    .saturating_sub(data.bounds.x.saturating_add(data.bounds.width));
                let gap_to_response = data
                    .bounds
                    .x
                    .saturating_sub(response_builder.x.saturating_add(response_builder.width));
                assert!(
                    data.bounds.x + data.bounds.width <= user_service.x,
                    "expected the nested child to remain before the parent's direct node along RL flow: child={:?} user_service=({}, {}, {}x{})",
                    data.bounds,
                    user_service.x,
                    user_service.y,
                    user_service.width,
                    user_service.height,
                );
                assert!(
                    response_builder.x + response_builder.width <= data.bounds.x,
                    "expected the parent-only response node to remain before the nested child along RL flow: child={:?} response_builder=({}, {}, {}x{})",
                    data.bounds,
                    response_builder.x,
                    response_builder.y,
                    response_builder.width,
                    response_builder.height,
                );
                assert!(
                    gap_to_user_service <= gap_to_response.saturating_add(2),
                    "expected the nested child to stay at least as close to the upstream parent-direct node as to the downstream parent-only response node in RL: child={:?} user_service=({}, {}, {}x{}) response_builder=({}, {}, {}x{}) gaps=({}, {})",
                    data.bounds,
                    user_service.x,
                    user_service.y,
                    user_service.width,
                    user_service.height,
                    response_builder.x,
                    response_builder.y,
                    response_builder.width,
                    response_builder.height,
                    gap_to_user_service,
                    gap_to_response,
                );
            }
        }
    }

    #[test]
    fn sibling_subgraphs_stay_separate_in_td_layout() {
        let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_td.md")
            .expect("read fixture");
        let parsed = parse(&input, false).expect("parse");
        let graph =
            apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

        let service = graph.get_subgraph("SG1").expect("service layer");
        let data = graph.get_subgraph("SG2").expect("data layer");
        let response = graph.get_node("Response").expect("response");
        let user_db = graph.get_node("D1").expect("user db");
        let order_db = graph.get_node("D2").expect("order db");
        let overlaps = service.bounds.x < data.bounds.x + data.bounds.width
            && service.bounds.x + service.bounds.width > data.bounds.x
            && service.bounds.y < data.bounds.y + data.bounds.height
            && service.bounds.y + service.bounds.height > data.bounds.y;

        assert!(
            !overlaps,
            "expected Mermaid sibling subgraphs to stay visually separate in TD: service={:?} data={:?}",
            service.bounds,
            data.bounds
        );
        assert!(
            data.bounds.y > service.bounds.y + service.bounds.height,
            "expected the sibling Data Layer to stay below the Service Layer in TD: service={:?} data={:?}",
            service.bounds,
            data.bounds
        );
        assert!(
            response.y > data.bounds.y + data.bounds.height,
            "expected Response Builder to remain below the sibling Data Layer in TD: data={:?} response=({}, {}, {}x{})",
            data.bounds,
            response.x,
            response.y,
            response.width,
            response.height
        );
        assert!(
            user_db.x >= order_db.x + 8,
            "expected route-aware nested width budgeting to widen the nested source span before converging to Response: user_db=({}, {}, {}x{}) order_db=({}, {}, {}x{})",
            user_db.x,
            user_db.y,
            user_db.width,
            user_db.height,
            order_db.x,
            order_db.y,
            order_db.width,
            order_db.height
        );
    }

    #[test]
    fn stacked_top_level_td_sibling_subgraphs_harmonize_widths_when_chain_connected() {
        let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_td.md")
            .expect("read fixture");
        let parsed = parse(&input, false).expect("parse");
        let graph =
            apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

        let service = graph.get_subgraph("SG1").expect("service layer");
        let data = graph.get_subgraph("SG2").expect("data layer");

        assert!(
            service.bounds.width.abs_diff(data.bounds.width) <= 1,
            "expected connected top-level TD siblings to keep closely harmonized frame widths for visual balance: service={:?} data={:?}",
            service.bounds,
            data.bounds
        );
        assert_eq!(
            service.bounds.x, data.bounds.x,
            "expected connected top-level TD siblings with harmonized widths to share the same left wall: service={:?} data={:?}",
            service.bounds,
            data.bounds
        );
    }

    #[test]
    fn stacked_top_level_td_sibling_subgraphs_stay_vertically_compact() {
        let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_td.md")
            .expect("read fixture");
        let parsed = parse(&input, false).expect("parse");
        let graph =
            apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

        let service = graph.get_subgraph("SG1").expect("service layer");
        let data = graph.get_subgraph("SG2").expect("data layer");
        let inter_subgraph_gap = data
            .bounds
            .y
            .saturating_sub(service.bounds.y.saturating_add(service.bounds.height));

        assert!(
            service.bounds.height <= 18,
            "expected Service Layer to stay vertically compact after mixed boundary fan-out compaction: service={:?}",
            service.bounds
        );
        assert!(
            inter_subgraph_gap <= 4,
            "expected the stacked TD sibling gap to stay compact after mixed boundary fan-out compaction: service={:?} data={:?} gap={}",
            service.bounds,
            data.bounds,
            inter_subgraph_gap
        );
    }

    #[test]
    fn sibling_subgraphs_stay_separate_in_horizontal_layouts() {
        for fixture in [
            "tests/fixtures/inputs/subgraph_complex_lr.md",
            "tests/fixtures/inputs/subgraph_complex_rl.md",
        ] {
            let input = std::fs::read_to_string(fixture).expect("read fixture");
            let parsed = parse(&input, false).expect("parse");
            let graph = apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default())
                .expect("layout");

            let outer = graph.get_subgraph("SG1").expect("service layer");
            let inner = graph.get_subgraph("SG2").expect("data layer");
            let user_service = graph.get_node("S1").expect("user service");
            let order_service = graph.get_node("S2").expect("order service");
            let response = graph.get_node("Response").expect("response");

            let overlaps = |node: &Node, bounds: &crate::graph::Rectangle| {
                let node_left = node.x;
                let node_right = node.x + node.width.saturating_sub(1);
                let node_top = node.y;
                let node_bottom = node.y + node.height.saturating_sub(1);
                let bounds_right = bounds.x + bounds.width;
                let bounds_bottom = bounds.y + bounds.height;

                node_left < bounds_right
                    && node_right >= bounds.x
                    && node_top < bounds_bottom
                    && node_bottom >= bounds.y
            };
            let subgraphs_overlap = outer.bounds.x < inner.bounds.x + inner.bounds.width
                && outer.bounds.x + outer.bounds.width > inner.bounds.x
                && outer.bounds.y < inner.bounds.y + inner.bounds.height
                && outer.bounds.y + outer.bounds.height > inner.bounds.y;

            assert!(
                !subgraphs_overlap,
                "expected Mermaid sibling subgraphs to stay visually separate for {fixture}: outer={:?} inner={:?}",
                outer.bounds,
                inner.bounds
            );
            assert!(
                !overlaps(user_service, &inner.bounds) && !overlaps(order_service, &inner.bounds),
                "expected SG2 to stay separate without swallowing SG1 sibling nodes for {fixture}: inner={:?} user_service=({}, {}, {}x{}) order_service=({}, {}, {}x{})",
                inner.bounds,
                user_service.x,
                user_service.y,
                user_service.width,
                user_service.height,
                order_service.x,
                order_service.y,
                order_service.width,
                order_service.height
            );
            assert!(
                !(outer.bounds.contains(response.x, response.y)
                    && outer.bounds.contains(
                        response.x + response.width.saturating_sub(1),
                        response.y + response.height.saturating_sub(1)
                    )),
                "expected Response Builder to avoid full containment within SG1 for {fixture}: outer={:?} response=({}, {}, {}x{})",
                outer.bounds,
                response.x,
                response.y,
                response.width,
                response.height
            );
        }
    }

    #[test]
    fn side_by_side_horizontal_top_level_siblings_harmonize_heights_when_route_gutters_overlap() {
        for fixture in [
            "tests/fixtures/inputs/subgraph_complex_lr.md",
            "tests/fixtures/inputs/subgraph_complex_rl.md",
        ] {
            let input = std::fs::read_to_string(fixture).expect("read fixture");
            let parsed = parse(&input, false).expect("parse");
            let graph = apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default())
                .expect("layout");

            let service = graph.get_subgraph("SG1").expect("service layer");
            let data = graph.get_subgraph("SG2").expect("data layer");

            assert!(
                service.bounds.height.abs_diff(data.bounds.height) <= 1,
                "expected horizontal top-level siblings to keep closely harmonized frame heights even when widened route gutters make the outer envelopes overlap for {fixture}: service={:?} data={:?}",
                service.bounds,
                data.bounds
            );
        }
    }

    #[test]
    fn side_by_side_lr_sibling_subgraphs_share_frame_height_when_close() {
        let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_lr.md")
            .expect("read fixture");
        let parsed = parse(&input, false).expect("parse");
        let graph =
            apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

        let service = graph.get_subgraph("SG1").expect("service layer");
        let data = graph.get_subgraph("SG2").expect("data layer");

        assert_eq!(
            service.bounds.y, data.bounds.y,
            "expected side-by-side LR siblings to share the same top row when frame-height harmonization applies: service={:?} data={:?}",
            service.bounds,
            data.bounds
        );
        assert_eq!(
            service.bounds.height, data.bounds.height,
            "expected side-by-side LR siblings with comparable content to share the same frame height: service={:?} data={:?}",
            service.bounds,
            data.bounds
        );
    }

    #[test]
    fn side_by_side_lr_top_level_siblings_balance_trailing_response_gap() {
        let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_lr.md")
            .expect("read fixture");
        let parsed = parse(&input, false).expect("parse");
        let graph =
            apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

        let service = graph.get_subgraph("SG1").expect("service layer");
        let data = graph.get_subgraph("SG2").expect("data layer");
        let response = graph.get_node("Response").expect("response");

        let inter_subgraph_gap = data
            .bounds
            .x
            .saturating_sub(service.bounds.x.saturating_add(service.bounds.width));
        let trailing_response_gap = response
            .x
            .saturating_sub(data.bounds.x.saturating_add(data.bounds.width));

        assert!(
            trailing_response_gap >= 6,
            "expected the LR trailing response gap to leave enough breathing room after the final top-level sibling instead of collapsing the connector into the response box: service={:?} data={:?} response=({}, {}, {}x{}) gap={}",
            service.bounds,
            data.bounds,
            response.x,
            response.y,
            response.width,
            response.height,
            trailing_response_gap,
        );
        assert!(
            inter_subgraph_gap <= trailing_response_gap.saturating_add(2),
            "expected the LR inter-subgraph lane to stay visually comparable to the trailing response gap instead of hoarding most of the horizontal slack in the middle: service={:?} data={:?} response=({}, {}, {}x{}) gaps=({}, {})",
            service.bounds,
            data.bounds,
            response.x,
            response.y,
            response.width,
            response.height,
            inter_subgraph_gap,
            trailing_response_gap,
        );
    }

    #[test]
    fn explicit_nested_child_route_budget_adds_horizontal_border_clearance() {
        let input = "graph TD\nA[API Gateway]\nsubgraph SG1[Service Layer]\nS1[User Service]\nS2[Order Service]\nsubgraph SG2[Data Layer]\nD1[(User DB)]\nD2[(Order DB)]\nend\nResponse[Response Builder]\nS1 --> S2\nS1 --> D1\nS2 --> D2\nD1 --> Response\nD2 --> Response\nend\nA --> S1\n";
        let parsed = parse(input, false).expect("parse");
        let graph =
            apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

        let child = graph.get_subgraph("SG2").expect("data layer");
        let user_db = graph.get_node("D1").expect("user db");
        let order_db = graph.get_node("D2").expect("order db");

        let left_margin = user_db.x.saturating_sub(child.bounds.x);
        let right_margin = child
            .bounds
            .x
            .saturating_add(child.bounds.width)
            .saturating_sub(order_db.x.saturating_add(order_db.width));

        assert!(
            left_margin >= 3,
            "expected nested child route budgeting to reserve at least three columns between the left border and the first child node: child={:?} user_db=({}, {}, {}x{})",
            child.bounds,
            user_db.x,
            user_db.y,
            user_db.width,
            user_db.height,
        );
        assert!(
            right_margin >= 3,
            "expected nested child route budgeting to reserve at least three columns between the right border and the last child node: child={:?} order_db=({}, {}, {}x{})",
            child.bounds,
            order_db.x,
            order_db.y,
            order_db.width,
            order_db.height,
        );
    }

    #[test]
    fn route_budgeted_subgraphs_include_declared_nested_children() {
        let mut graph = Graph::new();
        graph.add_subgraph(Subgraph::new("parent", Some("Parent".to_string())));

        let mut child = Subgraph::new("child", Some("Child".to_string()));
        child.parent_id = Some("parent".to_string());
        graph.add_subgraph(child);
        graph
            .get_subgraph_mut("parent")
            .expect("parent")
            .add_child("child");

        let budgeted = route_budgeted_subgraphs(&graph);

        assert_eq!(
            budgeted,
            vec!["child".to_string()],
            "expected declared nested children to participate in internal route budgeting"
        );
    }

    #[test]
    fn declared_nested_child_route_pressure_shifts_right_partition_not_just_outgoing_sources() {
        let mut graph = Graph::new();
        graph.add_node(Node::new("left", "Left"));
        graph.add_node(Node::new("right", "Right"));
        graph.add_node(Node::new("sibling", "Sibling"));
        graph.add_node(Node::new("ext_a", "ExtA"));
        graph.add_node(Node::new("ext_b", "ExtB"));
        graph.add_subgraph(Subgraph::new("parent", Some("Parent".to_string())));

        let mut child = Subgraph::new("child", Some("Child".to_string()));
        child.parent_id = Some("parent".to_string());
        graph.add_subgraph(child);
        graph
            .get_subgraph_mut("parent")
            .expect("parent")
            .add_child("child");

        graph.associate_node_with_subgraph("left", "child");
        graph.associate_node_with_subgraph("right", "child");
        graph.associate_node_with_subgraph("sibling", "child");
        graph.add_edge(Edge::new("left", "ext_a"));
        graph.add_edge(Edge::new("right", "ext_b"));

        let mut positions = HashMap::from([
            ("left".to_string(), Point::new(8, 0)),
            ("right".to_string(), Point::new(14, 0)),
            ("sibling".to_string(), Point::new(24, 0)),
            ("ext_a".to_string(), Point::new(0, 0)),
            ("ext_b".to_string(), Point::new(0, 4)),
        ]);
        let mut node_rects = HashMap::from([
            ("left".to_string(), Rect::new(8, 0, 5, 3)),
            ("right".to_string(), Rect::new(14, 0, 5, 3)),
            ("sibling".to_string(), Rect::new(24, 0, 6, 3)),
            ("ext_a".to_string(), Rect::new(0, 0, 4, 3)),
            ("ext_b".to_string(), Rect::new(0, 4, 4, 3)),
        ]);

        let shift = widen_subgraph_for_outgoing_route_pressure(
            &graph,
            &mut positions,
            &mut node_rects,
            "child",
        );

        assert!(
            shift > 0,
            "expected route pressure to widen the declared nested child span"
        );
        assert_eq!(
            positions.get("left").expect("left").x,
            8,
            "left partition should stay anchored"
        );
        assert_eq!(
            positions.get("right").expect("right").x,
            14 + shift,
            "right outgoing source should shift right"
        );
        assert_eq!(
            positions.get("sibling").expect("sibling").x,
            24 + shift,
            "non-source sibling on the right partition should shift with the widened subtree"
        );
    }

    #[test]
    fn internal_route_span_budget_detects_centered_nested_fanin() {
        let mut graph = Graph::new();
        graph.add_node(Node::new("left", "L"));
        graph.add_node(Node::new("middle", "M"));
        graph.add_node(Node::new("right", "R"));
        graph.add_node(Node::new("target", "T"));
        graph.add_subgraph(Subgraph::new("parent", Some("Parent".to_string())));

        let mut child = Subgraph::new("child", Some("Child".to_string()));
        child.parent_id = Some("parent".to_string());
        graph.add_subgraph(child);
        graph
            .get_subgraph_mut("parent")
            .expect("parent")
            .add_child("child");

        graph.associate_node_with_subgraph("left", "child");
        graph.associate_node_with_subgraph("middle", "child");
        graph.associate_node_with_subgraph("right", "child");

        graph.add_edge(Edge::new("left", "target"));
        graph.add_edge(Edge::new("middle", "target"));
        graph.add_edge(Edge::new("right", "target"));

        let node_rects = HashMap::from([
            ("left".to_string(), Rect::new(8, 0, 5, 3)),
            ("middle".to_string(), Rect::new(14, 0, 5, 3)),
            ("right".to_string(), Rect::new(20, 0, 5, 3)),
            ("target".to_string(), Rect::new(14, 8, 5, 3)),
        ]);

        let budget = internal_route_span_budget_x(
            &graph,
            &node_rects,
            "child",
            CoarseLayoutConfig::default().min_horizontal_spacing,
        )
        .expect("centered fan-in should need span budget");

        assert_eq!(
            budget.shift_x, 4,
            "expected centered nested fan-in to widen beyond the coarse node span"
        );
        assert_eq!(budget.pivot_center, 16);
    }

    #[test]
    fn widen_subgraph_for_internal_route_span_shifts_centered_nested_fanout_partition() {
        let mut graph = Graph::new();
        graph.add_node(Node::new("source", "Source"));
        graph.add_node(Node::new("left", "L"));
        graph.add_node(Node::new("middle", "M"));
        graph.add_node(Node::new("right", "R"));
        graph.add_subgraph(Subgraph::new("parent", Some("Parent".to_string())));

        let mut child = Subgraph::new("child", Some("Child".to_string()));
        child.parent_id = Some("parent".to_string());
        graph.add_subgraph(child);
        graph
            .get_subgraph_mut("parent")
            .expect("parent")
            .add_child("child");

        graph.associate_node_with_subgraph("left", "child");
        graph.associate_node_with_subgraph("middle", "child");
        graph.associate_node_with_subgraph("right", "child");

        graph.add_edge(Edge::new("source", "left"));
        graph.add_edge(Edge::new("source", "middle"));
        graph.add_edge(Edge::new("source", "right"));

        let mut positions = HashMap::from([
            ("source".to_string(), Point::new(14, 8)),
            ("left".to_string(), Point::new(8, 0)),
            ("middle".to_string(), Point::new(14, 0)),
            ("right".to_string(), Point::new(20, 0)),
        ]);
        let mut node_rects = HashMap::from([
            ("source".to_string(), Rect::new(14, 8, 5, 3)),
            ("left".to_string(), Rect::new(8, 0, 5, 3)),
            ("middle".to_string(), Rect::new(14, 0, 5, 3)),
            ("right".to_string(), Rect::new(20, 0, 5, 3)),
        ]);

        let shift = widen_subgraph_for_internal_route_span(
            &graph,
            &mut positions,
            &mut node_rects,
            "child",
            CoarseLayoutConfig::default().min_horizontal_spacing,
        );

        assert_eq!(
            shift, 4,
            "expected centered nested fan-out to claim extra span"
        );
        assert_eq!(
            positions.get("left").expect("left").x,
            8,
            "left partition should stay anchored"
        );
        assert_eq!(
            positions.get("middle").expect("middle").x,
            18,
            "middle target should shift with the widened right partition"
        );
        assert_eq!(
            positions.get("right").expect("right").x,
            24,
            "right target should shift with the widened right partition"
        );
    }

    #[test]
    fn nested_horizontal_subgraphs_keep_distinct_title_rows() {
        let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_nested_lr.md")
            .expect("read fixture");
        let parsed = parse(&input, false).expect("parse");
        let graph =
            apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

        let outer = graph.get_subgraph("Outer").expect("outer subgraph");
        let inner = graph.get_subgraph("Inner").expect("inner subgraph");
        let deep = graph.get_subgraph("Deep").expect("deep subgraph");

        assert!(
            outer.bounds.y < inner.bounds.y,
            "expected nested LR outer title row to stay above the inner title row: outer={:?} inner={:?}",
            outer.bounds,
            inner.bounds
        );
        assert!(
            inner.bounds.y < deep.bounds.y,
            "expected nested LR inner title row to stay above the deep title row: inner={:?} deep={:?}",
            inner.bounds,
            deep.bounds
        );
    }

    #[test]
    fn titled_vertical_subgraph_balances_left_and_right_inner_padding() {
        let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_basic_td.md")
            .expect("read fixture");
        let parsed = parse(&input, false).expect("parse");
        let graph =
            apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

        let subgraph = graph.get_subgraph("SG").expect("subgraph");
        let left_pad = subgraph
            .inner_bounds
            .x
            .saturating_sub(subgraph.bounds.x.saturating_add(1));
        let right_pad = subgraph
            .bounds
            .x
            .saturating_add(subgraph.bounds.width.saturating_sub(1))
            .saturating_sub(subgraph.inner_bounds.x + subgraph.inner_bounds.width);

        assert!(
            left_pad.abs_diff(right_pad) <= 1,
            "expected titled TD subgraph content to be horizontally centered inside the final frame: bounds={:?} inner={:?} pads=({}, {})",
            subgraph.bounds,
            subgraph.inner_bounds,
            left_pad,
            right_pad,
        );
    }

    #[test]
    fn titled_vertical_subgraph_balances_top_and_bottom_inner_padding() {
        let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_basic_td.md")
            .expect("read fixture");
        let parsed = parse(&input, false).expect("parse");
        let graph =
            apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

        let subgraph = graph.get_subgraph("SG").expect("subgraph");
        let top_pad = subgraph.inner_bounds.y.saturating_sub(subgraph.bounds.y);
        let bottom_pad = subgraph
            .bounds
            .y
            .saturating_add(subgraph.bounds.height.saturating_sub(1))
            .saturating_sub(subgraph.inner_bounds.y + subgraph.inner_bounds.height);

        assert!(
            top_pad.abs_diff(bottom_pad) <= 1,
            "expected titled TD subgraph content to be vertically centered inside the final frame: bounds={:?} inner={:?} pads=({}, {})",
            subgraph.bounds,
            subgraph.inner_bounds,
            top_pad,
            bottom_pad,
        );
    }

    #[test]
    fn titled_bt_subgraph_balances_top_and_bottom_inner_padding() {
        let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_basic_bt.md")
            .expect("read fixture");
        let parsed = parse(&input, false).expect("parse");
        let graph =
            apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

        let subgraph = graph.get_subgraph("SG").expect("subgraph");
        let top_pad = subgraph.inner_bounds.y.saturating_sub(subgraph.bounds.y);
        let bottom_pad = subgraph
            .bounds
            .y
            .saturating_add(subgraph.bounds.height.saturating_sub(1))
            .saturating_sub(subgraph.inner_bounds.y + subgraph.inner_bounds.height);

        assert!(
            top_pad.abs_diff(bottom_pad) <= 1,
            "expected titled BT subgraph content to be vertically centered inside the final frame: bounds={:?} inner={:?} pads=({}, {})",
            subgraph.bounds,
            subgraph.inner_bounds,
            top_pad,
            bottom_pad,
        );
    }

    #[test]
    fn titled_vertical_leaf_subgraphs_with_single_external_trunks_balance_horizontal_padding() {
        for fixture_path in [
            "tests/fixtures/inputs/subgraph_direct_td.md",
            "tests/fixtures/inputs/subgraph_direct_bt.md",
        ] {
            let input = std::fs::read_to_string(fixture_path).expect("read fixture");
            let parsed = parse(&input, false).expect("parse");
            let graph = apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default())
                .expect("layout");

            for subgraph_id in ["SG1", "SG2"] {
                let subgraph = graph.get_subgraph(subgraph_id).expect("subgraph");
                let left_pad = subgraph
                    .inner_bounds
                    .x
                    .saturating_sub(subgraph.bounds.x.saturating_add(1));
                let right_pad = subgraph
                    .bounds
                    .x
                    .saturating_add(subgraph.bounds.width.saturating_sub(1))
                    .saturating_sub(subgraph.inner_bounds.x + subgraph.inner_bounds.width);

                assert!(
                    left_pad.abs_diff(right_pad) <= 1,
                    "expected titled leaf subgraph {subgraph_id} in {fixture_path} to stay horizontally balanced even with a single external trunk: bounds={:?} inner={:?} pads=({}, {})",
                    subgraph.bounds,
                    subgraph.inner_bounds,
                    left_pad,
                    right_pad,
                );
            }
        }
    }

    #[test]
    fn marks_back_edges_and_leaves_cycle_routing_to_renderer() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;
        graph.nodes.push(Node::new("A", "A"));
        graph.nodes.push(Node::new("B", "B"));
        graph.edges.push(Edge::new("A", "B"));
        graph.edges.push(Edge::new("B", "A")); // back-edge creates a cycle

        let laid_out =
            apply_coarse_layout(graph, None, CoarseLayoutConfig::default()).expect("layout");

        assert!(laid_out.has_cycles(), "graph should be marked cyclic");
        assert!(
            laid_out.edges[1].is_back_edge,
            "back-edge should be flagged"
        );
        assert!(
            !laid_out.edges[0].is_back_edge,
            "forward edge should not be flagged"
        );
        // Only the forward edge should have a precomputed route; back-edges are rendered via the cycle gutter.
        assert!(
            laid_out.edge_routes.contains_key(&0),
            "forward edge should be routed"
        );
        assert!(
            !laid_out.edge_routes.contains_key(&1),
            "back-edge routing should be deferred to renderer"
        );
    }
}
