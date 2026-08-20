# Database fan-out tees need a quiet source stem

## Observation

The strict three-node database scene has two semantic paths from the source:
one through Redis and one directly to PostgreSQL. In LR/RL renders, a one-cell
source split appeared as a tiny `+-+`/`┬` hook. The routes were machine-clean,
but the direct path and the intermediate path required tracing around the
source corner.

## Hypothesis

The database scene planner shared only one primary-axis cell before turning
the bypass lane. A longer source-owned prefix should make the tee read as a
deliberate branch while preserving the two target ports.

## Repair and falsifier

`src/render/edge/database_fan_in.rs` now reserves two quiet source-stem cells
before the LR/RL bypass turns. The strict vertical source→cache→database
scene also branches one primary cell after the source exit, while
`src/layout/placement.rs` reserves one additional vertical rank cell so that
tee does not occupy the intermediate database's receiver shaft. The focused
unit regressions
`horizontal_database_bypass_reserves_a_two_cell_source_tee_stem` and
`strict_database_scene_headroom_gives_the_source_tee_a_quiet_stem` cover the
two ownership contracts; the independent database entry oracles continue to
check all direction/style/mode combinations.

## Promoted rule

A source-owned fan-out tee needs visible stem clearance before its first turn;
one-cell hooks are not sufficient evidence of readable route ownership. A
vertical tee must also leave the intermediate receiver's shaft row clear. The
target-side direct/intermediate route identity remains an open P2 watch until
the complete database homolog review confirms it is immediately legible.

## Follow-up

Re-review all database directions, styles, and optimization modes in both
canonical and authored lanes. Keep the broader corpus pass open for other
source tees, portal crossings, and title-adjacent turns that may share the
same human-eye failure mode.
