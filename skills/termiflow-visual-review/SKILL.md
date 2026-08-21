---
name: termiflow-visual-review
description: Review TermiFlow ASCII and Unicode diagram frames for human-visible semantic, routing, containment, text, and rendering defects with hash-bound evidence and targeted self-improvement.
version: 0.1.8
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
- The route-clarity oracle explicitly queues title-adjacent TD/TB and BT
  elbows (`+-+`, `┌─┘`, `└─┐`, and close Unicode variants) as human-review
  signals when they occur inside a titled subgraph with a declared boundary
  edge. This detector is deliberately not an automatic failure: node and
  subgraph border corners may be legitimate, so inspect the exact cells and
  surrounding route before recording the perceptual decision.
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
   combination in both lanes: the current inventory is 241 inputs, 964 rows,
   952 renderable frames, and 12 expected-error rows per lane. A missing
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
   In dense vertical crossing scenes, inspect the final turn before every
   target arrowhead: a corner directly adjacent to `↓`/`↑` (or `v`/`^`) is a
   human-eye defect even when the route is connected. Require one straight
   target-facing shaft cell, and verify that the corresponding `x`/`✕`
   crossing markers and source/target port identities remain present. Keep
   this invariant scoped to the topology-owned TD/TB/BT dense scene; do not
   export its lane shift to LR/RL side-port routing without fresh homolog
   evidence.
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
regenerated frame. The current self-improving loop must continue draining all 952
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

### H99 lesson: minimum turn clearance is a perceptual invariant

The current live corpus is 241 inputs, 964 rows per policy lane, 952
renderable frames, and 12 separately ledgered expected errors. Historical H84,
H89, and H91 counts above describe their original epochs; do not use them for
current coverage accounting.

For direct BT parallel edges, a route turn with only one visible shaft cell
between its corners composes as `┌─┘` or `+-+` and reads like damaged border
punctuation. The topology-owned lane allocator now requires a minimum
three-cell center clearance, leaving two visible shaft cells whenever the
scene has room. Keep the focused regression in both ASCII and Unicode and in
default and optimized modes. Inspect all four direction homologs and both
policy lanes before resolving the watch; do not generalize the BT fix to LR,
RL, or TD without a fresh frame.

The route critic may still report
`bt_title_boundary_hook_requires_human_review` with score zero and no findings.
That is a queue signal, not approval. Record a human-eye pass only when the
full frame makes each rail, portal, title gutter, arrowhead, and border
ownership immediately legible. In the authored/no-override packet, report the
effective policy and rendered glyph style observed in the frame; the requested
style label alone cannot cover an authored control row.

### H100 lesson: collinear LR/RL sibling bridges stay straight

The current live corpus is 241 inputs, 964 rows per policy lane, 952
renderable frames, and 12 separately ledgered expected errors. Historical
counts above describe their source epochs and must not be substituted for the
current denominator.

In the strict three-sibling LR/RL chain, the source and receiver edge ports can
already share one center row. Sending those collinear transitions through a
lower corridor creates two one-cell-inside elbows beside the neighboring
subgraph walls; the resulting `+------+`/`└──────┘` fragment looks like a tiny
second box even though the route is connected. The topology-owned scene must
keep an all-collinear chain on the actual edge center row and reserve a quiet
corridor only for transitions whose source and target rows differ.

The focused regression covers LR and RL, ASCII and Unicode, and default and
optimized rendering. It rejects the mini-border seam and requires horizontal
portal ownership to remain present. This rule is not inferred for TD, TB, BT,
nested, labeled, or crowded scenes; those require their own fresh homolog
review. A direct bridge is still a watch if a portal glyph becomes a junction,
an arrow shaft disappears, a node border is overwritten, or the middle sibling
loses locally legible incoming/outgoing ownership.

After this focused fix, regenerate both complete policy packets and drain a
fresh one-frame decision for every renderable row. Preserve the earlier H86,
H89, and H91 seam watches until the full matrix—including holdouts and
authored-policy controls—shows that the straight bridge improves the selected
homologs without exporting the rule into unrelated topology.

### H101 lesson: TD sibling corridors need a layout-owned turn band

The current live corpus is 241 inputs, 964 rows per policy lane, 952
renderable frames, and 12 separately ledgered expected-error rows.

In the direct three-sibling TD chain, two exterior rows are not enough when a
title-safe portal lane must move laterally. The specialized corridor selector
rejected the gap, the generic route emitted a compact border-adjacent elbow,
and geometry left the two cross-subgraph edges as untraced fallback edges. A
critic-clean score did not make that frame a visual pass.

The layout contract now reserves three exterior cells for direct stacked
TD/TB siblings: one quiet cell before the turn, one turn cell, and one portal
shaft cell before the receiving border. The renderer accepts a two-row
topology-owned fallback only when layout cannot expand and rejects a one-row
gap. The focused regression covers ASCII/Unicode and default/optimized modes
and requires all five edges to be traced with no geometry errors.

This rule is limited to direct, non-nested sibling crossings. Do not export it
to fan-in, fan-out, nested, labeled, or crowded scenes without a fresh
perceptual review of both the canonical requested-style and authored
no-override lanes. A fresh complete-corpus packet and one-frame decisions are
still required before resolving the TD watch or touching goldens.

### H102 lesson: TD terminal entries need distinct target-center lanes

In `collision_edge_corner_td`, two external boxes entered distinct nodes in
one titled TD subgraph. The generic portal route was connected and machine
clean, but a one-column source/target parity mismatch composed adjacent
`└┐`/`++` corner glyphs that read like a broken border. A connected route is
not a visual pass when a human eye sees a tiny damaged junction.

For a flat, titled, one-to-one TD/TB terminal-entry scene, the layout stage may
align each external source to its distinct internal target center. The proposal
set is staged and rejected unless every move is within the small displacement
budget, remains inside the canvas, clears the subgraph and unrelated nodes, and
does not collide with another proposed source. Fan-in, fan-out, nested,
labeled, and crowded scenes remain fail-closed.

The focused layout regression is
`td_terminal_entry_sources_align_to_distinct_target_centers`; the render
regression covers ASCII/Unicode and default/optimized modes and rejects both
the `++` and `└┐` artifacts. A fresh complete canonical and authored packet is
still required before resolving this watch or changing goldens. Inspect the
target lane, source shaft, portal ownership, arrowhead, and neighboring
subgraph wall as one human-eye unit; critic and route-clarity cleanliness alone
cannot close the micro-artifact.

### H104 lesson: dense vertical crossings need a quiet target shaft

In the six-lane `crossing_grid_td` and `crossing_grid_bt` scenes, the prior
dense allocator placed the final lane immediately beside its target
arrowhead. The connected, critic-clean frame still composed as a tiny
`└─┐↓`/`+-+v` (or mirrored BT) border-like hook. Human-eye review must inspect
the final turn and arrow as one unit, not approve the frame from route counts.

The topology-owned vertical lane band now keeps its two-cell pitch but uses
offsets `1,3,5,...` instead of `2,4,6,...`, leaving one straight shaft cell
before each target arrowhead. The rule is limited to TD/TB/BT. LR/RL retain
their earlier side-port spacing; a fresh homolog review caught and rejected
the first over-broad version before it entered the final packet.

Widening source ports to match target ports was explicitly falsified: it
removed the required explicit crossing markers and failed the independent
route-identity oracle. Preserve port ownership and use the narrow lane-band
repair instead. The focused regression and oracle commands are recorded in
the public lesson
[`dense-vertical-crossing-target-clearance-2026-08-13.md`](../../docs/visual-lessons/dense-vertical-crossing-target-clearance-2026-08-13.md).

After this kind of routing fix, rebuild both complete 241-input policy lanes,
validate the 964-row packets and separate 12-row expected-error ledgers, and
freshly inspect every affected direction/style/mode homolog before closing a
watch or touching goldens.

### H105 lesson: TD parallel sibling lanes clear title portal hooks

In strict flat titled TD/TB sibling scenes, already aligned parallel source and
target columns can still be routed through a title-adjacent bracket-like portal
shoulder. The route is connected and critic-clean, but a human reader can read
the shoulder as a title hook or shared border junction. The layout envelope
stage now applies a bounded, topology-gated translation to the aligned pair,
keeps the original title left edge, and reuses the live title-safe target lane.

The policy fails closed for mixed, labeled, nested, crowded, and non-aligned
scenes. Its focused regression is
`render_td_parallel_siblings_keep_target_lanes_clear_of_title_hooks` in
`tests/render_options_api/direction_matrix.rs`. Inspect both direct and
crossed parallel homologs in both requested-style lanes before resolving any
BT/LR/RL watch; this rule is not inferred for those directions. The complete
241-input/964-row packets and 12-row error ledgers remain required, and no
golden approval follows from the focused repair.

### H106 lesson: mixed sibling targets need a real visual gap

When a titled sibling scene combines an internal edge and a cross-subgraph edge
into the same vertical receiver, two connected arrowheads can still look like
one cramped junction. The exact TD/TB and BT mixed-target lowerers now prefer a
three-column minimum between their title-safe target entries, retaining the
old two-column choice only when the node cannot provide the wider pair.

The focused regression `mixed_vertical_sibling_targets_keep_a_readable_entry_gap`
in `tests/subgraph_boundary_arrows.rs` covers ASCII/Unicode and
default/optimized modes. A candidate is falsified by a lost shaft or arrowhead,
changed edge identity, node/title collision, or a worsened tight, triple,
nested, labeled, LR, or RL homolog. The machine-clean critic is not enough:
fresh one-frame review must confirm the receiver is locally readable in both
canonical requested-style and authored no-override lanes. The complete
241-input/964-row matrix, separate 12-row error ledgers, open BT title-hook
and TD chain watches, and golden gates remain active.

### H118/H119 current negative-loop guards

The current H118 fresh slice re-reviewed all four direct BT parallel
style/mode homologs in both policy lanes. Raising
`BT_SIBLING_MIN_RAIL_GAP` from 3 to 5 was falsified: the endpoint contract
became unsatisfiable for the ASCII collision case, distinct middle-boundary
roles collapsed into one shaft, and the focused BT sibling-chain test failed.
Never retry a BT spacing-only candidate without proving that the endpoint
contract still allocates every source/target role.

The H119 LR/RL follow-up alternated successive sibling transitions between
lower and upper quiet bands. Structural tests passed, but direct frames put an
upper bridge into a border-shaped band above the nodes. This was also
reverted. A green route trace or critic score is insufficient when a corridor
changes into a container-like contour. The next LR/RL candidate must change
local seam ownership while preserving one readable corridor band and must be
reviewed in both mirrors before promotion.

The H120 mixed-target follow-up tried the complementary placement shortcut:
move each internal LR/RL end node onto its paired start-node row. The focused
matrix rejected the target-entry scene in all eight LR/RL style/mode cases,
leaving only three arrows and an untraced `B->D` edge. This candidate was
reverted. Never promote a node-row alignment unless the target-entry scene
planner still proves all four edge identities and both mirrored route plans;
the next repair must own the scene endpoint contract, seam, or route search.

### H121 lesson: flat TD/TB single entries need a literal title gutter

In a flat titled subgraph with one unlabeled external direct entry, the
title-safe portal can be one column away from the source's otherwise natural
centerline. The connected route then composes a tiny `+-+`/`┌─┘`-like shoulder
beside the title. The critic may remain clean because this is a human reading
ambiguity, not a missing edge.

The topology-owned TD/TB rule gives this exact one-entry scene a zero extra
title margin and aligns the source only to the live, renderer-owned portal
lane. The predicate excludes nested, labeled, multi-entry, sibling, and
crowded scenes. The focused regression covers `subgraph_single_td` and
`subgraph_outside_td` across ASCII/Unicode and default/optimized rendering;
the route-clarity oracle also has a mutation test that queues an injected
title hook while leaving repaired frames unqueued.

The corresponding BT experiment aligned only the external source. It removed
the source-side offset but left a target-side title-gutter elbow, so endpoint
staging was explicitly falsified and retained as a watch. Do not retry that
experiment as a generic source/target alignment rule. A successful BT follow-
up must name target-side seam ownership, preserve every endpoint identity, and
pass the complete 241-input/964-row canonical and authored/no-override review
before a history resolution or golden approval.

The complex BT scene is also an explicit negative control for the single-entry
rule. `subgraph_complex_bt` has two titled subgraphs and shared Data/Service
rails; the predicate must require exactly one flat subgraph before it moves an
external source. If a complex multi-subgraph frame changes because of that
rule, treat it as a topology scope regression. The v10 four-cell complex-BT
holdout remains a P2 watch for long shared rails and title-adjacent elbows,
with a separate future hypothesis required before changing its portal planner.

### H127 lesson: quiet BT corridors need a shoulder detector

A source-side complex-BT experiment made one receiver shaft straighter and
removed a lower title elbow, but introduced an adjacent `┌┘`/`++` shoulder in
the empty corridor below the Data Layer border. All four style/mode homologs
were connected and machine route-clean; direct human-eye review rejected the
candidate and it was rolled back. A connected route can therefore regress in
the first quiet corridor row even when title-hook and segment checks are
clean.

The independent route-clarity review now queues
`bt_quiet_corridor_shoulder_requires_human_review` for adjacent Unicode corner
pairs or ASCII `++` pairs with vertical context between vertically stacked
titled BT sibling envelopes. This is a conservative P2 queue signal, not an
automatic failure or approval. Its focused mutation regression is
`bt_quiet_corridor_review_queues_adjacent_shoulders` in
`src/qa/route_clarity.rs`, and the unmodified complex-BT baseline must remain
unqueued by that new rule.

When reviewing a BT sibling corridor, inspect the first row below the upper
border, the last row above the lower border, and every adjacent corner pair in
between. Record the exact row/column and glyphs before naming an owner. Any
new shoulder falsifies the candidate even if the source/target endpoints,
route trace, critic, and geometry reports remain green. Keep both the
target-side title seam and the source-side corridor shoulder in the same
falsifier set, and rerun all four style/mode homologs in canonical and
authored/no-override lanes before resolving the watch or touching goldens.

### H133 lesson: a source-node repair can be narrower than a source-group repair

The follow-up experiment moved only the external API node onto the existing
S1 receiver lane, derived from the unique structural entry edge. It removed
the lower API → S1 title hook in all four ASCII/Unicode × default/optimized
frames and in both policy lanes, while leaving the titled sibling groups and
Data Layer nodes unchanged. The prior H127 shoulder did not recur. Keep this
as a topology-gated source-node policy, not a generalized source-group rule;
the shared sibling rail remains a named P2 watch and the full 952-renderable
row perceptual queue must still be drained before any golden or release
decision.

### H145 lesson: repeated BT title noise does not authorize a global row move

Fresh canonical and authored frames independently queued the BT title/border
lane in simple and parallel titled subgraphs, and a sibling frame also showed a
boundary-rail/title-hook ambiguity. A bounded experiment moved every BT title
from the final interior row to the preceding row. The simple title band looked
quieter, but the changed `collision_sibling_subgraphs_bt` homolog lost one of
Node D's two incoming arrow identities and collapsed route ownership into a
misleading junction beside `Right Group`. The candidate was reverted; the
hash-bound falsification is recorded in
`2026-08-21-h145-bt-title-row-falsification.md`.

Treat this as a negative-loop guard: a repeated symptom may cross topology
owners, while a global visual offset can hide the symptom by damaging a sibling
route. Do not change `subgraph_title_row` direction-wide to discharge a BT
title watch. Any follow-up must be selected by a typed receiver/portal scene,
keep title placement and edge lowering on the same topology-owned contract,
and inspect basic, parallel, sibling, untouched BT, and non-BT holdouts. A
lost arrowhead, merged route identity, changed receiver ownership, or new
title/border shoulder falsifies the candidate even if route counts and critic
checks remain green.

### H145 dense-RL scanability guard

`scale_dense_rl` is a separate human-eye watch: local routes can be connected
and clear while the full frame becomes difficult to scan because of excessive
blank horizontal spans or unbalanced visual massing. Do not approve it from
route clarity alone, and do not change rank spacing until before/after
measurements compare direction/style/mode homologs and preserve label clearance,
edge order, and node readability. Record the exact global composition issue and
its falsifier; a local clean segment is not evidence that the canvas is
composed well.

### H145 mixed-BT branch-rail guard

The canonical `junction_mixed_bt` frame is connected and retains all six
arrowheads, yet its upper fan-in rail, lower fan-out rail, and adjacent elbows
form a visually dense ladder. Queue this as
`bt_mixed_branch_rail_semantic_density_requires_human_review` until the same
topology is checked in all style/mode and policy homologs. A clean critic or a
complete arrow count does not prove that each branch is still individually
traceable to a human reader.

Before proposing a fix, compare the upper and lower rail ownership, arrowhead
separation, label clearance, and any border-like reading against untouched
fan-in/fan-out controls. Reject a candidate that makes one branch quieter by
merging an edge identity, moving a receiver off its source-owned lane, or
turning a shared rail into a container-like contour.

### H146 lesson: routed labels cannot own route turns

An edge label can leave its arrowhead visible while erasing the corner that
connects the arrow's shaft. That composes as a broken `←─label`/`+<-label`
route and is a P1 visual defect when the target edge becomes ambiguous. The
label writer must treat route corners, arrows, vertical shafts, title cells,
borders, and semantic owners as protected. It may replace only blank or
horizontal edge-owned cells and must choose a nearby safe slot deterministically;
if no slot exists, omitting the label is safer than damaging topology.

The focused regression is
`render_with_feedback_keeps_rl_convergent_labels_off_route_corners` in
`tests/render_options_api/direction_matrix.rs`, with the complete label golden
family checked in ASCII and Unicode. After changing label ownership, inspect
every changed label row in default and optimized modes and in both the
requested-style and authored-policy lanes. Exact frame-hash rebinding is
acceptable only for unchanged rows; changed rows require fresh one-frame
inspection. This rule does not generalize into a layout or portal-lane move.

### H147 lesson: safe BT sibling transitions should stay straight

Strict bottom-to-top chains of titled sibling subgraphs rendered repeated
long `┌────┘`/`+----+` shoulders because the endpoint contract deliberately
selected a non-collinear receiver lane even when the source lane was safe for
the target. That was visually noisy without adding route identity: the middle
boundary already had separate incoming and outgoing lanes.

Prefer the source-aligned target portal only when it is inside the target node,
title-safe, and separated from the next source and prior target lanes. Keep the
existing separated-lane search for unsafe or colliding candidates. The focused
regressions are `strict_bt_sibling_chain_prefers_straight_target_portal_lanes`
and `strict_bt_sibling_chain_separates_middle_boundary_roles` in
`tests/bt_sibling_chain_visual.rs`. Inspect both strict BT fixtures across
ASCII/Unicode, default/optimized, and canonical/authored lanes, plus BT
parallel, nested, complex, and non-BT controls. A lost middle boundary role,
title-safe portal, arrowhead, or required near-miss turn falsifies the rule.
This is an endpoint-contract policy, not a global layout shift.

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
