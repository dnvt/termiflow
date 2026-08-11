use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const AUDIT_SCHEMA: &str = "termiflow.visual_audit.row.v3";
pub const LEGACY_AUDIT_SCHEMA: &str = "termiflow.visual_audit.row.v2";
pub const IDENTITY_REF_SCHEMA: &str = "termiflow.visual_audit.identity_ref.v1";
pub const SUMMARY_SCHEMA: &str = "termiflow.visual_audit.summary.v2";
pub const METADATA_SCHEMA: &str = "termiflow.fixture_metadata.v1";
pub const EVIDENCE_SCHEMA: &str = "termiflow.render_evidence.v1";
pub const STYLES: &[&str] = &[
    "ascii", "unicode", "double", "rounded", "heavy", "dots", "plus", "stars", "blocks",
];
pub const MODES: &[&str] = &["default", "optimized"];
pub const KINDS: &[&str] = &["success", "warning", "expected_error"];
pub const STDERR_POLICIES: &[&str] = &["empty", "warning", "error"];
pub const DIRECTIONS: &[&str] = &["TD", "LR", "RL", "BT", "none"];

static LABEL_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn is_audit_schema(value: &Value) -> bool {
    matches!(
        value.as_str(),
        Some(AUDIT_SCHEMA) | Some(LEGACY_AUDIT_SCHEMA)
    )
}

pub fn identity_ref(identity: &Value, identity_sha256: &str) -> Result<Value> {
    let source_commit = identity
        .get("source_commit")
        .cloned()
        .ok_or_else(|| anyhow!("identity.source_commit is required for row identity"))?;
    let run_spec_id = identity
        .get("run_spec_id")
        .cloned()
        .ok_or_else(|| anyhow!("identity.run_spec_id is required for row identity"))?;
    let run_identity = identity
        .get("run_identity")
        .cloned()
        .ok_or_else(|| anyhow!("identity.run_identity is required for row identity"))?;
    let effective_sha256 = identity
        .get("provenance")
        .and_then(|value| value.get("effective_sha256"))
        .cloned()
        .ok_or_else(|| {
            anyhow!("identity.provenance.effective_sha256 is required for row identity")
        })?;
    Ok(json!({
        "schema": IDENTITY_REF_SCHEMA,
        "path": "identity.json",
        "sha256": identity_sha256,
        "source_commit": source_commit,
        "run_spec_id": run_spec_id,
        "run_identity": run_identity,
        "provenance_effective_sha256": effective_sha256,
    }))
}

pub fn validate_identity_ref(
    reference: Option<&Value>,
    identity: &Value,
    identity_sha256: &str,
    label: &str,
) -> Result<()> {
    let reference = reference.ok_or_else(|| anyhow!("{label} is missing"))?;
    let expected = identity_ref(identity, identity_sha256)?;
    if reference != &expected {
        bail!("{label} does not match identity.json");
    }
    Ok(())
}

pub fn validate_row_identity(
    row: &Value,
    identity: &Value,
    identity_sha256: &str,
    label: &str,
) -> Result<()> {
    if row.get("identity_ref").is_some() {
        return validate_identity_ref(row.get("identity_ref"), identity, identity_sha256, label);
    }
    if row.get("identity") == Some(identity) {
        return Ok(());
    }
    bail!("{label} does not match identity.json");
}

#[derive(Debug, Clone)]
pub struct FixtureMetadata {
    pub kind: String,
    pub direction: String,
    pub stderr_policy: String,
    pub stderr_contains: Vec<String>,
    pub expected_stderr: Option<String>,
}

#[derive(Debug)]
pub struct ProcessResult {
    pub status: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub fn parse_csv(value: &str, allowed: &[&str], label: &str) -> Result<Vec<String>> {
    let values: Vec<String> = value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    if values.is_empty() || values.iter().collect::<BTreeSet<_>>().len() != values.len() {
        bail!("{label} must contain unique non-empty values");
    }
    let unsupported: Vec<&str> = values
        .iter()
        .map(String::as_str)
        .filter(|item| !allowed.contains(item))
        .collect();
    if !unsupported.is_empty() {
        bail!("unsupported {label}: {}", unsupported.join(", "));
    }
    Ok(values)
}

pub fn sha256_bytes(value: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(value);
    encode_hex(digest.finalize())
}

fn encode_hex(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut rendered = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        rendered.push(HEX[(byte >> 4) as usize] as char);
        rendered.push(HEX[(byte & 0x0f) as usize] as char);
    }
    rendered
}

pub fn sha256_file(path: &Path) -> Result<String> {
    Ok(sha256_bytes(
        &fs::read(path).with_context(|| format!("read {}", path.display()))?,
    ))
}

pub fn write_bytes(path: &Path, content: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(path, content).with_context(|| format!("write {}", path.display()))
}

pub fn write_json(path: &Path, value: &Value) -> Result<()> {
    let mut rendered = serde_json::to_vec_pretty(value).context("serialize JSON")?;
    rendered.push(b'\n');
    write_bytes(path, &rendered)
}

pub fn load_json(path: &Path, label: &str) -> Result<Value> {
    let bytes = require_file(path, label)?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid {label} JSON: {}", path.display()))
}

pub fn require_file(path: &Path, label: &str) -> Result<Vec<u8>> {
    if path.is_symlink() || !path.is_file() {
        bail!("{label} is not a regular file: {}", path.display());
    }
    fs::read(path).with_context(|| format!("read {label} {}", path.display()))
}

pub fn safe_relative_path(path: &Path, root: &Path, label: &str) -> Result<PathBuf> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        bail!("{label} must be a non-empty relative path");
    }
    let mut current = root.to_path_buf();
    for component in path.components() {
        let std::path::Component::Normal(part) = component else {
            bail!("{label} contains an unsafe path: {}", path.display());
        };
        current.push(part);
        if current.is_symlink() {
            bail!("{label} must not use symlinks: {}", path.display());
        }
    }
    let resolved_root = root
        .canonicalize()
        .with_context(|| format!("resolve packet root {}", root.display()))?;
    let resolved = current
        .canonicalize()
        .with_context(|| format!("resolve {label} {}", path.display()))?;
    if resolved != resolved_root && !resolved.starts_with(&resolved_root) {
        bail!("{label} escapes its root: {}", path.display());
    }
    if !resolved.is_file() || resolved.is_symlink() {
        bail!("{label} is not a regular file: {}", path.display());
    }
    Ok(resolved)
}

pub fn validate_blob_ref(packet: &Path, reference: &Value, label: &str) -> Result<Vec<u8>> {
    let object = reference
        .as_object()
        .ok_or_else(|| anyhow!("{label} must be an object"))?;
    let path = object
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("{label}.path must be a non-empty string"))?;
    let resolved = safe_relative_path(Path::new(path), packet, &format!("{label}.path"))?;
    let content = fs::read(&resolved)?;
    let bytes = object
        .get("bytes")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("{label}.bytes must be a non-negative integer"))?;
    if bytes != content.len() as u64 {
        bail!(
            "{label} byte count mismatch: manifest={bytes} actual={}",
            content.len()
        );
    }
    let expected_hash = object
        .get("sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("{label}.sha256 must be a string"))?;
    let actual_hash = sha256_bytes(&content);
    if expected_hash != actual_hash {
        bail!("{label} sha256 mismatch: {}", resolved.display());
    }
    Ok(content)
}

pub fn repository_file(root: &Path, relative: &Value, label: &str) -> Result<PathBuf> {
    let path = relative
        .as_str()
        .ok_or_else(|| anyhow!("{label} must be a non-empty relative path"))?;
    safe_relative_path(Path::new(path), root, label)
}

pub fn git_is_ancestor(root: &Path, ancestor: &str, descendant: &str) -> bool {
    Command::new("git")
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .current_dir(root)
        .status()
        .is_ok_and(|status| status.success())
}

pub fn run_text(command: &[&str], cwd: &Path) -> String {
    let Some((program, args)) = command.split_first() else {
        return String::new();
    };
    Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_default()
}

pub fn source_identity(root: &Path, binary: &Path, display_profile: &str) -> Result<Value> {
    let rustc_verbose = run_text(&["rustc", "-Vv"], root);
    let mut rustc = BTreeMap::new();
    for line in rustc_verbose.lines() {
        if let Some((key, value)) = line.split_once(':') {
            rustc.insert(
                key.trim().to_owned(),
                Value::String(value.trim().to_owned()),
            );
        }
    }
    let binary_label = binary
        .canonicalize()
        .ok()
        .and_then(|path| {
            root.canonicalize()
                .ok()
                .and_then(|root| path.strip_prefix(root).ok().map(PathBuf::from))
        })
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| binary.to_string_lossy().to_string());
    let lockfile = root.join("Cargo.lock");
    let terminal = ["TERM", "LANG", "LC_ALL", "COLUMNS", "LINES"]
        .into_iter()
        .map(|key| {
            (
                key.to_owned(),
                Value::String(std::env::var(key).unwrap_or_default()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let target = rustc_verbose
        .lines()
        .find_map(|line| line.strip_prefix("host:").map(str::trim))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS));
    Ok(json!({
        "source_commit": run_text(&["git", "rev-parse", "HEAD"], root),
        "worktree_dirty": !run_text(&["git", "status", "--porcelain"], root).is_empty(),
        "rustc": rustc,
        "target": target,
        "binary": binary_label,
        "cargo_lock_sha256": lockfile.is_file().then(|| sha256_file(&lockfile)).transpose()?,
        "display_profile": display_profile,
        "terminal": terminal,
    }))
}

pub fn discover_binary(root: &Path, stage: &Path, supplied: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = supplied {
        let binary = if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        };
        if !binary.is_file() {
            bail!("binary is not a regular file: {}", binary.display());
        }
        return Ok(binary);
    }

    let output = Command::new("cargo")
        .args(["build", "--bin", "termiflow", "--message-format=json"])
        .current_dir(root)
        .output()
        .context("run cargo build for visual audit")?;
    write_bytes(&stage.join("logs/cargo-build.stdout.jsonl"), &output.stdout)?;
    write_bytes(&stage.join("logs/cargo-build.stderr.log"), &output.stderr)?;
    if !output.status.success() {
        bail!("cargo build failed with status {}", output.status);
    }
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let target = event.get("target").and_then(Value::as_object);
        let is_binary = target
            .and_then(|target| target.get("name"))
            .and_then(Value::as_str)
            .is_some_and(|name| name == "termiflow")
            && target
                .and_then(|target| target.get("kind"))
                .and_then(Value::as_array)
                .is_some_and(|kinds| kinds.iter().any(|kind| kind.as_str() == Some("bin")));
        if event.get("reason").and_then(Value::as_str) == Some("compiler-artifact") && is_binary {
            if let Some(path) = event.get("executable").and_then(Value::as_str) {
                let binary = PathBuf::from(path);
                if binary.is_file() {
                    return Ok(binary);
                }
            }
        }
    }
    bail!("Cargo completed without exposing an executable termiflow binary")
}

pub fn load_metadata(
    path: &Path,
    input_root: &Path,
) -> Result<(BTreeMap<String, FixtureMetadata>, Vec<u8>)> {
    let raw =
        fs::read(path).with_context(|| format!("read fixture metadata {}", path.display()))?;
    let document: Value = serde_json::from_slice(&raw).context("parse fixture metadata JSON")?;
    if document.get("schema").and_then(Value::as_str) != Some(METADATA_SCHEMA) {
        bail!("metadata schema must be {METADATA_SCHEMA}");
    }
    let records = document
        .get("fixtures")
        .and_then(Value::as_array)
        .filter(|records| !records.is_empty())
        .ok_or_else(|| anyhow!("metadata fixtures must be a non-empty list"))?;

    let mut metadata = BTreeMap::new();
    for record in records {
        let object = record
            .as_object()
            .ok_or_else(|| anyhow!("each fixture metadata record must be an object"))?;
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| {
                !name.is_empty()
                    && Path::new(name).file_name().and_then(|v| v.to_str()) == Some(*name)
            })
            .ok_or_else(|| anyhow!("invalid fixture metadata name"))?
            .to_owned();
        if metadata.contains_key(&name) {
            bail!("duplicate fixture metadata: {name}");
        }
        let kind = required_string(object, "kind", &name)?;
        let direction = required_string(object, "direction", &name)?;
        let stderr_policy = required_string(object, "stderr_policy", &name)?;
        if !KINDS.contains(&kind.as_str()) || !DIRECTIONS.contains(&direction.as_str()) {
            bail!("{name}: invalid fixture kind or direction");
        }
        if !STDERR_POLICIES.contains(&stderr_policy.as_str()) {
            bail!("{name}: invalid stderr policy");
        }
        let stderr_contains = object
            .get("stderr_contains")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("{name}: stderr_contains must be a string list"))?
            .iter()
            .map(|value| value.as_str().map(ToOwned::to_owned))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| anyhow!("{name}: stderr_contains must be a string list"))?;
        let expected_policy = match kind.as_str() {
            "success" => "empty",
            "warning" => "warning",
            "expected_error" => "error",
            _ => unreachable!(),
        };
        if stderr_policy != expected_policy {
            bail!("{name}: kind {kind} requires stderr policy {expected_policy}");
        }
        if direction == "none" && !name.starts_with("error_") {
            bail!("{name}: non-error fixture must declare a direction");
        }
        metadata.insert(
            name.clone(),
            FixtureMetadata {
                kind,
                direction,
                stderr_policy,
                stderr_contains,
                expected_stderr: object
                    .get("expected_stderr")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
            },
        );
    }

    let inputs = collect_inputs(input_root)?;
    let missing: Vec<_> = inputs
        .keys()
        .filter(|name| !metadata.contains_key(*name))
        .cloned()
        .collect();
    let unexpected: Vec<_> = metadata
        .keys()
        .filter(|name| !inputs.contains_key(*name))
        .cloned()
        .collect();
    if !missing.is_empty() || !unexpected.is_empty() {
        bail!(
            "fixture metadata/input mismatch; missing metadata: {missing:?}; metadata without input: {unexpected:?}"
        );
    }
    Ok((metadata, raw))
}

fn required_string(
    object: &serde_json::Map<String, Value>,
    key: &str,
    name: &str,
) -> Result<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("{name}: {key} must be a string"))
}

pub fn collect_inputs(root: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let mut paths = Vec::new();
    collect_files(root, &mut paths)?;
    let mut inputs = BTreeMap::new();
    for path in paths
        .into_iter()
        .filter(|path| path.extension().and_then(|v| v.to_str()) == Some("md"))
    {
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| anyhow!("input fixture has invalid UTF-8 name: {}", path.display()))?
            .to_owned();
        if inputs.insert(stem.clone(), path).is_some() {
            bail!("duplicate input fixture stem: {stem}");
        }
    }
    Ok(inputs)
}

fn collect_files(root: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    for entry in
        fs::read_dir(root).with_context(|| format!("read input root {}", root.display()))?
    {
        let path = entry?.path();
        if path.is_dir() {
            collect_files(&path, output)?;
        } else if path.is_file() {
            output.push(path);
        }
    }
    output.sort();
    Ok(())
}

pub fn case_id(input: &[u8], fixture: &str, style: &str, mode: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(input);
    digest.update([0]);
    digest.update(fixture.as_bytes());
    digest.update([0]);
    digest.update(style.as_bytes());
    digest.update([0]);
    digest.update(mode.as_bytes());
    encode_hex(digest.finalize())
}

pub fn dimensions(stdout: &[u8]) -> Value {
    let text = String::from_utf8_lossy(stdout);
    let lines: Vec<&str> = text.lines().collect();
    json!({ "stdout_rows": lines.len(), "stdout_max_codepoints": lines.iter().map(|line| line.chars().count()).max().unwrap_or(0), "stdout_bytes": stdout.len() })
}

pub fn expected_stderr(
    root: &Path,
    fixture: &str,
    style: &str,
    record: &FixtureMetadata,
) -> Option<Vec<u8>> {
    let path = record
        .expected_stderr
        .as_deref()
        .map(|path| root.join(path))
        .unwrap_or_else(|| {
            root.join("tests/fixtures/expected")
                .join(format!("{fixture}.{style}.txt"))
        });
    path.is_file().then(|| fs::read(path).ok()).flatten()
}

pub fn validate_streams(
    root: &Path,
    fixture: &str,
    style: &str,
    record: &FixtureMetadata,
    status: i32,
    stdout: &[u8],
    stderr: &[u8],
) -> Vec<String> {
    let mut failures = Vec::new();
    let stderr_text = String::from_utf8_lossy(stderr);
    match record.kind.as_str() {
        "success" => {
            if status != 0 {
                failures.push(format!("expected success, got status {status}"));
            }
            if stdout.is_empty() {
                failures.push("successful fixture produced empty stdout".to_owned());
            }
            if !stderr.is_empty() {
                failures.push("successful fixture wrote unexpected stderr".to_owned());
            }
        }
        "warning" => {
            if status != 0 {
                failures.push(format!("expected warning success, got status {status}"));
            }
            if stdout.is_empty() {
                failures.push("warning fixture produced empty stdout".to_owned());
            }
            for pattern in &record.stderr_contains {
                if !stderr_text.contains(pattern) {
                    failures.push(format!(
                        "stderr is missing expected warning text {pattern:?}"
                    ));
                }
            }
        }
        "expected_error" => {
            if status == 0 {
                failures.push("expected renderer error, got status 0".to_owned());
            }
            if !stdout.is_empty() {
                failures.push("expected-error fixture wrote stdout".to_owned());
            }
            match expected_stderr(root, fixture, style, record) {
                None => {
                    failures.push("expected-error fixture has no expected stderr file".to_owned())
                }
                Some(expected) if trim_newlines(stderr) != trim_newlines(&expected) => {
                    failures.push("stderr does not match expected error fixture".to_owned())
                }
                Some(_) => {}
            }
        }
        _ => failures.push(format!("unsupported fixture kind: {}", record.kind)),
    }
    failures
}

fn trim_newlines(value: &[u8]) -> &[u8] {
    let mut end = value.len();
    while end > 0 && matches!(value[end - 1], b'\r' | b'\n') {
        end -= 1;
    }
    &value[..end]
}

pub fn relative_to_root(path: &Path, root: &Path) -> String {
    path.canonicalize()
        .ok()
        .and_then(|path| {
            root.canonicalize()
                .ok()
                .and_then(|root| path.strip_prefix(root).ok().map(PathBuf::from))
        })
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default()
        })
}

pub fn process(command: &[String], cwd: &Path, timeout: Duration) -> ProcessResult {
    let Some((program, args)) = command.split_first() else {
        return ProcessResult {
            status: -127,
            stdout: Vec::new(),
            stderr: b"empty command\n".to_vec(),
        };
    };
    let started = Instant::now();
    let spawned = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut child) = spawned else {
        return ProcessResult {
            status: -127,
            stdout: Vec::new(),
            stderr: b"failed to start process\n".to_vec(),
        };
    };
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() >= timeout => {
                timed_out = true;
                let _ = child.kill();
                break;
            }
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(_) => break,
        }
    }
    let output = child.wait_with_output().unwrap_or_else(|_| Output {
        status: exit_status(-127),
        stdout: Vec::new(),
        stderr: b"failed to collect process output\n".to_vec(),
    });
    let status = if timed_out {
        -124
    } else {
        output.status.code().unwrap_or(-1)
    };
    let mut stderr = output.stderr;
    if timed_out {
        stderr.extend_from_slice(b"\ntermiflow visual audit: process timeout\n");
    }
    ProcessResult {
        status,
        stdout: output.stdout,
        stderr,
    }
}

#[cfg(unix)]
fn exit_status(code: i32) -> std::process::ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    std::process::ExitStatus::from_raw(code << 8)
}

#[cfg(not(unix))]
fn exit_status(_code: i32) -> std::process::ExitStatus {
    Command::new("cmd")
        .status()
        .expect("spawn command status fallback")
}

pub fn deterministic_digest(stage: &Path) -> Result<(String, String)> {
    let mut files = Vec::new();
    collect_files(stage, &mut files)?;
    let ignored = [
        "timings.jsonl",
        "COMPLETE.json",
        "PACKET.sha256",
        "run_state.json",
    ];
    let mut digest = Sha256::new();
    let mut listing = String::new();
    for path in files {
        let relative = path
            .strip_prefix(stage)
            .map_err(|_| anyhow!("packet path escaped stage"))?
            .to_string_lossy()
            .replace('\\', "/");
        if ignored.contains(&relative.as_str()) {
            continue;
        }
        let content =
            fs::read(&path).with_context(|| format!("read packet file {}", path.display()))?;
        let content_hash = sha256_bytes(&content);
        listing.push_str(&format!("{content_hash}  {relative}\n"));
        digest.update(relative.as_bytes());
        digest.update([0]);
        digest.update(&content);
        digest.update([0]);
    }
    Ok((encode_hex(digest.finalize()), listing))
}

pub fn now_label() -> String {
    let sequence = LABEL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| {
            format!(
                "{}-{}-{}-{sequence}",
                duration.as_secs(),
                duration.subsec_nanos(),
                std::process::id()
            )
        })
        .unwrap_or_else(|_| format!("0-{}-{sequence}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "termiflow-qa-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test directory");
        path
    }

    #[test]
    fn csv_rejects_duplicate_or_unknown_values() {
        assert!(parse_csv("ascii,ascii", &["ascii"], "styles").is_err());
        assert!(parse_csv("ruby", &["ascii"], "styles").is_err());
    }

    #[test]
    fn blob_validation_rejects_tampering() {
        let root = test_dir("blob");
        let path = root.join("frame.txt");
        fs::write(&path, b"stable\n").expect("write fixture");
        let reference =
            json!({ "path": "frame.txt", "bytes": 7, "sha256": sha256_bytes(b"stable\n") });
        assert_eq!(
            validate_blob_ref(&root, &reference, "frame").expect("valid blob"),
            b"stable\n"
        );
        fs::write(&path, b"changed\n").expect("tamper fixture");
        assert!(validate_blob_ref(&root, &reference, "frame").is_err());
        fs::remove_dir_all(root).expect("remove test directory");
    }

    #[test]
    fn relative_paths_reject_parent_components() {
        let root = test_dir("paths");
        assert!(safe_relative_path(Path::new("../outside"), &root, "frame").is_err());
        fs::remove_dir_all(root).expect("remove test directory");
    }

    #[test]
    fn now_labels_are_unique_for_parallel_stages() {
        let first = now_label();
        let second = now_label();
        assert_ne!(first, second);
    }

    #[test]
    fn identity_reference_is_compact_and_hash_bound() {
        let identity = json!({
            "source_commit": "commit",
            "run_spec_id": "spec",
            "run_identity": {"run_id": "run", "policy_sha256": "policy"},
            "provenance": {"effective_sha256": "effective"}
        });
        let reference = identity_ref(&identity, &"a".repeat(64)).expect("identity reference");
        assert_eq!(reference["schema"], IDENTITY_REF_SCHEMA);
        assert!(reference.to_string().len() < 1024);
        validate_identity_ref(Some(&reference), &identity, &"a".repeat(64), "row identity")
            .expect("valid identity reference");

        let mut tampered = reference.clone();
        tampered["sha256"] = Value::String("b".repeat(64));
        assert!(
            validate_identity_ref(Some(&tampered), &identity, &"a".repeat(64), "row identity")
                .is_err()
        );
    }
}
