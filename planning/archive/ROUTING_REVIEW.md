# Routing Review & Refactor Notes

This branch exists to collect small visual polish fixes and to drive a larger
review/refactor of the layout + routing abstraction boundaries.

## Goals

- Improve visual quality across fixtures without reintroducing overlaps.
- Reduce routing special-casing by consolidating portal/junction semantics.
- Make direction-specific behavior explicit (policies), not ad-hoc.

## Known Hotspots

- **Junction selection**: corner/tee/cross decisions are split between routing
  (`src/render/edge.rs`) and overlap resolution (`src/render/canvas.rs`).
- **Subgraph portals**: envelope sizing, carving, reinforcement, and cross-subgraph
  routing each encode slightly different assumptions.
- **Spacing policy**: layout spacing constants and routing stem/junction lengths
  interact in non-obvious ways; needs a single "budget" model.
- **Render ordering**: edges → boxes → port stamps works, but needs clear rules
  for "what is allowed to overwrite what".

## Suggested Refactor Direction

- Introduce a single **EdgeGlyphPolicy** that decides:
  - Which junction char to use for a merge/split for each direction.
  - When to force a tee vs allow an elbow to stay a corner.
  - When to prefer preserving borders vs edge continuity.
- Move all **subgraph portal math** into one place:
  - envelope geometry (outer/inner + padding),
  - portal slot allocation,
  - carve/reinforce rules,
  - cross-subgraph route entry/exit.
- Add a lightweight **fixture runner** (already provided as `scripts/render_fixtures.sh`)
  and a checklist for manual review after each change.

## Working Checklist

1. Run `scripts/render_fixtures.sh --ascii --unicode`
2. Review outputs under `artifacts/fixtures/<timestamp>/`
3. Fix issues by category:
   - LR/RL arrow spacing
   - BT merge/split row selection
   - TD portal/title interactions
4. Re-run `cargo test -q`
