# Termiflow Development Plan

> **Last Updated:** 2026-04-10
> **Status:** Active — pre-1.0 polish phase

---

## Quick Status

| Component | Status | Notes |
|-----------|--------|-------|
| Parser | ✅ Complete | 2-pass, forward refs, strict mode |
| Layout | ✅ Complete | Coarse waterfall, all 4 directions |
| Rendering | ✅ Complete | 9 styles, composite styling |
| Subgraphs | ⚠️ Partial | Declared nesting, horizontal parity, and anchored frame geometry are in place; broader architecture and polish still remain |
| Edge Labels | ✅ Complete | Pipe and text syntax, configurable width |
| Node Shapes | ✅ Complete | 14 shapes |
| Phase 6 Critic | ✅ Complete | 15 finding codes, topology analysis |
| Phase 6 Repair | ✅ Complete | Greedy single-pass, bounded |
| Phase 6 Provenance | ✅ Complete | Cell ownership tracking |
| Phase 6 Diff/TUI | ✅ Complete | Frame diff, ANSI presenter |
| Watch Mode | ✅ Complete | `--watch` live reload works |
| TUI Mode | ⚠️ Partial | `--tui` functional, UX polish and emulator variance pending |
| Beam Search (6.4) | ❌ Deferred | Greedy pass sufficient for now |
| Per-Element Styling | ❌ Planned | Phase 2 (deferred) |

**Tests:** full suite passing | **Clippy:** 0 warnings | **Unwrap violations:** 0

---

## Current Workstreams

- **Pre-OSS coordination** — see `planning/PRE_OSS_COORDINATION.md`.
  Status: Active. This is the canonical source of truth for pre-OSS sequencing:
  stabilization, planning/doc alignment, OSS hardening, launch, and post-beta
  backlog boundaries.
- **Open-source hardening sprint** — see `planning/OPEN_SOURCE_HARDENING.md`.
  Status: Active. Child plan for `PRE_OSS_COORDINATION.md` stage 4 only:
  package/release metadata, dependency governance, OSS hygiene, CI, and launch docs.

---

## Priority Queue (Pre-1.0)

### ~~P1: Fix BT Subgraph Border Corruption~~ ✅ DONE (2026-04-01)

Two-part fix in `src/layout.rs` (clearance loop) and `src/render/edge.rs`
(TD-style routing + `CellOwnerKind::PortalOpening` for merge portal).
Golden fixtures regenerated. 311 tests passing.

---

Recent completion:
- **Rendering precision program** — see `planning/RENDERING_PRECISION_PROGRAM.md`.
  Status: Complete (2026-04-10). Closed after nested-subgraph budgeting,
  synchronized presenter updates, grapheme-safe text-width handling, expanded
  audit/benchmark/oracle coverage, and doc-truth alignment.
- **Cell scene graph + hybrid orth router** — see `planning/CELL_SCENE_GRAPH_HYBRID_ORTH_ROUTER.md`.
  Status: Complete (2026-04-10). Closed as a bounded architecture pilot after
  landing the render-layer contract, shared display profile, geometry traces,
  the selective LR/RL hard-route pilot on `subgraph_complex_{lr,rl}`, and a
  full expected-fixture refresh from the verified renderer.

---

### P2: Add Unit Tests to Blind-Spot Modules (HIGH)

**Effort:** ~3–4 hrs | **Risk:** Medium-High

Three core modules have zero unit tests:
- `src/graph.rs` — graph construction, nesting, ID collision, forward refs
- `src/render/edge.rs` — divergent/convergent routing, label placement
- `src/render/shapes.rs` — junction placement, direction-aware rendering

~30–45 tests needed across all three.

---

### ~~P3: Execute Stage 3 Doc and Truth Alignment~~ ✅ DONE (2026-04-01)

Completed through `planning/DOC_TRUTH_ALIGNMENT.md`: public docs, CLI help,
roadmap wording, and session/planning state now agree on the current beta
wedge and caveats. Closeout also fixed the flaky BT default-render collision
cleanup regression that had blocked verification during the run.

---

### P4: Execute Stage 4 OSS Hardening (HIGH)

**Effort:** ~2–5 days | **Risk:** Medium

- follow `planning/OPEN_SOURCE_HARDENING.md`
- add package metadata, `deny.toml`, and baseline OSS docs / CI
- get the public repo surface and first beta launch checklist into reviewable shape

**Files:** `Cargo.toml`, `.github/`, top-level docs, `planning/`

---

### P5: Cycle Nested Fixture Completion (LOW)

**Effort:** ~30 min

Add `cycle_nested_bt.md` and `cycle_nested_rl.md` fixture inputs + expected outputs.

---

## Deferred / Post-1.0 Backlog

| Priority | Feature | Effort | Notes |
|----------|---------|--------|-------|
| HIGH | Mermaid edge IDs | Medium | Flowchart parity / docs-as-code fit |
| HIGH | Markdown-aware labels | Medium | Mermaid parity / label semantics |
| MEDIUM | Mermaid `@{}` shape family | Medium | Broader flowchart shape parity |
| MEDIUM | Bounded beam search (6.4) | Medium | Greedy pass sufficient for now |
| MEDIUM | Subgraph architecture follow-up | High | Reopen only if later evidence exceeds the completed selective pilot in `CELL_SCENE_GRAPH_HYBRID_ORTH_ROUTER.md` |
| MEDIUM | TUI UX polish | Medium | Viewport, keyboard, status bar |
| LOW | Engine rewrite revisit | Medium | `planning/ENGINE_REWRITE_EVALUATION.md` closed 2026-04-10 with "keep current engine"; reopen only if rewrite triggers are met |
| LOW | Per-element styling (`classDef`) | High | Phase 2 spec exists |
| LOW | Sequence diagrams | High | New diagram type |
| LOW | State diagrams | High | New diagram type |

---

## Reference Documents

| Document | Purpose |
|----------|---------|
| `AUDIT-mermaid-parity.md` | Comprehensive Mermaid vs Termiflow feature gaps |
| `AUDIT-termiflow-limits.md` | Internal limits reference (canvas, labels, routing) |
| `SUBGRAPH_TITLE_ALIGNMENT_AND_BALANCED_PADDING.md` | Completed plan for anchored titles and axis-balanced subgraph framing |
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

1. **Unicode multi-codepoint composition** — wrapping/truncation/preview/status
   now share one display-width policy, but the main render canvas is still
   char-backed and some grapheme composition remains emulator-sensitive
2. **Subgraph architecture headroom** — declared nesting, anchored titles, and
   balanced frame geometry are in place, but broader model/router evolution may
   still be warranted if later evidence reopens subgraph edge cases
3. **Per-element styling** — `classDef`, `:::` not supported
4. **Mermaid edge IDs / markdown labels / `@{}` shapes** — not supported yet
5. **Beam search** — greedy single-pass only; complex repair may miss optimal solution

---

*Updated 2026-04-10 during rendering-precision closeout*
