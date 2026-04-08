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
cargo clippy
cargo test
```

Golden fixture tests require regeneration after intentional output changes:

```bash
cargo test --features golden -- --ignored
# if outputs changed intentionally:
bash scripts/regenerate_golden.sh
```

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
