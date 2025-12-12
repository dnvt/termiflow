# TermiFlow Planning & Status

Internal plans, specs, and status live here to keep `/docs` consumer-friendly.

## Status Snapshot
- Parser/layout/render/CLI are demo-ready for `--print`.
- Cycle detection with gutter rendering and clipping warnings.
- Direction-agnostic rendering across TD/LR/BT/RL.
- Subgraphs (single-level) with portal-aware border piercing.
- Config precedence: CLI > in-file directive > `~/.config/termiflow/config.toml`.
- Tests: run `cargo test` (and `cargo test --features golden` when fixtures are present).

## Artifacts
- `phase2/` — Phase 2 design and implementation notes (per-element styling).
- `spec/SPEC.md` — Full technical specification (implementation contract).

## How to Use This Folder
- Add new plans or RFCs here (not under `/docs`).
- Keep summaries concise; link back to root `README.md` for user-facing info.
