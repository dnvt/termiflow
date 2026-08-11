# Visual lesson: horizontal sibling chains need receiver-owned corridor rows

## Observation

The full `tests/fixtures/inputs` corpus exposed a specific horizontal failure
in `collision_sibling_triple_rl` and its LR homolog. Three flat titled sibling
groups rendered their two cross-boundary transitions on the internal-node row.
The frame was structurally connected and the critic could be clean, but the
middle group's incoming and outgoing roles visually collapsed into one bus.
That forced a human reader to trace the whole row instead of seeing which
transition owned each boundary opening.

## Hypothesis and bounded fix

For a strict horizontal sibling chain, each cross-boundary transition needs a
receiver-owned corridor row distinct from the adjacent transition. The
renderer can safely own this only when it proves a flat, title-bearing chain
of rectangular two-node groups, one internal edge per group, one unlabeled
arrow between each adjacent pair, common frame height, and common node row.
Any cramped, nested, labeled, non-rectangular, cyclic, or non-chain scene must
retain the existing route policy.

The bounded implementation adds `lr-rl-sibling-chain` as a transactional
scene planner. It allocates quiet upper/lower rows, routes each transition
through its source and target boundary with explicit ownership, preserves a
visible receiver shaft, records target-entry decisions and a deterministic
contract digest, and returns only the cross-boundary edge indexes as claimed.
The internal edges remain on their ordinary precomputed routes.

## Falsifiers and evidence

The hypothesis is falsified by a continuous middle rail, collapsed boundary
rows, a missing receiver shaft, a route entering a node or title, a route
trace mismatch, a critic warning, or activation on a negative-control scene.

- The selector and renderer matrix passes for
  `collision_sibling_triple_lr` and `collision_sibling_triple_rl` across ASCII
  and Unicode styles, optimized and unoptimized modes.
- The route trace records two distinct boundary rows, two target-entry
  decisions, and a deterministic contract digest with no mismatches.
- The critic is clean for the focused LR/RL matrix after the receiver shaft
  and terminal corner ownership were made explicit.
- Negative controls pass for BT, TD, and the two-sibling horizontal fixture;
  the strict horizontal-chain planner does not activate there.
- BT sibling-chain focused tests and the independent scene-clearance oracle
  remain green after the new module is added.

## H88 full-corpus outcome

The fresh source-epoch review drained all 936 renderable rows in both the
requested-style and authored/no-override lanes, plus separate 12-row
expected-error ledgers. The strict LR/RL scene now separates the two
cross-boundary rails, but the receiver-owned U-shaped corridors still create
small box-like elbows at the sibling seams. The BT triple and parallel-BT
homologs also retain long rail/trunk impressions at titled borders. The critic
is clean, but human-eye review is not: the four historical risks remain open
and no golden approval is justified by this cycle.

The next falsifiable experiment is to improve corridor geometry or add a
layout-owned quiet band without widening activation beyond the strict scene
gates. It must be judged against all four style/mode homologs in both lanes
and must preserve the negative controls and full-corpus review contract.

## H89 outcome and falsified follow-up

The H89 source epoch regenerated both complete lanes: 948 packet rows each,
936 renderable frames, and 12 separately governed expected-error rows. Fresh
perceptual ledgers covered all 936 renderable frames in both lanes. Direct
inspection still finds the small U-shaped receiver elbows at LR/RL seams, the
long BT sibling trunk through titled borders, and the older two-sibling LR
shared-target ambiguity. Those records remain open; the critic's zero findings
do not close them.

A bounded candidate moved the LR/RL vertical turns one cell farther inside
each group. The LR homolog falsified it with disconnected junction arms and
shaftless arrows, so the candidate was reverted after focused tests. This
negative result is part of the lesson: the next experiment must change the
layout-owned quiet-band allocation or another explicit ownership boundary,
not repeat an inward coordinate nudge. The H89 review flow records seam
coordinates, adjacent border cells, receiver shafts, and the exact
ASCII/Unicode/style/mode homologs for every remaining watch.

This is a source-epoch lesson only. It requires a fresh 237-input canonical
packet, a fresh 237-input authored/no-override packet, separate expected-error
ledgers, and independent one-frame decisions for all 936 renderable rows in
each lane before any historical watch can be marked repaired or a golden
baseline can be approved.

## H91 focused negative result: upper/lower redistribution

A bounded follow-up spread the two receiver rails across the available upper
and lower quiet bands instead of packing both into the lower band. The focused
LR/RL selector, route-trace, and critic tests all passed, but the rendered
frames were visibly worse: the upper rail crossed the middle group's
title/border region and formed a second box-like enclosure (`┌─────┐`) at the
seam. The candidate was reverted before any full packet or golden operation.

This falsifies row redistribution as a sufficient fix. The next experiment
must introduce real layout-owned quiet space or transfer the seam ownership
boundary so route cells never occupy title/border territory. A future promoted
candidate must still pass both complete policy lanes, the separate expected
errors, and all 936 one-frame perceptual reviews per lane.
