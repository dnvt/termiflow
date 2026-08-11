#[path = "../qa/mod.rs"]
mod qa;

use std::path::PathBuf;

use anyhow::Result;
use clap::{ArgAction, Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "termiflow-qa",
    about = "TermiFlow Rust quality and visual tooling"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Build a reproducible visual-audit packet.
    VisualAudit(VisualAuditArgs),
    /// Validate a visual-audit packet and quality baseline.
    VisualValidate(VisualValidateArgs),
    /// Check or explicitly approve golden snapshot changes.
    Golden(GoldenArgs),
    /// Emit or record one visual frame review at a time.
    Review(ReviewArgs),
    /// Validate a canonical Mermaid fixture spec and emit its deterministic manifest.
    Schema(SchemaArgs),
    /// Execute evaluator-owned holdout rows into a golden-free packet and receipt.
    Holdout(HoldoutArgs),
    /// Review and validate intentional fixture failures separately from visual decisions.
    ErrorPolicy(ErrorPolicyArgs),
}

#[derive(Debug, Args)]
struct VisualValidateArgs {
    /// Visual-audit packet directory.
    #[arg(long)]
    packet: PathBuf,
    /// Versioned schema manifest that scopes a queue packet.
    #[arg(long)]
    queue_manifest: Option<PathBuf>,
    /// Validate the evaluator-owned holdout section of the queue manifest.
    #[arg(long, requires = "queue_manifest")]
    holdout: bool,
    /// Quality baseline JSON.
    #[arg(long, default_value = "tests/fixtures/quality_baseline.json")]
    baseline: PathBuf,
    /// Require clean source identity and exact baseline finding equality.
    #[arg(long)]
    strict_quality: bool,
}

#[derive(Debug, Args)]
struct GoldenArgs {
    /// Check only; this is the default safety mode.
    #[arg(long)]
    check: bool,
    /// Write changed snapshots after an explicit intent.
    #[arg(long)]
    approve: bool,
    /// Required change intent when approving.
    #[arg(long)]
    intent: Option<String>,
    /// Versioned schema manifest produced by `schema --emit-manifest`.
    #[arg(
        long,
        conflicts_with_all = ["metadata", "input_root", "styles", "fixtures"]
    )]
    manifest: Option<PathBuf>,
    /// Restrict the non-manifest check or approval to one or more fixture names.
    #[arg(long = "fixture", value_name = "NAME", action = ArgAction::Append)]
    fixtures: Vec<String>,
    /// Prebuilt renderer executable; skips Cargo discovery.
    #[arg(long)]
    binary: Option<PathBuf>,
    /// Fixture input directory.
    #[arg(long, default_value = "tests/fixtures/inputs")]
    input_root: PathBuf,
    /// Fixture metadata JSON.
    #[arg(long, default_value = "tests/fixtures/metadata.json")]
    metadata: PathBuf,
    /// Comma-separated styles.
    #[arg(long, default_value = "ascii,unicode")]
    styles: String,
    /// Optional JSON report destination.
    #[arg(long)]
    report: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct ReviewArgs {
    /// Visual-audit packet directory.
    #[arg(long)]
    packet: PathBuf,
    /// JSONL decision log.
    #[arg(long)]
    decisions: PathBuf,
    /// Hash-bound historical visual-risk ledger. Open records must be explicitly resolved before a pass.
    #[arg(long)]
    history: Option<PathBuf>,
    /// Require a newly inspected perceptual ledger; reject machine and carry-forward decisions.
    #[arg(long, conflicts_with_all = ["rebind_from_packet", "rebind_from_decisions", "prescreen_clean"])]
    fresh: bool,
    #[arg(long)]
    fixture: Option<String>,
    #[arg(long, value_parser = ["ascii", "unicode"])]
    style: Option<String>,
    #[arg(long, value_parser = ["default", "optimized"])]
    mode: Option<String>,
    #[arg(long, default_value = "ai")]
    reviewer: String,
    /// Emit exactly one unreviewed frame as JSON.
    #[arg(long)]
    next: bool,
    /// Validate and append one decision JSON object.
    #[arg(long)]
    record: Option<PathBuf>,
    /// Rebind exact successful perceptual decisions from a completed prior packet.
    #[arg(
        long,
        requires = "rebind_from_decisions",
        conflicts_with_all = ["next", "record", "prescreen_clean", "validate"]
    )]
    rebind_from_packet: Option<PathBuf>,
    /// Prior packet JSONL perceptual decision log used for exact-hash rebinding.
    #[arg(
        long,
        requires = "rebind_from_packet",
        conflicts_with_all = ["next", "record", "prescreen_clean", "validate"]
    )]
    rebind_from_decisions: Option<PathBuf>,
    /// Append conservative structural pre-screen decisions for clean rows.
    #[arg(long, conflicts_with_all = ["next", "record", "validate"])]
    prescreen_clean: bool,
    /// Require one valid decision for every selected reviewable row.
    #[arg(long)]
    validate: bool,
}

#[derive(Debug, Args)]
struct SchemaArgs {
    /// Canonical fixture-spec JSON.
    #[arg(long)]
    spec: PathBuf,
    /// Named fixture queue to validate and materialize.
    #[arg(long)]
    queue: String,
    /// Validate only and emit a summary without writing a manifest.
    #[arg(long, conflicts_with = "emit_manifest")]
    check: bool,
    /// Validate and atomically write one complete manifest.
    #[arg(long)]
    emit_manifest: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct HoldoutArgs {
    /// Canonical fixture-spec JSON.
    #[arg(long)]
    spec: PathBuf,
    /// Named fixture queue whose evaluator-owned rows should execute.
    #[arg(long)]
    queue: String,
    /// Golden-free holdout packet directory.
    #[arg(long)]
    out: PathBuf,
    /// Hash-bound holdout receipt JSON.
    #[arg(long)]
    receipt: PathBuf,
    /// Prebuilt renderer executable; skips Cargo discovery.
    #[arg(long)]
    binary: Option<PathBuf>,
    /// Stable display-profile identifier.
    #[arg(long, default_value = "terminal-grid-v1")]
    display_profile: String,
    /// Per-row process timeout in seconds.
    #[arg(long, default_value_t = 60.0)]
    timeout_seconds: f64,
}

#[derive(Debug, Args)]
struct ErrorPolicyArgs {
    /// Complete visual-audit packet containing expected-error rows.
    #[arg(long)]
    packet: PathBuf,
    /// JSONL expected-error policy ledger.
    #[arg(long)]
    records: PathBuf,
    /// Emit exactly one unrecorded expected-error row as JSON.
    #[arg(long)]
    next: bool,
    /// Validate and append one expected-error policy record JSON object.
    #[arg(long)]
    record: Option<PathBuf>,
    /// Require one valid policy record for every expected-error row.
    #[arg(long)]
    validate: bool,
}

#[derive(Debug, Args)]
struct VisualAuditArgs {
    /// Final packet directory.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Versioned schema manifest; audits only its reviewable queue rows.
    #[arg(long, conflicts_with_all = ["input_root", "metadata", "styles", "modes"])]
    schema_manifest: Option<PathBuf>,
    /// Comma-separated styles.
    #[arg(long, default_value = "ascii,unicode")]
    styles: String,
    /// Comma-separated render modes.
    #[arg(long, default_value = "default,optimized")]
    modes: String,
    /// Prebuilt renderer executable; skips Cargo discovery.
    #[arg(long)]
    binary: Option<PathBuf>,
    /// Fixture input directory.
    #[arg(long, default_value = "tests/fixtures/inputs")]
    input_root: PathBuf,
    /// Fixture metadata JSON.
    #[arg(long, default_value = "tests/fixtures/metadata.json")]
    metadata: PathBuf,
    /// Stable display-profile identifier.
    #[arg(long, default_value = "terminal-grid-v1")]
    display_profile: String,
    /// Per-row process timeout in seconds.
    #[arg(long, default_value_t = 60.0)]
    timeout_seconds: f64,
    /// Respect authored style directives in each input instead of injecting --style.
    #[arg(long)]
    respect_input_style: bool,
    /// Pause a private packet subprocess at a named persistence boundary (test-only).
    #[arg(long, hide = true, value_parser = ["stage-created", "writing", "ready", "before-publish", "after-publish"])]
    pause_at: Option<String>,
    /// Marker path written before a requested pause.
    #[arg(long, hide = true, requires = "pause_at")]
    pause_marker: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::VisualAudit(args) => qa::audit::run(qa::audit::AuditArgs {
            out: args.out,
            schema_manifest: args.schema_manifest,
            styles: args.styles,
            modes: args.modes,
            binary: args.binary,
            input_root: args.input_root,
            metadata: args.metadata,
            display_profile: args.display_profile,
            timeout_seconds: args.timeout_seconds,
            respect_input_style: args.respect_input_style,
            pause_at: args.pause_at,
            pause_marker: args.pause_marker,
        }),
        Command::VisualValidate(args) => qa::validate::run(qa::validate::ValidateArgs {
            packet: args.packet,
            queue_manifest: args.queue_manifest,
            holdout: args.holdout,
            baseline: args.baseline,
            strict_quality: args.strict_quality,
        }),
        Command::Golden(args) => {
            let status = qa::golden::run(qa::golden::GoldenArgs {
                check: args.check,
                approve: args.approve,
                intent: args.intent,
                manifest: args.manifest,
                fixtures: args.fixtures,
                binary: args.binary,
                input_root: args.input_root,
                metadata: args.metadata,
                styles: args.styles,
                report: args.report,
            })?;
            if status != 0 {
                std::process::exit(status);
            }
            Ok(())
        }
        Command::Review(args) => qa::review::run(qa::review::ReviewArgs {
            packet: args.packet,
            decisions: args.decisions,
            history: args.history,
            fresh: args.fresh,
            fixture: args.fixture,
            style: args.style,
            mode: args.mode,
            reviewer: args.reviewer,
            next: args.next,
            record: args.record,
            rebind_from_packet: args.rebind_from_packet,
            rebind_from_decisions: args.rebind_from_decisions,
            prescreen_clean: args.prescreen_clean,
            validate: args.validate,
        }),
        Command::Schema(args) => qa::spec::run(qa::spec::SpecArgs {
            spec: args.spec,
            queue: args.queue,
            check: args.check,
            emit_manifest: args.emit_manifest,
        }),
        Command::Holdout(args) => qa::holdout::run(qa::holdout::HoldoutArgs {
            spec: args.spec,
            queue: args.queue,
            out: args.out,
            receipt: args.receipt,
            binary: args.binary,
            display_profile: args.display_profile,
            timeout_seconds: args.timeout_seconds,
        }),
        Command::ErrorPolicy(args) => qa::error_policy::run(qa::error_policy::ErrorPolicyArgs {
            packet: args.packet,
            records: args.records,
            next: args.next,
            record: args.record,
            validate: args.validate,
        }),
    }
}
