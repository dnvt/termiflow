use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

#[test]
fn file_flag_alias_produces_output() {
    let expected = include_str!("fixtures/expected/flow_branch_td.unicode.txt");
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("termiflow"));
    cmd.args([
        "--print",
        "--style",
        "unicode",
        "-f",
        "tests/fixtures/inputs/flow_branch_td.md",
    ])
    .assert()
    .success()
    .stdout(expected);
}

#[test]
fn unsupported_diagram_type_errors() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("termiflow"));
    cmd.args(["--print", "tests/fixtures/inputs/error_sequence.md"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "diagram type not supported (found: 'sequenceDiagram')",
        ));
}
