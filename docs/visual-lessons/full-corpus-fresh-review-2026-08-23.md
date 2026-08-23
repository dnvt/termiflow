# Full-corpus fresh review keeps visual watches honest

Status: Baseline recorded; follow-up experiments remain open
Source epoch: H155 fresh corpus review, 2026-08-23
Population: all 241 `tests/fixtures/inputs` fixtures, both ASCII/Unicode
styles, and default/optimized modes

## Observation

The complete matrix contains 964 rows: 952 renderable frames and 12 governed
expected-error rows. A fresh one-frame perceptual review covered every
renderable row in both the canonical requested-style lane and the authored
no-override lane. The frames are generally connected and readable, but a
human-eye watch queue remains concentrated in a few repeatable families:

- subgraph portal and boundary seams, especially sibling, nested, chain, and
  direct cross-container routes;
- dense convergence, explicit crossing grids, and dense-scale channel spacing;
- long node and edge labels whose bounded ellipsis preserves geometry but loses
  visible semantic content;
- BT parallel and corner transitions where a one-cell seam or title-adjacent
  shoulder can look like a damaged container border.

These are perceptual watches, not automatic failures. A clean critic, geometry
trace, or golden snapshot cannot close them by itself.

## Hypotheses and falsifiers

1. Portal projection and route lowering need one shared, direction-aware owner
   for each boundary crossing. Falsifier: a change merges rails, overwrites a
   title/node, detaches an arrow, or regresses a matched direction/style/mode
   homolog.
2. Dense scenes need explicit channel budgets before rendering. Falsifier: a
   route becomes less readable, loses endpoint identity, or adds crossings in
   a control that was previously clear.
3. Label policy must make truncation or wrapping explicit and reviewable.
   Falsifier: the new policy clips text, changes display width, damages a
   route corner, or silently drops more semantic content.

## Required next loop

Use `scripts/review_visual_packet.sh` with `--fresh` after every renderer
experiment. Review the complete fixture corpus, not just the exposing fixture;
bind every watch/fail to exact frame cells; compare canonical and authored
style provenance; run the focused oracle and the full Rust matrix; then record
the observation, hypothesis, falsifier, and result in a follow-up lesson.

This public lesson records reproducible engineering evidence only. Private
Maestro capsules, transient packets, prompts, and provider traces stay out of
the OSS repository.
