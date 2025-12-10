//! Golden tests - compare actual output against expected fixtures
//!
//! These tests verify that termiflow produces consistent, expected output
//! for a set of reference inputs. Changes to rendering will cause these
//! tests to fail, requiring explicit regeneration of expected files.

use std::process::Command;

/// Run termiflow with given args and return (stdout, stderr)
fn run_termiflow(args: &[&str]) -> (String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_termiflow"))
        .args(args)
        .output()
        .expect("failed to execute termiflow");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (stdout, stderr)
}

// ============================================================================
// Unicode Style Golden Tests
// ============================================================================

#[test]
fn golden_simple_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/simple.md",
    ]);
    let expected = include_str!("fixtures/expected/simple.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for simple.md");
}

#[test]
fn golden_chain_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/chain.md",
    ]);
    let expected = include_str!("fixtures/expected/chain.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for chain.md");
}

#[test]
fn golden_database_nodes_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/database_nodes.md",
    ]);
    let expected = include_str!("fixtures/expected/database_nodes.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for database_nodes.md");
}

#[test]
fn golden_forward_ref_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/forward_ref.md",
    ]);
    let expected = include_str!("fixtures/expected/forward_ref.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for forward_ref.md");
}

#[test]
fn golden_with_config_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/with_config.md",
    ]);
    let expected = include_str!("fixtures/expected/with_config.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for with_config.md");
}

#[test]
fn golden_unsupported_unicode() {
    // Note: This fixture now renders correctly since subgraphs are enabled by default
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/unsupported.md",
    ]);

    let expected_stdout = include_str!("fixtures/expected/unsupported.unicode.txt");
    assert_eq!(
        stdout, expected_stdout,
        "Output mismatch for unsupported.md"
    );
}

// ============================================================================
// ASCII Style Golden Tests
// ============================================================================

#[test]
fn golden_simple_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/simple.md",
    ]);
    let expected = include_str!("fixtures/expected/simple.ascii.txt");
    assert_eq!(stdout, expected, "Output mismatch for simple.md (ASCII)");
}

#[test]
fn golden_chain_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/chain.md",
    ]);
    let expected = include_str!("fixtures/expected/chain.ascii.txt");
    assert_eq!(stdout, expected, "Output mismatch for chain.md (ASCII)");
}

#[test]
fn golden_database_nodes_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/database_nodes.md",
    ]);
    let expected = include_str!("fixtures/expected/database_nodes.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for database_nodes.md (ASCII)"
    );
}

#[test]
fn golden_forward_ref_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/forward_ref.md",
    ]);
    let expected = include_str!("fixtures/expected/forward_ref.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for forward_ref.md (ASCII)"
    );
}

#[test]
fn golden_with_config_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/with_config.md",
    ]);
    let expected = include_str!("fixtures/expected/with_config.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for with_config.md (ASCII)"
    );
}

#[test]
fn golden_unsupported_ascii() {
    // Note: This fixture now renders correctly since subgraphs are enabled by default
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/unsupported.md",
    ]);
    let expected = include_str!("fixtures/expected/unsupported.ascii.txt");
    assert_eq!(stdout, expected, "Output mismatch for unsupported.md (ASCII)");
}

// ============================================================================
// Edge Labels Golden Tests
// ============================================================================

#[test]
fn golden_labeled_edges_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/labeled_edges.md",
    ]);
    let expected = include_str!("fixtures/expected/labeled_edges.unicode.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for labeled_edges.md (Unicode)"
    );
}

#[test]
fn golden_labeled_edges_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/labeled_edges.md",
    ]);
    let expected = include_str!("fixtures/expected/labeled_edges.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for labeled_edges.md (ASCII)"
    );
}

// ============================================================================
// Node Shapes Golden Tests
// ============================================================================

#[test]
fn golden_shapes_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/shapes.md",
    ]);
    let expected = include_str!("fixtures/expected/shapes.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for shapes.md (Unicode)");
}

#[test]
fn golden_shapes_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/shapes.md",
    ]);
    let expected = include_str!("fixtures/expected/shapes.ascii.txt");
    assert_eq!(stdout, expected, "Output mismatch for shapes.md (ASCII)");
}

// ============================================================================
// Subgraph Golden Tests (subgraphs enabled by default)
// ============================================================================

#[test]
fn golden_subgraph_basic_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/subgraph_basic.md",
    ]);
    let expected = include_str!("fixtures/expected/subgraph_basic.unicode.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for subgraph_basic.md (Unicode)"
    );
}

#[test]
fn golden_subgraph_basic_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/subgraph_basic.md",
    ]);
    let expected = include_str!("fixtures/expected/subgraph_basic.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for subgraph_basic.md (ASCII)"
    );
}

#[test]
fn golden_subgraph_cross_edges_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/subgraph_cross_edges.md",
    ]);
    let expected = include_str!("fixtures/expected/subgraph_cross_edges.unicode.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for subgraph_cross_edges.md (Unicode)"
    );
}

#[test]
fn golden_subgraph_cross_edges_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/subgraph_cross_edges.md",
    ]);
    let expected = include_str!("fixtures/expected/subgraph_cross_edges.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for subgraph_cross_edges.md (ASCII)"
    );
}
