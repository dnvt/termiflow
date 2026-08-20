# TD parallel sibling lanes must clear title portal hooks

Status: promoted focused rule; complete-corpus perceptual closure and
unrelated boundary watches remain open.

## Observation

In `collision_parallel_edges_td` and `collision_parallel_cross_td`, the
connected TD routes could still turn beside the target subgraph title. The
frame contained a short bracket-like portal shoulder even though every
arrowhead, shaft, and crossing identity was present. A human reader could
mistake the shoulder for a title hook or a shared border junction.

## Hypothesis and falsifier

The smallest owner is the layout envelope stage: a strict flat pair of titled
sibling subgraphs with one-to-one parallel edges should share title-safe target
lanes before the route lowerer draws portals. The bounded policy translates the
already aligned source/target pairs, preserves the original title left edge,
and fails closed for mixed, labeled, nested, crowded, or non-aligned scenes.

The hypothesis is falsified if a crossed pair changes edge identity, an
arrowhead or shaft disappears, title clearance is lost, or a BT/LR/RL or
unrelated TD homolog worsens.

## Evidence

- Focused regression: `render_td_parallel_siblings_keep_target_lanes_clear_of_title_hooks` in `tests/render_options_api/direction_matrix.rs`.
- Focused gates: the `subgraph_boundary_arrows` suite, 77 render-option tests,
  33 independent oracles, and 37 layout tests pass across the source epoch.
- Complete-corpus result: both H106 packets validate at 241 inputs and 964
  rows per policy lane: 952 renderable rows and 12 expected-error rows.
- Human-eye result: the changed TD parallel rows pass in both canonical and
  authored lanes; the separate BT title-hook watch remains open, and the full
  952-row perceptual drainage is not yet complete.
- Golden result: no golden approval is implied.

## Promoted rule

When a strict TD/TB titled sibling scene is already pairwise aligned, allocate
the target lanes from the live title-safe portal policy before rendering. Keep
the translation topology-gated and fail closed rather than exporting it to
crossings, fan-in, nested, or horizontal scenes without fresh visual evidence.
