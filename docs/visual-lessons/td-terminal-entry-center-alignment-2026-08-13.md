# TD terminal entries need distinct target-center lanes

Status: promoted focused rule; complete-corpus closure and inherited boundary
watches remain open.

## Observation

In `tests/fixtures/inputs/collision_edge_corner_td.md`, two external boxes
entered different nodes in one titled TD subgraph. The routes were connected,
but a one-column source/target parity mismatch made the outside turn cells
touch: Unicode rendered `└┐` and ASCII rendered `++`. The critic and route
clarity checks were clean, so this was a human-eye micro-artifact rather than
a connectivity failure.

The current review denominator is 241 inputs, 964 rows per policy lane, 952
renderable frames, and 12 separately governed expected-error rows.

## Hypothesis and bounded fix

For a flat, titled, one-to-one TD/TB terminal-entry scene, each external source
should share the center lane of its distinct internal target. Aligning those
sources removes the adjacent corner glyphs while preserving each target's
identity. The layout stage now stages every move, rejects fan-in/fan-out,
nested, labeled, and crowded scenes, and commits only a complete collision-free
proposal set within a small displacement budget.

## Falsifiers and regression

The focused layout regression is
`td_terminal_entry_sources_align_to_distinct_target_centers` in
`src/layout/tests.rs`. The render regression
`direct_td_terminal_entries_use_target_center_portals_without_hooks` in
`tests/subgraph_boundary_arrows.rs` covers ASCII and Unicode, default and
optimized modes, and rejects both the ASCII `++` and Unicode `└┐` artifacts
alongside the existing compact-hook checks.

The rule is falsified by a source or target overlap, a displaced unrelated
node, a lost arrow shaft/arrowhead, merged target ownership, or a new artifact
in any negative-control topology. A fresh canonical and authored packet is
required before closing the visual watch or changing a golden.

## Public rule

Use a target-owned lane for strict one-to-one vertical terminal entries when
the move is small and provably safe. Keep ambiguous collector and nested
routes on their existing policies until their own topology-specific visual
evidence justifies a new rule.
