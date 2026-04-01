# /audit - Fixture and Rendering Audit

Comprehensive audit of golden test fixtures and rendering output for ASCII/Unicode issues.

## Arguments
- `$ARGUMENTS` - Optional: specific fixture family to audit (e.g., "subgraph", "edge", "flow")

## Instructions

### 1. Run Full Test Suite
```bash
cargo test 2>&1
```
Report any failures with details.

### 2. Audit Fixture Integrity

For each fixture family (or the specified `$ARGUMENTS` family):

**Check for common ASCII/Unicode issues:**
- Misaligned box corners (corners should align with borders)
- Broken junction characters (┬ ┴ ├ ┤ should connect properly)
- Arrow placement errors (arrows should point in flow direction)
- Label overflow (text should not exceed box boundaries)
- Missing edge segments (lines should be continuous)

**Fixture families to check:**
- `flow_*` - Basic flowcharts
- `edge_*` - Edge routing (convergent, divergent, complex)
- `subgraph_*` - Container rendering
- `label_*` - Edge labels
- `shape_*` - Node shapes
- `scale_*` - Dense/sparse layouts
- `crossing_*` - Edge crossing patterns

### 3. Visual Inspection Method

For each direction (TD, LR, BT, RL), render a representative fixture and verify:

```bash
# Render and inspect
cargo run --bin tw -- tests/fixtures/inputs/[family]_[name]_[direction].md
```

Check:
1. **Box integrity**: All 4 corners present, borders continuous
2. **Edge continuity**: No gaps in lines
3. **Junction correctness**: T-junctions match flow direction
4. **Arrow direction**: Arrows point along flow (↓ for TD, → for LR, etc.)
5. **Label positioning**: Labels don't overlap nodes or edges

### 4. Compare ASCII vs Unicode

For any fixture, compare both styles:
```bash
cargo run --bin tw -- --style ascii tests/fixtures/inputs/flow_simple_td.md
cargo run --bin tw -- --style unicode tests/fixtures/inputs/flow_simple_td.md
```

Verify structural equivalence (same layout, different characters).

### 5. Report Format

Output findings as:

```
## Fixture Audit Report

**Scope**: [All families / Specific family]
**Tests**: [X] passing

### Issues Found

#### [Fixture Name]
- **File**: `tests/fixtures/inputs/[name].md`
- **Issue**: [Description]
- **Direction(s)**: [TD/LR/BT/RL]
- **Style(s)**: [ascii/unicode/both]
- **Severity**: [Critical/Warning/Minor]

### Recommendations
[Prioritized list of fixes]

### Clean Fixtures
[List of families with no issues]
```

### 6. Quick Smoke Test

If no specific family requested, run this smoke test:
```bash
echo 'graph TD
A[Start] --> B{Decision}
B -->|Yes| C[End]
B -->|No| D[Loop]
D --> B' | cargo run --bin tw --
```

Verify the output renders correctly with:
- Rectangle (A), Diamond (B), Rectangles (C, D)
- Proper branching from B
- Edge labels "Yes" and "No" visible
- Back-edge from D to B (may render in gutter)
