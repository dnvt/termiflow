# Wide terminal fan-in: separate channel rows

## Observation

The first proof-gated four-to-eight-source TD/BT fan-in candidate rendered all
eight arrows and passed the structural ownership oracle, but its ASCII frames
placed adjacent horizontal channels directly beside another route's vertical
shaft. The machine critic found 12 `RouteTopologyMismatch` warnings in each
ASCII homolog because a horizontal `-`/`+` seam could be read as an accidental
junction. The Unicode homologs did not expose the same warning, but the same
human-eye ambiguity was a valid cross-style risk.

## Hypothesis and fix

The ambiguity belonged at the wide fan-in route/layout boundary, not in the
generic topology repair pass. Reserve one blank primary row between neighboring
channels, increase the topology-owned corridor from `count + 2` to
`2 * count + 2`, and keep the clone-based all-routes proof before committing the
canvas. The route remains pure TD/BT, rectangle-only, unlabeled, terminal, and
four-to-eight-source only.

## Evidence

- Fresh packet: 237 input fixtures × 4 directions × 2 styles × 2 modes = 948
  rows; 936 successful render rows and 12 typed expected-error rows.
- Structural packet validation: 948/948 rows, 0 findings.
- Fresh successful perceptual ledger: 936/936 rows; 8 changed wide-fan-in
  homologs received new one-frame decisions, all `pass`; unchanged rows were
  hash-rebound through the guarded QA command. Existing historical watches
  remain preserved rather than rewritten.
- Fresh expected-error ledger: 12/12 exact matches.
- Fresh evaluator-owned `junction-quad` holdout: 16/16 execution rows and
  16/16 one-frame perceptual passes.
- Focused wide-fan-in ASCII/Unicode audits: zero critic findings after the
  channel-pitch fix.

## Rule

For a proof-gated wide terminal fan-in, a route channel must not be visually
adjacent to a different route's vertical leg when that adjacency makes the
terminal glyph imply an unowned junction. Layout must reserve the route's
actual corridor before lowering, and every direction/style/mode homolog must
be re-rendered and reviewed in the complete existing fixture corpus.

## Follow-up

Keep the full 237-fixture/948-row visual loop mandatory for every subsequent
routing change. Preserve the 394 historical `watch` decisions as the next
repair queue; do not approve goldens, dependency upgrades, or release gates
until the final source epoch is clean and those decisions are resolved or
explicitly held with falsifiers.
