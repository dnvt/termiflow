# Visual lesson: quiet BT sibling corridors can hide tiny shoulders

Status: H133 source-node repair retained; shared sibling-rail renderer watch remains open

## Observation

The complex `subgraph_complex_bt` scene contains two vertically stacked titled
siblings with a long empty corridor between the Data Layer and Service Layer
borders. A bounded source-side alignment experiment made the API → source
receiver shaft straighter and removed the lower title elbow. Fresh Unicode and
ASCII frames nevertheless introduced an adjacent `┌┘`/`++` pair immediately
below the Data Layer border. The route was connected and the machine
route-clarity report was clean, but the tiny shoulder made the corridor look
like a damaged border hook.

This is a human-eye defect, not a missing-edge or endpoint-identity defect.
The candidate was rejected and fully rolled back. The target-only receiver
repair remains the active renderer baseline; its lower Service Layer seam and
shared rail stay explicitly open as P2 watches.

## Hypothesis and falsifier

Hypothesis: source-side BT title-portal lowering and border ownership were
selecting independent lanes; aligning the source receiver and entry corridor
to one lane would remove the lower seam without moving the ambiguity.

Falsifier: any of the four `subgraph_complex_bt` homologs—ASCII/Unicode ×
default/optimized—contains a new adjacent corner shoulder in the quiet
inter-subgraph corridor, even if all route and geometry checks remain green.
The source-side candidate met that falsifier in every homolog and was not
promoted to the renderer, golden snapshots, or release history.

## Reviewer-loop improvement

The independent route-clarity critic now preserves this class of evidence with
`bt_quiet_corridor_shoulder_requires_human_review`. It is a conservative P2
queue signal for adjacent `┌┘`/`└┐`/`╔╝`/`╚╗` or ASCII `++` pairs with vertical
route context in the empty corridor between vertically stacked titled BT
siblings. It does not approve or reject the frame; the AI/human one-frame
review still records the observation, ownership hypothesis, falsifier, and
homolog scope.

The focused regression is
`bt_quiet_corridor_review_queues_adjacent_shoulders` in
`src/qa/route_clarity.rs`. The upgraded full audit remains structurally
complete at 241 inputs / 964 rows per lane, with 952 renderable rows, 12
expected-error rows, and zero audit failures. Those numbers establish packet
coverage only; they do not close the 952-row perceptual queue.

## H133 narrow source-node experiment

The next topology-owned hypothesis moved only the external API node onto the
existing S1 receiver lane. It left both titled sibling groups and all Data
Layer nodes fixed, so it could address the lower API → S1 title hook without
repeating H127's source-group perturbation. The structural selector derives
the source receiver from the unique external entry edge and fails closed on
collision, clipping, envelope intrusion, or an unsupported topology.

Fresh H133 packets covered the complete 241-input corpus in both policy lanes
(964 rows per lane; 952 renderable and 12 expected-error rows), with strict
validation passing and zero packet findings. The eight changed complex-BT
frames—ASCII/Unicode × default/optimized in canonical/requested-style and
authored/no-style-override lanes—were each inspected one frame at a time.
Every frame shows the lower API → S1 hook removed and no new quiet-corridor
`┌┘`/`++` shoulder. The existing shared sibling rail remains an explicit P2
`bt_sibling_boundary_rail_requires_human_review` watch, so this is a retained
focused repair, not a claim that the scene is perfect or that the 952-row
fresh perceptual queue is closed.

## Required continuation

1. Keep the H126 target-only repair, the retained H133 source-node repair, and
   the H127 source-group falsification as separate hash-bound evidence.
2. Run the next bounded source/border-ownership experiment only with a
   precommitted check for both the lower title seam and the first quiet
   corridor row, in all four style/mode homologs and both policy lanes.
3. Freshly inspect changed rows first, then drain every 952-row perceptual
   queue before changing goldens or resolving the P2 watch.
4. Do not treat route-clarity cleanliness, critic cleanliness, or a complete
   packet as human-eye approval.
