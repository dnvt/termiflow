# Bounded long edge labels need an explicit policy review

Status: Promoted rule
Source epoch: H146f full-corpus visual convergence, 2026-08-22
Fixture family: `label_edge_long_bt` and the long-label homologs

## Observation

With the default edge-label budget, the BT fixture visibly renders the source
label `This is a very long edge label text` as `This is a very long…`. The
ellipsis is visible, the arrow shaft remains continuous, and the Start/End
nodes remain readable. The same behavior appears in both the canonical style
lane and the authored/no-style-override lane (including Unicode fallback).

This is a human-visible loss of source text, but it is not silent clipping.
Reviewers must distinguish a deliberate bounded-label ellipsis from a defect:
missing ellipsis, a split grapheme, overlap with a route/node/portal, or text
loss after an explicitly expanded budget remains a real visual defect.

## Hypothesis and falsifier

The default `max_edge_label_width=20`, `wrap_labels=false`, and
`max_label_lines=1` policy intentionally bounds vertical edge labels. The
focused regression in `tests/render_options_api/feedback.rs` requires visible
ellipsis at the default budget and complete text at an expanded budget.

The bounded-policy hypothesis is falsified if `--max-edge-label 40` still loses
text, if ASCII/Unicode measurement splits a grapheme, or if any direction,
style, mode, terminal-width, CJK/emoji, or portal/crossing homolog makes the
ellipsis ambiguous or colliding.

## Evidence

- Focused regression: `tests/render_options_api/feedback.rs` (`vertical_long_edge_label_is_bounded_without_silent_clipping` and `vertical_long_edge_label_honors_expanded_budget`).
- Public contract: `docs/reference.md` documents `--max-edge-label` and the default width budget.
- Complete-corpus result: current H146f packet scope is 241 inputs / 964 rows / 952 renderable rows / 12 expected-error rows; full perceptual closure remains open.
- Human-eye result: the default ellipsis is retained as an intentional P2 warning while the label-family homologs remain under review.

## Promoted rule or next experiment

Keep the bounded default unchanged for this cycle. Review the complete
long-label family before considering edge-label wrapping or a wider default;
`--wrap --max-lines` is not by itself an edge-label escape hatch. Any future
policy change must regenerate both canonical and authored packets and inspect
route ownership, arrow continuity, terminal-width/cropping, CJK/emoji, and
negative-control homologs before golden approval.
