# Task: BT Subgraph Title Position

**Status:** Completed
**Roadmap Slot:** Historical Reference
**Owner:** Maintainer
**Timeline:** 2026-04-01 -> 2026-04-03
**Aligned To:** `planning/PRE_OSS_COORDINATION.md` stage 2 stabilization
**Decision Links:** `DEC-003` indirectly via pre-OSS sequencing; rationale from the current BT subgraph defect review

## Objective

Make titled subgraphs in `Direction::BT` render their title on the lower edge
of the subgraph instead of the top/title row, while keeping BT routes visually
clean and preserving existing behavior for TD/LR/RL. In crowded BT layouts,
solve collisions by separating the layout / routing contract rather than
falling back to a top-interior title row.

## Scope

**In:** BT titled-subgraph title placement, BT title metadata/protection rules,
BT repair logic, BT route/title-row cleanup assumptions, and the tests/fixtures
needed to prove the new contract.

**Out:** General subgraph redesign, nested-subgraph support, LR/RL title
positioning changes, or unrelated BT routing cleanup outside titled subgraph
behavior.

## Current State

Today the renderer treats BT titled subgraphs as "top border plus title row just
under it" rather than as a true bottom-anchored container title.

Direct evidence from current code:
- `src/render/mod.rs` draws BT titles at `rect.y + 1`
- `src/render/repair.rs` restores BT titles at `bounds.y + 1`
- `src/render/edge.rs` protects and avoids a BT title row near the top of the
  subgraph

That matches the currently broken output pattern the user called out: the title
remains near the top even though visual flow is bottom-to-top.

## Desired End State

For titled BT subgraphs:
- the title sits on the literal bottom border row of the subgraph
- crowded sibling BT layouts stay on that bottom border row by reserving enough
  layout / routing space rather than falling back to the top
- incoming / outgoing BT routing does not corrupt the landed title placement
- title ownership / repair / route-protection logic all agree on the new title
  location
- TD/LR/RL output stays unchanged

## Success Criteria

- [x] BT titled subgraph fixtures place the title on the lower edge row rather
      than on the top/title row where that placement is safe.
- [x] No `SubgraphTitleCorrupted` findings remain for the targeted BT fixtures
      after the change.
- [x] BT routing still shows visible shafts / portals where expected after the
      title row moves.
- [x] Existing TD/LR/RL subgraph title behavior remains unchanged.
- [x] Tests are added or updated to assert the BT-specific title position
      contract explicitly, not just "title row is clean."

## Likely Touchpoints

- `src/render/mod.rs`
  Title draw position, title metadata ownership, BT cleanup assumptions.
- `src/render/repair.rs`
  `restore_subgraph_title()` currently restores BT titles near the top.
- `src/render/edge.rs`
  `is_subgraph_title_cell()`, BT convergent / portal logic, and any title-row
  avoidance paths that currently assume a top-adjacent BT title row.
- `tests/default_print.rs`
  Current assertions look for a clean title row but not for BT bottom placement.
- `tests/render_options_api.rs`
  Add or update BT-specific behavioral assertions.
- `tests/fixtures/expected/*bt*`
  Golden outputs will likely change for BT subgraph cases.

## Dependencies

- Parent sequencing remains `planning/PRE_OSS_COORDINATION.md` stage 2.
- This task should happen before broader doc alignment because public docs should
  describe the final BT subgraph behavior, not the current bug.
- Any work here must preserve the current green baseline (`cargo test`,
  `cargo clippy`, `cargo fmt --check`).

## Risks & Mitigations

- Moving the title may break BT portal / merge routing assumptions
  -> Update title-protection helpers and BT routing tests together.
- Repair logic may reintroduce the title at the old top position
  -> Treat draw, metadata, and repair as one contract change.
- Some current BT cleanup code may become dead or inverted after the move
  -> Re-evaluate cleanup helpers instead of carrying forward the old mental
     model.
- Golden changes may hide regressions in non-target BT cases
  -> Add explicit focused tests before regenerating or accepting fixture changes.

## Resolved Notes

- The landed contract is the literal bottom border row, not a lower interior
  fallback row.
- Crowded BT compositions were solved by widening the BT layout / envelope
  contract so titled parent and child envelopes do not fight for the same
  bottom row.
- BT single-edge and fanout entry paths now route around the bottom-border title
  span instead of using that row as a normal routing surface.

## Evidence To Gather

- Fresh render output for:
  - `tests/fixtures/inputs/subgraph_fanin_bt.md`
  - `tests/fixtures/inputs/subgraph_fanout_bt.md`
  - BT sibling-subgraph fixtures if touched
- Focused test evidence that the BT title is bottom-anchored.
- Critic findings before and after the change for targeted BT fixtures.

## Experiment Log

**Hypothesis:** Treating BT titled subgraphs as truly bottom-anchored containers
will both match directional intuition and reduce the current title-row
corruption pressure near the top of the subgraph.

**Intervention:** Move BT title placement and all title-aware routing / repair
logic from the current top-adjacent position to a bottom-anchored one.

**Expected Observation:** Targeted BT fixtures render with the title at the
bottom, with no title corruption and no new shaft / portal regressions.

**Actual Observation:** BT titles now anchor to the literal bottom border row
for clean cases such as `subgraph_fanin_bt` and `subgraph_labels_bt`, and
crowded cases such as `subgraph_complex_bt` and
`collision_parallel_cross_bt` stay clean by reserving BT entry / envelope space
instead of falling back to a top-interior title row. Targeted BT audit cases
are clean after the renderer / critic / repair / layout contract change.

**Conclusion:** Completed. The BT title contract is now explicit, tested, and
verified.
