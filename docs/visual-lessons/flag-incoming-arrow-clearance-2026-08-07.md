# Visual lesson: Flag incoming-arrow clearance

Date: 2026-08-07
Cycle: H21-C
Owner layers: shape policy, layout entry, render entry

## Observation

In LR Flag/asymmetric nodes, the incoming arrowhead and Mermaid left-point
could become visually ambiguous when the route terminated directly against the
point. Machine topology still reported a valid arrow, but the human-visible
row could read like a double tip (`><` in ASCII or `→<` in Unicode).

## Rule

Use one extra outward route cell for an incoming edge approaching a Flag from
the left in LR. Keep the Flag contour itself unchanged: the left-center point,
diagonal shoulders, label, and flat right wall remain shape-owned. Do not add
that clearance in unrelated directions or to unrelated shapes.

## Evidence

The complete H21-C packet covered all 237 existing inputs and 948 structural
rows. The four changed LR Flag frames were fresh human-eye P3 passes after the
fix, with the arrow still present, a visible separator cell, and the Flag point
still readable. The full primary ledger was 936/936; the expected-error lane
was 12/12; the separate evaluator-owned holdout was 16/16. Existing route
watches remain explicit, so this lesson does not claim the corpus is globally
perfect.

## Self-improvement loop update

Future cycles must continue to enumerate every existing input under
`tests/fixtures/inputs`, render every configured style and mode, inspect one
frame at a time, document exact human-eye defects, form a falsifiable
hypothesis, change one owner layer, rerun the complete packet, validate typed
errors, review the separate holdout, and preserve the next route-watch target.
Machine-clean evidence and golden deltas select or constrain work; neither is
human visual approval.

Next review target: the remaining explicit BT portal/sibling route watches,
starting from a clean source promotion checkout.
