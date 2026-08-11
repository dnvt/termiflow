---
name: termiflow-visual-review
description: Review TermiFlow ASCII and Unicode diagram frames for human-visible semantic, routing, containment, text, and rendering defects with hash-bound evidence and targeted self-improvement.
version: 0.1.0
allowed-tools: [Read, Grep, Bash]
---

# TermiFlow visual review

Use this skill when a rendering change, golden packet, visual audit, or
diagram-quality question needs perceptual review. It complements Rust packet
validation; it does not replace looking at the characters a terminal user will
see.

## Non-negotiable contract

- Work from a completed, immutable packet. Every decision is bound to one
  `case_id`, one evidence SHA-256, and one frame SHA-256.
- Present and inspect exactly one frame at a time. Never approve a batch from
  counters, filenames, source code, or a critic score.
- Record the observation before reading implementation details. The first
  question is “what would a human eye misunderstand?”
- A machine structural pre-screen is not perceptual approval. Rows with
  warnings, raw/geometry errors, critic findings, or a prior concern stay in
  the one-frame queue; fallback-risk rows should be explicitly selected for a
  full perceptual pass when routing changes are in scope.
- A user-provided frame, reproducible terminal output, or repeated human-eye
  contradiction overrides an earlier AI `pass`. Preserve the historical
  decision, create a fresh hash-bound corrective decision, and treat the row
  as `watch` or `fail` until the visual ambiguity is falsified. In particular,
  a BT rail that pierces two or more titled sibling boundaries, or three or
  more parallel BT portals that render as repeated `+`/`┼` junctions, is not a
  perceptual pass merely because the rail is straight and all arrows exist;
  portal ownership must be immediately legible to a human eye.
- Renderer-wide or fixture-directory quality work must enumerate every existing
  non-symlink `tests/fixtures/inputs` row and every configured style/mode
  combination. Review canaries and holdouts are probes, not substitutes: a
  machine-clean prescreen still requires one separate perceptual decision for
  each reviewable corpus frame before completion.
- This is a two-lane obligation. The canonical packet injects the requested
  `--style` so all style homologs are comparable; a supplemental packet must
  use `--respect-input-style` over the same complete input directory so
  authored `%% termiflow:` style/wrapping/spacing directives are reviewed as
  the terminal user actually receives them. The 20 directive-bearing inputs
  are high-risk, but the remaining inputs are explicit no-override controls
  and cannot be omitted. A row from one policy lane cannot cover the other.
- Every successful packet row must carry a hash-bound
  `termiflow.route_clarity.v1` report. Its `risk` or `inconclusive` status is a
  conservative reason to keep the row in the one-frame queue; even `clean` and
  `not_applicable` are evidence only and never replace the visual decision.
- Expected-error fixtures are reviewed through their explicit error-policy
  ledger. They are not silently counted as successful diagram frames, and they
  do not reduce the requirement to inspect every renderable input/style/mode
  row.
- Do not overwrite goldens or decisions in place. Use the Rust QA command and
  the guarded Bash wrapper; source fixes require a fresh packet and a second
  inspection.
- Use Rust and Bash only. Do not create or invoke Python/Ruby files or scripts.

## Workflow

1. Build or select both packets with `scripts/visual_audit.sh`: the canonical
   requested-style packet and a separate packet created with
   `--respect-input-style`. Confirm each completion marker, manifest identity,
   binary identity, expected-error count, route-clarity coverage, and argv/
   effective-policy provenance. For renderer-wide work, enumerate every
   non-symlink `tests/fixtures/inputs/*.md` file across every style/mode
   combination in both lanes: the current inventory is 237 inputs, 948 rows,
   936 renderable frames, and 12 expected-error rows per lane. A missing
   user-supplied path is a missing fixture, never an invented test.
2. Run the conservative Rust prescreen through
   `scripts/review_visual_packet.sh ... --prescreen-clean`. Treat its count and
   every route-clarity status as structural evidence only; the route report is
   deliberately not a perceptual verdict.
3. Pull one residual frame with `--next`. Read the complete frame payload and
   inspect the rendered ASCII/Unicode block without opening source first.
4. Use the checklist below. Stop at the first material ambiguity, but still
   record all dimensions that were actually inspected.
5. If prior visual watches, failures, or human contradictions exist, pass the
   append-only ledger with `--history HISTORY.jsonl`. The `--next` payload
   surfaces matching history and prioritizes unresolved risk. Write one
   decision JSON object with the exact hashes, `reviewer: ai` or
   `human`, a decision (`pass`, `watch`, `fail`, or `unclear`), severity,
   dimensions, observation, hypothesis, falsifier, expected observation,
   targeted next command, and affected homologs. An open history record cannot
   be hidden by an unqualified `pass`; resolve it with a hash-bound
   `history_resolution` (`falsified`, `repaired`, or `superseded`) only after
   inspecting its affected homologs. Append it through `--record`; never
   append by shell redirection.
6. Repeat `--next` only after the previous record succeeds. Finish with
   `--validate --history HISTORY.jsonl`; exception rows require perceptual
   decisions even if a machine pre-screen exists. The validator reports open
   historical risk separately from row coverage.
7. Close an accepted improvement cycle with
   `scripts/visual_cycle.sh --packet PACKET --decisions DECISIONS --record
   CYCLE.json --output RECEIPT.json`. The cycle record must bind the packet and
   decision hashes, exact observation details, owner-layer hypothesis,
   expected result, falsifier, fix or explicit hold, homologs, holdout result,
   and a durable lesson artifact. This command validates and receipts the
   cycle; it never appends decisions or approves goldens.

For a deliberate full perceptual pass, start with a fresh decisions file for
each policy lane, omit `--prescreen-clean`, and add `--fresh` to every review
command. Repeatedly pull one frame with `--fresh --next`, inspect it, and
append the hash-bound observation with `--fresh --record` before requesting
the next frame. Finish each lane with `--fresh --validate`. Fresh mode rejects
machine structural decisions and `carry_forward` records, so exact rebinding
cannot be mistaken for a new human-eye pass. When a prior visual history
ledger exists, add `--history HISTORY.jsonl` to every command so regenerated
packets cannot hide earlier watches or human contradictions. A no-override
style slot must state its effective authored policy; it is not a canonical
style result. This is slower but required after layout, policy, or
renderer-wide changes.
There is intentionally no structural-review escape hatch: a machine clean
pre-screen is structural coverage only and cannot close perceptual review.

## Human-eye checklist

Inspect these dimensions in order and state the result in the decision:

1. **Semantic topology** — Are every node, label, edge endpoint, direction,
   arrowhead, open link, bidirectional link, circle/cross endpoint, thick edge,
   and dotted edge visibly what the input claims? A clean edge count is not
   enough if two arrowheads collapse into one junction.
2. **Containment and portals** — Does every subgraph border contain exactly the
   intended nodes? Does a crossing look like a dedicated portal rather than a
   junction, branch, or edge-to-edge merge? Look at the border cell where the
   route enters and exits.
3. **Routing and geometry** — Look for routes through boxes, clipped endpoints,
  touching parallel shafts, accidental crossings, uneven fan-in/fan-out,
  cycle gutters that resemble borders, and excessive blank space. Trace one
  edge from source to target, not just its arrowhead.
   For BT cross-subgraph edges, trace the target shaft through the title-safe
   portal column, the physical border, and the external elbow. A
   `bt_title_portal_disconnection` or
   `bt_nested_title_portal_disconnection` finding is P1 when the route reads as
   detached, even if raw, semantic, geometry, and critic prescreens are clean.
   Also ask whether a long straight BT shaft visually collapses multiple
   sibling transitions into one trunk. Repeated border crossings at one x
   coordinate are a portal-ownership ambiguity until the frame explains which
   edge owns each opening.
4. **Text and display width** — Check node labels, edge labels, Unicode
   graphemes, alignment, wrapping, ellipsis, hard truncation, and labels that
   touch a border or arrow. If `wrap=true` is present, verify that the visible
   result actually wraps or that an explicit `max_lines` contract explains the
   truncation.
5. **Style fidelity** — Compare ASCII and Unicode homologs. Verify that a
   fallback route does not silently turn Thick/Dotted into ordinary shafts or
   a portal marker into a generic junction. Check arrow, border, corner,
   junction, and cycle glyph consistency.
6. **Canvas and terminal usability** — Check crop boundaries, leading/trailing
   blank columns, line-length spikes, rows that disappear at the terminal
   edge, and whether the diagram can be scanned without mentally repairing it.

## Decision discipline

Use `pass` only when the selected dimensions are human-readable and no
material ambiguity is visible. Use `watch` for a plausible defect that needs a
focused test or homolog comparison. Use `fail` when the diagram is misleading,
topology is lost, or a required visual contract is violated. Use `unclear` when
the packet or frame cannot support a reliable judgment; do not guess.

Every non-pass decision must include a falsifier. Good hypotheses name a
rendering boundary, for example `fanout_fallback_edge_kind_loss`,
`portal_marker_conflation`, `vertical_edge_label_hard_truncation`, or
`fixture_wrap_contract_mismatch`. For titled BT boundaries also use the named
`bt_title_portal_disconnection` and
`bt_nested_title_portal_disconnection` classes; they do not merely say “layout
bug”. Include cell coordinates when a single glyph or seam is the evidence.

## Self-improvement loop

For each `watch` or `fail`:

1. Preserve the original frame and decision hashes.
2. If a human contradiction overturned a prior AI pass, record the corrective
   decision before implementation and retain both hashes in the cycle note.
3. Add the smallest Rust regression test or fixture that isolates the observed
   ambiguity, preferably in every affected direction/style homolog.
4. Form one falsifiable implementation hypothesis. Change one rendering layer
   or policy boundary at a time.
5. Run targeted Rust tests, render the isolated fixture, and inspect the new
   frame one at a time.
6. Generate a fresh packet, rerun structural validation and perceptual review,
   then update the decision only with a new hash-bound record. Do not mutate the
   old record to make history look clean.
7. Record the cycle receipt with `scripts/visual_cycle.sh`; a `hold` or
   `falsified` disposition is valid when its next command and holdout status are
   explicit, but it is not a renderer-fix claim.
8. Update the golden only with `--approve --intent "..."` after the new frame is
   visibly better, the cycle receipt is accepted, and all strict checks pass.
9. After a renderer or routing fix, rerun the complete existing fixture corpus
   in both the requested-style and authored-policy/no-override lanes, and
   drain both full one-frame decision ledgers with the history ledger loaded.
   Promote the lesson only after focused defect rows, direction/style/mode
   homologs, evaluator holdouts, authored-policy controls, and all ordinary
   corpus rows are separately accounted for.

### H84 lesson: inspect the renderer-owned portal lane

For a titled TD/TB external entry, do not infer visual alignment from the
internal target center alone. Inspect the packet's portal/evidence geometry and
the rendered frame to identify the live title-safe portal lane actually consumed
by the route lowerer. A source can be centered on its target and still produce
an unnecessary source-to-portal elbow when the title-safe lane is offset.

If the defect is isolated to one flat titled subgraph with one internal node
and two ordinary edges, a bounded placement experiment may align the external
source to that live portal lane only after strict topology, canvas, envelope,
unrelated-node, proposal-overlap, and transactional guards pass. Add a negative
control for multi-entry, nested, labeled, or crowded scenes; never generalize
the single-entry move from a clean critic result. Rebuild both full 237-input
policy packets and freshly inspect every affected homolog before resolving the
watch or promoting the lesson.

### H84 full-corpus lessons: border labels, receiver lanes, and warning resolution

The full-corpus drainage pass adds three reusable rules to the loop:

- Treat an external edge label written into a titled subgraph border as P1
  until a fresh frame proves otherwise. A label such as `success` embedded in
  `━━success━┘│━━` is not merely a cosmetic border break: it can read as a
  group caption, route annotation, or portal glyph at the same time. Inspect
  text ownership and border continuity together, and require a separate
  edge-label band or an otherwise unambiguous placement before resolving the
  watch.
- Evaluate portal allocation by target receiver ownership, not only by source
  alignment or route continuity. A route may be continuous and all arrowheads
  may exist while an external TD/BT edge enters through a neighboring receiver
  lane and makes an internal horizontal hook. Record the exact portal cell,
  target lane, and internal elbow; compare flat, narrow, multi-entry, nested,
  labeled, and sibling-subgraph homologs before proposing a fix.
- Adjacent fan-in arrowheads require a split judgment. Symmetric two-source
  convergence with mirrored elbows can be a clean control; asymmetric
  three-way or complex fan-in with staggered detours is a P2 watch even when
  the critic is clean. The hypothesis must name receiver-slot allocation or
  merge geometry, and the falsifier must require separated, locally readable
  target attachments in every affected direction/style/mode homolog.

The route critic's `inconclusive` portal warning is a review queue, not a
verdict. Record a pass only when the complete frame makes portal ownership
immediately legible (for example, aligned parallel rails); record a watch when
long cross-group rails, paired receiver hooks, or external-sink fan-in require
global tracing. Every result remains an independent exact-hash decision: a
clean homolog does not cover a different style, mode, policy lane, or
regenerated frame. The self-improving loop must continue draining all 936
renderable rows in both the canonical and no-override packets, with the 12
expected-error rows governed separately, before a cycle or golden can be
called complete.

### H86 lesson: horizontal sibling-chain receiver rows

When three or more flat titled sibling groups form a strict LR/RL chain, a
single straight cross-group rail can make the middle group's incoming and
outgoing roles read as one bus even when all arrows and geometry checks pass.
Treat this as a receiver-ownership defect, not merely a junction-glyph issue.
The safe experiment is a topology-gated scene transaction that allocates one
quiet corridor row per adjacent transition, gives each source and target
boundary opening an explicit claim, and preserves a visible receiver shaft.
Require common frame height/common node row, rectangular two-node members,
unlabeled acyclic adjacent arrows, deterministic target-entry decisions, and
negative controls for BT/TD, two-sibling, nested, labeled, and crowded scenes.
The critic must be clean in ASCII and Unicode, optimized and unoptimized
homologs, and the full two-lane corpus must be regenerated before resolving
the corresponding visual-history watch.

H88 full-corpus review confirms the distinction: the receiver rows remove the
single uninterrupted LR/RL bus, but the resulting U-shaped seam elbows remain
visually awkward under human-eye review, while BT sibling/parallel rails still
read as long titled-border trunks. Keep those four history records open even
when the critic is clean. The next repair must be a bounded geometry/layout
experiment followed by fresh packets and 936 perceptual decisions in both
lanes; never resolve the history from machine cleanliness alone.

### H89 lesson: retain seam-level observations and falsified geometry

The H89 source epoch regenerated both complete policy lanes: 948 rows per lane,
936 renderable frames, and 12 separately ledgered expected errors, with zero
packet findings or integrity failures. Fresh perceptual ledgers covered all
936 frames in each lane. Their conservative queue split was 64 `pass` and 872
`watch`; that split is triage evidence, not a claim that every watch is a
confirmed defect. Direct inspection of the four historical homolog families
confirmed the recurring tiny-detail signals: doubled seam rails and U-shaped
receiver elbows in LR/RL, a long trunk through titled BT sibling borders, a
parallel-BT title portal shoulder, and a shared-target/border ambiguity in the
two-sibling LR scene.

For LR/RL sibling-chain reviews, record the seam window and exact row/column
of the elbow, adjacent frame wall, portal marker, and receiver shaft. Use the
finding class `horizontal_receiver_seam_elbow` when the route is connected but
the local shape reads like a box or a second border. For BT, record every
titled boundary crossed by one rail and use `bt_title_boundary_rail` or
`bt_title_portal_disconnection` when ownership is not locally legible. These
observations must remain open across ASCII/Unicode, optimized/unoptimized,
canonical, no-override, and holdout homologs.

The first H89 bounded follow-up moved LR/RL vertical turns one cell farther
inside each sibling frame. It was falsified: the LR homolog produced critic
findings for disconnected junction arms and arrows without a visible shaft.
The candidate was reverted, its focused tests were rerun, and the negative
result was retained. A failed experiment is a valid self-improvement outcome;
the next hypothesis must change the layout-owned quiet-band allocation or a
different seam ownership boundary, not repeat the same renderer coordinate
nudge. Never resolve an open history record from the 64/872 counters or a
clean critic alone.

### H90 lesson: package boundaries are part of the review flow

A published-crate consumer test exposed a non-visual fixture-boundary defect:
the strict LR/RL unit module read repository fixtures that the package
correctly excludes. Fixture-dependent tests must be behind the explicit
`maintainer-fixtures` feature, while repository CI enables that feature for
full visual coverage and the package contract runs without it. A package or
release failure is therefore a self-improvement finding: fix the ownership
boundary, rerun repository tests, rebuild both visual lanes, and reopen every
renderable frame before treating unchanged output as evidence. Do not solve a
package failure by adding the entire maintainer corpus to the consumer crate.

### H91 lesson: quiet-band redistribution is not a layout fix

A focused candidate redistributed the two LR/RL sibling-chain transition rails
across the upper and lower quiet bands. Structural selector, route-trace, and
critic tests remained green, but direct visual review falsified the candidate:
the upper rail crossed the middle group's title/border region and formed a
second box-like enclosure (`┌─────┐`) around the seam. The candidate was
reverted and was not promoted to a source epoch, packet, golden, or history
resolution. Green structural signals do not authorize a geometry change that
worsens the human-eye frame.

The next LR/RL hypothesis must allocate a genuine layout-owned quiet row or
change seam ownership so the route does not traverse title/border territory;
do not retry upper/lower row redistribution or another one-cell coordinate
nudge. Any promoted candidate still requires both full 237-input lanes,
separate 12-row expected-error ledgers, and 936 fresh perceptual decisions per
lane before it can affect visual history or goldens.

## Integration points

- Rust implementation: `termiflow-qa review`, `src/qa/review.rs`, and the
  packet/evidence validators, with `src/qa/visual_history.rs` enforcing
  append-only historical risk resolution.
- Bash entry points: `scripts/visual_audit.sh`,
  `scripts/review_visual_packet.sh`, `scripts/visual_validate.sh`, and
  `scripts/regenerate_golden.sh`, plus `scripts/schema_visual_cycle.sh` for
  the schema-to-candidate/packet/holdout boundary and `scripts/visual_cycle.sh`
  for the hash-bound fix/hold/lesson boundary.
- Cycle record contract: `tests/fixtures/visual_cycle_record.schema.json`.
- Visual history contract: `tests/fixtures/visual_review_history.schema.json`.
- Schema-cycle receipt contract:
  `tests/fixtures/schema_visual_cycle_summary.schema.json`.
- Contributor contract: `CONTRIBUTING.md` and `tests/fixtures/README.md`.
- Maestro checkpoints: record research, plan, implementation review, decision,
  and run/completion artifacts for each material workflow slice.
