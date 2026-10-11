//! Parallel edges of one kind between the same two nodes share a single drawn
//! line. Layout and render work on a copy that holds one edge per line, its
//! distinct labels joined, so the line is routed once and no label is drawn
//! over another. The caller gets its original edges back, each pointing at
//! the shared route.

use std::collections::HashMap;

use crate::graph::{EdgeKind, Graph};
use crate::render::RenderOutcome;

const LABEL_SEPARATOR: &str = ", ";

/// How the original edges map onto the merged lines.
pub(crate) struct ParallelEdgeMerge {
    original: Vec<crate::graph::Edge>,
    /// Merged line index for each original edge.
    line_of: Vec<usize>,
    /// First original edge index for each merged line.
    first_of: Vec<usize>,
}

/// Merge parallel same-kind edges. Returns `None` for the merge record when
/// the graph has none, leaving it untouched.
pub(crate) fn merge(mut graph: Graph) -> (Graph, Option<ParallelEdgeMerge>) {
    let mut lines_by_pair: HashMap<(&str, &str), Vec<(EdgeKind, usize)>> = HashMap::new();
    let mut line_of = Vec::with_capacity(graph.edges.len());
    let mut first_of = Vec::new();
    let mut labels: Vec<Vec<&str>> = Vec::new();
    for (index, edge) in graph.edges.iter().enumerate() {
        let pair = lines_by_pair
            .entry((edge.from.as_str(), edge.to.as_str()))
            .or_default();
        let line = match pair.iter().find(|(kind, _)| *kind == edge.kind) {
            Some(&(_, line)) => line,
            None => {
                first_of.push(index);
                labels.push(Vec::new());
                pair.push((edge.kind, first_of.len() - 1));
                first_of.len() - 1
            }
        };
        line_of.push(line);
        if let Some(label) = edge.label.as_deref() {
            if !labels[line].contains(&label) {
                labels[line].push(label);
            }
        }
    }
    if first_of.len() == graph.edges.len() {
        return (graph, None);
    }

    let joined: Vec<Option<String>> = labels
        .iter()
        .map(|parts| (!parts.is_empty()).then(|| parts.join(LABEL_SEPARATOR)))
        .collect();
    let original = std::mem::take(&mut graph.edges);
    graph.edges = first_of
        .iter()
        .zip(joined)
        .map(|(&index, label)| crate::graph::Edge {
            label,
            ..original[index].clone()
        })
        .collect();
    graph.edge_routes.clear();

    let merge = ParallelEdgeMerge {
        original,
        line_of,
        first_of,
    };
    (graph, Some(merge))
}

impl ParallelEdgeMerge {
    /// Put the original edges back on the laid-out graph and name each line
    /// in the outcome by its first original edge.
    pub(crate) fn restore(self, graph: &mut Graph, outcome: &mut RenderOutcome) {
        let mut routes = HashMap::new();
        for (index, &line) in self.line_of.iter().enumerate() {
            if let Some(route) = graph.edge_routes.get(&line) {
                routes.insert(index, route.clone());
            }
        }
        graph.edges = self
            .original
            .into_iter()
            .zip(&self.line_of)
            .map(|(mut edge, &line)| {
                edge.is_back_edge = graph.edges[line].is_back_edge;
                edge
            })
            .collect();
        graph.edge_routes = routes;

        let rename = |id: &mut String| rename_owner(id, &self.first_of);
        for frame in [
            &mut outcome.semantic_frame,
            &mut outcome.display_semantic_frame,
        ] {
            frame
                .cells
                .iter_mut()
                .filter_map(|cell| cell.owner_id.as_mut())
                .for_each(rename);
        }
        for finding in &mut outcome.critic_report.findings {
            finding.owner_ids.iter_mut().for_each(rename);
        }
        let trace = &mut outcome.portal_trace;
        trace
            .boundaries
            .iter_mut()
            .for_each(|b| rename(&mut b.edge_id));
        trace
            .cells
            .iter_mut()
            .filter_map(|c| c.owner_id.as_mut())
            .for_each(rename);
        for route in &mut trace.fallback_routes {
            rename(&mut route.owner_id);
            route.covered_edge_ids.iter_mut().for_each(rename);
            route
                .cells
                .iter_mut()
                .filter_map(|c| c.owner_id.as_mut())
                .for_each(rename);
            for decision in &mut route.entry_decisions {
                rename(&mut decision.edge_id);
                rename(&mut decision.owner_id);
            }
        }
        for rejection in &mut trace.fallback_route_rejections {
            rename(&mut rejection.owner_id);
        }
    }
}

/// Rewrite `edge:<line>:<from>-><to>` to name the line's first original edge.
fn rename_owner(id: &mut String, first_of: &[usize]) {
    let Some(rest) = id.strip_prefix("edge:") else {
        return;
    };
    let Some((line, tail)) = rest.split_once(':') else {
        return;
    };
    if let Some(&index) = line
        .parse::<usize>()
        .ok()
        .and_then(|line| first_of.get(line))
    {
        *id = format!("edge:{index}:{tail}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Edge;

    fn edge(from: &str, to: &str, label: Option<&str>, kind: EdgeKind) -> Edge {
        Edge {
            from: from.into(),
            to: to.into(),
            label: label.map(str::to_owned),
            is_back_edge: false,
            kind,
        }
    }

    #[test]
    fn merges_same_kind_pairs_and_joins_distinct_labels() {
        let mut graph = Graph::new();
        graph.edges = vec![
            edge("A", "B", None, EdgeKind::Arrow),
            edge("A", "C", None, EdgeKind::Arrow),
            edge("A", "B", Some("one"), EdgeKind::Arrow),
            edge("A", "B", Some("one"), EdgeKind::Arrow),
            edge("A", "B", Some("two"), EdgeKind::Dotted),
            edge("A", "B", Some("three"), EdgeKind::Arrow),
        ];
        let (merged, record) = merge(graph);
        let record = record.expect("parallel edges merge");
        let lines: Vec<_> = merged
            .edges
            .iter()
            .map(|e| (e.to.as_str(), e.kind, e.label.as_deref()))
            .collect();
        assert_eq!(
            lines,
            vec![
                ("B", EdgeKind::Arrow, Some("one, three")),
                ("C", EdgeKind::Arrow, None),
                ("B", EdgeKind::Dotted, Some("two")),
            ]
        );
        assert_eq!(record.line_of, vec![0, 1, 0, 0, 2, 0]);
        assert_eq!(record.first_of, vec![0, 1, 4]);

        let mut id = "edge:2:A->B".to_owned();
        rename_owner(&mut id, &record.first_of);
        assert_eq!(id, "edge:4:A->B");
    }

    #[test]
    fn leaves_graphs_without_parallel_edges_alone() {
        let mut graph = Graph::new();
        graph.edges = vec![
            edge("A", "B", None, EdgeKind::Arrow),
            edge("B", "A", None, EdgeKind::Arrow),
        ];
        assert!(merge(graph).1.is_none());
    }
}
