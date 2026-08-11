# Visual lesson: portal markers must declare their route axis

## Observation

The portal-marker repair exposed a broader golden scope than the first visual
review predicted: the exact-source golden check produced 72 candidate changes
across 36 LR/RL fixture stems in ASCII and Unicode. The old side-wall marker
could be rendered as a perpendicular junction-like glyph even when the edge
route itself was a horizontal shaft. TD/BT portal stems must remain vertical.

## Owner-layer hypothesis

Portal stamping must receive an explicit `PortalAxis` derived from the route
direction. `Horizontal` owns LR/RL wall crossings and uses the style's
horizontal edge glyph; `Vertical` owns TD/BT crossings and uses the vertical
edge glyph. Inferring the axis from a generic portal glyph or from neighboring
canvas cells is unsafe because those cells may already contain borders,
junctions, labels, or another route.

## Evidence and falsifier

The bounded homolog matrix covers 144 perceptual rows across 36 LR/RL variants,
ASCII/Unicode styles, and default/optimized modes. The evaluator-owned holdout
covers TD/BT/LR/RL in ASCII. Independent semantic, raw-text, geometry, and
critic checks passed; the one-frame human-eye ledger records every changed cell.
The hypothesis is falsified by a perpendicular side marker, a broken or
reversed arrow, label/border overwrite, clipping, a non-portal geometry change,
or any P0/P1/P2 finding in a homolog or holdout.

## Promotion rule

Keep the candidate golden changes separate from this lesson: require a
hash-bound golden review and explicit approval before updating snapshots. When
portal projection, style composition, route-overlap resolution, critic
ownership, or visual packet schemas change, regenerate the Mermaid-schema
golden candidates and repeat the immutable-frame, tiny-cell human-eye review
before promotion.
