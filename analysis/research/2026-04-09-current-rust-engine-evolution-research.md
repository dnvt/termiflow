# Research Brief: Continued Evolution Of The Current Rust Engine

**Date:** 2026-04-09
**Prompted by:** `/maestro:research --deep` after the engine rewrite evaluation spike
**Sub-questions:**
- Which strengths in the current TermiFlow engine make continued evolution credible through beta?
- Which technical seams offer the highest leverage without opening an architecture rewrite?
- What evidence shows those seams are already tractable inside the current Rust codebase?
- What should remain explicitly out of scope unless rewrite triggers are actually met?

## Executive Summary

The current Rust engine should remain the execution path through beta.

The strongest evidence is not just that representative pressure fixtures audit
clean; it is that the codebase already has the right kind of seams for further
improvement without an architectural reset. The current engine is modular at the
places that matter: parser/graph/layout/render are separated, nested-subgraph
work is already landing as targeted layout and portal changes, preview transport
is isolated in retained-frame presenters, and the project has critic, provenance,
fixture, and benchmark infrastructure that can turn visual polish into measured
work.

The best continued-evolution strategy is a four-part path:
1. keep pushing compound layout exactness through explicit lane budgeting and
   portal-aware sizing
2. unify text measurement and wrapping around grapheme-safe behavior where it
   matters
3. tighten watch/TUI transport with capability-aware synchronized updates
4. expand measured-quality coverage so layout and render tradeoffs are judged by
   evidence instead of intuition

The evidence does not support opening a rewrite stream or integrating an
external runtime router before beta. The remaining hard problem is true nested
hierarchy polish, not a general collapse of the Rust-native engine.

## Findings

### Finding 1

**Source:** `planning/ENGINE_REWRITE_EVALUATION.md`; `planning/RENDERING_PRECISION_PROGRAM.md`; `analysis/research/2026-04-09-engine-rewrite-evaluation-spike.md`; `planning/PLAN.md`
**Relevance:** High
**Confidence:** High
**Detail:** The planning and spike evidence agree that the current engine still
has practical headroom. The rewrite-evaluation spike found clean audit results
on representative dense routing, sibling-subgraph, and Unicode-pressure
fixtures, while `planning/RENDERING_PRECISION_PROGRAM.md` already narrows the
active work to bounded precision improvements rather than broad architectural
churn. `planning/PLAN.md` likewise frames nested subgraphs as partial and still
being hardened, not as proof that layout and rendering generally need
replacement. That convergence matters: continued evolution is not a fallback
position anymore; it is the evidence-backed default.

### Finding 2

**Source:** `src/layout.rs`; `src/portals.rs`; `analysis/2026-04-08-nested-subgraph-containment.md`; `tests/render_options_api.rs`
**Relevance:** High
**Confidence:** High
**Detail:** The highest-leverage seam is still compound layout exactness, and it
is already evolving successfully inside the current model. `src/layout.rs`
contains explicit route-pressure widening helpers and lane-routing logic, while
`src/portals.rs` owns shared envelope and portal-slot calculation rather than
burying those concerns inside ad hoc drawing code. The nested-subgraph thinking
capture correctly identified that true containment is a model/layout problem
first, and the current test surface in `tests/render_options_api.rs` now locks
multiple formerly-broken nested cases such as clean title rows, child bottom
borders, and fan-in spine placement. This is the strongest argument against a
rewrite: the codebase is already absorbing the hard nested work through focused,
local evolution.

### Finding 3

**Source:** `src/measure.rs`; `src/style.rs`; `src/tui/presenter.rs`; `src/bin/common/mod.rs`; `analysis/research/2026-04-09-termiflow-rendering-precision-strategy-research.md`
**Relevance:** High
**Confidence:** High
**Detail:** Text and terminal exactness remain the best low-risk follow-up once
compound layout pressure is stabilized. `src/style.rs` centralizes display-width
measurement around `unicode-width`, but `src/measure.rs` still wraps and hard
truncates by `char`, which leaves an obvious local upgrade path for grapheme-safe
behavior without disturbing the full renderer. Preview transport is similarly
contained: `src/tui/presenter.rs` already uses retained frames and diff-based
ANSI presenters for both `--watch` and `--tui`, and `src/bin/common/mod.rs`
keeps those modes as explicit, opt-in entry points. The earlier rendering
precision research already argued that synchronized updates and grapheme-aware
measurement are the next low-product-risk wins, and the current source layout
backs that up.

### Finding 4

**Source:** `tests/visual_audit.rs`; `tests/render_options_api.rs`; `benches/rendering.rs`; `analysis/research/2026-04-09-termiflow-rendering-precision-strategy-research.md`
**Relevance:** High
**Confidence:** Medium-High
**Detail:** TermiFlow is positioned to evolve by measurement, not guesswork, but
that quality layer still needs expansion. `tests/visual_audit.rs` already runs a
curated clean-fixture suite through the critic, and `tests/render_options_api.rs`
contains targeted topology regressions for complex subgraph and fan-in/fan-out
cases. `benches/rendering.rs` proves the project already accepts Criterion as a
normal tool, but the current benchmark set is still small and generic compared
with the dense compound cases now driving precision work. The implication is
clear: continue evolving the current engine, but make the next increments more
oracle-like and less anecdotal.

### Finding 5

**Source:** `planning/ENGINE_REWRITE_EVALUATION.md`; `analysis/research/2026-04-09-engine-rewrite-evaluation-spike.md`; `analysis/research/2026-04-09-termiflow-rendering-precision-strategy-research.md`
**Relevance:** Medium-High
**Confidence:** High
**Detail:** The work that should stay out of scope is also now clear. A full
engine rewrite, external runtime routing, and broad TUI redesign remain poor
fits for the current beta wedge. The rewrite-evaluation plan already preserves
one partial candidate and one offline external-oracle candidate if triggers are
met later, which is the right level of optionality. Continued evolution only
stays disciplined if the project keeps using those explicit boundaries instead
of smuggling rewrite-grade ambition into ordinary implementation slices.

## Convergence And Divergence

### Convergence

- The rewrite spike, the rendering-precision research, and the active program
  plan all point to the same default strategy: keep the Rust-native engine and
  improve it surgically.
- Compound layout exactness is the highest-value algorithmic seam.
- Unicode and preview exactness are real but more localized problems than
  nested compound layout.
- The repo already has the beginnings of a measured-quality pipeline through
  critic, provenance, curated fixture audits, and Criterion benches.

### Divergence

- The exact text-width path is still open: improve `unicode-width` usage,
  introduce grapheme segmentation selectively, or move a narrower slice to a
  more grapheme-aware width helper.
- The minimum useful oracle layer is still unsettled. It could start with rank
  sanity and route expectations before graduating to richer geometry checks.
- Nested hierarchy is clearly the hardest remaining rendering problem, but the
  exact cutoff between “bounded model work” and “rewrite trigger” still depends
  on future evidence.

### Notable Gaps

- No explicit, reusable per-subgraph lane-budget model beyond the current
  widening heuristics
- No grapheme-safe wrapping/truncation path yet
- No synchronized update primitives in the presenters yet
- No route-dense benchmark or oracle layer sized to the nested compound cases
  now driving the program

## Recommendation

**Recommendation:** Decide

Adopt “continue evolving the current Rust engine through beta” as the durable
working decision, with the rendering-precision program as the execution vehicle.

The recommended next execution order is:
1. generalize the current nested-subgraph widening work into an explicit
   lane-budget model
2. unify text measurement and make wrapping/truncation grapheme-safe where
   user-visible drift is highest
3. add capability-aware synchronized update primitives to the watch/TUI
   presenters
4. expand route-dense benchmark and geometry-oracle coverage before revisiting
   rewrite questions

Revisit `planning/ENGINE_REWRITE_EVALUATION.md` only if those steps fail to move
the remaining nested compound cases enough, or if complexity rises faster than
quality.
