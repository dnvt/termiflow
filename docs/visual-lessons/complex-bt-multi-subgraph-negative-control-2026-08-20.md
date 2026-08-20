# Visual lesson: complex BT multi-subgraph scenes are a negative control

Status: bounded watch; protects the exact single-entry BT improvement

## Observation

The exact single-entry BT source-lane rule improved `subgraph_single_bt`,
`subgraph_outside_bt`, and the narrow one-entry homologs. An earlier broader
predicate also moved the API Gateway in `subgraph_complex_bt`, a two-subgraph
scene. That changed an established complex topology without proving ownership
for its shared Data/Service rails.

## Hypothesis and guard

Single-entry source centering is safe only when the graph has one flat titled
subgraph and one direct unlabeled external entry. The complex BT scene must
remain an explicit negative control: `graph.subgraphs.len() == 1` is part of
the predicate, and multi-subgraph routes remain owned by their existing
boundary planner until a separate complex-BT hypothesis is tested.

## Falsifier and regression

The guard is falsified if a multi-subgraph complex BT render changes merely
because the single-entry rule is enabled, or if the targeted single/outside/
narrow homologs lose their improved source shaft. The current checks are:

```text
cargo test --locked --all-features --test golden
cargo test --locked --all-features --test independent_oracles td_single_subgraph_route_transaction_has_clean_portal_attachments -- --nocapture
cargo test --locked --all-features --test subgraph_boundary_arrows -- --nocapture
```

The human-eye holdout remains open because the restored complex frame still
has long shared rails and title-adjacent elbows. A future fix must name
multi-subgraph portal ownership, inspect all four style/mode cells in both
canonical and authored/no-override lanes, and prove that the single-entry
negative control is unchanged.

## Evidence

The v10 corpus packet covers 241 inputs, 964 rows, 952 renderable frames, and
12 expected-error rows in each lane. The four changed complex-BT cells were
freshly reviewed as P2 watches; they are not silently promoted by the machine
route-clarity oracle.
