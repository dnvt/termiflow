use std::collections::BTreeSet;
use std::error::Error;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::ffi::CString;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Map, Value};

static NAME_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) const RUN_SPEC_SCHEMA: &str = "termiflow.run_spec.v1";
pub(crate) const RUN_STATE_SCHEMA: &str = "termiflow.run_state.v2";
pub(crate) const RUN_IDENTITY_SCHEMA: &str = "termiflow.run_identity.v2";
const LEGACY_RUN_STATE_SCHEMA: &str = "termiflow.run_state.v1";
const LEGACY_RUN_IDENTITY_SCHEMA: &str = "termiflow.run_identity.v1";
#[allow(dead_code)]
pub(crate) const PERSISTENCE_CAPABILITY_TARGETS: &[&str] = &["linux-gnu", "macos-apple", "other"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PersistenceError {
    Conflict { path: PathBuf, detail: String },
    Unsupported { path: PathBuf, detail: String },
    Incomplete { path: PathBuf, detail: String },
    RecoveryRequired { path: PathBuf, detail: String },
    Cleanup { path: PathBuf, detail: String },
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Conflict { path, detail } => {
                write!(
                    formatter,
                    "persistence conflict at {}: {detail}",
                    path.display()
                )
            }
            Self::Unsupported { path, detail } => {
                write!(
                    formatter,
                    "unsupported persistence capability at {}: {detail}",
                    path.display()
                )
            }
            Self::Incomplete { path, detail } => {
                write!(
                    formatter,
                    "incomplete persistence artifact at {}: {detail}",
                    path.display()
                )
            }
            Self::RecoveryRequired { path, detail } => {
                write!(
                    formatter,
                    "persistence recovery required at {}: {detail}",
                    path.display()
                )
            }
            Self::Cleanup { path, detail } => {
                write!(
                    formatter,
                    "persistence cleanup failed at {}: {detail}",
                    path.display()
                )
            }
        }
    }
}

impl Error for PersistenceError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublishOutcome {
    Published,
    EqualReplay,
}

pub(crate) trait PersistenceOps {
    fn write_new(&self, path: &Path, content: &[u8]) -> io::Result<()>;
    fn claim_file(&self, staged: &Path, target: &Path) -> io::Result<()>;
    fn claim_dir(&self, target: &Path) -> io::Result<()>;
    fn claim_dir_no_replace(&self, staged: &Path, target: &Path) -> io::Result<()>;
    fn replace(&self, staged: &Path, target: &Path) -> io::Result<()>;
    fn append(&self, path: &Path, content: &[u8]) -> io::Result<()>;
    fn sync_path(&self, path: &Path) -> io::Result<()>;
    fn sync_tree(&self, path: &Path) -> io::Result<()> {
        self.sync_path(path)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SystemOps;

impl PersistenceOps for SystemOps {
    fn write_new(&self, path: &Path, content: &[u8]) -> io::Result<()> {
        let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
        file.write_all(content)?;
        file.sync_all()
    }

    fn claim_file(&self, staged: &Path, target: &Path) -> io::Result<()> {
        fs::hard_link(staged, target)
    }

    fn claim_dir(&self, target: &Path) -> io::Result<()> {
        fs::create_dir(target)
    }

    fn claim_dir_no_replace(&self, staged: &Path, target: &Path) -> io::Result<()> {
        atomic_directory_claim(staged, target)
    }

    fn replace(&self, staged: &Path, target: &Path) -> io::Result<()> {
        fs::rename(staged, target)
    }

    fn append(&self, path: &Path, content: &[u8]) -> io::Result<()> {
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        file.write_all(content)?;
        file.sync_all()
    }

    fn sync_path(&self, path: &Path) -> io::Result<()> {
        fs::File::open(path)?.sync_all()
    }

    fn sync_tree(&self, path: &Path) -> io::Result<()> {
        sync_tree(path)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }
}

#[cfg(target_os = "linux")]
const AT_FDCWD: std::os::raw::c_int = -100;
#[cfg(target_os = "linux")]
const RENAME_NOREPLACE: std::os::raw::c_uint = 1;
#[cfg(target_os = "macos")]
const RENAME_EXCL: std::os::raw::c_uint = 0x0000_0004;

#[cfg(target_os = "linux")]
extern "C" {
    fn renameat2(
        olddirfd: std::os::raw::c_int,
        oldpath: *const std::os::raw::c_char,
        newdirfd: std::os::raw::c_int,
        newpath: *const std::os::raw::c_char,
        flags: std::os::raw::c_uint,
    ) -> std::os::raw::c_int;
}

#[cfg(target_os = "macos")]
extern "C" {
    fn renamex_np(
        from: *const std::os::raw::c_char,
        to: *const std::os::raw::c_char,
        flags: std::os::raw::c_uint,
    ) -> std::os::raw::c_int;
}

/// Claim an absent final directory with the platform's explicit no-replace
/// primitive. There is intentionally no ordinary `rename` fallback here.
fn atomic_directory_claim(staged: &Path, target: &Path) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let staged = CString::new(staged.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "stage path contains NUL"))?;
        let target = CString::new(target.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "target path contains NUL"))?;
        let result = unsafe {
            renameat2(
                AT_FDCWD,
                staged.as_ptr(),
                AT_FDCWD,
                target.as_ptr(),
                RENAME_NOREPLACE,
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
    #[cfg(target_os = "macos")]
    {
        let staged = CString::new(staged.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "stage path contains NUL"))?;
        let target = CString::new(target.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "target path contains NUL"))?;
        let result = unsafe { renamex_np(staged.as_ptr(), target.as_ptr(), RENAME_EXCL) };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (staged, target);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "target has no proven atomic absent-final directory primitive",
        ))
    }
}

fn classify_atomic_directory_error(path: &Path, error: io::Error) -> anyhow::Error {
    if error.kind() == io::ErrorKind::AlreadyExists {
        return PersistenceError::Conflict {
            path: path.to_path_buf(),
            detail: error.to_string(),
        }
        .into();
    }
    let unsupported = match error.raw_os_error() {
        #[cfg(target_os = "linux")]
        Some(code) => matches!(code, 18 | 22 | 38 | 95), // EXDEV/EINVAL/ENOSYS/ENOTSUP
        #[cfg(target_os = "macos")]
        Some(code) => matches!(code, 18 | 22 | 45), // EXDEV/EINVAL/ENOTSUP
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        Some(_) => true,
        None => error.kind() == io::ErrorKind::Unsupported,
    };
    if unsupported
        || matches!(
            error.kind(),
            io::ErrorKind::Unsupported | io::ErrorKind::InvalidInput
        )
    {
        PersistenceError::Unsupported {
            path: path.to_path_buf(),
            detail: error.to_string(),
        }
        .into()
    } else {
        PersistenceError::Incomplete {
            path: path.to_path_buf(),
            detail: format!("atomic directory claim failed: {error}"),
        }
        .into()
    }
}

fn sequence() -> u64 {
    NAME_SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn run_spec_value(
    role: &str,
    source_sha256: &str,
    workload_sha256: &str,
    requested_final: &Path,
    display_profile: &str,
    requested_policy_context: &Value,
) -> Value {
    let unsigned = json!({
        "schema": RUN_SPEC_SCHEMA,
        "version": 1,
        "role": role,
        "source_sha256": source_sha256,
        "workload_sha256": workload_sha256,
        "requested_final": requested_final.to_string_lossy().replace('\\', "/"),
        "display_profile": display_profile,
        "requested_policy_context": requested_policy_context,
    });
    let run_spec_id = digest_json(&unsigned);
    let mut spec = unsigned;
    spec["run_spec_id"] = Value::String(run_spec_id);
    spec
}

pub(crate) fn run_spec_id(spec: &Value) -> Result<String> {
    validate_run_spec(spec)?;
    let mut unsigned = spec.clone();
    unsigned
        .as_object_mut()
        .ok_or_else(|| anyhow!("run_spec must be an object"))?
        .remove("run_spec_id");
    Ok(digest_json(&unsigned))
}

pub(crate) fn run_identity_value(
    run_spec_id: &str,
    role: &str,
    source_sha256: &str,
    workload_sha256: &str,
    policy_sha256: &str,
) -> Value {
    let unsigned = json!({
        "schema": RUN_IDENTITY_SCHEMA,
        "version": 2,
        "run_spec_id": run_spec_id,
        "role": role,
        "source_sha256": source_sha256,
        "workload_sha256": workload_sha256,
        "policy_sha256": policy_sha256,
    });
    let run_id = digest_json(&json!({
        "run_spec_id": run_spec_id,
        "policy_sha256": policy_sha256,
    }));
    let mut identity = unsigned;
    identity["run_id"] = Value::String(run_id);
    identity
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_state_value(
    run_spec_id: &str,
    run_identity: Option<&Value>,
    state: &str,
    requested_final: &Path,
    private_stage: &Path,
    candidate_packet_sha256: Option<&str>,
    created_at: &str,
    transition_reason: &str,
    final_claimed: bool,
    publication_guard: Option<&Path>,
) -> Value {
    let policy_pending = run_identity.is_none();
    let policy_sha256 = run_identity.and_then(|identity| identity.get("policy_sha256"));
    json!({
        "schema": RUN_STATE_SCHEMA,
        "version": 2,
        "run_spec_id": run_spec_id,
        "run_identity": run_identity,
        "policy_pending": policy_pending,
        "policy_sha256": policy_sha256,
        "state": state,
        "created_at": created_at,
        "last_transition_at": crate::qa::common::now_label(),
        "last_transition_reason": transition_reason,
        "owner": {
            "pid": std::process::id(),
            "host": host_identity(),
            "process_start": process_start_token(std::process::id()),
        },
        "requested_final": requested_final.to_string_lossy().replace('\\', "/"),
        "private_stage": private_stage.to_string_lossy().replace('\\', "/"),
        "candidate_packet_sha256": candidate_packet_sha256,
        "final_claimed": final_claimed,
        "publication_guard": publication_guard.map(|path| {
            json!({
                "path": path.to_string_lossy().replace('\\', "/"),
            })
        }),
    })
}

pub(crate) fn write_run_state(stage: &Path, state: &Value) -> Result<()> {
    let path = stage.join("run_state.json");
    let replace_existing = read_regular(&path)?.is_some();
    write_state_with_ops(&SystemOps, &path, state, replace_existing).map(|_| ())
}

fn write_state_with_ops<O: PersistenceOps>(
    ops: &O,
    path: &Path,
    state: &Value,
    replace_existing: bool,
) -> Result<PublishOutcome> {
    let mut bytes = serde_json::to_vec_pretty(state).context("serialize run state")?;
    bytes.push(b'\n');
    let outcome = if replace_existing {
        let current = read_regular(path)?.ok_or_else(|| PersistenceError::Incomplete {
            path: path.to_path_buf(),
            detail: "published state file is absent".to_owned(),
        })?;
        if current == bytes {
            PublishOutcome::EqualReplay
        } else {
            let staged = stage_bytes(ops, path, &bytes)?;
            let result = ops.replace(&staged, path).map_err(|error| {
                let _ = remove_file_if_present(ops, &staged);
                anyhow!("replace run state {}: {error}", path.display())
            });
            result?;
            remove_file_if_present(ops, &staged)?;
            PublishOutcome::Published
        }
    } else {
        publish_file_with_ops(ops, path, &bytes)?
    };
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("run state has no parent: {}", path.display()))?;
    ops.sync_path(path)
        .map_err(|error| PersistenceError::Unsupported {
            path: path.to_path_buf(),
            detail: format!("run state sync is unavailable: {error}"),
        })?;
    ops.sync_path(parent)
        .map_err(|error| PersistenceError::Unsupported {
            path: parent.to_path_buf(),
            detail: format!("run state parent sync is unavailable: {error}"),
        })?;
    Ok(outcome)
}

pub(crate) fn validate_run_spec(value: &Value) -> Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("run_spec must be an object"))?;
    exact_keys(
        object,
        &[
            "schema",
            "version",
            "role",
            "source_sha256",
            "workload_sha256",
            "requested_final",
            "display_profile",
            "requested_policy_context",
            "run_spec_id",
        ],
        "run_spec",
    )?;
    if object.get("schema").and_then(Value::as_str) != Some(RUN_SPEC_SCHEMA)
        || object.get("version").and_then(Value::as_u64) != Some(1)
    {
        bail!("run_spec schema/version is invalid");
    }
    for field in [
        "run_spec_id",
        "source_sha256",
        "workload_sha256",
        "role",
        "requested_final",
        "display_profile",
    ] {
        let value = object
            .get(field)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("run_spec.{field} must be a non-empty string"))?;
        if field.ends_with("sha256") || field == "run_spec_id" {
            validate_hash(value, &format!("run_spec.{field}"))?;
        }
    }
    let context = object
        .get("requested_policy_context")
        .ok_or_else(|| anyhow!("run_spec.requested_policy_context is missing"))?;
    if !context.is_object() {
        bail!("run_spec.requested_policy_context must be an object");
    }
    let mut unsigned = value.clone();
    unsigned
        .as_object_mut()
        .expect("run spec object was checked")
        .remove("run_spec_id");
    if object.get("run_spec_id").and_then(Value::as_str) != Some(digest_json(&unsigned).as_str()) {
        bail!("run_spec.run_spec_id does not match its fields");
    }
    Ok(())
}

pub(crate) fn validate_run_identity(value: &Value) -> Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("run_identity must be an object"))?;
    let schema = object.get("schema").and_then(Value::as_str);
    let version = object.get("version").and_then(Value::as_u64);
    if schema == Some(LEGACY_RUN_IDENTITY_SCHEMA) && version == Some(1) {
        return validate_legacy_run_identity(object);
    }
    if schema != Some(RUN_IDENTITY_SCHEMA) || version != Some(2) {
        bail!("run_identity schema/version is invalid");
    }
    exact_keys(
        object,
        &[
            "schema",
            "version",
            "run_spec_id",
            "role",
            "source_sha256",
            "workload_sha256",
            "policy_sha256",
            "run_id",
        ],
        "run_identity",
    )?;
    for field in [
        "run_id",
        "run_spec_id",
        "role",
        "source_sha256",
        "workload_sha256",
        "policy_sha256",
    ] {
        let value = object
            .get(field)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("run_identity.{field} must be a non-empty string"))?;
        if field != "role" {
            validate_hash(value, &format!("run_identity.{field}"))?;
        }
    }
    let expected = digest_json(&json!({
        "run_spec_id": object["run_spec_id"],
        "policy_sha256": object["policy_sha256"],
    }));
    if object.get("run_id").and_then(Value::as_str) != Some(expected.as_str()) {
        bail!("run_identity.run_id does not match run_spec_id and policy_sha256");
    }
    Ok(())
}

fn validate_legacy_run_identity(object: &Map<String, Value>) -> Result<()> {
    for field in [
        "run_id",
        "role",
        "source_sha256",
        "workload_sha256",
        "policy_sha256",
    ] {
        let value = object
            .get(field)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("legacy run_identity.{field} must be a non-empty string"))?;
        if field != "role" {
            validate_hash(value, &format!("run_identity.{field}"))?;
        }
    }
    Ok(())
}

pub(crate) fn validate_run_state(value: &Value) -> Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("run_state must be an object"))?;
    if object.get("schema").and_then(Value::as_str) == Some(LEGACY_RUN_STATE_SCHEMA)
        && object.get("version").and_then(Value::as_u64) == Some(1)
    {
        return validate_legacy_run_state(object);
    }
    if object.get("schema").and_then(Value::as_str) != Some(RUN_STATE_SCHEMA)
        || object.get("version").and_then(Value::as_u64) != Some(2)
    {
        bail!("run_state schema/version is invalid");
    }
    exact_keys(
        object,
        &[
            "schema",
            "version",
            "run_spec_id",
            "run_identity",
            "policy_pending",
            "policy_sha256",
            "state",
            "created_at",
            "last_transition_at",
            "last_transition_reason",
            "owner",
            "requested_final",
            "private_stage",
            "candidate_packet_sha256",
            "final_claimed",
            "publication_guard",
        ],
        "run_state",
    )?;
    let state = required_state(object)?;
    let run_spec_id = object
        .get("run_spec_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("run_state.run_spec_id is missing"))?;
    validate_hash(run_spec_id, "run_state.run_spec_id")?;
    let policy_pending = object
        .get("policy_pending")
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow!("run_state.policy_pending must be boolean"))?;
    match (policy_pending, object.get("run_identity")) {
        (true, Some(identity)) if !identity.is_null() => {
            bail!("pending run state cannot contain a final run identity")
        }
        (true, _) => {
            if matches!(state, "ready" | "published") {
                bail!("policy-pending run state cannot be authoritative");
            }
            if object
                .get("policy_sha256")
                .is_some_and(|value| !value.is_null())
            {
                bail!("pending run state cannot contain policy_sha256");
            }
        }
        (false, Some(identity)) => {
            validate_run_identity(identity)?;
            if identity.get("run_spec_id").and_then(Value::as_str) != Some(run_spec_id) {
                bail!("run_state.run_spec_id does not match run_identity");
            }
            let policy_sha256 = object
                .get("policy_sha256")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("final run state.policy_sha256 is missing"))?;
            validate_hash(policy_sha256, "run_state.policy_sha256")?;
            if identity.get("policy_sha256").and_then(Value::as_str) != Some(policy_sha256) {
                bail!("run_state.policy_sha256 does not match run_identity");
            }
        }
        (false, None) => bail!("final run state.run_identity is missing"),
    }
    for field in [
        "created_at",
        "last_transition_at",
        "last_transition_reason",
        "requested_final",
        "private_stage",
    ] {
        if object
            .get(field)
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            bail!("run_state.{field} must be a non-empty string");
        }
    }
    let owner = object
        .get("owner")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("run_state.owner must be an object"))?;
    if owner.get("pid").and_then(Value::as_u64).is_none()
        || owner
            .get("host")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        || owner
            .get("process_start")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
    {
        bail!("run_state.owner is invalid");
    }
    if let Some(candidate) = object
        .get("candidate_packet_sha256")
        .and_then(Value::as_str)
    {
        validate_hash(candidate, "run_state.candidate_packet_sha256")?;
    }
    if object
        .get("final_claimed")
        .and_then(Value::as_bool)
        .is_none()
    {
        bail!("run_state.final_claimed must be boolean");
    }
    if let Some(guard) = object
        .get("publication_guard")
        .filter(|value| !value.is_null())
    {
        let guard = guard
            .as_object()
            .ok_or_else(|| anyhow!("run_state.publication_guard must be an object or null"))?;
        let mut expected_guard_keys = vec!["path"];
        if guard.get("claim_sha256").is_some() {
            expected_guard_keys.push("claim_sha256");
        }
        exact_keys(guard, &expected_guard_keys, "run_state.publication_guard")?;
        if guard
            .get("path")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            bail!("run_state.publication_guard.path must be a non-empty string");
        }
        if let Some(claim_sha256) = guard.get("claim_sha256").and_then(Value::as_str) {
            validate_hash(claim_sha256, "run_state.publication_guard.claim_sha256")?;
        } else if state == "published" {
            bail!("published run state.publication_guard.claim_sha256 is missing");
        }
    }
    if state == "published" && !object["final_claimed"].as_bool().unwrap_or(false) {
        bail!("published run state must record final_claimed=true");
    }
    if matches!(state, "ready" | "published")
        && object
            .get("candidate_packet_sha256")
            .and_then(Value::as_str)
            .is_none()
    {
        bail!("authoritative run state must record candidate_packet_sha256");
    }
    Ok(())
}

fn validate_legacy_run_state(object: &Map<String, Value>) -> Result<()> {
    let state = object
        .get("state")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("legacy run_state.state is missing"))?;
    if !matches!(
        state,
        "claimed" | "writing" | "ready" | "published" | "failed" | "recovery-required"
    ) {
        bail!("legacy run_state.state is invalid: {state}");
    }
    validate_run_identity(
        object
            .get("run_identity")
            .ok_or_else(|| anyhow!("legacy run_state.run_identity is missing"))?,
    )?;
    for field in [
        "created_at",
        "last_transition_at",
        "requested_final",
        "private_stage",
    ] {
        if object
            .get(field)
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            bail!("legacy run_state.{field} must be a non-empty string");
        }
    }
    Ok(())
}

fn required_state(object: &Map<String, Value>) -> Result<&str> {
    let state = object
        .get("state")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("run_state.state is missing"))?;
    if !matches!(
        state,
        "planned" | "claimed" | "writing" | "ready" | "published" | "failed" | "recovery-required"
    ) {
        bail!("run_state.state is invalid: {state}");
    }
    Ok(state)
}

fn validate_hash(value: &str, label: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("{label} must be a SHA-256 digest");
    }
    Ok(())
}

fn digest_json(value: &Value) -> String {
    crate::qa::common::sha256_bytes(
        &serde_json::to_vec(&canonical_json(value)).expect("identity material is serializable"),
    )
}

fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort();
            let mut canonical = Map::new();
            for key in keys {
                canonical.insert(key.clone(), canonical_json(&object[key]));
            }
            Value::Object(canonical)
        }
        Value::Array(values) => Value::Array(values.iter().map(canonical_json).collect()),
        _ => value.clone(),
    }
}

fn process_start_token(pid: u32) -> String {
    Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "lstart="])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn host_identity() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            Command::new("hostname")
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "unknown".to_owned())
}

pub(crate) fn pause_if_requested(point: &str, path: &Path) -> Result<()> {
    if std::env::var("TERMIFLOW_QA_PAUSE_AT").ok().as_deref() != Some(point) {
        return Ok(());
    }
    let marker = std::env::var_os("TERMIFLOW_QA_PAUSE_MARKER")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("TERMIFLOW_QA_PAUSE_MARKER is required for pause point {point}"))?;
    crate::qa::common::write_json(
        &marker,
        &json!({
            "schema": "termiflow.qa_pause.v1",
            "point": point,
            "pid": std::process::id(),
            "path": path.to_string_lossy().replace('\\', "/"),
        }),
    )?;
    loop {
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn unique_sibling(path: &Path, purpose: &str) -> Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("target has no parent: {}", path.display()))?;
    let name = path
        .file_name()
        .ok_or_else(|| anyhow!("target has no file name: {}", path.display()))?
        .to_string_lossy();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    Ok(parent.join(format!(
        ".{name}.termiflow-{purpose}-{}-{nanos}-{}",
        std::process::id(),
        sequence()
    )))
}

pub(crate) fn guard_path(path: &Path, purpose: &str) -> Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("target has no parent: {}", path.display()))?;
    let name = path
        .file_name()
        .ok_or_else(|| anyhow!("target has no file name: {}", path.display()))?
        .to_string_lossy();
    Ok(parent.join(format!(".{name}.termiflow-{purpose}.lock")))
}

fn probe_directory_no_replace(parent: &Path) -> Result<()> {
    let probe_target = unique_sibling(&parent.join(".termiflow-capability"), "target")?;
    let probe_stage = unique_sibling(&parent.join(".termiflow-capability"), "stage")?;
    fs::create_dir(&probe_target).with_context(|| {
        format!(
            "create atomic-directory capability probe target {}",
            probe_target.display()
        )
    })?;
    fs::create_dir(&probe_stage).with_context(|| {
        format!(
            "create atomic-directory capability probe stage {}",
            probe_stage.display()
        )
    })?;
    let result = SystemOps.claim_dir_no_replace(&probe_stage, &probe_target);
    let cleanup_target = fs::remove_dir_all(&probe_target);
    let cleanup_stage = if probe_stage.exists() {
        fs::remove_dir_all(&probe_stage)
    } else {
        Ok(())
    };
    cleanup_target
        .and(cleanup_stage)
        .map_err(|error| PersistenceError::Cleanup {
            path: parent.to_path_buf(),
            detail: format!("capability probe cleanup failed: {error}"),
        })?;
    match result {
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(classify_atomic_directory_error(parent, error)),
        Ok(()) => Err(PersistenceError::Unsupported {
            path: parent.to_path_buf(),
            detail: "no-replace capability probe unexpectedly replaced its existing target"
                .to_owned(),
        }
        .into()),
    }
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    Ok(())
}

fn sync_tree(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "cannot durably sync a symlink",
        ));
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            sync_tree(&entry?.path())?;
        }
    } else if metadata.is_file() {
        fs::File::open(path)?.sync_all()?;
        return Ok(());
    } else {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "cannot durably sync a non-regular packet entry",
        ));
    }
    fs::File::open(path)?.sync_all()
}

fn remove_file_if_present<O: PersistenceOps>(ops: &O, path: &Path) -> Result<()> {
    match ops.remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(PersistenceError::Cleanup {
            path: path.to_path_buf(),
            detail: error.to_string(),
        }
        .into()),
    }
}

#[cfg(test)]
fn remove_dir_if_present<O: PersistenceOps>(ops: &O, path: &Path) -> Result<()> {
    match ops.remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(PersistenceError::Cleanup {
            path: path.to_path_buf(),
            detail: error.to_string(),
        }
        .into()),
    }
}

fn stage_bytes<O: PersistenceOps>(ops: &O, target: &Path, content: &[u8]) -> Result<PathBuf> {
    ensure_parent(target)?;
    let staged = unique_sibling(target, "stage")?;
    if let Err(error) = ops.write_new(&staged, content) {
        let cleanup = remove_file_if_present(ops, &staged);
        return match cleanup {
            Ok(()) => Err(anyhow!("stage {}: {error}", staged.display())),
            Err(cleanup_error) => Err(anyhow!(
                "stage {}: {error}; {cleanup_error}",
                staged.display()
            )),
        };
    }
    Ok(staged)
}

fn read_regular(path: &Path) -> Result<Option<Vec<u8>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("inspect {}", path.display())),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!(
            "persistence target is not a regular file: {}",
            path.display()
        );
    }
    Ok(Some(
        fs::read(path).with_context(|| format!("read {}", path.display()))?,
    ))
}

fn classify_claim_error(path: &Path, error: io::Error) -> anyhow::Error {
    let detail = error.to_string();
    match error.kind() {
        io::ErrorKind::AlreadyExists => PersistenceError::Conflict {
            path: path.to_path_buf(),
            detail,
        }
        .into(),
        io::ErrorKind::Unsupported | io::ErrorKind::InvalidInput => PersistenceError::Unsupported {
            path: path.to_path_buf(),
            detail,
        }
        .into(),
        _ => anyhow!("claim {}: {detail}", path.display()),
    }
}

pub(crate) fn reject_existing(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            let kind = if metadata.file_type().is_symlink() {
                "symlink"
            } else if metadata.is_dir() {
                "directory"
            } else {
                "file"
            };
            bail!("{label} already exists as a {kind}: {}", path.display());
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("inspect {label} {}", path.display())),
    }
}

fn reject_existing_as_conflict(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            let kind = if metadata.file_type().is_symlink() {
                "symlink"
            } else if metadata.is_dir() {
                "directory"
            } else {
                "file"
            };
            Err(PersistenceError::Conflict {
                path: path.to_path_buf(),
                detail: format!("{label} already exists as a {kind}"),
            }
            .into())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("inspect {label} {}", path.display())),
    }
}

pub(crate) fn publish_file(path: &Path, content: &[u8]) -> Result<PublishOutcome> {
    publish_file_with_ops(&SystemOps, path, content)
}

pub(crate) fn publish_file_with_ops<O: PersistenceOps>(
    ops: &O,
    path: &Path,
    content: &[u8],
) -> Result<PublishOutcome> {
    if let Some(existing) = read_regular(path)? {
        if existing == content {
            return Ok(PublishOutcome::EqualReplay);
        }
        return Err(PersistenceError::Conflict {
            path: path.to_path_buf(),
            detail: "existing bytes have a different identity".to_owned(),
        }
        .into());
    }

    let staged = stage_bytes(ops, path, content)?;
    match ops.claim_file(&staged, path) {
        Ok(()) => {
            remove_file_if_present(ops, &staged)?;
            Ok(PublishOutcome::Published)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let cleanup = remove_file_if_present(ops, &staged);
            cleanup?;
            let existing = read_regular(path);
            if existing?.as_deref() == Some(content) {
                Ok(PublishOutcome::EqualReplay)
            } else {
                Err(PersistenceError::Conflict {
                    path: path.to_path_buf(),
                    detail: "another publisher claimed a different identity".to_owned(),
                }
                .into())
            }
        }
        Err(error) => {
            let cleanup = remove_file_if_present(ops, &staged);
            match cleanup {
                Ok(()) => Err(classify_claim_error(path, error)),
                Err(cleanup_error) => Err(anyhow!(
                    "claim {} failed: {error}; {cleanup_error}",
                    path.display()
                )),
            }
        }
    }
}

pub(crate) fn publish_json(path: &Path, value: &Value) -> Result<PublishOutcome> {
    let mut bytes = serde_json::to_vec_pretty(value).context("serialize persisted JSON")?;
    bytes.push(b'\n');
    publish_file(path, &bytes)
}

#[cfg(test)]
pub(crate) fn claim_directory(path: &Path) -> Result<PathBuf> {
    ensure_parent(path)?;
    match SystemOps.claim_dir(path) {
        Ok(()) => Ok(path.to_path_buf()),
        Err(error) => Err(classify_claim_error(path, error)),
    }
}

/// Create a private sibling directory for a packet. The requested final path
/// remains absent until `publish_directory` succeeds.
pub(crate) fn claim_directory_stage(path: &Path) -> Result<PathBuf> {
    ensure_parent(path)?;
    reject_existing(path, "packet directory")?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("packet directory has no parent: {}", path.display()))?;
    probe_directory_no_replace(parent)?;
    recover_stale_stages(path)?;
    claim_directory_stage_with_ops(&SystemOps, path)
}

fn claim_directory_stage_with_ops<O: PersistenceOps>(ops: &O, path: &Path) -> Result<PathBuf> {
    ensure_parent(path)?;
    reject_existing(path, "packet directory")?;
    let stage = unique_sibling(path, "stage")?;
    match ops.claim_dir(&stage) {
        Ok(()) => Ok(stage),
        Err(error) => Err(classify_claim_error(&stage, error)),
    }
}

fn recover_stale_stages(target: &Path) -> Result<()> {
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("target has no parent: {}", target.display()))?;
    let name = target
        .file_name()
        .ok_or_else(|| anyhow!("target has no file name: {}", target.display()))?
        .to_string_lossy();
    let prefix = format!(".{name}.termiflow-stage-");
    let current_host = host_identity();
    for entry in fs::read_dir(parent).with_context(|| format!("scan {}", parent.display()))? {
        let entry = entry?;
        let candidate = entry.path();
        let candidate_name = entry.file_name().to_string_lossy().to_string();
        if !candidate_name.starts_with(&prefix) || !candidate.is_dir() {
            continue;
        }
        let state_path = candidate.join("run_state.json");
        let state_bytes = match fs::read(&state_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(PersistenceError::RecoveryRequired {
                    path: candidate,
                    detail: "private stage has no owner state; manual recovery is required"
                        .to_owned(),
                }
                .into());
            }
            Err(error) => {
                return Err(PersistenceError::RecoveryRequired {
                    path: candidate,
                    detail: format!("cannot inspect private stage state: {error}"),
                }
                .into());
            }
        };
        let state: Value = serde_json::from_slice(&state_bytes).map_err(|error| {
            PersistenceError::RecoveryRequired {
                path: candidate.clone(),
                detail: format!("private stage state is malformed: {error}"),
            }
        })?;
        validate_run_state(&state).map_err(|error| PersistenceError::RecoveryRequired {
            path: candidate.clone(),
            detail: format!("private stage state is invalid: {error}"),
        })?;
        if state
            .get("requested_final")
            .and_then(Value::as_str)
            .is_some_and(|requested| requested != target.to_string_lossy().replace('\\', "/"))
        {
            return Err(PersistenceError::RecoveryRequired {
                path: candidate,
                detail: "private stage is bound to a different requested final".to_owned(),
            }
            .into());
        }
        let owner = &state["owner"];
        let owner_host = owner["host"].as_str().unwrap_or("unknown");
        let owner_pid = owner["pid"].as_u64().unwrap_or_default();
        let owner_start = owner["process_start"].as_str().unwrap_or("unknown");
        if owner_host == "unknown" || current_host == "unknown" {
            return Err(PersistenceError::RecoveryRequired {
                path: candidate,
                detail: format!(
                    "private stage owner host is unverifiable ({owner_host:?}/{current_host:?}); explicit recovery is required"
                ),
            }
            .into());
        }
        if owner_host != current_host {
            return Err(PersistenceError::RecoveryRequired {
                path: candidate,
                detail: format!(
                    "private stage owner host {owner_host} is not current host; explicit recovery is required"
                ),
            }
            .into());
        }
        if process_is_alive(owner_pid) {
            let current_start = u32::try_from(owner_pid)
                .ok()
                .map(process_start_token)
                .unwrap_or_else(|| "unknown".to_owned());
            if owner_start == "unknown"
                || current_start == "unknown"
                || owner_start == current_start
            {
                return Err(PersistenceError::Conflict {
                    path: candidate,
                    detail: format!(
                        "private stage is still owned by {owner_host}:{owner_pid}; explicit recovery is required"
                    ),
                }
                .into());
            }
            return Err(PersistenceError::Conflict {
                path: candidate,
                detail: format!(
                    "private stage PID {owner_pid} was reused with a different process-start token"
                ),
            }
            .into());
        }
        if owner_start == "unknown" {
            return Err(PersistenceError::RecoveryRequired {
                path: candidate,
                detail: "private stage owner process-start identity is unverifiable; manual recovery is required"
                    .to_owned(),
            }
            .into());
        }
        let recovery = unique_sibling(target, "recovery")?;
        fs::rename(&candidate, &recovery).with_context(|| {
            format!(
                "quarantine stale private stage {} as {}",
                candidate.display(),
                recovery.display()
            )
        })?;
    }
    Ok(())
}

#[cfg(unix)]
fn process_is_alive(pid: u64) -> bool {
    if pid == 0 {
        return false;
    }
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(not(unix))]
fn process_is_alive(pid: u64) -> bool {
    pid == u64::from(std::process::id())
}

/// Publish a complete private packet directory into an absent final path.
///
/// The adjacent create-new lock is part of the publication protocol. A plain
/// rename is intentionally not treated as a portable no-overwrite claim.
pub(crate) fn publish_directory(
    stage: &Path,
    target: &Path,
    claim_content: &[u8],
) -> Result<PublishOutcome> {
    publish_directory_with_ops(&SystemOps, stage, target, claim_content)
}

pub(crate) fn publish_directory_with_ops<O: PersistenceOps>(
    ops: &O,
    stage: &Path,
    target: &Path,
    claim_content: &[u8],
) -> Result<PublishOutcome> {
    let stage_parent = stage
        .parent()
        .ok_or_else(|| anyhow!("stage has no parent: {}", stage.display()))?;
    let target_parent = target
        .parent()
        .ok_or_else(|| anyhow!("target has no parent: {}", target.display()))?;
    if stage_parent != target_parent {
        return Err(PersistenceError::Unsupported {
            path: target.to_path_buf(),
            detail: "stage and final packet must share a parent directory".to_owned(),
        }
        .into());
    }
    let stage_metadata =
        fs::symlink_metadata(stage).map_err(|error| PersistenceError::Incomplete {
            path: stage.to_path_buf(),
            detail: format!("private stage cannot be inspected: {error}"),
        })?;
    if stage_metadata.file_type().is_symlink() || !stage_metadata.is_dir() {
        return Err(PersistenceError::Incomplete {
            path: stage.to_path_buf(),
            detail: "private stage is not a regular directory".to_owned(),
        }
        .into());
    }
    reject_existing_as_conflict(target, "packet directory")?;

    let guard = guard_path(target, "publish")?;
    if let Err(error) = ops.write_new(&guard, claim_content) {
        return Err(classify_claim_error(&guard, error));
    }

    let mut final_claimed = false;
    let result: Result<PublishOutcome> = (|| {
        // The lock serializes all cooperative publishers. Re-check after it is
        // held so a winner can never be replaced by a check-then-claim loser.
        reject_existing_as_conflict(target, "packet directory")?;
        ops.sync_tree(stage)
            .map_err(|error| PersistenceError::Unsupported {
                path: stage.to_path_buf(),
                detail: format!("stage durability sync is unavailable: {error}"),
            })?;
        ops.claim_dir_no_replace(stage, target)
            .map_err(|error| classify_atomic_directory_error(target, error))?;
        final_claimed = true;
        ops.sync_path(target_parent)
            .map_err(|error| PersistenceError::RecoveryRequired {
                path: target_parent.to_path_buf(),
                detail: format!(
                    "final directory was claimed but parent durability sync failed: {error}"
                ),
            })?;
        mark_published_state_with_ops(ops, target, &guard, claim_content).map_err(|error| {
            PersistenceError::RecoveryRequired {
                path: target.to_path_buf(),
                detail: format!(
                    "final directory was claimed but published state could not be recorded: {error}"
                ),
            }
        })?;
        Ok(PublishOutcome::Published)
    })();
    let guard_cleanup = remove_file_if_present(ops, &guard);
    match (result, guard_cleanup) {
        (Ok(outcome), Ok(())) => Ok(outcome),
        (Err(error), Ok(())) => {
            if final_claimed {
                Err(PersistenceError::RecoveryRequired {
                    path: target.to_path_buf(),
                    detail: error.to_string(),
                }
                .into())
            } else {
                Err(error)
            }
        }
        (Ok(_), Err(cleanup_error)) => {
            if final_claimed {
                Err(PersistenceError::RecoveryRequired {
                    path: target.to_path_buf(),
                    detail: format!(
                        "final published but publication guard cleanup failed: {cleanup_error}"
                    ),
                }
                .into())
            } else {
                Err(cleanup_error)
            }
        }
        (Err(error), Err(cleanup_error)) => {
            if final_claimed {
                Err(PersistenceError::RecoveryRequired {
                    path: target.to_path_buf(),
                    detail: format!(
                        "{}; publication guard cleanup failed: {cleanup_error}",
                        error
                    ),
                }
                .into())
            } else {
                Err(anyhow!("{error}; {cleanup_error}"))
            }
        }
    }
}

fn mark_published_state_with_ops<O: PersistenceOps>(
    ops: &O,
    target: &Path,
    guard: &Path,
    claim_content: &[u8],
) -> Result<()> {
    let state_path = target.join("run_state.json");
    let Some(bytes) = read_regular(&state_path)? else {
        // The low-level persistence helper is also used by legacy tests and
        // generic directory receipts that do not carry private QA state.
        return Ok(());
    };
    let mut state: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse private run state {}", state_path.display()))?;
    validate_run_state(&state)?;
    state["state"] = Value::String("published".to_owned());
    state["final_claimed"] = Value::Bool(true);
    state["last_transition_at"] = Value::String(crate::qa::common::now_label());
    state["last_transition_reason"] = Value::String("final directory claim completed".to_owned());
    state["publication_guard"] = json!({
        "path": guard.to_string_lossy().replace('\\', "/"),
        "claim_sha256": crate::qa::common::sha256_bytes(claim_content),
    });
    write_state_with_ops(ops, &state_path, &state, true)?;
    Ok(())
}

/// Repair only the state/guard residue after a final directory claim. Packet
/// bytes are never republished or rewritten by this operation.
pub(crate) fn repair_published_state(target: &Path) -> Result<()> {
    let state_path = target.join("run_state.json");
    let bytes = read_regular(&state_path)?.ok_or_else(|| PersistenceError::RecoveryRequired {
        path: target.to_path_buf(),
        detail: "final packet has no run state to repair".to_owned(),
    })?;
    let mut state: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse final run state {}", state_path.display()))?;
    validate_run_state(&state)?;
    let complete = read_regular(&target.join("COMPLETE.json"))?.ok_or_else(|| {
        PersistenceError::RecoveryRequired {
            path: target.to_path_buf(),
            detail: "final packet has no completion marker".to_owned(),
        }
    })?;
    let complete: Value =
        serde_json::from_slice(&complete).context("parse final completion marker")?;
    if complete.get("schema").and_then(Value::as_str) != Some("termiflow.visual_audit.complete.v1")
    {
        bail!("final completion marker schema is invalid");
    }
    let candidate = state
        .get("candidate_packet_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("final run state candidate packet digest is missing"))?;
    validate_hash(candidate, "run_state.candidate_packet_sha256")?;
    let packet = complete
        .get("packet_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("final completion marker packet digest is missing"))?;
    validate_hash(packet, "complete.packet_sha256")?;
    if candidate != packet {
        bail!("final completion marker packet digest does not match run state");
    }
    let (actual_packet, packet_listing) = crate::qa::common::deterministic_digest(target)?;
    if actual_packet != packet {
        bail!("final completion marker packet digest does not match packet contents");
    }
    if crate::qa::common::require_file(&target.join("PACKET.sha256"), "packet listing")?
        != packet_listing.as_bytes()
    {
        bail!("final packet listing does not match packet contents");
    }
    let expected_guard = guard_path(target, "publish")?;
    let needs_state_repair = state["state"] != "published";
    if needs_state_repair {
        state["state"] = Value::String("published".to_owned());
        state["final_claimed"] = Value::Bool(true);
        state["last_transition_at"] = Value::String(crate::qa::common::now_label());
        state["last_transition_reason"] =
            Value::String("repaired after completed final directory claim".to_owned());
    }
    let recorded_guard = state
        .get("publication_guard")
        .and_then(Value::as_object)
        .and_then(|value| value.get("path"))
        .and_then(Value::as_str)
        .map(PathBuf::from);
    if let Some(recorded_guard) = &recorded_guard {
        if recorded_guard != &expected_guard {
            return Err(PersistenceError::RecoveryRequired {
                path: recorded_guard.clone(),
                detail: "published-state guard is not the expected packet sibling".to_owned(),
            }
            .into());
        }
    }
    let guard = expected_guard;
    let guard_digest = state
        .get("publication_guard")
        .and_then(Value::as_object)
        .and_then(|value| value.get("claim_sha256"))
        .and_then(Value::as_str);
    let actual_guard_digest = if let Some(guard_bytes) = read_regular(&guard)? {
        let actual_guard_digest = crate::qa::common::sha256_bytes(&guard_bytes);
        if let Some(expected) = guard_digest {
            if actual_guard_digest != expected {
                return Err(PersistenceError::RecoveryRequired {
                    path: guard,
                    detail: "publication guard identity does not match final state".to_owned(),
                }
                .into());
            }
        }
        Some(actual_guard_digest)
    } else {
        None
    };
    if needs_state_repair {
        state["publication_guard"] = actual_guard_digest.as_ref().map_or_else(
            || Value::Null,
            |claim_sha256| {
                json!({
                    "path": guard.to_string_lossy().replace('\\', "/"),
                    "claim_sha256": claim_sha256,
                })
            },
        );
        write_state_with_ops(&SystemOps, &state_path, &state, true)?;
    }
    if actual_guard_digest.is_some() {
        remove_file_if_present(&SystemOps, &guard)?;
    }
    Ok(())
}

fn exact_keys(object: &Map<String, Value>, expected: &[&str], label: &str) -> Result<()> {
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    let actual = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if actual != expected {
        let missing = expected.difference(&actual).copied().collect::<Vec<_>>();
        let unknown = actual.difference(&expected).copied().collect::<Vec<_>>();
        bail!("{label} keys drifted; missing={missing:?} unknown={unknown:?}");
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn claim_directory_with_ops<O: PersistenceOps>(ops: &O, path: &Path) -> Result<PathBuf> {
    ensure_parent(path)?;
    match ops.claim_dir(path) {
        Ok(()) => Ok(path.to_path_buf()),
        Err(error) => Err(classify_claim_error(path, error)),
    }
}

pub(crate) fn remove_incomplete_directory(path: &Path) -> Result<()> {
    match SystemOps.remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(PersistenceError::Incomplete {
            path: path.to_path_buf(),
            detail: format!("manual recovery is required: {error}"),
        }
        .into()),
    }
}

pub(crate) fn replace_with_intent(
    path: &Path,
    expected_old_sha256: Option<&str>,
    content: &[u8],
    intent: &str,
) -> Result<PublishOutcome> {
    replace_with_intent_with_ops(&SystemOps, path, expected_old_sha256, content, intent)
}

pub(crate) fn replace_with_intent_with_ops<O: PersistenceOps>(
    ops: &O,
    path: &Path,
    expected_old_sha256: Option<&str>,
    content: &[u8],
    intent: &str,
) -> Result<PublishOutcome> {
    if intent.trim().is_empty() {
        bail!("golden replacement intent must not be empty");
    }
    let current = read_regular(path)?;
    let current_sha256 = current.as_deref().map(crate::qa::common::sha256_bytes);
    if current_sha256.as_deref() != expected_old_sha256 {
        return Err(PersistenceError::Conflict {
            path: path.to_path_buf(),
            detail: format!(
                "old digest changed: expected {:?}, found {:?}",
                expected_old_sha256, current_sha256
            ),
        }
        .into());
    }
    if current.as_deref() == Some(content) {
        return Ok(PublishOutcome::EqualReplay);
    }

    ensure_parent(path)?;
    let guard = guard_path(path, "intent")?;
    let guard_content = format!(
        "intent={intent}\nold_sha256={}\nnew_sha256={}\n",
        expected_old_sha256.unwrap_or("<absent>"),
        crate::qa::common::sha256_bytes(content)
    );
    if let Err(error) = ops.write_new(&guard, guard_content.as_bytes()) {
        return Err(classify_claim_error(&guard, error));
    }

    let result = (|| {
        let reread = read_regular(path)?;
        let reread_sha256 = reread.as_deref().map(crate::qa::common::sha256_bytes);
        if reread_sha256.as_deref() != expected_old_sha256 {
            return Err(PersistenceError::Conflict {
                path: path.to_path_buf(),
                detail: "old digest changed while the replacement claim was held".to_owned(),
            }
            .into());
        }
        if reread.as_deref() == Some(content) {
            return Ok(PublishOutcome::EqualReplay);
        }
        let staged = stage_bytes(ops, path, content)?;
        match ops.replace(&staged, path) {
            Ok(()) => {
                remove_file_if_present(ops, &staged)?;
                Ok(PublishOutcome::Published)
            }
            Err(error) => {
                let cleanup = remove_file_if_present(ops, &staged);
                if let Err(cleanup_error) = cleanup {
                    return Err(anyhow!(
                        "replace {} failed: {error}; {cleanup_error}",
                        path.display()
                    ));
                }
                Err(anyhow!("replace {}: {error}", path.display()))
            }
        }
    })();
    let guard_cleanup = remove_file_if_present(ops, &guard);
    match (result, guard_cleanup) {
        (Ok(outcome), Ok(())) => Ok(outcome),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(cleanup_error)) => Err(cleanup_error),
        (Err(error), Err(cleanup_error)) => Err(anyhow!("{error}; {cleanup_error}")),
    }
}

pub(crate) fn canonical_without_timestamp(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut canonical = Map::new();
            for (key, value) in object {
                if key != "timestamp" {
                    canonical.insert(key.clone(), canonical_without_timestamp(value));
                }
            }
            Value::Object(canonical)
        }
        Value::Array(values) => {
            Value::Array(values.iter().map(canonical_without_timestamp).collect())
        }
        _ => value.clone(),
    }
}

pub(crate) fn semantically_equal_without_timestamp(left: &Value, right: &Value) -> bool {
    canonical_without_timestamp(left) == canonical_without_timestamp(right)
}

pub(crate) fn append_decision(path: &Path, decision: &Value) -> Result<()> {
    append_decision_checked(path, decision, || Ok(PublishOutcome::Published)).map(|_| ())
}

pub(crate) fn append_decision_checked<F>(
    path: &Path,
    decision: &Value,
    check: F,
) -> Result<PublishOutcome>
where
    F: FnOnce() -> Result<PublishOutcome>,
{
    let mut line = serde_json::to_vec(decision).context("serialize review decision")?;
    line.push(b'\n');
    append_line_with_ops(&SystemOps, path, &line, check)
}

#[cfg(test)]
pub(crate) fn append_decision_with_ops<O: PersistenceOps>(
    ops: &O,
    path: &Path,
    line: &[u8],
) -> Result<()> {
    append_line_with_ops(ops, path, line, || Ok(PublishOutcome::Published)).map(|_| ())
}

fn append_line_with_ops<O, F>(ops: &O, path: &Path, line: &[u8], check: F) -> Result<PublishOutcome>
where
    O: PersistenceOps,
    F: FnOnce() -> Result<PublishOutcome>,
{
    ensure_parent(path)?;
    let guard = guard_path(path, "review")?;
    let guard_content = format!(
        "path={} owner={} created={:?}\n",
        path.display(),
        std::process::id(),
        SystemTime::now()
    );
    if let Err(error) = ops.write_new(&guard, guard_content.as_bytes()) {
        return Err(classify_claim_error(&guard, error));
    }
    let result = (|| {
        let outcome = check()?;
        if outcome == PublishOutcome::EqualReplay {
            return Ok(outcome);
        }
        ops.append(path, line)
            .with_context(|| format!("append review decision {}", path.display()))?;
        Ok(PublishOutcome::Published)
    })();
    let guard_cleanup = remove_file_if_present(ops, &guard);
    match (result, guard_cleanup) {
        (Ok(outcome), Ok(())) => Ok(outcome),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(cleanup_error)) => Err(cleanup_error),
        (Err(error), Err(cleanup_error)) => Err(anyhow!("{error}; {cleanup_error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Failure {
        WriteNew,
        ClaimFile,
        ClaimDir,
        ClaimDirNoReplace,
        Replace,
        Append,
        SyncPath,
        RemoveFile,
        RemoveDir,
    }

    #[derive(Debug, Default)]
    struct InjectedOps {
        failure: Mutex<Option<Failure>>,
    }

    impl InjectedOps {
        fn fail_once(&self, failure: Failure) -> io::Result<()> {
            let mut configured = self.failure.lock().expect("failure lock");
            if configured.as_ref() == Some(&failure) {
                *configured = None;
                Err(io::Error::other(format!("injected {failure:?} failure")))
            } else {
                Ok(())
            }
        }
    }

    impl PersistenceOps for InjectedOps {
        fn write_new(&self, path: &Path, content: &[u8]) -> io::Result<()> {
            self.fail_once(Failure::WriteNew)?;
            SystemOps.write_new(path, content)
        }

        fn claim_file(&self, staged: &Path, target: &Path) -> io::Result<()> {
            self.fail_once(Failure::ClaimFile)?;
            SystemOps.claim_file(staged, target)
        }

        fn claim_dir(&self, target: &Path) -> io::Result<()> {
            self.fail_once(Failure::ClaimDir)?;
            SystemOps.claim_dir(target)
        }

        fn claim_dir_no_replace(&self, staged: &Path, target: &Path) -> io::Result<()> {
            self.fail_once(Failure::ClaimDirNoReplace)?;
            SystemOps.claim_dir_no_replace(staged, target)
        }

        fn replace(&self, staged: &Path, target: &Path) -> io::Result<()> {
            self.fail_once(Failure::Replace)?;
            SystemOps.replace(staged, target)
        }

        fn append(&self, path: &Path, content: &[u8]) -> io::Result<()> {
            self.fail_once(Failure::Append)?;
            SystemOps.append(path, content)
        }

        fn sync_path(&self, path: &Path) -> io::Result<()> {
            self.fail_once(Failure::SyncPath)?;
            SystemOps.sync_path(path)
        }

        fn remove_file(&self, path: &Path) -> io::Result<()> {
            self.fail_once(Failure::RemoveFile)?;
            SystemOps.remove_file(path)
        }

        fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
            self.fail_once(Failure::RemoveDir)?;
            SystemOps.remove_dir_all(path)
        }
    }

    fn test_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "termiflow-persist-{label}-{}-{}",
            std::process::id(),
            sequence()
        ));
        fs::create_dir_all(&path).expect("create persistence test directory");
        path
    }

    #[test]
    fn publish_is_absent_equal_replay_and_conflict_safe() {
        let root = test_dir("publish");
        let path = root.join("receipt.json");
        assert_eq!(
            publish_file(&path, b"one\n").expect("publish"),
            PublishOutcome::Published
        );
        assert_eq!(
            publish_file(&path, b"one\n").expect("equal replay"),
            PublishOutcome::EqualReplay
        );
        assert!(publish_file(&path, b"two\n").is_err());
        assert_eq!(fs::read(&path).expect("read receipt"), b"one\n");
        fs::remove_dir_all(root).expect("remove test directory");
    }

    #[test]
    fn run_identity_changes_when_policy_set_changes() {
        let run_spec_id = "a".repeat(64);
        let first = run_identity_value(
            &run_spec_id,
            "holdout",
            &"b".repeat(64),
            &"c".repeat(64),
            &"d".repeat(64),
        );
        let second = run_identity_value(
            &run_spec_id,
            "holdout",
            &"b".repeat(64),
            &"c".repeat(64),
            &"e".repeat(64),
        );
        validate_run_identity(&first).expect("first run identity is valid");
        validate_run_identity(&second).expect("second run identity is valid");
        assert_ne!(first["run_id"], second["run_id"]);
    }

    #[test]
    fn injected_stage_claim_and_cleanup_failures_are_fail_closed() {
        let root = test_dir("failures");
        let path = root.join("receipt");

        let write_failure = InjectedOps {
            failure: Mutex::new(Some(Failure::WriteNew)),
        };
        assert!(publish_file_with_ops(&write_failure, &path, b"x").is_err());
        assert!(!path.exists());

        let claim_failure = InjectedOps {
            failure: Mutex::new(Some(Failure::ClaimFile)),
        };
        assert!(publish_file_with_ops(&claim_failure, &path, b"x").is_err());
        assert!(!path.exists());

        let directory = root.join("claimed-directory");
        let directory_failure = InjectedOps {
            failure: Mutex::new(Some(Failure::ClaimDir)),
        };
        assert!(claim_directory_with_ops(&directory_failure, &directory).is_err());
        assert!(!directory.exists());

        let cleanup_failure = InjectedOps {
            failure: Mutex::new(Some(Failure::RemoveFile)),
        };
        assert!(publish_file_with_ops(&cleanup_failure, &path, b"x").is_err());
        assert!(path.exists());

        fs::remove_dir_all(root).expect("remove test directory");
    }

    #[test]
    fn directory_claim_and_replacement_are_intent_bound() {
        let root = test_dir("directory-replace");
        let directory = root.join("packet");
        assert_eq!(
            claim_directory(&directory).expect("claim directory"),
            directory
        );
        assert!(claim_directory(&directory).is_err());

        let golden = root.join("golden.txt");
        fs::write(&golden, b"old\n").expect("write old golden");
        let old_sha = crate::qa::common::sha256_bytes(b"old\n");
        assert_eq!(
            replace_with_intent(&golden, Some(&old_sha), b"new\n", "approved test intent")
                .expect("replace golden"),
            PublishOutcome::Published
        );
        assert_eq!(fs::read(&golden).expect("read new golden"), b"new\n");
        assert!(replace_with_intent(&golden, Some(&old_sha), b"third\n", "stale").is_err());
        fs::remove_dir_all(root).expect("remove test directory");
    }

    #[test]
    fn directory_publication_keeps_final_absent_until_complete() {
        let root = test_dir("directory-publication");
        let target = root.join("packet");
        let stage = claim_directory_stage(&target).expect("claim private stage");
        fs::create_dir_all(stage.join("frames")).expect("create packet frames");
        fs::write(stage.join("frames/frame.txt"), b"complete\n").expect("write packet frame");
        fs::write(stage.join("COMPLETE.json"), b"{\"complete\":true}\n")
            .expect("write completion marker");

        assert!(
            !target.exists(),
            "final packet must stay absent while writing"
        );
        assert_eq!(
            publish_directory(&stage, &target, b"run_id=test\n").expect("publish packet"),
            PublishOutcome::Published
        );
        assert_eq!(
            fs::read(target.join("frames/frame.txt")).expect("read published frame"),
            b"complete\n"
        );
        assert!(!stage.exists(), "stage is moved, not copied");
        assert!(!guard_path(&target, "publish")
            .expect("publish guard path")
            .exists());
        fs::remove_dir_all(root).expect("remove test directory");
    }

    #[test]
    fn directory_publication_rejects_winner_and_cross_parent_stage() {
        let root = test_dir("directory-conflict");
        let target = root.join("packet");
        let first = claim_directory_stage(&target).expect("claim first stage");
        fs::write(first.join("winner"), b"first\n").expect("write first stage");
        publish_directory(&first, &target, b"run_id=first\n").expect("publish first packet");

        let second = unique_sibling(&target, "manual").expect("unique second stage");
        fs::create_dir(&second).expect("create second stage");
        fs::write(second.join("winner"), b"second\n").expect("write second stage");
        let conflict = publish_directory(&second, &target, b"run_id=second\n");
        assert!(matches!(
            conflict
                .expect_err("existing final target must conflict")
                .downcast_ref::<PersistenceError>(),
            Some(PersistenceError::Incomplete { .. }) | Some(PersistenceError::Conflict { .. })
        ));
        assert_eq!(
            fs::read(target.join("winner")).expect("read winner"),
            b"first\n"
        );

        let other = test_dir("directory-conflict-other");
        let cross_stage = other.join("stage");
        fs::create_dir(&cross_stage).expect("create cross stage");
        let unsupported = publish_directory(&cross_stage, &target, b"run_id=cross\n");
        assert!(matches!(
            unsupported
                .expect_err("cross-parent stage must be unsupported")
                .downcast_ref::<PersistenceError>(),
            Some(PersistenceError::Unsupported { .. })
        ));
        fs::remove_dir_all(root).expect("remove test directory");
        fs::remove_dir_all(other).expect("remove other test directory");
    }

    #[test]
    fn directory_publication_fails_closed_when_durability_sync_is_unavailable() {
        let root = test_dir("directory-sync");
        let target = root.join("packet");
        let stage = claim_directory_stage(&target).expect("claim private stage");
        fs::write(stage.join("payload"), b"payload\n").expect("write stage payload");
        let failing = InjectedOps {
            failure: Mutex::new(Some(Failure::SyncPath)),
        };
        let result = publish_directory_with_ops(&failing, &stage, &target, b"run_id=sync\n");
        assert!(result.is_err());
        assert!(!target.exists());
        assert!(
            stage.exists(),
            "failed publication retains private recovery residue"
        );
        fs::remove_dir_all(root).expect("remove test directory");
    }

    #[test]
    fn claimed_directory_state_failure_repairs_without_republication() {
        let root = test_dir("directory-state-repair");
        let target = root.join("packet");
        let stage = claim_directory_stage(&target).expect("claim private stage");
        let (packet_digest, packet_listing) =
            crate::qa::common::deterministic_digest(&stage).expect("digest empty packet");
        fs::write(stage.join("PACKET.sha256"), packet_listing).expect("write packet listing");
        fs::write(
            stage.join("COMPLETE.json"),
            format!(
                "{{\"schema\":\"termiflow.visual_audit.complete.v1\",\"packet_sha256\":\"{packet_digest}\"}}\n"
            ),
        )
        .expect("write completion marker");
        let policy_digest = "e".repeat(64);
        let identity = run_identity_value(
            &"a".repeat(64),
            "test",
            &"b".repeat(64),
            &"c".repeat(64),
            &policy_digest,
        );
        let state = run_state_value(
            &"a".repeat(64),
            Some(&identity),
            "ready",
            &target,
            &stage,
            Some(&packet_digest),
            "created",
            "packet complete",
            false,
            Some(&guard_path(&target, "publish").expect("guard path")),
        );
        write_run_state(&stage, &state).expect("write ready state");
        let failing = InjectedOps {
            failure: Mutex::new(Some(Failure::Replace)),
        };
        let result = publish_directory_with_ops(&failing, &stage, &target, b"claim");
        assert!(matches!(
            result
                .expect_err("state write failure must require recovery")
                .downcast_ref::<PersistenceError>(),
            Some(PersistenceError::RecoveryRequired { .. })
        ));
        assert!(target.is_dir(), "claimed final must remain authoritative");
        assert!(target.join("run_state.json").is_file());
        assert!(repair_published_state(&target).is_ok());
        let repaired =
            crate::qa::common::load_json(&target.join("run_state.json"), "repaired run state")
                .expect("read repaired state");
        assert_eq!(repaired["state"], "published");
        assert_eq!(repaired["final_claimed"], true);
        assert!(!stage.exists(), "repair must never republish the old stage");
        fs::remove_dir_all(root).expect("remove test directory");
    }

    #[test]
    fn stale_stage_recovery_requires_verifiable_owner_identity() {
        let root = test_dir("stage-recovery");
        let target = root.join("packet");
        let missing_state = root.join(".packet.termiflow-stage-missing");
        fs::create_dir(&missing_state).expect("create missing-state stage");
        let missing = claim_directory_stage(&target).expect_err("missing state must stop retry");
        assert!(matches!(
            missing.downcast_ref::<PersistenceError>(),
            Some(PersistenceError::RecoveryRequired { .. })
        ));
        fs::remove_dir_all(&missing_state).expect("remove missing-state stage");

        let malformed_state = root.join(".packet.termiflow-stage-malformed");
        fs::create_dir(&malformed_state).expect("create malformed-state stage");
        fs::write(malformed_state.join("run_state.json"), b"{partial\n")
            .expect("write malformed state");
        let malformed =
            claim_directory_stage(&target).expect_err("malformed state must stop retry");
        assert!(matches!(
            malformed.downcast_ref::<PersistenceError>(),
            Some(PersistenceError::RecoveryRequired { .. })
        ));
        fs::remove_dir_all(&malformed_state).expect("remove malformed-state stage");

        let foreign_stage = root.join(".packet.termiflow-stage-foreign");
        fs::create_dir(&foreign_stage).expect("create foreign-owner stage");
        let identity = run_identity_value(
            &"a".repeat(64),
            "test",
            &"b".repeat(64),
            &"c".repeat(64),
            &"d".repeat(64),
        );
        let mut state = run_state_value(
            &"a".repeat(64),
            Some(&identity),
            "writing",
            &target,
            &foreign_stage,
            None,
            "created",
            "writing",
            false,
            Some(&guard_path(&target, "publish").expect("guard path")),
        );
        state["owner"]["host"] = Value::String("foreign-host.example".to_owned());
        fs::write(
            foreign_stage.join("run_state.json"),
            serde_json::to_vec_pretty(&state).expect("serialize foreign state"),
        )
        .expect("write foreign state");
        let foreign = claim_directory_stage(&target).expect_err("foreign owner must stop retry");
        assert!(matches!(
            foreign.downcast_ref::<PersistenceError>(),
            Some(PersistenceError::RecoveryRequired { .. })
        ));
        assert!(
            foreign_stage.exists(),
            "unverifiable stage must be preserved"
        );
        fs::remove_dir_all(root).expect("remove stage-recovery test directory");
    }

    #[test]
    fn post_claim_guard_cleanup_failure_requires_recovery() {
        let root = test_dir("guard-cleanup");
        let target = root.join("packet");
        let stage = claim_directory_stage(&target).expect("claim private stage");
        fs::write(stage.join("payload"), b"complete\n").expect("write packet payload");
        let failing = InjectedOps {
            failure: Mutex::new(Some(Failure::RemoveFile)),
        };
        let result = publish_directory_with_ops(&failing, &stage, &target, b"claim");
        assert!(matches!(
            result
                .expect_err("guard cleanup failure must require recovery")
                .downcast_ref::<PersistenceError>(),
            Some(PersistenceError::RecoveryRequired { .. })
        ));
        assert!(target.is_dir(), "final remains authoritative after claim");
        assert!(
            guard_path(&target, "publish")
                .expect("guard path")
                .is_file(),
            "failed cleanup must preserve the guard for recovery"
        );
        fs::remove_dir_all(root).expect("remove guard-cleanup test directory");
    }

    #[test]
    fn existing_publication_guard_is_preserved_for_manual_recovery() {
        let root = test_dir("stale-guard");
        let target = root.join("packet");
        let stage = claim_directory_stage(&target).expect("claim private stage");
        fs::write(stage.join("payload"), b"candidate\n").expect("write packet candidate");
        let guard = guard_path(&target, "publish").expect("guard path");
        fs::write(&guard, b"manual owner\n").expect("write stale guard");
        let result = publish_directory(&stage, &target, b"claim");
        assert!(matches!(
            result
                .expect_err("stale guard must prevent publication")
                .downcast_ref::<PersistenceError>(),
            Some(PersistenceError::Conflict { .. })
        ));
        assert!(!target.exists());
        assert!(stage.exists());
        assert!(guard.is_file());
        fs::remove_dir_all(root).expect("remove stale-guard test directory");
    }

    #[test]
    fn concurrent_directory_publishers_have_one_winner() {
        let root = test_dir("directory-race");
        let target = root.join("packet");
        let first = unique_sibling(&target, "race-first").expect("first stage path");
        let second = unique_sibling(&target, "race-second").expect("second stage path");
        fs::create_dir(&first).expect("create first stage");
        fs::create_dir(&second).expect("create second stage");
        fs::write(first.join("winner"), b"first\n").expect("write first candidate");
        fs::write(second.join("winner"), b"second\n").expect("write second candidate");

        let first_target = target.clone();
        let first_thread =
            std::thread::spawn(move || publish_directory(&first, &first_target, b"run_id=first\n"));
        let second_target = target.clone();
        let second_thread = std::thread::spawn(move || {
            publish_directory(&second, &second_target, b"run_id=second\n")
        });
        let first_result = first_thread.join().expect("join first publisher");
        let second_result = second_thread.join().expect("join second publisher");
        assert_eq!(
            usize::from(first_result.is_ok()) + usize::from(second_result.is_ok()),
            1
        );
        let winner = fs::read(target.join("winner")).expect("read directory winner");
        assert!(winner == b"first\n" || winner == b"second\n");
        fs::remove_dir_all(root).expect("remove test directory");
    }

    #[test]
    fn append_semantics_ignore_only_timestamp() {
        let first = serde_json::json!({"case_id":"case", "timestamp":"one", "finding":"none"});
        let second = serde_json::json!({"finding":"none", "case_id":"case", "timestamp":"two"});
        let conflict =
            serde_json::json!({"case_id":"case", "timestamp":"three", "finding":"route"});
        assert!(semantically_equal_without_timestamp(&first, &second));
        assert!(!semantically_equal_without_timestamp(&first, &conflict));
    }

    #[test]
    fn injected_append_and_directory_cleanup_failures_are_observed() {
        let root = test_dir("append-cleanup");
        let log = root.join("decisions.jsonl");
        let append_failure = InjectedOps {
            failure: Mutex::new(Some(Failure::Append)),
        };
        assert!(append_decision_with_ops(&append_failure, &log, b"{}\n").is_err());

        let directory = root.join("incomplete");
        fs::create_dir_all(&directory).expect("create incomplete directory");
        let cleanup_failure = InjectedOps {
            failure: Mutex::new(Some(Failure::RemoveDir)),
        };
        assert!(remove_dir_if_present(&cleanup_failure, &directory).is_err());
        fs::remove_dir_all(root).expect("remove test directory");
    }
}
