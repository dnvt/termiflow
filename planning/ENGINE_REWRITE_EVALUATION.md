# Project: Engine Rewrite Evaluation

**Status:** Complete - stay-the-course decision captured on 2026-04-10
**Roadmap Slot:** Historical Reference
**Owner:** Maintainer
**Timeline:** 2026-04-09 -> 2026-04-10
**Aligned To:** `planning/RENDERING_PRECISION_PROGRAM.md` and post-beta architecture options
**Decision Links:** `analysis/research/2026-04-09-termiflow-rendering-precision-strategy-research.md`; `analysis/research/2026-04-09-engine-rewrite-evaluation-spike.md`; `analysis/research/2026-04-09-current-rust-engine-evolution-research.md`; linked issue `#6`

## Objective

Define when a real engine rewrite would be justified, what options would be
evaluated, and what evidence would have to outweigh continued evolution of the
current Rust-native renderer.

## Scope

**In:** rewrite triggers, option set, evaluation criteria, prototype boundaries,
benchmark/oracle requirements, licensing and packaging constraints, and explicit
kill criteria for weak rewrite ideas.

**Out:** shipping a rewrite before beta, adopting an external runtime router for
the current beta path, replacing the CLI/TUI surface, or reopening scope that
the rendering-precision initiative can address more cheaply.

## Why This Exists

The research brief did not support a rewrite now, but it also did not prove the
current engine is the forever architecture. The right move is to make that fork
explicit and evidence-driven so future sessions do not relitigate the same
question from scratch.

## Success Criteria

- [x] The option set is explicit: continue evolving the current engine, replace
      selected subsystems, integrate an external layout/router, or pursue a
      full rewrite.
- [x] Rewrite triggers are measurable or at least observable, not intuition
      only.
- [x] Legal, packaging, and runtime tradeoffs are documented for any external
      engine or router candidate.
- [x] A bounded prototype plan exists for the top candidate options if the
      trigger is reached.
- [x] The evaluation ends in a decision gate: stay the course, prototype,
      rewrite, or kill the idea.

## Option Set To Compare

### Option A: Keep Evolving the Current Engine

- Continue with the rendering-precision initiative.
- Prefer explicit model and measurement improvements over architecture churn.

### Option B: Partial Subsystem Replacement

- Keep the parser/graph/render surface, but replace one layer such as ranking,
  orthogonal routing, or preview transport.
- Only viable if the boundary is technically clean and the integration cost is
  lower than continued in-house evolution.

### Option C: External Layout / Routing Integration

- Evaluate engines such as ELK or similar tools for offline or build-time
  validation, or potentially runtime integration.
- Must clear licensing, packaging, reproducibility, and label/border semantics
  constraints before it can be considered viable.

### Option D: Full Rewrite

- Replace the current layout/render core with a new engine.
- Highest potential upside, highest migration and schedule risk.

## Evaluation Criteria

- Precision improvement on dense compound diagrams
- Label and border semantics fidelity
- Unicode and preview consistency impact
- Reversibility
- Runtime and packaging complexity
- Licensing constraints
- Testability and benchmarkability
- Fit with the narrow beta wedge

## Dependencies

- The rendering-precision initiative must produce enough evidence to show
  whether the current engine still has practical headroom.
- Dense-graph benchmark and oracle coverage must exist before the comparison is
  meaningful.
- Post-beta user feedback should exist before high-cost rewrite options are
  considered seriously.

## Risks & Mitigations

- Rewrite planning becomes an architecture rabbit hole before beta
  -> keep this plan closed as a no-rewrite decision record and open a new,
     bounded prototype decision only if explicit triggers are met.
- A seductive external engine looks cleaner on paper than in integration
  reality
  -> require prototype evidence, legal review, and packaging analysis before
     any architecture commitment.
- Prototype work leaks into the shipping path prematurely
  -> isolate any experiments behind a clear spike boundary and keep runtime
     dependencies off the beta path.

## Open Questions

- Is the real pain layout quality, routing quality, label semantics, or all
  three together?
- Would a partial replacement create a cleaner architecture, or only split
  ownership across awkward boundaries?
- Is post-beta feedback likely to demand higher generality than the current
  narrow flowchart wedge?
- What level of nondeterminism would be acceptable from a more powerful but
  less stable engine?

## Evidence To Gather

- Results from the rendering-precision initiative, especially on nested and
  route-dense fixtures.
- Benchmark data that shows whether current-engine improvements are converging
  or flattening out.
- Prototype notes for at most one external candidate and one partial-rewrite
  candidate if triggers are met.
- Licensing and packaging analysis for external routing/layout dependencies.

## 2026-04-09 Spike Baseline

The current-engine baseline gathered in `analysis/research/2026-04-09-engine-rewrite-evaluation-spike.md`
is stronger than this plan originally assumed.

| Fixture | Concern | Verdict | Unicode size | Note |
|---------|---------|---------|--------------|------|
| `subgraph_complex_td` | nested, route-dense TD | Clean | `162 x 37` | no critic findings |
| `subgraph_complex_bt` | reverse-flow nested fan-in | Clean | `138 x 39` | no critic findings |
| `collision_sibling_subgraphs_td` | sibling containment / border pressure | Clean | `102 x 26` | no critic findings |
| `converge_cascade_td` | dense fan-in | Clean | `108 x 15` | no critic findings |
| `unicode_emoji_td` | grapheme / emoji width pressure | Clean | `48 x 13` | audit clean, still width-model dependent |
| `unicode_cjk_td` | CJK width pressure | Clean | `36 x 13` | audit clean, still emulator dependent |
| `subgraph_nested_td` | true nested hierarchy | Clean audit + parser warning | `76 x 18` | hierarchy preserved, but was still visually rough on the first spike |

That does not prove the engine is forever architecture, but it does mean the
rewrite triggers are not currently tripped by the representative fixture set
most likely to force the question.

## 2026-04-10 Refresh

The current worktree is materially stronger than the 2026-04-09 spike.

### Fresh Audit Snapshot

| Fixture | Concern | Audit | Size | Notes |
|---------|---------|-------|------|-------|
| `subgraph_complex_lr` | horizontal compound fan-in / side-wall portals | Clean | `340 x 17` | no critic findings; LR side-wall seams now stay clean openings |
| `subgraph_complex_rl` | reverse horizontal compound fan-in / side-wall portals | Clean | `343 x 17` | no critic findings; RL side-wall seams now stay clean openings |
| `subgraph_nested_td` | true nested hierarchy | Clean | `96 x 30` | no parser warning, hierarchy preserved, still denser than the flatter compound cases |

### Fresh Benchmark Snapshot

`cargo bench --bench rendering -- route_dense_subgraphs --noplot`

| Case | Time | Interpretation |
|------|------|----------------|
| `subgraph_complex_td` | `356.65 µs` | no performance change detected |
| `subgraph_complex_lr` | `347.59 µs` | change within noise threshold |
| `subgraph_complex_bt` | `297.39 µs` | no performance change detected |
| `subgraph_complex_rl` | `353.30 µs` | change within noise threshold |
| `collision_sibling_subgraphs_lr` | `156.92 µs` | change within noise threshold |
| `collision_sibling_subgraphs_rl` | `156.34 µs` | performance improved |

The refresh matters because the evaluation question is no longer "can the
engine survive dense compound routing at all?" The current answer is yes. The
remaining question is whether future precision work stalls badly enough that a
deeper architecture change becomes cheaper than continued bounded evolution.

## Prototype Boundaries From This Spike

### Partial Candidate Worth Preserving

**Name:** compound lane-budget planner

**Boundary:** keep parser, graph model, renderer, CLI, and TUI intact; replace
only the layout-side negotiation of nested containers, portal demand, and lane
budgeting.

**Success bar:** improve future trigger fixtures without introducing public API
changes or direction-specific special cases that exceed the current model.

**Kill criteria:** stop if the prototype requires renderer rewrites to stay
viable, cannot preserve deterministic output across directions, or fails to
beat the current engine on nested compound fixtures after bounded effort.

### External Candidate Worth Preserving

**Name:** ELK-backed offline oracle / comparison adapter

**Boundary:** no runtime dependency and no shipping rewrite. Export selected
fixtures to an external comparison format only to test whether an external
engine actually offers meaningfully better hierarchy or rank guidance.

**Success bar:** produce evidence on future trigger fixtures that an external
engine would materially improve the layout problem TermiFlow still has after the
rendering-precision program.

**Kill criteria:** stop if the adapter requires Node/JVM/runtime bundling,
introduces licensing or reproducibility problems, or still leaves border/label
semantics squarely on the TermiFlow side without a clear net win.

## Experiment Log

**Hypothesis:** the current TermiFlow engine can reach beta-quality precision
through bounded model and measurement work, making a rewrite unnecessary in the
near term.

**Intervention:** defer rewrite work until the rendering-precision initiative
produces evidence strong enough to support or weaken that hypothesis, then run a
bounded spike against representative fixtures before escalating.

**Expected Observation:** either the current engine reaches acceptable quality
without architecture churn, or a clear failure mode remains that maps cleanly
to a rewrite candidate.

**Actual Observation:** representative route-dense, sibling-subgraph, and
Unicode-heavy fixtures all audit clean on the current engine, and fresh
horizontal compound cases (`subgraph_complex_{lr,rl}`) now also audit clean
with the intended side-wall portal contract. `subgraph_nested_td` no longer
emits the experimental parser warning and also audits clean, though it remains
the visually densest hierarchy case and therefore the clearest place to watch
for future precision flattening. Fresh route-dense benchmarks show no evidence
of a performance cliff in the current engine path.

**Conclusion:** the refresh does not justify escalating to a rewrite. It
justifies preserving one partial candidate and one external-oracle candidate,
and routing near-term effort back into
`planning/RENDERING_PRECISION_PROGRAM.md`.

## Rewrite Triggers

Open a new rewrite/prototype decision only if one or more of these hold after the
rendering-precision initiative has produced evidence:

1. Dense compound-diagram quality remains below an acceptable bar even after
   explicit lane budgeting and measurement fixes.
2. Preview and render exactness still diverge materially after unified
   cell-metric work.
3. The current repair/scoring pipeline becomes too complex to maintain relative
   to the quality it delivers.
4. Post-beta user demand requires capabilities that the current architecture
   cannot add without compounding technical debt.

## Decision Gate

**2026-04-10 verdict:** Stay the course with Option A, continue evolving the
current Rust-native engine.

Why:
- the option set is explicit and the current winner is still the cheapest one
  that satisfies the beta wedge
- the current worktree now clears representative TD, LR, BT, and RL compound
  fixtures without tripping the rewrite triggers
- the remaining hard problem is quality headroom in future precision slices,
  not present-tense engine failure
- the best preserved alternatives are still a bounded partial candidate
  (compound lane-budget / cell-scene evolution) and an offline external-oracle
  candidate, not a runtime rewrite stream

What remains deferred:
- a deeper cell-scene / hybrid orth-router architecture path remains preserved
  in `planning/CELL_SCENE_GRAPH_HYBRID_ORTH_ROUTER.md`
- any external-engine comparison remains offline-only until a trigger is met
- no runtime rewrite work should enter the beta path from this plan

## Recommendation

Close this evaluation as a "do not rewrite now" decision record. Continue the
rendering-precision initiative and only reopen rewrite evaluation if the
trigger conditions are met or post-beta evidence changes the cost/benefit
picture materially.
