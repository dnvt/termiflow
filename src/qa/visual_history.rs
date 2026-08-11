use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use super::common;

pub(crate) const HISTORY_SCHEMA: &str = "termiflow.visual_review.history_record.v1";
pub(crate) const HISTORY_CONTEXT_SCHEMA: &str = "termiflow.visual_review.history_context.v1";
pub(crate) const HISTORY_RESOLUTION_SCHEMA: &str = "termiflow.visual_review.history_resolution.v1";

const STATUSES: &[&str] = &["open", "falsified", "repaired", "superseded"];
const SEVERITIES: &[&str] = &["P0", "P1", "P2", "P3"];
const DIMENSIONS: &[&str] = &["semantic", "containment", "route", "text", "readability"];
const SELECTOR_FIELDS: &[&str] = &["case_id", "fixture", "style", "mode", "direction"];
const CLOSED_STATUSES: &[&str] = &["falsified", "repaired", "superseded"];

#[derive(Debug, Default, Clone)]
pub(crate) struct HistoryLedger {
    records: Vec<Value>,
}

impl HistoryLedger {
    pub(crate) fn load(path: Option<&Path>) -> Result<Self> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let bytes = common::require_file(path, "visual history ledger")?;
        let text = String::from_utf8(bytes).context("visual history ledger is not UTF-8")?;
        let mut records = Vec::new();
        let mut ids = BTreeSet::new();
        for (number, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let record: Value = serde_json::from_str(line)
                .with_context(|| format!("visual history line {} is invalid JSON", number + 1))?;
            validate_record(&record)?;
            let history_id = non_empty_string(record.get("history_id"), "history_id")?;
            if !ids.insert(history_id.clone()) {
                bail!("duplicate visual history_id: {history_id}");
            }
            records.push(record);
        }
        Ok(Self { records })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub(crate) fn matching<'a>(&'a self, row: &Value) -> Vec<&'a Value> {
        self.records
            .iter()
            .filter(|record| selector_matches(record, row))
            .collect()
    }

    pub(crate) fn open_matching<'a>(&'a self, row: &Value) -> Vec<&'a Value> {
        self.records
            .iter()
            .filter(|record| record["status"] == "open" && selector_matches(record, row))
            .collect()
    }

    pub(crate) fn unresolved_open_ids(
        &self,
        row: &Value,
        resolved_ids: &BTreeSet<String>,
    ) -> BTreeSet<String> {
        self.open_matching(row)
            .into_iter()
            .filter_map(|record| record["history_id"].as_str())
            .filter(|history_id| !resolved_ids.contains(*history_id))
            .map(ToOwned::to_owned)
            .collect()
    }

    pub(crate) fn records_for_rows(&self, rows: &[Value]) -> BTreeSet<String> {
        rows.iter()
            .flat_map(|row| self.open_matching(row))
            .filter_map(|record| record["history_id"].as_str())
            .map(ToOwned::to_owned)
            .collect()
    }

    pub(crate) fn validate_open_selectors(
        &self,
        rows: &std::collections::BTreeMap<String, Value>,
    ) -> Result<()> {
        for record in self
            .records
            .iter()
            .filter(|record| record["status"] == "open")
        {
            if !rows.values().any(|row| selector_matches(record, row)) {
                let history_id = non_empty_string(record.get("history_id"), "history_id")?;
                bail!(
                    "open visual history record {history_id} matches no row in the current packet"
                );
            }
        }
        Ok(())
    }

    pub(crate) fn resolved_ids<'a, I>(&self, decisions: I) -> Result<BTreeSet<String>>
    where
        I: IntoIterator<Item = &'a Value>,
    {
        let mut resolved = BTreeSet::new();
        for decision in decisions {
            if let Some((status, ids)) = resolution_ids(decision)? {
                if CLOSED_STATUSES.contains(&status.as_str()) {
                    resolved.extend(ids);
                }
            }
        }
        Ok(resolved)
    }

    pub(crate) fn validate_ordered_decisions(
        &self,
        path: &Path,
        rows: &std::collections::BTreeMap<String, Value>,
    ) -> Result<()> {
        if self.is_empty() || !path.exists() {
            return Ok(());
        }
        let bytes = common::require_file(path, "decision log")?;
        let text = String::from_utf8(bytes).context("decision log is not UTF-8")?;
        let mut resolved = BTreeSet::new();
        for (number, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let decision: Value = serde_json::from_str(line)
                .with_context(|| format!("decision line {} is invalid JSON", number + 1))?;
            if decision.get("review_kind").and_then(Value::as_str) == Some("structural_prescreen") {
                continue;
            }
            let case_id = non_empty_string(decision.get("case_id"), "decision case_id")?;
            let row = rows
                .get(&case_id)
                .ok_or_else(|| anyhow!("decision references unknown case_id: {case_id}"))?;
            self.guard_decision(row, &decision, &resolved)?;
            if let Some((status, ids)) = resolution_ids(&decision)? {
                if CLOSED_STATUSES.contains(&status.as_str()) {
                    resolved.extend(ids);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn guard_decision(
        &self,
        row: &Value,
        decision: &Value,
        resolved_ids: &BTreeSet<String>,
    ) -> Result<()> {
        if decision.get("review_kind").and_then(Value::as_str) == Some("structural_prescreen") {
            return Ok(());
        }

        let case_id = non_empty_string(row.get("case_id"), "manifest case_id")?;
        let matching_open = self.open_matching(row);
        let matching_ids: BTreeSet<String> = matching_open
            .iter()
            .filter_map(|record| record["history_id"].as_str())
            .map(ToOwned::to_owned)
            .collect();
        let unresolved = matching_ids
            .difference(resolved_ids)
            .cloned()
            .collect::<BTreeSet<_>>();
        let resolution = resolution_ids(decision)?;

        if let Some((status, ids)) = &resolution {
            if status == "open" {
                if decision["decision"] == "pass" {
                    bail!(
                        "pass for {case_id} cannot acknowledge open visual history; record a non-pass decision or a closed resolution"
                    );
                }
            } else {
                if decision["decision"] != "pass" {
                    bail!("closed visual history resolution for {case_id} must accompany a pass");
                }
                if ids.iter().any(|history_id| {
                    !matching_open
                        .iter()
                        .any(|record| record["history_id"].as_str() == Some(history_id.as_str()))
                }) {
                    bail!(
                        "history resolution for {case_id} references an unknown or non-matching open history record"
                    );
                }
            }
        }

        if decision["decision"] == "pass" && !unresolved.is_empty() {
            let Some((status, ids)) = resolution else {
                bail!(
                    "unresolved visual history for {case_id}: pass requires history_resolution with status falsified, repaired, or superseded for {}",
                    unresolved.iter().cloned().collect::<Vec<_>>().join(", ")
                );
            };
            if !CLOSED_STATUSES.contains(&status.as_str()) || !unresolved.is_subset(&ids) {
                bail!(
                    "pass for {case_id} must close every unresolved visual history record: {}",
                    unresolved.iter().cloned().collect::<Vec<_>>().join(", ")
                );
            }
        }
        Ok(())
    }

    pub(crate) fn context(&self, row: &Value, resolved_ids: &BTreeSet<String>) -> Value {
        let records = self.matching(row);
        let open = records
            .iter()
            .filter(|record| record["status"] == "open")
            .map(|record| record["history_id"].clone())
            .collect::<Vec<_>>();
        let unresolved = open
            .iter()
            .filter_map(Value::as_str)
            .filter(|history_id| !resolved_ids.contains(*history_id))
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        json!({
            "schema": HISTORY_CONTEXT_SCHEMA,
            "records": records,
            "open_history_ids": open,
            "unresolved_open_history_ids": unresolved,
        })
    }
}

fn validate_record(record: &Value) -> Result<()> {
    let object = record
        .as_object()
        .ok_or_else(|| anyhow!("visual history record must be an object"))?;
    if record["schema"].as_str() != Some(HISTORY_SCHEMA) {
        bail!("visual history record schema must be {HISTORY_SCHEMA}");
    }
    for field in [
        "history_id",
        "observation",
        "hypothesis",
        "expected_observation_if_true",
        "falsifier",
        "next_command",
        "created_at",
    ] {
        non_empty_string(record.get(field), field)?;
    }
    let status = non_empty_string(record.get("status"), "status")?;
    if !STATUSES.contains(&status.as_str()) {
        bail!("unsupported visual history status: {status}");
    }
    let severity = non_empty_string(record.get("severity"), "severity")?;
    if !SEVERITIES.contains(&severity.as_str()) {
        bail!("unsupported visual history severity: {severity}");
    }
    let selector = record
        .get("selector")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("visual history selector must be an object"))?;
    if selector.is_empty() {
        bail!("visual history selector must identify at least one row field");
    }
    for (field, value) in selector {
        if !SELECTOR_FIELDS.contains(&field.as_str()) {
            bail!("unsupported visual history selector field: {field}");
        }
        if !value.as_str().is_some_and(|value| !value.trim().is_empty()) {
            bail!("visual history selector field {field} must be a non-empty string");
        }
    }
    let dimensions = record
        .get("dimensions")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("visual history dimensions must be a non-empty list"))?;
    let dimension_names: BTreeSet<&str> = dimensions.iter().filter_map(Value::as_str).collect();
    if dimensions.is_empty()
        || dimension_names.len() != dimensions.len()
        || dimensions
            .iter()
            .any(|dimension| !DIMENSIONS.contains(&dimension.as_str().unwrap_or_default()))
    {
        bail!("visual history dimensions are invalid");
    }
    let field = "affected_homologs";
    let values = record
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("visual history {field} must be a string list"))?;
    if !values.iter().all(Value::is_string) {
        bail!("visual history {field} must be a string list");
    }
    for field in object.keys() {
        if ![
            "schema",
            "history_id",
            "status",
            "severity",
            "selector",
            "dimensions",
            "observation",
            "hypothesis",
            "expected_observation_if_true",
            "falsifier",
            "affected_homologs",
            "next_command",
            "created_at",
            "source",
            "prior_decision_sha256",
        ]
        .contains(&field.as_str())
        {
            bail!("unsupported visual history field: {field}");
        }
    }
    if let Some(source) = record.get("source") {
        non_empty_string(Some(source), "source")?;
    }
    if let Some(prior) = record.get("prior_decision_sha256") {
        let value = non_empty_string(Some(prior), "prior_decision_sha256")?;
        if value.len() != 64 || !value.chars().all(|character| character.is_ascii_hexdigit()) {
            bail!("prior_decision_sha256 must be a 64-character hexadecimal hash");
        }
    }
    Ok(())
}

fn resolution_ids(decision: &Value) -> Result<Option<(String, BTreeSet<String>)>> {
    let Some(resolution) = decision.get("history_resolution") else {
        return Ok(None);
    };
    if resolution["schema"].as_str() != Some(HISTORY_RESOLUTION_SCHEMA) {
        bail!("history_resolution schema must be {HISTORY_RESOLUTION_SCHEMA}");
    }
    let status = non_empty_string(resolution.get("status"), "history_resolution status")?;
    if !STATUSES.contains(&status.as_str()) {
        bail!("unsupported history_resolution status: {status}");
    }
    let ids = resolution
        .get("history_ids")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("history_resolution history_ids must be a non-empty list"))?;
    let mut unique_ids = BTreeSet::new();
    for id in ids {
        let id = non_empty_string(Some(id), "history_resolution history_id")?;
        if !unique_ids.insert(id.clone()) {
            bail!("history_resolution history_ids must be unique");
        }
    }
    if unique_ids.is_empty() {
        bail!("history_resolution history_ids must be a non-empty list");
    }
    non_empty_string(resolution.get("note"), "history_resolution note")?;
    Ok(Some((status, unique_ids)))
}

fn selector_matches(record: &Value, row: &Value) -> bool {
    record["selector"].as_object().is_some_and(|selector| {
        selector
            .iter()
            .all(|(field, expected)| row.get(field) == Some(expected))
    })
}

fn non_empty_string(value: Option<&Value>, label: &str) -> Result<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("visual history {label} must be a non-empty string"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn record(selector: Value, status: &str) -> Value {
        json!({
            "schema": HISTORY_SCHEMA,
            "history_id": "history-1",
            "status": status,
            "severity": "P2",
            "selector": selector,
            "dimensions": ["route", "readability"],
            "observation": "two rails visually collapse",
            "hypothesis": "shared target entry ownership is ambiguous",
            "expected_observation_if_true": "the homolog shows separate rails",
            "falsifier": "a reviewed homolog preserves one unambiguous rail",
            "affected_homologs": ["fixture.unicode.default"],
            "next_command": "cargo test --test focused",
            "created_at": "2026-08-10T00:00:00Z"
        })
    }

    fn row(case_id: &str, style: &str) -> Value {
        json!({
            "case_id": case_id,
            "fixture": "collision_sibling_subgraphs_lr",
            "style": style,
            "mode": "default",
            "direction": "LR"
        })
    }

    fn pass(case_id: &str, resolution: Option<Value>) -> Value {
        let mut decision = json!({"case_id": case_id, "decision": "pass"});
        if let Some(resolution) = resolution {
            decision["history_resolution"] = resolution;
        }
        decision
    }

    fn closed_resolution() -> Value {
        json!({
            "schema": HISTORY_RESOLUTION_SCHEMA,
            "status": "repaired",
            "history_ids": ["history-1"],
            "note": "targeted repair and all homologs were visually rechecked"
        })
    }

    #[test]
    fn load_requires_a_regular_history_file_and_validates_records() {
        let root = std::env::temp_dir().join(format!(
            "termiflow-history-{}-{}",
            std::process::id(),
            common::now_label()
        ));
        fs::create_dir_all(&root).expect("create history temp directory");
        let path = root.join("history.jsonl");
        fs::write(
            &path,
            format!(
                "{}\n",
                serde_json::to_string(&record(
                    json!({"fixture": "collision_sibling_subgraphs_lr"}),
                    "open",
                ))
                .unwrap()
            ),
        )
        .expect("write history");
        let ledger = HistoryLedger::load(Some(&path)).expect("load history");
        assert_eq!(ledger.open_matching(&row("case", "ascii")).len(), 1);
        assert!(HistoryLedger::load(Some(&root.join("missing.jsonl"))).is_err());
        fs::remove_dir_all(root).expect("remove history temp directory");
    }

    #[test]
    fn selector_is_stable_and_history_prioritizes_exact_homolog_scope() {
        let ascii = record(json!({"style": "ascii"}), "open");
        let mut unicode = record(json!({"style": "unicode"}), "open");
        unicode["history_id"] = json!("history-2");
        let ledger = HistoryLedger {
            records: vec![ascii, unicode],
        };
        assert_eq!(ledger.open_matching(&row("case", "ascii")).len(), 1);
        assert_eq!(ledger.open_matching(&row("case", "unicode")).len(), 1);
        assert!(ledger.open_matching(&row("case", "plain")).is_empty());
    }

    #[test]
    fn open_history_rejects_unqualified_pass_but_accepts_explicit_repair() {
        let ledger = HistoryLedger {
            records: vec![record(json!({"case_id": "case"}), "open")],
        };
        let row = row("case", "ascii");
        assert!(ledger
            .guard_decision(&row, &pass("case", None), &BTreeSet::new())
            .is_err());
        ledger
            .guard_decision(
                &row,
                &pass("case", Some(closed_resolution())),
                &BTreeSet::new(),
            )
            .expect("closed resolution should permit pass");
    }

    #[test]
    fn malformed_resolution_cannot_close_another_frame() {
        let ledger = HistoryLedger {
            records: vec![record(json!({"case_id": "case"}), "open")],
        };
        let mut resolution = closed_resolution();
        resolution["history_ids"] = json!(["unknown"]);
        assert!(ledger
            .guard_decision(
                &row("case", "ascii"),
                &pass("case", Some(resolution)),
                &BTreeSet::new()
            )
            .is_err());
    }

    #[test]
    fn open_selector_cannot_disappear_from_the_current_packet() {
        let ledger = HistoryLedger {
            records: vec![record(json!({"fixture": "removed_fixture"}), "open")],
        };
        let rows = std::collections::BTreeMap::from([("case".to_owned(), row("case", "ascii"))]);
        assert!(ledger.validate_open_selectors(&rows).is_err());
    }
}
