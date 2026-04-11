# Initiative: Rendering Precision Program

**Status:** Complete
**Roadmap Slot:** Active Workstreams
**Owner:** Maintainer
**Timeline:** 2026-04-09 -> 2026-04-10
**Aligned To:** Pre-1.0 polish and first public beta rendering credibility
**Decision Links:** `analysis/research/2026-04-09-termiflow-rendering-precision-strategy-research.md`; `analysis/research/2026-04-09-current-rust-engine-evolution-research.md`; nested-subgraph rationale in `analysis/2026-04-08-nested-subgraph-containment.md`

## Objective

Increase TermiFlow's rendering precision enough that dense flowcharts, nested
containers, Unicode-heavy labels, and live preview modes all behave more
predictably without requiring a wholesale engine rewrite before beta.

## Scope

**In:** compound-layout exactness, explicit route-lane budgeting, grapheme-safe
text measurement where precision matters, preview/TUI width consistency,
smoother redraw primitives, geometry-oracle coverage, benchmark expansion, and
truth updates for internal limits and precision claims.

**Out:** a full engine rewrite before beta, runtime adoption of external
orthogonal routers, broad TUI product redesign, new diagram types, and
Mermaid-parity work unrelated to precision.

## Why This Exists

The 2026-04-09 research brief narrowed the strategy cleanly: TermiFlow's most
important precision gaps come from model boundaries and measurement discipline,
not from missing a generic layout engine. The current nested-subgraph Phase 3
slice is already proving the point. The renderer improves when layout reserves
space for route demand early instead of relying on late repair and drawing
heuristics to absorb the pressure.

This initiative turns that diagnosis into an execution program with bounded
scope and measurable outcomes.

## Success Criteria

- [x] Route-dense nested containers reserve enough width and border-exit budget
      that representative `TD`, `LR`, `BT`, and `RL` fixtures no longer rely on
      render-time compensation for obvious crowding cases.
- [x] Label wrapping, truncation, preview frames, and status rows use one
      consistent terminal-cell measurement model, with explicit coverage for
      emoji, combining marks, and CJK examples.
- [x] `--watch` and `--tui` redraws use synchronized update primitives when
      supported and degrade cleanly when not supported.
- [x] Benchmark and regression coverage expand to include route-dense,
      nested-container, and Unicode-heavy cases, with at least one reusable
      geometry-oracle layer for ranked or routed output sanity.
- [x] Internal docs and public docs agree on current precision boundaries,
      current limits, and what remains approximate across terminal emulators.

## Tracks

### 1. Layout / Model Exactness

- Complete the current Phase 3 nested-subgraph work in
  `planning/NESTED_SUBGRAPH_SUPPORT.md`.
- Generalize route-pressure widening into an explicit per-subgraph lane-budget
  model rather than a one-off widening heuristic.
- Keep compound containment, portal allocation, and border-exit ownership
  first-class in layout and provenance instead of pushing pressure into render.

### 2. Text / Terminal Exactness

- Unify terminal-cell width measurement across render, preview, frame diffing,
  viewport cropping, and status rows.
- Make wrapping and truncation grapheme-safe where user-visible drift matters.
- Add synchronized terminal updates and capability-aware fallbacks in preview
  presenters.

### 3. Measured Quality

- Expand fixture coverage for dense fan-in/fan-out, nested groups, and
  Unicode-heavy labels.
- Add a geometry-oracle layer for at least selected cases so layout quality is
  compared against explicit expectations instead of only visual intuition.
- Extend Criterion or equivalent coverage for route-dense and redraw-heavy
  cases so precision improvements do not hide cost regressions.

## Dependencies

- `planning/NESTED_SUBGRAPH_SUPPORT.md` remains the first active child project.
- The active worktree needs to stay stable enough that fixture, benchmark, and
  presenter work can be reviewed independently from OSS-hardening changes.
- OSS hardening remains active in parallel, so this program must preserve the
  narrow beta wedge rather than widening the product surface.

## Risks & Mitigations

- Scope creep turns a precision initiative into a general renderer rewrite
  -> keep explicit non-goals and route rewrite questions through the engine
     evaluation plan instead of smuggling them into execution.
- Grapheme-aware measurement changes introduce new regressions in existing
  fixtures
  -> add Unicode-specific tests before broad fixture regeneration and keep
     ASCII as the portability baseline.
- Terminal capability differences make redraw improvements inconsistent across
  emulators
  -> detect capabilities where possible, keep graceful fallbacks, and document
     approximation honestly.
- Better scoring infrastructure still overfits a narrow fixture set
  -> combine hand-curated regression fixtures, oracle checks, and benchmark
     cases instead of relying on one signal.

## Open Questions

- Should TermiFlow stay on `unicode-width` everywhere, or introduce
  `unicode-segmentation` plus a more grapheme-aware width helper for selected
  paths?
- How much of the main canvas needs grapheme awareness, versus the lower-risk
  path of fixing measurement and preview first?
- What is the minimum useful oracle layer: rank sanity, routed bend-count
  sanity, or cluster-crossing expectations?
- Which preview improvements belong in beta, and which should remain post-beta
  UX polish?

## Evidence To Gather

- Before/after renders for dense nested fixtures in all four directions.
- Unicode-heavy preview and wrapping cases that currently drift between render
  and preview width calculations.
- Redraw traces or visual confirmation that synchronized updates reduce tearing
  in supported terminals.
- Benchmark data for dense layouts and repeated live-preview redraws.
- Documentation diffs that eliminate stale precision or limit claims.

## Experiment Log

**Hypothesis:** explicit lane budgeting plus unified text measurement and
terminal redraw discipline will produce visibly better precision without the
cost and risk of a pre-beta engine rewrite.

**Intervention:** execute the program in three tracks: layout/model exactness,
text/terminal exactness, and measured quality.

**Expected Observation:** nested containers, dense edge fans, and Unicode-heavy
labels render more predictably, preview modes stop drifting from render width,
and benchmark coverage shows acceptable cost for those gains.

**Actual Observation:** The first execution slices are now materially in place.
Declared nested children participate in pre-envelope route budgeting and
side-biased re-placement instead of relying only on visually nested overlap
heuristics; TD/TB declared parents keep a visible title/border band above child
title rows; and nested border restoration no longer overwrites node-owned cells
in representative TD/BT cases. In parallel, preview/TUI paths now share
display-column measurement for frame construction, viewport clamping, inline
diffing, and presenter cursor motion, and redraw batches are wrapped in
synchronized update markers. Targeted layout, render-feedback, and TUI tests
cover those slices and are green in the current worktree. The horizontal
display-consistency child slice is now also closed: `LR`/`RL` visually nested
sibling subgraphs preserve semantic separation from parent-only and downstream
nodes, and their side-entry seams render with the dedicated portal marker
instead of junction corruption. The subgraph-crossing contract is now explicit
as well: borders are portal boundaries, not merge targets, and every used
border crossing now resolves to the dedicated pierce marker instead of
borrowing tee/cross/line glyphs from route topology.
The measured-quality track is now stronger too: the curated visual-audit suite
explicitly includes the route-dense `subgraph_complex_{lr,rl}` fixtures and the
LR/RL sibling-subgraph collision pair, Criterion now includes a
`route_dense_subgraphs` group built from the complex subgraph family across
orientations, and the critic has an explicit oracle for the side-wall contract
so junction-like LR/RL border merges are rejected while dedicated portal
markers remain accepted. The final text/terminal exactness slice is now
complete too:
node-label measurement, hard wrapping, preview/status wrapping, and edge-label
truncation now share grapheme-safe display-column helpers instead of mixing
byte, char, and width-based loops, and the curated audit now explicitly covers
wrapped labels plus Unicode-heavy emoji and CJK fixtures. Public and internal
docs were then refreshed to record the bounded remaining approximation
honestly: width budgeting is unified, but the main scene canvas is still
char-backed, so multi-codepoint grapheme composition remains a deeper
architecture follow-up instead of a hidden bug. Fresh closeout verification
also put the work on a stronger measured footing: `cargo llvm-cov
--summary-only` now reports `84.82%` region coverage and `84.00%` line
coverage across the crate, while the `route_dense_subgraphs` bench family
stayed effectively flat.

**Conclusion:** The initiative hypothesis held and the bounded beta target is
met. The highest-value precision gains came from better layout/model budgeting,
unified terminal measurement, synchronized presenter updates, and broader
quality instrumentation, not from a pre-beta engine rewrite. The remaining
deeper opportunity is architectural rather than corrective: if future evidence
shows the char-backed canvas or current router model is still a product-level
limit, take that work through
`planning/CELL_SCENE_GRAPH_HYBRID_ORTH_ROUTER.md` instead of reopening this
umbrella piecemeal.

## Sequencing

1. Finish the active nested-subgraph route-budgeting slice and use it as the
   first reference implementation for explicit lane budgeting.
2. Unify cell-width measurement across preview paths and introduce
   grapheme-safe wrapping/truncation where needed.
3. Add synchronized presenter updates and terminal-capability fallbacks.
4. Expand geometry-oracle and benchmark coverage.
5. Update precision and limits docs after the implementation surface is true.

## Relationship To Engine Rewrite Evaluation

This initiative assumes the current engine remains the execution path through
beta. If this program fails to produce acceptable quality or complexity stays
too high after the first two tracks, escalate to
`planning/ENGINE_REWRITE_EVALUATION.md` instead of widening scope ad hoc.

## Recommendation

Close this umbrella as complete. Keep the prepared cell-scene-graph / hybrid
orth-router plan in backlog as the next architectural step only if later beta
evidence shows the current char-backed canvas or router model is still too
limiting.
