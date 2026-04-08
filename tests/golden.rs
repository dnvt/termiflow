#![cfg(feature = "golden")]
//! Golden tests - compare actual output against expected fixtures.
//!
//! Naming convention: [category]_[name]_[direction].md
//! Categories: flow, edge, label, shape, parse, config, error, subgraph, scale, crossing
//! Direction: td (default), lr, rl, bt
//!
//! Error tests for invalid diagram types (e.g., error_sequence.md) omit the
//! direction suffix and compare stderr instead of stdout.

use std::fs;
use std::path::{Path, PathBuf};
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

fn list_inputs() -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = fs::read_dir("tests/fixtures/inputs")
        .expect("failed to read tests/fixtures/inputs")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("md"))
        .collect();
    entries.sort();
    entries
}

fn is_error_fixture(base: &str) -> bool {
    base.starts_with("error_")
}

fn expected_path(base: &str, style: &str) -> PathBuf {
    Path::new("tests/fixtures/expected").join(format!("{base}.{style}.txt"))
}

fn normalize_trailing_newline(text: &str) -> &str {
    text.strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .unwrap_or(text)
}

fn run_golden_for_style(style: &str) {
    let inputs = list_inputs();
    assert!(
        !inputs.is_empty(),
        "no fixtures found in tests/fixtures/inputs"
    );

    for input in inputs {
        let base = input
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("invalid fixture filename");
        let (stdout, stderr) = run_termiflow(&[
            "--print",
            "--style",
            style,
            input.to_str().expect("invalid fixture path"),
        ]);

        let expected_path = expected_path(base, style);
        let expected = fs::read_to_string(&expected_path)
            .unwrap_or_else(|_| panic!("missing expected fixture: {}", expected_path.display()));

        let stdout = normalize_trailing_newline(&stdout);
        let stderr = normalize_trailing_newline(&stderr);
        let expected = normalize_trailing_newline(&expected);

        if is_error_fixture(base) {
            assert_eq!(stderr, expected, "stderr mismatch for {base}.md ({style})");
            assert!(
                stdout.is_empty(),
                "{base}.md ({style}) should not write to stdout"
            );
        } else {
            assert_eq!(stdout, expected, "stdout mismatch for {base}.md ({style})");
        }
    }
}

#[test]
fn golden_unicode() {
    run_golden_for_style("unicode");
}

#[test]
fn golden_ascii() {
    run_golden_for_style("ascii");
}
