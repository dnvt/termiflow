#![cfg(all(feature = "qa", unix))]

//! Wrapper integration tests use synthetic packets and a structural-validation
//! stub; the real native review validator enforces history and decision guards.
//! Native review runs --validate, not --fresh; these are Unix/Bash process tests.
//! These fixture decisions are test data, never fresh perceptual review proof.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn file_hash(path: &Path) -> String {
    hash(&fs::read(path).unwrap())
}
fn write_json(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn executable(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
fn decision(case: &str, frame: &str, evidence: &str, resolution: Option<&str>) -> Value {
    let mut value = json!({"schema":"termiflow.visual_review.decision.v3","review_kind":"perceptual",
        "case_id":case,"frame_sha256":frame,"evidence_sha256":evidence,
        "decision":"pass","severity":"P3","watch_class":"not_applicable",
        "dimensions":["semantic","containment","route","text","readability"],
        "cells":[{"x":0,"y":0,"note":"synthetic wrapper test fixture"}],
        "finding":"none","observation":"Synthetic complete-frame fixture for process testing.",
        "hypothesis":"The wrapper preserves applicable history.","oracle_scope":"certified",
        "oracle_reason":"Synthetic test endpoints are fixed.","expected_observation_if_true":"History is validated on its own packet.",
        "falsifier":"An applicable open risk escapes validation.","affected_homologs":["fixture"],
        "next_command":"cargo test --features qa --test qa_cycle","reviewer":"ai","timestamp":"1970-01-01T00:00:00Z"});
    if let Some(id) = resolution {
        value["history_resolution"] = json!({"schema":"termiflow.visual_review.history_resolution.v1",
            "status":"repaired","history_ids":[id],"note":"Synthetic fixture resolution for wrapper tests."});
    }
    value
}
fn history(id: &str, selector: Value) -> Value {
    json!({"schema":"termiflow.visual_review.history_record.v1","history_id":id,"status":"open","severity":"P2",
        "selector":selector,"dimensions":["semantic","route"],"observation":"Synthetic applicable risk.",
        "hypothesis":"The endpoint may be ambiguous.","expected_observation_if_true":"A homolog exposes ambiguity.",
        "falsifier":"Independent fixture evidence preserves ownership.","affected_homologs":["fixture"],
        "next_command":"cargo test --features qa --test qa_cycle","created_at":"1970-01-01T00:00:00Z"})
}
struct Fixture {
    root: PathBuf,
    main: PathBuf,
    main_history: PathBuf,
    empty: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "termiflow-cycle-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("scripts")).unwrap();
        fs::create_dir_all(root.join("tests/fixtures")).unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR"));
        for name in ["visual_cycle.sh", "publish_receipt.sh"] {
            fs::copy(
                source.join("scripts").join(name),
                root.join("scripts").join(name),
            )
            .unwrap();
        }
        for name in [
            "visual_cycle_record.schema.json",
            "holdout_receipt.schema.json",
        ] {
            fs::copy(
                source.join("tests/fixtures").join(name),
                root.join("tests/fixtures").join(name),
            )
            .unwrap();
        }
        executable(
            &root.join("scripts/visual_validate.sh"),
            b"#!/bin/sh\n# Synthetic packet wrapper fixture; real review remains native.\nexit 0\n",
        );
        executable(&root.join("scripts/review_visual_packet.sh"), b"#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$CYCLE_ARGV_LOG\"\nif [ -n \"${CYCLE_MUTATE_HISTORY:-}\" ]; then printf '\\n' >> \"$CYCLE_MUTATE_HISTORY\"; fi\nexec \"$CYCLE_QA\" review \"$@\"\n");
        fs::write(root.join("lesson.md"), "Synthetic wrapper test lesson.\n").unwrap();
        let main = root.join("main");
        let holdout = root.join("holdout");
        for (packet, case) in [
            (&main, "main.ascii.default"),
            (&holdout, "holdout.ascii.default"),
        ] {
            fs::create_dir_all(packet.join("frames")).unwrap();
            fs::create_dir_all(packet.join("evidence")).unwrap();
            let frame = b"frame\n";
            let evidence = serde_json::to_vec(&json!({"schema":"termiflow.render_evidence.v1","warnings":[],"errors":[],
                "critic":{"findings":[]},"raw":{"errors":[]},"geometry":{"errors":[],"untraced_fallback_edges":[]},
                "semantic":{"owner_counts":{"Unknown":0}},"route_clarity":{"schema":"termiflow.route_clarity.v1",
                    "status":"not_applicable","findings":[],"topology":{"oracle":{"schema":"termiflow.route_clarity.topology.v1","scope":"certified"}}},
                "optimized":false,"repair_passes":0,"layout_attempts":1,"layout_repairs_applied":0})).unwrap();
            fs::write(packet.join("frames/frame.txt"), frame).unwrap();
            fs::write(packet.join("evidence/evidence.json"), &evidence).unwrap();
            let identity = json!({"source_commit":"c51c890876edc6b5b54c95703a885113488fe198","worktree_dirty":false,
                "provenance":{"effective_sha256":"b".repeat(64)}});
            let row = json!({"schema":"termiflow.visual_audit.row.v2","case_id":case,"fixture":"edge_kinds_lr","classification":"success","identity":identity,
                "style":"ascii","mode":"default","direction":"LR","input":"tests/fixtures/inputs/edge_kinds_lr.md",
                "dimensions":{"display":{"width":6,"height":1}},"stdout":{"path":"frames/frame.txt","bytes":frame.len(),"sha256":hash(frame)},
                "evidence":{"path":"evidence/evidence.json","bytes":evidence.len(),"sha256":hash(&evidence)},"findings":{"raw_errors":0,"critic":0,"geometry_errors":0}});
            fs::write(packet.join("manifest.jsonl"), format!("{row}\n")).unwrap();
            fs::write(packet.join("COMPLETE.json"), "{}\n").unwrap();
            fs::write(packet.join("PACKET.sha256"), "synthetic fixture\n").unwrap();
            write_json(&packet.join("identity.json"), &identity);
            fs::write(
                root.join(if case.starts_with("main") {
                    "main-decisions.jsonl"
                } else {
                    "holdout-decisions.jsonl"
                }),
                format!("{}\n", decision(case, &hash(frame), &hash(&evidence), None)),
            )
            .unwrap();
        }
        write_json(
            &root.join("queue.json"),
            &json!({"queue_id":"test","queue_sha256":"a".repeat(64)}),
        );
        write_json(
            &root.join("holdout-receipt.json"),
            &json!({"schema":"termiflow.holdout_receipt.v1","status":"passed",
            "queue_id":"test","queue_sha256":"a".repeat(64),"manifest_sha256":file_hash(&root.join("queue.json")),
            "expected_rows":1,"actual_rows":1,"packet":{"path":holdout,"manifest_sha256":file_hash(&holdout.join("manifest.jsonl")),
                "identity_sha256":file_hash(&holdout.join("identity.json")),"packet_sha256":file_hash(&holdout.join("PACKET.sha256"))},
            "rows":[{"status":"passed","frame":{"sha256":hash(b"frame\n")},"stderr":{"bytes":0},"evidence":null,"findings":{}}]}),
        );
        let main_history = root.join("main-history.jsonl");
        let empty = root.join("empty holdout.jsonl");
        fs::write(&empty, "").unwrap();
        fs::write(&main_history, "").unwrap();
        let this = Self {
            root,
            main,
            main_history,
            empty,
        };
        this.record(false, None, None);
        this
    }
    fn resolve(&self, lane: &str, id: &str) {
        let path = self.root.join(format!("{lane}-decisions.jsonl"));
        let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["history_resolution"] = json!({"schema":"termiflow.visual_review.history_resolution.v1","status":"repaired","history_ids":[id],"note":"Synthetic test resolution."});
        fs::write(path, format!("{value}\n")).unwrap();
    }
    fn main_risk(&self) {
        fs::write(
            &self.main_history,
            format!(
                "{}\n",
                history("main-risk", json!({"case_id":"main.ascii.default"}))
            ),
        )
        .unwrap();
        self.resolve("main", "main-risk");
        self.record(false, None, None);
    }
    fn binding(path: Option<&Path>) -> Value {
        path.map(|p| json!({"path":p.canonicalize().unwrap(),"sha256":file_hash(p)}))
            .unwrap_or(Value::Null)
    }
    fn record(&self, bound: bool, main_history: Option<&Path>, holdout_history: Option<&Path>) {
        let packet = json!({"complete_sha256":file_hash(&self.main.join("COMPLETE.json")),"manifest_sha256":file_hash(&self.main.join("manifest.jsonl")),
            "identity_sha256":file_hash(&self.main.join("identity.json")),"packet_checksum_sha256":file_hash(&self.main.join("PACKET.sha256"))});
        let mut review = json!({"decisions_sha256":file_hash(&self.root.join("main-decisions.jsonl")),"reviewed":1,"expected_rows":1});
        let mut holdout_review = json!({"receipt_sha256":file_hash(&self.root.join("holdout-receipt.json")),"decisions_sha256":file_hash(&self.root.join("holdout-decisions.jsonl")),"reviewed":1,"expected_rows":1});
        if bound {
            review["history"] = Self::binding(main_history);
            holdout_review["history"] = Self::binding(holdout_history);
        }
        let mut scope_holdout = holdout_review.clone();
        scope_holdout["status"] = json!("passed");
        let queue = json!({"queue_id":"test","queue_sha256":"a".repeat(64),"manifest_sha256":file_hash(&self.root.join("queue.json"))});
        let fix = json!({"status":"localized","summary":"Synthetic script fixture."});
        let homologs = json!(["fixture"]);
        let homolog_result = json!({"status":"passed","summary":"Synthetic controls."});
        let lesson = json!({"kind":"script","path":"lesson.md","sha256":file_hash(&self.root.join("lesson.md")),"rule":"Keep histories distinct.","next_review":"Explicit next cycle."});
        let golden = json!({"status":"not_requested"});
        write_json(
            &self.root.join("record.json"),
            &json!({"schema":"termiflow.visual_cycle_record.v2","cycle_id":"synthetic-test","disposition":"fixed",
            "packet":packet,"queue":queue,"review":review,"holdout_review":holdout_review,
            "scope":{"packet":packet,"queue":queue,"review":review,"holdout":scope_holdout,"fix":fix,"homologs":homologs,"homolog_result":homolog_result,"golden_approval":golden,"lesson":lesson},
            "observation":"Synthetic wrapper process fixture.","observation_details":[{"row":0,"column":0,"glyph":"f","note":"Synthetic test."}],
            "owner_layer":"script","hypothesis":"Keep histories scoped.","expected_observation_if_true":"Each validator receives only its history.","falsifier":"Main risk is omitted.","next_command":"cargo test --features qa --test qa_cycle",
            "fix":fix,"homologs":homologs,"homolog_result":homolog_result,"holdout":{"status":"passed","result":"Synthetic test."},"lesson":lesson,"golden_approval":golden}),
        );
    }
    fn run(
        &self,
        label: &str,
        main_history: Option<&Path>,
        holdout_history: Option<&Path>,
        mutate: Option<&Path>,
    ) -> Output {
        let mut cmd = Command::new("bash");
        cmd.current_dir(&self.root)
            .arg(self.root.join("scripts/visual_cycle.sh"))
            .arg("--packet")
            .arg(&self.main)
            .arg("--decisions")
            .arg(self.root.join("main-decisions.jsonl"))
            .arg("--queue-manifest")
            .arg(self.root.join("queue.json"))
            .arg("--holdout-receipt")
            .arg(self.root.join("holdout-receipt.json"))
            .arg("--holdout-decisions")
            .arg(self.root.join("holdout-decisions.jsonl"))
            .arg("--record")
            .arg(self.root.join("record.json"))
            .arg("--output")
            .arg(self.root.join(format!("{label}.receipt.json")))
            .env("CYCLE_QA", env!("CARGO_BIN_EXE_termiflow-qa"))
            .env("CYCLE_ARGV_LOG", self.root.join("argv.log"));
        if let Some(path) = main_history {
            cmd.arg("--history").arg(path);
        }
        if let Some(path) = holdout_history {
            cmd.arg("--holdout-history").arg(path);
        }
        if let Some(path) = mutate {
            cmd.env("CYCLE_MUTATE_HISTORY", path);
        }
        let out = cmd.output().unwrap();
        fs::write(self.root.join(format!("{label}.stdout")), &out.stdout).unwrap();
        fs::write(self.root.join(format!("{label}.stderr")), &out.stderr).unwrap();
        out
    }
    fn receipt(&self, label: &str) -> Value {
        serde_json::from_slice(&fs::read(self.root.join(format!("{label}.receipt.json"))).unwrap())
            .unwrap()
    }
    fn assert_rejected(&self, label: &str, out: &Output, message: &str) {
        assert!(!out.status.success(), "{label} unexpectedly accepted");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains(message),
            "{label}: expected {message:?}, got {out:?}"
        );
        assert!(
            !self.root.join(format!("{label}.receipt.json")).exists(),
            "failure published receipt"
        );
    }
}

#[test]
fn cycle_disjoint_histories_preserve_main_risk() {
    let f = Fixture::new();
    let baseline = f.run("legacy-positive", None, None, None);
    assert!(
        baseline.status.success(),
        "positive legacy fixture failed: {baseline:?}; {}",
        f.root.display()
    );
    f.main_risk();
    let shared = f.run("shared-rejection", Some(&f.main_history), None, None);
    f.assert_rejected(
        "shared-rejection",
        &shared,
        "open visual history record main-risk matches no row in the current packet",
    );
    f.record(true, Some(&f.main_history), Some(&f.empty));
    let distinct = f.run("disjoint", Some(&f.main_history), Some(&f.empty), None);
    assert!(
        distinct.status.success(),
        "disjoint applicable main risk and explicit empty holdout must validate: {distinct:?}; {}",
        f.root.display()
    );
    let receipt = f.receipt("disjoint");
    assert_eq!(
        receipt["review"]["history"],
        Fixture::binding(Some(&f.main_history))
    );
    assert_eq!(
        receipt["holdout_review"]["history"],
        Fixture::binding(Some(&f.empty))
    );
    assert_eq!(
        receipt["scope"]["holdout"]["history"],
        receipt["holdout_review"]["history"]
    );
    assert_eq!(
        receipt["scope"]["review"]["history"],
        receipt["review"]["history"]
    );
    assert_eq!(fs::metadata(&f.empty).unwrap().len(), 0);
    let argv = fs::read_to_string(f.root.join("argv.log")).unwrap();
    assert!(
        argv.contains(f.main_history.to_str().unwrap()) && argv.contains(f.empty.to_str().unwrap())
    );
}

#[test]
fn cycle_shared_history_default_and_bound_record_without_override() {
    let f = Fixture::new();
    let baseline = f.run("baseline", None, None, None);
    assert!(baseline.status.success(), "positive fixture: {baseline:?}");
    fs::write(
        &f.main_history,
        format!(
            "{}\n",
            history("shared-risk", json!({"fixture":"edge_kinds_lr"}))
        ),
    )
    .unwrap();
    for lane in ["main", "holdout"] {
        f.resolve(lane, "shared-risk");
    }
    f.record(false, None, None);
    let legacy = f.run("shared-legacy", Some(&f.main_history), None, None);
    assert!(
        legacy.status.success(),
        "shared default changed: {legacy:?}"
    );
    assert!(f.receipt("shared-legacy")["review"]
        .get("history")
        .is_none());
    assert!(f.receipt("shared-legacy")["holdout_review"]
        .get("history")
        .is_none());
    f.record(true, Some(&f.main_history), Some(&f.main_history));
    let bound = f.run("shared-bound", Some(&f.main_history), None, None);
    assert!(bound.status.success(), "bound shared default: {bound:?}");
    assert_eq!(
        f.receipt("shared-bound")["holdout_review"]["history"],
        Fixture::binding(Some(&f.main_history))
    );
    let argv = fs::read_to_string(f.root.join("argv.log")).unwrap();
    let resolved_history = f.main_history.canonicalize().unwrap();
    assert_eq!(
        argv.lines()
            .filter(|line| *line == resolved_history.to_str().unwrap())
            .count(),
        4
    );
}

#[test]
fn cycle_holdout_only_history_retains_its_applicable_risk() {
    let f = Fixture::new();
    let baseline = f.run("baseline", None, None, None);
    assert!(baseline.status.success(), "positive fixture: {baseline:?}");
    let holdout_history = f.root.join("holdout-risk.jsonl");
    fs::write(
        &holdout_history,
        format!(
            "{}\n",
            history("holdout-risk", json!({"case_id":"holdout.ascii.default"}))
        ),
    )
    .unwrap();
    f.record(true, None, Some(&holdout_history));
    let unresolved = f.run("unresolved", None, Some(&holdout_history), None);
    f.assert_rejected(
        "unresolved",
        &unresolved,
        "unresolved visual history for holdout.ascii.default",
    );
    f.resolve("holdout", "holdout-risk");
    f.record(true, None, Some(&holdout_history));
    let resolved = f.run("holdout-only", None, Some(&holdout_history), None);
    assert!(
        resolved.status.success(),
        "holdout-only binding: {resolved:?}"
    );
    assert_eq!(f.receipt("holdout-only")["review"]["history"], Value::Null);
    assert_eq!(
        f.receipt("holdout-only")["scope"]["review"]["history"],
        Value::Null
    );
    assert_eq!(
        f.receipt("holdout-only")["holdout_review"]["history"],
        Fixture::binding(Some(&holdout_history))
    );
    let wrong = f.root.join("unmatched-holdout.jsonl");
    fs::write(
        &wrong,
        format!(
            "{}\n",
            history("unmatched-risk", json!({"case_id":"no-such-case"}))
        ),
    )
    .unwrap();
    f.record(true, None, Some(&wrong));
    let unmatched = f.run("unmatched", None, Some(&wrong), None);
    f.assert_rejected(
        "unmatched",
        &unmatched,
        "open visual history record unmatched-risk matches no row in the current packet",
    );
}

#[test]
fn cycle_requires_all_four_actual_history_bindings() {
    let f = Fixture::new();
    f.main_risk();
    f.record(true, Some(&f.main_history), Some(&f.empty));
    let positive = f.run("positive", Some(&f.main_history), Some(&f.empty), None);
    assert!(
        positive.status.success(),
        "positive disjoint fixture: {positive:?}"
    );
    for mode in [
        "missing-one",
        "missing-all",
        "wrong-hash",
        "wrong-path",
        "swapped",
        "extra-binding-key",
        "partial-without-override",
    ] {
        f.record(true, Some(&f.main_history), Some(&f.empty));
        let record_path = f.root.join("record.json");
        let mut record: Value = serde_json::from_slice(&fs::read(&record_path).unwrap()).unwrap();
        match mode {
            "missing-one" => {
                record["scope"]["holdout"]
                    .as_object_mut()
                    .unwrap()
                    .remove("history");
            }
            "missing-all" => {
                f.record(false, None, None);
                record = serde_json::from_slice(&fs::read(&record_path).unwrap()).unwrap();
            }
            "wrong-hash" => {
                record["holdout_review"]["history"]["sha256"] = json!("0".repeat(64));
            }
            "wrong-path" => {
                record["holdout_review"]["history"]["path"] = json!(f.main_history);
            }
            "swapped" => {
                for pointer in ["/review/history", "/scope/review/history"] {
                    *record.pointer_mut(pointer).unwrap() = Fixture::binding(Some(&f.empty));
                }
                for pointer in ["/holdout_review/history", "/scope/holdout/history"] {
                    *record.pointer_mut(pointer).unwrap() = Fixture::binding(Some(&f.main_history));
                }
            }
            "extra-binding-key" => {
                record["holdout_review"]["history"]["unverified"] = json!(true);
            }
            "partial-without-override" => {
                f.record(false, None, None);
                record = serde_json::from_slice(&fs::read(&record_path).unwrap()).unwrap();
                record["review"]["history"] = Fixture::binding(Some(&f.main_history));
            }
            _ => unreachable!(),
        }
        write_json(&record_path, &record);
        let override_path = (mode != "partial-without-override").then_some(f.empty.as_path());
        let rejected = f.run(mode, Some(&f.main_history), override_path, None);
        f.assert_rejected(
            mode,
            &rejected,
            "cycle history bindings do not match selected ledgers",
        );
    }
    f.record(true, Some(&f.main_history), Some(&f.empty));
    let record_path = f.root.join("record.json");
    let mut record: Value = serde_json::from_slice(&fs::read(&record_path).unwrap()).unwrap();
    record["unrelated_extra"] = json!(true);
    write_json(&record_path, &record);
    let extra = f.run(
        "unrelated-extra",
        Some(&f.main_history),
        Some(&f.empty),
        None,
    );
    f.assert_rejected(
        "unrelated-extra",
        &extra,
        "cycle record failed its fail-closed contract",
    );
}

#[test]
fn cycle_rejects_stale_or_changed_history_without_publication() {
    let f = Fixture::new();
    f.main_risk();
    f.record(true, Some(&f.main_history), Some(&f.empty));
    let positive = f.run("positive", Some(&f.main_history), Some(&f.empty), None);
    assert!(
        positive.status.success(),
        "positive disjoint fixture: {positive:?}"
    );
    fs::write(&f.empty, "\n").unwrap();
    let stale = f.run("stale", Some(&f.main_history), Some(&f.empty), None);
    f.assert_rejected(
        "stale",
        &stale,
        "cycle history bindings do not match selected ledgers",
    );
    f.record(true, Some(&f.main_history), Some(&f.empty));
    let changed = f.run(
        "changed",
        Some(&f.main_history),
        Some(&f.empty),
        Some(&f.empty),
    );
    f.assert_rejected(
        "changed",
        &changed,
        "visual history changed during cycle validation",
    );
}

#[test]
fn cycle_rejects_missing_or_symlink_holdout_history() {
    let f = Fixture::new();
    f.main_risk();
    f.record(true, Some(&f.main_history), Some(&f.empty));
    let positive = f.run("positive", Some(&f.main_history), Some(&f.empty), None);
    assert!(
        positive.status.success(),
        "positive disjoint fixture: {positive:?}"
    );
    let missing = f.root.join("missing-history");
    let absent = f.run("absent", Some(&f.main_history), Some(&missing), None);
    f.assert_rejected("absent", &absent, "holdout-history does not exist:");
    let link = f.root.join("linked-history");
    std::os::unix::fs::symlink(&f.empty, &link).unwrap();
    let linked = f.run("linked", Some(&f.main_history), Some(&link), None);
    f.assert_rejected("linked", &linked, "holdout-history must not be a symlink:");
    let no_value = Command::new("bash")
        .arg(f.root.join("scripts/visual_cycle.sh"))
        .arg("--holdout-history")
        .output()
        .unwrap();
    assert!(!no_value.status.success());
    assert!(String::from_utf8_lossy(&no_value.stderr)
        .contains("--holdout-history requires a JSONL ledger"));
    let empty_value = f.run(
        "empty-value",
        Some(&f.main_history),
        Some(Path::new("")),
        None,
    );
    f.assert_rejected(
        "empty-value",
        &empty_value,
        "--holdout-history requires a JSONL ledger",
    );
}
