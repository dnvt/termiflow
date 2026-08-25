use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use super::{common, review};

pub const LEARNING_SCHEMA: &str = "termiflow.visual_learning.report.v1";
const WATCH_CLASSES: &[&str] = &[
    "confirmed_flaw",
    "topology_ambiguous",
    "inconclusive",
    "not_applicable",
];

#[derive(Debug)]
pub struct LearningArgs {
    pub packet: PathBuf,
    pub decisions: PathBuf,
    pub output: PathBuf,
    pub strict: bool,
}

#[derive(Debug, Default)]
struct HypothesisGroup {
    class: String,
    owner_layer: String,
    hypothesis: String,
    falsifier: String,
    next_command: String,
    affected_homologs: BTreeSet<String>,
    case_ids: BTreeSet<String>,
    fixture_families: BTreeSet<String>,
}

pub fn run(args: LearningArgs) -> Result<()> {
    let packet = resolve(&args.packet);
    let decisions_path = resolve(&args.decisions);
    let output = resolve(&args.output);
    let rows = review::load_manifest(&packet)?;
    let decision_bytes = fs::read(&decisions_path)
        .with_context(|| format!("read decision ledger {}", decisions_path.display()))?;
    let decisions_sha256 = common::sha256_bytes(&decision_bytes);

    if output.exists() {
        bail!(
            "learning report already exists; choose a new immutable output: {}",
            output.display()
        );
    }

    let mut perceptual_case_ids = BTreeSet::new();
    let mut class_counts = BTreeMap::<String, usize>::new();
    let mut family_counts = BTreeMap::<String, BTreeMap<String, usize>>::new();
    let mut groups = BTreeMap::<String, HypothesisGroup>::new();
    let mut decision_count = 0usize;
    let mut unclassified = Vec::new();

    for (line_number, line) in String::from_utf8(decision_bytes.clone())
        .context("decision ledger is not UTF-8")?
        .lines()
        .enumerate()
    {
        if line.trim().is_empty() {
            continue;
        }
        let decision: Value = serde_json::from_str(line)
            .with_context(|| format!("decision line {} is invalid JSON", line_number + 1))?;
        review::validate_decision(&decision, &rows)
            .with_context(|| format!("validate decision line {}", line_number + 1))?;
        if review::review_kind(&decision)? != "perceptual" {
            continue;
        }

        let case_id = required_string(&decision, "case_id")?;
        if !perceptual_case_ids.insert(case_id.to_owned()) {
            bail!("duplicate perceptual decision for learning report: {case_id}");
        }
        decision_count += 1;

        let decision_kind = required_string(&decision, "decision")?;
        let watch_class = decision
            .get("watch_class")
            .and_then(Value::as_str)
            .unwrap_or("unclassified");
        if args.strict {
            validate_strict_class(case_id, decision_kind, decision.get("watch_class"))?;
            if decision_kind != "pass" {
                validate_strict_human_eye(&decision, case_id)?;
            }
        }
        if watch_class == "unclassified" {
            unclassified.push(case_id.to_owned());
            *class_counts.entry(watch_class.to_owned()).or_default() += 1;
            continue;
        }
        if !WATCH_CLASSES.contains(&watch_class) {
            bail!(
                "unsupported watch_class {watch_class:?} for {case_id}; expected one of {}",
                WATCH_CLASSES.join(", ")
            );
        }

        *class_counts.entry(watch_class.to_owned()).or_default() += 1;
        let row = rows
            .get(case_id)
            .ok_or_else(|| anyhow!("decision references unknown row {case_id}"))?;
        let family = fixture_family(
            row.get("fixture")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
        );
        *family_counts
            .entry(family.clone())
            .or_default()
            .entry(watch_class.to_owned())
            .or_default() += 1;

        if watch_class == "not_applicable" {
            continue;
        }
        let owner_layer = if args.strict {
            required_string(&decision, "owner_layer")?.to_owned()
        } else {
            decision
                .get("owner_layer")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("unassigned")
                .to_owned()
        };
        let hypothesis = required_string(&decision, "hypothesis")?.to_owned();
        let falsifier = required_string(&decision, "falsifier")?.to_owned();
        let next_command = required_string(&decision, "next_command")?.to_owned();
        let key = common::sha256_bytes(
            format!("{watch_class}\n{owner_layer}\n{hypothesis}\n{falsifier}\n{next_command}")
                .as_bytes(),
        );
        let group = groups.entry(key).or_insert_with(|| HypothesisGroup {
            class: watch_class.to_owned(),
            owner_layer,
            hypothesis,
            falsifier,
            next_command,
            ..HypothesisGroup::default()
        });
        group.case_ids.insert(case_id.to_owned());
        group.fixture_families.insert(family);
        if let Some(homologs) = decision.get("affected_homologs").and_then(Value::as_array) {
            for homolog in homologs.iter().filter_map(Value::as_str) {
                group.affected_homologs.insert(homolog.to_owned());
            }
        }
    }

    let renderable_rows = rows
        .values()
        .filter(|row| row["classification"] != "expected_error")
        .count();
    let expected_error_rows = rows
        .values()
        .filter(|row| row["classification"] == "expected_error")
        .count();
    let missing = rows
        .values()
        .filter(|row| row["classification"] != "expected_error")
        .filter_map(|row| row["case_id"].as_str())
        .filter(|case_id| !perceptual_case_ids.contains(*case_id))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if args.strict && !missing.is_empty() {
        bail!(
            "learning coverage incomplete: {} renderable row(s) missing; first={}",
            missing.len(),
            missing[0]
        );
    }

    let complete_path = packet.join("COMPLETE.json");
    let manifest_path = packet.join("manifest.jsonl");
    let hypotheses = groups
        .into_iter()
        .map(|(hypothesis_id, group)| {
            let promotion = match group.class.as_str() {
                "confirmed_flaw" => "candidate_renderer_hypothesis",
                "topology_ambiguous" => "fixture_oracle_policy_needed",
                "inconclusive" => "reviewer_calibration_needed",
                _ => "not_applicable",
            };
            json!({
                "hypothesis_id": hypothesis_id,
                "watch_class": group.class,
                "promotion": promotion,
                "owner_layer": group.owner_layer,
                "hypothesis": group.hypothesis,
                "falsifier": group.falsifier,
                "next_command": group.next_command,
                "affected_homologs": group.affected_homologs.into_iter().collect::<Vec<_>>(),
                "fixture_families": group.fixture_families.into_iter().collect::<Vec<_>>(),
                "case_ids": group.case_ids.into_iter().collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();

    let report = json!({
        "schema": LEARNING_SCHEMA,
        "version": 1,
        "packet": {
            "path": packet,
            "complete_sha256": common::sha256_file(&complete_path)?,
            "manifest_sha256": common::sha256_file(&manifest_path)?,
        },
        "decisions": {
            "path": decisions_path,
            "sha256": decisions_sha256,
            "reviewed": decision_count,
            "renderable_rows": renderable_rows,
            "expected_error_rows": expected_error_rows,
            "strict": args.strict,
            "missing_case_ids": missing,
        },
        "coverage": {
            "status": if missing.is_empty() && unclassified.is_empty() { "classified" } else { "needs_iteration" },
            "unclassified_case_ids": unclassified,
        },
        "class_counts": class_counts,
        "family_counts": family_counts,
        "hypotheses": hypotheses,
        "next_actions": [
            "Inspect every confirmed_flaw frame and run one focused reversible hypothesis.",
            "Add fixtures or ownership oracles for topology_ambiguous rows before renderer changes.",
            "Calibrate inconclusive signals; do not promote them to visual defects or golden changes.",
            "Regenerate both canonical and authored full-corpus lanes after every source or dependency boundary.",
        ],
    });
    common::write_json(&output, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn resolve(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .expect("resolve current directory")
            .join(path)
    }
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| anyhow!("learning decision {field} must be a non-empty string"))
}

fn validate_strict_class(case_id: &str, decision: &str, value: Option<&Value>) -> Result<()> {
    let class = value
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| anyhow!("strict learning requires watch_class for {case_id}"))?;
    if !WATCH_CLASSES.contains(&class) {
        bail!("unsupported watch_class {class:?} for {case_id}");
    }
    if decision == "pass" && class != "not_applicable" {
        bail!("pass decision {case_id} must use watch_class=not_applicable");
    }
    if decision != "pass" && class == "not_applicable" {
        bail!("non-pass decision {case_id} cannot use watch_class=not_applicable");
    }
    Ok(())
}

fn validate_strict_human_eye(decision: &Value, case_id: &str) -> Result<()> {
    let observation = required_string(decision, "observation")?;
    if observation.contains("AI one-frame inspection")
        || observation.contains("nullxnull")
        || observation.contains("warning-bearing interaction is retained")
    {
        bail!(
            "strict learning rejects templated observation for {case_id}; record what the frame visibly shows"
        );
    }
    let cells = decision["cells"]
        .as_array()
        .ok_or_else(|| anyhow!("strict learning requires exact cells for {case_id}"))?;
    if cells.is_empty()
        || cells.iter().any(|cell| {
            cell["note"]
                .as_str()
                .is_some_and(|note| note.contains("frame-level watch"))
        })
    {
        bail!("strict learning rejects templated cells for {case_id}; anchor the visible flaw");
    }
    let finding = required_string(decision, "finding")?;
    if finding == "none" || finding == "stable-human-readable-id-or-none" {
        bail!("strict learning requires a concrete finding id for {case_id}");
    }
    required_string(decision, "owner_layer")?;
    Ok(())
}

fn fixture_family(fixture: &str) -> String {
    ["_td", "_bt", "_lr", "_rl"]
        .iter()
        .find_map(|suffix| fixture.strip_suffix(suffix))
        .unwrap_or(fixture)
        .to_owned()
}
