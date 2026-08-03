use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use sha2::{Digest, Sha256};

fn test_dir(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "termiflow-qa-process-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&path).expect("create process test directory");
    path
}

fn helper(root: &Path, staged: &Path, final_path: &Path) -> std::process::Output {
    Command::new(root.join("scripts/publish_receipt.sh"))
        .arg(staged)
        .arg(final_path)
        .output()
        .expect("run receipt publication helper")
}

fn qa_binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_termiflow-qa"))
}

fn sha256(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    Sha256::digest(bytes)
        .iter()
        .flat_map(|byte| {
            [
                HEX[(byte >> 4) as usize] as char,
                HEX[(byte & 0x0f) as usize] as char,
            ]
        })
        .collect()
}

fn write_review_fixture(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let packet = root.join("packet");
    let decisions = root.join("decisions.jsonl");
    let decision = root.join("decision.json");
    let frame = b"frame\n";
    let evidence = serde_json::to_vec(&json!({
        "schema": "termiflow.render_evidence.v1",
        "warnings": [],
        "errors": [],
        "critic": {"findings": []},
        "raw": {"errors": []},
        "geometry": {"errors": [], "untraced_fallback_edges": []},
        "semantic": {"owner_counts": {"Unknown": 0}}
    }))
    .expect("serialize review evidence");
    fs::create_dir_all(packet.join("frames")).expect("create review frames");
    fs::create_dir_all(packet.join("evidence")).expect("create review evidence");
    fs::write(packet.join("frames/frame.txt"), frame).expect("write review frame");
    fs::write(packet.join("evidence/evidence.json"), &evidence).expect("write review evidence");
    let frame_sha256 = sha256(frame);
    let evidence_sha256 = sha256(&evidence);
    let case_id = "review-persistence-case";
    let row = json!({
        "schema": "termiflow.visual_audit.row.v2",
        "case_id": case_id,
        "fixture": "review-fixture",
        "classification": "success",
        "style": "ascii",
        "mode": "default",
        "stdout": {"path": "frames/frame.txt", "bytes": frame.len(), "sha256": frame_sha256},
        "evidence": {"path": "evidence/evidence.json", "bytes": evidence.len(), "sha256": evidence_sha256}
    });
    fs::write(
        packet.join("manifest.jsonl"),
        format!(
            "{}\n",
            serde_json::to_string(&row).expect("serialize review row")
        ),
    )
    .expect("write review manifest");
    fs::write(packet.join("COMPLETE.json"), b"{}\n").expect("write review completion marker");

    let review = json!({
        "schema": "termiflow.visual_review.decision.v3",
        "review_kind": "perceptual",
        "case_id": case_id,
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
        "affected_homologs": [],
        "next_command": "scripts/review_visual_packet.sh --packet PACKET --decisions DECISIONS --next",
        "reviewer": "ai",
        "timestamp": "1970-01-01T00:00:00Z"
    });
    fs::write(
        &decision,
        serde_json::to_vec_pretty(&review).expect("serialize review decision"),
    )
    .expect("write review decision");
    (packet, decisions, decision)
}

#[test]
fn shell_receipt_claim_publishes_absent_file_and_removes_stage() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let directory = test_dir("publish");
    let staged = directory.join("staged.json");
    let final_path = directory.join("receipt.json");
    fs::write(&staged, b"{\"status\":\"passed\"}\n").expect("write staged receipt");

    let output = helper(&root, &staged, &final_path);
    assert!(output.status.success(), "helper failed: {output:?}");
    assert_eq!(
        fs::read(&final_path).expect("read final receipt"),
        b"{\"status\":\"passed\"}\n"
    );
    assert!(!staged.exists());
    fs::remove_dir_all(directory).expect("remove process test directory");
}

#[test]
fn shell_receipt_claim_rejects_existing_destination_without_overwrite() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let directory = test_dir("conflict");
    let staged = directory.join("staged.json");
    let final_path = directory.join("receipt.json");
    fs::write(&staged, b"new\n").expect("write staged receipt");
    fs::write(&final_path, b"old\n").expect("write existing receipt");

    let output = helper(&root, &staged, &final_path);
    assert!(
        !output.status.success(),
        "helper unexpectedly passed: {output:?}"
    );
    assert_eq!(
        fs::read(&final_path).expect("read existing receipt"),
        b"old\n"
    );
    assert_eq!(fs::read(&staged).expect("read staged receipt"), b"new\n");
    fs::remove_dir_all(directory).expect("remove process test directory");
}

#[test]
fn two_process_receipt_claims_have_one_winner() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let directory = test_dir("race");
    let first = directory.join("first.json");
    let second = directory.join("second.json");
    let final_path = directory.join("receipt.json");
    fs::write(&first, b"first\n").expect("write first staged receipt");
    fs::write(&second, b"second\n").expect("write second staged receipt");

    let mut first_child = Command::new(root.join("scripts/publish_receipt.sh"))
        .arg(&first)
        .arg(&final_path)
        .spawn()
        .expect("spawn first receipt publisher");
    let mut second_child = Command::new(root.join("scripts/publish_receipt.sh"))
        .arg(&second)
        .arg(&final_path)
        .spawn()
        .expect("spawn second receipt publisher");
    let first_status = first_child.wait().expect("wait for first publisher");
    let second_status = second_child.wait().expect("wait for second publisher");

    assert_eq!(
        usize::from(first_status.success()) + usize::from(second_status.success()),
        1,
        "exactly one publisher must claim the final path"
    );
    let final_bytes = fs::read(&final_path).expect("read claimed receipt");
    assert!(final_bytes == b"first\n" || final_bytes == b"second\n");
    fs::remove_dir_all(directory).expect("remove process test directory");
}

#[test]
fn shell_receipt_claim_rejects_cross_directory_stage() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let directory = test_dir("cross-directory");
    let other = test_dir("cross-directory-other");
    let staged = directory.join("staged.json");
    let final_path = other.join("receipt.json");
    fs::write(&staged, b"cross\n").expect("write staged receipt");

    let output = helper(&root, &staged, &final_path);
    assert!(
        !output.status.success(),
        "helper unexpectedly passed: {output:?}"
    );
    assert!(!final_path.exists());
    assert!(staged.exists());
    fs::remove_dir_all(directory).expect("remove first process test directory");
    fs::remove_dir_all(other).expect("remove second process test directory");
}

#[test]
fn two_review_processes_have_one_decision_writer() {
    let root = test_dir("review-race");
    let (packet, decisions, decision) = write_review_fixture(&root);
    let binary = env!("CARGO_BIN_EXE_termiflow-qa");
    let mut first = Command::new(binary)
        .args(["review", "--packet"])
        .arg(&packet)
        .args(["--decisions"])
        .arg(&decisions)
        .args(["--record"])
        .arg(&decision)
        .spawn()
        .expect("spawn first review writer");
    let mut second = Command::new(binary)
        .args(["review", "--packet"])
        .arg(&packet)
        .args(["--decisions"])
        .arg(&decisions)
        .args(["--record"])
        .arg(&decision)
        .spawn()
        .expect("spawn second review writer");
    let first_status = first.wait().expect("wait for first review writer");
    let second_status = second.wait().expect("wait for second review writer");

    assert_eq!(
        usize::from(first_status.success()) + usize::from(second_status.success()),
        1,
        "exactly one review writer must record the decision"
    );
    assert_eq!(
        fs::read_to_string(&decisions)
            .expect("read decision log")
            .lines()
            .count(),
        1
    );
    fs::remove_dir_all(root).expect("remove review race directory");
}

#[test]
fn malformed_review_log_and_existing_guard_fail_closed() {
    let root = test_dir("review-recovery");
    let (packet, decisions, decision) = write_review_fixture(&root);
    fs::write(&decisions, b"{partial\n").expect("write partial decision log");
    let malformed = qa_binary()
        .args(["review", "--packet"])
        .arg(&packet)
        .args(["--decisions"])
        .arg(&decisions)
        .args(["--record"])
        .arg(&decision)
        .output()
        .expect("run malformed review log");
    assert!(!malformed.status.success());
    assert!(String::from_utf8_lossy(&malformed.stderr).contains("invalid JSON"));
    assert_eq!(
        fs::read(&decisions).expect("read partial decision log"),
        b"{partial\n"
    );

    fs::remove_file(&decisions).expect("remove partial decision log");
    let guard = root.join(".decisions.jsonl.termiflow-review.lock");
    fs::write(&guard, b"manual recovery required\n").expect("write review guard");
    let guarded = qa_binary()
        .args(["review", "--packet"])
        .arg(&packet)
        .args(["--decisions"])
        .arg(&decisions)
        .args(["--record"])
        .arg(&decision)
        .output()
        .expect("run guarded review log");
    assert!(!guarded.status.success());
    assert!(String::from_utf8_lossy(&guarded.stderr).contains("persistence conflict"));
    assert!(!decisions.exists());
    assert!(guard.exists());
    fs::remove_dir_all(root).expect("remove review recovery directory");
}
