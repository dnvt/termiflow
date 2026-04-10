# Project: Nested Subgraph Support

**Status:** Completed
**Roadmap Slot:** Completed rendering-precision child slice
**Owner:** Maintainer
**Timeline:** 2026-04-08 -> 2026-04-10
**Aligned To:** `planning/RENDERING_PRECISION_PROGRAM.md`
**Decision Links:** Rationale captured in `analysis/2026-04-08-nested-subgraph-containment.md`; linked issue `#4`

## Objective

Support true nested subgraphs so parent containers fully enclose child
containers, nodes do not visually pierce child borders, and cross-boundary
edges route cleanly in all supported directions.

## Scope

**In:** parser support for nested subgraph trees, graph model parent/child
relationships, hierarchical envelope/layout logic, portal and routing updates
across ancestor boundaries, render updates for nested borders/titles, fixtures,
visual-audit coverage, and docs truth updates.

**Out:** subgraph-local direction overrides, edges targeting subgraph IDs,
subgraph styling, non-flowchart diagram types, and crates.io / OSS launch work.

## Why This Exists

Nested subgraphs started as a documented unsupported case. The original parser
warning path effectively flattened the inner subgraph, which caused misleading
output for diagrams that expected true containment. The user-visible symptom
was border overlap, but the underlying issue was that hierarchy was lost before
layout and render. That original failure mode is no longer the active problem:
hierarchy is now preserved, and the remaining work is the last layout/audit
slice needed to make nested rendering support credible across directions.

## Current Execution Slice

Execution is closed:

- parser/model hierarchy is preserved, including bare node-reference membership
  lines inside subgraphs
- bottom-up envelope construction and descendant-edge padding are in place
- portal-slot selection and layout clearance are ancestor-aware for nested
  border crossings
- route-dense nested children reserve internal fan-in/fan-out span before
  final envelopes
- nested render/audit coverage now includes dedicated `TD`, `BT`, `LR`, and
  `RL` regressions plus curated fixture coverage

## Success Criteria

- [x] Nested subgraph syntax is preserved in the parse/model layer instead of
      being flattened into the outer subgraph.
- [x] A child subgraph's outer bounds are fully contained within its parent's
      interior with at least one clear spacer row/column in all supported
      directions.
- [x] Representative nested fixtures for `TD`, `LR`, `BT`, and `RL` render
      without node-on-border artifacts or parent/child border collisions.
- [x] Route-dense nested children reserve enough horizontal budget that
      internal fan-in/fan-out paths do not collapse against child borders.
- [x] Cross-subgraph edges entering or leaving nested containers route through
      the correct ancestor border with stable ownership/provenance.
- [x] `--audit` and targeted tests flag nested-boundary corruption regressions.
- [x] `README.md` and `docs/reference.md` are updated to reflect the new
      support level and any remaining limits.

## Dependencies

- A stable execution branch or isolated worktree, since this change will touch
  parser, layout, portals, render, and large fixture sets.
- Agreement on rollout shape: full nested support vs. staged support with an
  interim stricter failure mode.
- A representative fixture set that covers the user's containment case plus
  cross-edge and multi-direction cases.

## Risks & Mitigations

- Parser/model changes create cascading regressions in flat subgraphs
  -> keep existing single-level fixtures green and add explicit parent/child
     model tests before layout work.
- Layout heuristics become more brittle if nesting is bolted onto the current
  overlap-expansion logic
  -> introduce explicit subgraph hierarchy data and ancestor-aware envelope
     computation instead of layering more overlap heuristics.
- Edge routing across ancestor chains creates ambiguous portal ownership
  -> extend provenance and portal slot logic with explicit ancestor traversal
     tests before broad fixture regeneration.
- Scope creep pulls in adjacent Mermaid gaps such as subgraph-local direction
  or styling
  -> keep those features explicitly out of scope for this project.

## Open Questions

- Should v1 of this project support arbitrary nesting depth or cap the first
  implementation at one nested level?
- Should unsupported nested syntax become a strict-mode failure immediately,
  even before full support lands?
- Do we want parent/child containment to reserve fixed gutters, or adapt gutter
  depth based on titles and portal density?

## Evidence To Gather

- The smallest set of real diagrams that demonstrate the user's intended
  containment semantics.
- Which current visual heuristics in `src/portals.rs` remain useful once
  explicit hierarchy exists.
- Whether arbitrary-depth nesting materially increases complexity relative to a
  one-level rollout.

## Experiment Log

**Hypothesis:** true hierarchical subgraph modeling will produce cleaner and
more predictable containment than expanding the current flat overlap heuristics.

**Intervention:** implement nested-subgraph support in ordered phases, starting
with parser/model preservation and a minimal nested-fixture matrix.

**Expected Observation:** the user's containment case and a representative
multi-direction fixture set render with full parent/child enclosure and without
border-eating artifacts.

**Actual Observation:** explicit parent envelopes now build cleanly from child
envelopes even when the parent has no direct nodes; descendant crossings count
as parent external edges during envelope construction; portal-slot
selection/layout clearance reason about ancestor-boundary crossings instead of
only the innermost subgraph; declared nested vertical children get both
side-biased outgoing-pressure widening and centered internal span budgeting;
and the horizontal directions now reserve enough headroom/side clearance that
nested titles and external node boxes do not collapse into the parent border.
Representative nested `TD`, `LR`, `BT`, and `RL` fixtures now audit clean in
both ASCII and Unicode.

**Conclusion:** the hypothesis held. Explicit hierarchy plus targeted layout
budgeting produced cleaner and more predictable containment than the old flat
overlap heuristics, and nested subgraph support is now complete for the
currently supported flowchart directions.

## Execution Outline

### Phase 1: Parser and Graph Model

- Add parent/child subgraph relationships to the graph model.
- Preserve nested `subgraph ... end` structure during parse.
- Replace the current "warn and flatten" behavior with stored hierarchy.
- Add unit tests for nested parsing, membership, and parent linkage.
- Status: done

### Phase 2: Hierarchical Envelope Construction

- Refactor subgraph envelope construction in `src/portals.rs` to operate on a
  tree rather than a flat set.
- Define containment rules: parent interior, child clearance, title clearance,
  and minimum spacer policy.
- Ensure nested envelopes do not rely on overlap as the primary signal.
- Status: done

### Phase 3: Layout and Portal Semantics

- Update layout so child subgraphs are placed within parent interior bounds.
- Extend portal slot selection to account for ancestor chains and nested border
  crossings.
- Clarify which border an edge pierces when moving from child to sibling,
  child to parent, or parent to external node.
- Status: done
- Current state:
  - `Graph` now exposes explicit edge boundary crossings (exclusive exit and
    enter ancestor chains).
  - `collect_portal_slots()` now opens every crossed ancestor boundary instead
    of only the innermost source/target subgraph.
  - TD/BT layout clearance loops now key off ancestor crossings instead of
    direct subgraph equality.
  - Declared nested `TD`/`TB` children now participate in both the earlier
    side-biased outgoing route-pressure widening pass and a centered internal
    route-span budget pass. When multiple child sources converge to one
    external target, or one external source fans into multiple child targets,
    layout can now widen the child's right partition even when the external
    anchor stays centered inside the child span.
  - Declared nested `LR`/`RL` parents now reserve enough vertical headroom for
    stepped title rows and enough horizontal clearance that adjacent external
    node boxes no longer collapse into the crossed parent border.
  - Dedicated render/audit regressions now cover nested `BT`, `LR`, and `RL`
    cases that previously relied on smoke coverage or manual inspection.
- Phase 3 exit gate:
  - completed: dedicated nested `BT` and horizontal render regressions exist
    in `tests/render_options_api.rs`
  - completed: representative nested fixtures for `TD`, `LR`, `BT`, and `RL`
    were re-run and reviewed clean
  - completed: curated visual-audit coverage now includes nested fixtures, so
    title corruption, wrong-border portal carving, and similar nested-boundary
    regressions fail fast instead of relying on manual inspection

### Phase 4: Rendering and Provenance

- Render nested borders and titles without parent/child collision.
- Keep portal carving and unused-hole repair ancestor-aware.
- Extend provenance ownership so nested border cells and portal openings remain
  attributable and repairable.

### Phase 5: Test and Audit Expansion

- Add nested fixtures covering:
  - user-style containment
  - nested TD/LR/BT/RL basics
  - child-to-parent and child-to-external edges
  - sibling nested groups within one parent
- Add `--audit` cases for border overlap, title corruption, and wrong-border
  portal carving in nested layouts.

### Phase 6: Docs and Rollout

- Update docs from "warn and ignore" to the actual supported nesting scope.
- If rollout is staged, document the remaining boundary clearly.
- Reassess `planning/PLAN.md` once the work becomes active rather than draft.

## Recommendation

Treat this as a real feature project, not a one-off render tweak. If immediate
implementation is deferred, add a smaller follow-up task to make unsupported
nested syntax fail more honestly so the renderer does not imply partial support.
