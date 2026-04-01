# MEMORY.md — TermiFlow Topic Index

This file is the index for persistent memory across sessions. Each entry links to a
topic file under the project memory store. Load topic files only when relevant to
the current task.

## Topics

| Topic | File | Description |
|-------|------|-------------|
| (none yet) | — | Add entries as concrete reusable patterns are discovered |

## Key Facts (session-level, no file needed)

- **Phase 6 is 60% shipped** — critic, repair, provenance, semantic, topology all exist in
  `src/render/`. PLAN.md was stale; updated 2026-04-01.
- **3 test blind spots**: `graph.rs`, `render/edge.rs`, `render/shapes.rs` have 0 unit tests
- **BT subgraph corruption** is the highest-priority open rendering defect
- **`too_many_arguments` pattern**: internal draw_* and routing fns use `#[allow]`, not structs
  (DEC-001). Apply same pattern to any future internal canvas helpers.
- **Dependency policy**: bump Cargo.toml version constraints when `cargo update` can't reach
  latest major (DEC-002). Always verify with `cargo test` after bumping.
- **Golden tests** are feature-gated: `cargo test --features golden -- --ignored`
  (not a bug that they show 0 in normal `cargo test`)

---

*Updated: 2026-04-01*
