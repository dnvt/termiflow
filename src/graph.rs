//! Graph data structures - Node, Edge, Graph, Subgraph

use std::collections::{HashMap, HashSet};

use serde::Serialize;

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

/// Rendered subgraph title text with additional wrapper padding on both sides.
///
/// The ordinary title contract is one space on either side. Topology-aware
/// portal policies may request extra visual gutter, but the visible title
/// characters remain the same.
pub fn subgraph_title_text_with_padding(title: &str, extra_padding: usize) -> String {
    subgraph_title_text_with_padding_sides(title, extra_padding, extra_padding)
}

/// Render a title token with independent leading and trailing wrapper padding.
pub fn subgraph_title_text_with_padding_sides(
    title: &str,
    leading_extra_padding: usize,
    trailing_extra_padding: usize,
) -> String {
    let leading = " ".repeat(leading_extra_padding.saturating_add(1));
    let trailing = " ".repeat(trailing_extra_padding.saturating_add(1));
    format!("{leading}{title}{trailing}")
}

/// Display width of the rendered subgraph title token.
pub fn subgraph_title_len(title: &str) -> usize {
    subgraph_title_text(title).chars().count()
}

/// Display width of a title token with additional wrapper padding.
pub fn subgraph_title_len_with_padding(title: &str, extra_padding: usize) -> usize {
    subgraph_title_len_with_padding_sides(title, extra_padding, extra_padding)
}

/// Display width of a title token with independent wrapper padding.
pub fn subgraph_title_len_with_padding_sides(
    title: &str,
    leading_extra_padding: usize,
    trailing_extra_padding: usize,
) -> usize {
    subgraph_title_text_with_padding_sides(title, leading_extra_padding, trailing_extra_padding)
        .chars()
        .count()
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

/// Inclusive x-span of a title token with additional wrapper padding.
pub fn subgraph_title_span_with_padding(
    left_x: usize,
    width: usize,
    title: &str,
    direction: Direction,
    extra_padding: usize,
) -> Option<(usize, usize)> {
    subgraph_title_span_with_padding_sides(
        left_x,
        width,
        title,
        direction,
        extra_padding,
        extra_padding,
    )
}

/// Inclusive x-span of a title token with independent wrapper padding.
pub fn subgraph_title_span_with_padding_sides(
    left_x: usize,
    width: usize,
    title: &str,
    direction: Direction,
    leading_extra_padding: usize,
    trailing_extra_padding: usize,
) -> Option<(usize, usize)> {
    if leading_extra_padding == 0 && trailing_extra_padding == 0 {
        return subgraph_title_span(left_x, width, title, direction);
    }

    let len =
        subgraph_title_len_with_padding_sides(title, leading_extra_padding, trailing_extra_padding);
    // Keep the token inside the interior border. The extra wrapper cells are
    // themselves the intentional visual gutter, so no further trailing cell
    // is required beyond the right wrapper.
    if len == 0 || len > width.saturating_sub(3) {
        return None;
    }

    let start = match direction {
        Direction::RL => left_x + width.saturating_sub(len + 2),
        Direction::TD | Direction::TB | Direction::LR | Direction::BT => left_x + 2,
    };
    Some((start, start + len.saturating_sub(1)))
}
/// Inclusive x-span of the visible title characters, excluding the one-cell
/// wrapper padding emitted by [`subgraph_title_text`].
///
/// Portal routing may use the wrapper cells as a deliberate gutter when a
/// topology-owned lane would otherwise be flush with a subgraph wall. Keeping
/// that distinction here lets layout, routing, and title restoration agree on
/// what is text versus what is visual padding.
pub fn subgraph_title_text_span(
    left_x: usize,
    width: usize,
    title: &str,
    direction: Direction,
) -> Option<(usize, usize)> {
    let (start, end) = subgraph_title_span(left_x, width, title, direction)?;
    (end > start).then_some((start + 1, end.saturating_sub(1)))
}

/// Inclusive x-span of visible title characters for a padded title token.
pub fn subgraph_title_text_span_with_padding(
    left_x: usize,
    width: usize,
    title: &str,
    direction: Direction,
    extra_padding: usize,
) -> Option<(usize, usize)> {
    subgraph_title_text_span_with_padding_sides(
        left_x,
        width,
        title,
        direction,
        extra_padding,
        extra_padding,
    )
}

/// Inclusive x-span of visible title characters for independently padded text.
pub fn subgraph_title_text_span_with_padding_sides(
    left_x: usize,
    width: usize,
    title: &str,
    direction: Direction,
    leading_extra_padding: usize,
    trailing_extra_padding: usize,
) -> Option<(usize, usize)> {
    let (start, end) = subgraph_title_span_with_padding_sides(
        left_x,
        width,
        title,
        direction,
        leading_extra_padding,
        trailing_extra_padding,
    )?;
    let leading = leading_extra_padding.saturating_add(1);
    let trailing = trailing_extra_padding.saturating_add(1);
    (end >= start.saturating_add(leading).saturating_add(trailing))
        .then_some((start.saturating_add(leading), end.saturating_sub(trailing)))
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
    /// Optional precomputed routes produced by layout/routing stages and kept
    /// as public compatibility data for render, trace, and route ownership.
    pub edge_routes: HashMap<usize, EdgeRoute>,
}

/// The narrow BT sibling scene whose target-side incoming edges need one
/// explicit port per edge.  Keeping the selector result typed prevents the
/// renderer from rediscovering this topology from fixture names or labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BtSiblingTargetEntryScene {
    pub source_subgraph_id: String,
    pub target_subgraph_id: String,
    pub source_lower_node_id: String,
    pub source_upper_node_id: String,
    pub target_lower_node_id: String,
    pub target_upper_node_id: String,
    pub source_internal_edge_index: usize,
    pub target_internal_edge_index: usize,
    pub lower_cross_edge_index: usize,
    pub upper_cross_edge_index: usize,
}

/// The exact flat BT sibling scene with three direct, pairwise rail crossings.
/// The selector is deliberately stricter than a generic parallel-edge count:
/// projection may only add directional seams when every source and target node
/// participates once and the two titled boundaries own the complete graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BtDirectParallelSiblingScene {
    pub source_subgraph_id: String,
    pub target_subgraph_id: String,
    pub edge_indices: Vec<usize>,
}

/// The bounded BT sibling scene whose target receivers can share one quiet
/// external-side lane. Layout and route lowering consume this same typed
/// topology selector so a staged receiver cannot drift away from its route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BtExternalSideReceiverScene {
    pub source_subgraph_id: String,
    pub target_subgraph_id: String,
    pub source_external_node_id: String,
    pub source_receiver_node_id: String,
    pub sink_external_node_id: String,
}

/// Graph direction (from Mermaid `graph TD/LR/TB/BT`)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum Direction {
    #[default]
    TD, // Top-down (same as TB)
    #[allow(dead_code)]
    TB, // Top to bottom
    LR, // Left to right
    RL, // Right to left
    BT, // Bottom to top
}

impl NodeShape {
    /// Extra outward space required before an incoming arrow reaches a shape.
    ///
    /// Most contours can accept the generic one-cell target entry. Diamonds
    /// need one more cell on every side because their visible contour tapers
    /// toward the center. Mermaid's asymmetric Flag has a left-facing point,
    /// so only a left-to-right approach needs the same visual separation.
    /// Database/cylinder contours intentionally use the generic one-cell
    /// entry: an extra shape-specific bridge places a terminal arrowhead before
    /// its final shaft and makes the arrow look detached from the receiver.
    pub(crate) fn incoming_edge_clearance(self, direction: Direction) -> usize {
        usize::from(
            self == NodeShape::Diamond
                || (self == NodeShape::Asymmetric && direction == Direction::LR),
        )
    }
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

    /// Return the exact flat titled parallel-TD scene whose external
    /// attachments may share the internal portal lanes.
    ///
    /// This is intentionally a topology predicate rather than a fixture-name
    /// or label predicate. Layout and render projection both consume the same
    /// capability so a visual policy cannot drift away from the placement
    /// policy that made the scene safe.
    pub(crate) fn td_parallel_external_attachment_ids(
        &self,
    ) -> Option<(String, String, String, String, String)> {
        if self.direction != Direction::TD
            || self.subgraphs.len() != 1
            || self.nodes.len() != 6
            || self.edges.len() != 6
        {
            return None;
        }

        let subgraph = self.subgraphs.first()?;
        if subgraph.parent_id.is_some()
            || !subgraph.child_ids.is_empty()
            || subgraph.title.is_none()
            || subgraph.node_ids.len() != 4
            || !subgraph.node_ids.iter().all(|node_id| {
                self.get_node(node_id).is_some()
                    && self.get_node_subgraph(node_id) == Some(subgraph.id.as_str())
            })
        {
            return None;
        }

        let mut entries = Vec::new();
        let mut exits = Vec::new();
        for edge in &self.edges {
            if edge.is_back_edge || edge.kind != EdgeKind::Arrow || edge.label.is_some() {
                continue;
            }
            let (exit_subgraphs, enter_subgraphs) =
                self.edge_boundary_crossings(&edge.from, &edge.to);
            if self.get_node_subgraph(&edge.from).is_none()
                && self.get_node_subgraph(&edge.to) == Some(subgraph.id.as_str())
                && exit_subgraphs.is_empty()
                && enter_subgraphs == vec![subgraph.id.as_str()]
            {
                entries.push((edge.from.clone(), edge.to.clone()));
            }
            if self.get_node_subgraph(&edge.from) == Some(subgraph.id.as_str())
                && self.get_node_subgraph(&edge.to).is_none()
                && exit_subgraphs == vec![subgraph.id.as_str()]
                && enter_subgraphs.is_empty()
            {
                exits.push((edge.from.clone(), edge.to.clone()));
            }
        }
        if entries.len() != 1 || exits.len() != 1 || entries[0].0 == exits[0].1 {
            return None;
        }

        let (entry_external, entry_internal) = entries.pop()?;
        let (exit_internal, exit_external) = exits.pop()?;
        if entry_internal == exit_internal {
            return None;
        }

        let branch_nodes: Vec<String> = subgraph
            .node_ids
            .iter()
            .filter(|node_id| *node_id != &entry_internal && *node_id != &exit_internal)
            .cloned()
            .collect();
        if branch_nodes.len() != 2 {
            return None;
        }

        let internal_edges: Vec<_> = self
            .edges
            .iter()
            .filter(|edge| {
                !edge.is_back_edge
                    && edge.kind == EdgeKind::Arrow
                    && edge.label.is_none()
                    && subgraph.node_ids.contains(&edge.from)
                    && subgraph.node_ids.contains(&edge.to)
            })
            .collect();
        if internal_edges.len() != 4 {
            return None;
        }

        let actual_edges: HashSet<(String, String)> = internal_edges
            .iter()
            .map(|edge| (edge.from.clone(), edge.to.clone()))
            .collect();
        let expected_edges: HashSet<(String, String)> = branch_nodes
            .iter()
            .flat_map(|branch| {
                [
                    (entry_internal.clone(), branch.clone()),
                    (branch.clone(), exit_internal.clone()),
                ]
            })
            .collect();
        if actual_edges != expected_edges {
            return None;
        }

        Some((
            subgraph.id.clone(),
            entry_external,
            entry_internal,
            exit_internal,
            exit_external,
        ))
    }

    /// Return the exact flat two-sibling BT scene where the target's internal
    /// incoming edge and one direct sibling crossing must retain independent
    /// target-side arrowheads.  The match is intentionally structural and
    /// excludes labels, back edges, nested containers, unsupported edge kinds,
    /// and extra branches.
    pub(crate) fn bt_sibling_target_entry_scene(&self) -> Option<BtSiblingTargetEntryScene> {
        if self.direction != Direction::BT
            || self.subgraphs.len() != 2
            || self.nodes.len() != 4
            || self.edges.len() != 4
            || self.has_cycles()
        {
            return None;
        }

        let subgraphs = self.subgraphs.iter().collect::<Vec<_>>();
        if subgraphs.iter().any(|subgraph| {
            !subgraph.bounds.is_valid()
                || subgraph.parent_id.is_some()
                || !subgraph.child_ids.is_empty()
                || subgraph.title.is_none()
                || subgraph.node_ids.len() != 2
                || !subgraph.node_ids.iter().all(|node_id| {
                    self.get_node(node_id).is_some()
                        && self.get_node_subgraph(node_id) == Some(subgraph.id.as_str())
                })
        }) {
            return None;
        }
        if subgraphs[0].bounds.y == subgraphs[1].bounds.y {
            return None;
        }

        let ordinary_edges: Vec<(usize, &Edge)> = self
            .edges
            .iter()
            .enumerate()
            .filter(|(_, edge)| {
                !edge.is_back_edge && edge.kind == EdgeKind::Arrow && edge.label.is_none()
            })
            .collect();
        if ordinary_edges.len() != 4 {
            return None;
        }
        if self
            .nodes
            .iter()
            .any(|node| node.shape != NodeShape::Rectangle)
        {
            return None;
        }

        let source_subgraph = subgraphs.iter().find(|subgraph| {
            subgraph.bounds.y > subgraphs[0].bounds.y.min(subgraphs[1].bounds.y)
        })?;
        let target_subgraph = subgraphs
            .iter()
            .find(|subgraph| subgraph.id != source_subgraph.id)?;
        if source_subgraph.bounds.y <= target_subgraph.bounds.y {
            return None;
        }

        let internal_edge = |subgraph: &Subgraph| {
            ordinary_edges
                .iter()
                .filter(|(_, edge)| {
                    subgraph.node_ids.contains(&edge.from) && subgraph.node_ids.contains(&edge.to)
                })
                .copied()
                .collect::<Vec<_>>()
        };
        let source_internal = internal_edge(source_subgraph);
        let target_internal = internal_edge(target_subgraph);
        if source_internal.len() != 1 || target_internal.len() != 1 {
            return None;
        }

        let source_lower_node_id = source_internal[0].1.from.clone();
        let source_upper_node_id = source_internal[0].1.to.clone();
        let target_lower_node_id = target_internal[0].1.from.clone();
        let target_upper_node_id = target_internal[0].1.to.clone();
        let cross_edges = ordinary_edges
            .iter()
            .filter(|(_, edge)| {
                source_subgraph.node_ids.contains(&edge.from)
                    && target_subgraph.node_ids.contains(&edge.to)
            })
            .copied()
            .collect::<Vec<_>>();
        if cross_edges.len() != 2 {
            return None;
        }

        let lower_cross = cross_edges.iter().find(|(_, edge)| {
            edge.from == source_lower_node_id && edge.to == target_lower_node_id
        })?;
        let upper_cross = cross_edges.iter().find(|(_, edge)| {
            edge.from == source_upper_node_id && edge.to == target_upper_node_id
        })?;
        Some(BtSiblingTargetEntryScene {
            source_subgraph_id: source_subgraph.id.clone(),
            target_subgraph_id: target_subgraph.id.clone(),
            source_lower_node_id,
            source_upper_node_id,
            target_lower_node_id,
            target_upper_node_id,
            source_internal_edge_index: source_internal[0].0,
            target_internal_edge_index: target_internal[0].0,
            lower_cross_edge_index: lower_cross.0,
            upper_cross_edge_index: upper_cross.0,
        })
    }

    /// Return the exact flat three-rail BT sibling scene whose existing portal
    /// slots should receive directional border seams in final projection.
    ///
    /// This intentionally excludes crossed two-rail pairs, internal edges,
    /// labels, nested containers, non-rectangle nodes, duplicate endpoints,
    /// cycles, and every extra graph edge. The route planner remains the owner
    /// of the three lanes; this selector only identifies their boundaries.
    pub(crate) fn bt_direct_parallel_sibling_scene(&self) -> Option<BtDirectParallelSiblingScene> {
        if self.direction != Direction::BT
            || self.subgraphs.len() != 2
            || self.nodes.len() != 6
            || self.edges.len() != 3
            || self.has_cycles()
        {
            return None;
        }

        let subgraphs = self
            .subgraphs
            .iter()
            .filter(|subgraph| {
                subgraph.parent_id.is_none()
                    && subgraph.child_ids.is_empty()
                    && subgraph
                        .title
                        .as_deref()
                        .is_some_and(|title| !title.is_empty())
                    && subgraph.bounds.is_valid()
                    && subgraph.node_ids.len() == 3
                    && subgraph.node_ids.iter().all(|node_id| {
                        self.get_node(node_id).is_some()
                            && self.get_node_subgraph(node_id) == Some(subgraph.id.as_str())
                    })
            })
            .collect::<Vec<_>>();
        if subgraphs.len() != 2 || subgraphs[0].bounds.y == subgraphs[1].bounds.y {
            return None;
        }

        if self
            .nodes
            .iter()
            .any(|node| node.shape != NodeShape::Rectangle)
        {
            return None;
        }

        let (source_subgraph, target_subgraph) = if subgraphs[0].bounds.y > subgraphs[1].bounds.y {
            (subgraphs[0], subgraphs[1])
        } else {
            (subgraphs[1], subgraphs[0])
        };
        if target_subgraph
            .bounds
            .y
            .saturating_add(target_subgraph.bounds.height)
            > source_subgraph.bounds.y
        {
            return None;
        }

        let source_node_ids: HashSet<&str> = source_subgraph
            .node_ids
            .iter()
            .map(String::as_str)
            .collect();
        let target_node_ids: HashSet<&str> = target_subgraph
            .node_ids
            .iter()
            .map(String::as_str)
            .collect();
        if source_node_ids.len() != 3
            || target_node_ids.len() != 3
            || source_node_ids
                .intersection(&target_node_ids)
                .next()
                .is_some()
            || self.nodes.iter().any(|node| {
                !source_node_ids.contains(node.id.as_str())
                    && !target_node_ids.contains(node.id.as_str())
            })
        {
            return None;
        }

        let mut source_endpoints = HashSet::new();
        let mut target_endpoints = HashSet::new();
        let mut edge_indices = Vec::with_capacity(self.edges.len());
        for (index, edge) in self.edges.iter().enumerate() {
            if edge.is_back_edge || edge.kind != EdgeKind::Arrow || edge.label.is_some() {
                return None;
            }
            let (exits, enters) = self.edge_boundary_crossings(&edge.from, &edge.to);
            if exits != vec![source_subgraph.id.as_str()]
                || enters != vec![target_subgraph.id.as_str()]
                || !source_node_ids.contains(edge.from.as_str())
                || !target_node_ids.contains(edge.to.as_str())
                || !source_endpoints.insert(edge.from.as_str())
                || !target_endpoints.insert(edge.to.as_str())
            {
                return None;
            }
            edge_indices.push(index);
        }
        if source_endpoints != source_node_ids || target_endpoints != target_node_ids {
            return None;
        }

        Some(BtDirectParallelSiblingScene {
            source_subgraph_id: source_subgraph.id.clone(),
            target_subgraph_id: target_subgraph.id.clone(),
            edge_indices,
        })
    }

    /// Return the exact two-subgraph BT scene whose target receiver lane may
    /// be staged beside an external sibling corridor. The match is structural
    /// and deliberately excludes labels, unsupported edge kinds, nested
    /// subgraphs, cycles, extra branches, and the narrower four-node sibling
    /// target-entry scene.
    pub(crate) fn bt_external_side_receiver_scene(&self) -> Option<BtExternalSideReceiverScene> {
        if self.direction != Direction::BT
            || self.subgraphs.len() != 2
            || self.nodes.len() != 6
            || self.edges.len() != 6
        {
            return None;
        }

        let subgraphs = self
            .subgraphs
            .iter()
            .filter(|subgraph| {
                subgraph.parent_id.is_none()
                    && subgraph.child_ids.is_empty()
                    && subgraph.title.is_some()
                    && subgraph.node_ids.len() == 2
                    && subgraph.node_ids.iter().all(|node_id| {
                        self.get_node(node_id).is_some()
                            && self.get_node_subgraph(node_id) == Some(subgraph.id.as_str())
                    })
            })
            .collect::<Vec<_>>();
        if subgraphs.len() != 2 {
            return None;
        }

        let ordinary_edges = self
            .edges
            .iter()
            .filter(|edge| {
                !edge.is_back_edge && edge.kind == EdgeKind::Arrow && edge.label.is_none()
            })
            .collect::<Vec<_>>();
        if ordinary_edges.len() != self.edges.len() {
            return None;
        }
        let internal_edge_count = |subgraph: &Subgraph| {
            ordinary_edges
                .iter()
                .filter(|edge| {
                    subgraph.node_ids.contains(&edge.from) && subgraph.node_ids.contains(&edge.to)
                })
                .count()
        };
        let (source_subgraph, target_subgraph) = match (
            internal_edge_count(subgraphs[0]),
            internal_edge_count(subgraphs[1]),
        ) {
            (1, 0) => (subgraphs[0], subgraphs[1]),
            (0, 1) => (subgraphs[1], subgraphs[0]),
            _ => return None,
        };

        let source_ids: HashSet<&str> = source_subgraph
            .node_ids
            .iter()
            .map(String::as_str)
            .collect();
        let target_ids: HashSet<&str> = target_subgraph
            .node_ids
            .iter()
            .map(String::as_str)
            .collect();
        let external_ids: HashSet<&str> = self
            .nodes
            .iter()
            .filter(|node| self.get_node_subgraph(&node.id).is_none())
            .map(|node| node.id.as_str())
            .collect();
        if source_ids.len() != 2 || target_ids.len() != 2 || external_ids.len() != 2 {
            return None;
        }

        let source_internal = ordinary_edges
            .iter()
            .filter(|edge| {
                source_ids.contains(edge.from.as_str()) && source_ids.contains(edge.to.as_str())
            })
            .count();
        let target_internal = ordinary_edges
            .iter()
            .filter(|edge| {
                target_ids.contains(edge.from.as_str()) && target_ids.contains(edge.to.as_str())
            })
            .count();
        let cross_edges = ordinary_edges
            .iter()
            .filter(|edge| {
                source_ids.contains(edge.from.as_str()) && target_ids.contains(edge.to.as_str())
            })
            .collect::<Vec<_>>();
        let entry_edges = ordinary_edges
            .iter()
            .filter(|edge| {
                external_ids.contains(edge.from.as_str()) && source_ids.contains(edge.to.as_str())
            })
            .collect::<Vec<_>>();
        let exit_edges = ordinary_edges
            .iter()
            .filter(|edge| {
                target_ids.contains(edge.from.as_str()) && external_ids.contains(edge.to.as_str())
            })
            .collect::<Vec<_>>();
        let cross_sources: HashSet<&str> =
            cross_edges.iter().map(|edge| edge.from.as_str()).collect();
        let cross_targets: HashSet<&str> =
            cross_edges.iter().map(|edge| edge.to.as_str()).collect();
        let exit_targets: HashSet<&str> = exit_edges.iter().map(|edge| edge.to.as_str()).collect();
        let source_external_node_id = entry_edges.first().map(|edge| edge.from.as_str());
        let sink_external_node_id = exit_edges.first().map(|edge| edge.to.as_str());
        if source_internal != 1
            || target_internal != 0
            || cross_edges.len() != 2
            || cross_sources.len() != 2
            || cross_targets.len() != 2
            || entry_edges.len() != 1
            || exit_edges.len() != 2
            || exit_targets.len() != 1
            || source_external_node_id.is_none()
            || sink_external_node_id.is_none()
            || source_external_node_id == sink_external_node_id
        {
            return None;
        }

        Some(BtExternalSideReceiverScene {
            source_subgraph_id: source_subgraph.id.clone(),
            target_subgraph_id: target_subgraph.id.clone(),
            source_external_node_id: source_external_node_id?.to_owned(),
            source_receiver_node_id: entry_edges.first()?.to.clone(),
            sink_external_node_id: sink_external_node_id?.to_owned(),
        })
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

    #[test]
    #[cfg(feature = "maintainer-fixtures")]
    fn bt_sibling_target_entry_selector_matches_only_the_typed_scene() {
        let mut graph = crate::parser::parse(
            include_str!("../tests/fixtures/inputs/collision_sibling_subgraphs_bt.md"),
            false,
        )
        .expect("parse BT sibling target fixture")
        .graph;
        for subgraph in &mut graph.subgraphs {
            let y = if subgraph.id == "Left" { 20 } else { 0 };
            subgraph.bounds = Rectangle::new(0, y, 30, 18);
        }

        let scene = graph
            .bt_sibling_target_entry_scene()
            .expect("fixture should match the exact scene selector");
        assert_eq!(scene.source_subgraph_id, "Left");
        assert_eq!(scene.target_subgraph_id, "Right");
        assert_eq!(scene.source_lower_node_id, "A");
        assert_eq!(scene.source_upper_node_id, "B");
        assert_eq!(scene.target_lower_node_id, "C");
        assert_eq!(scene.target_upper_node_id, "D");
        assert_eq!(
            [
                scene.source_internal_edge_index,
                scene.target_internal_edge_index,
                scene.lower_cross_edge_index,
                scene.upper_cross_edge_index,
            ],
            [0, 1, 2, 3]
        );
    }

    #[test]
    #[cfg(feature = "maintainer-fixtures")]
    fn bt_sibling_target_entry_selector_rejects_labels_and_other_directions() {
        let mut graph = crate::parser::parse(
            include_str!("../tests/fixtures/inputs/collision_sibling_subgraphs_bt.md"),
            false,
        )
        .expect("parse BT sibling target fixture")
        .graph;
        for subgraph in &mut graph.subgraphs {
            let y = if subgraph.id == "Left" { 20 } else { 0 };
            subgraph.bounds = Rectangle::new(0, y, 30, 18);
        }

        graph.edges[2].label = Some("crossing".to_owned());
        assert!(graph.bt_sibling_target_entry_scene().is_none());

        graph.edges[2].label = None;
        graph.direction = Direction::TD;
        assert!(graph.bt_sibling_target_entry_scene().is_none());
    }

    #[test]
    #[cfg(feature = "maintainer-fixtures")]
    fn bt_direct_parallel_sibling_selector_matches_three_pairwise_rails() {
        let mut graph = crate::parser::parse(
            include_str!("../tests/fixtures/inputs/collision_parallel_edges_bt.md"),
            false,
        )
        .expect("parse direct BT parallel fixture")
        .graph;
        for subgraph in &mut graph.subgraphs {
            let y = if subgraph.id == "SG1" { 20 } else { 0 };
            subgraph.bounds = Rectangle::new(0, y, 40, 18);
        }

        let scene = graph
            .bt_direct_parallel_sibling_scene()
            .expect("three pairwise BT rails should match the exact selector");
        assert_eq!(scene.source_subgraph_id, "SG1");
        assert_eq!(scene.target_subgraph_id, "SG2");
        assert_eq!(scene.edge_indices, vec![0, 1, 2]);
    }

    #[test]
    #[cfg(feature = "maintainer-fixtures")]
    fn bt_direct_parallel_sibling_selector_rejects_crossed_two_rail_control() {
        let mut graph = crate::parser::parse(
            include_str!("../tests/fixtures/inputs/collision_parallel_cross_bt.md"),
            false,
        )
        .expect("parse crossed BT parallel fixture")
        .graph;
        for subgraph in &mut graph.subgraphs {
            let y = if subgraph.id == "SG1" { 20 } else { 0 };
            subgraph.bounds = Rectangle::new(0, y, 40, 18);
        }

        assert!(graph.bt_direct_parallel_sibling_scene().is_none());

        graph.edges[0].label = Some("crossing".to_owned());
        assert!(graph.bt_direct_parallel_sibling_scene().is_none());
    }

    #[test]
    #[cfg(feature = "maintainer-fixtures")]
    fn bt_external_side_receiver_selector_matches_only_the_complex_scene() {
        let graph = crate::layout::coarse_waterfall(
            crate::parser::parse(
                include_str!("../tests/fixtures/inputs/subgraph_complex_bt.md"),
                false,
            )
            .expect("parse complex BT fixture")
            .graph,
        )
        .expect("layout complex BT fixture");

        let scene = graph
            .bt_external_side_receiver_scene()
            .expect("complex BT fixture should match the exact receiver scene");
        assert_eq!(scene.source_subgraph_id, "SG1");
        assert_eq!(scene.target_subgraph_id, "SG2");
        assert_eq!(scene.source_external_node_id, "API");
        assert_eq!(scene.source_receiver_node_id, "S1");
        assert_eq!(scene.sink_external_node_id, "Response");

        let mut labeled = graph.clone();
        labeled.edges[3].label = Some("crossing".to_owned());
        assert!(labeled.bt_external_side_receiver_scene().is_none());

        let mut wrong_direction = graph;
        wrong_direction.direction = Direction::TD;
        assert!(wrong_direction.bt_external_side_receiver_scene().is_none());
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
    fn subgraph_title_text_span_excludes_wrapper_padding() {
        assert_eq!(
            subgraph_title_span(0, 22, "Transform Stage", Direction::TD),
            Some((2, 18))
        );
        assert_eq!(
            subgraph_title_text_span(0, 22, "Transform Stage", Direction::TD),
            Some((3, 17))
        );
    }

    #[test]
    fn padded_subgraph_title_span_keeps_a_two_sided_visual_gutter() {
        assert_eq!(
            subgraph_title_text_with_padding("Transform Stage", 1),
            "  Transform Stage  "
        );
        assert_eq!(
            subgraph_title_span_with_padding(0, 22, "Transform Stage", Direction::TD, 1),
            Some((2, 20))
        );
        assert_eq!(
            subgraph_title_text_span_with_padding(0, 22, "Transform Stage", Direction::TD, 1),
            Some((4, 18))
        );
    }

    #[test]
    fn side_aware_title_padding_preserves_the_anchor_and_wall_gutter() {
        assert_eq!(
            subgraph_title_text_with_padding_sides("Group 3", 0, 1),
            " Group 3  "
        );
        assert_eq!(
            subgraph_title_span_with_padding_sides(0, 14, "Group 3", Direction::TD, 0, 1),
            Some((2, 11))
        );
        assert_eq!(
            subgraph_title_text_span_with_padding_sides(0, 14, "Group 3", Direction::TD, 0, 1),
            Some((3, 9))
        );
    }

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
    fn incoming_edge_clearance_is_direction_aware_for_flag() {
        assert_eq!(
            NodeShape::Asymmetric.incoming_edge_clearance(Direction::LR),
            1
        );
        assert_eq!(
            NodeShape::Asymmetric.incoming_edge_clearance(Direction::RL),
            0
        );
        assert_eq!(
            NodeShape::Asymmetric.incoming_edge_clearance(Direction::TD),
            0
        );
        assert_eq!(
            NodeShape::Asymmetric.incoming_edge_clearance(Direction::BT),
            0
        );
        assert_eq!(NodeShape::Diamond.incoming_edge_clearance(Direction::RL), 1);
        for direction in [
            Direction::TD,
            Direction::TB,
            Direction::BT,
            Direction::LR,
            Direction::RL,
        ] {
            assert_eq!(
                NodeShape::Database.incoming_edge_clearance(direction),
                0,
                "database entry must use the generic terminal clearance for {direction:?}"
            );
        }
        assert_eq!(
            NodeShape::Rectangle.incoming_edge_clearance(Direction::LR),
            0
        );
    }

    #[test]
    fn node_shape_default_is_rectangle() {
        assert_eq!(NodeShape::default(), NodeShape::Rectangle);
    }
}
