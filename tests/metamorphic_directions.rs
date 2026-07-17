use std::collections::BTreeMap;
use std::fs;

use termiflow::{parse, render, BaseStyle, RenderOptions};

fn direction_family(path: &std::path::Path) -> Option<(String, String)> {
    let stem = path.file_stem()?.to_str()?;
    for direction in ["td", "lr", "bt", "rl"] {
        if let Some(family) = stem.strip_suffix(&format!("_{direction}")) {
            return Some((family.to_string(), direction.to_string()));
        }
    }
    None
}

#[test]
fn direction_families_preserve_parser_semantics_and_renderability() {
    let mut families: BTreeMap<String, BTreeMap<String, std::path::PathBuf>> = BTreeMap::new();
    for entry in fs::read_dir("tests/fixtures/inputs").expect("read fixture directory") {
        let path = entry.expect("read fixture entry").path();
        if let Some((family, direction)) = direction_family(&path) {
            families.entry(family).or_default().insert(direction, path);
        }
    }

    for (family, directions) in families {
        if directions.len() != 4 {
            assert!(matches!(
                family.as_str(),
                "warn_classDef" | "warn_malformed"
            ));
            continue;
        }
        let mut baseline = None;
        for path in directions.values() {
            let input = fs::read_to_string(path).expect("read direction fixture");
            let graph = parse(&input, false).expect("parse direction fixture").graph;
            let semantics = (graph.nodes.len(), graph.edges.len(), graph.subgraphs.len());
            assert_eq!(
                baseline.get_or_insert(semantics),
                &semantics,
                "semantic drift in {family}"
            );
            for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
                for optimized in [false, true] {
                    let output = render(
                        &input,
                        RenderOptions::new()
                            .with_style(style)
                            .with_optimize_render(optimized),
                    )
                    .expect("render direction fixture");
                    assert!(
                        !output.trim().is_empty(),
                        "empty {family} {style:?} optimized={optimized}"
                    );
                }
            }
        }
    }
}

#[test]
fn generated_adversarial_cases_render_in_every_direction_and_primary_style() {
    for direction in ["TD", "LR", "BT", "RL"] {
        for body in [
            "A[Source] --> B[Left]\nA --> C[Middle]\nA --> D[Right]",
            "A[Left] --> D[Target]\nB[Middle] --> D\nC[Right] --> D",
            "subgraph Outer\n  subgraph Inner\n    A[Alpha] --> B[Beta]\n  end\nend\nB --> C[Outside]",
            "A[An exceptionally long label that should remain legible] --> B[Emoji 😀 node]",
        ] {
            let input = format!("graph {direction}\n{body}");
            let parsed = parse(&input, false).expect("parse generated adversarial case");
            assert!(!parsed.graph.nodes.is_empty());
            for style in [BaseStyle::Ascii, BaseStyle::Unicode] {
                let output = render(&input, RenderOptions::new().with_style(style))
                    .expect("render generated adversarial case");
                assert!(!output.trim().is_empty());
            }
        }
    }
}
