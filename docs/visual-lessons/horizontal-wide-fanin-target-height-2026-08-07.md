# Horizontal wide fan-in: preserve target ports without hiding the compactness cost

## Evidence

- H28 reviewed the complete existing fixture corpus: 237 inputs, 948 primary
  rows across four directions, two styles, and two render modes; the twelve
  typed expected-error rows were validated separately.
- Only the eight `converge_deep` LR/RL rows changed after the renderer update;
  the other 940 primary frames were exact hash-bound carry-forwards.
- Each changed frame showed eight source boxes, eight direct horizontal rails,
  eight distinct target-side arrowheads, intact labels and contours, no raw,
  geometry, or critic findings, and identical default/optimized geometry.

## Learned rule

For a pure four-to-eight-source horizontal terminal fan-in, semantic edge count
is not enough. The target must expose one identifiable arrival row per source,
and each source row must align with its target row. A cloned-canvas proof may
commit the routes only after those rows are aligned and disjoint.

## Remaining watch

The direct-port hypothesis makes the target intentionally tall: the current
four-row pitch exposes all eight arrivals but leaves blank target interior above,
between, and below the centered label. This is readable and semantically clear,
but it is a P2 compactness/readability watch rather than a claim that the visual
form is final.

## Next falsifiable hypothesis

Test whether a target-side port layout can preserve eight distinct arrivals with
less perceived empty target area—without reintroducing a shared comb, ambiguous
edge ownership, route-through-node cells, or default/optimized divergence. Reject
the experiment if any of those conditions regress, or if source/target row
alignment becomes non-deterministic.
