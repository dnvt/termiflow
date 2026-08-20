# Vertical mixed edge kinds need target-facing shaft headroom

Status: promoted focused rule; complete-corpus perceptual closure and golden
approval remain open.

## Observation

In `tests/fixtures/inputs/edge_kinds_bt.md`, the compact vertical fan-out
placed the shared junction immediately beside the target arrow row. Thick and
Dotted branches therefore had no writable target-facing shaft cell and
collapsed into the ordinary `─` rail in Unicode. Circle and cross endpoint
markers remained distinct, which made the partial loss of edge-kind identity
especially easy to miss in a machine-clean frame. The same topology risked the
TD homolog.

## Hypothesis and bounded fix

The vertical fan-out planner was correct about the shared junction but the
layout policy reserved only the ordinary fan-out gap. A topology-gated extra
primary gap for a source with multiple outgoing Thick/Dotted edges, combined
with a blank-corridor junction retreat when safe, gives each branch a writable
target-facing cell. Unicode then preserves `┃` and `╎`; ASCII keeps its
documented vertical Thick `|` fallback while retaining Dotted `:`.

The rule is limited to vertical mixed-kind fan-outs. Ordinary fan-outs and
horizontal mixed-kind routes retain their existing spacing and route owners.

## Falsifiers and regression

The hypothesis is falsified by a missing `┃`/`╎` shaft, a lost endpoint or
label, a node/border collision, a changed ASCII fallback without an explicit
contract decision, or a less readable TD/BT homolog. Focused coverage is:

```text
cargo test --locked --all-features --test independent_oracles mixed_edge_kind -- --nocapture
cargo test --locked --all-features --test render_options_api feedback::render_with_feedback_keeps_vertical_dual_junction_fanout_shafts_visible -- --nocapture
```

The independent oracle covers TD/BT, ASCII/Unicode, and default/optimized
modes. A fresh full canonical and authored packet review is still required
before promoting the changed snapshots or closing the release gate.

## Public rule

When a vertical fan-out contains multiple outgoing Thick/Dotted edges, reserve
enough primary headroom for a target-facing branch shaft before lowering the
shared junction. Preserve kind-specific shaft glyphs through the final route
ownership pass, and treat ASCII fallback differences as an explicit style
contract rather than silently approving a visual collapse.
