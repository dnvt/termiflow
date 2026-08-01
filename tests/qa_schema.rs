use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn qa_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_termiflow-qa"))
}

fn spec_path() -> &'static str {
    "tests/fixtures/fixture_spec.json"
}

fn temp_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "termiflow-schema-{label}-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos()
    ))
}

fn write_mutated_spec(label: &str, mutate: impl FnOnce(&mut Value)) -> PathBuf {
    let source = fs::read_to_string(spec_path()).expect("read canonical fixture spec");
    let mut value: Value = serde_json::from_str(&source).expect("parse canonical fixture spec");
    mutate(&mut value);
    let path = temp_path(label);
    fs::write(
        &path,
        serde_json::to_vec_pretty(&value).expect("serialize mutated spec"),
    )
    .expect("write mutated fixture spec");
    path
}

fn run_check(path: &Path) -> std::process::Output {
    qa_command()
        .args(["schema", "--spec"])
        .arg(path)
        .args(["--check"])
        .output()
        .expect("run termiflow-qa schema check")
}

#[test]
fn canonical_canary_checks_and_manifest_is_reproducible() {
    let check = qa_command()
        .args(["schema", "--spec", spec_path(), "--check"])
        .output()
        .expect("run schema check");
    assert!(check.status.success(), "schema check failed: {check:?}");
    let summary: Value = serde_json::from_slice(&check.stdout).expect("parse check summary");
    assert_eq!(summary["row_count"], 16);
    assert_eq!(summary["negative_case_count"], 1);
    assert_eq!(summary["holdout_variant_count"], 1);

    let first = temp_path("manifest-a");
    let second = temp_path("manifest-b");
    for path in [&first, &second] {
        let output = qa_command()
            .args(["schema", "--spec", spec_path(), "--emit-manifest"])
            .arg(path)
            .output()
            .expect("run schema manifest generation");
        assert!(
            output.status.success(),
            "manifest generation failed: {output:?}"
        );
    }
    assert_eq!(
        fs::read(&first).expect("read first manifest"),
        fs::read(&second).expect("read second manifest")
    );
    let manifest: Value =
        serde_json::from_slice(&fs::read(&first).expect("read manifest")).expect("parse manifest");
    assert_eq!(manifest["schema"], "termiflow.fixture_manifest.v1");
    assert_eq!(manifest["row_count"], 16);
    assert_eq!(manifest["negative_case_count"], 1);
    assert_eq!(manifest["holdout_variant_count"], 1);
    assert_eq!(manifest["rows"].as_array().expect("rows").len(), 16);
    assert_eq!(
        manifest["negative_cases"]
            .as_array()
            .expect("negative cases")
            .len(),
        1
    );
    fs::remove_file(first).expect("remove first manifest");
    fs::remove_file(second).expect("remove second manifest");
}

#[test]
fn unknown_root_field_fails_closed() {
    let path = write_mutated_spec("unknown-root", |value| {
        value["unexpected"] = Value::Bool(true);
    });
    let output = run_check(&path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field"));
    fs::remove_file(path).expect("remove mutated spec");
}

#[test]
fn direction_source_mismatch_fails_closed() {
    let path = write_mutated_spec("direction", |value| {
        value["cases"][0]["variants"][0]["direction"] = Value::String("LR".to_owned());
    });
    let output = run_check(&path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("does not match"));
    fs::remove_file(path).expect("remove mutated spec");
}

#[test]
fn duplicate_case_fails_closed() {
    let path = write_mutated_spec("duplicate", |value| {
        let duplicate = value["cases"][0].clone();
        value["cases"]
            .as_array_mut()
            .expect("cases")
            .push(duplicate);
    });
    let output = run_check(&path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("duplicate fixture spec case"));
    fs::remove_file(path).expect("remove mutated spec");
}

#[test]
fn malformed_semantic_reference_fails_closed() {
    let path = write_mutated_spec("semantic", |value| {
        value["cases"][0]["semantic"]["edges"][0]["from"] = Value::String("missing".to_owned());
    });
    let output = run_check(&path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown node"));
    fs::remove_file(path).expect("remove mutated spec");
}

#[test]
fn evaluator_holdout_is_not_emitted_as_a_reviewable_row() {
    let path = temp_path("holdout-manifest");
    let output = qa_command()
        .args(["schema", "--spec", spec_path(), "--emit-manifest"])
        .arg(&path)
        .output()
        .expect("run holdout manifest generation");
    assert!(output.status.success());
    let manifest: Value =
        serde_json::from_slice(&fs::read(&path).expect("read manifest")).expect("parse manifest");
    assert!(manifest["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .all(|row| row["holdout"] != "evaluator_owned"));
    assert_eq!(manifest["holdouts"].as_array().expect("holdouts").len(), 1);
    fs::remove_file(path).expect("remove holdout manifest");
}
