# Mixed BT sibling receivers must replace stale portal slots

Status: Promoted topology-owned rule; ASCII seam ambiguity remains a P2 watch
Source epoch: H159 mixed BT sibling receiver repair, 2026-08-23
Fixture family: `collision_sibling_subgraphs_bt` and its ASCII/Unicode,
default/optimized, canonical/authored-policy homologs

## Observation

The mixed BT sibling frame contained two crossed edges entering the upper
`Right Group` while that group also had an internal edge. The routes were
connected and the arrowheads existed, but the final projection retained the
generic, center-biased portal slots beside the receiver-aligned lanes selected
by the scene transaction. A human eye could read the resulting seams as a
shared border junction rather than two individually owned portals. In ASCII,
the repaired frame still has a conservative seam watch; in Unicode, the added
quiet target band makes the receiver relationship locally legible.

## Hypothesis and falsifier

The scene planner owns the physical crossing lanes, so final portal
projection must replace—not append to—the precomputed generic slots for this
exact mixed sibling topology. The target envelope must also reserve a quiet
band before the title-safe row. The hypothesis is falsified if any homolog
loses an edge identity or arrowhead, projects a lane away from its receiver,
creates a title/border hook, changes an unrelated sibling topology, or makes
the final target turn less readable.

## Promoted rule

For exactly two flat titled BT siblings with two ordinary unlabeled
source-to-target crossings and one internal edge in each group:

- clear the stale generic top/bottom portal slots before committing the
  scene-owned boundary claims;
- allocate the target lane with the same edge-specific title margin used by
  the portal policy;
- reserve the extra target quiet band only for this exact mixed scene;
- keep ordinary two-edge sibling crossings, nested groups, labels, cycles,
  crowded layouts, LR/RL side portals, and unrelated BT scenes on their
  existing contracts.

The focused regressions are
`exact_two_bt_siblings_replace_stale_generic_portal_slots`,
`exact_two_bt_siblings_leave_a_quiet_row_before_each_target_title`, and
`exact_two_bt_siblings_route_clarity_is_clean_across_matrix` in
`tests/subgraph_boundary_arrows.rs`. The complete locked suite and the full
482-file golden check must remain green.

## Evidence and next loop

The H159 canonical and authored packets cover the current 241-input corpus:
964 rows per lane, 952 renderable decisions, and 12 expected-error policy
rows. Changed BT homologs require fresh one-frame inspection; unchanged rows
may be rebound only by exact case, policy, evidence, and frame hashes. The
ASCII seam watch must remain named until a later topology-owned experiment
proves local portal ownership without exporting a scalar spacing change.

Before any further golden or release decision, regenerate both policy lanes,
validate the expected-error ledgers, inspect the affected BT homologs and
holdouts, and rerun the full tests, dependency-currency, and release gates.
