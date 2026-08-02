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
- Do not overwrite goldens or decisions in place. Use the Rust QA command and
  the guarded Bash wrapper; source fixes require a fresh packet and a second
  inspection.
- Use Rust and Bash only. Do not create or invoke Python/Ruby files or scripts.

## Workflow

1. Build or select a packet with `scripts/visual_audit.sh`. Confirm its
   completion marker, manifest identity, binary identity, and expected-error
   count.
2. Run the conservative Rust prescreen through
   `scripts/review_visual_packet.sh ... --prescreen-clean`. Treat its count as
   structural coverage only.
3. Pull one residual frame with `--next`. Read the complete frame payload and
   inspect the rendered ASCII/Unicode block without opening source first.
4. Use the checklist below. Stop at the first material ambiguity, but still
   record all dimensions that were actually inspected.
5. Write one decision JSON object with the exact hashes, `reviewer: ai` or
   `human`, a decision (`pass`, `watch`, `fail`, or `unclear`), severity,
   dimensions, observation, hypothesis, falsifier, expected observation,
   targeted next command, and affected homologs. Append it through
   `--record`; never append by shell redirection.
6. Repeat `--next` only after the previous record succeeds. Finish with
   `--validate`; exception rows require perceptual decisions even if a machine
   pre-screen exists.

For a deliberate full perceptual pass, use `--include-structural` when pulling
the queue. This is slower but useful after layout or renderer-wide changes.

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
`fixture_wrap_contract_mismatch`; they do not merely say “layout bug”. Include
cell coordinates when a single glyph or seam is the evidence.

## Self-improvement loop

For each `watch` or `fail`:

1. Preserve the original frame and decision hashes.
2. Add the smallest Rust regression test or fixture that isolates the observed
   ambiguity, preferably in every affected direction/style homolog.
3. Form one falsifiable implementation hypothesis. Change one rendering layer
   or policy boundary at a time.
4. Run targeted Rust tests, render the isolated fixture, and inspect the new
   frame one at a time.
5. Generate a fresh packet, rerun structural validation and perceptual review,
   then update the decision only with a new hash-bound record. Do not mutate the
   old record to make history look clean.
6. Update the golden only with `--approve --intent "..."` after the new frame is
   visibly better and all strict checks pass.

## Integration points

- Rust implementation: `termiflow-qa review`, `src/qa/review.rs`, and the
  packet/evidence validators.
- Bash entry points: `scripts/visual_audit.sh`,
  `scripts/review_visual_packet.sh`, `scripts/visual_validate.sh`, and
  `scripts/regenerate_golden.sh`.
- Contributor contract: `CONTRIBUTING.md` and `tests/fixtures/README.md`.
- Maestro checkpoints: record research, plan, implementation review, decision,
  and run/completion artifacts for each material workflow slice.
