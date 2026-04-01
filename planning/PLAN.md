# Termiflow Development Plan

> **Last Updated:** 2026-04-01
> **Status:** Active — pre-1.0 polish phase

---

## Quick Status

| Component | Status | Notes |
|-----------|--------|-------|
| Parser | ✅ Complete | 2-pass, forward refs, strict mode |
| Layout | ✅ Complete | Coarse waterfall, all 4 directions |
| Rendering | ✅ Complete | 9 styles, composite styling |
| Subgraphs | ✅ Complete (single-level) | Nested warns + ignores |
| Edge Labels | ✅ Complete | Pipe and text syntax, configurable width |
| Node Shapes | ✅ Complete | 9 shapes |
| Phase 6 Critic | ✅ Complete | 15 finding codes, topology analysis |
| Phase 6 Repair | ✅ Complete | Greedy single-pass, bounded |
| Phase 6 Provenance | ✅ Complete | Cell ownership tracking |
| Phase 6 Diff/TUI | ✅ Complete | Frame diff, ANSI presenter |
| Watch Mode | ✅ Complete | `--watch` live reload works |
| TUI Mode | ⚠️ Partial | `--tui` functional, UX polish pending |
| Beam Search (6.4) | ❌ Deferred | Greedy pass sufficient for now |
| Per-Element Styling | ❌ Planned | Phase 2 (deferred) |

**Tests:** 299 passing | **Clippy:** 0 warnings | **Unwrap violations:** 0

---

## Priority Queue (Pre-1.0)

### P1: Fix BT Subgraph Border Corruption (HIGH)

**Effort:** ~2–3 hrs | **Severity:** High

Confirmed visual regression: junction characters (`┴`, `┼`) bleed into subgraph
title rows in BT direction. Affects `subgraph_fanin_bt`, `subgraph_fanout_bt`.

Phase 6 critic detects this (`SubgraphTitleCorrupted` finding). Repair logic
needs to prevent it at write time, not just detect after the fact.

**Files:** `src/render/shapes.rs`, `src/render/repair.rs`, `src/render/critic.rs`

---

### P2: Add Unit Tests to Blind-Spot Modules (HIGH)

**Effort:** ~3–4 hrs | **Risk:** Medium-High

Three core modules have zero unit tests:
- `src/graph.rs` — graph construction, nesting, ID collision, forward refs
- `src/render/edge.rs` — divergent/convergent routing, label placement
- `src/render/shapes.rs` — junction placement, direction-aware rendering

~30–45 tests needed across all three.

---

### P3: Update Documentation to Match Reality (MEDIUM)

**Effort:** ~1 hr

- `--audit` and `--optimize-render` CLI flags exist but are undocumented in `docs/reference.md`
- Add `--render-iterations N` flag for iteration control
- README positioning should be honest: "simple-to-moderate flowcharts"

**Files:** `docs/reference.md`, `README.md`, `src/bin/common/mod.rs`

---

### P4: Open Links — `---` Edge Syntax (QUICK WIN)

**Effort:** ~1–2 hrs | **User Impact:** High

Parser recognizes open edges. Render: skip arrowhead when `EdgeKind::Open`.
Add `edge_kinds_*` fixture variants. High Mermaid parity value for low cost.

**Files:** `src/parser.rs`, `src/render/edge.rs`, `tests/fixtures/`

---

### P5: Cycle Nested Fixture Completion (LOW)

**Effort:** ~30 min

Add `cycle_nested_bt.md` and `cycle_nested_rl.md` fixture inputs + expected outputs.

---

## Deferred / Post-1.0 Backlog

| Priority | Feature | Effort | Notes |
|----------|---------|--------|-------|
| HIGH | Dotted edges (`-.->`) | Medium | Edge type parity |
| HIGH | Thick edges (`==>`) | Low | Edge type parity |
| MEDIUM | Bounded beam search (6.4) | Medium | Greedy pass sufficient for now |
| MEDIUM | Nested subgraphs | High | Significant arch change; warns + ignores today |
| MEDIUM | TUI UX polish | Medium | Viewport, keyboard, status bar |
| LOW | Per-element styling (`classDef`) | High | Phase 2 spec exists |
| LOW | Sequence diagrams | High | New diagram type |
| LOW | State diagrams | High | New diagram type |

---

## Reference Documents

| Document | Purpose |
|----------|---------|
| `AUDIT-mermaid-parity.md` | Comprehensive Mermaid vs Termiflow feature gaps |
| `AUDIT-termiflow-limits.md` | Internal limits reference (canvas, labels, routing) |
| `spec/SPEC.md` | Technical specification |
| `RFC-001-expanded-edge-routing.md` | Edge routing algorithm (implemented) |
| `PHASE6_RENDER_FEEDBACK_ENGINE.md` | Phase 6 spec (60% implemented) |
| `RENDERING_ISSUES_AUDIT.md` | Open rendering defects |

---

## Verification Commands

```bash
# All tests
cargo test

# Golden fixtures
cargo test --features golden -- --ignored

# Critic audit output
echo 'graph TD; A-->B-->C' | cargo run --bin tw -- --audit

# Watch mode
cargo run --bin tw -- --watch diagram.md

# TUI mode
cargo run --bin tw -- --tui diagram.md

# Quality check
cargo clippy && cargo fmt --check
```

---

## Known Limitations

1. **BT subgraph borders** — junction chars bleed into title rows (P1 fix)
2. **Sibling subgraph collision** — envelope merging artifacts in dense graphs
3. **Nested subgraphs** — warns and ignores (Mermaid supports; arch change required)
4. **Per-element styling** — `classDef`, `:::` not supported
5. **Beam search** — greedy single-pass only; complex repair may miss optimal solution

---

*Updated 2026-04-01 from deep audit review (see `analysis/reviews/2026-04-01-deep-audit.md`)*
