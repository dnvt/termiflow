# Deep Audit Review — 2026-04-01

**Scope:** Full codebase, strategy, and test coverage
**Mode:** `--deep` (3 parallel agents)
**Confidence:** 78%

---

## Summary Verdict

The codebase is in excellent shape on code quality. The confidence gap comes from:
1. BT subgraph rendering defects — documented but unresolved
2. Three core modules with no unit tests
3. PLAN.md stale — describes shipped work as future work

---

## Code Quality (Agent 1 — 88/100)

- **Zero unwrap violations** in all `src/` production code
- **Architecture intact**: one-way pipeline, Phase 6 modules cleanly isolated
- **45 public exports**, well-organized, non-leaking
- **0 clippy warnings, fmt clean**
- `lazy_static` replaceable with `OnceLock` (low priority)

### Top 3 Code Risks
1. Phase 6 repair/critic complexity (2,100+ lines in critic) — well-tested, monitor edge cases
2. TUI framework ships but `--tui` not fully CLI-activated
3. `lazy_static` cosmetic modernization opportunity

---

## Test Coverage (Agent 2 — 82/100)

**299 tests total** (230 unit, 54 integration, 14 binary, 3 doc)

### Critical Blind Spots

| Module | Tests | Risk |
|--------|-------|------|
| `src/graph.rs` | **0** | High |
| `src/render/edge.rs` | **0** | Medium-High |
| `src/render/shapes.rs` | **0** | Medium |

### New Phase 6 Coverage
- critic: 18 tests, repair: 5, provenance: 2, semantic: 1, topology: 2
- TUI: 14 tests (frame, live, presenter)

### Golden Runner
Feature-gated: `cargo test --features golden -- --ignored`
62 families × 4 directions × 2 styles = 513 expected outputs

### Missing
- `cycle_nested` BT + RL fixture variants
- Performance/scale regression tests

---

## Strategy & Plan (Agent 3)

### Phase Reality

| Phase | Plan Says | Reality |
|-------|-----------|---------|
| 3a, 3b, 4 | "Ready" | ✅ Shipped |
| 5 (TUI) | "Blocked" | ✅ Functionally complete |
| 6.0–6.3, 6.5 | "Proposed" | ✅ Shipped (60% of Phase 6) |
| 6.4 (beam search) | "Proposed" | ❌ Deferred (greedy sufficient) |
| 6.6 (live preview) | "Proposed" | ✅ Partial |

### Open Rendering Defects

| Issue | Severity | Status |
|-------|----------|--------|
| BT subgraph border corruption | **High** | Open |
| Sibling subgraph collision | Medium-High | Open |
| LR subgraph left border missing | Medium | Open |

### Top Mermaid Parity Gaps

| Feature | Impact | Effort |
|---------|--------|--------|
| Open links (`---`) | High | Low — quick win |
| Dotted edges (`-.->`) | High | Medium |
| Thick edges (`==>`) | Medium | Low |
| Nested subgraphs | Medium | High |

### Strategic Positioning
- Differentiation: ASCII output, pipe-friendliness, native live preview
- #1 gap: BT subgraph and sibling collision defects undermine quality claim
- Ready for 1.0 with honest "simple-to-moderate flowcharts" positioning

---

## Priority Action Plan

### Tier 1 — Before 1.0
1. Fix BT subgraph border corruption
2. Add unit tests: graph.rs, edge.rs, shapes.rs (~30–45 tests)
3. Update planning/PLAN.md to reflect shipped state

### Tier 2 — High value
4. Implement open links (`---`) — quick win
5. Document `--audit`, `--optimize-render`; add `--render-iterations N`
6. Add `cycle_nested` BT + RL fixtures

### Tier 3 — Post-1.0
7. Bounded beam search (Phase 6.4)
8. Dotted / thick edge types
9. Nested subgraph support (arch decision required)
