use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

fn minimal_visual_fixture(root: &Path) -> (PathBuf, PathBuf) {
    let input_root = root.join("inputs");
    let metadata_path = root.join("metadata.json");
    fs::create_dir_all(&input_root).expect("create minimal visual fixture input root");
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/inputs/flow_simple_td.md"),
        input_root.join("flow_simple_td.md"),
    )
    .expect("copy minimal visual fixture");
    let mut metadata: serde_json::Value = serde_json::from_slice(
        &fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/metadata.json"))
            .expect("read fixture metadata"),
    )
    .expect("parse fixture metadata");
    let fixtures = metadata["fixtures"]
        .as_array()
        .expect("fixture metadata array")
        .iter()
        .filter(|fixture| fixture["name"] == "flow_simple_td")
        .cloned()
        .collect();
    metadata["fixtures"] = serde_json::Value::Array(fixtures);
    fs::write(
        &metadata_path,
        serde_json::to_vec_pretty(&metadata).expect("serialize minimal fixture metadata"),
    )
    .expect("write minimal fixture metadata");
    (input_root, metadata_path)
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
fn visual_packet_process_death_matrix_preserves_final_path_and_recovery_state() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let renderer = env!("CARGO_BIN_EXE_termiflow");
    let fixture_directory = test_dir("visual-process-fixture");
    let (input_root, metadata_path) = minimal_visual_fixture(&fixture_directory);
    let input_root_arg = input_root.to_str().expect("minimal input root path");
    let metadata_arg = metadata_path.to_str().expect("minimal metadata path");
    for point in [
        "stage-created",
        "writing",
        "ready",
        "before-publish",
        "after-publish",
    ] {
        let directory = test_dir(&format!("visual-process-death-{point}"));
        let packet = directory.join("packet");
        let marker = directory.join("pause.json");
        let mut child = qa_binary()
            .current_dir(&root)
            .args([
                "visual-audit",
                "--out",
                packet.to_str().expect("packet path"),
                "--input-root",
                input_root_arg,
                "--metadata",
                metadata_arg,
                "--styles",
                "ascii",
                "--modes",
                "default",
                "--binary",
                renderer,
                "--pause-at",
                point,
                "--pause-marker",
                marker.to_str().expect("marker path"),
            ])
            .spawn()
            .expect("spawn paused visual audit");

        let mut observed = false;
        for _ in 0..1200 {
            if marker.is_file() {
                observed = true;
                break;
            }
            if let Some(status) = child.try_wait().expect("poll paused visual audit") {
                let output = child
                    .wait_with_output()
                    .expect("read early-exit visual audit output");
                panic!(
                    "paused visual audit at {point} exited early: {status}; stdout={}; stderr={}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        if !observed {
            let _ = child.kill();
            let output = child
                .wait_with_output()
                .expect("read missing-marker visual audit output");
            panic!(
                "pause marker was not observed at {point}; stdout={}; stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let final_visible = packet.is_dir();
        assert_eq!(
            final_visible,
            point == "after-publish",
            "final path visibility is wrong at {point}"
        );
        let stage = fs::read_dir(&directory)
            .expect("read process-death directory")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(".packet.termiflow-stage-"))
            });
        if point == "after-publish" {
            let state: serde_json::Value = serde_json::from_slice(
                &fs::read(packet.join("run_state.json")).expect("read published run state"),
            )
            .expect("parse published run state");
            assert_eq!(state["schema"], "termiflow.run_state.v2");
            assert_eq!(state["state"], "published");
            assert!(packet.join("COMPLETE.json").is_file());
            assert!(stage.is_none(), "post-publish pause must not leave a stage");
        } else if point == "stage-created" {
            assert!(stage.is_some(), "stage-created pause must leave a stage");
            assert!(
                !stage
                    .as_ref()
                    .expect("stage exists")
                    .join("run_state.json")
                    .exists(),
                "stage-created pause must precede owner-state creation"
            );
        } else {
            let stage = stage.expect("private stage directory");
            let state: serde_json::Value = serde_json::from_slice(
                &fs::read(stage.join("run_state.json")).expect("read private run state"),
            )
            .expect("parse private run state");
            assert_eq!(state["schema"], "termiflow.run_state.v2");
            assert_eq!(
                state["state"],
                if point == "ready" || point == "before-publish" {
                    "ready"
                } else {
                    "writing"
                }
            );
        }

        child.kill().expect("kill paused visual audit");
        child.wait().expect("wait for killed visual audit");

        if point == "after-publish" {
            let retry = qa_binary()
                .current_dir(&root)
                .args([
                    "visual-audit",
                    "--out",
                    packet.to_str().expect("packet path"),
                    "--input-root",
                    input_root_arg,
                    "--metadata",
                    metadata_arg,
                    "--styles",
                    "ascii",
                    "--modes",
                    "default",
                    "--binary",
                    renderer,
                ])
                .output()
                .expect("retry published visual audit");
            assert!(
                !retry.status.success(),
                "retry must not republish a final packet"
            );
            assert!(String::from_utf8_lossy(&retry.stderr).contains("already exists"));
        } else if point == "stage-created" {
            let retry = qa_binary()
                .current_dir(&root)
                .args([
                    "visual-audit",
                    "--out",
                    packet.to_str().expect("packet path"),
                    "--input-root",
                    input_root_arg,
                    "--metadata",
                    metadata_arg,
                    "--styles",
                    "ascii",
                    "--modes",
                    "default",
                    "--binary",
                    renderer,
                ])
                .output()
                .expect("retry missing-state visual audit");
            assert!(
                !retry.status.success(),
                "missing owner state must require recovery"
            );
            assert!(String::from_utf8_lossy(&retry.stderr).contains("owner state"));
        } else {
            let retry = qa_binary()
                .current_dir(&root)
                .args([
                    "visual-audit",
                    "--out",
                    packet.to_str().expect("packet path"),
                    "--input-root",
                    input_root_arg,
                    "--metadata",
                    metadata_arg,
                    "--styles",
                    "ascii",
                    "--modes",
                    "default",
                    "--binary",
                    renderer,
                ])
                .output()
                .expect("retry staged visual audit");
            assert!(retry.status.success(), "retry failed at {point}: {retry:?}");
            assert!(
                packet.is_dir(),
                "retry must publish a complete final packet"
            );
            assert!(
                fs::read_dir(&directory)
                    .expect("read recovery directory")
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .any(|path| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.starts_with(".packet.termiflow-recovery-"))
                    }),
                "dead staged owner must be quarantined at {point}"
            );
        }
        fs::remove_dir_all(directory).expect("remove process test directory");
    }
    fs::remove_dir_all(fixture_directory).expect("remove minimal visual fixture directory");
}

#[test]
fn two_process_visual_packet_claims_have_one_winner() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let renderer = env!("CARGO_BIN_EXE_termiflow");
    let fixture_directory = test_dir("visual-race-fixture");
    let (input_root, metadata_path) = minimal_visual_fixture(&fixture_directory);
    let directory = test_dir("visual-race");
    let packet = directory.join("packet");
    let input_root_arg = input_root.to_str().expect("race input root path");
    let metadata_arg = metadata_path.to_str().expect("race metadata path");
    let packet_arg = packet.to_str().expect("race packet path");
    let spawn = || {
        qa_binary()
            .current_dir(&root)
            .args([
                "visual-audit",
                "--out",
                packet_arg,
                "--input-root",
                input_root_arg,
                "--metadata",
                metadata_arg,
                "--styles",
                "ascii",
                "--modes",
                "default",
                "--binary",
                renderer,
            ])
            .spawn()
            .expect("spawn concurrent visual audit")
    };
    let first = spawn();
    let second = spawn();
    let first = first
        .wait_with_output()
        .expect("read first visual race output");
    let second = second
        .wait_with_output()
        .expect("read second visual race output");
    assert_eq!(
        usize::from(first.status.success()) + usize::from(second.status.success()),
        1,
        "exactly one visual packet publisher must claim the final path; first={first:?}; second={second:?}"
    );
    assert!(packet.join("COMPLETE.json").is_file());
    let state: serde_json::Value = serde_json::from_slice(
        &fs::read(packet.join("run_state.json")).expect("read race published run state"),
    )
    .expect("parse race published run state");
    assert_eq!(state["state"], "published");
    fs::remove_dir_all(directory).expect("remove visual race directory");
    fs::remove_dir_all(fixture_directory).expect("remove visual race fixture directory");
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
