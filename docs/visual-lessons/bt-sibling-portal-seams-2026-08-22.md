# Strict BT sibling portals need directional border seams

Status: Promoted focused rule; broader titled BT rails remain a Watch
Source epoch: H150 strict BT sibling-seam experiment, 2026-08-22
Fixture family: `collision_sibling_triple_bt`, `subgraph_chain_bt`, and
`subgraph_multi_bt`

## Observation

In strict stacked BT sibling scenes, a cross-boundary route can run through a
titled subgraph border as an uninterrupted vertical rail. The route is
connected, but a human reader can mistake the rail for a continued group
border or a shared trunk. The risk is tiny and local: the exact boundary cell
looks like an ordinary `│`/`|` continuation instead of an intentional route
opening.

The H150 frames add directional border seams at topology-owned portal cells:
`┬`/`┴` for Unicode, the corresponding mixed-weight box-drawing seam when the
subgraph border is heavy, and the truthful `+` representation for ASCII. The
seam clarifies the crossing without changing endpoint order or route
connectivity. `collision_sibling_subgraphs_bt` remains a separate negative
control because its two parallel cross-boundary edges do not satisfy the
strict-chain scene contract.

## Hypothesis and falsifier

The strict BT sibling scene selector already owns the route and portal
decisions, so final portal projection can safely re-apply a directional seam
for every selected sibling boundary after generic repair passes. The critic
must recognize those topology-owned seams as valid portal markers.

The hypothesis is falsified if a seam is emitted on a non-owned boundary, a
title or node is overwritten, a route loses an arrowhead or trace, the rail
becomes harder to follow, or any canonical/authored style-mode homolog gains a
new border collision. The parallel BT, sibling-subgraph, LR/RL, TD, label,
density, and cycle families remain controls rather than being silently folded
into this rule.

## Evidence

- Focused owner and oracle: `src/render/edge/bt_sibling_scene.rs`,
  `src/render/portal_projection.rs`, `src/render/critic.rs`, and
  `tests/subgraph_boundary_arrows.rs`.
- Focused command: `cargo test --locked --test subgraph_boundary_arrows`.
- Complete-corpus result: 241 inputs, 964 rows per policy lane, 952
  renderable rows, and 12 separately governed expected-error rows; both H150
  packets validate with zero machine findings.
- Fresh human-eye result: 952/952 perceptual decisions in each lane; the
  canonical lane records 556 pass / 396 P2 watch and the authored lane records
  604 pass / 348 P2 watch, with zero fail and no missing or duplicate rows.
- Golden result: six intentional snapshots approved with an explicit H150
  seam intent; the subsequent golden check is current.

## Promoted rule or next experiment

Keep the seam rule limited to the strict BT sibling-chain selector and retain
the broader boundary rails as watches. The next experiment should target
`collision_sibling_subgraphs_bt` only if a topology-owned parallel-boundary
policy can distinguish its two rails without making the frame read as a bus.
Long-label ellipsis, junction attachment, dense crossing, and cycle-gutter
readability remain independent experiments. Every follow-up must regenerate
both full policy lanes and perform fresh one-frame review again.
