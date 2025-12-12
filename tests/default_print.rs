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
