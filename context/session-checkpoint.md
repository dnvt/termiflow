# Session Checkpoint

**Date:** 2026-04-01
**Branch:** main

## Where We Are

Stage 3 doc/truth alignment is now landed across the public docs, CLI help, and
planning surface. `README.md`, `docs/reference.md`, `src/bin/common/mod.rs`,
`planning/PLAN.md`, and `planning/PRE_OSS_COORDINATION.md` now agree on the
current TermiFlow beta wedge: `--watch` is the safer live-preview mode,
`--tui` is partial, Mermaid support stays flowchart-focused, and the main
syntax/Unicode/emulator caveats are explicit.

Verification is green on the current branch:
- `cargo clippy` passes
- `cargo fmt --check` passes
- `cargo test` passes

The worktree is still heavily dirty from the larger pre-existing Phase 5/6
batch, so this checkpoint reflects the current branch state rather than a clean
release-candidate boundary.

### Completed this session

1. **Public docs aligned with current product truth**
   - `README.md` now frames TermiFlow as a focused terminal-native Mermaid
     flowchart renderer and local preview tool rather than as broad Mermaid
     parity.
   - `docs/reference.md` now documents the real supported edge kinds, grouped
     edges, current syntax gaps, and Unicode/emulator caveats.

2. **CLI help aligned with the same beta framing**
   - `src/bin/common/mod.rs` now describes `--watch` as the safer live-preview
     mode in normal scrollback and `--tui` as a partial alternate-screen mode.
   - Added a CLI-help test so the new wording stays covered.

3. **Planning docs and session metadata refreshed**
   - `planning/PRE_OSS_COORDINATION.md` no longer points Stage 3 at the wrong
     child plan or stale test counts.
   - `planning/PLAN.md` no longer understates node-shape support or treats
     dotted/thick edges as deferred work.
   - `planning/BT_SUBGRAPH_TITLE_POSITION.md` is marked as a completed
     historical task record and `planning/DOC_TRUTH_ALIGNMENT.md` is now marked
     complete.
   - `context/current-task.md` and `context/blockers.md` now point to Stage 4
     readiness instead of the temporary render-test blocker.

4. **Verification blocker fixed during closeout**
   - The flaky
     `tests/render_options_api.rs::default_render_fixes_obvious_degree_mismatch_cases`
     failure was traced to unordered portal iteration in the BT render cleanup
     path.
   - `src/render/mod.rs` now processes portal groups and slot positions in a
     deterministic order.
   - `tests/render_options_api.rs` now repeats the unicode BT collision case 64
     times in one test process to guard against the old flake.

## Code Health

- `cargo clippy`: pass
- `cargo fmt --check`: pass
- `cargo test`: pass
- No new production `unwrap` / `expect` / `panic!` paths were added in this
  doc-alignment run

## Open Issues

### Rendering Defects
- Sibling subgraph collision — Medium-High severity
- LR subgraph left border missing — Medium severity

### Test Blind Spots
- `src/graph.rs` — 0 unit tests
- `src/render/edge.rs` — 0 unit tests
- `src/render/shapes.rs` — 0 unit tests

### Planning / Launch Work
- OSS hardening: package boundary, metadata, license, CI, and public beta docs

## Priority Queue

1. Begin Stage 4 OSS hardening (`planning/OPEN_SOURCE_HARDENING.md`)
2. Add blind-spot unit tests in `src/graph.rs`, `src/render/edge.rs`, and
   `src/render/shapes.rs`
3. Classify remaining sibling-subgraph / LR-border issues as beta limitations
   or launch blockers

## Next Session Prompt

Resume with `planning/OPEN_SOURCE_HARDENING.md`. The public story and test
baseline are green again, so the next useful work is package/repo hardening.
