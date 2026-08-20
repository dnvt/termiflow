# Visual lesson: opposite corridor bands can become border-shaped

Status: Falsified bounded LR/RL candidate; existing lower-band rule remains
under watch

## H119 falsified alternating-band experiment

The fresh RL homologs showed that successive sibling transitions placed in the
same lower quiet band can read as one long border-like corridor. A bounded
candidate changed `src/render/edge/lr_rl_sibling_chain.rs` so successive
transitions alternated between lower and upper quiet bands.

The focused LR/RL visual test and the BT sibling-chain regression passed, but
direct Unicode and ASCII frames were worse: the upper bridge sat above the
node row and formed a second horizontal border-shaped rail between titled
groups. Moving a transition into the opposite band changed its location, not
its ownership grammar. The candidate was reverted and was not promoted to a
packet, golden, or release rule.

## Next hypothesis and falsifiers

The next LR/RL repair must preserve one readable corridor band while changing
the local seam/portal ownership shape—possibly by an explicit inter-group
transition channel or route-owned boundary marker. It is falsified by a
bridge that reads as a container border, a shared bus, a compact mini-box, a
lost arrow, or a regression in the LR mirror, non-chain horizontal controls,
or vertical directions.

## Public boundary

This lesson records the visible result, the source owner, the focused test
outcome, and the next falsifier. Private Maestro capsules, provider traces,
prompts, and transient packet paths remain outside the OSS repository.
