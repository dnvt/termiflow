# Visual lesson: safe BT sibling transitions should stay straight

Status: Promoted rule
Source epoch: h145d-bt-straight-2026-08-21
Fixture family: `collision_sibling_triple_bt`, `subgraph_chain_bt`

## Observation

Strict bottom-to-top chains of titled sibling subgraphs rendered a repeated
long elbow between each pair of groups. The route crossed one boundary at the
source lane, moved sideways through the open corridor, then entered the target
at a different lane. In the terminal this read as a dangling `┌────┘`/`+----+`
shoulder attached to the group border, even though the edge was connected and
all machine route checks were green.

## Hypothesis and falsifier

Hypothesis: the strict-chain endpoint contract was choosing a non-collinear
receiver lane to separate boundary roles that were already distinct at the
middle sibling. Prefer the source lane when it is inside the target node,
title-safe, and separated from the next source and prior target lanes. Keep the
existing lateral-lane search as the fallback when that predicate is false.

The hypothesis is falsified if a safe transition still emits a corridor elbow,
if two middle-boundary roles collapse, if a target title or arrow is damaged,
or if an unsafe/near-miss chain loses its required turn. Check ASCII/Unicode,
default/optimized, canonical/authored lanes and the untouched BT parallel,
nested, complex, and non-BT controls.

## Evidence

- Focused regression: `strict_bt_sibling_chain_prefers_straight_target_portal_lanes` and `strict_bt_sibling_chain_separates_middle_boundary_roles` in `tests/bt_sibling_chain_visual.rs`; the direction-matrix, subgraph-boundary, BT parallel, independent-oracle, and layout-contract tests pass.
- Complete-corpus result: 241 inputs, 964 packet rows per policy lane, 952 renderable rows, and 12 separately governed expected-error rows; packet validation reports zero findings and intact hashes.
- Human-eye result: the eight changed rows per policy lane are the two strict BT fixtures across both styles and modes; they are the only frame-hash changes from the preceding label-fix epoch. All eight changed rows per lane were freshly inspected and recorded as improved-but-still-watched: the elbows are gone, while straight boundary-rail ownership remains open. The unchanged rows are hash-bound carry-forward decisions; existing BT title, corridor, and dense-route watches remain open.
- Golden result: the explicit intent-bound workflow approved exactly four snapshots, limited to the two strict BT fixtures in ASCII and Unicode; the full golden check is clean afterward.

## Promoted rule or next experiment

For a topology-proven strict BT sibling chain, a source-aligned target portal is
the preferred route because it preserves the simplest human-eye reading. Keep
middle boundary roles distinct and retain the separated-lane fallback for
title-unsafe, node-unsafe, colliding, or near-miss scenes. This is a narrow
endpoint-contract rule, not permission for a global layout shift or for
collapsing unrelated sibling channels.
