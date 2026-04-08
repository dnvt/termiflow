# Rendering Issues Audit

This document catalogs rendering issues discovered during the Phase 3b audit. Issues are grouped by type and severity for future resolution.

**Audit Date:** 2026-01-28
**Tested Directions:** TD, LR, RL, BT
**Test Set:** All fixtures + custom complex diagrams

---

## Issue Categories

### Category 1: BT Direction - Subgraph Rendering Issues

**Severity:** High
**Affected Tests:** `subgraph_fanin_bt.md`, `subgraph_fanout_bt.md`

#### Issue 1.1: Corrupted Subgraph Borders in BT

**Example:** `subgraph_fanin_bt`
```
┌────────┴────────────────────────────┼──────┴───────┐
│        ┼────────[  Data Sources  ]──┴──────┼       │
│ ┌──────┴─────┐    ┌──────┴─────┐    ┌──────┴─────┐ │
```

**Problems:**
- `┴` junctions appear on subgraph top border line
- `┼` characters mixed into subgraph title row
- Box-edge junctions (`┴`) embedded in node top borders

**Expected:** Clean subgraph borders with junctions on edge paths only

**Root Cause:** BT direction places junction characters during border rendering phase, causing overlap with subgraph envelope characters.

---

### Category 2: LR Direction - Subgraph Alignment

**Severity:** Medium
**Affected Tests:** `subgraph_fanout_lr.md`

#### Issue 2.1: Missing Left Border in LR Subgraphs

**Example:** `subgraph_fanout_lr`
```
┌──[  Handler Group  ]──┐
                         │                       │
```

**Problems:**
- Top border `┌` starts at column 0
- Left `│` border characters are missing or misaligned
- Content appears shifted relative to border

**Expected:** Consistent left and right borders aligning with top corners

**Root Cause:** Subgraph envelope calculation may not account for LR spacing adjustments.

---

### Category 3: Complex Edge Patterns - Junction Placement

**Severity:** Medium
**Affected Tests:** `edge_branch_td.md`, custom microservices diagrams

#### Issue 3.1: Ambiguous Junction Characters in Branch+Converge Patterns

**Example:** `edge_branch_td` bottom section
```
        ┌─────────┤
        │         └─────────┬─────────┘
        ↓                   ↓
```

**Problems:**
- `┤` junction suggests incoming edge from right, but context is unclear
- Multiple corner/junction types in close proximity
- Visual flow is harder to trace than simpler patterns

**Clarification Needed:** This may be correct behavior for complex edge routing. Need to verify against edge routing algorithm intent.

---

### Category 4: Nested Subgraphs

**Severity:** Medium
**Affected Tests:** `subgraph_complex_td.md`

#### Issue 4.1: Nested Subgraph Border Corruption

**Example:** `subgraph_complex_td`
```
│ ┌──────────[  Data Layer  ]──────────┐
│ │        ↓                   ↓       │
│ ┌─────────────────┐    /───────────\ │
```

**Problems:**
- Mixed `│ ┌` at line start instead of `│ │` for nested context
- Nested subgraph left border inconsistent
- Arrow and junction placement within nested boundaries unclear

**Root Cause:** Nested subgraph envelope calculation may overlap with parent borders.

---

### Category 5: Arrow-to-Junction Connection

**Severity:** Low
**Affected Tests:** Custom microservices diagrams

#### Issue 5.1: Missing Junction Before Arrow

**User Report:** "API Gateway to Product Service and Order Service. missing the ┬ before the ↓"

**Context:** In some divergent edge patterns, the junction character (`┬`) that should precede a downward arrow (`↓`) appears to be missing or misplaced.

**Status:** Need to create minimal reproduction case

#### Issue 5.2: Flipped Junction Direction

**User Report:** "Product Service to Message Queue has its ┬ flipped. should come from up"

**Context:** Junction characters may have incorrect orientation relative to edge flow direction.

**Status:** Need to create minimal reproduction case

---

## Summary Table

| Category | Severity | Direction | Example Fixture |
|----------|----------|-----------|-----------------|
| BT Subgraph Borders | High | BT | `subgraph_fanin_bt` |
| LR Subgraph Alignment | Medium | LR | `subgraph_fanout_lr` |
| Branch+Converge Junctions | Medium | TD | `edge_branch_td` |
| Nested Subgraphs | Medium | TD | `subgraph_complex_td` |
| Missing/Flipped Junctions | Low | TD | Custom diagrams |

---

## Recommended Fix Order

1. **BT Subgraph Borders** - Highest impact, affects basic BT subgraph usability
2. **LR Subgraph Alignment** - Visible border gaps affect visual quality
3. **Nested Subgraphs** - Common pattern in real-world diagrams
4. **Junction Clarity** - Lower priority, existing patterns often work

---

## Notes

- All golden tests pass, meaning current fixtures match expected outputs
- Some "issues" may be intentional design choices or edge cases in complex routing
- LR/RL aspect ratio compensation (Phase 3b) is working correctly
- Simple patterns (fan-out, fan-in, single subgraph) render correctly in all directions

---

## Test Commands for Reproduction

```bash
# BT subgraph issue
cargo run --quiet --bin termiflow -- tests/fixtures/inputs/subgraph_fanin_bt.md --style unicode

# LR subgraph alignment
cargo run --quiet --bin termiflow -- tests/fixtures/inputs/subgraph_fanout_lr.md --style unicode

# Complex TD patterns
cargo run --quiet --bin termiflow -- tests/fixtures/inputs/subgraph_complex_td.md --style unicode
cargo run --quiet --bin termiflow -- tests/fixtures/inputs/edge_branch_td.md --style unicode
```

---

## Collision Fixture Audit (2026-01-30)

### Category 6: Sibling Subgraph Collision Issues

**Severity:** Medium-High
**Affected Tests:** `collision_sibling_subgraphs_*`, `collision_sibling_tight_*`, `collision_sibling_triple_*`

#### Issue 6.1: BT Sibling Subgraph Title Corruption

**Example:** `collision_sibling_subgraphs_bt`
```
│───────┼──────────────┐│      │
│   [  Left Group  ]   ││      │
│ ┌─────┴────┐    ┌─────┴────┐ │
```

**Problems:**
- Title row corrupted by edge junction characters
- Subgraph borders merge with internal edges
- Right Group envelope absorbs Left Group content

**Root Cause:** BT title placement algorithm not accounting for overlapping subgraph envelopes.

#### Issue 6.2: TD Sibling Subgraphs - Envelope Nesting

**Example:** `collision_sibling_subgraphs_td`
- Left Group and Right Group borders merge unexpectedly
- Right Group renders inside Left Group envelope

**Expected:** Two distinct subgraph boxes side by side with edges crossing between them.

---

### Category 7: Cycle Back-Edge Rendering

**Severity:** Low-Medium
**Affected Tests:** `cycle_long_bt`, `cycle_selfloop_lr`

#### Issue 7.1: Broken Back-Edge Path in BT Cycles

**Example:** `cycle_long_bt`
```
┌──────────┐
│  Step 6  │──────│
└──────────┘      │
                  │
...
│  Step 1  │↓─────│
└──────────┘
```

**Problems:**
- Back-edge connection incomplete at top (Step 6)
- `↓` arrow direction incorrect at bottom (should be `↑` in BT)
- Visual loop path unclear

#### Issue 7.2: Self-Loop Rendering Failure in LR

**Example:** `cycle_selfloop_lr`
```
┌─────────┐    ┌────────┐
│  Retry  ├───→│  Done  │
└─────────┘    └────────┘
     ├
     │
     ┼
```

**Problems:**
- Self-loop path renders as disconnected vertical segment
- `┼` character orphaned at bottom
- Expected: loop from Retry back to Retry

---

### Category 8: Nested Subgraph Direction Issues

**Severity:** Medium
**Affected Tests:** `subgraph_nested_bt`, `subgraph_nested_lr`

#### Issue 8.1: BT Nested Subgraph Border Corruption

**Example:** `subgraph_nested_bt`
```
       ↑───────┤
┌──────────────│─┐
│  [  Outer  ]─┐ │
│      │         │
```

**Problems:**
- Edge path merges into subgraph border
- Nested subgraph title split across border
- Junction characters `┤` on wrong side

---

### Category 9: Parallel Edge Crossings

**Severity:** Low
**Affected Tests:** `collision_parallel_cross_td`

#### Issue 9.1: Crossing Edges Don't Actually Cross

**Example:** `collision_parallel_cross_td`
- A1→D and B→C edges should visually cross
- Current output: edges route parallel without crossing indication

**Status:** May be by design - junction detection avoids creating ambiguous crossings.

---

## Updated Summary Table

| Category | Severity | Direction | Example Fixture |
|----------|----------|-----------|-----------------|
| BT Subgraph Borders | **High** | BT | `subgraph_fanin_bt` |
| Sibling Subgraph Collision | **Medium-High** | BT, TD | `collision_sibling_subgraphs_*` |
| LR Subgraph Alignment | Medium | LR | `subgraph_fanout_lr` |
| Branch+Converge Junctions | Medium | TD | `edge_branch_td` |
| Nested Subgraphs | Medium | TD, BT | `subgraph_complex_td`, `subgraph_nested_bt` |
| Cycle Back-Edge Rendering | Low-Medium | BT, LR | `cycle_long_bt`, `cycle_selfloop_lr` |
| Missing/Flipped Junctions | Low | TD | Custom diagrams |
| Parallel Edge Crossings | Low | TD | `collision_parallel_cross_td` |

---

## Collision Fixture Test Commands

```bash
# Sibling subgraph collision
cargo run --quiet --bin termiflow -- tests/fixtures/inputs/collision_sibling_subgraphs_td.md --style unicode
cargo run --quiet --bin termiflow -- tests/fixtures/inputs/collision_sibling_subgraphs_bt.md --style unicode

# Edge-to-corner collision
cargo run --quiet --bin termiflow -- tests/fixtures/inputs/collision_edge_corner_td.md --style unicode
cargo run --quiet --bin termiflow -- tests/fixtures/inputs/collision_edge_corner_bt.md --style unicode

# Parallel edges
cargo run --quiet --bin termiflow -- tests/fixtures/inputs/collision_parallel_edges_td.md --style unicode
cargo run --quiet --bin termiflow -- tests/fixtures/inputs/collision_parallel_cross_td.md --style unicode

# Cycle issues
cargo run --quiet --bin termiflow -- tests/fixtures/inputs/cycle_long_bt.md --style unicode
cargo run --quiet --bin termiflow -- tests/fixtures/inputs/cycle_selfloop_lr.md --style unicode
```
