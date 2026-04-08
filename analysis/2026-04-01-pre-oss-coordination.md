# Thinking Capture — Pre-OSS Coordination

**Date:** 2026-04-01
**Mode:** adaptive / deep
**Duration:** ~25 minutes

## Key Ideas

1. TermiFlow is already past the "is there a product here?" threshold. The
   current reality is a working flowchart renderer with watch mode, partial TUI,
   critic/repair infrastructure, 299 passing tests, and a coherent CLI/library
   core.
2. The main risk is not missing headline functionality. The main risk is
   coordination drift: shipped features, unresolved rendering defects, stale
   deeper plans, and OSS readiness work are not yet sequenced under one frame.
3. The work should be split into four lanes:
   stabilize what already ships, align docs/plans with reality, execute the OSS
   hardening sprint, and keep broader roadmap items explicitly parked.
4. OSS should not wait for full Mermaid parity. It should wait for a narrower
   gate: no known high-severity visual regressions in the public beta path, docs
   that match reality, baseline repo hygiene, and a clean package/repo story.
5. The right coordination principle is "protect the narrow product." That means
   fixing defects and trust gaps around the existing wedge before expanding
   feature scope again.

## Assumptions Surfaced

- The first OSS goal is a credible public beta, not a 1.0 launch.
- The most valuable early audience is likely CLI users first, library
  integrators second.
- Known high-severity rendering regressions matter more to launch trust than
  additional Mermaid syntax wins.
- Deferred items like full styling, nested subgraphs, and new diagram types are
  real roadmap work, but they should not shape the first OSS launch gate.

## Open Questions

- Should `--tui` be labeled "partial" or "experimental" in the first public
  beta?
- Which planning and architecture docs should remain public for credibility vs
  move out of the public surface for clarity?
- After the public repo launch, what is the trigger for shifting attention from
  stabilization to parity expansion?

## Next Step

`/maestro:plan "Pre-OSS coordination initiative: sequence stabilization, planning alignment, OSS hardening, and deferred roadmap work for TermiFlow"`
