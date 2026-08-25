use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
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
        "route_clarity": {
            "schema": "termiflow.route_clarity.v1",
            "status": "not_applicable",
            "findings": []
        },
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

fn run_review_with_stdin(packet: &Path, decisions: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut command = qa_command();
    command
        .args(["review", "--packet"])
        .arg(packet)
        .args(["--decisions"])
        .arg(decisions)
        .args(args)
        .stdin(Stdio::piped());
    let mut child = command.spawn().expect("spawn review command");
    child
        .stdin
        .take()
        .expect("review stdin is piped")
        .write_all(input)
        .expect("write review decision to stdin");
    child.wait_with_output().expect("wait for review command")
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
        "watch_class": "not_applicable",
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

fn write_prior_decision(path: &Path, frame_sha256: &str, evidence_sha256: &str) {
    let mut decision = perceptual_decision(frame_sha256, evidence_sha256);
    decision["policy_sha256"] = Value::String(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
    );
    fs::write(
        path,
        format!(
            "{}\n",
            serde_json::to_string(&decision).expect("serialize prior decision")
        ),
    )
    .expect("write prior decision");
}

fn write_manifest_packet(root: &Path, rows: &[Value]) {
    fs::create_dir_all(root.join("frames")).expect("create packet frames");
    fs::create_dir_all(root.join("evidence")).expect("create packet evidence");
    let manifest = rows
        .iter()
        .map(|row| serde_json::to_string(row).expect("serialize manifest row"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(root.join("manifest.jsonl"), format!("{manifest}\n")).expect("write packet manifest");
    fs::write(root.join("COMPLETE.json"), b"{}\n").expect("write packet completion marker");
}

fn review_row(
    case_id: &str,
    classification: &str,
    frame_path: &str,
    frame: &[u8],
    evidence_path: &str,
    evidence: &[u8],
) -> Value {
    let mut row = json!({
        "schema": "termiflow.visual_audit.row.v2",
        "case_id": case_id,
        "fixture": "edge_kinds_lr",
        "classification": classification,
        "style": "ascii",
        "mode": "default",
        "direction": "LR",
        "input": "tests/fixtures/inputs/edge_kinds_lr.md",
        "stdout": {
            "path": frame_path,
            "bytes": frame.len(),
            "sha256": sha256(frame)
        },
        "evidence": {
            "path": evidence_path,
            "bytes": evidence.len(),
            "sha256": sha256(evidence)
        },
        "policy": {"sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}
    });
    if classification == "expected_error" {
        row["evidence"] = Value::Null;
        row["policy"] = Value::Null;
    }
    row
}

#[test]
fn review_rebinds_only_exact_renderable_rows_and_preserves_history() {
    let root = unique_temp_dir("rebind");
    let prior_packet = root.join("prior-packet");
    let current_packet = root.join("current-packet");
    fs::create_dir_all(&prior_packet).expect("create prior packet");
    fs::create_dir_all(&current_packet).expect("create current packet");
    let (frame_sha256, evidence_sha256) = write_packet(&prior_packet);
    fs::create_dir_all(current_packet.join("frames")).expect("create current frames");
    fs::create_dir_all(current_packet.join("evidence")).expect("create current evidence");
    fs::write(current_packet.join("frames/frame.txt"), b"frame\n").expect("write current frame");
    let current_evidence =
        fs::read(prior_packet.join("evidence/evidence.json")).expect("read prior evidence");
    fs::write(
        current_packet.join("evidence/evidence.json"),
        &current_evidence,
    )
    .expect("write current evidence");
    let current_row = json!({
        "schema": "termiflow.visual_audit.row.v2",
        "case_id": "review-fixture.ascii.default",
        "fixture": "edge_kinds_lr",
        "classification": "success",
        "style": "ascii",
        "mode": "default",
        "direction": "LR",
        "input": "tests/fixtures/inputs/edge_kinds_lr.md",
        "stdout": {"path": "frames/frame.txt", "bytes": 6, "sha256": frame_sha256},
        "evidence": {"path": "evidence/evidence.json", "bytes": current_evidence.len(), "sha256": evidence_sha256},
        "policy": {"sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}
    });
    let prior_manifest =
        fs::read_to_string(prior_packet.join("manifest.jsonl")).expect("read prior manifest");
    let mut prior_value: Value =
        serde_json::from_str(prior_manifest.lines().next().unwrap()).expect("parse prior row");
    prior_value["direction"] = Value::String("LR".to_owned());
    prior_value["policy"] =
        json!({"sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"});
    fs::write(
        prior_packet.join("manifest.jsonl"),
        format!(
            "{}\n",
            serde_json::to_string(&prior_value).expect("serialize prior row")
        ),
    )
    .expect("rewrite prior manifest");
    fs::write(
        current_packet.join("manifest.jsonl"),
        format!(
            "{}\n",
            serde_json::to_string(&current_row).expect("serialize current row")
        ),
    )
    .expect("write current manifest");
    fs::write(prior_packet.join("COMPLETE.json"), b"{}\n").expect("prior complete");
    fs::write(current_packet.join("COMPLETE.json"), b"{}\n").expect("current complete");

    let prior_decisions = root.join("prior-decisions.jsonl");
    write_prior_decision(&prior_decisions, &frame_sha256, &evidence_sha256);
    let mut legacy_decision: Value = serde_json::from_str(
        fs::read_to_string(&prior_decisions)
            .expect("read legacy prior decision")
            .lines()
            .next()
            .expect("legacy prior decision line"),
    )
    .expect("parse legacy prior decision");
    legacy_decision["decision"] = Value::String("watch".to_owned());
    legacy_decision["severity"] = Value::String("P2".to_owned());
    legacy_decision["watch_class"] = Value::String("topology_ambiguous".to_owned());
    legacy_decision["finding"] = Value::String("legacy-topology-watch".to_owned());
    legacy_decision["cells"] = json!([{"x": 0, "y": 0, "note": "legacy route ownership watch"}]);
    legacy_decision["next_command"] = Value::String(
        "scripts/review_visual_packet.sh --packet stale --decisions stale --fresh --next"
            .to_owned(),
    );
    legacy_decision["hypothesis"] = Value::String(
        "The route remains ambiguous. The fresh packet should retain this watch across its homologs. The fresh packet should retain this watch across its homologs."
            .to_owned(),
    );
    legacy_decision
        .as_object_mut()
        .expect("legacy decision object")
        .remove("owner_layer");
    fs::write(
        &prior_decisions,
        format!(
            "{}\n",
            serde_json::to_string(&legacy_decision).expect("serialize legacy prior decision")
        ),
    )
    .expect("write legacy prior decision");
    let current_decisions = root.join("current-decisions.jsonl");
    let result = run_review(
        &current_packet,
        &current_decisions,
        &[
            "--rebind-from-packet",
            prior_packet.to_str().expect("prior packet path is UTF-8"),
            "--rebind-from-decisions",
            prior_decisions
                .to_str()
                .expect("prior decision path is UTF-8"),
        ],
    );
    assert!(result.status.success(), "rebind failed: {result:?}");
    let summary: Value = serde_json::from_slice(&result.stdout).expect("parse rebind summary");
    assert_eq!(summary["rebound"], 1);
    assert_eq!(summary["legacy_owner_layer_filled"], 1);
    let rebound: Value = serde_json::from_str(
        fs::read_to_string(&current_decisions)
            .expect("read rebound decision")
            .lines()
            .next()
            .unwrap(),
    )
    .expect("parse rebound decision");
    assert_eq!(
        rebound["carry_forward"]["schema"],
        "termiflow.visual_review.carry_forward.v1"
    );
    assert_eq!(rebound["decision"], "watch");
    assert_eq!(rebound["frame_sha256"], frame_sha256);
    assert_eq!(rebound["owner_layer"], "reviewer_calibration");
    assert_eq!(
        rebound["next_command"],
        format!(
            "scripts/review_visual_packet.sh --packet {} --decisions {} --next",
            current_packet.display(),
            current_decisions.display()
        )
    );
    assert_eq!(
        rebound["hypothesis"],
        "The route remains ambiguous. The fresh packet should retain this watch across its homologs."
    );
    assert_eq!(
        rebound["carry_forward"]["owner_layer_provenance"],
        "legacy decision lacked owner_layer; rebound as reviewer_calibration for current-epoch learning"
    );

    fs::remove_dir_all(root).expect("remove temporary rebind packets");
}

#[test]
fn review_rebind_rejects_changed_hashes_and_non_success_rows() {
    let root = unique_temp_dir("rebind-boundaries");
    let prior_packet = root.join("prior-packet");
    let current_packet = root.join("current-packet");
    let same_frame = b"same-frame\n";
    let old_frame = b"old-frame\n";
    let new_frame = b"new-frame\n";
    let stable_frame = b"stable-frame\n";
    let same_evidence = br#"{"v":"same"}"#;
    let old_frame_evidence = br#"{"v":"old-frame"}"#;
    let old_evidence = br#"{"v":"old-evidence"}"#;
    let new_evidence = br#"{"v":"new-evidence"}"#;

    for packet in [&prior_packet, &current_packet] {
        fs::create_dir_all(packet.join("frames")).expect("create packet frames");
        fs::create_dir_all(packet.join("evidence")).expect("create packet evidence");
    }
    fs::write(prior_packet.join("frames/same.txt"), same_frame).expect("write same prior frame");
    fs::write(prior_packet.join("frames/old.txt"), old_frame).expect("write old frame");
    fs::write(prior_packet.join("frames/stable.txt"), stable_frame)
        .expect("write stable prior frame");
    fs::write(prior_packet.join("evidence/same.json"), same_evidence)
        .expect("write same prior evidence");
    fs::write(prior_packet.join("evidence/frame.json"), old_frame_evidence)
        .expect("write old frame evidence");
    fs::write(prior_packet.join("evidence/old.json"), old_evidence).expect("write old evidence");

    fs::write(current_packet.join("frames/same.txt"), same_frame).expect("write same frame");
    fs::write(current_packet.join("frames/new.txt"), new_frame).expect("write new frame");
    fs::write(current_packet.join("frames/stable.txt"), stable_frame)
        .expect("write stable current frame");
    fs::write(current_packet.join("evidence/same.json"), same_evidence)
        .expect("write same evidence");
    fs::write(
        current_packet.join("evidence/frame.json"),
        old_frame_evidence,
    )
    .expect("write current frame evidence");
    fs::write(current_packet.join("evidence/new.json"), new_evidence).expect("write new evidence");

    let prior_rows = vec![
        review_row(
            "review-same.ascii.default",
            "success",
            "frames/same.txt",
            same_frame,
            "evidence/same.json",
            same_evidence,
        ),
        review_row(
            "review-frame-change.ascii.default",
            "success",
            "frames/old.txt",
            old_frame,
            "evidence/frame.json",
            old_frame_evidence,
        ),
        review_row(
            "review-evidence-change.ascii.default",
            "success",
            "frames/stable.txt",
            stable_frame,
            "evidence/old.json",
            old_evidence,
        ),
        review_row(
            "review-warning.ascii.default",
            "warning",
            "frames/same.txt",
            same_frame,
            "evidence/same.json",
            same_evidence,
        ),
    ];
    let current_rows = vec![
        review_row(
            "review-same.ascii.default",
            "success",
            "frames/same.txt",
            same_frame,
            "evidence/same.json",
            same_evidence,
        ),
        review_row(
            "review-frame-change.ascii.default",
            "success",
            "frames/new.txt",
            new_frame,
            "evidence/frame.json",
            old_frame_evidence,
        ),
        review_row(
            "review-evidence-change.ascii.default",
            "success",
            "frames/stable.txt",
            stable_frame,
            "evidence/new.json",
            new_evidence,
        ),
        review_row(
            "review-warning.ascii.default",
            "warning",
            "frames/same.txt",
            same_frame,
            "evidence/same.json",
            same_evidence,
        ),
        review_row(
            "review-expected-error.ascii.default",
            "expected_error",
            "frames/same.txt",
            same_frame,
            "evidence/same.json",
            same_evidence,
        ),
    ];
    write_manifest_packet(&prior_packet, &prior_rows);
    write_manifest_packet(&current_packet, &current_rows);

    let mut prior_decisions = Vec::new();
    for (case_id, frame, evidence) in [
        (
            "review-same.ascii.default",
            &same_frame[..],
            &same_evidence[..],
        ),
        (
            "review-frame-change.ascii.default",
            &old_frame[..],
            &old_frame_evidence[..],
        ),
        (
            "review-evidence-change.ascii.default",
            &stable_frame[..],
            &old_evidence[..],
        ),
        (
            "review-warning.ascii.default",
            &same_frame[..],
            &same_evidence[..],
        ),
    ] {
        let mut decision = perceptual_decision(&sha256(frame), &sha256(evidence));
        decision["case_id"] = Value::String(case_id.to_owned());
        decision["policy_sha256"] = Value::String(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        );
        prior_decisions.push(serde_json::to_string(&decision).expect("serialize prior decision"));
    }
    let prior_decisions_path = root.join("prior-decisions.jsonl");
    fs::write(
        &prior_decisions_path,
        format!("{}\n", prior_decisions.join("\n")),
    )
    .expect("write prior decisions");

    let current_decisions = root.join("current-decisions.jsonl");
    let result = run_review(
        &current_packet,
        &current_decisions,
        &[
            "--rebind-from-packet",
            prior_packet.to_str().expect("prior packet path is UTF-8"),
            "--rebind-from-decisions",
            prior_decisions_path
                .to_str()
                .expect("prior decisions path is UTF-8"),
        ],
    );
    assert!(result.status.success(), "rebind failed: {result:?}");
    let summary: Value = serde_json::from_slice(&result.stdout).expect("parse rebind summary");
    assert_eq!(summary["rebound"], 2);
    assert_eq!(summary["rebound_warning"], 1);
    assert_eq!(summary["skipped_changed"], 2);
    assert_eq!(summary["skipped_missing_history"], 0);
    assert_eq!(summary["skipped_without_perceptual"], 0);
    let lines = fs::read_to_string(&current_decisions)
        .expect("read current decisions")
        .lines()
        .count();
    assert_eq!(lines, 2, "only exact renderable rows may be rebound");

    fs::remove_dir_all(root).expect("remove temporary boundary packets");
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
    assert_eq!(
        frame["route_clarity"]["schema"],
        "termiflow.route_clarity.v1"
    );
    assert_eq!(
        frame["review_rubric"]["schema"],
        "termiflow.visual_review.rubric.v1"
    );
    assert_eq!(
        frame["review_rubric"]["machine_evidence_is_triage_only"],
        true
    );
    assert_eq!(
        frame["review_rubric"]["watch_or_fail_requires_exact_cells"],
        true
    );

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
    let record = run_review_with_stdin(
        &packet,
        &decisions,
        &["--record", "-"],
        &serde_json::to_vec(&decision).expect("serialize stdin perceptual decision"),
    );
    assert!(record.status.success(), "stdin record failed: {record:?}");

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

#[test]
fn fresh_review_rejects_carry_forward_and_machine_decisions() {
    let root = unique_temp_dir("fresh-guard");
    let packet = root.join("packet");
    let decisions = root.join("decisions.jsonl");
    fs::create_dir_all(&packet).expect("create packet");
    let (frame_sha256, evidence_sha256) = write_packet(&packet);

    let mut carried = perceptual_decision(&frame_sha256, &evidence_sha256);
    carried["carry_forward"] = json!({
        "schema": "termiflow.visual_review.carry_forward.v1",
        "reason": "exact packet identity"
    });
    fs::write(
        &decisions,
        format!(
            "{}\n",
            serde_json::to_string(&carried).expect("serialize carried decision")
        ),
    )
    .expect("write carried decision");
    let rejected_carry = run_review(&packet, &decisions, &["--fresh", "--validate"]);
    assert!(!rejected_carry.status.success());
    assert!(String::from_utf8_lossy(&rejected_carry.stderr).contains("carry-forward"));

    fs::remove_file(&decisions).expect("remove carried decision");
    let prescreen = run_review(&packet, &decisions, &["--prescreen-clean"]);
    assert!(
        prescreen.status.success(),
        "prescreen failed: {prescreen:?}"
    );
    let rejected_machine = run_review(&packet, &decisions, &["--fresh", "--validate"]);
    assert!(!rejected_machine.status.success());
    assert!(String::from_utf8_lossy(&rejected_machine.stderr).contains("machine structural"));

    fs::remove_file(&decisions).expect("remove machine decision");
    let next = run_review(&packet, &decisions, &["--fresh", "--next"]);
    assert!(next.status.success(), "fresh next failed: {next:?}");
    let decision_path = root.join("fresh-decision.json");
    let decision = perceptual_decision(&frame_sha256, &evidence_sha256);
    fs::write(
        &decision_path,
        serde_json::to_vec_pretty(&decision).expect("serialize fresh decision"),
    )
    .expect("write fresh decision");
    let recorded = run_review(
        &packet,
        &decisions,
        &[
            "--fresh",
            "--record",
            decision_path.to_str().expect("decision path is UTF-8"),
        ],
    );
    assert!(
        recorded.status.success(),
        "fresh record failed: {recorded:?}"
    );
    let complete = run_review(&packet, &decisions, &["--fresh", "--validate"]);
    assert!(
        complete.status.success(),
        "fresh validation failed: {complete:?}"
    );

    fs::remove_dir_all(root).expect("remove temporary review packet");
}

#[test]
fn visual_history_is_prioritized_and_guards_an_unqualified_pass() {
    let root = unique_temp_dir("history");
    let packet = root.join("packet");
    let decisions = root.join("decisions.jsonl");
    let history = root.join("history.jsonl");
    fs::create_dir_all(&packet).expect("create packet");
    let (frame_sha256, evidence_sha256) = write_packet(&packet);
    fs::write(
        &history,
        format!(
            "{}\n",
            serde_json::to_string(&json!({
                "schema": "termiflow.visual_review.history_record.v1",
                "history_id": "historical-shared-target",
                "status": "open",
                "severity": "P2",
                "selector": {"case_id": "review-fixture.ascii.default"},
                "dimensions": ["semantic", "route"],
                "observation": "the shared target entry may collapse",
                "hypothesis": "target ownership is ambiguous",
                "expected_observation_if_true": "homologs show distinct entries",
                "falsifier": "all homologs preserve distinct entries",
                "affected_homologs": ["review-fixture.unicode.default"],
                "next_command": "cargo test --test focused",
                "created_at": "2026-08-10T00:00:00Z"
            }))
            .expect("serialize history")
        ),
    )
    .expect("write history");

    let next = run_review(
        &packet,
        &decisions,
        &[
            "--history",
            history.to_str().expect("history path is UTF-8"),
            "--next",
        ],
    );
    assert!(next.status.success(), "history next failed: {next:?}");
    let next_payload: Value = serde_json::from_slice(&next.stdout).expect("parse history frame");
    assert_eq!(
        next_payload["history"]["unresolved_open_history_ids"][0],
        "historical-shared-target"
    );

    let decision_path = root.join("decision.json");
    let decision = perceptual_decision(&frame_sha256, &evidence_sha256);
    fs::write(
        &decision_path,
        serde_json::to_vec_pretty(&decision).expect("serialize decision"),
    )
    .expect("write decision");
    let rejected = run_review(
        &packet,
        &decisions,
        &[
            "--history",
            history.to_str().expect("history path is UTF-8"),
            "--record",
            decision_path.to_str().expect("decision path is UTF-8"),
        ],
    );
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("unresolved visual history"));

    let mut repaired = decision;
    repaired["history_resolution"] = json!({
        "schema": "termiflow.visual_review.history_resolution.v1",
        "status": "repaired",
        "history_ids": ["historical-shared-target"],
        "note": "the target and its homolog were rechecked after the focused repair"
    });
    fs::write(
        &decision_path,
        serde_json::to_vec_pretty(&repaired).expect("serialize repaired decision"),
    )
    .expect("write repaired decision");
    let recorded = run_review(
        &packet,
        &decisions,
        &[
            "--history",
            history.to_str().expect("history path is UTF-8"),
            "--record",
            decision_path.to_str().expect("decision path is UTF-8"),
        ],
    );
    assert!(
        recorded.status.success(),
        "repaired record failed: {recorded:?}"
    );

    let complete = run_review(
        &packet,
        &decisions,
        &[
            "--history",
            history.to_str().expect("history path is UTF-8"),
            "--validate",
        ],
    );
    assert!(
        complete.status.success(),
        "history validation failed: {complete:?}"
    );
    let coverage: Value = serde_json::from_slice(&complete.stdout).expect("parse history coverage");
    assert_eq!(coverage["history_open_unresolved"], json!([]));

    fs::remove_dir_all(root).expect("remove temporary history packet");
}
