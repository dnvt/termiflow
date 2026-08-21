use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn qa_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_termiflow-qa"))
}

fn renderer_binary() -> &'static str {
    env!("CARGO_BIN_EXE_termiflow")
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
        .args(["--queue", "canonical-smoke"])
        .args(["--check"])
        .output()
        .expect("run termiflow-qa schema check")
}

fn emit_manifest(label: &str) -> PathBuf {
    emit_manifest_for(label, "canonical-smoke")
}

fn emit_manifest_for(label: &str, queue: &str) -> PathBuf {
    let path = temp_path(label);
    let output = qa_command()
        .args([
            "schema",
            "--spec",
            spec_path(),
            "--queue",
            queue,
            "--emit-manifest",
        ])
        .arg(&path)
        .output()
        .expect("run schema manifest generation");
    assert!(
        output.status.success(),
        "manifest generation failed: {output:?}"
    );
    path
}

fn run_manifest(manifest: &Path, report: Option<&Path>) -> std::process::Output {
    let mut command = qa_command();
    command.args(["golden", "--manifest"]).arg(manifest);
    if let Some(report) = report {
        command.args(["--check", "--report"]).arg(report);
    } else {
        command.arg("--check");
    }
    command.output().expect("run manifest golden check")
}

#[test]
fn canonical_canary_checks_and_manifest_is_reproducible() {
    let check = qa_command()
        .args([
            "schema",
            "--spec",
            spec_path(),
            "--queue",
            "canonical-smoke",
            "--check",
        ])
        .output()
        .expect("run schema check");
    assert!(check.status.success(), "schema check failed: {check:?}");
    let summary: Value = serde_json::from_slice(&check.stdout).expect("parse check summary");
    assert_eq!(summary["row_count"], 16);
    assert_eq!(summary["negative_case_count"], 1);
    assert_eq!(summary["holdout_variant_count"], 1);
    assert_eq!(summary["holdout_row_count"], 4);

    let first = temp_path("manifest-a");
    let second = temp_path("manifest-b");
    for path in [&first, &second] {
        let output = qa_command()
            .args([
                "schema",
                "--spec",
                spec_path(),
                "--queue",
                "canonical-smoke",
                "--emit-manifest",
            ])
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
    assert_eq!(manifest["schema"], "termiflow.fixture_manifest.v2");
    assert_eq!(manifest["row_count"], 16);
    assert_eq!(manifest["negative_case_count"], 1);
    assert_eq!(manifest["holdout_variant_count"], 1);
    assert_eq!(manifest["holdout_row_count"], 4);
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
fn unknown_queue_fails_closed() {
    let output = qa_command()
        .args([
            "schema",
            "--spec",
            spec_path(),
            "--queue",
            "missing-queue",
            "--check",
        ])
        .output()
        .expect("run unknown queue check");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown fixture spec queue"));
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
        .args([
            "schema",
            "--spec",
            spec_path(),
            "--queue",
            "canonical-smoke",
            "--emit-manifest",
        ])
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
    assert_eq!(manifest["holdouts"].as_array().expect("holdouts").len(), 4);
    assert!(manifest["holdouts"]
        .as_array()
        .expect("holdouts")
        .iter()
        .all(|row| row["golden"].is_null()));
    fs::remove_file(path).expect("remove holdout manifest");
}

#[test]
fn junction_queue_is_deterministic_and_holdout_isolated() {
    let check = qa_command()
        .args([
            "schema",
            "--spec",
            spec_path(),
            "--queue",
            "junction-quad",
            "--check",
        ])
        .output()
        .expect("run junction schema check");
    assert!(
        check.status.success(),
        "junction schema check failed: {check:?}"
    );
    let summary: Value = serde_json::from_slice(&check.stdout).expect("parse junction summary");
    assert_eq!(summary["queue_id"], "junction-quad");
    assert_eq!(summary["row_count"], 16);
    assert_eq!(summary["negative_case_count"], 0);
    assert_eq!(summary["holdout_variant_count"], 4);
    assert_eq!(summary["holdout_row_count"], 16);

    let first = emit_manifest_for("junction-manifest-a", "junction-quad");
    let second = emit_manifest_for("junction-manifest-b", "junction-quad");
    assert_eq!(
        fs::read(&first).expect("read first junction manifest"),
        fs::read(&second).expect("read second junction manifest")
    );
    let manifest: Value =
        serde_json::from_slice(&fs::read(&first).expect("read junction manifest"))
            .expect("parse junction manifest");
    assert!(manifest["rows"]
        .as_array()
        .expect("junction rows")
        .iter()
        .all(|row| row["case_id"] == "junction_quad_canary"));
    assert!(manifest["rows"]
        .as_array()
        .expect("junction rows")
        .iter()
        .all(|row| row["holdout"] != "evaluator_owned"));
    assert!(manifest["holdouts"]
        .as_array()
        .expect("junction holdouts")
        .iter()
        .all(|row| row["holdout"] == "evaluator_owned" && row["golden"].is_null()));
    fs::remove_file(first).expect("remove first junction manifest");
    fs::remove_file(second).expect("remove second junction manifest");
}

#[test]
fn scoped_visual_packet_and_holdout_receipt_are_hash_bound() {
    let manifest = emit_manifest_for("scoped-visual-manifest", "junction-quad");
    let packet = temp_path("scoped-visual-packet").with_extension("packet");
    let audit = qa_command()
        .args(["visual-audit", "--schema-manifest"])
        .arg(&manifest)
        .args(["--binary", renderer_binary()])
        .args(["--out"])
        .arg(&packet)
        .output()
        .expect("run scoped visual audit");
    assert!(
        audit.status.success(),
        "scoped visual audit failed: {audit:?}"
    );

    let validation = qa_command()
        .args(["visual-validate", "--packet"])
        .arg(&packet)
        .args(["--queue-manifest"])
        .arg(&manifest)
        .output()
        .expect("validate scoped visual packet");
    assert!(
        validation.status.success(),
        "scoped visual validation failed: {validation:?}"
    );

    let holdout_packet = temp_path("scoped-holdout-packet").with_extension("packet");
    let receipt = temp_path("scoped-holdout-receipt");
    let holdout = qa_command()
        .args([
            "holdout",
            "--spec",
            spec_path(),
            "--queue",
            "junction-quad",
            "--binary",
            renderer_binary(),
            "--out",
        ])
        .arg(&holdout_packet)
        .args(["--receipt"])
        .arg(&receipt)
        .output()
        .expect("run scoped holdout executor");
    assert!(
        holdout.status.success(),
        "scoped holdout execution failed: {holdout:?}"
    );
    let holdout_validation = qa_command()
        .args(["visual-validate", "--packet"])
        .arg(&holdout_packet)
        .args(["--queue-manifest"])
        .arg(&manifest)
        .args(["--holdout"])
        .output()
        .expect("validate scoped holdout packet");
    assert!(
        holdout_validation.status.success(),
        "scoped holdout validation failed: {holdout_validation:?}"
    );
    let receipt_value: Value =
        serde_json::from_slice(&fs::read(&receipt).expect("read holdout receipt"))
            .expect("parse holdout receipt");
    assert_eq!(receipt_value["schema"], "termiflow.holdout_receipt.v1");
    assert_eq!(receipt_value["queue_id"], "junction-quad");
    assert_eq!(receipt_value["expected_rows"], 16);
    assert_eq!(receipt_value["actual_rows"], 16);
    assert_eq!(receipt_value["status"], "passed");
    assert!(receipt_value["rows"]
        .as_array()
        .expect("receipt rows")
        .iter()
        .all(|row| row["status"] == "passed"));

    let receipt_before_retry = fs::read(&receipt).expect("read receipt before retry");
    let retry = qa_command()
        .args([
            "holdout",
            "--spec",
            spec_path(),
            "--queue",
            "junction-quad",
            "--binary",
            renderer_binary(),
            "--out",
        ])
        .arg(&holdout_packet)
        .args(["--receipt"])
        .arg(&receipt)
        .output()
        .expect("retry scoped holdout executor");
    assert!(
        retry.status.success(),
        "holdout reconciliation failed: {retry:?}"
    );
    assert!(String::from_utf8_lossy(&retry.stdout).contains("reconciled"));
    assert_eq!(
        fs::read(&receipt).expect("read receipt after retry"),
        receipt_before_retry,
        "retry must not overwrite an authoritative holdout receipt"
    );

    fs::remove_file(manifest).expect("remove scoped manifest");
    fs::remove_dir_all(packet).expect("remove scoped packet");
    fs::remove_dir_all(holdout_packet).expect("remove scoped holdout packet");
    fs::remove_file(receipt).expect("remove scoped receipt");
}

#[test]
fn source_only_holdout_materializes_and_validates() {
    let manifest = emit_manifest_for("source-only-holdout-manifest", "canonical-smoke");
    let holdout_packet = temp_path("source-only-holdout-packet").with_extension("packet");
    let receipt = temp_path("source-only-holdout-receipt");
    let holdout = qa_command()
        .args([
            "holdout",
            "--spec",
            spec_path(),
            "--queue",
            "canonical-smoke",
            "--binary",
            renderer_binary(),
            "--out",
        ])
        .arg(&holdout_packet)
        .args(["--receipt"])
        .arg(&receipt)
        .output()
        .expect("run source-only holdout executor");
    assert!(
        holdout.status.success(),
        "source-only holdout execution failed: {holdout:?}"
    );
    let validation = qa_command()
        .args(["visual-validate", "--packet"])
        .arg(&holdout_packet)
        .args(["--queue-manifest"])
        .arg(&manifest)
        .args(["--holdout"])
        .output()
        .expect("validate source-only holdout packet");
    assert!(
        validation.status.success(),
        "source-only holdout validation failed: {validation:?}"
    );
    let receipt_value: Value =
        serde_json::from_slice(&fs::read(&receipt).expect("read source-only receipt"))
            .expect("parse source-only receipt");
    assert_eq!(receipt_value["queue_id"], "canonical-smoke");
    assert_eq!(receipt_value["expected_rows"], 4);
    assert_eq!(receipt_value["actual_rows"], 4);
    assert_eq!(receipt_value["status"], "passed");

    fs::remove_file(manifest).expect("remove source-only manifest");
    fs::remove_dir_all(holdout_packet).expect("remove source-only packet");
    fs::remove_file(receipt).expect("remove source-only receipt");
}

#[test]
fn manifest_golden_bridge_checks_canary_and_negative_contract() {
    let manifest_path = emit_manifest("golden-bridge-manifest");
    let report_path = temp_path("golden-bridge-report");
    let output = run_manifest(&manifest_path, Some(&report_path));
    assert!(output.status.success(), "golden bridge failed: {output:?}");
    let report: Value =
        serde_json::from_slice(&fs::read(&report_path).expect("read golden bridge report"))
            .expect("parse golden bridge report");
    assert_eq!(report["schema"], "termiflow.golden_manifest_update.v1");
    assert_eq!(report["eligible_rows"], 8);
    assert_eq!(report["candidate_count"], 8);
    assert_eq!(report["changed_candidate_count"], 0);
    assert_eq!(
        report["negative_results"]
            .as_array()
            .expect("negative results")
            .len(),
        4
    );
    assert_eq!(report["holdout_variant_count"], 1);
    assert_eq!(report["holdout_row_count"], 4);
    fs::remove_file(manifest_path).expect("remove manifest");
    fs::remove_file(report_path).expect("remove report");
}

#[test]
fn manifest_golden_report_is_reproducible() {
    let manifest_path = emit_manifest("deterministic-golden-manifest");
    let first_report = temp_path("deterministic-golden-report-a");
    let second_report = temp_path("deterministic-golden-report-b");
    let first = run_manifest(&manifest_path, Some(&first_report));
    let second = run_manifest(&manifest_path, Some(&second_report));
    assert!(
        first.status.success(),
        "first golden bridge failed: {first:?}"
    );
    assert!(
        second.status.success(),
        "second golden bridge failed: {second:?}"
    );
    assert_eq!(
        fs::read(&first_report).expect("read first report"),
        fs::read(&second_report).expect("read second report")
    );
    fs::remove_file(manifest_path).expect("remove manifest");
    fs::remove_file(first_report).expect("remove first report");
    fs::remove_file(second_report).expect("remove second report");
}

#[test]
fn missing_golden_stem_fails_closed() {
    let path = write_mutated_spec("missing-golden-stem", |value| {
        value["cases"][0]["variants"][0]
            .as_object_mut()
            .expect("variant object")
            .remove("golden_stem");
    });
    let output = run_check(&path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("golden_stem"));
    fs::remove_file(path).expect("remove mutated spec");
}

#[test]
fn manifest_source_hash_mismatch_fails_before_render() {
    let source_manifest = emit_manifest("source-mismatch-source");
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(&source_manifest).expect("read manifest"))
            .expect("parse manifest");
    manifest["rows"][0]["source"] = Value::String("graph TD\nA[changed] --> B\n".to_owned());
    let mutated_manifest = temp_path("source-mismatch-mutated");
    fs::write(
        &mutated_manifest,
        serde_json::to_vec_pretty(&manifest).expect("serialize mutated manifest"),
    )
    .expect("write mutated manifest");
    let output = run_manifest(&mutated_manifest, None);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("source hash mismatch"));
    fs::remove_file(source_manifest).expect("remove source manifest");
    fs::remove_file(mutated_manifest).expect("remove mutated manifest");
}

#[test]
fn manifest_unknown_field_fails_closed() {
    let source_manifest = emit_manifest("unknown-manifest-source");
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(&source_manifest).expect("read manifest"))
            .expect("parse manifest");
    manifest["unexpected"] = Value::Bool(true);
    let mutated_manifest = temp_path("unknown-manifest-mutated");
    fs::write(
        &mutated_manifest,
        serde_json::to_vec_pretty(&manifest).expect("serialize mutated manifest"),
    )
    .expect("write mutated manifest");
    let output = run_manifest(&mutated_manifest, None);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field"));
    fs::remove_file(source_manifest).expect("remove source manifest");
    fs::remove_file(mutated_manifest).expect("remove mutated manifest");
}

#[test]
fn manifest_golden_check_never_writes_changed_target() {
    let source_manifest = emit_manifest("no-write-source");
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(&source_manifest).expect("read manifest"))
            .expect("parse manifest");
    let target = Path::new("tests/fixtures/expected/subgraph_narrow_bt_new.ascii.txt");
    assert!(!target.exists());
    manifest["rows"]
        .as_array_mut()
        .expect("manifest rows")
        .iter_mut()
        .find(|row| row["golden"].is_object())
        .expect("golden row")["golden"]["path"] =
        Value::String(target.to_string_lossy().replace('\\', "/"));
    let mutated_manifest = temp_path("no-write-mutated");
    fs::write(
        &mutated_manifest,
        serde_json::to_vec_pretty(&manifest).expect("serialize mutated manifest"),
    )
    .expect("write mutated manifest");
    let output = run_manifest(&mutated_manifest, None);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("does not match golden_stem"));
    assert!(!target.exists(), "check mode wrote a golden target");
    fs::remove_file(source_manifest).expect("remove source manifest");
    fs::remove_file(mutated_manifest).expect("remove mutated manifest");
}
