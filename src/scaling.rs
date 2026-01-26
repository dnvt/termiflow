//! Adaptive Grid Scaling
//!
//! This module provides intelligent scaling of diagram layouts based on
//! diagram complexity and available terminal space. Instead of using fixed
//! cell dimensions, the scaling system:
//!
//! 1. Analyzes diagram metrics (node count, edge density, label widths)
//! 2. Considers terminal width constraints
//! 3. Computes appropriate spacing configuration
//!
//! # Complexity Formula
//!
//! ```text
//! complexity = (node_count * 0.5 + edge_count * 0.3 + max_depth * 0.2) / 10
//! ```
//!
//! - Complexity < 2.0 → spacious (more breathing room)
//! - Complexity 2.0-4.0 → default (standard layout)
//! - Complexity > 4.0 → compact (fit more in less space)

use crate::graph::Graph;
use crate::spacing::{SpacingConfig, SpacingMode};

/// Scaling mode configuration
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ScalingMode {
    /// Automatic scaling based on diagram complexity and terminal width
    #[default]
    Auto,
    /// Fixed scaling (use default spacing regardless of diagram size)
    Fixed,
}

impl std::str::FromStr for ScalingMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" | "automatic" | "adaptive" => Ok(ScalingMode::Auto),
            "fixed" | "static" | "manual" => Ok(ScalingMode::Fixed),
            _ => Err(()),
        }
    }
}

/// Metrics computed from a diagram for scaling decisions
#[derive(Debug, Clone)]
pub struct DiagramMetrics {
    /// Total number of nodes
    pub node_count: usize,
    /// Total number of edges (excluding back-edges)
    pub edge_count: usize,
    /// Maximum display width of any node label
    pub max_label_width: usize,
    /// Maximum depth (number of layers/ranks)
    pub max_depth: usize,
    /// Maximum number of nodes in any single layer
    pub max_layer_width: usize,
    /// Computed complexity score (0.0 = trivial, higher = more complex)
    pub complexity_score: f32,
}

impl DiagramMetrics {
    /// Compute metrics from a parsed graph
    pub fn from_graph(graph: &Graph) -> Self {
        let node_count = graph.nodes.len();
        let edge_count = graph.edges.iter().filter(|e| !e.is_back_edge).count();

        let max_label_width = graph
            .nodes
            .iter()
            .map(|n| unicode_width::UnicodeWidthStr::width(n.label.as_str()))
            .max()
            .unwrap_or(0);

        // Calculate max_depth and layer widths
        let max_depth = if graph.nodes.is_empty() {
            0
        } else {
            graph.nodes.iter().map(|n| n.rank).max().unwrap_or(0) + 1
        };

        // Count nodes per rank to find max layer width
        let mut rank_counts: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();
        for node in &graph.nodes {
            *rank_counts.entry(node.rank).or_insert(0) += 1;
        }
        let max_layer_width = rank_counts.values().max().copied().unwrap_or(0);

        // Compute complexity score
        let complexity_score = Self::compute_complexity(
            node_count,
            edge_count,
            max_depth,
            max_layer_width,
        );

        Self {
            node_count,
            edge_count,
            max_label_width,
            max_depth,
            max_layer_width,
            complexity_score,
        }
    }

    /// Compute the complexity score from individual metrics
    fn compute_complexity(
        node_count: usize,
        edge_count: usize,
        max_depth: usize,
        max_layer_width: usize,
    ) -> f32 {
        // Weighted formula considering different aspects of complexity
        let node_factor = node_count as f32 * 0.4;
        let edge_factor = edge_count as f32 * 0.3;
        let depth_factor = max_depth as f32 * 0.15;
        let width_factor = max_layer_width as f32 * 0.15;

        (node_factor + edge_factor + depth_factor + width_factor) / 10.0
    }

    /// Determine the recommended spacing mode based on complexity
    pub fn recommended_spacing_mode(&self) -> SpacingMode {
        if self.complexity_score < 2.0 {
            SpacingMode::Spacious
        } else if self.complexity_score <= 4.0 {
            SpacingMode::Default
        } else {
            SpacingMode::Compact
        }
    }

    /// Check if the diagram is considered "dense" (many nodes/edges for its depth)
    pub fn is_dense(&self) -> bool {
        if self.max_depth == 0 {
            return false;
        }
        let avg_nodes_per_layer = self.node_count as f32 / self.max_depth as f32;
        avg_nodes_per_layer > 4.0
    }
}

/// Budget constraints for canvas dimensions
#[derive(Debug, Clone)]
pub struct CanvasBudget {
    /// Maximum allowed width in characters
    pub max_width: usize,
    /// Target width (e.g., from terminal) - may be smaller than max
    pub target_width: Option<usize>,
    /// Maximum allowed height in rows
    pub max_height: usize,
    /// Target height (e.g., from terminal rows) - may be smaller than max
    pub target_height: Option<usize>,
}

impl Default for CanvasBudget {
    fn default() -> Self {
        Self {
            max_width: 500,
            target_width: None,
            max_height: 200,
            target_height: None,
        }
    }
}

impl CanvasBudget {
    /// Create a budget based on terminal dimensions (if available)
    pub fn from_terminal() -> Self {
        let mut budget = Self::default();

        // Try to detect terminal width
        if let Ok(term_width) = std::env::var("COLUMNS") {
            if let Ok(w) = term_width.parse::<usize>() {
                budget.target_width = Some(w.saturating_sub(2)); // Leave margin
            }
        }

        // Try to detect terminal height
        if let Ok(term_height) = std::env::var("LINES") {
            if let Ok(h) = term_height.parse::<usize>() {
                budget.target_height = Some(h.saturating_sub(2)); // Leave margin
            }
        }

        budget
    }

    /// Create a budget with explicit dimensions
    pub fn with_dimensions(width: usize, height: usize) -> Self {
        Self {
            max_width: width,
            target_width: Some(width),
            max_height: height,
            target_height: Some(height),
        }
    }

    /// Compute appropriate spacing configuration based on metrics and budget
    pub fn compute_spacing(&self, metrics: &DiagramMetrics) -> SpacingConfig {
        // Start with recommended spacing based on complexity
        let base_mode = metrics.recommended_spacing_mode();
        let mut config = SpacingConfig::from_mode(base_mode);

        // Adjust based on target width constraints
        if let Some(target_width) = self.target_width {
            if metrics.max_layer_width > 0 && metrics.max_label_width > 0 {
                // Estimate required width with current spacing
                let estimated_width = estimate_canvas_width(metrics, &config);

                if estimated_width > target_width {
                    // Need to compress - try compact spacing first
                    let compact = SpacingConfig::compact();
                    let compact_width = estimate_canvas_width(metrics, &compact);

                    if compact_width <= target_width {
                        config = compact;
                    } else {
                        // Even compact is too wide - reduce label width
                        config = compact;
                        let excess = compact_width - target_width;
                        let label_reduction = excess / metrics.max_layer_width.max(1);
                        config.max_label_width = config
                            .max_label_width
                            .saturating_sub(label_reduction)
                            .max(8); // Minimum 8 chars for readability
                    }
                } else if estimated_width < target_width / 2 && metrics.complexity_score < 2.0 {
                    // Diagram is small relative to target - use spacious
                    config = SpacingConfig::spacious();
                }
            }
        }

        config
    }

    /// Get the effective width constraint
    pub fn effective_width(&self) -> usize {
        self.target_width.unwrap_or(self.max_width).min(self.max_width)
    }

    /// Get the effective height constraint
    pub fn effective_height(&self) -> usize {
        self.target_height.unwrap_or(self.max_height).min(self.max_height)
    }
}

/// Estimate canvas width for given metrics and spacing
fn estimate_canvas_width(metrics: &DiagramMetrics, spacing: &SpacingConfig) -> usize {
    if metrics.max_layer_width == 0 {
        return 0;
    }

    let avg_label_width = metrics.max_label_width.min(spacing.max_label_width);
    let box_width = avg_label_width + spacing.box_padding * 2 + 2;
    let node_width = box_width.max(spacing.box_min_width);

    metrics.max_layer_width * node_width
        + (metrics.max_layer_width.saturating_sub(1)) * spacing.col_spacing
        + spacing.col_spacing * 2 // margins
}

/// Estimate canvas height for given metrics and spacing
#[allow(dead_code)]
fn estimate_canvas_height(metrics: &DiagramMetrics, spacing: &SpacingConfig) -> usize {
    if metrics.max_depth == 0 {
        return 0;
    }

    metrics.max_depth * spacing.box_height
        + (metrics.max_depth.saturating_sub(1)) * spacing.row_spacing
        + spacing.row_spacing * 2 // margins
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, Node};

    fn make_test_graph(node_count: usize, depth: usize) -> Graph {
        let mut graph = Graph::new();
        let nodes_per_layer = node_count / depth.max(1);

        for i in 0..node_count {
            let label = format!("Node{}", i);
            let mut node = Node::new(&format!("n{}", i), &label);
            node.rank = i / nodes_per_layer.max(1);
            graph.nodes.push(node);
        }

        // Add some edges
        for i in 0..node_count.saturating_sub(1) {
            if i + nodes_per_layer.max(1) < node_count {
                graph
                    .edges
                    .push(Edge::new(&format!("n{}", i), &format!("n{}", i + nodes_per_layer.max(1))));
            }
        }

        graph
    }

    #[test]
    fn test_metrics_from_empty_graph() {
        let graph = Graph::new();
        let metrics = DiagramMetrics::from_graph(&graph);

        assert_eq!(metrics.node_count, 0);
        assert_eq!(metrics.edge_count, 0);
        assert_eq!(metrics.max_depth, 0);
        assert_eq!(metrics.complexity_score, 0.0);
    }

    #[test]
    fn test_metrics_from_simple_graph() {
        let graph = make_test_graph(4, 2);
        let metrics = DiagramMetrics::from_graph(&graph);

        assert_eq!(metrics.node_count, 4);
        assert!(metrics.edge_count > 0);
        assert!(metrics.max_depth >= 2);
    }

    #[test]
    fn test_complexity_scaling() {
        let small = make_test_graph(5, 2);
        let medium = make_test_graph(20, 4);
        let large = make_test_graph(50, 5);

        let small_metrics = DiagramMetrics::from_graph(&small);
        let medium_metrics = DiagramMetrics::from_graph(&medium);
        let large_metrics = DiagramMetrics::from_graph(&large);

        assert!(small_metrics.complexity_score < medium_metrics.complexity_score);
        assert!(medium_metrics.complexity_score < large_metrics.complexity_score);
    }

    #[test]
    fn test_recommended_spacing() {
        let small = make_test_graph(3, 2);
        let large = make_test_graph(100, 10);

        let small_metrics = DiagramMetrics::from_graph(&small);
        let large_metrics = DiagramMetrics::from_graph(&large);

        // Small diagram should recommend spacious or default
        let small_mode = small_metrics.recommended_spacing_mode();
        assert!(matches!(
            small_mode,
            SpacingMode::Spacious | SpacingMode::Default
        ));

        // Large diagram should recommend compact
        assert_eq!(
            large_metrics.recommended_spacing_mode(),
            SpacingMode::Compact
        );
    }

    #[test]
    fn test_canvas_budget_default() {
        let budget = CanvasBudget::default();
        assert_eq!(budget.max_width, 500);
        assert_eq!(budget.max_height, 200);
        assert!(budget.target_width.is_none());
    }

    #[test]
    fn test_canvas_budget_with_dimensions() {
        let budget = CanvasBudget::with_dimensions(80, 24);
        assert_eq!(budget.effective_width(), 80);
        assert_eq!(budget.effective_height(), 24);
    }

    #[test]
    fn test_compute_spacing_for_wide_diagram() {
        let mut graph = Graph::new();
        // Create a wide diagram (10 nodes in one layer)
        for i in 0..10 {
            let mut node = Node::new(&format!("n{}", i), "LongLabel123");
            node.rank = 0;
            graph.nodes.push(node);
        }

        let metrics = DiagramMetrics::from_graph(&graph);
        let budget = CanvasBudget::with_dimensions(80, 24);
        let spacing = budget.compute_spacing(&metrics);

        // Should use compact spacing to fit
        assert!(spacing.col_spacing <= SpacingConfig::default().col_spacing);
    }

    #[test]
    fn test_scaling_mode_parse() {
        assert_eq!("auto".parse(), Ok(ScalingMode::Auto));
        assert_eq!("automatic".parse(), Ok(ScalingMode::Auto));
        assert_eq!("fixed".parse(), Ok(ScalingMode::Fixed));
        assert_eq!("static".parse(), Ok(ScalingMode::Fixed));
        assert!("invalid".parse::<ScalingMode>().is_err());
    }

    #[test]
    fn test_width_estimation() {
        let graph = make_test_graph(6, 3);
        let metrics = DiagramMetrics::from_graph(&graph);
        let spacing = SpacingConfig::default();

        let width = estimate_canvas_width(&metrics, &spacing);
        assert!(width > 0);
    }

    #[test]
    fn test_is_dense() {
        // Sparse: 4 nodes across 4 layers = 1 per layer
        let sparse = make_test_graph(4, 4);
        let sparse_metrics = DiagramMetrics::from_graph(&sparse);
        assert!(!sparse_metrics.is_dense());

        // Dense: 20 nodes across 2 layers = 10 per layer
        let dense = make_test_graph(20, 2);
        let dense_metrics = DiagramMetrics::from_graph(&dense);
        assert!(dense_metrics.is_dense());
    }
}
