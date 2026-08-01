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
const DECISIONS: &[&str] = &["pass", "fail", "watch", "unclear"];
const SEVERITIES: &[&str] = &["P0", "P1", "P2", "P3"];
const DIMENSIONS: &[&str] = &["semantic", "containment", "route", "text", "readability"];

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
        append_decision(&decisions_path, &decision)?;
        println!("{}", decision["case_id"].as_str().unwrap_or_default());
        return Ok(());
    }
    if args.validate {
        let selected = filtered_rows(&args, &rows);
        let missing: Vec<_> = selected
            .iter()
            .filter_map(|row| row["case_id"].as_str())
            .filter(|case_id| !decisions.contains_key(*case_id))
            .collect();
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

fn load_decisions(path: &Path, rows: &BTreeMap<String, Value>) -> Result<BTreeMap<String, Value>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let bytes = common::require_file(path, "decision log")?;
    let text = String::from_utf8(bytes).context("decision log is not UTF-8")?;
    let mut decisions = BTreeMap::new();
    for (number, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let decision: Value = serde_json::from_str(line)
            .with_context(|| format!("decision line {} is invalid JSON", number + 1))?;
        validate_decision(&decision, rows)?;
        let case_id = decision["case_id"].as_str().unwrap_or_default().to_owned();
        if decisions.insert(case_id.clone(), decision).is_some() {
            bail!("duplicate decision for case_id: {case_id}");
        }
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

fn selected_rows(
    args: &ReviewArgs,
    rows: &BTreeMap<String, Value>,
    decisions: &BTreeMap<String, Value>,
) -> Vec<Value> {
    rows.values()
        .filter(|row| {
            row["classification"] != "expected_error"
                && !decisions.contains_key(row["case_id"].as_str().unwrap_or_default())
                && matches_filter(args, row)
        })
        .cloned()
        .collect()
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
