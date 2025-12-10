//! Graph data structures - Node, Edge, Graph, Subgraph
//!
//! See SPEC §2.3 for full struct definitions

use std::collections::{HashMap, HashSet};

/// Node shape variants from Mermaid syntax
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum NodeShape {
    #[default]
    Rectangle, // [text] - default box
    Rounded,          // (text) - rounded corners
    Diamond,          // {text} - decision diamond
    Circle,           // ((text)) - circular node
    Stadium,          // ([text]) - pill/stadium shape
    Asymmetric,       // >text] - flag shape
    Parallelogram,    // [/text/] - parallelogram (lean right)
    ParallelogramAlt, // [\text\] - parallelogram (lean left)
    Trapezoid,        // [/text\] - trapezoid (wider top)
    TrapezoidAlt,     // [\text/] - trapezoid (wider bottom)
    Hexagon,          // {{text}} - hexagon
    Database,         // [(text)] - cylinder/database
    Subroutine,       // [[text]] - subroutine box
}

/// Node in the graph (positioned after layout)
#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub shape: NodeShape, // Node shape from syntax
    #[allow(dead_code)]
    pub click_target: Option<String>, // Drill-down target from `click ID "file.md"`
    pub x: usize,         // Column position (set by layout)
    pub y: usize,         // Row position (set by layout)
    pub width: usize,     // Calculated from label
    pub rank: usize,      // Depth in graph (0 = root)
}

impl Node {
    /// Create a new node with default rectangle shape
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self::with_shape(id, label, NodeShape::Rectangle)
    }

    /// Create a new node with a specific shape
    pub fn with_shape(id: impl Into<String>, label: impl Into<String>, shape: NodeShape) -> Self {
        let label = label.into();
        Self {
            id: id.into(),
            width: crate::style::box_width(&label),
            label,
            shape,
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
    pub from: String,          // Source node ID
    pub to: String,            // Target node ID
    pub label: Option<String>, // Optional edge label (from -->|label| syntax)
    pub is_back_edge: bool,    // True if this edge creates a cycle
}

impl Edge {
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            label: None,
            is_back_edge: false,
        }
    }

    pub fn with_label(
        from: impl Into<String>,
        to: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            label: Some(label.into()),
            is_back_edge: false,
        }
    }
}

/// Rectangle for bounding boxes
#[derive(Debug, Clone, Default)]
pub struct Rectangle {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

/// Subgraph grouping nodes together (single-level only in v1)
#[derive(Debug, Clone)]
pub struct Subgraph {
    pub id: String,
    pub title: Option<String>,
    pub node_ids: HashSet<String>,
    pub bounds: Rectangle,          // Calculated during layout
    pub rank_range: (usize, usize), // Min/max rank of contained nodes
}

impl Subgraph {
    pub fn new(id: impl Into<String>, title: Option<String>) -> Self {
        Self {
            id: id.into(),
            title,
            node_ids: HashSet::new(),
            bounds: Rectangle::default(),
            rank_range: (0, 0),
        }
    }
}

/// Complete graph with nodes, edges, and subgraphs
#[derive(Debug, Clone, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub subgraphs: Vec<Subgraph>,
    pub node_subgraph: HashMap<String, String>, // node_id -> subgraph_id
    pub direction: Direction,
    pub warnings: Vec<String>,
}

/// Graph direction (from Mermaid `graph TD/LR/TB/BT`)
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Direction {
    #[default]
    TD, // Top-down (same as TB)
    #[allow(dead_code)]
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

    pub fn add_subgraph(&mut self, subgraph: Subgraph) {
        if !self.subgraphs.iter().any(|s| s.id == subgraph.id) {
            self.subgraphs.push(subgraph);
        }
    }

    pub fn get_subgraph(&self, id: &str) -> Option<&Subgraph> {
        self.subgraphs.iter().find(|s| s.id == id)
    }

    pub fn get_subgraph_mut(&mut self, id: &str) -> Option<&mut Subgraph> {
        self.subgraphs.iter_mut().find(|s| s.id == id)
    }

    pub fn associate_node_with_subgraph(&mut self, node_id: String, subgraph_id: String) {
        self.node_subgraph.insert(node_id.clone(), subgraph_id.clone());
        if let Some(subgraph) = self.get_subgraph_mut(&subgraph_id) {
            subgraph.node_ids.insert(node_id);
        }
    }
}
