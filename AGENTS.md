# AGENTS.md

Contributor notes for AI coding assistants working on TermiFlow.

## Canonical Sources

- `README.md` for user-facing behavior, supported features, and current product status
- `docs/reference.md` for CLI flags, syntax, and documented limitations
- `src/` for implementation truth
- `tests/fixtures/README.md` for golden-test conventions
- `planning/PLAN.md` for active roadmap context

## Fast Path

| Task | Command |
|------|---------|
| Build | `cargo build` |
| Run tests | `cargo test` |
| Lint | `cargo clippy` |
| Format check | `cargo fmt --check` |
| Render a file | `cargo run --bin tw -- diagram.md` |
| Render stdin | `echo 'graph TD; A-->B' | cargo run --bin tw --` |
| Regenerate goldens | `cargo test --features golden -- --ignored` |

If you change rendering behavior, review the updated files under
`tests/fixtures/expected/` before finalizing the change.

## Working Rules

1. Read the relevant module before editing it.
2. If you are fixing a bug, add or update the failing test first when practical.
3. If you touch parser, layout, portals, or rendering code, verify both ASCII and
   Unicode output and check direction-sensitive cases (`TD`, `LR`, `BT`, `RL`)
   when the change can affect orientation.
4. Keep `docs/` user-facing and concise. Put design notes, research, and work
   plans under `planning/`, `analysis/`, or `decisions/`.
5. Treat generated workflow files under `.claude/commands/` as automation
   surface, not as product documentation.

## Architecture Quick Reference

- `src/parser.rs`: two-pass Mermaid flowchart parser
- `src/layout.rs`: coarse layout and balancing logic
- `src/portals.rs`: subgraph envelopes and portal-slot allocation
- `src/orientation.rs`: direction-agnostic coordinate mapping
- `src/render/`: canvas, shapes, edges, cycles, semantic frame, and final output
- `src/style.rs`: built-in styles plus composite style parsing

Core pipeline:

```text
Mermaid / JSON input
  -> parser
  -> graph model
  -> layout
  -> render canvas + semantic frame
  -> stdout / watch / tui presentation
```

## Optional Slash-Command Surface

This repo includes tracked slash-command helpers in `.claude/commands/` for
contributors who use command-driven workflows. Common ones are:

- `/start`
- `/render`
- `/fixture`
- `/validate`
- `/debug`
- `/maestro:start`
- `/maestro:plan`
- `/maestro:run`
- `/maestro:review`

Do not treat those command files as canonical product docs. Keep public behavior
described in `README.md` and `docs/reference.md`.
