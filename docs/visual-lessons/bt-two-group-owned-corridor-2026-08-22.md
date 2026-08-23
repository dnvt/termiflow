# Two-group BT sibling routes need a traced, compact corridor

Status: Promoted focused rule; complete-corpus re-review remains required
Source epoch: post-H148 two-group BT scene-contract experiment, 2026-08-22
Fixture family: `subgraph_multi_bt` and structurally equivalent titled BT pairs

## Observation

`subgraph_multi_bt` rendered its inter-group edge through a ten-row exterior
void and the evidence marked `edge:2:B->C` as an untraced fallback edge. The
route was visible, but its empty corridor and title-adjacent elbows made the
boundary ownership look accidental.

## Hypothesis and falsifier

The smallest strict sibling scene is a two-group pair, not only a three-group
chain. Reusing the topology-owned endpoint contract and compacting the pair to
the minimum three-row corridor should preserve title clearance while making the
cross-group edge a fully traced route.

The hypothesis is falsified if a two-group homolog clips a title, loses an
arrowhead, reintroduces an untraced edge, collapses the corridor into a border
continuation, or regresses unrelated BT subgraph layouts.

## Evidence

- Shared topology predicate and compaction: `src/portals.rs` and
  `src/layout/constraints.rs`.
- Two-group endpoint contract: `src/layout_render_contract.rs`.
- Scene-owned route planner: `src/render/edge/bt_sibling_scene.rs`.
- Focused contract coverage: `tests/bt_sibling_chain_visual.rs` and the
  maintainer-fixture contract test.
- H148 canonical and authored packets: 964 rows each; `subgraph_multi_bt`
  now reports `traced_edges: 3/3` and no untraced fallback edge. The fresh
  perceptual ledger reviewed all 952 renderable rows in both lanes.

## Next experiment

Re-run the complete corpus after the next route change and keep the pair rule
only if the compact corridor remains readable across ASCII/Unicode, default/
optimized, and authored-style lanes without shifting the residual BT rail
ambiguity into another family.
