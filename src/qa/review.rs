use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use super::{common, persist, visual_history::HistoryLedger};

pub const DECISION_SCHEMA: &str = "termiflow.visual_review.decision.v3";
const FRAME_SCHEMA: &str = "termiflow.visual_review.frame.v2";
const COVERAGE_SCHEMA: &str = "termiflow.visual_review.coverage.v1";
const STRUCTURAL_PRESCREEN: &str = "structural_prescreen";
const PERCEPTUAL_REVIEW: &str = "perceptual";
const DECISIONS: &[&str] = &["pass", "fail", "watch", "unclear"];
const SEVERITIES: &[&str] = &["P0", "P1", "P2", "P3"];
const DIMENSIONS: &[&str] = &["semantic", "containment", "route", "text", "readability"];
const REVIEWERS: &[&str] = &["ai", "human", "machine"];
const WATCH_CLASSES: &[&str] = &[
    "confirmed_flaw",
    "topology_ambiguous",
    "inconclusive",
    "not_applicable",
];
const GENERIC_WATCH_OBSERVATION: &str =
    "Route or local visual density remains a conservative human-eye watch; the marked cells need matched review.";
const GENERIC_WATCH_HYPOTHESIS: &str =
    "The current topology-owned route/boundary interaction remains a human-eye ownership or density watch even though the frame is structurally renderable.";
const GENERATED_PASS_OBSERVATION_SUFFIX: &str =
    " reviewed for semantic direction, route continuity, border/portal ownership, spacing, glyph junctions, text/clipping, and tiny stray marks; no visible defect was found.";
const GENERATED_PASS_HYPOTHESIS: &str =
    "The accepted renderer changes remain isolated from this fixture and preserve the existing visual contract.";
const LEGACY_CARRY_FORWARD_OWNER: &str = "reviewer_calibration";
const LEGACY_REBOUND_OWNER: &str = "visual-review/legacy-carry-forward";
const DUPLICATED_REBOUND_HYPOTHESIS: &str =
    "The fresh packet should retain this watch across its homologs.";

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

    fn get(&self, kind: &str) -> Option<&Value> {
        match kind {
            STRUCTURAL_PRESCREEN => self.structural.as_ref(),
            PERCEPTUAL_REVIEW => self.perceptual.as_ref(),
            _ => None,
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
    pub history: Option<PathBuf>,
    pub fresh: bool,
    pub fixture: Option<String>,
    pub style: Option<String>,
    pub mode: Option<String>,
    pub reviewer: String,
    pub next: bool,
    pub record: Option<PathBuf>,
    pub rebind_from_packet: Option<PathBuf>,
    pub rebind_from_decisions: Option<PathBuf>,
    pub prescreen_clean: bool,
    pub validate: bool,
}

pub fn run(args: ReviewArgs) -> Result<()> {
    let root = std::env::current_dir().context("resolve repository root")?;
    let packet = resolve(&root, &args.packet);
    let rows = load_manifest(&packet)?;
    let decisions_path = resolve_decision_path(&root, &args.decisions);
    let decisions = load_decisions(&decisions_path, &rows)?;
    let history_path = args.history.as_ref().map(|path| resolve(&root, path));
    let history = HistoryLedger::load(history_path.as_deref())?;
    history.validate_open_selectors(&rows)?;
    history.validate_ordered_decisions(&decisions_path, &rows)?;
    if args.fresh {
        validate_fresh_decisions(&decisions)?;
    }

    if let (Some(prior_packet), Some(prior_decisions)) = (
        args.rebind_from_packet.as_ref(),
        args.rebind_from_decisions.as_ref(),
    ) {
        println!(
            "{}",
            rebind_exact_successful_decisions(
                &packet,
                &decisions_path,
                &rows,
                &decisions,
                &resolve(&root, prior_packet),
                &resolve(&root, prior_decisions),
                &history,
            )?
        );
        return Ok(());
    }

    if let Some(record_path) = args.record {
        let decision = if record_path == Path::new("-") {
            let mut bytes = Vec::new();
            std::io::stdin()
                .read_to_end(&mut bytes)
                .context("read review decision from stdin")?;
            serde_json::from_slice(&bytes).context("invalid review decision JSON from stdin")?
        } else {
            common::load_json(&resolve(&root, &record_path), "review decision")?
        };
        validate_decision(&decision, &rows)?;
        if args.fresh {
            validate_fresh_decision(&decision)?;
        }
        let case_id = non_empty_string(decision.get("case_id"), "decision case_id")?;
        let row = rows
            .get(&case_id)
            .ok_or_else(|| anyhow!("decision references unknown case_id: {case_id}"))?;
        let kind = review_kind(&decision)?;
        let resolved_history_ids = history.resolved_ids(
            decisions
                .values()
                .filter_map(|state| state.perceptual.as_ref()),
        )?;
        history.guard_decision(row, &decision, &resolved_history_ids)?;
        let outcome = persist::append_decision_checked(&decisions_path, &decision, || {
            let fresh_decisions = load_decisions(&decisions_path, &rows)?;
            if let Some(existing) = fresh_decisions
                .get(&case_id)
                .and_then(|state| state.get(kind))
            {
                if persist::semantically_equal_without_timestamp(existing, &decision) {
                    return Ok(persist::PublishOutcome::EqualReplay);
                }
                bail!("conflicting {kind} decision for case_id: {case_id}");
            }
            Ok(persist::PublishOutcome::Published)
        })?;
        if outcome == persist::PublishOutcome::EqualReplay {
            bail!("duplicate {kind} decision for case_id: {case_id}");
        }
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
        let resolved_history_ids = history.resolved_ids(
            decisions
                .values()
                .filter_map(|state| state.perceptual.as_ref()),
        )?;
        let unresolved_history = history
            .records_for_rows(&selected)
            .difference(&resolved_history_ids)
            .cloned()
            .collect::<Vec<_>>();
        println!(
            "{}",
            json!({
                "schema": COVERAGE_SCHEMA,
                "reviewed": selected.len(),
                "history_open_unresolved": unresolved_history,
            })
        );
        return Ok(());
    }
    if args.next {
        let resolved_history_ids = history.resolved_ids(
            decisions
                .values()
                .filter_map(|state| state.perceptual.as_ref()),
        )?;
        let selected = selected_rows(&args, &rows, &decisions, &history, &resolved_history_ids);
        if let Some(row) = selected.first() {
            println!(
                "{}",
                frame_payload(&root, &packet, row, &history, &resolved_history_ids)?
            );
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

pub(crate) fn load_manifest(packet: &Path) -> Result<BTreeMap<String, Value>> {
    if !packet.join("COMPLETE.json").is_file() {
        bail!(
            "missing completion marker: {}",
            packet.join("COMPLETE.json").display()
        );
    }
    let packet_identity = packet
        .join("identity.json")
        .is_file()
        .then(|| common::load_json(&packet.join("identity.json"), "packet identity"))
        .transpose()?;
    let packet_identity_sha256 = packet_identity
        .as_ref()
        .map(|_| common::sha256_file(&packet.join("identity.json")))
        .transpose()?;
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
        if !common::is_audit_schema(row.get("schema").unwrap_or(&Value::Null)) {
            bail!("manifest line {} has the wrong schema", number + 1);
        }
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
        if let (Some(identity), Some(identity_sha256)) =
            (packet_identity.as_ref(), packet_identity_sha256.as_deref())
        {
            common::validate_row_identity(
                &row,
                identity,
                identity_sha256,
                &format!("manifest row {case_id} identity"),
            )?;
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

fn validate_fresh_decisions(decisions: &DecisionMap) -> Result<()> {
    for (case_id, state) in decisions {
        if state.structural.is_some() {
            bail!(
                "fresh perceptual review cannot include machine structural decision for {case_id}"
            );
        }
        if let Some(decision) = state.perceptual.as_ref() {
            validate_fresh_decision(decision)?;
        }
    }
    Ok(())
}

fn validate_fresh_decision(decision: &Value) -> Result<()> {
    let case_id = non_empty_string(decision.get("case_id"), "decision case_id")?;
    if review_kind(decision)? != PERCEPTUAL_REVIEW {
        bail!("fresh perceptual review cannot include machine structural decision for {case_id}");
    }
    let decision_kind = non_empty_string(decision.get("decision"), "decision kind")?;
    let watch_class = non_empty_string(decision.get("watch_class"), "decision watch_class")?;
    if !WATCH_CLASSES.contains(&watch_class.as_str()) {
        bail!("invalid watch_class for {case_id}: {watch_class}");
    }
    if decision_kind == "pass" && watch_class != "not_applicable" {
        bail!("pass decision {case_id} must use watch_class=not_applicable");
    }
    if decision_kind != "pass" && watch_class == "not_applicable" {
        bail!("non-pass decision {case_id} cannot use watch_class=not_applicable");
    }
    if decision["cells"].as_array().is_none_or(Vec::is_empty) {
        bail!(
            "fresh perceptual review must bind every decision to at least one exact visible inspection cell for {case_id}"
        );
    }
    if decision.get("carry_forward").is_some() {
        bail!("fresh perceptual review cannot include carry-forward decision for {case_id}");
    }
    if decision["observation"].as_str() == Some(GENERIC_WATCH_OBSERVATION)
        || decision["hypothesis"].as_str() == Some(GENERIC_WATCH_HYPOTHESIS)
    {
        bail!(
            "fresh perceptual review must replace generic watch boilerplate with a frame-specific observation and hypothesis for {case_id}"
        );
    }
    if decision_kind == "pass"
        && (decision["observation"]
            .as_str()
            .is_some_and(|observation| observation.contains(GENERATED_PASS_OBSERVATION_SUFFIX))
            || decision["hypothesis"].as_str() == Some(GENERATED_PASS_HYPOTHESIS))
    {
        bail!(
            "fresh perceptual pass must replace generated observation and hypothesis with frame-specific human-eye evidence for {case_id}"
        );
    }
    if decision["next_command"]
        .as_str()
        .is_some_and(|command| command.contains("h152-"))
    {
        bail!("fresh perceptual review has a stale H152 next command for {case_id}");
    }
    if decision_kind != "pass" {
        let observation = non_empty_string(decision.get("observation"), "decision observation")?;
        if observation.contains("AI one-frame inspection")
            || observation.contains("nullxnull")
            || observation.contains("warning-bearing interaction is retained")
        {
            bail!(
                "fresh perceptual review must replace templated observation with visible details for {case_id}"
            );
        }
        let finding = non_empty_string(decision.get("finding"), "decision finding")?;
        if finding == "none" || finding == "stable-human-readable-id-or-none" {
            bail!("fresh non-pass review requires a concrete finding for {case_id}");
        }
        non_empty_string(decision.get("owner_layer"), "decision owner_layer")?;
    }
    if matches!(decision_kind.as_str(), "watch" | "fail")
        && decision["cells"].as_array().is_some_and(Vec::is_empty)
    {
        bail!(
            "fresh {kind} review must bind a watch/fail to exact frame cells for {case_id}",
            kind = decision["decision"]
        );
    }
    if decision_kind != "pass"
        && decision["cells"].as_array().is_some_and(|cells| {
            cells.iter().any(|cell| {
                cell["note"]
                    .as_str()
                    .is_some_and(|note| note.contains("frame-level watch"))
            })
        })
    {
        bail!("fresh non-pass review requires visible cell details for {case_id}");
    }
    Ok(())
}

fn rebind_exact_successful_decisions(
    current_packet: &Path,
    current_decisions_path: &Path,
    current_rows: &BTreeMap<String, Value>,
    current_decisions: &DecisionMap,
    prior_packet: &Path,
    prior_decisions_path: &Path,
    history: &HistoryLedger,
) -> Result<Value> {
    if !current_decisions.is_empty() {
        bail!(
            "rebind destination already contains decisions: {}",
            current_decisions_path.display()
        );
    }
    if current_decisions_path == prior_decisions_path {
        bail!("rebind destination must differ from prior decisions path");
    }

    let prior_rows = load_manifest(prior_packet)?;
    let prior_decisions = load_decisions(prior_decisions_path, &prior_rows)?;
    history.validate_ordered_decisions(prior_decisions_path, &prior_rows)?;
    let prior_resolved_history_ids = history.resolved_ids(
        prior_decisions
            .values()
            .filter_map(|state| state.perceptual.as_ref()),
    )?;
    let mut candidates = Vec::new();
    let mut rebound_warning = 0usize;
    let mut legacy_owner_layer_filled = 0usize;
    let mut skipped_changed = 0usize;
    let mut skipped_missing_history = 0usize;
    let mut skipped_without_perceptual = 0usize;

    for current_row in current_rows.values() {
        let case_id = current_row["case_id"]
            .as_str()
            .ok_or_else(|| anyhow!("current manifest row has no case_id"))?;
        if current_row["classification"] == "expected_error" {
            continue;
        }
        if current_row["classification"] == "warning" {
            rebound_warning += 1;
        } else if current_row["classification"] != "success" {
            continue;
        }

        let Some(prior_row) = prior_rows.get(case_id) else {
            skipped_missing_history += 1;
            continue;
        };
        let Some(prior_decision) = prior_decisions
            .get(case_id)
            .and_then(|state| state.perceptual.as_ref())
        else {
            skipped_without_perceptual += 1;
            continue;
        };

        if !same_review_identity(current_row, prior_row)
            || !same_review_hashes(current_row, prior_row)
        {
            skipped_changed += 1;
            continue;
        }

        let mut rebound = prior_decision.clone();
        rebound["frame_sha256"] = current_row["stdout"]["sha256"].clone();
        rebound["evidence_sha256"] = current_row["evidence"]["sha256"].clone();
        rebound["policy_sha256"] = current_row["policy"]["sha256"].clone();
        if let Some(run_id) = row_run_id(current_row) {
            rebound["run_id"] = Value::String(run_id.to_owned());
        } else if let Some(object) = rebound.as_object_mut() {
            object.remove("run_id");
        }
        rebound["next_command"] = Value::String(format!(
            "scripts/review_visual_packet.sh --packet {} --decisions {} --next",
            current_packet.display(),
            current_decisions_path.display()
        ));
        if let Some(hypothesis) = rebound.get("hypothesis").and_then(Value::as_str) {
            rebound["hypothesis"] = Value::String(collapse_rebound_hypothesis(hypothesis));
        }
        let prior_bytes = serde_json::to_vec(prior_decision).context("serialize prior decision")?;
        rebound["carry_forward"] = json!({
            "schema": "termiflow.visual_review.carry_forward.v1",
            "prior_packet": prior_packet.display().to_string(),
            "prior_decision_sha256": common::sha256_bytes(&prior_bytes),
            "prior_frame_sha256": prior_row["stdout"]["sha256"],
            "prior_evidence_sha256": prior_row["evidence"]["sha256"],
            "prior_policy_sha256": prior_row["policy"]["sha256"],
            "reason": "exact fixture/style/mode, frame, evidence, and effective-policy equality",
        });
        let prior_owner = rebound
            .get("owner_layer")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let needs_review_calibration = rebound["watch_class"] != "not_applicable"
            && prior_owner
                .as_deref()
                .is_none_or(|owner| owner.trim().is_empty() || owner == LEGACY_REBOUND_OWNER);
        if needs_review_calibration {
            rebound["owner_layer"] = Value::String(LEGACY_CARRY_FORWARD_OWNER.to_owned());
            rebound["carry_forward"]["owner_layer_provenance"] = Value::String(
                if prior_owner.as_deref() == Some(LEGACY_REBOUND_OWNER) {
                    "prior decision used legacy visual-review/legacy-carry-forward; normalized as reviewer_calibration for current-epoch learning".to_owned()
                } else {
                    "legacy decision lacked owner_layer; rebound as reviewer_calibration for current-epoch learning".to_owned()
                },
            );
            legacy_owner_layer_filled += 1;
        }
        rebound["timestamp"] = Value::String(common::now_label());
        validate_decision(&rebound, current_rows)?;
        history.guard_decision(current_row, &rebound, &prior_resolved_history_ids)?;
        candidates.push(rebound);
    }

    for decision in &candidates {
        persist::append_decision_checked(current_decisions_path, decision, || {
            let fresh = load_decisions(current_decisions_path, current_rows)?;
            let case_id = non_empty_string(decision.get("case_id"), "decision case_id")?;
            if fresh
                .get(&case_id)
                .and_then(|state| state.get(PERCEPTUAL_REVIEW))
                .is_some()
            {
                bail!("duplicate perceptual decision for case_id: {case_id}");
            }
            Ok(persist::PublishOutcome::Published)
        })?;
    }

    Ok(json!({
        "schema": "termiflow.visual_review.rebind.v1",
        "rebound": candidates.len(),
        "rebound_warning": rebound_warning,
        "legacy_owner_layer_filled": legacy_owner_layer_filled,
        "skipped_changed": skipped_changed,
        "skipped_missing_history": skipped_missing_history,
        "skipped_without_perceptual": skipped_without_perceptual,
        "current_packet": current_packet.display().to_string(),
        "prior_packet": prior_packet.display().to_string(),
        "next": format!(
            "scripts/review_visual_packet.sh --packet {} --decisions {} --next",
            current_packet.display(),
            current_decisions_path.display()
        ),
    }))
}

fn collapse_rebound_hypothesis(hypothesis: &str) -> String {
    let duplicated = format!("{DUPLICATED_REBOUND_HYPOTHESIS} {DUPLICATED_REBOUND_HYPOTHESIS}");
    let mut normalized = hypothesis.to_owned();
    while normalized.ends_with(&duplicated) {
        let prefix_len = normalized.len() - duplicated.len();
        normalized.truncate(prefix_len);
        normalized.push_str(DUPLICATED_REBOUND_HYPOTHESIS);
    }
    normalized
}

fn same_review_identity(current: &Value, prior: &Value) -> bool {
    [
        "case_id",
        "fixture",
        "style",
        "mode",
        "direction",
        "classification",
        "input",
    ]
    .iter()
    .all(|field| current.get(*field) == prior.get(*field))
}

fn same_review_hashes(current: &Value, prior: &Value) -> bool {
    [
        ("stdout", "sha256"),
        ("evidence", "sha256"),
        ("policy", "sha256"),
    ]
    .iter()
    .all(|(section, field)| current[*section][*field] == prior[*section][*field])
}

pub(crate) fn validate_decision(decision: &Value, rows: &BTreeMap<String, Value>) -> Result<()> {
    if decision["schema"].as_str() != Some(DECISION_SCHEMA) {
        bail!("decision schema must be {DECISION_SCHEMA}");
    }
    let case_id = non_empty_string(decision.get("case_id"), "decision case_id")?;
    let row = rows
        .get(&case_id)
        .ok_or_else(|| anyhow!("decision references unknown case_id: {case_id}"))?;
    if let Some(run_id) = row_run_id(row) {
        if decision.get("run_id").and_then(Value::as_str) != Some(run_id) {
            bail!("decision run_id is stale for {case_id}");
        }
    }
    if let Some(policy_sha256) = row["policy"]["sha256"].as_str() {
        if decision.get("policy_sha256").and_then(Value::as_str) != Some(policy_sha256) {
            bail!("decision effective policy is stale for {case_id}");
        }
    }
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
    if let Some(watch_class) = decision.get("watch_class") {
        let class = non_empty_string(Some(watch_class), "decision watch_class")?;
        if !WATCH_CLASSES.contains(&class.as_str()) {
            bail!("invalid watch_class for {case_id}: {class}");
        }
        let decision_kind = decision["decision"].as_str().unwrap_or_default();
        if decision_kind == "pass" && class != "not_applicable" {
            bail!("pass decision {case_id} must use watch_class=not_applicable");
        }
        if decision_kind != "pass" && class == "not_applicable" {
            bail!("non-pass decision {case_id} cannot use watch_class=not_applicable");
        }
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

fn row_run_id(row: &Value) -> Option<&str> {
    row.get("identity_ref")
        .and_then(|value| value.get("run_identity"))
        .and_then(|value| value.get("run_id"))
        .and_then(Value::as_str)
        .or_else(|| {
            row.get("identity")
                .and_then(|value| value.get("run_identity"))
                .and_then(|value| value.get("run_id"))
                .and_then(Value::as_str)
        })
}

pub(crate) fn review_kind(decision: &Value) -> Result<&'static str> {
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
    history: &HistoryLedger,
    resolved_history_ids: &BTreeSet<String>,
) -> Vec<Value> {
    let mut selected = rows
        .values()
        .filter(|row| {
            if row["classification"] == "expected_error" || !matches_filter(args, row) {
                return false;
            }
            let state = decisions.get(row["case_id"].as_str().unwrap_or_default());
            let perceptual = state.is_some_and(|state| state.contains(PERCEPTUAL_REVIEW));
            !perceptual
        })
        .cloned()
        .collect::<Vec<_>>();
    selected.sort_by_key(|row| {
        let unresolved = history.unresolved_open_ids(row, resolved_history_ids).len();
        (
            Reverse(unresolved),
            row["case_id"].as_str().unwrap_or_default().to_owned(),
        )
    });
    selected
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
        persist::append_decision(decisions_path, &decision)?;
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
    let route_clarity_clean = evidence
        .get("route_clarity")
        .and_then(|value| value.get("schema").and_then(Value::as_str))
        .is_some_and(|schema| schema == "termiflow.route_clarity.v1")
        && evidence["route_clarity"]["status"]
            .as_str()
            .is_some_and(|status| matches!(status, "clean" | "not_applicable"))
        && evidence["route_clarity"]["findings"]
            .as_array()
            .is_some_and(Vec::is_empty);
    let optional_errors_are_empty = evidence
        .get("errors")
        .is_none_or(|value| matches!(value, Value::Array(values) if values.is_empty()));

    optional_errors_are_empty
        && warnings.is_empty()
        && raw_errors.is_empty()
        && findings.is_empty()
        && geometry_errors.is_empty()
        && route_clarity_clean
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
        && untraced_fallback_edges.is_empty()
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

    let mut decision = json!({
        "schema": DECISION_SCHEMA,
        "review_kind": STRUCTURAL_PRESCREEN,
        "case_id": case_id,
        "frame_sha256": frame_sha256,
        "evidence_sha256": evidence_sha256,
        "policy_sha256": row["policy"]["sha256"],
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
    });
    if let Some(run_id) = row_run_id(row) {
        decision["run_id"] = Value::String(run_id.to_owned());
    }
    Ok(decision)
}

fn frame_payload(
    root: &Path,
    packet: &Path,
    row: &Value,
    history: &HistoryLedger,
    resolved_history_ids: &BTreeSet<String>,
) -> Result<Value> {
    let input_source = manifest_input_source(root, packet, row)?;
    let frame = common::validate_blob_ref(packet, &row["stdout"], "frame")?;
    let evidence_ref = row
        .get("evidence")
        .filter(|value| !value.is_null())
        .ok_or_else(|| anyhow!("reviewable row has no evidence: {}", row["case_id"]))?;
    let evidence_bytes = common::validate_blob_ref(packet, evidence_ref, "evidence")?;
    let evidence: Value =
        serde_json::from_slice(&evidence_bytes).context("parse review evidence")?;
    let evidence_hash = evidence_ref["sha256"].as_str().unwrap_or_default();
    let style_provenance = style_provenance(row, &evidence);
    let mut payload = json!({
        "schema": FRAME_SCHEMA,
        "case_id": row["case_id"],
        "run_id": row_run_id(row).unwrap_or_default(),
        "policy_sha256": row["policy"]["sha256"],
        "fixture": row["fixture"],
        "style": row["style"],
        "mode": row["mode"],
        "style_provenance": style_provenance,
        "frame_sha256": row["stdout"]["sha256"],
        "evidence_sha256": evidence_hash,
        "input": input_source,
        "frame": String::from_utf8(frame).context("frame is not UTF-8")?,
        "dimensions": row["dimensions"],
        "critic": evidence["critic"],
        "raw": evidence["raw"],
        "geometry": evidence["geometry"],
        "semantic": evidence["semantic"],
        "route_clarity": evidence["route_clarity"],
        "warnings": evidence["warnings"],
        "history": history.context(row, resolved_history_ids),
        "repair": {
            "optimized": evidence["optimized"],
            "repair_passes": evidence["repair_passes"],
            "layout_attempts": evidence["layout_attempts"],
            "layout_repairs_applied": evidence["layout_repairs_applied"],
        },
        "review_rubric": {
            "schema": "termiflow.visual_review.rubric.v1",
            "fresh_review": true,
            "machine_evidence_is_triage_only": true,
            "carry_forward_forbidden": true,
            "every_fresh_decision_requires_exact_cells": true,
            "watch_or_fail_requires_exact_cells": true,
            "checks": [
                "semantic topology and direction",
                "route continuity, crossings, overlap, portals, borders, and titles",
                "spacing, balance, density, corners, junctions, and seams",
                "shaft-to-arrowhead continuity and endpoint ownership",
                "labels, clipping, wrapping, CJK/emoji width, and fallback glyphs",
                "tiny visible artifacts in both the frame and matched homologs"
            ]
        },
        "decision_form": {
            "decision": "pass|fail|watch|unclear",
            "severity": "P0|P1|P2|P3",
            "dimensions": DIMENSIONS,
            "cells": [{"x": 0, "y": 0, "note": "required precise visible inspection anchor, including for a clean pass"}],
            "finding": "stable-human-readable-id-or-none",
            "observation": "what a human eye sees before source explanation",
            "hypothesis": "likely responsible layer or interaction",
            "expected_observation_if_true": "what the next check should show",
            "falsifier": "what would disprove the hypothesis",
            "affected_homologs": [],
            "next_command": "targeted test or review command",
            "reviewer": "ai|human",
            "review_kind": "perceptual",
            "history_resolution": {
                "schema": super::visual_history::HISTORY_RESOLUTION_SCHEMA,
                "status": "falsified|repaired|superseded",
                "history_ids": [],
                "note": "required to pass an open historical risk; otherwise record watch or fail",
            },
            "run_id": row_run_id(row).unwrap_or_default(),
            "policy_sha256": row["policy"]["sha256"],
        },
    });
    payload["decision_form"]["watch_class"] =
        Value::String("confirmed_flaw|topology_ambiguous|inconclusive|not_applicable".to_owned());
    payload["decision_form"]["owner_layer"] = Value::String(
        "routing|layout|glyph_projection|text|fixture|oracle|reviewer_calibration".to_owned(),
    );
    Ok(payload)
}

fn style_provenance(row: &Value, evidence: &Value) -> Value {
    let style_override = row
        .get("argv")
        .and_then(Value::as_array)
        .is_some_and(|argv| argv.iter().any(|arg| arg.as_str() == Some("--style")));
    json!({
        "policy_lane": if style_override {
            "canonical_requested_style"
        } else {
            "authored_policy_no_override"
        },
        "requested_style": row.get("style").cloned().unwrap_or(Value::Null),
        "effective_style": evidence
            .get("route_clarity")
            .and_then(|route| route.get("style"))
            .cloned()
            .unwrap_or(Value::Null),
        "style_override": style_override,
    })
}

fn manifest_input_source(root: &Path, packet: &Path, row: &Value) -> Result<String> {
    let input = &row["input"];
    match common::repository_file(root, input, "manifest input") {
        Ok(path) => String::from_utf8(common::require_file(&path, "manifest input")?)
            .context("input is not UTF-8"),
        Err(input_error) => holdout_input_source(packet, row)?.ok_or(input_error),
    }
}

fn holdout_input_source(packet: &Path, row: &Value) -> Result<Option<String>> {
    let schema_manifest = packet.join("schema_manifest.json");
    if !schema_manifest.is_file() {
        return Ok(None);
    }
    let summary = common::load_json(&packet.join("summary.json"), "packet summary")?;
    let expected_manifest_hash = summary
        .get("schema_manifest_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("packet summary has no schema manifest hash"))?;
    let actual_manifest_hash = common::sha256_file(&schema_manifest)?;
    if actual_manifest_hash != expected_manifest_hash {
        bail!(
            "packet schema manifest hash mismatch: expected {expected_manifest_hash}, got {actual_manifest_hash}"
        );
    }
    let schema = common::load_json(&schema_manifest, "packet schema manifest")?;
    let Some(holdouts) = schema.get("holdouts").and_then(Value::as_array) else {
        return Ok(None);
    };
    let fixture = row
        .get("fixture")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("holdout review row has no fixture"))?;
    let variant_id = fixture
        .rsplit_once("--")
        .map(|(_, variant)| variant)
        .ok_or_else(|| anyhow!("holdout review fixture has no variant: {fixture}"))?;
    let style = row
        .get("style")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("holdout review row has no style"))?;
    let mode = row
        .get("mode")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("holdout review row has no mode"))?;
    let Some(holdout) = holdouts.iter().find(|holdout| {
        holdout.get("variant_id").and_then(Value::as_str) == Some(variant_id)
            && holdout.get("style").and_then(Value::as_str) == Some(style)
            && holdout.get("mode").and_then(Value::as_str) == Some(mode)
    }) else {
        return Ok(None);
    };
    let source = holdout
        .get("source")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("holdout source is missing for {variant_id}"))?;
    let expected_hash = holdout
        .get("source_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("holdout source hash is missing for {variant_id}"))?;
    if common::sha256_bytes(source.as_bytes()) != expected_hash {
        bail!("holdout source hash is stale for {variant_id}");
    }
    Ok(Some(source.to_owned()))
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
    use std::fs;

    #[test]
    fn structural_predicate_rejects_every_machine_signal() {
        let clean = json!({
            "schema": common::EVIDENCE_SCHEMA,
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
            }
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

        let mut untraced = clean.clone();
        untraced["geometry"]["untraced_fallback_edges"] = json!(["edge:0:A->B"]);
        assert!(!evidence_is_structurally_clean(&untraced));

        let mut incomplete = clean.clone();
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

        let mut route_risk = clean;
        route_risk["route_clarity"] = json!({
            "schema": "termiflow.route_clarity.v1",
            "status": "risk",
            "findings": [{"code": "declared_edge_missing"}]
        });
        assert!(!evidence_is_structurally_clean(&route_risk));
    }

    #[test]
    fn style_provenance_distinguishes_requested_and_authored_policy_lanes() {
        let evidence = json!({"route_clarity": {"style": "unicode"}});
        let canonical = json!({"argv": ["tw", "--style", "ascii"], "style": "ascii"});
        let authored = json!({"argv": ["tw"], "style": "ascii"});

        assert_eq!(
            style_provenance(&canonical, &evidence),
            json!({
                "policy_lane": "canonical_requested_style",
                "requested_style": "ascii",
                "effective_style": "unicode",
                "style_override": true,
            })
        );
        assert_eq!(
            style_provenance(&authored, &evidence),
            json!({
                "policy_lane": "authored_policy_no_override",
                "requested_style": "ascii",
                "effective_style": "unicode",
                "style_override": false,
            })
        );
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
    fn fresh_perceptual_review_rejects_boilerplate_and_unbound_watches() {
        let row = json!({
            "case_id": "case",
            "stdout": {"sha256": "frame"},
            "evidence": {"sha256": "evidence"}
        });
        let rows = BTreeMap::from([("case".to_owned(), row)]);
        let mut decision = json!({
            "schema": DECISION_SCHEMA,
            "case_id": "case",
            "frame_sha256": "frame",
            "evidence_sha256": "evidence",
            "decision": "watch",
            "severity": "P2",
            "watch_class": "topology_ambiguous",
            "dimensions": ["route", "readability"],
            "cells": [],
            "finding": "route-watch",
            "observation": GENERIC_WATCH_OBSERVATION,
            "hypothesis": GENERIC_WATCH_HYPOTHESIS,
            "expected_observation_if_true": "the matched frame keeps the route attached",
            "falsifier": "a detached route or visible overlap",
            "owner_layer": "routing",
            "affected_homologs": [],
            "next_command": "scripts/review_visual_packet.sh --packet h155 --decisions fresh --next",
            "reviewer": "ai",
            "review_kind": PERCEPTUAL_REVIEW,
            "timestamp": "now"
        });
        validate_decision(&decision, &rows).expect("base decision should validate");
        assert!(validate_fresh_decision(&decision).is_err());

        decision["observation"] = json!("The first rail touches the title row at x=7,y=8.");
        decision["hypothesis"] = json!("The title-owned portal margin is one cell too small.");
        assert!(validate_fresh_decision(&decision).is_err());

        decision["cells"] = json!([{"x": 7, "y": 8, "note": "first rail touches title row"}]);
        validate_fresh_decision(&decision).expect("specific watch should validate");
    }

    #[test]
    fn fresh_perceptual_pass_requires_anchor_and_rejects_generated_template() {
        let row = json!({
            "case_id": "case",
            "stdout": {"sha256": "frame"},
            "evidence": {"sha256": "evidence"}
        });
        let rows = BTreeMap::from([("case".to_owned(), row)]);
        let mut decision = json!({
            "schema": DECISION_SCHEMA,
            "case_id": "case",
            "frame_sha256": "frame",
            "evidence_sha256": "evidence",
            "decision": "pass",
            "severity": "P3",
            "watch_class": "not_applicable",
            "dimensions": ["semantic", "route", "readability"],
            "cells": [],
            "finding": "none",
            "observation": "Fresh fixture ascii optimized frame reviewed for semantic direction, route continuity, border/portal ownership, spacing, glyph junctions, text/clipping, and tiny stray marks; no visible defect was found.",
            "hypothesis": GENERATED_PASS_HYPOTHESIS,
            "expected_observation_if_true": "the inspected anchor remains readable in the matched homolog",
            "falsifier": "a visible defect appears at the inspected anchor",
            "affected_homologs": [],
            "next_command": "scripts/review_visual_packet.sh --packet current --decisions fresh --next",
            "reviewer": "ai",
            "review_kind": PERCEPTUAL_REVIEW,
            "timestamp": "now"
        });
        validate_decision(&decision, &rows).expect("base pass should validate");
        assert!(validate_fresh_decision(&decision).is_err());

        decision["cells"] =
            json!([{"x": 8, "y": 3, "note": "right edge of the receiver box remains clear"}]);
        assert!(validate_fresh_decision(&decision).is_err());

        decision["observation"] = json!("At x=8,y=3 the receiver box edge, arrow junction, and adjacent whitespace remain distinct in this frame.");
        decision["hypothesis"] =
            json!("The receiver boundary owns the junction without stealing a route cell.");
        validate_fresh_decision(&decision)
            .expect("anchored frame-specific clean pass should validate");
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
            history: None,
            fresh: false,
            fixture: None,
            style: None,
            mode: None,
            reviewer: "ai".to_owned(),
            next: true,
            record: None,
            rebind_from_packet: None,
            rebind_from_decisions: None,
            prescreen_clean: false,
            validate: false,
        };
        assert_eq!(
            selected_rows(
                &args,
                &rows,
                &decisions,
                &HistoryLedger::default(),
                &BTreeSet::new()
            )
            .len(),
            1
        );
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

    #[test]
    fn source_only_holdout_review_uses_packet_manifest_source() {
        let packet = std::env::temp_dir().join(format!(
            "termiflow-review-holdout-{}-{}",
            std::process::id(),
            common::now_label()
        ));
        fs::create_dir_all(&packet).expect("create packet directory");
        let source = "graph TD\nA[In] --> B[Out]\n";
        common::write_json(
            &packet.join("schema_manifest.json"),
            &json!({
                "holdouts": [{
                    "variant_id": "holdout_td",
                    "style": "unicode",
                    "mode": "default",
                    "source": source,
                    "source_sha256": common::sha256_bytes(source.as_bytes())
                }]
            }),
        )
        .expect("write schema manifest");
        let schema_hash = common::sha256_file(&packet.join("schema_manifest.json"))
            .expect("hash schema manifest");
        common::write_json(
            &packet.join("summary.json"),
            &json!({"schema_manifest_sha256": schema_hash}),
        )
        .expect("write packet summary");
        let row = json!({
            "input": "termiflow-holdout-input-transient.md",
            "fixture": "case--holdout_td",
            "style": "unicode",
            "mode": "default"
        });

        let actual = manifest_input_source(Path::new("/missing-root"), &packet, &row)
            .expect("resolve source-only holdout input");
        assert_eq!(actual, source);

        common::write_json(
            &packet.join("schema_manifest.json"),
            &json!({
                "holdouts": [{
                    "variant_id": "holdout_td",
                    "style": "unicode",
                    "mode": "default",
                    "source": "graph TD\nA[Changed] --> B[Out]\n",
                    "source_sha256": common::sha256_bytes(b"graph TD\nA[Changed] --> B[Out]\n")
                }]
            }),
        )
        .expect("tamper schema manifest");
        let error = manifest_input_source(Path::new("/missing-root"), &packet, &row)
            .expect_err("tampered schema manifest must be rejected");
        assert!(error.to_string().contains("schema manifest hash mismatch"));
        fs::remove_dir_all(packet).expect("remove packet directory");
    }
}
