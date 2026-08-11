use super::*;

#[test]
fn render_with_feedback_coordinates_bt_multi_entry_boundary_scene() {
    let input =
        std::fs::read_to_string("tests/fixtures/inputs/collision_edge_along_border_bt.md").unwrap();

    for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
        for optimize in [false, true] {
            let outcome = termiflow::render_with_feedback(
                &input,
                termiflow::RenderOptions::new()
                    .with_style(style)
                    .with_optimize_render(optimize),
            )
            .unwrap();

            for label in ["A", "B", "C", "X1", "X2", "X3", "Target Group"] {
                assert!(
                    outcome.output.contains(label),
                    "expected {label:?} to remain visible for {style:?}, optimize={optimize}\n{}",
                    outcome.output
                );
            }
            assert_eq!(
                outcome.critic_report.audit_summary().verdict,
                termiflow::AuditVerdict::Clean,
                "expected clean BT multi-entry scene for {style:?}, optimize={optimize}\n{}",
                outcome.output
            );
            if style == termiflow::BaseStyle::Ascii {
                assert!(
                    !outcome.output.contains("+-+") && !outcome.output.contains("+ -"),
                    "expected no accidental ASCII boundary seam for optimize={optimize}\n{}",
                    outcome.output
                );
            } else {
                assert!(
                    !outcome.output.contains("┌─┘") && !outcome.output.contains("┘┌"),
                    "expected no adjacent Unicode boundary corners for optimize={optimize}\n{}",
                    outcome.output
                );
            }
        }
    }
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
        let top_border = lines
            .get(title_idx.saturating_sub(1))
            .copied()
            .expect("top border row");
        assert!(
            !top_border.contains(if style == termiflow::BaseStyle::Ascii {
                "||"
            } else {
                "││"
            }),
            "expected one portal shaft on the TD top border for {:?}, got:\n{}",
            style,
            outcome.output
        );
        let approach_row = lines
            .get(title_idx.saturating_sub(2))
            .copied()
            .expect("fan-out approach row");
        assert!(
            !approach_row.contains(if style == termiflow::BaseStyle::Ascii {
                "++"
            } else {
                "└┐"
            }),
            "expected a clean single-lane fan-out approach for {:?}, got:\n{}",
            style,
            outcome.output
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
fn render_with_feedback_keeps_narrow_td_subgraph_portal_corners_separated() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_narrow_td.md").unwrap();

    for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
        for optimize in [false, true] {
            let outcome = termiflow::render_with_feedback(
                &input,
                termiflow::RenderOptions::new()
                    .with_style(style)
                    .with_optimize_render(optimize),
            )
            .unwrap();

            assert_eq!(
                outcome.critic_report.audit_summary().verdict,
                termiflow::AuditVerdict::Clean,
                "expected narrow TD subgraph portal route to be clean for {style:?}, optimize={optimize}\n{}",
                outcome.output
            );
            if style == termiflow::BaseStyle::Ascii {
                assert!(
                    !outcome.output.contains("++"),
                    "expected no doubled ASCII border cells for optimize={optimize}\n{}",
                    outcome.output
                );
            } else {
                assert!(
                    !outcome.output.contains("└┐") && !outcome.output.contains("┌┘"),
                    "expected no adjacent Unicode boundary corners for optimize={optimize}\n{}",
                    outcome.output
                );
            }
        }
    }
}

#[test]
fn render_with_feedback_keeps_direct_td_sibling_turn_in_the_gap() {
    let fixtures = [
        (
            "tests/fixtures/inputs/collision_sibling_tight_td.md",
            "S1",
            "S2",
            ["S1", "S2", "A", "B"],
        ),
        (
            "tests/fixtures/inputs/subgraph_direct_td.md",
            "Group 1",
            "Group 2",
            ["Group 1", "Group 2", "Node A", "Node B"],
        ),
    ];

    for (fixture, upper_title, lower_title, labels) in fixtures {
        let input = std::fs::read_to_string(fixture).expect("read direct TD fixture");
        for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
            for optimize in [false, true] {
                let outcome = termiflow::render_with_feedback(
                    &input,
                    termiflow::RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimize),
                )
                .unwrap();
                let lines: Vec<&str> = outcome.output.lines().collect();
                let horizontal = if style == termiflow::BaseStyle::Ascii {
                    '-'
                } else {
                    '━'
                };
                let has_long_border =
                    |line: &str| line.chars().filter(|ch| *ch == horizontal).count() >= 10;
                let upper_title_row = lines
                    .iter()
                    .position(|line| line.contains(upper_title))
                    .expect("upper title row");
                let lower_title_row = lines
                    .iter()
                    .position(|line| line.contains(lower_title))
                    .expect("lower title row");
                let borders: Vec<usize> = (upper_title_row + 1..lower_title_row)
                    .filter(|index| has_long_border(lines[*index]))
                    .collect();
                assert!(
                    borders.len() >= 2,
                    "expected both direct sibling borders for {fixture}, style={style:?}, optimize={optimize}\n{}",
                    outcome.output
                );
                let upper_border = borders[0];
                let lower_border = *borders.last().expect("lower sibling border");
                let is_corridor_turn = |line: &str| {
                    if style == termiflow::BaseStyle::Ascii {
                        line.chars().filter(|ch| *ch == '+').count() >= 2
                            && line.chars().any(|ch| ch == '-')
                    } else {
                        line.contains('└') && line.contains('┐') && line.contains('─')
                    }
                };
                let turn_row = (upper_border + 1..lower_border)
                    .find(|index| is_corridor_turn(lines[*index]))
                    .unwrap_or_else(|| {
                        panic!(
                            "direct corridor turn row missing for {fixture}, style={style:?}, optimize={optimize}; borders=({upper_border}, {lower_border})\n{}",
                            outcome.output
                        )
                    });

                assert!(
                    turn_row - upper_border >= 2 && lower_border - turn_row >= 2,
                    "expected direct TD turn to have a straight row before each sibling border for {fixture}, style={style:?}, optimize={optimize}; turn={turn_row}, borders=({upper_border}, {lower_border})\n{}",
                    outcome.output
                );
                for label in labels {
                    assert!(
                        outcome.output.contains(label),
                        "expected {label:?} to remain visible for {fixture}, style={style:?}, optimize={optimize}\n{}",
                        outcome.output
                    );
                }
                assert_eq!(
                    outcome.critic_report.audit_summary().verdict,
                    termiflow::AuditVerdict::Clean,
                    "expected clean direct TD sibling corridor for {fixture}, style={style:?}, optimize={optimize}\n{}",
                    outcome.output
                );
            }
        }
    }
}

#[test]
fn render_with_feedback_gives_stacked_td_siblings_two_connector_rows() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/collision_sibling_triple_td.md")
        .expect("read triple sibling fixture");

    for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
        for optimize in [false, true] {
            let outcome = termiflow::render_with_feedback(
                &input,
                termiflow::RenderOptions::new()
                    .with_style(style)
                    .with_optimize_render(optimize),
            )
            .unwrap();
            let lines: Vec<&str> = outcome.output.lines().collect();
            let horizontal = if style == termiflow::BaseStyle::Ascii {
                '-'
            } else {
                '━'
            };
            let arrow = if style == termiflow::BaseStyle::Ascii {
                'v'
            } else {
                '↓'
            };
            let has_long_border =
                |line: &str| line.chars().filter(|ch| *ch == horizontal).count() >= 10;

            for (upper, lower) in [("Group 1", "Group 2"), ("Group 2", "Group 3")] {
                let upper_title = lines
                    .iter()
                    .position(|line| line.contains(upper))
                    .expect("upper group title");
                let lower_title = lines
                    .iter()
                    .position(|line| line.contains(lower))
                    .expect("lower group title");
                let borders: Vec<usize> = (upper_title + 1..lower_title)
                    .filter(|index| has_long_border(lines[*index]))
                    .collect();
                assert!(
                    borders.len() >= 2,
                    "expected both sibling border rows for {upper}->{lower}, style={style:?}, optimize={optimize}\n{}",
                    outcome.output
                );
                let gap = borders[borders.len() - 1]
                    .saturating_sub(borders[0])
                    .saturating_sub(1);
                assert!(
                    gap >= 2,
                    "expected two connector rows for {upper}->{lower}, got {gap}, style={style:?}, optimize={optimize}\n{}",
                    outcome.output
                );
            }

            for label in [
                "Group 1", "Group 2", "Group 3", "A1", "A2", "B1", "B2", "C1", "C2",
            ] {
                assert!(
                    outcome.output.contains(label),
                    "expected {label:?} to remain visible for {style:?}, optimize={optimize}\n{}",
                    outcome.output
                );
            }
            let title_edge = if style == termiflow::BaseStyle::Ascii {
                '|'
            } else {
                '│'
            };
            for title in ["Group 2", "Group 3"] {
                let title_line = lines
                    .iter()
                    .find(|line| line.contains(title))
                    .expect("sibling title row");
                let edge_count = title_line.chars().filter(|ch| *ch == title_edge).count();
                let minimum = if style == termiflow::BaseStyle::Ascii {
                    3
                } else {
                    1
                };
                assert!(
                    edge_count >= minimum,
                    "expected a visible title-safe sibling portal beside {title} for {style:?}, optimize={optimize}\n{}",
                    outcome.output
                );
            }
            assert!(
                outcome.output.chars().filter(|ch| *ch == arrow).count() >= 5,
                "expected all five TD arrows for {style:?}, optimize={optimize}\n{}",
                outcome.output
            );
            assert_eq!(
                outcome.critic_report.audit_summary().verdict,
                termiflow::AuditVerdict::Clean,
                "expected clean stacked TD siblings for {style:?}, optimize={optimize}\n{}",
                outcome.output
            );
        }
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
fn render_with_feedback_keeps_complex_td_data_layer_bottom_exit_portals_distinct() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_td.md").unwrap();
    let parsed = termiflow::parse(&input, false).unwrap();
    let graph = termiflow::coarse_waterfall(parsed.graph).unwrap();
    let data = graph.get_subgraph("SG2").expect("data layer");

    let outcome =
        termiflow::render_canvas_with_feedback(&graph, &termiflow::Config::default()).unwrap();
    let bottom_y = data.bounds.y + data.bounds.height.saturating_sub(1);
    let portal_shaft = termiflow::CompositeStyle::from_base(termiflow::BaseStyle::Unicode)
        .to_style_chars(termiflow::BaseStyle::Unicode)
        .edge_v;
    let portal_count = (data.bounds.x..data.bounds.x + data.bounds.width)
        .filter_map(|x| outcome.semantic_frame.get(x, bottom_y))
        .filter(|cell| {
            cell.owner_kind == termiflow::render::semantic::CellOwnerKind::PortalOpening
                && cell.ch == portal_shaft
        })
        .count();

    assert_eq!(
        portal_count, 2,
        "expected the TD Data Layer fan-in to leave two distinct bottom exit portals\n{}",
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
    let portal_shaft = termiflow::CompositeStyle::from_base(termiflow::BaseStyle::Unicode)
        .to_style_chars(termiflow::BaseStyle::Unicode)
        .edge_v;
    let portal_count = (data.bounds.x..data.bounds.x + data.bounds.width)
        .filter_map(|x| outcome.semantic_frame.get(x, top_y))
        .filter(|cell| {
            cell.owner_kind == termiflow::render::semantic::CellOwnerKind::PortalOpening
                && cell.ch == portal_shaft
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
    let portal_shaft = termiflow::CompositeStyle::from_base(termiflow::BaseStyle::Unicode)
        .to_style_chars(termiflow::BaseStyle::Unicode)
        .edge_v;
    let edge_owned_cells = (inner.bounds.x..inner.bounds.x + inner.bounds.width)
        .filter_map(|x| outcome.semantic_frame.get(x, bottom_y))
        .filter(|cell| {
            cell.owner_kind == termiflow::render::semantic::CellOwnerKind::PortalOpening
                && cell.ch == portal_shaft
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
    let portal_shaft = termiflow::CompositeStyle::from_base(termiflow::BaseStyle::Unicode)
        .to_style_chars(termiflow::BaseStyle::Unicode)
        .edge_v;
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
        top_border.chars().filter(|&ch| ch == portal_shaft).count(),
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
            let parsed = termiflow::parse(&input, false).unwrap();
            let graph = termiflow::coarse_waterfall(parsed.graph).unwrap();
            let _inner = graph.get_subgraph("SG2").expect("data layer");
            let outcome = termiflow::render_with_feedback(
                &input,
                termiflow::RenderOptions::new().with_style(style),
            )
            .unwrap();
            let mut used_side_portals = 0;
            for y in 0..outcome.semantic_frame.height {
                for x in 0..outcome.semantic_frame.width {
                    if outcome.semantic_frame.get(x, y).is_some_and(|cell| {
                        cell.owner_kind == termiflow::render::semantic::CellOwnerKind::PortalOpening
                            && cell.ch == portal_marker
                    }) {
                        used_side_portals += 1;
                    }
                }
            }

            assert!(
                used_side_portals >= 1,
                "expected the declared nested horizontal side-entry to keep a visible side-aware portal for {direction} in {:?}, got {}\n{}",
                style,
                used_side_portals,
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
                    && cell.ch == portal_marker
            })
            .expect("expected dedicated nested child exit portal on the fan-in border");

        assert!(
            portal_y > min_source_y && portal_y < max_source_y,
            "expected the nested fan-in exit portal to stay centered between source rows for {direction}, got y={} with source rows {} and {}\n{}",
            portal_y,
            min_source_y,
            max_source_y,
            outcome.output
        );
        assert!(
            portal.ch == portal_marker,
            "expected the centered merge portal to use the dedicated side marker for {direction}, got '{}'\n{}",
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
                            && cell.ch == portal_marker)
                            .then_some((y, cell.ch))
                    })
                })
                .collect();

            assert_eq!(
                used_side_portals.len(),
                1,
                "expected one clean side portal for declared nested fan-in in {direction} / {:?}\n{}",
                style,
                outcome.output
            );
            assert!(
                used_side_portals[0].1 == portal_marker,
                "expected the declared nested exit portal to use the dedicated side marker for {direction} in {:?}, got '{}'\n{}",
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
fn render_with_feedback_keeps_bt_parallel_scene_boundary_and_fanout_cells_clean() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_parallel_bt.md").unwrap();

    for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
        let outcome = termiflow::render_with_feedback(
            &input,
            termiflow::RenderOptions::new().with_style(style),
        )
        .unwrap();

        assert_eq!(
            outcome.critic_report.audit_summary().verdict,
            termiflow::AuditVerdict::Clean,
            "expected clean BT parallel-scene output for {:?}\n{}",
            style,
            outcome.output
        );

        match style {
            termiflow::BaseStyle::Ascii => {
                assert!(outcome
                    .output
                    .contains("+-----------------|----------------+"));
                assert!(outcome.output.contains("+-------+-------+"));
                assert!(!outcome.output.contains("+-+"));
            }
            termiflow::BaseStyle::Unicode => {
                assert!(outcome
                    .output
                    .lines()
                    .any(|line| line.contains("Process") && line.contains('│')));
                assert!(outcome.output.contains("└───────┬───────┘"));
                assert!(!outcome.output.contains("├─┘"));
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn render_with_feedback_keeps_bt_parallel_sibling_crossings_off_boundary_elbows() {
    let input =
        std::fs::read_to_string("tests/fixtures/inputs/collision_parallel_cross_bt.md").unwrap();

    for style in [termiflow::BaseStyle::Ascii, termiflow::BaseStyle::Unicode] {
        for optimize in [false, true] {
            let outcome = termiflow::render_with_feedback(
                &input,
                termiflow::RenderOptions::new()
                    .with_style(style)
                    .with_optimize_render(optimize),
            )
            .unwrap();

            assert_eq!(
                outcome.critic_report.audit_summary().verdict,
                termiflow::AuditVerdict::Clean,
                "expected clean BT sibling-crossing output for {:?}, optimize={optimize}\n{}",
                style,
                outcome.output
            );
            assert!(
                outcome.output.contains("A1")
                    && outcome.output.contains("A2")
                    && outcome.output.contains("B1")
                    && outcome.output.contains("B2")
                    && outcome.output.contains("Source")
                    && outcome.output.contains("Target"),
                "expected all sibling-crossing labels to remain readable for {:?}, optimize={optimize}\n{}",
                style,
                outcome.output
            );

            match style {
                termiflow::BaseStyle::Ascii => assert!(
                    !outcome.output.contains("+-+"),
                    "expected no one-cell ASCII boundary elbow for optimize={optimize}\n{}",
                    outcome.output
                ),
                termiflow::BaseStyle::Unicode => assert!(
                    !outcome.output.contains("┌─┘"),
                    "expected no one-cell Unicode boundary elbow for optimize={optimize}\n{}",
                    outcome.output
                ),
                _ => unreachable!(),
            }
        }
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
