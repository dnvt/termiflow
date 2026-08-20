# Dense vertical crossings need a quiet target shaft

Status: promoted focused rule; complete-corpus closure and inherited crossing
watches remain open.

## Observation

The dense crossing-grid fixtures
`tests/fixtures/inputs/crossing_grid_td.md` and
`tests/fixtures/inputs/crossing_grid_bt.md` contain six dedicated crossing
lanes between each rank pair. Before this repair, the final lane in the
minimum accepted rank gap ended on the row immediately beside its target
arrowhead. In terminal output that composed as `└─┐↓`/`+-+v` in TD and the
mirrored `┌─┐↑`/`+-+^` family in BT. The graph was connected and explicit
crossing markers were present, but the tiny corner read like damaged border
punctuation to a human eye.

The current review denominator is 241 inputs, 964 rows per policy lane, 952
renderable frames, and 12 separately ledgered expected-error rows.

## Hypothesis and falsified candidate

The defect belongs to the dense vertical lane allocator: with a two-cell lane
pitch, offsets `2,4,6,...` put the last lane at `target_entry - 1` in the
minimum corridor. A candidate that widened the source-side ports to match the
target-side ports was falsified because it removed the explicit crossing
markers and failed the independent route-identity oracle. Port ownership must
remain unchanged.

## Fix and falsifier

For TD/TB/BT dense crossing scenes only, the lane band now uses the same
two-cell pitch with offsets `1,3,5,...`. This leaves a straight target-facing
shaft cell before every arrowhead while preserving dedicated source/target
ports and explicit crossings. LR/RL keep their prior side-port lane spacing;
the vertical repair must not export into horizontal composition.

The hypothesis is falsified if a supported vertical crossing row loses an
arrow or `x`/`✕` marker, fails the route-identity or explicit-marker oracle,
puts a corner directly beside a terminal arrowhead again, or changes an
unrelated LR/RL homolog.

## Regression

Focused coverage includes:

```text
cargo test --locked --features "qa maintainer-fixtures" --lib render::edge::dense_crossing::tests -- --nocapture
cargo test --locked --features qa --test render_options_api direction_matrix::vertical_crossing_grid_terminal_heads_keep_a_straight_shaft_cell -- --nocapture
cargo test --locked --features qa --test independent_oracles -- --nocapture
```

The focused render regression covers TD and BT, ASCII and Unicode, and
default and optimized modes. The independent oracle keeps the source/target
ports, route identity, and explicit crossing markers honest. Fresh canonical
and authored full packets are still required before resolving inherited
human-eye watches or changing goldens.

## Public rule

In a dense vertical crossing scene, a terminal arrowhead needs one visibly
straight shaft cell on its target-facing side. Allocate that clearance in the
topology-owned lane band; do not widen or reassign ports merely to hide the
corner. Keep the rule scoped to proven dense TD/TB/BT crossings and inspect
LR/RL, authored style provenance, holdouts, and the complete corpus separately.
