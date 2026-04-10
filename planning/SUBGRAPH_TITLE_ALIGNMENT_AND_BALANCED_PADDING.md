# Task: Subgraph Title Alignment and Balanced Padding

**Status:** Complete
**Roadmap Slot:** Completed rendering-precision child slice
**Owner:** Maintainer
**Timeline:** 2026-04-10 planning and implementation
**Aligned To:** nested-subgraph hardening and render-consistency work

## Objective

Change the subgraph contract in two fundamental ways:

1. subgraph titles should no longer be centered; they should anchor to the
   beginning of the subgraph border according to direction
2. subgraph interior padding should try to remain visually balanced on all
   sides, even when that means growing the subgraph larger than the minimum
   route-fit size

This is not a renderer-only polish pass. In the current architecture, title
placement and envelope sizing are coupled to portal protection, nested
containment, route budgeting, and several clearance heuristics. The work must
therefore be planned as a structural subgraph-model change.

## Outcome

This slice landed.

- Titles are now direction-anchored through one shared geometry contract:
  - `TD` / `TB` / `LR`: top-left anchored
  - `RL`: top-right anchored
  - `BT`: bottom-left anchored
- Subgraph envelope sizing now computes hard per-side route/title demand first,
  then applies balanced visual padding targets by axis:
  - one horizontal target for left/right
  - one vertical target for top/bottom
- The implementation deliberately did **not** ship a single equal padding
  target across all four sides. That variant broke `LR`/`RL` locality,
  over-expanded horizontal frames, and caused sibling/child semantic drift.
- Render, portal protection, critic/provenance/repair helpers, focused tests,
  and golden fixtures were updated together under the same title-span model.

## Scope

**In:** title anchoring rules, title-span protection rules, envelope/padding
model changes, portal-slot protection, layout clearance repercussions, nested
containment repercussions, tests/fixtures/docs.

**Out:** ANSI title styling, general node-label alignment, unrelated TUI work,
new diagram types, or broad style/color work.

## Why This Needs A Real Plan

At planning time the engine assumed:

- title spans are centered inside the subgraph border
- top/bottom/side padding can differ substantially based on title fit, route
  pressure, external crossings, and direction
- `outer` and `inner` bounds serve double duty as both visual padding and route
  reservation bands

That means the requested behavior collides with multiple existing assumptions:

- `src/render/mod.rs`
  `title_span()` and `draw_subgraph_title()` center titles from raw bounds width.
- `src/portals.rs`
  `build_envelope()` computes asymmetric `top_pad`, `bottom_pad`, and
  `side_pad` from route/title heuristics.
- `src/portals.rs`
  `collect_portal_slots()` protects a centered title span and pushes top/bottom
  entry points away from it.
- `src/layout.rs`
  nested keepouts, route-pressure widening, and parent/child clearance logic
  assume the current asymmetric envelope shape and centered title band.
- `src/render/critic.rs`
  title corruption checks implicitly assume current title placement semantics.
- tests and golden fixtures across all four directions currently lock centered
  title output and asymmetric-but-stable envelope behavior.

If we only change title drawing and add some extra padding, we will likely
re-break:

- top-entry portal placement
- nested title stair-stepping
- BT bottom-title rules
- LR/RL local packing
- critic expectations around clean title rows and portal artifacts

## Original State At Planning Time

### Title Placement

- TD/TB/LR/RL titles are centered on the top border row.
- BT titles are centered on the bottom border row from the earlier BT-specific
  contract change.
- Portal/title protection logic uses centered span math in both render and
  portal collection.

### Padding / Envelope Model

- `build_envelope()` in `src/portals.rs` grows each side differently based on:
  - title fit
  - nested-route side budget
  - external incoming/outgoing crossing counts
  - TD/TB vs BT directional needs
  - merge/branch clearance heuristics
- `inner` and `outer` bounds currently encode both:
  - visible decorative padding
  - routing safety/reservation bands

This is efficient for the current heuristics, but it makes "balanced padding"
ambiguous, because some extra rows/columns exist for routing pressure rather
than visual symmetry.

## Desired End State

### Title Contract

Titles anchor to the leading edge of the subgraph according to orientation:

- `TD` / `TB`: top-left anchored
- `LR`: top-left anchored
- `RL`: top-right anchored
- `BT`: bottom-left anchored

This preserves the earlier BT border-row decision while replacing only the
horizontal centering rule.

### Padding Contract

Subgraphs should present visually balanced interior breathing room whenever
possible:

- top/bottom/left/right padding should converge toward symmetry from the
  viewer’s perspective
- mandatory routing/title reservations can still force expansion
- when one side needs more space, the opposite side should usually grow too so
  the subgraph still reads as balanced rather than lopsided

Important nuance: perfect equality is not always possible or even desirable.
The title band and certain route-entry obligations are real asymmetries. The
contract should therefore be:

"Balance visible padding aggressively after satisfying hard routing/title
constraints."

The shipped version makes that concrete as **axis-balanced padding**, not a
single equal pad on all four sides.

## Recommended Model Change

The cleanest way to support this is to stop treating subgraph bounds as a
single fused padding primitive.

Introduce three conceptual layers:

1. **Content box**
   The smallest rectangle that encloses direct node content plus descendant
   child envelopes.
2. **Visual frame box**
   The visible decorative border/title box that users perceive as padding.
3. **Route reservation bands**
   Extra per-side space needed for portal trunks, merge/fanout clearance, title
   keepouts, and nested boundary protection.

Then derive final outer bounds like this:

- compute per-side mandatory reservation demand
- compute a balanced visual padding target
- set each side to `max(mandatory_side_demand, balanced_target_for_that_axis)`

This preserves correctness while making symmetry a first-class layout goal.

## Success Criteria

- [x] Titles are no longer centered in any direction; they follow the new
      anchored contract consistently.
- [x] Balanced padding visibly improves across simple, nested, and
      route-crossing subgraph cases, using axis-balanced targets after hard
      per-side demand.
- [x] Portal slots and route entry/exit logic protect the new anchored title
      spans correctly.
- [x] Nested parent/child title rows still stair-step cleanly after the new
      anchoring.
- [x] LR/RL localized packing remains compact and does not regress into sibling
      drift or false nesting.
- [x] BT bottom-border title behavior remains intact except for the new
      horizontal anchoring.
- [x] Golden fixtures and focused geometry tests cover all four directions.

## Phased Plan

### Phase 1: Define the New Contract Explicitly

- Introduce a single orientation-aware title-anchor helper shared by render,
  portal protection, and layout.
- Replace the implicit centered-title assumption with explicit title-origin
  semantics:
  - title row
  - title start column
  - title protected span
- Decide whether the protected span includes only text padding or also a
  configurable title keepout margin.

**Exit:** no code still depends on "center title from width" as an unstated
assumption.

### Phase 2: Separate Visual Balance From Route Demand

- Refactor envelope construction in `src/portals.rs` so per-side mandatory
  routing/title reservations are computed first.
- Introduce a balanced padding target:
  - one target for horizontal sides
  - one target for vertical sides
- Build `outer` from:
  - content box
  - balanced padding target
  - mandatory side overrides where required

This is the core structural change. Without it, "balanced padding" will stay a
series of one-off heuristics.

**Exit:** envelope generation can explain each side as either balanced padding
or mandatory reservation, not a mixed ad hoc number.

### Phase 3: Rework Portal/Title Protection Around Anchored Spans

- Update `collect_portal_slots()` in `src/portals.rs` to avoid the new anchored
  title spans instead of centered spans.
- Update `adjust_portal_slots_for_title()` in `src/layout.rs` to use the same
  anchored span contract.
- Re-evaluate BT corner/title nudging under the new anchor rules.

**Exit:** portals never "think" the title is centered once the new contract
lands.

### Phase 4: Update Layout Clearance and Nested Rules

- Audit all nested keepouts and width-budget passes in `src/layout.rs` that
  currently assume centered title mass or asymmetrical top/side bias.
- Re-check:
  - declared parent-title keepout rules
  - centered fan-in / inbound fan-out widening
  - LR/RL visual nesting/local compaction
  - BT title-band clearance
- Convert any fixed offsets that were tuned for centered titles into
  orientation-aware anchored-band calculations.

**Exit:** nested and cross-boundary routing still behave coherently under the
new frame geometry.

### Phase 5: Render and Critic Alignment

- Update `src/render/mod.rs` title drawing to use anchored title spans.
- Revisit critic checks in `src/render/critic.rs` that detect title corruption
  or portal misuse near title rows.
- Ensure title restoration/cleanup logic remains valid for BT and non-BT.

**Exit:** render, cleanup, and critic logic agree on the same title geometry.

### Phase 6: Fixture and Audit Expansion

- Add focused unit tests for:
  - anchored title span coordinates
  - balanced padding geometry on simple subgraphs
  - title-span portal avoidance in all directions
  - nested parent/child title stair-stepping after anchoring
- Add or refresh goldens for:
  - simple TD/LR/RL/BT titled subgraphs
  - entry-edge title cases
  - nested title cases
  - dense LR/RL route cases

**Exit:** the new contract is locked by geometry assertions and visible output.

## Likely Touchpoints

- `src/render/mod.rs`
  title-span math, title drawing, BT cleanup assumptions
- `src/portals.rs`
  `build_envelope()`, title fit, per-side padding, portal slot title avoidance
- `src/layout.rs`
  nested keepouts, portal slot adjustment, route-pressure widening, local
  packing, obstacle/gutter assumptions
- `src/render/critic.rs`
  title corruption and portal artifact interpretation
- `tests/default_print.rs`
- `tests/render_options_api.rs`
- `tests/fixtures/expected/*`
- `planning/NESTED_SUBGRAPH_SUPPORT.md`
  if nested containment rules need to absorb the new balanced-frame contract

## Biggest Risks

### 1. Visual symmetry vs routing correctness

If we literally force equal padding on all sides, we can break legitimate route
clearance that currently depends on asymmetric reservations.

**Mitigation:** balance only after mandatory side demand is computed.

### 2. Nested subgraph regressions

Nested work has already accumulated several targeted keepouts and width-budget
passes tuned to current border/title geometry.

**Mitigation:** treat nested cases as first-class acceptance gates, not
post-fix cleanup.

### 3. LR/RL over-expansion

Balanced padding can easily make horizontal layouts feel bloated and less local.

**Mitigation:** keep compaction/locality checks as explicit non-regression
criteria for LR/RL.

### 4. BT contract drift

BT recently got a special bottom-title contract. A generic anchoring rewrite can
accidentally re-center or re-topify BT logic.

**Mitigation:** preserve BT row placement as a hard invariant and only change
its horizontal anchor.

## Resolved Decisions

- "Beginning of the subgraph box" landed as a direction-aware leading edge:
  top-left for `TD` / `TB` / `LR`, top-right for `RL`, and bottom-left for
  `BT`.
- Title keepout uses the shared title span contract across render, portal, and
  layout logic instead of renderer-local centered math.
- Balanced padding is evaluated against final hard routing/title demand, not
  direct content alone.
- `LR` / `RL` do **not** force strict four-side equality. They use the same
  axis-balanced target model, then rely on locality/compaction protections to
  stop visually balanced frames from drifting into false containment.

## Recommended Rollout Strategy

Do not merge this as one giant change.

Recommended sequence:

1. land title-anchor helper + non-render geometry tests
2. refactor envelope construction to expose per-side demand vs balanced target
3. update portal/title protection
4. repair nested and LR/RL layout fallout
5. switch render output and goldens

That keeps the failure surface attributable and makes reversions possible if
balanced padding causes layout blow-ups.

## Verification

- `cargo fmt --check`
- `cargo clippy`
- `cargo test`
- `cargo test --features golden`

## Conclusion

The key design move was the right one:

**separate balanced visual padding from mandatory routing reservation**

That made start-aligned titles and more balanced frames coherent instead of
competing heuristics layered on top of the old centered/asymmetric model. The
important implementation correction was to stop short of one equal pad on all
four sides; axis-balanced padding plus hard per-side demand is the stable
contract that survived `LR`/`RL` locality, nested containment, and the full
golden suite.
