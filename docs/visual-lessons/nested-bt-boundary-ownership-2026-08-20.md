# Visual lesson: nested BT portals need boundary-by-boundary ownership

Status: Watch; topology hypothesis is open

## Observation

The current nested BT entry frame keeps every node, label, arrow, and border
connected, but one long vertical rail can pierce the Deep, Inner, and Outer
title envelopes at the same apparent column. The small title-adjacent elbow
near the deepest target makes that rail briefly read like a shared boundary
trunk rather than one edge entering one nested node. ASCII and Unicode preserve
the same ambiguity, and authored/no-override rendering does not remove it.

This is a human-eye ownership concern, not a machine topology failure:
critic, raw, and geometry checks can all be clean while the reader still has
to trace the route through several borders.

## Bounded result: quiet target turns are now promoted

The first bounded experiment did not attempt to solve boundary-by-boundary
ownership. It promoted the smaller, independently falsifiable rule that a
nested BT entry must not choose a common lane only one or two columns from both
the external source stem and the receiving arrow lane when a title-safe lane
with a visible shaft exists. Portal-slot discovery, route lowering, and the
portal trace now share that selector, so the visual repair cannot be hidden by
layout/render disagreement.

The `subgraph_nested_bt` ASCII/Unicode × default/optimized matrix now keeps a
multi-cell turn before both arrowheads, preserves both traced edges, and stays
critic-clean. This is a promoted local rule, not golden approval for nested
boundary ownership: the long common rail remains a P2 watch and still needs a
separate channel-allocation experiment.

Reproduce the focused matrix with:

```text
cargo test --locked --features qa --test subgraph_boundary_arrows nested_bt_external_entry_keeps_a_quiet_target_turn -- --nocapture
```

## Hypothesis and falsifier

The nested BT entry lowerer and portal collector currently agree on one common
safe lane for the complete entered-boundary chain. That agreement prevents
detached seams, but it may over-optimize straightness at the expense of local
boundary ownership. A bounded follow-up should compare a boundary-by-boundary
channel allocation against the current common-lane control, with explicit
claims at every entered boundary and a negative control for one-boundary BT
entries.

The hypothesis is falsified if a stepped allocation produces a worse shared
trunk, title collision, border-shaped elbow, detached arrow, route mismatch,
or regression in nested TD/TB and flat BT sibling homologs. It is promoted only
when the complete ASCII/Unicode × default/optimized matrix, authored controls,
and a holdout nested topology are locally legible without route tracing.

## Next experiment

Inspect `subgraph_nested_bt` and its mirrored/style/mode homologs one frame at
a time, then prototype one shared portal/lowering policy in
`src/portals.rs` and `src/render/edge/subgraph.rs`. Rebuild both complete
requested-style and authored packets before resolving this watch. Keep the
packet, provider traces, and private Maestro capsules out of the public OSS
lesson; publish only the observation, owner hypothesis, falsifier, and
reproducible repository commands.
