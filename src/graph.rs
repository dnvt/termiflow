//! Graph data structures - Node, Edge, Graph, Subgraph

use std::collections::{HashMap, HashSet};

use crate::geom::EdgeRoute;
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

// ============================================================================
// Subgraph Support
// ============================================================================

/// Rectangle for bounding boxes (used by subgraphs)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rectangle {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl Rectangle {
    /// Create a new rectangle
    pub fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Check if this rectangle contains a point
    #[inline]
    pub fn contains(&self, x: usize, y: usize) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    /// Check if this rectangle is valid (non-zero dimensions)
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.width > 0 && self.height > 0
    }
}

/// Subgraph grouping nodes together (single-level only in v1)
///
/// Subgraphs provide visual grouping of related nodes with:
/// - Dashed border to distinguish from node boxes
/// - Optional title displayed at the top
/// - Automatic bounds calculation from contained nodes
#[derive(Debug, Clone)]
pub struct Subgraph {
    /// Unique identifier for the subgraph
    pub id: String,
    /// Optional title shown in the subgraph border
    pub title: Option<String>,
    /// Set of node IDs contained in this subgraph
    pub node_ids: HashSet<String>,
    /// Bounding box calculated during layout
    pub bounds: Rectangle,
    /// Min/max rank of contained nodes (for layout ordering)
    pub rank_range: (usize, usize),
}

impl Subgraph {
    /// Create a new subgraph with optional title
    pub fn new(id: impl Into<String>, title: Option<String>) -> Self {
        Self {
            id: id.into(),
            title,
            node_ids: HashSet::new(),
            bounds: Rectangle::default(),
            rank_range: (0, 0),
        }
    }

    /// Check if this subgraph contains a node
    #[inline]
    pub fn contains_node(&self, node_id: &str) -> bool {
        self.node_ids.contains(node_id)
    }

    /// Add a node to this subgraph
    pub fn add_node(&mut self, node_id: impl Into<String>) {
        self.node_ids.insert(node_id.into());
    }

    /// Check if the subgraph has a title
    #[inline]
    pub fn has_title(&self) -> bool {
        self.title.is_some()
    }
}

/// Complete graph with nodes and edges
#[derive(Debug, Clone, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub direction: Direction,
    pub warnings: Vec<String>,
    /// Subgraphs for visual grouping (single-level only in v1)
    pub subgraphs: Vec<Subgraph>,
    /// Maps node ID to its containing subgraph ID (if any)
    pub node_subgraph: HashMap<String, String>,
    /// Optional precomputed routes (kept for legacy/experimental spikes; the
    /// main pipeline uses the deterministic waterfall layout + live routing)
    pub edge_routes: HashMap<usize, EdgeRoute>,
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

    // ========================================================================
    // Subgraph Methods
    // ========================================================================

    /// Add a subgraph to the graph
    pub fn add_subgraph(&mut self, subgraph: Subgraph) {
        if self.get_subgraph(&subgraph.id).is_none() {
            self.subgraphs.push(subgraph);
        }
    }

    /// Get a subgraph by ID
    pub fn get_subgraph(&self, id: &str) -> Option<&Subgraph> {
        self.subgraphs.iter().find(|s| s.id == id)
    }

    /// Get a mutable reference to a subgraph by ID
    pub fn get_subgraph_mut(&mut self, id: &str) -> Option<&mut Subgraph> {
        self.subgraphs.iter_mut().find(|s| s.id == id)
    }

    /// Associate a node with a subgraph (tracks membership)
    pub fn associate_node_with_subgraph(&mut self, node_id: &str, subgraph_id: &str) {
        self.node_subgraph
            .insert(node_id.to_string(), subgraph_id.to_string());
        if let Some(subgraph) = self.get_subgraph_mut(subgraph_id) {
            subgraph.add_node(node_id);
        }
    }

    /// Get the subgraph containing a node (if any)
    pub fn get_node_subgraph(&self, node_id: &str) -> Option<&str> {
        self.node_subgraph.get(node_id).map(|s| s.as_str())
    }

    /// Check if the graph has any subgraphs
    #[inline]
    pub fn has_subgraphs(&self) -> bool {
        !self.subgraphs.is_empty()
    }
}
