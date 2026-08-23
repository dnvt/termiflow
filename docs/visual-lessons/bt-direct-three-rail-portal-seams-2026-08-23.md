# Direct three-rail BT portals need a directional seam, not a shared trunk

Status: Promoted focused rule; title-boundary clearance remains a Watch
Source epoch: H152 direct three-rail BT sibling-seam experiment, 2026-08-23
Fixture family: `collision_parallel_edges_bt`, with crossed and multi-subgraph controls

## Observation

The direct BT fixture contains two titled sibling groups and three independent
source-to-target edges. The rails, arrows, and endpoint lanes were already
distinct, but the boundary cells could look like a single vertical trunk where
plain route glyphs crossed the horizontal group borders. In the default mixed
style this appeared as a light `│` over a heavy `━`; in a uniform ASCII lane,
`+` is the only available explicit border seam.

## Hypothesis and falsifier

The graph itself can prove this narrow topology: exactly two flat titled sibling
subgraphs, three nodes per group, three unlabeled direct Arrow edges, unique
one-to-one endpoints, no cycles or back edges, and no crossed or extra edges.
Using that selector only in final portal projection and critic acceptance should
render three directional seams at both boundaries without changing layout,
lane pairing, titles, arrows, or unrelated diagrams.

The hypothesis is falsified if the selector admits a crossed or broader scene,
merges rails, changes endpoint lanes, detaches an arrow, overwrites a title or
node border, creates a new machine finding, or worsens any matched direction,
style, mode, or authored-policy homolog.

## Evidence

- Graph-owned selector: `Graph::bt_direct_parallel_sibling_scene` in
  `src/graph.rs`.
- Scene projection and acceptance: `src/render/edge/bt_sibling_scene.rs`,
  `src/render/portal_projection.rs`, and `src/render/critic.rs`.
- Independent raw-frame oracle coverage: `tests/independent_oracles.rs`.
- Focused visual coverage: `tests/bt_parallel_sibling_visual.rs` and
  `tests/subgraph_boundary_arrows.rs`.
- Final H152 packets cover all 241 inputs, 964 rows, both policy lanes, all
  four directions, both styles, and both render modes; packet validation has
  zero findings.
- Fresh perceptual review covers 952/952 renderable rows in each lane and
  expected-error policy covers 12/12 in each lane. The target scene remains a
  P2 watch only for title-adjacent clearance; the three crossings are explicit.
- Golden check is current. Rust formatting, 586-test full matrix, clippy,
  package verification, cargo-audit, and cargo-deny all pass.

## Promoted rule or next experiment

Keep the exact three-rail seam rule and the ASCII/Unicode seam vocabulary in
the independent oracle. The next experiment should own title-boundary
clearance and portal hooks separately, with matched controls for parallel
identity, labels, junctions, density, fan-in, and cycles. Every follow-up must
regenerate both complete policy lanes and perform a fresh one-frame review; a
machine-clean frame is not a human-eye sign-off.
