//! Read-only, indexed queries over the public compatibility graph.
//!
//! `Graph` intentionally remains a simple public vector-based data model. This
//! view pays the indexing cost once at a pipeline boundary so repeated render
//! queries do not repeatedly scan node and subgraph IDs. It never mutates the
//! source graph and preserves `Graph::get_node`'s first-match behavior for the
//! infallible constructor used by the renderer.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::graph::{Edge, Graph, Node, Subgraph};

/// Stable index of a node in the source graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct NodeId(usize);

impl NodeId {
    #[inline]
    pub(crate) const fn index(self) -> usize {
        self.0
    }
}

/// Stable index of an edge in the source graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct EdgeId(usize);

#[allow(dead_code)]
impl EdgeId {
    #[inline]
    pub(crate) const fn from_index(index: usize) -> Self {
        Self(index)
    }

    #[inline]
    pub(crate) const fn index(self) -> usize {
        self.0
    }
}

/// Stable index of a subgraph in the source graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct SubgraphId(usize);

impl SubgraphId {
    #[inline]
    pub(crate) const fn index(self) -> usize {
        self.0
    }
}

/// Subgraph boundaries crossed by an edge, ordered innermost to outermost.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct BoundaryCrossing {
    pub(crate) exiting: Vec<SubgraphId>,
    pub(crate) entering: Vec<SubgraphId>,
}

/// Errors returned by the strict index constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IndexedGraphError {
    DuplicateNodeId {
        id: String,
        first: usize,
        second: usize,
    },
    DuplicateSubgraphId {
        id: String,
        first: usize,
        second: usize,
    },
    SubgraphCycle {
        id: String,
    },
}

impl fmt::Display for IndexedGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateNodeId { id, first, second } => write!(
                f,
                "duplicate node id `{id}` at source indices {first} and {second}"
            ),
            Self::DuplicateSubgraphId { id, first, second } => write!(
                f,
                "duplicate subgraph id `{id}` at source indices {first} and {second}"
            ),
            Self::SubgraphCycle { id } => write!(f, "subgraph parent cycle includes `{id}`"),
        }
    }
}

impl std::error::Error for IndexedGraphError {}

/// Immutable query index borrowed from a [`Graph`].
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct IndexedGraph<'a> {
    graph: &'a Graph,
    node_by_id: HashMap<&'a str, NodeId>,
    subgraph_by_id: HashMap<&'a str, SubgraphId>,
    outgoing: Vec<Vec<EdgeId>>,
    incoming: Vec<Vec<EdgeId>>,
    node_ancestry: Vec<Vec<SubgraphId>>,
    subgraph_ancestry: Vec<Vec<SubgraphId>>,
    edge_boundaries: Vec<BoundaryCrossing>,
}

#[allow(dead_code)]
impl<'a> IndexedGraph<'a> {
    /// Build an index while preserving the source graph's first-match lookup.
    ///
    /// The renderer uses this constructor so malformed hand-built public
    /// graphs retain the historical behavior of `Graph::get_node`. Call
    /// [`Self::try_new`] at validated boundaries that require unique IDs.
    pub(crate) fn new(graph: &'a Graph) -> Self {
        Self::build(graph, false).expect("lossy indexed graph construction cannot fail")
    }

    /// Build an index and reject duplicate IDs or cyclic subgraph ancestry.
    pub(crate) fn try_new(graph: &'a Graph) -> Result<Self, IndexedGraphError> {
        Self::build(graph, true)
    }

    fn build(graph: &'a Graph, strict: bool) -> Result<Self, IndexedGraphError> {
        let mut node_by_id: HashMap<&'a str, NodeId> = HashMap::with_capacity(graph.nodes.len());
        for (index, node) in graph.nodes.iter().enumerate() {
            if let Some(existing) = node_by_id.get(node.id.as_str()).copied() {
                if strict {
                    return Err(IndexedGraphError::DuplicateNodeId {
                        id: node.id.clone(),
                        first: existing.index(),
                        second: index,
                    });
                }
                continue;
            }
            node_by_id.insert(node.id.as_str(), NodeId(index));
        }

        let mut subgraph_by_id: HashMap<&'a str, SubgraphId> =
            HashMap::with_capacity(graph.subgraphs.len());
        for (index, subgraph) in graph.subgraphs.iter().enumerate() {
            if let Some(existing) = subgraph_by_id.get(subgraph.id.as_str()).copied() {
                if strict {
                    return Err(IndexedGraphError::DuplicateSubgraphId {
                        id: subgraph.id.clone(),
                        first: existing.index(),
                        second: index,
                    });
                }
                continue;
            }
            subgraph_by_id.insert(subgraph.id.as_str(), SubgraphId(index));
        }

        let mut parent_ids = vec![None; graph.subgraphs.len()];
        for (index, subgraph) in graph.subgraphs.iter().enumerate() {
            parent_ids[index] = subgraph
                .parent_id
                .as_deref()
                .and_then(|parent| subgraph_by_id.get(parent).copied());
        }

        let mut subgraph_ancestry = Vec::with_capacity(graph.subgraphs.len());
        for (index, subgraph) in graph.subgraphs.iter().enumerate() {
            subgraph_ancestry.push(build_ancestry(
                SubgraphId(index),
                &parent_ids,
                graph,
                strict,
                subgraph,
            )?);
        }

        let mut node_ancestry = Vec::with_capacity(graph.nodes.len());
        for node in &graph.nodes {
            let direct = graph
                .node_subgraph
                .get(&node.id)
                .and_then(|id| subgraph_by_id.get(id.as_str()).copied());
            node_ancestry.push(build_ancestry_from_optional(
                direct,
                &parent_ids,
                graph,
                strict,
                node.id.as_str(),
            )?);
        }

        let mut outgoing = vec![Vec::new(); graph.nodes.len()];
        let mut incoming = vec![Vec::new(); graph.nodes.len()];
        for (index, edge) in graph.edges.iter().enumerate() {
            let edge_id = EdgeId(index);
            if let Some(node_id) = node_by_id.get(edge.from.as_str()).copied() {
                outgoing[node_id.index()].push(edge_id);
            }
            if let Some(node_id) = node_by_id.get(edge.to.as_str()).copied() {
                incoming[node_id.index()].push(edge_id);
            }
        }

        let edge_boundaries = graph
            .edges
            .iter()
            .map(|edge| {
                let from = node_by_id.get(edge.from.as_str()).copied();
                let to = node_by_id.get(edge.to.as_str()).copied();
                boundary_between(
                    from.and_then(|id| node_ancestry.get(id.index())),
                    to.and_then(|id| node_ancestry.get(id.index())),
                )
            })
            .collect();

        Ok(Self {
            graph,
            node_by_id,
            subgraph_by_id,
            outgoing,
            incoming,
            node_ancestry,
            subgraph_ancestry,
            edge_boundaries,
        })
    }

    #[inline]
    pub(crate) fn graph(&self) -> &'a Graph {
        self.graph
    }

    #[inline]
    pub(crate) fn node_id(&self, id: &str) -> Option<NodeId> {
        self.node_by_id.get(id).copied()
    }

    #[inline]
    pub(crate) fn subgraph_id(&self, id: &str) -> Option<SubgraphId> {
        self.subgraph_by_id.get(id).copied()
    }

    #[inline]
    pub(crate) fn node(&self, id: NodeId) -> Option<&'a Node> {
        self.graph.nodes.get(id.index())
    }

    #[inline]
    pub(crate) fn node_by_name(&self, id: &str) -> Option<&'a Node> {
        self.node_id(id).and_then(|node| self.node(node))
    }

    #[inline]
    pub(crate) fn edge(&self, id: EdgeId) -> Option<&'a Edge> {
        self.graph.edges.get(id.index())
    }

    #[inline]
    pub(crate) fn subgraph(&self, id: SubgraphId) -> Option<&'a Subgraph> {
        self.graph.subgraphs.get(id.index())
    }

    #[inline]
    pub(crate) fn outgoing(&self, node: NodeId) -> &[EdgeId] {
        self.outgoing
            .get(node.index())
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    #[inline]
    pub(crate) fn incoming(&self, node: NodeId) -> &[EdgeId] {
        self.incoming
            .get(node.index())
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    #[inline]
    pub(crate) fn node_ancestry(&self, node: NodeId) -> &[SubgraphId] {
        self.node_ancestry
            .get(node.index())
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    #[inline]
    pub(crate) fn subgraph_ancestry(&self, subgraph: SubgraphId) -> &[SubgraphId] {
        self.subgraph_ancestry
            .get(subgraph.index())
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    #[inline]
    pub(crate) fn edge_boundaries(&self, edge: EdgeId) -> Option<&BoundaryCrossing> {
        self.edge_boundaries.get(edge.index())
    }
}

fn build_ancestry(
    id: SubgraphId,
    parents: &[Option<SubgraphId>],
    graph: &Graph,
    strict: bool,
    subgraph: &Subgraph,
) -> Result<Vec<SubgraphId>, IndexedGraphError> {
    build_ancestry_from_optional(Some(id), parents, graph, strict, subgraph.id.as_str())
}

fn build_ancestry_from_optional(
    start: Option<SubgraphId>,
    parents: &[Option<SubgraphId>],
    graph: &Graph,
    strict: bool,
    context_id: &str,
) -> Result<Vec<SubgraphId>, IndexedGraphError> {
    let mut result = Vec::new();
    let mut current = start;
    let mut visited = HashSet::new();
    while let Some(id) = current {
        if !visited.insert(id) {
            if strict {
                let cycle_id = graph
                    .subgraphs
                    .get(id.index())
                    .map(|subgraph| subgraph.id.clone())
                    .unwrap_or_else(|| context_id.to_string());
                return Err(IndexedGraphError::SubgraphCycle { id: cycle_id });
            }
            break;
        }
        result.push(id);
        current = parents.get(id.index()).copied().flatten();
    }
    Ok(result)
}

fn boundary_between(
    from: Option<&Vec<SubgraphId>>,
    to: Option<&Vec<SubgraphId>>,
) -> BoundaryCrossing {
    let from = from.map(Vec::as_slice).unwrap_or_default();
    let to = to.map(Vec::as_slice).unwrap_or_default();
    let mut from_exclusive_len = from.len();
    let mut to_exclusive_len = to.len();
    while from_exclusive_len > 0
        && to_exclusive_len > 0
        && from[from_exclusive_len - 1] == to[to_exclusive_len - 1]
    {
        from_exclusive_len -= 1;
        to_exclusive_len -= 1;
    }
    BoundaryCrossing {
        exiting: from[..from_exclusive_len].to_vec(),
        entering: to[..to_exclusive_len].to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, Node, Subgraph};

    fn graph_with_nested_subgraphs() -> Graph {
        let mut graph = Graph::new();
        graph.add_node(Node::new("a", "A"));
        graph.add_node(Node::new("b", "B"));
        graph.add_node(Node::new("c", "C"));
        graph.add_edge(Edge::new("a", "b"));
        graph.add_edge(Edge::new("b", "c"));

        graph.add_subgraph(Subgraph::new("outer", Some("Outer".to_string())));
        graph.add_subgraph(Subgraph::new("inner", Some("Inner".to_string())));
        graph
            .get_subgraph_mut("inner")
            .expect("inner exists")
            .parent_id = Some("outer".to_string());
        graph.associate_node_with_subgraph("a", "inner");
        graph.associate_node_with_subgraph("b", "outer");
        graph
    }

    #[test]
    fn indexes_lookup_and_adjacency_without_scanning() {
        let graph = graph_with_nested_subgraphs();
        let index = IndexedGraph::try_new(&graph).expect("fixture IDs are unique");
        let a = index.node_id("a").expect("a is indexed");
        let b = index.node_id("b").expect("b is indexed");

        assert_eq!(index.node(a).map(|node| node.id.as_str()), Some("a"));
        assert_eq!(index.outgoing(a), &[EdgeId(0)]);
        assert_eq!(index.incoming(b), &[EdgeId(0)]);
        assert_eq!(
            index.edge(EdgeId(1)).map(|edge| edge.to.as_str()),
            Some("c")
        );
    }

    #[test]
    fn precomputes_nested_ancestry_and_boundaries() {
        let graph = graph_with_nested_subgraphs();
        let index = IndexedGraph::try_new(&graph).expect("fixture IDs are unique");
        let a = index.node_id("a").expect("a is indexed");
        let b = index.node_id("b").expect("b is indexed");
        let inner = index.subgraph_id("inner").expect("inner is indexed");
        let outer = index.subgraph_id("outer").expect("outer is indexed");

        assert_eq!(index.node_ancestry(a), &[inner, outer]);
        assert_eq!(index.node_ancestry(b), &[outer]);
        assert_eq!(index.subgraph_ancestry(inner), &[inner, outer]);
        let crossing = index.edge_boundaries(EdgeId(0)).expect("edge is indexed");
        assert_eq!(crossing.exiting, vec![inner]);
        assert!(crossing.entering.is_empty());
    }

    #[test]
    fn strict_index_rejects_duplicate_ids() {
        let mut graph = Graph::new();
        graph.nodes.push(Node::new("same", "one"));
        graph.nodes.push(Node::new("same", "two"));

        assert!(matches!(
            IndexedGraph::try_new(&graph),
            Err(IndexedGraphError::DuplicateNodeId { .. })
        ));
    }

    #[test]
    fn lossy_index_preserves_first_match_behavior() {
        let mut graph = Graph::new();
        graph.nodes.push(Node::new("same", "first"));
        graph.nodes.push(Node::new("same", "second"));
        let index = IndexedGraph::new(&graph);
        assert_eq!(
            index.node_by_name("same").map(|node| node.label.as_str()),
            Some("first")
        );
    }

    #[test]
    fn unknown_edge_endpoints_have_empty_adjacency_side() {
        let mut graph = Graph::new();
        graph.add_node(Node::new("a", "A"));
        graph.add_edge(Edge::new("a", "missing"));
        let index = IndexedGraph::try_new(&graph).expect("unknown endpoints are not duplicate IDs");
        let a = index.node_id("a").expect("a is indexed");
        assert_eq!(index.outgoing(a), &[EdgeId(0)]);
        assert!(index
            .edge_boundaries(EdgeId(0))
            .expect("edge is indexed")
            .entering
            .is_empty());
    }
}
