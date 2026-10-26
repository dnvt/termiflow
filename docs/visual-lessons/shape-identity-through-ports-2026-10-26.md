# Visual lesson: shape identity through ports

Date: 2026-10-26
Status: code and focused controls reviewed; complete perceptual cycle pending
Scope: Subroutine and Stadium projection, including the ASCII side-rail fallback

A declared shape must remain distinguishable when its label does not name the
shape. The reviewed all-shapes frames made connected Subroutine and Stadium
nodes look like ordinary rectangles, even though their labels and arrowheads
survived. Generic side-port painting erased their distinctive strokes. A later
source-junction pass could erase a preserved Subroutine wall again.

Keep shape-owned contours through both drawing and later port publication.
Stadium retains its curved sides; the Unicode Subroutine retains its double
wall. The single-stroke Subroutine fallback uses a second rail in the existing
padding, preserving the node rectangle and external connection positions.
Ordinary boxes keep their normal ports, including double-border styles.

Padding safety requires two checks. The full label must fit the proposed inner
band, including meaningful spaces. Every actual rail cell must also be in
bounds and empty before any rail is written. Grapheme display width alone is
insufficient: the existing label writer can place a wide-character continuation
in that band for a complex emoji. Admit both complete rails transactionally;
otherwise retain the emitted label cells. This change does not repair the
pre-existing complex-grapheme writer or promise a cue in an undersized box.

The focused regressions read raw rendered text and compare reached nodes with
declared neighbors. They cover isolated, source, target, through, fanout and
cycle attachments, mirrors, styles and optimization modes. Negative controls
cover ordinary shape ports, fallback and mixed fonts, narrow and multiline
labels, CJK, emoji, combining marks, whitespace and clipped canvases. Seven
selected compiling mutations independently falsify the relevant guards.

The evidence remains scoped to this candidate. Full canonical and authored
corpus review, separate evaluator holdouts, generated-frame comparisons,
topology checks and the accepted visual-cycle receipt precede intent-bound
golden approval. Final source checks and current maintainer approval remain
separate gates. The private remediation catalog retains every original P2
observation and both rejected candidate hypotheses.
