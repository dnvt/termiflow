# Blockers

**Updated:** 2026-04-01

## Active Blockers

- `cargo fmt --check` currently fails on a pre-existing formatting diff in
  `src/crossing.rs`. This did not come from the pulse setup changes, but it
  blocks a fully clean `/maestro:run` verification report until the source file
  is formatted or the underlying change is adjusted.

## Dependencies

- Phase 4 (Remove Auto-Scaling) is listed as prerequisite for Phase 5 (TUI Mode) in PLAN.md.
  However, TUI skeleton files exist — check if the dependency still holds.
