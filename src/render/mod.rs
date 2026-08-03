//! Render module - 2D character grid rendering for diagrams.
//!
//! This module handles the final rendering phase:
//! - Box drawing for nodes (9 shapes supported)
//! - Direction-agnostic edge routing (TD, LR, BT, RL)
//! - Junction/crossing detection for overlapping paths
//!
//! Rendering order: edges first, then boxes (boxes overwrite edge lines).
//!
//! # Module Structure
//!
//! - `canvas` - Canvas struct and character classification
//! - `contract` - Code-facing render-layer contract
//! - `edge` - Normal edge routing (all directions)
//! - `edge_policy` - Graph-aware route-entry policies
//! - `cycle` - Cycle/loop edge routing through gutters
//! - `trace` - Normalized geometry traces for non-glyph inspection
//! - `shapes` - Box drawing for all 9 node shapes

pub mod canvas;
pub mod contract;
pub mod critic;
pub mod cycle;
#[cfg(test)]
mod determinism;
pub mod edge;
mod edge_policy;
pub mod evidence;
mod labels;
mod outcome;
mod pipeline;
mod portal_projection;
mod portal_restore;
pub(crate) mod precomputed;
pub mod provenance;
pub mod repair;
pub(crate) mod scene;
pub mod semantic;
pub mod shapes;
pub mod topology;
pub mod trace;

// Re-exports
pub use canvas::Canvas;
pub use contract::{
    current_render_layer_contract, RenderLayer, RenderLayerContract, RenderLayerSpec,
};
pub use outcome::RenderOutcome;
pub use trace::{
    EdgeTrace, GeometryTrace, NodeTrace, RectTrace, SegmentAxis, SegmentTrace, SubgraphTrace,
};

use crate::config::Config;
use crate::graph::Graph;
#[cfg(test)]
use crate::graph::{Direction, Node};
use crate::runtime;
#[cfg(test)]
use crate::style::BaseStyle;
use anyhow::Result;

#[cfg(test)]
use labels::format_edge_label_with_limit;
#[cfg(all(test, feature = "maintainer-fixtures"))]
use portal_projection::title_span;
use portal_projection::{is_textual, subgraph_title_y};

// ============================================================================
// Main Render Function
// ============================================================================

/// Render a graph to a string.
///
/// This is the main entry point for the render module. It:
/// 1. Calculates canvas dimensions from node positions
/// 2. Draws all edges (sorted for optimal junction creation)
/// 3. Draws all boxes (overwriting any edge lines that pass through)
pub fn render(graph: &Graph, config: &Config) -> Result<String> {
    Ok(render_with_feedback(graph, config)?.output)
}
/// Render a graph and return semantic/critic details for the final frame.
pub fn render_with_feedback(graph: &Graph, config: &Config) -> Result<RenderOutcome> {
    runtime::with_captured(|| pipeline::render_with_feedback(graph, config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display_width;
    use crate::geom::{EdgeRoute, Segment};
    use crate::graph::Subgraph;
    use crate::CompositeStyle;
    use crate::Edge;

    #[test]
    fn precomputed_back_edge_renders_with_back_glyphs() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;

        let mut a = Node::new("A", "A");
        a.x = 0;
        a.y = 0;
        a.width = 5;

        let mut b = Node::new("B", "B");
        b.x = 8;
        b.y = 0;
        b.width = 5;

        graph.nodes.push(a);
        graph.nodes.push(b);

        let mut edge = Edge::new("B", "A");
        edge.is_back_edge = true;
        graph.edges.push(edge);

        let mut route = EdgeRoute::new();
        route.push_segment(
            crate::geom::Point::new(8 + 5, 1),
            crate::geom::Point::new(0, 1),
        );
        graph.edge_routes.insert(0, route);

        let config = Config::builder()
            .style(CompositeStyle::from_base(BaseStyle::Unicode))
            .crop(false)
            .build(&crate::parser::ParseConfig::default());

        let output = render(&graph, &config).expect("render back edge");

        // Unicode back edges use dotted style, ensure we see a back-edge glyph sequence.
        assert!(
            output.contains("⋯") || output.contains("┄") || output.contains('─'),
            "expected back-edge route to render with visible glyphs, got:\n{output}"
        );
    }

    #[test]
    fn diagonal_precomputed_route_falls_back_to_rendered_routing() {
        let mut graph = Graph::new();
        graph.direction = Direction::LR;

        let mut source = Node::new("A", "A");
        source.x = 0;
        source.y = 0;
        source.width = 5;

        let mut target = Node::new("B", "B");
        target.x = 12;
        target.y = 0;
        target.width = 5;

        graph.nodes.push(source);
        graph.nodes.push(target);
        graph.edges.push(Edge::new("A", "B"));
        graph.edge_routes.insert(
            0,
            EdgeRoute {
                segments: vec![Segment::new(
                    crate::geom::Point::new(5, 1),
                    crate::geom::Point::new(12, 3),
                )],
            },
        );

        let config = Config::builder()
            .style(CompositeStyle::from_base(BaseStyle::Unicode))
            .crop(false)
            .build(&crate::parser::ParseConfig::default());
        let output = render(&graph, &config).expect("render malformed route fallback");
        let chars = config.composite_style.to_style_chars(BaseStyle::Unicode);

        assert!(
            output.contains(chars.arrow_right),
            "invalid diagonal route should fall back to a visible routed arrow, got:\n{output}"
        );
    }

    fn char_at(output: &str, x: usize, y: usize) -> Option<char> {
        output.lines().nth(y).and_then(|line| line.chars().nth(x))
    }

    #[test]
    fn edge_label_truncation_preserves_grapheme_clusters() {
        let family = "👨‍👩‍👧‍👦";
        assert_eq!(
            format_edge_label_with_limit(&format!("{family}{family}"), display_width(family) + 1),
            format!("{family}…")
        );
    }

    #[test]
    fn edge_label_truncation_preserves_combining_clusters() {
        let accented = "e\u{301}";
        assert_eq!(
            format_edge_label_with_limit(&format!("{accented}{accented}{accented}"), 2),
            format!("{accented}…")
        );
    }

    #[test]
    fn cross_subgraph_edge_uses_side_aware_top_border_portal_td() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;

        let mut a = Node::new("A", "A");
        a.x = 2;
        a.y = 0;
        a.width = 6;

        let mut b = Node::new("B", "B");
        b.x = 6;
        b.y = 6;
        b.width = 6;

        graph.nodes.push(a);
        graph.nodes.push(b);
        graph.edges.push(Edge::new("A", "B"));

        let mut sg = Subgraph::new("sg", Some("Group".into()));
        sg.add_node("B");
        // Outer bounds with room for portals; inner bounds minimal
        sg.bounds = crate::graph::Rectangle::new(5, 4, 8, 6);
        sg.inner_bounds = crate::graph::Rectangle::new(5, 5, 8, 4);
        graph.add_subgraph(sg);
        graph.associate_node_with_subgraph("B", "sg");

        // Precompute a route that runs along the subgraph border then inside.
        let mut route = EdgeRoute::new();
        route.push_segment(crate::geom::Point::new(3, 2), crate::geom::Point::new(9, 2)); // border-ish
        route.push_segment(crate::geom::Point::new(9, 2), crate::geom::Point::new(9, 6)); // inside drop
        graph.edge_routes.insert(0, route);
        graph.edges[0].label = Some("LBL".into());

        let config = Config::builder()
            .style(CompositeStyle::from_base(BaseStyle::Unicode))
            .crop(false)
            .build(&crate::parser::ParseConfig::default());

        let output = render(&graph, &config).expect("render td portal");
        let portal_y = graph.get_subgraph("sg").map(|sg| sg.bounds.y).unwrap_or(0);
        let portal_x = graph.get_node("B").map(|n| n.center_x()).unwrap_or(0);
        let glyph = char_at(&output, portal_x, portal_y).unwrap_or(' ');
        let portal_shaft = glyph
            == CompositeStyle::from_base(BaseStyle::Unicode)
                .to_style_chars(BaseStyle::Unicode)
                .edge_v;
        assert!(
            portal_shaft,
            "expected side-aware portal shaft on top border at ({portal_x},{portal_y}), got '{glyph}'\n{output}",
        );
    }

    #[test]
    fn cross_subgraph_edge_pierces_border_lr_as_clean_side_opening() {
        let mut graph = Graph::new();
        graph.direction = Direction::LR;

        let mut a = Node::new("A", "A");
        a.x = 0;
        a.y = 2;
        a.width = 6;

        let mut b = Node::new("B", "B");
        b.x = 10;
        b.y = 2;
        b.width = 6;

        graph.nodes.push(a);
        graph.nodes.push(b);
        graph.edges.push(Edge::new("A", "B"));

        let mut sg = Subgraph::new("sg", Some("Group".into()));
        sg.add_node("B");
        sg.bounds = crate::graph::Rectangle::new(8, 0, 10, 5);
        sg.inner_bounds = crate::graph::Rectangle::new(8, 0, 10, 5);
        graph.add_subgraph(sg);
        graph.associate_node_with_subgraph("B", "sg");

        let mut route = EdgeRoute::new();
        route.push_segment(
            crate::geom::Point::new(5, 3),
            crate::geom::Point::new(12, 3),
        );
        route.push_segment(
            crate::geom::Point::new(12, 3),
            crate::geom::Point::new(12, 4),
        );
        graph.edge_routes.insert(0, route);
        graph.edges[0].label = Some("LBL".into());

        let config = Config::builder()
            .style(CompositeStyle::from_base(BaseStyle::Unicode))
            .build(&crate::parser::ParseConfig::default());

        let output = render(&graph, &config).expect("render lr portal");
        let portal_x = graph.get_subgraph("sg").map(|sg| sg.bounds.x).unwrap_or(0);
        let sg = graph.get_subgraph("sg").expect("subgraph");
        let glyph = ((sg.bounds.y + 1)..(sg.bounds.y + sg.bounds.height.saturating_sub(1)))
            .filter_map(|y| char_at(&output, portal_x, y))
            .find(|glyph| {
                *glyph
                    == CompositeStyle::from_base(BaseStyle::Unicode)
                        .to_style_chars(BaseStyle::Unicode)
                        .portal_pierce
            })
            .unwrap_or(' ');
        let is_pierced = glyph != ' ';
        assert!(
            is_pierced,
            "expected dedicated portal marker somewhere on left border x={portal_x}, got '{glyph}'\n{output}"
        );
    }

    #[test]
    #[cfg(feature = "maintainer-fixtures")]
    fn td_top_portals_outside_the_title_span_keep_a_visible_stem() {
        let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_td.md")
            .expect("read fixture");
        let parsed = crate::parser::parse(&input, false).expect("parse");
        let graph = crate::layout::apply_coarse_layout(
            parsed.graph,
            None,
            crate::layout::CoarseLayoutConfig::default(),
        )
        .expect("layout");

        let node_rects = crate::portals::node_rects_from_graph(&graph);
        let portal_slots =
            crate::portals::collect_portal_slots(&graph, &node_rects, graph.direction);
        let data_layer = graph.get_subgraph("SG2").expect("data layer");
        let title_y = subgraph_title_y(&data_layer.bounds, graph.direction);
        let title_span = title_span(
            &data_layer.bounds,
            data_layer.title.as_deref().expect("title"),
            graph.direction,
        )
        .expect("title span");

        let config = Config::builder()
            .style(CompositeStyle::from_base(BaseStyle::Unicode))
            .crop(false)
            .build(&crate::parser::ParseConfig::default());
        let output = render(&graph, &config).expect("render td portals");

        let top_slots = portal_slots
            .get("SG2")
            .expect("SG2 portal slots")
            .top
            .iter()
            .copied()
            .filter(|x| *x < title_span.0 || *x > title_span.1)
            .collect::<Vec<_>>();
        assert!(
            !top_slots.is_empty(),
            "expected at least one SG2 top portal outside the title span: slots={:?} title_span={:?}",
            portal_slots.get("SG2"),
            title_span,
        );

        for x in top_slots {
            let glyph = char_at(&output, x, title_y).unwrap_or(' ');
            assert_ne!(
                glyph, ' ',
                "expected a visible stem directly below the top portal outside the title span at ({x},{title_y}), got blank\n{output}",
            );
        }
    }

    #[test]
    fn td_labels_avoid_subgraph_border_text() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;
        let mut a = Node::new("A", "A");
        a.x = 0;
        a.y = 0;
        a.width = 5;
        let mut b = Node::new("B", "B");
        b.x = 0;
        b.y = 9;
        b.width = 5;
        graph.nodes.push(a);
        graph.nodes.push(b);
        let mut edge = Edge::new("A", "B");
        edge.label = Some("LBL".into());
        graph.edges.push(edge);

        let mut sg = Subgraph::new("sg", Some("Group".into()));
        sg.add_node("B");
        sg.bounds = crate::graph::Rectangle::new(0, 8, 9, 8);
        sg.inner_bounds = crate::graph::Rectangle::new(0, 9, 9, 6);
        graph.add_subgraph(sg);
        graph.associate_node_with_subgraph("B", "sg");

        let config = Config::builder()
            .style(CompositeStyle::from_base(BaseStyle::Unicode))
            .build(&crate::parser::ParseConfig::default());

        let output = render(&graph, &config).expect("render td label");
        // Ensure the label landed below the subgraph top border row.
        let sg = graph.get_subgraph("sg").unwrap();
        let top = sg.bounds.y;
        let label_row = output
            .lines()
            .enumerate()
            .find_map(|(i, line)| line.contains("LBL").then_some(i))
            .unwrap_or(0);
        assert!(
            label_row != top,
            "expected label not to overwrite subgraph top border (row {top}), got label at row {label_row}:\n{output}"
        );
    }
}
