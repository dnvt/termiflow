use assert_cmd::Command;

/// Ensure wide graphs emit clipping warnings to stderr.
#[test]
fn warns_on_width_clipping() {
    // Build a wide graph by fanning out many children in rank 1
    let mut input = String::from("graph TD\nA[Root]\n");
    for i in 0..60 {
        let node = format!("N{i}[N{i}]\n");
        input.push_str(&node);
        input.push_str(&format!("A --> N{i}\n"));
    }

    #[allow(deprecated)]
    let assert = Command::cargo_bin("termiflow")
        .expect("binary exists")
        .args(["--print", "--style", "unicode"])
        .write_stdin(input)
        .assert()
        .success();

    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("Graph too wide"),
        "expected width clipping warning, got: {stderr}"
    );
}
