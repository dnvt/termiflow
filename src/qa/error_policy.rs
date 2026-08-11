use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use super::{common, persist};

pub const RECORD_SCHEMA: &str = "termiflow.expected_error_policy.record.v1";
const NEXT_SCHEMA: &str = "termiflow.expected_error_policy.next.v1";
const COVERAGE_SCHEMA: &str = "termiflow.expected_error_policy.coverage.v1";
const RECORD_SCHEMA_PATH: &str = "tests/fixtures/error_policy_record.schema.json";

const RECORD_FIELDS: &[&str] = &[
    "schema",
    "case_id",
    "packet",
    "run_id",
    "policy_sha256",
    "record_schema_sha256",
    "fixture",
    "direction",
    "style",
    "mode",
    "input",
    "source_sha256",
    "input_sha256",
    "status",
    "stdout",
    "stderr",
    "expected_stderr",
    "stderr_policy",
    "stderr_contains",
    "result",
    "observation",
    "owner",
    "hypothesis",
    "expected_observation_if_true",
    "falsifier",
    "next_command",
    "reviewer",
    "timestamp",
];

#[derive(Debug)]
pub struct ErrorPolicyArgs {
    pub packet: PathBuf,
    pub records: PathBuf,
    pub next: bool,
    pub record: Option<PathBuf>,
    pub validate: bool,
}

#[derive(Debug)]
struct ExpectedRow {
    row: Value,
    expected_stderr: Value,
    stderr_contains: Value,
    input_sha256: String,
}

#[derive(Debug)]
struct PacketContext {
    manifest_sha256: String,
    identity_sha256: String,
    complete_sha256: String,
    packet_checksum_sha256: String,
    packet_digest_sha256: String,
    run_id: String,
    policy_sha256: String,
    source_sha256: String,
    metadata_sha256: String,
    record_schema_sha256: String,
    order: Vec<String>,
    rows: BTreeMap<String, ExpectedRow>,
}

pub fn run(args: ErrorPolicyArgs) -> Result<()> {
    let root = std::env::current_dir().context("resolve repository root")?;
    let packet = resolve(&root, &args.packet);
    let context = load_context(&root, &packet)?;
    let records_path = resolve(&root, &args.records);
    let records = load_records(&records_path, &context)?;
    let actions =
        usize::from(args.next) + usize::from(args.record.is_some()) + usize::from(args.validate);
    if actions != 1 {
        bail!("use exactly one of --next, --record PATH, or --validate");
    }

    if let Some(record_path) = args.record {
        let record = common::load_json(
            &resolve(&root, &record_path),
            "expected-error policy record",
        )?;
        validate_record(&record, &context)?;
        let case_id = non_empty_string(record.get("case_id"), "record case_id")?;
        if records.contains_key(&case_id) {
            bail!("duplicate expected-error policy record for case_id: {case_id}");
        }
        let outcome = persist::append_decision_checked(&records_path, &record, || {
            let current = load_records(&records_path, &context)?;
            if current.contains_key(&case_id) {
                bail!("duplicate expected-error policy record for case_id: {case_id}");
            }
            Ok(persist::PublishOutcome::Published)
        })?;
        if outcome == persist::PublishOutcome::EqualReplay {
            bail!("duplicate expected-error policy record for case_id: {case_id}");
        }
        println!("{case_id}");
        return Ok(());
    }

    if args.validate {
        let missing: Vec<&str> = context
            .order
            .iter()
            .map(String::as_str)
            .filter(|case_id| !records.contains_key(*case_id))
            .collect();
        if let Some(first) = missing.first() {
            bail!(
                "expected-error policy coverage incomplete: {} row(s) missing; first={first}",
                missing.len()
            );
        }
        println!(
            "{}",
            json!({
                "schema": COVERAGE_SCHEMA,
                "expected": context.rows.len(),
                "reviewed": records.len(),
                "missing": [],
                "packet": packet_claim(&context),
                "run_id": context.run_id,
                "policy_sha256": context.policy_sha256,
            })
        );
        return Ok(());
    }

    for case_id in &context.order {
        if !records.contains_key(case_id) {
            let expected = context
                .rows
                .get(case_id)
                .ok_or_else(|| anyhow!("packet order references unknown case_id: {case_id}"))?;
            println!("{}", next_payload(&context, expected));
            return Ok(());
        }
    }
    println!(
        "{}",
        json!({
            "schema": NEXT_SCHEMA,
            "done": true,
            "packet": packet_claim(&context),
            "run_id": context.run_id,
            "policy_sha256": context.policy_sha256,
        })
    );
    Ok(())
}

fn resolve(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn load_context(root: &Path, packet: &Path) -> Result<PacketContext> {
    if !packet.is_dir() {
        bail!("packet directory does not exist: {}", packet.display());
    }
    let complete_path = packet.join("COMPLETE.json");
    let identity_path = packet.join("identity.json");
    let manifest_path = packet.join("manifest.jsonl");
    let checksum_path = packet.join("PACKET.sha256");
    let complete = common::load_json(&complete_path, "completion marker")?;
    let identity = common::load_json(&identity_path, "packet identity")?;
    let complete_sha256 = common::sha256_file(&complete_path)?;
    let identity_sha256 = common::sha256_file(&identity_path)?;
    let manifest_sha256 = common::sha256_file(&manifest_path)?;
    let packet_checksum_sha256 = common::sha256_file(&checksum_path)?;
    let checksum = common::require_file(&checksum_path, "packet listing")?;
    let (packet_digest_sha256, listing) = common::deterministic_digest(packet)?;
    if checksum != listing.as_bytes() {
        bail!("PACKET.sha256 does not match packet contents");
    }
    if complete.get("packet_sha256").and_then(Value::as_str) != Some(packet_digest_sha256.as_str())
        || complete.get("manifest_sha256").and_then(Value::as_str) != Some(manifest_sha256.as_str())
    {
        bail!("completion marker does not match packet contents");
    }
    let run_identity = identity
        .get("run_identity")
        .ok_or_else(|| anyhow!("packet identity has no run_identity"))?;
    persist::validate_run_identity(run_identity)?;
    let run_id = non_empty_string(run_identity.get("run_id"), "identity.run_identity.run_id")?;
    let policy_sha256 = non_empty_hash(
        run_identity.get("policy_sha256"),
        "identity.run_identity.policy_sha256",
    )?;
    let source_sha256 = non_empty_hash(
        identity
            .get("provenance")
            .and_then(|value| value.get("effective_sha256")),
        "identity.provenance.effective_sha256",
    )?;

    let schema_path = root.join(RECORD_SCHEMA_PATH);
    let record_schema = common::load_json(&schema_path, "expected-error policy record schema")?;
    validate_schema_document(&record_schema)?;
    let record_schema_sha256 = common::sha256_file(&schema_path)?;
    let metadata_path = root.join("tests/fixtures/metadata.json");
    let input_root = root.join("tests/fixtures/inputs");
    let (metadata, _) = common::load_metadata(&metadata_path, &input_root)?;
    let metadata_sha256 = common::sha256_file(&metadata_path)?;
    let manifest = common::require_file(&manifest_path, "manifest")?;
    let text = String::from_utf8(manifest).context("manifest is not UTF-8")?;
    let mut rows = BTreeMap::new();
    let mut order = Vec::new();
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
        if row.get("classification").and_then(Value::as_str) != Some("expected_error") {
            continue;
        }
        if rows.contains_key(&case_id) {
            bail!("duplicate expected-error manifest case_id: {case_id}");
        }
        let expected =
            validate_packet_row(root, packet, &row, &metadata, &identity, &identity_sha256)?;
        order.push(case_id.clone());
        rows.insert(case_id, expected);
    }
    if rows.is_empty() {
        bail!("packet contains no expected-error rows");
    }
    Ok(PacketContext {
        manifest_sha256,
        identity_sha256,
        complete_sha256,
        packet_checksum_sha256,
        packet_digest_sha256,
        run_id,
        policy_sha256,
        source_sha256,
        metadata_sha256,
        record_schema_sha256,
        order,
        rows,
    })
}

fn validate_packet_row(
    root: &Path,
    packet: &Path,
    row: &Value,
    metadata: &BTreeMap<String, common::FixtureMetadata>,
    identity: &Value,
    identity_sha256: &str,
) -> Result<ExpectedRow> {
    let case_id = non_empty_string(row.get("case_id"), "expected-error case_id")?;
    let fixture = non_empty_string(row.get("fixture"), &format!("{case_id} fixture"))?;
    let record = metadata
        .get(&fixture)
        .ok_or_else(|| anyhow!("expected-error row references unknown fixture: {fixture}"))?;
    if record.kind != "expected_error" {
        bail!("{fixture} is not classified as expected_error in metadata");
    }
    let style = non_empty_string(row.get("style"), &format!("{case_id} style"))?;
    let mode = non_empty_string(row.get("mode"), &format!("{case_id} mode"))?;
    if !common::STYLES.contains(&style.as_str()) || !common::MODES.contains(&mode.as_str()) {
        bail!("{case_id} has invalid style or mode");
    }
    if non_empty_string(row.get("direction"), &format!("{case_id} direction"))? != record.direction
    {
        bail!("{case_id} direction does not match fixture metadata");
    }
    common::validate_row_identity(
        row,
        identity,
        identity_sha256,
        &format!("{case_id} identity"),
    )?;
    let input_ref = row
        .get("input")
        .ok_or_else(|| anyhow!("{case_id} input is missing"))?;
    let input_path = common::repository_file(root, input_ref, &format!("{case_id} input"))?;
    let input = common::require_file(&input_path, &format!("{case_id} input"))?;
    if common::case_id(&input, &fixture, &style, &mode) != case_id {
        bail!("{case_id} does not match input/style/mode identity");
    }
    let stdout = row
        .get("stdout")
        .ok_or_else(|| anyhow!("{case_id} stdout is missing"))?;
    let stderr = row
        .get("stderr")
        .ok_or_else(|| anyhow!("{case_id} stderr is missing"))?;
    let stdout_bytes = common::validate_blob_ref(packet, stdout, &format!("{case_id} stdout"))?;
    let stderr_bytes = common::validate_blob_ref(packet, stderr, &format!("{case_id} stderr"))?;
    if !stdout_bytes.is_empty()
        || row
            .get("status")
            .and_then(Value::as_i64)
            .is_none_or(|status| status == 0)
    {
        bail!("{case_id} does not satisfy the expected-error exit/stdout contract");
    }
    if stderr.get("policy").and_then(Value::as_str) != Some(record.stderr_policy.as_str()) {
        bail!("{case_id} stderr policy does not match metadata");
    }
    let status = row["status"].as_i64().unwrap_or_default() as i32;
    let stream_failures = common::validate_streams(
        root,
        &fixture,
        &style,
        record,
        status,
        &stdout_bytes,
        &stderr_bytes,
    );
    if !stream_failures.is_empty() {
        bail!(
            "{case_id} expected-error stream contract failed: {}",
            stream_failures.join("; ")
        );
    }
    let expected_stderr = expected_stderr_ref(root, &fixture, &style, record)?;
    Ok(ExpectedRow {
        row: row.clone(),
        expected_stderr,
        stderr_contains: json!(record.stderr_contains),
        input_sha256: common::sha256_bytes(&input),
    })
}

fn expected_stderr_ref(
    root: &Path,
    fixture: &str,
    style: &str,
    record: &common::FixtureMetadata,
) -> Result<Value> {
    let relative = record
        .expected_stderr
        .clone()
        .unwrap_or_else(|| format!("tests/fixtures/expected/{fixture}.{style}.txt"));
    let path = Path::new(&relative);
    if path.is_absolute() {
        bail!("expected stderr path must be relative: {relative}");
    }
    let resolved = common::safe_relative_path(path, root, "expected stderr path")?;
    let bytes = common::require_file(&resolved, "expected stderr")?;
    Ok(json!({
        "path": relative.replace('\\', "/"),
        "bytes": bytes.len(),
        "sha256": common::sha256_bytes(&bytes),
    }))
}

fn load_records(path: &Path, context: &PacketContext) -> Result<BTreeMap<String, Value>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let bytes = common::require_file(path, "expected-error policy ledger")?;
    let text = String::from_utf8(bytes).context("expected-error policy ledger is not UTF-8")?;
    let mut records = BTreeMap::new();
    for (number, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record: Value = serde_json::from_str(line)
            .with_context(|| format!("policy ledger line {} is invalid JSON", number + 1))?;
        validate_record(&record, context)
            .with_context(|| format!("policy ledger line {} is invalid", number + 1))?;
        let case_id = non_empty_string(record.get("case_id"), "record case_id")?;
        if records.insert(case_id.clone(), record).is_some() {
            bail!("duplicate expected-error policy record for case_id: {case_id}");
        }
    }
    Ok(records)
}

fn validate_record(record: &Value, context: &PacketContext) -> Result<()> {
    validate_record_shape(record)?;
    let case_id = non_empty_string(record.get("case_id"), "record case_id")?;
    let expected = context
        .rows
        .get(&case_id)
        .ok_or_else(|| anyhow!("record references unknown expected-error case_id: {case_id}"))?;
    if record["packet"] != packet_claim(context) {
        bail!("record packet evidence is stale for {case_id}");
    }
    for (field, expected_value) in [
        ("run_id", Value::String(context.run_id.clone())),
        (
            "policy_sha256",
            Value::String(context.policy_sha256.clone()),
        ),
        (
            "record_schema_sha256",
            Value::String(context.record_schema_sha256.clone()),
        ),
        ("fixture", expected.row["fixture"].clone()),
        ("direction", expected.row["direction"].clone()),
        ("style", expected.row["style"].clone()),
        ("mode", expected.row["mode"].clone()),
        ("input", expected.row["input"].clone()),
        (
            "source_sha256",
            Value::String(context.source_sha256.clone()),
        ),
        ("input_sha256", Value::String(expected.input_sha256.clone())),
        ("status", expected.row["status"].clone()),
        ("stdout", blob_claim(&expected.row["stdout"])),
        ("stderr", blob_claim(&expected.row["stderr"])),
        ("expected_stderr", expected.expected_stderr.clone()),
        ("stderr_policy", expected.row["stderr"]["policy"].clone()),
        ("stderr_contains", expected.stderr_contains.clone()),
    ] {
        if record.get(field) != Some(&expected_value) {
            bail!("record {field} is stale or does not match packet for {case_id}");
        }
    }
    if record["result"] != Value::String("matched".to_owned()) {
        bail!("record result must be matched for {case_id}");
    }
    for field in [
        "observation",
        "owner",
        "hypothesis",
        "expected_observation_if_true",
        "falsifier",
        "next_command",
        "reviewer",
        "timestamp",
    ] {
        non_empty_string(record.get(field), &format!("record {field}"))?;
    }
    if !matches!(record["reviewer"].as_str(), Some("ai" | "human")) {
        bail!("record reviewer must be ai or human for {case_id}");
    }
    Ok(())
}

fn validate_record_shape(record: &Value) -> Result<()> {
    let object = record
        .as_object()
        .ok_or_else(|| anyhow!("expected-error policy record must be an object"))?;
    let allowed: BTreeSet<&str> = RECORD_FIELDS.iter().copied().collect();
    let unexpected: Vec<&str> = object
        .keys()
        .map(String::as_str)
        .filter(|key| !allowed.contains(key))
        .collect();
    if !unexpected.is_empty() {
        bail!(
            "record contains unsupported fields: {}",
            unexpected.join(", ")
        );
    }
    for field in RECORD_FIELDS {
        if !object.contains_key(*field) {
            bail!("record {field} is required");
        }
    }
    if record["schema"].as_str() != Some(RECORD_SCHEMA) {
        bail!("record schema must be {RECORD_SCHEMA}");
    }
    validate_hash(record.get("run_id"), "record run_id")?;
    validate_hash(record.get("policy_sha256"), "record policy_sha256")?;
    validate_hash(
        record.get("record_schema_sha256"),
        "record record_schema_sha256",
    )?;
    validate_hash(record.get("source_sha256"), "record source_sha256")?;
    validate_hash(record.get("input_sha256"), "record input_sha256")?;
    for field in [
        "case_id",
        "fixture",
        "direction",
        "style",
        "mode",
        "input",
        "reviewer",
    ] {
        non_empty_string(record.get(field), &format!("record {field}"))?;
    }
    let status = record["status"]
        .as_i64()
        .ok_or_else(|| anyhow!("record status must be an integer"))?;
    if status == 0 {
        bail!("record status must be non-zero");
    }
    validate_blob_shape(record.get("stdout"), "record stdout")?;
    validate_blob_shape(record.get("stderr"), "record stderr")?;
    validate_blob_shape(record.get("expected_stderr"), "record expected_stderr")?;
    let contains = record["stderr_contains"]
        .as_array()
        .ok_or_else(|| anyhow!("record stderr_contains must be a string list"))?;
    if !contains.iter().all(Value::is_string) {
        bail!("record stderr_contains must be a string list");
    }
    for field in [
        "stderr_policy",
        "result",
        "observation",
        "owner",
        "hypothesis",
        "expected_observation_if_true",
        "falsifier",
        "next_command",
        "timestamp",
    ] {
        non_empty_string(record.get(field), &format!("record {field}"))?;
    }
    Ok(())
}

fn validate_blob_shape(value: Option<&Value>, label: &str) -> Result<()> {
    let object = value
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("{label} must be an object"))?;
    let allowed = ["path", "bytes", "sha256"];
    let unexpected: Vec<&str> = object
        .keys()
        .map(String::as_str)
        .filter(|key| !allowed.contains(key))
        .collect();
    if !unexpected.is_empty() {
        bail!(
            "{label} contains unsupported fields: {}",
            unexpected.join(", ")
        );
    }
    non_empty_string(object.get("path"), &format!("{label}.path"))?;
    let bytes = object
        .get("bytes")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("{label}.bytes must be a non-negative integer"))?;
    let _ = bytes;
    validate_hash(object.get("sha256"), &format!("{label}.sha256"))
}

fn blob_claim(value: &Value) -> Value {
    json!({
        "path": value["path"],
        "bytes": value["bytes"],
        "sha256": value["sha256"],
    })
}

fn validate_hash(value: Option<&Value>, label: &str) -> Result<()> {
    let hash = non_empty_string(value, label)?;
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("{label} must be a lowercase SHA-256 digest");
    }
    Ok(())
}

fn non_empty_hash(value: Option<&Value>, label: &str) -> Result<String> {
    let hash = non_empty_string(value, label)?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("{label} must be a SHA-256 digest");
    }
    Ok(hash)
}

fn non_empty_string(value: Option<&Value>, label: &str) -> Result<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("{label} must be a non-empty string"))
}

fn packet_claim(context: &PacketContext) -> Value {
    json!({
        "manifest_sha256": context.manifest_sha256,
        "identity_sha256": context.identity_sha256,
        "complete_sha256": context.complete_sha256,
        "packet_checksum_sha256": context.packet_checksum_sha256,
        "packet_digest_sha256": context.packet_digest_sha256,
        "metadata_sha256": context.metadata_sha256,
    })
}

fn next_payload(context: &PacketContext, expected: &ExpectedRow) -> Value {
    let row = &expected.row;
    let template = json!({
        "schema": RECORD_SCHEMA,
        "case_id": row["case_id"],
        "packet": packet_claim(context),
        "run_id": context.run_id,
        "policy_sha256": context.policy_sha256,
        "record_schema_sha256": context.record_schema_sha256,
        "fixture": row["fixture"],
        "direction": row["direction"],
        "style": row["style"],
        "mode": row["mode"],
        "input": row["input"],
        "source_sha256": context.source_sha256,
        "input_sha256": expected.input_sha256,
        "status": row["status"],
        "stdout": blob_claim(&row["stdout"]),
        "stderr": blob_claim(&row["stderr"]),
        "expected_stderr": expected.expected_stderr,
        "stderr_policy": row["stderr"]["policy"],
        "stderr_contains": expected.stderr_contains,
        "result": "matched",
        "observation": "REPLACE with the human-readable observed process result.",
        "owner": "REPLACE with the owning parser/layout/render subsystem.",
        "hypothesis": "REPLACE with a falsifiable hypothesis if this policy regresses.",
        "expected_observation_if_true": "REPLACE with the expected observation.",
        "falsifier": "REPLACE with the command or evidence that would falsify it.",
        "next_command": "REPLACE with the smallest next verification command.",
        "reviewer": "ai",
        "timestamp": common::now_label(),
    });
    json!({
        "schema": NEXT_SCHEMA,
        "done": false,
        "packet": packet_claim(context),
        "run_id": context.run_id,
        "policy_sha256": context.policy_sha256,
        "row": {
            "case_id": row["case_id"],
            "fixture": row["fixture"],
            "direction": row["direction"],
            "style": row["style"],
            "mode": row["mode"],
            "input": row["input"],
            "status": row["status"],
            "stdout": blob_claim(&row["stdout"]),
            "stderr": blob_claim(&row["stderr"]),
            "expected_stderr": expected.expected_stderr,
            "stderr_policy": row["stderr"]["policy"],
            "stderr_contains": expected.stderr_contains,
            "source_sha256": context.source_sha256,
            "input_sha256": expected.input_sha256,
        },
        "record_template": template,
    })
}

fn validate_schema_document(schema: &Value) -> Result<()> {
    if schema.get("$id").and_then(Value::as_str) != Some(RECORD_SCHEMA)
        || schema.get("type").and_then(Value::as_str) != Some("object")
        || schema.get("additionalProperties") != Some(&Value::Bool(false))
    {
        bail!("expected-error policy record schema has an invalid identity or openness policy");
    }
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("expected-error policy schema required list is missing"))?;
    let required: BTreeSet<&str> = required.iter().filter_map(Value::as_str).collect();
    if RECORD_FIELDS.iter().any(|field| !required.contains(field)) {
        bail!("expected-error policy schema omits a required record field");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context() -> (PacketContext, ExpectedRow) {
        let row = json!({
            "case_id": "expected-error-test-case",
            "fixture": "error_empty",
            "direction": "none",
            "style": "ascii",
            "mode": "default",
            "input": "tests/fixtures/inputs/error_empty.md",
            "status": 1,
            "stdout": {
                "path": "frames/error.txt",
                "bytes": 0,
                "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            },
            "stderr": {
                "path": "logs/error.txt",
                "bytes": 3,
                "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "policy": "error"
            }
        });
        let expected = ExpectedRow {
            row: row.clone(),
            expected_stderr: json!({
                "path": "tests/fixtures/expected/error_empty.ascii.txt",
                "bytes": 3,
                "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            }),
            stderr_contains: json!([]),
            input_sha256: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                .to_owned(),
        };
        let case_id = row["case_id"].as_str().expect("test case id").to_owned();
        let mut rows = BTreeMap::new();
        rows.insert(
            case_id.clone(),
            ExpectedRow {
                row,
                expected_stderr: expected.expected_stderr.clone(),
                stderr_contains: expected.stderr_contains.clone(),
                input_sha256: expected.input_sha256.clone(),
            },
        );
        let context = PacketContext {
            manifest_sha256: "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
                .to_owned(),
            identity_sha256: "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
                .to_owned(),
            complete_sha256: "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
                .to_owned(),
            packet_checksum_sha256:
                "1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
            packet_digest_sha256:
                "2222222222222222222222222222222222222222222222222222222222222222".to_owned(),
            run_id: "3333333333333333333333333333333333333333333333333333333333333333".to_owned(),
            policy_sha256: "4444444444444444444444444444444444444444444444444444444444444444"
                .to_owned(),
            source_sha256: "5555555555555555555555555555555555555555555555555555555555555555"
                .to_owned(),
            metadata_sha256: "6666666666666666666666666666666666666666666666666666666666666666"
                .to_owned(),
            record_schema_sha256:
                "7777777777777777777777777777777777777777777777777777777777777777".to_owned(),
            order: vec![case_id],
            rows,
        };
        (context, expected)
    }

    #[test]
    fn record_shape_rejects_unknown_fields() {
        let mut record = serde_json::Map::new();
        for field in RECORD_FIELDS {
            record.insert((*field).to_owned(), Value::Null);
        }
        record.insert("unexpected".to_owned(), Value::Null);
        let error = validate_record_shape(&Value::Object(record)).expect_err("unknown field");
        assert!(error.to_string().contains("unsupported fields"));
    }

    #[test]
    fn record_shape_requires_falsifier_and_process_evidence() {
        let record = json!({"schema": RECORD_SCHEMA});
        let error = validate_record_shape(&record).expect_err("missing required fields");
        assert!(error.to_string().contains("case_id") || error.to_string().contains("required"));
    }

    #[test]
    fn valid_record_binds_expected_error_row() {
        let (context, expected) = test_context();
        let record = next_payload(&context, &expected)["record_template"].clone();
        validate_record(&record, &context).expect("valid expected-error record");
    }

    #[test]
    fn record_validation_rejects_stale_packet_evidence() {
        let (context, expected) = test_context();
        let mut record = next_payload(&context, &expected)["record_template"].clone();
        record["packet"]["manifest_sha256"] = Value::String(
            "8888888888888888888888888888888888888888888888888888888888888888".to_owned(),
        );
        let error = validate_record(&record, &context).expect_err("stale packet");
        assert!(error.to_string().contains("packet evidence is stale"));
    }

    #[test]
    fn record_validation_rejects_missing_falsifier() {
        let (context, expected) = test_context();
        let mut record = next_payload(&context, &expected)["record_template"].clone();
        record
            .as_object_mut()
            .expect("record object")
            .remove("falsifier");
        let error = validate_record(&record, &context).expect_err("missing falsifier");
        assert!(error.to_string().contains("falsifier is required"));
    }

    #[test]
    fn schema_contract_is_closed_and_complete() {
        let schema: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/error_policy_record.schema.json"
        ))
        .expect("schema JSON");
        validate_schema_document(&schema).expect("schema contract");
    }
}
