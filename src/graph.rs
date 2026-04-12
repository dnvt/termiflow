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
    DoubleCircle,     // (((text))) - double circle (event/start)
}

/// Node in the graph (positioned after layout)
#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub label: String,
    /// Pre-measured label lines for rendering (optional; empty means "use label").
    pub label_lines: Vec<String>,
    pub shape: NodeShape, // Node shape from syntax
    #[allow(dead_code)]
    pub click_target: Option<String>, // Drill-down target from `click ID "file.md"`
    pub x: usize,         // Column position (set by layout)
    pub y: usize,         // Row position (set by layout)
    pub width: usize,     // Calculated from label
    pub height: usize,    // Box height in rows (default = BOX_HEIGHT)
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
            label_lines: Vec::new(),
            shape,
            click_target: None,
            x: 0,
            y: 0,
            height: BOX_HEIGHT,
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
        let h = self.height.max(BOX_HEIGHT);
        self.y + h / 2
    }

    #[inline]
    pub fn bottom_y(&self) -> usize {
        self.y + self.height.max(BOX_HEIGHT)
    }
}

/// Visual/semantic kind of an edge, matching Mermaid flowchart syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EdgeKind {
    #[default]
    Arrow, // --> standard directed with arrowhead
    Open,          // --- open link, no arrowhead
    Thick,         // ==> heavy/bold shaft with arrowhead
    Dotted,        // -.-> dashed shaft with arrowhead
    Bidirectional, // <--> arrowheads on both ends
    CircleEnd,     // --o circle end marker (non-directional)
    CrossEnd,      // --x cross end marker (non-directional)
}

/// Edge connecting two nodes
#[derive(Debug, Clone)]
pub struct Edge {
    pub from: String,          // Source node ID
    pub to: String,            // Target node ID
    pub label: Option<String>, // Optional edge label (from -->|label| syntax)
    pub is_back_edge: bool,    // True if this edge creates a cycle
    pub kind: EdgeKind,        // Visual/semantic kind of the edge
}

impl Edge {
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            label: None,
            is_back_edge: false,
            kind: EdgeKind::Arrow,
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
            kind: EdgeKind::Arrow,
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

/// Rendered subgraph title text.
pub fn subgraph_title_text(title: &str) -> String {
    format!(" {title} ")
}

/// Display width of the rendered subgraph title token.
pub fn subgraph_title_len(title: &str) -> usize {
    subgraph_title_text(title).chars().count()
}

/// Interior row that carries the subgraph title for the given orientation.
pub fn subgraph_title_row(top_y: usize, height: usize, direction: Direction) -> usize {
    if matches!(direction, Direction::BT) {
        top_y + height.saturating_sub(2)
    } else {
        top_y.saturating_add(1)
    }
}

/// Horizontal title origin inside a subgraph container for the given orientation.
///
/// Titles are anchored to the leading edge of the subgraph based on direction:
/// TD/TB/LR anchor left, RL anchors right, and BT anchors bottom-left.
pub fn subgraph_title_start_x(
    left_x: usize,
    width: usize,
    title: &str,
    direction: Direction,
) -> Option<usize> {
    let len = subgraph_title_len(title);
    if len == 0 || len > width.saturating_sub(4) {
        return None;
    }

    Some(match direction {
        Direction::RL => left_x + width.saturating_sub(len + 2),
        Direction::TD | Direction::TB | Direction::LR | Direction::BT => left_x.saturating_add(2),
    })
}

/// Inclusive x-span of the rendered title token inside the subgraph container.
pub fn subgraph_title_span(
    left_x: usize,
    width: usize,
    title: &str,
    direction: Direction,
) -> Option<(usize, usize)> {
    let start = subgraph_title_start_x(left_x, width, title, direction)?;
    let end = start + subgraph_title_len(title).saturating_sub(1);
    Some((start, end))
}

/// Subgraph grouping nodes together.
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
    /// Parent subgraph ID when this subgraph is nested.
    pub parent_id: Option<String>,
    /// Child subgraph IDs in declaration order.
    pub child_ids: Vec<String>,
    /// Set of node IDs contained in this subgraph
    pub node_ids: HashSet<String>,
    /// Bounding box calculated during layout
    pub bounds: Rectangle,
    /// Inner bounds (content box; excludes gutters/padding)
    pub inner_bounds: Rectangle,
    /// Min/max rank of contained nodes (for layout ordering)
    pub rank_range: (usize, usize),
}

impl Subgraph {
    /// Create a new subgraph with optional title
    pub fn new(id: impl Into<String>, title: Option<String>) -> Self {
        Self {
            id: id.into(),
            title,
            parent_id: None,
            child_ids: Vec::new(),
            node_ids: HashSet::new(),
            bounds: Rectangle::default(),
            inner_bounds: Rectangle::default(),
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

    /// Add a child subgraph to this subgraph, preserving declaration order.
    pub fn add_child(&mut self, child_id: impl Into<String>) {
        let child_id = child_id.into();
        if !self.child_ids.iter().any(|existing| existing == &child_id) {
            self.child_ids.push(child_id);
        }
    }

    /// Check if the subgraph has a title
    #[inline]
    pub fn has_title(&self) -> bool {
        self.title.is_some()
    }

    /// Check if this subgraph has a parent.
    #[inline]
    pub fn has_parent(&self) -> bool {
        self.parent_id.is_some()
    }

    /// Check if this subgraph has nested child subgraphs.
    #[inline]
    pub fn has_children(&self) -> bool {
        !self.child_ids.is_empty()
    }
}

/// Complete graph with nodes and edges
#[derive(Debug, Clone, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub direction: Direction,
    pub warnings: Vec<String>,
    /// Subgraphs for visual grouping. Nested parent/child structure may be present
    /// even when later layout/render phases do not yet fully exploit it.
    pub subgraphs: Vec<Subgraph>,
    /// Maps node ID to its containing subgraph ID (if any)
    pub node_subgraph: HashMap<String, String>,
    /// Optional precomputed routes (kept for legacy/experimental spikes; the
    /// main pipeline uses the deterministic waterfall layout + live routing)
    pub edge_routes: HashMap<usize, EdgeRoute>,
}

/// Graph direction (from Mermaid `graph TD/LR/TB/BT`)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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
        if let Some(previous_id) = self
            .node_subgraph
            .insert(node_id.to_string(), subgraph_id.to_string())
            .filter(|previous_id| previous_id != subgraph_id)
        {
            if let Some(previous_subgraph) = self.get_subgraph_mut(&previous_id) {
                previous_subgraph.node_ids.remove(node_id);
            }
        }
        if let Some(subgraph) = self.get_subgraph_mut(subgraph_id) {
            subgraph.add_node(node_id);
        }
    }

    /// Get the subgraph containing a node (if any)
    pub fn get_node_subgraph(&self, node_id: &str) -> Option<&str> {
        self.node_subgraph.get(node_id).map(|s| s.as_str())
    }

    /// Return the node's subgraph ancestry from innermost to outermost.
    pub fn node_subgraph_chain<'a>(&'a self, node_id: &str) -> Vec<&'a str> {
        let mut chain = Vec::new();
        let mut current = self.get_node_subgraph(node_id);
        while let Some(current_id) = current {
            chain.push(current_id);
            current = self
                .get_subgraph(current_id)
                .and_then(|subgraph| subgraph.parent_id.as_deref());
        }
        chain
    }

    /// Return true when `ancestor_id` is a declared ancestor of `descendant_id`.
    pub fn is_subgraph_ancestor(&self, ancestor_id: &str, descendant_id: &str) -> bool {
        let mut current = self
            .get_subgraph(descendant_id)
            .and_then(|subgraph| subgraph.parent_id.as_deref());
        while let Some(parent_id) = current {
            if parent_id == ancestor_id {
                return true;
            }
            current = self
                .get_subgraph(parent_id)
                .and_then(|subgraph| subgraph.parent_id.as_deref());
        }
        false
    }

    /// Return the subgraph borders an edge exits and enters.
    ///
    /// Each vector is ordered from innermost to outermost exclusive boundary,
    /// stopping at the nearest common ancestor shared by the endpoints.
    pub fn edge_boundary_crossings<'a>(
        &'a self,
        from_node_id: &str,
        to_node_id: &str,
    ) -> (Vec<&'a str>, Vec<&'a str>) {
        let from_chain = self.node_subgraph_chain(from_node_id);
        let to_chain = self.node_subgraph_chain(to_node_id);

        let mut from_exclusive_len = from_chain.len();
        let mut to_exclusive_len = to_chain.len();
        while from_exclusive_len > 0
            && to_exclusive_len > 0
            && from_chain[from_exclusive_len - 1] == to_chain[to_exclusive_len - 1]
        {
            from_exclusive_len -= 1;
            to_exclusive_len -= 1;
        }

        (
            from_chain[..from_exclusive_len].to_vec(),
            to_chain[..to_exclusive_len].to_vec(),
        )
    }

    /// Check whether an edge crosses any subgraph boundary.
    pub fn edge_crosses_subgraph_boundary(&self, from_node_id: &str, to_node_id: &str) -> bool {
        let (exit_subgraphs, enter_subgraphs) =
            self.edge_boundary_crossings(from_node_id, to_node_id);
        !exit_subgraphs.is_empty() || !enter_subgraphs.is_empty()
    }

    /// Check whether a node belongs to a subgraph directly or through one of its
    /// nested descendants.
    pub fn is_node_in_subgraph_tree(&self, node_id: &str, subgraph_id: &str) -> bool {
        self.node_subgraph_chain(node_id).contains(&subgraph_id)
    }

    /// Check if the graph has any subgraphs
    #[inline]
    pub fn has_subgraphs(&self) -> bool {
        !self.subgraphs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Node
    // =========================================================================

    #[test]
    fn node_new_defaults() {
        let n = Node::new("id", "label");
        assert_eq!(n.id, "id");
        assert_eq!(n.label, "label");
        assert_eq!(n.shape, NodeShape::Rectangle);
        assert_eq!(n.x, 0);
        assert_eq!(n.y, 0);
        assert_eq!(n.rank, 0);
        assert_eq!(n.height, crate::style::BOX_HEIGHT);
        assert!(n.label_lines.is_empty());
        assert!(n.click_target.is_none());
    }

    #[test]
    fn node_with_shape_stores_shape() {
        let shapes = [
            NodeShape::Diamond,
            NodeShape::Circle,
            NodeShape::Stadium,
            NodeShape::Hexagon,
            NodeShape::Database,
            NodeShape::Subroutine,
            NodeShape::DoubleCircle,
            NodeShape::Asymmetric,
            NodeShape::Parallelogram,
            NodeShape::ParallelogramAlt,
            NodeShape::Trapezoid,
            NodeShape::TrapezoidAlt,
        ];
        for shape in shapes {
            let n = Node::with_shape("x", "label", shape);
            assert_eq!(n.shape, shape, "shape variant {shape:?} not stored");
        }
    }

    #[test]
    fn node_center_x_even_width() {
        let mut n = Node::new("a", "");
        n.x = 10;
        n.width = 20;
        assert_eq!(n.center_x(), 20); // 10 + 20/2
    }

    #[test]
    fn node_center_x_odd_width() {
        let mut n = Node::new("a", "");
        n.x = 0;
        n.width = 11;
        assert_eq!(n.center_x(), 5); // 0 + 11/2 (integer)
    }

    #[test]
    fn node_center_y_uses_height_max_box_height() {
        let bh = crate::style::BOX_HEIGHT;
        let mut n = Node::new("a", "");
        n.y = 10;

        // height < BOX_HEIGHT → uses BOX_HEIGHT
        n.height = bh.saturating_sub(1).max(1);
        assert_eq!(n.center_y(), 10 + bh / 2);

        // height > BOX_HEIGHT → uses height
        n.height = bh + 4;
        assert_eq!(n.center_y(), 10 + (bh + 4) / 2);

        // height == BOX_HEIGHT
        n.height = bh;
        assert_eq!(n.center_y(), 10 + bh / 2);
    }

    #[test]
    fn node_bottom_y_enforces_min_height() {
        let bh = crate::style::BOX_HEIGHT;
        let mut n = Node::new("a", "");
        n.y = 5;

        // height < BOX_HEIGHT → bottom_y uses BOX_HEIGHT
        n.height = 1;
        assert_eq!(n.bottom_y(), 5 + bh);

        // height > BOX_HEIGHT → bottom_y uses height
        n.height = bh + 2;
        assert_eq!(n.bottom_y(), 5 + bh + 2);
    }

    // =========================================================================
    // Edge
    // =========================================================================

    #[test]
    fn edge_new_defaults() {
        let e = Edge::new("a", "b");
        assert_eq!(e.from, "a");
        assert_eq!(e.to, "b");
        assert!(e.label.is_none());
        assert!(!e.is_back_edge);
        assert_eq!(e.kind, EdgeKind::Arrow);
    }

    #[test]
    fn edge_with_label_stores_label() {
        let e = Edge::with_label("x", "y", "hello");
        assert_eq!(e.label, Some("hello".to_string()));
        assert_eq!(e.from, "x");
        assert_eq!(e.to, "y");
        assert!(!e.is_back_edge);
        assert_eq!(e.kind, EdgeKind::Arrow);
    }

    #[test]
    fn edge_kind_default_is_arrow() {
        assert_eq!(EdgeKind::default(), EdgeKind::Arrow);
    }

    // =========================================================================
    // Rectangle
    // =========================================================================

    #[test]
    fn rectangle_contains_inclusive_corners() {
        let r = Rectangle::new(5, 10, 4, 3); // x=5..8, y=10..12
        assert!(r.contains(5, 10)); // top-left
        assert!(r.contains(8, 12)); // bottom-right (x+w-1, y+h-1)
        assert!(!r.contains(9, 12)); // one past right
        assert!(!r.contains(5, 13)); // one past bottom
        assert!(!r.contains(4, 10)); // one before left
        assert!(!r.contains(5, 9)); // one above top
        assert!(r.contains(7, 11)); // interior
    }

    #[test]
    fn rectangle_contains_zero_dimensions() {
        // Zero width: nothing inside
        let r = Rectangle::new(5, 5, 0, 5);
        assert!(!r.contains(5, 5));

        // Zero height: nothing inside
        let r = Rectangle::new(5, 5, 5, 0);
        assert!(!r.contains(5, 5));
    }

    #[test]
    fn rectangle_is_valid() {
        assert!(Rectangle::new(0, 0, 1, 1).is_valid());
        assert!(Rectangle::new(5, 5, 10, 10).is_valid());
        assert!(!Rectangle::new(0, 0, 0, 5).is_valid());
        assert!(!Rectangle::new(0, 0, 5, 0).is_valid());
        assert!(!Rectangle::new(0, 0, 0, 0).is_valid());
    }

    // =========================================================================
    // Subgraph
    // =========================================================================

    #[test]
    fn subgraph_new_empty() {
        let sg = Subgraph::new("sg1", Some("My Group".to_string()));
        assert_eq!(sg.id, "sg1");
        assert_eq!(sg.title, Some("My Group".to_string()));
        assert!(sg.parent_id.is_none());
        assert!(sg.child_ids.is_empty());
        assert!(sg.node_ids.is_empty());
        assert!(!sg.bounds.is_valid());
        assert_eq!(sg.rank_range, (0, 0));
    }

    #[test]
    fn subgraph_no_title() {
        let sg = Subgraph::new("sg", None);
        assert!(!sg.has_title());
        assert!(sg.title.is_none());
    }

    #[test]
    fn subgraph_has_title() {
        let sg = Subgraph::new("sg", Some("Title".to_string()));
        assert!(sg.has_title());
    }

    #[test]
    fn subgraph_tracks_children_without_duplicates() {
        let mut sg = Subgraph::new("parent", None);
        assert!(!sg.has_children());
        assert!(!sg.has_parent());

        sg.add_child("child");
        sg.add_child("child");

        assert!(sg.has_children());
        assert_eq!(sg.child_ids, vec!["child".to_string()]);
    }

    #[test]
    fn subgraph_add_and_contains_node() {
        let mut sg = Subgraph::new("sg", None);
        assert!(!sg.contains_node("n1"));
        sg.add_node("n1");
        assert!(sg.contains_node("n1"));
        assert!(!sg.contains_node("n2"));

        // Adding same node twice is idempotent (HashSet)
        sg.add_node("n1");
        assert_eq!(sg.node_ids.len(), 1);
    }

    #[test]
    fn subgraph_contains_node_is_case_sensitive() {
        let mut sg = Subgraph::new("sg", None);
        sg.add_node("Node");
        assert!(sg.contains_node("Node"));
        assert!(!sg.contains_node("node"));
    }

    // =========================================================================
    // Graph
    // =========================================================================

    #[test]
    fn graph_new_is_empty() {
        let g = Graph::new();
        assert!(g.nodes.is_empty());
        assert!(g.edges.is_empty());
        assert!(g.warnings.is_empty());
        assert!(g.subgraphs.is_empty());
        assert!(!g.has_subgraphs());
        assert!(!g.has_cycles());
        assert_eq!(g.direction, Direction::TD);
    }

    #[test]
    fn graph_add_node_and_get() {
        let mut g = Graph::new();
        g.add_node(Node::new("a", "Alpha"));
        assert_eq!(g.nodes.len(), 1);
        assert!(g.get_node("a").is_some());
        assert_eq!(
            g.get_node("a").expect("node 'a' was just added").label,
            "Alpha"
        );
        assert!(g.get_node("b").is_none());
    }

    #[test]
    fn graph_add_node_deduplicates_by_id() {
        let mut g = Graph::new();
        g.add_node(Node::new("a", "first"));
        g.add_node(Node::new("a", "second")); // duplicate — should be skipped
        assert_eq!(g.nodes.len(), 1);
        assert_eq!(
            g.get_node("a").expect("node 'a' was added first").label,
            "first"
        );
    }

    #[test]
    fn graph_add_edge_no_dedup() {
        let mut g = Graph::new();
        g.add_edge(Edge::new("a", "b"));
        g.add_edge(Edge::new("a", "b")); // duplicate allowed
        assert_eq!(g.edges.len(), 2);
    }

    #[test]
    fn graph_add_warning() {
        let mut g = Graph::new();
        g.add_warning("warn1".to_string());
        g.add_warning("warn2".to_string());
        assert_eq!(g.warnings, vec!["warn1", "warn2"]);
    }

    #[test]
    fn graph_has_cycles_reflects_back_edges() {
        let mut g = Graph::new();
        g.add_edge(Edge::new("a", "b"));
        assert!(!g.has_cycles());

        let mut back = Edge::new("b", "a");
        back.is_back_edge = true;
        g.add_edge(back);
        assert!(g.has_cycles());
    }

    #[test]
    fn graph_add_subgraph_and_get() {
        let mut g = Graph::new();
        g.add_subgraph(Subgraph::new("sg1", None));
        assert!(g.has_subgraphs());
        assert!(g.get_subgraph("sg1").is_some());
        assert!(g.get_subgraph("sg2").is_none());
    }

    #[test]
    fn graph_add_subgraph_deduplicates_by_id() {
        let mut g = Graph::new();
        g.add_subgraph(Subgraph::new("sg", Some("First".to_string())));
        g.add_subgraph(Subgraph::new("sg", Some("Second".to_string())));
        assert_eq!(g.subgraphs.len(), 1);
        assert_eq!(
            g.get_subgraph("sg")
                .expect("subgraph 'sg' was just added")
                .title,
            Some("First".to_string())
        );
    }

    #[test]
    fn graph_associate_node_with_subgraph() {
        let mut g = Graph::new();
        g.add_subgraph(Subgraph::new("sg", None));
        g.add_node(Node::new("n1", "Node 1"));

        g.associate_node_with_subgraph("n1", "sg");

        assert_eq!(g.get_node_subgraph("n1"), Some("sg"));
        assert!(g
            .get_subgraph("sg")
            .expect("subgraph 'sg' was just added")
            .contains_node("n1"));
    }

    #[test]
    fn graph_associate_node_with_subgraph_reassigns_membership() {
        let mut g = Graph::new();
        g.add_subgraph(Subgraph::new("outer", None));
        g.add_subgraph(Subgraph::new("inner", None));
        g.add_node(Node::new("n1", "Node 1"));

        g.associate_node_with_subgraph("n1", "outer");
        g.associate_node_with_subgraph("n1", "inner");

        assert_eq!(g.get_node_subgraph("n1"), Some("inner"));
        assert!(!g
            .get_subgraph("outer")
            .expect("outer subgraph should exist")
            .contains_node("n1"));
        assert!(g
            .get_subgraph("inner")
            .expect("inner subgraph should exist")
            .contains_node("n1"));
    }

    #[test]
    fn graph_get_node_subgraph_returns_none_for_unassociated() {
        let mut g = Graph::new();
        g.add_node(Node::new("n1", "Node 1"));
        assert!(g.get_node_subgraph("n1").is_none());
        assert!(g.get_node_subgraph("nonexistent").is_none());
    }

    #[test]
    fn graph_is_node_in_subgraph_tree_checks_ancestor_chain() {
        let mut g = Graph::new();
        g.add_subgraph(Subgraph::new("outer", None));
        g.add_subgraph(Subgraph::new("inner", None));
        g.get_subgraph_mut("inner").unwrap().parent_id = Some("outer".to_string());
        g.get_subgraph_mut("outer").unwrap().add_child("inner");
        g.add_node(Node::new("n1", "Node 1"));
        g.associate_node_with_subgraph("n1", "inner");

        assert!(g.is_node_in_subgraph_tree("n1", "inner"));
        assert!(g.is_node_in_subgraph_tree("n1", "outer"));
        assert!(!g.is_node_in_subgraph_tree("n1", "missing"));
        assert!(!g.is_node_in_subgraph_tree("missing-node", "outer"));
    }

    #[test]
    fn graph_node_subgraph_chain_orders_inner_to_outer() {
        let mut g = Graph::new();
        g.add_subgraph(Subgraph::new("outer", None));
        g.add_subgraph(Subgraph::new("inner", None));
        g.get_subgraph_mut("inner").unwrap().parent_id = Some("outer".to_string());
        g.get_subgraph_mut("outer").unwrap().add_child("inner");
        g.add_node(Node::new("n1", "Node 1"));
        g.associate_node_with_subgraph("n1", "inner");

        assert_eq!(g.node_subgraph_chain("n1"), vec!["inner", "outer"]);
    }

    #[test]
    fn graph_edge_boundary_crossings_child_to_parent_exit_only_child() {
        let mut g = Graph::new();
        g.add_subgraph(Subgraph::new("parent", None));
        g.add_subgraph(Subgraph::new("child", None));
        g.get_subgraph_mut("child").unwrap().parent_id = Some("parent".to_string());
        g.get_subgraph_mut("parent").unwrap().add_child("child");
        g.add_node(Node::new("inner", "Inner"));
        g.add_node(Node::new("outer", "Outer"));
        g.associate_node_with_subgraph("inner", "child");
        g.associate_node_with_subgraph("outer", "parent");

        let (exits, enters) = g.edge_boundary_crossings("inner", "outer");
        assert_eq!(exits, vec!["child"]);
        assert!(enters.is_empty());
    }

    #[test]
    fn graph_edge_boundary_crossings_between_siblings_skip_common_parent() {
        let mut g = Graph::new();
        g.add_subgraph(Subgraph::new("parent", None));
        g.add_subgraph(Subgraph::new("left", None));
        g.add_subgraph(Subgraph::new("right", None));
        g.get_subgraph_mut("left").unwrap().parent_id = Some("parent".to_string());
        g.get_subgraph_mut("right").unwrap().parent_id = Some("parent".to_string());
        g.get_subgraph_mut("parent").unwrap().add_child("left");
        g.get_subgraph_mut("parent").unwrap().add_child("right");
        g.add_node(Node::new("a", "A"));
        g.add_node(Node::new("b", "B"));
        g.associate_node_with_subgraph("a", "left");
        g.associate_node_with_subgraph("b", "right");

        let (exits, enters) = g.edge_boundary_crossings("a", "b");
        assert_eq!(exits, vec!["left"]);
        assert_eq!(enters, vec!["right"]);
    }

    #[test]
    fn graph_edge_boundary_crossings_external_to_nested_include_all_entered_ancestors() {
        let mut g = Graph::new();
        g.add_subgraph(Subgraph::new("parent", None));
        g.add_subgraph(Subgraph::new("child", None));
        g.get_subgraph_mut("child").unwrap().parent_id = Some("parent".to_string());
        g.get_subgraph_mut("parent").unwrap().add_child("child");
        g.add_node(Node::new("outside", "Outside"));
        g.add_node(Node::new("inside", "Inside"));
        g.associate_node_with_subgraph("inside", "child");

        let (exits, enters) = g.edge_boundary_crossings("outside", "inside");
        assert!(exits.is_empty());
        assert_eq!(enters, vec!["child", "parent"]);
        assert!(g.edge_crosses_subgraph_boundary("outside", "inside"));
    }

    #[test]
    fn graph_is_subgraph_ancestor_checks_parent_chain() {
        let mut g = Graph::new();
        g.add_subgraph(Subgraph::new("outer", Some("Outer".into())));
        g.add_subgraph(Subgraph::new("inner", Some("Inner".into())));
        g.add_subgraph(Subgraph::new("leaf", Some("Leaf".into())));

        g.get_subgraph_mut("inner").unwrap().parent_id = Some("outer".to_string());
        g.get_subgraph_mut("leaf").unwrap().parent_id = Some("inner".to_string());

        assert!(g.is_subgraph_ancestor("outer", "inner"));
        assert!(g.is_subgraph_ancestor("outer", "leaf"));
        assert!(g.is_subgraph_ancestor("inner", "leaf"));
        assert!(!g.is_subgraph_ancestor("leaf", "inner"));
        assert!(!g.is_subgraph_ancestor("inner", "outer"));
    }

    #[test]
    fn graph_get_node_mut_allows_mutation() {
        let mut g = Graph::new();
        g.add_node(Node::new("a", "Original"));
        if let Some(n) = g.get_node_mut("a") {
            n.label = "Modified".to_string();
        }
        assert_eq!(
            g.get_node("a").expect("node 'a' was just added").label,
            "Modified"
        );
    }

    #[test]
    fn graph_get_subgraph_mut_allows_mutation() {
        let mut g = Graph::new();
        g.add_subgraph(Subgraph::new("sg", None));
        if let Some(sg) = g.get_subgraph_mut("sg") {
            sg.title = Some("New Title".to_string());
        }
        assert_eq!(
            g.get_subgraph("sg")
                .expect("subgraph 'sg' was just added")
                .title,
            Some("New Title".to_string())
        );
    }

    #[test]
    fn direction_default_is_td() {
        assert_eq!(Direction::default(), Direction::TD);
    }

    #[test]
    fn node_shape_default_is_rectangle() {
        assert_eq!(NodeShape::default(), NodeShape::Rectangle);
    }
}
