//! Waterfall layout algorithm
//!
//! Implements simplified Sugiyama-style layout:
//! 1. Rank assignment via BFS from roots
//! 2. X-coordinate assignment per rank
//! 3. Y-coordinate from rank
//!
//! See SPEC §2.6 for algorithm details

use anyhow::Result;
use crate::graph::Graph;

/// Apply waterfall layout to position all nodes
///
/// # Algorithm
/// 1. Find roots (nodes with no incoming edges)
/// 2. Assign ranks via BFS
/// 3. Detect cycles, mark back-edges
/// 4. Assign X coordinates per rank
/// 5. Assign Y coordinates from rank
pub fn waterfall(mut graph: Graph) -> Result<Graph> {
    // TODO: Implement full layout (Day 2)

    if graph.nodes.is_empty() {
        return Ok(graph);
    }

    // Placeholder: simple sequential layout
    for (i, node) in graph.nodes.iter_mut().enumerate() {
        // Calculate node width from label
        node.width = crate::style::box_width(&node.label);
        node.rank = i;
        node.x = 2; // Left margin
        node.y = i * 5; // BOX_HEIGHT + ROW_SPACING
    }

    Ok(graph)
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
        assert_eq!(result.nodes[0].x, 2); // Left margin
    }
}
