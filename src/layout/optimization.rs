//! Layer ordering, cycle marking, and coordinate balancing.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::geom::{Point, Rect};
use crate::graph::Graph;
use crate::orientation::{Axis, OrientedCoords};

use super::placement::gap_for_axis;
use super::CoarseLayoutConfig;

pub(super) fn assign_layers(graph: &Graph) -> Vec<Vec<usize>> {
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

pub(super) fn node_extent_primary(node: &crate::graph::Node, coords: &OrientedCoords) -> usize {
    match coords.primary {
        Axis::Vertical => node.height,
        Axis::Horizontal => node.width,
    }
}

pub(super) fn node_extent_secondary(node: &crate::graph::Node, coords: &OrientedCoords) -> usize {
    match coords.secondary {
        Axis::Vertical => node.height,
        Axis::Horizontal => node.width,
    }
}

pub(super) fn mark_back_edges(graph: &mut Graph) -> bool {
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
// Coordinate Balancing
// -----------------------------------------------------------------------------

pub(super) fn balance_coordinates(
    graph: &Graph,
    positions: &mut HashMap<String, Point>,
    node_rects: &mut HashMap<String, Rect>,
    layers: &[Vec<usize>],
    coords: &OrientedCoords,
    config: &CoarseLayoutConfig,
    prior_positions: Option<&HashMap<String, Point>>,
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
                prior_positions,
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
                prior_positions,
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
    prior_positions: Option<&HashMap<String, Point>>,
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

        // A repair candidate carries an explicit position map. Preserve those
        // positions through the balancing sweeps; otherwise the barycenter
        // pass silently erases the nudge before the candidate is evaluated.
        if prior_positions.is_some_and(|prior| prior.contains_key(node_id)) {
            let new_pos = current_pos.max(min_pos);
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
            continue;
        }

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
