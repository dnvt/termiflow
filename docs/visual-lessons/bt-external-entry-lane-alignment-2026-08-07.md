# Visual lesson: BT external entries must share lane ownership

Date: 2026-08-07
Source epoch: H18

## Observation

The `collision_edge_along_border_bt` family contains three external source
boxes entering three direct children of one titled BT subgraph. The first
implementation moved each source to its target center. Machine geometry stayed
clean, but the first target center was inside the rendered title span. The
route planner correctly moved that portal to a title-safe lane, leaving a
visible exterior elbow immediately below the subgraph border.

The human eye saw that elbow as a layout defect even though the node count,
arrow count, semantic ownership, and critic score all passed.

## Durable rule

For a topology-proven titled BT multi-entry scene:

1. derive candidate lanes from the live envelope and title-safe portal policy;
2. choose lanes source-first, target-second, with a deterministic tie-break;
3. stage external source translations on those lanes only when node/envelope/
   canvas keepouts remain clear;
4. make the renderer use the same source-first tie-break;
5. place the interior fan-in branch row one clear row above the title row; and
6. leave unsupported or unsafe scenes to the generic fallback unchanged.

The route planner and placement planner must not independently optimize source
and target centers. A local target-center heuristic can be geometrically valid
and still disagree with the actual title-safe lane owner.

## Verification

- Focused changed homologs: ASCII/Unicode × default/optimized, four fresh
  perceptual passes.
- Full visual packet: 237 inputs, 936 primary rows, 12 expected-error rows,
  948 total rows; all structurally valid.
- Full perceptual ledger: 936/936, with the existing watch backlog preserved.
- Expected-error ledger: 12/12 separately validated.
- `junction-quad` holdout: 16/16 perceptual passes.
- Independent oracles: 20/20 passed.
- Goldens were not approved; the existing 333-snapshot delta remains explicit.

## Next use

When a future frame reports `bt_title_boundary_hook_requires_human_review`,
inspect whether the horizontal segment is an intentional interior branch row
with a clear title gap or an exterior border elbow. Do not close the finding
from arrow counts alone, and do not globally suppress the route-clarity
warning without a fresh one-frame decision.
