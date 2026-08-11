# Visual lesson: TD mixed target-entry identity

Date: 2026-08-09
Source packet: `/tmp/termiflow-h78-mixed-target-r1-packet`
Review ledger: `/tmp/termiflow-h78-mixed-target-r1-review.jsonl`
Expected-error ledger: `/tmp/termiflow-h78-expected-error-policy.jsonl`

## Corpus scope and validation

The visual self-improvement loop includes every existing schema under
`tests/fixtures/inputs`, not only the fixture that motivated this repair:

- 237 input schemas expanded into ASCII/Unicode × default/optimized;
- 948 total packet rows;
- 936 renderable rows in the perceptual review denominator;
- 12 intentional parser-error rows reviewed separately by the expected-error
  policy ledger;
- exact unchanged-row rebinds plus direct human-eye review for every changed
  row;
- current renderable decisions: 608 pass, 323 watch, 5 fail, 0 unclear;
- current severity inventory: 176 P2 and 760 P3.

The H78 packet is machine-complete: packet digest
`db8ae8e9675feb0564f5db6a9c38909574a0c6c669dc24454612303d05d9be7d`, manifest
digest `9507225b69b89c28f0352c3af14bacfc21335d31784abe8ff1269652ef8c1831`,
and the non-strict visual validator reports 948 rows with zero findings.
Machine-clean is not visual approval; the five fail decisions and every watch
decision remain explicit evidence for the next loop.

## Changed-row human-eye assessment

Only the four homologous rows for `collision_sibling_subgraphs_td` changed:

- ASCII/default;
- ASCII/optimized;
- Unicode/default;
- Unicode/optimized.

All other renderable rows were re-bound only when frame, evidence, and policy
identity matched the prior packet. The four changed frames preserve the two
group envelopes, all labels, four edge arrowheads, and two visually distinct
entries into `Node D`. The target border lanes remain readable, and Unicode
corner glyphs preserve the same topology as ASCII. The conservative decision
for all four rows is P2 `watch`: the portal rails are now traceable but remain
visually close enough that a human can still read them as one shared corridor
at a glance. These rows are not golden-approved.

## Repair hypothesis and falsifier

Hypothesis: generic convergence is erasing target-entry identity when a target
subgraph receives one internal edge and multiple cross-subgraph edges. A typed
TD scene transaction needs separate target ports and non-overlapping cross
corridors, while refusing to claim unrelated cells or silently falling back to
a misleading shared trunk.

Falsifier: if a structurally exact TD scene with distinct target ports and
transactionally separated corridors still reads as one ambiguous target entry,
the remaining defect is in portal compositing/glyph precedence or in the
human-eye oracle, not in target-port allocation. The next experiment must then
measure rendered portal identity and compositor ownership directly.

## H78 implementation result

The new selector/lowerer is deliberately narrow and fail-closed. It recognizes
only the exact flat two-sibling TD topology, allocates two target ports, routes
the internal target edge and both cross-subgraph entries through distinct
corridors, and claims all four scene edges transactionally. LR/RL/BT homologs
remain holdouts; they were included in the complete packet and were not changed
by this slice.

Focused tests cover ASCII/Unicode × default/optimized and assert four raw
arrowheads, no shaftless arrows, clean geometry/ownership evidence, and no
critic findings. The full packet confirms that only the four intended rows
changed. No expected-error row was included in the renderable review count.

## Next self-improvement loop

The next loop must preserve the 948-row replay denominator, the separate 12-row
error ledger, and one-frame human-eye review of every changed and warning row.
It should compare TD against LR/RL/BT homologs and test whether a typed portal
identity record can make the two target entries unmistakable without adding
labels or creating border hooks. Promotion to a golden requires agreement
between raw semantic/geometry oracles, ownership and route evidence, the
machine critic, and perceptual review; a zero-finding prescreen alone is not
enough.
