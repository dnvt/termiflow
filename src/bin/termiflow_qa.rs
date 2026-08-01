#[path = "../qa/mod.rs"]
mod qa;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

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
}

#[derive(Debug, Args)]
struct VisualValidateArgs {
    /// Visual-audit packet directory.
    #[arg(long)]
    packet: PathBuf,
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
    /// Require one valid decision for every selected reviewable row.
    #[arg(long)]
    validate: bool,
}

#[derive(Debug, Args)]
struct SchemaArgs {
    /// Canonical fixture-spec JSON.
    #[arg(long)]
    spec: PathBuf,
    /// Validate only and emit a summary without writing a manifest.
    #[arg(long, conflicts_with = "emit_manifest")]
    check: bool,
    /// Validate and atomically write one complete manifest.
    #[arg(long)]
    emit_manifest: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct VisualAuditArgs {
    /// Final packet directory.
    #[arg(long)]
    out: Option<PathBuf>,
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::VisualAudit(args) => qa::audit::run(qa::audit::AuditArgs {
            out: args.out,
            styles: args.styles,
            modes: args.modes,
            binary: args.binary,
            input_root: args.input_root,
            metadata: args.metadata,
            display_profile: args.display_profile,
            timeout_seconds: args.timeout_seconds,
        }),
        Command::VisualValidate(args) => qa::validate::run(qa::validate::ValidateArgs {
            packet: args.packet,
            baseline: args.baseline,
            strict_quality: args.strict_quality,
        }),
        Command::Golden(args) => {
            let status = qa::golden::run(qa::golden::GoldenArgs {
                check: args.check,
                approve: args.approve,
                intent: args.intent,
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
            fixture: args.fixture,
            style: args.style,
            mode: args.mode,
            reviewer: args.reviewer,
            next: args.next,
            record: args.record,
            validate: args.validate,
        }),
        Command::Schema(args) => qa::spec::run(qa::spec::SpecArgs {
            spec: args.spec,
            check: args.check,
            emit_manifest: args.emit_manifest,
        }),
    }
}
