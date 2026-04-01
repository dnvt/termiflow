//! Edge Crossing Minimization
//!
//! This module provides adaptive layer-sweep algorithms to minimize edge crossings
//! in layered graph layouts. The implementation improves upon the fixed 4-pass
//! barycenter heuristic with:
//!
//! - Convergence detection (stops early when improvement plateaus)
//! - Crossing count evaluation (tracks actual crossing reduction)
//! - Median heuristic (less outlier-sensitive than barycenter)
//! - Configurable pass limits and thresholds
//!
//! # Algorithm
//!
//! The crossing minimizer uses layer-by-layer sweeps to reorder nodes within each
//! layer based on the positions of connected nodes in adjacent layers. Two heuristics
//! are supported:
//!
//! - **Barycenter**: Average position of neighbors (original method)
//! - **Median**: Median position of neighbors (more robust to outliers)

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::graph::Graph;

/// Configuration for crossing minimization
#[derive(Debug, Clone)]
pub struct CrossingConfig {
    /// Maximum number of sweep passes (default: 10)
    pub max_passes: usize,
    /// Stop when improvement falls below this threshold (default: 0.05 = 5%)
    pub convergence_threshold: f32,
    /// Which heuristic to use for layer ordering
    pub heuristic: Heuristic,
}

impl Default for CrossingConfig {
    fn default() -> Self {
        Self {
            max_passes: 10,
            convergence_threshold: 0.05,
            heuristic: Heuristic::Median,
        }
    }
}

/// Heuristic for calculating node positions during layer sweep
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Heuristic {
    /// Use average (mean) position of connected neighbors
    Barycenter,
    /// Use median position of connected neighbors (more robust to outliers)
    Median,
}

/// Edge crossing minimizer with adaptive convergence detection
pub struct CrossingMinimizer {
    config: CrossingConfig,
}

impl CrossingMinimizer {
    /// Create a new crossing minimizer with default configuration
    pub fn new() -> Self {
        Self {
            config: CrossingConfig::default(),
        }
    }

    /// Create a crossing minimizer with custom configuration
    pub fn with_config(config: CrossingConfig) -> Self {
        Self { config }
    }

    /// Minimize edge crossings by reordering nodes within layers.
    ///
    /// This method modifies `layers` in place and returns the final crossing count.
    ///
    /// # Arguments
    /// * `graph` - The graph being laid out
    /// * `layers` - Mutable reference to layer assignments (Vec<Vec<usize>> where each inner vec
    ///   contains node indices for that layer)
    ///
    /// # Returns
    /// The final number of edge crossings after minimization
    pub fn minimize(&self, graph: &Graph, layers: &mut [Vec<usize>]) -> usize {
        if layers.len() <= 1 {
            return 0;
        }

        let mut prev_crossings = self.count_crossings(graph, layers);
        let initial_crossings = prev_crossings;

        for pass in 0..self.config.max_passes {
            // Down sweep (layer 0 is fixed, reorder 1..n based on layer above)
            for i in 1..layers.len() {
                self.sort_layer(graph, layers, i, i - 1);
            }

            // Up sweep (last layer is fixed, reorder n-1..0 based on layer below)
            for i in (0..layers.len().saturating_sub(1)).rev() {
                self.sort_layer(graph, layers, i, i + 1);
            }

            let current_crossings = self.count_crossings(graph, layers);

            // Check for convergence
            if prev_crossings > 0 {
                let improvement =
                    (prev_crossings as f32 - current_crossings as f32) / prev_crossings as f32;
                if improvement < self.config.convergence_threshold {
                    if std::env::var("TERMIFLOW_DEBUG_CROSSING").is_ok() {
                        eprintln!(
                            "crossing: converged at pass {} ({} -> {} crossings, {:.1}% improvement)",
                            pass + 1,
                            initial_crossings,
                            current_crossings,
                            (1.0 - (current_crossings as f32 / initial_crossings as f32)) * 100.0
                        );
                    }
                    return current_crossings;
                }
            }

            if current_crossings == 0 {
                if std::env::var("TERMIFLOW_DEBUG_CROSSING").is_ok() {
                    eprintln!("crossing: achieved zero crossings at pass {}", pass + 1);
                }
                return 0;
            }

            prev_crossings = current_crossings;
        }

        if std::env::var("TERMIFLOW_DEBUG_CROSSING").is_ok() {
            let final_crossings = self.count_crossings(graph, layers);
            eprintln!(
                "crossing: completed {} passes ({} -> {} crossings, {:.1}% reduction)",
                self.config.max_passes,
                initial_crossings,
                final_crossings,
                if initial_crossings > 0 {
                    (1.0 - (final_crossings as f32 / initial_crossings as f32)) * 100.0
                } else {
                    0.0
                }
            );
        }

        self.count_crossings(graph, layers)
    }

    /// Count the total number of edge crossings in the current layout.
    ///
    /// An edge crossing occurs when edges (u1,v1) and (u2,v2) between adjacent
    /// layers cross each other. This happens when u1 < u2 but v1 > v2 (or vice versa).
    pub fn count_crossings(&self, graph: &Graph, layers: &[Vec<usize>]) -> usize {
        let mut total = 0;

        // Build position maps for each layer
        let layer_positions: Vec<HashMap<usize, usize>> = layers
            .iter()
            .map(|layer| {
                layer
                    .iter()
                    .enumerate()
                    .map(|(pos, &node_idx)| (node_idx, pos))
                    .collect()
            })
            .collect();

        // Check crossings between each pair of adjacent layers
        for layer_idx in 0..layers.len().saturating_sub(1) {
            let upper_pos = &layer_positions[layer_idx];
            let lower_pos = &layer_positions[layer_idx + 1];

            // Collect edges between these layers
            let edges: Vec<(usize, usize)> = graph
                .edges
                .iter()
                .filter(|e| !e.is_back_edge)
                .filter_map(|e| {
                    let from_idx = graph.nodes.iter().position(|n| n.id == e.from)?;
                    let to_idx = graph.nodes.iter().position(|n| n.id == e.to)?;

                    // Edge goes from upper layer to lower layer
                    if upper_pos.contains_key(&from_idx) && lower_pos.contains_key(&to_idx) {
                        Some((*upper_pos.get(&from_idx)?, *lower_pos.get(&to_idx)?))
                    } else if upper_pos.contains_key(&to_idx) && lower_pos.contains_key(&from_idx) {
                        Some((*upper_pos.get(&to_idx)?, *lower_pos.get(&from_idx)?))
                    } else {
                        None
                    }
                })
                .collect();

            // Count crossings using inversion count
            for i in 0..edges.len() {
                for j in (i + 1)..edges.len() {
                    let (u1, v1) = edges[i];
                    let (u2, v2) = edges[j];
                    // Edges cross if their endpoints are in opposite order
                    if (u1 < u2 && v1 > v2) || (u1 > u2 && v1 < v2) {
                        total += 1;
                    }
                }
            }
        }

        total
    }

    /// Sort nodes in target_layer based on their connections to reference_layer.
    fn sort_layer(
        &self,
        graph: &Graph,
        layers: &mut [Vec<usize>],
        target_idx: usize,
        ref_idx: usize,
    ) {
        let ref_layer = layers[ref_idx].clone();
        let target_layer = &mut layers[target_idx];

        let weights = self.calculate_weights(graph, target_layer, &ref_layer);

        // Group nodes by subgraph to maintain contiguity
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
                        avg_weight: 0.0,
                    });
                    subgraph_clusters.insert(sg.to_string(), idx);
                }
            } else {
                clusters.push(Cluster {
                    nodes: vec![node_idx],
                    avg_weight: 0.0,
                });
            }
        }

        // Sort nodes within each cluster and calculate cluster weight
        for cluster in &mut clusters {
            cluster.nodes.sort_by(|&a, &b| {
                let wa = weights.get(&a).copied().unwrap_or(f32::MAX);
                let wb = weights.get(&b).copied().unwrap_or(f32::MAX);
                wa.partial_cmp(&wb).unwrap_or(Ordering::Equal)
            });

            let mut sum = 0.0;
            let mut count = 0.0;
            for &node_idx in &cluster.nodes {
                if let Some(&val) = weights.get(&node_idx) {
                    sum += val;
                    count += 1.0;
                }
            }
            cluster.avg_weight = if count > 0.0 { sum / count } else { f32::MAX };
        }

        // Sort clusters by average weight
        clusters.sort_by(|a, b| {
            a.avg_weight
                .partial_cmp(&b.avg_weight)
                .unwrap_or(Ordering::Equal)
        });

        // Flatten clusters back to layer
        *target_layer = clusters.into_iter().flat_map(|c| c.nodes).collect();
    }

    /// Calculate weights for each node based on positions of connected neighbors.
    fn calculate_weights(
        &self,
        graph: &Graph,
        target_layer: &[usize],
        ref_layer: &[usize],
    ) -> HashMap<usize, f32> {
        let mut weights = HashMap::new();

        // Build position lookup for reference layer
        let ref_positions: HashMap<&str, usize> = ref_layer
            .iter()
            .enumerate()
            .map(|(i, &idx)| (graph.nodes[idx].id.as_str(), i))
            .collect();

        for &node_idx in target_layer {
            let node_id = &graph.nodes[node_idx].id;
            let mut neighbor_positions: Vec<f32> = Vec::new();

            // Find all neighbors in the reference layer
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
                    neighbor_positions.push(pos as f32);
                }
            }

            if !neighbor_positions.is_empty() {
                let weight = match self.config.heuristic {
                    Heuristic::Barycenter => {
                        // Mean position
                        neighbor_positions.iter().sum::<f32>() / neighbor_positions.len() as f32
                    }
                    Heuristic::Median => {
                        // Median position (more robust to outliers)
                        neighbor_positions
                            .sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
                        let mid = neighbor_positions.len() / 2;
                        if neighbor_positions.len().is_multiple_of(2)
                            && neighbor_positions.len() > 1
                        {
                            (neighbor_positions[mid - 1] + neighbor_positions[mid]) / 2.0
                        } else {
                            neighbor_positions[mid]
                        }
                    }
                };
                weights.insert(node_idx, weight);
            }
        }

        weights
    }
}

impl Default for CrossingMinimizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal cluster structure for grouping subgraph nodes
#[derive(Debug)]
struct Cluster {
    nodes: Vec<usize>,
    avg_weight: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, Node};

    fn make_test_graph() -> (Graph, Vec<Vec<usize>>) {
        // Create a simple graph:
        //   A   B   C     (layer 0)
        //    \ / \ /
        //     D   E       (layer 1)
        let mut graph = Graph::new();
        graph.nodes.push(Node::new("A", "A"));
        graph.nodes.push(Node::new("B", "B"));
        graph.nodes.push(Node::new("C", "C"));
        graph.nodes.push(Node::new("D", "D"));
        graph.nodes.push(Node::new("E", "E"));

        graph.edges.push(Edge::new("A", "D"));
        graph.edges.push(Edge::new("B", "D"));
        graph.edges.push(Edge::new("B", "E"));
        graph.edges.push(Edge::new("C", "E"));

        let layers = vec![vec![0, 1, 2], vec![3, 4]];
        (graph, layers)
    }

    #[test]
    fn test_count_crossings_no_crossing() {
        let (graph, layers) = make_test_graph();
        let minimizer = CrossingMinimizer::new();
        let crossings = minimizer.count_crossings(&graph, &layers);
        // A->D, B->D, B->E, C->E with order A,B,C and D,E has no crossings
        assert_eq!(crossings, 0);
    }

    #[test]
    fn test_count_crossings_with_crossing() {
        let (graph, mut layers) = make_test_graph();
        // Reverse the lower layer to create crossings
        layers[1].reverse(); // Now [E, D] instead of [D, E]
        let minimizer = CrossingMinimizer::new();
        let crossings = minimizer.count_crossings(&graph, &layers);
        // With D,E swapped, edges should cross
        assert!(crossings > 0);
    }

    #[test]
    fn test_minimize_reduces_crossings() {
        let (graph, mut layers) = make_test_graph();
        // Start with a bad order that has crossings
        layers[1].reverse();

        let minimizer = CrossingMinimizer::new();
        let initial = minimizer.count_crossings(&graph, &layers);
        let final_count = minimizer.minimize(&graph, &mut layers);

        assert!(final_count <= initial, "Crossings should not increase");
    }

    #[test]
    fn test_median_heuristic() {
        let config = CrossingConfig {
            heuristic: Heuristic::Median,
            ..Default::default()
        };
        let minimizer = CrossingMinimizer::with_config(config);
        let (graph, mut layers) = make_test_graph();
        let _result = minimizer.minimize(&graph, &mut layers);
        // Should complete without panic
    }

    #[test]
    fn test_barycenter_heuristic() {
        let config = CrossingConfig {
            heuristic: Heuristic::Barycenter,
            max_passes: 4,              // Match original behavior
            convergence_threshold: 0.0, // Always run all passes
        };
        let minimizer = CrossingMinimizer::with_config(config);
        let (graph, mut layers) = make_test_graph();
        let _result = minimizer.minimize(&graph, &mut layers);
        // Should complete without panic
    }

    #[test]
    fn test_convergence_detection() {
        let config = CrossingConfig {
            max_passes: 100,
            convergence_threshold: 0.01,
            ..Default::default()
        };
        let minimizer = CrossingMinimizer::with_config(config);
        let (graph, mut layers) = make_test_graph();

        // Even with 100 max passes, should converge early
        let _result = minimizer.minimize(&graph, &mut layers);
    }

    #[test]
    fn test_empty_layers() {
        let graph = Graph::new();
        let mut layers: Vec<Vec<usize>> = vec![];
        let minimizer = CrossingMinimizer::new();
        let result = minimizer.minimize(&graph, &mut layers);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_single_layer() {
        let mut graph = Graph::new();
        graph.nodes.push(Node::new("A", "A"));
        graph.nodes.push(Node::new("B", "B"));
        let mut layers = vec![vec![0, 1]];

        let minimizer = CrossingMinimizer::new();
        let result = minimizer.minimize(&graph, &mut layers);
        assert_eq!(result, 0);
    }
}
