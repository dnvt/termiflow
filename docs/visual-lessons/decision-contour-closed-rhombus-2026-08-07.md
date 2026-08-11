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

- Packet: `/var/folders/n_/fn0_190d2rgbq7kxy6cfkh4w0000gn/T/tmp.FLGzqm2zx0/packet/COMPLETE.json`
  — SHA-256 `30f286981b731a2850fbc57c0fcbb0a90d4aaad578610ddde6ed1c36d94a94b7`
- Primary ledger: `/var/folders/n_/fn0_190d2rgbq7kxy6cfkh4w0000gn/T/tmp.Wm84xSVZwM/decisions.XXXXXX.jsonl`
  — SHA-256 `b7372930a45aafaa4bc7fb40fb28ccb231d5d5717dce08d932902b9beb00abc2`
- Expected-error ledger: `/tmp/termiflow-h21-errors.XkrucB/records.jsonl`
  — SHA-256 `9b3a7b02deb0be6d058cb46e072bfc89bbde4e4da5ab7356c545fd7bd0e27af1`
- Holdout receipt: `/tmp/termiflow-h21-holdout.uGRF1x/receipt.json`
  — SHA-256 `868ea0c9b371e12832fb64e18b38f2187a127155a5829495a01b5d6dc6fdae7d`
- Holdout ledger: `/tmp/termiflow-h21-holdout.uGRF1x/holdout-decisions.jsonl`
  — SHA-256 `05500fa07d54a03228480f77443ae16bcdf38a7714897594d36ae888d33f81f1`
- Golden check: `/tmp/termiflow-h21-golden-check.json`
  — SHA-256 `af0ecfd79649b87772324c63a39b56dad9259640389daa02d4d0854d7cd84ac3`
- Strict-quality stderr: `/tmp/termiflow-h21-gates.3R68jv/strict_quality.stderr`
  — SHA-256 `621f3cae1cb5b40c429963a117a1cdc041a7d3896660a129fa6d4906697177a0`
- Source hashes: `src/render/shapes.rs` `fec1eabd41406d0f72377c124fed03c3d433987a75f99534c4269357d1fca9be`;
  `tests/independent_oracles.rs` `f0ed37818edd8ebdb43054f49fb36b0faf75891cbd67a084a97526b50beb9f19`.
