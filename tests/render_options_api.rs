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
        let stem_row = lines.get(title_idx + 1).copied().expect("row below title");
        let row_width = stem_row.chars().count();
        let interior_verticals = stem_row
            .chars()
            .enumerate()
            .filter(|(idx, ch)| {
                *idx > 0 && *idx + 1 < row_width && matches!(ch, '|' | '│' | ':' | '┃' | '║')
            })
            .count();

        assert_eq!(
            interior_verticals, 1,
            "expected one interior entry stem below titled subgraph for {:?}, got row:\n{}",
            style, stem_row
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
fn render_with_feedback_preserves_visually_nested_inner_subgraph_border_cells() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_td.md").unwrap();
    let parsed = termiflow::parse(&input, false).unwrap();
    let graph = termiflow::coarse_waterfall(parsed.graph).unwrap();

    let inner = graph.get_subgraph("SG2").expect("inner subgraph");
    let overlapping_node = graph.get_node("S2").expect("overlapping node");
    assert!(
        overlapping_node.y > inner.bounds.y
            && overlapping_node.y < inner.bounds.y + inner.bounds.height.saturating_sub(1),
        "expected S2 row to overlap SG2 vertical span: sg={:?} node=({}, {})",
        inner.bounds,
        overlapping_node.x,
        overlapping_node.y
    );

    let outcome =
        termiflow::render_canvas_with_feedback(&graph, &termiflow::Config::default()).unwrap();
    let cell = outcome
        .semantic_frame
        .get(inner.bounds.x, overlapping_node.y)
        .expect("border cell");

    assert_eq!(
        cell.owner_kind,
        termiflow::render::semantic::CellOwnerKind::SubgraphBorder,
        "expected inner subgraph left border to survive visual nesting overlap\n{}",
        outcome.output
    );
}

#[test]
fn render_with_feedback_keeps_nested_child_bottom_border_clean_after_fanin() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_td.md").unwrap();
    let parsed = termiflow::parse(&input, false).unwrap();
    let graph = termiflow::coarse_waterfall(parsed.graph).unwrap();
    let inner = graph.get_subgraph("SG2").expect("inner subgraph");

    let outcome =
        termiflow::render_canvas_with_feedback(&graph, &termiflow::Config::default()).unwrap();
    let bottom_y = inner.bounds.y + inner.bounds.height.saturating_sub(1);
    let edge_owned_cells = (inner.bounds.x..inner.bounds.x + inner.bounds.width)
        .filter_map(|x| outcome.semantic_frame.get(x, bottom_y))
        .filter(|cell| cell.owner_kind == termiflow::render::semantic::CellOwnerKind::EdgeSegment)
        .count();

    assert_eq!(
        edge_owned_cells, 1,
        "expected the nested child bottom border to expose a single exit portal after fan-in routing\n{}",
        outcome.output
    );
}

#[test]
fn render_with_feedback_keeps_nested_child_fanin_spine_off_left_wall() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_td.md").unwrap();
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
            data_idx > service_idx,
            "expected complex BT titles on separate rows with the outer title below the inner title for {:?}\n{}",
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
fn render_with_feedback_places_supported_bt_subgraph_titles_on_bottom_edge() {
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
                lines.len().saturating_sub(1),
                "expected BT title on the bottom border row for fixture {} in {:?}\n{}",
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

    for (style, must_contain, must_not_contain) in [
        (
            termiflow::BaseStyle::Unicode,
            "┌──────┐        ┌─────┐",
            "┌──────│        ┌─────┐",
        ),
        (
            termiflow::BaseStyle::Ascii,
            "+------+        +-----+",
            "+------|        +-----+",
        ),
    ] {
        let output =
            termiflow::render(&input, termiflow::RenderOptions::new().with_style(style)).unwrap();

        assert!(
            output.contains(must_contain),
            "expected verified LR border-contact fixture to preserve node corners against the subgraph wall for {:?}\n{}",
            style,
            output
        );
        assert!(
            !output.contains(must_not_contain),
            "expected LR border-contact fixture to avoid clobbering box corners with a subgraph wall for {:?}\n{}",
            style,
            output
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
            "├─────┬────────────────┐",
            "└────────────────────┼────→",
        ),
        (
            termiflow::BaseStyle::Ascii,
            "+-----+----------------+",
            "+--------------------+---->",
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

    for (style, source_border_crossing) in [
        (termiflow::BaseStyle::Unicode, "┏━━━━━┼━━━━━━━━━━━┼━━━━┓"),
        (termiflow::BaseStyle::Ascii, "+-----+-----------+----+"),
    ] {
        let output =
            termiflow::render(&input, termiflow::RenderOptions::new().with_style(style)).unwrap();

        assert!(
            output.contains(source_border_crossing),
            "expected verified BT parallel-cross fixture to preserve shared border intersections for {:?}\n{}",
            style,
            output
        );
    }
}
