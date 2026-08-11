# Visual lesson: close vertical Decision contours

Date: 2026-08-07
Slice: H21-A
Status: Decision contour localized; Flag attachment remains a separate hypothesis

## Observation

The H20 complete-corpus review found that vertical `shape_all` Decision nodes
were technically owned by the shape renderer but visually read as detached
terminal markers: ASCII showed isolated `^`/`v` cells and Unicode showed
isolated point cells above and below the label. The topology and arrow counts
were correct; the human-eye defect was that the contour did not read as one
bounded Decision shape.

H21-A changed only the Decision painter's three-row contour. TD/TB/BT now use
the same closed rhombus shoulders, label row, and lower shoulders already used
by the horizontal contour. The node height and route coordinates remain
unchanged, so the fix removes the marker ambiguity without a layout rewrite.
The Flag left-point contour and its LR incoming-edge attachment were not
changed in this slice.

## Owner-layer hypothesis

The defect belonged to shape-contour ownership, not to graph semantics or
general layout. A shape renderer must provide a complete visual boundary for
its measured rows; generic edge/junction repair must not be asked to make
point-only contour cells legible. Keep endpoint attachment separate when an
edge meets an asymmetric point, because hiding or moving an arrowhead can
silently weaken directed-edge semantics.

## Complete review result

The current source epoch regenerated all 237 existing Mermaid inputs in four
directions, two styles, and two modes: 936 primary rows plus 12 separately
typed expected-error rows, 948 structural rows total.

- 908 primary rows carried exact frame/evidence/policy hashes.
- 16 primary frames changed and 12 unchanged frames had changed diagnostic
  evidence; all 28 were reopened one frame at a time.
- The changed family is limited to vertical `shape_all` and
  `subgraph_shapes` Decision homologs in TD/BT. All repaired Decision frames
  passed at P3; the primary ledger is 936/936 with 573 pass, 363 watch, and
  no fail or unclear decisions.
- The remaining watches are existing route, portal, containment, label, and
  Flag-attachment observations. They were not reclassified as fixed merely
  because the Decision contour improved.
- Expected-error policy is 12/12 exact matches in its separate ledger.
- The evaluator-owned `junction-quad` holdout is 16/16 structurally passed
  and 16/16 perceptually passed.

## Reusable review rule

After every renderer source epoch, regenerate the entire existing-input
corpus. Carry a prior perceptual decision only when frame, evidence, and policy
hashes all match. Reopen every changed frame and every changed diagnostic row
one at a time, record what the human eye sees before naming a source layer,
then map each watch to a falsifiable hypothesis. Keep expected errors,
holdouts, raw/semantic/geometry/critic oracles, golden approval, and strict
source identity as separate gates.

## Follow-up

H21-C remains the next bounded experiment: LR Flag entries can place an edge
arrowhead immediately beside the Flag's left point, producing an odd `><` or
`→<` pair. Any fix must preserve one visible directed arrow per edge, keep the
Flag point recognizable, and be rerun against the same 237-input/948-row
contract plus the 16-row holdout.

## Evidence references

- Packet: H21 complete packet (private run artifact).
- Primary ledger: H21 primary perceptual ledger (private run artifact).
- Expected-error ledger: H21 expected-error ledger (private run artifact).
- Holdout receipt: H21 holdout receipt (private run artifact).
- Holdout ledger: H21 holdout ledger (private run artifact).
- Golden check: H21 intent-bound golden report (private run artifact).
- Strict-quality stderr: H21 strict-quality diagnostic (private run artifact).
- Source hashes: `src/render/shapes.rs` `fec1eabd41406d0f72377c124fed03c3d433987a75f99534c4269357d1fca9be`;
  `tests/independent_oracles.rs` `f0ed37818edd8ebdb43054f49fb36b0faf75891cbd67a084a97526b50beb9f19`.
