# Visual lesson: BT title and portal hooks need an owned route channel

## Observation

The complete fixture packet is the review population: every existing input is
rendered in its declared directions, ASCII and Unicode, and default and
optimized modes. In the 2026-08-05 packet, the four homologs of
`collision_parallel_edges_bt` visibly preserve the three routes but bend the
first A1-to-B1 rail into a `└─┐`/`┌─┘` (or `+-+`) hairpin at both titled
subgraph boundaries. The same packet shows the strict BT sibling-chain
homologs bending incoming cross-group rails around `Group 2` and `Group 3`
titles before returning to the node column.

The machine geometry, raw connectivity, and critic checks can remain clean
while these frames still look mechanically kinked. The independent
`termiflow.route_clarity.v1` report now detects title-adjacent horizontal
elbows as a conservative `P2` queue signal. It must not turn that signal into
an automatic failure or approval.

## Hypotheses

1. Parallel BT boundary reservations select a title-safe portal column that
   does not share the source/target node rail, so the first edge receives an
   unnecessary lateral detour.
2. Strict BT sibling-chain routing avoids a title by bending late at the
   title-safe interior row instead of reserving a stable route channel during
   layout. The resulting boundary hook does not communicate ownership clearly
   to a human reader.

The likely owners are the topology-derived BT portal-slot policy, the
title-safe column allocator, and the BT fallback scene planner. A fix must be
topology-derived; it may not special-case fixture names or labels.

## Falsifiers and promotion

The hypotheses are falsified if a layout/portal change introduces a missing or
reversed arrow, title overwrite, border break, disconnected raw route,
cross-style drift, or a P0/P1/P2 finding in any homolog or evaluator holdout.
They are also falsified if the current hooks are shown to be the only legible
representation under the declared narrow-width contract.

Before promotion, render the complete 948-row packet, validate independent
semantic/raw/geometry evidence, review every affected ASCII/Unicode ×
default/optimized homolog one frame at a time, and run the evaluator-owned
holdout. Keep golden candidates separate; no snapshot or quality-baseline
change is authorized by this lesson.

The current fresh packet remains open for the full 936-row perceptual ledger
and the separate 12-row expected-error ledger.

## Bounded cycle result — 2026-08-05

The first hypothesis was tested with the smallest policy change: exact-two
parallel crossings retain the wider title margin, while three-or-more aligned
crossings stay on the literal title-safe column. The focused regression
`render_bt_parallel_edges_avoids_adjacent_title_route_corners` passes for
ASCII/Unicode and default/optimized modes. A fresh complete packet
`/tmp/termiflow-route-clarity-audit-20260805-6` validates 948/948 rows, and
all four `collision_parallel_edges_bt` homologs receive fresh one-frame
`pass` decisions; the former boundary hairpin is gone.

The second hypothesis remains open. All four
`collision_sibling_triple_bt` homologs receive fresh P2 `watch` decisions with
the repeated title-adjacent hooks still visible. That is a deliberate hold,
not a golden approval: the next cycle must reserve a title-aware sibling
portal channel, rerun focused regressions, regenerate the full packet, and
re-review the affected homologs plus holdouts before promotion.
