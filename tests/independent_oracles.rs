//! Oracles that derive expectations from parser output and raw text only.
//!
//! These deliberately do not consume `SemanticFrame`, provenance, or critic
//! findings. Geometry checks consume the normalized trace as a separate input.

use std::collections::HashSet;
use std::fs;
use termiflow::{
    layout_and_render_with_feedback, measure, parse, BaseStyle, CompositeStyle, Config,
    GeometryTrace, RectTrace, RenderOptions,
};

fn secondary_center_for(x: usize, y: usize, width: usize, height: usize, direction: &str) -> usize {
    match direction {
        "TD" | "BT" => x + width / 2,
        "LR" | "RL" => y + height / 2,
        other => panic!("unsupported junction direction {other}"),
    }
}

fn raw_frame_errors(input: &str, frame: &str) -> Vec<String> {
    let parsed = parse(input, false).expect("parse oracle input");
    let mut errors = Vec::new();

    if frame.trim().is_empty() {
        errors.push("rendered frame is empty".to_string());
    }
    for node in &parsed.graph.nodes {
        if !node.label.is_empty() && !frame.contains(&node.label) {
            errors.push(format!(
                "node label {:?} is absent from raw frame",
                node.label
            ));
        }
    }

    errors.extend(raw_topology_errors(frame, parsed.graph.edges.len()));
    errors
}

fn shared_fan_in_raw_frame_errors(input: &str, frame: &str) -> Vec<String> {
    let parsed = parse(input, false).expect("parse fan-in oracle input");
    let mut errors = Vec::new();
    if frame.trim().is_empty() {
        errors.push("rendered frame is empty".to_string());
    }
    for node in &parsed.graph.nodes {
        if !node.label.is_empty() && !frame.contains(&node.label) {
            errors.push(format!(
                "node label {:?} is absent from raw frame",
                node.label
            ));
        }
    }

    let expected_arrowheads = parsed
        .graph
        .nodes
        .iter()
        .filter(|node| {
            parsed
                .graph
                .edges
                .iter()
                .any(|edge| !edge.is_back_edge && edge.to == node.id)
        })
        .count();
    errors.extend(raw_topology_errors(frame, expected_arrowheads));
    errors
}

fn junction_quad_raw_frame_errors(input: &str, frame: &str) -> Vec<String> {
    let parsed = parse(input, false).expect("parse junction quad oracle input");
    let (_, _, _, expected_arrows) = dual_junction_shape(&parsed.graph);
    let mut errors = Vec::new();
    for node in &parsed.graph.nodes {
        if !node.label.is_empty() && !frame.contains(&node.label) {
            errors.push(format!(
                "node label {:?} is absent from raw frame",
                node.label
            ));
        }
    }
    // Incoming semantic edges intentionally share one merge arrowhead. The
    // independent geometry trace still requires every semantic edge record to
    // survive alongside the raw-frame shaft/arrow checks.
    errors.extend(raw_topology_errors(frame, expected_arrows));
    errors
}

fn dual_junction_shape(graph: &termiflow::Graph) -> (String, Vec<String>, usize, usize) {
    let candidates: Vec<_> = graph
        .nodes
        .iter()
        .filter_map(|node| {
            let incoming: HashSet<&str> = graph
                .edges
                .iter()
                .filter(|edge| edge.to == node.id && edge.from != node.id)
                .map(|edge| edge.from.as_str())
                .collect();
            let outgoing: HashSet<&str> = graph
                .edges
                .iter()
                .filter(|edge| edge.from == node.id && edge.to != node.id)
                .map(|edge| edge.to.as_str())
                .collect();
            (incoming.len() >= 2 && outgoing.len() >= 2).then(|| {
                let mut targets: Vec<String> = outgoing.into_iter().map(str::to_owned).collect();
                targets.sort_unstable();
                (node.id.clone(), targets, incoming.len())
            })
        })
        .collect();
    assert_eq!(
        candidates.len(),
        1,
        "expected exactly one dual-junction anchor"
    );
    let (anchor, targets, incoming_count) = candidates
        .into_iter()
        .next()
        .expect("dual-junction candidate");
    let semantic_edges = graph.edges.len();
    let expected_arrows = semantic_edges.saturating_sub(incoming_count.saturating_sub(1));
    (anchor, targets, semantic_edges, expected_arrows)
}

fn pure_fan_in_shape(graph: &termiflow::Graph) -> (String, Vec<String>, usize) {
    let candidates: Vec<_> = graph
        .nodes
        .iter()
        .filter_map(|node| {
            let incoming: HashSet<&str> = graph
                .edges
                .iter()
                .filter(|edge| !edge.is_back_edge && edge.to == node.id && edge.from != node.id)
                .map(|edge| edge.from.as_str())
                .collect();
            let outgoing = graph
                .edges
                .iter()
                .any(|edge| !edge.is_back_edge && edge.from == node.id && edge.to != node.id);
            if incoming.len() < 2 || outgoing {
                return None;
            }

            let mut sources: Vec<String> = incoming.into_iter().map(str::to_owned).collect();
            sources.sort_unstable();
            Some((node.id.clone(), sources))
        })
        .collect();
    assert_eq!(
        candidates.len(),
        1,
        "expected exactly one pure fan-in target"
    );
    let (anchor, sources) = candidates
        .into_iter()
        .next()
        .expect("pure fan-in candidate");
    (anchor, sources, graph.edges.len())
}

fn raw_topology_errors(frame: &str, expected_edges: usize) -> Vec<String> {
    let mut errors = Vec::new();
    let cells: Vec<Vec<char>> = frame.lines().map(|line| line.chars().collect()).collect();
    let arrows = cells
        .iter()
        .flat_map(|row| row.iter())
        .filter(|ch| {
            matches!(
                ch,
                '>' | '<' | '^' | 'v' | '→' | '←' | '↑' | '↓' | '▶' | '◀' | '▲' | '▼'
            )
        })
        .count();
    if arrows < expected_edges {
        errors.push(format!(
            "raw frame has {arrows} arrowheads for {expected_edges} directed edges",
        ));
    }

    for (y, row) in cells.iter().enumerate() {
        for (x, ch) in row.iter().enumerate() {
            let predecessor = match ch {
                'v' | '↓' | '▼' => y
                    .checked_sub(1)
                    .and_then(|py| cells.get(py).and_then(|r| r.get(x))),
                '^' | '↑' | '▲' => cells.get(y + 1).and_then(|r| r.get(x)),
                '>' | '→' | '▶' => x.checked_sub(1).and_then(|px| row.get(px)),
                '<' | '←' | '◀' => row.get(x + 1),
                _ => continue,
            };
            if !predecessor.is_some_and(|glyph| is_route_glyph(*glyph)) {
                errors.push(format!(
                    "arrowhead at ({x},{y}) lacks a visible raw-frame shaft"
                ));
            }
        }
    }
    errors
}

fn is_route_glyph(ch: char) -> bool {
    matches!(
        ch,
        '-' | '|'
            | '+'
            | '='
            | ':'
            | '.'
            | '─'
            | '│'
            | '┌'
            | '┐'
            | '└'
            | '┘'
            | '├'
            | '┤'
            | '┬'
            | '┴'
            | '┼'
            | '═'
            | '║'
            | '╔'
            | '╗'
            | '╚'
            | '╝'
            | '╠'
            | '╣'
            | '╦'
            | '╩'
            | '╬'
            | '━'
            | '┃'
            | '╋'
    )
}

fn node_geometry_errors(trace: &GeometryTrace) -> Vec<String> {
    let mut errors = Vec::new();
    for node in &trace.nodes {
        if node.width == 0 || node.height == 0 {
            errors.push(format!("node {} has an empty rectangle", node.id));
        }
    }
    for (index, left) in trace.nodes.iter().enumerate() {
        for right in &trace.nodes[index + 1..] {
            if rectangles_overlap(
                &RectTrace {
                    x: left.x,
                    y: left.y,
                    width: left.width,
                    height: left.height,
                },
                &RectTrace {
                    x: right.x,
                    y: right.y,
                    width: right.width,
                    height: right.height,
                },
            ) {
                errors.push(format!(
                    "node rectangles {} and {} overlap",
                    left.id, right.id
                ));
            }
        }
    }
    errors
}

fn geometry_errors(trace: &GeometryTrace) -> Vec<String> {
    let mut errors = node_geometry_errors(trace);
    for edge in &trace.edges {
        if !trace.nodes.iter().any(|node| node.id == edge.from)
            || !trace.nodes.iter().any(|node| node.id == edge.to)
        {
            errors.push(format!("edge {} has an unknown endpoint", edge.owner_id));
        }
        if edge.segments.is_empty() {
            errors.push(format!("edge {} has no geometry segments", edge.owner_id));
        }
    }
    errors
}

fn rectangles_overlap(left: &RectTrace, right: &RectTrace) -> bool {
    left.x < right.x.saturating_add(right.width)
        && right.x < left.x.saturating_add(left.width)
        && left.y < right.y.saturating_add(right.height)
        && right.y < left.y.saturating_add(left.height)
}

#[test]
fn raw_frame_oracle_covers_ascii_unicode_and_all_directions() {
    for direction in ["TD", "LR", "BT", "RL"] {
        let input = format!("graph {direction}\nA[Alpha] --> B[Beta]");
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            let output = termiflow::render(&input, RenderOptions::new().with_style(style))
                .expect("render oracle fixture");
            assert!(
                raw_frame_errors(&input, &output).is_empty(),
                "{direction} {style:?}:\n{output}"
            );
        }
    }
}

#[test]
fn raw_frame_oracle_rejects_label_and_shaft_mutations() {
    let input = "graph TD\nA[Alpha] --> B[Beta]";
    let output = termiflow::render(input, RenderOptions::new().with_style(BaseStyle::Ascii))
        .expect("render mutation fixture");
    assert!(raw_frame_errors(input, &output.replace("Alpha", "     "))
        .iter()
        .any(|error| error.contains("Alpha")));

    let shaftless = output.replace(['|', '-', '+'], " ");
    assert!(raw_frame_errors(input, &shaftless)
        .iter()
        .any(|error| error.contains("lacks a visible")));
}

#[test]
fn geometry_oracle_rejects_an_overlapping_trace_mutation() {
    let input = "graph TD\nA[Alpha] --> B[Beta]";
    let mut graph = parse(input, false).expect("parse geometry fixture").graph;
    let config = Config::default();
    measure::measure_graph(&mut graph, &config);
    let (graph, _) =
        layout_and_render_with_feedback(graph, config).expect("layout geometry fixture");
    let trace = GeometryTrace::from_graph(&graph);
    assert!(geometry_errors(&trace).is_empty());

    let mut corrupted = trace.clone();
    corrupted.nodes[1].x = corrupted.nodes[0].x;
    corrupted.nodes[1].y = corrupted.nodes[0].y;
    assert!(geometry_errors(&corrupted)
        .iter()
        .any(|error| error.contains("overlap")));
}

fn geometry_trace_for(input: &str) -> GeometryTrace {
    let mut graph = parse(input, false).expect("parse geometry fixture").graph;
    let mut config = Config::default();
    config.spacing = config.spacing.for_direction(graph.direction);
    measure::measure_graph(&mut graph, &config);
    let (graph, _) =
        layout_and_render_with_feedback(graph, config).expect("layout geometry fixture");
    GeometryTrace::from_graph(&graph)
}

#[test]
fn full_primary_corpus_raw_topology_reports_are_deterministic() {
    let mut inputs: Vec<_> = fs::read_dir("tests/fixtures/inputs")
        .expect("read fixture directory")
        .map(|entry| entry.expect("read fixture entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .filter(|path| {
            !path
                .file_stem()
                .is_some_and(|stem| stem.to_string_lossy().starts_with("error_"))
        })
        .collect();
    inputs.sort();

    for path in inputs {
        let input = fs::read_to_string(&path).expect("read fixture");
        let expected_edges = parse(&input, false)
            .expect("parse fixture")
            .graph
            .edges
            .len();
        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let first = termiflow::render(
                    &input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("render fixture");
                let second = termiflow::render(
                    &input,
                    RenderOptions::new()
                        .with_style(style)
                        .with_optimize_render(optimized),
                )
                .expect("repeat render fixture");
                assert_eq!(
                    raw_topology_errors(&first, expected_edges),
                    raw_topology_errors(&second, expected_edges),
                    "non-deterministic raw-topology report for {} {style:?} optimized={optimized}",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn full_fixture_corpus_geometry_traces_are_deterministic() {
    let mut inputs: Vec<_> = fs::read_dir("tests/fixtures/inputs")
        .expect("read fixture directory")
        .map(|entry| entry.expect("read fixture entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .filter(|path| {
            !path
                .file_stem()
                .is_some_and(|stem| stem.to_string_lossy().starts_with("error_"))
        })
        .collect();
    inputs.sort();

    for path in inputs {
        let input = fs::read_to_string(&path).expect("read fixture");
        let first = geometry_trace_for(&input);
        let second = geometry_trace_for(&input);
        assert_eq!(
            first,
            second,
            "non-deterministic geometry trace for {}",
            path.display()
        );
    }
}

#[test]
fn junction_quad_independent_oracle_covers_the_full_review_matrix() {
    for prefix in ["junction_quad", "junction_quad_holdout"] {
        for direction in ["TD", "BT", "LR", "RL"] {
            let input = fs::read_to_string(if prefix == "junction_quad" {
                format!("tests/fixtures/inputs/{prefix}_{direction}.md")
            } else {
                format!("tests/fixtures/holdouts/inputs/{prefix}_{direction}.md")
            })
            .expect("read junction quad fixture");
            let (anchor_id, target_ids, semantic_edges, _) =
                dual_junction_shape(&parse(&input, false).expect("parse junction quad").graph);

            for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
                for optimized in [false, true] {
                    let render_case = || {
                        let mut graph = parse(&input, false)
                            .expect("parse junction quad fixture")
                            .graph;
                        let mut config = Config::default();
                        config.optimize_render = optimized;
                        config.composite_style = CompositeStyle::from_base(style);
                        config.spacing = config.spacing.for_direction(graph.direction);
                        measure::measure_graph(&mut graph, &config);
                        let (graph, outcome) = layout_and_render_with_feedback(graph, config)
                            .expect("render junction quad fixture");
                        (outcome.output, GeometryTrace::from_graph(&graph))
                    };

                    let (first_output, first_trace) = render_case();
                    let (second_output, second_trace) = render_case();
                    assert_eq!(
                        first_output, second_output,
                        "non-deterministic raw frame for {prefix} {direction} {style:?} optimized={optimized}"
                    );
                    assert_eq!(
                        first_trace, second_trace,
                        "non-deterministic geometry for {prefix} {direction} {style:?} optimized={optimized}"
                    );

                    let raw_errors = junction_quad_raw_frame_errors(&input, &first_output);
                    assert!(
                        raw_errors.is_empty(),
                        "raw-frame oracle failed for {prefix} {direction} {style:?} optimized={optimized}: {raw_errors:?}\n{first_output}"
                    );
                    let node_errors = node_geometry_errors(&first_trace);
                    assert!(
                        node_errors.is_empty(),
                        "geometry oracle failed for {prefix} {direction} {style:?} optimized={optimized}: {node_errors:?}"
                    );
                    assert_eq!(
                        first_trace.edges.len(),
                        semantic_edges,
                        "junction quad lost a semantic edge trace for {prefix} {direction} {style:?} optimized={optimized}"
                    );

                    let center = |id: &str| {
                        let node = first_trace
                            .nodes
                            .iter()
                            .find(|node| node.id == id)
                            .unwrap_or_else(|| panic!("missing node {id}"));
                        secondary_center_for(node.x, node.y, node.width, node.height, direction)
                    };
                    let target_midpoint =
                        (center(&target_ids[0]) + center(target_ids.last().expect("target"))) / 2;
                    assert!(
                        center(&anchor_id).abs_diff(target_midpoint) <= 1,
                        "dual-junction midpoint drift for {prefix} {direction} {style:?} optimized={optimized}: anchor={} target_midpoint={target_midpoint}",
                        center(&anchor_id),
                    );
                }
            }
        }
    }
}

#[test]
fn pure_fan_in_independent_oracle_covers_the_full_scale_matrix() {
    for direction in ["TD", "BT", "LR", "RL"] {
        let input = fs::read_to_string(format!(
            "tests/fixtures/inputs/scale_dense_{}.md",
            direction.to_lowercase()
        ))
        .expect("read scale dense fixture");
        let parsed = parse(&input, false).expect("parse scale dense fixture");
        let (anchor_id, source_ids, semantic_edges) = pure_fan_in_shape(&parsed.graph);

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            for optimized in [false, true] {
                let render_case = || {
                    let mut graph = parse(&input, false)
                        .expect("parse scale dense fixture")
                        .graph;
                    let mut config = Config::default();
                    config.optimize_render = optimized;
                    config.composite_style = CompositeStyle::from_base(style);
                    config.spacing = config.spacing.for_direction(graph.direction);
                    measure::measure_graph(&mut graph, &config);
                    let (graph, outcome) = layout_and_render_with_feedback(graph, config)
                        .expect("render scale dense fixture");
                    (
                        outcome.output.clone(),
                        GeometryTrace::from_graph(&graph),
                        outcome,
                    )
                };

                let (first_output, first_trace, first_outcome) = render_case();
                let (second_output, second_trace, _) = render_case();
                assert_eq!(
                    first_output, second_output,
                    "non-deterministic raw frame for scale_dense {direction} {style:?} optimized={optimized}"
                );
                assert_eq!(
                    first_trace, second_trace,
                    "non-deterministic geometry for scale_dense {direction} {style:?} optimized={optimized}"
                );

                assert!(
                    shared_fan_in_raw_frame_errors(&input, &first_output).is_empty(),
                    "raw-frame oracle failed for scale_dense {direction} {style:?} optimized={optimized}: {:?}\n{first_output}",
                    shared_fan_in_raw_frame_errors(&input, &first_output)
                );
                assert!(
                    node_geometry_errors(&first_trace).is_empty(),
                    "node geometry oracle failed for scale_dense {direction} {style:?} optimized={optimized}: {:?}",
                    node_geometry_errors(&first_trace)
                );
                assert_eq!(
                    first_trace.edges.len(),
                    semantic_edges,
                    "scale dense lost a semantic edge trace for {direction} {style:?} optimized={optimized}"
                );
                assert!(
                    first_outcome
                        .critic_report
                        .findings
                        .iter()
                        .all(|finding| finding.code != termiflow::FindingCode::RouteSymmetryImbalance),
                    "pure fan-in critic finding remained for scale_dense {direction} {style:?} optimized={optimized}: {:?}",
                    first_outcome.critic_report.findings
                );

                let center = |id: &str| {
                    let node = first_trace
                        .nodes
                        .iter()
                        .find(|node| node.id == id)
                        .unwrap_or_else(|| panic!("missing node {id}"));
                    secondary_center_for(node.x, node.y, node.width, node.height, direction)
                };
                let source_centers: Vec<usize> = source_ids.iter().map(|id| center(id)).collect();
                let source_midpoint =
                    (source_centers.iter().min().copied().expect("source center")
                        + source_centers.iter().max().copied().expect("source center"))
                        / 2;
                assert!(
                    center(&anchor_id).abs_diff(source_midpoint) <= 1,
                    "pure fan-in midpoint drift for scale_dense {direction} {style:?} optimized={optimized}: anchor={} midpoint={source_midpoint}",
                    center(&anchor_id)
                );
            }
        }
    }
}

#[test]
fn subgraph_fan_in_remains_owned_by_boundary_layout() {
    for direction in ["TD", "BT", "LR", "RL"] {
        let input = fs::read_to_string(format!(
            "tests/fixtures/inputs/subgraph_fanin_{}.md",
            direction.to_lowercase()
        ))
        .expect("read subgraph fan-in fixture");
        let parsed = parse(&input, false).expect("parse subgraph fan-in fixture");
        let (anchor_id, source_ids, _) = pure_fan_in_shape(&parsed.graph);
        assert!(
            source_ids.iter().any(|source_id| {
                parsed
                    .graph
                    .edge_crosses_subgraph_boundary(source_id, &anchor_id)
            }),
            "fixture must cross a declared boundary for {direction}"
        );

        for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
            let outcome =
                termiflow::render_with_feedback(&input, RenderOptions::new().with_style(style))
                    .expect("render subgraph fan-in fixture");
            assert!(
                outcome
                    .critic_report
                    .findings
                    .iter()
                    .all(|finding| finding.code != termiflow::FindingCode::RouteSymmetryImbalance),
                "boundary-owned fan-in should not be claimed by the pure normalizer for {direction} {style:?}: {:?}",
                outcome.critic_report.findings
            );
        }
    }
}
