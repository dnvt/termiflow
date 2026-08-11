# Visual lesson: full-corpus subgraph boundaries and target entries

Date: 2026-08-08
Source packet: H37 sibling complete packet (private run artifact)
Review ledger: H37 perceptual ledger (private run artifact)
Expected-error ledger: H37 expected-error policy ledger (private run artifact)

## Corpus scope and validation

The visual self-improvement loop reviewed the complete `tests/fixtures/inputs`
corpus, not a hand-picked subset:

- 948 total fixture/style/mode rows;
- 936 renderable rows reviewed one frame at a time;
- 12 intentional parser-error rows validated in a separate expected-error ledger;
- ASCII and Unicode styles, each in default and optimized modes;
- visual review coverage: `936/936`;
- expected-error coverage: `12/12`;
- review decisions: 679 pass, 240 watch, 17 fail;
- severity inventory: 46 P1, 208 P2, 682 P3.

Packet evidence:

- complete packet SHA-256: `0ba974f26d4e322c397d5b5cc9c9cc7aec368fa66493c7139d747c93287dd126`;
- manifest SHA-256: `febf2ebd18f3734ff9db8665d3658d82dce97f785e1966d19ffbf718afbe071e`;
- packet checksum SHA-256: `6292550910cf7af3e08bb5a7bed5b14554054760feed8212367aa73b70d9b756`;
- packet digest SHA-256: `90e04a9b9b33e54501df4cf4282777bb59a70a93552c11476eafbadce86406b8`.

Machine-clean is not visual approval. The 240 watch rows remain explicit
human-eye observations, and the 17 fail rows remain open hypotheses rather than
being hidden by the prescreen or warning ledger.

## What the full corpus taught us

The new sibling-subgraph fan-in allocator is clean in its four direct complex
fan-in direction cases. The complete corpus found adjacent defects outside that
narrow topology:

1. Nested subgraphs reuse one portal rail across ancestor depths. TD and BT
   frames show doubled `||` seams, `++`/plus-like junctions, broken contour
   corners, and title-adjacent route elbows. LR/RL default frames can look
   clean, so the fix must be checked across all directions, styles, and modes.

2. Complex subgraphs reuse boundary corridors for unrelated transitions. TD and
   BT show long aligned service/data rails and dense target-entry turns. RL also
   produces a concrete Response/Order Service rectangle overlap plus route-cell
   ownership violations. LR is less severe but still has long parallel
   cross-subgraph routes whose transition identity is weak.

3. Parallel and stacked sibling transitions remain visually ambiguous. The
   `collision_parallel_edges_bt`, `collision_sibling_triple_bt`, and
   `subgraph_chain_bt` families preserve arrow counts but repeatedly render a
   shared central border seam. This is a P2 identity problem even when the
   geometry oracle reports no hard error.

4. Cycle and intentional-warning families are generally healthy. Their
   external gutter routes keep back-edges separate from the forward path. The
   malformed and empty parser fixtures also preserve their expected stderr
   contracts; those rows must stay outside the visual-pass denominator.

## Falsifiable next-loop hypotheses

### H38: depth-aware boundary ownership

Hypothesis: boundary rasterization and route compositing are depth-blind. Each
ancestor subgraph needs its own portal lane and ownership record, with border
continuity preserved at every non-portal cell and route turns prohibited from
using title/corner cells.

Falsifier: if distinct depth-specific portal lanes still render as doubled bars,
shared plus junctions, or broken corners, the remaining defect is in the final
cell compositor's glyph precedence rather than layout allocation.

Acceptance evidence:

- a nested TD/BT golden oracle reports one portal owner per depth;
- no doubled border glyphs or plus-like boundary collisions in ASCII;
- Unicode preserves the same ownership and contour semantics;
- LR/RL homologs remain unchanged or improve;
- fresh review of all nested homologs plus the complete 948-row packet.

### H39: protected complex-subgraph target corridors

Hypothesis: complex-subgraph target placement and target-entry allocation do not
reserve existing node rectangles and source-to-data corridors transactionally.
Response must be placed outside the enclosing service/data envelopes, and every
incoming target edge must reserve a distinct ownership-valid corridor before
fallback routes or border restoration run.

Falsifier: if Response no longer overlaps but route claims still land on
source-owned cells, the route transaction/claim model—not target placement—is
the primary defect.

Acceptance evidence:

- TD/BT/LR/RL complex fixture rectangles are pairwise non-overlapping;
- every edge has an ownership-valid trace with no fallback claim violation;
- both Response entries are visually distinct and remain outside the final
  subgraph border;
- all four directions pass ASCII/Unicode × default/optimized focused tests;
- complete packet regeneration and one-frame review follow every source epoch.

### H40: transition-specific sibling portals

Hypothesis: multiple sibling transitions are allocated to one median lane even
when they are different edge groups. The allocator must preserve transition
identity through boundary projection and rasterization, not merely preserve the
number of arrowheads.

Falsifier: if raw geometry has distinct lanes but the frame still reads as one
trunk, repair the boundary compositor and visual identity policy instead.

Acceptance evidence:

- `collision_parallel_edges_bt`, `collision_sibling_triple_bt`, and
  `subgraph_chain_bt` expose distinct portal ownership;
- no shared-trunk watch finding remains for the repaired family;
- all edge identities remain traceable without adding misleading labels;
- complete corpus review confirms no regressions in unrelated fixtures.

## Loop contract

Every renderer change must produce a fresh packet and source/run identity,
re-run machine geometry and ownership checks, review changed and warning rows
visually one frame at a time, validate the 12 expected errors separately, and
then review the full 948-row corpus again. A golden test is accepted only when
the raw oracle, machine prescreen, and perceptual review agree; a clean
prescreen alone is insufficient.
