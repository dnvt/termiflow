use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use super::common;
use super::provenance::{self, ProvenanceInputs};

#[derive(Debug)]
pub struct AuditArgs {
    pub out: Option<PathBuf>,
    pub schema_manifest: Option<PathBuf>,
    pub styles: String,
    pub modes: String,
    pub binary: Option<PathBuf>,
    pub input_root: PathBuf,
    pub metadata: PathBuf,
    pub display_profile: String,
    pub timeout_seconds: f64,
}

#[derive(Debug, Clone)]
pub struct SchemaPacketResult {
    pub out: PathBuf,
    pub manifest_sha256: String,
    pub identity_sha256: String,
    pub packet_sha256: String,
    pub queue_id: String,
    pub queue_sha256: String,
    pub row_count: usize,
}

struct SchemaPacketIdentity<'a> {
    manifest: &'a Value,
    manifest_sha256: &'a str,
    queue_id: &'a str,
    queue_sha256: &'a str,
    role: &'a str,
}

struct SchemaWorkload {
    manifest: Value,
    manifest_bytes: Vec<u8>,
    manifest_sha256: String,
    queue_id: String,
    queue_sha256: String,
    metadata: BTreeMap<String, common::FixtureMetadata>,
    input_paths: BTreeMap<String, PathBuf>,
    styles: Vec<String>,
    modes: Vec<String>,
    planned_count: usize,
}

pub fn run(args: AuditArgs) -> Result<()> {
    let root = std::env::current_dir().context("resolve repository root")?;
    if let Some(schema_manifest) = args.schema_manifest.as_deref() {
        let out = resolve_from_root(
            &root,
            &args.out.clone().unwrap_or_else(|| {
                root.join("artifacts/visual-audit")
                    .join(common::now_label())
            }),
        );
        return run_schema_packet(
            &root,
            &resolve_from_root(&root, schema_manifest),
            &out,
            false,
            args.binary.as_deref(),
            &args.display_profile,
            args.timeout_seconds,
        )
        .map(|_| ());
    }
    let styles = common::parse_csv(&args.styles, common::STYLES, "styles")?;
    let modes = common::parse_csv(&args.modes, common::MODES, "modes")?;
    if !args.timeout_seconds.is_finite() || args.timeout_seconds <= 0.0 {
        bail!("timeout-seconds must be a finite positive number");
    }

    let input_root = resolve_from_root(&root, &args.input_root);
    if !input_root.is_dir() {
        bail!("missing input root: {}", input_root.display());
    }
    let metadata_path = resolve_from_root(&root, &args.metadata);
    let (metadata, metadata_bytes) = common::load_metadata(&metadata_path, &input_root)?;
    let input_paths = common::collect_inputs(&input_root)?;
    let planned_count = input_paths.len() * styles.len() * modes.len();

    let out = args
        .out
        .map(|path| resolve_from_root(&root, &path))
        .unwrap_or_else(|| {
            root.join("artifacts/visual-audit")
                .join(common::now_label())
        });
    let expected_root = root
        .join("tests/fixtures/expected")
        .canonicalize()
        .unwrap_or_else(|_| root.join("tests/fixtures/expected"));
    let out_canonical_parent = out
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        .unwrap_or_else(|| out.parent().unwrap_or(&root).to_path_buf());
    let out_canonical = out_canonical_parent.join(out.file_name().unwrap_or_default());
    if out_canonical == expected_root || out_canonical.starts_with(&expected_root) {
        bail!("refusing to write a visual packet inside golden expected outputs");
    }
    if out.exists() {
        bail!("final packet already exists: {}", out.display());
    }
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let stage = out.parent().unwrap_or(&root).join(format!(
        ".{}.staging.{}.{}",
        out.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id(),
        common::now_label()
    ));
    if stage.exists() {
        bail!("staging path already exists: {}", stage.display());
    }
    fs::create_dir_all(stage.join("frames"))?;
    fs::create_dir_all(stage.join("logs"))?;
    fs::create_dir_all(stage.join("evidence"))?;

    let result = build_packet(
        &root,
        &stage,
        &out,
        &input_root,
        &metadata_path,
        metadata,
        metadata_bytes,
        &input_paths,
        &styles,
        &modes,
        args.binary.as_deref(),
        &args.display_profile,
        Duration::from_secs_f64(args.timeout_seconds),
        planned_count,
        None,
    );
    if result.is_err() {
        let _ = fs::remove_dir_all(&stage);
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn build_packet(
    root: &Path,
    stage: &Path,
    out: &Path,
    input_root: &Path,
    metadata_path: &Path,
    metadata: std::collections::BTreeMap<String, common::FixtureMetadata>,
    metadata_bytes: Vec<u8>,
    input_paths: &std::collections::BTreeMap<String, PathBuf>,
    styles: &[String],
    modes: &[String],
    supplied_binary: Option<&Path>,
    display_profile: &str,
    timeout: Duration,
    planned_count: usize,
    schema: Option<&SchemaPacketIdentity<'_>>,
) -> Result<()> {
    let binary = common::discover_binary(root, stage, supplied_binary)?;
    let base_identity = common::source_identity(root, &binary, display_profile)?;
    let identity = provenance::enrich_identity(
        root,
        &binary,
        &base_identity,
        &ProvenanceInputs {
            input_root,
            metadata_path,
            metadata_bytes: &metadata_bytes,
            input_paths,
            styles,
            modes,
            display_profile,
        },
    )?;
    let metadata_value: Value =
        serde_json::from_slice(&metadata_bytes).context("parse metadata for packet")?;
    common::write_json(&stage.join("metadata.json"), &metadata_value)?;
    if let Some(schema) = schema {
        common::write_json(&stage.join("schema_manifest.json"), schema.manifest)?;
    }
    common::write_json(&stage.join("identity.json"), &identity)?;

    let mut rows = Vec::new();
    let mut timings = Vec::new();
    let mut failures = Vec::new();
    let mut planned_ids = BTreeSet::new();

    for (fixture, input_path) in input_paths {
        let input_bytes =
            fs::read(input_path).with_context(|| format!("read {}", input_path.display()))?;
        for style in styles {
            for mode in modes {
                let case_id = common::case_id(&input_bytes, fixture, style, mode);
                if !planned_ids.insert(case_id.clone()) {
                    bail!("planned case IDs are not unique");
                }
                let stem = format!("{fixture}.{style}.{mode}");
                let frame_rel = format!("frames/{stem}.txt");
                let log_rel = format!("logs/{stem}.log");
                let evidence_rel = format!("evidence/{stem}.json");
                let frame_path = stage.join(&frame_rel);
                let log_path = stage.join(&log_rel);
                let evidence_path = stage.join(&evidence_rel);
                let mut command = vec![
                    binary.to_string_lossy().to_string(),
                    "--print".to_owned(),
                    "--style".to_owned(),
                    style.clone(),
                    "--audit-json".to_owned(),
                    evidence_path.to_string_lossy().to_string(),
                ];
                if mode == "optimized" {
                    command.insert(4, "--optimize-render".to_owned());
                }
                command.push(input_path.to_string_lossy().to_string());
                let mut argv = vec![
                    common::relative_to_root(&binary, root),
                    "--print".to_owned(),
                    "--style".to_owned(),
                    style.clone(),
                ];
                if mode == "optimized" {
                    argv.push("--optimize-render".to_owned());
                }
                argv.push(common::relative_to_root(input_path, root));
                let started = Instant::now();
                let process = common::process(&command, root, timeout);
                let duration_ns = started.elapsed().as_nanos();
                common::write_bytes(&frame_path, &process.stdout)?;
                common::write_bytes(&log_path, &process.stderr)?;

                let record = metadata
                    .get(fixture)
                    .with_context(|| format!("missing metadata for {fixture}"))?;
                let mut row_failures = common::validate_streams(
                    root,
                    fixture,
                    style,
                    record,
                    process.status,
                    &process.stdout,
                    &process.stderr,
                );
                let mut evidence = None;
                if evidence_path.is_file() {
                    match fs::read(&evidence_path)
                        .ok()
                        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                    {
                        Some(value)
                            if value.get("schema").and_then(Value::as_str)
                                == Some(common::EVIDENCE_SCHEMA) =>
                        {
                            evidence = Some(value)
                        }
                        Some(_) => row_failures.push("evidence schema mismatch".to_owned()),
                        None => row_failures.push("invalid evidence JSON".to_owned()),
                    }
                } else if record.kind != "expected_error" {
                    row_failures.push("successful render did not produce evidence JSON".to_owned());
                }
                failures.extend(
                    row_failures
                        .into_iter()
                        .map(|failure| format!("{stem}: {failure}")),
                );

                let evidence_dimensions = evidence
                    .as_ref()
                    .and_then(|value| value.get("display"))
                    .cloned()
                    .unwrap_or(Value::Null);
                let evidence_ref = evidence.as_ref().map(|value| {
                    let bytes = fs::read(&evidence_path).unwrap_or_default();
                    json!({
                        "path": evidence_rel,
                        "sha256": common::sha256_bytes(&bytes),
                        "bytes": bytes.len(),
                        "schema": value.get("schema").cloned().unwrap_or(Value::Null),
                    })
                });
                rows.push(json!({
                    "schema": common::AUDIT_SCHEMA,
                    "case_id": case_id,
                    "fixture": fixture,
                    "style": style,
                    "mode": mode,
                    "direction": record.direction,
                    "classification": record.kind,
                    "input": common::relative_to_root(input_path, root),
                    "argv": argv,
                    "status": process.status,
                    "stdout": { "path": frame_rel, "sha256": common::sha256_bytes(&process.stdout), "bytes": process.stdout.len() },
                    "stderr": { "path": log_rel, "sha256": common::sha256_bytes(&process.stderr), "bytes": process.stderr.len(), "policy": record.stderr_policy },
                    "evidence": evidence_ref,
                    "dimensions": { "stdout_rows": common::dimensions(&process.stdout)["stdout_rows"], "stdout_max_codepoints": common::dimensions(&process.stdout)["stdout_max_codepoints"], "stdout_bytes": process.stdout.len(), "display": evidence_dimensions },
                    "findings": {
                        "critic": evidence.as_ref().and_then(|value| value["critic"]["findings"].as_array()).map_or(0, Vec::len),
                        "raw_errors": evidence.as_ref().and_then(|value| value["raw"]["errors"].as_array()).map_or(0, Vec::len),
                        "geometry_errors": evidence.as_ref().and_then(|value| value["geometry"]["errors"].as_array()).map_or(0, Vec::len),
                    },
                    "identity": identity,
                    "timing": { "path": "timings.jsonl", "case_id": case_id },
                }));
                timings.push(json!({ "case_id": case_id, "duration_ns": duration_ns }));
            }
        }
    }

    if rows.len() != planned_count || planned_ids.len() != planned_count {
        failures.push(format!(
            "case coverage mismatch: planned={planned_count} actual={}",
            rows.len()
        ));
    }
    rows.sort_by(|left, right| left["case_id"].as_str().cmp(&right["case_id"].as_str()));
    timings.sort_by(|left, right| left["case_id"].as_str().cmp(&right["case_id"].as_str()));
    write_jsonl(&stage.join("manifest.jsonl"), &rows)?;
    write_jsonl(&stage.join("timings.jsonl"), &timings)?;
    let mut summary = json!({
        "schema": common::SUMMARY_SCHEMA,
        "binary": identity["binary"],
        "expected_rows": planned_count,
        "actual_rows": rows.len(),
        "primary_rows": rows.iter().filter(|row| row["classification"] != "expected_error").count(),
        "expected_error_rows": rows.iter().filter(|row| row["classification"] == "expected_error").count(),
        "warning_rows": rows.iter().filter(|row| row["classification"] == "warning").count(),
        "failures": failures,
        "styles": styles,
        "modes": modes,
    });
    if let Some(schema) = schema {
        summary["workload"] = Value::String("schema_queue".to_owned());
        summary["schema_manifest_sha256"] = Value::String(schema.manifest_sha256.to_owned());
        summary["queue_id"] = Value::String(schema.queue_id.to_owned());
        summary["queue_sha256"] = Value::String(schema.queue_sha256.to_owned());
        summary["schema_role"] = Value::String(schema.role.to_owned());
    } else {
        summary["workload"] = Value::String("metadata_corpus".to_owned());
    }
    common::write_json(&stage.join("summary.json"), &summary)?;
    if !failures.is_empty() {
        bail!(
            "visual audit failed:\n{}",
            failures
                .iter()
                .take(20)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    let (packet_digest, packet_listing) = common::deterministic_digest(stage)?;
    common::write_bytes(&stage.join("PACKET.sha256"), packet_listing.as_bytes())?;
    let manifest_hash = common::sha256_file(&stage.join("manifest.jsonl"))?;
    common::write_json(
        &stage.join("COMPLETE.json"),
        &json!({
            "schema": "termiflow.visual_audit.complete.v1",
            "completed_at": common::now_label(),
            "rows": rows.len(),
            "manifest_sha256": manifest_hash,
            "packet_sha256": packet_digest,
        }),
    )?;
    fs::rename(stage, out).with_context(|| format!("publish visual packet {}", out.display()))?;
    eprintln!(
        "visual audit complete: {} ({} rows)",
        out.display(),
        rows.len()
    );
    let _ = metadata_path;
    Ok(())
}

pub fn run_schema_packet(
    root: &Path,
    manifest_path: &Path,
    out: &Path,
    holdouts: bool,
    supplied_binary: Option<&Path>,
    display_profile: &str,
    timeout_seconds: f64,
) -> Result<SchemaPacketResult> {
    if !timeout_seconds.is_finite() || timeout_seconds <= 0.0 {
        bail!("timeout-seconds must be a finite positive number");
    }
    let manifest_bytes = common::require_file(manifest_path, "schema manifest")?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes)
        .with_context(|| format!("parse schema manifest JSON: {}", manifest_path.display()))?;
    let workload = schema_workload(root, manifest_path, manifest_bytes, manifest, holdouts)?;
    let (out, stage) = prepare_packet_output(root, out)?;
    let identity = SchemaPacketIdentity {
        manifest: &workload.manifest,
        manifest_sha256: &workload.manifest_sha256,
        queue_id: &workload.queue_id,
        queue_sha256: &workload.queue_sha256,
        role: if holdouts { "holdout" } else { "review" },
    };
    let result = build_packet(
        root,
        &stage,
        &out,
        &root.join("tests/fixtures"),
        manifest_path,
        workload.metadata,
        workload.manifest_bytes.clone(),
        &workload.input_paths,
        &workload.styles,
        &workload.modes,
        supplied_binary,
        display_profile,
        Duration::from_secs_f64(timeout_seconds),
        workload.planned_count,
        Some(&identity),
    );
    if result.is_err() {
        let _ = fs::remove_dir_all(&stage);
    }
    result?;
    let manifest_sha256 = common::sha256_file(&out.join("manifest.jsonl"))?;
    let identity_sha256 = common::sha256_file(&out.join("identity.json"))?;
    let packet_sha256 = common::sha256_file(&out.join("PACKET.sha256"))?;
    Ok(SchemaPacketResult {
        out,
        manifest_sha256,
        identity_sha256,
        packet_sha256,
        queue_id: workload.queue_id,
        queue_sha256: workload.queue_sha256,
        row_count: workload.planned_count,
    })
}

fn schema_workload(
    root: &Path,
    manifest_path: &Path,
    manifest_bytes: Vec<u8>,
    manifest: Value,
    holdouts: bool,
) -> Result<SchemaWorkload> {
    let object = manifest
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("schema manifest must be an object"))?;
    if object.get("schema").and_then(Value::as_str) != Some("termiflow.fixture_manifest.v2") {
        bail!("schema manifest schema must be termiflow.fixture_manifest.v2");
    }
    if object.get("spec_schema").and_then(Value::as_str) != Some("termiflow.fixture_spec.v2")
        || object.get("spec_version").and_then(Value::as_i64) != Some(2)
    {
        bail!("schema manifest spec identity is invalid");
    }
    let queue_id = object
        .get("queue_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("schema manifest queue_id is missing"))?
        .to_owned();
    let queue_sha256 = object
        .get("queue_sha256")
        .and_then(Value::as_str)
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| anyhow::anyhow!("schema manifest queue_sha256 is invalid"))?
        .to_owned();
    let key = if holdouts { "holdouts" } else { "rows" };
    let rows = object
        .get(key)
        .and_then(Value::as_array)
        .filter(|rows| !rows.is_empty())
        .ok_or_else(|| anyhow::anyhow!("schema manifest {key} must be a non-empty array"))?;
    let mut metadata: BTreeMap<String, common::FixtureMetadata> = BTreeMap::new();
    let mut input_paths: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut styles_seen = BTreeSet::new();
    let mut modes_seen = BTreeSet::new();
    let mut matrix_keys = BTreeSet::new();

    for row in rows {
        let case_id = required_manifest_string(row, "case_id", key)?;
        let variant_id = required_manifest_string(row, "variant_id", key)?;
        let fixture = format!("{case_id}--{variant_id}");
        let direction = required_manifest_string(row, "direction", key)?;
        if !matches!(direction.as_str(), "TD" | "LR" | "BT" | "RL") {
            bail!("schema manifest {fixture} has invalid direction {direction}");
        }
        let style = required_manifest_string(row, "style", key)?;
        let mode = required_manifest_string(row, "mode", key)?;
        if !common::STYLES.contains(&style.as_str()) || !common::MODES.contains(&mode.as_str()) {
            bail!("schema manifest {fixture} has invalid style or mode");
        }
        if required_manifest_string(row, "kind", key)? != "success" {
            bail!("schema manifest {fixture} is not a successful render row");
        }
        let holdout_class = required_manifest_string(row, "holdout", key)?;
        let expected_holdout = if holdouts { "evaluator_owned" } else { "none" };
        if holdout_class != expected_holdout {
            bail!(
                "schema manifest {fixture} has holdout class {holdout_class}, expected {expected_holdout}"
            );
        }
        let input_path = required_manifest_string(row, "input_path", key)?;
        let input_path = common::safe_relative_path(
            Path::new(&input_path),
            root,
            &format!("schema manifest {fixture} input_path"),
        )?;
        let source = required_manifest_string(row, "source", key)?;
        let source_sha256 = required_manifest_string(row, "source_sha256", key)?;
        let input_bytes = fs::read(&input_path)
            .with_context(|| format!("read schema manifest input {}", input_path.display()))?;
        if input_bytes != source.as_bytes() {
            bail!("schema manifest source does not match input_path for {fixture}");
        }
        if common::sha256_bytes(&input_bytes) != source_sha256 {
            bail!("schema manifest source hash mismatch for {fixture}");
        }
        let matrix_key = format!("{fixture}\u{1f}{style}\u{1f}{mode}");
        if !matrix_keys.insert(matrix_key) {
            bail!("schema manifest contains duplicate row for {fixture}.{style}.{mode}");
        }
        styles_seen.insert(style);
        modes_seen.insert(mode);

        let record = common::FixtureMetadata {
            kind: "success".to_owned(),
            direction: direction.clone(),
            stderr_policy: "empty".to_owned(),
            stderr_contains: Vec::new(),
            expected_stderr: None,
        };
        if let Some(previous) = metadata.get(&fixture) {
            if previous.direction != record.direction {
                bail!("schema manifest changes direction within {fixture}");
            }
        } else {
            metadata.insert(fixture.clone(), record);
            input_paths.insert(fixture, input_path);
        }
    }

    let styles: Vec<String> = common::STYLES
        .iter()
        .filter(|style| styles_seen.contains(**style))
        .map(|style| (*style).to_owned())
        .collect();
    let modes: Vec<String> = common::MODES
        .iter()
        .filter(|mode| modes_seen.contains(**mode))
        .map(|mode| (*mode).to_owned())
        .collect();
    let expected_per_fixture = styles.len() * modes.len();
    for fixture in metadata.keys() {
        let prefix = format!("{fixture}\u{1f}");
        let actual = matrix_keys
            .iter()
            .filter(|key| key.starts_with(&prefix))
            .count();
        if actual != expected_per_fixture {
            bail!(
                "schema manifest matrix is incomplete for {fixture}: expected {expected_per_fixture}, got {actual}"
            );
        }
    }
    let planned_count = input_paths.len() * styles.len() * modes.len();
    if planned_count != rows.len() {
        bail!(
            "schema manifest {key} count does not match its matrix: expected {planned_count}, got {}",
            rows.len()
        );
    }
    let manifest_sha256 = common::sha256_bytes(&manifest_bytes);
    let _ = manifest_path;
    Ok(SchemaWorkload {
        manifest,
        manifest_bytes,
        manifest_sha256,
        queue_id,
        queue_sha256,
        metadata,
        input_paths,
        styles,
        modes,
        planned_count,
    })
}

fn required_manifest_string(row: &Value, key: &str, section: &str) -> Result<String> {
    row.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            anyhow::anyhow!("schema manifest {section}.{key} must be a non-empty string")
        })
}

fn prepare_packet_output(root: &Path, out: &Path) -> Result<(PathBuf, PathBuf)> {
    let expected_root = root
        .join("tests/fixtures/expected")
        .canonicalize()
        .unwrap_or_else(|_| root.join("tests/fixtures/expected"));
    let out_canonical_parent = out
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        .unwrap_or_else(|| out.parent().unwrap_or(root).to_path_buf());
    let out_canonical = out_canonical_parent.join(out.file_name().unwrap_or_default());
    if out_canonical == expected_root || out_canonical.starts_with(&expected_root) {
        bail!("refusing to write a visual packet inside golden expected outputs");
    }
    if out.exists() {
        bail!("final packet already exists: {}", out.display());
    }
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let stage = out.parent().unwrap_or(root).join(format!(
        ".{}.staging.{}.{}",
        out.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id(),
        common::now_label()
    ));
    if stage.exists() {
        bail!("staging path already exists: {}", stage.display());
    }
    fs::create_dir_all(stage.join("frames"))?;
    fs::create_dir_all(stage.join("logs"))?;
    fs::create_dir_all(stage.join("evidence"))?;
    Ok((out.to_path_buf(), stage))
}

fn resolve_from_root(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn write_jsonl(path: &Path, values: &[Value]) -> Result<()> {
    let mut content = Vec::new();
    for value in values {
        content.extend_from_slice(
            serde_json::to_string(value)
                .context("serialize JSONL row")?
                .as_bytes(),
        );
        content.push(b'\n');
    }
    common::write_bytes(path, &content)
}
