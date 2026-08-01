use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::json;

use super::common;

#[derive(Debug)]
pub struct GoldenArgs {
    pub check: bool,
    pub approve: bool,
    pub intent: Option<String>,
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

fn resolve(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}
