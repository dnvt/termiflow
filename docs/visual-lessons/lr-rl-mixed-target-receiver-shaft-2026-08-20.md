# LR/RL mixed targets need a quiet receiver shaft

Status: bounded fix under watch; focused homolog coverage is recorded, but
complete-corpus perceptual closure and the long-rail ownership watch remain
open.

## Observation

In `tests/fixtures/inputs/collision_sibling_subgraphs_lr.md` and its reverse
direction counterpart, a cross-group edge entered a receiver immediately
after a turn. The ASCII shoulder rendered as `+>+` or `+<+`, and the Unicode
equivalent compressed into `┌→┤` or `├←┐`. The graph topology was present, but
the arrowhead visually fused with a box corner or a sibling border seam. The
same local artifact appeared in canonical requested-style and authored
no-override lanes across default and optimized modes.

## Hypothesis and bounded fix

The typed LR/RL sibling-target router placed the source bend, target portal
turn, and internal receiver turn one oriented cell from their owners. That
left no quiet shaft cell between a route turn and the arrowhead. Increasing
the source, target-portal, and internal-target turn clearances to two oriented
cells is the smallest topology-owned change for this exact two-subgraph
scene. It preserves the existing boundary claims and does not alter generic
edge fallback routing.

## Falsifiers and regression

The hypothesis is falsified if any changed homolog recreates the compact
shoulder, loses an arrowhead, clips a label, introduces an untraced fallback,
or makes the long cross-group rail harder to follow. The focused coverage is:

```text
cargo test --locked --features qa --test subgraph_boundary_arrows horizontal_mixed_target -- --nocapture
cargo test --locked --features qa --test lr_rl_sibling_chain_visual -- --nocapture
```

Fresh canonical and authored packet reviews cover the exact LR/RL scene plus
chain and triple holdouts. The local shoulder is improved in all inspected
rows; the long rail crossing sibling borders remains a P2 watch until a
broader topology-owned policy either makes ownership immediate or documents
it as an intentional boundary convention.

## Public rule

For the typed flat LR/RL mixed-target scene, keep one quiet oriented shaft
cell between each source/portal turn and its receiver arrowhead. Gate the
clearance by the exact topology and retain a separate human-eye review for
the global cross-group rail; machine traceability alone cannot approve the
visual result.
