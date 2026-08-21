# Visual lesson: routed labels must protect route corners

Status: Promoted rule
Source epoch: h145c-label-fix-2026-08-21
Fixture family: `label_basic_rl`, `label_edge_long_rl`, `label_junction_lr`, `label_junction_rl`, `subgraph_labels_rl`

## Observation

In convergent RL diagrams, an edge label could replace a routed corner while
leaving the arrowhead visible. The result looked like a broken path: the
`label_junction_rl` `Other -> Target` route rendered as `←─label 2` instead of
preserving its turn. The same ownership mistake changed several basic,
long-label, and titled-subgraph homologs. This was a route-continuity defect,
not merely a spacing preference.

## Hypothesis and falsifier

Hypothesis: routed label placement was treating any non-arrow cell as writable.
Restricting labels to blank or horizontal edge-owned cells, and searching for
the nearest safe slot, should preserve turns, arrows, title cells, and other
semantic owners while retaining readable labels.

The hypothesis is falsified if any ASCII/Unicode, default/optimized, or
canonical/authored homolog overwrites a corner, arrow, border, title, or
semantic cell; loses a shaft or arrowhead; clips a label unexpectedly; or
changes unrelated geometry.

## Evidence

- Focused regression: `render_with_feedback_keeps_rl_convergent_labels_off_route_corners` in `tests/render_options_api/direction_matrix.rs`; the full direction-matrix and subgraph-boundary suites pass.
- Complete-corpus result: 241 inputs, 964 packet rows per policy lane, 952 renderable rows, and 12 separately validated expected-error rows.
- Human-eye result: 20 changed rows per lane were freshly inspected across both styles and modes and all passed; the complete current ledgers contain 856 passes, 96 retained watches, and zero failures. The 932 unchanged rows per lane were rebound only when fixture/style/mode, frame, evidence, and policy hashes matched exactly.
- Golden result: the explicit label intent approved 10 snapshots; `cargo test --features golden --test golden` passes.

## Promoted rule or next experiment

Label writers may replace only blank or horizontal edge-owned cells. They must
never erase route turns, arrows, vertical shafts, title cells, borders, or
other semantic owners. If no safe slot exists, the label is omitted rather
than damaging topology. Keep the remaining TD/BT title-rail and corridor
watches open; this rule does not authorize a global layout change.
