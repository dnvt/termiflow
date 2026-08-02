use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Map, Value};

use super::common;

pub const SPEC_SCHEMA: &str = "termiflow.fixture_spec.v1";
pub const MANIFEST_SCHEMA: &str = "termiflow.fixture_manifest.v1";
const CHECK_SCHEMA: &str = "termiflow.fixture_spec_check.v1";
const SPEC_VERSION: i64 = 1;
const HOLDOUTS: &[&str] = &["none", "shared", "evaluator_owned"];
const SEVERITIES: &[&str] = &["P0", "P1", "P2", "P3"];
const DIMENSIONS: &[&str] = &["semantic", "containment", "route", "text", "readability"];
const REVIEW_CLASSES: &[&str] = &[
    "semantic_mismatch",
    "illegal_overwrite",
    "border_portal_contact",
    "missing_arrow_shaft",
    "arrow_direction",
    "junction_topology",
    "title_alignment",
    "clipping",
    "spacing",
    "glyph_style",
    "unicode_width",
];

#[derive(Debug)]
pub struct SpecArgs {
    pub spec: PathBuf,
    pub check: bool,
    pub emit_manifest: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct ValidatedCase {
    id: String,
    family: String,
    semantic: Value,
    homologs: Vec<String>,
    variants: Vec<ValidatedVariant>,
}

#[derive(Debug, Clone)]
struct ValidatedVariant {
    id: String,
    direction: String,
    input_path: Option<String>,
    source: String,
    source_sha256: String,
    golden_stem: Option<String>,
    styles: Vec<String>,
    modes: Vec<String>,
    kind: String,
    stderr_policy: String,
    stderr_contains: Vec<String>,
    holdout: String,
    review_targets: Value,
}

#[derive(Debug)]
struct ValidatedSpec {
    normalized_bytes: Vec<u8>,
    cases: Vec<ValidatedCase>,
    reviewable_rows: usize,
    negative_cases: usize,
    holdout_variants: usize,
}

pub fn run(args: SpecArgs) -> Result<()> {
    if args.check == args.emit_manifest.is_some() {
        bail!("choose exactly one of --check or --emit-manifest");
    }

    let root = std::env::current_dir().context("resolve repository root")?;
    let spec_path = resolve(&root, &args.spec);
    let raw = common::require_file(&spec_path, "fixture spec")?;
    let document: Value = serde_json::from_slice(&raw)
        .with_context(|| format!("parse fixture spec JSON: {}", spec_path.display()))?;
    let validated = validate(&root, &document)?;
    let spec_sha256 = common::sha256_bytes(&validated.normalized_bytes);
    let manifest = build_manifest(&validated, &spec_sha256)?;

    if let Some(path) = args.emit_manifest {
        let output_path = resolve(&root, &path);
        if output_path.canonicalize().ok() == spec_path.canonicalize().ok() {
            bail!("manifest output must not overwrite the fixture spec");
        }
        let bytes = json_bytes(&manifest)?;
        atomic_write(&output_path, &bytes)?;
        println!(
            "{}",
            check_summary(&spec_sha256, &validated, Some(&output_path))
        );
    } else {
        println!("{}", check_summary(&spec_sha256, &validated, None));
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

fn validate(root: &Path, document: &Value) -> Result<ValidatedSpec> {
    let object = object(document, "fixture spec")?;
    allowed_keys(object, &["schema", "spec_version", "cases"], "fixture spec")?;
    if required_string(object, "schema", "fixture spec")? != SPEC_SCHEMA {
        bail!("fixture spec schema must be {SPEC_SCHEMA}");
    }
    if required_i64(object, "spec_version", "fixture spec")? != SPEC_VERSION {
        bail!("fixture spec spec_version must be {SPEC_VERSION}");
    }

    let case_values = required_array(object, "cases", "fixture spec")?;
    if case_values.is_empty() {
        bail!("fixture spec cases must be non-empty");
    }

    let mut case_ids = BTreeSet::new();
    let mut cases = Vec::with_capacity(case_values.len());
    for value in case_values {
        let case = validate_case(root, value)?;
        if !case_ids.insert(case.id.clone()) {
            bail!("duplicate fixture spec case id: {}", case.id);
        }
        cases.push(case);
    }

    for case in &cases {
        for homolog in &case.homologs {
            if !case_ids.contains(homolog) {
                bail!(
                    "case {} references unknown homolog case {}",
                    case.id,
                    homolog
                );
            }
        }
    }

    let mut directions = BTreeSet::new();
    let mut styles = BTreeSet::new();
    let mut modes = BTreeSet::new();
    let mut golden_stems = BTreeSet::new();
    let mut reviewable_rows = 0;
    let mut negative_cases = 0;
    let mut holdout_variants = 0;
    for case in &cases {
        for variant in &case.variants {
            if variant.kind == "success" && variant.holdout != "evaluator_owned" {
                let stem = variant
                    .golden_stem
                    .as_deref()
                    .ok_or_else(|| anyhow!("variant {} is missing golden_stem", variant.id))?;
                if !golden_stems.insert(stem) {
                    bail!("duplicate golden_stem: {stem}");
                }
            }
            if variant.holdout == "evaluator_owned" {
                holdout_variants += 1;
                continue;
            }
            if variant.kind == "success" {
                directions.insert(variant.direction.as_str());
                styles.extend(variant.styles.iter().map(String::as_str));
                modes.extend(variant.modes.iter().map(String::as_str));
                reviewable_rows += variant.styles.len() * variant.modes.len();
            } else {
                negative_cases += 1;
            }
        }
    }

    for direction in ["TD", "LR", "BT", "RL"] {
        if !directions.contains(direction) {
            bail!("fixture spec success canary is missing direction {direction}");
        }
    }
    for style in ["ascii", "unicode"] {
        if !styles.contains(style) {
            bail!("fixture spec success canary is missing style {style}");
        }
    }
    for mode in ["default", "optimized"] {
        if !modes.contains(mode) {
            bail!("fixture spec success canary is missing mode {mode}");
        }
    }
    if reviewable_rows != 16 {
        bail!("fixture spec canary must emit exactly 16 reviewable rows, got {reviewable_rows}");
    }
    if negative_cases == 0 {
        bail!("fixture spec must contain at least one warning or expected-error variant");
    }

    let normalized_bytes = json_bytes(&normalize(document))?;
    Ok(ValidatedSpec {
        normalized_bytes,
        cases,
        reviewable_rows,
        negative_cases,
        holdout_variants,
    })
}

fn validate_case(root: &Path, value: &Value) -> Result<ValidatedCase> {
    let fields = object(value, "fixture spec case")?;
    allowed_keys(
        fields,
        &["id", "family", "semantic", "homologs", "variants"],
        "fixture spec case",
    )?;
    let id = required_id(fields, "id", "fixture spec case")?;
    let family = non_empty_string(fields, "family", "fixture spec case")?;
    let semantic = validate_semantic(fields.get("semantic"), &id)?;
    let homologs = string_list(fields, "homologs", &format!("case {id}"), true)?;
    let variant_values = required_array(fields, "variants", &format!("case {id}"))?;
    if variant_values.is_empty() {
        bail!("case {id} variants must be non-empty");
    }

    let mut variant_ids = BTreeSet::new();
    let mut variants = Vec::with_capacity(variant_values.len());
    for value in variant_values {
        let variant = validate_variant(root, value, &id, &semantic)?;
        if !variant_ids.insert(variant.id.clone()) {
            bail!("case {id} has duplicate variant id {}", variant.id);
        }
        variants.push(variant);
    }

    Ok(ValidatedCase {
        id,
        family,
        semantic,
        homologs,
        variants,
    })
}

fn validate_semantic(value: Option<&Value>, case_id: &str) -> Result<Value> {
    let semantic_object = object(
        value.ok_or_else(|| anyhow!("case {case_id} is missing semantic"))?,
        "semantic",
    )?;
    allowed_keys(
        semantic_object,
        &["nodes", "edges", "subgraphs", "labels"],
        "semantic",
    )?;
    let nodes = string_list(
        semantic_object,
        "nodes",
        &format!("case {case_id} semantic"),
        true,
    )?;
    let node_set: BTreeSet<&str> = nodes.iter().map(String::as_str).collect();

    let edges = required_array(
        semantic_object,
        "edges",
        &format!("case {case_id} semantic"),
    )?;
    if edges.is_empty() {
        bail!("case {case_id} semantic edges must be non-empty");
    }
    let mut normalized_edges = Vec::with_capacity(edges.len());
    let mut edge_keys = BTreeSet::new();
    for edge in edges {
        let edge_object = object(edge, &format!("case {case_id} semantic edge"))?;
        allowed_keys(edge_object, &["from", "to"], "semantic edge")?;
        let from = non_empty_string(edge_object, "from", "semantic edge")?;
        let to = non_empty_string(edge_object, "to", "semantic edge")?;
        if !node_set.contains(from.as_str()) || !node_set.contains(to.as_str()) {
            bail!("case {case_id} semantic edge references an unknown node");
        }
        if !edge_keys.insert((from.clone(), to.clone())) {
            bail!("case {case_id} semantic edges contain a duplicate");
        }
        normalized_edges.push(json!({"from": from, "to": to}));
    }

    let subgraphs = required_array(
        semantic_object,
        "subgraphs",
        &format!("case {case_id} semantic"),
    )?;
    let mut subgraph_ids = BTreeSet::new();
    let mut normalized_subgraphs = Vec::with_capacity(subgraphs.len());
    for subgraph in subgraphs {
        let subgraph_object = object(subgraph, &format!("case {case_id} semantic subgraph"))?;
        allowed_keys(subgraph_object, &["id", "members"], "semantic subgraph")?;
        let id = required_id(subgraph_object, "id", "semantic subgraph")?;
        if !subgraph_ids.insert(id.clone()) {
            bail!("case {case_id} semantic has duplicate subgraph {id}");
        }
        let members = string_list(subgraph_object, "members", &format!("subgraph {id}"), true)?;
        if members
            .iter()
            .any(|member| !node_set.contains(member.as_str()))
        {
            bail!("case {case_id} subgraph {id} references an unknown node");
        }
        normalized_subgraphs.push(json!({"id": id, "members": members}));
    }

    let labels = object(
        semantic_object
            .get("labels")
            .ok_or_else(|| anyhow!("case {case_id} semantic is missing labels"))?,
        &format!("case {case_id} semantic labels"),
    )?;
    let mut normalized_labels = Map::new();
    for (node, label) in labels {
        if !node_set.contains(node.as_str()) {
            bail!("case {case_id} semantic label references unknown node {node}");
        }
        let label = label
            .as_str()
            .filter(|label| !label.is_empty())
            .ok_or_else(|| anyhow!("case {case_id} label {node} must be non-empty string"))?;
        normalized_labels.insert(node.clone(), Value::String(label.to_owned()));
    }
    if normalized_labels.len() != nodes.len() {
        bail!("case {case_id} semantic labels must cover every node");
    }

    Ok(json!({
        "nodes": nodes,
        "edges": normalized_edges,
        "subgraphs": normalized_subgraphs,
        "labels": normalized_labels,
    }))
}

fn validate_variant(
    root: &Path,
    value: &Value,
    case_id: &str,
    _semantic: &Value,
) -> Result<ValidatedVariant> {
    let fields = object(value, &format!("case {case_id} variant"))?;
    allowed_keys(
        fields,
        &[
            "id",
            "direction",
            "source",
            "input_path",
            "golden_stem",
            "styles",
            "modes",
            "kind",
            "stderr_policy",
            "stderr_contains",
            "holdout",
            "review_targets",
        ],
        &format!("case {case_id} variant"),
    )?;
    let id = required_id(fields, "id", &format!("case {case_id} variant"))?;
    let direction = non_empty_string(fields, "direction", &format!("variant {id}"))?;
    if !["TD", "LR", "BT", "RL"].contains(&direction.as_str()) {
        bail!("variant {id} has unsupported direction {direction}");
    }
    let source = non_empty_string(fields, "source", &format!("variant {id}"))?;
    if source_direction(&source)? != direction {
        bail!("variant {id} direction does not match its Mermaid source");
    }
    let input_path = fields
        .get("input_path")
        .map(|value| {
            value
                .as_str()
                .filter(|path| !path.is_empty())
                .map(ToOwned::to_owned)
                .ok_or_else(|| anyhow!("variant {id} input_path must be a non-empty string"))
        })
        .transpose()?;
    if let Some(input_path) = &input_path {
        let path = common::safe_relative_path(
            Path::new(input_path),
            root,
            &format!("variant {id} input_path"),
        )?;
        let input = fs::read(&path).with_context(|| format!("read variant {id} input"))?;
        if input != source.as_bytes() {
            bail!("variant {id} source does not match input_path bytes");
        }
    }

    let styles = allowed_list(fields, "styles", &format!("variant {id}"), common::STYLES)?;
    let modes = allowed_list(fields, "modes", &format!("variant {id}"), common::MODES)?;
    let kind = non_empty_string(fields, "kind", &format!("variant {id}"))?;
    if !common::KINDS.contains(&kind.as_str()) {
        bail!("variant {id} has unsupported kind {kind}");
    }
    let stderr_policy = non_empty_string(fields, "stderr_policy", &format!("variant {id}"))?;
    if !common::STDERR_POLICIES.contains(&stderr_policy.as_str()) {
        bail!("variant {id} has unsupported stderr policy {stderr_policy}");
    }
    let expected_policy = match kind.as_str() {
        "success" => "empty",
        "warning" => "warning",
        "expected_error" => "error",
        _ => unreachable!(),
    };
    if stderr_policy != expected_policy {
        bail!("variant {id} kind {kind} requires stderr policy {expected_policy}");
    }
    let stderr_contains = string_list(fields, "stderr_contains", &format!("variant {id}"), false)?;
    if kind != "success" && stderr_contains.is_empty() {
        bail!("variant {id} negative outcome must declare stderr_contains");
    }
    let holdout = non_empty_string(fields, "holdout", &format!("variant {id}"))?;
    if !HOLDOUTS.contains(&holdout.as_str()) {
        bail!("variant {id} has unsupported holdout class {holdout}");
    }
    let golden_stem = fields
        .get("golden_stem")
        .map(|value| {
            let stem = value
                .as_str()
                .filter(|stem| !stem.trim().is_empty())
                .ok_or_else(|| anyhow!("variant {id} golden_stem must be a non-empty string"))?;
            identifier(stem.to_owned(), &format!("variant {id} golden_stem"))
        })
        .transpose()?;
    if kind == "success" && holdout != "evaluator_owned" && golden_stem.is_none() {
        bail!("success variant {id} must declare golden_stem");
    }
    if kind != "success" && golden_stem.is_some() {
        bail!("negative variant {id} must not declare golden_stem");
    }
    let review_targets = validate_review_targets(fields, &id, kind == "success")?;
    let source_sha256 = common::sha256_bytes(source.as_bytes());

    Ok(ValidatedVariant {
        id,
        direction,
        source,
        source_sha256,
        input_path,
        golden_stem,
        styles,
        modes,
        kind,
        stderr_policy,
        stderr_contains,
        holdout,
        review_targets,
    })
}

fn validate_review_targets(fields: &Map<String, Value>, id: &str, required: bool) -> Result<Value> {
    let values = required_array(fields, "review_targets", &format!("variant {id}"))?;
    if required && values.is_empty() {
        bail!("success variant {id} must declare review_targets");
    }
    let mut targets = Vec::with_capacity(values.len());
    let mut identities = BTreeSet::new();
    for value in values {
        let target = object(value, &format!("variant {id} review target"))?;
        allowed_keys(
            target,
            &["dimension", "severity_floor", "class", "region"],
            "review target",
        )?;
        let dimension = non_empty_string(target, "dimension", "review target")?;
        if !DIMENSIONS.contains(&dimension.as_str()) {
            bail!("variant {id} review target has unsupported dimension {dimension}");
        }
        let severity = non_empty_string(target, "severity_floor", "review target")?;
        if !SEVERITIES.contains(&severity.as_str()) {
            bail!("variant {id} review target has unsupported severity {severity}");
        }
        let class = non_empty_string(target, "class", "review target")?;
        if !REVIEW_CLASSES.contains(&class.as_str()) {
            bail!("variant {id} review target has unsupported class {class}");
        }
        let region = non_empty_string(target, "region", "review target")?;
        let identity = format!("{dimension}:{severity}:{class}:{region}");
        if !identities.insert(identity) {
            bail!("variant {id} has duplicate review target");
        }
        targets.push(json!({
            "dimension": dimension,
            "severity_floor": severity,
            "class": class,
            "region": region,
        }));
    }
    Ok(Value::Array(targets))
}

fn build_manifest(spec: &ValidatedSpec, spec_sha256: &str) -> Result<Value> {
    let mut rows = Vec::new();
    let mut negative_cases = Vec::new();
    let mut holdouts = Vec::new();
    for case in &spec.cases {
        for variant in &case.variants {
            if variant.holdout == "evaluator_owned" {
                holdouts.push(json!({
                    "case_id": case.id,
                    "variant_id": variant.id,
                    "direction": variant.direction,
                    "source_sha256": variant.source_sha256,
                }));
                continue;
            }
            if variant.kind != "success" {
                negative_cases.push(json!({
                    "case_id": case.id,
                    "variant_id": variant.id,
                    "direction": variant.direction,
                    "kind": variant.kind,
                    "stderr_policy": variant.stderr_policy,
                    "stderr_contains": variant.stderr_contains,
                    "source": variant.source,
                    "source_sha256": variant.source_sha256,
                    "input_path": variant.input_path,
                    "styles": variant.styles,
                    "modes": variant.modes,
                }));
                continue;
            }
            for style in &variant.styles {
                for mode in &variant.modes {
                    let golden =
                        if mode == "default" && matches!(style.as_str(), "ascii" | "unicode") {
                            json!({
                                "mode": "default",
                                "path": format!(
                                    "tests/fixtures/expected/{}.{}.txt",
                                    variant.golden_stem.as_deref().unwrap_or_default(),
                                    style
                                ),
                            })
                        } else {
                            Value::Null
                        };
                    rows.push(json!({
                        "case_id": case.id,
                        "family": case.family,
                        "variant_id": variant.id,
                        "direction": variant.direction,
                        "style": style,
                        "mode": mode,
                        "kind": variant.kind,
                        "source": variant.source,
                        "source_sha256": variant.source_sha256,
                        "input_path": variant.input_path,
                        "golden_stem": variant.golden_stem,
                        "golden": golden,
                        "holdout": variant.holdout,
                        "homologs": case.homologs,
                        "semantic": case.semantic,
                        "review_targets": variant.review_targets,
                    }));
                }
            }
        }
    }
    rows.sort_by_key(row_key);
    negative_cases.sort_by_key(row_key);
    holdouts.sort_by_key(row_key);

    Ok(json!({
        "schema": MANIFEST_SCHEMA,
        "spec_schema": SPEC_SCHEMA,
        "spec_version": SPEC_VERSION,
        "spec_sha256": spec_sha256,
        "row_count": rows.len(),
        "negative_case_count": negative_cases.len(),
        "holdout_variant_count": holdouts.len(),
        "rows": rows,
        "negative_cases": negative_cases,
        "holdouts": holdouts,
    }))
}

fn row_key(value: &Value) -> String {
    ["case_id", "variant_id", "style", "mode"]
        .iter()
        .filter_map(|key| value.get(*key).and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\u{1f}")
}

fn check_summary(spec_sha256: &str, spec: &ValidatedSpec, manifest: Option<&Path>) -> Value {
    let mut summary = json!({
        "schema": CHECK_SCHEMA,
        "spec_schema": SPEC_SCHEMA,
        "spec_sha256": spec_sha256,
        "row_count": spec.reviewable_rows,
        "negative_case_count": spec.negative_cases,
        "holdout_variant_count": spec.holdout_variants,
    });
    if let Some(path) = manifest {
        summary["manifest"] = Value::String(path.to_string_lossy().replace('\\', "/"));
    }
    summary
}

fn source_direction(source: &str) -> Result<String> {
    let line = source
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| anyhow!("Mermaid source must not be empty"))?;
    let mut fields = line.split_whitespace();
    let keyword = fields.next().unwrap_or_default();
    if keyword != "graph" && keyword != "flowchart" {
        bail!("Mermaid source must begin with graph/flowchart direction");
    }
    let direction = fields
        .next()
        .ok_or_else(|| anyhow!("Mermaid source is missing direction"))?;
    match direction {
        "TD" | "TB" => Ok("TD".to_owned()),
        "LR" | "RL" | "BT" => Ok(direction.to_owned()),
        other => bail!("unsupported Mermaid source direction {other}"),
    }
}

fn object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| anyhow!("{label} must be an object"))
}

fn allowed_keys(object: &Map<String, Value>, allowed: &[&str], label: &str) -> Result<()> {
    let allowed: BTreeSet<&str> = allowed.iter().copied().collect();
    let unknown: Vec<_> = object
        .keys()
        .filter(|key| !allowed.contains(key.as_str()))
        .cloned()
        .collect();
    if !unknown.is_empty() {
        bail!("{label} contains unknown field(s): {}", unknown.join(", "));
    }
    Ok(())
}

fn required_array<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<&'a [Value]> {
    object
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| anyhow!("{label} {key} must be an array"))
}

fn required_string(object: &Map<String, Value>, key: &str, label: &str) -> Result<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("{label} {key} must be a string"))
}

fn non_empty_string(object: &Map<String, Value>, key: &str, label: &str) -> Result<String> {
    let value = required_string(object, key, label)?;
    if value.trim().is_empty() {
        bail!("{label} {key} must be non-empty");
    }
    Ok(value)
}

fn required_id(object: &Map<String, Value>, key: &str, label: &str) -> Result<String> {
    let value = non_empty_string(object, key, label)?;
    identifier(value, &format!("{label} {key}"))
}

fn identifier(value: String, label: &str) -> Result<String> {
    if value.chars().any(|character| {
        !(character.is_ascii_alphanumeric() || character == '_' || character == '-')
    }) {
        bail!("{label} must contain only ASCII letters, numbers, '_' or '-'");
    }
    Ok(value)
}

fn required_i64(object: &Map<String, Value>, key: &str, label: &str) -> Result<i64> {
    object
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow!("{label} {key} must be an integer"))
}

fn string_list(
    object: &Map<String, Value>,
    key: &str,
    label: &str,
    non_empty: bool,
) -> Result<Vec<String>> {
    let values = required_array(object, key, label)?;
    let mut output = Vec::with_capacity(values.len());
    let mut seen = BTreeSet::new();
    for value in values {
        let item = value
            .as_str()
            .filter(|item| !item.trim().is_empty())
            .ok_or_else(|| anyhow!("{label} {key} must contain non-empty strings"))?
            .to_owned();
        if !seen.insert(item.clone()) {
            bail!("{label} {key} contains duplicate value {item}");
        }
        output.push(item);
    }
    if non_empty && output.is_empty() {
        bail!("{label} {key} must be non-empty");
    }
    Ok(output)
}

fn allowed_list(
    object: &Map<String, Value>,
    key: &str,
    label: &str,
    allowed: &[&str],
) -> Result<Vec<String>> {
    let values = string_list(object, key, label, true)?;
    if let Some(value) = values
        .iter()
        .find(|value| !allowed.contains(&value.as_str()))
    {
        bail!("{label} {key} contains unsupported value {value}");
    }
    Ok(values)
}

fn normalize(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut normalized = Map::new();
            let mut keys: Vec<_> = object.keys().collect();
            keys.sort();
            for key in keys {
                normalized.insert(key.clone(), normalize(&object[key]));
            }
            Value::Object(normalized)
        }
        Value::Array(values) => {
            let mut normalized: Vec<Value> = values.iter().map(normalize).collect();
            normalized.sort_by(|left, right| {
                let left = serde_json::to_string(left).unwrap_or_default();
                let right = serde_json::to_string(right).unwrap_or_default();
                left.cmp(&right)
            });
            Value::Array(normalized)
        }
        scalar => scalar.clone(),
    }
}

fn json_bytes(value: &Value) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value).context("serialize fixture spec JSON")?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("manifest output has no parent directory"))?;
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("manifest output has invalid filename"))?;
    let temporary = parent.join(format!(".{name}.{}.tmp", std::process::id()));
    fs::write(&temporary, bytes)
        .with_context(|| format!("write temporary manifest {}", temporary.display()))?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error).with_context(|| format!("publish manifest {}", path.display()));
    }
    Ok(())
}
