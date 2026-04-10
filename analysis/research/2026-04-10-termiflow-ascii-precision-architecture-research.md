# Research Brief: TermiFlow ASCII Precision Architecture

**Date:** 2026-04-10
**Prompted by:** `/maestro:research --deep` on deeper algorithms and concepts for ASCII/Mermaid rendering precision, dynamic placement, organic cell structure, multiple render layers, and boundary-aware boxes/edges
**Research question:** Which layout, routing, cell-model, and text-measurement architectures would most improve TermiFlow's ASCII rendering flexibility and precision without forcing a pre-beta engine rewrite?
**Sub-questions:**
- How should compound subgraphs and border crossings be represented in layout so nested and sibling containers stay semantically correct?
- What routing model best improves boundary-aware orthogonal edges, shared lanes, and final glyph placement on a terminal grid?
- What should an "organic cell structure" mean in this codebase: final glyphs only, or a richer per-cell semantic model with layers?
- How should TermiFlow handle grapheme segmentation and terminal width policy so wrapping, truncation, preview, and cursor math stay consistent?
- What measurement/oracle strategy would let the project prove precision gains instead of relying on visual intuition alone?

## Executive Summary

The most promising path is a cell-first, hybrid layered renderer:

1. keep the current layered layout core, but formalize compound subgraphs with explicit border nodes, border bands, portal lanes, and keepout constraints
2. add a selective orthogonal visibility-graph router for the hard cases instead of replacing all routing with a foreign engine
3. promote the existing semantic/provenance frame into a real multi-layer render model, where the final ASCII glyph is a projection of richer cell state
4. define a single display-width policy based on grapheme segmentation, with terminal-aware width tailoring rather than blind reliance on East Asian Width
5. build an offline oracle harness from Graphviz and ELK outputs plus local critic metrics, so geometry quality can be scored and compared

The strongest external evidence supports this direction. Sander's compound-directed-graph layout work, ELK's hierarchy handling, and Graphviz's compound edge model all treat containment and cross-boundary routing as first-class layout concerns. Wybrow's orthogonal connector routing work shows that visibility-graph routing plus nudging is the right mental model for predictable orthogonal edges. Unicode's own guidance warns that East Asian Width is not a complete terminal-width solution, while Ratatui and Crossterm reinforce the value of explicit buffers, diffs, and synchronized updates.

Inference from those sources: TermiFlow should stop treating the canvas glyph as the primary state. The primary state should be a semantic cell scene graph with occupancy, ownership, connectivity, and draw priority. ASCII or Unicode glyphs should be resolved only after that model is stable.

## Findings

### Finding 1: Compound subgraphs want global layering with explicit border nodes, not recursive "large node" layout

**Source:** local code in `src/layout.rs`, `src/portals.rs`, `src/render/edge.rs`; Georg Sander, *Layout of Compound Directed Graphs* (1996), https://publikationen.sulb.uni-saarland.de/bitstream/20.500.11880/25862/1/tr-A03-96.pdf; ELK Layered, https://eclipse.dev/elk/reference/algorithms/org-eclipse-elk-layered.html; ELK hierarchy handling, https://eclipse.dev/elk/reference/options/org-eclipse-elk-hierarchyHandling.html; Graphviz `compound`, https://graphviz.org/docs/attrs/compound/
**Relevance:** High
**Confidence:** High
**Source quality:** High. Peer-reviewed/report-level prior art plus official engine docs plus direct repo evidence.
**Detail:** Sander's core move is still the right one for TermiFlow: keep a global layer assignment, but insert explicit border nodes and nesting constraints so subgraph rectangles are part of ranking, dummy-node assignment, and edge anchoring. ELK makes the same architectural point from a modern implementation angle: compound graphs and cross-hierarchy edges work best when hierarchy is included in one layout run, not flattened into separate local runs. Graphviz's `compound=true` with `lhead` and `ltail` is another expression of the same principle: cluster crossings are not a late render decoration, they are a layout-level edge contract.

TermiFlow is already drifting toward this model. `compute_envelopes`, portal slots, and route-pressure widening in `src/layout.rs` / `src/portals.rs` are layout-level compensation for missing first-class border/lane entities. The new nested fixes are working because they reserve space earlier.

Inference for TermiFlow:
- add explicit per-subgraph border bands and side/top/bottom portal lanes to the layout model
- treat parent borders as layout participants, not just rectangles derived from content extents
- replace "visual nesting" promotion with explicit keepout contracts between sibling envelopes and declared children
- track subgraph lane budgets separately for entry, exit, pass-through, and title clearance

### Finding 2: The strongest routing model is orthogonal visibility graph plus final nudging, applied selectively

**Source:** Michael Wybrow et al., *Orthogonal Connector Routing*, https://users.monash.edu/~mwybrow/papers/wybrow-gd-2009.pdf; Graphviz and Dynagraph paper, https://graphviz.org/documentation/EGKNW03.pdf; Graphviz `splines` docs, https://graphviz.org/doc/info/attrs.html
**Relevance:** High
**Confidence:** High
**Source quality:** High. Academic paper plus official Graphviz documentation.
**Detail:** Wybrow's paper is unusually close to TermiFlow's actual hard cases. It models objects as rectangles, builds an orthogonal visibility graph from interesting x/y coordinates, uses A* over `(point, direction)` state to optimize length plus bends, and then performs a third "nudging" phase to separate shared segments and center routes in alleys. That is exactly the missing distinction in TermiFlow today: route topology and final glyph placement are still partially interleaved.

Graphviz's own docs reinforce the limit of simplistic orthogonal routing. `splines=ortho` is explicit that routing still does not properly handle ports or, in `dot`, edge labels. That warning matters because TermiFlow's difficult cases are portal crossings and labels near constrained boundaries. This is not evidence against orthogonal routing. It is evidence that the routing state must include ports, ownership, and label pressure.

Inference for TermiFlow:
- build a selective orthogonal visibility graph from node rectangles, subgraph border bands, and portal anchors
- use it only for the hard cases: cross-subgraph routes, dense sibling crossings, and critic-flagged repairs
- keep cheap deterministic routing for simple same-rank/same-column cases
- add a distinct post-route nudging stage that orders shared lanes, applies minimum separation, and centers routes in open corridors before glyph resolution

### Finding 3: Straightness, balancing, and shared-port merging should be explicit placement objectives, not incidental outcomes

**Source:** ELK layered docs, https://eclipse.dev/elk/reference/algorithms/org-eclipse-elk-layered.html; ELK `favorStraightEdges`, https://eclipse.dev/elk/reference/options/org-eclipse-elk-layered-nodePlacement-favorStraightEdges.html; ELK `mergeHierarchyEdges`, https://eclipse.dev/elk/reference/options/org-eclipse-elk-layered-mergeHierarchyEdges.html; Graphviz `group`, https://graphviz.org/doc/info/attrs.html
**Relevance:** High
**Confidence:** High
**Source quality:** High. Official engine/reference docs.
**Detail:** ELK exposes useful precision levers that map directly to TermiFlow's current ad hoc heuristics: favor straight edges, merge hierarchy-crossing edges where appropriate, and distinguish placement/balancing strategy from routing style. Graphviz's `group` attribute expresses a similar preference, keeping edges straighter and reducing crossings for grouped nodes. These are not niche options; they are evidence that good layered layouts need declared objective priorities.

TermiFlow currently approximates these objectives through widening, shifting, and repair. That works, but the objective function is implicit and dispersed.

Inference for TermiFlow:
- define explicit placement profiles such as `favor_straight`, `favor_balance`, and `favor_compact`
- expose per-subgraph or per-fixture "shared port merge" behavior as a layout/routing policy, not a render side effect
- move same-source fan-out, same-target fan-in, and cross-hierarchy port sharing into one scoring function so critics and repairs can target a common objective

### Finding 4: Dynamic placement should be local and critic-driven, not full-canvas re-layout

**Source:** Graphviz and Dynagraph paper, https://graphviz.org/documentation/EGKNW03.pdf; local code in `src/render/critic.rs`, `src/render/repair.rs`, `src/layout.rs`
**Relevance:** High
**Confidence:** Medium-High
**Source quality:** High for Graphviz/Dynagraph and local code; medium for transfer into TermiFlow's exact architecture.
**Detail:** Dynagraph's incremental layered layout is important because it shows how to preserve mental continuity: update ranks and routes only near changed neighborhoods, then rerun local crossing reduction and coordinate optimization with penalties for unnecessary movement. TermiFlow already has the seeds of this model. The critic gives spatially local findings, the repair loop is bounded, and layout already has retry/correction seams.

Inference for TermiFlow:
- promote critic findings into local "damage regions" or "precision neighborhoods"
- relayout only the affected corridor, subgraph ring, or rank neighborhood when a finding is local
- preserve stable coordinates elsewhere unless a global pass is explicitly requested
- use this both for watch/TUI edit responsiveness and for bounded automatic repair

This is likely the right meaning of "dynamic placement" for this project: not physics, but small, deterministic, locality-preserving relayouts.

### Finding 5: Text-cell precision needs a declared grapheme policy and a separate width policy

**Source:** local code in `src/style.rs`, `src/measure.rs`, `src/tui/frame.rs`; Unicode UAX #29, https://www.unicode.org/reports/tr29/; Unicode UAX #11, https://www.unicode.org/reports/tr11/; `unicode-segmentation` docs, https://docs.rs/unicode-segmentation/latest/unicode_segmentation/; `unicode-width` docs, https://docs.rs/unicode-width/latest/unicode_width/; `unicode-display-width` docs, https://docs.rs/unicode-display-width/latest/unicode_display_width/
**Relevance:** High
**Confidence:** High
**Source quality:** High. Unicode normative annexes plus current crate docs plus direct repo evidence.
**Detail:** The repo currently splits text handling in a risky way:
- preview/frame code in `src/tui/frame.rs` is already grapheme-aware
- width calculation in `src/style.rs` uses `unicode-width`
- wrapping and truncation in `src/measure.rs` still iterate by `char`

Unicode UAX #29 is clear that grapheme clusters are the right default unit for "user-perceived characters." UAX #11 is equally clear that East Asian Width is useful but not an off-the-shelf terminal solution; modern terminal emulators require tailoring. `unicode-display-width` makes the same point from the Rust side: grapheme-aware notional width is better than codepoint counting, but real editors and terminals still diverge.

Inference for TermiFlow:
- define a `DisplayProfile` with two distinct choices:
  - segmentation policy: extended grapheme clusters
  - width policy: terminal-notional width with explicit tailoring rules
- use the same profile in label measurement, truncation, wrapping, viewport math, status rows, diffing, and cursor movement
- stop slicing labels on `char` boundaries anywhere user-visible text can be truncated or wrapped
- keep ASCII as a first-class portability mode, but document Unicode as profile-based rather than "exact"

### Finding 6: TermiFlow should formalize a multi-layer cell scene graph

**Source:** local code in `src/render/semantic.rs`, `src/render/provenance.rs`, `src/render/critic.rs`, `src/render/topology.rs`, `src/render/canvas.rs`; Ratatui rendering docs, https://ratatui.rs/concepts/rendering/ and https://ratatui.rs/concepts/rendering/under-the-hood/
**Relevance:** High
**Confidence:** High
**Source quality:** High for repo evidence and official TUI docs; the resulting architecture is an explicit inference.
**Detail:** The repo already contains the beginning of a cell scene graph:
- `Canvas` stores char plus metadata and z-index
- `SemanticFrame` snapshots cell ownership and role
- `refresh_provenance` stamps node, border, title, portal, and edge ownership
- the critic reasons over semantic cells, not raw strings

Ratatui's buffer model is the clean external precedent: the cell buffer is the render truth, and terminal bytes are a later flush. That is the key conceptual step TermiFlow should finish.

Inference for TermiFlow:
- define render layers explicitly, for example:
  - layout reservation layer: keepouts, border bands, lane corridors, text bounds
  - route topology layer: portal anchors, polylines, bend intents, shared-lane ordering
  - semantic cell layer: owner kind, owner id, role, connectivity, z-index
  - glyph resolution layer: final ASCII/Unicode characters after overlap policy
  - critic/repair layer: findings, candidate mutations, accepted edits
  - transport layer: cropped frame, viewport slice, diff segments, synchronized terminal batch
- make the final glyph a derived artifact, not the mutable source of truth
- store connectivity per cell or per segment explicitly so topology repair stops inferring too much from final characters

This is the strongest fit for the user's "organic cell structure" idea. The cells become living geometry records with semantic identity, not dead pixels.

### Finding 7: Quality should be scored with offline oracles and geometry traces, not goldens alone

**Source:** Graphviz `dot` guide plain output, https://graphviz.org/pdf/dotguide.pdf; Mermaid layout docs, https://mermaid.js.org/intro/syntax-reference and https://mermaid.js.org/syntax/flowchart.html; local code in `src/render/critic.rs`, `tests/render_options_api.rs`, `tests/visual_audit.rs`, `benches/rendering.rs`
**Relevance:** High
**Confidence:** High
**Source quality:** High.
**Detail:** Graphviz's `-Tplain` output exposes node coordinates, sizes, and edge control points in a parseable text format. Mermaid exposes ELK configuration for node placement and edge merging. TermiFlow already has critic findings, semantic ownership, golden fixtures, and Criterion benches. These ingredients are enough to build a stronger oracle layer without adding any runtime dependency.

Inference for TermiFlow:
- keep visual goldens, but add geometry assertions:
  - rank ordering
  - border containment
  - portal crossing count
  - bend count bounds
  - shared-lane spacing
  - label-to-node and label-to-edge distances
- use Graphviz `-Tplain` and selected ELK outputs only as offline reference points for specific fixture families
- store normalized geometry traces for critical fixtures so improvements are comparable even when final glyph art shifts slightly
- extend benchmarks to include route-dense compound cases and watch/TUI redraw churn

### Finding 8: Full runtime adoption of an external router is still the wrong near-term move

**Source:** Graphviz `splines=ortho` docs, https://graphviz.org/doc/info/attrs.html; ELK layered docs, https://eclipse.dev/elk/reference/algorithms/org-eclipse-elk-layered.html; prior local research in `analysis/research/2026-04-09-termiflow-rendering-precision-strategy-research.md`
**Relevance:** Medium-High
**Confidence:** High
**Source quality:** High.
**Detail:** The counterargument remains important. ELK is excellent, but it is a general layout engine with a different runtime and product boundary. Graphviz's orthogonal routing still has documented limitations with ports and edge labels. External engines would help as comparison tools and offline oracles, but they do not remove TermiFlow's need to own terminal cell semantics, ASCII glyph resolution, portal ownership, and label behavior.

Inference for TermiFlow:
- use external engines for comparison and truth pressure
- do not put them on the runtime path before beta
- direct complexity budget into a better internal cell/routing model instead

## Convergence And Divergence

### Convergence

- Compound layout quality improves when borders and cross-boundary anchors participate in ranking and placement, not just rendering.
- Orthogonal routing quality depends on two separate phases: route topology search and final lane nudging.
- Text precision must be grapheme-aware and width-policy-aware; codepoint iteration is not enough.
- A cell buffer with ownership and connectivity is the right internal abstraction for terminal rendering and repair.
- Locality-preserving relayout is a stronger strategy for interactive precision work than repeated global redraw heuristics.

### Divergence

- General layout engines support richer graph classes, but their runtime integration cost is still disproportionate for TermiFlow's beta wedge.
- ELK's hierarchy handling prefers integrated compound layout, while TermiFlow still has some inherited flattening/visual-promotion behavior in horizontal cases.
- Grapheme-aware width crates improve notional correctness, but no crate can force identical rendering across terminals.
- A full visibility graph for every edge may be too expensive or too complex; the strongest evidence supports selective application for hard routes.

### What Surprised Me

- The repo is closer to a semantic renderer than the current architecture documents admit. `SemanticFrame`, provenance stamping, topology checks, and the critic already form the beginning of a scene graph.
- Sander's 1996 compound-directed-graph work maps unusually well onto the exact subgraph problems TermiFlow is facing now.
- Wybrow's separation between route search and final nudging is a very strong conceptual fit for ASCII rendering, where the last one or two cells often determine whether the output looks "correct" or broken.

### Notable Gaps

- No explicit border-node or portal-lane entities in the layout model yet
- No selective orthogonal visibility-graph router for critic-flagged hard cases
- No single display-width profile shared across label measurement, wrapping, truncation, preview, and presenter math
- No normalized geometry trace/oracle harness beyond current goldens and critic verdicts
- No documented render-layer architecture, even though the code is already halfway there

## Recommendation

**Recommendation:** Plan

Run a focused architecture plan with four concrete tracks:

1. **Compound lane graph**
   - add border bands, portal lanes, and sibling-child keepouts as first-class layout entities
   - refactor horizontal parity work around explicit semantics instead of visual-promotion heuristics

2. **Hybrid orth router**
   - introduce an orthogonal visibility-graph router for hard routes only
   - add a final nudging phase for shared lanes and corridor-centering before glyph resolution

3. **Organic cell structure**
   - promote `SemanticFrame` plus provenance into the canonical render scene graph
   - formalize render layers and make final glyphs a projection of richer cell state

4. **Text and oracle discipline**
   - define one grapheme-aware display profile for all user-visible width math
   - add offline geometry oracles from Graphviz `-Tplain` and selected ELK comparisons
   - expand benchmarks around compound routing and watch/TUI redraws

## Suggested Follow-up

- `/maestro:plan` a child architecture plan for "cell scene graph + hybrid orth router"
- `/maestro:decide` whether the visibility-graph router should be selective-only or become the default for all cross-subgraph edges
- `/maestro:run` the lowest-risk enabling slice first: unify grapheme measurement and formalize the semantic cell layers
