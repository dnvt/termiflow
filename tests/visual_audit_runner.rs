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
