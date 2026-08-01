use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("termiflow-{name}-{nonce}"))
}

#[test]
fn visual_audit_rejects_a_failing_renderer_without_publishing_a_run() {
    let output = unique_temp_dir("visual-audit-failure");
    let status = Command::new("bash")
        .arg("scripts/visual_audit.sh")
        .args([
            "--out",
            output.to_str().expect("temporary path is UTF-8"),
            "--binary",
            "/usr/bin/false",
            "--styles",
            "ascii",
            "--modes",
            "default",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("run visual audit script");

    assert!(!status.success(), "audit accepted a failing renderer");
    assert!(
        !output.exists(),
        "failed audit published a final artifact directory"
    );

    let parent = output.parent().expect("temporary directory parent");
    let prefix = format!("{}.staging.", output.file_name().unwrap().to_string_lossy());
    for entry in fs::read_dir(parent).expect("read temporary directory") {
        let path = entry.expect("read temporary entry").path();
        if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(&prefix))
        {
            fs::remove_dir_all(path).expect("remove retained failed staging artifact");
        }
    }
}

#[test]
fn visual_audit_publishes_relative_complete_packet_for_tiny_fixture() {
    let root = unique_temp_dir("visual-audit-success");
    let input_root = root.join("inputs");
    let metadata = root.join("metadata.json");
    let output = root.join("packet");
    fs::create_dir_all(&input_root).expect("create temporary input root");
    fs::write(
        input_root.join("tiny_td.md"),
        "graph TD\nA[Alpha] --> B[Beta]\n",
    )
    .expect("write temporary fixture");
    fs::write(
        &metadata,
        r#"{
  "schema": "termiflow.fixture_metadata.v1",
  "fixtures": [{
    "name": "tiny_td",
    "kind": "success",
    "direction": "TD",
    "stderr_policy": "empty",
    "stderr_contains": []
  }]
}
"#,
    )
    .expect("write temporary metadata");

    let status = Command::new("bash")
        .arg("scripts/visual_audit.sh")
        .args([
            "--out",
            output.to_str().expect("temporary path is UTF-8"),
            "--binary",
            env!("CARGO_BIN_EXE_termiflow"),
            "--input-root",
            input_root.to_str().expect("temporary path is UTF-8"),
            "--metadata",
            metadata.to_str().expect("temporary path is UTF-8"),
            "--styles",
            "ascii",
            "--modes",
            "default",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("run visual audit script");

    assert!(status.success(), "tiny visual audit failed: {status}");
    let manifest = output.join("manifest.jsonl");
    let manifest_text = fs::read_to_string(&manifest).expect("read manifest");
    let row: serde_json::Value =
        serde_json::from_str(manifest_text.trim()).expect("parse manifest row");
    assert_eq!(row["schema"], "termiflow.visual_audit.row.v2");
    assert_eq!(row["status"], 0);
    assert!((output.join(row["stdout"]["path"].as_str().unwrap())).is_file());
    assert!((output.join(row["stderr"]["path"].as_str().unwrap())).is_file());
    assert!((output.join(row["evidence"]["path"].as_str().unwrap())).is_file());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            &fs::read_to_string(output.join("summary.json")).unwrap()
        )
        .unwrap()["actual_rows"],
        1
    );
    assert!(fs::read_dir(root.parent().unwrap())
        .expect("read temporary parent")
        .filter_map(|entry| entry.ok())
        .all(
            |entry| !entry.file_name().to_string_lossy().starts_with(&format!(
                "{}.staging.",
                output.file_name().unwrap().to_string_lossy()
            ))
        ));

    fs::remove_dir_all(root).expect("remove temporary visual packet");
}
