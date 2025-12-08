# TermiFlow Planning & Status

Internal plans, specs, and status live here to keep `/docs` consumer-friendly.

## Status Snapshot (Dec 8, 2024)
- Phase 1: parser/layout/render/CLI complete; demo-ready.
- Cycle detection with gutter rendering and clipping warnings.
- Config precedence: CLI > in-file directive > `~/.config/termiflow/config.toml`.
- Tests: `cargo test` green.

## Artifacts
- `phase2/` — Phase 2 design and implementation notes (per-element styling).
- `spec/SPEC.md` — Full technical specification (implementation contract).

## How to Use This Folder
- Add new plans or RFCs here (not under `/docs`).
- Keep summaries concise; link back to root `README.md` for user-facing info.
