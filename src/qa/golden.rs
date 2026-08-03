use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use super::common;

#[derive(Debug)]
pub struct GoldenArgs {
    pub check: bool,
    pub approve: bool,
    pub intent: Option<String>,
    pub manifest: Option<PathBuf>,
    pub binary: Option<PathBuf>,
    pub input_root: PathBuf,
    pub metadata: PathBuf,
    pub styles: String,
    pub report: Option<PathBuf>,
}

pub fn run(args: GoldenArgs) -> Result<i32> {
    if args.check && args.approve {
        bail!("--check and --approve are mutually exclusive");
    }
    if args.approve && args.intent.as_deref().is_none_or(str::is_empty) {
        bail!("--approve requires --intent TEXT");
    }
    let root = std::env::current_dir().context("resolve repository root")?;
    if let Some(manifest) = args.manifest.as_deref() {
        let manifest_path = resolve(&root, manifest);
        let stage =
            std::env::temp_dir().join(format!("termiflow-golden-manifest-{}", common::now_label()));
        fs::create_dir_all(&stage)?;
        let result = run_manifest_checks(&root, &stage, &manifest_path, &args);
        let _ = fs::remove_dir_all(&stage);
        return result;
    }
    let styles = common::parse_csv(&args.styles, &["ascii", "unicode"], "styles")?;
    let input_root = resolve(&root, &args.input_root);
    let metadata_path = resolve(&root, &args.metadata);
    let (metadata, _) = common::load_metadata(&metadata_path, &input_root)?;
    let input_paths = common::collect_inputs(&input_root)?;
    let stage = std::env::temp_dir().join(format!("termiflow-golden-{}", common::now_label()));
    fs::create_dir_all(&stage)?;
    let result = run_checks(&root, &stage, &metadata, &input_paths, &styles, &args);
    let _ = fs::remove_dir_all(&stage);
    result
}

fn run_checks(
    root: &Path,
    stage: &Path,
    metadata: &std::collections::BTreeMap<String, common::FixtureMetadata>,
    input_paths: &std::collections::BTreeMap<String, PathBuf>,
    styles: &[String],
    args: &GoldenArgs,
) -> Result<i32> {
    let binary = common::discover_binary(root, stage, args.binary.as_deref())?;
    let mut changes = Vec::new();
    let mut failures = Vec::new();
    let mut candidates: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for fixture in metadata.keys() {
        let input_path = input_paths
            .get(fixture)
            .with_context(|| format!("missing input for {fixture}"))?;
        for style in styles {
            let command = vec![
                binary.to_string_lossy().to_string(),
                "--print".to_owned(),
                "--style".to_owned(),
                style.clone(),
                input_path.to_string_lossy().to_string(),
            ];
            let process = common::process(&command, root, std::time::Duration::from_secs(60));
            let record = metadata.get(fixture).expect("metadata key exists");
            let stream_failures = common::validate_streams(
                root,
                fixture,
                style,
                record,
                process.status,
                &process.stdout,
                &process.stderr,
            );
            if !stream_failures.is_empty() {
                failures.extend(
                    stream_failures
                        .into_iter()
                        .map(|failure| format!("{fixture}.{style}: {failure}")),
                );
                continue;
            }
            let output = if record.kind == "expected_error" {
                process.stderr
            } else {
                process.stdout
            };
            let expected = root
                .join("tests/fixtures/expected")
                .join(format!("{fixture}.{style}.txt"));
            let previous = expected
                .is_file()
                .then(|| fs::read(&expected))
                .transpose()?;
            if previous.as_deref() != Some(output.as_slice()) {
                changes.push(json!({
                    "path": common::relative_to_root(&expected, root),
                    "fixture": fixture,
                    "style": style,
                    "old_sha256": previous.as_deref().map(common::sha256_bytes),
                    "new_sha256": common::sha256_bytes(&output),
                    "old_bytes": previous.as_ref().map(Vec::len),
                    "new_bytes": output.len(),
                }));
                candidates.push((expected, output));
            }
        }
    }
    if !failures.is_empty() {
        eprintln!("golden update: renderer contract failed:");
        for failure in failures.iter().take(20) {
            eprintln!("  {failure}");
        }
        bail!("golden update renderer contract failed");
    }
    if args.approve {
        for (path, output) in candidates {
            common::atomic_replace(&path, &output)?;
        }
        eprintln!(
            "golden update approved: wrote {} snapshot(s)",
            changes.len()
        );
    } else if changes.is_empty() {
        eprintln!("golden update check: snapshots are current");
    } else {
        eprintln!(
            "golden update check: {} snapshot change(s) require --approve --intent",
            changes.len()
        );
    }
    let report = json!({
        "schema": "termiflow.golden_update.v1",
        "mode": if args.approve { "approve" } else { "check" },
        "intent": args.intent,
        "source_commit": common::run_text(&["git", "rev-parse", "HEAD"], root),
        "checked_files": metadata.len() * styles.len(),
        "changes": changes,
        "created_at": common::now_label(),
    });
    let rendered = {
        let mut bytes = serde_json::to_vec_pretty(&report)?;
        bytes.push(b'\n');
        bytes
    };
    if let Some(path) = &args.report {
        common::write_bytes(&resolve(root, path), &rendered)?;
    } else {
        print!("{}", String::from_utf8_lossy(&rendered));
    }
    Ok(
        if !args.approve && !report["changes"].as_array().is_none_or(Vec::is_empty) {
            1
        } else {
            0
        },
    )
}

const MANIFEST_SCHEMA: &str = "termiflow.fixture_manifest.v2";
const MANIFEST_REPORT_SCHEMA: &str = "termiflow.golden_manifest_update.v1";

struct ManifestData {
    manifest_sha256: String,
    spec_sha256: String,
    queue_id: String,
    queue_sha256: String,
    rows: Vec<Value>,
    negative_cases: Vec<Value>,
    holdout_variant_count: usize,
    holdout_row_count: usize,
}

fn run_manifest_checks(
    root: &Path,
    stage: &Path,
    manifest_path: &Path,
    args: &GoldenArgs,
) -> Result<i32> {
    let manifest_bytes = common::require_file(manifest_path, "fixture manifest")?;
    let manifest_sha256 = common::sha256_bytes(&manifest_bytes);
    let document: Value = serde_json::from_slice(&manifest_bytes)
        .with_context(|| format!("parse fixture manifest JSON: {}", manifest_path.display()))?;
    let manifest = validate_manifest(root, &document, manifest_sha256)?;
    let binary = common::discover_binary(root, stage, args.binary.as_deref())?;
    let identity = common::source_identity(root, &binary, "terminal-grid-v1")?;

    let mut failures = Vec::new();
    let mut changes = Vec::new();
    let mut candidates = Vec::new();
    let mut negative_results = Vec::new();
    let eligible_rows = manifest
        .rows
        .iter()
        .filter(|row| row.get("golden").is_some_and(|value| !value.is_null()))
        .count();

    for row in &manifest.rows {
        let Some(golden) = row.get("golden").filter(|value| !value.is_null()) else {
            continue;
        };
        let case_id = value_string(row.get("case_id"), "manifest case_id")?;
        let variant_id = value_string(row.get("variant_id"), "manifest variant_id")?;
        let source = value_string(row.get("source"), "manifest source")?;
        let source_path = stage_source(stage, &format!("{case_id}--{variant_id}"), &source)?;
        let style = value_string(row.get("style"), "manifest style")?;
        let target = target_path(
            root,
            value_string(
                golden.get("path"),
                &format!("manifest row {variant_id} golden path"),
            )?,
        )?;
        let command = vec![
            binary.to_string_lossy().to_string(),
            "--print".to_owned(),
            "--style".to_owned(),
            style.clone(),
            source_path.to_string_lossy().to_string(),
        ];
        let process = common::process(&command, root, std::time::Duration::from_secs(60));
        let row_label = format!(
            "{}.{}.{}",
            value_string(row.get("case_id"), "manifest case_id")?,
            style,
            value_string(row.get("mode"), "manifest mode")?
        );
        if process.status != 0 {
            failures.push(format!(
                "{row_label}: expected success, got status {}",
                process.status
            ));
            continue;
        }
        if process.stdout.is_empty() {
            failures.push(format!(
                "{row_label}: successful fixture produced empty stdout"
            ));
            continue;
        }
        if !process.stderr.is_empty() {
            failures.push(format!(
                "{row_label}: successful fixture wrote unexpected stderr"
            ));
            continue;
        }

        let previous = read_target(&target)?;
        if previous.as_deref() != Some(process.stdout.as_slice()) {
            changes.push(json!({
                "path": common::relative_to_root(&target, root),
                "case_id": row["case_id"],
                "variant_id": row["variant_id"],
                "style": row["style"],
                "mode": row["mode"],
                "source_sha256": row["source_sha256"],
                "old_sha256": previous.as_deref().map(common::sha256_bytes),
                "new_sha256": common::sha256_bytes(&process.stdout),
                "old_bytes": previous.as_ref().map(Vec::len),
                "new_bytes": process.stdout.len(),
            }));
            candidates.push((target, process.stdout));
        }
    }

    for negative in &manifest.negative_cases {
        let kind = value_string(negative.get("kind"), "negative kind")?;
        let variant_id = value_string(negative.get("variant_id"), "negative variant_id")?;
        if kind == "expected_error" {
            failures.push(format!(
                "{variant_id}: expected_error requires an explicit stderr oracle in the manifest"
            ));
            continue;
        }
        let source = value_string(negative.get("source"), "negative source")?;
        let case_id = value_string(negative.get("case_id"), "negative case_id")?;
        let source_path = stage_source(stage, &format!("{case_id}--{variant_id}"), &source)?;
        let styles = string_array(negative.get("styles"), "negative styles")?;
        let modes = string_array(negative.get("modes"), "negative modes")?;
        for style in styles {
            for mode in &modes {
                let mut command = vec![
                    binary.to_string_lossy().to_string(),
                    "--print".to_owned(),
                    "--style".to_owned(),
                    style.clone(),
                ];
                if mode == "optimized" {
                    command.push("--optimize-render".to_owned());
                }
                command.push(source_path.to_string_lossy().to_string());
                let process = common::process(&command, root, std::time::Duration::from_secs(60));
                let mut row_failures = Vec::new();
                if process.status != 0 {
                    row_failures.push(format!(
                        "expected warning success, got status {}",
                        process.status
                    ));
                }
                if process.stdout.is_empty() {
                    row_failures.push("warning fixture produced empty stdout".to_owned());
                }
                let stderr = String::from_utf8_lossy(&process.stderr);
                for pattern in
                    string_array(negative.get("stderr_contains"), "negative stderr_contains")?
                {
                    if !stderr.contains(&pattern) {
                        row_failures.push(format!(
                            "stderr is missing expected warning text {pattern:?}"
                        ));
                    }
                }
                if !row_failures.is_empty() {
                    failures.extend(
                        row_failures
                            .iter()
                            .map(|failure| format!("{variant_id}.{style}.{mode}: {failure}")),
                    );
                }
                negative_results.push(json!({
                    "variant_id": variant_id,
                    "style": style,
                    "mode": mode,
                    "kind": kind,
                    "status": process.status,
                    "stdout_sha256": common::sha256_bytes(&process.stdout),
                    "stderr_sha256": common::sha256_bytes(&process.stderr),
                    "failures": row_failures,
                }));
            }
        }
    }

    let report = json!({
        "schema": MANIFEST_REPORT_SCHEMA,
        "mode": if args.approve { "approve" } else { "check" },
        "manifest_path": common::relative_to_root(manifest_path, root),
        "manifest_sha256": manifest.manifest_sha256,
        "spec_sha256": manifest.spec_sha256,
        "queue_id": manifest.queue_id,
        "queue_sha256": manifest.queue_sha256,
        "identity": identity,
        "eligible_rows": eligible_rows,
        "candidate_count": eligible_rows,
        "changed_candidate_count": candidates.len(),
        "negative_results": negative_results,
        "holdout_variant_count": manifest.holdout_variant_count,
        "holdout_row_count": manifest.holdout_row_count,
        "failures": failures,
        "changes": changes,
    });

    if !report["failures"].as_array().is_none_or(Vec::is_empty) {
        emit_report(root, args.report.as_ref(), &report)?;
        bail!(
            "schema golden bridge failed:\n{}",
            report["failures"]
                .as_array()
                .unwrap_or(&Vec::new())
                .iter()
                .filter_map(Value::as_str)
                .take(20)
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    if args.approve {
        for (path, output) in candidates {
            common::atomic_replace(&path, &output)?;
        }
        eprintln!(
            "golden manifest update approved: wrote {} snapshot(s)",
            report["changes"].as_array().map_or(0, Vec::len)
        );
    } else if report["changes"].as_array().is_none_or(Vec::is_empty) {
        eprintln!("golden manifest check: snapshots are current");
    } else {
        eprintln!("golden manifest check: snapshot changes require --approve --intent");
    }
    emit_report(root, args.report.as_ref(), &report)?;
    Ok(
        if !args.approve && !report["changes"].as_array().is_none_or(Vec::is_empty) {
            1
        } else {
            0
        },
    )
}

fn validate_manifest(
    root: &Path,
    document: &Value,
    manifest_sha256: String,
) -> Result<ManifestData> {
    let object = document
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("fixture manifest must be an object"))?;
    let allowed = BTreeSet::from([
        "schema",
        "spec_schema",
        "spec_version",
        "queue_id",
        "families",
        "queue_sha256",
        "spec_sha256",
        "row_count",
        "negative_case_count",
        "holdout_variant_count",
        "holdout_row_count",
        "rows",
        "negative_cases",
        "holdouts",
    ]);
    let unknown: Vec<_> = object
        .keys()
        .filter(|key| !allowed.contains(key.as_str()))
        .cloned()
        .collect();
    if !unknown.is_empty() {
        bail!(
            "fixture manifest contains unknown field(s): {}",
            unknown.join(", ")
        );
    }
    if document.get("schema").and_then(Value::as_str) != Some(MANIFEST_SCHEMA) {
        bail!("fixture manifest schema must be {MANIFEST_SCHEMA}");
    }
    if document.get("spec_schema").and_then(Value::as_str) != Some("termiflow.fixture_spec.v2")
        || document.get("spec_version").and_then(Value::as_i64) != Some(2)
    {
        bail!("fixture manifest spec identity is invalid");
    }
    let queue_id = value_string(document.get("queue_id"), "manifest queue_id")?;
    let _families = string_array(document.get("families"), "manifest families")?;
    let queue_sha256 = value_string(document.get("queue_sha256"), "manifest queue_sha256")?;
    let spec_sha256 = value_string(document.get("spec_sha256"), "manifest spec_sha256")?;
    let row_values = document
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("manifest rows must be an array"))?;
    let expected_rows = document
        .get("row_count")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("manifest row_count must be an integer"))?;
    if expected_rows != row_values.len() as u64 {
        bail!("manifest row_count does not match rows");
    }
    let declared_negative = document
        .get("negative_case_count")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("manifest negative_case_count must be an integer"))?;
    let declared_holdout_rows = document
        .get("holdout_row_count")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("manifest holdout_row_count must be an integer"))?;

    let mut row_ids = BTreeSet::new();
    let mut targets = BTreeSet::new();
    for row in row_values {
        let case_id = value_string(row.get("case_id"), "manifest case_id")?;
        let variant_id = value_string(row.get("variant_id"), "manifest variant_id")?;
        let style = value_string(row.get("style"), "manifest style")?;
        let mode = value_string(row.get("mode"), "manifest mode")?;
        if !common::STYLES.contains(&style.as_str()) || !common::MODES.contains(&mode.as_str()) {
            bail!("manifest row {variant_id} has unsupported style or mode");
        }
        let key = format!("{case_id}\u{1f}{variant_id}\u{1f}{style}\u{1f}{mode}");
        if !row_ids.insert(key) {
            bail!("manifest contains duplicate row {variant_id}.{style}.{mode}");
        }
        if value_string(row.get("kind"), "manifest kind")? != "success" {
            bail!("manifest row {variant_id} is not a success row");
        }
        if row.get("holdout").and_then(Value::as_str) == Some("evaluator_owned") {
            bail!("evaluator-owned holdout leaked into manifest rows: {variant_id}");
        }
        let source = value_string(row.get("source"), "manifest source")?;
        let source_sha256 = value_string(row.get("source_sha256"), "manifest source_sha256")?;
        if common::sha256_bytes(source.as_bytes()) != source_sha256 {
            bail!("manifest source hash mismatch for {variant_id}");
        }
        if let Some(input_path) = row.get("input_path").and_then(Value::as_str) {
            let path = common::safe_relative_path(
                Path::new(input_path),
                root,
                &format!("manifest {variant_id} input_path"),
            )?;
            if fs::read(&path)? != source.as_bytes() {
                bail!("manifest source does not match input_path for {variant_id}");
            }
        }
        let golden_stem = value_string(row.get("golden_stem"), "manifest golden_stem")?;
        if !safe_identifier(&golden_stem) {
            bail!("manifest golden_stem is unsafe for {variant_id}");
        }
        let golden = row.get("golden").filter(|value| !value.is_null());
        let is_golden_row = mode == "default" && matches!(style.as_str(), "ascii" | "unicode");
        if is_golden_row && golden.is_none() {
            bail!("manifest default row is missing a golden target for {variant_id}.{style}");
        }
        if !is_golden_row && golden.is_some() {
            bail!("manifest golden target has unsupported mode or style for {variant_id}");
        }
        if let Some(golden) = golden {
            let golden_object = golden
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("manifest golden target must be an object"))?;
            let allowed_golden = BTreeSet::from(["mode", "path"]);
            if golden_object
                .keys()
                .any(|key| !allowed_golden.contains(key.as_str()))
            {
                bail!("manifest golden target contains unknown fields for {variant_id}");
            }
            if golden.get("mode").and_then(Value::as_str) != Some("default") {
                bail!("manifest golden target mode must be default for {variant_id}");
            }
            let relative_path = value_string(golden.get("path"), "manifest golden path")?;
            let path = target_path(root, relative_path.clone())?;
            let expected_name = format!("{golden_stem}.{style}.txt");
            if path.file_name().and_then(|name| name.to_str()) != Some(expected_name.as_str()) {
                bail!("manifest golden path does not match golden_stem for {variant_id}");
            }
            if !targets.insert(path) {
                bail!("manifest contains duplicate golden target for {variant_id}");
            }
        }
    }

    let negative_cases = document
        .get("negative_cases")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("manifest negative_cases must be an array"))?
        .clone();
    if declared_negative != negative_cases.len() as u64 {
        bail!("manifest negative_case_count does not match negative_cases");
    }
    for negative in &negative_cases {
        let variant_id = value_string(negative.get("variant_id"), "negative variant_id")?;
        let source = value_string(negative.get("source"), "negative source")?;
        let expected_hash = value_string(negative.get("source_sha256"), "negative source_sha256")?;
        if common::sha256_bytes(source.as_bytes()) != expected_hash {
            bail!("negative source hash mismatch for {variant_id}");
        }
        let kind = value_string(negative.get("kind"), "negative kind")?;
        if !matches!(kind.as_str(), "warning" | "expected_error") {
            bail!("negative variant {variant_id} has unsupported kind {kind}");
        }
        let stderr_policy = value_string(negative.get("stderr_policy"), "negative stderr_policy")?;
        let expected_policy = if kind == "warning" {
            "warning"
        } else {
            "error"
        };
        if stderr_policy != expected_policy {
            bail!("negative variant {variant_id} has invalid stderr_policy");
        }
        let _ = string_array(negative.get("styles"), "negative styles")?;
        let _ = string_array(negative.get("modes"), "negative modes")?;
        if let Some(input_path) = negative.get("input_path").and_then(Value::as_str) {
            let path = common::safe_relative_path(
                Path::new(input_path),
                root,
                &format!("negative {variant_id} input_path"),
            )?;
            if fs::read(&path)? != source.as_bytes() {
                bail!("negative source does not match input_path for {variant_id}");
            }
        }
    }
    let holdouts = document
        .get("holdouts")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("manifest holdouts must be an array"))?
        .clone();
    if declared_holdout_rows != holdouts.len() as u64 {
        bail!("manifest holdout_row_count does not match holdouts");
    }
    let mut holdout_row_ids = BTreeSet::new();
    let mut holdout_variant_ids = BTreeSet::new();
    for holdout in &holdouts {
        let case_id = value_string(holdout.get("case_id"), "holdout case_id")?;
        let variant_id = value_string(holdout.get("variant_id"), "holdout variant_id")?;
        let _family = value_string(holdout.get("family"), "holdout family")?;
        let _direction = value_string(holdout.get("direction"), "holdout direction")?;
        let style = value_string(holdout.get("style"), "holdout style")?;
        let mode = value_string(holdout.get("mode"), "holdout mode")?;
        if !common::STYLES.contains(&style.as_str()) || !common::MODES.contains(&mode.as_str()) {
            bail!("holdout {variant_id} has unsupported style or mode");
        }
        let key = format!("{case_id}\u{1f}{variant_id}\u{1f}{style}\u{1f}{mode}");
        if !holdout_row_ids.insert(key) {
            bail!("manifest contains duplicate holdout row {variant_id}.{style}.{mode}");
        }
        holdout_variant_ids.insert(format!("{case_id}\u{1f}{variant_id}"));
        if value_string(holdout.get("kind"), "holdout kind")? != "success" {
            bail!("holdout {variant_id} is not a success row");
        }
        if value_string(holdout.get("holdout"), "holdout class")? != "evaluator_owned" {
            bail!("holdout {variant_id} is not evaluator-owned");
        }
        if holdout.get("golden").is_some_and(|value| !value.is_null()) {
            bail!("evaluator-owned holdout has a golden target: {variant_id}");
        }
        if holdout
            .get("golden_stem")
            .is_some_and(|value| !value.is_null())
        {
            bail!("evaluator-owned holdout has a golden stem: {variant_id}");
        }
        let source = value_string(holdout.get("source"), "holdout source")?;
        let expected_hash = value_string(holdout.get("source_sha256"), "holdout source_sha256")?;
        if common::sha256_bytes(source.as_bytes()) != expected_hash {
            bail!("holdout source hash mismatch for {variant_id}");
        }
        if let Some(input_path) = holdout.get("input_path").and_then(Value::as_str) {
            let path = common::safe_relative_path(
                Path::new(input_path),
                root,
                &format!("holdout {variant_id} input_path"),
            )?;
            if fs::read(&path)? != source.as_bytes() {
                bail!("holdout source does not match input_path for {variant_id}");
            }
        }
    }
    let holdout_variant_count = holdout_variant_ids.len();
    if document
        .get("holdout_variant_count")
        .and_then(Value::as_u64)
        != Some(holdout_variant_count as u64)
    {
        bail!("manifest holdout_variant_count does not match holdouts");
    }

    Ok(ManifestData {
        manifest_sha256,
        spec_sha256,
        queue_id,
        queue_sha256,
        rows: row_values.clone(),
        negative_cases,
        holdout_variant_count,
        holdout_row_count: holdouts.len(),
    })
}

fn stage_source(stage: &Path, variant_id: &str, source: &str) -> Result<PathBuf> {
    if !safe_identifier(variant_id) {
        bail!("unsafe manifest variant_id: {variant_id}");
    }
    let path = stage.join("inputs").join(format!("{variant_id}.md"));
    if path.is_file() {
        if fs::read(&path)? != source.as_bytes() {
            bail!("manifest variant {variant_id} has conflicting source text");
        }
    } else {
        common::write_bytes(&path, source.as_bytes())?;
    }
    Ok(path)
}

fn safe_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
}

fn target_path(root: &Path, relative: String) -> Result<PathBuf> {
    let path = Path::new(&relative);
    if path.is_absolute() {
        bail!("golden target must be relative: {relative}");
    }
    let components: Vec<String> = path
        .components()
        .map(|component| match component {
            std::path::Component::Normal(value) => Ok(value.to_string_lossy().to_string()),
            _ => Err(anyhow::anyhow!(
                "golden target contains unsafe path: {relative}"
            )),
        })
        .collect::<Result<_>>()?;
    let stem = components.get(3).and_then(|component| {
        component
            .strip_suffix(".ascii.txt")
            .or_else(|| component.strip_suffix(".unicode.txt"))
    });
    if components.len() != 4
        || components[0] != "tests"
        || components[1] != "fixtures"
        || components[2] != "expected"
        || !matches!(stem, Some(stem) if !stem.is_empty() && stem.chars().all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-'))
    {
        bail!("golden target must be tests/fixtures/expected/<stem>.ascii|unicode.txt: {relative}");
    }
    let expected_root = root
        .join("tests/fixtures/expected")
        .canonicalize()
        .context("resolve golden expected root")?;
    let candidate = root.join(path);
    for component in ["tests", "tests/fixtures", "tests/fixtures/expected"] {
        if root.join(component).is_symlink() {
            bail!("golden target must not use symlinks: {relative}");
        }
    }
    let parent = candidate
        .parent()
        .and_then(|path| path.canonicalize().ok())
        .ok_or_else(|| anyhow::anyhow!("golden target parent is missing: {relative}"))?;
    if parent != expected_root {
        bail!("golden target escapes expected root: {relative}");
    }
    if candidate.is_symlink() || (candidate.exists() && !candidate.is_file()) {
        bail!("golden target is not a regular file: {relative}");
    }
    Ok(candidate)
}

fn read_target(path: &Path) -> Result<Option<Vec<u8>>> {
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(fs::read(path).with_context(|| {
        format!("read golden target {}", path.display())
    })?))
}

fn value_string(value: Option<&Value>, label: &str) -> Result<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("{label} must be a non-empty string"))
}

fn string_array(value: Option<&Value>, label: &str) -> Result<Vec<String>> {
    let values = value
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("{label} must be a string list"))?;
    let mut output = Vec::with_capacity(values.len());
    let mut seen = BTreeSet::new();
    for value in values {
        let item = value_string(Some(value), label)?;
        if !seen.insert(item.clone()) {
            bail!("{label} contains duplicate value {item}");
        }
        output.push(item);
    }
    if output.is_empty() {
        bail!("{label} must not be empty");
    }
    Ok(output)
}

fn emit_report(root: &Path, report_path: Option<&PathBuf>, report: &Value) -> Result<()> {
    let mut bytes =
        serde_json::to_vec_pretty(report).context("serialize golden manifest report")?;
    bytes.push(b'\n');
    if let Some(path) = report_path {
        common::write_bytes(&resolve(root, path), &bytes)?;
    } else {
        print!("{}", String::from_utf8_lossy(&bytes));
    }
    Ok(())
}

fn resolve(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}
