use super::*;

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
            let visible_used_portals: Vec<()> = (0..frame.height)
                .flat_map(|y| {
                    (0..frame.width).filter_map(move |x| {
                        let cell = frame.get(x, y)?;
                        (cell.owner_kind
                            == termiflow::render::semantic::CellOwnerKind::PortalOpening
                            && ((y > 0 && is_route_neighbor(frame, x, y - 1))
                                || (y + 1 < frame.height && is_route_neighbor(frame, x, y + 1))
                                || (x > 0 && is_route_neighbor(frame, x - 1, y))
                                || (x + 1 < frame.width && is_route_neighbor(frame, x + 1, y))))
                        .then_some(())
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
fn render_with_feedback_preserves_rl_labeled_edge_arrowheads() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_labels_rl.md").unwrap();

    for (style, arrow) in [
        (termiflow::BaseStyle::Ascii, '<'),
        (termiflow::BaseStyle::Unicode, '←'),
    ] {
        let outcome = termiflow::render_with_feedback(
            &input,
            termiflow::RenderOptions::new().with_style(style),
        )
        .unwrap();

        assert!(
            outcome.output.contains("submit") && outcome.output.contains("success"),
            "expected both labeled RL edges to remain readable in {:?}\n{}",
            style,
            outcome.output
        );
        assert_eq!(
            outcome
                .output
                .chars()
                .filter(|character| *character == arrow)
                .count(),
            2,
            "expected both labeled RL edges to retain visible arrowheads in {:?}\n{}",
            style,
            outcome.output
        );
    }
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

    for style in [termiflow::BaseStyle::Unicode, termiflow::BaseStyle::Ascii] {
        let parsed = termiflow::parse(&input, false).unwrap();
        let graph = termiflow::coarse_waterfall(parsed.graph).unwrap();
        let portal_marker = termiflow::CompositeStyle::from_base(style)
            .to_style_chars(style)
            .portal_pierce;
        let outcome = termiflow::render_with_feedback(
            &input,
            termiflow::RenderOptions::new().with_style(style),
        )
        .unwrap();
        let used_horizontal_portals = graph
            .subgraphs
            .iter()
            .flat_map(|subgraph| {
                let bounds = &subgraph.bounds;
                let left_x = bounds.x;
                let right_x = bounds.x + bounds.width.saturating_sub(1);
                let y_start = bounds.y.saturating_add(1);
                let y_end = bounds.y + bounds.height.saturating_sub(1);
                (y_start..y_end).flat_map(move |y| [(left_x, y), (right_x, y)])
            })
            .filter(|&(x, y)| {
                outcome.semantic_frame.get(x, y).is_some_and(|cell| {
                    cell.owner_kind == termiflow::render::semantic::CellOwnerKind::PortalOpening
                        && cell.ch == portal_marker
                })
            })
            .count();

        assert!(
            used_horizontal_portals >= 2,
            "expected verified LR sibling-subgraph crossings to use dedicated side portals for {:?}, got {}\n{}",
            style,
            used_horizontal_portals,
            outcome.output
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
            "┗━━━━━━━━━━━│━━━━━━━━━│━━━━━━━━┛",
            "┏━━━━━━━━━│━━━━━━━━━━━│━━━━━━━━┓",
        ),
        (
            termiflow::BaseStyle::Ascii,
            "+-----------|---------|--------+",
            "+---------|-----------|--------+",
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
