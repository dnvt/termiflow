use super::*;
use crate::graph::{Direction, Graph, Node, Rectangle, Subgraph};
use crate::render::semantic::{CellMeta, CellOwnerKind, SemanticFrame};
use crate::style::{BaseStyle, CompositeStyle};

fn unicode_chars() -> StyleChars {
    CompositeStyle::default().to_style_chars(BaseStyle::Unicode)
}

#[test]
fn baseline_report_adds_empty_frame_finding_for_non_empty_graph() {
    let mut graph = Graph::new();
    graph.direction = Direction::TD;
    graph.add_node(Node::new("A", "A"));

    let frame = SemanticFrame {
        width: 4,
        height: 2,
        cells: vec![Default::default(); 8],
    };

    let report = analyze(&graph, &frame, Direction::TD, &unicode_chars());

    assert!(report
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::EmptyRenderedFrame));
    assert!(report.score >= 100);
}

#[test]
fn analyze_reports_arrow_without_shaft() {
    let frame = SemanticFrame {
        width: 2,
        height: 1,
        cells: vec![
            Default::default(),
            CellMeta {
                ch: '>',
                owner_kind: CellOwnerKind::ArrowHead,
                owner_id: None,
                role: CellRole::ArrowTip,
                z_index: 0,
            },
        ],
    };

    let report = analyze(&Graph::new(), &frame, Direction::LR, &unicode_chars());
    assert!(report
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::ArrowWithoutVisibleShaft));
}

#[test]
fn analyze_ignores_arrow_with_label_occluding_stem() {
    let frame = SemanticFrame {
        width: 1,
        height: 3,
        cells: vec![
            CellMeta {
                ch: '^',
                owner_kind: CellOwnerKind::ArrowHead,
                owner_id: Some("edge:0:A->B".to_string()),
                role: CellRole::ArrowTip,
                z_index: 5,
            },
            CellMeta {
                ch: 'L',
                owner_kind: CellOwnerKind::EdgeLabel,
                owner_id: Some("edge:0:A->B".to_string()),
                role: CellRole::Text,
                z_index: 6,
            },
            CellMeta {
                ch: '│',
                owner_kind: CellOwnerKind::EdgeSegment,
                owner_id: Some("edge:0:A->B".to_string()),
                role: CellRole::Vertical,
                z_index: 5,
            },
        ],
    };

    let report = analyze(&Graph::new(), &frame, Direction::BT, &unicode_chars());
    assert!(!report
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::ArrowWithoutVisibleShaft));
}

#[test]
fn analyze_ignores_arrow_using_subgraph_border_pierce() {
    let frame = SemanticFrame {
        width: 1,
        height: 2,
        cells: vec![
            CellMeta {
                ch: '↑',
                owner_kind: CellOwnerKind::ArrowHead,
                owner_id: Some("edge:0:A->B".to_string()),
                role: CellRole::ArrowTip,
                z_index: 5,
            },
            CellMeta {
                ch: '┬',
                owner_kind: CellOwnerKind::SubgraphBorder,
                owner_id: Some("SG".to_string()),
                role: CellRole::Border,
                z_index: 1,
            },
        ],
    };

    let report = analyze(&Graph::new(), &frame, Direction::BT, &unicode_chars());
    assert!(!report
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::ArrowWithoutVisibleShaft));
}

#[test]
fn analyze_reports_crowded_edge_label() {
    let frame = SemanticFrame {
        width: 3,
        height: 2,
        cells: vec![
            CellMeta {
                ch: 'L',
                owner_kind: CellOwnerKind::EdgeLabel,
                owner_id: Some("edge:0:A->B".to_string()),
                role: CellRole::Text,
                z_index: 6,
            },
            CellMeta {
                ch: '┼',
                owner_kind: CellOwnerKind::Junction,
                owner_id: Some("edge:1:C->D".to_string()),
                role: CellRole::Junction,
                z_index: 5,
            },
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        ],
    };

    let report = analyze(&Graph::new(), &frame, Direction::LR, &unicode_chars());
    assert!(report
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::CrowdedEdgeLabel));
}

#[test]
fn analyze_ignores_label_next_to_plain_foreign_route_segment() {
    let frame = SemanticFrame {
        width: 3,
        height: 1,
        cells: vec![
            CellMeta {
                ch: 'L',
                owner_kind: CellOwnerKind::EdgeLabel,
                owner_id: Some("edge:0:A->B".to_string()),
                role: CellRole::Text,
                z_index: 6,
            },
            CellMeta {
                ch: '─',
                owner_kind: CellOwnerKind::EdgeSegment,
                owner_id: Some("edge:1:C->D".to_string()),
                role: CellRole::Horizontal,
                z_index: 5,
            },
            Default::default(),
        ],
    };

    let report = analyze(&Graph::new(), &frame, Direction::LR, &unicode_chars());
    assert!(!report
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::CrowdedEdgeLabel));
}

#[test]
fn analyze_reports_route_crossing_node_interior() {
    let mut graph = Graph::new();
    let mut node = Node::new("A", "A");
    node.x = 0;
    node.y = 0;
    node.width = 5;
    graph.add_node(node);

    let mut cells = vec![CellMeta::default(); 15];
    cells[7] = CellMeta {
        ch: '│',
        owner_kind: CellOwnerKind::EdgeSegment,
        owner_id: Some("edge:0:X->A".to_string()),
        role: CellRole::Vertical,
        z_index: 5,
    };
    let frame = SemanticFrame {
        width: 5,
        height: 3,
        cells,
    };

    let report = analyze(&graph, &frame, Direction::TD, &unicode_chars());
    assert!(report
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::RouteCrossesNodeInterior));
}

#[test]
fn analyze_reports_subgraph_title_corruption() {
    let mut graph = Graph::new();
    let mut subgraph = Subgraph::new("sg", Some("Svc".to_string()));
    let corrupted_title = "─vc";
    let width = crate::graph::subgraph_title_text("Svc").chars().count() + 4;
    subgraph.bounds = Rectangle {
        x: 0,
        y: 0,
        width,
        height: 3,
    };
    graph.add_subgraph(subgraph);

    let title_y = subgraph_title_y(
        &graph.get_subgraph("sg").expect("subgraph").bounds,
        Direction::TD,
    );
    let start_x =
        crate::graph::subgraph_title_start_x(0, width, "Svc", Direction::TD).expect("title start");
    let mut cells = vec![CellMeta::default(); width * 3];
    for (offset, ch) in corrupted_title.chars().enumerate() {
        cells[title_y * width + start_x + offset] = CellMeta {
            ch,
            owner_kind: CellOwnerKind::SubgraphTitle,
            owner_id: Some("sg".to_string()),
            role: CellRole::Text,
            z_index: 2,
        };
    }

    let frame = SemanticFrame {
        width,
        height: 3,
        cells,
    };

    let report = analyze(&graph, &frame, Direction::TD, &unicode_chars());
    assert!(report
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::SubgraphTitleCorrupted));
}

#[test]
fn analyze_does_not_report_subgraph_title_corruption_for_title_text_with_v() {
    let mut graph = Graph::new();
    let mut subgraph = Subgraph::new("sg", Some("Service".to_string()));
    let title_fmt = crate::graph::subgraph_title_text("Service");
    let width = title_fmt.chars().count() + 4;
    subgraph.bounds = Rectangle {
        x: 0,
        y: 0,
        width,
        height: 3,
    };
    graph.add_subgraph(subgraph);

    let title_y = subgraph_title_y(
        &graph.get_subgraph("sg").expect("subgraph").bounds,
        Direction::TD,
    );
    let start_x = crate::graph::subgraph_title_start_x(0, width, "Service", Direction::TD)
        .expect("title start");
    let mut cells = vec![CellMeta::default(); width * 3];
    for (offset, ch) in title_fmt.chars().enumerate() {
        cells[title_y * width + start_x + offset] = CellMeta {
            ch,
            owner_kind: CellOwnerKind::SubgraphTitle,
            owner_id: Some("sg".to_string()),
            role: CellRole::Text,
            z_index: 2,
        };
    }

    let frame = SemanticFrame {
        width,
        height: 3,
        cells,
    };

    let report = analyze(&graph, &frame, Direction::TD, &unicode_chars());
    assert!(!report
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::SubgraphTitleCorrupted));
}

#[test]
fn analyze_reports_route_topology_mismatch_for_wrong_corner() {
    let mut cells = vec![CellMeta::default(); 9];
    cells[4] = CellMeta {
        ch: '┘',
        owner_kind: CellOwnerKind::CycleEdge,
        owner_id: Some("edge:0:A->A".to_string()),
        role: CellRole::Corner,
        z_index: 5,
    };
    cells[5] = CellMeta {
        ch: '─',
        owner_kind: CellOwnerKind::CycleEdge,
        owner_id: Some("edge:0:A->A".to_string()),
        role: CellRole::Horizontal,
        z_index: 5,
    };
    cells[7] = CellMeta {
        ch: '│',
        owner_kind: CellOwnerKind::CycleEdge,
        owner_id: Some("edge:0:A->A".to_string()),
        role: CellRole::Vertical,
        z_index: 5,
    };
    let frame = SemanticFrame {
        width: 3,
        height: 3,
        cells,
    };

    let report = analyze(&Graph::new(), &frame, Direction::LR, &unicode_chars());
    assert!(report
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::RouteTopologyMismatch));
}

#[test]
fn analyze_accepts_styled_straight_route_variants() {
    for glyph in ['━', '╌'] {
        let frame = SemanticFrame {
            width: 3,
            height: 1,
            cells: (0..3)
                .map(|_| CellMeta {
                    ch: glyph,
                    owner_kind: CellOwnerKind::EdgeSegment,
                    owner_id: Some("edge:0:A->B".to_string()),
                    role: CellRole::Horizontal,
                    z_index: 1,
                })
                .collect(),
        };

        let report = analyze(&Graph::new(), &frame, Direction::LR, &unicode_chars());
        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::RouteTopologyMismatch));
    }
}

#[test]
fn analyze_reports_route_topology_mismatch_for_junction_like_lr_side_pierce() {
    let mut graph = Graph::new();
    let mut subgraph = Subgraph::new("sg", Some("Svc".to_string()));
    subgraph.bounds = Rectangle {
        x: 1,
        y: 0,
        width: 3,
        height: 5,
    };
    graph.add_subgraph(subgraph);

    let mut cells = vec![CellMeta::default(); 5 * 5];
    for y in 1..=3 {
        cells[y * 5 + 1] = CellMeta {
            ch: '|',
            owner_kind: CellOwnerKind::SubgraphBorder,
            owner_id: Some("sg".to_string()),
            role: CellRole::Vertical,
            z_index: 2,
        };
    }
    cells[2 * 5] = CellMeta {
        ch: '-',
        owner_kind: CellOwnerKind::EdgeSegment,
        owner_id: Some("edge:0:A->B".to_string()),
        role: CellRole::Horizontal,
        z_index: 4,
    };
    cells[2 * 5 + 1] = CellMeta {
        ch: '+',
        owner_kind: CellOwnerKind::Junction,
        owner_id: Some("edge:0:A->B".to_string()),
        role: CellRole::Junction,
        z_index: 4,
    };
    cells[2 * 5 + 2] = CellMeta {
        ch: '>',
        owner_kind: CellOwnerKind::ArrowHead,
        owner_id: Some("edge:0:A->B".to_string()),
        role: CellRole::ArrowTip,
        z_index: 4,
    };
    let frame = SemanticFrame {
        width: 5,
        height: 5,
        cells,
    };

    let findings = find_subgraph_border_portal_artifacts(
        &graph,
        &frame,
        Direction::LR,
        &crate::style::ASCII_CHARS,
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding.code == FindingCode::RouteTopologyMismatch),
        "expected junction-like side pierce to trigger a border-portal artifact finding: {findings:?}"
    );
}

#[test]
fn analyze_does_not_report_route_topology_mismatch_for_clean_lr_side_opening() {
    let mut graph = Graph::new();
    let mut subgraph = Subgraph::new("sg", Some("Svc".to_string()));
    subgraph.bounds = Rectangle {
        x: 1,
        y: 0,
        width: 3,
        height: 5,
    };
    graph.add_subgraph(subgraph);

    let mut cells = vec![CellMeta::default(); 5 * 5];
    for y in 1..=3 {
        cells[y * 5 + 1] = CellMeta {
            ch: '|',
            owner_kind: CellOwnerKind::SubgraphBorder,
            owner_id: Some("sg".to_string()),
            role: CellRole::Vertical,
            z_index: 2,
        };
    }
    cells[2 * 5] = CellMeta {
        ch: '-',
        owner_kind: CellOwnerKind::EdgeSegment,
        owner_id: Some("edge:0:A->B".to_string()),
        role: CellRole::Horizontal,
        z_index: 4,
    };
    cells[2 * 5 + 1] = CellMeta {
        ch: '-',
        owner_kind: CellOwnerKind::PortalOpening,
        owner_id: Some("sg".to_string()),
        role: CellRole::Portal,
        z_index: 4,
    };
    cells[2 * 5 + 2] = CellMeta {
        ch: '-',
        owner_kind: CellOwnerKind::EdgeSegment,
        owner_id: Some("edge:0:A->B".to_string()),
        role: CellRole::Horizontal,
        z_index: 4,
    };
    cells[2 * 5 + 3] = CellMeta {
        ch: '>',
        owner_kind: CellOwnerKind::ArrowHead,
        owner_id: Some("edge:0:A->B".to_string()),
        role: CellRole::ArrowTip,
        z_index: 4,
    };
    let frame = SemanticFrame {
        width: 5,
        height: 5,
        cells,
    };

    let findings = find_subgraph_border_portal_artifacts(
        &graph,
        &frame,
        Direction::LR,
        &crate::style::ASCII_CHARS,
    );
    assert!(
        findings.is_empty(),
        "expected clean horizontal side opening to avoid border-portal artifact finding: {findings:?}"
    );
}

#[test]
fn analyze_reports_edge_label_collision_with_node() {
    let mut graph = Graph::new();
    let mut node = Node::new("A", "A");
    node.x = 0;
    node.y = 0;
    node.width = 7;
    node.height = 3;
    graph.add_node(node);

    // Place an edge label cell inside the node bounding box (x=3, y=1).
    let mut cells = vec![CellMeta::default(); 7 * 3];
    cells[7 + 3] = CellMeta {
        ch: 'X',
        owner_kind: CellOwnerKind::EdgeLabel,
        owner_id: Some("edge:0:A->B".to_string()),
        role: CellRole::Text,
        z_index: 6,
    };
    let frame = SemanticFrame {
        width: 7,
        height: 3,
        cells,
    };

    let report = analyze(&graph, &frame, Direction::TD, &unicode_chars());
    assert!(report
        .findings
        .iter()
        .any(|f| f.code == FindingCode::EdgeLabelCollidesWithNode));
    let collision = report
        .findings
        .iter()
        .find(|f| f.code == FindingCode::EdgeLabelCollidesWithNode)
        .unwrap();
    assert_eq!(collision.owner_ids, vec!["edge:0:A->B"]);
    assert_eq!(collision.cells, vec![(3, 1)]);
}

#[test]
fn audit_summary_marks_empty_report_clean() {
    let report = CriticReport {
        score: 0,
        findings: Vec::new(),
        notes: Vec::new(),
    };

    let summary = report.audit_summary();
    assert_eq!(summary.verdict, AuditVerdict::Clean);
    assert!(summary.is_clean());
    assert_eq!(summary.highlights.len(), 0);
}

#[test]
fn ascii_plus_corner_is_not_flagged_as_junction_mismatch() {
    let chars = CompositeStyle::default().to_style_chars(BaseStyle::Ascii);
    let frame = SemanticFrame {
        width: 2,
        height: 2,
        cells: vec![
            CellMeta {
                ch: '+',
                owner_kind: CellOwnerKind::Junction,
                owner_id: Some("edge:0:A->B".to_string()),
                role: CellRole::Junction,
                z_index: 5,
            },
            CellMeta {
                ch: '-',
                owner_kind: CellOwnerKind::EdgeSegment,
                owner_id: Some("edge:0:A->B".to_string()),
                role: CellRole::Horizontal,
                z_index: 5,
            },
            CellMeta {
                ch: '|',
                owner_kind: CellOwnerKind::EdgeSegment,
                owner_id: Some("edge:0:A->B".to_string()),
                role: CellRole::Vertical,
                z_index: 5,
            },
            CellMeta::default(),
        ],
    };

    let report = analyze(&Graph::new(), &frame, Direction::TD, &chars);
    assert!(!report
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::JunctionTopologyMismatch));
}

#[test]
fn analyze_reports_route_symmetry_imbalance_for_skewed_fanout() {
    let mut graph = Graph::new();
    graph.direction = Direction::TD;

    let mut a = Node::new("A", "A");
    a.x = 8;
    a.y = 0;
    a.width = 5;
    let mut b = Node::new("B", "B");
    b.x = 0;
    b.y = 8;
    b.width = 5;
    let mut c = Node::new("C", "C");
    c.x = 20;
    c.y = 8;
    c.width = 5;

    graph.add_node(a);
    graph.add_node(b);
    graph.add_node(c);
    graph.add_edge(crate::graph::Edge::new("A", "B"));
    graph.add_edge(crate::graph::Edge::new("A", "C"));

    let report = analyze(
        &graph,
        &SemanticFrame::default(),
        Direction::TD,
        &unicode_chars(),
    );

    assert!(report
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::RouteSymmetryImbalance));
}

#[test]
fn analyze_ignores_balanced_crossing_permutation_rows() {
    let mut graph = Graph::new();
    graph.direction = Direction::BT;

    let mut a1 = Node::new("A1", "Node A1");
    a1.x = 8;
    a1.y = 14;
    a1.width = 13;
    let mut a2 = Node::new("A2", "Node A2");
    a2.x = 25;
    a2.y = 14;
    a2.width = 13;
    let mut a3 = Node::new("A3", "Node A3");
    a3.x = 42;
    a3.y = 14;
    a3.width = 13;

    let mut b3 = Node::new("B3", "Node B3");
    b3.x = 0;
    b3.y = 7;
    b3.width = 13;
    let mut b2 = Node::new("B2", "Node B2");
    b2.x = 17;
    b2.y = 7;
    b2.width = 13;
    let mut b1 = Node::new("B1", "Node B1");
    b1.x = 34;
    b1.y = 7;
    b1.width = 13;

    let mut c1 = Node::new("C1", "Node C1");
    c1.x = 8;
    c1.y = 0;
    c1.width = 13;
    let mut c2 = Node::new("C2", "Node C2");
    c2.x = 25;
    c2.y = 0;
    c2.width = 13;
    let mut c3 = Node::new("C3", "Node C3");
    c3.x = 42;
    c3.y = 0;
    c3.width = 13;

    graph.add_node(a1);
    graph.add_node(a2);
    graph.add_node(a3);
    graph.add_node(b3);
    graph.add_node(b2);
    graph.add_node(b1);
    graph.add_node(c1);
    graph.add_node(c2);
    graph.add_node(c3);

    graph.add_edge(crate::graph::Edge::new("A1", "B2"));
    graph.add_edge(crate::graph::Edge::new("A1", "B3"));
    graph.add_edge(crate::graph::Edge::new("A2", "B1"));
    graph.add_edge(crate::graph::Edge::new("A2", "B3"));
    graph.add_edge(crate::graph::Edge::new("A3", "B1"));
    graph.add_edge(crate::graph::Edge::new("A3", "B2"));

    graph.add_edge(crate::graph::Edge::new("B1", "C2"));
    graph.add_edge(crate::graph::Edge::new("B1", "C3"));
    graph.add_edge(crate::graph::Edge::new("B2", "C1"));
    graph.add_edge(crate::graph::Edge::new("B2", "C3"));
    graph.add_edge(crate::graph::Edge::new("B3", "C1"));
    graph.add_edge(crate::graph::Edge::new("B3", "C2"));

    let report = analyze(
        &graph,
        &SemanticFrame::default(),
        Direction::BT,
        &unicode_chars(),
    );

    assert!(!report
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::RouteSymmetryImbalance));
}

#[test]
fn analyze_reports_branch_spacing_imbalance_for_uneven_fanout() {
    let mut graph = Graph::new();
    graph.direction = Direction::TD;

    let mut a = Node::new("A", "A");
    a.x = 20;
    a.y = 0;
    a.width = 9;

    let mut b = Node::new("B", "B");
    b.x = 0;
    b.y = 8;
    b.width = 7;

    let mut c = Node::new("C", "C");
    c.x = 12;
    c.y = 8;
    c.width = 7;

    let mut d = Node::new("D", "D");
    d.x = 42;
    d.y = 8;
    d.width = 7;

    graph.add_node(a);
    graph.add_node(b);
    graph.add_node(c);
    graph.add_node(d);
    graph.add_edge(crate::graph::Edge::new("A", "B"));
    graph.add_edge(crate::graph::Edge::new("A", "C"));
    graph.add_edge(crate::graph::Edge::new("A", "D"));

    let report = analyze(
        &graph,
        &SemanticFrame::default(),
        Direction::TD,
        &unicode_chars(),
    );

    assert!(report
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::BranchSpacingImbalance));
    assert!(!report
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::RouteSymmetryImbalance));
}

#[test]
fn analyze_does_not_report_branch_spacing_imbalance_for_even_fanout() {
    let mut graph = Graph::new();
    graph.direction = Direction::TD;

    let mut a = Node::new("A", "A");
    a.x = 20;
    a.y = 0;
    a.width = 9;

    let mut b = Node::new("B", "B");
    b.x = 0;
    b.y = 8;
    b.width = 7;

    let mut c = Node::new("C", "C");
    c.x = 21;
    c.y = 8;
    c.width = 7;

    let mut d = Node::new("D", "D");
    d.x = 42;
    d.y = 8;
    d.width = 7;

    graph.add_node(a);
    graph.add_node(b);
    graph.add_node(c);
    graph.add_node(d);
    graph.add_edge(crate::graph::Edge::new("A", "B"));
    graph.add_edge(crate::graph::Edge::new("A", "C"));
    graph.add_edge(crate::graph::Edge::new("A", "D"));

    let report = analyze(
        &graph,
        &SemanticFrame::default(),
        Direction::TD,
        &unicode_chars(),
    );

    assert!(!report
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::BranchSpacingImbalance));
}

#[test]
fn analyze_reports_branch_crowding_for_dense_fanout() {
    let mut graph = Graph::new();
    graph.direction = Direction::TD;

    let mut a = Node::new("A", "A");
    a.x = 12;
    a.y = 0;
    a.width = 9;

    let mut b = Node::new("B", "B");
    b.x = 4;
    b.y = 8;
    b.width = 7;

    let mut c = Node::new("C", "C");
    c.x = 11;
    c.y = 8;
    c.width = 7;

    let mut d = Node::new("D", "D");
    d.x = 18;
    d.y = 8;
    d.width = 7;

    graph.add_node(a);
    graph.add_node(b);
    graph.add_node(c);
    graph.add_node(d);
    graph.add_edge(crate::graph::Edge::new("A", "B"));
    graph.add_edge(crate::graph::Edge::new("A", "C"));
    graph.add_edge(crate::graph::Edge::new("A", "D"));

    let report = analyze(
        &graph,
        &SemanticFrame::default(),
        Direction::TD,
        &unicode_chars(),
    );

    assert!(report
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::BranchCrowding));
    assert!(!report
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::BranchSpacingImbalance));
}

#[test]
fn analyze_does_not_report_branch_crowding_for_roomy_fanout() {
    let mut graph = Graph::new();
    graph.direction = Direction::TD;

    let mut a = Node::new("A", "A");
    a.x = 20;
    a.y = 0;
    a.width = 9;

    let mut b = Node::new("B", "B");
    b.x = 0;
    b.y = 8;
    b.width = 7;

    let mut c = Node::new("C", "C");
    c.x = 16;
    c.y = 8;
    c.width = 7;

    let mut d = Node::new("D", "D");
    d.x = 32;
    d.y = 8;
    d.width = 7;

    graph.add_node(a);
    graph.add_node(b);
    graph.add_node(c);
    graph.add_node(d);
    graph.add_edge(crate::graph::Edge::new("A", "B"));
    graph.add_edge(crate::graph::Edge::new("A", "C"));
    graph.add_edge(crate::graph::Edge::new("A", "D"));

    let report = analyze(
        &graph,
        &SemanticFrame::default(),
        Direction::TD,
        &unicode_chars(),
    );

    assert!(!report
        .findings
        .iter()
        .any(|finding| finding.code == FindingCode::BranchCrowding));
}

#[test]
fn analyze_does_not_report_chain_too_cramped_for_visibly_separated_rl_nodes() {
    let mut graph = Graph::new();
    graph.direction = Direction::RL;

    let mut source = Node::new("D1", "User DB");
    source.x = 30;
    source.y = 9;
    source.width = 13;

    let mut target = Node::new("Response", "Response Builder");
    target.x = 49;
    target.y = 7;
    target.width = 22;

    graph.add_node(source);
    graph.add_node(target);
    graph.add_edge(crate::graph::Edge::new("D1", "Response"));

    let report = analyze(
        &graph,
        &SemanticFrame::default(),
        Direction::RL,
        &unicode_chars(),
    );

    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::ChainTooCrampedLR),
        "expected RL spacing heuristic to honor physical box separation instead of logical edge order: {report:?}"
    );
}
