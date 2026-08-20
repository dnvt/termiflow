# Visual lesson: BT parallel portals must share node lanes

Status: Promoted focused rule; complete-corpus and authored-policy closure
remain open

## Observation

After the first BT parallel repair added turn clearance, the route was still
visibly awkward in `collision_parallel_edges_bt`. The route planner selected
title-safe boundary rails that differed from the paired node centers, so the
first receiver still rendered a title-adjacent `└──┐`/`+--+` hook. The route
was connected, all arrows existed, and the machine critic was clean, but a
human eye could not immediately tell that the boundary rail belonged to the
node pair.

## Hypothesis and bounded fix

The strict three-edge BT sibling scene needs one shared policy at both layout
and route ownership: the same title margin and minimum lane gap must be used
when selecting rails. After moving a complete source/target pair, the layout
must preserve each envelope's left title anchor; otherwise recomputing the
envelope changes the title-safe lane underneath the placement decision.

The route allocator may accept an exact source and target lane when that means
there is no turn at all. Non-zero offsets below the existing three-cell turn
clearance remain rejected. The bounded change is topology-gated to the strict
flat three-edge BT parallel scene and keeps transactional fallback validation.

The implementation is shared through the BT parallel portal constants in
`src/portals.rs`, the final placement alignment in
`src/layout/envelope_stage.rs`, and the scene-owned lowerer in
`src/render/edge/bt_parallel_sibling.rs`. The focused regression in
`tests/bt_parallel_sibling_visual.rs` now requires every source and target
portal slot to equal its paired live node lane across ASCII/Unicode and
default/optimized rendering.

## Result and falsifiers

The four focused frames now show three continuous, separately scannable rails
with direct node attachments and no title-adjacent horizontal hook. The
intentional junctions at the source boxes remain local and readable. The rule
is falsified by a merged rail, detached arrow, lost portal claim, any new
short non-zero turn, a fallback mismatch, or a regression in a non-matching
BT scene or another direction/style/mode homolog.

The next required step is fresh review of the changed homologs and the full
canonical and authored/no-override corpus before resolving historical watches
or approving goldens.

## Public boundary

This lesson records visible observations, code ownership, tests, and
falsifiers only. Private Maestro capsules, prompts, provider traces, and
transient packet paths remain outside the OSS repository.
