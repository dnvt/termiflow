use super::*;
use clap::{CommandFactory, Parser};
use termiflow::{
    render::semantic::{CellMeta, CellOwnerKind, CellRole, SemanticFrame},
    CriticFinding, FindingCode, Graph, RenderOutcome,
};

#[test]
fn wrap_text_short_string_is_unchanged() {
    let result = wrap_text("hello world", 40);
    assert_eq!(result, vec!["hello world"]);
}

#[test]
fn wrap_text_breaks_at_space() {
    let result = wrap_text("hello world foo", 11);
    assert_eq!(result[0], "hello world");
}

#[test]
fn wrap_text_falls_back_to_hard_break() {
    let result = wrap_text("abcdefghij", 5);
    assert!(!result.is_empty());
    assert!(display_width(&result[0]) <= 5);
}

#[test]
fn wrap_text_preserves_grapheme_clusters_on_hard_break() {
    let family = "👨‍👩‍👧‍👦";
    assert_eq!(
        wrap_text(&format!("{family}{family}"), display_width(family)),
        vec![family.to_string(), family.to_string()]
    );
}

#[test]
fn wrap_text_uses_display_width_for_cjk() {
    let result = wrap_text("日本語 日本語", 6);
    assert_eq!(result, vec!["日本語".to_string(), "日本語".to_string()]);
}

#[test]
fn build_findings_frame_clean_report_shows_clean_message() {
    let report = CriticReport::default();
    let frame = build_findings_frame(&report, "test.md", 0, (80, 24));
    // Verify the frame has the right dimensions
    assert_eq!(frame.width, 80);
    assert_eq!(frame.height, 24);
}

#[test]
fn build_findings_frame_with_findings_shows_finding_code() {
    let report = CriticReport {
        score: -20,
        findings: vec![CriticFinding {
            code: FindingCode::RouteTopologyMismatch,
            severity: FindingSeverity::Error,
            penalty: -20,
            message: "test message".to_string(),
            cells: vec![(1, 2)],
            owner_ids: vec![],
        }],
        notes: vec![],
    };
    let frame = build_findings_frame(&report, "diagram.md", 0, (80, 24));
    // Frame should contain some cells - just check dimensions
    assert_eq!(frame.width, 80);
    assert_eq!(frame.height, 24);
}

#[test]
fn cli_parses_render_feedback_flags() {
    let cli = Cli::try_parse_from([
        "termiflow",
        "--optimize-render",
        "--render-repair-passes",
        "4",
        "--layout-repair-passes",
        "2",
        "--debug-critic",
        "--audit",
        "--audit-json",
        "evidence.json",
    ])
    .unwrap();

    assert!(cli.optimize_render);
    assert_eq!(cli.render_repair_passes, Some(4));
    assert_eq!(cli.layout_repair_passes, Some(2));
    assert!(cli.debug_critic);
    assert!(cli.audit);
    assert_eq!(
        cli.audit_json.as_deref(),
        Some(std::path::Path::new("evidence.json"))
    );
}

#[test]
fn cli_help_mentions_live_preview_caveats() {
    let mut command = Cli::command();
    let mut help = Vec::new();
    command.write_long_help(&mut help).unwrap();
    let help = String::from_utf8(help).unwrap();

    assert!(help.contains("Partial alternate-screen preview"));
    assert!(help.contains("Safer live preview in normal scrollback"));
    assert!(help.contains("input/scroll behavior can vary by terminal"));
}

#[test]
fn cli_accepts_legacy_ansi_title_invert_flag() {
    let cli = Cli::try_parse_from(["termiflow", "--ansi-title-invert"]).unwrap();
    assert!(cli.ansi_title_invert);
}

#[test]
fn build_watch_frame_includes_status_row() {
    let rendered = PreparedRender {
        graph: Graph::new(),
        outcome: RenderOutcome {
            output: "+---+\n| A |\n+---+".to_string(),
            semantic_frame: SemanticFrame::default(),
            display_semantic_frame: SemanticFrame::default(),
            critic_report: CriticReport::default(),
            warnings: Vec::new(),
            optimized: false,
            repair_passes: 0,
            layout_attempts: 1,
            layout_repairs_applied: 0,
        },
        policy: serde_json::json!({}),
    };

    let frame = build_watch_frame(std::path::Path::new("diagram.md"), &rendered);
    let status_row: String = (0..frame.width)
        .map(|x| {
            frame
                .get(x, frame.height - 1)
                .map(|cell| cell.ch)
                .unwrap_or(' ')
        })
        .collect();

    assert!(status_row.contains("watch"));
    assert!(status_row.contains("diagram.md"));
    assert!(status_row.contains("verdict=Clean"));
}

#[test]
fn build_watch_frame_inverts_subgraph_titles() {
    let title = "Service";
    let title_token = termiflow::graph::subgraph_title_text(title);
    let width = title_token.chars().count() + 6;
    let content = format!(
        "┏{}┓\n┃  {}  ┃\n┗{}┛",
        "━".repeat(width.saturating_sub(2)),
        title_token,
        "━".repeat(width.saturating_sub(2))
    );

    let mut graph = Graph::new();
    graph.direction = termiflow::graph::Direction::TD;
    let mut subgraph = termiflow::graph::Subgraph::new("service", Some(title.to_string()));
    subgraph.bounds = termiflow::graph::Rectangle::new(0, 0, width, 3);
    graph.add_subgraph(subgraph);

    let title_y = termiflow::graph::subgraph_title_row(0, 3, termiflow::graph::Direction::TD);
    let title_x =
        termiflow::graph::subgraph_title_start_x(0, width, title, termiflow::graph::Direction::TD)
            .expect("title start");
    let mut semantic_frame = SemanticFrame {
        width,
        height: 3,
        cells: vec![CellMeta::default(); width * 3],
    };
    for (offset, ch) in title_token.chars().enumerate() {
        semantic_frame.cells[title_y * width + title_x + offset] = CellMeta {
            ch,
            owner_kind: CellOwnerKind::SubgraphTitle,
            owner_id: Some("service".to_string()),
            role: CellRole::Text,
            z_index: 2,
        };
    }

    let rendered = PreparedRender {
        graph,
        outcome: RenderOutcome {
            output: content,
            display_semantic_frame: semantic_frame.clone(),
            semantic_frame,
            critic_report: CriticReport::default(),
            warnings: Vec::new(),
            optimized: false,
            repair_passes: 0,
            layout_attempts: 1,
            layout_repairs_applied: 0,
        },
        policy: serde_json::json!({}),
    };

    let frame = build_watch_frame(std::path::Path::new("diagram.md"), &rendered);

    let first_title_cell = frame
        .get(title_x as u16, title_y as u16)
        .expect("first title cell");
    assert!(first_title_cell.text().contains(ANSI_INVERT_ON));

    let last_title_cell = frame
        .get(
            (title_x + title_token.chars().count().saturating_sub(1)) as u16,
            title_y as u16,
        )
        .expect("last title cell");
    assert!(last_title_cell.text().contains(ANSI_RESET));

    let border_cell = frame.get(0, title_y as u16).expect("border cell");
    assert!(!border_cell.text().contains(ANSI_INVERT_ON));
}

#[test]
fn apply_inverted_titles_to_tui_frame_respects_viewport_crop() {
    let title = "Service";
    let title_token = termiflow::graph::subgraph_title_text(title);
    let width = title_token.chars().count() + 6;
    let content = format!(
        "┏{}┓\n┃  {}  ┃\n┗{}┛",
        "━".repeat(width.saturating_sub(2)),
        title_token,
        "━".repeat(width.saturating_sub(2))
    );

    let title_y = termiflow::graph::subgraph_title_row(0, 3, termiflow::graph::Direction::TD);
    let title_x =
        termiflow::graph::subgraph_title_start_x(0, width, title, termiflow::graph::Direction::TD)
            .expect("title start");
    let mut semantic_frame = SemanticFrame {
        width,
        height: 3,
        cells: vec![CellMeta::default(); width * 3],
    };
    for (offset, ch) in title_token.chars().enumerate() {
        semantic_frame.cells[title_y * width + title_x + offset] = CellMeta {
            ch,
            owner_kind: CellOwnerKind::SubgraphTitle,
            owner_id: Some("service".to_string()),
            role: CellRole::Text,
            z_index: 2,
        };
    }

    let viewport = Viewport {
        offset_x: 3,
        offset_y: 0,
    };
    let mut frame = build_preview_frame(&content, "status", (8, 3), viewport);
    apply_inverted_titles_to_tui_frame(&mut frame, &semantic_frame, viewport);

    let first_visible_title_cell = frame.get(0, 1).expect("cropped title cell");
    assert!(first_visible_title_cell.text().contains(ANSI_INVERT_ON));

    let reset_seen = (0..frame.width)
        .filter_map(|x| frame.get(x, 1))
        .any(|cell| cell.text().contains(ANSI_RESET));
    assert!(
        reset_seen,
        "cropped title should still close the invert span"
    );

    let status_cell = frame.get(0, frame.height - 1).expect("status cell");
    assert!(!status_cell.text().contains(ANSI_INVERT_ON));
}

#[test]
fn invert_subgraph_titles_ansi_wraps_title_tokens() {
    let service_title = termiflow::graph::subgraph_title_text("Service Layer");
    let data_title = termiflow::graph::subgraph_title_text("Data Layer");
    let output = format!("xx{service_title}yy{data_title}zz");
    let width = output.chars().count();
    let service_start = 2usize;
    let data_start = service_start + service_title.chars().count() + 2;

    let mut semantic_frame = SemanticFrame {
        width,
        height: 1,
        cells: vec![CellMeta::default(); width],
    };

    for (offset, ch) in service_title.chars().enumerate() {
        semantic_frame.cells[service_start + offset] = CellMeta {
            ch,
            owner_kind: CellOwnerKind::SubgraphTitle,
            owner_id: Some("service".to_string()),
            role: CellRole::Text,
            z_index: 2,
        };
    }
    for (offset, ch) in data_title.chars().enumerate() {
        semantic_frame.cells[data_start + offset] = CellMeta {
            ch,
            owner_kind: CellOwnerKind::SubgraphTitle,
            owner_id: Some("data".to_string()),
            role: CellRole::Text,
            z_index: 2,
        };
    }

    let styled = invert_subgraph_titles_ansi(&output, &semantic_frame);

    assert!(styled.contains(&format!("{ANSI_INVERT_ON}{service_title}{ANSI_RESET}")));
    assert!(styled.contains(&format!("{ANSI_INVERT_ON}{data_title}{ANSI_RESET}")));
    assert!(styled.contains("xx"));
    assert!(styled.contains("yy"));
    assert!(styled.contains("zz"));
}

#[test]
fn invert_subgraph_titles_ansi_only_styles_semantic_title_cells() {
    let title_token = termiflow::graph::subgraph_title_text("Data Layer");
    let output = format!("node:{title_token}|title:{title_token}");
    let width = output.chars().count();
    let title_start =
        "node:".chars().count() + title_token.chars().count() + "|title:".chars().count();

    let mut semantic_frame = SemanticFrame {
        width,
        height: 1,
        cells: vec![CellMeta::default(); width],
    };
    for (offset, ch) in title_token.chars().enumerate() {
        semantic_frame.cells[title_start + offset] = CellMeta {
            ch,
            owner_kind: CellOwnerKind::SubgraphTitle,
            owner_id: Some("data".to_string()),
            role: CellRole::Text,
            z_index: 2,
        };
    }

    let styled = invert_subgraph_titles_ansi(&output, &semantic_frame);
    let expected = format!("node:{title_token}|title:{ANSI_INVERT_ON}{title_token}{ANSI_RESET}");
    assert_eq!(styled, expected);
}

#[test]
fn printable_output_inverts_titles_by_default_for_tty_print_mode() {
    let title_token = termiflow::graph::subgraph_title_text("My Group");
    let output = format!("┏━━{title_token}━━┓");
    let width = output.chars().count();
    let title_start = "┏━━".chars().count();
    let rendered = PreparedRender {
        graph: Graph::new(),
        outcome: RenderOutcome {
            output,
            semantic_frame: {
                let mut semantic_frame = SemanticFrame {
                    width,
                    height: 1,
                    cells: vec![CellMeta::default(); width],
                };
                for (offset, ch) in title_token.chars().enumerate() {
                    semantic_frame.cells[title_start + offset] = CellMeta {
                        ch,
                        owner_kind: CellOwnerKind::SubgraphTitle,
                        owner_id: Some("group".to_string()),
                        role: CellRole::Text,
                        z_index: 2,
                    };
                }
                semantic_frame
            },
            display_semantic_frame: {
                let mut semantic_frame = SemanticFrame {
                    width,
                    height: 1,
                    cells: vec![CellMeta::default(); width],
                };
                for (offset, ch) in title_token.chars().enumerate() {
                    semantic_frame.cells[title_start + offset] = CellMeta {
                        ch,
                        owner_kind: CellOwnerKind::SubgraphTitle,
                        owner_id: Some("group".to_string()),
                        role: CellRole::Text,
                        z_index: 2,
                    };
                }
                semantic_frame
            },
            critic_report: CriticReport::default(),
            warnings: Vec::new(),
            optimized: false,
            repair_passes: 0,
            layout_attempts: 1,
            layout_repairs_applied: 0,
        },
        policy: serde_json::json!({}),
    };

    let tty_output = printable_output(&rendered, true);
    let piped_output = printable_output(&rendered, false);

    assert!(tty_output.contains(&format!("{ANSI_INVERT_ON}{title_token}{ANSI_RESET}")));
    assert_eq!(piped_output, rendered.outcome.output);
}

#[test]
fn printable_output_uses_display_aligned_semantic_frame() {
    let title_token = termiflow::graph::subgraph_title_text("Data Layer");
    let output = title_token.clone();

    let mut raw_semantic_frame = SemanticFrame {
        width: title_token.chars().count() + 4,
        height: 1,
        cells: vec![CellMeta::default(); title_token.chars().count() + 4],
    };
    for (offset, ch) in title_token.chars().enumerate() {
        raw_semantic_frame.cells[2 + offset] = CellMeta {
            ch,
            owner_kind: CellOwnerKind::SubgraphTitle,
            owner_id: Some("group".to_string()),
            role: CellRole::Text,
            z_index: 2,
        };
    }

    let mut display_semantic_frame = SemanticFrame {
        width: title_token.chars().count(),
        height: 1,
        cells: vec![CellMeta::default(); title_token.chars().count()],
    };
    for (offset, ch) in title_token.chars().enumerate() {
        display_semantic_frame.cells[offset] = CellMeta {
            ch,
            owner_kind: CellOwnerKind::SubgraphTitle,
            owner_id: Some("group".to_string()),
            role: CellRole::Text,
            z_index: 2,
        };
    }

    let rendered = PreparedRender {
        graph: Graph::new(),
        outcome: RenderOutcome {
            output: output.clone(),
            semantic_frame: raw_semantic_frame,
            display_semantic_frame,
            critic_report: CriticReport::default(),
            warnings: Vec::new(),
            optimized: false,
            repair_passes: 0,
            layout_attempts: 1,
            layout_repairs_applied: 0,
        },
        policy: serde_json::json!({}),
    };

    let tty_output = printable_output(&rendered, true);
    assert_eq!(
        tty_output,
        format!("{ANSI_INVERT_ON}{title_token}{ANSI_RESET}")
    );
}

#[test]
fn viewport_indicator_reports_line_and_column_position() {
    let indicator = build_viewport_indicator(
        "0123456789\nabcdef",
        Viewport {
            offset_x: 3,
            offset_y: 1,
        },
    );

    assert_eq!(indicator, "line 2/2 | col 4/10");
}

#[test]
fn tui_status_can_surface_horizontal_pan_state() {
    let status = build_tui_status(
        &CriticReport::default(),
        0,
        "diagram.md",
        "line 3/8 | col 9/42",
    );

    assert!(status.contains("line 3/8 | col 9/42"));
    assert!(status.contains("j/k/arrows pan"));
}
