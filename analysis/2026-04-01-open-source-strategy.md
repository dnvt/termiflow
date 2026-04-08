# Thinking Capture — Open-Sourcing TermiFlow

**Date:** 2026-04-01
**Mode:** adaptive / deep
**Duration:** ~20 minutes

## Key Ideas

1. Open-source the product, not the whole working repo. The coherent public
   surface is the renderer/library/CLI/tooling/docs/tests. The AI workflow
   scaffolding and session-state folders are operationally useful, but they are
   not part of the user value proposition.
2. The right launch story is "focused flowchart renderer for terminals" rather
   than "full Mermaid implementation." The current wedge is already sharp:
   flowchart-only, pipe-friendly, ASCII/Unicode output, watch/TUI preview, and
   strong fixture coverage.
3. Timing is close, but not "publish the repo exactly as it stands." A short
   open-source hardening sprint is needed first: packaging boundaries, release
   metadata, public docs consistency, and repo hygiene.
4. Crates.io packaging and public GitHub release are different readiness bars.
   A public repo could happen first. Publishing the crate requires excluding
   non-package files such as `.claude/commands/maestro:*.md`, which currently
   break `cargo package`.
5. TUI/watch do not need to be perfect to open-source. They do need a clear
   status label. Public users can handle "experimental" better than
   contradictory docs.

## Assumptions Surfaced

- The goal is a real public OSS release, not just source-available code.
- The highest-value public artifact is the renderer/CLI, not the local Maestro
  operating system around it.
- Early community feedback is more valuable than waiting for full Mermaid
  parity.
- A narrow promise is acceptable if it is documented honestly.

## Open Questions

- Public repo first, crates.io release first, or both together?
- Should `--tui` and `--watch` be part of the launch headline or marked
  experimental?
- Which planning/design docs are worth publishing as technical credibility,
  versus keeping private to reduce noise?
- Is the intended public audience library integrators, CLI users, or both?

## Next Step

`/maestro:plan "Open-source hardening sprint for TermiFlow: public repo scope, packaging cleanup, release metadata, CI, and launch docs"`
