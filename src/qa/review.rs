use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use super::common;

pub const DECISION_SCHEMA: &str = "termiflow.visual_review.decision.v3";
const FRAME_SCHEMA: &str = "termiflow.visual_review.frame.v2";
const COVERAGE_SCHEMA: &str = "termiflow.visual_review.coverage.v1";
const STRUCTURAL_PRESCREEN: &str = "structural_prescreen";
const PERCEPTUAL_REVIEW: &str = "perceptual";
const DECISIONS: &[&str] = &["pass", "fail", "watch", "unclear"];
const SEVERITIES: &[&str] = &["P0", "P1", "P2", "P3"];
const DIMENSIONS: &[&str] = &["semantic", "containment", "route", "text", "readability"];
const REVIEWERS: &[&str] = &["ai", "human", "machine"];

#[derive(Debug, Default)]
struct DecisionState {
    structural: Option<Value>,
    perceptual: Option<Value>,
}

type DecisionMap = BTreeMap<String, DecisionState>;

impl DecisionState {
    fn contains(&self, kind: &str) -> bool {
        match kind {
            STRUCTURAL_PRESCREEN => self.structural.is_some(),
            PERCEPTUAL_REVIEW => self.perceptual.is_some(),
            _ => false,
        }
    }

    fn insert(&mut self, kind: &str, case_id: &str, decision: Value) -> Result<()> {
        let slot = match kind {
            STRUCTURAL_PRESCREEN => &mut self.structural,
            PERCEPTUAL_REVIEW => &mut self.perceptual,
            _ => bail!("unsupported review kind for {case_id}: {kind}"),
        };
        if slot.is_some() {
            bail!("duplicate {kind} decision for case_id: {case_id}");
        }
        *slot = Some(decision);
        Ok(())
    }
}

#[derive(Debug)]
pub struct ReviewArgs {
    pub packet: PathBuf,
    pub decisions: PathBuf,
    pub fixture: Option<String>,
    pub style: Option<String>,
    pub mode: Option<String>,
    pub reviewer: String,
    pub next: bool,
    pub record: Option<PathBuf>,
    pub prescreen_clean: bool,
    pub validate: bool,
}

pub fn run(args: ReviewArgs) -> Result<()> {
    let root = std::env::current_dir().context("resolve repository root")?;
    let packet = resolve(&root, &args.packet);
    let rows = load_manifest(&packet)?;
    let decisions_path = resolve_decision_path(&root, &args.decisions);
    let decisions = load_decisions(&decisions_path, &rows)?;

    if let Some(record_path) = args.record {
        let decision = common::load_json(&resolve(&root, &record_path), "review decision")?;
        validate_decision(&decision, &rows)?;
        let case_id = non_empty_string(decision.get("case_id"), "decision case_id")?;
        let kind = review_kind(&decision)?;
        if decisions
            .get(&case_id)
            .is_some_and(|state| state.contains(kind))
        {
            bail!("duplicate {kind} decision for case_id: {case_id}");
        }
        append_decision(&decisions_path, &decision)?;
        println!("{case_id}");
        return Ok(());
    }
    if args.prescreen_clean {
        println!(
            "{}",
            prescreen_clean(&args, &packet, &rows, &decisions, &decisions_path)?
        );
        return Ok(());
    }
    if args.validate {
        let selected = filtered_rows(&args, &rows);
        let mut missing = Vec::new();
        for row in &selected {
            let Some(case_id) = row["case_id"].as_str() else {
                missing.push("<missing-case-id>".to_owned());
                continue;
            };
            let covered = decisions.get(case_id).is_some_and(covers_row);
            if !covered {
                missing.push(case_id.to_owned());
            }
        }
        if let Some(first) = missing.first() {
            bail!(
                "review coverage incomplete: {} case(s) missing; first={first}",
                missing.len()
            );
        }
        println!(
            "{}",
            json!({ "schema": COVERAGE_SCHEMA, "reviewed": selected.len() })
        );
        return Ok(());
    }
    if args.next {
        let selected = selected_rows(&args, &rows, &decisions);
        if let Some(row) = selected.first() {
            println!("{}", frame_payload(&root, &packet, row)?);
        } else {
            println!("{}", json!({ "schema": FRAME_SCHEMA, "done": true }));
        }
        return Ok(());
    }
    let _ = args.reviewer;
    bail!("use --next, --record PATH, or --validate for deterministic one-frame review")
}

fn resolve(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn resolve_decision_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn load_manifest(packet: &Path) -> Result<BTreeMap<String, Value>> {
    if !packet.join("COMPLETE.json").is_file() {
        bail!(
            "missing completion marker: {}",
            packet.join("COMPLETE.json").display()
        );
    }
    let bytes = common::require_file(&packet.join("manifest.jsonl"), "manifest")?;
    let text = String::from_utf8(bytes).context("manifest is not UTF-8")?;
    let mut rows = BTreeMap::new();
    for (number, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let row: Value = serde_json::from_str(line)
            .with_context(|| format!("manifest line {} is invalid JSON", number + 1))?;
        let case_id = non_empty_string(row.get("case_id"), "manifest case_id")?;
        if rows.contains_key(&case_id) {
            bail!("duplicate manifest case_id: {case_id}");
        }
        let stdout = row
            .get("stdout")
            .ok_or_else(|| anyhow!("manifest row {case_id} has no stdout record"))?;
        let object = stdout
            .as_object()
            .ok_or_else(|| anyhow!("manifest row {case_id} has invalid stdout record"))?;
        let path = non_empty_string(
            object.get("path"),
            &format!("manifest row {case_id} stdout path"),
        )?;
        let expected_hash = non_empty_string(
            object.get("sha256"),
            &format!("manifest row {case_id} stdout hash"),
        )?;
        let frame = common::safe_relative_path(Path::new(&path), packet, "manifest stdout path")?;
        let actual_hash = common::sha256_file(&frame)?;
        if actual_hash != expected_hash {
            bail!("frame hash mismatch for {case_id}: {actual_hash} != {expected_hash}");
        }
        rows.insert(case_id, row);
    }
    Ok(rows)
}

fn load_decisions(path: &Path, rows: &BTreeMap<String, Value>) -> Result<DecisionMap> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let bytes = common::require_file(path, "decision log")?;
    let text = String::from_utf8(bytes).context("decision log is not UTF-8")?;
    let mut decisions = DecisionMap::new();
    for (number, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let decision: Value = serde_json::from_str(line)
            .with_context(|| format!("decision line {} is invalid JSON", number + 1))?;
        validate_decision(&decision, rows)?;
        let case_id = non_empty_string(decision.get("case_id"), "decision case_id")?;
        let kind = review_kind(&decision)?;
        decisions
            .entry(case_id.clone())
            .or_default()
            .insert(kind, &case_id, decision)?;
    }
    Ok(decisions)
}

fn validate_decision(decision: &Value, rows: &BTreeMap<String, Value>) -> Result<()> {
    if decision["schema"].as_str() != Some(DECISION_SCHEMA) {
        bail!("decision schema must be {DECISION_SCHEMA}");
    }
    let case_id = non_empty_string(decision.get("case_id"), "decision case_id")?;
    let row = rows
        .get(&case_id)
        .ok_or_else(|| anyhow!("decision references unknown case_id: {case_id}"))?;
    if decision["frame_sha256"] != row["stdout"]["sha256"] {
        bail!("stale frame hash for {case_id}; regenerate the next frame");
    }
    let evidence_hash = row
        .get("evidence")
        .and_then(|value| value["sha256"].as_str());
    if decision["evidence_sha256"].as_str() != evidence_hash {
        bail!("stale evidence hash for {case_id}; regenerate the next frame");
    }
    review_kind(decision)?;
    if !DECISIONS.contains(&decision["decision"].as_str().unwrap_or_default()) {
        bail!("invalid decision for {case_id}");
    }
    if !SEVERITIES.contains(&decision["severity"].as_str().unwrap_or_default()) {
        bail!("invalid severity for {case_id}");
    }
    let dimensions = decision["dimensions"]
        .as_array()
        .ok_or_else(|| anyhow!("dimensions must be a non-empty list for {case_id}"))?;
    let dimension_names: std::collections::BTreeSet<&str> =
        dimensions.iter().filter_map(Value::as_str).collect();
    if dimensions.is_empty()
        || dimension_names.len() != dimensions.len()
        || dimensions
            .iter()
            .any(|value| !DIMENSIONS.contains(&value.as_str().unwrap_or_default()))
    {
        bail!("invalid dimensions for {case_id}");
    }
    let cells = decision["cells"]
        .as_array()
        .ok_or_else(|| anyhow!("cells must be a list for {case_id}"))?;
    for cell in cells {
        if cell["x"].as_i64().is_none()
            || cell["y"].as_i64().is_none()
            || non_empty_string(cell.get("note"), "cell note").is_err()
        {
            bail!("invalid cell coordinate or note for {case_id}");
        }
    }
    for field in [
        "observation",
        "hypothesis",
        "expected_observation_if_true",
        "falsifier",
        "finding",
        "next_command",
        "reviewer",
        "timestamp",
    ] {
        non_empty_string(decision.get(field), &format!("decision {field}"))?;
    }
    let homologs = decision["affected_homologs"]
        .as_array()
        .ok_or_else(|| anyhow!("affected_homologs must be a string list for {case_id}"))?;
    if !homologs.iter().all(Value::is_string) {
        bail!("affected_homologs must be a string list for {case_id}");
    }
    Ok(())
}

fn review_kind(decision: &Value) -> Result<&'static str> {
    let reviewer = non_empty_string(decision.get("reviewer"), "decision reviewer")?;
    if !REVIEWERS.contains(&reviewer.as_str()) {
        bail!("unsupported reviewer: {reviewer}");
    }

    match (
        reviewer.as_str(),
        decision.get("review_kind").and_then(Value::as_str),
    ) {
        ("machine", Some(STRUCTURAL_PRESCREEN)) => Ok(STRUCTURAL_PRESCREEN),
        ("machine", _) => {
            bail!("machine decisions must declare review_kind={STRUCTURAL_PRESCREEN}")
        }
        (_, None) | ("ai" | "human", Some(PERCEPTUAL_REVIEW)) => Ok(PERCEPTUAL_REVIEW),
        (_, Some(STRUCTURAL_PRESCREEN)) => {
            bail!("only machine decisions may use review_kind={STRUCTURAL_PRESCREEN}")
        }
        (_, Some(kind)) => bail!("unsupported review_kind: {kind}"),
    }
}

fn selected_rows(
    args: &ReviewArgs,
    rows: &BTreeMap<String, Value>,
    decisions: &DecisionMap,
) -> Vec<Value> {
    rows.values()
        .filter(|row| {
            if row["classification"] == "expected_error" || !matches_filter(args, row) {
                return false;
            }
            let state = decisions.get(row["case_id"].as_str().unwrap_or_default());
            let perceptual = state.is_some_and(|state| state.contains(PERCEPTUAL_REVIEW));
            !perceptual
        })
        .cloned()
        .collect()
}

fn covers_row(state: &DecisionState) -> bool {
    state.contains(PERCEPTUAL_REVIEW)
}

fn filtered_rows(args: &ReviewArgs, rows: &BTreeMap<String, Value>) -> Vec<Value> {
    rows.values()
        .filter(|row| row["classification"] != "expected_error" && matches_filter(args, row))
        .cloned()
        .collect()
}

fn matches_filter(args: &ReviewArgs, row: &Value) -> bool {
    args.fixture
        .as_deref()
        .is_none_or(|fixture| row["fixture"].as_str() == Some(fixture))
        && args
            .style
            .as_deref()
            .is_none_or(|style| row["style"].as_str() == Some(style))
        && args
            .mode
            .as_deref()
            .is_none_or(|mode| row["mode"].as_str() == Some(mode))
}

fn prescreen_clean(
    args: &ReviewArgs,
    packet: &Path,
    rows: &BTreeMap<String, Value>,
    decisions: &DecisionMap,
    decisions_path: &Path,
) -> Result<Value> {
    let mut recorded = 0usize;
    let mut skipped = 0usize;
    let mut first_skipped = None;

    for row in rows.values().filter(|row| {
        row["classification"] != "expected_error"
            && !decisions
                .get(row["case_id"].as_str().unwrap_or_default())
                .is_some_and(|state| {
                    state.contains(STRUCTURAL_PRESCREEN) || state.contains(PERCEPTUAL_REVIEW)
                })
            && matches_filter(args, row)
    }) {
        if !is_structurally_clean(packet, row)? {
            skipped += 1;
            if first_skipped.is_none() {
                first_skipped = row["case_id"].as_str().map(ToOwned::to_owned);
            }
            continue;
        }

        let decision = structural_decision(row, packet, decisions_path)?;
        validate_decision(&decision, rows)?;
        append_decision(decisions_path, &decision)?;
        recorded += 1;
    }

    Ok(json!({
        "schema": COVERAGE_SCHEMA,
        "review_kind": STRUCTURAL_PRESCREEN,
        "recorded": recorded,
        "skipped_for_one_frame_review": skipped,
        "first_residual_case_id": first_skipped,
    }))
}

fn is_structurally_clean(packet: &Path, row: &Value) -> Result<bool> {
    let evidence_ref = row
        .get("evidence")
        .filter(|value| !value.is_null())
        .ok_or_else(|| anyhow!("reviewable row has no evidence: {}", row["case_id"]))?;
    let evidence_bytes = common::validate_blob_ref(packet, evidence_ref, "evidence")?;
    let evidence: Value =
        serde_json::from_slice(&evidence_bytes).context("parse review evidence")?;

    Ok(evidence_is_structurally_clean(&evidence))
}

fn evidence_is_structurally_clean(evidence: &Value) -> bool {
    if evidence.get("schema").and_then(Value::as_str) != Some(common::EVIDENCE_SCHEMA) {
        return false;
    }
    let Some(warnings) = evidence.get("warnings").and_then(Value::as_array) else {
        return false;
    };
    let Some(raw_errors) = evidence
        .get("raw")
        .and_then(|value| value.get("errors"))
        .and_then(Value::as_array)
    else {
        return false;
    };
    let Some(findings) = evidence
        .get("critic")
        .and_then(|value| value.get("findings"))
        .and_then(Value::as_array)
    else {
        return false;
    };
    let Some(geometry_errors) = evidence
        .get("geometry")
        .and_then(|value| value.get("errors"))
        .and_then(Value::as_array)
    else {
        return false;
    };
    let Some(untraced_fallback_edges) = evidence
        .get("geometry")
        .and_then(|value| value.get("untraced_fallback_edges"))
        .and_then(Value::as_array)
    else {
        return false;
    };
    let Some(owner_counts) = evidence
        .get("semantic")
        .and_then(|value| value.get("owner_counts"))
        .and_then(Value::as_object)
    else {
        return false;
    };
    let optional_errors_are_empty = evidence
        .get("errors")
        .is_none_or(|value| matches!(value, Value::Array(values) if values.is_empty()));

    optional_errors_are_empty
        && warnings.is_empty()
        && raw_errors.is_empty()
        && findings.is_empty()
        && geometry_errors.is_empty()
        && owner_counts.values().all(Value::is_u64)
        && warnings.iter().all(Value::is_string)
        && raw_errors.iter().all(Value::is_string)
        && findings.iter().all(|finding| {
            finding.is_object()
                && finding.get("code").is_some_and(Value::is_string)
                && finding.get("message").is_some_and(Value::is_string)
        })
        && geometry_errors.iter().all(Value::is_string)
        && untraced_fallback_edges.iter().all(Value::is_string)
}

fn structural_decision(row: &Value, packet: &Path, decisions_path: &Path) -> Result<Value> {
    let case_id = non_empty_string(row.get("case_id"), "manifest case_id")?;
    let frame_sha256 = non_empty_string(
        row.get("stdout").and_then(|value| value.get("sha256")),
        "manifest frame sha256",
    )?;
    let evidence_sha256 = non_empty_string(
        row.get("evidence").and_then(|value| value.get("sha256")),
        "manifest evidence sha256",
    )?;

    Ok(json!({
        "schema": DECISION_SCHEMA,
        "review_kind": STRUCTURAL_PRESCREEN,
        "case_id": case_id,
        "frame_sha256": frame_sha256,
        "evidence_sha256": evidence_sha256,
        "decision": "pass",
        "severity": "P3",
        "dimensions": DIMENSIONS,
        "cells": [],
        "finding": "none",
        "observation": "Structural pre-screen found no warnings, critic findings, render errors, or geometry errors; perceptual review remains required for residual rows.",
        "hypothesis": "The machine-checkable evidence is clean for this frame, so no automated defect signal is present.",
        "expected_observation_if_true": "A one-frame visual review should confirm readable labels, connected routes, correct arrowheads, and no visible overlap.",
        "falsifier": "Any visible semantic error, overlap, clipping, ambiguous route, or text/readability defect in the frame disproves this clean pre-screen.",
        "affected_homologs": [],
        "next_command": format!(
            "scripts/review_visual_packet.sh --packet {} --decisions {} --next",
            packet.display(),
            decisions_path.display(),
        ),
        "reviewer": "machine",
        "timestamp": common::now_label(),
    }))
}

fn frame_payload(root: &Path, packet: &Path, row: &Value) -> Result<Value> {
    let input = common::repository_file(root, &row["input"], "manifest input")?;
    let frame = common::validate_blob_ref(packet, &row["stdout"], "frame")?;
    let evidence_ref = row
        .get("evidence")
        .filter(|value| !value.is_null())
        .ok_or_else(|| anyhow!("reviewable row has no evidence: {}", row["case_id"]))?;
    let evidence_bytes = common::validate_blob_ref(packet, evidence_ref, "evidence")?;
    let evidence: Value =
        serde_json::from_slice(&evidence_bytes).context("parse review evidence")?;
    let evidence_hash = evidence_ref["sha256"].as_str().unwrap_or_default();
    Ok(json!({
        "schema": FRAME_SCHEMA,
        "case_id": row["case_id"],
        "fixture": row["fixture"],
        "style": row["style"],
        "mode": row["mode"],
        "frame_sha256": row["stdout"]["sha256"],
        "evidence_sha256": evidence_hash,
        "input": String::from_utf8(common::require_file(&input, "manifest input")?).context("input is not UTF-8")?,
        "frame": String::from_utf8(frame).context("frame is not UTF-8")?,
        "dimensions": row["dimensions"],
        "critic": evidence["critic"],
        "raw": evidence["raw"],
        "geometry": evidence["geometry"],
        "semantic": evidence["semantic"],
        "warnings": evidence["warnings"],
        "repair": {
            "optimized": evidence["optimized"],
            "repair_passes": evidence["repair_passes"],
            "layout_attempts": evidence["layout_attempts"],
            "layout_repairs_applied": evidence["layout_repairs_applied"],
        },
        "decision_form": {
            "decision": "pass|fail|watch|unclear",
            "severity": "P0|P1|P2|P3",
            "dimensions": DIMENSIONS,
            "cells": [{"x": 0, "y": 0, "note": "optional precise cell observation"}],
            "finding": "stable-human-readable-id-or-none",
            "observation": "what a human eye sees before source explanation",
            "hypothesis": "likely responsible layer or interaction",
            "expected_observation_if_true": "what the next check should show",
            "falsifier": "what would disprove the hypothesis",
            "affected_homologs": [],
            "next_command": "targeted test or review command",
            "reviewer": "ai|human",
            "review_kind": "perceptual",
        },
    }))
}

fn append_decision(path: &Path, decision: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut stream = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open decision log {}", path.display()))?;
    let mut line = serde_json::to_vec(decision)?;
    line.push(b'\n');
    stream.write_all(&line).context("append review decision")
}

fn non_empty_string(value: Option<&Value>, label: &str) -> Result<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("{label} must be a non-empty string"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structural_predicate_rejects_every_machine_signal() {
        let clean = json!({
            "schema": common::EVIDENCE_SCHEMA,
            "warnings": [],
            "errors": [],
            "critic": {"findings": []},
            "raw": {"errors": []},
            "geometry": {"errors": [], "untraced_fallback_edges": []},
            "semantic": {"owner_counts": {"Unknown": 0}}
        });
        assert!(evidence_is_structurally_clean(&clean));

        for signal in [
            ("warnings", json!(["warning"])),
            ("errors", json!(["error"])),
        ] {
            let mut evidence = clean.clone();
            evidence[signal.0] = signal.1;
            assert!(!evidence_is_structurally_clean(&evidence));
        }

        for (section, key) in [
            ("critic", "findings"),
            ("raw", "errors"),
            ("geometry", "errors"),
        ] {
            let mut evidence = clean.clone();
            evidence[section][key] = json!(["signal"]);
            assert!(!evidence_is_structurally_clean(&evidence));
        }

        let mut incomplete = clean;
        incomplete["raw"] = json!({});
        assert!(!evidence_is_structurally_clean(&incomplete));

        let missing_fallback = json!({
            "schema": common::EVIDENCE_SCHEMA,
            "warnings": [],
            "critic": {"findings": []},
            "raw": {"errors": []},
            "geometry": {"errors": []},
            "semantic": {"owner_counts": {"Unknown": 0}}
        });
        assert!(!evidence_is_structurally_clean(&missing_fallback));

        let missing_owners = json!({
            "schema": common::EVIDENCE_SCHEMA,
            "warnings": [],
            "critic": {"findings": []},
            "raw": {"errors": []},
            "geometry": {"errors": [], "untraced_fallback_edges": []}
        });
        assert!(!evidence_is_structurally_clean(&missing_owners));
    }

    #[test]
    fn structural_decision_is_hash_bound_and_machine_labeled() {
        let row = json!({
            "case_id": "case",
            "fixture": "fixture",
            "style": "ascii",
            "mode": "default",
            "stdout": {"sha256": "frame"},
            "evidence": {"sha256": "evidence"}
        });
        let decision = structural_decision(
            &row,
            Path::new("/tmp/packet"),
            Path::new("/tmp/decisions.jsonl"),
        )
        .expect("structural decision");
        let rows = BTreeMap::from([("case".to_owned(), row)]);

        validate_decision(&decision, &rows).expect("valid structural decision");
        assert_eq!(decision["reviewer"], "machine");
        assert_eq!(decision["review_kind"], STRUCTURAL_PRESCREEN);
        assert!(decision["next_command"]
            .as_str()
            .expect("next command")
            .contains("--packet /tmp/packet --decisions /tmp/decisions.jsonl --next"));
    }

    #[test]
    fn machine_decision_requires_structural_kind() {
        let row = json!({
            "case_id": "case",
            "stdout": {"sha256": "frame"},
            "evidence": {"sha256": "evidence"}
        });
        let rows = BTreeMap::from([("case".to_owned(), row)]);
        let decision = json!({
            "schema": DECISION_SCHEMA,
            "case_id": "case",
            "frame_sha256": "frame",
            "evidence_sha256": "evidence",
            "decision": "pass",
            "severity": "P3",
            "dimensions": ["readability"],
            "cells": [],
            "finding": "none",
            "observation": "clear",
            "hypothesis": "none",
            "expected_observation_if_true": "none",
            "falsifier": "none",
            "affected_homologs": [],
            "next_command": "none",
            "reviewer": "machine",
            "timestamp": "now"
        });
        assert!(validate_decision(&decision, &rows).is_err());
    }

    #[test]
    fn perceptual_review_kind_is_accepted_for_ai_and_human() {
        let row = json!({
            "case_id": "case",
            "stdout": {"sha256": "frame"},
            "evidence": {"sha256": "evidence"}
        });
        let rows = BTreeMap::from([("case".to_owned(), row)]);
        for reviewer in ["ai", "human"] {
            let decision = json!({
                "schema": DECISION_SCHEMA,
                "case_id": "case",
                "frame_sha256": "frame",
                "evidence_sha256": "evidence",
                "decision": "pass",
                "severity": "P3",
                "dimensions": ["readability"],
                "cells": [],
                "finding": "none",
                "observation": "clear",
                "hypothesis": "none",
                "expected_observation_if_true": "none",
                "falsifier": "none",
                "affected_homologs": [],
                "next_command": "none",
                "reviewer": reviewer,
                "review_kind": PERCEPTUAL_REVIEW,
                "timestamp": "now"
            });
            validate_decision(&decision, &rows).expect("perceptual decision should validate");
            assert_eq!(
                review_kind(&decision).expect("review kind"),
                PERCEPTUAL_REVIEW
            );
        }
    }

    #[test]
    fn structural_decision_does_not_close_perceptual_queue() {
        let row = json!({
            "case_id": "case",
            "classification": "success",
            "fixture": "fixture",
            "style": "ascii",
            "mode": "default"
        });
        let rows = BTreeMap::from([("case".to_owned(), row)]);
        let decisions = BTreeMap::from([(
            "case".to_owned(),
            DecisionState {
                structural: Some(json!({"reviewer": "machine"})),
                perceptual: None,
            },
        )]);
        let args = ReviewArgs {
            packet: PathBuf::new(),
            decisions: PathBuf::new(),
            fixture: None,
            style: None,
            mode: None,
            reviewer: "ai".to_owned(),
            next: true,
            record: None,
            prescreen_clean: false,
            validate: false,
        };
        assert_eq!(selected_rows(&args, &rows, &decisions).len(), 1);
    }

    #[test]
    fn decision_requires_current_evidence_hash() {
        let row = json!({
            "case_id": "case",
            "stdout": {"sha256": "frame"},
            "evidence": {"sha256": "evidence"}
        });
        let rows = BTreeMap::from([("case".to_owned(), row)]);
        let decision = json!({
            "schema": DECISION_SCHEMA,
            "case_id": "case",
            "frame_sha256": "frame",
            "evidence_sha256": "stale",
            "decision": "pass",
            "severity": "P3",
            "dimensions": ["readability"],
            "cells": [],
            "finding": "none",
            "observation": "clear",
            "hypothesis": "none",
            "expected_observation_if_true": "none",
            "falsifier": "none",
            "affected_homologs": [],
            "next_command": "none",
            "reviewer": "ai",
            "timestamp": "now"
        });
        assert!(validate_decision(&decision, &rows).is_err());
    }
}
