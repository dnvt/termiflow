# Fixtures Audit – 2026-01-27 (Updated)

## Scope
- **Render source:** `artifacts/fixtures/20260127-175118`
- **Inputs:** 101 fixtures (100 directional + `error_sequence`)
- **Outputs reviewed:** 202 (ascii + unicode for each input)
- **Directions covered:** TD, LR, RL, BT

## Method
1. Rendered all fixtures with `scripts/render_fixtures.sh --ascii --unicode`.
2. Ran structural heuristics (arrow connectivity, isolated segments, border continuity) to triage.
3. Manually reviewed flagged outputs and re‑checked prior high‑severity cases.

## Fixed in this pass
### 1) LR/RL subgraph border breaks at portals (fan‑in/out)
**Previously:** border gaps and stray `─`/`-` at portal rows.
**Now:** portals render as `┼`/`+` crossings, borders remain intact.

**Examples**
- `subgraph_fanin_lr` (unicode)
- `subgraph_fanout_lr` / `subgraph_fanout_rl` (unicode)

### 2) LR/RL subgraph title overlap (complex layout)
**Previously:** titles merged/overwrote each other.
**Now:** subgraphs are separated by an extra column; titles no longer overlap.

**Examples**
- `subgraph_complex_lr.unicode.txt`
- `subgraph_complex_rl.unicode.txt`

### 3) BT title row corruption
**Previously:** title text was pierced by portals/edges.
**Now:** titles render inside the subgraph (row below top border) and are redrawn last. Title rows are clean.

**Examples**
- `subgraph_single_bt.unicode.txt`
- `subgraph_outside_bt.unicode.txt`
- `subgraph_complex_bt.unicode.txt`

### 4) Portal glyph styling mismatch (LR/RL)
**Previously:** portal crossings used edge style (`┼`/`+`), making heavy subgraph borders look “thin” at crossings.
**Now:** portal crossings use subgraph border style (`╋`/`+` for heavy/ascii), preserving border weight.

**Examples**
- `subgraph_fanin_lr.unicode.txt`

### 5) BT top/bottom border piercings
**Previously:** BT portals inserted light verticals into heavy borders (e.g., `│`), producing “pierced” borders.
**Now:** BT portals render as subgraph-style crossings (`╋`/`+`), keeping borders intact.

**Examples**
- `subgraph_complex_bt.unicode.txt`

### 6) BT portal shifts clobbered corners
**Previously:** title‑avoidance could shift a portal onto the border corner, replacing `┓/┏` with a crossing.
**Now:** portal shifts stay within the interior span, preserving corners.

**Examples**
- `subgraph_single_bt.unicode.txt`

### 7) BT portal glyph weight
**Previously:** BT portals always used full crossings (`╋`/`+`), which felt heavy when the title row blocks the downward stem.
**Now:** BT portals use junctions (`┻/┳`) when the vertical does not continue through the border, producing a lighter, more accurate join.

**Examples**
- `subgraph_single_bt.unicode.txt`

### 8) BT junction orientation + title-row suppression
**Previously:** some BT top portals rendered `┻` even when the stem is above, and title-row artifacts could force `╋`.
**Now:** top/bottom borders use direction‑correct junctions (`┳` for stem‑above, `┻` for stem‑below) and ignore the BT title row for vertical continuity.

**Examples**
- `subgraph_complex_bt.unicode.txt`

### 9) BT double junctions on boxlike tops
**Previously:** adjacent `┴┴` (unicode) / `++` (ascii) on some boxlike tops where only one edge reaches.
**Now:** BT junction stamping aligns to the actual outgoing stem column; duplicate joins are gone.

**Examples**
- `shape_database_bt.unicode.txt`
- `scale_dense_bt.unicode.txt`

### 10) BT portal corner adjacency
**Previously:** top border showed `┳┓` (unicode) / `++` (ascii) when a portal slot was forced to the rightmost interior column.
**Now:** BT portal positions nudge one cell away from corners when they can do so without crossing title text.

**Examples**
- `subgraph_labels_bt.unicode.txt`
- `subgraph_outside_bt.unicode.txt`

### 11) BT ASCII double‑plus on horizontal runs
**Previously:** ASCII BT horizontal runs could show `++` where only one vertical stem exists.
**Now:** ASCII cleanup collapses adjacent `++` to `-+` when only one stem is present.

**Examples**
- `scale_dense_bt.ascii.txt` (upper horizontal spine)

## Remaining issues / limitations
### A) BT ASCII junctions adjacent to box corners (cosmetic)
**Symptoms:** `++` persists when a vertical stem lands in the column adjacent to a box corner; ASCII has no distinct glyph to avoid the adjacency.

**Examples**
- `scale_dense_bt.ascii.txt` (Input 3 top border)

## Patterns / Root Causes
1. **BT title row conflict**
   - Resolved by moving BT titles inside the subgraph (row below top border).

## Suggested Fix Directions (next pass)
_No further fixes queued._
   - Keep portals one cell away from corners when space allows and title spans permit.

---

## Notes
- Full BT visual sweep performed; no correctness issues found beyond the cosmetic items above.
- Heuristic flags remain for labels and shape glyphs; those are expected and not regressions.
- All non‑subgraph flows, labels, shapes, and edge routing remain visually correct across TD/LR/BT/RL.
