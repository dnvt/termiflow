# Visual lesson: flat TD/TB external entries need a literal title-gutter lane

Status: Promoted local TD/TB rule; BT counterpart remains a bounded watch

## Observation

In the flat, titled one-entry fixtures `subgraph_single_td` and
`subgraph_outside_td`, the connected external edge could approach the live
title-safe portal one column away from the source lane. The resulting title
row contained a tiny `+-+`/`┌─┘`-like shoulder. A terminal reader can mistake
that shoulder for a damaged group border even though the source, target,
arrowhead, and geometry reports are present.

## Hypothesis and fix

The TD/TB slot collector and route lowerer disagreed about the clearance
margin around the title token. For exactly one unlabeled direct external entry
into a flat titled subgraph, the live portal may use the literal title gutter
when it is safe; the layout stage then aligns the external source to that
owned lane. The rule is topology-gated and remains fail-closed for nested,
multi-entry, labeled, sibling, and crowded scenes.

## Falsifier and regression

The rule is falsified if any affected ASCII/Unicode or default/optimized
homolog recreates the title hook, loses an arrow or shaft, collides with the
title/node/border, or changes a negative-control scene. The focused tests are:

```text
cargo test --locked --features qa --test subgraph_boundary_arrows flat_td_external_entries_use_one_title_gutter_lane_across_matrix -- --nocapture
cargo test --locked --features qa --test subgraph_boundary_arrows -- --nocapture
cargo test --locked --features qa --bin termiflow-qa td_tb_title_boundary_review_queues_injected_hook_but_not_repaired_entries -- --nocapture
```

The BT source-centering experiment improved the external shaft but did not
remove the target-side title-gutter elbow. That endpoint-staging hypothesis
was falsified and remains a named watch; it must not be generalized from the
TD rule.

## Evidence and next gate

The current corpus contract remains 241 inputs, 964 rows, 952 renderable
frames, and 12 separately governed expected-error rows in each canonical and
authored/no-override lane. A fresh complete packet and one-frame review in
both lanes are still required before resolving the historical watch or
approving any golden.
