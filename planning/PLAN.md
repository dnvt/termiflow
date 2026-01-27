# Termiflow Development Plan

> **Last Updated:** 2026-01-27
> **Status:** Active development

---

## Quick Status

| Component | Status | Notes |
|-----------|--------|-------|
| Parser | ✅ Complete | 2-pass, forward refs, strict mode |
| Layout | ✅ Complete | Coarse waterfall, all 4 directions |
| Rendering | ✅ Complete | 9 styles, composite styling |
| Subgraphs | ✅ Complete | Single-level, portal-aware |
| Edge Labels | ✅ Complete | Pipe and text syntax |
| Node Shapes | ✅ Complete | 9 shapes |
| TUI Mode | ❌ Stub | Flag exists, not implemented |
| Per-Element Styling | ❌ Planned | Phase 2 (deferred) |

**Tests:** 131+ passing

---

## Active Work Queue

### Phase 3a: Edge Label Config (QUICK WIN)

**Effort:** ~30 min | **Status:** Ready

Edge labels are hardcoded to 12 chars while node labels allow 20 (configurable).

**Fix:**
```rust
// Add --max-edge-label CLI flag
// Add max_edge_label_width to Config (default: 20)
// Wire through format_edge_label() in render/mod.rs
```

**Files:** `src/bin/common/mod.rs`, `src/config.rs`, `src/render/mod.rs`

---

### Phase 3b: LR/RL Aspect Ratio (URGENT)

**Effort:** ~2-3 hrs | **Status:** Ready

Terminal chars have ~2:1 aspect ratio. LR/RL layouts look cramped horizontally.

**Fix:**
```rust
// Increase STEM_LENGTH_HORIZONTAL from 3 → 6
// Apply aspect multiplier to horizontal segments
```

**Files:** `src/style.rs`, `src/layout.rs`, `src/render/edge.rs`

---

### Phase 4: Remove Auto-Scaling

**Effort:** ~1-2 hrs | **Status:** Ready (prerequisite for Phase 5)

Currently auto-detects terminal width and compresses diagrams. Users want natural sizing.

**Fix:**
- Make scaling opt-in (`--fit-terminal`)
- Raise/remove canvas limits (500x200 → 10000x5000)
- Remove clipping warnings by default

**Files:** `src/scaling.rs`, `src/style.rs`, `src/render/mod.rs`

---

### Phase 5: TUI Mode

**Effort:** ~4-6 hrs | **Status:** Ready (blocked by Phase 4)

Large diagrams need scrollable navigation.

**Implementation:**
- Add `ratatui` and `crossterm` dependencies
- Create `src/tui/` module
- Arrow keys, PgUp/PgDn, status bar
- Wire up existing `--tui` flag

**Files:** `Cargo.toml`, `src/tui/mod.rs` (NEW), `src/bin/common/mod.rs`

---

## Total Remaining Work

| Phase | Effort | Cumulative |
|-------|--------|------------|
| 3a | 30 min | 30 min |
| 3b | 2-3 hrs | ~3 hrs |
| 4 | 1-2 hrs | ~5 hrs |
| 5 | 4-6 hrs | ~10 hrs |

**All 4 phases: ~8-12 hours**

---

## Backlog (Future Work)

| Priority | Feature | Effort | Notes |
|----------|---------|--------|-------|
| HIGH | Open links (`---`) | Low | Edge type parity |
| HIGH | Dotted edges (`-.->`) | Medium | Edge type parity |
| HIGH | Thick edges (`==>`) | Low | Edge type parity |
| MEDIUM | Nested subgraphs | High | Significant arch change |
| MEDIUM | Per-element styling | High | Phase 2 spec exists |
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

---

## Archived Documents

The following documents are historical and kept for reference only:

| Document | Status | Notes |
|----------|--------|-------|
| `ROADMAP.md` | Archived | Superseded by this file |
| `FUTURE_WORK.md` | Archived | Merged into backlog above |
| `LAYOUT_ROUTING_SPIKE.md` | Archived | Implemented |
| `ROUTING_REVIEW.md` | Archived | Completed |
| `SUBGRAPH_MIGRATION.md` | Archived | Implemented |
| `phase2/` | Deferred | Per-element styling (future) |

---

## Verification Commands

```bash
# Phase 3a: Test edge label config
echo 'graph TD
A -->|validates credentials| B' | cargo run --bin termiflow -- --max-edge-label 25

# Phase 3b: Test LR aspect ratio
echo 'graph LR
A --> B --> C' | cargo run --bin termiflow --

# Phase 4: Test without auto-scaling
COLUMNS=40 cargo run --bin termiflow -- tests/fixtures/inputs/flow_simple_td.md

# Phase 5: Test TUI mode
cargo run --bin termiflow -- --tui tests/fixtures/inputs/crossing_grid_td.md

# All tests
cargo test

# Regenerate golden fixtures
cargo test --features golden -- --ignored
```

---

## Known Limitations

1. **Sibling subgraph overlap** - Service/Data layers may render as nested
2. **Nested subgraphs** - Warns and ignores (Mermaid supports)
3. **Per-element styling** - `classDef`, `:::` not supported yet

---

*Consolidated from multiple planning documents on 2026-01-27*
