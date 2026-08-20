# Visual lesson: BT parallel turns need visible shaft clearance

Status: Promoted focused rule; full-corpus closure and adjacent seam watches
remain open

## Observation

In `collision_parallel_edges_bt`, the leftmost source-to-target detour could
place its turn corners one cell apart. Unicode rendered that as `┌─┘`/`└─┐`
and ASCII as `+-+`; the route was connected and machine-clean, but a human
eye could read the mark as a damaged subgraph border rather than an intentional
edge. The critic's zero score did not detect the ambiguity.

The source epoch covers the current 241-input corpus: 964 rows per policy
lane, 952 renderable frames, and 12 separately governed expected errors. The
four changed BT parallel rows (ASCII/Unicode × default/optimized) were freshly
inspected in both the canonical requested-style and authored/no-override lanes:
8/8 focused perceptual decisions pass. The route-clarity warning remains an
explicit human-review queue signal, not a machine approval.

## Hypothesis and bounded fix

The topology-owned BT parallel lane allocator must keep each source and target
turn at least three cells from its node center when the scene has room. That
leaves two visible shaft cells between corners and preserves three distinct
boundary lanes without changing edge ownership or falling back to generic
routing.

The focused regression in
`tests/bt_parallel_sibling_visual.rs` rejects adjacent and one-cell corner
compositions in both glyph styles and both render modes. The implementation is
scoped to `src/render/edge/bt_parallel_sibling.rs`; no LR, RL, or TD behavior
is inferred from this BT rule.

## Falsifiers and next loop

The rule is falsified by a merged rail, detached arrow, title collision,
unreadable portal ownership, a new fallback route, or a homolog that regresses
under ASCII/Unicode, default/optimized, direction, or authored-policy review.
The next review must inspect the sibling-triple BT/TD/LR/RL seams, then drain
the complete current matrix in both lanes before approving goldens or a release.

## Public boundary

This lesson records visible observations, code ownership, tests, and
falsifiers only. Private Maestro capsules, prompts, provider traces, and
transient packet paths remain outside the OSS repository.
