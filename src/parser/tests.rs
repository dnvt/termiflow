use super::*;

// === FATAL ERROR TESTS ===

#[test]
fn test_empty_input_fails() {
    let result = parse("", false);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Empty file"));
}

#[test]
fn test_whitespace_only_fails() {
    let result = parse("   \n\n   ", false);
    assert!(result.is_err());
}

#[test]
fn test_no_direction_fails() {
    let result = parse("A[Node] --> B[Other]", false);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("No graph direction"));
}

#[test]
fn test_unsupported_diagram_type_sequence() {
    let result = parse("sequenceDiagram\nA->>B: hi", false);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("diagram type not supported"));
}

// === DIRECTION PARSING ===

#[test]
fn test_direction_td() {
    let result = parse("graph TD\nA[Node]", false).unwrap();
    assert!(matches!(result.graph.direction, Direction::TD));
}

#[test]
fn test_direction_tb() {
    let result = parse("graph TB\nA[Node]", false).unwrap();
    assert!(matches!(result.graph.direction, Direction::TD));
}

#[test]
fn test_direction_lr() {
    let result = parse("graph LR\nA[Node]", false).unwrap();
    assert!(matches!(result.graph.direction, Direction::LR));
}

#[test]
fn test_direction_bt() {
    let result = parse("graph BT\nA[Node]", false).unwrap();
    assert!(matches!(result.graph.direction, Direction::BT));
}

#[test]
fn test_direction_flowchart_alias() {
    let result = parse("flowchart LR\nA[Node]", false).unwrap();
    assert!(matches!(result.graph.direction, Direction::LR));
}

// === NODE PARSING ===

#[test]
fn test_single_node() {
    let result = parse("graph TD\nA[Gateway]", false).unwrap();
    assert_eq!(result.graph.nodes.len(), 1);
    assert_eq!(result.graph.nodes[0].id, "A");
    assert_eq!(result.graph.nodes[0].label, "Gateway");
}

#[test]
fn test_multiple_nodes() {
    let input = "graph TD\nA[First]\nB[Second]\nC[Third]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.nodes.len(), 3);
}

#[test]
fn test_database_node() {
    let result = parse("graph TD\nDB[(Database)]", false).unwrap();
    assert_eq!(result.graph.nodes.len(), 1);
    assert_eq!(result.graph.nodes[0].label, "Database");
}

#[test]
fn test_node_with_spaces_in_label() {
    let result = parse("graph TD\nA[My Long Label]", false).unwrap();
    assert_eq!(result.graph.nodes[0].label, "My Long Label");
}

// === EDGE PARSING ===

#[test]
fn test_single_edge() {
    let input = "graph TD\nA[Start] --> B[End]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.edges.len(), 1);
    assert_eq!(result.graph.edges[0].from, "A");
    assert_eq!(result.graph.edges[0].to, "B");
}

#[test]
fn test_multiple_edges() {
    let input = "graph TD\nA[A] --> B[B]\nB --> C[C]\nA --> C";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.edges.len(), 3);
}

#[test]
fn test_edge_with_long_arrow() {
    let input = "graph TD\nA[A] ---> B[B]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.edges.len(), 1);
}

// === FORWARD REFERENCE (TWO-PASS) ===

#[test]
fn test_forward_reference() {
    // B is referenced before defined
    let input = "graph TD\nA[Start] --> B\nB[End]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.nodes.len(), 2);
    // B should have its label from definition
    let b_node = result.graph.nodes.iter().find(|n| n.id == "B").unwrap();
    assert_eq!(b_node.label, "End");
}

#[test]
fn test_undefined_node_auto_create() {
    // C is never defined, should auto-create with ID as label
    let input = "graph TD\nA[Start] --> B[Middle] --> C";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.nodes.len(), 3);
    let c_node = result.graph.nodes.iter().find(|n| n.id == "C").unwrap();
    assert_eq!(c_node.label, "C");
    // Should have warning about auto-create
    assert!(result
        .graph
        .warnings
        .iter()
        .any(|w| w.contains("'C' referenced but never defined")));
}

// === CLICK TARGETS ===

#[test]
fn test_click_target() {
    let input = r#"graph TD
A[Gateway]
click A "gateway.md""#;
    let result = parse(input, false).unwrap();
    assert_eq!(
        result.graph.nodes[0].click_target,
        Some("gateway.md".to_string())
    );
}

#[test]
fn test_click_target_single_quotes() {
    let input = "graph TD\nA[Node]\nclick A 'file.md'";
    let result = parse(input, false).unwrap();
    assert_eq!(
        result.graph.nodes[0].click_target,
        Some("file.md".to_string())
    );
}

// === CONFIG DIRECTIVES ===

#[test]
fn test_config_style() {
    let input = "graph TD\n%% termiflow: style=unicode\nA[Node]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.config.style, Some("unicode".to_string()));
}

#[test]
fn test_config_max_label() {
    let input = "graph TD\n%% termiflow: max_label=30\nA[Node]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.config.max_label, Some(30));
}

#[test]
fn test_config_wrap_labels() {
    let input = "graph TD\n%% termiflow: wrap=true\nA[Node]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.config.wrap_labels, Some(true));
}

#[test]
fn test_config_max_label_lines() {
    let input = "graph TD\n%% termiflow: max_lines=3\nA[Node]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.config.max_label_lines, Some(3));
}

#[test]
fn test_config_spacing_mode() {
    let input = "graph TD\n%% termiflow: spacing=compact\nA[Node]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.config.spacing_mode, Some(SpacingMode::Compact));
}

#[test]
fn test_config_optimize_render() {
    let input = "graph TD\n%% termiflow: optimize_render=true\nA[Node]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.config.optimize_render, Some(true));
}

#[test]
fn test_config_render_repair_passes() {
    let input = "graph TD\n%% termiflow: render_repair_passes=4\nA[Node]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.config.render_repair_passes, Some(4));
}

#[test]
fn test_config_layout_repair_passes() {
    let input = "graph TD\n%% termiflow: layout_repair_passes=3\nA[Node]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.config.layout_repair_passes, Some(3));
}

#[test]
fn test_config_debug_critic() {
    let input = "graph TD\n%% termiflow: debug_critic=yes\nA[Node]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.config.debug_critic, Some(true));
}

// === COMMENTS ===

#[test]
fn test_comments_ignored() {
    let input = "graph TD\n%% This is a comment\nA[Node]\n%% Another comment";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.nodes.len(), 1);
}

// === STRICT MODE ===

#[test]
fn test_strict_mode_unsupported_syntax() {
    let input = "graph TD\nsubgraph X\nA[Node]";
    // Lenient mode: should warn but parse
    let lenient = parse(input, false).unwrap();
    assert!(!lenient.graph.warnings.is_empty());

    // Strict mode: should fail
    let strict = parse(input, true);
    assert!(strict.is_err());
}

#[test]
fn test_strict_mode_allows_auto_create() {
    // Auto-create warnings are INFORMATIONAL, not affected by strict
    let input = "graph TD\nA[Start] --> B";
    let result = parse(input, true).unwrap();
    assert_eq!(result.graph.nodes.len(), 2);
    // Warning should still be present
    assert!(result
        .graph
        .warnings
        .iter()
        .any(|w| w.contains("'B' referenced")));
}

// === UNSUPPORTED SYNTAX DETECTION ===

#[test]
fn test_subgraph_basic() {
    let input = "graph TD\nsubgraph SG1 [My Subgraph]\nA[Node A]\nB[Node B]\nend\nC[Outside]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.subgraphs.len(), 1);
    let sg = &result.graph.subgraphs[0];
    assert_eq!(sg.id, "SG1");
    assert_eq!(sg.title, Some("My Subgraph".to_string()));
    assert!(sg.contains_node("A"));
    assert!(sg.contains_node("B"));
    assert!(!sg.contains_node("C"));
    // Check node_subgraph mapping
    assert_eq!(result.graph.get_node_subgraph("A"), Some("SG1"));
    assert_eq!(result.graph.get_node_subgraph("B"), Some("SG1"));
    assert_eq!(result.graph.get_node_subgraph("C"), None);
}

#[test]
fn test_subgraph_plain_title() {
    let input = "graph TD\nsubgraph My Title\nA[Node]\nend";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.subgraphs.len(), 1);
    let sg = &result.graph.subgraphs[0];
    assert_eq!(sg.id, "my_title");
    assert_eq!(sg.title, Some("My Title".to_string()));
    assert!(sg.contains_node("A"));
}

#[test]
fn test_subgraph_explicit_node_definition_overrides_prior_outside_reference() {
    let input = "graph LR\nA[Source] --> B\nsubgraph SG [Group]\n    B[Target]\n    C[Other]\nend";
    let result = parse(input, false).unwrap();

    assert_eq!(result.graph.get_node_subgraph("B"), Some("SG"));
    let sg = result.graph.get_subgraph("SG").expect("subgraph SG");
    assert!(sg.contains_node("B"));
    assert!(sg.contains_node("C"));
}

#[test]
fn test_subgraph_unclosed_warns() {
    let input = "graph TD\nsubgraph X\nA[Node]";
    let result = parse(input, false).unwrap();
    assert!(result
        .graph
        .warnings
        .iter()
        .any(|w| w.contains("Unclosed subgraph")));
}

#[test]
fn test_subgraph_nested_preserves_hierarchy_without_warning() {
    let input = "graph TD\nsubgraph Outer\nA[Node]\nsubgraph Inner\nB[Node]\nend\nend";
    let result = parse(input, false).unwrap();
    assert!(!result
        .graph
        .warnings
        .iter()
        .any(|w| w.contains("Nested subgraphs are experimental")));
    assert_eq!(result.graph.subgraphs.len(), 2);

    let outer = result.graph.get_subgraph("outer").expect("outer subgraph");
    let inner = result.graph.get_subgraph("inner").expect("inner subgraph");

    assert!(outer.contains_node("A"));
    assert!(!outer.contains_node("B"));
    assert_eq!(outer.parent_id, None);
    assert_eq!(outer.child_ids, vec!["inner".to_string()]);

    assert!(inner.contains_node("B"));
    assert_eq!(inner.parent_id.as_deref(), Some("outer"));
    assert!(inner.child_ids.is_empty());

    assert_eq!(result.graph.get_node_subgraph("A"), Some("outer"));
    assert_eq!(result.graph.get_node_subgraph("B"), Some("inner"));
}

#[test]
fn test_subgraph_nested_parses_in_strict_mode() {
    let input = "graph TD\nsubgraph Outer\nA[Node]\nsubgraph Inner\nB[Node]\nend\nend";
    let result = parse(input, true).expect("nested subgraphs should parse in strict mode");
    assert_eq!(result.graph.subgraphs.len(), 2);
}

#[test]
fn test_subgraph_nested_bracket_syntax_preserves_parent_child_links() {
    let input =
        "graph TD\nsubgraph OUTER [Outer]\nA[Node]\nsubgraph INNER [Inner]\nB[Node]\nend\nend";
    let result = parse(input, false).unwrap();

    let outer = result.graph.get_subgraph("OUTER").expect("outer subgraph");
    let inner = result.graph.get_subgraph("INNER").expect("inner subgraph");

    assert_eq!(inner.parent_id.as_deref(), Some("OUTER"));
    assert_eq!(outer.child_ids, vec!["INNER".to_string()]);
    assert_eq!(result.graph.get_node_subgraph("A"), Some("OUTER"));
    assert_eq!(result.graph.get_node_subgraph("B"), Some("INNER"));
}

#[test]
fn test_nested_service_data_sample_preserves_parent_child_links() {
    let input = "graph TD\nA[API Gateway] --> B[User Service]\nsubgraph SL[Service Layer]\nB\nsubgraph DL[Data Layer]\nC[Order Service] --> D[(Order DB)]\nE[(User DB)]\nend\nB --> E\nD --> F[Response Builder]\nE --> F\nend";
    let result = parse(input, false).unwrap();

    let service = result.graph.get_subgraph("SL").expect("service layer");
    let data = result.graph.get_subgraph("DL").expect("data layer");

    assert_eq!(service.child_ids, vec!["DL".to_string()]);
    assert_eq!(data.parent_id.as_deref(), Some("SL"));
    assert_eq!(result.graph.get_node_subgraph("B"), Some("SL"));
    assert_eq!(result.graph.get_node_subgraph("C"), Some("DL"));
    assert_eq!(result.graph.get_node_subgraph("D"), Some("DL"));
    assert_eq!(result.graph.get_node_subgraph("E"), Some("DL"));
}

#[test]
fn test_subgraph_multiple() {
    let input = "graph TD\nsubgraph SG1 [First]\nA[A]\nend\nsubgraph SG2 [Second]\nB[B]\nend";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.subgraphs.len(), 2);
    assert_eq!(result.graph.subgraphs[0].id, "SG1");
    assert_eq!(result.graph.subgraphs[1].id, "SG2");
    assert!(result.graph.subgraphs[0].contains_node("A"));
    assert!(result.graph.subgraphs[1].contains_node("B"));
}

#[test]
fn test_edge_label_pipe_style() {
    // Pipe-style edge labels should be parsed and preserved
    let input = "graph TD\nA[Start] -->|validate| B[Process]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.edges.len(), 1);
    assert_eq!(result.graph.edges[0].from, "A");
    assert_eq!(result.graph.edges[0].to, "B");
    assert_eq!(result.graph.edges[0].label, Some("validate".to_string()));
}

#[test]
fn test_edge_label_text_style() {
    // Text-style edge labels should be parsed and preserved
    let input = "graph TD\nA[Start] -- process --> B[End]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.edges.len(), 1);
    assert_eq!(result.graph.edges[0].from, "A");
    assert_eq!(result.graph.edges[0].to, "B");
    assert_eq!(result.graph.edges[0].label, Some("process".to_string()));
}

#[test]
fn test_edge_label_multiple() {
    // Multiple labeled edges should preserve all labels
    let input = "graph TD\nA[Start] -->|yes| B[Success]\nA -->|no| C[Retry]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.edges.len(), 2);
    assert_eq!(result.graph.edges[0].label, Some("yes".to_string()));
    assert_eq!(result.graph.edges[1].label, Some("no".to_string()));
}

#[test]
fn test_edge_label_mixed_with_unlabeled() {
    // Both labeled and unlabeled edges should be parsed
    let input = "graph TD\nA --> B\nB -->|done| C";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.edges.len(), 2);
    assert_eq!(result.graph.edges[0].from, "A");
    assert_eq!(result.graph.edges[0].to, "B");
    assert!(result.graph.edges[0].label.is_none()); // Unlabeled
    assert_eq!(result.graph.edges[1].from, "B");
    assert_eq!(result.graph.edges[1].to, "C");
    assert_eq!(result.graph.edges[1].label, Some("done".to_string()));
}

#[test]
fn test_style_unsupported() {
    let input = "graph TD\nA[Node]\nstyle A fill:#f00";
    let result = parse(input, false).unwrap();
    assert!(result
        .graph
        .warnings
        .iter()
        .any(|w| w.contains("Mermaid styling not supported")));
}

// === MULTIPLE GRAPH DIRECTIONS ===

#[test]
fn test_multiple_directions_warns() {
    let input = "graph TD\nA[A]\ngraph LR\nB[B]";
    let result = parse(input, false).unwrap();
    // Should use first direction (TD)
    assert!(matches!(result.graph.direction, Direction::TD));
    // Should have warning
    assert!(result
        .graph
        .warnings
        .iter()
        .any(|w| w.contains("Multiple graph directions")));
}

// === EDGE CHAIN TESTS ===

#[test]
fn test_edge_chain_simple() {
    let input = "graph TD\nA --> B --> C --> D";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.edges.len(), 3);
    assert_eq!(result.graph.edges[0].from, "A");
    assert_eq!(result.graph.edges[0].to, "B");
    assert_eq!(result.graph.edges[1].from, "B");
    assert_eq!(result.graph.edges[1].to, "C");
    assert_eq!(result.graph.edges[2].from, "C");
    assert_eq!(result.graph.edges[2].to, "D");
}

#[test]
fn test_edge_chain_with_inline_labels() {
    // Test chains where nodes have labels defined inline
    let input = "graph TD\nA[Start] --> B[Middle] --> C[End]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.edges.len(), 2);
    assert_eq!(result.graph.nodes.len(), 3);
    // Verify labels were captured
    let b_node = result.graph.nodes.iter().find(|n| n.id == "B").unwrap();
    assert_eq!(b_node.label, "Middle");
}

#[test]
fn test_edge_chain_mixed_definitions() {
    // Mix of inline and separate definitions
    let input = "graph TD\nA --> B[Process] --> C\nC[Output]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.edges.len(), 2);
    let c_node = result.graph.nodes.iter().find(|n| n.id == "C").unwrap();
    assert_eq!(c_node.label, "Output");
}

// === NODE SHAPE TESTS ===

#[test]
fn test_edge_kind_open() {
    let result = parse("graph TD\nA --- B", false).unwrap();
    assert_eq!(result.graph.edges.len(), 1);
    assert_eq!(result.graph.edges[0].kind, EdgeKind::Open);
}

#[test]
fn test_edge_kind_thick() {
    let result = parse("graph TD\nA ==> B", false).unwrap();
    assert_eq!(result.graph.edges.len(), 1);
    assert_eq!(result.graph.edges[0].kind, EdgeKind::Thick);
}

#[test]
fn test_edge_kind_dotted() {
    let result = parse("graph TD\nA -.-> B", false).unwrap();
    assert_eq!(result.graph.edges.len(), 1);
    assert_eq!(result.graph.edges[0].kind, EdgeKind::Dotted);
}

#[test]
fn test_edge_kind_open_with_label() {
    let result = parse("graph TD\nA ---|link| B", false).unwrap();
    assert_eq!(result.graph.edges.len(), 1);
    assert_eq!(result.graph.edges[0].kind, EdgeKind::Open);
    assert_eq!(result.graph.edges[0].label.as_deref(), Some("link"));
}

#[test]
fn test_edge_kind_thick_with_label() {
    let result = parse("graph TD\nA ==>|bold| B", false).unwrap();
    assert_eq!(result.graph.edges.len(), 1);
    assert_eq!(result.graph.edges[0].kind, EdgeKind::Thick);
    assert_eq!(result.graph.edges[0].label.as_deref(), Some("bold"));
}

#[test]
fn test_edge_kind_dotted_with_label() {
    let result = parse("graph TD\nA -.->|opt| B", false).unwrap();
    assert_eq!(result.graph.edges.len(), 1);
    assert_eq!(result.graph.edges[0].kind, EdgeKind::Dotted);
    assert_eq!(result.graph.edges[0].label.as_deref(), Some("opt"));
}

#[test]
fn test_edge_kind_arrow_unchanged() {
    let result = parse("graph TD\nA --> B", false).unwrap();
    assert_eq!(result.graph.edges[0].kind, EdgeKind::Arrow);
}

#[test]
fn test_edge_kind_bidirectional() {
    let result = parse("graph TD\nA <--> B", false).unwrap();
    assert_eq!(result.graph.edges.len(), 1);
    assert_eq!(result.graph.edges[0].kind, EdgeKind::Bidirectional);
    assert!(result.graph.edges[0].label.is_none());
}

#[test]
fn test_edge_kind_bidirectional_with_label() {
    let result = parse("graph TD\nA <-->|sync| B", false).unwrap();
    assert_eq!(result.graph.edges.len(), 1);
    assert_eq!(result.graph.edges[0].kind, EdgeKind::Bidirectional);
    assert_eq!(result.graph.edges[0].label.as_deref(), Some("sync"));
}

#[test]
fn test_edge_kind_bidirectional_extended() {
    // longer arrows <---> are also valid
    let result = parse("graph LR\nA <---> B", false).unwrap();
    assert_eq!(result.graph.edges[0].kind, EdgeKind::Bidirectional);
}

#[test]
fn test_grouped_edge_multi_source() {
    // A & B --> C  generates A→C and B→C
    let result = parse("graph TD\nA & B --> C", false).unwrap();
    assert_eq!(result.graph.nodes.len(), 3);
    assert_eq!(result.graph.edges.len(), 2);
    let has_ac = result
        .graph
        .edges
        .iter()
        .any(|e| e.from == "A" && e.to == "C");
    let has_bc = result
        .graph
        .edges
        .iter()
        .any(|e| e.from == "B" && e.to == "C");
    assert!(has_ac, "expected A→C edge");
    assert!(has_bc, "expected B→C edge");
}

#[test]
fn test_grouped_edge_multi_target() {
    // D --> E & F  generates D→E and D→F
    let result = parse("graph TD\nD --> E & F", false).unwrap();
    assert_eq!(result.graph.nodes.len(), 3);
    assert_eq!(result.graph.edges.len(), 2);
    let has_de = result
        .graph
        .edges
        .iter()
        .any(|e| e.from == "D" && e.to == "E");
    let has_df = result
        .graph
        .edges
        .iter()
        .any(|e| e.from == "D" && e.to == "F");
    assert!(has_de, "expected D→E edge");
    assert!(has_df, "expected D→F edge");
}

#[test]
fn test_grouped_edge_cartesian() {
    // A & B --> C & D  generates 4 edges
    let result = parse("graph TD\nA & B --> C & D", false).unwrap();
    assert_eq!(result.graph.nodes.len(), 4);
    assert_eq!(result.graph.edges.len(), 4);
    let pairs: Vec<(&str, &str)> = result
        .graph
        .edges
        .iter()
        .map(|e| (e.from.as_str(), e.to.as_str()))
        .collect();
    assert!(pairs.contains(&("A", "C")));
    assert!(pairs.contains(&("A", "D")));
    assert!(pairs.contains(&("B", "C")));
    assert!(pairs.contains(&("B", "D")));
}

#[test]
fn test_grouped_edge_with_label() {
    // E & F -->|shared| G  generates 2 edges both with label "shared"
    let result = parse("graph TD\nE & F -->|shared| G", false).unwrap();
    assert_eq!(result.graph.edges.len(), 2);
    assert!(result
        .graph
        .edges
        .iter()
        .all(|e| e.label.as_deref() == Some("shared")));
    assert!(result
        .graph
        .edges
        .iter()
        .any(|e| e.from == "E" && e.to == "G"));
    assert!(result
        .graph
        .edges
        .iter()
        .any(|e| e.from == "F" && e.to == "G"));
}

#[test]
fn test_grouped_edge_all_ids_registered() {
    // All IDs in & groups must produce nodes even if only referenced in groups
    let result = parse("graph TD\nA & B --> C", false).unwrap();
    let ids: Vec<&str> = result.graph.nodes.iter().map(|n| n.id.as_str()).collect();
    assert!(ids.contains(&"A"), "A should be a node");
    assert!(ids.contains(&"B"), "B should be a node");
    assert!(ids.contains(&"C"), "C should be a node");
}

#[test]
fn test_edge_kind_mixed_in_same_graph() {
    let input = "graph TD\nA --> B\nB --- C\nC ==> D\nD -.-> E\nE <--> A";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.edges.len(), 5);
    assert_eq!(result.graph.edges[0].kind, EdgeKind::Arrow);
    assert_eq!(result.graph.edges[1].kind, EdgeKind::Open);
    assert_eq!(result.graph.edges[2].kind, EdgeKind::Thick);
    assert_eq!(result.graph.edges[3].kind, EdgeKind::Dotted);
    assert_eq!(result.graph.edges[4].kind, EdgeKind::Bidirectional);
}

#[test]
fn test_node_shape_rectangle() {
    let result = parse("graph TD\nA[Rectangle]", false).unwrap();
    assert_eq!(result.graph.nodes[0].shape, NodeShape::Rectangle);
    assert_eq!(result.graph.nodes[0].label, "Rectangle");
}

#[test]
fn test_node_shape_rounded() {
    let result = parse("graph TD\nA(Rounded)", false).unwrap();
    assert_eq!(result.graph.nodes[0].shape, NodeShape::Rounded);
    assert_eq!(result.graph.nodes[0].label, "Rounded");
}

#[test]
fn test_node_shape_diamond() {
    let result = parse("graph TD\nA{Decision}", false).unwrap();
    assert_eq!(result.graph.nodes[0].shape, NodeShape::Diamond);
    assert_eq!(result.graph.nodes[0].label, "Decision");
}

#[test]
fn test_node_shape_circle() {
    let result = parse("graph TD\nA((Circle))", false).unwrap();
    assert_eq!(result.graph.nodes[0].shape, NodeShape::Circle);
    assert_eq!(result.graph.nodes[0].label, "Circle");
}

#[test]
fn test_node_shape_stadium() {
    let result = parse("graph TD\nA([Stadium])", false).unwrap();
    assert_eq!(result.graph.nodes[0].shape, NodeShape::Stadium);
    assert_eq!(result.graph.nodes[0].label, "Stadium");
}

#[test]
fn test_node_shape_hexagon() {
    let result = parse("graph TD\nA{{Hexagon}}", false).unwrap();
    assert_eq!(result.graph.nodes[0].shape, NodeShape::Hexagon);
    assert_eq!(result.graph.nodes[0].label, "Hexagon");
}

#[test]
fn test_node_shape_database() {
    let result = parse("graph TD\nDB[(Database)]", false).unwrap();
    assert_eq!(result.graph.nodes[0].shape, NodeShape::Database);
    assert_eq!(result.graph.nodes[0].label, "Database");
}

#[test]
fn test_node_shape_subroutine() {
    let result = parse("graph TD\nA[[Subroutine]]", false).unwrap();
    assert_eq!(result.graph.nodes[0].shape, NodeShape::Subroutine);
    assert_eq!(result.graph.nodes[0].label, "Subroutine");
}

#[test]
fn test_node_shape_asymmetric() {
    let result = parse("graph TD\nA>Flag]", false).unwrap();
    assert_eq!(result.graph.nodes[0].shape, NodeShape::Asymmetric);
    assert_eq!(result.graph.nodes[0].label, "Flag");
}

#[test]
fn test_node_shape_parallelogram() {
    let result = parse("graph TD\nA[/Parallelogram/]", false).unwrap();
    assert_eq!(result.graph.nodes[0].shape, NodeShape::Parallelogram);
    assert_eq!(result.graph.nodes[0].label, "Parallelogram");
}

#[test]
fn test_node_shape_parallelogram_alt() {
    let result = parse(
        r"graph TD
A[\ParAlt\]",
        false,
    )
    .unwrap();
    assert_eq!(result.graph.nodes[0].shape, NodeShape::ParallelogramAlt);
    assert_eq!(result.graph.nodes[0].label, "ParAlt");
}

#[test]
fn test_node_shape_trapezoid() {
    let result = parse(
        r"graph TD
A[/Trap\]",
        false,
    )
    .unwrap();
    assert_eq!(result.graph.nodes[0].shape, NodeShape::Trapezoid);
    assert_eq!(result.graph.nodes[0].label, "Trap");
}

#[test]
fn test_node_shape_trapezoid_alt() {
    let result = parse(
        r"graph TD
A[\TrapAlt/]",
        false,
    )
    .unwrap();
    assert_eq!(result.graph.nodes[0].shape, NodeShape::TrapezoidAlt);
    assert_eq!(result.graph.nodes[0].label, "TrapAlt");
}

#[test]
fn test_node_shapes_mixed() {
    let input = "graph TD\nA[Rectangle]\nB(Rounded)\nC{Diamond}\nD[(Database)]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.nodes.len(), 4);

    let a = result.graph.nodes.iter().find(|n| n.id == "A").unwrap();
    assert_eq!(a.shape, NodeShape::Rectangle);

    let b = result.graph.nodes.iter().find(|n| n.id == "B").unwrap();
    assert_eq!(b.shape, NodeShape::Rounded);

    let c = result.graph.nodes.iter().find(|n| n.id == "C").unwrap();
    assert_eq!(c.shape, NodeShape::Diamond);

    let d = result.graph.nodes.iter().find(|n| n.id == "D").unwrap();
    assert_eq!(d.shape, NodeShape::Database);
}

#[test]
fn test_node_shapes_with_edges() {
    let input = "graph TD\nA{Decision} --> B((Success))\nA --> C[Failure]";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.nodes.len(), 3);
    assert_eq!(result.graph.edges.len(), 2);

    let a = result.graph.nodes.iter().find(|n| n.id == "A").unwrap();
    assert_eq!(a.shape, NodeShape::Diamond);

    let b = result.graph.nodes.iter().find(|n| n.id == "B").unwrap();
    assert_eq!(b.shape, NodeShape::Circle);

    let c = result.graph.nodes.iter().find(|n| n.id == "C").unwrap();
    assert_eq!(c.shape, NodeShape::Rectangle);
}

#[test]
fn test_undefined_node_default_rectangle() {
    let input = "graph TD\nA --> B";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.nodes[0].shape, NodeShape::Rectangle);
    assert_eq!(result.graph.nodes[1].shape, NodeShape::Rectangle);
}

#[test]
fn test_edge_circle_end_plain() {
    let result = parse("graph TD\nA --o B", false).unwrap();
    assert_eq!(result.graph.edges.len(), 1);
    assert_eq!(result.graph.edges[0].kind, EdgeKind::CircleEnd);
    assert_eq!(result.graph.edges[0].from, "A");
    assert_eq!(result.graph.edges[0].to, "B");
    assert!(result.graph.edges[0].label.is_none());
}

#[test]
fn test_edge_cross_end_plain() {
    let result = parse("graph TD\nA --x B", false).unwrap();
    assert_eq!(result.graph.edges.len(), 1);
    assert_eq!(result.graph.edges[0].kind, EdgeKind::CrossEnd);
    assert_eq!(result.graph.edges[0].from, "A");
    assert_eq!(result.graph.edges[0].to, "B");
}

#[test]
fn test_edge_circle_end_with_label() {
    let result = parse("graph TD\nA --o|ok| B", false).unwrap();
    assert_eq!(result.graph.edges[0].kind, EdgeKind::CircleEnd);
    assert_eq!(result.graph.edges[0].label, Some("ok".to_string()));
}

#[test]
fn test_edge_cross_end_with_label() {
    let result = parse("graph TD\nA --x|no| B", false).unwrap();
    assert_eq!(result.graph.edges[0].kind, EdgeKind::CrossEnd);
    assert_eq!(result.graph.edges[0].label, Some("no".to_string()));
}

#[test]
fn test_all_edge_kinds_together() {
    let input = "graph TD\nA --> B\nB --- C\nC ==> D\nD -.-> E\nE <--> F\nF --o G\nG --x H";
    let result = parse(input, false).unwrap();
    assert_eq!(result.graph.edges.len(), 7);
    assert_eq!(result.graph.edges[5].kind, EdgeKind::CircleEnd);
    assert_eq!(result.graph.edges[6].kind, EdgeKind::CrossEnd);
}

#[test]
fn test_node_double_circle_shape() {
    let result = parse("graph TD\nA(((Event)))", false).unwrap();
    assert_eq!(result.graph.nodes[0].shape, NodeShape::DoubleCircle);
    assert_eq!(result.graph.nodes[0].label, "Event");
}

#[test]
fn test_double_circle_not_confused_with_circle() {
    let result = parse("graph TD\nA((Circle))\nB(((Double)))", false).unwrap();
    assert_eq!(result.graph.nodes[0].shape, NodeShape::Circle);
    assert_eq!(result.graph.nodes[1].shape, NodeShape::DoubleCircle);
}

#[test]
fn test_double_circle_in_edge() {
    let result = parse("graph TD\nA(((Start))) --> B", false).unwrap();
    let start = result.graph.nodes.iter().find(|n| n.id == "A").unwrap();
    assert_eq!(start.shape, NodeShape::DoubleCircle);
    assert_eq!(start.label, "Start");
}

// === HTML ENTITY DECODING TESTS ===

#[test]
fn decode_mermaid_label_amp() {
    assert_eq!(decode_mermaid_label("A &amp; B"), "A & B");
}

#[test]
fn decode_mermaid_label_lt_gt() {
    assert_eq!(decode_mermaid_label("x &lt; y &gt; z"), "x < y > z");
}

#[test]
fn decode_mermaid_label_quot_apos() {
    assert_eq!(decode_mermaid_label("say &quot;hi&quot;"), "say \"hi\"");
    assert_eq!(decode_mermaid_label("it&apos;s"), "it's");
}

#[test]
fn decode_mermaid_label_nbsp() {
    assert_eq!(decode_mermaid_label("a&nbsp;b"), "a b");
}

#[test]
fn decode_mermaid_label_strips_inline_tags() {
    assert_eq!(decode_mermaid_label("<b>bold</b>"), "bold");
    assert_eq!(decode_mermaid_label("<i>italic</i>"), "italic");
    assert_eq!(decode_mermaid_label("<s>strike</s>"), "strike");
    assert_eq!(decode_mermaid_label("<u>under</u>"), "under");
    assert_eq!(decode_mermaid_label("<em>em</em>"), "em");
    assert_eq!(decode_mermaid_label("<strong>strong</strong>"), "strong");
    assert_eq!(decode_mermaid_label("<code>code</code>"), "code");
}

#[test]
fn decode_mermaid_label_combined() {
    assert_eq!(
        decode_mermaid_label("<b>Input &amp; Output</b>"),
        "Input & Output"
    );
}

#[test]
fn decode_mermaid_label_passthrough() {
    // Text with no entities or tags should come through unchanged
    assert_eq!(decode_mermaid_label("plain text"), "plain text");
    assert_eq!(decode_mermaid_label(""), "");
}

#[test]
fn html_entity_node_label_roundtrip() {
    let result = parse("graph TD\nA[Input &amp; Output]", false).unwrap();
    let node = result.graph.nodes.iter().find(|n| n.id == "A").unwrap();
    assert_eq!(node.label, "Input & Output");
}

#[test]
fn html_entity_edge_label_roundtrip() {
    let result = parse("graph LR\nA -->|x &lt; y| B", false).unwrap();
    assert_eq!(result.graph.edges[0].label.as_deref(), Some("x < y"));
}

#[test]
fn bold_tag_stripped_from_node_label() {
    let result = parse("graph TD\nA[<b>important</b>]", false).unwrap();
    let node = result.graph.nodes.iter().find(|n| n.id == "A").unwrap();
    assert_eq!(node.label, "important");
}

#[test]
fn classdef_lines_warn_without_creating_fake_nodes() {
    let result = parse(
        "graph TD\nA[Start]:::highlight --> B[End]\nclassDef highlight fill:#f00",
        false,
    )
    .unwrap();

    assert_eq!(result.graph.nodes.len(), 2);
    assert!(result.graph.nodes.iter().any(|node| node.id == "A"));
    assert!(result.graph.nodes.iter().any(|node| node.id == "B"));
    assert!(!result.graph.nodes.iter().any(|node| node.id == "highlight"));
    assert_eq!(result.graph.edges.len(), 1);
    assert!(result
        .graph
        .warnings
        .iter()
        .any(|warning| warning.contains("Mermaid classes not supported")));
}
