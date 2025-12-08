//! Waterfall layout algorithm (deterministic, edge-aware)
//!
//! See SPEC §2.6 for algorithm details

use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::Result;

use crate::graph::{Direction, Graph};
use crate::style::{box_width, BOX_HEIGHT, BOX_MIN_WIDTH, COL_SPACING, ROW_SPACING};

/// Apply waterfall layout to position all nodes
pub fn waterfall(mut graph: Graph) -> Result<Graph> {
    if graph.nodes.is_empty() {
        return Ok(graph);
    }

    // Map node id -> index for quick lookup
    let mut index_map: HashMap<String, usize> = HashMap::new();
    for (idx, node) in graph.nodes.iter().enumerate() {
        index_map.insert(node.id.clone(), idx);
    }

    // Build adjacency with edge indices
    let mut adj: Vec<Vec<(usize, usize)>> = vec![vec![]; graph.nodes.len()];
    for (edge_idx, edge) in graph.edges.iter().enumerate() {
        if let (Some(&from_idx), Some(&to_idx)) =
            (index_map.get(&edge.from), index_map.get(&edge.to))
        {
            adj[from_idx].push((to_idx, edge_idx));
        }
    }

    // Detect cycles and mark back-edges
    if detect_cycles(&mut graph, &adj) {
        graph
            .warnings
            .push("termiflow: warning: Cycle detected, rendering back-edges in gutter"
                .to_string());
    }

    // Recompute indegree ignoring back-edges for rank assignment
    let mut indegree = vec![0usize; graph.nodes.len()];
    for (edge_idx, edge) in graph.edges.iter().enumerate() {
        if edge.is_back_edge {
            continue;
        }
        if let Some(&to_idx) = index_map.get(&edge.to) {
            indegree[to_idx] += 1;
        }
        if let Some(&from_idx) = index_map.get(&edge.from) {
            if let Some(list) = adj.get_mut(from_idx) {
                // ensure adjacency includes edge index for later parent lookup
                if !list.iter().any(|&(_, ei)| ei == edge_idx) {
                    if let Some(&to_idx) = index_map.get(&edge.to) {
                        list.push((to_idx, edge_idx));
                    }
                }
            }
        }
    }

    // Kahn topological layering (lenient for cycles)
    let mut queue: VecDeque<usize> = indegree
        .iter()
        .enumerate()
        .filter_map(|(idx, &deg)| if deg == 0 { Some(idx) } else { None })
        .collect();

    let mut order: Vec<usize> = Vec::new();
    let mut rank: Vec<usize> = vec![0; graph.nodes.len()];

    while let Some(u) = queue.pop_front() {
        order.push(u);
        for &(v, edge_idx) in &adj[u] {
            if graph.edges.get(edge_idx).map(|e| e.is_back_edge).unwrap_or(false) {
                continue;
            }
            if indegree[v] > 0 {
                indegree[v] -= 1;
                if indegree[v] == 0 {
                    rank[v] = rank[u] + 1;
                    queue.push_back(v);
                }
            }
        }
    }

    // Any nodes not processed (cycle/disconnected) retain rank 0 but keep deterministic order
    for idx in 0..graph.nodes.len() {
        if !order.contains(&idx) {
            order.push(idx);
        }
    }

    // Mark back-edges: to at same or higher rank
    for edge in graph.edges.iter_mut() {
        if let (Some(&from_idx), Some(&to_idx)) =
            (index_map.get(&edge.from), index_map.get(&edge.to))
        {
            if rank[to_idx] <= rank[from_idx] {
                edge.is_back_edge = true;
            }
        }
    }

    // Group nodes by rank
    let max_rank = *rank.iter().max().unwrap_or(&0);
    let mut by_rank: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
    for (idx, r) in rank.iter().enumerate() {
        by_rank[*r].push(idx);
    }
    for nodes in &mut by_rank {
        nodes.sort_by_key(|&idx| graph.nodes[idx].id.clone()); // deterministic within rank
    }

    let row_gap = BOX_HEIGHT + ROW_SPACING;

    // Precompute rank widths for LR spacing
    let rank_widths: Vec<usize> = by_rank
        .iter()
        .map(|nodes| {
            nodes
                .iter()
                .map(|&idx| box_width(&graph.nodes[idx].label))
                .max()
                .unwrap_or(BOX_MIN_WIDTH)
        })
        .collect();

    // Assign coordinates (TD/TB: horizontal per rank; LR: vertical stack per rank)
    // Parent positions for centering (only used in vertical orientation)
    let mut placed_centers: HashMap<String, usize> = HashMap::new();

    let mut rank_offset_x: Vec<usize> = vec![0; by_rank.len()];
    if matches!(graph.direction, Direction::LR) {
        let mut offset = 0usize;
        for (r, w) in rank_widths.iter().enumerate() {
            rank_offset_x[r] = offset;
            offset += w + COL_SPACING;
        }
    }

    for (r, nodes) in by_rank.iter().enumerate() {
        let mut cursor_primary = 0usize;
        let rank_y = r * row_gap;
        let rank_x = rank_offset_x[r];

        for &idx in nodes {
            let node = &mut graph.nodes[idx];
            node.width = box_width(&node.label);
            node.rank = r;

            match graph.direction {
                Direction::TD | Direction::BT | Direction::TB => {
                    // Center under parent(s) when possible
                    let parents: Vec<usize> = graph
                        .edges
                        .iter()
                        .filter(|e| !e.is_back_edge && e.to == node.id)
                        .filter_map(|e| placed_centers.get(&e.from).copied())
                        .collect();
                    let desired_center = if parents.len() == 1 {
                        parents[0]
                    } else if parents.len() > 1 {
                        parents.iter().sum::<usize>() / parents.len()
                    } else {
                        cursor_primary + node.width / 2
                    };
                    let mut x = desired_center.saturating_sub(node.width / 2);
                    if x < cursor_primary {
                        x = cursor_primary;
                    }
                    node.x = x;
                    node.y = rank_y;
                    cursor_primary = node.x + node.width + COL_SPACING;
                    placed_centers.insert(node.id.clone(), node.x + node.width / 2);
                }
                Direction::LR => {
                    node.x = rank_x;
                    node.y = cursor_primary;
                    cursor_primary += BOX_HEIGHT + ROW_SPACING;
                }
            }
        }
    }

    // Flip coordinates for BT (bottom-to-top)
    if matches!(graph.direction, Direction::BT) {
        let max_y = max_rank * row_gap;
        for node in &mut graph.nodes {
            node.y = max_y.saturating_sub(node.rank * row_gap);
        }
    }

    // Minimal guard: ensure width is at least BOX_MIN_WIDTH (already enforced by box_width)
    for node in &mut graph.nodes {
        if node.width < BOX_MIN_WIDTH {
            node.width = BOX_MIN_WIDTH;
        }
    }

    Ok(graph)
}

/// DFS-based cycle detection; marks back-edges in graph.edges and returns true if any cycles found
fn detect_cycles(graph: &mut Graph, adj: &[Vec<(usize, usize)>]) -> bool {
    let mut state = vec![0u8; graph.nodes.len()]; // 0=unvisited,1=visiting,2=done
    let mut has_cycle = false;

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
            if state[v] == 1 {
                *has_cycle = true;
                if let Some(edge) = edges.get_mut(edge_idx) {
                    edge.is_back_edge = true;
                }
                seen_edges.insert(edge_idx);
            } else if state[v] == 0 {
                dfs(v, state, adj, edges, has_cycle, seen_edges);
            }
        }
        state[u] = 2;
    }

    let mut seen_edges = HashSet::new();
    for u in 0..graph.nodes.len() {
        if state[u] == 0 {
            dfs(
                u,
                &mut state,
                adj,
                &mut graph.edges,
                &mut has_cycle,
                &mut seen_edges,
            );
        }
    }
    has_cycle
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Node;

    #[test]
    fn test_empty_graph() {
        let graph = Graph::new();
        let result = waterfall(graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_single_node() {
        let mut graph = Graph::new();
        graph.add_node(Node::new("A", "Node A"));
        let result = waterfall(graph).unwrap();
        assert_eq!(result.nodes.len(), 1);
        assert_eq!(result.nodes[0].x, 0);
    }

    #[test]
    fn test_cycle_marks_back_edge_and_warning() {
        let mut graph = Graph::new();
        graph.add_node(Node::new("A", "A"));
        graph.add_node(Node::new("B", "B"));
        graph.add_edge(crate::graph::Edge::new("A", "B"));
        graph.add_edge(crate::graph::Edge::new("B", "A"));

        let result = waterfall(graph).unwrap();
        assert!(result.edges.iter().any(|e| e.is_back_edge));
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("Cycle detected")));
    }
}
