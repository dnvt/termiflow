# Termiflow Internal Limits & Precision Notes

> **Reviewed:** 2026-04-10
> **Purpose:** quick truth source for current beta-era limits and approximations

---

## Canvas And Spacing

| Item | Current value | Notes |
|------|---------------|-------|
| Max canvas width | 500 cells | Clipped with warning when exceeded |
| Max canvas height | 200 rows | Clipped with warning when exceeded |
| Min box width | 5 cells | Shared spacing default |
| Default node height | 3 rows | Taller only when wrapped multiline labels are enabled |
| Row spacing | 2 rows | Shared spacing default |
| Column spacing | 3 cells | Shared spacing default |
| Cycle gutter | 4 cells | Used for routed back-edges |

These numbers live in the shared spacing layer, not in ad hoc renderer-local
constants.

---

## Label Behavior

| Item | Current value | Configurable |
|------|---------------|--------------|
| Node label width budget | 20 columns by default | `--max-label` |
| Edge label width budget | 20 columns by default | `--max-edge-label` |
| Label wrapping | Off by default | `--wrap` |
| Max wrapped lines | 1 by default | `--max-lines` |
| Node truncation suffix | `...` | No |
| Edge truncation suffix | `…` | No |

Current behavior:

- Wrapping and truncation use display columns, not byte count.
- Node-label measurement, wrapped-line splitting, preview/status wrapping, and
  edge-label truncation are grapheme-safe.
- Manual line breaks are normalized from `CRLF`, `<br>`, `<br/>`, `<br />`, and
  literal `\n`.
- In single-line mode, normalized breaks collapse to spaces before truncation.

Important boundary:

- The final diagram canvas is still char-backed. Width budgeting is now
  consistent, but some multi-codepoint grapheme composition on the main canvas
  can still depend on terminal behavior because a rendered cell stores a single
  character, not a full text cluster.

---

## Subgraphs

Current declared support:

- Nested `subgraph ... end` containers are supported in `TD`, `LR`, `BT`, and
  `RL`.
- Parent/child containment, title headroom, portal allocation, and clean
  side-wall openings are all part of the active renderer contract.

Border-crossing rule:

- Borders are portal boundaries, not merge or branch targets.
- `TD` / `TB` / `BT`: a real top/bottom pierce may look like a crossing on the
  border row.
- `LR` / `RL`: a side-wall pierce must stay a clean horizontal opening rather
  than turning into a junction glyph on the border column.

---

## Preview Modes

- `--watch` is the safer live-preview mode when normal scrollback matters.
- `--tui` remains a partial alternate-screen mode because raw-mode input and
  fullscreen interaction still depend on the terminal emulator.
- Presenter updates use synchronized update markers where supported and degrade
  cleanly when they are not.

---

## Routing And Quality Surface

- Route-dense nested fixtures now have explicit regression coverage in the
  curated visual-audit suite.
- Criterion includes a `route_dense_subgraphs` benchmark group.
- The critic includes a reusable oracle for the `LR` / `RL` side-wall portal
  contract so bad border merges are rejected without relying only on goldens.

---

## Still Approximate

- Unicode ambiguous-width behavior still depends on terminal configuration.
- The main canvas remains char-backed, so full multi-codepoint grapheme scene
  composition is deferred to the deeper cell-scene-graph architecture plan.
- Mermaid styling/classes (`style`, `classDef`, `:::`), edge IDs, markdown
  labels, and `@{}` shapes remain unsupported.

---

## Do Not Repeat

The following claims are no longer true and should not reappear in docs:

- "Nested subgraphs are not supported"
- "Line breaks are not supported"
- "Edge labels are hardcoded to 12 columns"
- "`--wrap` is only an experimental placeholder"
