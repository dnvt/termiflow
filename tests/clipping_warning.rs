use assert_cmd::Command;

/// Phase 4 raised canvas limits to 10000×5000 and silenced clipping warnings.
/// Wide graphs now render fully without any "Graph too wide" stderr noise.
#[test]
fn wide_graphs_no_longer_emit_clipping_warnings() {
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
        !stderr.contains("Graph too wide"),
        "unexpected clipping warning: {stderr}"
    );

    // Output should be non-empty (diagram rendered fully)
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        !stdout.trim().is_empty(),
        "expected non-empty diagram output"
    );
}
