# Visual lesson: collinear LR/RL sibling bridges need distinct corridors

Status: Promoted focused rule; full-corpus closure and inherited seam watches
remain open

## Observation

In `collision_sibling_triple_lr` and its mirrored
`collision_sibling_triple_rl` homolog, each cross-group edge leaves and enters
on the same node center row. The initial scene-owned repair kept those edges
collinear, which made the internal sibling edges and cross-group transitions
read as one uninterrupted bus through every titled seam. A second experiment
used distinct lower rows but left only the ordinary inter-group gap; ASCII
rendered a short `+------+` bridge and Unicode rendered the equivalent compact
corner pair, which a human eye could mistake for a tiny second box. The
defect is perceptual transition ownership, not connectivity or arrowhead loss.

## Hypothesis and bounded fix

The LR/RL sibling-chain lowerer should reserve distinct lower quiet-corridor
rows for sibling transitions, even when their endpoints are collinear. The
layout contract must also preserve a longer lateral inter-subgraph bridge for
this exact topology. H114 showed that the previous eight-cell minimum still
looked like a detached box in the three-stage `subgraph_chain` homologs, so
the promoted contract now reserves a sixteen-cell minimum. The resulting
corners are separated from node borders by an empty row and the bridge is long
enough to read as a deliberate transition, not a miniature box. Both layout
and rendering consume the same topology gate; ordinary horizontal diagrams
keep their existing compact spacing.

The focused regression in `tests/lr_rl_sibling_chain_visual.rs` covers both
mirrors, the collision and three-stage chain homologs, ASCII/Unicode, and
default/optimized modes, including the promoted minimum visible bridge
length. The maintainer-feature unit tests verify scene gating, distinct
corridor selection, route transactionality, and negative controls. The broader
render-options and subgraph-boundary suites remain required gates.

## H114 falsified boundary-turn experiment

A follow-up moved the vertical turns from one cell inside each titled group to
the adjacent inter-group cells and retained the center-row portal as an
explicit attachment. The route stayed connected and its focused tests passed,
but fresh ASCII/Unicode frames still showed doubled border rails and compact
corner pairs at both seams. That candidate was reverted. The surviving rule is
the lower distinct-corridor allocation plus the layout-owned lateral bridge
gap; another boundary-coordinate nudge must not be retried without a new
falsifiable visual prediction.

## H117 falsified quiet-band-spacing experiment

The next bounded candidate expanded the strict LR/RL envelope and separated
the two lower corridor rows with an additional blank row. Its focused
ASCII/Unicode, default/optimized matrix stayed structurally clean and the
route remained transactional, but direct frames still showed each seam as a
U-shaped mini-border: the extra vertical spacing separated the two fragments
without changing their ownership shape. The candidate was reverted and must
not be retried as another spacing-only adjustment.

This negative result narrows the next hypothesis. The fix must change the
route ownership geometry or the layout boundary that owns the turn, rather
than only adding rows below the same source/target elbows. Keep the LR/RL
`subgraph_chain` and collision homolog watches open until a candidate removes
the mini-border impression in both directions and preserves the existing
portal, arrow, and negative-control contracts.

## Falsifiers and next loop

The rule is falsified by a broken node border, a portal rendered as a generic
junction, a missing arrow shaft, a short box-like bridge, merged
middle-sibling ownership, or a regression in a non-collinear, nested, labeled,
BT, TD, or TB homolog. The next step is a fresh complete packet in both
canonical requested-style and authored no-override lanes, followed by
independent one-frame review of all 952 renderable rows per lane and the
separate 12-row expected-error ledgers.

## Public boundary

This lesson contains visible observations, code ownership, tests, and
falsifiers only. Private Maestro capsules, prompts, provider traces, and
transient packet paths remain outside the OSS repository.
