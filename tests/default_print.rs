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
fn from_json_flag_renders_json_graphs() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("tw");
    cmd.arg("--from-json")
        .write_stdin(
            r#"{"direction":"TD","nodes":[{"id":"A","label":"Upstream"},{"id":"B","label":"Downstream"}],"edges":[{"from":"A","to":"B"}]}"#,
        )
        .assert()
        .success()
        .stdout(predicate::str::contains("Upstream"))
        .stdout(predicate::str::contains("Downstream"));
}

#[test]
fn audit_flag_emits_clean_visual_summary_for_simple_diagram() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("tw");
    cmd.arg("--audit")
        .write_stdin("flowchart TD\nA[Start] --> B[End]\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("audit verdict=Clean"))
        .stderr(predicate::str::contains("warnings=0"))
        .stderr(predicate::str::contains("errors=0"));
}

#[test]
fn audit_flag_keeps_diagram_output_newline_terminated() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("tw");
    let assert = cmd
        .args(["--audit", "--style", "ascii"])
        .write_stdin("flowchart TD\nA[Start] --> B[End]\n")
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    let last_line = stdout.lines().last().unwrap_or("").trim_start();

    assert!(
        stdout.ends_with('\n'),
        "expected audit-print stdout to end with newline, got:\n{}",
        stdout
    );
    assert!(
        last_line.starts_with('+') || last_line.starts_with('\\'),
        "expected the diagram border to remain the last stdout line, got:\n{}",
        stdout
    );
}

#[test]
fn output_is_cropped_by_default() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("tw");
    let assert = cmd
        .write_stdin("flowchart TD\nA[Start]\n")
        .assert()
        .success();

    let output = String::from_utf8_lossy(&assert.get_output().stdout);
    let first_nonempty = output.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    assert!(
        !first_nonempty.starts_with(' '),
        "expected cropped output (no leading margin), got:\n{}",
        output
    );
}

#[test]
fn compact_flag_produces_tighter_output() {
    let input = "flowchart TD\nA[Start] --> B[Process] --> C[End]\n";

    let mut normal = assert_cmd::cargo::cargo_bin_cmd!("tw");
    let normal_out = normal
        .write_stdin(input)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let mut compact = assert_cmd::cargo::cargo_bin_cmd!("tw");
    let compact_out = compact
        .arg("--compact")
        .write_stdin(input)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let normal_s = String::from_utf8_lossy(&normal_out);
    let compact_s = String::from_utf8_lossy(&compact_out);

    assert!(
        compact_s.lines().count() <= normal_s.lines().count(),
        "expected compact output to be no taller than normal\nnormal:\n{}\ncompact:\n{}",
        normal_s,
        compact_s
    );
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
fn directive_style_applies_without_cli_override() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("tw");
    let assert = cmd
        .write_stdin("graph TD\n%% termiflow: style=ascii\nA[Node]\n")
        .assert()
        .success();

    let output = String::from_utf8_lossy(&assert.get_output().stdout);
    let first_line = output.lines().next().unwrap_or("");
    assert!(
        first_line.starts_with('+'),
        "expected ascii border from in-file directive, got:\n{}",
        output
    );
}

#[test]
fn directive_max_label_applies_without_cli_override() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("tw");
    let assert = cmd
        .write_stdin("graph TD\n%% termiflow: max_label=5\nA[abcdefghij]\n")
        .assert()
        .success();

    let output = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(
        !output.contains("abcdefghij"),
        "expected in-file max_label directive to truncate, got:\n{}",
        output
    );
    assert!(
        output.contains("..."),
        "expected truncated output with ellipsis, got:\n{}",
        output
    );
}

#[test]
fn directive_wrap_applies_without_cli_override() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("tw");
    let assert = cmd
        .write_stdin(
            "graph TD\n%% termiflow: wrap=true\n%% termiflow: max_lines=3\nA[hello world from termiflow]\n",
        )
        .assert()
        .success();

    let output = String::from_utf8_lossy(&assert.get_output().stdout);
    let lines: Vec<&str> = output.lines().collect();
    let label_lines: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| (l.contains("hello world") || l.contains("termiflow")).then_some(i))
        .collect();

    assert!(
        label_lines.len() >= 2 && label_lines[0] != label_lines[1],
        "expected wrapped label from in-file directives, got:\n{}",
        output
    );
    assert!(
        !output.contains("..."),
        "expected wrap (not ellipsis truncation), got:\n{}",
        output
    );
}

#[test]
fn classdef_warning_does_not_render_bogus_node() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("termiflow");
    let assert = cmd
        .arg("--print")
        .arg("tests/fixtures/inputs/warn_classDef_td.md")
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);

    assert!(stdout.contains("Start"));
    assert!(stdout.contains("End"));
    assert!(
        !stdout.contains("highlight"),
        "expected classDef to be ignored during rendering, got:\n{}",
        stdout
    );
    assert!(
        stderr.contains("Mermaid classes not supported"),
        "expected classDef warning, got:\n{}",
        stderr
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
        output.contains("Canvas::")
            && (output.contains("set_edge_char") || output.contains("set_edge_")),
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
        .stdout(predicate::str::contains("Container"))
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

#[test]
fn subgraph_fanin_bt_title_renders_on_bottom_interior_row() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("tw");
    let assert = cmd
        .arg("tests/fixtures/inputs/subgraph_fanin_bt.md")
        .assert()
        .success();

    let output = String::from_utf8_lossy(&assert.get_output().stdout);
    let lines: Vec<&str> = output.lines().collect();
    let title_idx = lines
        .iter()
        .position(|line| line.contains("Data Sources"))
        .expect("BT title row");

    assert_eq!(
        title_idx,
        lines.len().saturating_sub(2),
        "expected BT subgraph title on the bottom interior row, got:\n{}",
        output
    );
    let bottom_border = lines.last().copied().unwrap_or_default();
    assert!(
        !bottom_border.contains("Data Sources"),
        "expected BT bottom border to remain separate from the title row, got:\n{}",
        output
    );
}
