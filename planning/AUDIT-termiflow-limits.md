# Termiflow Internal Limits & Constraints

> **Generated**: 2026-01-27
> **Purpose**: Quick reference for developers and users

---

## Canvas Limits

| Limit | Value | Location | Behavior |
|-------|-------|----------|----------|
| **Max width** | 500 chars | `style.rs:21` | Clipped + warning |
| **Max height** | 200 rows | `style.rs:22` | Clipped + warning |
| **Min canvas** | 1×1 | `render/mod.rs` | Clamped |

---

## Label Constraints

| Constraint | Value | Location | Configurable |
|------------|-------|----------|--------------|
| **Max width** | 20 chars | `style.rs:19` | ✅ `--max-label` |
| **Max lines** | 3 (default) | `config.rs` | ✅ `--max-lines` |
| **Truncation** | `...` suffix | `style.rs:519` | ❌ |
| **Wrapping** | Off (default) | `config.rs` | ✅ `--wrap` |
| **Line breaks** | Not supported | `measure.rs` | ❌ |

### Truncation Behavior

```
Input:  "VeryLongLabelText"  (17 chars)
Limit:  10 chars
Output: "VeryLo..."          (9 chars + ellipsis)
```

### Wrapping Behavior

```
Input:  "This is a very long label"
Width:  10 chars
Lines:  3 max

Output:
  "This is a"
  "very long"
  "label"

If > 3 lines:
  "This is a"
  "very long"
  "lab..."
```

---

## Box Dimensions

| Dimension | Value | Formula | Location |
|-----------|-------|---------|----------|
| **Min width** | 5 chars | Fixed | `style.rs:8` |
| **Fixed height** | 3 rows | Fixed (non-wrapped) | `style.rs:7` |
| **Padding** | 2 chars | Left + right | `style.rs:9` |
| **Actual width** | 7-26 chars | `min(label, 20) + 4 + 2` | `style.rs:543` |

### Box Width Calculation

```rust
box_width = min(display_width(label), MAX_LABEL_WIDTH) + BOX_PADDING*2 + 2
          = min(label_width, 20) + 4 + 2
          = 7 to 26 characters
```

---

## Spacing Constants

| Constant | Value | Location | Purpose |
|----------|-------|----------|---------|
| **Row spacing** | 2 rows | `style.rs:10` | Vertical gap between ranks |
| **Column spacing** | 3 chars | `style.rs:11` | Horizontal gap between nodes |
| **Stem (vertical)** | 1 row | `style.rs:12` | TD/BT edge exit length |
| **Stem (horizontal)** | 3 chars | `style.rs:13` | LR/RL edge exit length |
| **Junction height** | 1 row | `style.rs:14` | Junction row spacing |
| **Drop height** | 1 row | `style.rs:15` | Multi-target drop spacing |
| **Cycle gutter** | 4 chars | `style.rs:17` | Back-edge margin |

---

## Routing Constraints

| Constraint | Value | Location | Behavior |
|------------|-------|----------|----------|
| **Max routing steps** | 2500 | `layout.rs:2517` | Warning + abort |
| **Node padding** | 1 cell | `layout.rs` | Obstacle margin |
| **Gutter (TD/BT)** | Right side | `render/mod.rs` | Back-edge routing |
| **Gutter (LR/RL)** | Bottom side | `render/mod.rs` | Back-edge routing |

### Routing Abort

When pathfinding exceeds 2500 steps:
```
termiflow: warning: routing aborted after 2500 steps
```
Edge may render incorrectly or be dropped.

---

## Parser Constraints

| Constraint | Value | Behavior |
|------------|-------|----------|
| **Node ID chars** | `[a-zA-Z0-9_]` | Others rejected |
| **Node ID length** | Unlimited | No enforcement |
| **Forward references** | Supported | Two-pass parsing |
| **Nested subgraphs** | Not supported | Warning + ignored |
| **Multiple directions** | First wins | Warning |

### Valid Node IDs

```
✅ A, Node1, my_node, A1B2C3
❌ my-node (hyphen), A B (space), @node (special char)
```

---

## Configuration Precedence

```
1. CLI flags           (highest)
2. In-file directives  (%% termiflow: key=value)
3. Config file         (~/.config/termiflow/config.toml)
4. Defaults            (lowest)
```

### Default Values

| Key | Default | Type |
|-----|---------|------|
| `max_label_width` | 20 | Number |
| `wrap_labels` | false | Boolean |
| `max_label_lines` | 1 | Number |
| `crop` | true | Boolean |
| `pad` | 0 | Number |
| `strict_parsing` | false | Boolean |

---

## Shape-Specific Limits

### Shapes Supporting Multiline

- Rectangle ✅
- Rounded ✅
- Stadium ✅
- Hexagon ✅
- Database ✅
- Subroutine ✅
- Asymmetric ✅
- Parallelogram ✅
- Trapezoid ✅

### Single-Line Only

- Diamond ❌ (single line)
- Circle ❌ (single line)

---

## Edge Label Limits

| Constraint | Value | Notes |
|------------|-------|-------|
| **Max width** | **12 chars (HARDCODED)** | `render/mod.rs:1541` |
| **Convergent edges** | **10 chars** | Even shorter! |
| **Configurable** | ❌ No | No `--max-edge-label` flag |
| **Position** | Vertical segment only | TD/BT: on vertical, LR/RL: inline |
| **Line breaks** | Not supported | Single line only |
| **Fan-in/out** | Labels skipped | Labeled edges with degree > 1 |

### Edge Label Truncation

```rust
// src/render/mod.rs:1540-1541
fn format_edge_label(label: &str) -> String {
    format_edge_label_with_limit(label, 12)  // HARDCODED!
}
```

**Example:**
```
Input:  A -->|validates credentials| B
Output: validates c…   (12 chars with ellipsis)
```

**Gap**: Node labels allow 20 chars (configurable), but edge labels are stuck at 12.

---

## Character Encoding

| Character Type | Width | Notes |
|----------------|-------|-------|
| ASCII | 1 | Standard |
| CJK | 2 | Double-width |
| Emoji | Variable | May cause alignment issues |
| Combining marks | 0 | Supported |

### Example

```rust
display_width("Hello")   = 5   // 5 ASCII chars
display_width("日本語")   = 6   // 3 CJK × 2 width
display_width("café")    = 4   // 4 chars (é = 1 width)
```

---

## Error Thresholds

### Fatal Errors (Always Stop)

- Empty file
- No direction found
- Unsupported diagram type

### Strict Mode Fatals

- Nested subgraphs
- Unsupported syntax (classDef, style, etc.)
- Malformed edges/nodes
- Nested brackets in labels
- Pipe in labels
- Multiple directions
- Content before direction

### Never Fatal

- Node auto-created from edge reference

---

## Memory & Performance

| Metric | Value | Notes |
|--------|-------|-------|
| **Max canvas memory** | ~100KB | 500×200 chars |
| **Node storage** | HashMap | O(1) lookup |
| **Edge storage** | Vec | O(n) iteration |
| **Routing complexity** | O(n²) worst | Capped at 2500 steps |

---

## Environment Variables

| Variable | Effect |
|----------|--------|
| `TERMIFLOW_DEBUG_TIMING` | Enable timing debug output |
| `TERMIFLOW_DISABLE_PORTALS` | Disable portal carving |

---

## Silent Behaviors

These operations are **silent** (no warning):

1. Out-of-bounds canvas `set()` - no-op
2. Out-of-bounds canvas `get()` - returns space
3. Edge to invisible node - skipped
4. Unknown config keys - ignored

---

## Quick Limits Summary

```
┌──────────────────────────────────────────┐
│  TERMIFLOW LIMITS AT A GLANCE            │
├──────────────────────────────────────────┤
│  Canvas:       500 × 200 chars           │
│  Node label:   20 chars (configurable)   │
│  Edge label:   12 chars (HARDCODED!)     │
│  Lines:        3 max (configurable)      │
│  Box width:    7-26 chars                │
│  Box height:   3 rows (fixed)            │
│  Routing:      2500 steps max            │
│  Gutter:       4 chars                   │
└──────────────────────────────────────────┘
```

---

*End of Limits Document*
