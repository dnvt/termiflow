# Mixed sibling targets need a real visual gap

Status: promoted focused rule; complete-corpus perceptual closure and
independent sibling-chain watches remain open.

## Observation

In `collision_sibling_subgraphs_td` and its BT homolog, the internal route
`C -> D` and the cross-subgraph route `B -> D` both arrived at `D`. The old
frame rendered their target tips one cell apart (`↓ ↓` / `↑ ↑`). Connectivity
and critic checks were clean, but the receiver looked like a cramped shared
junction. The repaired frame keeps two spacer cells between the tips
(`↓  ↓` / `↑  ↑`) without losing either route.

## Hypothesis and falsifier

The exact mixed sibling/internal target lowerers own this ambiguity. Prefer a
three-column minimum between two title-safe target entries, while retaining a
two-column fallback only when the node cannot provide the wider pair. The
policy is limited to the typed TD/TB and BT scene selectors.

The hypothesis is falsified if a target arrow or shaft disappears, the
internal/cross edge identities change, the wider pair collides with a node or
title, or a tight, triple, nested, labeled, LR, or RL homolog worsens.

## Evidence

- Focused regression: `mixed_vertical_sibling_targets_keep_a_readable_entry_gap`
  in `tests/subgraph_boundary_arrows.rs`.
- Focused gates: 13 boundary-arrow tests, 77 render-option tests, 33
  independent oracles, and 37 layout tests pass across ASCII/Unicode and
  default/optimized modes.
- Complete-corpus result: both H106 packets validate at 241 inputs and 964
  rows per policy lane: 952 renderable rows and 12 expected-error rows.
- Human-eye result: all 8 changed TD/BT rows per policy lane pass; the BT
  parallel-title hook and TD triple-chain boundary-hook remain separate watches.
  The full 952-row perceptual drainage is still open.
- Golden result: no golden approval is implied.

## Promoted rule

When semantically distinct internal and cross-subgraph edges share a vertical
receiver, reserve a visibly separated target-entry pair in the scene-owned
lowerer. Do not use a clean critic score or a connected shaft as proof that
adjacent arrowheads are human-readable.
