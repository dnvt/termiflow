use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use super::provenance::{self, ProvenanceInputs};
use super::{common, persist, route_clarity};

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
    pub respect_input_style: bool,
    pub pause_at: Option<String>,
    pub pause_marker: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct SchemaPacketResult {
    pub out: PathBuf,
    pub manifest_sha256: String,
    pub identity_sha256: String,
    pub packet_sha256: String,
    pub complete_sha256: String,
    pub deterministic_packet_sha256: String,
    pub queue_id: String,
    pub queue_sha256: String,
    pub row_count: usize,
    pub run_identity: Value,
}

pub struct SchemaPacketOptions<'a> {
    pub holdouts: bool,
    pub supplied_binary: Option<&'a Path>,
    pub display_profile: &'a str,
    pub timeout_seconds: f64,
    pub respect_input_style: bool,
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
    temporary_inputs: Vec<PathBuf>,
}

pub fn run(args: AuditArgs) -> Result<()> {
    match (&args.pause_at, &args.pause_marker) {
        (Some(point), Some(marker)) => {
            std::env::set_var("TERMIFLOW_QA_PAUSE_AT", point);
            std::env::set_var("TERMIFLOW_QA_PAUSE_MARKER", marker);
        }
        (None, None) => {}
        _ => bail!("--pause-at and --pause-marker must be provided together"),
    }
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
            SchemaPacketOptions {
                holdouts: false,
                supplied_binary: args.binary.as_deref(),
                display_profile: &args.display_profile,
                timeout_seconds: args.timeout_seconds,
                respect_input_style: args.respect_input_style,
            },
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
    let stage = persist::claim_directory_stage(&out)?;
    fs::create_dir_all(stage.join("frames"))?;
    fs::create_dir_all(stage.join("logs"))?;
    fs::create_dir_all(stage.join("evidence"))?;
    persist::pause_if_requested("stage-created", &stage)?;

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
        !args.respect_input_style,
        None,
    );
    match result {
        Ok(()) => {
            if let Err(error) = validate_packet_for_publication(&stage, "visual-audit", None) {
                if let Err(cleanup) = persist::remove_incomplete_directory(&stage) {
                    eprintln!("visual audit recovery required: {cleanup}");
                }
                return Err(error);
            }
            persist::pause_if_requested("before-publish", &stage)?;
            persist::publish_directory(&stage, &out, b"publisher=visual-audit\n")?;
            persist::pause_if_requested("after-publish", &out)?;
            eprintln!("visual audit complete: {}", out.display());
            Ok(())
        }
        Err(error) => {
            if let Err(cleanup) = persist::remove_incomplete_directory(&stage) {
                eprintln!("visual audit recovery required: {cleanup}");
            }
            Err(error)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_packet(
    root: &Path,
    stage: &Path,
    _out: &Path,
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
    style_override: bool,
    schema: Option<&SchemaPacketIdentity<'_>>,
) -> Result<()> {
    let binary = common::discover_binary(root, stage, supplied_binary)?;
    let argv_contract =
        provenance::argv_contract(&common::relative_to_root(&binary, root), style_override);
    let base_identity = common::source_identity(root, &binary, display_profile)?;
    let mut identity = provenance::enrich_identity(
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
            argv_contract: &argv_contract,
        },
    )?;
    let role = schema.map(|value| value.role).unwrap_or("visual-audit");
    let initial_source_sha256 = identity["provenance"]["effective_sha256"]
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("provenance effective digest is missing"))?;
    let workload_sha256 =
        common::sha256_bytes(&serde_json::to_vec(&identity["provenance"]["workload"])?);
    let requested_policy_context = json!({
        "policy_schema": termiflow::config::EFFECTIVE_POLICY_SCHEMA,
        "role": role,
        "styles": styles,
        "modes": modes,
        "display_profile": display_profile,
        "argv_contract": argv_contract,
        "schema_queue": schema.map(|value| json!({
            "queue_id": value.queue_id,
            "manifest_sha256": value.manifest_sha256,
        })),
    });
    let run_spec = persist::run_spec_value(
        role,
        &initial_source_sha256,
        &workload_sha256,
        _out,
        display_profile,
        &requested_policy_context,
    );
    let run_spec_id = persist::run_spec_id(&run_spec)?;
    common::write_json(&stage.join("run_spec.json"), &run_spec)?;
    let created_at = common::now_label();
    let publication_guard = persist::guard_path(_out, "publish")?;
    identity["run_spec_id"] = Value::String(run_spec_id.clone());
    persist::write_run_state(
        stage,
        &persist::run_state_value(
            &run_spec_id,
            None,
            "claimed",
            _out,
            stage,
            None,
            &created_at,
            "private stage claimed",
            false,
            Some(&publication_guard),
        ),
    )?;
    persist::write_run_state(
        stage,
        &persist::run_state_value(
            &run_spec_id,
            None,
            "writing",
            _out,
            stage,
            None,
            &created_at,
            "packet content writing started",
            false,
            Some(&publication_guard),
        ),
    )?;
    persist::pause_if_requested("writing", stage)?;
    let metadata_value: Value =
        serde_json::from_slice(&metadata_bytes).context("parse metadata for packet")?;
    common::write_json(&stage.join("metadata.json"), &metadata_value)?;
    if let Some(schema) = schema {
        common::write_json(&stage.join("schema_manifest.json"), schema.manifest)?;
    }

    let mut rows = Vec::new();
    let mut timings = Vec::new();
    let mut failures = Vec::new();
    let mut planned_ids = BTreeSet::new();
    let mut policy_observations = Vec::new();

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
                let mut command = vec![binary.to_string_lossy().to_string(), "--print".to_owned()];
                if style_override {
                    command.push("--style".to_owned());
                    command.push(style.clone());
                }
                if mode == "optimized" {
                    command.push("--optimize-render".to_owned());
                }
                command.push("--audit-json".to_owned());
                command.push(evidence_path.to_string_lossy().to_string());
                command.push(input_path.to_string_lossy().to_string());
                let mut argv = vec![
                    common::relative_to_root(&binary, root),
                    "--print".to_owned(),
                ];
                if style_override {
                    argv.push("--style".to_owned());
                    argv.push(style.clone());
                }
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

                if record.kind != "expected_error" {
                    if let Some(mut value) = evidence.take() {
                        let route_style = value
                            .get("policy")
                            .filter(|policy| !policy.is_null())
                            .map(provenance::effective_route_style)
                            .transpose()
                            .with_context(|| {
                                format!("{stem}: effective route-clarity style is invalid")
                            })?
                            .map(str::to_owned)
                            .unwrap_or_else(|| style.clone());
                        let route_report = route_clarity::analyze(
                            &input_bytes,
                            &process.stdout,
                            &route_style,
                            mode,
                        )
                        .with_context(|| format!("{stem}: route-clarity analysis failed"))?;
                        let route_status = route_report
                            .get("status")
                            .and_then(Value::as_str)
                            .unwrap_or("inconclusive");
                        if matches!(route_status, "risk" | "inconclusive") {
                            let warnings = value
                                .get_mut("warnings")
                                .and_then(Value::as_array_mut)
                                .ok_or_else(|| {
                                    anyhow!("{stem}: evidence.warnings is not a string list")
                                })?;
                            for code in route_report["findings"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(|finding| finding["code"].as_str())
                            {
                                warnings.push(Value::String(format!(
                                    "route_clarity:{route_status}:{code}"
                                )));
                            }
                        }
                        append_fallback_geometry_warning(&mut value, &stem)?;
                        value["route_clarity"] = route_report;
                        common::write_json(&evidence_path, &value)?;
                        evidence = Some(value);
                    }
                    match evidence.as_ref().and_then(|value| value.get("policy")) {
                        Some(policy) => {
                            provenance::validate_policy(policy)
                                .with_context(|| format!("{stem}: invalid effective policy"))?;
                            let policy_sha256 = policy
                                .get("sha256")
                                .and_then(Value::as_str)
                                .ok_or_else(|| {
                                    anyhow::anyhow!("{stem}: effective policy digest is missing")
                                })?;
                            policy_observations.push(json!({
                                "case_id": case_id,
                                "policy_sha256": policy_sha256,
                                "policy": policy,
                            }));
                        }
                        None => row_failures.push(
                            "successful render did not produce effective policy evidence"
                                .to_owned(),
                        ),
                    }
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
                    "policy": evidence
                        .as_ref()
                        .and_then(|value| value.get("policy"))
                        .cloned()
                        .unwrap_or(Value::Null),
                    "dimensions": { "stdout_rows": common::dimensions(&process.stdout)["stdout_rows"], "stdout_max_codepoints": common::dimensions(&process.stdout)["stdout_max_codepoints"], "stdout_bytes": process.stdout.len(), "display": evidence_dimensions },
                    "findings": {
                        "critic": evidence.as_ref().and_then(|value| value["critic"]["findings"].as_array()).map_or(0, Vec::len),
                        "raw_errors": evidence.as_ref().and_then(|value| value["raw"]["errors"].as_array()).map_or(0, Vec::len),
                        "geometry_errors": evidence.as_ref().and_then(|value| value["geometry"]["errors"].as_array()).map_or(0, Vec::len),
                    },
                    "identity_ref": Value::Null,
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
    provenance::bind_policy_observations(&mut identity, &mut policy_observations)?;
    let policy_set_sha256 = identity["provenance"]["policy_set"]["sha256"]
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("policy set digest is missing"))?;
    let run_identity = persist::run_identity_value(
        &run_spec_id,
        role,
        &initial_source_sha256,
        &workload_sha256,
        &policy_set_sha256,
    );
    identity["run_identity"] = run_identity.clone();
    common::write_json(&stage.join("identity.json"), &identity)?;
    let identity_sha256 = common::sha256_file(&stage.join("identity.json"))?;
    let row_identity_ref = common::identity_ref(&identity, &identity_sha256)?;
    for row in &mut rows {
        row["identity_ref"] = row_identity_ref.clone();
    }
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
    persist::write_run_state(
        stage,
        &persist::run_state_value(
            &run_spec_id,
            Some(&run_identity),
            "ready",
            _out,
            stage,
            Some(&packet_digest),
            &created_at,
            "packet complete and policy-bound digests written",
            false,
            Some(&publication_guard),
        ),
    )?;
    persist::pause_if_requested("ready", stage)?;
    let _ = metadata_path;
    Ok(())
}

fn append_fallback_geometry_warning(evidence: &mut Value, stem: &str) -> Result<()> {
    let fallback_count = evidence
        .get("geometry")
        .and_then(|geometry| geometry.get("untraced_fallback_edges"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if fallback_count == 0 {
        return Ok(());
    }

    let warnings = evidence
        .get_mut("warnings")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow!("{stem}: evidence.warnings is not a string list"))?;
    let warning = format!("geometry:fallback_edges_requires_human_review:{fallback_count}");
    if !warnings
        .iter()
        .any(|existing| existing.as_str() == Some(warning.as_str()))
    {
        warnings.push(Value::String(warning));
    }
    Ok(())
}

/// Validate the private packet's identity and referenced bytes immediately
/// before the irreversible directory claim. This is intentionally narrower
/// than the baseline/quality validator: custom QA matrices remain publishable,
/// while no incomplete or internally inconsistent packet can become visible.
fn validate_packet_for_publication(
    stage: &Path,
    expected_role: &str,
    expected_queue: Option<(&str, &str)>,
) -> Result<()> {
    let run_spec = common::load_json(&stage.join("run_spec.json"), "staged run spec")?;
    persist::validate_run_spec(&run_spec)?;
    let identity = common::load_json(&stage.join("identity.json"), "staged identity")?;
    provenance::validate_identity(&identity)?;
    let identity_sha256 = common::sha256_file(&stage.join("identity.json"))?;
    let run_identity = identity
        .get("run_identity")
        .ok_or_else(|| anyhow!("staged identity is missing run_identity"))?;
    persist::validate_run_identity(run_identity)?;
    if run_identity.get("role").and_then(Value::as_str) != Some(expected_role)
        || run_spec.get("role").and_then(Value::as_str) != Some(expected_role)
        || run_identity.get("run_spec_id") != run_spec.get("run_spec_id")
    {
        bail!("staged packet role or run-spec identity is inconsistent");
    }
    let requested_contract = run_spec
        .get("requested_policy_context")
        .and_then(|context| context.get("argv_contract"))
        .ok_or_else(|| anyhow!("staged run spec is missing argv_contract"))?;
    provenance::validate_argv_contract(requested_contract)?;
    let identity_contract = provenance::identity_argv_contract(&identity)?
        .ok_or_else(|| anyhow!("staged identity is missing argv_contract"))?;
    if requested_contract != identity_contract {
        bail!("staged run spec and identity argv contracts differ");
    }

    let state = common::load_json(&stage.join("run_state.json"), "staged run state")?;
    persist::validate_run_state(&state)?;
    if state["state"] != "ready"
        || state["final_claimed"] != Value::Bool(false)
        || state["run_identity"] != *run_identity
    {
        bail!("staged packet is not in an unclaimed ready state");
    }

    if let Some((queue_id, queue_sha256)) = expected_queue {
        let schema_manifest = common::load_json(
            &stage.join("schema_manifest.json"),
            "staged schema manifest",
        )?;
        if schema_manifest.get("queue_id").and_then(Value::as_str) != Some(queue_id)
            || schema_manifest.get("queue_sha256").and_then(Value::as_str) != Some(queue_sha256)
        {
            bail!("staged schema queue identity is inconsistent");
        }
    }

    let manifest = common::require_file(&stage.join("manifest.jsonl"), "staged manifest")?;
    let mut row_count = 0usize;
    for (line_number, line) in String::from_utf8(manifest.clone())?.lines().enumerate() {
        if line.trim().is_empty() {
            bail!("staged manifest line {} is empty", line_number + 1);
        }
        let row: Value = serde_json::from_str(line)
            .with_context(|| format!("parse staged manifest line {}", line_number + 1))?;
        if !common::is_audit_schema(row.get("schema").unwrap_or(&Value::Null)) {
            bail!(
                "staged manifest line {} has the wrong schema",
                line_number + 1
            );
        }
        common::validate_row_identity(
            &row,
            &identity,
            &identity_sha256,
            &format!("staged manifest line {} identity", line_number + 1),
        )?;
        provenance::validate_row_argv(
            identity_contract,
            &row,
            &format!("staged manifest line {}", line_number + 1),
        )?;
        common::validate_blob_ref(stage, &row["stdout"], "staged frame")?;
        common::validate_blob_ref(stage, &row["stderr"], "staged stderr")?;
        if row.get("evidence").is_some_and(|value| !value.is_null()) {
            common::validate_blob_ref(stage, &row["evidence"], "staged evidence")?;
        }
        if row["classification"] != "expected_error" {
            let policy = row
                .get("policy")
                .filter(|value| !value.is_null())
                .ok_or_else(|| anyhow!("staged successful row is missing policy"))?;
            provenance::validate_policy(policy)?;
        }
        row_count += 1;
    }
    if row_count == 0 {
        bail!("staged manifest is empty");
    }
    let summary = common::load_json(&stage.join("summary.json"), "staged summary")?;
    if summary.get("failures") != Some(&Value::Array(Vec::new()))
        || summary.get("actual_rows").and_then(Value::as_u64) != Some(row_count as u64)
    {
        bail!("staged summary does not match the manifest");
    }
    let complete = common::load_json(&stage.join("COMPLETE.json"), "staged completion marker")?;
    if complete.get("schema").and_then(Value::as_str) != Some("termiflow.visual_audit.complete.v1")
        || complete.get("rows").and_then(Value::as_u64) != Some(row_count as u64)
        || complete.get("manifest_sha256").and_then(Value::as_str)
            != Some(common::sha256_bytes(&manifest).as_str())
    {
        bail!("staged completion marker does not match the manifest");
    }
    let packet_digest = complete
        .get("packet_sha256")
        .and_then(Value::as_str)
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| anyhow!("staged completion marker packet digest is invalid"))?;
    let (actual_packet_digest, packet_listing) = common::deterministic_digest(stage)?;
    if actual_packet_digest != packet_digest
        || common::require_file(&stage.join("PACKET.sha256"), "staged packet listing")?
            != packet_listing.as_bytes()
    {
        bail!("staged packet digest or listing is stale");
    }
    Ok(())
}

pub fn run_schema_packet(
    root: &Path,
    manifest_path: &Path,
    out: &Path,
    options: SchemaPacketOptions<'_>,
) -> Result<SchemaPacketResult> {
    let SchemaPacketOptions {
        holdouts,
        supplied_binary,
        display_profile,
        timeout_seconds,
        respect_input_style,
    } = options;
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
        !respect_input_style,
        Some(&identity),
    );
    for path in &workload.temporary_inputs {
        let _ = fs::remove_file(path);
    }
    match result {
        Ok(()) => {
            if let Err(error) = validate_packet_for_publication(
                &stage,
                if holdouts { "holdout" } else { "review" },
                Some((&workload.queue_id, &workload.queue_sha256)),
            ) {
                if let Err(cleanup) = persist::remove_incomplete_directory(&stage) {
                    eprintln!("schema packet recovery required: {cleanup}");
                }
                return Err(error);
            }
            persist::pause_if_requested("before-publish", &stage)?;
            persist::publish_directory(&stage, &out, b"publisher=schema-packet\n")?;
            persist::pause_if_requested("after-publish", &out)?;
        }
        Err(error) => {
            if let Err(cleanup) = persist::remove_incomplete_directory(&stage) {
                eprintln!("schema packet recovery required: {cleanup}");
            }
            return Err(error);
        }
    }
    let manifest_sha256 = common::sha256_file(&out.join("manifest.jsonl"))?;
    let identity_sha256 = common::sha256_file(&out.join("identity.json"))?;
    let packet_sha256 = common::sha256_file(&out.join("PACKET.sha256"))?;
    let complete_sha256 = common::sha256_file(&out.join("COMPLETE.json"))?;
    let complete = common::load_json(&out.join("COMPLETE.json"), "packet completion marker")?;
    let deterministic_packet_sha256 = complete
        .get("packet_sha256")
        .and_then(Value::as_str)
        .filter(|value| value.len() == 64)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("packet completion marker is missing packet_sha256"))?;
    let packet_identity = common::load_json(&out.join("identity.json"), "packet identity")?;
    let run_identity = packet_identity
        .get("run_identity")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("packet identity is missing run_identity"))?;
    persist::validate_run_identity(&run_identity)?;
    Ok(SchemaPacketResult {
        out,
        manifest_sha256,
        identity_sha256,
        packet_sha256,
        complete_sha256,
        deterministic_packet_sha256,
        queue_id: workload.queue_id,
        queue_sha256: workload.queue_sha256,
        row_count: workload.planned_count,
        run_identity,
    })
}

/// Re-open a complete schema packet for receipt reconciliation without
/// rerunning its renderer or creating a second publication candidate.
pub fn load_published_schema_packet(
    out: &Path,
    queue_id: &str,
    queue_sha256: &str,
) -> Result<SchemaPacketResult> {
    if queue_sha256.len() != 64 || !queue_sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("published packet queue_sha256 must be a SHA-256 digest");
    }
    let identity = common::load_json(&out.join("identity.json"), "published packet identity")?;
    provenance::validate_identity(&identity)?;
    let run_identity = identity
        .get("run_identity")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("published packet identity is missing run_identity"))?;
    persist::validate_run_identity(&run_identity)?;
    if run_identity["role"] != "holdout" {
        bail!("published packet run role is not holdout");
    }
    let schema_manifest = common::load_json(
        &out.join("schema_manifest.json"),
        "published packet schema manifest",
    )?;
    if schema_manifest.get("queue_id").and_then(Value::as_str) != Some(queue_id)
        || schema_manifest.get("queue_sha256").and_then(Value::as_str) != Some(queue_sha256)
    {
        bail!("published packet queue identity does not match the requested holdout");
    }
    let state = common::load_json(&out.join("run_state.json"), "published packet run state")?;
    persist::validate_run_state(&state)?;
    if state["run_identity"] != run_identity {
        bail!("published packet state identity does not match identity.json");
    }
    if state["state"] != "published" {
        persist::repair_published_state(out)
            .context("repair published state before receipt reconciliation")?;
    }
    let state = common::load_json(&out.join("run_state.json"), "repaired packet run state")?;
    persist::validate_run_state(&state)?;
    if state["state"] != "published" || state["run_identity"] != run_identity {
        bail!("published packet state was not repaired to its final identity");
    }
    let complete = common::load_json(&out.join("COMPLETE.json"), "published packet completion")?;
    if complete.get("schema").and_then(Value::as_str) != Some("termiflow.visual_audit.complete.v1")
    {
        bail!("published packet completion schema is invalid");
    }
    let manifest_sha256 = common::sha256_file(&out.join("manifest.jsonl"))?;
    if complete.get("manifest_sha256").and_then(Value::as_str) != Some(manifest_sha256.as_str()) {
        bail!("published packet completion manifest digest is stale");
    }
    let complete_packet_sha256 = complete
        .get("packet_sha256")
        .and_then(Value::as_str)
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| anyhow::anyhow!("published packet completion packet digest is invalid"))?;
    let (actual_packet_sha256, packet_listing) = common::deterministic_digest(out)?;
    if actual_packet_sha256 != complete_packet_sha256 {
        bail!("published packet completion packet digest is stale");
    }
    if common::require_file(&out.join("PACKET.sha256"), "published packet listing")?
        != packet_listing.as_bytes()
    {
        bail!("published packet listing is stale");
    }
    let packet_sha256 = common::sha256_file(&out.join("PACKET.sha256"))?;
    let complete_sha256 = common::sha256_file(&out.join("COMPLETE.json"))?;
    let rows = load_packet_rows_for_reconciliation(out)?;
    if complete.get("rows").and_then(Value::as_u64) != Some(rows.len() as u64) {
        bail!("published packet completion row count is stale");
    }
    Ok(SchemaPacketResult {
        out: out.to_path_buf(),
        manifest_sha256,
        identity_sha256: common::sha256_file(&out.join("identity.json"))?,
        packet_sha256,
        complete_sha256,
        deterministic_packet_sha256: complete_packet_sha256.to_owned(),
        queue_id: queue_id.to_owned(),
        queue_sha256: queue_sha256.to_owned(),
        row_count: rows.len(),
        run_identity,
    })
}

fn load_packet_rows_for_reconciliation(out: &Path) -> Result<Vec<Value>> {
    let bytes = common::require_file(&out.join("manifest.jsonl"), "published packet manifest")?;
    String::from_utf8(bytes)
        .context("published packet manifest is not UTF-8")?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).context("parse published packet row"))
        .collect()
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
    let mut temporary_inputs = Vec::new();

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
        let source = required_manifest_string(row, "source", key)?;
        let source_sha256 = required_manifest_string(row, "source_sha256", key)?;
        let input_path = match row
            .get("input_path")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            Some(input_path) => common::safe_relative_path(
                Path::new(input_path),
                root,
                &format!("schema manifest {fixture} input_path"),
            )?,
            None if holdouts => {
                if common::sha256_bytes(source.as_bytes()) != source_sha256 {
                    bail!("schema manifest source hash mismatch for {fixture}");
                }
                let input_path = std::env::temp_dir().join(format!(
                    "termiflow-holdout-input-{}-{}-{}.md",
                    std::process::id(),
                    common::now_label(),
                    &source_sha256[..16]
                ));
                if input_path.exists() {
                    bail!(
                        "temporary holdout input already exists: {}",
                        input_path.display()
                    );
                }
                common::write_bytes(&input_path, source.as_bytes())?;
                temporary_inputs.push(input_path.clone());
                input_path
            }
            None => {
                bail!("schema manifest {key}.input_path must be a non-empty string")
            }
        };
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
        temporary_inputs,
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
    let stage = persist::claim_directory_stage(out)?;
    fs::create_dir_all(stage.join("frames"))?;
    fs::create_dir_all(stage.join("logs"))?;
    fs::create_dir_all(stage.join("evidence"))?;
    persist::pause_if_requested("stage-created", &stage)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_geometry_warning_is_counted_and_idempotent() {
        let mut evidence = json!({
            "warnings": [],
            "geometry": {
                "untraced_fallback_edges": ["edge:0:A->B", "edge:1:A->C"]
            }
        });

        append_fallback_geometry_warning(&mut evidence, "case").unwrap();
        append_fallback_geometry_warning(&mut evidence, "case").unwrap();

        assert_eq!(
            evidence["warnings"],
            json!(["geometry:fallback_edges_requires_human_review:2"])
        );
        assert_eq!(
            evidence["geometry"]["untraced_fallback_edges"],
            json!(["edge:0:A->B", "edge:1:A->C"])
        );
    }

    #[test]
    fn fallback_geometry_warning_is_quiet_for_traced_geometry() {
        let mut evidence = json!({
            "warnings": [],
            "geometry": {"untraced_fallback_edges": []}
        });

        append_fallback_geometry_warning(&mut evidence, "case").unwrap();

        assert_eq!(evidence["warnings"], json!([]));
    }
}
