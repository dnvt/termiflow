# Project: Horizontal Display Consistency

**Status:** Complete
**Roadmap Slot:** Completed rendering-precision child slice
**Owner:** Maintainer
**Date:** 2026-04-10
**Aligned To:** Rendering-precision program

## Objective

Make the same graph render with directionally consistent semantics across
`TD`, `LR`, and `RL` without faking containment. For the `Service Layer` /
`Data Layer` family, `LR`/`RL` should keep the `TD` visual feel of a nested
pipeline while preserving the real sibling memberships of the graph.

## Current Diagnosis

The current horizontal parity fix is not correct.

- It improved the old flat-sibling `LR`/`RL` layout, but it promoted a sibling
  visual-nesting case into actual containment.
- In the bad `LR` frame, `Order Service` is rendered inside the `Data Layer`
  box. That changes graph semantics and does not match the `TD` reference.
- The `┼→`-style left-wall seam on `Data Layer` is also a border/portal
  composition bug, even when the audit summary currently reports `Clean`.

## Root Cause

The current `LR`/`RL` heuristic is using a declared-nesting-style outcome for a
different problem.

- `subgraph_complex_td` already reads as visually nested because stacked
  geometry lets `Data Layer` sit inside the `Service Layer` envelope while
  still excluding parent-only nodes.
- The horizontal implementation copied the "child becomes contained inside the
  parent envelope" part, but it did not preserve the equally important keepout:
  parent-only nodes like `Order Service` must remain outside the child box.
- The current acceptance tests are therefore too weak in the wrong direction:
  they reward horizontal containment, when the real requirement is horizontal
  visual overlap with semantic separation.

## Decision

Recommended model: **visual nesting without semantic containment promotion**.

- `Service Layer` may visually envelop `Data Layer` in `LR`/`RL`.
- `Data Layer` must still be bounded only around its own nodes and route lanes.
- Parent-only nodes and parent-local trunks must remain outside the child box.
- Portal seams on the child wall must render as clean openings, not junction
  corruption.

Rejected alternatives:

1. Revert all horizontal visual nesting.
   This would restore sibling correctness, but it would also throw away the
   parity we actually want from the `TD` reference.

2. Keep the current containment promotion.
   This preserves the visual "nesting" impression, but it is semantically
   wrong and already contradicted by direct visual inspection.

## Execution Plan

1. Rewrite the acceptance surface.
   Replace the current "horizontal visual nesting means SG2 is contained in SG1"
   assumption with the correct one:
   - SG1 may visually contain SG2.
   - SG2 must not contain `S1` or `S2`.
   - `Response Builder` must remain outside SG1.
   - `Data Layer` title row must still staircase below `Service Layer`.
   - The rendered left/right wall seams must not use junction-like glyphs for
     a simple entry opening.

2. Split detection from placement.
   Keep a detector for the multi-lane sibling pipeline case, but stop using it
   to justify true child-containment geometry.
   - Detect the sibling visual-nesting candidate.
   - Compute a parent-local pocket for the child box.
   - Preserve explicit keepouts from parent-only nodes that overlap the same
     vertical band.
   - Mirror the rule for `RL`.

3. Add a child keepout rule.
   Introduce a dedicated `LR`/`RL` keepout pass for visually nested sibling
   children:
   - Child left wall must stay after the right edge of overlapping parent-only
     content in `LR`.
   - Child right wall must stay before the left edge of overlapping parent-only
     content in `RL`.
   - Cross-boundary parent→child routes must still have enough lane budget to
     enter cleanly after the keepout is enforced.

4. Repair the portal/border composition path.
   Audit the side-entry path so visually nested sibling entries render as a
   clean side opening or stub, not a `┼`/junction seam.
   - Tighten render assertions around the exact bad seam family.
   - If the critic still misses it, add or refine a finding for this pattern.

5. Rebaseline only the intended fixtures.
   Update the `subgraph_complex_lr` / `subgraph_complex_rl` golden snapshots
   only after the semantic keepout and seam cleanup are both verified.

## Exit Criteria

- `LR` and `RL` keep the same graph visually nested in the same qualitative
  way as `TD`.
- `Order Service` is outside `Data Layer` in both `LR` and `RL`.
- No junction-like side-wall seam appears on the child border.
- `cargo fmt --check`, `cargo clippy`, `cargo test`, and the targeted golden
  checks pass.

## Execution Outcome

- Rewrote the horizontal acceptance surface around semantic separation instead
  of sibling-containment promotion.
- Added an `LR`/`RL` child-pocket keepout so visually nested sibling children
  stay clear of overlapping parent-only content.
- Re-applied horizontal external-node clearance after the child pocket shift so
  downstream nodes like `Response Builder` stay outside the visually nested
  outer envelope.
- Repaired side-wall portal ownership/restoration so horizontal entry seams
  render as simple portal openings instead of junction corruption.
- Rebaselined the intended `subgraph_complex_lr` / `subgraph_complex_rl`
  snapshots in both ASCII and Unicode.

## Verification

- `cargo fmt --check`
- `cargo clippy`
- `cargo test`
- direct audited renders for `subgraph_complex_{lr,rl}` in ASCII and Unicode
- targeted snapshot diffs for:
  - `tests/fixtures/expected/subgraph_complex_lr.ascii.txt`
  - `tests/fixtures/expected/subgraph_complex_lr.unicode.txt`
  - `tests/fixtures/expected/subgraph_complex_rl.ascii.txt`
  - `tests/fixtures/expected/subgraph_complex_rl.unicode.txt`

## Immediate Next Run

Return to `planning/RENDERING_PRECISION_PROGRAM.md` and choose the next child
slice intentionally; this horizontal consistency repair is closed.
