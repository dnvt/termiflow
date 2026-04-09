# Project: Nested Subgraph Support

**Status:** Active
**Roadmap Slot:** Active Workstreams
**Owner:** Maintainer
**Timeline:** 2026-04-08 -> TBD
**Aligned To:** Post-1.0 Mermaid parity backlog and diagram-fidelity work
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

Nested subgraphs are currently documented as unsupported. The parser warns and
ignores the inner subgraph, which causes misleading output for diagrams that
expect true containment. The user-visible symptom is border overlap, but the
underlying issue is that hierarchy is flattened before layout and render.

## Current Execution Slice

Phase 3 is still in progress, but the focus has changed:

- keep the Phase 1 parser/model hierarchy intact, including bare node-reference
  membership lines inside subgraphs
- keep the Phase 2 bottom-up envelope construction and descendant-edge padding
- keep portal-slot selection and layout clearance ancestor-aware for nested
  border crossings
- estimate internal fan-in/fan-out span before final envelopes
- widen child subgraphs when internal merges/exits would otherwise crowd the
  child border or sibling nodes
- place nodes inside that widened child before render, instead of relying on
  post-layout edge routing to absorb congestion

## Success Criteria

- [ ] Nested subgraph syntax is preserved in the parse/model layer instead of
      being flattened into the outer subgraph.
- [ ] A child subgraph's outer bounds are fully contained within its parent's
      interior with at least one clear spacer row/column in all supported
      directions.
- [ ] Representative nested fixtures for `TD`, `LR`, `BT`, and `RL` render
      without node-on-border artifacts or parent/child border collisions.
- [ ] Route-dense nested children reserve enough horizontal budget that
      internal fan-in/fan-out paths do not collapse against child borders.
- [ ] Cross-subgraph edges entering or leaving nested containers route through
      the correct ancestor border with stable ownership/provenance.
- [ ] `--audit` and targeted tests flag nested-boundary corruption regressions.
- [ ] `README.md` and `docs/reference.md` are updated to reflect the new
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

**Actual Observation:** explicit parent envelopes can now be built from child
envelopes even when the parent has no direct nodes; descendant crossings are
counted as parent external edges during envelope construction; portal-slot
selection/layout clearance now reason about ancestor-boundary crossings instead
of only the innermost subgraph; and the remaining `subgraph_complex_td` defect
is no longer basic containment. The current failure is that nested child width
is still sized from node boxes plus minimal clearance, so render-time fan-in
and exit geometry is forced to compete inside an envelope that layout never
budgeted for.

**Conclusion:** pending

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
- Status: in progress
- Current state:
  - `Graph` now exposes explicit edge boundary crossings (exclusive exit and
    enter ancestor chains).
  - `collect_portal_slots()` now opens every crossed ancestor boundary instead
    of only the innermost source/target subgraph.
  - TD/BT layout clearance loops now key off ancestor crossings instead of
    direct subgraph equality.
  - Remaining defect: nested child width is still computed from contained node
    boxes and minimal clearance only, not from the internal route span that the
    child must host.
  - User-visible symptom: route-dense nested children such as `Data Layer`
    still compress `Order DB` / `User DB` exits and nested fan-in geometry into
    a width budget chosen before those routes are considered.
- Next slice:
  - add a pre-envelope route-demand estimate for nested children
  - derive a minimum child width from contained nodes plus internal merge/exit
    span, not only from node-box extents
  - place nodes within that widened child before final envelope construction
  - keep render focused on drawing clean routes instead of compensating for
    under-sized nested envelopes

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
