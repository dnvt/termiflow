# TD mixed target entries need a short title-safe bridge

Status: promoted focused rule; complete-corpus perceptual closure and other
boundary watches remain open.

## Observation

In `tests/fixtures/inputs/collision_sibling_subgraphs_td.md`, the target
subgraph receives one internal edge and one sibling edge. A centered pair of
target entry columns preserved both arrowheads, but the sibling route had to
make a long horizontal bridge immediately below the target title. The route
was connected, yet the bridge read like a second fragment of the subgraph
container. ASCII and Unicode showed the same ownership ambiguity in default
and optimized modes.

## Hypothesis and bounded fix

The scene planner scored target pairs by center distance and then selected the
leftmost tied pair. That score ignored the downstream distance from a
title-safe portal to the selected arrow column. For this strict flat TD scene,
the tied pair nearer the title-safe portal shortens the bridge while retaining
the minimum three-column entry gap. The rule remains local to the typed scene;
it is not a general fan-in or nested-subgraph heuristic.

## Falsifiers and regression

The hypothesis is falsified by a route crossing, a title or node collision, a
critic topology mismatch, a lost shaft/arrowhead, an entry gap below the
required spacing, or a regression in the BT, horizontal, nested, labeled, or
crowded controls. The focused coverage is:

```text
cargo test --locked --features qa --test subgraph_boundary_arrows exact_td_mixed_target_keeps_both_entries_visible_across_matrix -- --nocapture
cargo test --locked --features qa --test subgraph_boundary_arrows mixed_vertical_sibling_targets_keep_a_readable_entry_gap -- --nocapture
```

The first test covers ASCII/Unicode and default/optimized rendering, including
the absence of fallback rejection and critic findings. A fresh canonical and
authored packet is still required before closing the complete-corpus watch or
approving a golden.

## Public rule

For a typed flat TD mixed-target scene, choose target entries by downstream
portal clearance after satisfying the minimum separation, not by center
distance alone. Keep the policy topology-gated and fail closed when the
title-safe corridor cannot be proved.
