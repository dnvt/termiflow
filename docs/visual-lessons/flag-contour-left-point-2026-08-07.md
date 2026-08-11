# Visual lesson: preserve the Flag point and review its edge attachment

Date: 2026-08-07
Slice: H20
Status: Flag contour localized; follow-up route hypotheses remain open

## Observation

The existing `tests/fixtures/inputs/shape_all_*` corpus exposed a real shape
fidelity defect. The old Flag painter approximated Mermaid's asymmetric
left-point shape with generic box corners, which made the left contour look
open or rail-like and allowed route-junction stamping to overwrite shape-owned
cells. A dedicated painter now preserves a left-center point, diagonal upper
and lower shoulders, the label, and a continuous right wall in ASCII and
Unicode across TD, BT, LR, and RL.

The fresh changed-frame review also found two remaining human-eye issues:

- TD/BT shape chains still show extra Decision terminal markers above and
  below the node (ASCII `^`/`v`, Unicode `◇`). This is a separate endpoint
  routing/junction hypothesis, not a Flag contour failure.
- LR incoming edges can place an arrowhead immediately before the Flag point,
  producing a visually odd `><` or arrow-to-point pair even though the Flag
  contour itself is now coherent. This is a route/shape attachment hypothesis.

## Owner-layer hypothesis

Flag geometry belongs to the shape renderer and must remain shape-owned during
route repair. Mermaid's asymmetric contour is not a generic rectangle with a
decorative corner. Route attachment needs a separate policy for an edge that
terminates at the left point, so the edge marker does not visually compete with
the point. Decision terminal markers should be audited in endpoint/junction
ownership code rather than folded into the Flag fix.

## Evidence

- Complete corpus: 237 Mermaid input fixtures × 4 directions × 2 styles × 2
  modes = 948 packet rows: 936 primary rows and 12 expected-error rows.
- Primary perceptual review: 936/936 one-frame decisions; 562 pass and 374
  watch, with no fail or unclear decision. Of these, 905 rows carried exact
  frame/evidence/policy hashes, 15 diagnostic-hash rows were re-reviewed, and
  all 16 changed Flag frames were reviewed individually.
- Expected-error policy: 12/12 exact matches in its separate ledger.
- Evaluator-owned `junction-quad` holdout: 16/16 execution rows and 16/16
  hash-bound perceptual passes.
- Independent render oracles: the Flag point-and-shoulder matrix passes across
  TD/BT/LR/RL, ASCII/Unicode, and default/optimized modes.
- Golden check remains unapproved: 337 snapshot changes are reported. Strict
  quality validation is intentionally held because this authorized checkout is
  dirty; no snapshot approval is implied by this lesson.

## Reusable review rule

After every renderer source epoch, regenerate and review the complete existing
fixture corpus. Carry a prior decision only when frame, evidence, and policy
hashes all match exactly. Review every changed frame one at a time, record the
human-eye observation before naming a source layer, and leave a falsifiable
watch when an adjacent route still creates a misleading glyph. Keep expected
errors, evaluator holdouts, independent oracles, and golden approval as
separate gates.

## Follow-up

The next slice should target Decision endpoint-marker ownership and the
incoming-edge/Flag-point attachment policy. Its acceptance matrix must include
the complete 237-fixture corpus plus the 16-row holdout, with explicit checks
that the corrected Flag contour remains unchanged while the follow-up route
artifacts disappear.

## Evidence references

- Packet: H20 complete packet (private run artifact; reproduce from the contributor workflow).
- Primary decisions: H20 primary perceptual ledger (private run artifact).
- Expected errors: H20 expected-error ledger (private run artifact).
- Holdout receipt: H20 holdout receipt (private run artifact).
- Holdout decisions: H20 holdout ledger (private run artifact).
- Golden check: H20 intent-bound golden report (private run artifact).
