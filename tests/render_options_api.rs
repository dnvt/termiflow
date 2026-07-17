fn explicit_nested_service_data_input(direction: &str) -> String {
    format!(
        "graph {direction}\nAPI[API Gateway]\nsubgraph SG1 [Service Layer]\nS1[User Service]\nS2[Order Service]\nsubgraph SG2 [Data Layer]\nD1[(User DB)]\nD2[(Order DB)]\nend\nResponse[Response Builder]\nS1 --> S2\nS1 --> D1\nS2 --> D2\nD1 --> Response\nD2 --> Response\nend\nAPI --> S1\n"
    )
}

fn rectangles_overlap(a: &termiflow::graph::Rectangle, b: &termiflow::graph::Rectangle) -> bool {
    a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y
}

#[test]
fn render_options_applies_max_edge_label_width() {
    // LR layout places edge labels inline on the horizontal shaft, which avoids
    // the shaft-bisection issue that can occur with TD and very wide nodes.
    let input = "graph LR\nA[Start] -->|edge label width test| B[End]";
    let output = termiflow::render(
        input,
        termiflow::RenderOptions::new().with_max_edge_label_width(30),
    )
    .unwrap();

    assert!(
        output.contains("edge label width test"),
        "edge label not found in output:\n{output}"
    );
}

#[test]
fn render_options_applies_composite_style() {
    let input = "graph TD\nA[Node]";
    let output = termiflow::render(
        input,
        termiflow::RenderOptions::new()
            .with_composite_style(termiflow::CompositeStyle::parse("corner:plus,border:ascii")),
    )
    .unwrap();

    let first_line = output.lines().next().unwrap_or("");
    assert!(first_line.starts_with('+'));
    assert!(first_line.contains('-'));
}

#[test]
fn render_options_default_respects_in_file_style_directive() {
    let input = "graph TD\n%% termiflow: style=ascii\nA[Node]";
    let output = termiflow::render(input, termiflow::RenderOptions::default()).unwrap();

    let first_line = output.lines().next().unwrap_or("");
    assert!(first_line.starts_with('+'));
}

#[test]
fn render_options_default_respects_in_file_wrap_directive() {
    let input =
        "graph TD\n%% termiflow: wrap=true\n%% termiflow: max_lines=3\nA[hello world from termiflow]";
    let output = termiflow::render(input, termiflow::RenderOptions::default()).unwrap();
    let lines: Vec<&str> = output.lines().collect();
    let label_lines: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| (l.contains("hello world") || l.contains("termiflow")).then_some(i))
        .collect();

    assert!(label_lines.len() >= 2 && label_lines[0] != label_lines[1]);
    assert!(!output.contains("..."));
}

#[test]
fn render_options_enable_render_feedback_controls() {
    let options = termiflow::RenderOptions::new()
        .with_optimize_render(true)
        .with_render_repair_passes(5)
        .with_layout_repair_passes(3)
        .with_debug_critic(true);

    assert!(options.optimize_render);
    assert_eq!(options.render_repair_passes, 5);
    assert_eq!(options.layout_repair_passes, 3);
    assert!(options.debug_critic);
}

#[test]
fn render_with_feedback_returns_semantic_and_critic_data() {
    let outcome = termiflow::render_with_feedback(
        "graph TD\nA[Start] --> B[End]",
        termiflow::RenderOptions::new().with_optimize_render(true),
    )
    .unwrap();

    assert!(outcome.optimized);
    assert_eq!(outcome.repair_passes, 2);
    assert!(outcome.layout_attempts >= 1);
    assert!(outcome.output.contains("Start"));
    assert!(outcome.semantic_frame.width > 0);
    assert!(outcome
        .critic_report
        .notes
        .iter()
        .any(|note| note == "nodes=2"));
}

#[test]
fn render_with_feedback_can_attempt_layout_repair_candidates() {
    let outcome = termiflow::render_with_feedback(
        "graph LR\nA[Start] --> B[Middle] --> C[End]",
        termiflow::RenderOptions::new()
            .with_compact(true)
            .with_optimize_render(true)
            .with_layout_repair_passes(1),
    )
    .unwrap();

    assert!(outcome.layout_attempts >= 1);
    assert!(outcome.output.contains("Middle"));
}

#[test]
fn render_with_feedback_does_not_flag_simple_edge_label_as_crowded() {
    let outcome = termiflow::render_with_feedback(
        "graph LR\nA[Start] -->|ok| B[End]",
        termiflow::RenderOptions::new().with_optimize_render(true),
    )
    .unwrap();

    assert!(!outcome
        .critic_report
        .findings
        .iter()
        .any(|finding| finding.code == termiflow::FindingCode::CrowdedEdgeLabel));
}

#[test]
fn render_with_feedback_preserves_cross_subgraph_edge_ownership() {
    let outcome = termiflow::render_with_feedback(
        "graph TD\nA[Start] --> B[Inside]\nsubgraph SG[Group]\nB\nend",
        termiflow::RenderOptions::new().with_optimize_render(true),
    )
    .unwrap();

    assert!(outcome.semantic_frame.cells.iter().any(|cell| {
        cell.owner_id.as_deref() == Some("edge:0:A->B")
            && matches!(
                cell.owner_kind,
                termiflow::render::semantic::CellOwnerKind::EdgeSegment
                    | termiflow::render::semantic::CellOwnerKind::ArrowHead
            )
    }));
}

#[test]
fn render_with_feedback_marks_back_edge_cells_as_cycle_edges() {
    let outcome = termiflow::render_with_feedback(
        "graph TD\nA[Start] --> B[End]\nB --> A",
        termiflow::RenderOptions::new().with_optimize_render(true),
    )
    .unwrap();

    assert!(outcome.semantic_frame.cells.iter().any(|cell| {
        cell.owner_id.as_deref() == Some("edge:1:B->A")
            && cell.owner_kind == termiflow::render::semantic::CellOwnerKind::CycleEdge
    }));
}

#[test]
fn render_canvas_with_feedback_preserves_precomputed_edge_ownership() {
    let mut graph = termiflow::Graph::new();
    graph.direction = termiflow::graph::Direction::LR;

    let mut start = termiflow::Node::new("A", "Start");
    start.x = 4;
    start.y = 0;

    let mut end = termiflow::Node::new("B", "End");
    end.x = start.width + 6;
    end.y = 0;

    graph.add_node(start.clone());
    graph.add_node(end.clone());
    graph.add_edge(termiflow::Edge::new("A", "B"));

    let mut route = termiflow::geom::EdgeRoute::new();
    route.push_segment(
        termiflow::geom::Point::new(start.x + start.width, start.center_y()),
        termiflow::geom::Point::new(end.x.saturating_sub(1), end.center_y()),
    );
    graph.edge_routes.insert(0, route);

    let outcome =
        termiflow::render_canvas_with_feedback(&graph, &termiflow::Config::default()).unwrap();

    assert!(outcome.semantic_frame.cells.iter().any(|cell| {
        cell.owner_id.as_deref() == Some("edge:0:A->B")
            && matches!(
                cell.owner_kind,
                termiflow::render::semantic::CellOwnerKind::EdgeSegment
                    | termiflow::render::semantic::CellOwnerKind::ArrowHead
            )
    }));
}

#[test]
fn render_canvas_with_feedback_preserves_precomputed_cycle_ownership() {
    let mut graph = termiflow::Graph::new();
    graph.direction = termiflow::graph::Direction::LR;

    let mut start = termiflow::Node::new("A", "Start");
    start.x = 4;
    start.y = 0;

    let mut end = termiflow::Node::new("B", "End");
    end.x = start.width + 10;
    end.y = 4;

    graph.add_node(start.clone());
    graph.add_node(end.clone());

    let mut edge = termiflow::Edge::new("B", "A");
    edge.is_back_edge = true;
    graph.add_edge(edge);

    let mut route = termiflow::geom::EdgeRoute::new();
    route.push_segment(
        termiflow::geom::Point::new(end.x + end.width, end.center_y()),
        termiflow::geom::Point::new(end.x + end.width + 2, end.center_y()),
    );
    route.push_segment(
        termiflow::geom::Point::new(end.x + end.width + 2, end.center_y()),
        termiflow::geom::Point::new(end.x + end.width + 2, start.center_y()),
    );
    route.push_segment(
        termiflow::geom::Point::new(end.x + end.width + 2, start.center_y()),
        termiflow::geom::Point::new(start.x + start.width, start.center_y()),
    );
    graph.edge_routes.insert(0, route);

    let outcome =
        termiflow::render_canvas_with_feedback(&graph, &termiflow::Config::default()).unwrap();

    assert!(outcome.semantic_frame.cells.iter().any(|cell| {
        cell.owner_id.as_deref() == Some("edge:0:B->A")
            && cell.owner_kind == termiflow::render::semantic::CellOwnerKind::CycleEdge
    }));
}

#[test]
fn render_with_feedback_lr_cycle_avoids_false_junction_mismatch() {
    let outcome = termiflow::render_with_feedback(
        "graph LR\nStart[Start] --> Process[Process] --> Check[Check] --> Done[Done]\nCheck --> Start",
        termiflow::RenderOptions::new().with_optimize_render(true),
    )
    .unwrap();

    assert!(!outcome
        .critic_report
        .findings
        .iter()
        .any(|finding| finding.code == termiflow::FindingCode::JunctionTopologyMismatch));
    assert!(!outcome
        .critic_report
        .findings
        .iter()
        .any(|finding| finding.code == termiflow::FindingCode::RouteTopologyMismatch));
}

#[test]
fn render_with_feedback_lr_self_loop_shows_connected_visible_loop() {
    let outcome = termiflow::render_with_feedback(
        "graph LR\nA[Self] --> A",
        termiflow::RenderOptions::new().with_optimize_render(true),
    )
    .unwrap();

    assert!(outcome.output.contains("↑"));
    assert!(outcome.output.contains("──────"));
    assert!(!outcome
        .critic_report
        .findings
        .iter()
        .any(|finding| finding.code == termiflow::FindingCode::JunctionTopologyMismatch));
    assert!(!outcome
        .critic_report
        .findings
        .iter()
        .any(|finding| finding.code == termiflow::FindingCode::RouteTopologyMismatch));
}

#[test]
fn render_canvas_with_feedback_flags_skewed_branch_symmetry() {
    let mut graph = termiflow::Graph::new();
    graph.direction = termiflow::graph::Direction::TD;

    let mut a = termiflow::Node::new("A", "Start");
    a.x = 10;
    a.y = 0;
    a.width = 9;

    let mut b = termiflow::Node::new("B", "Left");
    b.x = 0;
    b.y = 8;
    b.width = 8;

    let mut c = termiflow::Node::new("C", "Right");
    c.x = 26;
    c.y = 8;
    c.width = 9;

    graph.add_node(a);
    graph.add_node(b);
    graph.add_node(c);
    graph.add_edge(termiflow::Edge::new("A", "B"));
    graph.add_edge(termiflow::Edge::new("A", "C"));

    let outcome =
        termiflow::render_canvas_with_feedback(&graph, &termiflow::Config::default()).unwrap();

    assert!(outcome
        .critic_report
        .findings
        .iter()
        .any(|finding| finding.code == termiflow::FindingCode::RouteSymmetryImbalance));
}

#[test]
fn render_canvas_with_feedback_flags_branch_spacing_imbalance() {
    let mut graph = termiflow::Graph::new();
    graph.direction = termiflow::graph::Direction::TD;

    let mut a = termiflow::Node::new("A", "Start");
    a.x = 20;
    a.y = 0;
    a.width = 9;

    let mut b = termiflow::Node::new("B", "Left");
    b.x = 0;
    b.y = 8;
    b.width = 7;

    let mut c = termiflow::Node::new("C", "Middle");
    c.x = 12;
    c.y = 8;
    c.width = 7;

    let mut d = termiflow::Node::new("D", "Right");
    d.x = 42;
    d.y = 8;
    d.width = 7;

    graph.add_node(a);
    graph.add_node(b);
    graph.add_node(c);
    graph.add_node(d);
    graph.add_edge(termiflow::Edge::new("A", "B"));
    graph.add_edge(termiflow::Edge::new("A", "C"));
    graph.add_edge(termiflow::Edge::new("A", "D"));

    let outcome =
        termiflow::render_canvas_with_feedback(&graph, &termiflow::Config::default()).unwrap();

    assert!(outcome
        .critic_report
        .findings
        .iter()
        .any(|finding| finding.code == termiflow::FindingCode::BranchSpacingImbalance));
    assert!(!outcome
        .critic_report
        .findings
        .iter()
        .any(|finding| finding.code == termiflow::FindingCode::RouteSymmetryImbalance));
}

#[test]
fn render_canvas_with_feedback_flags_dense_branch_crowding() {
    let mut graph = termiflow::Graph::new();
    graph.direction = termiflow::graph::Direction::TD;

    let mut a = termiflow::Node::new("A", "Start");
    a.x = 12;
    a.y = 0;
    a.width = 9;

    let mut b = termiflow::Node::new("B", "Left");
    b.x = 4;
    b.y = 8;
    b.width = 7;

    let mut c = termiflow::Node::new("C", "Middle");
    c.x = 11;
    c.y = 8;
    c.width = 7;

    let mut d = termiflow::Node::new("D", "Right");
    d.x = 18;
    d.y = 8;
    d.width = 7;

    graph.add_node(a);
    graph.add_node(b);
    graph.add_node(c);
    graph.add_node(d);
    graph.add_edge(termiflow::Edge::new("A", "B"));
    graph.add_edge(termiflow::Edge::new("A", "C"));
    graph.add_edge(termiflow::Edge::new("A", "D"));

    let outcome =
        termiflow::render_canvas_with_feedback(&graph, &termiflow::Config::default()).unwrap();

    assert!(outcome
        .critic_report
        .findings
        .iter()
        .any(|finding| finding.code == termiflow::FindingCode::BranchCrowding));
    assert!(!outcome
        .critic_report
        .findings
        .iter()
        .any(|finding| finding.code == termiflow::FindingCode::BranchSpacingImbalance));
}

#[test]
fn render_with_feedback_optimizes_convergent_edge_labels() {
    let outcome = termiflow::render_with_feedback(
        "graph TD\nA[Source] -->|label 1| C[Target]\nB[Other] -->|label 2| C",
        termiflow::RenderOptions::new().with_optimize_render(true),
    )
    .unwrap();

    assert!(outcome.output.contains("label 1"));
    assert!(outcome.output.contains("label 2"));
    assert!(!outcome
        .critic_report
        .findings
        .iter()
        .any(|finding| finding.code == termiflow::FindingCode::CrowdedEdgeLabel));
    assert_eq!(
        outcome.critic_report.audit_summary().verdict,
        termiflow::AuditVerdict::Clean
    );
}

#[test]
fn render_with_feedback_collapses_td_subgraph_fanout_to_single_entry_stem() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_fanout_td.md").unwrap();

    for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
        let outcome = termiflow::render_with_feedback(
            &input,
            termiflow::RenderOptions::new()
                .with_style(style)
                .with_optimize_render(true),
        )
        .unwrap();

        let lines: Vec<&str> = outcome.output.lines().collect();
        let title_idx = lines
            .iter()
            .position(|line| line.contains("Handler Group"))
            .expect("title row");
        let stem_band = lines
            .iter()
            .skip(title_idx)
            .take(2)
            .copied()
            .collect::<Vec<_>>();
        let interior_verticals = stem_band
            .iter()
            .map(|row| {
                let row_width = row.chars().count();
                row.chars()
                    .enumerate()
                    .filter(|(idx, ch)| {
                        *idx > 0
                            && *idx + 1 < row_width
                            && matches!(ch, '|' | '│' | ':' | '┃' | '║')
                    })
                    .count()
            })
            .sum::<usize>();

        assert_eq!(
            interior_verticals, 1,
            "expected one shared interior entry stem across the title row and spacer row for {:?}, got rows:\n{}",
            style,
            stem_band.join("\n")
        );
        assert_eq!(
            outcome.critic_report.audit_summary().verdict,
            termiflow::AuditVerdict::Clean,
            "expected visually clean output for {:?}\n{}",
            style,
            outcome.output
        );
    }
}

#[test]
fn render_with_feedback_keeps_complex_td_subgraph_titles_clean() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_td.md").unwrap();

    for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
        let outcome = termiflow::render_with_feedback(
            &input,
            termiflow::RenderOptions::new()
                .with_style(style)
                .with_optimize_render(true),
        )
        .unwrap();

        assert!(!outcome
            .critic_report
            .findings
            .iter()
            .any(|finding| finding.code == termiflow::FindingCode::SubgraphTitleCorrupted));
        assert_eq!(
            outcome.critic_report.audit_summary().verdict,
            termiflow::AuditVerdict::Clean,
            "expected visually clean optimized output for {:?}\n{}",
            style,
            outcome.output
        );
    }
}

#[test]
fn render_with_feedback_keeps_complex_td_data_layer_bottom_exit_to_one_portal() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_td.md").unwrap();
    let parsed = termiflow::parse(&input, false).unwrap();
    let graph = termiflow::coarse_waterfall(parsed.graph).unwrap();
    let data = graph.get_subgraph("SG2").expect("data layer");

    let outcome =
        termiflow::render_canvas_with_feedback(&graph, &termiflow::Config::default()).unwrap();
    let bottom_y = data.bounds.y + data.bounds.height.saturating_sub(1);
    let portal_marker = termiflow::CompositeStyle::from_base(termiflow::BaseStyle::Unicode)
        .to_style_chars(termiflow::BaseStyle::Unicode)
        .portal_pierce;
    let portal_count = (data.bounds.x..data.bounds.x + data.bounds.width)
        .filter_map(|x| outcome.semantic_frame.get(x, bottom_y))
        .filter(|cell| {
            cell.owner_kind == termiflow::render::semantic::CellOwnerKind::PortalOpening
                && cell.ch == portal_marker
        })
        .count();

    assert_eq!(
        portal_count, 1,
        "expected the TD Data Layer fan-in to leave one clean bottom exit portal\n{}",
        outcome.output
    );
}

#[test]
fn render_with_feedback_keeps_complex_td_data_layer_top_entries_visible() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_td.md").unwrap();
    let parsed = termiflow::parse(&input, false).unwrap();
    let graph = termiflow::coarse_waterfall(parsed.graph).unwrap();
    let data = graph.get_subgraph("SG2").expect("data layer");

    let outcome =
        termiflow::render_canvas_with_feedback(&graph, &termiflow::Config::default()).unwrap();
    let top_y = data.bounds.y;
    let portal_marker = termiflow::CompositeStyle::from_base(termiflow::BaseStyle::Unicode)
        .to_style_chars(termiflow::BaseStyle::Unicode)
        .portal_pierce;
    let portal_count = (data.bounds.x..data.bounds.x + data.bounds.width)
        .filter_map(|x| outcome.semantic_frame.get(x, top_y))
        .filter(|cell| {
            cell.owner_kind == termiflow::render::semantic::CellOwnerKind::PortalOpening
                && cell.ch == portal_marker
        })
        .count();

    assert_eq!(
        portal_count, 2,
        "expected the TD Data Layer to expose both top-entry border crossings\n{}",
        outcome.output
    );
}

#[test]
fn render_with_feedback_keeps_horizontal_sibling_subgraph_layout_contract() {
    fn node_overlaps_subgraph(
        node: &termiflow::Node,
        subgraph: &termiflow::graph::Subgraph,
    ) -> bool {
        let node_left = node.x;
        let node_right = node.x + node.width.saturating_sub(1);
        let node_top = node.y;
        let node_bottom = node.y + node.height.saturating_sub(1);
        let bounds = &subgraph.bounds;
        let bounds_right = bounds.x + bounds.width;
        let bounds_bottom = bounds.y + bounds.height;

        node_left < bounds_right
            && node_right >= bounds.x
            && node_top < bounds_bottom
            && node_bottom >= bounds.y
    }

    for fixture in [
        "tests/fixtures/inputs/subgraph_complex_lr.md",
        "tests/fixtures/inputs/subgraph_complex_rl.md",
    ] {
        let input = std::fs::read_to_string(fixture).unwrap();
        let _outcome = termiflow::render_with_feedback(
            &input,
            termiflow::RenderOptions::new().with_style(termiflow::BaseStyle::Unicode),
        )
        .unwrap();
        let parsed = termiflow::parse(&input, false).unwrap();
        let graph = termiflow::coarse_waterfall(parsed.graph).unwrap();
        let outer = graph.get_subgraph("SG1").expect("service layer");
        let inner = graph.get_subgraph("SG2").expect("data layer");
        let user_service = graph.get_node("S1").expect("user service");
        let order_service = graph.get_node("S2").expect("order service");
        let response = graph.get_node("Response").expect("response");

        assert!(
            !rectangles_overlap(&outer.bounds, &inner.bounds),
            "expected Mermaid sibling subgraphs to stay visually separate for {fixture}: outer={:?} inner={:?}",
            outer.bounds,
            inner.bounds
        );
        assert!(
            !node_overlaps_subgraph(user_service, inner) && !node_overlaps_subgraph(order_service, inner),
            "expected SG2 to stay separate without swallowing SG1 sibling nodes for {fixture}: inner={:?} user_service=({}, {}, {}x{}) order_service=({}, {}, {}x{})",
            inner.bounds,
            user_service.x,
            user_service.y,
            user_service.width,
            user_service.height,
            order_service.x,
            order_service.y,
            order_service.width,
            order_service.height
        );
        assert!(
            !(outer.bounds.contains(response.x, response.y)
                && outer.bounds.contains(
                    response.x + response.width.saturating_sub(1),
                    response.y + response.height.saturating_sub(1)
                )),
            "expected Response Builder to avoid full containment within SG1 for {fixture}: outer={:?} response=({}, {}, {}x{})",
            outer.bounds,
            response.x,
            response.y,
            response.width,
            response.height
        );
    }
}

#[test]
fn render_with_feedback_preserves_declared_nested_inner_subgraph_border_cells() {
    let input = explicit_nested_service_data_input("TD");
    let parsed = termiflow::parse(&input, false).unwrap();
    let graph = termiflow::coarse_waterfall(parsed.graph).unwrap();

    let inner = graph.get_subgraph("SG2").expect("inner subgraph");
    let outcome =
        termiflow::render_canvas_with_feedback(&graph, &termiflow::Config::default()).unwrap();
    let border_y = inner.bounds.y + (inner.bounds.height / 2).max(1);
    let cell = outcome
        .semantic_frame
        .get(inner.bounds.x, border_y)
        .expect("border cell");

    assert_eq!(
        cell.owner_kind,
        termiflow::render::semantic::CellOwnerKind::SubgraphBorder,
        "expected declared nested child left border to remain owned by the inner subgraph\n{}",
        outcome.output
    );
}

#[test]
fn render_with_feedback_keeps_nested_child_bottom_border_clean_after_fanin() {
    let input = explicit_nested_service_data_input("TD");
    let parsed = termiflow::parse(&input, false).unwrap();
    let graph = termiflow::coarse_waterfall(parsed.graph).unwrap();
    let inner = graph.get_subgraph("SG2").expect("inner subgraph");

    let outcome =
        termiflow::render_canvas_with_feedback(&graph, &termiflow::Config::default()).unwrap();
    let bottom_y = inner.bounds.y + inner.bounds.height.saturating_sub(1);
    let portal_marker = termiflow::CompositeStyle::from_base(termiflow::BaseStyle::Unicode)
        .to_style_chars(termiflow::BaseStyle::Unicode)
        .portal_pierce;
    let edge_owned_cells = (inner.bounds.x..inner.bounds.x + inner.bounds.width)
        .filter_map(|x| outcome.semantic_frame.get(x, bottom_y))
        .filter(|cell| {
            cell.owner_kind == termiflow::render::semantic::CellOwnerKind::PortalOpening
                && cell.ch == portal_marker
        })
        .count();

    assert_eq!(
        edge_owned_cells, 1,
        "expected the nested child bottom border to expose a single exit portal after fan-in routing\n{}",
        outcome.output
    );
}

#[test]
fn render_with_feedback_keeps_nested_child_top_entries_visible_on_top_border() {
    let input = explicit_nested_service_data_input("TD");
    let parsed = termiflow::parse(&input, false).unwrap();
    let graph = termiflow::coarse_waterfall(parsed.graph).unwrap();

    let outcome =
        termiflow::render_canvas_with_feedback(&graph, &termiflow::Config::default()).unwrap();
    let portal_marker = termiflow::CompositeStyle::from_base(termiflow::BaseStyle::Unicode)
        .to_style_chars(termiflow::BaseStyle::Unicode)
        .portal_pierce;
    let lines: Vec<&str> = outcome.output.lines().collect();
    let title_idx = lines
        .iter()
        .position(|line| line.contains("Data Layer"))
        .expect("nested child title row");
    let top_border = lines
        .get(title_idx.saturating_sub(1))
        .copied()
        .expect("nested child top border row");

    assert_eq!(
        top_border.chars().filter(|&ch| ch == portal_marker).count(),
        2,
        "expected the nested child top border to keep two visible entry portals after balancing\n{}",
        outcome.output
    );
}

#[test]
fn render_with_feedback_keeps_nested_child_fanin_spine_off_left_wall() {
    let input = explicit_nested_service_data_input("TD");
    let parsed = termiflow::parse(&input, false).unwrap();
    let graph = termiflow::coarse_waterfall(parsed.graph).unwrap();
    let inner = graph.get_subgraph("SG2").expect("inner subgraph");

    let outcome =
        termiflow::render_canvas_with_feedback(&graph, &termiflow::Config::default()).unwrap();
    let left_interior_x = inner.bounds.x + 1;
    let bottom_y = inner.bounds.y + inner.bounds.height.saturating_sub(1);
    let edge_owned_cells = ((bottom_y.saturating_sub(2))..bottom_y)
        .filter_map(|y| outcome.semantic_frame.get(left_interior_x, y))
        .filter(|cell| cell.owner_kind == termiflow::render::semantic::CellOwnerKind::EdgeSegment)
        .count();

    assert_eq!(
        edge_owned_cells, 0,
        "expected the nested child fan-in spine to stay off the left interior wall\n{}",
        outcome.output
    );
}

#[test]
fn render_with_feedback_keeps_nested_td_external_entry_from_staircasing_across_ancestors() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_nested_td.md").unwrap();
    let parsed = termiflow::parse(&input, false).unwrap();
    let graph = termiflow::coarse_waterfall(parsed.graph).unwrap();
    let source = graph.get_node("B").expect("source node");
    let deep = graph.get_subgraph("Deep").expect("deep subgraph");

    let outcome =
        termiflow::render_canvas_with_feedback(&graph, &termiflow::Config::default()).unwrap();
    let start_y = source.bottom_y().saturating_add(1);
    let end_y = deep.bounds.y.saturating_sub(1);

    let horizontal_route_rows = (start_y..=end_y)
        .filter(|&y| {
            (0..outcome.semantic_frame.width).any(|x| {
                outcome.semantic_frame.get(x, y).is_some_and(|cell| {
                    matches!(
                        cell.owner_kind,
                        termiflow::render::semantic::CellOwnerKind::EdgeSegment
                            | termiflow::render::semantic::CellOwnerKind::Junction
                            | termiflow::render::semantic::CellOwnerKind::PortalOpening
                    ) && matches!(
                        cell.role,
                        termiflow::render::semantic::CellRole::Horizontal
                            | termiflow::render::semantic::CellRole::Corner
                            | termiflow::render::semantic::CellRole::Junction
                    )
                })
            })
        })
        .count();

    assert_eq!(
        horizontal_route_rows, 1,
        "expected nested TD top-entry to use one shared horizontal jog before the straight descent\n{}",
        outcome.output
    );
}

#[test]
fn render_with_feedback_keeps_declared_nested_horizontal_side_entries_simple_on_borders() {
    for direction in ["LR", "RL"] {
        let input = explicit_nested_service_data_input(direction);
        for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
            let portal_marker = termiflow::CompositeStyle::from_base(style)
                .to_style_chars(style)
                .portal_pierce;
            let outcome = termiflow::render_with_feedback(
                &input,
                termiflow::RenderOptions::new().with_style(style),
            )
            .unwrap();
            let db_lines: Vec<&str> = outcome
                .output
                .lines()
                .filter(|line| line.contains("User DB") || line.contains("Order DB"))
                .collect();

            assert!(
                db_lines.iter().any(|line| line.contains(portal_marker)),
                "expected the declared nested horizontal side-entry to keep a visible dedicated portal marker for {direction} in {:?}\n{}",
                style,
                outcome.output
            );
        }
    }
}

#[test]
fn render_with_feedback_centers_declared_nested_horizontal_fanin_exit_between_sources() {
    for (direction, merge_on_right_border) in [("LR", true), ("RL", false)] {
        let input = explicit_nested_service_data_input(direction);
        let parsed = termiflow::parse(&input, false).unwrap();
        let graph = termiflow::coarse_waterfall(parsed.graph).unwrap();
        let inner = graph.get_subgraph("SG2").expect("inner subgraph");
        let user_db = graph.get_node("D1").expect("user db");
        let order_db = graph.get_node("D2").expect("order db");
        let min_source_y = user_db.center_y().min(order_db.center_y());
        let max_source_y = user_db.center_y().max(order_db.center_y());
        let border_x = if merge_on_right_border {
            inner.bounds.x + inner.bounds.width.saturating_sub(1)
        } else {
            inner.bounds.x
        };
        let portal_marker = termiflow::CompositeStyle::from_base(termiflow::BaseStyle::Unicode)
            .to_style_chars(termiflow::BaseStyle::Unicode)
            .portal_pierce;
        let outcome =
            termiflow::render_canvas_with_feedback(&graph, &termiflow::Config::default()).unwrap();
        let (portal_y, portal) = ((inner.bounds.y + 1)
            ..(inner.bounds.y + inner.bounds.height.saturating_sub(1)))
            .filter_map(|y| {
                outcome
                    .semantic_frame
                    .get(border_x, y)
                    .map(|cell| (y, cell))
            })
            .find(|(_, cell)| {
                cell.owner_kind == termiflow::render::semantic::CellOwnerKind::PortalOpening
                    || cell.ch == portal_marker
            })
            .expect("expected dedicated nested child exit portal on the fan-in border");

        assert!(
            portal_y > min_source_y && portal_y < max_source_y,
            "expected the horizontal nested fan-in exit portal to stay centered between source rows for {direction}, got y={} with source rows {} and {}\n{}",
            portal_y,
            min_source_y,
            max_source_y,
            outcome.output
        );
        assert!(
            portal.ch == portal_marker,
            "expected the centered merge portal to use the dedicated portal marker for {direction}, got '{}'\n{}",
            portal.ch,
            outcome.output
        );
        assert_eq!(
            outcome.critic_report.audit_summary().verdict,
            termiflow::AuditVerdict::Clean,
            "expected visually clean centered horizontal nested fan-in output for {direction}\n{}",
            outcome.output
        );
    }
}

#[test]
fn render_with_feedback_uses_one_clean_horizontal_exit_portal_for_declared_nested_fanin() {
    for (direction, use_right_border) in [("LR", true), ("RL", false)] {
        let input = explicit_nested_service_data_input(direction);
        let parsed = termiflow::parse(&input, false).unwrap();
        let graph = termiflow::coarse_waterfall(parsed.graph).unwrap();
        let inner = graph.get_subgraph("SG2").expect("inner subgraph");
        let border_x = if use_right_border {
            inner.bounds.x + inner.bounds.width.saturating_sub(1)
        } else {
            inner.bounds.x
        };

        for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
            let portal_marker = termiflow::CompositeStyle::from_base(style)
                .to_style_chars(style)
                .portal_pierce;
            let outcome = termiflow::render_canvas_with_feedback(
                &graph,
                &termiflow::Config {
                    composite_style: termiflow::CompositeStyle::from_base(style),
                    ..termiflow::Config::default()
                },
            )
            .unwrap();

            let used_side_portals: Vec<(usize, char)> = ((inner.bounds.y + 1)
                ..(inner.bounds.y + inner.bounds.height.saturating_sub(1)))
                .filter_map(|y| {
                    outcome.semantic_frame.get(border_x, y).and_then(|cell| {
                        (cell.owner_kind
                            == termiflow::render::semantic::CellOwnerKind::PortalOpening
                            || cell.ch == portal_marker)
                            .then_some((y, cell.ch))
                    })
                })
                .collect();

            assert_eq!(
                used_side_portals.len(),
                1,
                "expected one clean horizontal exit portal for declared nested fan-in in {direction} / {:?}\n{}",
                style,
                outcome.output
            );
            assert!(
                used_side_portals[0].1 == portal_marker,
                "expected the declared nested exit portal to use the dedicated portal marker for {direction} in {:?}, got '{}'\n{}",
                style,
                used_side_portals[0].1,
                outcome.output
            );
        }
    }
}

#[test]
fn render_with_feedback_keeps_horizontal_sibling_subgraph_parity_clean() {
    for fixture in [
        "tests/fixtures/inputs/subgraph_complex_lr.md",
        "tests/fixtures/inputs/subgraph_complex_rl.md",
    ] {
        let input = std::fs::read_to_string(fixture).unwrap();
        let parsed = termiflow::parse(&input, false).unwrap();
        let graph = termiflow::coarse_waterfall(parsed.graph).unwrap();
        let outer = graph.get_subgraph("SG1").expect("service layer");
        let inner = graph.get_subgraph("SG2").expect("data layer");
        let user_service = graph.get_node("S1").expect("user service");
        let order_service = graph.get_node("S2").expect("order service");
        let response = graph.get_node("Response").expect("response");

        for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
            let outcome = termiflow::render_with_feedback(
                &input,
                termiflow::RenderOptions::new().with_style(style),
            )
            .unwrap();

            assert!(
                !outcome.critic_report.findings.iter().any(|finding| {
                    matches!(
                        finding.code,
                        termiflow::FindingCode::RouteTopologyMismatch
                            | termiflow::FindingCode::SubgraphTitleCorrupted
                            | termiflow::FindingCode::ArrowTouchesSubgraphBorder
                    )
                }),
                "expected clean horizontal sibling seams for {} in {:?}\n{}",
                fixture,
                style,
                outcome.output
            );
            assert!(
                !rectangles_overlap(&outer.bounds, &inner.bounds),
                "expected sibling horizontal subgraphs to stay separate for {} in {:?}: outer={:?} inner={:?}\n{}",
                fixture,
                style,
                outer.bounds,
                inner.bounds,
                outcome.output
            );
            assert!(
                !inner.bounds.contains(user_service.x, user_service.y)
                    && !inner.bounds.contains(order_service.x, order_service.y),
                "expected the sibling data subgraph to exclude service sibling nodes for {} in {:?}: inner={:?} user_service=({}, {}, {}x{}) order_service=({}, {}, {}x{})\n{}",
                fixture,
                style,
                inner.bounds,
                user_service.x,
                user_service.y,
                user_service.width,
                user_service.height,
                order_service.x,
                order_service.y,
                order_service.width,
                order_service.height,
                outcome.output
            );
            assert!(
                !(outer.bounds.contains(response.x, response.y)
                    && outer.bounds.contains(
                        response.x + response.width.saturating_sub(1),
                        response.y + response.height.saturating_sub(1)
                    )),
                "expected the sibling service subgraph to avoid fully containing Response Builder for {} in {:?}: outer={:?} response=({}, {}, {}x{})\n{}",
                fixture,
                style,
                outer.bounds,
                response.x,
                response.y,
                response.width,
                response.height,
                outcome.output
            );
            assert_eq!(
                outcome.critic_report.audit_summary().verdict,
                termiflow::AuditVerdict::Clean,
                "expected visually clean horizontal sibling parity output for {} in {:?}\n{}",
                fixture,
                style,
                outcome.output
            );
        }
    }
}

#[test]
fn render_with_feedback_keeps_explicit_nested_titles_separate_from_parent_rows() {
    let input = "graph TD\nA[API Gateway] --> B[User Service]\nsubgraph SL[Service Layer]\nB\nsubgraph DL[Data Layer]\nC[Order Service] --> D[(Order DB)]\nE[(User DB)]\nend\nB --> E\nD --> F[Response Builder]\nE --> F\nend";

    for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
        let outcome = termiflow::render_with_feedback(
            input,
            termiflow::RenderOptions::new().with_style(style),
        )
        .unwrap();

        let lines: Vec<&str> = outcome.output.lines().collect();
        let api_idx = lines
            .iter()
            .position(|line| line.contains("API Gateway"))
            .expect("api row");
        let service_idx = lines
            .iter()
            .position(|line| line.contains("Service Layer"))
            .expect("service layer title row");
        let user_idx = lines
            .iter()
            .position(|line| line.contains("User Service"))
            .expect("user service row");
        let data_idx = lines
            .iter()
            .position(|line| line.contains("Data Layer"))
            .expect("data layer title row");
        let response_idx = lines
            .iter()
            .position(|line| line.contains("Response Builder"))
            .expect("response row");

        assert!(
            service_idx > api_idx,
            "expected the service-layer title row to stay below the external API box for {:?}\n{}",
            style,
            outcome.output
        );
        assert!(
            data_idx > user_idx,
            "expected the nested data-layer title row to stay below the parent's direct node row for {:?}\n{}",
            style,
            outcome.output
        );
        assert!(
            response_idx > data_idx,
            "expected the parent-only response node to render below the nested child title row for {:?}\n{}",
            style,
            outcome.output
        );
        assert!(
            !lines[service_idx].contains("API Gateway"),
            "expected the service-layer title row to stay free of API-box text for {:?}\n{}",
            style,
            outcome.output
        );
        assert!(
            !lines[data_idx].contains("User Service"),
            "expected the data-layer title row to stay free of parent direct-node text for {:?}\n{}",
            style,
            outcome.output
        );
    }
}

#[test]
fn render_with_feedback_keeps_parent_title_above_declared_nested_child_fanin() {
    let input =
        "graph TD\nT[Target]\nsubgraph P[Parent]\nsubgraph C[Child]\nS1[One]\nS2[Two]\nS3[Three]\nend\nend\nS1 --> T\nS2 --> T\nS3 --> T\n";

    for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
        let outcome = termiflow::render_with_feedback(
            input,
            termiflow::RenderOptions::new().with_style(style),
        )
        .unwrap();

        assert!(!outcome
            .critic_report
            .findings
            .iter()
            .any(|finding| { finding.code == termiflow::FindingCode::SubgraphTitleCorrupted }));
        let lines: Vec<&str> = outcome.output.lines().collect();
        let parent_idx = lines
            .iter()
            .position(|line| line.contains("Parent"))
            .expect("parent title row");
        let child_idx = lines
            .iter()
            .position(|line| line.contains("Child"))
            .expect("child title row");
        assert!(
            child_idx > parent_idx,
            "expected the declared child title row to stay below the parent title row for {:?}\n{}",
            style,
            outcome.output
        );
        assert_eq!(
            outcome.critic_report.audit_summary().verdict,
            termiflow::AuditVerdict::Clean,
            "expected visually clean parent-only nested fan-in output for {:?}\n{}",
            style,
            outcome.output
        );
    }
}

#[test]
fn render_with_feedback_keeps_explicit_nested_horizontal_variants_clean() {
    for direction in ["LR", "RL"] {
        let input = format!(
            "graph {direction}\nA[API Gateway] --> B[User Service]\nsubgraph SL[Service Layer]\nB\nsubgraph DL[Data Layer]\nC[Order Service] --> D[(Order DB)]\nE[(User DB)]\nend\nB --> E\nD --> F[Response Builder]\nE --> F\nend\n"
        );

        for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
            let outcome = termiflow::render_with_feedback(
                &input,
                termiflow::RenderOptions::new().with_style(style),
            )
            .unwrap();

            assert!(!outcome.critic_report.findings.iter().any(|finding| {
                matches!(
                    finding.code,
                    termiflow::FindingCode::ArrowTouchesSubgraphBorder
                        | termiflow::FindingCode::ArrowWithoutVisibleShaft
                        | termiflow::FindingCode::SubgraphTitleCorrupted
                        | termiflow::FindingCode::RouteTopologyMismatch
                )
            }));
            assert_eq!(
                outcome.critic_report.audit_summary().verdict,
                termiflow::AuditVerdict::Clean,
                "expected visually clean explicit nested horizontal output for {direction} in {:?}\n{}",
                style,
                outcome.output
            );

            let lines: Vec<&str> = outcome.output.lines().collect();
            let parent_idx = lines
                .iter()
                .position(|line| line.contains("Service Layer"))
                .expect("parent title row");
            let child_idx = lines
                .iter()
                .position(|line| line.contains("Data Layer"))
                .expect("child title row");
            assert!(
                child_idx > parent_idx,
                "expected the explicit nested child title row to staircase below the parent title row for {direction} in {:?}\n{}",
                style,
                outcome.output
            );
        }
    }
}

#[test]
fn render_with_feedback_keeps_complex_bt_subgraph_connectors_clean() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_bt.md").unwrap();

    for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
        let outcome = termiflow::render_with_feedback(
            &input,
            termiflow::RenderOptions::new().with_style(style),
        )
        .unwrap();

        assert!(!outcome.critic_report.findings.iter().any(|finding| {
            matches!(
                finding.code,
                termiflow::FindingCode::ArrowTouchesSubgraphBorder
                    | termiflow::FindingCode::ArrowWithoutVisibleShaft
                    | termiflow::FindingCode::SubgraphTitleCorrupted
            )
        }));
        let lines: Vec<&str> = outcome.output.lines().collect();
        let service_idx = lines
            .iter()
            .position(|line| line.contains("Service Layer"))
            .expect("service layer title row");
        let data_idx = lines
            .iter()
            .position(|line| line.contains("Data Layer"))
            .expect("data layer title row");
        assert!(
            data_idx < service_idx,
            "expected BT data-layer title rows to stay above service-layer title rows for {:?}\n{}",
            style,
            outcome.output
        );
        assert_eq!(
            outcome.critic_report.audit_summary().verdict,
            termiflow::AuditVerdict::Clean,
            "expected visually clean default BT subgraph output for {:?}\n{}",
            style,
            outcome.output
        );
        let corrupted_user_db = match style {
            termiflow::BaseStyle::Ascii => "|  User DB| |",
            termiflow::BaseStyle::Unicode => "│  User DB│ │",
            _ => unreachable!(),
        };
        let clean_user_db = match style {
            termiflow::BaseStyle::Ascii => "|  User DB  |",
            termiflow::BaseStyle::Unicode => "│  User DB  │",
            _ => unreachable!(),
        };
        assert!(
            !outcome.output.contains(corrupted_user_db),
            "expected the BT inner subgraph border not to bisect the User DB node for {:?}\n{}",
            style,
            outcome.output
        );
        assert!(
            outcome.output.contains(clean_user_db),
            "expected the User DB node border to render cleanly for {:?}\n{}",
            style,
            outcome.output
        );
    }
}

#[test]
fn render_with_feedback_keeps_declared_nested_bt_centered_boundary_groups_clean() {
    let input =
        "graph BT\nT[Target]\nsubgraph P[Parent]\nsubgraph C[Child]\nL[Left]\nM[Middle]\nR[Right]\nend\nS[Source]\nend\nS --> L\nS --> M\nS --> R\nL --> T\nM --> T\nR --> T\n";

    for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
        let outcome = termiflow::render_with_feedback(
            input,
            termiflow::RenderOptions::new().with_style(style),
        )
        .unwrap();

        assert!(!outcome.critic_report.findings.iter().any(|finding| {
            matches!(
                finding.code,
                termiflow::FindingCode::ArrowTouchesSubgraphBorder
                    | termiflow::FindingCode::ArrowWithoutVisibleShaft
                    | termiflow::FindingCode::SubgraphTitleCorrupted
            )
        }));
        let lines: Vec<&str> = outcome.output.lines().collect();
        let parent_idx = lines
            .iter()
            .position(|line| line.contains("Parent"))
            .expect("parent title row");
        let child_idx = lines
            .iter()
            .position(|line| line.contains("Child"))
            .expect("child title row");
        assert!(
            parent_idx > child_idx,
            "expected the declared BT parent title row to stay below the nested child title row for {:?}\n{}",
            style,
            outcome.output
        );
        assert_eq!(
            outcome.critic_report.audit_summary().verdict,
            termiflow::AuditVerdict::Clean,
            "expected visually clean declared BT centered-boundary output for {:?}\n{}",
            style,
            outcome.output
        );
    }
}

#[test]
fn render_with_feedback_keeps_nested_horizontal_subgraphs_clean() {
    for fixture in [
        "tests/fixtures/inputs/subgraph_nested_lr.md",
        "tests/fixtures/inputs/subgraph_nested_rl.md",
    ] {
        for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
            let input = std::fs::read_to_string(fixture).unwrap();
            let outcome = termiflow::render_with_feedback(
                &input,
                termiflow::RenderOptions::new().with_style(style),
            )
            .unwrap();

            assert!(
                !outcome.critic_report.findings.iter().any(|finding| {
                    matches!(
                        finding.code,
                        termiflow::FindingCode::RouteTopologyMismatch
                            | termiflow::FindingCode::SubgraphTitleCorrupted
                    )
                }),
                "expected clean nested horizontal subgraph borders/titles for {} in {:?}\n{}",
                fixture,
                style,
                outcome.output
            );
            assert_eq!(
                outcome.critic_report.audit_summary().verdict,
                termiflow::AuditVerdict::Clean,
                "expected visually clean nested horizontal output for {} in {:?}\n{}",
                fixture,
                style,
                outcome.output
            );

            let lines: Vec<&str> = outcome.output.lines().collect();
            let outer_idx = lines
                .iter()
                .position(|line| line.contains("Outer"))
                .expect("outer title row");
            let inner_idx = lines
                .iter()
                .position(|line| line.contains("Inner"))
                .expect("inner title row");
            let deep_idx = lines
                .iter()
                .position(|line| line.contains("Deep"))
                .expect("deep title row");
            assert!(
                outer_idx < inner_idx && inner_idx < deep_idx,
                "expected nested horizontal titles to stair-step by depth for {} in {:?}\n{}",
                fixture,
                style,
                outcome.output
            );
        }
    }
}

#[test]
fn render_with_feedback_keeps_horizontal_sibling_semantics_consistent_across_styles() {
    for fixture in [
        "tests/fixtures/inputs/subgraph_complex_lr.md",
        "tests/fixtures/inputs/subgraph_complex_rl.md",
    ] {
        let input = std::fs::read_to_string(fixture).unwrap();
        let parsed = termiflow::parse(&input, false).unwrap();
        let graph = termiflow::coarse_waterfall(parsed.graph).unwrap();
        let outer = graph.get_subgraph("SG1").expect("service layer");
        let inner = graph.get_subgraph("SG2").expect("data layer");
        let user_service = graph.get_node("S1").expect("user service");
        let order_service = graph.get_node("S2").expect("order service");
        let response = graph.get_node("Response").expect("response");

        for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
            let outcome = termiflow::render_with_feedback(
                &input,
                termiflow::RenderOptions::new().with_style(style),
            )
            .unwrap();

            assert!(
                !outcome.critic_report.findings.iter().any(|finding| {
                    finding.code == termiflow::FindingCode::SubgraphTitleCorrupted
                }),
                "expected sibling horizontal titles to stay intact for {} in {:?}\n{}",
                fixture,
                style,
                outcome.output
            );
            assert!(
                !rectangles_overlap(&outer.bounds, &inner.bounds),
                "expected sibling horizontal subgraphs to stay separate for {} in {:?}: outer={:?} inner={:?}\n{}",
                fixture,
                style,
                outer.bounds,
                inner.bounds,
                outcome.output
            );
            assert!(
                !inner.bounds.contains(user_service.x, user_service.y)
                    && !inner.bounds.contains(order_service.x, order_service.y),
                "expected the sibling data subgraph to exclude service sibling nodes for {} in {:?}: inner={:?} user_service=({}, {}, {}x{}) order_service=({}, {}, {}x{})\n{}",
                fixture,
                style,
                inner.bounds,
                user_service.x,
                user_service.y,
                user_service.width,
                user_service.height,
                order_service.x,
                order_service.y,
                order_service.width,
                order_service.height,
                outcome.output
            );
            assert!(
                !(outer.bounds.contains(response.x, response.y)
                    && outer.bounds.contains(
                        response.x + response.width.saturating_sub(1),
                        response.y + response.height.saturating_sub(1)
                    )),
                "expected the sibling service subgraph to avoid fully containing Response Builder for {} in {:?}: outer={:?} response=({}, {}, {}x{})\n{}",
                fixture,
                style,
                outer.bounds,
                response.x,
                response.y,
                response.width,
                response.height,
                outcome.output
            );
        }
    }
}

#[test]
fn render_with_feedback_keeps_subgraph_complex_direction_matrix_clean() {
    fn is_route_neighbor(
        frame: &termiflow::render::semantic::SemanticFrame,
        x: usize,
        y: usize,
    ) -> bool {
        frame.get(x, y).is_some_and(|cell| {
            matches!(
                cell.owner_kind,
                termiflow::render::semantic::CellOwnerKind::EdgeSegment
                    | termiflow::render::semantic::CellOwnerKind::CycleEdge
                    | termiflow::render::semantic::CellOwnerKind::ArrowHead
                    | termiflow::render::semantic::CellOwnerKind::Junction
                    | termiflow::render::semantic::CellOwnerKind::PortalOpening
            ) || matches!(
                cell.ch,
                'v' | '^' | '<' | '>' | '↓' | '↑' | '←' | '→' | '▼' | '▲' | '◀' | '▶'
            )
        })
    }

    for fixture in [
        "tests/fixtures/inputs/subgraph_complex_td.md",
        "tests/fixtures/inputs/subgraph_complex_bt.md",
        "tests/fixtures/inputs/subgraph_complex_lr.md",
        "tests/fixtures/inputs/subgraph_complex_rl.md",
    ] {
        let input = std::fs::read_to_string(fixture).unwrap();

        for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
            let portal_marker = termiflow::CompositeStyle::from_base(style)
                .to_style_chars(style)
                .portal_pierce;
            let outcome = termiflow::render_with_feedback(
                &input,
                termiflow::RenderOptions::new().with_style(style),
            )
            .unwrap();

            assert!(
                !outcome.critic_report.findings.iter().any(|finding| {
                    matches!(
                        finding.code,
                        termiflow::FindingCode::SubgraphTitleCorrupted
                            | termiflow::FindingCode::ArrowTouchesSubgraphBorder
                            | termiflow::FindingCode::ArrowWithoutVisibleShaft
                    )
                }),
                "expected stable subgraph-complex directional connections for {} in {:?}\n{}",
                fixture,
                style,
                outcome.output
            );
            assert!(
                matches!(
                    outcome.critic_report.audit_summary().verdict,
                    termiflow::AuditVerdict::Clean | termiflow::AuditVerdict::NeedsReview
                ),
                "expected acceptable subgraph-complex direction matrix output for {} in {:?}\n{}",
                fixture,
                style,
                outcome.output
            );

            let frame = &outcome.semantic_frame;
            let visible_used_portals: Vec<char> = (0..frame.height)
                .flat_map(|y| {
                    (0..frame.width).filter_map(move |x| {
                        let cell = frame.get(x, y)?;
                        (cell.owner_kind
                            == termiflow::render::semantic::CellOwnerKind::PortalOpening
                            && cell.ch == portal_marker
                            && ((y > 0 && is_route_neighbor(frame, x, y - 1))
                                || (y + 1 < frame.height && is_route_neighbor(frame, x, y + 1))
                                || (x > 0 && is_route_neighbor(frame, x - 1, y))
                                || (x + 1 < frame.width && is_route_neighbor(frame, x + 1, y))))
                        .then_some(cell.ch)
                    })
                })
                .collect();

            assert!(
                !visible_used_portals.is_empty(),
                "expected at least one used portal in {} in {:?}\n{}",
                fixture,
                style,
                outcome.output
            );
        }
    }
}

#[test]
fn render_with_feedback_keeps_multi_td_subgraph_entries_with_visible_shafts() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_multi_td.md").unwrap();

    let outcome = termiflow::render_with_feedback(
        &input,
        termiflow::RenderOptions::new()
            .with_style(termiflow::BaseStyle::Ascii)
            .with_optimize_render(true),
    )
    .unwrap();

    assert!(!outcome
        .critic_report
        .findings
        .iter()
        .any(|finding| finding.code == termiflow::FindingCode::ArrowWithoutVisibleShaft));
    assert_eq!(
        outcome.critic_report.audit_summary().verdict,
        termiflow::AuditVerdict::Clean,
        "expected clean multi-subgraph TD output\n{}",
        outcome.output
    );
}

#[test]
fn render_with_feedback_keeps_td_edge_labels_off_final_arrow_shaft() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_labels_td.md").unwrap();

    let outcome = termiflow::render_with_feedback(
        &input,
        termiflow::RenderOptions::new()
            .with_style(termiflow::BaseStyle::Ascii)
            .with_optimize_render(true),
    )
    .unwrap();

    assert!(!outcome
        .critic_report
        .findings
        .iter()
        .any(|finding| finding.code == termiflow::FindingCode::ArrowWithoutVisibleShaft));
    assert_eq!(
        outcome.critic_report.audit_summary().verdict,
        termiflow::AuditVerdict::Clean,
        "expected clean labeled TD output\n{}",
        outcome.output
    );
}

#[test]
fn render_with_feedback_keeps_td_sibling_subgraph_arrows_off_foreign_borders() {
    let input =
        std::fs::read_to_string("tests/fixtures/inputs/collision_sibling_subgraphs_td.md").unwrap();

    for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
        let outcome = termiflow::render_with_feedback(
            &input,
            termiflow::RenderOptions::new().with_style(style),
        )
        .unwrap();

        assert!(
            !outcome.critic_report.findings.iter().any(|finding| {
                finding.code == termiflow::FindingCode::ArrowTouchesSubgraphBorder
            }),
            "expected TD sibling-subgraph arrows to avoid foreign borders for {:?}\n{}",
            style,
            outcome.output
        );
        assert_eq!(
            outcome.critic_report.audit_summary().verdict,
            termiflow::AuditVerdict::Clean,
            "expected clean TD sibling-subgraph output for {:?}\n{}",
            style,
            outcome.output
        );
    }
}

#[test]
fn render_with_feedback_keeps_ascii_horizontal_edge_labels_clean_in_default_render() {
    for fixture in [
        "tests/fixtures/inputs/label_basic_lr.md",
        "tests/fixtures/inputs/label_basic_rl.md",
        "tests/fixtures/inputs/label_edge_long_lr.md",
        "tests/fixtures/inputs/label_edge_long_rl.md",
    ] {
        let input = std::fs::read_to_string(fixture).unwrap();
        let outcome = termiflow::render_with_feedback(
            &input,
            termiflow::RenderOptions::new().with_style(termiflow::BaseStyle::Ascii),
        )
        .unwrap();

        assert!(
            !outcome.critic_report.findings.iter().any(|finding| {
                matches!(
                    finding.code,
                    termiflow::FindingCode::EdgeLabelCollidesWithNode
                        | termiflow::FindingCode::ArrowWithoutVisibleShaft
                        | termiflow::FindingCode::RouteCrossesNodeInterior
                )
            }),
            "expected clean ASCII horizontal edge labels for {}\n{}",
            fixture,
            outcome.output
        );
        assert_eq!(
            outcome.critic_report.audit_summary().verdict,
            termiflow::AuditVerdict::Clean,
            "expected clean default ASCII label output for {}\n{}",
            fixture,
            outcome.output
        );
    }
}

#[test]
fn render_with_feedback_default_lr_corner_subgraph_route_is_topologically_clean() {
    let input =
        std::fs::read_to_string("tests/fixtures/inputs/collision_edge_corner_lr.md").unwrap();

    for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
        let outcome = termiflow::render_with_feedback(
            &input,
            termiflow::RenderOptions::new().with_style(style),
        )
        .unwrap();

        assert!(
            !outcome
                .critic_report
                .findings
                .iter()
                .any(|finding| finding.code == termiflow::FindingCode::RouteTopologyMismatch),
            "expected no route topology mismatch for {:?}\n{}",
            style,
            outcome.output
        );
        assert_eq!(
            outcome.critic_report.audit_summary().verdict,
            termiflow::AuditVerdict::Clean,
            "expected visually clean default output for {:?}\n{}",
            style,
            outcome.output
        );
    }
}

#[test]
fn render_with_feedback_default_bt_subgraph_exits_keep_visible_arrow_shafts() {
    for fixture in [
        "tests/fixtures/inputs/subgraph_fanin_bt.md",
        "tests/fixtures/inputs/subgraph_labels_bt.md",
    ] {
        for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
            let input = std::fs::read_to_string(fixture).unwrap();

            let outcome = termiflow::render_with_feedback(
                &input,
                termiflow::RenderOptions::new().with_style(style),
            )
            .unwrap();

            assert!(
                !outcome
                    .critic_report
                    .findings
                    .iter()
                    .any(|finding| finding.code == termiflow::FindingCode::ArrowWithoutVisibleShaft),
                "expected visible BT shaft for fixture {} in {:?}\n{}",
                fixture,
                style,
                outcome.output
            );
            assert_eq!(
                outcome.critic_report.audit_summary().verdict,
                termiflow::AuditVerdict::Clean,
                "expected clean BT subgraph exit for fixture {} in {:?}\n{}",
                fixture,
                style,
                outcome.output
            );
        }
    }
}

#[test]
fn render_with_feedback_places_supported_bt_subgraph_titles_on_bottom_interior_row() {
    for (fixture, title) in [
        ("tests/fixtures/inputs/subgraph_fanin_bt.md", "Data Sources"),
        ("tests/fixtures/inputs/subgraph_labels_bt.md", "Auth Flow"),
    ] {
        for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
            let input = std::fs::read_to_string(fixture).unwrap();

            let outcome = termiflow::render_with_feedback(
                &input,
                termiflow::RenderOptions::new().with_style(style),
            )
            .unwrap();

            let lines: Vec<&str> = outcome.output.lines().collect();
            let title_idx = lines
                .iter()
                .position(|line| line.contains(title))
                .expect("BT title row");

            assert_eq!(
                title_idx,
                lines.len().saturating_sub(2),
                "expected BT title on the bottom interior row for fixture {} in {:?}\n{}",
                fixture,
                style,
                outcome.output
            );
            assert!(
                !lines
                    .last()
                    .is_some_and(|bottom_border| bottom_border.contains(title)),
                "expected BT title to stay off the bottom border row for fixture {} in {:?}\n{}",
                fixture,
                style,
                outcome.output
            );
            assert_eq!(
                outcome.critic_report.audit_summary().verdict,
                termiflow::AuditVerdict::Clean,
                "expected clean BT title placement for fixture {} in {:?}\n{}",
                fixture,
                style,
                outcome.output
            );
        }
    }
}

#[test]
fn render_with_feedback_keeps_bt_titled_subgraph_entries_clear_of_title_row() {
    let inputs = [
        (
            "graph BT\nIn[Input]\nsubgraph G [Processing]\n  P1[Parse]\n  P2[Transform]\n  P3[Validate]\nend\nOut[Output]\nIn --> P1\nP1 --> P2\nP2 --> P3\nP3 --> Out\n",
            termiflow::BaseStyle::Unicode,
        ),
        (
            "graph BT\nsubgraph W [Workers]\n  W1[Worker 1]\n  W2[Worker 2]\n  W3[Worker 3]\nend\nSource[Source] --> W1\nSource --> W2\nSource --> W3\n",
            termiflow::BaseStyle::Ascii,
        ),
    ];

    for (input, style) in inputs {
        let outcome = termiflow::render_with_feedback(
            input,
            termiflow::RenderOptions::new().with_style(style),
        )
        .unwrap();

        assert!(
            !outcome.critic_report.findings.iter().any(|finding| {
                finding.code == termiflow::FindingCode::SubgraphTitleCorrupted
            }),
            "expected BT titled-subgraph entry path to stay off the protected title gutter for {:?}\n{}",
            style,
            outcome.output
        );
        assert_eq!(
            outcome.critic_report.audit_summary().verdict,
            termiflow::AuditVerdict::Clean,
            "expected clean BT titled-subgraph entry routing for {:?}\n{}",
            style,
            outcome.output
        );
    }
}

#[test]
fn render_with_feedback_keeps_bt_simple_fanout_source_below_title_row() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_fanout_bt.md").unwrap();

    let outcome = termiflow::render_with_feedback(
        &input,
        termiflow::RenderOptions::new().with_style(termiflow::BaseStyle::Unicode),
    )
    .unwrap();

    assert!(
        outcome.output.contains("Request Router"),
        "expected BT fanout source node to remain visible\n{}",
        outcome.output
    );
    assert!(
        !outcome.output.contains("ReHandler Group"),
        "expected BT fanout title row to stay uncorrupted\n{}",
        outcome.output
    );
    assert_eq!(
        outcome.critic_report.audit_summary().verdict,
        termiflow::AuditVerdict::Clean,
        "expected clean BT fanout routing\n{}",
        outcome.output
    );
}

#[test]
fn render_with_feedback_keeps_lr_subgraph_fanins_clean() {
    let input = "graph LR\nsubgraph S [Sources]\n  S1[Source 1]\n  S2[Source 2]\n  S3[Source 3]\nend\nS1 --> Sink[Sink]\nS2 --> Sink\nS3 --> Sink\n";

    for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
        let outcome = termiflow::render_with_feedback(
            input,
            termiflow::RenderOptions::new().with_style(style),
        )
        .unwrap();

        assert!(
            !outcome
                .critic_report
                .findings
                .iter()
                .any(|finding| { finding.code == termiflow::FindingCode::RouteTopologyMismatch }),
            "expected LR subgraph fan-in seams to avoid route-topology artifacts for {:?}\n{}",
            style,
            outcome.output
        );
        assert_eq!(
            outcome.critic_report.audit_summary().verdict,
            termiflow::AuditVerdict::Clean,
            "expected clean LR subgraph fan-in routing for {:?}\n{}",
            style,
            outcome.output
        );
    }
}

#[test]
fn render_with_feedback_keeps_lr_sibling_subgraph_exits_clean() {
    let input = "graph LR\nsubgraph A [Frontend]\n  UI[UI]\n  Auth[Auth]\nend\nsubgraph B [Backend]\n  API[API]\n  DB[Database]\nend\nUI --> API\nAuth --> API\nAPI --> DB\n";

    for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
        let outcome = termiflow::render_with_feedback(
            input,
            termiflow::RenderOptions::new().with_style(style),
        )
        .unwrap();

        assert!(
            !outcome
                .critic_report
                .findings
                .iter()
                .any(|finding| { finding.code == termiflow::FindingCode::RouteTopologyMismatch }),
            "expected LR sibling-subgraph exits to avoid border seam artifacts for {:?}\n{}",
            style,
            outcome.output
        );
        assert_eq!(
            outcome.critic_report.audit_summary().verdict,
            termiflow::AuditVerdict::Clean,
            "expected clean LR sibling-subgraph routing for {:?}\n{}",
            style,
            outcome.output
        );
    }
}

#[test]
fn render_with_feedback_keeps_converge_cascade_fanins_centered_in_all_directions() {
    for fixture in [
        "tests/fixtures/inputs/converge_cascade_td.md",
        "tests/fixtures/inputs/converge_cascade_bt.md",
        "tests/fixtures/inputs/converge_cascade_lr.md",
        "tests/fixtures/inputs/converge_cascade_rl.md",
    ] {
        let input = std::fs::read_to_string(fixture).unwrap();

        for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
            let outcome = termiflow::render_with_feedback(
                &input,
                termiflow::RenderOptions::new()
                    .with_style(style)
                    .with_optimize_render(true),
            )
            .unwrap();

            assert!(
                !outcome
                    .critic_report
                    .findings
                    .iter()
                    .any(|finding| finding.code == termiflow::FindingCode::RouteSymmetryImbalance),
                "expected centered cascade fan-ins for {:?} in {}\n{}",
                style,
                fixture,
                outcome.output
            );
            assert_eq!(
                outcome.critic_report.audit_summary().verdict,
                termiflow::AuditVerdict::Clean,
                "expected visually clean cascade fan-ins for {:?} in {}\n{}",
                style,
                fixture,
                outcome.output
            );
        }
    }
}

#[test]
fn render_with_feedback_treats_crossing_grids_as_visually_clean() {
    for fixture in [
        "tests/fixtures/inputs/crossing_grid_td.md",
        "tests/fixtures/inputs/crossing_grid_bt.md",
        "tests/fixtures/inputs/crossing_grid_lr.md",
        "tests/fixtures/inputs/crossing_grid_rl.md",
    ] {
        let input = std::fs::read_to_string(fixture).unwrap();

        for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
            let outcome = termiflow::render_with_feedback(
                &input,
                termiflow::RenderOptions::new()
                    .with_style(style)
                    .with_optimize_render(true),
            )
            .unwrap();

            assert!(
                !outcome
                    .critic_report
                    .findings
                    .iter()
                    .any(|finding| finding.code == termiflow::FindingCode::RouteSymmetryImbalance),
                "expected no false symmetry imbalance for {:?} in {}\n{}",
                style,
                fixture,
                outcome.output
            );
            assert_eq!(
                outcome.critic_report.audit_summary().verdict,
                termiflow::AuditVerdict::Clean,
                "expected visually clean crossing grid for {:?} in {}\n{}",
                style,
                fixture,
                outcome.output
            );
        }
    }
}

#[test]
fn default_render_fixes_obvious_degree_mismatch_cases() {
    let cascade = std::fs::read_to_string("tests/fixtures/inputs/converge_cascade_bt.md").unwrap();
    let cascade_outcome = termiflow::render_with_feedback(
        &cascade,
        termiflow::RenderOptions::new().with_style(termiflow::BaseStyle::Ascii),
    )
    .unwrap();
    assert_eq!(
        cascade_outcome.critic_report.audit_summary().verdict,
        termiflow::AuditVerdict::Clean,
        "expected default ascii cascade cleanup to fix degree mismatches\n{}",
        cascade_outcome.output
    );

    let collision =
        std::fs::read_to_string("tests/fixtures/inputs/collision_parallel_cross_bt.md").unwrap();
    for run in 0..64 {
        let collision_outcome = termiflow::render_with_feedback(
            &collision,
            termiflow::RenderOptions::new().with_style(termiflow::BaseStyle::Unicode),
        )
        .unwrap();
        assert_eq!(
            collision_outcome.critic_report.audit_summary().verdict,
            termiflow::AuditVerdict::Clean,
            "expected default unicode collision cleanup to fix degree mismatches on run {}\n{}",
            run,
            collision_outcome.output
        );
    }

    let sibling =
        std::fs::read_to_string("tests/fixtures/inputs/collision_sibling_subgraphs_bt.md").unwrap();
    let sibling_outcome = termiflow::render_with_feedback(
        &sibling,
        termiflow::RenderOptions::new().with_style(termiflow::BaseStyle::Unicode),
    )
    .unwrap();
    assert_eq!(
        sibling_outcome.critic_report.audit_summary().verdict,
        termiflow::AuditVerdict::Clean,
        "expected default unicode sibling-subgraph cleanup to keep BT title rows clean\n{}",
        sibling_outcome.output
    );
    assert!(!sibling_outcome
        .critic_report
        .findings
        .iter()
        .any(|finding| { finding.code == termiflow::FindingCode::SubgraphTitleCorrupted }));
}

#[test]
fn render_matches_verified_collision_edge_along_border_lr_snapshots() {
    let input =
        std::fs::read_to_string("tests/fixtures/inputs/collision_edge_along_border_lr.md").unwrap();

    for (style, source_top, target_top, must_not_contain) in [
        (
            termiflow::BaseStyle::Unicode,
            "┌──────┐",
            "┌─────┐",
            "┌──────│        ┌─────┐",
        ),
        (
            termiflow::BaseStyle::Ascii,
            "+------+",
            "+-----+",
            "+------|        +-----+",
        ),
    ] {
        let outcome = termiflow::render_with_feedback(
            &input,
            termiflow::RenderOptions::new().with_style(style),
        )
        .unwrap();

        assert!(
            outcome.output.contains(source_top) && outcome.output.contains(target_top),
            "expected verified LR border-contact fixture to preserve node corners against the subgraph wall for {:?}\n{}",
            style,
            outcome.output
        );
        assert!(
            !outcome.output.contains(must_not_contain),
            "expected LR border-contact fixture to avoid clobbering box corners with a subgraph wall for {:?}\n{}",
            style,
            outcome.output
        );
        assert_eq!(
            outcome.critic_report.audit_summary().verdict,
            termiflow::AuditVerdict::Clean,
            "expected verified LR border-contact fixture to stay visually clean for {:?}\n{}",
            style,
            outcome.output
        );
    }
}

#[test]
fn render_matches_verified_collision_edge_along_border_rl_snapshots() {
    let input =
        std::fs::read_to_string("tests/fixtures/inputs/collision_edge_along_border_rl.md").unwrap();

    for (style, source_top, target_top, must_not_contain) in [
        (
            termiflow::BaseStyle::Unicode,
            "┌──────┐",
            "┌─────┐",
            "│ ┌─────┐        ┌──────┐",
        ),
        (
            termiflow::BaseStyle::Ascii,
            "+------+",
            "+-----+",
            "| +-----+        +------+",
        ),
    ] {
        let outcome = termiflow::render_with_feedback(
            &input,
            termiflow::RenderOptions::new().with_style(style),
        )
        .unwrap();

        assert!(
            outcome.output.contains(source_top) && outcome.output.contains(target_top),
            "expected verified RL border-contact fixture to preserve node corners against the subgraph wall for {:?}\n{}",
            style,
            outcome.output
        );
        assert!(
            !outcome.output.contains(must_not_contain),
            "expected RL border-contact fixture to avoid clobbering box corners with a subgraph wall for {:?}\n{}",
            style,
            outcome.output
        );
        assert_eq!(
            outcome.critic_report.audit_summary().verdict,
            termiflow::AuditVerdict::Clean,
            "expected verified RL border-contact fixture to stay visually clean for {:?}\n{}",
            style,
            outcome.output
        );
    }
}

#[test]
fn render_matches_verified_collision_sibling_subgraphs_lr_snapshots() {
    let input =
        std::fs::read_to_string("tests/fixtures/inputs/collision_sibling_subgraphs_lr.md").unwrap();

    for (style, upper_crossing, lower_crossing) in [
        (
            termiflow::BaseStyle::Unicode,
            "│  Node B  ├───┼───┼",
            "└──────────────────────┼───┼─→│  Node C  ├───┘",
        ),
        (
            termiflow::BaseStyle::Ascii,
            "|  Node B  +---+---+",
            "+----------------------+---+->|  Node C  +---+",
        ),
    ] {
        let output =
            termiflow::render(&input, termiflow::RenderOptions::new().with_style(style)).unwrap();

        assert!(
            output.contains(upper_crossing) && output.contains(lower_crossing),
            "expected verified LR sibling-subgraph crossings to preserve border intersections for {:?}\n{}",
            style,
            output
        );
    }
}

#[test]
fn render_matches_verified_collision_parallel_cross_bt_snapshots() {
    let input =
        std::fs::read_to_string("tests/fixtures/inputs/collision_parallel_cross_bt.md").unwrap();

    for (style, target_crossing, source_crossing) in [
        (
            termiflow::BaseStyle::Unicode,
            "┗━━━━━━━━━━━┼━━━━━━━━━┼━━━━━━━━┛",
            "┏━━━━━━━━━┼━━━━━━━━━━━┼━━━━━━━━┓",
        ),
        (
            termiflow::BaseStyle::Ascii,
            "+-----------+---------+--------+",
            "+---------+-----------+--------+",
        ),
    ] {
        let outcome = termiflow::render_with_feedback(
            &input,
            termiflow::RenderOptions::new().with_style(style),
        )
        .unwrap();

        assert!(
            outcome.output.contains(target_crossing) && outcome.output.contains(source_crossing),
            "expected verified BT parallel-cross fixture to preserve shared border intersections for {:?}\n{}",
            style,
            outcome.output
        );
        assert_eq!(
            outcome.critic_report.audit_summary().verdict,
            termiflow::AuditVerdict::Clean,
            "expected verified BT parallel-cross fixture to stay visually clean for {:?}\n{}",
            style,
            outcome.output
        );
    }
}
