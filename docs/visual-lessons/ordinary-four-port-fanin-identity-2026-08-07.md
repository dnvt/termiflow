# Ordinary four-port fan-in must preserve edge identity

## Evidence

The H31 full-corpus packet reviewed all 237 Mermaid inputs across ASCII and
Unicode styles and default and optimized modes: 936 renderable rows plus 12
expected-error policy rows. In the four `scale_dense_{td,bt,lr,rl}` fixtures,
the four `Output 1`–`Output 4` edges entering `Done` rendered with one shared
target arrowhead in the affected rows. The semantic edge count remained
complete, so semantic completeness alone was not a sufficient visual oracle.

Stable finding: `parallel_fanin_target_arrowhead_collapsed_done`.

## Hypothesis

The ordinary identity lowerer can preserve four distinct incoming edges if
measurement reserves the same separated target span that routing consumes and
the raw-frame evaluator requires one visible target arrowhead per declared
edge. The existing three-port cap is the missing policy boundary.

## Bounded fix

Support exactly two through four incoming edges only when the graph is
rectangular, unlabeled, acyclic, subgraph-free, and uses supported arrow edges.
Keep clone-first route planning and atomic commit. Do not widen this fix to
shapes, labels, portals, subgraphs, or dense crossing ownership.

## Falsifiers and holdouts

- any four-port direction/style/mode loses a target arrowhead or collides with
  a node, border, or sibling route;
- the independent raw-frame oracle disagrees with the renderer policy;
- a two- or three-port homolog regresses;
- subgraph, shape, label, or portal rows change without their own hypothesis;
- the full fresh 948-row packet introduces a new P1/P2 finding outside this
  family.

## Promotion rule

Do not update goldens from this lesson. Promote it only after a fresh full
packet, one-frame review of all 936 renderable rows, separate review of all 12
expected-error rows, and independent holdout checks agree.
