# Whole-group BT title staging does not own the title boundary

Status: Falsified candidate; title-boundary clearance remains a Watch
Source epoch: H153 direct three-rail BT title-clearance experiment, 2026-08-23
Fixture family: `collision_parallel_edges_bt`, with the H152 control

## Observation

The direct three-rail BT frame had explicit boundary seams after H152, but a
human-eye review still saw the `Target` title too close to the route/border
relationship. A candidate that translated the complete titled group to the
right was visually tempting because it appeared to create more title gutter.

## Hypothesis and falsifier

A shared rightward staging offset might move the title, nodes, and route lanes
into a cleaner fixed relationship. The candidate was falsified at two offsets:
later layout epochs re-centered the titled envelope, moving node centers and
route lanes without giving the title boundary an owner, and short-turn elbows
appeared at the portal transitions. The strict slot/rail predicate therefore
failed, even though the complete group moved together in an intermediate
frame.

## Evidence

- The H152 control remains the source of truth for the direct three-rail seam.
- Focused BT visual coverage, formatting, and diff checks remain green after
  reverting the candidate.
- The candidate must not be retried unchanged: a complete-group translation
  does not survive title-envelope re-centering.

## Promoted rule or next experiment

Do not suppress the human-eye watch or add a global staging offset. The next
experiment must make the titled envelope or portal projection own a stable
title gutter before later centering/reflow epochs. It must preserve endpoint
slot identity, three distinct rails, titles, arrows, labels, and matched
controls, then rerun the complete canonical and authored fixture corpus with a
fresh human-eye review.
