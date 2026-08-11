# Visual lesson: direction-aware shape contours

Date: 2026-08-07
Slice: H19
Status: Decision contour accepted; Flag contour remains the next P1 slice

## What the complete review taught us

A shape glyph is part of the route contract, not decorative text. A diamond
that is drawn with vertical points in a horizontal flow looks detached or
rotated even when the graph topology and edge endpoints are correct. A generic
route-junction repair can make the same problem worse by replacing a shape
contour cell with a junction glyph.

The safe rule is:

- pass flow direction into every direction-sensitive shape renderer;
- preserve contour cells as shape ownership, including when a route is
  adjacent; and
- make horizontal contours closed and visibly aligned with LR/RL flow instead
  of relying on a vertical point pair.

H19 applies that rule to Decision nodes. TD/BT retain explicit top and bottom
points; LR/RL use a closed diagonal-and-label contour. The source-junction
pass skips diamonds so route repair cannot erase the contour.

## Evidence and self-improvement loop

The authoritative H19 run reviewed all 237 Mermaid input fixtures in both
ASCII/Unicode styles and default/optimized modes: 936 primary rows, plus 12
expected-error rows, 948 total. The final primary ledger has 936/936
one-frame decisions: 560 pass and 376 watch, with no fail or unclear decision.
The separate evaluator-owned `junction-quad` holdout has 16/16 perceptual
passes.

The machine packet is not the visual verdict. H19 deliberately retained 15
P1 `shape-contour-fidelity-watch` decisions because Flag still reads as an
asymmetric/open-looking contour in the affected frames. That observation is
the hypothesis seed for H20: repair Flag contour ownership and then repeat the
entire corpus/expected-error/holdout/oracle cycle.

## Reusable review rule

After any renderer source epoch—including lint-only changes that alter source
identity—regenerate the full packet. Carry decisions only when exact frame,
evidence, and policy hashes match. Review every changed frame one at a time,
record what the human eye sees before naming a responsible layer, and include
an expected observation and falsifier. Keep golden snapshot approval separate
from the review decision.

## Evidence references

- Packet: `/tmp/termiflow-h19-final-cycle.MmaS7r/packet`
- Primary decisions: `/tmp/termiflow-h19-final-decisions.jsonl`
- Expected errors: `/tmp/termiflow-h19-final-expected-errors.jsonl`
- Holdout decisions: `/tmp/termiflow-h19-final-holdout-decisions.jsonl`
- Golden check: `/tmp/termiflow-h19-final-golden-check.json`
- Durable review: `thinking/reviews/2026-08-07-h19-shape-contour-complete-corpus-review.md`
- Durable decision: `decisions/DEC-2026-08-07-h19-shape-contour-direction-aware-diamond.md`
