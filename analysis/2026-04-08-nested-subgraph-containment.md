# Thinking Capture — Nested Subgraph Containment

**Date:** 2026-04-08
**Mode:** adaptive (`/maestro:think --deep`)
**Duration:** ~20 minutes

## Topic

The rendered graph looks visually wrong because the user expects true nested
containers:

- `Order Service` should sit fully inside `Data Layer`
- `Data Layer` should sit fully inside `Service Layer`

Today that expectation fails for a stronger reason than local border spacing:
TermiFlow does not fully support nested subgraphs yet. At the time of this
thinking pass, the parser warned and effectively flattened inner subgraphs into
the outer one.

## Angles Explored

### 1. User-Visible / Diagram Semantics

From the user's point of view, the current output is misleading, not merely
ugly. The graph author asked for nested modules, but the renderer silently
flattens the hierarchy into a single visible container. That produces exactly
the visual artifact they called out: a node appears to "eat" a child border
because the child border was never really modeled.

Implication:
- If output implies support before containment is real, users will keep reading
  overlap artifacts as renderer bugs instead of unsupported semantics.
- If we want the rendered result to match user intent, we need real nested
  subgraph support.

### 2. Rendering / Layout Architecture

The problem is upstream of border drawing. Current envelope expansion in
`src/portals.rs` handles "visually nested" sibling overlap heuristics, but it
still assumes a flat subgraph set. That works for portal cleanup and some
cross-subgraph cases, but it is not a substitute for a parent/child subgraph
tree.

Implication:
- Patching `compute_envelopes()` alone will not fully solve this class of issue.
- True containment requires hierarchical ownership through parse, layout,
  envelope construction, portal routing, and render.

### 3. Product / Scope Discipline

Nested subgraphs are already documented as unsupported in `README.md`,
`docs/reference.md`, and `planning/PLAN.md`. That means the immediate user pain
is valid, but it is not a hidden regression against promised behavior.

Implication:
- This should not displace current pre-OSS hardening by accident.
- The right next step is a scoped backlog plan, not an ad hoc rendering patch.

## Options Considered

### Option A: Patch Current Overlap Heuristics

Teach the existing envelope logic to expand parent borders more aggressively
when subgraph rectangles overlap.

Pros:
- smaller short-term code change
- may improve some visually nested cases

Cons:
- does not change parse semantics
- still flattens nested Mermaid syntax
- high risk of more edge-case heuristics with no clean model underneath

### Option B: Implement True Nested Subgraph Support

Model subgraphs as a tree, preserve parent/child relationships from the parser,
then make layout, portals, and render respect ancestor containment.

Pros:
- matches user intent and Mermaid semantics
- solves the actual cause of the artifact
- creates a clean base for future subgraph features

Cons:
- materially larger change
- touches parser, graph model, layout, routing, render, tests, and docs

### Option C: Keep It Unsupported but Fail More Clearly

Retain the current no-nesting scope, but make nested subgraphs a stronger
warning or strict-mode error and avoid rendering output that looks partially
supported.

Pros:
- cheapest honest behavior
- reduces user confusion

Cons:
- does not satisfy the requested visual behavior
- keeps a high-value Mermaid gap open

## Key Insights

1. This is primarily a feature-gap problem, not a border-polish problem.
2. The current "visually nested" overlap logic is useful, but only as a helper
   inside a true hierarchy, not as a replacement for one.
3. The best long-term answer is Option B. The best short-term guardrail is to
   consider part of Option C so unsupported nesting fails more honestly.

## Assumptions Surfaced

- We assume the desired behavior is Mermaid-like nested containment, not a
  one-off custom layout rule.
- We assume all four directions (`TD`, `LR`, `BT`, `RL`) should behave
  consistently once nesting is supported.
- We assume parent/child containment must hold even when cross-subgraph edges
  create competing portal demands.

## Open Questions

- Should the first implementation support arbitrary nesting depth, or exactly
  one additional level beyond today's flat model?
- Do we want a short-term UX safeguard before full support, such as turning
  nested subgraphs into stricter failures?
- Should nested subgraph support remain post-1.0 backlog, or is user demand
  high enough to pull it earlier?

## Next Step

`/maestro:plan` completed in parallel as a draft backlog plan:

- [`planning/NESTED_SUBGRAPH_SUPPORT.md`](../planning/NESTED_SUBGRAPH_SUPPORT.md)

## Follow-Up Note

Later execution proved the core diagnosis correct. Phase 1 and Phase 2 landed,
and Phase 3 started:

- nested parser/model hierarchy is now preserved
- bare node-reference lines inside subgraphs now correctly attach membership
- ancestor-aware portal-slot selection and layout clearance are in progress

The remaining visible defect is narrower than the original diagnosis: root-level
headroom for titled parents still needs to be reserved so a parent border can
sit visibly above its topmost nested child.
