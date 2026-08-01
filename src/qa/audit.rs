use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use super::common;

#[derive(Debug)]
pub struct AuditArgs {
    pub out: Option<PathBuf>,
    pub styles: String,
    pub modes: String,
    pub binary: Option<PathBuf>,
    pub input_root: PathBuf,
    pub metadata: PathBuf,
    pub display_profile: String,
    pub timeout_seconds: f64,
}

pub fn run(args: AuditArgs) -> Result<()> {
    let root = std::env::current_dir().context("resolve repository root")?;
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
) -> Result<()> {
    let binary = common::discover_binary(root, stage, supplied_binary)?;
    let identity = common::source_identity(root, &binary, display_profile)?;
    let metadata_value: Value =
        serde_json::from_slice(&metadata_bytes).context("parse metadata for packet")?;
    common::write_json(&stage.join("metadata.json"), &metadata_value)?;
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
    common::write_json(
        &stage.join("summary.json"),
        &json!({
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
        }),
    )?;
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
