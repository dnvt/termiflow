# Project: Cell Scene Graph + Hybrid Orth Router

**Status:** Complete
**Roadmap Slot:** Completed Architecture Slice
**Owner:** Maintainer
**Timeline:** 2026-04-10 -> 2026-05-15
**Aligned To:** `planning/RENDERING_PRECISION_PROGRAM.md`
**Decision Links:** `analysis/research/2026-04-10-termiflow-ascii-precision-architecture-research.md`; `analysis/research/2026-04-09-termiflow-rendering-precision-strategy-research.md`; `planning/ENGINE_REWRITE_EVALUATION.md`

## Objective

Define and pilot a cell-first rendering architecture where hard compound-layout
and orthogonal-routing cases are solved through explicit semantic layers,
selective visibility-graph routing, and deterministic glyph resolution instead
of growing more ad hoc border and overlap heuristics.

## Scope

**In:** a render-layer contract, a cell-scene graph centered on ownership and
connectivity, a selective hybrid orthogonal router for hard routes, a unified
display-width profile for user-visible text math, and an oracle/trace surface
for geometry validation.

**Out:** a pre-beta full engine rewrite, runtime adoption of ELK or Graphviz,
replacing every existing routing path at once, a broad TUI redesign, or new
diagram types.

## Why This Exists

The current renderer is already halfway to this architecture, but the center of
gravity is still wrong. `Canvas`, provenance, the semantic frame, and the
critic already capture meaning beyond the final glyph, while layout is already
adding portal and lane heuristics to reserve space before render.

The next leap in precision is not more one-off glyph repairs. It is making the
semantic cell model and hard-route topology explicit so the final ASCII or
Unicode frame becomes a projection of richer geometry rather than the thing the
engine keeps reverse-engineering after the fact.

## Current State

- `src/layout.rs` and `src/portals.rs` already reserve some route pressure and
  containment budget, but they do not yet model border bands, portal lanes, or
  sibling-child keepouts as first-class entities.
- `src/render/semantic.rs`, `src/render/provenance.rs`, and
  `src/render/critic.rs` already form the beginnings of a scene graph, but the
  final glyph is still too often treated as the primary state.
- `src/display_profile.rs` now centralizes grapheme segmentation and
  display-width policy, and `style`, `measure`, `render`, `tui`, `scaling`,
  and CLI preview paths now consume that shared contract.
- `src/render/contract.rs` now codifies the current render-layer contract in
  code-facing types, and `src/render/trace.rs` provides a normalized geometry
  trace surface for dense compound-routing fixture families.
- The remaining deeper limitation is still explicit: the final render canvas is
  char-backed, but the selective orth-router pilot has landed on
  `subgraph_complex_{lr,rl}` and is now verified with geometry traces,
  critic/audit coverage, and refreshed goldens.

## Architecture Shape

### 1. Compound Lane Graph

- Represent each subgraph with explicit border bands and per-side portal lanes,
  not only derived outer rectangles.
- Distinguish declared child containment from visually nested sibling overlap.
- Add first-class keepouts for parent-only nodes, trunks, titles, and lane
  corridors so overlap is deliberate instead of incidental.

### 2. Cell Scene Graph

- Promote semantic ownership and role metadata into the canonical render model.
- Separate these layers:
  - layout reservation
  - route topology
  - semantic cells
  - glyph resolution
  - critic/repair
  - terminal transport
- Make final characters a projection of semantic state rather than the mutable
  state that later passes must reinterpret.

### 3. Hybrid Orth Router

- Keep existing deterministic cheap routing for simple cases.
- Add a selective orthogonal visibility-graph router for hard cases:
  - cross-subgraph routes
  - dense sibling crossings
  - critic-flagged repair candidates
- Add a final nudging phase so shared lanes separate cleanly and routes center
  in available alleys before glyph resolution.

### 4. Display Profile + Oracle Layer

- Define one display profile for grapheme segmentation and terminal-notional
  width policy.
- Reuse it across measurement, wrapping, truncation, preview, diffing, and
  cursor math.
- Add normalized geometry traces and at least one offline oracle path using
  Graphviz `-Tplain` and selected ELK comparisons for targeted fixture
  families.

## Success Criteria

- [x] A documented render-layer contract exists and is reflected in code-facing
      types or module boundaries for reservation, topology, semantic cells,
      glyph resolution, and transport.
- [x] At least one hard-route fixture family uses the selective orth-router
      pilot and renders without the current seam/corridor artifacts while
      preserving graph semantics.
- [x] Semantic cells can carry enough ownership and connectivity information
      that topology repair no longer depends on re-inferring hard cases from
      final glyphs alone.
- [x] Label wrapping, truncation, preview slicing, and cursor/diff math use the
      same display profile for grapheme and width handling.
- [x] A geometry trace or oracle harness exists for at least one dense
      compound-routing fixture family and can distinguish intended geometry
      improvements from incidental snapshot churn.
- [x] Criterion or equivalent benchmark coverage includes at least one
      route-dense compound case touched by the pilot.

## Dependencies

- The completed horizontal-parity and frame-geometry slices now provide the
  stable acceptance baseline this project depends on.
- The existing rendering-precision worktree needs to stay reviewable enough
  that the architecture work can land in thin slices rather than one invasive
  rewrite.
- External engines remain offline references only. Runtime precision work must
  stay Rust-native through beta.

## Risks & Mitigations

- Architecture churn without measurable quality gain
  -> require a pilot fixture family, critic deltas, and before/after geometry
     traces before widening the migration surface.
- The selective router grows into a shadow rewrite
  -> constrain it to hard routes first and keep cheap paths on the current
     deterministic router until the pilot proves value.
- Cell layers become an abstract model with no landing zone in code
  -> tie each layer to concrete modules and migration slices instead of writing
     a purely conceptual architecture note.
- Display-profile changes break existing fixtures broadly
  -> land measurement unification with focused Unicode-heavy tests before broad
     snapshot regeneration.
- Offline oracle comparisons cause more confusion than clarity
  -> normalize oracle use to narrow questions such as rank order, crossing
     count, bend count, containment, and label-distance sanity.

## Open Questions

- Should the selective orth-router become the default for all cross-subgraph
  edges, or remain a critic-triggered / hard-case-only path through beta?
- What is the narrowest useful scene-graph type surface:
  extend `Canvas` metadata, or introduce a separate route/cell structure before
  glyph projection?
- Should the unified display profile stay on `unicode-width` plus grapheme
  segmentation, or move selected paths to `unicode-display-width`?
- Which fixture family should serve as the pilot:
  `subgraph_complex_{lr,rl}`, a new dense cross-subgraph corridor case, or a
  more isolated route-only harness?
- Where should normalized geometry traces live:
  alongside goldens, under `tests/fixtures/oracles/`, or as serialized
  debug-only artifacts?

## Evidence To Gather

- A mapping from current modules to the proposed layer boundaries, including
  where ownership, connectivity, and z-order should live after migration.
- One pilot fixture family with current critic findings, route shape, seam
  artifacts, and benchmark timing captured before any architectural change.
- Width/segmentation examples that currently diverge between `measure`,
  `style`, and TUI frame math.
- An offline Graphviz `-Tplain` or ELK comparison for the pilot family that is
  strong enough to validate geometry shape but narrow enough to avoid false
  authority.

## Experiment Log

**Hypothesis:** a cell-scene graph plus selective orthogonal visibility-graph
routing will improve hard ASCII precision cases more reliably than adding more
heuristic border and overlap repairs to the current glyph-first pipeline.

**Intervention:** implement the architecture in thin slices:
layer contract first, display profile second, selective router pilot third,
oracle/benchmark layer fourth.

**Expected Observation:** the pilot fixtures preserve semantics, lose the
current seam/corridor artifacts, and become easier to reason about in semantic
and critic terms without unacceptable performance cost.

**Actual Observation:** The enabling slice and the pilot both landed. The
display-profile contract is explicit in `src/display_profile.rs` and is shared
by measurement, wrapping, truncation, scaling, preview slicing, and cursor
math instead of staying implicit under style helpers. The render-layer
contract is codified in `src/render/contract.rs` with concrete module
ownership for reservation, topology, semantic cells, glyph projection, and
terminal transport. `src/render/trace.rs` and `tests/geometry_trace.rs` now
exercise the dense `subgraph_complex_{lr,rl}` family without depending on
final glyph snapshots. The selective LR/RL cross-subgraph fan-in pilot now
routes the hard `D1/D2 -> Response` family through explicit precomputed lanes
in `src/layout.rs`, widens the visual nesting alley in `src/portals.rs`,
stamps both semantic and visually pierced side walls as `PortalOpening` cells
in `src/render/mod.rs` / `src/render/provenance.rs`, and keeps the critic
strict enough to reject junction-like LR/RL wall merges in
`src/render/critic.rs`. The curated audits are clean, the direct API tests are
clean, and the full expected fixture set was refreshed from the current
`termiflow` binary so the goldens now reflect the verified renderer.

**Conclusion:** This child architecture plan is complete. The current engine
now has a real cell-oriented landing zone, a selective hard-route pilot, and a
route/portal semantic contract strong enough for the targeted compound cases
that justified this work. Keep the selective router selective-only through
beta; widen it only if later evidence shows another hard-route family that the
deterministic path cannot cover cleanly.

## Sequencing

1. Finish the current horizontal parity semantics repair so the architecture
   work starts from a correct acceptance target.
2. Write the code-facing layer contract:
   reservation, topology, semantic cells, glyph resolution, transport.
3. Define the unified display profile and land it in the shared measurement
   paths without changing routing yet.
4. Build the selective orth-router pilot on one hard fixture family.
5. Add nudging, geometry traces, and benchmark coverage for that pilot.
6. Decide whether to expand the router surface or keep it selective through
   beta.

## Immediate Follow-up

1. Return to `planning/PRE_OSS_COORDINATION.md` and
   `planning/OPEN_SOURCE_HARDENING.md`; the architecture pilot no longer blocks
   the release-facing workstreams.
2. Reopen this architecture thread only if a later route family needs the same
   selective-lane treatment or if the char-backed canvas becomes the limiting
   factor instead of the routing contract.
