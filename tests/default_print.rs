use predicates::prelude::*;

#[test]
fn default_mode_prints_from_stdin() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("termiflow");
    cmd.write_stdin("flowchart TD\nA[Upstream] --> B[Downstream]\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Upstream"))
        .stdout(predicate::str::contains("Downstream"));
}

#[test]
fn tw_binary_alias_exists_and_prints() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("tw");
    cmd.write_stdin("flowchart LR\nA[Upstream] --> B[Downstream]\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Upstream"))
        .stdout(predicate::str::contains("Downstream"));
}

#[test]
fn wrap_flag_renders_multiline_boxes() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("tw");
    let assert = cmd
        .args(["--wrap", "--max-lines", "3"])
        .write_stdin("flowchart TD\nA[hello world from termiflow]\n")
        .assert()
        .success();

    let output = String::from_utf8_lossy(&assert.get_output().stdout);
    let lines: Vec<&str> = output.lines().collect();
    assert!(
        lines.len() >= 4,
        "expected a taller box (>=4 lines), got {} lines:\n{}",
        lines.len(),
        output
    );

    let label_lines: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| (l.contains("hello world") || l.contains("termiflow")).then_some(i))
        .collect();
    assert!(
        label_lines.len() >= 2 && label_lines[0] != label_lines[1],
        "expected wrapped label on multiple lines, got:\n{}",
        output
    );
    assert!(
        !output.contains("..."),
        "expected wrap (not ellipsis truncation), got:\n{}",
        output
    );
}
