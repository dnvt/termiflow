//! Graph data structures - Node, Edge, Graph
//!
//! See SPEC §2.3 for full struct definitions

/// Node in the graph (positioned after layout)
#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub click_target: Option<String>, // Drill-down target from `click ID "file.md"`
    pub x: usize,                      // Column position (set by layout)
    pub y: usize,                      // Row position (set by layout)
    pub width: usize,                  // Calculated from label
    pub rank: usize,                   // Depth in graph (0 = root)
}

impl Node {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        let label = label.into();
        Self {
            id: id.into(),
            width: crate::style::box_width(&label),
            label,
            click_target: None,
            x: 0,
            y: 0,
            rank: 0,
        }
    }
}

/// Edge connecting two nodes
#[derive(Debug, Clone)]
pub struct Edge {
    pub from: String,       // Source node ID
    pub to: String,         // Target node ID
    pub is_back_edge: bool, // True if this edge creates a cycle
}

impl Edge {
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            is_back_edge: false,
        }
    }
}

/// Complete graph with nodes and edges
#[derive(Debug, Clone, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub direction: Direction,
    pub warnings: Vec<String>,
}

/// Graph direction (from Mermaid `graph TD/LR/TB/BT`)
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Direction {
    #[default]
    TD, // Top-down (same as TB)
    TB, // Top to bottom
    LR, // Left to right
    BT, // Bottom to top
}

impl Graph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn get_node_mut(&mut self, id: &str) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    pub fn has_cycles(&self) -> bool {
        self.edges.iter().any(|e| e.is_back_edge)
    }

    pub fn add_node(&mut self, node: Node) {
        if self.get_node(&node.id).is_none() {
            self.nodes.push(node);
        }
    }

    pub fn add_edge(&mut self, edge: Edge) {
        self.edges.push(edge);
    }

    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }
}
