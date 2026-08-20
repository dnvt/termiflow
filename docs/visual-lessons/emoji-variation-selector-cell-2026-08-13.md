# Emoji variation selectors must share the base glyph cell

## Observation

The `unicode_emoji_{lr,rl,td,bt}` fixtures rendered `Process ⚙️` as
`Process ️`: the zero-width variation selector remained visible while the
gear base glyph disappeared. The machine critic, geometry trace, and route
clarity checks were clean because the defect was in label-cell serialization,
not graph topology.

## Hypothesis

The canvas stored one `char` per logical cell. Label drawing wrote the
variation selector into the same cell as the preceding glyph, replacing the
base character. Keeping a separate combining-mark stream per base cell should
preserve the complete grapheme without changing route geometry or terminal
cell budgeting.

## Repair and falsifier

`src/render/canvas.rs` now retains zero-width combining marks alongside their
base cell and serializes them after that base glyph. The focused falsifier was
that the base cell would still be replaced, or that a standalone variation
selector would remain in the output. The canvas unit regression and the
direction/style/optimization matrix in `tests/unicode_emoji_visual.rs` now
require `Process ⚙️` and reject `Process ️`.

## Promoted rule

Text rendering must preserve grapheme attachments independently from the
logical cell grid. A zero-width mark may not replace a visible base glyph, and
the full emoji homolog matrix must be re-reviewed after any canvas or label
placement change.

## Follow-up

Regenerate both canonical and authored visual packets after this repair, then
perform fresh human-eye review of every emoji direction/style/mode row and the
changed label homologs. Keep the broader corpus pass open because the same
machine-clean/human-noisy distinction can occur in subgraph portal routes.
