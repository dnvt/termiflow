use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use super::common;

pub const PROVENANCE_SCHEMA: &str = "termiflow.source_provenance.v1";
pub const POLICY_SET_SCHEMA: &str = "termiflow.effective_policy_set.v1";

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

    if let Some(policy_set) = object.get("policy_set") {
        validate_policy_set(policy_set)?;
    }

    let host = required_object(object.get("host"), "identity.json.provenance.host")?;
    for field in ["os", "arch", "uname_srm"] {
        required_string(
            host.get(field),
            &format!("identity.json.provenance.host.{field}"),
        )?;
    }
    Ok(())
}

/// Bind the effective per-row render policies observed while a packet is
/// being built. Legacy packets may omit this record; new packets always write
/// it before the packet becomes publishable.
pub fn bind_policy_observations(identity: &mut Value, observations: &mut Vec<Value>) -> Result<()> {
    for record in observations.iter() {
        let policy = record
            .get("policy")
            .ok_or_else(|| anyhow!("policy observation is missing policy"))?;
        validate_policy(policy)?;
        required_string(record.get("case_id"), "policy observation.case_id")?;
        required_hash(
            record.get("policy_sha256"),
            "policy observation.policy_sha256",
        )?;
    }
    observations.sort_by_key(|record| {
        (
            record["case_id"].as_str().unwrap_or_default().to_owned(),
            record["policy_sha256"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        )
    });
    observations.dedup_by(|left, right| left == right);
    for pair in observations.windows(2) {
        if pair[0]["case_id"] == pair[1]["case_id"]
            && pair[0]["policy_sha256"] != pair[1]["policy_sha256"]
        {
            bail!(
                "case {} observed more than one effective policy",
                pair[0]["case_id"].as_str().unwrap_or_default()
            );
        }
    }
    let unsigned_set = json!({
        "schema": POLICY_SET_SCHEMA,
        "version": 1,
        "records": observations,
    });
    let set_sha256 = common::sha256_bytes(&serde_json::to_vec(
        &termiflow::config::canonical_json(&unsigned_set),
    )?);
    let policy_set = json!({
        "schema": POLICY_SET_SCHEMA,
        "version": 1,
        "records": observations,
        "sha256": set_sha256,
    });
    let provenance = identity
        .get_mut("provenance")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| anyhow!("identity.json.provenance must be an object"))?;
    provenance.insert("policy_set".to_owned(), policy_set);
    let unsigned = Value::Object(provenance.clone());
    provenance.insert(
        "effective_sha256".to_owned(),
        Value::String(common::sha256_bytes(&serde_json::to_vec(
            &unsigned_without_effective(&unsigned),
        )?)),
    );
    Ok(())
}

fn unsigned_without_effective(value: &Value) -> Value {
    let mut unsigned = value.clone();
    if let Some(object) = unsigned.as_object_mut() {
        object.remove("effective_sha256");
    }
    unsigned
}

pub fn validate_policy_set(value: &Value) -> Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("identity.json.provenance.policy_set must be an object"))?;
    if object.get("schema").and_then(Value::as_str) != Some(POLICY_SET_SCHEMA)
        || object.get("version").and_then(Value::as_u64) != Some(1)
    {
        bail!("identity.json.provenance.policy_set schema/version is invalid");
    }
    let records = object
        .get("records")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("identity.json.provenance.policy_set.records must be an array"))?;
    for record in records {
        let record_object = record
            .as_object()
            .ok_or_else(|| anyhow!("policy-set records must be objects"))?;
        required_string(
            record_object.get("case_id"),
            "identity.json.provenance.policy_set.records.case_id",
        )?;
        let policy = record_object
            .get("policy")
            .ok_or_else(|| anyhow!("policy-set record is missing policy"))?;
        validate_policy(policy)?;
        let declared = required_hash(
            record_object.get("policy_sha256"),
            "identity.json.provenance.policy_set.records.policy_sha256",
        )?;
        let actual = policy
            .get("sha256")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("policy-set record policy.sha256 is missing"))?;
        if declared != actual {
            bail!("policy-set record digest does not match policy.sha256");
        }
    }
    for pair in records.windows(2) {
        let left_case = pair[0]["case_id"].as_str().unwrap_or_default();
        let right_case = pair[1]["case_id"].as_str().unwrap_or_default();
        if left_case >= right_case {
            bail!("policy-set records must be sorted by unique case_id");
        }
    }
    let unsigned = json!({
        "schema": POLICY_SET_SCHEMA,
        "version": 1,
        "records": records,
    });
    let expected = common::sha256_bytes(&serde_json::to_vec(&termiflow::config::canonical_json(
        &unsigned,
    ))?);
    if object.get("sha256").and_then(Value::as_str) != Some(expected.as_str()) {
        bail!("identity.json.provenance.policy_set.sha256 does not match its records");
    }
    Ok(())
}

pub fn validate_policy(value: &Value) -> Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("effective render policy must be an object"))?;
    if object.get("schema").and_then(Value::as_str)
        != Some(termiflow::config::EFFECTIVE_POLICY_SCHEMA)
        || object.get("version").and_then(Value::as_u64) != Some(1)
    {
        bail!("effective render policy schema/version is invalid");
    }
    exact_keys(
        object,
        &["schema", "version", "fields", "sha256"],
        "effective render policy",
    )?;
    let fields = object
        .get("fields")
        .ok_or_else(|| anyhow!("effective render policy.fields is required"))?;
    let fields_object = fields
        .as_object()
        .ok_or_else(|| anyhow!("effective render policy.fields must be an object"))?;
    exact_keys(
        fields_object,
        &[
            "config",
            "runtime",
            "boundary",
            "environment",
            "contract_fields",
        ],
        "effective render policy.fields",
    )?;
    validate_policy_fields(fields_object)?;
    let contract_fields = fields_object
        .get("contract_fields")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("effective policy.contract_fields must be a string array"))?;
    let expected_contract_fields = termiflow::config::EFFECTIVE_POLICY_CONTRACT_FIELDS
        .iter()
        .map(|field| Value::String((*field).to_owned()))
        .collect::<Vec<_>>();
    if *contract_fields != expected_contract_fields {
        bail!("effective policy.contract_fields drift from code-owned list");
    }
    let expected = termiflow::config::policy_digest(fields);
    if object.get("sha256").and_then(Value::as_str) != Some(expected.as_str()) {
        bail!("effective render policy.sha256 does not match fields");
    }
    Ok(())
}

fn validate_policy_fields(fields: &serde_json::Map<String, Value>) -> Result<()> {
    let config = required_object(fields.get("config"), "effective policy.config")?;
    exact_keys(
        config,
        &[
            "max_label_width",
            "max_edge_label_width",
            "wrap_labels",
            "max_label_lines",
            "crop",
            "pad",
            "strict_parsing",
            "composite_style",
            "spacing",
            "optimize_render",
            "render_repair_passes",
            "layout_repair_passes",
            "debug_critic",
        ],
        "effective policy.config",
    )?;
    for field in [
        "max_label_width",
        "max_edge_label_width",
        "max_label_lines",
        "pad",
        "render_repair_passes",
        "layout_repair_passes",
    ] {
        if !config[field].is_u64() {
            bail!("effective policy.config.{field} must be a non-negative integer");
        }
    }
    for field in [
        "wrap_labels",
        "crop",
        "strict_parsing",
        "optimize_render",
        "debug_critic",
    ] {
        if !config[field].is_boolean() {
            bail!("effective policy.config.{field} must be boolean");
        }
    }
    let composite = required_object(
        config.get("composite_style"),
        "effective policy.config.composite_style",
    )?;
    exact_keys(
        composite,
        &[
            "corner", "border", "arrow", "edge", "junction", "back", "subgraph", "fallback",
        ],
        "effective policy.config.composite_style",
    )?;
    if composite["fallback"] != Value::String("Unicode".to_owned()) {
        bail!("effective policy.config.composite_style.fallback is invalid");
    }
    for field in [
        "corner", "border", "arrow", "edge", "junction", "back", "subgraph",
    ] {
        if !composite[field].is_string() && !composite[field].is_null() {
            bail!("effective policy.config.composite_style.{field} must be string or null");
        }
    }
    let spacing = required_object(config.get("spacing"), "effective policy.config.spacing")?;
    exact_keys(
        spacing,
        &[
            "box_height",
            "box_min_width",
            "box_padding",
            "row_spacing",
            "col_spacing",
            "node_margin",
            "subgraph_gutter",
            "stem_length_vertical",
            "stem_length_horizontal",
            "edge_junction_height",
            "edge_drop_height",
            "max_label_width",
            "max_canvas_width",
            "max_canvas_height",
            "cycle_gutter",
        ],
        "effective policy.config.spacing",
    )?;
    if spacing.values().any(|value| !value.is_u64()) {
        bail!("effective policy.config.spacing must contain only non-negative integers");
    }

    let runtime = required_object(fields.get("runtime"), "effective policy.runtime")?;
    exact_keys(
        runtime,
        &["compatibility", "diagnostics", "terminal"],
        "effective policy.runtime",
    )?;
    let compatibility = required_object(
        runtime.get("compatibility"),
        "effective policy.runtime.compatibility",
    )?;
    exact_keys(
        compatibility,
        &[
            "optimize_render",
            "disable_portals",
            "render_repair_passes",
            "layout_repair_passes",
        ],
        "effective policy.runtime.compatibility",
    )?;
    for field in ["optimize_render", "disable_portals"] {
        if !compatibility[field].is_boolean() {
            bail!("effective policy.runtime.compatibility.{field} must be boolean");
        }
    }
    for field in ["render_repair_passes", "layout_repair_passes"] {
        if !compatibility[field].is_u64() && !compatibility[field].is_null() {
            bail!("effective policy.runtime.compatibility.{field} must be integer or null");
        }
    }
    let diagnostics = required_object(
        runtime.get("diagnostics"),
        "effective policy.runtime.diagnostics",
    )?;
    exact_keys(
        diagnostics,
        &[
            "timing", "routes", "fan_in", "fan_out", "cross", "crossing", "critic",
        ],
        "effective policy.runtime.diagnostics",
    )?;
    if diagnostics.values().any(|value| !value.is_boolean()) {
        bail!("effective policy.runtime.diagnostics must contain only booleans");
    }
    let terminal = required_object(runtime.get("terminal"), "effective policy.runtime.terminal")?;
    exact_keys(
        terminal,
        &["columns", "lines"],
        "effective policy.runtime.terminal",
    )?;
    if terminal
        .values()
        .any(|value| !value.is_u64() && !value.is_null())
    {
        bail!("effective policy.runtime.terminal values must be integer or null");
    }

    let boundary = required_object(fields.get("boundary"), "effective policy.boundary")?;
    exact_keys(
        boundary,
        &[
            "direction",
            "display_profile",
            "scaling_mode",
            "from_json",
            "fit_terminal",
        ],
        "effective policy.boundary",
    )?;
    for field in ["direction", "display_profile", "scaling_mode"] {
        required_string(
            boundary.get(field),
            &format!("effective policy.boundary.{field}"),
        )?;
    }
    for field in ["from_json", "fit_terminal"] {
        if !boundary[field].is_boolean() {
            bail!("effective policy.boundary.{field} must be boolean");
        }
    }

    let environment = required_object(fields.get("environment"), "effective policy.environment")?;
    exact_keys(
        environment,
        &["TERM", "LANG", "LC_ALL"],
        "effective policy.environment",
    )?;
    if environment
        .values()
        .any(|value| !value.is_string() && !value.is_null())
    {
        bail!("effective policy.environment values must be string or null");
    }
    Ok(())
}

fn exact_keys(
    object: &serde_json::Map<String, Value>,
    expected: &[&str],
    label: &str,
) -> Result<()> {
    let expected = expected
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let actual = object
        .keys()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    if actual != expected {
        let missing = expected.difference(&actual).copied().collect::<Vec<_>>();
        let unknown = actual.difference(&expected).copied().collect::<Vec<_>>();
        bail!("{label} keys drifted; missing={missing:?} unknown={unknown:?}");
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

    #[test]
    fn effective_policy_validation_is_strict_and_canonical() {
        let policy = termiflow::config::effective_render_policy(
            &termiflow::Config::default(),
            termiflow::graph::Direction::TD,
            "test-display",
            "Fixed",
            false,
            false,
        );
        validate_policy(&policy).expect("default effective policy is valid");

        let fields = policy["fields"].clone();
        let mut reordered = serde_json::Map::new();
        for key in [
            "environment",
            "contract_fields",
            "boundary",
            "config",
            "runtime",
        ] {
            reordered.insert(key.to_owned(), fields[key].clone());
        }
        assert_eq!(
            termiflow::config::policy_digest(&fields),
            termiflow::config::policy_digest(&Value::Object(reordered))
        );

        let mut unknown = policy.clone();
        unknown["fields"]["unknown"] = Value::Bool(true);
        assert!(validate_policy(&unknown).is_err());
    }
}
