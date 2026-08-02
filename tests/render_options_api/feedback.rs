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
fn vertical_long_edge_label_is_bounded_without_silent_clipping() {
    let input = "graph BT\nA[Start] -->|This is a very long edge label text| B[End]";
    let output = termiflow::render(
        input,
        termiflow::RenderOptions::new().with_style(termiflow::BaseStyle::Ascii),
    )
    .unwrap();

    assert!(
        output.contains('…'),
        "expected a visible ellipsis when the vertical label exceeds the canvas width:\n{output}"
    );
    assert!(
        !output.lines().any(|line| line.trim() == "This is a very"),
        "the edge label must not be silently clipped to a misleading prefix:\n{output}"
    );
}

#[test]
fn horizontal_edge_labels_stay_readable_when_the_route_has_no_margin() {
    for (direction, input) in [
        ("LR", include_str!("../fixtures/inputs/label_basic_lr.md")),
        ("RL", include_str!("../fixtures/inputs/label_basic_rl.md")),
    ] {
        for optimize_render in [false, true] {
            let output = termiflow::render(
                input,
                termiflow::RenderOptions::new()
                    .with_style(termiflow::BaseStyle::Ascii)
                    .with_optimize_render(optimize_render),
            )
            .unwrap();

            for label in ["validate", "success", "error"] {
                assert!(
                    output.contains(label),
                    "expected complete {direction} edge label {label:?} with optimize_render={optimize_render}:\n{output}"
                );
                let label_line = output
                    .lines()
                    .find(|line| line.contains(label))
                    .expect("complete edge label should occupy a rendered line");
                assert!(
                    label_line.contains('-'),
                    "{direction} edge label {label:?} must remain attached to its route instead of floating on a blank row with optimize_render={optimize_render}:\n{output}"
                );
            }
            assert!(
                !output.contains("succ…") && !output.contains("v…"),
                "{direction} edge labels must not silently truncate when the raw route span can fit them with no margin:\n{output}"
            );
        }
    }
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
fn render_with_feedback_skips_false_positive_label_repairs() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/label_junction_bt.md").unwrap();
    let outcome = termiflow::render_with_feedback(
        &input,
        termiflow::RenderOptions::new().with_optimize_render(true),
    )
    .unwrap();

    assert_eq!(outcome.layout_attempts, 1);
    assert_eq!(outcome.layout_repairs_applied, 0);
    assert!(!outcome
        .critic_report
        .findings
        .iter()
        .any(|finding| finding.code == termiflow::FindingCode::CrowdedEdgeLabel));
}

#[test]
fn render_with_feedback_preserves_and_applies_fanout_layout_repairs() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/junction_quad_td.md").unwrap();
    let baseline =
        termiflow::render_with_feedback(&input, termiflow::RenderOptions::default()).unwrap();
    let optimized = termiflow::render_with_feedback(
        &input,
        termiflow::RenderOptions::new().with_optimize_render(true),
    )
    .unwrap();

    let baseline_finding = baseline
        .critic_report
        .findings
        .iter()
        .find(|finding| finding.code == termiflow::FindingCode::RouteSymmetryImbalance)
        .expect("baseline should expose the skewed fan-out");

    assert!(baseline_finding.message.contains("by 7 cell(s)"));
    assert!(optimized.layout_repairs_applied >= 1);
    assert!(optimized.layout_attempts > 1);
    assert_ne!(optimized.output, baseline.output);
    assert!(optimized.critic_report.findings.iter().all(|finding| {
        finding.code != termiflow::FindingCode::RouteSymmetryImbalance
            || !finding.message.contains("by 7 cell(s)")
    }));
}
