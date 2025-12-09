# RFC-001: Expanded Edge Routing (Universal Vertical Stems)

**Status**: Implemented
**Date**: 2025-12-09
**Decision**: 47
**Implemented**: 2025-12-09

## Problem Statement

Current edge rendering uses a compact single-row horizontal span between source and destination nodes. This works acceptably for Unicode (box-drawing junction characters are visually distinct), but fails for ASCII where `+` is ambiguous as both corner and junction.

**Current ASCII output:**
```
    +-----------+
    |  Gateway  |
    +-----------+
    --------------   <- No junction chars, just dashes
    v            v
+--------+   +-------+
|  Auth  |   |  API  |
```

**Current Unicode output:**
```
    └───────────┘
    ┌─────┴──────┐   <- Junction char embedded in horizontal span
    ▼            ▼
```

Both styles suffer from:
1. **No vertical stem** from source node - edge appears to emerge from thin air
2. **Junction characters blend** with horizontal span - unclear where split occurs
3. **Direction ambiguous** in ASCII - can't distinguish corner from junction

## Proposed Solution

Implement **universal expanded edge routing** with explicit vertical stems for ALL styles.

**Proposed ASCII output:**
```
    +-----------+
    |  Gateway  |
    +-----+-----+
          |          <- Vertical stem from source
    +-----+-----+    <- Horizontal span with clear junction
    |           |    <- Vertical stems to destinations
    v           v
+--------+   +-------+
|  Auth  |   |  API  |
```

**Proposed Unicode output:**
```
    ┌───────────┐
    │  Gateway  │
    └─────┬─────┘
          │          <- Vertical stem from source
    ┌─────┴─────┐    <- Horizontal span with clear junction
    │           │    <- Vertical stems to destinations
    ↓           ↓    <- Thin arrows (▼ reserved for heavy/double)
┌────────┐   ┌───────┐
│  Auth  │   │  API  │
```

## Design Specification

### Layout Changes

**Current constants** (`src/style.rs`):
```rust
pub const BOX_HEIGHT: usize = 3;
pub const ROW_SPACING: usize = 2;  // Space between box bottom and next box top
pub const COL_SPACING: usize = 3;
```

**New constants**:
```rust
pub const BOX_HEIGHT: usize = 3;
pub const EDGE_STEM_HEIGHT: usize = 1;     // Vertical stem from source box
pub const EDGE_JUNCTION_HEIGHT: usize = 1; // Horizontal junction row
pub const EDGE_DROP_HEIGHT: usize = 1;     // Vertical drop to destination
pub const ROW_SPACING: usize = 4;          // Was 2, now: stem(1) + junction(1) + drop(1) + arrow(1)
pub const COL_SPACING: usize = 3;
```

### Edge Routing Algorithm

For edges from node `A` to nodes `B` and `C`:

```
Phase 1: Source Stem
- Draw vertical line from center of A's bottom edge
- Length: EDGE_STEM_HEIGHT (1 row)

Phase 2: Junction Span
- Draw horizontal line at junction_y = A.y + BOX_HEIGHT + EDGE_STEM_HEIGHT
- Span from leftmost destination center to rightmost destination center
- Place junction character (┴/+) at source stem intersection
- Place corner characters (┌/┐ or +) at destination drop points

Phase 3: Destination Stems
- Draw vertical lines from each corner down to destination arrows
- Length: EDGE_DROP_HEIGHT (1 row)

Phase 4: Arrows
- Place arrow characters at end of each destination stem
```

### Visual Structure

```
Source Row (y = src.y):
┌───────────┐
│   Label   │
└─────┬─────┘  <- Box bottom with junction indicator
      │        <- Stem (y = src.y + BOX_HEIGHT)
┌─────┴─────┐  <- Junction row (y = src.y + BOX_HEIGHT + 1)
│           │  <- Drop segments (y = src.y + BOX_HEIGHT + 2)
▼           ▼  <- Arrows (y = src.y + BOX_HEIGHT + 3)
Dest Row (y = src.y + BOX_HEIGHT + ROW_SPACING)
```

### Character Mapping

| Position | Unicode (light) | ASCII | Heavy/Double |
|----------|-----------------|-------|--------------|
| Source stem | `│` | `\|` | `┃` / `║` |
| Junction (split) | `┴` | `+` | `┻` / `╩` |
| Corner left | `┌` | `+` | `┏` / `╔` |
| Corner right | `┐` | `+` | `┓` / `╗` |
| Drop segment | `│` | `\|` | `┃` / `║` |
| Arrow down | `↓` | `v` | `▼` |
| Arrow up | `↑` | `^` | `▲` |
| Arrow left | `←` | `<` | `◀` |
| Arrow right | `→` | `>` | `▶` |
| Horizontal span | `─` | `-` | `━` / `═` |

### Arrow Style Strategy

**Thin arrows** (`↓ ↑ ← →`) for light-weight styles:
- `unicode` (default)
- `rounded`
- `dots`
- `plus`
- `stars`
- `blocks`

**Filled chevrons** (`▼ ▲ ◀ ▶`) reserved for bold styles:
- `heavy`
- `double`
- `ascii` uses `v ^ < >` for maximum compatibility

### Edge Cases

**Single destination (no split):**
```
┌─────┐       +-----+
│  A  │       |  A  |
└──┬──┘       +--+--+
   │             |
   ▼             v
┌─────┐       +-----+
│  B  │       |  B  |
```
- No junction row needed
- Direct vertical stem to arrow

**Multiple sources to same destination (fan-in):**
```
┌─────┐   ┌─────┐      +-----+   +-----+
│  A  │   │  B  │      |  A  |   |  B  |
└──┬──┘   └──┬──┘      +--+--+   +--+--+
   │         │            |         |
   └────┬────┘            +----+----+
        │                      |
        ▼                      v
    ┌───────┐              +-------+
    │   C   │              |   C   |
```
- Junction merges multiple source stems
- Single drop to destination

**Cross-rank edges (skip ranks):**
```
┌─────┐
│  A  │
└──┬──┘
   │
   ├─────────┐   <- T-junction for intermediate destination
   │         │
   ▼         │
┌─────┐      │
│  B  │      │   <- C is one rank below B
└─────┘      │
             ▼
         ┌─────┐
         │  C  │
```

## Implementation Plan

### Phase 0: Arrow Style Fix (Pre-requisite)
**File**: `src/style.rs`

Current `UNICODE_CHARS` uses `▼` for `arrow_down`. Change to thin arrows:

```rust
// Before (line ~284):
arrow_down: '▼',
arrow_up: '^',
arrow_left: '<',
arrow_right: '>',

// After:
arrow_down: '↓',
arrow_up: '↑',
arrow_left: '←',
arrow_right: '→',
```

Styles already correct:
- `ascii`: `v ^ < >` ✓
- `double`: `▼ ▲ ◀ ▶` ✓ (bold)
- `rounded`: `↓ ↑ ← →` ✓
- `heavy`: `▼ ▲ ◀ ▶` ✓ (bold)
- `dots`: `↓ ↑ ← →` ✓
- `plus`: `v ^ < >` ✓
- `stars`: `↓ ↑ ← →` ✓
- `blocks`: `▼ ▲ ◀ ▶` ✓ (bold)

### Phase 1: Layout Adjustment
**File**: `src/layout.rs`

1. Update `ROW_SPACING` constant to 4
2. Modify `waterfall()` to use new spacing
3. Update tests for new coordinates

### Phase 2: Edge Routing Rewrite
**File**: `src/render/edge.rs`

1. Add `route_expanded_edge()` function:
   ```rust
   pub fn route_expanded_edge(
       from: &Node,
       to_nodes: &[&Node],  // Support multi-target
       canvas: &mut Canvas,
       style: &StyleChars,
   )
   ```

2. Implement four-phase drawing:
   - `draw_source_stem()`
   - `draw_junction_span()`
   - `draw_destination_stems()`
   - `draw_arrows()`

3. Keep `route_edge()` as fallback for single-target straight lines
4. Update `route_back_edge()` to work with new spacing

### Phase 3: Main Render Integration
**File**: `src/render/mod.rs`

1. Group edges by source node
2. Call `route_expanded_edge()` for multi-target sources
3. Call `route_edge()` for single-target sources
4. Maintain edge sorting (straight first, L-shaped second)

### Phase 4: Golden Test Updates
**Files**: `tests/fixtures/expected/*.txt`

1. Regenerate all golden files with new spacing
2. Add new fixture: `fan_out.md` (one source, multiple targets)
3. Add new fixture: `fan_in.md` (multiple sources, one target)
4. Verify ASCII and Unicode variants

### Phase 5: Documentation
**Files**: `README.md`, `CLAUDE.md`, `docs/README.md`

1. Update example outputs in docs
2. Document the visual structure
3. Note breaking change in output format

## Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|------------|
| Golden test churn | Medium | Batch update all goldens in single commit |
| Edge case bugs | Medium | Add comprehensive test fixtures |
| Performance (more drawing) | Low | Negligible - still O(edges) |
| Breaking downstream tools | Low | Output format change is intentional improvement |

## Testing Strategy

1. **Unit tests**: New edge routing functions
2. **Golden tests**: All existing + 2 new fixtures
3. **Visual inspection**: Manual review of ASCII vs Unicode parity
4. **Integration**: Full render pipeline with complex graphs

## Rollback Plan

If issues discovered:
1. Revert `ROW_SPACING` to 2
2. Revert edge routing changes
3. Regenerate golden files

All changes are in-tree and reversible.

## Timeline

| Task | Estimate |
|------|----------|
| Phase 0: Arrow fix | 5 min |
| Phase 1: Layout | 15 min |
| Phase 2: Edge routing | 45 min |
| Phase 3: Integration | 15 min |
| Phase 4: Golden tests | 20 min |
| Phase 5: Docs | 10 min |
| **Total** | ~2 hours |

## Acceptance Criteria

- [x] ASCII output shows clear junction characters at edge splits
- [x] Unicode output matches ASCII structure with box-drawing chars
- [x] Unicode uses thin arrows (`↓`), heavy/double use filled (`▼`)
- [x] All 63 tests pass
- [x] No clippy warnings
- [x] Golden files regenerated
- [ ] README examples updated

---

*Approved: 2025-12-09*
