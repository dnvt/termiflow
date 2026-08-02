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
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --doc --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
cargo deny check advisories bans licenses sources
cargo package --allow-dirty --list
scripts/regenerate_golden.sh --check
scripts/visual_audit.sh --styles ascii --modes default
scripts/visual_validate.sh --packet PATH --strict-quality
scripts/review_visual_packet.sh --packet PATH --decisions PATH --prescreen-clean
scripts/review_visual_packet.sh --packet PATH --decisions PATH --next
scripts/review_visual_packet.sh --packet PATH --decisions PATH --next --include-structural
scripts/review_visual_packet.sh --packet PATH --decisions PATH --validate
```

The project declares Rust 1.88 as its MSRV. Install it with:

```bash
rustup toolchain install 1.88.0
cargo +1.88.0 test --all-targets --all-features
```

Golden fixture tests are part of the normal feature-enabled suite:

```bash
cargo test --features golden
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
full pass when routing changes are in scope. Use
`--include-structural` for a deliberate full perceptual pass. The reusable
agent procedure is documented in
[`skills/termiflow-visual-review/SKILL.md`](skills/termiflow-visual-review/SKILL.md).
A layout-repair budget warning is intentionally a strict-gate failure until
that one-frame review happens.

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
