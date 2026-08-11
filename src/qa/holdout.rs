use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use super::{audit, common, persist, spec};

const RECEIPT_SCHEMA: &str = "termiflow.holdout_receipt.v1";

#[derive(Debug)]
pub struct HoldoutArgs {
    pub spec: PathBuf,
    pub queue: String,
    pub out: PathBuf,
    pub receipt: PathBuf,
    pub binary: Option<PathBuf>,
    pub display_profile: String,
    pub timeout_seconds: f64,
}

pub fn run(args: HoldoutArgs) -> Result<()> {
    let root = std::env::current_dir().context("resolve repository root")?;
    let spec_path = resolve(&root, &args.spec);
    let (manifest, manifest_bytes) = spec::load_manifest(&root, &spec_path, &args.queue)?;
    let holdouts = manifest
        .get("holdouts")
        .and_then(Value::as_array)
        .filter(|rows| !rows.is_empty())
        .ok_or_else(|| anyhow!("selected queue has no holdout rows"))?;
    let queue_id = manifest
        .get("queue_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("holdout manifest queue_id is missing"))?;
    let queue_sha256 = manifest
        .get("queue_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("holdout manifest queue_sha256 is missing"))?;
    let out = resolve(&root, &args.out);
    let receipt_path = resolve(&root, &args.receipt);
    if let Some(parent) = receipt_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }

    if out.exists() {
        let outcome = reconcile_published_packet_receipt(
            &root,
            &out,
            &receipt_path,
            &manifest,
            &manifest_bytes,
            holdouts,
        )?;
        println!(
            "holdout receipt reconciled ({outcome:?}): {} ({} rows); receipt {}",
            out.display(),
            holdouts.len(),
            receipt_path.display()
        );
        return Ok(());
    }
    if receipt_path.exists() {
        return Err(persist::PersistenceError::RecoveryRequired {
            path: receipt_path,
            detail:
                "receipt exists while its final packet is absent; preserve both for manual recovery"
                    .to_owned(),
        }
        .into());
    }

    let temporary_manifest = std::env::temp_dir().join(format!(
        "termiflow-holdout-manifest-{}-{}.json",
        std::process::id(),
        common::now_label()
    ));
    if temporary_manifest.exists() {
        bail!("temporary holdout manifest already exists");
    }
    common::write_bytes(&temporary_manifest, &manifest_bytes)?;
    let packet = audit::run_schema_packet(
        &root,
        &temporary_manifest,
        &out,
        audit::SchemaPacketOptions {
            holdouts: true,
            supplied_binary: args.binary.as_deref(),
            display_profile: &args.display_profile,
            timeout_seconds: args.timeout_seconds,
            respect_input_style: false,
        },
    );
    let _ = fs::remove_file(&temporary_manifest);
    let packet = packet?;
    let packet = audit::load_published_schema_packet(&packet.out, queue_id, queue_sha256)
        .context("validate published holdout packet before receipt claim")?;
    let receipt = build_receipt(&root, &manifest, &manifest_bytes, &packet, holdouts)?;
    let outcome = persist::publish_json(&receipt_path, &receipt)?;
    if receipt["status"] != "passed" {
        bail!(
            "holdout execution produced failed rows; receipt {}",
            receipt_path.display()
        );
    }
    println!(
        "holdout execution passed: {} ({} rows); receipt {}",
        packet.out.display(),
        packet.row_count,
        receipt_path.display()
    );
    if outcome == persist::PublishOutcome::EqualReplay {
        eprintln!("holdout receipt was an equal replay");
    }
    Ok(())
}

/// Reconcile the one-sided crash boundary between a published packet and its
/// holdout receipt. This operation never reruns the renderer or republishes a
/// final directory.
pub(crate) fn reconcile_published_packet_receipt(
    root: &Path,
    out: &Path,
    receipt_path: &Path,
    manifest: &Value,
    manifest_bytes: &[u8],
    holdouts: &[Value],
) -> Result<persist::PublishOutcome> {
    let queue_id = manifest
        .get("queue_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("holdout manifest queue_id is missing"))?;
    let queue_sha256 = manifest
        .get("queue_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("holdout manifest queue_sha256 is missing"))?;
    let packet = audit::load_published_schema_packet(out, queue_id, queue_sha256)?;
    let receipt = build_receipt(root, manifest, manifest_bytes, &packet, holdouts)?;
    if receipt_path.is_file() {
        let existing = fs::read(receipt_path)
            .with_context(|| format!("read existing holdout receipt {}", receipt_path.display()))?;
        let existing_value: Value = serde_json::from_slice(&existing).map_err(|error| {
            persist::PersistenceError::RecoveryRequired {
                path: receipt_path.to_path_buf(),
                detail: format!("existing receipt is malformed: {error}"),
            }
        })?;
        if existing_value != receipt {
            return Err(persist::PersistenceError::Conflict {
                path: receipt_path.to_path_buf(),
                detail: "existing receipt identity differs from the complete published packet"
                    .to_owned(),
            }
            .into());
        }
    }
    persist::publish_json(receipt_path, &receipt)
}

fn build_receipt(
    root: &Path,
    manifest: &Value,
    manifest_bytes: &[u8],
    packet: &audit::SchemaPacketResult,
    holdouts: &[Value],
) -> Result<Value> {
    let packet_rows = load_packet_rows(&packet.out)?;
    persist::validate_run_identity(&packet.run_identity)?;
    if packet_rows.len() != holdouts.len() {
        bail!(
            "holdout packet row count mismatch: expected {}, got {}",
            holdouts.len(),
            packet_rows.len()
        );
    }
    let expected: BTreeMap<String, &Value> = holdouts
        .iter()
        .map(|row| {
            let fixture = format!(
                "{}--{}",
                required_string(row, "case_id")?,
                required_string(row, "variant_id")?
            );
            let key = format!(
                "{fixture}\u{1f}{}\u{1f}{}",
                required_string(row, "style")?,
                required_string(row, "mode")?
            );
            Ok((key, row))
        })
        .collect::<Result<_>>()?;
    let mut seen = BTreeMap::new();
    let mut rows = Vec::new();
    let mut all_passed = true;
    for packet_row in packet_rows {
        let fixture = required_string(&packet_row, "fixture")?;
        let style = required_string(&packet_row, "style")?;
        let mode = required_string(&packet_row, "mode")?;
        let key = format!("{fixture}\u{1f}{style}\u{1f}{mode}");
        let expected_row = expected
            .get(&key)
            .ok_or_else(|| anyhow!("holdout packet row is outside the queue: {key}"))?;
        if seen.insert(key.clone(), ()).is_some() {
            bail!("duplicate holdout packet row: {key}");
        }
        let evidence = packet_row.get("evidence").cloned().unwrap_or(Value::Null);
        let evidence_document = evidence
            .get("path")
            .and_then(Value::as_str)
            .map(|path| packet.out.join(path))
            .filter(|path| path.is_file())
            .and_then(|path| fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .unwrap_or(Value::Null);
        let geometry_errors = evidence_document["geometry"]["errors"]
            .as_array()
            .map_or(0, Vec::len);
        let passed = packet_row["status"].as_i64() == Some(0)
            && packet_row["stdout"]["bytes"]
                .as_u64()
                .is_some_and(|bytes| bytes > 0)
            && packet_row["stderr"]["bytes"].as_u64() == Some(0)
            && geometry_errors == 0;
        all_passed &= passed;
        rows.push(json!({
            "case_id": packet_row["case_id"],
            "fixture": fixture,
            "variant_id": expected_row["variant_id"],
            "style": style,
            "mode": mode,
            "direction": expected_row["direction"],
            "source_sha256": expected_row["source_sha256"],
            "input_path": expected_row["input_path"],
            "semantic": expected_row["semantic"],
            "review_targets": expected_row["review_targets"],
            "status": if passed { "passed" } else { "failed" },
            "frame": packet_row["stdout"],
            "stderr": packet_row["stderr"],
            "evidence": evidence,
            "findings": packet_row["findings"],
        }));
    }
    if seen.len() != expected.len() {
        bail!(
            "holdout packet coverage mismatch: expected {}, got {}",
            expected.len(),
            seen.len()
        );
    }
    rows.sort_by_key(|row| {
        format!(
            "{}\u{1f}{}\u{1f}{}",
            row["fixture"].as_str().unwrap_or_default(),
            row["style"].as_str().unwrap_or_default(),
            row["mode"].as_str().unwrap_or_default()
        )
    });
    Ok(json!({
        "schema": RECEIPT_SCHEMA,
        "state": "published",
        "run_identity": packet.run_identity,
        "queue_id": packet.queue_id,
        "queue_sha256": packet.queue_sha256,
        "manifest_sha256": common::sha256_bytes(manifest_bytes),
        "spec_sha256": manifest["spec_sha256"],
        "packet": {
            "path": packet_path(root, &packet.out),
            "manifest_sha256": packet.manifest_sha256,
            "identity_sha256": packet.identity_sha256,
            "packet_sha256": packet.packet_sha256,
            "complete_sha256": packet.complete_sha256,
            "deterministic_packet_sha256": packet.deterministic_packet_sha256,
            "run_id": packet.run_identity["run_id"],
            "policy_sha256": packet.run_identity["policy_sha256"],
        },
        "expected_rows": expected.len(),
        "actual_rows": rows.len(),
        "status": if all_passed { "passed" } else { "failed" },
        "rows": rows,
    }))
}

fn packet_path(root: &Path, packet: &Path) -> String {
    if packet
        .canonicalize()
        .is_ok_and(|path| root.canonicalize().is_ok_and(|root| path.starts_with(root)))
    {
        common::relative_to_root(packet, root)
    } else {
        packet.to_string_lossy().replace('\\', "/")
    }
}

fn load_packet_rows(packet: &Path) -> Result<Vec<Value>> {
    let bytes = common::require_file(&packet.join("manifest.jsonl"), "holdout packet manifest")?;
    let text = String::from_utf8(bytes).context("holdout packet manifest is not UTF-8")?;
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).context("parse holdout packet row"))
        .collect()
}

fn required_string(value: &Value, key: &str) -> Result<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("holdout row {key} must be a non-empty string"))
}

fn resolve(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}
