# Research Brief: TermiFlow Rendering Precision Strategy

**Date:** 2026-04-09
**Prompted by:** `/maestro:research --deep` on overall TermiFlow strategy, rendering exactness, visualization accuracy, terminal fluidity, and adjacent Rust/CLI/TUI/layout domains
**Sub-questions:**
- Which current precision limits come from TermiFlow's own geometry and routing model, and which come from terminal-display reality?
- What proven layout and routing patterns from Mermaid, Graphviz, ELK, and related systems are most relevant to TermiFlow's compound-subgraph and orthogonal-routing problems?
- Which Rust and terminal primitives could improve Unicode correctness, redraw fluidity, and visual stability with low product risk?
- Where should TermiFlow tighten scope or documentation instead of adding more algorithmic complexity?

## Executive Summary

TermiFlow does not need a wholesale engine rewrite to get materially more exact. The strongest opportunities are narrower and more surgical:

1. split compound-node containment from edge-routing budget more explicitly, especially for nested subgraphs and dense fan-in/fan-out
2. improve text measurement and wrapping at the grapheme level rather than the codepoint level
3. use terminal frame primitives more deliberately for smoother redraws
4. strengthen geometry and oracle-style testing so "precision" is measured, not inferred

The evidence does not support pulling in a heavyweight external router or doing a full TUI rewrite before beta. Mature engines still have limitations around orthogonal routing with ports and labels, and terminal Unicode width remains inherently approximate across emulators. The pragmatic path is to keep the Rust-native core, sharpen the model boundaries, and be more explicit about what "exact" means in a terminal renderer.

## Findings

### Finding 1

**Source:** local code in `src/layout.rs`, `src/portals.rs`, `src/render/edge.rs`, `context/current-task.md`, `planning/NESTED_SUBGRAPH_SUPPORT.md`; Graphviz `dot` and `compound` docs; ELK hierarchy handling docs; Mermaid flowchart docs on subgraphs and ELK  
**Relevance:** High  
**Confidence:** High  
**Source quality:** High for local code and official docs; medium for cross-system inference  
**Detail:** TermiFlow's hardest visible defects cluster around one seam: subgraph containment and route demand are still partially negotiated through heuristics instead of a fully explicit compound-layout contract. The current Phase 3 work already points in this direction: `widen_subgraph_for_outgoing_route_pressure` in `src/layout.rs` and ancestor-aware envelopes in `src/portals.rs` are compensating for the fact that child containers are still largely sized from content extents plus minimal clearance. Official prior art reinforces this separation. Graphviz `dot` is explicitly a layered engine for directed graphs and exposes `compound`, `lhead`, and `ltail` for cluster-boundary clipping rather than treating cluster crossing as a rendering afterthought. ELK documents hierarchy handling as a distinct control point and notes that including hierarchy in one layout run helps cross-hierarchical edges, while Graphviz-dot-style flattening discards child padding information. Mermaid likewise documents subgraph semantics and now exposes ELK as the better renderer for larger or more complex diagrams. The implication for TermiFlow is not "switch to ELK"; it is that compound containment, portal allocation, and route lane demand should be first-class layout inputs, not something render absorbs late.

### Finding 2

**Source:** Graphviz `splines` docs; ELK Libavoid integration blog; local code in `src/render/edge.rs`, `src/render/mod.rs`, `src/portals.rs`  
**Relevance:** High  
**Confidence:** High  
**Source quality:** High  
**Detail:** External orthogonal routers are not a silver bullet. Graphviz documents that `splines=ortho` uses axis-aligned segments, but "the routing does not handle ports or, in dot, edge labels." ELK's own write-up on adopting Libavoid describes the integration as substantial enough to warrant an out-of-process server, and it carries LGPL obligations. That matters because TermiFlow's hardest cases are exactly port-like border crossings plus label placement. The repo already reflects this: portal slots, title rows, and label collision handling are core concerns in `src/portals.rs` and `src/render/mod.rs`. A Rust-native hybrid remains the strongest fit for beta: preserve the current layered layout, make portal and lane budgeting more explicit, and keep edge aesthetics in render. Reaching for a foreign router now would add integration, licensing, packaging, and reproducibility costs while still leaving label and border semantics on TermiFlow's side.

### Finding 3

**Source:** local code in `src/style.rs`, `src/measure.rs`, `src/tui/frame.rs`, `docs/reference.md`; `unicode-width` crate docs; `unicode-display-width` crate docs; `unicode-segmentation` docs  
**Relevance:** High  
**Confidence:** High  
**Source quality:** High  
**Detail:** Unicode exactness is partly fixable and partly impossible. Today, TermiFlow measures width with `unicode-width` through `UnicodeWidthStr` in `src/style.rs`, but wrapping and hard truncation in `src/measure.rs` still iterate by `char`, not grapheme cluster. That means display width and wrap boundaries can diverge on emoji sequences, combining marks, and other multi-codepoint graphemes. `unicode-display-width` explicitly handles grapheme clusters and describes its width model in grapheme terms, while `unicode-segmentation` provides standard grapheme iteration. This is a real upgrade opportunity. At the same time, the same `unicode-display-width` docs are explicit that notional Unicode width does not match real rendering in many terminals, shells, or editors, and ambiguous-width characters remain environment-sensitive. TermiFlow should therefore improve its internal text model, but avoid promising universal Unicode fidelity. The honest product stance remains: grapheme-safe measurement plus an ASCII portability mode, not "perfect Unicode everywhere."

### Finding 4

**Source:** local code in `src/tui/presenter.rs`, `src/tui/frame.rs`, `src/bin/common/mod.rs`; crossterm terminal docs; Ratatui "Rendering under the hood" docs; Ratatui inline viewport example  
**Relevance:** Medium-High  
**Confidence:** High  
**Source quality:** High  
**Detail:** Preview fluidity has low-risk room to improve without a TUI redesign. TermiFlow already keeps a retained `TerminalFrame` and computes per-cell diffs for `AnsiDiffPresenter` and `InlinePresenter`. That is directionally aligned with Ratatui's model, where widgets render into an intermediate buffer and the terminal flush happens after the frame is built. Crossterm exposes `BeginSynchronizedUpdate` and `EndSynchronizedUpdate`, plus alternate-screen and raw-mode primitives. TermiFlow's presenters currently flush raw diffs directly, but do not wrap redraws in synchronized frame boundaries. For watch mode and fullscreen TUI mode, that is a cheap next step with clear upside: less tearing and fewer half-painted frames on emulators that support it. Ratatui's inline viewport example also shows a mature pattern for inline terminal surfaces that does not require taking over the whole screen. The evidence does not argue for replacing the current presenters with Ratatui wholesale. It argues for borrowing a few primitives and patterns: synchronized updates, stronger resize handling, and a clearer boundary between "frame model" and "terminal transport."

### Finding 5

**Source:** local code in `src/render/critic.rs`, `src/render/provenance.rs`, `benches/rendering.rs`, `analysis/reviews/2026-04-01-deep-audit.md`; D2 troubleshooting guide; Graphviz plain text output docs  
**Relevance:** High  
**Confidence:** Medium-High  
**Source quality:** High for repo state and official docs; medium for operational inference  
**Detail:** TermiFlow is already unusually well-positioned to improve exactness through measurement rather than by intuition. The `CriticReport`, semantic frame, provenance ownership, and bounded repair loop give the project the beginnings of a scored rendering pipeline. The missing piece is better oracles and benchmark coverage. The current Criterion bench covers small and moderate diagrams, but there is still no scale/performance regression harness for route-dense cases. Graphviz's plain text output exposes control points and label positions, which could be used as an offline oracle for rank sanity and cluster-crossing expectations in selected fixtures. D2's troubleshooting guidance also matches the current nested-subgraph work: when shapes are highly connected, extra dimensions give connections more surface area to route aesthetically. That lines up with the route-pressure widening now under way in `src/layout.rs`. TermiFlow should lean harder into "measure geometry quality and compare alternatives" rather than continuing to stack special-case drawing heuristics without a scoring harness.

### Finding 6

**Source:** `planning/AUDIT-termiflow-limits.md`, `planning/spec/SPEC.md`, `docs/reference.md`, local source in `src/spacing.rs`, `src/config.rs`, `src/render/mod.rs`, `src/lib.rs`  
**Relevance:** Medium  
**Confidence:** High  
**Source quality:** High  
**Detail:** Some apparent precision constraints are documentation drift, not current engine limits. `planning/AUDIT-termiflow-limits.md` and parts of `planning/spec/SPEC.md` still describe a `500×200` canvas cap and hardcoded 12-character edge labels, but current source has `10000×5000` canvas bounds in `src/spacing.rs` and configurable edge label width flowing through `Config` and `RenderOptions`. This matters strategically because it means some of the perceived ceiling on TermiFlow is already lower in docs than in code. Before adding complexity, the project should keep internal limits docs current so research and planning do not optimize against stale constraints.

## Convergence And Divergence

### Convergence

- Mature layout systems treat hierarchy, ranking, and routing as related but distinct concerns.
- Orthogonal routing remains hard even in established engines, especially once ports, edge labels, and compound boundaries are involved.
- Terminal text width is a policy approximation, not a ground truth. Conservative portability modes remain necessary.
- More "surface area" for highly connected nodes or subgraphs is a practical way to reduce routing ugliness. TermiFlow's current route-pressure widening work is pointed in the right direction.
- Buffered frame rendering is the right mental model for smooth terminal previews. TermiFlow's presenters already align with that approach.

### Divergence

- External engines like ELK or Libavoid offer stronger generality, but their integration cost conflicts with TermiFlow's narrow Rust-native beta wedge.
- Ratatui could absorb more of the TUI transport layer, but TermiFlow's custom presenters are already solving a narrower problem. A full migration would cost more than it returns right now.
- Unicode-measurement crates can improve correctness materially, but no crate can make emulator-specific rendering perfectly deterministic.

### What Surprised Me

- The repo is already closer to a "feedback-guided renderer" than the docs imply. Critic, repair, semantic ownership, and layout retry are meaningful infrastructure, not side experiments.
- Several internal "limits" documents are stale enough to mislead strategy work.
- Graphviz still documents real limitations for orthogonal routing with ports and labels. That lowers the value of chasing a generic external router as an immediate answer.

### Notable Gaps

- No large-scale geometry or performance oracle suite for route-dense and Unicode-heavy cases
- No grapheme-safe wrapping and truncation path yet
- No explicit per-side or per-subgraph lane-budget model beyond current widening heuristics
- No terminal capability detection or graceful use of synchronized updates in presenters

## Recommendation

**Recommendation:** Plan

TermiFlow should pursue a focused "rendering precision program" with three tracks, and it should not broaden scope beyond that before beta:

1. **Layout/model exactness**
   - Make compound containment and route-lane budgeting explicit in layout.
   - Extend the current nested-subgraph Phase 3 work into a reusable per-subgraph port/lane budget instead of accumulating more render-time compensations.
   - Keep external router integration out of the beta path.

2. **Text and terminal exactness**
   - Make wrapping and truncation grapheme-aware with `unicode-segmentation` plus either a carefully updated `unicode-width` usage or a targeted move to `unicode-display-width`.
   - Add explicit documentation that Unicode width remains approximate across emulators and preserve ASCII as the portability baseline.
   - Add synchronized terminal updates to `AnsiDiffPresenter` and `InlinePresenter` where supported.

3. **Measured quality**
   - Expand benchmark and regression coverage for dense fan-in/fan-out, nested containers, Unicode-heavy labels, and watch/TUI redraws.
   - Use critic/provenance data as a scored acceptance layer for candidate layouts.
   - Add a small oracle track using external references only for verification, not runtime dependency: Graphviz plain output for rank/edge sanity and hand-curated fixture expectations for compound borders and labels.

### Suggested Follow-up

- `/maestro:plan` a "Rendering Precision Program" with separate slices for compound-lane budgeting, grapheme-safe text measurement, and synchronized redraws.
- `/maestro:decide` only if you want a durable record that external router integration is out of scope before beta.
- `/maestro:run` directly if you want to start with the lowest-risk slice first: grapheme-safe wrapping/truncation plus synchronized presenter updates.
