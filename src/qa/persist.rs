use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{Map, Value};

static NAME_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PersistenceError {
    Conflict { path: PathBuf, detail: String },
    Unsupported { path: PathBuf, detail: String },
    Incomplete { path: PathBuf, detail: String },
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
    fn replace(&self, staged: &Path, target: &Path) -> io::Result<()>;
    fn append(&self, path: &Path, content: &[u8]) -> io::Result<()>;
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

    fn replace(&self, staged: &Path, target: &Path) -> io::Result<()> {
        fs::rename(staged, target)
    }

    fn append(&self, path: &Path, content: &[u8]) -> io::Result<()> {
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        file.write_all(content)?;
        file.sync_all()
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }
}

fn sequence() -> u64 {
    NAME_SEQUENCE.fetch_add(1, Ordering::Relaxed)
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

fn guard_path(path: &Path, purpose: &str) -> Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("target has no parent: {}", path.display()))?;
    let name = path
        .file_name()
        .ok_or_else(|| anyhow!("target has no file name: {}", path.display()))?
        .to_string_lossy();
    Ok(parent.join(format!(".{name}.termiflow-{purpose}.lock")))
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    Ok(())
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

pub(crate) fn claim_directory(path: &Path) -> Result<PathBuf> {
    ensure_parent(path)?;
    match SystemOps.claim_dir(path) {
        Ok(()) => Ok(path.to_path_buf()),
        Err(error) => Err(classify_claim_error(path, error)),
    }
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
        Replace,
        Append,
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

        fn replace(&self, staged: &Path, target: &Path) -> io::Result<()> {
            self.fail_once(Failure::Replace)?;
            SystemOps.replace(staged, target)
        }

        fn append(&self, path: &Path, content: &[u8]) -> io::Result<()> {
            self.fail_once(Failure::Append)?;
            SystemOps.append(path, content)
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
