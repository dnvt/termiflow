# BT sibling entries need a quiet row on both sides of the turn

Status: Promoted focused rule; complete-corpus re-review remains required
Source epoch: post-H146f bounded BT sibling quiet-row experiment, 2026-08-22
Fixture family: `subgraph_direct_bt`, `collision_sibling_tight_bt`,
`collision_sibling_triple_bt`, and BT chain homologs

## Observation

A BT edge entering a titled sibling subgraph can render an arrow immediately
beside a horizontal turn, or leave the turn directly against the title row.
Both forms are machine-traceable but read as a tiny damaged hook to a human.
The defect was especially visible in the direct and tight sibling fixtures.

## Hypothesis and falsifier

For a flat titled target receiving exactly one edge from a sibling subgraph,
the envelope must reserve one extra bottom row and the route must use the
sequence `arrow → vertical shaft → turn → vertical shaft → title`. External
source entries are excluded: applying the same upward shift there produced a
worse arrow-adjacent hook in `collision_edge_corner_bt`.

The hypothesis is falsified if the extra row creates a new title collision,
breaks arrow/geometry traceability, changes unrelated BT topologies, or fails
to preserve the strict stacked-chain corridor contract.

## Evidence

- Topology-owned envelope predicate: `src/portals/envelopes.rs`.
- Sibling-only route policy: `src/render/edge/subgraph.rs`.
- Focused coverage: `tests/subgraph_boundary_arrows.rs` quiet-turn and strict
  chain tests.
- Golden workflow: 12 BT snapshot changes were approved with an explicit
  intent after visual inspection; external `collision_edge_corner_bt` stayed
  on its cleaner established route.

## Next experiment

Regenerate both canonical and authored full-corpus packets and review every BT
title/portal homolog. Keep the rule only if repeated human-eye review confirms
that the added row reduces hooks without turning long vertical corridors into
new whitespace defects.
