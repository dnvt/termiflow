//! Waterfall layout algorithm (deterministic, edge-aware)
//!
//! See SPEC §2.6 for algorithm details

use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::Result;

use crate::graph::{Direction, Graph, Rectangle};
use crate::style::{box_width, BOX_HEIGHT, BOX_MIN_WIDTH, COL_SPACING};

/// Result of rank calculation: (node_id -> rank, adjacency list by node index)
type RankResult = (HashMap<String, usize>, Vec<Vec<usize>>);

/// Row spacing for single-target edges (compact: stem → label → arrow)
const ROW_SPACING_SINGLE: usize = 3;
/// Row spacing for multi-target edges (needs extra row: stem → junction → label → arrow)
const ROW_SPACING_MULTI: usize = 4;

/// Apply waterfall layout to position all nodes
pub fn waterfall(mut graph: Graph) -> Result<Graph> {
    if graph.nodes.is_empty() {
        return Ok(graph);
    }

    // If we have subgraphs, use hierarchical layout
    if !graph.subgraphs.is_empty() {
        return hierarchical_waterfall(graph);
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

    // Calculate per-rank spacing based on edge complexity
    // A rank needs ROW_SPACING_MULTI if:
    // 1. Fan-out: ANY source at that rank has multiple targets, OR
    // 2. Fan-in: ANY target at the next rank has multiple sources from this rank
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

            if has_fan_out || has_fan_in {
                ROW_SPACING_MULTI
            } else {
                ROW_SPACING_SINGLE
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
    if matches!(graph.direction, Direction::LR) {
        let mut offset = 0usize;
        for (r, w) in rank_widths.iter().enumerate() {
            rank_offset_x[r] = offset;
            offset += w + COL_SPACING;
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
                Direction::LR => rank_x,
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
                Direction::LR => {
                    node.y = cursor_primary;
                    cursor_primary += BOX_HEIGHT + ROW_SPACING_MULTI;
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

    // Center rows within the diagram (TD/TB/BT only)
    if matches!(
        graph.direction,
        Direction::TD | Direction::TB | Direction::BT
    ) {
        center_rows(&mut graph, &by_rank);
    }

    // Calculate subgraph bounds if any subgraphs exist
    if !graph.subgraphs.is_empty() {
        calculate_subgraph_bounds(&mut graph);
    }

    Ok(graph)
}

// ============================================================================
// PHASE 1: MEASUREMENT - Data structures and functions
// ============================================================================

/// Pre-calculated metrics for a subgraph (computed before positioning)
#[derive(Debug, Clone)]
struct SubgraphMetrics {
    id: String,
    content_width: usize,      // Width needed for nodes (max node width + padding)
    title_width: usize,        // Width needed for title
    final_width: usize,        // max(content_width, title_width)
    rank_range: (usize, usize), // (min_rank, max_rank) of contained nodes
    has_title: bool,
}

/// Connection between subgraphs (for centering calculations)
#[derive(Debug, Clone)]
struct SubgraphConnection {
    from_sg: String,
    to_sg: String,
}

/// Calculate metrics for all subgraphs (Phase 1)
fn calculate_subgraph_metrics(
    graph: &Graph,
    by_rank: &[Vec<usize>],
) -> HashMap<String, SubgraphMetrics> {
    const SUBGRAPH_PADDING_H: usize = 2;

    let mut metrics: HashMap<String, SubgraphMetrics> = HashMap::new();

    for subgraph in &graph.subgraphs {
        if subgraph.node_ids.is_empty() {
            continue;
        }

        // Calculate content width: max node width + horizontal padding
        let max_node_width = graph.nodes.iter()
            .filter(|n| subgraph.node_ids.contains(&n.id))
            .map(|n| box_width(&n.label).max(BOX_MIN_WIDTH))
            .max()
            .unwrap_or(BOX_MIN_WIDTH);
        let content_width = max_node_width + 2 * SUBGRAPH_PADDING_H;

        // Calculate title width: title text + border chars + padding
        let title_width = if let Some(ref title) = subgraph.title {
            crate::style::display_width(title) + 4 // 2 borders + 2 padding
        } else {
            0
        };

        // Final width is the maximum
        let final_width = content_width.max(title_width);

        // Find rank range for this subgraph
        let ranks: Vec<usize> = by_rank.iter()
            .enumerate()
            .filter(|(_, nodes)| {
                nodes.iter().any(|&idx| subgraph.node_ids.contains(&graph.nodes[idx].id))
            })
            .map(|(r, _)| r)
            .collect();

        let min_rank = ranks.iter().min().copied().unwrap_or(0);
        let max_rank = ranks.iter().max().copied().unwrap_or(0);

        metrics.insert(subgraph.id.clone(), SubgraphMetrics {
            id: subgraph.id.clone(),
            content_width,
            title_width,
            final_width,
            rank_range: (min_rank, max_rank),
            has_title: subgraph.title.is_some(),
        });
    }

    metrics
}

/// Find connections between subgraphs (for centering)
fn find_subgraph_connections(graph: &Graph) -> Vec<SubgraphConnection> {
    let mut connections: Vec<SubgraphConnection> = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();

    for edge in &graph.edges {
        if edge.is_back_edge {
            continue;
        }

        let from_sg = graph.node_subgraph.get(&edge.from);
        let to_sg = graph.node_subgraph.get(&edge.to);

        // Only track cross-subgraph connections
        match (from_sg, to_sg) {
            (Some(from), Some(to)) if from != to => {
                let key = (from.clone(), to.clone());
                if !seen.contains(&key) {
                    seen.insert(key);
                    connections.push(SubgraphConnection {
                        from_sg: from.clone(),
                        to_sg: to.clone(),
                    });
                }
            }
            (None, Some(to)) => {
                // Global node → subgraph (track for centering global nodes)
                let key = (String::new(), to.clone());
                if !seen.contains(&key) {
                    seen.insert(key);
                    connections.push(SubgraphConnection {
                        from_sg: String::new(), // Empty = global node
                        to_sg: to.clone(),
                    });
                }
            }
            _ => {}
        }
    }

    connections
}

// ============================================================================
// PHASE 2: SUBGRAPH POSITIONING
// ============================================================================

/// Check if two rank ranges overlap
fn ranges_overlap(a: (usize, usize), b: (usize, usize)) -> bool {
    // Ranges [a.0, a.1] and [b.0, b.1] overlap if max(start) <= min(end)
    a.0.max(b.0) <= a.1.min(b.1)
}

/// Build a map of subgraph flow relationships from the graph edges.
/// Returns: HashMap where key has edges TO any subgraph in the value set.
fn build_subgraph_flow_graph(graph: &Graph) -> HashMap<String, HashSet<String>> {
    let mut flow_to: HashMap<String, HashSet<String>> = HashMap::new();

    for edge in &graph.edges {
        if edge.is_back_edge {
            continue;
        }
        let from_sg = graph.node_subgraph.get(&edge.from);
        let to_sg = graph.node_subgraph.get(&edge.to);

        if let (Some(from), Some(to)) = (from_sg, to_sg) {
            if from != to {
                // Edge goes from one subgraph to another
                flow_to.entry(from.clone()).or_default().insert(to.clone());
            }
        }
    }

    flow_to
}

/// Check if two subgraphs have a flow relationship (edges between them in either direction)
fn has_flow_relationship(
    sg_a: &str,
    sg_b: &str,
    flow_graph: &HashMap<String, HashSet<String>>,
) -> bool {
    // Check if A → B
    let a_to_b = flow_graph.get(sg_a).map_or(false, |targets| targets.contains(sg_b));
    // Check if B → A
    let b_to_a = flow_graph.get(sg_b).map_or(false, |targets| targets.contains(sg_a));
    a_to_b || b_to_a
}

/// Group subgraphs into horizontal "columns" based on rank ranges only.
///
/// Key insight from Mermaid: Subgraphs are laid out in a single column by default,
/// stacked vertically. Side-by-side layout only happens when subgraphs have no
/// dependency relationship (completely independent).
///
/// For now, use a simple approach: all subgraphs in a single column (vertical stack).
/// This matches the Mermaid behavior for most diagrams.
///
/// Returns: Vec<Vec<String>> where each inner Vec is a column of subgraphs.
fn group_subgraphs_into_columns(
    metrics: &HashMap<String, SubgraphMetrics>,
    subgraph_order: &[String],
    graph: &Graph,
) -> Vec<Vec<String>> {
    let flow_graph = build_subgraph_flow_graph(graph);

    // Find groups of subgraphs that are truly independent (no flow between them)
    // For now, check if there are any subgraphs with NO flow relationship to any other
    let mut independent_groups: Vec<Vec<String>> = Vec::new();

    for sg_id in subgraph_order {
        if metrics.get(sg_id).is_none() {
            continue;
        }

        // Check if this subgraph has flow with any existing group
        let mut found_group = false;
        for group in &mut independent_groups {
            let has_flow_with_any = group.iter().any(|existing_id| {
                has_flow_relationship(sg_id, existing_id, &flow_graph)
            });

            if has_flow_with_any {
                // Has flow relationship - must be in same group (column)
                group.push(sg_id.clone());
                found_group = true;
                break;
            }
        }

        if !found_group {
            // Check if we can merge with an existing group through transitivity
            // by checking if ANY subgraph in existing groups has flow with us
            let mut merged = false;
            for group in &mut independent_groups {
                // Check transitive flow: if A→B and B→C, then A and C are in same group
                for existing_id in group.clone() {
                    if has_flow_relationship(sg_id, &existing_id, &flow_graph) {
                        group.push(sg_id.clone());
                        merged = true;
                        break;
                    }
                }
                if merged {
                    break;
                }
            }

            if !merged {
                // Truly independent - new group
                independent_groups.push(vec![sg_id.clone()]);
            }
        }
    }

    // Now convert groups to columns
    // Groups with flow relationships become a single column
    // Multiple independent groups become multiple columns
    independent_groups
}

/// Position subgraphs with overlap-aware column layout
fn position_subgraphs(
    metrics: &HashMap<String, SubgraphMetrics>,
    connections: &[SubgraphConnection],
    subgraph_order: &[String],
    graph: &Graph,
) -> HashMap<String, usize> {
    let mut positions: HashMap<String, usize> = HashMap::new();

    if subgraph_order.is_empty() {
        return positions;
    }

    // Group subgraphs into columns based on flow relationships and rank ranges
    let columns = group_subgraphs_into_columns(metrics, subgraph_order, graph);

    // Track parent-child relationships for centering
    let mut parent_center: HashMap<String, usize> = HashMap::new();

    // Calculate width of each column (max width of subgraphs in that column)
    let column_widths: Vec<usize> = columns.iter()
        .map(|col| {
            col.iter()
                .filter_map(|id| metrics.get(id).map(|m| m.final_width))
                .max()
                .unwrap_or(BOX_MIN_WIDTH)
        })
        .collect();

    // Calculate cumulative X offsets for each column
    let spacing = COL_SPACING * 2;
    let mut column_x_starts: Vec<usize> = vec![0; columns.len()];
    for i in 1..columns.len() {
        column_x_starts[i] = column_x_starts[i - 1] + column_widths[i - 1] + spacing;
    }

    // First pass: position each subgraph centered within its column
    for (col_idx, column) in columns.iter().enumerate() {
        let col_x = column_x_starts[col_idx];
        let col_width = column_widths[col_idx];

        for sg_id in column {
            let width = metrics.get(sg_id).map(|m| m.final_width).unwrap_or(BOX_MIN_WIDTH);
            // Center within column
            let x = col_x + (col_width.saturating_sub(width)) / 2;
            positions.insert(sg_id.clone(), x);
            parent_center.insert(sg_id.clone(), x + width / 2);
        }
    }

    // Second pass: adjust first column based on parent connections (centering under parents)
    // Only adjust if there's a clear parent relationship
    if !columns.is_empty() {
        for sg_id in &columns[0] {
            // Check if this subgraph has a parent connection from a global node
            let _parent_x = connections.iter()
                .find(|c| c.to_sg == *sg_id && c.from_sg.is_empty())
                .map(|_| {
                    // Has global parent - keep centered (already positioned)
                });
        }
    }

    positions
}

// ============================================================================
// MAIN LAYOUT FUNCTION
// ============================================================================

/// Apply hierarchical waterfall layout for graphs with subgraphs
///
/// Supports TD/TB (top-down), LR (left-right), and BT (bottom-top) directions.
fn hierarchical_waterfall(mut graph: Graph) -> Result<Graph> {
    // Constants for subgraph spacing - must match calculate_subgraph_bounds
    // title_space = TITLE_HEIGHT(1) + TITLE_PADDING(1) + ENTRY_ARROW_SPACE(1) = 3
    // This is extra vertical space added between ranks when a subgraph starts
    const SUBGRAPH_ENTRY_SPACE: usize = 3; // Title + padding + entry arrow

    // ========================================================================
    // PHASE 1: MEASUREMENT
    // ========================================================================

    // Step 1.1: Calculate ranks for all nodes
    let (_node_ranks, by_rank) = calculate_node_ranks(&mut graph)?;

    // Step 2: Position nodes with subgraph-aware grouping
    let max_rank = by_rank.len();
    let is_horizontal = matches!(graph.direction, Direction::LR);

    // Determine which ranks need extra top space for subgraph titles
    // A rank needs top space if it's the first rank of any subgraph
    let mut rank_needs_sg_top: Vec<bool> = vec![false; max_rank];
    // Track which ranks are the last rank of a subgraph (need exit space after them)
    let mut rank_needs_sg_bottom: Vec<bool> = vec![false; max_rank];

    for subgraph in &graph.subgraphs {
        if subgraph.node_ids.is_empty() {
            continue;
        }
        // Find the min/max rank of nodes in this subgraph
        let ranks: Vec<usize> = by_rank
            .iter()
            .enumerate()
            .filter(|(_, nodes)| {
                nodes.iter().any(|&idx| {
                    subgraph.node_ids.contains(&graph.nodes[idx].id)
                })
            })
            .map(|(r, _)| r)
            .collect();

        if let Some(&min_rank) = ranks.iter().min() {
            rank_needs_sg_top[min_rank] = true;
        }
        if let Some(&max_rank) = ranks.iter().max() {
            rank_needs_sg_bottom[max_rank] = true;
        }
    }

    // Calculate cumulative offsets for each rank, accounting for subgraph titles
    let mut rank_offsets: Vec<usize> = vec![0; max_rank];

    // First rank may need offset for subgraph entry space
    if !rank_needs_sg_top.is_empty() && rank_needs_sg_top[0] {
        rank_offsets[0] = SUBGRAPH_ENTRY_SPACE;
    }

    // Extra space needed when a subgraph ends (for exit junction + stem)
    const SUBGRAPH_EXIT_SPACE: usize = 2; // Junction row + stem row

    for r in 1..max_rank {
        let base_spacing = calculate_rank_spacing(&graph, r - 1);

        // Calculate extra spacing needed:
        // - Entry only: title_space (3) + stem_row (1) = 4
        // - Exit only: junction_row (1) + stem_row (1) = 2
        // - Both (cross-subgraph): need exit junction BELOW source subgraph bottom,
        //   plus stem to entry, plus entry_space. Total = 5 to ensure entry > exit + 1
        let both_exit_and_entry = rank_needs_sg_bottom[r - 1] && rank_needs_sg_top[r];
        let spacing = if both_exit_and_entry {
            base_spacing.max(5) // Cross-subgraph: junction + stem + entry
        } else if rank_needs_sg_top[r] {
            base_spacing.max(4) // Entry: title_space(3) + stem_row(1)
        } else if rank_needs_sg_bottom[r - 1] {
            base_spacing.max(SUBGRAPH_EXIT_SPACE) // Exit: junction + stem
        } else {
            base_spacing
        };

        if is_horizontal {
            // LR: ranks advance in X, nodes stack in Y
            let max_width = by_rank[r - 1]
                .iter()
                .map(|&idx| box_width(&graph.nodes[idx].label).max(BOX_MIN_WIDTH))
                .max()
                .unwrap_or(BOX_MIN_WIDTH);
            rank_offsets[r] = rank_offsets[r - 1] + max_width + COL_SPACING + spacing;
        } else {
            // TD/TB/BT: ranks advance in Y, nodes spread in X
            rank_offsets[r] = rank_offsets[r - 1] + BOX_HEIGHT + spacing;
        }
    }

    // Order subgraphs by their minimum node rank (earlier ranks = more left)
    // This creates a natural left-to-right flow matching the topology
    let mut subgraph_min_rank: Vec<(String, usize)> = graph.subgraphs.iter()
        .map(|s| {
            let min_rank = by_rank.iter()
                .enumerate()
                .filter(|(_, nodes)| nodes.iter().any(|&idx| s.node_ids.contains(&graph.nodes[idx].id)))
                .map(|(r, _)| r)
                .min()
                .unwrap_or(usize::MAX);
            (s.id.clone(), min_rank)
        })
        .collect();
    subgraph_min_rank.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    let subgraph_order: Vec<String> = subgraph_min_rank.into_iter().map(|(id, _)| id).collect();

    // Step 1.2: Calculate subgraph metrics (widths considering both content and title)
    let sg_metrics = calculate_subgraph_metrics(&graph, &by_rank);

    // Step 1.3: Find subgraph connections (for centering)
    let sg_connections = find_subgraph_connections(&graph);

    // ========================================================================
    // PHASE 2: SUBGRAPH POSITIONING
    // ========================================================================

    // Position subgraphs using flow-aware column layout
    let subgraph_x_start = position_subgraphs(&sg_metrics, &sg_connections, &subgraph_order, &graph);

    // Build a map of final subgraph widths for node positioning
    let subgraph_widths: HashMap<String, usize> = sg_metrics.iter()
        .map(|(id, m)| (id.clone(), m.final_width))
        .collect();

    // Build global node chain: each global node maps to its ultimate target subgraph
    // This allows cascading centering: A → B → subgraph means A and B both center over subgraph
    let mut global_node_target_sg: HashMap<String, String> = HashMap::new();
    let mut global_node_direct_target: HashMap<String, String> = HashMap::new(); // For edge following

    // First pass: find direct targets for each global node
    for node in &graph.nodes {
        if !graph.node_subgraph.contains_key(&node.id) {
            for edge in &graph.edges {
                if edge.from == node.id && !edge.is_back_edge {
                    global_node_direct_target.insert(node.id.clone(), edge.to.clone());
                    break;
                }
            }
        }
    }

    // Second pass: follow chains to find ultimate subgraph target
    for node in &graph.nodes {
        if !graph.node_subgraph.contains_key(&node.id) {
            let mut current = node.id.clone();
            let mut visited: HashSet<String> = HashSet::new();

            // Follow the chain until we hit a subgraph or a cycle
            while let Some(target) = global_node_direct_target.get(&current) {
                if visited.contains(target) {
                    break; // Cycle detected
                }
                visited.insert(current.clone());

                if let Some(sg) = graph.node_subgraph.get(target) {
                    // Found a subgraph target
                    global_node_target_sg.insert(node.id.clone(), sg.clone());
                    break;
                }
                current = target.clone();
            }
        }
    }

    // Position for orphan global nodes (not connected to any subgraph)
    let global_x_start = subgraph_x_start.values().max().copied().unwrap_or(0)
        + subgraph_widths.values().max().copied().unwrap_or(BOX_MIN_WIDTH)
        + COL_SPACING * 2;

    // ========================================================================
    // PHASE 3: NODE POSITIONING (distributed within subgraphs)
    // ========================================================================

    // Pre-compute node widths
    let node_widths: Vec<usize> = graph.nodes.iter()
        .map(|n| box_width(&n.label).max(BOX_MIN_WIDTH))
        .collect();

    for rank_idx in 0..max_rank {
        let nodes_at_rank = &by_rank[rank_idx];

        // Group nodes by their subgraph (or "global" for nodes not in subgraphs)
        let mut nodes_by_sg: HashMap<Option<String>, Vec<usize>> = HashMap::new();
        for &node_idx in nodes_at_rank {
            let sg_id = graph.node_subgraph.get(&graph.nodes[node_idx].id).cloned();
            nodes_by_sg.entry(sg_id).or_default().push(node_idx);
        }

        // Position each group
        for (sg_id, node_indices) in nodes_by_sg {
            if is_horizontal {
                // LR mode: not fully implemented for subgraphs yet
                for &node_idx in &node_indices {
                    let node = &mut graph.nodes[node_idx];
                    node.width = node_widths[node_idx];
                    node.rank = rank_idx;
                    node.x = rank_offsets[rank_idx];
                    node.y = 0;
                }
                continue;
            }

            // TD/TB/BT mode: distribute nodes horizontally within their subgraph
            let (sg_start, sg_width) = if let Some(ref sg) = sg_id {
                (
                    subgraph_x_start.get(sg).copied().unwrap_or(0),
                    subgraph_widths.get(sg).copied().unwrap_or(BOX_MIN_WIDTH),
                )
            } else {
                // Global nodes: find target subgraph or use orphan position
                let first_node_id = &graph.nodes[node_indices[0]].id;
                if let Some(target_sg) = global_node_target_sg.get(first_node_id) {
                    (
                        subgraph_x_start.get(target_sg).copied().unwrap_or(0),
                        subgraph_widths.get(target_sg).copied().unwrap_or(BOX_MIN_WIDTH),
                    )
                } else {
                    (global_x_start, BOX_MIN_WIDTH)
                }
            };

            if node_indices.len() == 1 {
                // Single node: center within subgraph
                let node_idx = node_indices[0];
                let node = &mut graph.nodes[node_idx];
                node.width = node_widths[node_idx];
                node.rank = rank_idx;
                let sg_center = sg_start + sg_width / 2;
                node.x = sg_center.saturating_sub(node.width / 2);
                node.y = rank_offsets[rank_idx];
            } else {
                // Multiple nodes: distribute horizontally within subgraph
                // Calculate total width needed for all nodes with spacing
                let total_node_width: usize = node_indices.iter()
                    .map(|&idx| node_widths[idx])
                    .sum();
                let total_spacing = COL_SPACING * (node_indices.len() - 1);
                let total_needed = total_node_width + total_spacing;

                // Center the group within the subgraph
                let group_start = sg_start + (sg_width.saturating_sub(total_needed)) / 2;

                // Place nodes left to right
                let mut cursor_x = group_start;
                // Sort by node ID for deterministic ordering
                let mut sorted_indices = node_indices.clone();
                sorted_indices.sort_by_key(|&idx| graph.nodes[idx].id.clone());

                for &node_idx in &sorted_indices {
                    let node = &mut graph.nodes[node_idx];
                    node.width = node_widths[node_idx];
                    node.rank = rank_idx;
                    node.x = cursor_x;
                    node.y = rank_offsets[rank_idx];
                    cursor_x += node.width + COL_SPACING;
                }
            }
        }
    }

    // Step 3: Flip Y coordinates for BT (bottom-to-top)
    if matches!(graph.direction, Direction::BT) {
        let max_y = rank_offsets.last().copied().unwrap_or(0);
        for node in &mut graph.nodes {
            node.y = max_y.saturating_sub(rank_offsets[node.rank]);
        }
    }

    // Step 4: Calculate subgraph bounds
    calculate_subgraph_bounds(&mut graph);

    // Step 5: Center the entire layout
    center_layout(&mut graph);

    Ok(graph)
}

/// Calculate node ranks using topological sort with subgraph separation
///
/// Ensures nodes in downstream subgraphs have ranks strictly greater than
/// all nodes in upstream subgraphs, preventing visual overlap.
fn calculate_node_ranks(graph: &mut Graph) -> Result<RankResult> {
    let mut index_map: HashMap<String, usize> = HashMap::new();
    for (idx, node) in graph.nodes.iter().enumerate() {
        index_map.insert(node.id.clone(), idx);
    }

    // Build adjacency list
    let mut adj: Vec<Vec<usize>> = vec![vec![]; graph.nodes.len()];
    let mut indegree = vec![0usize; graph.nodes.len()];

    for edge in &graph.edges {
        if edge.is_back_edge {
            continue;
        }
        if let (Some(&from_idx), Some(&to_idx)) =
            (index_map.get(&edge.from), index_map.get(&edge.to)) {
            adj[from_idx].push(to_idx);
            indegree[to_idx] += 1;
        }
    }

    // Step 1: Calculate subgraph base ranks using topological sort on subgraph flow graph
    // This ensures downstream subgraphs start at higher ranks
    let subgraph_base_rank = calculate_subgraph_base_ranks(graph);

    // Step 2: Calculate local ranks WITHIN each subgraph
    // This ignores edges that cross subgraph boundaries

    // For each subgraph, calculate local ranks based only on internal edges
    let mut local_ranks = vec![0usize; graph.nodes.len()];

    // Calculate local ranks within each subgraph independently
    for subgraph in &graph.subgraphs {
        // Get node indices in this subgraph
        let sg_node_ids: HashSet<&String> = subgraph.node_ids.iter().collect();
        let sg_node_indices: Vec<usize> = graph.nodes.iter()
            .enumerate()
            .filter(|(_, n)| sg_node_ids.contains(&n.id))
            .map(|(i, _)| i)
            .collect();

        if sg_node_indices.is_empty() {
            continue;
        }

        // Build subgraph-local indegree (only counting edges from within the subgraph)
        let mut local_indegree: HashMap<usize, usize> = HashMap::new();
        for &idx in &sg_node_indices {
            local_indegree.insert(idx, 0);
        }

        for edge in &graph.edges {
            if edge.is_back_edge {
                continue;
            }
            let from_in_sg = sg_node_ids.contains(&edge.from);
            let to_in_sg = sg_node_ids.contains(&edge.to);
            if from_in_sg && to_in_sg {
                // Internal edge
                if let Some(&to_idx) = index_map.get(&edge.to) {
                    *local_indegree.entry(to_idx).or_insert(0) += 1;
                }
            }
        }

        // Topological sort within this subgraph
        let mut queue: VecDeque<usize> = local_indegree.iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&idx, _)| idx)
            .collect();

        while let Some(u) = queue.pop_front() {
            let current_rank = local_ranks[u];
            for &v in &adj[u] {
                // Only process if target is also in this subgraph
                if !local_indegree.contains_key(&v) {
                    continue;
                }
                if let Some(deg) = local_indegree.get_mut(&v) {
                    if *deg > 0 {
                        *deg -= 1;
                        local_ranks[v] = local_ranks[v].max(current_rank + 1);
                        if *deg == 0 {
                            queue.push_back(v);
                        }
                    }
                }
            }
        }
    }

    // Also handle global nodes (not in any subgraph)
    {
        let global_nodes: Vec<usize> = graph.nodes.iter()
            .enumerate()
            .filter(|(_, n)| graph.node_subgraph.get(&n.id).is_none())
            .map(|(i, _)| i)
            .collect();

        // For global nodes, use their topological position
        let mut queue: VecDeque<usize> = VecDeque::new();
        let mut working_indegree = indegree.clone();

        for &idx in &global_nodes {
            if working_indegree[idx] == 0 {
                queue.push_back(idx);
            }
        }

        while let Some(u) = queue.pop_front() {
            if graph.node_subgraph.get(&graph.nodes[u].id).is_some() {
                continue; // Skip subgraph nodes
            }
            let current_rank = local_ranks[u];
            for &v in &adj[u] {
                if graph.node_subgraph.get(&graph.nodes[v].id).is_some() {
                    continue; // Don't propagate into subgraphs
                }
                working_indegree[v] = working_indegree[v].saturating_sub(1);
                local_ranks[v] = local_ranks[v].max(current_rank + 1);
                if working_indegree[v] == 0 {
                    queue.push_back(v);
                }
            }
        }
    }

    // Step 3: Calculate final ranks by adding subgraph base offset
    // Subgraph groups are separated by computing:
    // - Max local rank in each subgraph
    // - Cumulative offset for each subgraph based on flow order

    // Get max local rank per subgraph
    let mut sg_max_local_rank: HashMap<String, usize> = HashMap::new();
    for (idx, node) in graph.nodes.iter().enumerate() {
        if let Some(sg_id) = graph.node_subgraph.get(&node.id) {
            let current_max = sg_max_local_rank.entry(sg_id.clone()).or_insert(0);
            *current_max = (*current_max).max(local_ranks[idx]);
        }
    }

    // Calculate cumulative rank offset for each subgraph based on flow order
    let mut sg_rank_offset: HashMap<String, usize> = HashMap::new();
    let mut subgraphs_by_base: Vec<(String, usize)> = subgraph_base_rank.into_iter().collect();
    subgraphs_by_base.sort_by_key(|(id, base)| (*base, id.clone()));

    let mut cumulative_offset = 0usize;
    let mut prev_base = 0usize;
    let mut prev_level_max_rank = 0usize;
    for (sg_id, base) in &subgraphs_by_base {
        if *base > prev_base {
            // Moving to a new flow level - add the max rank from previous level + 1
            cumulative_offset += prev_level_max_rank + 1;
            prev_level_max_rank = 0;
        }
        // Track max local rank at this level
        let my_max = sg_max_local_rank.get(sg_id).copied().unwrap_or(0);
        prev_level_max_rank = prev_level_max_rank.max(my_max);
        sg_rank_offset.insert(sg_id.clone(), cumulative_offset);
        prev_base = *base;
    }

    // Calculate the max local rank of global nodes (nodes not in any subgraph)
    // This determines where subgraphs should start
    let global_node_indices: Vec<usize> = graph.nodes.iter()
        .enumerate()
        .filter(|(_, n)| graph.node_subgraph.get(&n.id).is_none())
        .map(|(idx, _)| idx)
        .collect();

    let max_global_local_rank = global_node_indices.iter()
        .map(|&idx| local_ranks[idx])
        .max()
        .unwrap_or(0);

    // If there are global nodes, subgraphs need to start after them
    let subgraph_base_offset = if !global_node_indices.is_empty() {
        // Subgraphs start after all global nodes
        max_global_local_rank + 1
    } else {
        0
    };

    // Calculate final ranks
    let mut ranks = vec![0usize; graph.nodes.len()];
    let mut node_ranks: HashMap<String, usize> = HashMap::new();

    for (idx, node) in graph.nodes.iter().enumerate() {
        let base_offset = if let Some(sg_id) = graph.node_subgraph.get(&node.id) {
            // Subgraph nodes: start after global nodes + cumulative subgraph offset
            subgraph_base_offset + sg_rank_offset.get(sg_id).copied().unwrap_or(0)
        } else {
            // Global nodes: use their local rank directly
            0
        };
        ranks[idx] = base_offset + local_ranks[idx];
        node_ranks.insert(node.id.clone(), ranks[idx]);
    }

    // Group nodes by final rank
    let max_rank = *ranks.iter().max().unwrap_or(&0);
    let mut by_rank: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
    for (idx, &rank) in ranks.iter().enumerate() {
        by_rank[rank].push(idx);
    }

    Ok((node_ranks, by_rank))
}

/// Calculate base rank for each subgraph based on flow dependencies
/// Returns a map of subgraph_id -> base_rank where downstream subgraphs have higher ranks
fn calculate_subgraph_base_ranks(graph: &Graph) -> HashMap<String, usize> {
    let flow_graph = build_subgraph_flow_graph(graph);

    // Build subgraph-level adjacency and indegree
    let subgraph_ids: Vec<String> = graph.subgraphs.iter().map(|s| s.id.clone()).collect();
    let mut sg_indegree: HashMap<String, usize> = subgraph_ids.iter().map(|id| (id.clone(), 0)).collect();

    // Count incoming edges to each subgraph
    for (from_sg, to_sgs) in &flow_graph {
        for to_sg in to_sgs {
            if let Some(count) = sg_indegree.get_mut(to_sg) {
                *count += 1;
            }
        }
    }

    // Topological sort on subgraphs
    let mut queue: VecDeque<String> = sg_indegree.iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(id, _)| id.clone())
        .collect();

    let mut sg_ranks: HashMap<String, usize> = HashMap::new();

    // Initialize root subgraphs at rank 0
    for sg_id in &queue {
        sg_ranks.insert(sg_id.clone(), 0);
    }

    while let Some(current) = queue.pop_front() {
        let current_rank = sg_ranks.get(&current).copied().unwrap_or(0);

        if let Some(targets) = flow_graph.get(&current) {
            for target in targets {
                // Assign max of current rank + 1 or existing rank
                let new_rank = current_rank + 1;
                let existing = sg_ranks.entry(target.clone()).or_insert(0);
                *existing = (*existing).max(new_rank);

                if let Some(count) = sg_indegree.get_mut(target) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        queue.push_back(target.clone());
                    }
                }
            }
        }
    }

    // Any subgraphs not visited (disconnected) get rank 0
    for sg_id in &subgraph_ids {
        sg_ranks.entry(sg_id.clone()).or_insert(0);
    }

    sg_ranks
}

/// Calculate spacing for a rank based on edge complexity
fn calculate_rank_spacing(graph: &Graph, rank: usize) -> usize {
    // Check if this rank has multi-target edges
    let mut has_multi_target = false;
    
    for edge in &graph.edges {
        if edge.is_back_edge {
            continue;
        }
        
        if let Some(from_node) = graph.get_node(&edge.from) {
            if from_node.rank == rank {
                let target_count = graph.edges.iter()
                    .filter(|e| !e.is_back_edge && e.from == edge.from)
                    .count();
                if target_count > 1 {
                    has_multi_target = true;
                    break;
                }
            }
        }
    }
    
    if has_multi_target {
        ROW_SPACING_MULTI
    } else {
        ROW_SPACING_SINGLE
    }
}

/// Center the entire layout
fn center_layout(graph: &mut Graph) {
    if graph.nodes.is_empty() {
        return;
    }

    // Find the minimum x position (considering both nodes and subgraph bounds)
    let node_min_x = graph.nodes.iter().map(|n| n.x).min().unwrap_or(0);
    let subgraph_min_x = graph.subgraphs.iter().map(|s| s.bounds.x).min().unwrap_or(usize::MAX);
    let min_x = node_min_x.min(subgraph_min_x);

    // Shift all nodes and subgraph bounds so the leftmost is at x=0
    if min_x > 0 {
        for node in &mut graph.nodes {
            node.x = node.x.saturating_sub(min_x);
        }
        for subgraph in &mut graph.subgraphs {
            subgraph.bounds.x = subgraph.bounds.x.saturating_sub(min_x);
        }
    }
}

/// Calculate bounding boxes for subgraphs based on their contained nodes.
///
/// Padding values match the node box spacing (BOX_PADDING, ROW_SPACING, COL_SPACING)
/// to create visual consistency between nodes and their containing subgraphs.
fn calculate_subgraph_bounds(graph: &mut Graph) {
    // Padding matches node box spacing for visual consistency
    const SUBGRAPH_PADDING_H: usize = 2; // Match BOX_PADDING (horizontal each side)
    const SUBGRAPH_PADDING_V: usize = 1; // Vertical padding below content
    const TITLE_HEIGHT: usize = 1; // Just the title row itself
    const TITLE_PADDING: usize = 1; // Empty row between title and content
    const ENTRY_ARROW_SPACE: usize = 1; // Arrow appears ON border (no extra row)

    for subgraph in &mut graph.subgraphs {
        if subgraph.node_ids.is_empty() {
            // Empty subgraph: set minimal bounds to avoid rendering issues
            subgraph.bounds = Rectangle {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            };
            subgraph.rank_range = (0, 0);
            continue;
        }

        let mut min_x = usize::MAX;
        let mut max_x = 0;
        let mut min_y = usize::MAX;
        let mut max_y = 0;
        let mut min_rank = usize::MAX;
        let mut max_rank = 0;

        // Find bounds of all nodes in this subgraph
        for node in &graph.nodes {
            if subgraph.node_ids.contains(&node.id) {
                min_x = min_x.min(node.x);
                max_x = max_x.max(node.x + node.width);
                min_y = min_y.min(node.y);
                max_y = max_y.max(node.y + BOX_HEIGHT);
                min_rank = min_rank.min(node.rank);
                max_rank = max_rank.max(node.rank);
            }
        }

        // Only set bounds if we found nodes
        if min_x != usize::MAX {
            let content_width = max_x.saturating_sub(min_x);
            let content_height = max_y.saturating_sub(min_y);

            // Title takes extra vertical space if present
            // Layout: border(row 0) → title(row 1) → padding(row 2) → content(row 3)
            // Entry arrows appear ON the border row (row 0)
            let title_space = if subgraph.title.is_some() {
                TITLE_HEIGHT + TITLE_PADDING + ENTRY_ARROW_SPACE // title + padding + border
            } else {
                1 // Just the border row (arrow is on the border)
            };

            // Calculate minimum width needed for title (if present)
            // Title needs: 2 border chars + 2 padding chars + title text
            let title_width = if let Some(ref title) = subgraph.title {
                crate::style::display_width(title) + 4 // 2 for borders, 2 for padding
            } else {
                0
            };

            // Width is MAX of content width and title width
            let total_width = (content_width + 2 * SUBGRAPH_PADDING_H).max(title_width);

            subgraph.bounds = Rectangle {
                x: min_x.saturating_sub(SUBGRAPH_PADDING_H),
                y: min_y.saturating_sub(title_space),
                width: total_width,
                height: content_height + title_space + SUBGRAPH_PADDING_V,
            };
            subgraph.rank_range = (min_rank, max_rank);
        } else {
            // No nodes found, set empty bounds
            subgraph.bounds = Rectangle::default();
            subgraph.rank_range = (0, 0);
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
            "tests/fixtures/inputs/chain.md",
            "tests/fixtures/inputs/database_nodes.md",
            "tests/fixtures/inputs/forward_ref.md",
            "tests/fixtures/inputs/simple.md",
            "tests/fixtures/inputs/with_config.md",
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
    fn test_subgraph_fixtures_anchor_left() {
        // Subgraph fixtures: the leftmost element is the subgraph border, not necessarily a node
        let fixtures = [
            "tests/fixtures/inputs/unsupported.md",
            "tests/fixtures/inputs/subgraph_basic.md",
            "tests/fixtures/inputs/subgraph_cross_edges.md",
        ];

        for path in fixtures {
            let input = std::fs::read_to_string(path).expect("read fixture");
            let parsed = crate::parser::parse_with_config(&input, false, true).expect("parse fixture");
            let graph = waterfall(parsed.graph).expect("layout fixture");

            // For subgraph layouts, check that the leftmost element (node or subgraph) is at x=0
            let node_min_x = graph.nodes.iter().map(|n| n.x).min().unwrap_or(usize::MAX);
            let sg_min_x = graph.subgraphs.iter()
                .filter(|s| s.bounds.width > 0)
                .map(|s| s.bounds.x)
                .min()
                .unwrap_or(usize::MAX);
            let overall_min = node_min_x.min(sg_min_x);

            assert_eq!(
                overall_min, 0,
                "expected left anchor at x=0 for subgraph fixture {} (got {})",
                path, overall_min
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

    // ========================================================================
    // Subgraph Layout Tests
    // ========================================================================

    #[test]
    fn test_subgraph_bounds_calculated() {
        use crate::graph::Subgraph;

        let mut graph = Graph::new();
        graph.add_node(Node::new("A", "API"));
        graph.add_node(Node::new("B", "Cache"));
        graph.add_edge(crate::graph::Edge::new("A", "B"));

        // Create subgraph containing both nodes
        let mut sg = Subgraph::new("backend", Some("Backend".to_string()));
        sg.node_ids.insert("A".to_string());
        sg.node_ids.insert("B".to_string());
        graph.subgraphs.push(sg);
        graph.node_subgraph.insert("A".to_string(), "backend".to_string());
        graph.node_subgraph.insert("B".to_string(), "backend".to_string());

        let result = waterfall(graph).unwrap();

        // Subgraph bounds should be calculated
        assert_eq!(result.subgraphs.len(), 1);
        let bounds = &result.subgraphs[0].bounds;
        assert!(bounds.width > 0, "subgraph should have positive width");
        assert!(bounds.height > 0, "subgraph should have positive height");

        // Bounds should contain all nodes
        let a = result.get_node("A").unwrap();
        let b = result.get_node("B").unwrap();
        assert!(a.x >= bounds.x, "node A should be within subgraph x bounds");
        assert!(b.x >= bounds.x, "node B should be within subgraph x bounds");
        assert!(a.y >= bounds.y, "node A should be within subgraph y bounds");
        assert!(b.y >= bounds.y, "node B should be within subgraph y bounds");
    }

    #[test]
    fn test_subgraph_nodes_have_padding() {
        use crate::graph::Subgraph;

        let mut graph = Graph::new();
        graph.add_node(Node::new("A", "Node"));

        let mut sg = Subgraph::new("group", None);
        sg.node_ids.insert("A".to_string());
        graph.subgraphs.push(sg);
        graph.node_subgraph.insert("A".to_string(), "group".to_string());

        let result = waterfall(graph).unwrap();

        // Node should have padding from subgraph border
        let bounds = &result.subgraphs[0].bounds;
        let node = result.get_node("A").unwrap();

        // Node x should be > subgraph x (has left padding)
        assert!(
            node.x > bounds.x,
            "node should have left padding from subgraph border"
        );
    }

    #[test]
    fn test_subgraph_title_affects_width() {
        use crate::graph::Subgraph;

        let mut graph = Graph::new();
        graph.add_node(Node::new("A", "X")); // Very short label

        let mut sg = Subgraph::new("group", Some("This Is A Long Subgraph Title".to_string()));
        sg.node_ids.insert("A".to_string());
        graph.subgraphs.push(sg);
        graph.node_subgraph.insert("A".to_string(), "group".to_string());

        let result = waterfall(graph).unwrap();

        let bounds = &result.subgraphs[0].bounds;
        let node = result.get_node("A").unwrap();

        // Subgraph width should accommodate title (wider than just node)
        assert!(
            bounds.width > node.width + 4,
            "subgraph should be wide enough for title"
        );
    }

    #[test]
    fn test_multiple_subgraphs_dont_overlap() {
        use crate::graph::Subgraph;

        let mut graph = Graph::new();
        graph.add_node(Node::new("A", "Node A"));
        graph.add_node(Node::new("B", "Node B"));

        // Two separate subgraphs at same rank
        let mut sg1 = Subgraph::new("group1", None);
        sg1.node_ids.insert("A".to_string());
        graph.subgraphs.push(sg1);
        graph.node_subgraph.insert("A".to_string(), "group1".to_string());

        let mut sg2 = Subgraph::new("group2", None);
        sg2.node_ids.insert("B".to_string());
        graph.subgraphs.push(sg2);
        graph.node_subgraph.insert("B".to_string(), "group2".to_string());

        let result = waterfall(graph).unwrap();

        let bounds1 = &result.subgraphs[0].bounds;
        let bounds2 = &result.subgraphs[1].bounds;

        // Subgraphs should not overlap horizontally
        let sg1_right = bounds1.x + bounds1.width;
        let sg2_right = bounds2.x + bounds2.width;

        let no_overlap = sg1_right <= bounds2.x || sg2_right <= bounds1.x;
        assert!(no_overlap, "subgraphs should not overlap");
    }

    #[test]
    fn test_empty_subgraph_has_zero_bounds() {
        use crate::graph::Subgraph;

        let mut graph = Graph::new();
        graph.add_node(Node::new("A", "Outside"));

        // Empty subgraph (no nodes)
        let sg = Subgraph::new("empty", None);
        graph.subgraphs.push(sg);

        let result = waterfall(graph).unwrap();

        let bounds = &result.subgraphs[0].bounds;
        assert_eq!(bounds.width, 0, "empty subgraph should have zero width");
        assert_eq!(bounds.height, 0, "empty subgraph should have zero height");
    }

    #[test]
    fn test_cross_boundary_edge_layout() {
        use crate::graph::Subgraph;

        let mut graph = Graph::new();
        graph.add_node(Node::new("X", "External"));
        graph.add_node(Node::new("A", "Internal"));
        graph.add_edge(crate::graph::Edge::new("X", "A"));

        // Only A is in subgraph
        let mut sg = Subgraph::new("group", None);
        sg.node_ids.insert("A".to_string());
        graph.subgraphs.push(sg);
        graph.node_subgraph.insert("A".to_string(), "group".to_string());

        let result = waterfall(graph).unwrap();

        let external = result.get_node("X").unwrap();
        let internal = result.get_node("A").unwrap();

        // External node should be at rank 0, internal at rank 1
        assert_eq!(external.rank, 0, "external should be at rank 0");
        assert_eq!(internal.rank, 1, "internal should be at rank 1");

        // Internal node should be below external (TD layout)
        assert!(internal.y > external.y, "internal should be below external");
    }

    #[test]
    fn test_subgraph_fixture_layout() {
        let fixtures = [
            "tests/fixtures/inputs/subgraph_basic.md",
            "tests/fixtures/inputs/subgraph_cross_edges.md",
        ];

        for path in fixtures {
            let input = std::fs::read_to_string(path).expect("read fixture");
            let parsed = crate::parser::parse_with_config(&input, false, true).expect("parse fixture");
            let graph = waterfall(parsed.graph).expect("layout fixture");

            // Verify subgraphs have valid bounds
            for sg in &graph.subgraphs {
                if !sg.node_ids.is_empty() {
                    assert!(
                        sg.bounds.width > 0 && sg.bounds.height > 0,
                        "subgraph {} should have positive bounds in {}",
                        sg.id,
                        path
                    );
                }
            }

            // Verify nodes are positioned correctly
            let min_x = graph.nodes.iter().map(|n| n.x).min().unwrap_or(0);
            let min_sg_x = graph.subgraphs.iter().map(|s| s.bounds.x).min().unwrap_or(0);
            let overall_min = min_x.min(min_sg_x);
            assert_eq!(
                overall_min, 0,
                "layout should be anchored at x=0 for {}",
                path
            );
        }
    }
}
