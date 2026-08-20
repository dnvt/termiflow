# Visual lesson: mixed LR/RL target rows cannot be repaired by placement alone

Status: Falsified bounded LR/RL candidate; mixed target-entry watch remains
open
Source epoch: H120 fresh visual pass
Fixture family: `collision_sibling_subgraphs_lr` / `collision_sibling_subgraphs_rl`

## Observation

In the exact two-subgraph mixed-target scene, the internal edges and the
cross-subgraph edges share a receiver area. The human-eye concern is the
compact `+>+` / `┌→┤` shoulder beside the target node and the long seam-like
cross-subgraph rail. It is tempting to straighten each internal source/target
pair onto one row so the internal arrows become visually simple.

## Hypothesis and falsifier

Hypothesis: a topology-gated layout policy that moves each internal end node
onto its paired start-node row will remove the shoulder while preserving the
two cross-edge identities.

Falsifier: any lost arrow, fallback scene, changed route identity, collision,
or regression in either LR/RL mirror or any ASCII/Unicode/default/optimized
homolog. The candidate was falsified immediately: the focused matrix reported
only three arrows and rejected the target-entry scene in all eight LR/RL
style/mode cases.

## Evidence

- Focused regression: `exact_horizontal_mixed_target_keeps_both_entries_visible_across_matrix` in
  `tests/subgraph_boundary_arrows.rs` failed in all eight matrix cases.
- Machine symptom: the route planner reported `no collision-free horizontal
  target plan`; `edge:3:B->D` became untraced.
- Human-eye result: no candidate packet or golden was approved. The existing
  mixed-target rows remain a P2 watch and must still be reviewed in both
  policy lanes.
- Golden result: unchanged; this candidate was reverted before packet
  regeneration.

## Next experiment

The next repair must change the LR/RL target-entry scene owner—its endpoint
contract, seam ownership, or route-plan search—rather than moving nodes after
the generic ranks are chosen. A candidate is acceptable only if the scene
planner still emits four connected edges in every mirror/style/mode and the
direct frame no longer reads as a compact junction or shared container rail.

Private packets, ledgers, Maestro capsules, and provider traces remain outside
the OSS repository; this page records the reusable negative result.
