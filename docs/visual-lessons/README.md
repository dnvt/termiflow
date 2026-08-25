# Visual QA lessons

This directory is the public, durable record of what the TermiFlow renderer
needs to make diagrams readable to a human eye. A lesson captures a visible
symptom, the smallest plausible ownership hypothesis, a falsifier, the
focused regression or oracle, and the follow-up required before a rule is
promoted.

These are engineering lessons, not private AI transcripts. The public record
contains the observation and the evidence needed to review the decision. Raw
prompts, model/provider traces, private Maestro capsules, transient packet
directories, and user-sensitive diagrams remain private.

## Review contract

The denominator is the complete `tests/fixtures/inputs` corpus, not only the
fixture that exposed the problem. A normal full cycle currently covers 242
inputs and 968 packet rows: 956 renderable rows plus 12 separately governed
expected-error rows. Human-eye decisions remain distinct from machine critic,
semantic, geometry, and golden checks.

The reusable [visual review rubric](../visual-review-rubric.md) defines the
fresh one-frame protocol, exact-cell requirement for watches, and evidence
hierarchy used by the QA CLI.

A lesson is not by itself a golden approval or a release sign-off. Snapshot
changes require the separate intent-bound golden workflow in
[`CONTRIBUTING.md`](../../CONTRIBUTING.md). Historical lessons retain their
source epoch and open watches; a later clean machine check does not silently
erase a human-eye concern.

## Lesson status

Each lesson states its own status. Read these terms literally:

- **Observation:** a visible behavior recorded before naming a code owner.
- **Hypothesis:** a falsifiable explanation and the narrow change it predicts.
- **Falsified:** a candidate or explanation was rejected by a focused result;
  the negative result is still useful and should not be retried unchanged.
- **Promoted rule:** the behavior has a focused regression or oracle and is
  safe to carry into the next complete-corpus cycle.
- **Watch:** the frame is machine-clean or partially improved, but a human-eye
  concern still needs a named next experiment.

## Public lesson index

### Boundary, portal, and route ownership

- [Portal markers must declare their route axis](portal-axis-explicit-ownership-2026-08-03.md)
- [BT title and portal hooks need an owned route channel](bt-title-portal-hooks-2026-08-05.md)
- [BT external entries must share lane ownership](bt-external-entry-lane-alignment-2026-08-07.md)
- [Strict subgraph fan-in target-entry identity](subgraph-fanin-target-entry-identity-2026-08-07.md)
- [Parallel subgraph fan-in target identity](subgraph-parallel-fanin-target-arrow-identity-2026-08-08.md)
- [Full-corpus subgraph boundaries and target entries](full-corpus-subgraph-boundary-and-target-entry-2026-08-08.md)
- [TD mixed target-entry identity](td-mixed-target-entry-identity-2026-08-09.md)
- [TD mixed target entries need a short title-safe bridge](td-mixed-target-title-clearance-2026-08-20.md)
- [Database intermediate nodes preserve terminal entry identity](database-intermediate-terminal-entry-2026-08-08.md)
- [Horizontal sibling chains need receiver-owned corridor rows](horizontal-sibling-chain-receiver-rails-2026-08-11.md)
- [Scoped BT sibling channels and vertical junction headroom](bt-sibling-channel-and-dual-junction-shaft-2026-08-13.md)
- [BT sibling rails need role ownership](bt-sibling-rail-ownership-2026-08-13.md)
- [Nested BT portals need boundary-by-boundary ownership](nested-bt-boundary-ownership-2026-08-20.md)
- [Direct BT portals need a quiet target turn](direct-bt-portal-turn-clearance-2026-08-20.md)
- [BT sibling entries need a quiet row on both sides of the turn](bt-sibling-quiet-row-2026-08-22.md)
- [Two-group BT sibling routes need a traced, compact corridor](bt-two-group-owned-corridor-2026-08-22.md)
- [Strict BT sibling portals need directional border seams](bt-sibling-portal-seams-2026-08-22.md)
- [Parallel BT sibling rails need an exact scene-owned seam](bt-parallel-sibling-seams-2026-08-22.md)
- [Direct three-rail BT portals need a directional seam](bt-direct-three-rail-portal-seams-2026-08-23.md)
- [Whole-group BT title staging does not own the title boundary](bt-title-staging-envelope-recentering-2026-08-23.md)
- [Complex BT multi-subgraph scenes are a negative control](complex-bt-multi-subgraph-negative-control-2026-08-20.md)
- [Quiet BT sibling corridors can hide tiny shoulders](complex-bt-quiet-corridor-shoulder-2026-08-20.md)
- [Safe BT sibling transitions should stay straight](bt-sibling-straight-transitions-2026-08-21.md)
- [BT parallel turns need visible shaft clearance](bt-parallel-turn-clearance-2026-08-13.md)
- [BT parallel portals must share node lanes](bt-parallel-title-safe-lane-alignment-2026-08-13.md)
- [Collinear LR/RL sibling bridges need distinct corridors](lr-rl-collinear-bridge-2026-08-13.md)
- [Opposite LR/RL corridor bands can become border-shaped](lr-rl-alternating-band-falsified-2026-08-13.md)
- [Mixed LR/RL target rows need route-owner repair](lr-rl-mixed-target-row-alignment-falsified-2026-08-13.md)
- [LR/RL mixed targets need a quiet receiver shaft](lr-rl-mixed-target-receiver-shaft-2026-08-20.md)
- [TD sibling corridors need a layout-owned turn band](td-sibling-corridor-turn-band-2026-08-13.md)
- [TD sibling title gutters must preserve owned route rails](td-sibling-title-gutter-route-ownership-2026-08-20.md)
- [Complex TD title portals need source-owned straight lanes](td-complex-title-portal-corridor-2026-08-25.md)
- [Flat TD/TB external entries need a literal title-gutter lane](td-flat-external-title-gutter-2026-08-20.md)
- [TD terminal entries need distinct target-center lanes](td-terminal-entry-center-alignment-2026-08-13.md)
- [Dense vertical crossings need a quiet target shaft](dense-vertical-crossing-target-clearance-2026-08-13.md)
- [TD parallel sibling lanes clear title portal hooks](td-parallel-title-safe-lane-alignment-2026-08-13.md)
- [Mixed sibling targets need a real visual gap](mixed-sibling-target-entry-gap-2026-08-13.md)

### Labels and text ownership

- [Routed labels must protect route corners](routed-labels-protect-route-corners-2026-08-21.md)
- [Bounded long edge labels need an explicit policy review](bounded-long-edge-label-ellipsis-2026-08-22.md)
- [Full-corpus fresh review keeps visual watches honest](full-corpus-fresh-review-2026-08-23.md)

### Shapes, contours, and attachment clarity

- [Close vertical Decision contours](decision-contour-closed-rhombus-2026-08-07.md)
- [Direction-aware shape contours](shape-contour-direction-aware-diamond-2026-08-07.md)
- [Preserve the Flag point and review its edge attachment](flag-contour-left-point-2026-08-07.md)
- [Flag incoming-arrow clearance](flag-incoming-arrow-clearance-2026-08-07.md)
- [Emoji variation selectors must share the base glyph cell](emoji-variation-selector-cell-2026-08-13.md)

### Fan-in, target ports, and channel separation

- [Database fan-out tees need a quiet source stem](database-fanout-source-tee-stem-2026-08-13.md)
- [Wide terminal fan-in needs separate channel rows](wide-terminal-fanin-channel-separation-2026-08-07.md)
- [Horizontal wide fan-in preserves target ports](horizontal-wide-fanin-target-height-2026-08-07.md)
- [Ordinary four-port fan-in preserves edge identity](ordinary-four-port-fanin-identity-2026-08-07.md)
- [Mixed three-branch junctions need explicit target ports](mixed-three-branch-junction-target-ports-2026-08-07.md)
- [Vertical mixed edge kinds need target-facing shaft headroom](vertical-mixed-edge-kind-shafts-2026-08-20.md)
- [Dual-junction target ports must follow the route policy](dual-junction-target-port-measurement-2026-08-07.md)
- [BT parallel title-gutter spacing is an unsafe shortcut](bt-parallel-title-gutter-falsified-2026-08-08.md)

## Reproducing a lesson

Start with the repository's locked contributor workflow and the named fixture
in the lesson. Use the focused test or oracle first, then run the complete
visual cycle before changing a golden or closing a historical watch. Transient
packets and ledgers are local evidence; public lessons refer to them by cycle
and role rather than publishing developer-machine paths.

New lessons should follow the [public lesson template](TEMPLATE.md) and link
to repository files, fixture names, test commands, and durable rules wherever
possible.
