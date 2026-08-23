# Visual review rubric

TermiFlow's visual reviewer is a human-eye pass over one rendered frame at a
time. Machine critic, parser, geometry, route, and golden checks are triage;
they never replace visual judgment.

## Required protocol

For every renderable packet row, the reviewer must:

1. Read the Mermaid schema and identify direction, topology, node/edge count,
   subgraphs, labels, shapes, and any authored style directive.
2. Inspect the complete frame before reading the machine evidence. Check
   semantics, route continuity, crossings, overlap, portals, borders, titles,
   spacing, density, corners, junctions, seams, arrowhead attachment, labels,
   clipping, wrapping, CJK/emoji width, fallback glyphs, and tiny artifacts.
3. Compare the relevant style/mode/direction homologs and matched controls.
4. Record a frame-specific observation and a falsifiable ownership hypothesis.
   A `watch` or `fail` must include exact `x,y` cells and a note for every
   visible concern. A `pass` must still describe what was visibly checked.
5. Record the current frame, evidence, effective-policy, and run hashes. A
   fresh review may not contain a carry-forward record or stale next command.

Expected-error rows use a separate behavioral ledger: exit status, stdout,
stderr policy, and expected diagnostic text must be reviewed explicitly.

## What counts as fresh

Fresh means the reviewer opened the current packet row and wrote a new
perceptual decision for that exact frame. Rebinding an unchanged frame is
useful delta evidence, but it is not fresh visual coverage and cannot close a
full-corpus acceptance gate.

## Evidence hierarchy

1. Human-eye observation of the current frame.
2. Matched homolog/control comparison.
3. Independent semantic, route, geometry, critic, and raw-glyph oracles.
4. Golden snapshots and machine summaries.

The lower layers support the observation; they cannot overrule a visible
defect or silently discharge a human-eye watch. If a warning is intentional,
name the exact glyph/cell behavior, why it is readable, and the regression or
falsifier that keeps it intentional.
