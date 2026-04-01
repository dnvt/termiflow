# Current Task

**Updated:** 2026-04-01
**Status:** Stage 3 doc/truth alignment complete — Stage 4 OSS hardening is next

## Active Work

- `planning/PRE_OSS_COORDINATION.md` remains the canonical pre-OSS pipeline.
- `planning/DOC_TRUTH_ALIGNMENT.md` is now complete: `README.md`,
  `docs/reference.md`, CLI help, and the roadmap/planning surface agree on the
  current beta wedge and caveats.
- `--watch` is explicitly framed as the safer live-preview mode in normal
  scrollback, while `--tui` remains documented as a partial alternate-screen
  mode whose input/scroll behavior depends on the terminal emulator.
- The flaky BT unicode collision-cleanup regression in
  `tests/render_options_api.rs` was stabilized by removing unordered portal
  iteration from the render cleanup path and by adding a repeated-run
  regression check.
- Repo verification is currently clean: `cargo fmt --check`, `cargo test`, and
  `cargo clippy` all pass.

## Why Now

Stage 4 OSS hardening depends on a truthful public story and a green baseline.
Both are now in place, so the next useful work is package/repo hardening rather
than more Stage 3 cleanup.

## Immediate Next Action

1. Return to `planning/PRE_OSS_COORDINATION.md` and pick the next highest-value
   Stage 4 slice.
2. Move directly into `planning/OPEN_SOURCE_HARDENING.md`.
3. Keep the public docs aligned with the now-landed beta framing while package
   and repo metadata work proceeds.

## Key Linked Files

- `planning/PRE_OSS_COORDINATION.md` — canonical pre-OSS pipeline
- `planning/DOC_TRUTH_ALIGNMENT.md` — completed Stage 3 execution slice
- `planning/OPEN_SOURCE_HARDENING.md` — next likely execution lane
- `README.md` — public product story and beta caveats
- `docs/reference.md` — supported syntax and portability notes
- `tests/render_options_api.rs` — repeated-run regression coverage for the BT
  unicode collision cleanup path
