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
fixture that exposed the problem. A normal full cycle currently covers 237
inputs and 948 packet rows: 936 renderable rows plus 12 separately governed
expected-error rows. Human-eye decisions remain distinct from machine critic,
semantic, geometry, and golden checks.

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
- [Database intermediate nodes preserve terminal entry identity](database-intermediate-terminal-entry-2026-08-08.md)
- [Horizontal sibling chains need receiver-owned corridor rows](horizontal-sibling-chain-receiver-rails-2026-08-11.md)
- [Scoped BT sibling channels and vertical junction headroom](bt-sibling-channel-and-dual-junction-shaft-2026-08-13.md)

### Shapes, contours, and attachment clarity

- [Close vertical Decision contours](decision-contour-closed-rhombus-2026-08-07.md)
- [Direction-aware shape contours](shape-contour-direction-aware-diamond-2026-08-07.md)
- [Preserve the Flag point and review its edge attachment](flag-contour-left-point-2026-08-07.md)
- [Flag incoming-arrow clearance](flag-incoming-arrow-clearance-2026-08-07.md)

### Fan-in, target ports, and channel separation

- [Wide terminal fan-in needs separate channel rows](wide-terminal-fanin-channel-separation-2026-08-07.md)
- [Horizontal wide fan-in preserves target ports](horizontal-wide-fanin-target-height-2026-08-07.md)
- [Ordinary four-port fan-in preserves edge identity](ordinary-four-port-fanin-identity-2026-08-07.md)
- [Mixed three-branch junctions need explicit target ports](mixed-three-branch-junction-target-ports-2026-08-07.md)
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
