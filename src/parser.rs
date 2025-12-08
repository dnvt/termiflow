//! Mermaid-Lite parser with two-pass parsing
//!
//! See SPEC §1 for supported syntax and error handling

use anyhow::Result;
use crate::graph::{Graph, Direction};

/// Parse Mermaid content into a Graph
///
/// Uses two-pass parsing:
/// - Pass 1: Collect all node identifiers
/// - Pass 2: Build graph with validation
///
/// # Arguments
/// * `input` - Mermaid flowchart content
/// * `strict` - If true, exit on any warning
pub fn parse(input: &str, strict: bool) -> Result<Graph> {
    let _ = strict; // TODO: implement strict mode

    // TODO: Implement full parser (Day 1)
    // For now, return a placeholder graph

    if input.trim().is_empty() {
        anyhow::bail!("termiflow: error: Empty file (no nodes)");
    }

    // Check for graph direction
    if !input.contains("graph ") {
        anyhow::bail!("termiflow: error: No graph direction found (expected 'graph TD/LR/TB/BT')");
    }

    let mut graph = Graph::new();
    graph.direction = parse_direction(input)?;

    // Placeholder: return empty graph for now
    Ok(graph)
}

/// Parse the graph direction from input
fn parse_direction(input: &str) -> Result<Direction> {
    // Simple regex-free check for now
    if input.contains("graph LR") {
        Ok(Direction::LR)
    } else if input.contains("graph BT") {
        Ok(Direction::BT)
    } else if input.contains("graph TB") || input.contains("graph TD") {
        Ok(Direction::TD)
    } else {
        anyhow::bail!("termiflow: error: Invalid direction (expected TD/LR/TB/BT)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_input_fails() {
        let result = parse("", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_no_direction_fails() {
        let result = parse("A[Node]", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_direction_parsing() {
        assert!(matches!(parse_direction("graph TD"), Ok(Direction::TD)));
        assert!(matches!(parse_direction("graph LR"), Ok(Direction::LR)));
        assert!(matches!(parse_direction("graph TB"), Ok(Direction::TD)));
        assert!(matches!(parse_direction("graph BT"), Ok(Direction::BT)));
    }
}
