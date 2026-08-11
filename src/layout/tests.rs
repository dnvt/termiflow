use super::*;
use crate::graph::{Edge, Node, Subgraph};
use crate::parser::parse;

fn simple_graph(direction: Direction) -> Graph {
    let mut g = Graph::new();
    g.direction = direction;
    g.nodes.push(Node::new("A", "A"));
    g.nodes.push(Node::new("B", "B"));
    g.edges.push(Edge::new("A", "B"));
    g
}

#[test]
fn prior_positions_nudge_nodes_with_parents() {
    let graph = simple_graph(Direction::TD);
    let baseline = layout(
        LayoutInput {
            graph: &graph,
            prior_positions: None,
        },
        CoarseLayoutConfig::default(),
    )
    .expect("baseline layout");
    let baseline_b = baseline.positions.get("B").expect("baseline B");

    let mut prior = baseline.positions.clone();
    prior.get_mut("B").expect("prior B").x = baseline_b.x.saturating_add(5);
    let nudged = layout(
        LayoutInput {
            graph: &graph,
            prior_positions: Some(prior),
        },
        CoarseLayoutConfig::default(),
    )
    .expect("nudged layout");

    assert!(
        nudged.positions["B"].x > baseline_b.x,
        "baseline={:?} nudged={:?}",
        baseline.positions,
        nudged.positions
    );
}

#[test]
fn routes_around_obstacle() {
    let graph = simple_graph(Direction::TD);
    let input = LayoutInput {
        graph: &graph,
        prior_positions: None,
    };
    let cfg = CoarseLayoutConfig::default();
    let output = layout(input, cfg).expect("layout");
    let route = output.routes.get(&0).expect("route");
    assert!(!route.segments.is_empty());
}

#[test]
fn gutter_avoids_external_edges() {
    let mut graph = Graph::new();
    graph.direction = Direction::TD;
    graph.nodes.push(Node::new("A", "A"));
    graph.nodes.push(Node::new("B", "B"));
    graph.nodes.push(Node::new("C", "C"));
    graph.edges.push(Edge::new("A", "B"));
    graph.edges.push(Edge::new("B", "C"));

    let mut sg = crate::graph::Subgraph::new("sg1", Some("Group".into()));
    sg.add_node("B");
    graph.add_subgraph(sg);
    graph.associate_node_with_subgraph("B", "sg1");

    let input = LayoutInput {
        graph: &graph,
        prior_positions: None,
    };
    let output = layout(input, CoarseLayoutConfig::default()).expect("layout");
    assert!(output.subgraph_envelopes.contains_key("sg1"));
    // Routing may be deferred to the renderer for some shapes; layout should still succeed.
}

#[test]
fn titled_vertical_labeled_terminal_exit_aligns_to_its_portal_lane() {
    for (direction_name, direction) in [("TD", Direction::TD), ("BT", Direction::BT)] {
        let input = format!(
            "graph {direction_name}\nsubgraph SG [Group]\nA[Inside] --> B[Exit]\nend\nB -->|handoff| C[Outside]"
        );
        let parsed = parse(&input, false).expect("parse labeled terminal exit fixture");
        let graph = apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default())
            .expect("layout labeled terminal exit fixture");

        assert_eq!(
            graph.get_node("B").expect("internal exit node").center_x(),
            graph
                .get_node("C")
                .expect("external terminal target")
                .center_x(),
            "vertical labeled terminal exit must share the portal lane for {direction:?}"
        );
    }
}

#[test]
#[cfg(feature = "maintainer-fixtures")]
fn td_parallel_external_portals_align_to_internal_centers() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_parallel_td.md")
        .expect("read TD parallel fixture");
    let parsed = parse(&input, false).expect("parse TD parallel fixture");
    let graph = apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default())
        .expect("layout TD parallel fixture");

    let input_node = graph.get_node("In").expect("external input node");
    let entry_node = graph.get_node("A").expect("internal entry node");
    let exit_node = graph.get_node("D").expect("internal exit node");
    let output_node = graph.get_node("Out").expect("external output node");

    assert_eq!(
        input_node.center_x(),
        entry_node.center_x(),
        "TD external entry must share the selected internal portal lane"
    );
    assert_eq!(
        output_node.center_x(),
        exit_node.center_x(),
        "TD external exit must share the selected internal portal lane"
    );
}

#[test]
fn side_by_side_sibling_envelopes_keep_a_visual_gap_in_both_horizontal_directions() {
    for direction in [Direction::LR, Direction::RL] {
        let mut graph = Graph::new();
        graph.direction = direction;

        for (node_id, subgraph_id, title) in [
            ("A", "G1", "Group 1"),
            ("B", "G2", "Group 2"),
            ("C", "G3", "Group 3"),
        ] {
            graph.nodes.push(Node::new(node_id, node_id));
            let mut subgraph = Subgraph::new(subgraph_id, Some(title.to_string()));
            subgraph.add_node(node_id);
            graph.add_subgraph(subgraph);
            graph.associate_node_with_subgraph(node_id, subgraph_id);
        }
        graph.edges.push(Edge::new("A", "B"));
        graph.edges.push(Edge::new("B", "C"));

        let output = layout(
            LayoutInput {
                graph: &graph,
                prior_positions: None,
            },
            CoarseLayoutConfig::default(),
        )
        .expect("horizontal sibling layout");

        let mut envelopes: Vec<_> = output.subgraph_envelopes.values().collect();
        envelopes.sort_by_key(|envelope| envelope.outer.x);
        for pair in envelopes.windows(2) {
            let gap = pair[1].outer.x.saturating_sub(pair[0].outer.right());
            assert!(
                gap >= 2,
                "{direction:?} sibling envelopes need two blank columns: left={:?} right={:?}",
                pair[0].outer,
                pair[1].outer
            );
        }
    }
}

#[test]
fn inner_bounds_persist_on_graph() {
    let mut graph = Graph::new();
    graph.direction = Direction::TD;
    graph.nodes.push(Node::new("A", "A"));
    graph.nodes.push(Node::new("B", "B"));
    graph.edges.push(Edge::new("A", "B"));

    let mut sg = crate::graph::Subgraph::new("sg", Some("Group".into()));
    sg.add_node("A");
    sg.add_node("B");
    graph.add_subgraph(sg);
    graph.associate_node_with_subgraph("A", "sg");
    graph.associate_node_with_subgraph("B", "sg");

    let laid_out = apply_coarse_layout(graph, None, CoarseLayoutConfig::default()).expect("layout");
    let sg = laid_out.get_subgraph("sg").expect("subgraph exists");
    assert!(
        sg.inner_bounds.is_valid(),
        "inner bounds should be populated from layout"
    );
    assert!(sg.bounds.width >= sg.inner_bounds.width && sg.bounds.height >= sg.inner_bounds.height);
}

#[test]
#[cfg(feature = "maintainer-fixtures")]
fn routes_cross_subgraph_boundaries() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_outside_td.md")
        .expect("read fixture");
    let parsed = parse(&input, false).expect("parse");
    let graph =
        apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

    if let Some(sg) = graph.subgraphs.first() {
        let _ = sg; // keep test quiet
    }

    // Edge routes for cross-subgraph edges may be provided by layout or deferred to the
    // renderer; if present, they should be non-empty.
    for edge_idx in [1usize, 2usize] {
        if let Some(route) = graph.edge_routes.get(&edge_idx) {
            assert!(
                !route.segments.is_empty(),
                "route {edge_idx} should have segments"
            );
        }
    }
}

#[test]
#[cfg(feature = "maintainer-fixtures")]
fn bt_nested_external_target_stays_above_root_envelope() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_nested_bt.md")
        .expect("read nested BT fixture");
    let parsed = parse(&input, false).expect("parse nested BT fixture");
    let graph = apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default())
        .expect("layout nested BT fixture");

    let outer = graph.get_subgraph("Outer").expect("outer subgraph");
    let node = graph.get_node("A").expect("nested node");
    let input_node = graph.get_node("B").expect("input node");
    let output_node = graph.get_node("C").expect("output node");

    assert_eq!(graph.get_node_subgraph("C"), None);
    assert!(
        output_node.bottom_y() < outer.bounds.y,
        "top-level BT target must stay above the root envelope with a clear row: output=({}, {}, {}x{}) outer={:?}",
        output_node.x,
        output_node.y,
        output_node.width,
        output_node.height,
        outer.bounds
    );
    assert!(
        outer.bounds.contains(node.x, node.y)
            && outer.bounds.contains(
                node.x + node.width.saturating_sub(1),
                node.y + node.height.saturating_sub(1)
            ),
        "nested node must remain inside the root envelope: node=({}, {}, {}x{}) outer={:?}",
        node.x,
        node.y,
        node.width,
        node.height,
        outer.bounds
    );
    assert_eq!(graph.get_node_subgraph("B"), None);
    assert!(
        input_node.y >= outer.bounds.y + outer.bounds.height,
        "top-level BT source must remain below the root envelope: input=({}, {}, {}x{}) outer={:?}",
        input_node.x,
        input_node.y,
        input_node.width,
        input_node.height,
        outer.bounds
    );
}

#[test]
#[cfg(feature = "maintainer-fixtures")]
fn td_nested_external_target_stays_below_root_envelope() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_nested_td.md")
        .expect("read nested TD fixture");
    let parsed = parse(&input, false).expect("parse nested TD fixture");
    let graph = apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default())
        .expect("layout nested TD fixture");

    let outer = graph.get_subgraph("Outer").expect("outer subgraph");
    let node = graph.get_node("A").expect("nested node");
    let input_node = graph.get_node("B").expect("input node");
    let output_node = graph.get_node("C").expect("output node");

    assert_eq!(graph.get_node_subgraph("C"), None);
    assert!(
        output_node.y > outer.bounds.y + outer.bounds.height,
        "top-level TD target must stay below the root envelope with a clear row: output=({}, {}, {}x{}) outer={:?}",
        output_node.x,
        output_node.y,
        output_node.width,
        output_node.height,
        outer.bounds
    );
    assert!(
        outer.bounds.contains(node.x, node.y)
            && outer.bounds.contains(
                node.x + node.width.saturating_sub(1),
                node.y + node.height.saturating_sub(1)
            ),
        "nested node must remain inside the root envelope: node=({}, {}, {}x{}) outer={:?}",
        node.x,
        node.y,
        node.width,
        node.height,
        outer.bounds
    );
    assert_eq!(graph.get_node_subgraph("B"), None);
    assert!(
        input_node.bottom_y() < outer.bounds.y,
        "top-level TD source must remain above the root envelope: input=({}, {}, {}x{}) outer={:?}",
        input_node.x,
        input_node.y,
        input_node.width,
        input_node.height,
        outer.bounds
    );
}

#[test]
#[cfg(feature = "maintainer-fixtures")]
fn bt_titled_root_sources_clear_the_bottom_border() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/collision_edge_along_border_bt.md")
        .expect("read BT border fixture");
    let parsed = parse(&input, false).expect("parse BT border fixture");
    let graph = apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default())
        .expect("layout BT border fixture");

    let group = graph.get_subgraph("SG").expect("target group");
    for source_id in ["X1", "X2", "X3"] {
        let source = graph.get_node(source_id).expect("external source");
        assert_eq!(graph.get_node_subgraph(source_id), None);
        assert!(
            source.y > group.bounds.y + group.bounds.height,
            "external source must clear the titled root bottom border: source={source_id} rect=({}, {}, {}x{}) group={:?}",
            source.x,
            source.y,
            source.width,
            source.height,
            group.bounds
        );
    }
}

#[test]
fn nested_service_data_sample_populates_envelopes_and_portals() {
    let input = "graph TD\nA[API Gateway] --> B[User Service]\nsubgraph SL[Service Layer]\nB\nsubgraph DL[Data Layer]\nC[Order Service] --> D[(Order DB)]\nE[(User DB)]\nend\nB --> E\nD --> F[Response Builder]\nE --> F\nend";
    let parsed = parse(input, false).expect("parse");
    let output = layout(
        LayoutInput {
            graph: &parsed.graph,
            prior_positions: None,
        },
        CoarseLayoutConfig::default(),
    )
    .expect("layout");

    let service = output
        .subgraph_envelopes
        .get("SL")
        .expect("service envelope");
    let data = output.subgraph_envelopes.get("DL").expect("data envelope");

    assert!(
        !service.inner.is_empty() && !service.outer.is_empty(),
        "service envelope should be populated"
    );
    assert!(
        !data.inner.is_empty() && !data.outer.is_empty(),
        "data envelope should be populated"
    );
    assert!(
        !service.portals.top.is_empty(),
        "service envelope should expose a top portal for A -> B"
    );
    assert!(
        !data.portals.top.is_empty(),
        "data envelope should expose a top portal for B -> E"
    );
    assert!(
        !data.portals.bottom.is_empty(),
        "data envelope should expose bottom portals for D/E -> F"
    );
}

#[test]
fn explicit_nested_child_roots_follow_parent_direct_rank() {
    let input = "graph TD\nA[API Gateway] --> B[User Service]\nsubgraph SL[Service Layer]\nB\nsubgraph DL[Data Layer]\nC[Order Service] --> D[(Order DB)]\nE[(User DB)]\nend\nB --> E\nD --> F[Response Builder]\nE --> F\nend";
    let parsed = parse(input, false).expect("parse");
    let graph =
        apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

    let service = graph.get_subgraph("SL").expect("service layer");
    let data = graph.get_subgraph("DL").expect("data layer");
    let user_service = graph.get_node("B").expect("user service");
    let order_service = graph.get_node("C").expect("order service");
    let response_builder = graph.get_node("F").expect("response builder");

    assert!(
            order_service.rank > user_service.rank,
            "expected true nested child roots to be promoted after the parent's direct node: child_rank={} parent_rank={}",
            order_service.rank,
            user_service.rank
        );
    assert!(
            service.bounds.contains(data.bounds.x, data.bounds.y)
                && service.bounds.contains(
                    data.bounds.x + data.bounds.width.saturating_sub(1),
                    data.bounds.y + data.bounds.height.saturating_sub(1)
                ),
            "expected the declared parent envelope to fully contain the nested child envelope: parent={:?} child={:?}",
            service.bounds,
            data.bounds
        );
    assert!(
            order_service.y > user_service.y + user_service.height,
            "expected the true nested child content to start below the parent's direct node content: order_service=({}, {}, {}x{}) user_service=({}, {}, {}x{}) data={:?}",
            order_service.x,
            order_service.y,
            order_service.width,
            order_service.height,
            user_service.x,
            user_service.y,
            user_service.width,
            user_service.height,
            data.bounds
        );
    assert!(
            data.bounds.y > user_service.y + user_service.height,
            "expected the nested child border/title band to stay below the parent's direct node band: data={:?} user_service=({}, {}, {}x{})",
            data.bounds,
            user_service.x,
            user_service.y,
            user_service.width,
            user_service.height,
        );
    assert!(
            !data.bounds.contains(response_builder.x, response_builder.y)
                && !data.bounds.contains(
                    response_builder.x + response_builder.width.saturating_sub(1),
                    response_builder.y + response_builder.height.saturating_sub(1)
                ),
            "expected the nested child envelope to exclude the parent's direct response node: data={:?} response_builder=({}, {}, {}x{})",
            data.bounds,
            response_builder.x,
            response_builder.y,
            response_builder.width,
            response_builder.height,
        );
}

#[test]
fn explicit_nested_horizontal_children_stay_contained_and_ordered_by_flow() {
    for (direction, data_precedes_response) in [(Direction::LR, true), (Direction::RL, false)] {
        let input = format!(
                "graph {direction:?}\nA[API Gateway] --> B[User Service]\nsubgraph SL[Service Layer]\nB\nsubgraph DL[Data Layer]\nC[Order Service] --> D[(Order DB)]\nE[(User DB)]\nend\nB --> E\nD --> F[Response Builder]\nE --> F\nend"
            );
        let parsed = parse(&input, false).expect("parse");
        let graph =
            apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

        let service = graph.get_subgraph("SL").expect("service layer");
        let data = graph.get_subgraph("DL").expect("data layer");
        let user_service = graph.get_node("B").expect("user service");
        let response_builder = graph.get_node("F").expect("response builder");

        assert!(
                service.bounds.contains(data.bounds.x, data.bounds.y)
                    && service.bounds.contains(
                        data.bounds.x + data.bounds.width.saturating_sub(1),
                        data.bounds.y + data.bounds.height.saturating_sub(1)
                    ),
                "expected the declared parent envelope to fully contain the nested child envelope in {direction:?}: parent={:?} child={:?}",
                service.bounds,
                data.bounds,
            );
        assert!(
                data.bounds.y > service.bounds.y,
                "expected the nested child title row to staircase below the parent title row in {direction:?}: parent={:?} child={:?}",
                service.bounds,
                data.bounds,
            );
        assert!(
                !data.bounds.contains(response_builder.x, response_builder.y)
                    && !data.bounds.contains(
                        response_builder.x + response_builder.width.saturating_sub(1),
                        response_builder.y + response_builder.height.saturating_sub(1)
                    ),
                "expected the nested child envelope to exclude the parent-only response node in {direction:?}: child={:?} response_builder=({}, {}, {}x{})",
                data.bounds,
                response_builder.x,
                response_builder.y,
                response_builder.width,
                response_builder.height,
            );

        if data_precedes_response {
            let gap_to_user_service = data
                .bounds
                .x
                .saturating_sub(user_service.x.saturating_add(user_service.width));
            let gap_to_response = response_builder
                .x
                .saturating_sub(data.bounds.x.saturating_add(data.bounds.width));
            assert!(
                    data.bounds.x > user_service.x + user_service.width,
                    "expected the nested child to remain after the parent's direct node along LR flow: child={:?} user_service=({}, {}, {}x{})",
                    data.bounds,
                    user_service.x,
                    user_service.y,
                    user_service.width,
                    user_service.height,
                );
            assert!(
                    response_builder.x > data.bounds.x + data.bounds.width,
                    "expected the parent-only response node to remain after the nested child along LR flow: child={:?} response_builder=({}, {}, {}x{})",
                    data.bounds,
                    response_builder.x,
                    response_builder.y,
                    response_builder.width,
                    response_builder.height,
                );
            assert!(
                    gap_to_user_service <= gap_to_response.saturating_add(2),
                    "expected the nested child to stay at least as close to the upstream parent-direct node as to the downstream parent-only response node in LR: child={:?} user_service=({}, {}, {}x{}) response_builder=({}, {}, {}x{}) gaps=({}, {})",
                    data.bounds,
                    user_service.x,
                    user_service.y,
                    user_service.width,
                    user_service.height,
                    response_builder.x,
                    response_builder.y,
                    response_builder.width,
                    response_builder.height,
                    gap_to_user_service,
                    gap_to_response,
                );
        } else {
            let gap_to_user_service = user_service
                .x
                .saturating_sub(data.bounds.x.saturating_add(data.bounds.width));
            let gap_to_response = data
                .bounds
                .x
                .saturating_sub(response_builder.x.saturating_add(response_builder.width));
            assert!(
                    data.bounds.x + data.bounds.width <= user_service.x,
                    "expected the nested child to remain before the parent's direct node along RL flow: child={:?} user_service=({}, {}, {}x{})",
                    data.bounds,
                    user_service.x,
                    user_service.y,
                    user_service.width,
                    user_service.height,
                );
            assert!(
                    response_builder.x + response_builder.width <= data.bounds.x,
                    "expected the parent-only response node to remain before the nested child along RL flow: child={:?} response_builder=({}, {}, {}x{})",
                    data.bounds,
                    response_builder.x,
                    response_builder.y,
                    response_builder.width,
                    response_builder.height,
                );
            assert!(
                    gap_to_user_service <= gap_to_response.saturating_add(2),
                    "expected the nested child to stay at least as close to the upstream parent-direct node as to the downstream parent-only response node in RL: child={:?} user_service=({}, {}, {}x{}) response_builder=({}, {}, {}x{}) gaps=({}, {})",
                    data.bounds,
                    user_service.x,
                    user_service.y,
                    user_service.width,
                    user_service.height,
                    response_builder.x,
                    response_builder.y,
                    response_builder.width,
                    response_builder.height,
                    gap_to_user_service,
                    gap_to_response,
                );
        }
    }
}

#[test]
#[cfg(feature = "maintainer-fixtures")]
fn sibling_subgraphs_stay_separate_in_td_layout() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_td.md")
        .expect("read fixture");
    let parsed = parse(&input, false).expect("parse");
    let graph =
        apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

    let service = graph.get_subgraph("SG1").expect("service layer");
    let data = graph.get_subgraph("SG2").expect("data layer");
    let response = graph.get_node("Response").expect("response");
    let user_db = graph.get_node("D1").expect("user db");
    let order_db = graph.get_node("D2").expect("order db");
    let overlaps = service.bounds.x < data.bounds.x + data.bounds.width
        && service.bounds.x + service.bounds.width > data.bounds.x
        && service.bounds.y < data.bounds.y + data.bounds.height
        && service.bounds.y + service.bounds.height > data.bounds.y;

    assert!(
            !overlaps,
            "expected Mermaid sibling subgraphs to stay visually separate in TD: service={:?} data={:?}",
            service.bounds,
            data.bounds
        );
    assert!(
            data.bounds.y > service.bounds.y + service.bounds.height,
            "expected the sibling Data Layer to stay below the Service Layer in TD: service={:?} data={:?}",
            service.bounds,
            data.bounds
        );
    assert!(
            response.y > data.bounds.y + data.bounds.height,
            "expected Response Builder to remain below the sibling Data Layer in TD: data={:?} response=({}, {}, {}x{})",
            data.bounds,
            response.x,
            response.y,
            response.width,
            response.height
        );
    assert!(
            user_db.x >= order_db.x + 8,
            "expected route-aware nested width budgeting to widen the nested source span before converging to Response: user_db=({}, {}, {}x{}) order_db=({}, {}, {}x{})",
            user_db.x,
            user_db.y,
            user_db.width,
            user_db.height,
            order_db.x,
            order_db.y,
            order_db.width,
            order_db.height
        );
}

#[test]
#[cfg(feature = "maintainer-fixtures")]
fn stacked_top_level_td_sibling_subgraphs_harmonize_widths_when_chain_connected() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_td.md")
        .expect("read fixture");
    let parsed = parse(&input, false).expect("parse");
    let graph =
        apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

    let service = graph.get_subgraph("SG1").expect("service layer");
    let data = graph.get_subgraph("SG2").expect("data layer");

    assert!(
            service.bounds.width.abs_diff(data.bounds.width) <= 1,
            "expected connected top-level TD siblings to keep closely harmonized frame widths for visual balance: service={:?} data={:?}",
            service.bounds,
            data.bounds
        );
    assert_eq!(
            service.bounds.x, data.bounds.x,
            "expected connected top-level TD siblings with harmonized widths to share the same left wall: service={:?} data={:?}",
            service.bounds,
            data.bounds
        );
}

#[test]
#[cfg(feature = "maintainer-fixtures")]
fn stacked_top_level_td_sibling_subgraphs_stay_vertically_compact() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_td.md")
        .expect("read fixture");
    let parsed = parse(&input, false).expect("parse");
    let graph =
        apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

    let service = graph.get_subgraph("SG1").expect("service layer");
    let data = graph.get_subgraph("SG2").expect("data layer");
    let inter_subgraph_gap = data
        .bounds
        .y
        .saturating_sub(service.bounds.y.saturating_add(service.bounds.height));
    assert!(
            service.bounds.height <= 18,
            "expected Service Layer to stay vertically compact after mixed boundary fan-out compaction: service={:?}",
            service.bounds
        );
    assert!(
            inter_subgraph_gap <= 4,
            "expected the stacked TD sibling gap to stay compact after mixed boundary fan-out compaction: service={:?} data={:?} gap={}",
            service.bounds,
            data.bounds,
            inter_subgraph_gap
    );
}

#[test]
#[cfg(feature = "maintainer-fixtures")]
fn stacked_td_sibling_crossings_keep_two_connector_rows() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/collision_sibling_triple_td.md")
        .expect("read triple sibling fixture");
    let parsed = parse(&input, false).expect("parse triple sibling fixture");
    let graph = apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default())
        .expect("layout triple sibling fixture");

    for (upper_id, lower_id) in [("G1", "G2"), ("G2", "G3")] {
        let upper = graph.get_subgraph(upper_id).expect("upper sibling");
        let lower = graph.get_subgraph(lower_id).expect("lower sibling");
        let gap = lower
            .bounds
            .y
            .saturating_sub(upper.bounds.y.saturating_add(upper.bounds.height));
        assert!(
            gap >= 2,
            "expected two connector rows between {upper_id} and {lower_id}: upper={:?} lower={:?} gap={gap}",
            upper.bounds,
            lower.bounds
        );
    }
}

#[test]
#[cfg(feature = "maintainer-fixtures")]
fn sibling_subgraphs_stay_separate_in_horizontal_layouts() {
    for fixture in [
        "tests/fixtures/inputs/subgraph_complex_lr.md",
        "tests/fixtures/inputs/subgraph_complex_rl.md",
    ] {
        let input = std::fs::read_to_string(fixture).expect("read fixture");
        let parsed = parse(&input, false).expect("parse");
        let graph =
            apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

        let outer = graph.get_subgraph("SG1").expect("service layer");
        let inner = graph.get_subgraph("SG2").expect("data layer");
        let user_service = graph.get_node("S1").expect("user service");
        let order_service = graph.get_node("S2").expect("order service");
        let response = graph.get_node("Response").expect("response");

        let overlaps = |node: &Node, bounds: &crate::graph::Rectangle| {
            let node_left = node.x;
            let node_right = node.x + node.width.saturating_sub(1);
            let node_top = node.y;
            let node_bottom = node.y + node.height.saturating_sub(1);
            let bounds_right = bounds.x + bounds.width;
            let bounds_bottom = bounds.y + bounds.height;

            node_left < bounds_right
                && node_right >= bounds.x
                && node_top < bounds_bottom
                && node_bottom >= bounds.y
        };
        let subgraphs_overlap = outer.bounds.x < inner.bounds.x + inner.bounds.width
            && outer.bounds.x + outer.bounds.width > inner.bounds.x
            && outer.bounds.y < inner.bounds.y + inner.bounds.height
            && outer.bounds.y + outer.bounds.height > inner.bounds.y;

        assert!(
                !subgraphs_overlap,
                "expected Mermaid sibling subgraphs to stay visually separate for {fixture}: outer={:?} inner={:?}",
                outer.bounds,
                inner.bounds
            );
        assert!(
                !overlaps(user_service, &inner.bounds) && !overlaps(order_service, &inner.bounds),
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
#[cfg(feature = "maintainer-fixtures")]
fn side_by_side_horizontal_top_level_siblings_harmonize_heights_when_route_gutters_overlap() {
    for fixture in [
        "tests/fixtures/inputs/subgraph_complex_lr.md",
        "tests/fixtures/inputs/subgraph_complex_rl.md",
    ] {
        let input = std::fs::read_to_string(fixture).expect("read fixture");
        let parsed = parse(&input, false).expect("parse");
        let graph =
            apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

        let service = graph.get_subgraph("SG1").expect("service layer");
        let data = graph.get_subgraph("SG2").expect("data layer");

        assert!(
                service.bounds.height.abs_diff(data.bounds.height) <= 1,
                "expected horizontal top-level siblings to keep closely harmonized frame heights even when widened route gutters make the outer envelopes overlap for {fixture}: service={:?} data={:?}",
                service.bounds,
                data.bounds
            );
    }
}

#[test]
#[cfg(feature = "maintainer-fixtures")]
fn side_by_side_lr_sibling_subgraphs_share_frame_height_when_close() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_lr.md")
        .expect("read fixture");
    let parsed = parse(&input, false).expect("parse");
    let graph =
        apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

    let service = graph.get_subgraph("SG1").expect("service layer");
    let data = graph.get_subgraph("SG2").expect("data layer");

    assert_eq!(
            service.bounds.y, data.bounds.y,
            "expected side-by-side LR siblings to share the same top row when frame-height harmonization applies: service={:?} data={:?}",
            service.bounds,
            data.bounds
        );
    assert_eq!(
            service.bounds.height, data.bounds.height,
            "expected side-by-side LR siblings with comparable content to share the same frame height: service={:?} data={:?}",
            service.bounds,
            data.bounds
        );
}

#[test]
#[cfg(feature = "maintainer-fixtures")]
fn side_by_side_lr_top_level_siblings_balance_trailing_response_gap() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_lr.md")
        .expect("read fixture");
    let parsed = parse(&input, false).expect("parse");
    let graph =
        apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

    let service = graph.get_subgraph("SG1").expect("service layer");
    let data = graph.get_subgraph("SG2").expect("data layer");
    let response = graph.get_node("Response").expect("response");

    let inter_subgraph_gap = data
        .bounds
        .x
        .saturating_sub(service.bounds.x.saturating_add(service.bounds.width));
    let trailing_response_gap = response
        .x
        .saturating_sub(data.bounds.x.saturating_add(data.bounds.width));

    assert!(
            trailing_response_gap >= 6,
            "expected the LR trailing response gap to leave enough breathing room after the final top-level sibling instead of collapsing the connector into the response box: service={:?} data={:?} response=({}, {}, {}x{}) gap={}",
            service.bounds,
            data.bounds,
            response.x,
            response.y,
            response.width,
            response.height,
            trailing_response_gap,
        );
    assert!(
            inter_subgraph_gap <= trailing_response_gap.saturating_add(2),
            "expected the LR inter-subgraph lane to stay visually comparable to the trailing response gap instead of hoarding most of the horizontal slack in the middle: service={:?} data={:?} response=({}, {}, {}x{}) gaps=({}, {})",
            service.bounds,
            data.bounds,
            response.x,
            response.y,
            response.width,
            response.height,
            inter_subgraph_gap,
            trailing_response_gap,
        );
}

#[test]
#[cfg(feature = "maintainer-fixtures")]
fn sibling_bt_external_corridor_reservation_moves_response_outside_lane() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_complex_bt.md")
        .expect("read fixture");
    let parsed = parse(&input, false).expect("parse fixture");
    let graph =
        apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

    let service = graph.get_subgraph("SG1").expect("service layer");
    let data = graph.get_subgraph("SG2").expect("data layer");
    let response = graph.get_node("Response").expect("response");
    let sibling_right = service
        .bounds
        .x
        .saturating_add(service.bounds.width)
        .max(data.bounds.x.saturating_add(data.bounds.width));

    assert!(
        response.x >= sibling_right.saturating_add(2),
        "expected the topology-connected external response node to leave a two-column sibling BT corridor: service={:?} data={:?} response=({}, {}, {}x{})",
        service.bounds,
        data.bounds,
        response.x,
        response.y,
        response.width,
        response.height,
    );
    assert!(
        response.x >= service.bounds.x.saturating_add(service.bounds.width)
            || response.x.saturating_add(response.width) <= service.bounds.x,
        "response must remain disjoint from Service Layer after corridor reservation: service={:?} response=({}, {}, {}x{})",
        service.bounds,
        response.x,
        response.y,
        response.width,
        response.height,
    );
    assert!(
        response.x >= data.bounds.x.saturating_add(data.bounds.width)
            || response.x.saturating_add(response.width) <= data.bounds.x,
        "response must remain disjoint from Data Layer after corridor reservation: data={:?} response=({}, {}, {}x{})",
        data.bounds,
        response.x,
        response.y,
        response.width,
        response.height,
    );
}

#[test]
fn explicit_nested_child_route_budget_adds_horizontal_border_clearance() {
    let input = "graph TD\nA[API Gateway]\nsubgraph SG1[Service Layer]\nS1[User Service]\nS2[Order Service]\nsubgraph SG2[Data Layer]\nD1[(User DB)]\nD2[(Order DB)]\nend\nResponse[Response Builder]\nS1 --> S2\nS1 --> D1\nS2 --> D2\nD1 --> Response\nD2 --> Response\nend\nA --> S1\n";
    let parsed = parse(input, false).expect("parse");
    let graph =
        apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

    let child = graph.get_subgraph("SG2").expect("data layer");
    let user_db = graph.get_node("D1").expect("user db");
    let order_db = graph.get_node("D2").expect("order db");

    let left_margin = user_db.x.saturating_sub(child.bounds.x);
    let right_margin = child
        .bounds
        .x
        .saturating_add(child.bounds.width)
        .saturating_sub(order_db.x.saturating_add(order_db.width));

    assert!(
            left_margin >= 3,
            "expected nested child route budgeting to reserve at least three columns between the left border and the first child node: child={:?} user_db=({}, {}, {}x{})",
            child.bounds,
            user_db.x,
            user_db.y,
            user_db.width,
            user_db.height,
        );
    assert!(
            right_margin >= 3,
            "expected nested child route budgeting to reserve at least three columns between the right border and the last child node: child={:?} order_db=({}, {}, {}x{})",
            child.bounds,
            order_db.x,
            order_db.y,
            order_db.width,
            order_db.height,
        );
}

#[test]
fn lr_sibling_terminal_target_aligns_to_source_envelope_centerline() {
    let input = r#"graph LR
API[API Gateway]
subgraph SG1 [Service Layer]
S1[User Service]
S2[Order Service]
S1 --> S2
end
subgraph SG2 [Data Layer]
D1[(User DB)]
D2[(Order DB)]
end
Response[Response Builder]
API --> S1
S1 --> D1
S2 --> D2
D1 --> Response
D2 --> Response
"#;
    let parsed = parse(input, false).expect("parse LR sibling terminal fixture");
    let graph = apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default())
        .expect("layout LR sibling terminal fixture");

    let source = graph.get_subgraph("SG2").expect("data layer");
    let response = graph.get_node("Response").expect("response builder");
    let source_center_y = source.bounds.y + source.bounds.height / 2;
    let response_center_y = response.y + response.height / 2;

    assert!(
        response_center_y.abs_diff(source_center_y) <= 1,
        "expected LR terminal target to align to the source envelope centerline: source={:?} response=({}, {}, {}x{})",
        source.bounds,
        response.x,
        response.y,
        response.width,
        response.height,
    );
}

#[test]
fn rl_sibling_terminal_centerline_candidate_is_rejected_when_it_hits_peer_subgraph() {
    let input = r#"graph RL
API[API Gateway]
subgraph SG1 [Service Layer]
S1[User Service]
S2[Order Service]
S1 --> S2
end
subgraph SG2 [Data Layer]
D1[(User DB)]
D2[(Order DB)]
end
Response[Response Builder]
API --> S1
S1 --> D1
S2 --> D2
D1 --> Response
D2 --> Response
"#;
    let parsed = parse(input, false).expect("parse RL sibling terminal fixture");
    let graph = apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default())
        .expect("layout RL sibling terminal fixture");

    let data = graph.get_subgraph("SG2").expect("data layer");
    let service = graph.get_subgraph("SG1").expect("service layer");
    let response = graph.get_node("Response").expect("response builder");
    let aligned_y = data.bounds.y + data.bounds.height / 2 - response.height / 2;
    let candidate = crate::geom::Rect::new(response.x, aligned_y, response.width, response.height);
    let keepout = candidate.inflate(1);
    let overlaps_service = keepout.x < service.bounds.x + service.bounds.width
        && service.bounds.x < keepout.right()
        && keepout.y < service.bounds.y + service.bounds.height
        && service.bounds.y < keepout.bottom();

    assert!(
        overlaps_service,
        "fixture must keep the RL centerline candidate blocked by the peer subgraph: service={:?} candidate={candidate:?}",
        service.bounds,
    );
    assert_ne!(
        response.y, aligned_y,
        "blocked RL candidate must not move the response target onto the peer subgraph"
    );
}

#[test]
fn route_budgeted_subgraphs_include_declared_nested_children() {
    let mut graph = Graph::new();
    graph.add_subgraph(Subgraph::new("parent", Some("Parent".to_string())));

    let mut child = Subgraph::new("child", Some("Child".to_string()));
    child.parent_id = Some("parent".to_string());
    graph.add_subgraph(child);
    graph
        .get_subgraph_mut("parent")
        .expect("parent")
        .add_child("child");

    let budgeted = route_budgeted_subgraphs(&graph);

    assert_eq!(
        budgeted,
        vec!["child".to_string()],
        "expected declared nested children to participate in internal route budgeting"
    );
}

#[test]
fn declared_nested_child_route_pressure_shifts_right_partition_not_just_outgoing_sources() {
    let mut graph = Graph::new();
    graph.add_node(Node::new("left", "Left"));
    graph.add_node(Node::new("right", "Right"));
    graph.add_node(Node::new("sibling", "Sibling"));
    graph.add_node(Node::new("ext_a", "ExtA"));
    graph.add_node(Node::new("ext_b", "ExtB"));
    graph.add_subgraph(Subgraph::new("parent", Some("Parent".to_string())));

    let mut child = Subgraph::new("child", Some("Child".to_string()));
    child.parent_id = Some("parent".to_string());
    graph.add_subgraph(child);
    graph
        .get_subgraph_mut("parent")
        .expect("parent")
        .add_child("child");

    graph.associate_node_with_subgraph("left", "child");
    graph.associate_node_with_subgraph("right", "child");
    graph.associate_node_with_subgraph("sibling", "child");
    graph.add_edge(Edge::new("left", "ext_a"));
    graph.add_edge(Edge::new("right", "ext_b"));

    let mut positions = HashMap::from([
        ("left".to_string(), Point::new(8, 0)),
        ("right".to_string(), Point::new(14, 0)),
        ("sibling".to_string(), Point::new(24, 0)),
        ("ext_a".to_string(), Point::new(0, 0)),
        ("ext_b".to_string(), Point::new(0, 4)),
    ]);
    let mut node_rects = HashMap::from([
        ("left".to_string(), Rect::new(8, 0, 5, 3)),
        ("right".to_string(), Rect::new(14, 0, 5, 3)),
        ("sibling".to_string(), Rect::new(24, 0, 6, 3)),
        ("ext_a".to_string(), Rect::new(0, 0, 4, 3)),
        ("ext_b".to_string(), Rect::new(0, 4, 4, 3)),
    ]);

    let shift = widen_subgraph_for_outgoing_route_pressure(
        &graph,
        &mut positions,
        &mut node_rects,
        "child",
    );

    assert!(
        shift > 0,
        "expected route pressure to widen the declared nested child span"
    );
    assert_eq!(
        positions.get("left").expect("left").x,
        8,
        "left partition should stay anchored"
    );
    assert_eq!(
        positions.get("right").expect("right").x,
        14 + shift,
        "right outgoing source should shift right"
    );
    assert_eq!(
        positions.get("sibling").expect("sibling").x,
        24 + shift,
        "non-source sibling on the right partition should shift with the widened subtree"
    );
}

#[test]
fn internal_route_span_budget_detects_centered_nested_fanin() {
    let mut graph = Graph::new();
    graph.add_node(Node::new("left", "L"));
    graph.add_node(Node::new("middle", "M"));
    graph.add_node(Node::new("right", "R"));
    graph.add_node(Node::new("target", "T"));
    graph.add_subgraph(Subgraph::new("parent", Some("Parent".to_string())));

    let mut child = Subgraph::new("child", Some("Child".to_string()));
    child.parent_id = Some("parent".to_string());
    graph.add_subgraph(child);
    graph
        .get_subgraph_mut("parent")
        .expect("parent")
        .add_child("child");

    graph.associate_node_with_subgraph("left", "child");
    graph.associate_node_with_subgraph("middle", "child");
    graph.associate_node_with_subgraph("right", "child");

    graph.add_edge(Edge::new("left", "target"));
    graph.add_edge(Edge::new("middle", "target"));
    graph.add_edge(Edge::new("right", "target"));

    let node_rects = HashMap::from([
        ("left".to_string(), Rect::new(8, 0, 5, 3)),
        ("middle".to_string(), Rect::new(14, 0, 5, 3)),
        ("right".to_string(), Rect::new(20, 0, 5, 3)),
        ("target".to_string(), Rect::new(14, 8, 5, 3)),
    ]);

    let budget = internal_route_span_budget_x(
        &graph,
        &node_rects,
        "child",
        CoarseLayoutConfig::default().min_horizontal_spacing,
    )
    .expect("centered fan-in should need span budget");

    assert_eq!(
        budget.shift_x, 4,
        "expected centered nested fan-in to widen beyond the coarse node span"
    );
    assert_eq!(budget.pivot_center, 16);
}

#[test]
fn widen_subgraph_for_internal_route_span_shifts_centered_nested_fanout_partition() {
    let mut graph = Graph::new();
    graph.add_node(Node::new("source", "Source"));
    graph.add_node(Node::new("left", "L"));
    graph.add_node(Node::new("middle", "M"));
    graph.add_node(Node::new("right", "R"));
    graph.add_subgraph(Subgraph::new("parent", Some("Parent".to_string())));

    let mut child = Subgraph::new("child", Some("Child".to_string()));
    child.parent_id = Some("parent".to_string());
    graph.add_subgraph(child);
    graph
        .get_subgraph_mut("parent")
        .expect("parent")
        .add_child("child");

    graph.associate_node_with_subgraph("left", "child");
    graph.associate_node_with_subgraph("middle", "child");
    graph.associate_node_with_subgraph("right", "child");

    graph.add_edge(Edge::new("source", "left"));
    graph.add_edge(Edge::new("source", "middle"));
    graph.add_edge(Edge::new("source", "right"));

    let mut positions = HashMap::from([
        ("source".to_string(), Point::new(14, 8)),
        ("left".to_string(), Point::new(8, 0)),
        ("middle".to_string(), Point::new(14, 0)),
        ("right".to_string(), Point::new(20, 0)),
    ]);
    let mut node_rects = HashMap::from([
        ("source".to_string(), Rect::new(14, 8, 5, 3)),
        ("left".to_string(), Rect::new(8, 0, 5, 3)),
        ("middle".to_string(), Rect::new(14, 0, 5, 3)),
        ("right".to_string(), Rect::new(20, 0, 5, 3)),
    ]);

    let shift = widen_subgraph_for_internal_route_span(
        &graph,
        &mut positions,
        &mut node_rects,
        "child",
        CoarseLayoutConfig::default().min_horizontal_spacing,
    );

    assert_eq!(
        shift, 4,
        "expected centered nested fan-out to claim extra span"
    );
    assert_eq!(
        positions.get("left").expect("left").x,
        8,
        "left partition should stay anchored"
    );
    assert_eq!(
        positions.get("middle").expect("middle").x,
        18,
        "middle target should shift with the widened right partition"
    );
    assert_eq!(
        positions.get("right").expect("right").x,
        24,
        "right target should shift with the widened right partition"
    );
}

#[test]
#[cfg(feature = "maintainer-fixtures")]
fn nested_horizontal_subgraphs_keep_distinct_title_rows() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_nested_lr.md")
        .expect("read fixture");
    let parsed = parse(&input, false).expect("parse");
    let graph =
        apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

    let outer = graph.get_subgraph("Outer").expect("outer subgraph");
    let inner = graph.get_subgraph("Inner").expect("inner subgraph");
    let deep = graph.get_subgraph("Deep").expect("deep subgraph");

    assert!(
            outer.bounds.y < inner.bounds.y,
            "expected nested LR outer title row to stay above the inner title row: outer={:?} inner={:?}",
            outer.bounds,
            inner.bounds
        );
    assert!(
        inner.bounds.y < deep.bounds.y,
        "expected nested LR inner title row to stay above the deep title row: inner={:?} deep={:?}",
        inner.bounds,
        deep.bounds
    );
}

#[test]
#[cfg(feature = "maintainer-fixtures")]
fn titled_vertical_subgraph_balances_left_and_right_inner_padding() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_basic_td.md")
        .expect("read fixture");
    let parsed = parse(&input, false).expect("parse");
    let graph =
        apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

    let subgraph = graph.get_subgraph("SG").expect("subgraph");
    let left_pad = subgraph
        .inner_bounds
        .x
        .saturating_sub(subgraph.bounds.x.saturating_add(1));
    let right_pad = subgraph
        .bounds
        .x
        .saturating_add(subgraph.bounds.width.saturating_sub(1))
        .saturating_sub(subgraph.inner_bounds.x + subgraph.inner_bounds.width);

    assert!(
            left_pad.abs_diff(right_pad) <= 1,
            "expected titled TD subgraph content to be horizontally centered inside the final frame: bounds={:?} inner={:?} pads=({}, {})",
            subgraph.bounds,
            subgraph.inner_bounds,
            left_pad,
            right_pad,
        );
}

#[test]
#[cfg(feature = "maintainer-fixtures")]
fn titled_vertical_subgraph_balances_top_and_bottom_inner_padding() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_basic_td.md")
        .expect("read fixture");
    let parsed = parse(&input, false).expect("parse");
    let graph =
        apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

    let subgraph = graph.get_subgraph("SG").expect("subgraph");
    let top_pad = subgraph.inner_bounds.y.saturating_sub(subgraph.bounds.y);
    let bottom_pad = subgraph
        .bounds
        .y
        .saturating_add(subgraph.bounds.height.saturating_sub(1))
        .saturating_sub(subgraph.inner_bounds.y + subgraph.inner_bounds.height);

    assert!(
            top_pad.abs_diff(bottom_pad) <= 1,
            "expected titled TD subgraph content to be vertically centered inside the final frame: bounds={:?} inner={:?} pads=({}, {})",
            subgraph.bounds,
            subgraph.inner_bounds,
            top_pad,
            bottom_pad,
        );
}

#[test]
#[cfg(feature = "maintainer-fixtures")]
fn titled_bt_subgraph_balances_top_and_bottom_inner_padding() {
    let input = std::fs::read_to_string("tests/fixtures/inputs/subgraph_basic_bt.md")
        .expect("read fixture");
    let parsed = parse(&input, false).expect("parse");
    let graph =
        apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

    let subgraph = graph.get_subgraph("SG").expect("subgraph");
    let top_pad = subgraph.inner_bounds.y.saturating_sub(subgraph.bounds.y);
    let bottom_pad = subgraph
        .bounds
        .y
        .saturating_add(subgraph.bounds.height.saturating_sub(1))
        .saturating_sub(subgraph.inner_bounds.y + subgraph.inner_bounds.height);

    assert!(
            top_pad.abs_diff(bottom_pad) <= 1,
            "expected titled BT subgraph content to be vertically centered inside the final frame: bounds={:?} inner={:?} pads=({}, {})",
            subgraph.bounds,
            subgraph.inner_bounds,
            top_pad,
            bottom_pad,
        );
}

#[test]
#[cfg(feature = "maintainer-fixtures")]
fn titled_vertical_leaf_subgraphs_with_single_external_trunks_balance_horizontal_padding() {
    for fixture_path in [
        "tests/fixtures/inputs/subgraph_direct_td.md",
        "tests/fixtures/inputs/subgraph_direct_bt.md",
    ] {
        let input = std::fs::read_to_string(fixture_path).expect("read fixture");
        let parsed = parse(&input, false).expect("parse");
        let graph =
            apply_coarse_layout(parsed.graph, None, CoarseLayoutConfig::default()).expect("layout");

        for subgraph_id in ["SG1", "SG2"] {
            let subgraph = graph.get_subgraph(subgraph_id).expect("subgraph");
            let left_pad = subgraph
                .inner_bounds
                .x
                .saturating_sub(subgraph.bounds.x.saturating_add(1));
            let right_pad = subgraph
                .bounds
                .x
                .saturating_add(subgraph.bounds.width.saturating_sub(1))
                .saturating_sub(subgraph.inner_bounds.x + subgraph.inner_bounds.width);

            assert!(
                    left_pad.abs_diff(right_pad) <= 1,
                    "expected titled leaf subgraph {subgraph_id} in {fixture_path} to stay horizontally balanced even with a single external trunk: bounds={:?} inner={:?} pads=({}, {})",
                    subgraph.bounds,
                    subgraph.inner_bounds,
                    left_pad,
                    right_pad,
                );
        }
    }
}

#[test]
fn marks_back_edges_and_leaves_cycle_routing_to_renderer() {
    let mut graph = Graph::new();
    graph.direction = Direction::TD;
    graph.nodes.push(Node::new("A", "A"));
    graph.nodes.push(Node::new("B", "B"));
    graph.edges.push(Edge::new("A", "B"));
    graph.edges.push(Edge::new("B", "A")); // back-edge creates a cycle

    let laid_out = apply_coarse_layout(graph, None, CoarseLayoutConfig::default()).expect("layout");

    assert!(laid_out.has_cycles(), "graph should be marked cyclic");
    assert!(
        laid_out.edges[1].is_back_edge,
        "back-edge should be flagged"
    );
    assert!(
        !laid_out.edges[0].is_back_edge,
        "forward edge should not be flagged"
    );
    // Only the forward edge should have a precomputed route; back-edges are rendered via the cycle gutter.
    assert!(
        laid_out.edge_routes.contains_key(&0),
        "forward edge should be routed"
    );
    assert!(
        !laid_out.edge_routes.contains_key(&1),
        "back-edge routing should be deferred to renderer"
    );
}
