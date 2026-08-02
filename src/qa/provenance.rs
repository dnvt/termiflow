use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use super::common;

pub const PROVENANCE_SCHEMA: &str = "termiflow.source_provenance.v1";

pub struct ProvenanceInputs<'a> {
    pub input_root: &'a Path,
    pub metadata_path: &'a Path,
    pub metadata_bytes: &'a [u8],
    pub input_paths: &'a BTreeMap<String, PathBuf>,
    pub styles: &'a [String],
    pub modes: &'a [String],
    pub display_profile: &'a str,
}

pub fn enrich_identity(
    root: &Path,
    binary: &Path,
    base_identity: &Value,
    inputs: &ProvenanceInputs<'_>,
) -> Result<Value> {
    let tracked_paths = git_paths(root, &["ls-files", "-z"])?;
    let untracked_paths = git_paths(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    let tracked_tree_sha256 = digest_file_set(root, &tracked_paths)?;
    let untracked_files_sha256 = digest_file_set(root, &untracked_paths)?;
    let tracked_diff_sha256 =
        common::sha256_bytes(&git_bytes(root, &["diff", "--binary", "HEAD", "--"])?);
    let source_state = json!({
        "tracked_tree_sha256": tracked_tree_sha256,
        "tracked_diff_sha256": tracked_diff_sha256,
        "untracked_files_sha256": untracked_files_sha256,
        "untracked_paths": untracked_paths,
        "tracked_file_count": tracked_paths.len(),
        "untracked_file_count": untracked_paths.len(),
    });

    let cargo_metadata = command_bytes(
        root,
        "cargo",
        &["metadata", "--locked", "--format-version", "1"],
    )?;
    let cargo_metadata_sha256 = common::sha256_bytes(&cargo_metadata);
    let rustc_verbose = command_bytes(root, "rustc", &["-Vv"])?;
    let features = if cfg!(feature = "qa") {
        vec!["qa"]
    } else {
        Vec::new()
    };
    let build = json!({
        "cargo_manifest_sha256": file_sha256(&root.join("Cargo.toml"))?,
        "cargo_lock_sha256": file_sha256(&root.join("Cargo.lock"))?,
        "cargo_metadata_sha256": cargo_metadata_sha256,
        "rust_toolchain_sha256": first_file_sha256(root, &["rust-toolchain.toml", "rust-toolchain"])?,
        "cargo_config_sha256": optional_file_sha256(&root.join(".cargo/config.toml"))?,
        "rustc_verbose_sha256": common::sha256_bytes(&rustc_verbose),
        "target": base_identity["target"],
        "profile": std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_owned()),
        "features": features,
        "binary_sha256": common::sha256_file(binary)?,
    });

    let input_digest = digest_named_files(root, inputs.input_paths)?;
    let fixture_spec = root.join("tests/fixtures/fixture_spec.json");
    let baseline = root.join("tests/fixtures/quality_baseline.json");
    let workload = json!({
        "input_root": common::relative_to_root(inputs.input_root, root),
        "metadata_path": common::relative_to_root(inputs.metadata_path, root),
        "metadata_sha256": common::sha256_bytes(inputs.metadata_bytes),
        "inputs_sha256": input_digest,
        "fixture_spec_sha256": file_sha256(&fixture_spec)?,
        "baseline_sha256": file_sha256(&baseline)?,
        "styles": inputs.styles,
        "modes": inputs.modes,
        "display_profile": inputs.display_profile,
    });

    let host = json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "uname_srm": command_text(root, "uname", &["-srm"])?,
    });
    let mut provenance = json!({
        "schema": PROVENANCE_SCHEMA,
        "source": source_state,
        "build": build,
        "workload": workload,
        "host": host,
    });
    let effective_sha256 = common::sha256_bytes(&serde_json::to_vec(&provenance)?);
    provenance["effective_sha256"] = Value::String(effective_sha256);

    let mut identity = base_identity.clone();
    identity["provenance"] = provenance;
    Ok(identity)
}

pub fn validate_identity(identity: &Value) -> Result<()> {
    let provenance = identity
        .get("provenance")
        .ok_or_else(|| anyhow!("identity.json.provenance is required"))?;
    let object = required_object(Some(provenance), "identity.json.provenance")?;
    if object.get("schema").and_then(Value::as_str) != Some(PROVENANCE_SCHEMA) {
        bail!("identity.json.provenance.schema must be {PROVENANCE_SCHEMA}");
    }
    let effective_sha256 = required_hash(
        object.get("effective_sha256"),
        "identity.json.provenance.effective_sha256",
    )?;
    let mut unsigned = provenance.clone();
    let Some(unsigned) = unsigned.as_object_mut() else {
        bail!("identity.json.provenance must remain an object after cloning");
    };
    unsigned.remove("effective_sha256");
    let actual_sha256 = common::sha256_bytes(&serde_json::to_vec(&unsigned)?);
    if effective_sha256 != actual_sha256 {
        bail!("identity.json.provenance.effective_sha256 does not match its fields");
    }

    let source = required_object(object.get("source"), "identity.json.provenance.source")?;
    for field in [
        "tracked_tree_sha256",
        "tracked_diff_sha256",
        "untracked_files_sha256",
    ] {
        required_hash(
            source.get(field),
            &format!("identity.json.provenance.source.{field}"),
        )?;
    }
    let untracked_paths = required_string_array(
        source.get("untracked_paths"),
        "identity.json.provenance.source.untracked_paths",
    )?;
    required_count(
        source.get("tracked_file_count"),
        "identity.json.provenance.source.tracked_file_count",
    )?;
    let untracked_count = required_count(
        source.get("untracked_file_count"),
        "identity.json.provenance.source.untracked_file_count",
    )?;
    if untracked_count != untracked_paths.len() as u64 {
        bail!("identity.json.provenance.source.untracked_file_count does not match paths");
    }

    let build = required_object(object.get("build"), "identity.json.provenance.build")?;
    for field in [
        "cargo_manifest_sha256",
        "cargo_lock_sha256",
        "cargo_metadata_sha256",
        "rustc_verbose_sha256",
        "binary_sha256",
    ] {
        required_hash(
            build.get(field),
            &format!("identity.json.provenance.build.{field}"),
        )?;
    }
    optional_hash(
        build.get("rust_toolchain_sha256"),
        "identity.json.provenance.build.rust_toolchain_sha256",
    )?;
    optional_hash(
        build.get("cargo_config_sha256"),
        "identity.json.provenance.build.cargo_config_sha256",
    )?;
    required_string(build.get("target"), "identity.json.provenance.build.target")?;
    required_string(
        build.get("profile"),
        "identity.json.provenance.build.profile",
    )?;
    required_string_array(
        build.get("features"),
        "identity.json.provenance.build.features",
    )?;

    let workload = required_object(object.get("workload"), "identity.json.provenance.workload")?;
    for field in [
        "metadata_sha256",
        "inputs_sha256",
        "fixture_spec_sha256",
        "baseline_sha256",
    ] {
        required_hash(
            workload.get(field),
            &format!("identity.json.provenance.workload.{field}"),
        )?;
    }
    for field in ["input_root", "metadata_path", "display_profile"] {
        required_string(
            workload.get(field),
            &format!("identity.json.provenance.workload.{field}"),
        )?;
    }
    required_string_array(
        workload.get("styles"),
        "identity.json.provenance.workload.styles",
    )?;
    required_string_array(
        workload.get("modes"),
        "identity.json.provenance.workload.modes",
    )?;

    let host = required_object(object.get("host"), "identity.json.provenance.host")?;
    for field in ["os", "arch", "uname_srm"] {
        required_string(
            host.get(field),
            &format!("identity.json.provenance.host.{field}"),
        )?;
    }
    Ok(())
}

fn required_object<'a>(
    value: Option<&'a Value>,
    label: &str,
) -> Result<&'a serde_json::Map<String, Value>> {
    value
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("{label} must be an object"))
}

fn required_string<'a>(value: Option<&'a Value>, label: &str) -> Result<&'a str> {
    value
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| anyhow!("{label} must be a non-empty string"))
}

fn required_hash<'a>(value: Option<&'a Value>, label: &str) -> Result<&'a str> {
    let hash = required_string(value, label)?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("{label} must be a 64-character hexadecimal SHA-256");
    }
    Ok(hash)
}

fn optional_hash(value: Option<&Value>, label: &str) -> Result<()> {
    if value.is_some_and(|value| !value.is_null()) {
        required_hash(value, label)?;
    }
    Ok(())
}

fn required_count(value: Option<&Value>, label: &str) -> Result<u64> {
    value
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("{label} must be a non-negative integer"))
}

fn required_string_array<'a>(value: Option<&'a Value>, label: &str) -> Result<&'a [Value]> {
    let values = value
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("{label} must be a string list"))?;
    if !values.iter().all(Value::is_string) {
        bail!("{label} must be a string list");
    }
    Ok(values)
}

fn git_paths(root: &Path, args: &[&str]) -> Result<Vec<String>> {
    let bytes = git_bytes(root, args)?;
    let mut paths = String::from_utf8(bytes)
        .context("git path listing is not UTF-8")?
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn git_bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    command_bytes(root, "git", args)
}

fn command_bytes(root: &Path, program: &str, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .with_context(|| format!("run {program} {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "{program} {} failed with status {}: {}",
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}

fn command_text(root: &Path, program: &str, args: &[&str]) -> Result<String> {
    String::from_utf8(command_bytes(root, program, args)?)
        .map(|text| text.trim().to_owned())
        .with_context(|| format!("{program} output is not UTF-8"))
}

fn digest_file_set(root: &Path, paths: &[String]) -> Result<String> {
    let mut encoded = Vec::new();
    for relative in paths {
        let path = root.join(relative);
        let bytes = fs::read(&path)
            .with_context(|| format!("read source identity file {}", path.display()))?;
        encoded.extend_from_slice(relative.as_bytes());
        encoded.push(0);
        encoded.extend_from_slice(&bytes);
        encoded.push(0);
    }
    Ok(common::sha256_bytes(&encoded))
}

fn digest_named_files(root: &Path, paths: &BTreeMap<String, PathBuf>) -> Result<String> {
    let mut encoded = Vec::new();
    for (name, path) in paths {
        let bytes =
            fs::read(path).with_context(|| format!("read workload input {}", path.display()))?;
        encoded.extend_from_slice(name.as_bytes());
        encoded.push(0);
        encoded.extend_from_slice(common::relative_to_root(path, root).as_bytes());
        encoded.push(0);
        encoded.extend_from_slice(&bytes);
        encoded.push(0);
    }
    Ok(common::sha256_bytes(&encoded))
}

fn file_sha256(path: &Path) -> Result<String> {
    if !path.is_file() {
        bail!("required provenance file is missing: {}", path.display());
    }
    common::sha256_file(path)
}

fn optional_file_sha256(path: &Path) -> Result<Option<String>> {
    path.is_file()
        .then(|| common::sha256_file(path))
        .transpose()
}

fn first_file_sha256(root: &Path, candidates: &[&str]) -> Result<Option<String>> {
    for candidate in candidates {
        let path = root.join(candidate);
        if path.is_file() {
            return Ok(Some(common::sha256_file(&path)?));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("termiflow-provenance-{}", common::now_label()));
        fs::create_dir_all(&root).expect("create provenance temp root");
        root
    }

    #[test]
    fn file_set_digest_binds_path_and_content() {
        let root = temp_root();
        fs::write(root.join("one.txt"), b"one").expect("write first file");
        fs::write(root.join("two.txt"), b"two").expect("write second file");
        let first = digest_file_set(&root, &["one.txt".to_owned(), "two.txt".to_owned()])
            .expect("digest initial set");
        fs::write(root.join("two.txt"), b"changed").expect("change second file");
        let changed = digest_file_set(&root, &["one.txt".to_owned(), "two.txt".to_owned()])
            .expect("digest changed set");
        assert_ne!(first, changed);
        fs::remove_dir_all(root).expect("remove provenance temp root");
    }

    #[test]
    fn named_workload_digest_binds_fixture_name() {
        let root = temp_root();
        let input = root.join("fixture.md");
        fs::write(&input, b"graph TD; A-->B").expect("write fixture");
        let mut first = BTreeMap::new();
        first.insert("fixture_a".to_owned(), input.clone());
        let mut second = BTreeMap::new();
        second.insert("fixture_b".to_owned(), input);
        assert_ne!(
            digest_named_files(&root, &first).expect("digest first workload"),
            digest_named_files(&root, &second).expect("digest second workload")
        );
        fs::remove_dir_all(root).expect("remove provenance temp root");
    }

    #[test]
    fn validation_rejects_tampered_effective_digest() {
        let hash = "a".repeat(64);
        let mut provenance = json!({
            "schema": PROVENANCE_SCHEMA,
            "source": {
                "tracked_tree_sha256": hash,
                "tracked_diff_sha256": hash,
                "untracked_files_sha256": hash,
                "untracked_paths": [],
                "tracked_file_count": 0,
                "untracked_file_count": 0
            },
            "build": {
                "cargo_manifest_sha256": hash,
                "cargo_lock_sha256": hash,
                "cargo_metadata_sha256": hash,
                "rust_toolchain_sha256": hash,
                "cargo_config_sha256": null,
                "rustc_verbose_sha256": hash,
                "target": "test-target",
                "profile": "debug",
                "features": [],
                "binary_sha256": hash
            },
            "workload": {
                "input_root": "inputs",
                "metadata_path": "metadata.json",
                "metadata_sha256": hash,
                "inputs_sha256": hash,
                "fixture_spec_sha256": hash,
                "baseline_sha256": hash,
                "styles": ["ascii"],
                "modes": ["default"],
                "display_profile": "test"
            },
            "host": {
                "os": "test",
                "arch": "test",
                "uname_srm": "test"
            }
        });
        let effective_sha256 = common::sha256_bytes(
            &serde_json::to_vec(&provenance).expect("serialize unsigned provenance"),
        );
        provenance["effective_sha256"] = Value::String(effective_sha256);
        let mut identity = json!({ "provenance": provenance });
        validate_identity(&identity).expect("valid provenance identity");
        identity["provenance"]["source"]["tracked_tree_sha256"] = Value::String("b".repeat(64));
        assert!(validate_identity(&identity).is_err());
    }
}
