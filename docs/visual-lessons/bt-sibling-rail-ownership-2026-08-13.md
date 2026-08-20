# Visual lesson: BT sibling rails need role ownership, not a larger gap

Status: Falsified spacing-only candidate; route-owner hypothesis remains open

## Observation

The strict BT sibling-chain fixtures `collision_sibling_triple_bt` and
`subgraph_chain_bt` can make successive titled boundaries look like one bus:
long rails pierce multiple group borders and short turns are easy to read as
extra border fragments. The visible problem is transition ownership, not a
missing arrow or a disconnected route.

## H118 falsified minimum-gap experiment

A bounded candidate raised `BT_SIBLING_MIN_RAIL_GAP` in
`src/layout_render_contract.rs` from 3 to 5. In the ASCII/optimized collision
case, the endpoint contract could no longer allocate the two distinct middle
boundary roles. The scene fell back to one shared shaft, and
`tests/bt_sibling_chain_visual.rs` failed
`strict_bt_sibling_chain_separates_middle_boundary_roles`.

The candidate was reverted. The focused BT sibling-chain suite and the
subgraph-boundary suite pass again. Increasing spacing alone is therefore not
a valid repair: it can make the contract unsatisfiable and reduce visible
ownership rather than improve it.

## Next hypothesis and falsifiers

The next candidate must change the topology-owned lane/route agreement in
`src/layout_render_contract.rs` and `src/render/edge/bt_sibling_scene.rs`,
while preserving title-safe target entries, distinct source/target lanes, two
middle-boundary roles, and one-column frame ownership. It is falsified by a
shared shaft, a collapsed middle boundary, a title collision, a generic
junction, a missing arrow, or a regression in TD/TB/LR/RL, nested, labeled,
parallel, and non-chain controls.

## Public boundary

This lesson records the visible result, repository ownership, focused test,
and falsifiers. Private Maestro capsules, provider traces, prompts, and
transient packet paths remain outside the OSS repository.
