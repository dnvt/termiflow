use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use super::common;
use super::provenance;

const BASELINE_SCHEMA: &str = "termiflow.quality_baseline.v1";
const COMPLETE_SCHEMA: &str = "termiflow.visual_audit.complete.v1";
const LAYOUT_BUDGET_WARNING_PREFIX: &str = "layout repair candidate budget capped at ";

type Signature = (String, String, String, String, String, String);

#[derive(Debug)]
pub struct ValidateArgs {
    pub packet: PathBuf,
    pub baseline: PathBuf,
    pub strict_quality: bool,
}

pub fn run(args: ValidateArgs) -> Result<()> {
    let root = std::env::current_dir().context("resolve repository root")?;
    let packet = resolve_from_root(&root, &args.packet);
    let baseline_path = resolve_from_root(&root, &args.baseline);
    let baseline = load_baseline(&baseline_path)?;
    validate_packet_integrity(&root, &packet, &baseline, args.strict_quality)
}

fn resolve_from_root(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn load_baseline(path: &Path) -> Result<Value> {
    let baseline = common::load_json(path, "quality baseline")?;
    if baseline.get("schema").and_then(Value::as_str) != Some(BASELINE_SCHEMA) {
        bail!("quality baseline schema must be {BASELINE_SCHEMA}");
    }
    let identity = baseline
        .get("identity")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("baseline identity must be an object"))?;
    let policy = baseline
        .get("exception_policy")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("baseline exception_policy must be an object"))?;
    for field in ["source_commit", "cargo_lock_sha256", "display_profile"] {
        non_empty_string(identity.get(field), &format!("baseline.identity.{field}"))?;
    }
    for field in ["styles", "modes"] {
        unique_string_array(identity.get(field), &format!("baseline.identity.{field}"))?;
    }
    for field in ["owner", "expires", "hypothesis", "next_command"] {
        non_empty_string(
            policy.get(field),
            &format!("baseline.exception_policy.{field}"),
        )?;
    }
    Ok(baseline)
}

fn non_empty_string(value: Option<&Value>, label: &str) -> Result<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("{label} must be a non-empty string"))
}

fn unique_string_array(value: Option<&Value>, label: &str) -> Result<Vec<String>> {
    let values = value
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("{label} must be a unique non-empty list"))?;
    if values.is_empty() {
        bail!("{label} must be a unique non-empty list");
    }
    let mut result = Vec::new();
    for value in values {
        let item = value
            .as_str()
            .filter(|item| !item.is_empty())
            .ok_or_else(|| anyhow!("{label} must contain only non-empty strings"))?;
        if result.iter().any(|existing| existing == item) {
            bail!("{label} must contain unique values");
        }
        result.push(item.to_owned());
    }
    Ok(result)
}

fn validate_identity(root: &Path, identity: &Value, baseline: &Value, strict: bool) -> Result<()> {
    provenance::validate_identity(identity)?;
    let identity = identity
        .as_object()
        .ok_or_else(|| anyhow!("identity.json must contain an object"))?;
    for field in ["source_commit", "cargo_lock_sha256", "display_profile"] {
        non_empty_string(identity.get(field), &format!("identity.json.{field}"))?;
    }
    if !identity
        .get("worktree_dirty")
        .is_some_and(Value::is_boolean)
    {
        bail!("identity.json.worktree_dirty must be boolean");
    }
    let baseline_identity = baseline["identity"]
        .as_object()
        .ok_or_else(|| anyhow!("baseline.identity must be an object"))?;
    for field in ["cargo_lock_sha256", "display_profile"] {
        if identity.get(field) != baseline_identity.get(field) {
            bail!("packet {field} does not match the quality baseline");
        }
    }
    if strict && identity["worktree_dirty"] == Value::Bool(true) {
        bail!("strict quality validation requires a clean source worktree");
    }
    let baseline_commit = non_empty_string(
        baseline_identity.get("source_commit"),
        "baseline.identity.source_commit",
    )?;
    let packet_commit =
        non_empty_string(identity.get("source_commit"), "identity.json.source_commit")?;
    if strict {
        let current_commit = common::run_text(&["git", "rev-parse", "HEAD"], root);
        if current_commit.is_empty() || current_commit != packet_commit {
            bail!("strict quality validation requires packet source commit to match current HEAD");
        }
    }
    if packet_commit == "unknown"
        || !common::git_is_ancestor(root, &baseline_commit, &packet_commit)
    {
        bail!("packet source commit is not the baseline commit or a descendant of it; regenerate the packet after the baseline was established");
    }
    Ok(())
}

fn load_manifest(packet: &Path) -> Result<Vec<Value>> {
    let bytes = common::require_file(&packet.join("manifest.jsonl"), "manifest")?;
    let text = String::from_utf8(bytes).context("manifest is not UTF-8")?;
    let mut rows = Vec::new();
    for (number, line) in text.lines().enumerate() {
        if line.is_empty() {
            bail!("manifest line {} is empty", number + 1);
        }
        let row: Value = serde_json::from_str(line)
            .with_context(|| format!("manifest line {} is invalid JSON", number + 1))?;
        if !row.is_object() {
            bail!("manifest line {} must be an object", number + 1);
        }
        if row.get("schema").and_then(Value::as_str) != Some(common::AUDIT_SCHEMA) {
            bail!("manifest line {} has the wrong schema", number + 1);
        }
        rows.push(row);
    }
    if rows.is_empty() {
        bail!("manifest is empty");
    }
    Ok(rows)
}

fn load_repository_metadata(root: &Path) -> Result<BTreeMap<String, common::FixtureMetadata>> {
    let metadata_path = root.join("tests/fixtures/metadata.json");
    let input_root = root.join("tests/fixtures/inputs");
    common::load_metadata(&metadata_path, &input_root).map(|(metadata, _)| metadata)
}

fn validate_packet_metadata(
    root: &Path,
    packet: &Path,
) -> Result<BTreeMap<String, common::FixtureMetadata>> {
    let metadata = load_repository_metadata(root)?;
    let packet_metadata = common::load_json(&packet.join("metadata.json"), "packet metadata")?;
    let repository_metadata = common::load_json(
        &root.join("tests/fixtures/metadata.json"),
        "repository metadata",
    )?;
    if packet_metadata != repository_metadata {
        bail!("packet metadata does not match tests/fixtures/metadata.json");
    }
    Ok(metadata)
}

fn expected_case_ids(
    root: &Path,
    metadata: &BTreeMap<String, common::FixtureMetadata>,
    styles: &[String],
    modes: &[String],
) -> Result<BTreeSet<String>> {
    let mut expected = BTreeSet::new();
    for fixture in metadata.keys() {
        let input_path = root
            .join("tests/fixtures/inputs")
            .join(format!("{fixture}.md"));
        let input = common::require_file(&input_path, &format!("fixture input {fixture}"))?;
        for style in styles {
            for mode in modes {
                expected.insert(common::case_id(&input, fixture, style, mode));
            }
        }
    }
    Ok(expected)
}

fn validate_timing_file(packet: &Path, rows: &[Value]) -> Result<()> {
    let bytes = common::require_file(&packet.join("timings.jsonl"), "timings")?;
    let text = String::from_utf8(bytes).context("timings is not UTF-8")?;
    let mut timing_ids = BTreeSet::new();
    for (number, line) in text.lines().enumerate() {
        let record: Value = serde_json::from_str(line)
            .with_context(|| format!("timings line {} is invalid JSON", number + 1))?;
        let case_id = non_empty_string(
            record.get("case_id"),
            &format!("timings line {} case_id", number + 1),
        )?;
        let duration = record
            .get("duration_ns")
            .and_then(Value::as_u64)
            .ok_or_else(|| anyhow!("timings line {} has an invalid duration_ns", number + 1))?;
        let _ = duration;
        if !timing_ids.insert(case_id) {
            bail!("timings line {} has a duplicate case_id", number + 1);
        }
    }
    let row_ids: BTreeSet<String> = rows
        .iter()
        .filter_map(|row| row["case_id"].as_str().map(ToOwned::to_owned))
        .collect();
    if timing_ids != row_ids {
        bail!("timings do not cover exactly the manifest case IDs");
    }
    Ok(())
}

fn validate_evidence(
    packet: &Path,
    row: &Value,
    classification: &str,
    strict: bool,
) -> Result<(Vec<Signature>, BTreeSet<String>)> {
    let fixture = row["fixture"].as_str().unwrap_or("unknown");
    let evidence_ref = row.get("evidence");
    if classification == "expected_error" {
        if evidence_ref.is_some_and(|value| !value.is_null()) {
            bail!("{fixture}: expected-error row must not have evidence");
        }
        return Ok((Vec::new(), BTreeSet::new()));
    }
    let evidence_ref = evidence_ref
        .filter(|value| !value.is_null())
        .ok_or_else(|| anyhow!("{fixture}: successful row is missing evidence"))?;
    let raw_bytes = common::validate_blob_ref(packet, evidence_ref, "evidence")?;
    let evidence: Value = serde_json::from_slice(&raw_bytes)
        .with_context(|| format!("{fixture}: evidence is invalid JSON"))?;
    if evidence.get("schema").and_then(Value::as_str) != Some(common::EVIDENCE_SCHEMA) {
        bail!("{fixture}: evidence schema mismatch");
    }
    let warnings = evidence
        .get("warnings")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("{fixture}: evidence.warnings must be a string list"))?;
    if !warnings.iter().all(Value::is_string) {
        bail!("{fixture}: evidence.warnings must be a string list");
    }
    if strict
        && warnings
            .iter()
            .filter_map(Value::as_str)
            .any(|warning| warning.starts_with(LAYOUT_BUDGET_WARNING_PREFIX))
    {
        bail!("{fixture}: layout repair budget warning requires one-frame review");
    }
    if evidence.get("display") != row["dimensions"].get("display") {
        bail!("{fixture}: row/evidence display dimensions differ");
    }
    let raw_errors = evidence["raw"]["errors"]
        .as_array()
        .ok_or_else(|| anyhow!("{fixture}: raw.errors must be a string list"))?;
    let findings = evidence["critic"]["findings"]
        .as_array()
        .ok_or_else(|| anyhow!("{fixture}: critic.findings must be an object list"))?;
    let geometry_errors = evidence["geometry"]["errors"]
        .as_array()
        .ok_or_else(|| anyhow!("{fixture}: geometry.errors must be a string list"))?;
    if !raw_errors.iter().all(Value::is_string)
        || !geometry_errors.iter().all(Value::is_string)
        || !findings.iter().all(Value::is_object)
    {
        bail!("{fixture}: evidence layer arrays contain invalid values");
    }
    if !geometry_errors.is_empty() {
        bail!("{fixture}: geometry errors are not permitted");
    }
    if row["findings"]["raw_errors"].as_u64() != Some(raw_errors.len() as u64)
        || row["findings"]["critic"].as_u64() != Some(findings.len() as u64)
        || row["findings"]["geometry_errors"].as_u64() != Some(geometry_errors.len() as u64)
    {
        bail!("{fixture}: evidence finding counts do not match the manifest");
    }
    let mut signatures = Vec::new();
    for message in raw_errors.iter().filter_map(Value::as_str) {
        signatures.push((
            fixture.to_owned(),
            row["style"].as_str().unwrap_or_default().to_owned(),
            row["mode"].as_str().unwrap_or_default().to_owned(),
            "raw".to_owned(),
            "RawFrameError".to_owned(),
            message.to_owned(),
        ));
    }
    let mut codes = BTreeSet::new();
    for finding in findings {
        let code = non_empty_string(
            finding.get("code"),
            &format!("{fixture}: critic finding code"),
        )?;
        let message = non_empty_string(
            finding.get("message"),
            &format!("{fixture}: critic finding message"),
        )?;
        codes.insert(code.clone());
        signatures.push((
            fixture.to_owned(),
            row["style"].as_str().unwrap_or_default().to_owned(),
            row["mode"].as_str().unwrap_or_default().to_owned(),
            "critic".to_owned(),
            code,
            message,
        ));
    }
    Ok((signatures, codes))
}

fn expected_error_stderr(
    root: &Path,
    fixture: &str,
    style: &str,
    record: &common::FixtureMetadata,
) -> Result<Vec<u8>> {
    let path = record
        .expected_stderr
        .as_deref()
        .map(|path| {
            common::repository_file(
                root,
                &Value::String(path.to_owned()),
                &format!("{fixture} expected stderr"),
            )
        })
        .transpose()?
        .unwrap_or_else(|| {
            root.join("tests/fixtures/expected")
                .join(format!("{fixture}.{style}.txt"))
        });
    common::require_file(&path, &format!("{fixture} expected stderr"))
}

fn trim_newlines(value: &[u8]) -> &[u8] {
    let mut end = value.len();
    while end > 0 && matches!(value[end - 1], b'\r' | b'\n') {
        end -= 1;
    }
    &value[..end]
}

#[allow(clippy::too_many_arguments)]
fn validate_rows(
    root: &Path,
    packet: &Path,
    rows: &[Value],
    metadata: &BTreeMap<String, common::FixtureMetadata>,
    packet_identity: &Value,
    styles: &[String],
    modes: &[String],
    strict: bool,
) -> Result<(BTreeSet<Signature>, BTreeSet<String>)> {
    let expected_ids = expected_case_ids(root, metadata, styles, modes)?;
    let mut seen = BTreeSet::new();
    let mut signatures = BTreeSet::new();
    let mut codes = BTreeSet::new();
    for row in rows {
        let case_id = non_empty_string(row.get("case_id"), "manifest case_id")?;
        if !seen.insert(case_id.clone()) {
            bail!("manifest contains a duplicate case_id");
        }
        let fixture = non_empty_string(row.get("fixture"), "manifest fixture")?;
        let style = non_empty_string(row.get("style"), &format!("{fixture} style"))?;
        let mode = non_empty_string(row.get("mode"), &format!("{fixture} mode"))?;
        let classification = non_empty_string(
            row.get("classification"),
            &format!("{fixture} classification"),
        )?;
        let record = metadata
            .get(&fixture)
            .ok_or_else(|| anyhow!("manifest has an unknown fixture: {fixture}"))?;
        if !styles.contains(&style)
            || !modes.contains(&mode)
            || !common::KINDS.contains(&classification.as_str())
        {
            bail!("{fixture}: invalid style/mode/classification");
        }
        if classification != record.kind
            || row["direction"].as_str() != Some(record.direction.as_str())
        {
            bail!("{fixture}: row does not match fixture metadata");
        }
        if row["stderr"]["policy"].as_str() != Some(record.stderr_policy.as_str()) {
            bail!("{fixture}: stderr policy mismatch");
        }
        let input_path = common::repository_file(root, &row["input"], &format!("{fixture} input"))?;
        let actual_case_id = common::case_id(&fs::read(&input_path)?, &fixture, &style, &mode);
        if actual_case_id != case_id {
            bail!("{fixture}: case_id does not match its input/style/mode");
        }
        if !expected_ids.contains(&case_id) {
            bail!("{fixture}: case_id is outside the declared packet matrix");
        }
        if row.get("identity") != Some(packet_identity) {
            bail!("{fixture}: row identity differs from identity.json");
        }
        let stdout =
            common::validate_blob_ref(packet, &row["stdout"], &format!("{fixture} stdout"))?;
        let stderr =
            common::validate_blob_ref(packet, &row["stderr"], &format!("{fixture} stderr"))?;
        let dimensions = common::dimensions(&stdout);
        for field in ["stdout_rows", "stdout_max_codepoints", "stdout_bytes"] {
            if row["dimensions"].get(field) != dimensions.get(field) {
                bail!("{fixture}: stdout dimensions do not match the frame");
            }
        }
        let status = row["status"]
            .as_i64()
            .ok_or_else(|| anyhow!("{fixture}: status must be an integer"))?;
        match classification.as_str() {
            "success" if status != 0 || stdout.is_empty() || !stderr.is_empty() => {
                bail!("{fixture}: success stream contract failed")
            }
            "warning" => {
                if status != 0 || stdout.is_empty() {
                    bail!("{fixture}: warning stream contract failed");
                }
                let text = String::from_utf8_lossy(&stderr);
                for pattern in &record.stderr_contains {
                    if !text.contains(pattern) {
                        bail!("{fixture}: warning stderr lacks {pattern:?}");
                    }
                }
            }
            "expected_error" => {
                if status == 0 || !stdout.is_empty() {
                    bail!("{fixture}: expected-error stream contract failed");
                }
                let expected = expected_error_stderr(root, &fixture, &style, record)?;
                if trim_newlines(&stderr) != trim_newlines(&expected) {
                    bail!("{fixture}: expected-error stderr differs from golden error");
                }
            }
            _ => {}
        }
        let (row_signatures, row_codes) = validate_evidence(packet, row, &classification, strict)?;
        if classification == "expected_error"
            && row.get("findings")
                != Some(&json!({"critic": 0, "geometry_errors": 0, "raw_errors": 0}))
        {
            bail!("{fixture}: expected-error findings must be zero");
        }
        signatures.extend(row_signatures);
        codes.extend(row_codes);
    }
    if seen != expected_ids {
        bail!(
            "manifest coverage mismatch: expected {} rows, found {}",
            expected_ids.len(),
            seen.len()
        );
    }
    Ok((signatures, codes))
}

fn expand_baseline(
    baseline: &Value,
    metadata: &BTreeMap<String, common::FixtureMetadata>,
) -> Result<BTreeSet<Signature>> {
    let exceptions = baseline["exceptions"]
        .as_array()
        .ok_or_else(|| anyhow!("baseline.exceptions must be a list"))?;
    let mut expected = BTreeSet::new();
    for exception in exceptions {
        let fixture = non_empty_string(exception.get("fixture"), "baseline exception fixture")?;
        if !metadata.contains_key(&fixture) {
            bail!("invalid baseline exception target: {fixture}");
        }
        let styles = exception["styles"]
            .as_array()
            .ok_or_else(|| anyhow!("baseline exception styles must be a list"))?;
        let modes = exception["modes"]
            .as_array()
            .ok_or_else(|| anyhow!("baseline exception modes must be a list"))?;
        let layer = non_empty_string(exception.get("layer"), "baseline exception layer")?;
        if layer != "raw" && layer != "critic" {
            bail!("invalid baseline exception layer: {layer}");
        }
        let code = non_empty_string(exception.get("code"), "baseline exception code")?;
        let message = non_empty_string(exception.get("message"), "baseline exception message")?;
        for style in styles
            .iter()
            .map(Value::as_str)
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| anyhow!("baseline styles must be strings"))?
        {
            if !common::STYLES.contains(&style) {
                bail!("unsupported baseline style: {style}");
            }
            for mode in modes
                .iter()
                .map(Value::as_str)
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| anyhow!("baseline modes must be strings"))?
            {
                if !common::MODES.contains(&mode) {
                    bail!("unsupported baseline mode: {mode}");
                }
                let signature = (
                    fixture.clone(),
                    style.to_owned(),
                    mode.to_owned(),
                    layer.clone(),
                    code.clone(),
                    message.clone(),
                );
                if !expected.insert(signature.clone()) {
                    bail!("duplicate expanded baseline exception: {:?}", signature);
                }
            }
        }
    }
    Ok(expected)
}

fn validate_quality(
    baseline: &Value,
    metadata: &BTreeMap<String, common::FixtureMetadata>,
    observed: &BTreeSet<Signature>,
    critic_codes: &BTreeSet<String>,
    strict: bool,
) -> Result<()> {
    let forbidden = baseline["forbidden_codes"]
        .as_array()
        .ok_or_else(|| anyhow!("baseline.forbidden_codes must be a string list"))?;
    let forbidden_observed: Vec<_> = forbidden
        .iter()
        .filter_map(Value::as_str)
        .filter(|code| critic_codes.contains(*code))
        .collect();
    if !forbidden_observed.is_empty() {
        bail!(
            "forbidden critic findings: {}",
            forbidden_observed.join(", ")
        );
    }
    if strict {
        let expected = expand_baseline(baseline, metadata)?;
        let missing: Vec<_> = expected.difference(observed).collect();
        let unexpected: Vec<_> = observed.difference(&expected).collect();
        if !missing.is_empty() || !unexpected.is_empty() {
            bail!(
                "quality baseline drift; missing={} unexpected={}",
                missing.len(),
                unexpected.len()
            );
        }
    }
    Ok(())
}

fn validate_packet_integrity(
    root: &Path,
    packet: &Path,
    baseline: &Value,
    strict: bool,
) -> Result<()> {
    if !packet.is_dir() {
        bail!("packet directory does not exist: {}", packet.display());
    }
    let packet_identity = common::load_json(&packet.join("identity.json"), "identity")?;
    validate_identity(root, &packet_identity, baseline, strict)?;
    let metadata = validate_packet_metadata(root, packet)?;
    let summary = common::load_json(&packet.join("summary.json"), "summary")?;
    if summary["schema"].as_str() != Some(common::SUMMARY_SCHEMA) {
        bail!("summary schema must be {}", common::SUMMARY_SCHEMA);
    }
    let styles = summary["styles"]
        .as_array()
        .ok_or_else(|| anyhow!("summary.styles is invalid"))?
        .iter()
        .map(|value| value.as_str().map(ToOwned::to_owned))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| anyhow!("summary.styles is invalid"))?;
    let modes = summary["modes"]
        .as_array()
        .ok_or_else(|| anyhow!("summary.modes is invalid"))?
        .iter()
        .map(|value| value.as_str().map(ToOwned::to_owned))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| anyhow!("summary.modes is invalid"))?;
    if styles.is_empty()
        || styles
            .iter()
            .any(|style| !common::STYLES.contains(&style.as_str()))
        || styles.len() != styles.iter().collect::<BTreeSet<_>>().len()
    {
        bail!("summary.styles is invalid");
    }
    if modes.is_empty()
        || modes
            .iter()
            .any(|mode| !common::MODES.contains(&mode.as_str()))
        || modes.len() != modes.iter().collect::<BTreeSet<_>>().len()
    {
        bail!("summary.modes is invalid");
    }
    if summary["binary"] != packet_identity["binary"] {
        bail!("summary binary does not match identity.json");
    }
    if json!(styles) != baseline["identity"]["styles"]
        || json!(modes) != baseline["identity"]["modes"]
    {
        bail!("packet style/mode matrix does not match the quality baseline");
    }
    let rows = load_manifest(packet)?;
    let expected_rows = metadata.len() * styles.len() * modes.len();
    if summary["expected_rows"].as_u64() != Some(expected_rows as u64)
        || summary["actual_rows"].as_u64() != Some(rows.len() as u64)
    {
        bail!("summary row counts do not match metadata or manifest");
    }
    if summary["primary_rows"].as_u64()
        != Some(
            rows.iter()
                .filter(|row| row["classification"] != "expected_error")
                .count() as u64,
        )
        || summary["expected_error_rows"].as_u64()
            != Some(
                rows.iter()
                    .filter(|row| row["classification"] == "expected_error")
                    .count() as u64,
            )
        || summary["warning_rows"].as_u64()
            != Some(
                rows.iter()
                    .filter(|row| row["classification"] == "warning")
                    .count() as u64,
            )
    {
        bail!("summary classification counts are incorrect");
    }
    if summary["failures"] != json!([]) {
        bail!("visual audit summary contains failures");
    }
    validate_timing_file(packet, &rows)?;
    let (observed, codes) = validate_rows(
        root,
        packet,
        &rows,
        &metadata,
        &packet_identity,
        &styles,
        &modes,
        strict,
    )?;
    validate_quality(baseline, &metadata, &observed, &codes, strict)?;
    let complete = common::load_json(&packet.join("COMPLETE.json"), "completion marker")?;
    if complete["schema"].as_str() != Some(COMPLETE_SCHEMA) {
        bail!("completion schema must be {COMPLETE_SCHEMA}");
    }
    let manifest_hash = common::sha256_file(&packet.join("manifest.jsonl"))?;
    if complete["rows"].as_u64() != Some(rows.len() as u64)
        || complete["manifest_sha256"].as_str() != Some(manifest_hash.as_str())
    {
        bail!("completion marker does not match the manifest");
    }
    let (digest, listing) = common::deterministic_digest(packet)?;
    if common::require_file(&packet.join("PACKET.sha256"), "packet listing")? != listing.as_bytes()
    {
        bail!("PACKET.sha256 does not match packet contents");
    }
    if complete["packet_sha256"].as_str() != Some(digest.as_str()) {
        bail!("completion packet digest does not match packet contents");
    }
    println!(
        "visual validation passed: {} ({} rows, {} findings, {})",
        packet.display(),
        rows.len(),
        observed.len(),
        if strict { "strict" } else { "integrity" }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> BTreeMap<String, common::FixtureMetadata> {
        let mut values = BTreeMap::new();
        values.insert(
            "fixture_td".to_owned(),
            common::FixtureMetadata {
                kind: "success".to_owned(),
                direction: "TD".to_owned(),
                stderr_policy: "empty".to_owned(),
                stderr_contains: Vec::new(),
                expected_stderr: None,
            },
        );
        values
    }

    #[test]
    fn compact_baseline_expands_each_matrix_cell() {
        let baseline = json!({
            "exceptions": [{
                "fixture": "fixture_td",
                "styles": ["ascii", "unicode"],
                "modes": ["default", "optimized"],
                "layer": "critic",
                "code": "Example",
                "message": "known"
            }]
        });
        assert_eq!(
            expand_baseline(&baseline, &metadata())
                .expect("expand baseline")
                .len(),
            4
        );
    }

    #[test]
    fn strict_quality_rejects_unlisted_finding() {
        let baseline = json!({ "forbidden_codes": [], "exceptions": [] });
        let observed = BTreeSet::from([(
            "fixture_td".to_owned(),
            "ascii".to_owned(),
            "default".to_owned(),
            "raw".to_owned(),
            "RawFrameError".to_owned(),
            "new".to_owned(),
        )]);
        assert!(
            validate_quality(&baseline, &metadata(), &observed, &BTreeSet::new(), true).is_err()
        );
    }

    #[test]
    fn strict_evidence_rejects_layout_budget_warning() {
        let root =
            std::env::temp_dir().join(format!("termiflow-qa-evidence-{}", std::process::id()));
        fs::create_dir_all(&root).expect("create evidence directory");
        let evidence = json!({
            "schema": common::EVIDENCE_SCHEMA,
            "display": {"width": 1, "height": 1, "non_space_cells": 1},
            "warnings": ["layout repair candidate budget capped at 32; omitted 1 candidate(s)"],
            "raw": {"errors": []},
            "critic": {"findings": []},
            "geometry": {"errors": []}
        });
        let path = root.join("evidence.json");
        let bytes = serde_json::to_vec(&evidence).expect("serialize evidence");
        fs::write(&path, &bytes).expect("write evidence");
        let row = json!({
            "fixture": "fixture_td",
            "style": "ascii",
            "mode": "default",
            "dimensions": {"display": evidence["display"]},
            "findings": {"critic": 0, "geometry_errors": 0, "raw_errors": 0},
            "evidence": {"path": "evidence.json", "bytes": bytes.len(), "sha256": common::sha256_bytes(&bytes)}
        });
        assert!(validate_evidence(&root, &row, "success", true).is_err());
        fs::remove_dir_all(root).expect("remove evidence directory");
    }
}
