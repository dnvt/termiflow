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

#[test]
fn wrap_prefers_code_delimiters_over_mid_word_splits() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("tw");
    let assert = cmd
        .args(["--wrap", "--max-lines", "3", "--max-label", "18"])
        .write_stdin("flowchart TD\nA[route_convergent_edges]\nB[Canvas::set_edge_char]\n")
        .assert()
        .success();

    let output = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(
        output.contains("route_convergent_") || output.contains("route_convergent"),
        "expected delimiter-aware wrapping, got:\n{}",
        output
    );
    assert!(
        !output.contains("route_convergent_edg"),
        "expected to avoid mid-word splits, got:\n{}",
        output
    );
    assert!(
        output.contains("Canvas::") && (output.contains("set_edge_char") || output.contains("set_edge_")),
        "expected delimiter-aware wrapping for `Canvas::set_edge_char`, got:\n{}",
        output
    );
    assert!(
        !output.contains("set_edge_cha"),
        "expected to avoid mid-word splits, got:\n{}",
        output
    );
}

#[test]
fn subgraph_title_stays_clean_for_entry_edge_td() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("tw");
    cmd.arg("tests/fixtures/inputs/subgraph_single_td.md")
        .assert()
        .success()
        .stdout(predicate::str::contains("┏━━[  Container  ]━━┓"))
        .stdout(predicate::str::contains("┼").not());
}

#[test]
fn converge_bt_uses_correct_merge_corners() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("tw");
    cmd.arg("tests/fixtures/inputs/edge_converge_bt.md")
        .assert()
        .success()
        .stdout(predicate::str::contains("┌────────┴────────┐"))
        .stdout(predicate::str::contains("└────────┴────────┘").not());
}

#[test]
fn subgraph_fanout_td_title_stays_clean() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("tw");
    let assert = cmd
        .arg("tests/fixtures/inputs/subgraph_fanout_td.md")
        .assert()
        .success();

    let output = String::from_utf8_lossy(&assert.get_output().stdout);
    let title_row = output
        .lines()
        .find(|l| l.contains("Handler Group"))
        .unwrap_or_default();
    assert!(
        !title_row.contains('┼'),
        "expected no edge to pierce the title row, got:\n{}",
        output
    );
}

#[test]
fn subgraph_complex_td_title_stays_clean() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("tw");
    let assert = cmd
        .arg("tests/fixtures/inputs/subgraph_complex_td.md")
        .assert()
        .success();

    let output = String::from_utf8_lossy(&assert.get_output().stdout);
    let title_row = output
        .lines()
        .find(|l| l.contains("Data Layer"))
        .unwrap_or_default();
    assert!(
        !title_row.contains('┼'),
        "expected no edge to pierce the title row, got:\n{}",
        output
    );
}
