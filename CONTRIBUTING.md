# Contributing to TermiFlow

Thanks for your interest in contributing!

## Before You Start

- Search existing issues before opening a new one.
- For non-trivial changes, open an issue first to discuss the approach.
- Fork the repo and work on a feature branch, not `main`.

## Development Setup

```bash
git clone https://github.com/dnvt/termiflow
cd termiflow
cargo build
cargo test
```

## Quality Bar

All PRs must pass before merge:

```bash
bash scripts/check_toolchain_policy.sh
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo test --locked --doc --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --all-features
cargo bench --locked --no-run --all-features
scripts/benchmark_gate.sh --sample-size 10 --receipt /tmp/termiflow-benchmark/receipt.json
cargo deny --locked check advisories bans licenses sources
cargo audit --deny warnings
cargo build --locked --release
# Run package and publish checks from a clean checkout.
cargo package --locked --list
cargo publish --dry-run --locked
scripts/regenerate_golden.sh --check
scripts/visual_audit.sh --out /tmp/termiflow-visual-packet --styles ascii,unicode --modes default,optimized
scripts/visual_validate.sh --packet /tmp/termiflow-visual-packet --strict-quality
scripts/review_visual_packet.sh --packet /tmp/termiflow-visual-packet --decisions /tmp/termiflow-review-decisions.jsonl --prescreen-clean
scripts/review_visual_packet.sh --packet /tmp/termiflow-visual-packet --decisions /tmp/termiflow-review-decisions.jsonl --next
scripts/review_visual_packet.sh --packet /tmp/termiflow-visual-packet --decisions /tmp/termiflow-review-decisions.jsonl --record /tmp/one-review.json
scripts/review_visual_packet.sh --packet /tmp/termiflow-visual-packet --decisions /tmp/termiflow-review-decisions.jsonl --validate
```

The package and publish commands are clean-checkout release gates; a dirty
working tree must not be hidden with `--allow-dirty`. The visual commands are
also sequential: `--prescreen-clean` records conservative machine structural
coverage only, while each `--next` frame must be inspected and appended with
`--record` before another frame is requested. A deliberate full perceptual
pass starts with a fresh decisions file and omits `--prescreen-clean`, then
drains the queue one frame at a time until `--validate` succeeds. There is no
structural-review escape hatch.

### Dependency and toolchain maintenance

At each maintenance refresh, evaluate the Rust stable release and every
direct, development, build, and reachable transitive Cargo dependency against
the newest published releases, including major versions. Refresh
`Cargo.toml`/`Cargo.lock`, adapt code and benchmarks, and re-run the MSRV,
security, package, release, and visual gates. Record an evidence-backed reason
for every older, duplicate, unreachable, or otherwise unmovable entry. The
current pinned versions are a dated observation, not permission to skip the
next absolute-latest review.

The project declares Rust 1.88 as its MSRV. Install it with:

```bash
rustup toolchain install 1.88.0
cargo +1.88.0 test --locked --all-targets --all-features
```

Golden fixture tests are part of the normal feature-enabled suite:

```bash
cargo test --locked --features golden
# if outputs changed intentionally:
scripts/regenerate_golden.sh --approve --intent "describe the rendering change"
```

Only regenerate goldens for an intentional, reviewed rendering change. A
dependency, documentation, or refactor-only change must leave them untouched.
The updater is check-only unless `--approve --intent "..."` is supplied. The
visual audit runner is Rust-backed, works through stock macOS Bash, and
writes immutable packets outside the golden directory. A full packet must pass
`visual_validate.sh --strict-quality`; this verifies packet hashes, fixture
contracts, evidence integrity, and exact drift against the checked-in quality
baseline. Review individual frames with `review_visual_packet.sh` before
changing that baseline. The review command emits exactly one frame per `--next`
call; record the observation, evidence hash, hypothesis, falsifier, related
fixtures, and next command before requesting the next frame. `--prescreen-clean`
records only machine structural coverage; warnings, critic findings, and
visual concerns still require perceptual review. Include fallback routes in a
full pass when routing changes are in scope. For a deliberate full perceptual
pass, start with a fresh decisions file and omit `--prescreen-clean`; repeatedly
pull one frame with `--next`, record the observation with `--record`, and finish
with `--validate`. There is intentionally no structural-review escape hatch.
The reusable agent procedure is documented in
[`skills/termiflow-visual-review/SKILL.md`](skills/termiflow-visual-review/SKILL.md).
A layout-repair budget warning is intentionally a strict-gate failure until
that one-frame review happens.

### QA persistence and recovery

QA packets, holdout receipts, review decisions, benchmark receipts, and
dependency-currency receipts are fail-closed publication artifacts. Writers
claim an absent final path or directory; they never overwrite an existing
artifact through a check-then-rename fallback. Receipt scripts stage JSON in
the final directory and use `scripts/publish_receipt.sh`, which requires a
same-directory hard-link claim. A final receipt that already exists is a
conflict and must be inspected or given a new run path before rerunning the
gate.

`COMPLETE.json` is the last packet write. A packet directory without a valid
completion marker, manifest hash, and packet hash is incomplete and must fail
validation. If an interrupted run leaves an incomplete packet or staged
receipt, preserve it for inspection unless its ownership and safe cleanup are
known; do not delete a guard or orphan solely because it is old. A conflicting
complete artifact is not repaired in place—verify its identity, retain it, and
rerun with an explicit new output path when the run is intentionally new.

Review JSONL has a single writer. The private writer guard is create-new and
manual recovery only; a stale guard must be investigated before removal.
Malformed or partial JSONL is a hard failure and is never auto-trimmed. An
equal semantic replay (all fields except `timestamp`) is a no-op, while a
different decision for the same `(case_id, review_kind)` is a conflict.

Repository automation is intentionally limited to Rust and Bash. Do not add
Ruby or Python source files, generated helpers, or runtime requirements.

## Coding Conventions

- No `.unwrap()` or `.expect()` in production code paths (`src/`), except at
  startup where the message is meaningful.
- Canvas coordinates are `(col, row)` — x = column, y = row.
- Use `OrientedCoords` for direction-agnostic layout rather than duplicating
  TD/LR/BT/RL branches.
- Render pipeline is one-way: parser → graph → layout → canvas → output.
  Do not reach backward.

## Reporting Bugs

Open a GitHub issue with:
1. TermiFlow version (`tw --version`)
2. Your terminal emulator and OS
3. The Mermaid input that reproduces the problem
4. Expected vs. actual output (screenshots welcome)

## Scope

TermiFlow is a focused Mermaid **flowchart** renderer for terminals. Contributions
that add new diagram types (sequence, state, ER) are out of scope for the initial
beta. Improvements to flowchart rendering, CLI ergonomics, and performance are
welcome.
