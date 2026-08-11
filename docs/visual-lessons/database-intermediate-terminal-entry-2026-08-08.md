# Visual lesson: database intermediate nodes must preserve terminal entry identity

## Observation

The complete `tests/fixtures/inputs` corpus remains the acceptance surface,
not just the focused regression fixtures. The fresh H35-v3 packet generated
948 rows across every input, direction, style, and rendering mode: 936
renderable frames plus 12 expected-error rows. The full perceptual ledger
reviewed all 936 frames one at a time, and the separate error-policy ledger
reviewed all 12 expected failures.

The previous `shape_database_{td,bt,lr,rl}` golden family exposed a specific
human-eye defect: a three-edge topology with a rectangle source, an
intermediate database node, and a terminal database target could show a
mid-route arrow or an ambiguous target entry. The later full-corpus review
also caught a terminal database arrow-order defect: the shape-specific bridge
put the arrowhead before its final shaft, so a visually connected route still
looked detached. Both observations demonstrate why visual review must reject
a passing selector test when the rendered frame gains an awkward extra corner
or reverses the human-readable terminal order.

## Hypothesis and fix

For this exact proof-gated topology, the intermediate database node is a
route waypoint, not a terminal target. The renderer must reserve the complete
source-to-intermediate-to-target transaction before drawing any generic
convergence route, assign two distinct target ports, and preserve the source
exit marker after node borders are rendered.

The bounded implementation therefore:

- keeps ordinary fan-in identity strict while allowing only the proven
  Rectangle/Database shape family;
- selects only a terminal Database target and rejects an intermediate
  Database target in the ordinary identity planner;
- lowers the exact three-node/three-edge topology transactionally across TD,
  TB, BT, LR, and RL with two target ports;
- uses the generic one-cell receiver entry for terminal Database arrows, so the
  arrowhead is the final route cell immediately outside the contour rather than
  a head followed by a shape bridge;
- claims all scene edges before generic diamond planning or stabilization can
  rewrite them; and
- repairs the source border after node drawing without moving the production
  staging lane outside the source-relative geometry.

The synthetic unit scene uses a minimum-width source only to exercise the
selector's valid geometry. It does not change the production staging formula;
the visual packet caught and rejected the tempting but awkward outside-source
staging variant.

## Falsifiers and evidence

The hypothesis is falsified by any missing intermediate or terminal arrowhead,
an arrow entering the wrong database contour, a collapsed pair of target
entries, a route crossing a node interior, or a new corner that makes the
diagram visually less legible.

- Focused independent oracle:
  `intermediate_database_entry_is_terminal_across_direction_style_and_mode_matrix`
  passes across four directions, two styles, and two modes.
- Focused unit and build checks pass for the database fan-in planner.
- Fresh packet: `/tmp/termiflow-h35-target-entry-final-v3`.
- Packet completion SHA256:
  `682860367333050356024d6881cd39b998228e2299cf0ae96ddb4c18a88eb9a8`.
- The current H84 perceptual ledger identified the P1 terminal ordering in
  `edge_branch_td` and `shape_database_td` (and its LR homolog family); the
  generic receiver-entry change is a bounded repair hypothesis that still
  requires fresh two-lane packet evidence before promotion.
- Expected-error coverage: 12/12 records validated separately.

## Remaining queue

The sibling-subgraph P2 boundary-rail watches and the P1
`td_sibling_subgraph_boundary_rail_and_missing_target_arrow` findings remain
open hypotheses. They are not silently converted into passes by the clean
database result. Any renderer, dependency, Rust toolchain, or source-epoch
change must regenerate the complete packet and repeat both ledgers before a
new golden baseline is accepted.
