//! Golden tests - compare actual output against expected fixtures
//!
//! Naming convention: [category]_[name]_[direction].md
//! Categories: flow, edge, label, shape, parse, config, error
//! Direction: td (default), lr, rl, bt
//!
//! Note: Error tests for invalid diagram types (e.g., error_sequence.md) may omit
//! the direction suffix since they're rejected before direction processing.

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
// Flow Tests - Basic flowchart flow
// ============================================================================

#[test]
fn golden_flow_simple_td_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/flow_simple_td.md",
    ]);
    let expected = include_str!("fixtures/expected/flow_simple_td.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for flow_simple_td.md");
}

#[test]
fn golden_flow_simple_td_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/flow_simple_td.md",
    ]);
    let expected = include_str!("fixtures/expected/flow_simple_td.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for flow_simple_td.md (ASCII)"
    );
}

#[test]
fn golden_flow_branch_td_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/flow_branch_td.md",
    ]);
    let expected = include_str!("fixtures/expected/flow_branch_td.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for flow_branch_td.md");
}

#[test]
fn golden_flow_branch_td_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/flow_branch_td.md",
    ]);
    let expected = include_str!("fixtures/expected/flow_branch_td.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for flow_branch_td.md (ASCII)"
    );
}

#[test]
fn golden_flow_chain_td_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/flow_chain_td.md",
    ]);
    let expected = include_str!("fixtures/expected/flow_chain_td.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for flow_chain_td.md");
}

#[test]
fn golden_flow_chain_td_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/flow_chain_td.md",
    ]);
    let expected = include_str!("fixtures/expected/flow_chain_td.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for flow_chain_td.md (ASCII)"
    );
}

#[test]
fn golden_flow_simple_lr_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/flow_simple_lr.md",
    ]);
    let expected = include_str!("fixtures/expected/flow_simple_lr.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for flow_simple_lr.md");
}

#[test]
fn golden_flow_simple_lr_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/flow_simple_lr.md",
    ]);
    let expected = include_str!("fixtures/expected/flow_simple_lr.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for flow_simple_lr.md (ASCII)"
    );
}

#[test]
fn golden_flow_simple_rl_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/flow_simple_rl.md",
    ]);
    let expected = include_str!("fixtures/expected/flow_simple_rl.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for flow_simple_rl.md");
}

#[test]
fn golden_flow_simple_rl_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/flow_simple_rl.md",
    ]);
    let expected = include_str!("fixtures/expected/flow_simple_rl.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for flow_simple_rl.md (ASCII)"
    );
}

#[test]
fn golden_flow_simple_bt_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/flow_simple_bt.md",
    ]);
    let expected = include_str!("fixtures/expected/flow_simple_bt.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for flow_simple_bt.md");
}

#[test]
fn golden_flow_simple_bt_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/flow_simple_bt.md",
    ]);
    let expected = include_str!("fixtures/expected/flow_simple_bt.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for flow_simple_bt.md (ASCII)"
    );
}

// ============================================================================
// Edge Tests - Edge routing (branch, converge, complex)
// ============================================================================

#[test]
fn golden_edge_branch_td_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/edge_branch_td.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_branch_td.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for edge_branch_td.md");
}

#[test]
fn golden_edge_branch_td_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/edge_branch_td.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_branch_td.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for edge_branch_td.md (ASCII)"
    );
}

#[test]
fn golden_edge_branch_lr_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/edge_branch_lr.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_branch_lr.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for edge_branch_lr.md");
}

#[test]
fn golden_edge_branch_lr_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/edge_branch_lr.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_branch_lr.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for edge_branch_lr.md (ASCII)"
    );
}

#[test]
fn golden_edge_branch_rl_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/edge_branch_rl.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_branch_rl.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for edge_branch_rl.md");
}

#[test]
fn golden_edge_branch_rl_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/edge_branch_rl.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_branch_rl.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for edge_branch_rl.md (ASCII)"
    );
}

#[test]
fn golden_edge_branch_bt_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/edge_branch_bt.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_branch_bt.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for edge_branch_bt.md");
}

#[test]
fn golden_edge_branch_bt_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/edge_branch_bt.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_branch_bt.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for edge_branch_bt.md (ASCII)"
    );
}

#[test]
fn golden_edge_complex_td_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/edge_complex_td.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_complex_td.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for edge_complex_td.md");
}

#[test]
fn golden_edge_complex_td_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/edge_complex_td.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_complex_td.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for edge_complex_td.md (ASCII)"
    );
}

#[test]
fn golden_edge_complex_lr_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/edge_complex_lr.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_complex_lr.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for edge_complex_lr.md");
}

#[test]
fn golden_edge_complex_lr_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/edge_complex_lr.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_complex_lr.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for edge_complex_lr.md (ASCII)"
    );
}

#[test]
fn golden_edge_complex_rl_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/edge_complex_rl.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_complex_rl.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for edge_complex_rl.md");
}

#[test]
fn golden_edge_complex_rl_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/edge_complex_rl.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_complex_rl.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for edge_complex_rl.md (ASCII)"
    );
}

#[test]
fn golden_edge_complex_bt_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/edge_complex_bt.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_complex_bt.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for edge_complex_bt.md");
}

#[test]
fn golden_edge_complex_bt_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/edge_complex_bt.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_complex_bt.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for edge_complex_bt.md (ASCII)"
    );
}

#[test]
fn golden_edge_converge_td_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/edge_converge_td.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_converge_td.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for edge_converge_td.md");
}

#[test]
fn golden_edge_converge_td_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/edge_converge_td.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_converge_td.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for edge_converge_td.md (ASCII)"
    );
}

#[test]
fn golden_edge_converge_lr_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/edge_converge_lr.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_converge_lr.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for edge_converge_lr.md");
}

#[test]
fn golden_edge_converge_lr_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/edge_converge_lr.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_converge_lr.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for edge_converge_lr.md (ASCII)"
    );
}

#[test]
fn golden_edge_converge_rl_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/edge_converge_rl.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_converge_rl.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for edge_converge_rl.md");
}

#[test]
fn golden_edge_converge_rl_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/edge_converge_rl.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_converge_rl.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for edge_converge_rl.md (ASCII)"
    );
}

#[test]
fn golden_edge_converge_bt_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/edge_converge_bt.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_converge_bt.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for edge_converge_bt.md");
}

#[test]
fn golden_edge_converge_bt_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/edge_converge_bt.md",
    ]);
    let expected = include_str!("fixtures/expected/edge_converge_bt.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for edge_converge_bt.md (ASCII)"
    );
}

// ============================================================================
// Label Tests - Edge labels
// ============================================================================

#[test]
fn golden_label_basic_td_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/label_basic_td.md",
    ]);
    let expected = include_str!("fixtures/expected/label_basic_td.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for label_basic_td.md");
}

#[test]
fn golden_label_basic_td_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/label_basic_td.md",
    ]);
    let expected = include_str!("fixtures/expected/label_basic_td.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for label_basic_td.md (ASCII)"
    );
}

// ============================================================================
// Shape Tests - Node shapes
// ============================================================================

#[test]
fn golden_shape_all_td_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/shape_all_td.md",
    ]);
    let expected = include_str!("fixtures/expected/shape_all_td.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for shape_all_td.md");
}

#[test]
fn golden_shape_all_td_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/shape_all_td.md",
    ]);
    let expected = include_str!("fixtures/expected/shape_all_td.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for shape_all_td.md (ASCII)"
    );
}

#[test]
fn golden_shape_database_td_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/shape_database_td.md",
    ]);
    let expected = include_str!("fixtures/expected/shape_database_td.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for shape_database_td.md");
}

#[test]
fn golden_shape_database_td_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/shape_database_td.md",
    ]);
    let expected = include_str!("fixtures/expected/shape_database_td.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for shape_database_td.md (ASCII)"
    );
}

// ============================================================================
// Parse Tests - Parser features
// ============================================================================

#[test]
fn golden_parse_forward_td_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/parse_forward_td.md",
    ]);
    let expected = include_str!("fixtures/expected/parse_forward_td.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for parse_forward_td.md");
}

#[test]
fn golden_parse_forward_td_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/parse_forward_td.md",
    ]);
    let expected = include_str!("fixtures/expected/parse_forward_td.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for parse_forward_td.md (ASCII)"
    );
}

// ============================================================================
// Config Tests - Configuration directives
// ============================================================================

#[test]
fn golden_config_style_td_unicode() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/config_style_td.md",
    ]);
    let expected = include_str!("fixtures/expected/config_style_td.unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for config_style_td.md");
}

#[test]
fn golden_config_style_td_ascii() {
    let (stdout, _) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/config_style_td.md",
    ]);
    let expected = include_str!("fixtures/expected/config_style_td.ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for config_style_td.md (ASCII)"
    );
}

// ============================================================================
// Subgraph Tests - Basic subgraph parsing (rendering is Phase 5)
// ============================================================================

#[test]
fn golden_subgraph_basic_td_unicode() {
    // Subgraphs are parsed but visual rendering is not yet implemented
    // This test verifies parsing works - nodes inside subgraph are rendered normally
    let (stdout, _stderr) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/subgraph_basic_td.md",
    ]);

    let expected = include_str!("fixtures/expected/subgraph_basic_td_unicode.txt");
    assert_eq!(stdout, expected, "Output mismatch for subgraph_basic_td.md");
}

#[test]
fn golden_subgraph_basic_td_ascii() {
    let (stdout, _stderr) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/subgraph_basic_td.md",
    ]);
    let expected = include_str!("fixtures/expected/subgraph_basic_td_ascii.txt");
    assert_eq!(
        stdout, expected,
        "Output mismatch for subgraph_basic_td.md (ASCII)"
    );
}

// ============================================================================
// Error Tests - Unsupported features
// ============================================================================

#[test]
fn golden_error_sequence_unicode() {
    let (stdout, stderr) = run_termiflow(&[
        "--print",
        "--style",
        "unicode",
        "tests/fixtures/inputs/error_sequence.md",
    ]);
    let expected = include_str!("fixtures/expected/error_sequence.unicode.txt");
    assert_eq!(stderr, expected, "Output mismatch for error_sequence.md");
    assert!(stdout.is_empty(), "error_sequence.md should not write to stdout");
}

#[test]
fn golden_error_sequence_ascii() {
    let (stdout, stderr) = run_termiflow(&[
        "--print",
        "--style",
        "ascii",
        "tests/fixtures/inputs/error_sequence.md",
    ]);
    let expected = include_str!("fixtures/expected/error_sequence.ascii.txt");
    assert_eq!(
        stderr, expected,
        "Output mismatch for error_sequence.md (ASCII)"
    );
    assert!(stdout.is_empty(), "error_sequence.md should not write to stdout");
}
