# Current Task

**Updated:** 2026-04-01
**Status:** Pre-1.0 quality pass complete — ready for rendering defect sprint

## Active Work

Session work committed. Next focus is **P1: Fix BT subgraph border corruption**.

Confirmed bug: junction characters (`┴`, `┼`) bleed into subgraph title rows in
BT direction. Visible in `subgraph_fanin_bt` and `subgraph_fanout_bt` fixtures.
Phase 6 critic detects it (`SubgraphTitleCorrupted`). Repair logic needs to
prevent it at write time.

## Why Now

Highest-severity open defect. Unblocks sibling-collision work. Critic already
detects it — need to extend repair to prevent it.

## Immediate Next Action

1. Reproduce locally: `echo '...' | cargo run --bin tw -- --audit`
2. Read `src/render/shapes.rs` BT border logic + `src/render/repair.rs` title repair
3. Trace where junction chars enter title row
4. Extend repair or prevent at source

## Key Linked Files

- `src/render/shapes.rs` — BT top-border rendering (draw_boxlike)
- `src/render/repair.rs` — restore_subgraph_title
- `src/render/critic.rs` — SubgraphTitleCorrupted finding
- `tests/fixtures/inputs/subgraph_fanin_bt.md` — reproducer
- `planning/RENDERING_ISSUES_AUDIT.md` — Category 1.1
