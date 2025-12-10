//! Waterfall layout algorithm (deterministic, edge-aware)
//!
//! See SPEC §2.6 for algorithm details

use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::Result;

use crate::graph::{Direction, Graph, Rectangle};
use crate::style::{box_width, BOX_HEIGHT, BOX_MIN_WIDTH, COL_SPACING, STEM_LENGTH_HORIZONTAL};

/// Row spacing for simple edges without labels (minimal: stem → arrow)
const ROW_SPACING_MINIMAL: usize = 2;
/// Row spacing for labeled edges (stem → label → arrow)
const ROW_SPACING_LABELED: usize = 3;
/// Row spacing for fan-in (convergent) edges without labels (stems → junction → arrow)
const ROW_SPACING_FANIN: usize = 3;
/// Row spacing for fan-out (divergent) edges without labels (stem → junction → drops → arrows)
const ROW_SPACING_FANOUT: usize = 4;
/// Row spacing for multi-target edges with labels (stem → junction → label → arrow)
const ROW_SPACING_MULTI_LABELED: usize = 4;

/// Padding inside subgraph border (space between border and contained nodes)
const SUBGRAPH_PADDING: usize = 1;
/// Extra space for subgraph title line
const SUBGRAPH_TITLE_HEIGHT: usize = 1;

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
            .push("termiflow: warning: Cycle detected, rendering back-edges in gutter".to_string());
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
            if graph
                .edges
                .get(edge_idx)
                .map(|e| e.is_back_edge)
                .unwrap_or(false)
            {
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

    // Calculate per-rank spacing based on edge complexity and labels
    // Priority: ROW_SPACING_MULTI > ROW_SPACING_LABELED > ROW_SPACING_MINIMAL
    // - MULTI: Fan-out (source has multiple targets) or fan-in (target has multiple sources)
    // - LABELED: Any edge from this rank has a label
    // - MINIMAL: Simple edges without labels (most compact)
    let rank_spacing: Vec<usize> = (0..=max_rank)
        .map(|r| {
            // Check fan-out: source has multiple targets
            let mut has_fan_out = false;
            for &idx in &by_rank[r] {
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

            // Check fan-in: target at next rank has multiple sources from this rank
            let mut has_fan_in = false;
            if r < max_rank {
                for &idx in &by_rank[r + 1] {
                    let target_id = &graph.nodes[idx].id;
                    let source_count = graph
                        .edges
                        .iter()
                        .filter(|e| {
                            !e.is_back_edge
                                && &e.to == target_id
                                && by_rank[r]
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
            let has_labels = by_rank[r].iter().any(|&idx| {
                let source_id = &graph.nodes[idx].id;
                graph.edges.iter().any(|e| {
                    !e.is_back_edge && &e.from == source_id && e.label.is_some()
                })
            });

            // Priority: labeled multi > unlabeled fanout > unlabeled fanin > labeled > minimal
            if has_fan_out || has_fan_in {
                if has_labels {
                    ROW_SPACING_MULTI_LABELED
                } else if has_fan_out {
                    // Divergent needs space for drops: stem → junction → drops → arrows
                    ROW_SPACING_FANOUT
                } else {
                    // Convergent is more compact: stems → junction → arrow
                    ROW_SPACING_FANIN
                }
            } else if has_labels {
                ROW_SPACING_LABELED
            } else {
                ROW_SPACING_MINIMAL
            }
        })
        .collect();

    // Build cumulative Y offsets for each rank
    let mut rank_y_offset: Vec<usize> = vec![0; max_rank + 1];
    for r in 1..=max_rank {
        // Y offset for rank r = previous rank's offset + box height + spacing from previous rank
        rank_y_offset[r] = rank_y_offset[r - 1] + BOX_HEIGHT + rank_spacing[r - 1];
    }

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
    if matches!(graph.direction, Direction::LR | Direction::RL) {
        // Build a map from node index to rank (by_rank is rank -> [node indices])
        let mut node_to_rank: HashMap<usize, usize> = HashMap::new();
        for (rank, node_indices) in by_rank.iter().enumerate() {
            for &idx in node_indices {
                node_to_rank.insert(idx, rank);
            }
        }

        // Calculate max label width for edges between each pair of adjacent ranks
        // This ensures enough spacing for inline labels in horizontal layouts
        let mut max_label_width_per_rank: Vec<usize> = vec![0; by_rank.len()];

        // Track divergent edges (one source to multiple targets at different Y positions)
        // These need extra spacing for the vertical junction span
        let mut has_divergent_per_rank: Vec<bool> = vec![false; by_rank.len()];
        let mut targets_per_source: HashMap<String, Vec<String>> = HashMap::new();

        for edge in &graph.edges {
            if edge.is_back_edge {
                continue;
            }

            // Track targets per source for divergent edge detection
            targets_per_source
                .entry(edge.from.clone())
                .or_default()
                .push(edge.to.clone());

            if let Some(ref label) = edge.label {
                // Find ranks of source and target
                if let (Some(&from_idx), Some(&_to_idx)) =
                    (index_map.get(&edge.from), index_map.get(&edge.to))
                {
                    if let Some(&from_rank) = node_to_rank.get(&from_idx) {
                        // label + spaces around it + edge chars + arrow
                        // Format: ─ label ─→  needs: 1 + 1 + label + 1 + 1 + 1 = label + 5
                        let label_len = label.chars().count().min(12) + 6;
                        if label_len > max_label_width_per_rank[from_rank] {
                            max_label_width_per_rank[from_rank] = label_len;
                        }
                    }
                }
            }
        }

        // Mark ranks that have divergent edges (multiple targets from same source)
        for (source_id, targets) in &targets_per_source {
            if targets.len() > 1 {
                if let Some(&from_idx) = index_map.get(source_id) {
                    if let Some(&from_rank) = node_to_rank.get(&from_idx) {
                        has_divergent_per_rank[from_rank] = true;
                    }
                }
            }
        }

        // Detect convergent edges (multiple sources to same target)
        // These need extra spacing in the SOURCE rank for the merge junction
        let mut sources_per_target: HashMap<String, Vec<String>> = HashMap::new();
        for edge in &graph.edges {
            if edge.is_back_edge {
                continue;
            }
            sources_per_target
                .entry(edge.to.clone())
                .or_default()
                .push(edge.from.clone());
        }

        let mut has_convergent_per_rank: Vec<bool> = vec![false; by_rank.len()];
        for sources in sources_per_target.values() {
            if sources.len() > 1 {
                // Mark all SOURCE ranks that feed into this convergent target
                for source_id in sources {
                    if let Some(&from_idx) = index_map.get(source_id) {
                        if let Some(&from_rank) = node_to_rank.get(&from_idx) {
                            has_convergent_per_rank[from_rank] = true;
                        }
                    }
                }
            }
        }

        let mut offset = 0usize;
        for (r, w) in rank_widths.iter().enumerate() {
            rank_offset_x[r] = offset;
            // Add extra spacing for labels if any edges from this rank have labels
            let label_spacing = max_label_width_per_rank[r];
            // Add extra spacing for divergent/convergent edges (junction/merge span needs room)
            // STEM_LENGTH_HORIZONTAL (3) + junction span (1) + box-edge junction (1) + padding (2) = 7
            let junction_spacing = if has_divergent_per_rank[r] || has_convergent_per_rank[r] {
                STEM_LENGTH_HORIZONTAL + 1 + 1 + 2 // junction span + box-edge + padding
            } else {
                0
            };
            let spacing = COL_SPACING.max(label_spacing).max(junction_spacing);
            offset += w + spacing;
        }
    }

    // Pre-calculate node widths before the mutable loop
    let node_widths: Vec<usize> = graph.nodes.iter().map(|n| box_width(&n.label)).collect();

    // Build a map of node_id -> (x, width) that we update as we go
    // Initially just store the widths, x will be set during layout
    let mut node_positions: HashMap<String, (usize, usize)> = HashMap::new();

    for (r, nodes) in by_rank.iter().enumerate() {
        let mut cursor_primary = 0usize;
        let rank_y = rank_y_offset[r]; // Use dynamic Y offset based on edge complexity
        let rank_x = rank_offset_x[r];

        for &idx in nodes {
            let node_id = graph.nodes[idx].id.clone();
            let node_width = node_widths[idx];

            // Get parent info from already-placed nodes
            let parent_info: Vec<(usize, usize)> = graph
                .edges
                .iter()
                .filter(|e| !e.is_back_edge && e.to == node_id)
                .filter_map(|e| node_positions.get(&e.from).copied())
                .collect();

            let node_x = match graph.direction {
                Direction::TD | Direction::BT | Direction::TB => {
                    if parent_info.is_empty() {
                        // Root node: start at cursor
                        cursor_primary
                    } else if parent_info.len() == 1 {
                        // Single parent: center child under parent
                        let (parent_x, parent_width) = parent_info[0];
                        let parent_center = parent_x + parent_width / 2;
                        let child_x = parent_center.saturating_sub(node_width / 2);
                        child_x.max(cursor_primary)
                    } else {
                        // Multiple parents: center under the average
                        let avg_center = parent_info
                            .iter()
                            .map(|(px, pw)| px + pw / 2)
                            .sum::<usize>()
                            / parent_info.len();
                        let mut x = avg_center.saturating_sub(node_width / 2);
                        x = x.max(cursor_primary);
                        x
                    }
                }
                Direction::LR | Direction::RL => rank_x,
            };

            // Update the node
            let node = &mut graph.nodes[idx];
            node.width = node_width;
            node.rank = r;
            node.x = node_x;

            match graph.direction {
                Direction::TD | Direction::BT | Direction::TB => {
                    node.y = rank_y;
                    cursor_primary = node.x + node.width + COL_SPACING;
                    placed_centers.insert(node.id.clone(), node.x + node.width / 2);
                }
                Direction::LR | Direction::RL => {
                    node.y = cursor_primary;
                    cursor_primary += BOX_HEIGHT + 1; // Minimal spacing for horizontal layouts
                }
            }

            // Store position for child nodes to reference
            node_positions.insert(node_id, (node_x, node_width));
        }
    }

    // Flip coordinates for BT (bottom-to-top)
    if matches!(graph.direction, Direction::BT) {
        let max_y = rank_y_offset[max_rank];
        for node in &mut graph.nodes {
            node.y = max_y.saturating_sub(rank_y_offset[node.rank]);
        }
    }

    // Minimal guard: ensure width is at least BOX_MIN_WIDTH (already enforced by box_width)
    for node in &mut graph.nodes {
        if node.width < BOX_MIN_WIDTH {
            node.width = BOX_MIN_WIDTH;
        }
    }

    // Center rows/columns within the diagram based on direction
    match graph.direction {
        Direction::TD | Direction::TB | Direction::BT => {
            center_rows(&mut graph, &by_rank);
        }
        Direction::LR | Direction::RL => {
            center_columns(&mut graph, &by_rank);
        }
    }

    // For RL (Right-to-Left), reverse X coordinates
    if matches!(graph.direction, Direction::RL) {
        let max_x = graph.nodes.iter().map(|n| n.x + n.width).max().unwrap_or(0);
        for node in &mut graph.nodes {
            node.x = max_x.saturating_sub(node.x + node.width);
        }
    }

    // Calculate subgraph bounds after all node positioning is complete
    if graph.has_subgraphs() {
        calculate_subgraph_bounds(&mut graph);
    }

    Ok(graph)
}

/// Center columns of nodes vertically for LR layout
fn center_columns(graph: &mut Graph, by_rank: &[Vec<usize>]) {
    if graph.nodes.is_empty() {
        return;
    }

    // Calculate the vertical span (top_edge, bottom_edge) of each rank/column
    let rank_spans: Vec<(usize, usize)> = by_rank
        .iter()
        .map(|nodes| {
            if nodes.is_empty() {
                return (0, 0);
            }
            let top = nodes
                .iter()
                .map(|&idx| graph.nodes[idx].y)
                .min()
                .unwrap_or(0);
            let bottom = nodes
                .iter()
                .map(|&idx| graph.nodes[idx].y + BOX_HEIGHT)
                .max()
                .unwrap_or(0);
            (top, bottom)
        })
        .collect();

    // Find the maximum vertical span
    let max_height = rank_spans
        .iter()
        .map(|(_, bottom)| *bottom)
        .max()
        .unwrap_or(0);

    // Center each column vertically
    for (rank, nodes) in by_rank.iter().enumerate() {
        let (top, bottom) = rank_spans[rank];
        let span_height = bottom - top;
        
        if span_height < max_height {
            let offset = (max_height - span_height) / 2;
            
            // Apply offset to all nodes in this column
            for &idx in nodes {
                graph.nodes[idx].y += offset;
            }
        }
    }
}

/// Center each row of nodes within the diagram width
/// For rows with multiple nodes, center the group
/// For rows with single node that has edges, preserve parent-child alignment
fn center_rows(graph: &mut Graph, by_rank: &[Vec<usize>]) {
    if graph.nodes.is_empty() {
        return;
    }

    // Calculate the span (left_edge, right_edge) of each rank
    let rank_spans: Vec<(usize, usize)> = by_rank
        .iter()
        .map(|nodes| {
            if nodes.is_empty() {
                return (0, 0);
            }
            let left = nodes
                .iter()
                .map(|&idx| graph.nodes[idx].x)
                .min()
                .unwrap_or(0);
            let right = nodes
                .iter()
                .map(|&idx| graph.nodes[idx].x + graph.nodes[idx].width)
                .max()
                .unwrap_or(0);
            (left, right)
        })
        .collect();

    // Find the maximum row width (this becomes the diagram width)
    let diagram_width = rank_spans
        .iter()
        .map(|(l, r)| r.saturating_sub(*l))
        .max()
        .unwrap_or(0);

    if diagram_width == 0 {
        return;
    }

    for (r, nodes) in by_rank.iter().enumerate() {
        if nodes.is_empty() {
            continue;
        }

        let (row_left, row_right) = rank_spans[r];
        let row_width = row_right.saturating_sub(row_left);

        if nodes.len() == 1 {
            // Single-node row: center the node within the diagram width
            let idx = nodes[0];
            let target_left = (diagram_width.saturating_sub(graph.nodes[idx].width)) / 2;
            let current_left = graph.nodes[idx].x;
            if target_left != current_left {
                if target_left > current_left {
                    graph.nodes[idx].x += target_left - current_left;
                } else {
                    graph.nodes[idx].x = graph.nodes[idx]
                        .x
                        .saturating_sub(current_left - target_left);
                }
            }
        } else {
            // Multi-node row: center the entire group of nodes
            let target_left = (diagram_width.saturating_sub(row_width)) / 2;
            let current_left = row_left;

            if target_left != current_left {
                let shift = target_left as isize - current_left as isize;
                for &idx in nodes {
                    if shift > 0 {
                        graph.nodes[idx].x += shift as usize;
                    } else {
                        graph.nodes[idx].x = graph.nodes[idx].x.saturating_sub((-shift) as usize);
                    }
                }
            }
        }
    }
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

/// Calculate bounding boxes for all subgraphs based on their contained nodes.
///
/// For each subgraph:
/// 1. Find min/max coordinates of all contained nodes
/// 2. Add padding for the subgraph border
/// 3. Add title space if the subgraph has a title
/// 4. Store bounds and rank_range on the subgraph
fn calculate_subgraph_bounds(graph: &mut Graph) {
    // Build node_id -> (x, y, width, rank) lookup from current graph state
    let node_info: HashMap<&str, (usize, usize, usize, usize)> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), (n.x, n.y, n.width, n.rank)))
        .collect();

    for subgraph in &mut graph.subgraphs {
        if subgraph.node_ids.is_empty() {
            // Empty subgraph - no bounds to calculate
            subgraph.bounds = Rectangle::default();
            subgraph.rank_range = (0, 0);
            continue;
        }

        // Find min/max coordinates and ranks of all nodes in this subgraph
        let mut min_x = usize::MAX;
        let mut min_y = usize::MAX;
        let mut max_x = 0usize;
        let mut max_y = 0usize;
        let mut min_rank = usize::MAX;
        let mut max_rank = 0usize;

        for node_id in &subgraph.node_ids {
            if let Some(&(x, y, width, rank)) = node_info.get(node_id.as_str()) {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x + width);
                max_y = max_y.max(y + BOX_HEIGHT);
                min_rank = min_rank.min(rank);
                max_rank = max_rank.max(rank);
            }
        }

        // Ensure we found at least one node
        if min_x == usize::MAX {
            subgraph.bounds = Rectangle::default();
            subgraph.rank_range = (0, 0);
            continue;
        }

        // Add padding for the subgraph border
        // Border takes 1 char on each side, plus internal padding
        let padding = SUBGRAPH_PADDING;

        // Title space: if subgraph has a title, add extra height at top
        // For now, title goes at top regardless of direction (Phase 5 can adjust)
        let title_space = if subgraph.has_title() {
            SUBGRAPH_TITLE_HEIGHT
        } else {
            0
        };

        // Calculate bounds with padding
        // x/y are the top-left corner of the subgraph border
        let bounds_x = min_x.saturating_sub(padding);
        let bounds_y = min_y.saturating_sub(padding + title_space);
        let bounds_width = (max_x - min_x) + (padding * 2);
        let bounds_height = (max_y - min_y) + (padding * 2) + title_space;

        subgraph.bounds = Rectangle::new(bounds_x, bounds_y, bounds_width, bounds_height);
        subgraph.rank_range = (min_rank, max_rank);
    }
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
        assert!(result.warnings.iter().any(|w| w.contains("Cycle detected")));
    }

    #[test]
    fn test_roots_anchor_at_zero_simple() {
        let mut graph = Graph::new();
        graph.add_node(Node::new("A", "A"));
        graph.add_node(Node::new("B", "B"));
        graph.add_node(Node::new("C", "C"));
        graph.add_edge(crate::graph::Edge::new("A", "B"));
        graph.add_edge(crate::graph::Edge::new("A", "C"));

        let result = waterfall(graph).unwrap();
        // Verify layout produces valid positions (leftmost node at reasonable position)
        let min_x = result.nodes.iter().map(|n| n.x).min().unwrap_or(0);
        assert_eq!(min_x, 0, "leftmost node should anchor at x=0");
        // Children should be ordered left-to-right
        assert!(
            result.get_node("C").unwrap().x >= result.get_node("B").unwrap().x,
            "children should be ordered left-to-right"
        );
    }

    #[test]
    fn test_roots_anchor_at_zero_multiple_ranks() {
        // Fan-out followed by fan-in; earlier ranks should not shift right
        let mut graph = Graph::new();
        graph.add_node(Node::new("A", "Root"));
        graph.add_node(Node::new("B", "Left"));
        graph.add_node(Node::new("C", "Right"));
        graph.add_node(Node::new("D", "Sink"));
        graph.add_edge(crate::graph::Edge::new("A", "B"));
        graph.add_edge(crate::graph::Edge::new("A", "C"));
        graph.add_edge(crate::graph::Edge::new("B", "D"));
        graph.add_edge(crate::graph::Edge::new("C", "D"));

        let result = waterfall(graph).unwrap();
        // Verify layout anchors leftmost node at x=0
        let min_x = result.nodes.iter().map(|n| n.x).min().unwrap_or(0);
        assert_eq!(min_x, 0, "leftmost node should anchor at x=0");
        assert!(
            result.get_node("C").unwrap().x >= result.get_node("B").unwrap().x,
            "children should remain ordered"
        );
    }

    #[test]
    fn test_fixtures_anchor_left() {
        let fixtures = [
            "tests/fixtures/inputs/flow_chain_td.md",
            "tests/fixtures/inputs/shape_database_td.md",
            "tests/fixtures/inputs/parse_forward_td.md",
            "tests/fixtures/inputs/flow_branch_td.md",
            "tests/fixtures/inputs/config_style_td.md",
            "tests/fixtures/inputs/subgraph_basic_td.md",
        ];

        for path in fixtures {
            let input = std::fs::read_to_string(path).expect("read fixture");
            let parsed = crate::parser::parse(&input, false).expect("parse fixture");
            let graph = waterfall(parsed.graph).expect("layout fixture");
            let min_x = graph.nodes.iter().map(|n| n.x).min().unwrap_or(0);
            assert_eq!(
                min_x, 0,
                "expected left anchor at x=0 for fixture {} (got {})",
                path, min_x
            );
        }
    }

    #[test]
    fn test_lr_orientation_positions() {
        let mut graph = Graph::new();
        graph.direction = Direction::LR;
        graph.add_node(Node::new("A", "Root"));
        graph.add_node(Node::new("B", "Mid"));
        graph.add_node(Node::new("C", "Leaf"));
        graph.add_edge(crate::graph::Edge::new("A", "B"));
        graph.add_edge(crate::graph::Edge::new("B", "C"));

        let result = waterfall(graph).unwrap();
        // Columns should advance in x; y resets per column
        let a = result.get_node("A").unwrap();
        let b = result.get_node("B").unwrap();
        let c = result.get_node("C").unwrap();
        assert_eq!(a.y, 0);
        assert_eq!(b.y, 0);
        assert_eq!(c.y, 0);
        assert!(b.x > a.x, "next column should be to the right");
        assert!(c.x > b.x, "third column should be further right");
    }

    #[test]
    fn test_bt_orientation_flips_vertical() {
        let mut graph = Graph::new();
        graph.direction = Direction::BT;
        graph.add_node(Node::new("A", "Root"));
        graph.add_node(Node::new("B", "Child"));
        graph.add_edge(crate::graph::Edge::new("A", "B"));

        let result = waterfall(graph).unwrap();
        let a = result.get_node("A").unwrap();
        let b = result.get_node("B").unwrap();
        assert!(a.y > b.y, "BT should place root below its child");
    }

    // === SUBGRAPH BOUNDS TESTS ===

    #[test]
    fn test_subgraph_bounds_basic() {
        use crate::graph::Subgraph;

        let mut graph = Graph::new();
        graph.add_node(Node::new("A", "Node A"));
        graph.add_node(Node::new("B", "Node B"));
        graph.add_edge(crate::graph::Edge::new("A", "B"));

        // Create subgraph containing both nodes
        let mut sg = Subgraph::new("SG1", Some("Test Group".to_string()));
        sg.add_node("A");
        sg.add_node("B");
        graph.add_subgraph(sg);
        graph.associate_node_with_subgraph("A", "SG1");
        graph.associate_node_with_subgraph("B", "SG1");

        let result = waterfall(graph).unwrap();

        // Verify subgraph bounds were calculated
        assert_eq!(result.subgraphs.len(), 1);
        let sg_bounds = &result.subgraphs[0].bounds;
        assert!(sg_bounds.is_valid(), "subgraph should have valid bounds");

        // Bounds should encompass both nodes with padding
        let a = result.get_node("A").unwrap();
        let b = result.get_node("B").unwrap();
        assert!(
            sg_bounds.x <= a.x,
            "bounds x should be at or before node A"
        );
        assert!(
            sg_bounds.y <= a.y,
            "bounds y should be at or before node A (with title space)"
        );

        // Verify rank_range
        let (min_rank, max_rank) = result.subgraphs[0].rank_range;
        assert_eq!(min_rank, 0);
        assert_eq!(max_rank, 1);
    }

    #[test]
    fn test_subgraph_bounds_empty() {
        use crate::graph::Subgraph;

        let mut graph = Graph::new();
        graph.add_node(Node::new("A", "Node A"));

        // Create empty subgraph
        let sg = Subgraph::new("Empty", None);
        graph.add_subgraph(sg);

        let result = waterfall(graph).unwrap();

        // Empty subgraph should have default (zero) bounds
        let sg_bounds = &result.subgraphs[0].bounds;
        assert!(!sg_bounds.is_valid(), "empty subgraph should have invalid bounds");
    }

    #[test]
    fn test_subgraph_bounds_with_title() {
        use crate::graph::Subgraph;

        let mut graph = Graph::new();
        graph.add_node(Node::new("A", "Node"));

        // Subgraph with title
        let mut sg_with_title = Subgraph::new("SG1", Some("My Title".to_string()));
        sg_with_title.add_node("A");
        graph.add_subgraph(sg_with_title);
        graph.associate_node_with_subgraph("A", "SG1");

        let result = waterfall(graph).unwrap();
        let bounds_with_title = result.subgraphs[0].bounds.height;

        // Now test without title
        let mut graph2 = Graph::new();
        graph2.add_node(Node::new("A", "Node"));

        let mut sg_no_title = Subgraph::new("SG2", None);
        sg_no_title.add_node("A");
        graph2.add_subgraph(sg_no_title);
        graph2.associate_node_with_subgraph("A", "SG2");

        let result2 = waterfall(graph2).unwrap();
        let bounds_no_title = result2.subgraphs[0].bounds.height;

        // Subgraph with title should be taller
        assert!(
            bounds_with_title > bounds_no_title,
            "subgraph with title ({}) should be taller than without ({})",
            bounds_with_title,
            bounds_no_title
        );
    }
}
