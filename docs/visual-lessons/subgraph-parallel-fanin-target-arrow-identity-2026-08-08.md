# Visual lesson: parallel subgraph fan-in target identity

Date: 2026-08-08
Source packet: H45/H46 durable sidecar packets
Review ledger: H45/H46 durable sidecar JSONL ledgers

## Corpus-wide review contract

All 237 schemas under `tests/fixtures/inputs` are in scope. The generator
expands each one into ASCII/Unicode × default/optimized rows: 948 total, of
which 936 are renderable and 12 are intentional expected-error cases. Every
row receives machine evidence, and every renderable row must eventually pass
through the one-frame AI human-eye review. The reviewer records small visual
defects—border hooks, route/contour ambiguity, spacing, text collisions, and
ownership confusion—as durable observations rather than treating a clean
machine critic as a golden approval.

Each observation feeds a self-improving loop: stable finding family →
hypothesis → falsifier → affected homologs → focused regression/golden test
→ narrow codebase repair → full packet regeneration → fresh visual review.
Schema identity, variant, frame/evidence hashes, observation, and policy are
kept together so a changed diagram cannot inherit stale approval. Rust and all
direct/transitive dependencies are also planned for an absolute-latest
modernization checkpoint, followed by the complete test, golden, packet, and
visual gates.

## Human-eye observation

The `subgraph_parallel_bt` fixture was reviewed fresh in all four variants:
ASCII/Unicode × default/optimized. Every frame has the correct boxes, labels,
subgraph boundary, route ownership, and expected arrow count. The machine
critic and route-clarity checks are clean.

The two `Path 1` and `Path 2` edges entering `End` nevertheless merge into a
single central shaft and one upward arrowhead. The external `Input → Start`
and `End → Output` routes use that same central portal column through the
subgraph boundary. The routes are traceable, but the target does not visually
mark both incoming edge identities; the repeated central alignment creates a
single-trunk reading. Unicode corner glyphs clarify the rail shape without
changing the identity ambiguity. All four variants are therefore P2 `watch`,
not `pass`.

## Hypothesis

The BT fan-in route-lowering and portal projection policies collapse multiple
target arrowheads at a shared junction and reuse one central boundary column.
Optimization and glyph style affect policy metadata and characters but do not
change the underlying target-entry geometry.

## Falsifier

A homolog that independently marks both incoming target entries, or separates
the external portal column from the internal fan-in shaft while preserving
ownership and containment, would falsify the shared-policy explanation. A
future fix that improves only one style or mode is insufficient.

## Required next experiment

Research the target-entry corridor and fan-in arrowhead policy, then plan a
bounded repair with explicit BT and mirrored-direction homologs. Acceptance
requires:

- independent human-readable incoming edge identity at `End`;
- no shared-junction ambiguity or route-through-box artifact;
- intact Process boundary, title, portals, and all six labels;
- clean critic, geometry, semantic-owner, and route-clarity evidence;
- fresh one-frame review of all four style/mode variants;
- complete 948-row packet regeneration after the source epoch.

This lesson is a hypothesis input, not a golden approval. The current full
corpus remains open until every changed and warning row has a perceptual
decision.

## H46 repair result

The bounded experiment selected only the exact flat BT scene and shared its
two-port capacity policy between measurement and rendering. The scene lowerer
uses the existing transactional identity router on a subgraph-free graph view,
but permits the two BT horizontal legs to use the same corridor row only after
the route-cell collision proof shows that their spans are disjoint. The route
lowerer then reapplies the authoritative corner glyphs after line projection;
this prevents a temporary corner-plus-vertical write from becoming a visually
false tee (`├`/`┤`).

Fresh H46 visual review of `subgraph_parallel_bt` shows two distinct `End`
arrowheads in ASCII/default, with the critic, geometry, route trace, labels,
and external `Input`/`Output` lanes clean. The exact unit and integration
oracles cover the target coordinates and all four ASCII/Unicode ×
default/optimized homologs. The full H46 packet is machine-clean, but the
corpus perceptual ledger is not yet closed; the remaining rows must continue
through the one-frame loop. The sibling-subgraph BT boundary-rail watch remains
an independent P2 hypothesis and is not treated as resolved by this repair.

## H46 perceptual ledger checkpoint

The canonical H46 ledger now contains 224 unique renderable-row decisions:
188 passes, 28 watches, and 8 P2 failures. The failures repeatedly expose
shared target-arrowhead identity loss in sibling-subgraph and parallel fan-in
layouts; watches cover boundary-rail/title hooks, nested portal ownership,
label-in-border ambiguity, direct TD portal/title ownership, database
route-clarity false positives, and small ASCII/Unicode contour noise. This
confirms that the visual reviewer is
exercising dense, warning, shape, text-width, and sibling-boundary cases
instead of closing the gate from machine-clean evidence alone. The queue
remains open for 712 renderable rows and the separate 12-row expected-error
policy ledger.

The latest queue expanded the same repair hypothesis beyond the original
parallel fan-in:

- `collision_sibling_triple_td` preserved all five heads, but two cross-group
  transitions produced doubled boundary rails and title-adjacent hooks;
- `subgraph_direct_td` preserved its single head, but the portal pierced the
  receiving group's top/title rows and required deliberate route tracing.

These are boundary-projection and ownership-policy evidence, not isolated
fixture failures. A future repair should reserve a portal lane that cannot
claim title/border cells, preserve explicit source/target ownership through
border restore, and then re-run the full corpus before promoting any golden.
