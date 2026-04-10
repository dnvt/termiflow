# Research: Engine Rewrite Evaluation Spike

**Date:** 2026-04-09
**Status:** Complete
**Related Plan:** `planning/ENGINE_REWRITE_EVALUATION.md`
**Related Program:** `planning/RENDERING_PRECISION_PROGRAM.md`
**Decision Context:** `decisions/DEC-003-launch-public-repo-before-crates-io.md`

## Objective

Capture a bounded evidence baseline for whether TermiFlow's current engine has
already hit practical limits that would justify opening a rewrite stream.

This spike is not a rewrite prototype. It is a decision-support checkpoint.

## Scope

**In:** baseline measurements against representative high-pressure fixtures,
manual visual review of the riskiest nested case, and preservation of concrete
prototype boundaries if rewrite triggers are ever reached.

**Out:** shipping engine changes in this branch, integrating external engines,
benchmarking every fixture, or making a rewrite commitment.

## Commands Run

```bash
cargo build
./target/debug/termiflow --print --style unicode --audit tests/fixtures/inputs/subgraph_complex_td.md >/tmp/subgraph_complex_td.out
./target/debug/termiflow --print --style unicode --audit tests/fixtures/inputs/subgraph_complex_bt.md >/tmp/subgraph_complex_bt.out
./target/debug/termiflow --print --style unicode --audit tests/fixtures/inputs/collision_sibling_subgraphs_td.md >/tmp/collision_sibling_subgraphs_td.out
./target/debug/termiflow --print --style unicode --audit tests/fixtures/inputs/converge_cascade_td.md >/tmp/converge_cascade_td.out
./target/debug/termiflow --print --style unicode --audit tests/fixtures/inputs/unicode_emoji_td.md >/tmp/unicode_emoji_td.out
./target/debug/termiflow --print --style unicode --audit tests/fixtures/inputs/unicode_cjk_td.md >/tmp/unicode_cjk_td.out
./target/debug/termiflow --print --style unicode --audit tests/fixtures/inputs/subgraph_nested_td.md >/tmp/subgraph_nested_td.out
wc -L /tmp/*.out
wc -l /tmp/*.out
```

## Baseline Results

| Fixture | Concern | Audit | Size | Notes |
|---------|---------|-------|------|-------|
| `subgraph_complex_td` | nested, route-dense TD | Clean | `162 x 37` | no critic findings |
| `subgraph_complex_bt` | reverse-flow nested fan-in | Clean | `138 x 39` | no critic findings |
| `collision_sibling_subgraphs_td` | sibling containment / border pressure | Clean | `102 x 26` | no critic findings |
| `converge_cascade_td` | dense fan-in | Clean | `108 x 15` | no critic findings |
| `unicode_emoji_td` | emoji width pressure | Clean | `48 x 13` | still terminal-width dependent |
| `unicode_cjk_td` | CJK width pressure | Clean | `36 x 13` | still terminal-width dependent |
| `subgraph_nested_td` | true nested hierarchy | Clean audit + parser warning | `76 x 18` | hierarchy preserved, still visually rough |

## Visual Read

The current engine is materially stronger than the original rewrite-evaluation
plan assumed.

What held up well:
- Dense compound routing fixtures now audit clean.
- Sibling-subgraph containment is credible on the representative TD case.
- Unicode-heavy fixtures do not currently expose a clear rewrite trigger.

What still stands out:
- True nested hierarchy remains the main architectural warning sign.
- `subgraph_nested_td` still emits the experimental nested-subgraph parser
  warning.
- The nested case is no longer a collapse/failure, but it is also not yet the
  kind of clean, general layout that would let us declare the problem solved.

## Interpretation

This spike does not support opening a rewrite stream now.

The evidence says the current renderer still has meaningful headroom, and the
best near-term return remains the incremental rendering-precision path. A
rewrite would currently be solving a narrower problem than the earlier plan
assumed: not general routing failure, but the residual complexity of fully
credible nested compound layout.

## Prototype Boundaries Worth Preserving

### Partial Candidate

**Name:** compound lane-budget planner

Keep the existing parser, graph model, renderer, CLI, and TUI. Prototype only
layout-side budgeting for nested containers, portal demand, and lane spacing.

Pursue this only if future trigger fixtures show that the current layout model
cannot absorb that pressure cleanly.

### External Candidate

**Name:** ELK-backed offline oracle / comparison adapter

Do not ship an external runtime dependency. Use an external engine only as an
offline comparison oracle to test whether hierarchy/ranking guidance is
meaningfully better on future trigger fixtures.

Pursue this only if the current engine clearly flattens out and the adapter can
stay packaging-neutral.

## Recommendation

Keep `planning/ENGINE_REWRITE_EVALUATION.md` as a draft backlog evaluation.
Route active effort into `planning/RENDERING_PRECISION_PROGRAM.md` until one of
the rewrite triggers is actually met.

Do not escalate from evaluation to rewrite unless future evidence shows at
least one of these:
- nested compound layout quality stalls after bounded model work
- preview/render exactness remains materially divergent after metric unification
- repair/scoring complexity becomes disproportionately high for the quality won
- post-beta demand requires capabilities that the current architecture cannot
  absorb without compounding debt
