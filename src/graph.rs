//! Graph data structures - Node, Edge, Graph

use crate::style::BOX_HEIGHT;

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

    /// Visual center x-coordinate
    #[inline]
    pub fn center_x(&self) -> usize {
        self.x + self.width / 2
    }

    /// Visual center y-coordinate
    #[inline]
    pub fn center_y(&self) -> usize {
        self.y + BOX_HEIGHT / 2
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
    #[allow(dead_code)]
    TB, // Top to bottom
    LR, // Left to right
    RL, // Right to left
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
