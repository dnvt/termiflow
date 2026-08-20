# Visual lesson: direct BT portals need a quiet target turn

Status: Promoted rule; focused regression added, complete-corpus watch remains open

## Observation

The direct `subgraph_direct_bt` route preserved its node, title, arrow, and
border semantics while producing a compact `++` shoulder in ASCII and a
`└┐`/`┌┘` shoulder in Unicode. The route was machine-clean, but the two
adjacent turns looked like a stray bracket attached to the target boundary.

## Hypothesis and fix

The BT portal selector correctly avoided the complete title token, but it did
not account for the receiving node's arrow lane. When those lanes were one
column apart, both boundary turns collapsed into a tiny visual hook. The same
defect also appeared when an external source node approached the target
portal one column away, so the route must protect both ends of the crossing.
The shared BT target-portal policy now moves the physical portal far enough
from the source stem and arrow lane to leave at least two visible shaft cells,
and layout slot collection uses the same policy as route lowering.

## Falsifier and regression

The rule is falsified if a direct BT homolog recreates `++`, `+-+`, `└┐`,
`┌┘`, `└─┐`, or `┌─┘`, loses the target arrow, introduces a title collision,
or disagrees between default and optimized rendering. The focused regression
covers ASCII/Unicode × default/optimized:

```text
cargo test --locked --features qa --test subgraph_boundary_arrows direct_bt_subgraph_portals_keep_a_quiet_turn_shaft -- --nocapture
cargo test --locked --features qa --test subgraph_boundary_arrows narrow_bt_external_portals_keep_the_source_node_turn_clear -- --nocapture
```

Keep the full canonical and authored corpus review open; this lesson only
promotes the local portal rule and does not sign off the release.
