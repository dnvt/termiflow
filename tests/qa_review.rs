use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn qa_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_termiflow-qa"))
}

fn unique_temp_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("termiflow-qa-review-{label}-{nonce}"));
    fs::create_dir_all(&path).expect("create temporary review directory");
    path
}

fn sha256(bytes: &[u8]) -> String {
    hex(Sha256::digest(bytes))
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    bytes
        .as_ref()
        .iter()
        .flat_map(|byte| {
            [
                HEX[(byte >> 4) as usize] as char,
                HEX[(byte & 0x0f) as usize] as char,
            ]
        })
        .collect()
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
        "optimized": false,
        "repair_passes": 0,
        "layout_attempts": 1,
        "layout_repairs_applied": 0
    }))
    .expect("serialize evidence");

    fs::create_dir_all(root.join("frames")).expect("create frame directory");
    fs::create_dir_all(root.join("evidence")).expect("create evidence directory");
    fs::write(root.join("frames/frame.txt"), frame).expect("write frame");
    fs::write(root.join("evidence/evidence.json"), &evidence).expect("write evidence");

    let row = json!({
        "schema": "termiflow.visual_audit.row.v2",
        "case_id": "review-fixture.ascii.default",
        "fixture": "edge_kinds_lr",
        "classification": "success",
        "style": "ascii",
        "mode": "default",
        "input": "tests/fixtures/inputs/edge_kinds_lr.md",
        "dimensions": {"display": {"width": 6, "height": 1}},
        "stdout": {
            "path": "frames/frame.txt",
            "bytes": frame.len(),
            "sha256": sha256(frame)
        },
        "evidence": {
            "path": "evidence/evidence.json",
            "bytes": evidence.len(),
            "sha256": sha256(&evidence)
        },
        "findings": {"raw_errors": 0, "critic": 0, "geometry_errors": 0}
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

fn run_review(packet: &Path, decisions: &Path, args: &[&str]) -> Output {
    let mut command = qa_command();
    command
        .args(["review", "--packet"])
        .arg(packet)
        .args(["--decisions"])
        .arg(decisions)
        .args(args);
    command.output().expect("run review command")
}

fn perceptual_decision(frame_sha256: &str, evidence_sha256: &str) -> Value {
    json!({
        "schema": "termiflow.visual_review.decision.v3",
        "review_kind": "perceptual",
        "case_id": "review-fixture.ascii.default",
        "frame_sha256": frame_sha256,
        "evidence_sha256": evidence_sha256,
        "decision": "pass",
        "severity": "P3",
        "dimensions": ["semantic", "containment", "route", "text", "readability"],
        "cells": [{"x": 0, "y": 0, "note": "frame is readable"}],
        "finding": "none",
        "observation": "The complete frame is readable and visually connected.",
        "hypothesis": "No renderer defect is visible in this frame.",
        "expected_observation_if_true": "The homolog frame should preserve the same visible topology.",
        "falsifier": "A homolog or holdout frame shows a missing route, clipped label, or ambiguous endpoint.",
        "affected_homologs": ["edge_kinds_lr.unicode", "edge_kinds_rl.ascii"],
        "next_command": "scripts/review_visual_packet.sh --packet PACKET --decisions DECISIONS --next",
        "reviewer": "ai",
        "timestamp": "1970-01-01T00:00:00Z"
    })
}

#[test]
fn review_cli_keeps_full_perceptual_pass_separate_from_structural_prescreen() {
    let root = unique_temp_dir("sequence");
    let packet = root.join("packet");
    let decisions = root.join("decisions.jsonl");
    fs::create_dir_all(&packet).expect("create packet");
    let (frame_sha256, evidence_sha256) = write_packet(&packet);

    let prescreen = run_review(&packet, &decisions, &["--prescreen-clean"]);
    assert!(
        prescreen.status.success(),
        "prescreen failed: {prescreen:?}"
    );
    let prescreen_summary: Value =
        serde_json::from_slice(&prescreen.stdout).expect("parse prescreen summary");
    assert_eq!(prescreen_summary["recorded"], 1);

    let next = run_review(&packet, &decisions, &["--next"]);
    assert!(next.status.success(), "next failed: {next:?}");
    let next_payload: Value = serde_json::from_slice(&next.stdout).expect("parse next payload");
    assert_eq!(next_payload["schema"], "termiflow.visual_review.frame.v2");
    assert!(next_payload["case_id"].is_string());
    let frame = next_payload;
    assert_eq!(frame["case_id"], "review-fixture.ascii.default");
    assert_eq!(frame["decision_form"]["review_kind"], "perceptual");

    let before_review = run_review(&packet, &decisions, &["--validate"]);
    assert!(
        !before_review.status.success(),
        "validation should require perceptual review: {before_review:?}"
    );
    assert!(String::from_utf8_lossy(&before_review.stderr).contains("review coverage incomplete"));

    let removed_flag = run_review(&packet, &decisions, &["--next", "--include-structural"]);
    assert!(
        !removed_flag.status.success(),
        "removed structural opt-in unexpectedly accepted: {removed_flag:?}"
    );

    let decision_path = root.join("perceptual.json");
    let decision = perceptual_decision(&frame_sha256, &evidence_sha256);
    fs::write(
        &decision_path,
        serde_json::to_vec_pretty(&decision).expect("serialize perceptual decision"),
    )
    .expect("write perceptual decision");
    let record = run_review(
        &packet,
        &decisions,
        &[
            "--record",
            decision_path.to_str().expect("decision path is UTF-8"),
        ],
    );
    assert!(record.status.success(), "record failed: {record:?}");

    let duplicate = run_review(
        &packet,
        &decisions,
        &[
            "--record",
            decision_path.to_str().expect("decision path is UTF-8"),
        ],
    );
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("duplicate perceptual"));

    let mut stale = decision;
    stale["frame_sha256"] = Value::String("stale".to_owned());
    fs::write(
        &decision_path,
        serde_json::to_vec_pretty(&stale).expect("serialize stale decision"),
    )
    .expect("write stale decision");
    let stale_result = run_review(
        &packet,
        &decisions,
        &[
            "--record",
            decision_path.to_str().expect("decision path is UTF-8"),
        ],
    );
    assert!(!stale_result.status.success());
    assert!(String::from_utf8_lossy(&stale_result.stderr).contains("stale frame hash"));

    let complete = run_review(&packet, &decisions, &["--validate"]);
    assert!(complete.status.success(), "validation failed: {complete:?}");
    let coverage: Value = serde_json::from_slice(&complete.stdout).expect("parse coverage");
    assert_eq!(coverage["reviewed"], 1);

    let done = run_review(&packet, &decisions, &["--next"]);
    assert!(done.status.success(), "final next failed: {done:?}");
    let done_payload: Value = serde_json::from_slice(&done.stdout).expect("parse done payload");
    assert_eq!(done_payload["done"], true);

    let decision_lines = fs::read_to_string(&decisions)
        .expect("read decision log")
        .lines()
        .count();
    assert_eq!(
        decision_lines, 2,
        "duplicate or stale decision was appended"
    );
    fs::remove_dir_all(root).expect("remove temporary review packet");
}
