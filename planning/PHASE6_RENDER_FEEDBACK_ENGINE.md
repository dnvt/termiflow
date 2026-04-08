# Phase 6 Plan: Render Feedback Engine and Terminal Diff Renderer

> Status: Proposed
> Authoring context: created on `main` at commit `3bd28c4` after verifying `HEAD == origin/main`
> Purpose: design a "draw -> inspect -> repair -> redraw" system for TermiFlow, plus a future frame-diff terminal presenter inspired by modern agentic terminal UIs

---

## 1. Executive Summary

TermiFlow already has a strong deterministic pipeline:

```text
Mermaid text -> Parser -> Graph -> Measure -> Layout -> Render -> String
```

The next quality step is not "use an AI to look at ASCII". The correct next step is to make the renderer capable of understanding its own output structurally, scoring defects, and applying bounded local repairs before producing final output.

This phase introduces two related but distinct capabilities:

1. **Render Feedback Engine**
   - Render into a rich internal framebuffer, not chars alone.
   - Preserve provenance for every cell.
   - Run a structural critic on the rendered result.
   - Generate repair actions.
   - Re-run layout and rendering until the score converges or the budget is exhausted.

2. **Terminal Diff Renderer**
   - Maintain a previous frame and a next frame.
   - Diff them at the cell level.
   - Emit only the minimum ANSI updates needed to redraw the terminal.
   - Use this for future live preview and TUI modes.

The first capability improves diagram quality.
The second improves the UX of displaying evolving diagrams.

They are complementary but should be implemented independently.

---

## 2. Motivation

### 2.1 Current State

TermiFlow produces good deterministic diagrams, but quality control is mostly encoded indirectly in layout and render heuristics. When a diagram looks wrong, the system usually cannot answer:

- Which exact cells are wrong?
- Which object owns those cells?
- What local constraint was violated?
- What minimal change would improve the result?

Today, correctness is enforced by:

- unit tests
- golden fixtures
- manual visual inspection
- heuristic logic spread across layout and render modules

This is enough for a static renderer, but it is not enough for self-improving output.

### 2.2 Target State

We want a system that can say:

- "this arrowhead landed on a subgraph border"
- "this fan-out junction has insufficient shaft length"
- "this LR chain is horizontally cramped"
- "this edge label is too close to a corner"
- "this portal opening was carved but unused"
- "this sibling subgraph pair visually reads as nested"

And then act on that information automatically.

### 2.3 Why This Is the Right Model

The guiding idea is similar to how advanced terminal UIs now think about rendering:

- keep an internal model
- produce a whole frame
- diff it against prior state
- update only what changed

For TermiFlow, we extend that idea one level deeper:

- keep a semantic model of the diagram
- produce a semantic frame
- compare it against desired structural constraints
- apply local repairs
- re-render until acceptable

This is the practical, deterministic version of "the diagram becomes aware of how it looks".

---

## 3. Goals

### 3.1 Primary Goals

- Improve final ASCII/Unicode diagram quality without introducing nondeterminism.
- Make visual defects explainable with machine-readable findings.
- Keep the core renderer deterministic and testable.
- Support bounded multi-pass repair without exploding complexity.
- Prepare the codebase for live preview and future TUI rendering without full-screen flicker.

### 3.2 Secondary Goals

- Provide developer diagnostics for why a fixture changed.
- Reduce hand-coded one-off heuristics by replacing them with a critic + repair loop.
- Improve LR/RL aspect ratio handling through measurable penalties instead of ad hoc tuning.
- Provide a stable foundation for future interactive diagram editing or viewport navigation.

### 3.3 Non-Goals

- Do not build a model-driven visual reasoning loop in this phase.
- Do not introduce OCR, screenshots, or image-model inspection.
- Do not attempt mathematically perfect global layout optimization.
- Do not block existing `--print` usage on expensive optimization by default.
- Do not rewrite the parser/layout/render stack from scratch.

---

## 4. Design Principles

1. **Deterministic first**
   - Same input, same config, same optimization budget should produce the same output.

2. **Structure over pixels**
   - Every rendered cell should know what semantic object produced it.

3. **Local repairs before global search**
   - Fix the smallest thing that improves the score.

4. **Bounded search**
   - No unbounded iterative refinement.

5. **Explainability**
   - Every repair should cite the findings that motivated it.

6. **Preserve current architecture**
   - Build around parser, measure, layout, render instead of replacing them.

7. **Opt-in rollout**
   - Keep current output path as baseline until each step is validated.

---

## 5. Current Architecture Fit

The existing codebase already contains the right seam lines for this work:

- `src/lib.rs`
  - top-level orchestration
- `src/graph.rs`
  - graph data model
- `src/measure.rs`
  - label and node geometry
- `src/layout.rs`
  - node placement and partial route generation
- `src/render/mod.rs`
  - rendering orchestration
- `src/render/canvas.rs`
  - 2D drawing surface
- `src/render/edge.rs`
  - edge routing and rendering
- `src/orientation.rs`
  - TD/LR/BT/RL abstraction
- `src/portals.rs`
  - subgraph portal logic

The biggest gap is not missing rendering functionality. The gap is missing **semantic persistence** between layout and final character output.

Today, much of the meaning is lost after glyph selection.

---

## 6. Proposed High-Level Architecture

### 6.1 New Pipeline

```text
Parse
  -> Measure
  -> Candidate Layout
  -> Semantic Render Frame
  -> Critic
  -> Repair Planner
  -> Candidate Mutation
  -> Re-render
  -> Best Frame Selection
  -> Char Projection
  -> Optional Terminal Frame Diff
```

### 6.2 New Subsystems

1. **Semantic Frame**
   - Rich framebuffer with provenance and topology metadata per cell.

2. **Critic**
   - Converts a semantic frame into findings and a score.

3. **Repair Planner**
   - Maps findings to local repair actions.

4. **Repair Loop**
   - Applies actions, re-renders, compares candidate scores.

5. **Terminal Presenter**
   - Diffs current and prior frame for low-flicker redraw.

---

## 7. Core New Concepts

### 7.1 Semantic Cell Ownership

Every canvas cell should store:

- visible glyph
- style family
- owner kind
- owner id
- semantic role
- z-layer
- collision metadata

Example owner kinds:

- `NodeBorder`
- `NodeFill`
- `NodeLabel`
- `EdgeSegment`
- `ArrowHead`
- `Junction`
- `SubgraphBorder`
- `SubgraphTitle`
- `CycleEdge`
- `PortalOpening`

Example semantic roles:

- `TopBorder`
- `BottomBorder`
- `LeftBorder`
- `RightBorder`
- `HorizontalEdge`
- `VerticalEdge`
- `Turn`
- `MergeJunction`
- `SplitJunction`
- `EdgeLabel`
- `Background`

### 7.2 Critic Finding

Each finding should include:

- unique code
- severity
- numeric penalty
- affected cells
- owning objects
- explanation
- suggested repair classes

Examples:

- `ARROW_ON_BORDER`
- `UNUSED_PORTAL_CARVE`
- `JUNCTION_TOPOLOGY_MISMATCH`
- `EDGE_LABEL_NEAR_CORNER`
- `SUBGRAPH_VISUAL_FALSE_NESTING`
- `LR_CHAIN_CRAMPED`
- `EDGE_SHAFT_TOO_SHORT`
- `CYCLE_GUTTER_CLIPPED`

### 7.3 Repair Action

Repairs should be explicit and serializable:

- shift node
- expand rank gap
- adjust stem length
- move label anchor
- move portal slot
- reroute edge
- widen subgraph gutter
- expand canvas budget
- reserve obstacle halo around text

Each repair should include:

- target object id
- parameters
- expected impact
- reversibility
- cost

---

## 8. Proposed Rust Data Model

The exact names may change, but the first design should look close to this:

```rust
pub enum CellOwnerKind {
    Empty,
    NodeBorder,
    NodeFill,
    NodeLabel,
    EdgeSegment,
    ArrowHead,
    Junction,
    SubgraphBorder,
    SubgraphTitle,
    CycleEdge,
    PortalOpening,
}

pub enum CellRole {
    Background,
    Horizontal,
    Vertical,
    Corner,
    SplitJunction,
    MergeJunction,
    Cross,
    LabelText,
    BoxInterior,
    BorderTop,
    BorderBottom,
    BorderLeft,
    BorderRight,
    ArrowTip,
    PortalGap,
}

pub struct CellMeta {
    pub ch: char,
    pub owner_kind: CellOwnerKind,
    pub owner_id: Option<String>,
    pub role: CellRole,
    pub z_index: u8,
    pub collided_with: SmallVec<[CellCollision; 2]>,
}

pub struct SemanticFrame {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<CellMeta>,
    pub graph_snapshot: Graph,
    pub debug_notes: Vec<String>,
}

pub struct CriticFinding {
    pub code: FindingCode,
    pub severity: FindingSeverity,
    pub penalty: i32,
    pub message: String,
    pub cells: Vec<Point>,
    pub owner_ids: Vec<String>,
    pub suggested_repairs: Vec<RepairClass>,
}

pub struct CriticReport {
    pub total_score: i32,
    pub findings: Vec<CriticFinding>,
}

pub struct RepairAction {
    pub class: RepairClass,
    pub description: String,
    pub cost: u16,
    pub target_ids: Vec<String>,
    pub patch: RepairPatch,
}

pub struct RepairIteration {
    pub before_score: i32,
    pub action: RepairAction,
    pub after_score: i32,
}

pub struct RepairSession {
    pub iterations: Vec<RepairIteration>,
    pub best_score: i32,
    pub converged: bool,
}
```

Important rule:
The final ASCII string remains a projection from `SemanticFrame`, not the source of truth.

---

## 9. Critic Design

### 9.1 Scoring Model

Use a penalty-based score where lower is worse and zero is ideal.

Suggested categories:

| Category | Purpose |
|----------|---------|
| Collision | Penalize edges or labels occupying forbidden areas |
| Topology | Penalize malformed corners, tees, crosses, or arrow attachments |
| Readability | Penalize visually cramped or ambiguous geometry |
| Container integrity | Penalize subgraph border/title corruption |
| Directional aesthetics | Penalize LR/RL or BT-specific ugliness |
| Clipping | Penalize any content lost to canvas limits |
| Stability | Penalize noisy repairs that worsen nearby regions |

### 9.2 Initial Finding Set

Phase 1 critic rules should be concrete and easy to test:

1. `EDGE_OVER_TEXT`
2. `EDGE_OVER_BOX_INTERIOR`
3. `ARROW_ON_SUBGRAPH_BORDER`
4. `ARROW_WITHOUT_VISIBLE_SHAFT`
5. `JUNCTION_WRONG_GLYPH_FOR_TOPOLOGY`
6. `PORTAL_CARVED_BUT_UNUSED`
7. `SUBGRAPH_TITLE_PIERCED`
8. `SUBGRAPH_SIBLING_OVERLAP`
9. `EDGE_LABEL_TOUCHES_TURN`
10. `EDGE_LABEL_TOUCHES_JUNCTION`
11. `CYCLE_GUTTER_CLIPPED`
12. `CHAIN_TOO_CRAMPED_LR`

### 9.3 Topology-Aware Criticism

The critic should not rely solely on glyph identity.

It should infer neighborhood topology:

- north/south/east/west connectivity
- whether a cell is an endpoint
- whether a tee is semantically split or merge
- whether a corner direction matches route intent
- whether an arrowhead lands on the correct side of its target

This will catch cases where the glyph is plausible but semantically wrong.

### 9.4 Critic Output for Developers

Add a debug mode that emits:

- total score
- sorted findings
- per-finding cell coordinates
- owner ids
- suggested repairs

This should be serializable to JSON for fixture debugging.

---

## 10. Repair Strategy

### 10.1 Philosophy

Repairs should be:

- local
- cheap
- understandable
- composable
- reversible

The system should not jump straight to full relayout unless a local repair class cannot help.

### 10.2 Repair Classes

#### A. Geometry Repairs

- shift one node along secondary axis by `+/-1`
- shift one subgraph cohort together
- widen one rank boundary
- increase local row/column spacing
- increase local stem length

#### B. Routing Repairs

- reroute a single edge with a stronger obstacle halo
- move a junction row or column
- pick a different fan-in merge location
- pick a different fan-out branch line
- prefer alternate portal slot

#### C. Label Repairs

- move edge label anchor
- reserve extra padding around edge label
- prefer opposite side of segment
- allow segment extension to host label cleanly

#### D. Container Repairs

- widen subgraph gutter
- move title span reservation
- prevent portal carving under title span
- push sibling subgraphs apart

#### E. Canvas Repairs

- increase canvas limits for this render
- preserve cycle gutter width
- disable cropping until after optimization

### 10.3 Search Algorithm

Recommended rollout:

1. **Greedy single-step improvement**
   - Generate repairs from top findings.
   - Try each action independently.
   - Keep the best improvement.

2. **Bounded beam search**
   - Beam width: `3-5`
   - Iteration limit: `5-10`
   - Action fanout cap per iteration: `10-20`

3. **Hard stop**
   - Stop when no candidate improves score.
   - Stop when iteration budget is exhausted.

### 10.4 Determinism Rules

To keep outputs stable:

- sort findings deterministically
- sort repair actions deterministically
- stable tie-breaking by owner id, coordinates, then action class
- no randomness

---

## 11. Multi-Pass Repair Loop

### 11.1 Render Optimization Loop

```text
candidate_0 = base layout + base semantic render
score_0 = critic(candidate_0)

for iteration in 1..=budget:
    repairs = planner(top_findings(candidate_n))
    tested = apply_and_rerender_each(repairs)
    best = min_score(tested)
    if best.score >= candidate_n.score:
        stop
    candidate_n = best

return candidate_best
```

### 11.2 Two Operating Modes

#### Default mode

- zero or one repair pass
- tuned for CLI speed

#### High-quality mode

- multiple repair iterations
- intended for fixture generation, screenshot output, future TUI preview, and regression analysis

Suggested CLI concepts for later:

- `--optimize-render=off|basic|full`
- `--render-debug-report`
- `--render-iterations N`

Do not add CLI surface in the first implementation step.

---

## 12. Terminal Diff Renderer

This is a separate subsystem from diagram optimization.

### 12.1 Purpose

When TermiFlow eventually supports live preview or TUI navigation, it should not clear and repaint the full terminal every update. Instead it should:

- keep previous frame
- keep next frame
- diff them cell-by-cell
- emit minimal cursor moves and text writes

### 12.2 Target Interface

```rust
pub struct TerminalFrame {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<StyledCell>,
}

pub trait TerminalPresenter {
    fn present(&mut self, next: &TerminalFrame) -> io::Result<()>;
}
```

### 12.3 Rules

- Prefer in-place region updates over full-screen clears.
- Keep scrollback-friendly behavior where possible.
- Reserve alt-screen mode for future cases where viewport control truly requires it.
- Support a "dumb terminal fallback" that does full redraws.

### 12.4 Why This Matters to TermiFlow

This is useful for:

- future `--tui`
- future `--watch`
- live preview in editor integration
- debugging optimization iterations visually

---

## 13. Incremental Rollout Plan

### Phase 6.0: Baseline Instrumentation

Deliverables:

- add render audit report type
- add hidden debug command or env var to dump findings
- measure current rendering time and candidate defect frequency

Files likely touched:

- `src/render/mod.rs`
- `src/render/canvas.rs`
- `src/lib.rs`
- `tests/`

Exit criteria:

- baseline fixtures can be rendered with a debug report
- no output changes

### Phase 6.1: Rich Canvas / Semantic Frame

Deliverables:

- augment canvas cells with provenance and roles
- retain collision metadata instead of only final glyph
- add conversion from semantic frame to current string output

Files likely touched:

- `src/render/canvas.rs`
- `src/render/mod.rs`
- `src/render/edge.rs`
- `src/render/shapes.rs`

Exit criteria:

- all existing tests pass
- semantic projection reproduces current output

### Phase 6.2: Structural Critic

Deliverables:

- implement first 8-12 findings
- add JSON report output under debug flag or env var
- add focused unit tests for each finding type

Files likely touched:

- `src/render/critic.rs` (new)
- `src/render/mod.rs`
- `tests/`

Exit criteria:

- critics detect known bad synthetic cases
- no default output changes

### Phase 6.3: Local Repair Engine

Deliverables:

- implement repair planner
- implement greedy first-pass repair loop
- wire in optional high-quality render mode

Files likely touched:

- `src/render/repair.rs` (new)
- `src/layout.rs`
- `src/portals.rs`
- `src/render/edge.rs`
- `src/lib.rs`

Exit criteria:

- selected golden fixtures improve without regressions
- optimization remains deterministic

### Phase 6.4: Beam Search and Candidate Selection

Deliverables:

- bounded beam search for difficult layouts
- score trace for debugging
- protection against oscillation

Exit criteria:

- measurable improvement on historically ugly fixtures
- bounded runtime with no path explosion

### Phase 6.5: Terminal Frame Diff Presenter

Deliverables:

- terminal frame abstraction
- frame diff algorithm
- ANSI presenter
- full redraw fallback

Files likely touched:

- `src/tui/frame.rs` (new)
- `src/tui/presenter.rs` (new)
- `src/tui/mod.rs` (new)
- `src/bin/common/mod.rs`

Exit criteria:

- live redraw works without full-screen clear on every update
- no regression to current print mode

### Phase 6.6: Live Preview / TUI Integration

Deliverables:

- hook optimized semantic render into future `--tui`
- add viewport navigation
- optionally support watching file changes

Exit criteria:

- interactive preview feels stable
- redraws are localized

---

## 14. File and Module Plan

### New Files

- `src/render/critic.rs`
- `src/render/repair.rs`
- `src/render/semantic.rs`
- `src/tui/frame.rs`
- `src/tui/presenter.rs`
- `src/tui/mod.rs`
- `tests/render_critic.rs`
- `tests/render_repair.rs`

### Existing Files Most Likely to Change

- `src/render/canvas.rs`
- `src/render/mod.rs`
- `src/render/edge.rs`
- `src/render/shapes.rs`
- `src/layout.rs`
- `src/portals.rs`
- `src/lib.rs`
- `src/bin/common/mod.rs`
- `planning/PLAN.md`
- `tests/fixtures/`

### Refactor Priority

Before aggressive repair logic, the following should be made more parameterizable:

- edge routing decisions in `src/render/edge.rs`
- rank-boundary spacing decisions in `src/layout.rs`
- portal slot selection in `src/portals.rs`
- label placement in `src/render/mod.rs`

Without this, the repair loop will have too few levers.

---

## 15. Testing Strategy

### 15.1 Unit Tests

Add isolated tests for:

- critic topology classification
- finding generation
- repair action generation
- repair determinism
- frame diff correctness

### 15.2 Golden Tests

Add fixture families specifically for render feedback:

- `feedback_arrow_border_*`
- `feedback_short_shaft_*`
- `feedback_subgraph_title_pierce_*`
- `feedback_lr_cramped_*`
- `feedback_label_corner_*`

These fixtures should verify both:

- final rendered output
- optional machine-readable critic report snapshots

### 15.3 Property Tests

Recommended later:

- topology/glyph consistency properties
- no text cells overwritten by edges
- arrows remain reachable from their source route
- portal openings must either be used or not carved

### 15.4 Performance Tests

Benchmark:

- base render time
- semantic render overhead
- critic pass overhead
- repair iteration overhead
- beam search worst-case budget

Target budgets:

- default print mode should remain close to current performance
- high-quality mode can be slower but must remain bounded and measurable

---

## 16. Observability and Debugging

Add debug outputs that help explain optimization:

- `TERMIFLOW_DEBUG_CRITIC=1`
  - print findings and scores

- `TERMIFLOW_DEBUG_REPAIRS=1`
  - print proposed actions and their score deltas

- `TERMIFLOW_DEBUG_FRAME_DIFF=1`
  - print terminal diff statistics

- `TERMIFLOW_DUMP_SEMANTIC_FRAME=path.json`
  - dump semantic frame to JSON for inspection

These should never affect deterministic output.

---

## 17. Risks

### 17.1 Complexity Risk

This phase can easily become a second layout engine by accident.

Mitigation:

- limit repairs to local, explicit action classes
- avoid general simulated annealing or large global search

### 17.2 Performance Risk

Multi-pass repair can become expensive on large diagrams.

Mitigation:

- make optimization opt-in or tiered
- use hard iteration and beam caps
- cache expensive topology queries

### 17.3 Debuggability Risk

A repair loop can make output harder to reason about if the system mutates silently.

Mitigation:

- require each action to be logged with its motivating findings
- expose optimization traces in debug mode

### 17.4 Regression Risk

Fixing one local defect may worsen another.

Mitigation:

- compare total score, not single finding improvements
- maintain regression fixtures for known historical failures

### 17.5 Architectural Drift Risk

The new semantic layer could diverge from the char projection layer.

Mitigation:

- semantic frame must remain the sole source of truth
- string output is a projection, never the canonical state

---

## 18. Open Questions

1. Should optimization happen in `layout`, `render`, or a new orchestration layer above both?
   - Recommendation: above both, so repairs can touch either subsystem.

2. Should the critic run on every render by default?
   - Recommendation: compute light-weight findings by default later, but start opt-in.

3. Should the repair loop be part of the library API?
   - Recommendation: yes, eventually via additional render options.

4. Should terminal diff rendering preserve scrollback or use alt-screen?
   - Recommendation: preserve scrollback first, alt-screen only for fully interactive viewport mode.

5. Should any model-based inspection be added later?
   - Recommendation: only as a developer tool, never as the primary repair engine.

---

## 19. Recommended Milestones

### Milestone A

Semantic frame exists and reproduces current output exactly.

### Milestone B

Critic detects known structural defects with good coverage.

### Milestone C

Single-step repair improves a small set of historically ugly fixtures.

### Milestone D

High-quality mode produces measurably better LR/RL and subgraph-heavy outputs.

### Milestone E

Future TUI/live preview uses a terminal frame diff presenter with localized redraws.

---

## 20. Immediate Next Steps

Recommended implementation order:

1. Refactor `Canvas` to preserve semantic cell metadata.
2. Add a semantic-frame-to-string projection that preserves current output byte-for-byte.
3. Add a small `critic.rs` with 3 initial findings:
   - `ARROW_ON_SUBGRAPH_BORDER`
   - `JUNCTION_WRONG_GLYPH_FOR_TOPOLOGY`
   - `CHAIN_TOO_CRAMPED_LR`
4. Add debug report dumping.
5. Add one greedy repair action for each of those findings.
6. Benchmark overhead before expanding the critic surface.

This is the smallest path that proves the concept without overcommitting to a full rewrite.

---

## 21. Success Criteria

This phase is successful if:

- TermiFlow can explain why a diagram looks wrong.
- TermiFlow can improve selected ugly diagrams automatically.
- Output remains deterministic.
- Existing print mode remains stable.
- The codebase gains a reusable frame model that supports future live redraw and TUI work.

This phase is not successful if:

- the optimizer becomes opaque
- performance becomes unbounded
- visual quality gains depend on one-off hacks with no critic visibility

---

## 22. Final Recommendation

Proceed with this phase.

The codebase is already well-structured enough to support it, especially because:

- parser, measure, layout, and render are separated
- direction abstraction already exists
- rendering already behaves like a framebuffer pass
- the major remaining quality issues are visual and local, not parser-level

The right strategy is:

- semantic framebuffer
- structural critic
- bounded local repair loop
- optional terminal frame diff presenter

That combination gives TermiFlow the practical form of render awareness needed to iteratively improve ASCII diagrams while staying explainable and deterministic.
