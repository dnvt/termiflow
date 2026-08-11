# Visual lesson: mixed three-branch junctions need explicit target ports

Date: 2026-08-07
Hypothesis: an exact horizontal three-branch fan-out/fan-in junction must expose
one visually identifiable arrowhead for every semantic edge, including three
distinct arrivals on the target contour, in both directions, styles, and
render modes.

## Failure observed

The H26 machine evidence for `junction_mixed_lr` and `junction_mixed_rl` was
clean: six semantic edges, no raw errors, no geometry errors, and no critic
findings. A human-eye review still found only four visible arrowheads: the
three source fan-out arrows were clear, but the three branch-to-target edges
shared an ambiguous sink arrival. The missing distinction was a visual
ownership defect, not an edge-count defect.

## Correction

The exact five-node, six-edge, three-branch family now has a dedicated
fail-closed policy in `src/render/dedicated_fan_in.rs`. It is restricted to
the exact unlabeled rectangle topology in LR/RL, allocates three target-side
lanes, and is independently covered by
`tests/independent_oracles.rs`. The policy rejects near-miss graphs rather
than changing their routing implicitly.

## Acceptance evidence

- H27 focused review: all eight changed homologs (LR/RL × ASCII/Unicode ×
  default/optimized) inspected one frame at a time; each shows six visible
  arrowheads and intact source/branch/target contours and labels.
- Full corpus: all 237 existing input fixtures × four directions × two styles
  × two modes = 948 packet rows; 936 successful rows reviewed and 12 typed
  expected-error rows validated separately.
- H27 changed scope: eight mixed-junction rows changed; 940 rows carried
  forward by hash-bound ledger rebinding.
- Fresh evaluator-owned `junction-quad` holdout: 16/16 execution rows and
  16/16 perceptual passes.
- Golden and baseline approval remain separate and were not changed.

## Rule

For a multi-edge junction, count and ownership evidence must agree with what a
human eye can identify. Every semantic branch must have a distinct route into
its declared target port; a clean critic score or six-edge trace cannot waive
an ambiguous or shared arrival.

## Follow-up

Keep the complete 237-fixture/948-row packet mandatory after every renderer or
layout source change. Add a strict structural predicate and an independent
raw-frame oracle for each new junction family, then inspect every changed
homolog before carrying the ledger forward. Continue the repair queue for the
remaining P1/P2 findings rather than approving goldens from machine evidence
alone.
