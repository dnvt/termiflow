//! Final-frame provenance stamping for semantic analysis and repair.

use std::collections::HashMap;

use crate::geom::Segment;
use crate::graph::{Direction, Graph, Node};
use crate::portals::PortalSlots;
use crate::style::{StyleChars, BOX_HEIGHT};

use super::canvas;
use super::canvas::Canvas;
use super::semantic::{CellOwnerKind, CellRole};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EdgeLabelPlacement {
    pub owner_id: String,
    pub cells: Vec<(usize, usize)>,
}

pub fn edge_owner_id(edge_idx: usize, edge: &crate::graph::Edge) -> String {
    format!("edge:{edge_idx}:{}->{}", edge.from, edge.to)
}

pub fn refresh_provenance(
    canvas: &mut Canvas,
    graph: &Graph,
    chars: &StyleChars,
    portal_slots: &HashMap<String, PortalSlots>,
    direction: Direction,
    edge_label_placements: &[EdgeLabelPlacement],
) {
    let preserved_edge_meta = canvas.explicit_edge_meta();
    canvas.refresh_inferred_meta();
    for (x, y, meta) in preserved_edge_meta {
        if canvas.get(x, y) == meta.ch {
            canvas.set_meta_only(
                x,
                y,
                meta.owner_kind,
                meta.owner_id.as_deref(),
                meta.role,
                meta.z_index,
            );
        }
    }

    for subgraph in &graph.subgraphs {
        annotate_subgraph_region(canvas, subgraph, direction);
    }
    for node in &graph.nodes {
        annotate_node_region(canvas, node, chars);
    }
    annotate_edge_routes(canvas, graph, chars);
    annotate_edge_labels(canvas, edge_label_placements);
    annotate_portal_openings(canvas, graph, portal_slots);
}

fn annotate_subgraph_region(
    canvas: &mut Canvas,
    subgraph: &crate::graph::Subgraph,
    direction: Direction,
) {
    let bounds = &subgraph.bounds;
    if !bounds.is_valid() {
        return;
    }

    let x0 = bounds.x;
    let x1 = bounds.x + bounds.width.saturating_sub(1);
    let y0 = bounds.y;
    let y1 = bounds.y + bounds.height.saturating_sub(1);

    for x in x0..=x1 {
        canvas.set_meta_only(
            x,
            y0,
            CellOwnerKind::SubgraphBorder,
            Some(&subgraph.id),
            CellRole::Border,
            1,
        );
        canvas.set_meta_only(
            x,
            y1,
            CellOwnerKind::SubgraphBorder,
            Some(&subgraph.id),
            CellRole::Border,
            1,
        );
    }
    for y in y0..=y1 {
        canvas.set_meta_only(
            x0,
            y,
            CellOwnerKind::SubgraphBorder,
            Some(&subgraph.id),
            CellRole::Border,
            1,
        );
        canvas.set_meta_only(
            x1,
            y,
            CellOwnerKind::SubgraphBorder,
            Some(&subgraph.id),
            CellRole::Border,
            1,
        );
    }

    if let Some(title) = subgraph.title.as_deref() {
        let title_fmt = crate::graph::subgraph_title_text(title);
        if let Some(start_x) =
            crate::graph::subgraph_title_start_x(bounds.x, bounds.width, title, direction)
        {
            let title_y = super::subgraph_title_y(bounds, direction);
            for (i, _) in title_fmt.chars().enumerate() {
                let x = start_x + i;
                if x < canvas.width {
                    canvas.set_meta_only(
                        x,
                        title_y,
                        CellOwnerKind::SubgraphTitle,
                        Some(&subgraph.id),
                        CellRole::Text,
                        2,
                    );
                }
            }
        }
    }
}

fn annotate_node_region(canvas: &mut Canvas, node: &Node, chars: &StyleChars) {
    for y in node.y..node.y + node.height.max(BOX_HEIGHT) {
        for x in node.x..node.x + node.width {
            if x >= canvas.width || y >= canvas.height {
                continue;
            }
            if matches!(
                canvas.get_meta(x, y).map(|meta| meta.owner_kind),
                Some(CellOwnerKind::SubgraphTitle | CellOwnerKind::PortalOpening)
            ) {
                continue;
            }
            let ch = canvas.get(x, y);
            let (owner_kind, role) = if ch == ' ' {
                (CellOwnerKind::NodeFill, CellRole::Fill)
            } else if canvas::is_horizontal(ch, chars)
                || canvas::is_vertical(ch, chars)
                || canvas::is_junction(ch, chars)
                || canvas::is_corner(ch, chars)
                || matches!(ch, '(' | ')' | '<' | '>' | '/' | '\\')
            {
                (CellOwnerKind::NodeBorder, CellRole::Border)
            } else {
                (CellOwnerKind::NodeLabel, CellRole::Text)
            };
            canvas.set_meta_only(x, y, owner_kind, Some(&node.id), role, 3);
        }
    }
}

fn annotate_portal_openings(
    canvas: &mut Canvas,
    graph: &Graph,
    portal_slots: &HashMap<String, PortalSlots>,
) {
    for subgraph in &graph.subgraphs {
        let Some(slots) = portal_slots.get(&subgraph.id) else {
            continue;
        };
        let bounds = &subgraph.bounds;
        for &x in &slots.top {
            annotate_portal_cell(canvas, x, bounds.y, &subgraph.id);
        }
        for &x in &slots.bottom {
            annotate_portal_cell(
                canvas,
                x,
                bounds.y + bounds.height.saturating_sub(1),
                &subgraph.id,
            );
        }
        for &y in &slots.left {
            annotate_portal_cell(canvas, bounds.x, y, &subgraph.id);
        }
        for &y in &slots.right {
            annotate_portal_cell(
                canvas,
                bounds.x + bounds.width.saturating_sub(1),
                y,
                &subgraph.id,
            );
        }
    }
}

fn annotate_portal_cell(canvas: &mut Canvas, x: usize, y: usize, owner_id: &str) {
    if x >= canvas.width || y >= canvas.height {
        return;
    }
    let ch = canvas.get(x, y);
    if ch != ' ' && super::is_textual(ch) {
        return;
    }
    if matches!(
        canvas.get_meta(x, y).map(|meta| meta.owner_kind),
        Some(
            CellOwnerKind::NodeBorder
                | CellOwnerKind::NodeFill
                | CellOwnerKind::NodeLabel
                | CellOwnerKind::SubgraphTitle
        )
    ) {
        return;
    }
    canvas.set_meta_only(
        x,
        y,
        CellOwnerKind::PortalOpening,
        Some(owner_id),
        CellRole::Portal,
        4,
    );
}

fn annotate_edge_routes(canvas: &mut Canvas, graph: &Graph, chars: &StyleChars) {
    let mut edge_ids: Vec<usize> = graph.edge_routes.keys().copied().collect();
    edge_ids.sort_unstable();

    for edge_idx in edge_ids {
        let Some(route) = graph.edge_routes.get(&edge_idx) else {
            continue;
        };
        let Some(edge) = graph.edges.get(edge_idx) else {
            continue;
        };
        let owner_id = edge_owner_id(edge_idx, edge);
        let owner_kind = if edge.is_back_edge {
            CellOwnerKind::CycleEdge
        } else {
            CellOwnerKind::EdgeSegment
        };

        for seg in &route.segments {
            annotate_segment(canvas, seg, chars, owner_kind, &owner_id);
        }

        for pair in route.segments.windows(2) {
            let turn = pair[0].to;
            annotate_edge_point(canvas, turn.x, turn.y, chars, owner_kind, &owner_id, true);
        }

        if let Some(last_seg) = route.segments.last() {
            annotate_arrow_tip(canvas, last_seg.to.x, last_seg.to.y, chars, &owner_id);
        }
    }
}

fn annotate_segment(
    canvas: &mut Canvas,
    seg: &Segment,
    chars: &StyleChars,
    owner_kind: CellOwnerKind,
    owner_id: &str,
) {
    if seg.from.x == seg.to.x {
        let x = seg.from.x;
        let (start, end) = if seg.from.y <= seg.to.y {
            (seg.from.y, seg.to.y)
        } else {
            (seg.to.y, seg.from.y)
        };
        for y in start..=end {
            annotate_edge_point(canvas, x, y, chars, owner_kind, owner_id, false);
        }
    } else if seg.from.y == seg.to.y {
        let y = seg.from.y;
        let (start, end) = if seg.from.x <= seg.to.x {
            (seg.from.x, seg.to.x)
        } else {
            (seg.to.x, seg.from.x)
        };
        for x in start..=end {
            annotate_edge_point(canvas, x, y, chars, owner_kind, owner_id, false);
        }
    }
}

fn annotate_edge_point(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    chars: &StyleChars,
    owner_kind: CellOwnerKind,
    owner_id: &str,
    allow_corner: bool,
) {
    if x >= canvas.width || y >= canvas.height {
        return;
    }

    let ch = canvas.get(x, y);
    let Some(existing) = canvas.get_meta(x, y).cloned() else {
        return;
    };

    if matches!(
        existing.owner_kind,
        CellOwnerKind::NodeLabel | CellOwnerKind::SubgraphTitle
    ) {
        return;
    }
    if matches!(
        existing.owner_kind,
        CellOwnerKind::NodeBorder | CellOwnerKind::SubgraphBorder
    ) && !canvas::is_arrow(ch)
        && !canvas::is_junction(ch, chars)
    {
        return;
    }

    let role = if canvas::is_arrow(ch) {
        CellRole::ArrowTip
    } else if canvas::is_junction(ch, chars) {
        CellRole::Junction
    } else if allow_corner && canvas::is_corner(ch, chars) {
        CellRole::Corner
    } else if canvas::is_horizontal(ch, chars) {
        CellRole::Horizontal
    } else if canvas::is_vertical(ch, chars) {
        CellRole::Vertical
    } else if canvas::is_corner(ch, chars) {
        CellRole::Corner
    } else {
        return;
    };

    let final_owner_kind = if role == CellRole::ArrowTip {
        CellOwnerKind::ArrowHead
    } else {
        owner_kind
    };

    canvas.set_meta_only(x, y, final_owner_kind, Some(owner_id), role, 5);
}

fn annotate_arrow_tip(canvas: &mut Canvas, x: usize, y: usize, chars: &StyleChars, owner_id: &str) {
    annotate_edge_point(
        canvas,
        x,
        y,
        chars,
        CellOwnerKind::ArrowHead,
        owner_id,
        true,
    );
}

fn annotate_edge_labels(canvas: &mut Canvas, placements: &[EdgeLabelPlacement]) {
    for placement in placements {
        for &(x, y) in &placement.cells {
            if x >= canvas.width || y >= canvas.height || canvas.get(x, y) == ' ' {
                continue;
            }
            canvas.set_meta_only(
                x,
                y,
                CellOwnerKind::EdgeLabel,
                Some(&placement.owner_id),
                CellRole::Text,
                6,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{EdgeRoute, Point};
    use crate::graph::{Direction, Edge, Graph, Node};
    use crate::style::{BaseStyle, CompositeStyle};

    fn unicode_chars() -> StyleChars {
        CompositeStyle::default().to_style_chars(BaseStyle::Unicode)
    }

    #[test]
    fn refresh_provenance_marks_edge_label_cells() {
        let chars = unicode_chars();
        let mut graph = Graph::new();
        graph.direction = Direction::LR;
        let mut a = Node::new("A", "A");
        a.x = 0;
        a.y = 0;
        a.width = 7;
        let mut b = Node::new("B", "B");
        b.x = 14;
        b.y = 0;
        b.width = 7;
        graph.add_node(a);
        graph.add_node(b);
        graph.add_edge(Edge::with_label("A", "B", "go"));

        let mut route = EdgeRoute::new();
        route.push_segment(Point::new(7, 1), Point::new(13, 1));
        graph.edge_routes.insert(0, route);

        let mut canvas = Canvas::new(24, 4);
        canvas.set(10, 1, 'g');
        canvas.set(11, 1, 'o');

        refresh_provenance(
            &mut canvas,
            &graph,
            &chars,
            &HashMap::new(),
            Direction::LR,
            &[EdgeLabelPlacement {
                owner_id: edge_owner_id(0, &graph.edges[0]),
                cells: vec![(10, 1), (11, 1)],
            }],
        );

        assert_eq!(
            canvas.get_meta(10, 1).map(|cell| cell.owner_kind),
            Some(CellOwnerKind::EdgeLabel)
        );
        assert_eq!(
            canvas
                .get_meta(11, 1)
                .and_then(|cell| cell.owner_id.clone()),
            Some(edge_owner_id(0, &graph.edges[0]))
        );
    }

    #[test]
    fn refresh_provenance_preserves_explicit_edge_owner_metadata() {
        let chars = unicode_chars();
        let mut graph = Graph::new();
        graph.direction = Direction::LR;

        let mut canvas = Canvas::new(4, 1);
        canvas.set_edge_char_owned(
            1,
            0,
            '─',
            &chars,
            CellOwnerKind::EdgeSegment,
            "edge:0:A->B",
            5,
        );

        refresh_provenance(
            &mut canvas,
            &graph,
            &chars,
            &HashMap::new(),
            Direction::LR,
            &[],
        );

        assert_eq!(
            canvas.get_meta(1, 0).and_then(|cell| cell.owner_id.clone()),
            Some("edge:0:A->B".to_string())
        );
        assert_eq!(
            canvas.get_meta(1, 0).map(|cell| cell.owner_kind),
            Some(CellOwnerKind::EdgeSegment)
        );
    }
}
