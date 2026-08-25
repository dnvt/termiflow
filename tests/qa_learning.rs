use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn unique_temp_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("termiflow-qa-learning-{label}-{nonce}"));
    fs::create_dir_all(&path).expect("create temporary learning directory");
    path
}

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_packet(root: &Path) -> (String, String) {
    let frame = b"frame\n";
    let evidence = serde_json::to_vec(&json!({
        "schema": "termiflow.render_evidence.v1",
        "warnings": [],
        "errors": [],
        "critic": {"findings": []},
        "raw": {"errors": []},
        "geometry": {"errors": [], "untraced_fallback_edges": []},
        "semantic": {"owner_counts": {"Unknown": 0}},
        "route_clarity": {"schema": "termiflow.route_clarity.v1", "status": "clean", "findings": []}
    }))
    .expect("serialize evidence");
    fs::create_dir_all(root.join("frames")).expect("create frame directory");
    fs::create_dir_all(root.join("evidence")).expect("create evidence directory");
    fs::write(root.join("frames/frame.txt"), frame).expect("write frame");
    fs::write(root.join("evidence/evidence.json"), &evidence).expect("write evidence");

    let row = json!({
        "schema": "termiflow.visual_audit.row.v2",
        "case_id": "learning-fixture.ascii.default",
        "fixture": "edge_branch_lr",
        "classification": "success",
        "style": "ascii",
        "mode": "default",
        "direction": "LR",
        "input": "tests/fixtures/inputs/edge_branch_lr.md",
        "stdout": {"path": "frames/frame.txt", "bytes": frame.len(), "sha256": sha256(frame)},
        "evidence": {"path": "evidence/evidence.json", "bytes": evidence.len(), "sha256": sha256(&evidence)}
    });
    fs::write(
        root.join("manifest.jsonl"),
        format!(
            "{}\n",
            serde_json::to_string(&row).expect("serialize manifest row")
        ),
    )
    .expect("write manifest");
    fs::write(root.join("COMPLETE.json"), b"{}\n").expect("write completion marker");
    (sha256(frame), sha256(&evidence))
}

fn decision(frame_sha256: &str, evidence_sha256: &str, watch_class: Option<&str>) -> Value {
    let mut value = json!({
        "schema": "termiflow.visual_review.decision.v3",
        "review_kind": "perceptual",
        "case_id": "learning-fixture.ascii.default",
        "frame_sha256": frame_sha256,
        "evidence_sha256": evidence_sha256,
        "decision": "watch",
        "severity": "P2",
        "dimensions": ["route", "readability"],
        "cells": [{"x": 0, "y": 0, "note": "the route needs a matched homolog check"}],
        "finding": "learning_watch",
        "owner_layer": "routing",
        "observation": "A human-visible route ambiguity remains in this frame.",
        "hypothesis": "The route planner may allocate a visually ambiguous receiver lane.",
        "expected_observation_if_true": "A focused homolog check will show the same receiver ambiguity.",
        "falsifier": "A matched homolog is clear and the ambiguity cannot be reproduced.",
        "affected_homologs": ["edge_branch_rl"],
        "next_command": "cargo test --locked --features qa --test qa_learning",
        "reviewer": "ai",
        "timestamp": "1970-01-01T00:00:00Z"
    });
    if let Some(class) = watch_class {
        value["watch_class"] = Value::String(class.to_owned());
    }
    value
}

fn run_learning(packet: &Path, decisions: &Path, output: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_termiflow-qa"))
        .args(["learn", "--packet"])
        .arg(packet)
        .args(["--decisions"])
        .arg(decisions)
        .args(["--output"])
        .arg(output)
        .args(["--strict"])
        .output()
        .expect("run learning command")
}

#[test]
fn learning_report_groups_typed_watch_and_rejects_unclassified_rows() {
    let root = unique_temp_dir("typed-watch");
    let packet = root.join("packet");
    fs::create_dir_all(&packet).expect("create packet");
    let (frame_sha256, evidence_sha256) = write_packet(&packet);
    let decisions = root.join("decisions.jsonl");
    fs::write(
        &decisions,
        format!(
            "{}\n",
            serde_json::to_string(&decision(
                &frame_sha256,
                &evidence_sha256,
                Some("inconclusive")
            ))
            .expect("serialize decision")
        ),
    )
    .expect("write decisions");
    let output = root.join("learning.json");
    let result = run_learning(&packet, &decisions, &output);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value =
        serde_json::from_slice(&fs::read(&output).expect("read report")).expect("parse report");
    assert_eq!(report["schema"], "termiflow.visual_learning.report.v1");
    assert_eq!(report["coverage"]["status"], "classified");
    assert_eq!(report["class_counts"]["inconclusive"], 1);
    assert_eq!(report["hypotheses"].as_array().map(Vec::len), Some(1));

    let mut templated = decision(&frame_sha256, &evidence_sha256, Some("inconclusive"));
    templated["observation"] = Value::String(
        "AI one-frame inspection of learning-fixture at 0x0: a warning remains.".to_owned(),
    );
    fs::write(
        &decisions,
        format!(
            "{}\n",
            serde_json::to_string(&templated).expect("serialize templated decision")
        ),
    )
    .expect("replace decisions with templated record");
    let rejected_template = run_learning(&packet, &decisions, &root.join("templated.json"));
    assert!(!rejected_template.status.success());
    assert!(String::from_utf8_lossy(&rejected_template.stderr).contains("templated"));

    fs::write(
        &decisions,
        format!(
            "{}\n",
            serde_json::to_string(&decision(&frame_sha256, &evidence_sha256, None))
                .expect("serialize unclassified decision")
        ),
    )
    .expect("replace decisions");
    let rejected = run_learning(&packet, &decisions, &root.join("rejected.json"));
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("watch_class"));

    let _ = fs::remove_dir_all(root);
}
