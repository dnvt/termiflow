# Termiflow vs Mermaid: Comprehensive Feature Audit

> **Generated**: 2026-01-27
> **Purpose**: Gap analysis for roadmap planning
> **Scope**: Flowchart diagrams only (Mermaid's `graph`/`flowchart` syntax)

---

## Executive Summary

| Category | Termiflow | Mermaid | Parity |
|----------|-----------|---------|--------|
| **Node Shapes** | 9 shapes | 30+ shapes | 30% |
| **Edge Types** | 1 (solid arrow) | 6 types | 17% |
| **Edge Labels** | 12 chars max | Unlimited | 40% |
| **Text/Wrapping** | Opt-in (`--wrap`) | Auto by default | 60% |
| **Directions** | 4 (TD/LR/BT/RL) | 5 (+TB alias) | 100% |
| **Subgraphs** | Single-level | Nested + directions | 40% |
| **Styling** | Composite (global) | Per-element CSS | 20% |
| **Interactivity** | Click targets only | Full callbacks | 30% |

**Overall Feature Parity: ~35%**

---

## Part 1: Node Shapes

### Termiflow Supported (9 shapes)

| Shape | Syntax | Rendering | Status |
|-------|--------|-----------|--------|
| Rectangle | `A[Label]` | `┌────┐` | ✅ Full |
| Rounded | `A(Label)` | `╭────╮` | ✅ Full |
| Diamond | `A{Label}` | `◇` shape | ✅ Full |
| Circle | `A((Label))` | `( )` | ✅ Full |
| Stadium | `A([Label])` | Pill shape | ✅ Full |
| Hexagon | `A{{Label}}` | `⬡` shape | ✅ Full |
| Database | `A[(Label)]` | Cylinder | ✅ Full |
| Subroutine | `A[[Label]]` | `║ ║` bars | ✅ Full |
| Asymmetric | `A>Label]` | Flag shape | ✅ Full |

### Mermaid Additional Shapes (NOT in Termiflow)

| Shape | Mermaid Syntax | Priority | Complexity |
|-------|----------------|----------|------------|
| Double Circle | `A(((Label)))` | LOW | Easy |
| Parallelogram | `A[/Label/]` | MEDIUM | Medium |
| Parallelogram Alt | `A[\Label\]` | MEDIUM | Medium |
| Trapezoid | `A[/Label\]` | MEDIUM | Medium |
| Trapezoid Alt | `A[\Label/]` | MEDIUM | Medium |

### Mermaid v11.3.0+ Semantic Shapes (30+ shapes)

These use the new `@{ shape: "name" }` syntax:

| Category | Shapes | Priority |
|----------|--------|----------|
| **Process** | Process, Subprocess, Multi-Process, Divided Process | LOW |
| **Data** | Database, Disk Storage, Direct Access Storage, Stored Data | LOW |
| **Documents** | Document, Multi-Document, Lined Document, Tagged Document | LOW |
| **Control** | Decision, Loop Limit, Prepare Conditional, Junction | MEDIUM |
| **I/O** | Manual Input, Manual Operation, Display, Paper Tape | LOW |
| **Special** | Cloud, Delay, Extract, Collate, Summary | LOW |
| **Terminals** | Start, Stop, Terminal Point, Event | MEDIUM |
| **Visual** | Icon, Image (with URLs) | HIGH |

**Recommendation**: Parallelogram/Trapezoid are parsed but not rendered. Icon/Image shapes would require significant architecture changes.

---

## Part 2: Edge Types

### Termiflow Supported

| Type | Syntax | Character | Status |
|------|--------|-----------|--------|
| Solid Arrow | `-->` | `─→` | ✅ Full |
| Extended Arrow | `--->`, `---->` | Same | ✅ Parsed (no effect) |

### Mermaid Edge Types (NOT in Termiflow)

| Type | Mermaid Syntax | Visual | Priority | Complexity |
|------|----------------|--------|----------|------------|
| **Open Link** | `---` | `───` (no arrow) | HIGH | Easy |
| **Dotted** | `-.-` | `┄┄┄` | HIGH | Medium |
| **Dotted Arrow** | `-.->` | `┄┄→` | HIGH | Medium |
| **Thick** | `===` | `━━━` | MEDIUM | Easy |
| **Thick Arrow** | `==>` | `━━▶` | MEDIUM | Easy |
| **Circle End** | `--o` | `───○` | LOW | Easy |
| **Cross End** | `--x` | `───×` | LOW | Easy |
| **Bidirectional** | `<-->` | `←──→` | MEDIUM | Medium |

### Edge Length Modifiers

| Mermaid | Effect | Termiflow |
|---------|--------|-----------|
| `---` | 1 rank span | ❌ Not supported |
| `----` | 2 rank span | ❌ Not supported |
| `-----` | 3 rank span | ❌ Not supported |

**Note**: Termiflow parses longer arrows (`--->`) but they have no layout effect.

---

## Part 3: Edge Labels

### Termiflow Supported

| Feature | Syntax | Status |
|---------|--------|--------|
| Pipe syntax | `A -->│label│ B` | ✅ Full |
| Text syntax | `A -- label --> B` | ✅ Full |
| Truncation | Auto | ⚠️ **Hardcoded 12 chars** |

### Critical Gap: Edge Label Length

**Current Implementation** (`src/render/mod.rs:1540-1541`):
```rust
fn format_edge_label(label: &str) -> String {
    format_edge_label_with_limit(label, 12)  // HARDCODED!
}
```

- Standard edges: **12 characters max**
- Convergent edges: **10 characters max**
- No configuration option exists
- No `--max-edge-label` flag
- Labels are truncated with `…` (ellipsis)

**Example:**
```
Input:  A -->|validates user credentials| B
Output: validates u…
```

### Mermaid Comparison

| Feature | Mermaid | Termiflow | Priority |
|---------|---------|-----------|----------|
| **Unlimited length** | ✅ | ❌ 12 chars | **HIGH** |
| **Multi-line labels** | ✅ `<br/>` | ❌ | MEDIUM |
| **Configurable limit** | ✅ `wrappingWidth` | ❌ | **HIGH** |
| **Markdown in labels** | ✅ `**bold**` | ❌ | LOW |

### Recommended Fixes

1. **Add `--max-edge-label` flag** - Allow user configuration
2. **Increase default** - 20 chars to match node labels
3. **Multi-line support** - Allow `<br/>` in edge labels (future)

---

## Part 4: Text Length & Descriptions (NEW)

This section covers how long text is handled across all diagram elements.

### Node Labels

| Feature | Termiflow | Mermaid | Status |
|---------|-----------|---------|--------|
| **Default max width** | 20 chars | Unlimited* | ⚠️ Gap |
| **Auto-wrap** | Off (opt-in) | On by default | ⚠️ Gap |
| **Manual line breaks** | ✅ `<br/>`, `\n` | ✅ | Parity |
| **Max lines** | 3 (configurable) | Unlimited | ⚠️ Gap |
| **Truncation indicator** | `...` | None (wraps) | Different |

*Mermaid has `maxTextSize` global limit but no per-node limit.

#### Termiflow Node Label Options

```bash
# Default: truncate at 20 chars
termiflow diagram.md

# Enable wrapping
termiflow --wrap diagram.md

# Custom width + lines
termiflow --wrap --max-label 40 --max-lines 5 diagram.md

# In-file directive
%% termiflow: wrap=true
%% termiflow: max_label=40
%% termiflow: max_lines=5
```

#### Example Comparison

**Mermaid (auto-wraps by default):**
```
┌─────────────────────────────────┐
│ This is a very long description │
│ that automatically wraps to     │
│ multiple lines                  │
└─────────────────────────────────┘
```

**Termiflow (default - truncates):**
```
┌────────────────────────┐
│  This is a very lo...  │
└────────────────────────┘
```

**Termiflow (with `--wrap`):**
```
┌────────────────────────┐
│  This is a very long   │
│  description that      │
│  wraps to multiple...  │
└────────────────────────┘
```

### Edge Labels

| Feature | Termiflow | Mermaid | Status |
|---------|-----------|---------|--------|
| **Max length** | 12 chars (hardcoded) | Unlimited | ❌ **Critical Gap** |
| **Configurable** | ❌ No flag | ✅ `wrappingWidth` | ❌ Gap |
| **Multi-line** | ❌ | ✅ `<br/>` | ❌ Gap |
| **Wrapping** | ❌ | ✅ Auto | ❌ Gap |

### Subgraph Titles

| Feature | Termiflow | Mermaid | Status |
|---------|-----------|---------|--------|
| **Long titles** | ✅ Expands box | ✅ | Parity |
| **Multi-line** | ❌ Single line | ❌ Single line | Parity |
| **Max length** | Canvas width | Unlimited | ⚠️ Practical limit |

#### Termiflow Handles Long Subgraph Titles Well:

```
┏━[  This is a very long subgraph title that describes the services  ]━┓
┃                                                                      ┃
┃ ┌──────────┐                                                         ┃
┃ │  Node A  │                                                         ┃
┃ └──────────┘                                                         ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
```

### Shape-Specific Multiline Support

| Shape | Multiline | Notes |
|-------|-----------|-------|
| Rectangle | ✅ | Full support |
| Rounded | ✅ | Full support |
| Stadium | ✅ | Full support |
| Hexagon | ✅ | Full support |
| Database | ✅ | Full support |
| Subroutine | ✅ | Full support |
| Asymmetric | ✅ | Full support |
| Parallelogram | ✅ | Full support |
| Trapezoid | ✅ | Full support |
| **Diamond** | ❌ | Single-line only |
| **Circle** | ❌ | Single-line only |

### Text Handling Priority Matrix

| Issue | Impact | Effort | Priority |
|-------|--------|--------|----------|
| **Edge label 12-char limit** | HIGH | LOW | ⭐⭐⭐⭐⭐ |
| **Add `--max-edge-label` flag** | HIGH | LOW | ⭐⭐⭐⭐⭐ |
| **Default auto-wrap** | MEDIUM | LOW | ⭐⭐⭐⭐ |
| **Edge label multiline** | MEDIUM | MEDIUM | ⭐⭐⭐ |
| **Diamond/Circle multiline** | LOW | MEDIUM | ⭐⭐ |
| **Markdown text styling** | LOW | HIGH | ⭐ (deprioritized) |

### Recommended Implementation Order

1. **Phase 1: Edge Label Parity** (Quick Win)
   - Add `--max-edge-label` CLI flag
   - Increase default from 12 → 20 chars
   - Wire through Config system
   - ~1-2 hours work

2. **Phase 2: Default Behavior**
   - Consider changing default `wrap_labels` to `true`
   - Or add `--no-wrap` flag for backwards compat
   - Update documentation

3. **Phase 3: Edge Label Multiline** (Future)
   - Parse `<br/>` in edge labels
   - Vertical label stacking on edges
   - More complex - defer

---

## Part 5: Subgraphs

### Termiflow Supported

| Feature | Status | Notes |
|---------|--------|-------|
| Basic subgraphs | ✅ | `subgraph ID [Title]` |
| Titled subgraphs | ✅ | `subgraph Title` |
| Multiple subgraphs | ✅ | Horizontal/vertical stacking |
| Edges crossing subgraphs | ✅ | Portal routing |

### Mermaid Additional Features (NOT in Termiflow)

| Feature | Mermaid Syntax | Priority | Complexity |
|---------|----------------|----------|------------|
| **Nested subgraphs** | Subgraph inside subgraph | HIGH | High |
| **Subgraph direction** | `direction LR` inside subgraph | MEDIUM | High |
| **Edges to subgraphs** | `A --> subgraphId` | LOW | Medium |
| **Subgraph styling** | `style subgraphId fill:#f9f` | LOW | Medium |

**Current Behavior**: Nested subgraphs emit warning and are ignored.

---

## Part 6: Styling System

### Termiflow Supported

| Feature | Syntax | Status |
|---------|--------|--------|
| Global style | `--style unicode` | ✅ Full |
| Composite styles | `--style corner:dots,border:heavy` | ✅ Full |
| In-file directives | `%% termiflow: style=rounded` | ✅ Full |
| 9 base styles | ascii, unicode, double, rounded, heavy, dots, plus, stars, blocks | ✅ Full |
| 7 style components | corner, border, arrow, edge, junction, back, subgraph | ✅ Full |

### Mermaid Styling (NOT in Termiflow)

| Feature | Mermaid Syntax | Priority | Complexity |
|---------|----------------|----------|------------|
| **classDef** | `classDef myClass fill:#f9f` | HIGH | High |
| **Class application** | `A:::myClass` | HIGH | High |
| **Per-node style** | `style A fill:#f9f` | HIGH | High |
| **Per-link style** | `linkStyle 0 stroke:#ff3` | MEDIUM | High |
| **Default class** | `classDef default fill:#fff` | MEDIUM | Medium |
| **CSS properties** | fill, stroke, stroke-width, color, etc. | HIGH | High |

**Gap Analysis**: Termiflow has NO per-element styling. All styling is global. This is a fundamental architecture difference - terminal output doesn't support colors/fills natively.

### Possible Termiflow Approach

For terminal styling, consider:
1. **ANSI colors** via escape codes (requires terminal support detection)
2. **Character-based differentiation** (different box styles per class)
3. **Bold/dim text** for emphasis

---

## Part 7: Interactivity

### Termiflow Supported

| Feature | Syntax | Status |
|---------|--------|--------|
| Click targets | `click A "path.md"` | ✅ Parsed & stored |

**Note**: Click targets are stored in `Node.click_target` but no TUI mode exists to use them.

### Mermaid Interactivity (NOT in Termiflow)

| Feature | Mermaid Syntax | Priority |
|---------|----------------|----------|
| **Callback functions** | `click A callback` | N/A (requires JS) |
| **URL links** | `click A "https://..."` | LOW (TUI needed) |
| **Tooltips** | `click A callback "tooltip"` | LOW (TUI needed) |
| **Security mode** | `securityLevel: loose/strict` | N/A |

---

## Part 8: Text & Labels (LEGACY - see Part 4)

### Termiflow Constraints

| Constraint | Value | Configurable |
|------------|-------|--------------|
| Max label width | 20 chars | ✅ `--max-label` |
| Max label lines | 3 (wrapped) | ✅ `--max-lines` |
| Wrapping | Off by default | ✅ `--wrap` |
| Truncation suffix | `...` | ❌ Hardcoded |
| Unicode width | Supported | ✅ CJK = 2 cols |

### Mermaid Text Features (NOT in Termiflow)

| Feature | Status | Notes |
|---------|--------|-------|
| **Markdown strings** | ❌ | `**bold**`, `*italic*` |
| **HTML entities** | ❌ | `&amp;`, `&lt;`, etc. |
| **Line breaks** | ❌ | `<br/>` in labels |
| **Auto-wrapping** | ❌ | Mermaid auto-wraps long text |

---

## Part 9: Layout & Rendering

### Termiflow Constraints

| Limit | Value | Behavior When Exceeded |
|-------|-------|------------------------|
| Canvas width | 500 chars | Clipped + warning |
| Canvas height | 200 rows | Clipped + warning |
| Routing steps | 2500 max | Aborted + warning |
| Box height | 3 rows (fixed) | N/A |
| Box min width | 5 chars | N/A |
| Column spacing | 3 chars | Hardcoded |
| Row spacing | 2 rows | Hardcoded |
| Cycle gutter | 4 chars | Hardcoded |

### Mermaid Layout Features (NOT in Termiflow)

| Feature | Mermaid | Termiflow |
|---------|---------|-----------|
| **Renderer selection** | dagre, elk | Single algorithm |
| **Curve styles** | basis, cardinal, linear, step | Manhattan only |
| **Width config** | `%%{init: {flowchart: {width: 100}}}%%` | ❌ |
| **Wrap width** | Configurable | ❌ |
| **Rank separation** | Configurable | ❌ Hardcoded |
| **Node separation** | Configurable | ❌ Hardcoded |

---

## Part 10: Syntax & Parsing

### Termiflow Parsing Features

| Feature | Status |
|---------|--------|
| Two-pass parsing | ✅ Forward references work |
| Strict mode | ✅ `--strict` flag |
| Warning collection | ✅ All warnings to stderr |
| Comments | ✅ `%%` lines |
| Config directives | ✅ `%% termiflow: key=value` |

### Mermaid Syntax (NOT in Termiflow)

| Feature | Mermaid Syntax | Status |
|---------|----------------|--------|
| **Init directive** | `%%{init: {...}}%%` | ❌ Not parsed |
| **Theme config** | `%%{init: {theme: 'dark'}}%%` | ❌ N/A |
| **Flowchart config** | `%%{init: {flowchart: {...}}}%%` | ❌ Not parsed |
| **Accessibility** | `accTitle`, `accDescr` | ❌ Not parsed |

---

## Part 11: Error Handling

### Termiflow Error Categories

| Category | Behavior | Strict Mode |
|----------|----------|-------------|
| **Fatal** | Immediate exit | Always fatal |
| **Warning** | Log + continue | Becomes fatal |
| **Info** | Log + continue | Never fatal |

### Fatal Errors
- Empty file
- No direction found
- Unsupported diagram type (sequence, class, etc.)

### Warnings (fatal in strict mode)
- Nested subgraphs
- Nested brackets in labels
- Pipe in labels
- Mermaid styling syntax
- Malformed edges/nodes
- Multiple directions
- Content before direction

### Informational (never fatal)
- Node auto-created from edge reference

---

## Part 12: Feature Priority Matrix

### URGENT Priority (Visual Harmony)

| Feature | Impact | Effort | ROI |
|---------|--------|--------|-----|
| **LR/RL aspect ratio fix** | HIGH - UX | Low | ⭐⭐⭐⭐⭐ |

LR/RL layouts look cramped horizontally and stretched vertically due to terminal character aspect ratio (~2:1). Need to multiply horizontal distances by 2 for visual harmony with TD/BT layouts.

### CRITICAL Priority (Quick Wins)

| Feature | Impact | Effort | ROI |
|---------|--------|--------|-----|
| **Edge label 12→20 chars** | HIGH | Very Low | ⭐⭐⭐⭐⭐ |
| **Add `--max-edge-label`** | HIGH | Very Low | ⭐⭐⭐⭐⭐ |
| **Default auto-wrap** | MEDIUM | Very Low | ⭐⭐⭐⭐ |

### HIGH Priority (Core Gaps)

| Feature | Impact | Effort | ROI |
|---------|--------|--------|-----|
| **Remove auto-scaling** | High - UX | Low | ⭐⭐⭐⭐⭐ |
| **TUI mode (scrollable)** | High - UX | Medium | ⭐⭐⭐⭐ |
| Open links (no arrow) | High - common use | Low | ⭐⭐⭐⭐⭐ |
| Dotted edges | High - common use | Medium | ⭐⭐⭐⭐ |
| Thick edges | Medium | Low | ⭐⭐⭐⭐ |
| Nested subgraphs | High - complex diagrams | High | ⭐⭐⭐ |
| Parallelogram shapes | Medium | Low | ⭐⭐⭐⭐ |

### MEDIUM Priority (Nice to Have)

| Feature | Impact | Effort | ROI |
|---------|--------|--------|-----|
| Bidirectional edges | Medium | Medium | ⭐⭐⭐ |
| Circle/cross endpoints | Low | Low | ⭐⭐⭐ |
| Subgraph directions | Medium | High | ⭐⭐ |
| Multi-line edge labels | Low | Medium | ⭐⭐ |
| Edge length control | Low | High | ⭐ |

### LOW Priority (Future/Complex)

| Feature | Impact | Effort | ROI |
|---------|--------|--------|-----|
| Per-element styling | Medium | Very High | ⭐ |
| ANSI color output | Medium | High | ⭐⭐ |
| Icon/image shapes | Low | Very High | ⭐ |
| Semantic shapes (30+) | Low | High | ⭐ |
| Curve styles | Low | Very High | ⭐ |

---

## Part 13: Recommended Roadmap

### Phase 0: Text Handling Quick Wins (PRIORITY - 1-2 days)

**Rationale**: Highest impact, lowest effort fixes for text/description support.

1. **Edge Label Length** (Critical):
   ```rust
   // Current (src/render/mod.rs:1541):
   format_edge_label_with_limit(label, 12)  // Too short!

   // Fix:
   format_edge_label_with_limit(label, config.max_edge_label_width)
   ```

   - Add `--max-edge-label` CLI flag (default: 20)
   - Add `max_edge_label_width` to Config
   - Wire through render functions

2. **Consider Default Auto-Wrap**:
   - Current: `wrap_labels = false`
   - Option A: Change default to `true`
   - Option B: Add `--no-wrap` for backwards compat
   - Mermaid auto-wraps by default

**Files to modify**:
- `src/bin/common/mod.rs` - Add `--max-edge-label` flag
- `src/config.rs` - Add `max_edge_label_width` field
- `src/render/mod.rs` - Pass config to `format_edge_label()`

---

### Phase 1: Edge Type Parity (2-3 weeks)

1. **Parser changes**:
   - Add regex for `---` (open link)
   - Add regex for `-.-` and `-.->` (dotted)
   - Add regex for `===` and `==>` (thick)
   - Store edge type in `Edge` struct

2. **Style changes**:
   - Add `edge_dotted_h`, `edge_dotted_v` characters
   - Add `edge_thick_h`, `edge_thick_v` characters

3. **Render changes**:
   - Pass edge type through routing
   - Select character based on edge type

### Phase 2: Shape Completion (Estimated: 1 week)

1. **Enable parallelogram/trapezoid**:
   - Already parsed (enum exists)
   - Add rendering in `shapes.rs`

2. **Add double circle**:
   - Parser regex: `\(\(\(([^)]*)\)\)\)`
   - Render as larger circle

### Phase 3: Nested Subgraphs (Estimated: 3-4 weeks)

1. **Parser changes**:
   - Track subgraph stack (not just current)
   - Build parent-child relationships

2. **Layout changes**:
   - Recursive envelope computation
   - Inside-out sizing algorithm

3. **Render changes**:
   - Nested border drawing
   - Portal routing through multiple levels

### Phase 4: Remove Auto-Scaling (1-2 days)

**Problem:** Current behavior auto-scales diagrams to fit terminal width:
- Detects terminal size from `COLUMNS`/`LINES`
- Compresses spacing (compact mode)
- Reduces label widths
- Clips at hard limits (500x200)

**Solution:** Make scaling opt-in, render at natural size by default.

1. **Add `--fit-terminal` flag** (opt-in scaling)
2. **Remove terminal width auto-detection** by default
3. **Raise canvas limits** to 10000x5000 (or remove)

**Files to modify:**
- `src/scaling.rs` - Make scaling opt-in
- `src/style.rs` - Raise `MAX_CANVAS_*` constants
- `src/render/mod.rs` - Remove/optionalize clipping
- `src/bin/common/mod.rs` - Add flags

---

### Phase 5: TUI Mode (1-2 weeks)

**Problem:** Large diagrams don't fit in terminal, get clipped.

**Solution:** Implement `--tui` for scrollable interactive view.

**MVP Features:**
- Arrow key navigation (up/down/left/right)
- Page up/down for fast scrolling
- Home/End to jump to corners
- `q` to quit
- Status bar showing position

**Implementation:**
```rust
// Cargo.toml
ratatui = "0.26"
crossterm = "0.27"
```

**Files to create:**
- `src/tui/mod.rs` - TUI module
- `src/tui/app.rs` - App state
- `src/tui/ui.rs` - Rendering

---

### Phase 6: Enhanced Styling (4-6 weeks) - Deprioritized

1. **Parse `classDef`**:
   - Extract class name and properties
   - Store in graph metadata

2. **Parse `:::` syntax**:
   - Apply class to node
   - Store in node metadata

3. **Terminal-appropriate styling**:
   - Map fill colors to ANSI backgrounds (optional)
   - Map stroke styles to border styles
   - Bold/dim text support

---

## Appendix A: Character Sets

### Current Termiflow Character Palette

```
Horizontal edges:  - ─ ═ ━
Vertical edges:    | │ ║ ┃ :
Corners:           + ┌ ┐ └ ┘ ╭ ╮ ╯ ╰
Junctions:         ┬ ┴ ├ ┤ ┼
Arrows:            v ^ < > ↓ ↑ ← → ▼ ▲ ◀ ▶
```

### Proposed Additions for Dotted Edges

```
Dotted horizontal: ┄ ┈ ╌ ╎ · · ·
Dotted vertical:   ┆ ┊ ╎ ·
Dotted corners:    Use same as solid (visual simplicity)
```

### Proposed Additions for Endpoints

```
Circle end:        ○ ●
Cross end:         × ✕
Diamond end:       ◇ ◆
```

---

## Appendix B: Test Coverage Gaps

### Missing Test Categories

| Category | Current | Needed |
|----------|---------|--------|
| Dotted edges | 0 | 4+ (all directions) |
| Thick edges | 0 | 4+ |
| Open links | 0 | 4+ |
| Parallelogram | 0 | 4+ |
| Trapezoid | 0 | 4+ |
| Double circle | 0 | 4+ |
| Nested subgraphs | 0 (warning test only) | 8+ |
| Edge type mixing | 0 | 4+ |

---

## Appendix C: Breaking Changes Risk

### Safe Changes (No Breaking)
- Adding new edge types
- Adding new shapes
- Adding new style options

### Potentially Breaking
- Changing default truncation behavior
- Modifying spacing constants
- Changing junction resolution logic

### Definitely Breaking
- Changing existing syntax parsing
- Removing features
- Changing canvas size limits

---

## Appendix D: Quick Reference - Syntax Comparison

| Feature | Mermaid | Termiflow | Gap |
|---------|---------|-----------|-----|
| Rectangle | `A[text]` | `A[text]` | ✅ |
| Rounded | `A(text)` | `A(text)` | ✅ |
| Circle | `A((text))` | `A((text))` | ✅ |
| Diamond | `A{text}` | `A{text}` | ✅ |
| Hexagon | `A{{text}}` | `A{{text}}` | ✅ |
| Stadium | `A([text])` | `A([text])` | ✅ |
| Database | `A[(text)]` | `A[(text)]` | ✅ |
| Subroutine | `A[[text]]` | `A[[text]]` | ✅ |
| Asymmetric | `A>text]` | `A>text]` | ✅ |
| Double Circle | `A(((text)))` | ❌ | ❌ |
| Parallelogram | `A[/text/]` | Parsed, not rendered | ⚠️ |
| Trapezoid | `A[/text\]` | Parsed, not rendered | ⚠️ |
| Arrow | `-->` | `-->` | ✅ |
| Open | `---` | ❌ | ❌ |
| Dotted | `-.->` | ❌ | ❌ |
| Thick | `==>` | ❌ | ❌ |
| Circle end | `--o` | ❌ | ❌ |
| Cross end | `--x` | ❌ | ❌ |
| Bidirectional | `<-->` | ❌ | ❌ |
| Edge label | `--\|text\|-->` | `--\|text\|-->` | ✅ |
| Subgraph | `subgraph` | `subgraph` | ✅ |
| Nested subgraph | Yes | ❌ (warning) | ❌ |
| classDef | Yes | ❌ (warning) | ❌ |
| style | Yes | ❌ (warning) | ❌ |
| click | Yes | Partial | ⚠️ |

---

*End of Audit Document*
