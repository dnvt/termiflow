# BT parallel title gutter: spacing-only repair is unsafe

## Observation

The direct three-edge BT source-subgraph-to-target-subgraph family can be
machine-clean while still reading mechanically at the target title/border
seam. A one-cell horizontal gutter moved the first portal, but the existing
route had to jog back to the target node center. That produced adjacent
`++`/`└┐` corner pairs; moving the turn higher damaged the first arrow region.

## Lesson

Do not repair a BT title seam by shifting a portal column unless the route
planner proves a separate interior turn corridor and preserves the target
arrow. A portal slot, route entry, title-row restoration, and final projection
must be reviewed as one ownership transaction.

## Reviewer rule

Machine raw/critic/geometry cleanliness is not perceptual approval. Review the
title row, the border row, the first lane, and every tiny corner/junction cell
before accepting a spacing hypothesis. Keep the finding open if the output
looks like a border seam or punctuation even when all declared arrows exist.

## Epoch rule

The protected primary checkout and the audit checkout currently render this
family differently (`│` versus `┼`, and different title-row continuity). Never
carry a frame decision across those source epochs. Bind every future decision
to fixture, style, mode, binary/source identity, frame hash, evidence hash,
and effective policy hash.

## Next hypothesis

Investigate preservation of the authoritative selected BT portal through title
wrapper padding after title restoration, without moving lanes or changing
border glyph policy. Falsify it if it touches title text, changes route
geometry, creates a corner pair, or cannot be shared across the authorized
renderer epoch.
