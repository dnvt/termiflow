# Visual lesson: strict subgraph fan-in target-entry identity

## Observation

The existing fixture corpus must remain the visual acceptance surface. The
fresh H32 packet exercised all 237 inputs across the four directions, ASCII
and Unicode styles, and default and optimized modes: 948 rows total, with 936
renderable rows and 12 expected-error policy rows. The bounded strict family
`subgraph_fanin_{td,bt,lr,rl}` covered all 16 homologs.

Before the fix, the strict subgraph fan-in scene preserved three source-side
boundary lanes but collapsed the exterior route into one target entry. The
first attempted repair then exposed two final-frame defects: inferred graph
provenance overwrote one explicit target-arrow owner, and generic LR/RL portal
cleanup added wall markers beside scene-owned lanes. The LR/RL orientation
helper also selected the opposite physical corner arms.

## Hypothesis and fix

For a proof-gated strict scene, the route contract must own the complete
source→portal→target path, including each target port and arrow owner. The
final frame must preserve that explicit contract after inferred provenance,
portal projection, and border cleanup.

The implementation therefore:

- reserves the target span and primary corridor through the shared strict
  subgraph fan-in policy;
- plans and validates one collision-free Manhattan route per declared edge;
- reapplies explicit edge metadata after inferred route annotation;
- corrects LR/RL physical corner selection and protects straight middle lanes;
- suppresses generic side-portal inference for a boundary already owned by a
  scene route contract; and
- verifies target-port placement and edge ownership with an independent
  oracle.

## Evidence

- Focused independent oracle: `strict_subgraph_fan_in_preserves_boundary_lanes_and_target_ports` passes.
- Strict selector and orientation unit tests pass.
- Fresh packet: `/tmp/termiflow-h32-final`.
- Packet result: 948/948 rows generated with no packet failures; the four
  strict directions are critic-clean in the reviewed Unicode/default rows,
  and their route-clarity reports are clean.

## Remaining queue

The packet's strict quality validator reports a `cargo_lock_sha256` mismatch
against the checked-in quality baseline. That mismatch belongs to the broader
absolute-latest Rust/dependency source-epoch update and must be reconciled
there; this lesson does not approve a golden or quality-baseline change.

Future cycles must continue to review every existing input, not only the
focused family, and must repeat the homolog review after any dependency or
renderer source-epoch change.
