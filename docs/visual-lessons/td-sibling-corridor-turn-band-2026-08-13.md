# TD sibling corridors need a layout-owned turn band

Status: promoted focused rule; complete-corpus closure and inherited boundary
watches remain open.

## Observation

In the three titled sibling fixture
`tests/fixtures/inputs/collision_sibling_triple_td.md`, the four canonical and
four authored TD rows initially rendered a rail that left one sibling border,
bent immediately before the next border, and entered the target title band at
an offset column. The rendered labels and arrows were recoverable, but the
`┌────┘`/`+-+` family of compact junctions read like a damaged container edge.
The evidence geometry also reported only three traced edges and two
`untraced_fallback_edges` (`A2->B` and `B2->C`).

The current corpus denominator for this review cycle is 241 inputs, 964 rows
per policy lane, 952 renderable frames, and 12 expected-error rows.

## Hypothesis

The layout reserved only two exterior cells between stacked sibling envelopes.
That left no quiet cell before the turn and no portal-shaft cell after it, so
the specialized topology-owned TD corridor rejected the gap and the generic
cross-subgraph route took over.

## Fix and falsifier

The layout contract now reserves three exterior cells for direct stacked TD/TB
sibling transitions: a quiet row, a turn row, and a portal-shaft row. The
renderer also accepts the smallest two-row corridor as a topology-owned
fallback when another layout cannot expand, while rejecting a one-row gap.

The hypothesis is falsified if any focused TD homolog still reports an
untraced fallback edge, a route rejection, or a border/title junction that is
not locally attributable after the layout-owned band is applied.

## Regression

The focused coverage is:

```text
cargo test --locked --features "qa maintainer-fixtures" --lib layout::tests::stacked_td_sibling_crossings_keep_three_connector_cells -- --nocapture
cargo test --locked --features "qa maintainer-fixtures" --lib render::edge::subgraph -- --nocapture
cargo test --locked --features qa --test subgraph_boundary_arrows -- --nocapture
```

The last test covers ASCII and Unicode, default and optimized rendering, and
requires all five edges to be traced with no fallback geometry errors. A fresh
canonical and authored packet is still required before resolving the visual
watch or changing any golden.

## Public rule

Direct stacked TD/TB sibling edges own their complete exterior corridor. Keep
enough layout headroom for the turn and portal shaft; do not let a generic
border-entry heuristic silently replace the route. Preserve this rule only for
the direct, non-nested sibling topology; fan-in, fan-out, nested, labeled, and
crowded routes need their own fresh visual review.
