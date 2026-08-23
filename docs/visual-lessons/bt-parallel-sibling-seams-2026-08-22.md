# Parallel BT sibling rails need an exact scene-owned seam

Status: Promoted focused rule; generic portal ownership remains a Watch
Source epoch: H151 parallel BT sibling-seam experiment, 2026-08-22/23
Fixture family: `collision_sibling_subgraphs_bt`, with strict-chain and
parallel sibling controls

## Observation

The two cross-boundary edges in `collision_sibling_subgraphs_bt` form two
independent rails between titled sibling groups. Before H151, the rails were
connected but their boundary crossings could read like uninterrupted group
borders or a shared trunk. The tiny ambiguity occurs at the title-adjacent
border cells, not at the node endpoints.

## Hypothesis and falsifier

The graph-owned `bt_sibling_target_entry_scene` selector already identifies
the exact two-rail topology. Reusing that selector in final portal projection
and critic acceptance should add directional border seams while preserving
independent rails, node lanes, endpoint identity, and route traces.

The hypothesis is falsified if the selector matches a non-equivalent topology,
collapses the rails into a bus, overwrites a title or node border, detaches an
arrow, changes a non-BT homolog, or produces a new machine or human-eye
failure in either policy lane. Long labels, junctions, density, fan-in, cycles,
and generic portal ownership remain separate controls.

## Evidence

- Owner and projection: `src/render/edge/bt_sibling_scene.rs` and
  `src/render/portal_projection.rs`.
- Critic allowlist: `src/render/critic.rs`.
- Focused oracle: `tests/subgraph_boundary_arrows.rs`.
- Both full policy packets: 964 rows each, zero machine findings and complete
  integrity; both cover every direction, style, and mode.
- Fresh perceptual review: 952/952 in each lane, with zero fail; residual P2
  watches remain explicit rather than being counted as resolved.
- Golden: exactly two intentional snapshots approved and final check current.

## Promoted rule or next experiment

Keep the exact scene-owned parallel seam rule. The next experiment should target
title-boundary ownership and clearance with matched portal controls; it must
not broaden this selector based on fixture names or fold unrelated labels,
junctions, density, fan-in, or cycle behavior into the same patch. Every
follow-up regenerates both complete policy lanes and performs fresh one-frame
review again.
