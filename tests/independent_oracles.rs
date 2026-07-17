//! Oracles that derive expectations from parser output and raw text only.
//!
//! These deliberately do not consume `SemanticFrame`, provenance, or critic
//! findings. Geometry checks consume the normalized trace as a separate input.

use std::fs;
use termiflow::{
    layout_and_render_with_feedback, measure, parse, BaseStyle, Config, GeometryTrace, RectTrace,
    RenderOptions,
};

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

fn geometry_errors(trace: &GeometryTrace) -> Vec<String> {
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
