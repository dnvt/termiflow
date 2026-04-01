# Project: Doc and Truth Alignment for Public Beta

**Status:** Completed
**Roadmap Slot:** Historical Reference
**Owner:** Maintainer
**Timeline:** 2026-04-01 -> 2026-04-08
**Aligned To:** `planning/PRE_OSS_COORDINATION.md` stage 3 and first public TermiFlow beta
**Decision Links:** `DEC-003` for launch sequencing; rationale in `analysis/ingest/2026-04-01-pulse-sweep.md`; pulse inputs `1`, `2`, `4`, `5`, `9`

## Parent Plan

`planning/PRE_OSS_COORDINATION.md` is the canonical source of truth. This file
owns pipeline stage 3 only: docs and planning alignment.

## Objective

Make the repo tell the truth about the current TermiFlow wedge before public
beta expectations harden: what works, what is partial, what is deferred, and
where terminal-emulator behavior or Mermaid syntax drift changes the user story.

## Scope

**In:** `README.md`, `docs/reference.md`, CLI help text, selected planning docs,
feature-status framing for `--watch`, `--tui`, `--audit`, optimization flags,
Mermaid compatibility wording, Unicode/emulator caveats, and launch-story copy.

**Out:** implementing new renderer features, broad Mermaid parity work, post-beta
editor integration, deep TUI performance optimization, and crates.io packaging
mechanics owned by `planning/OPEN_SOURCE_HARDENING.md`.

## Success Criteria

- [x] `README.md`, `docs/reference.md`, and CLI help agree on the current
      product wedge and installation story.
- [x] `--tui` and `--watch` are labeled honestly for first public beta, with
      emulator/input caveats stated where they materially affect behavior.
- [x] `--audit`, `--optimize-render`, and related critic/repair controls are
      documented consistently.
- [x] Public docs frame TermiFlow as a focused terminal-native Mermaid
      flowchart renderer and local workflow companion, not as full Mermaid
      parity.
- [x] Mermaid syntax docs call out the most relevant current gaps from pulse
      intake: edge IDs, `@{}` shapes, markdown labels, and lexical footguns such
      as lowercase `end`.
- [x] Unicode portability guidance exists for at least emulator-configurable
      width policy, emoji/CJK caveats, and the boundary between portable and
      non-portable fixtures.
- [x] `planning/PLAN.md`, `planning/PRE_OSS_COORDINATION.md`, and
      `planning/OPEN_SOURCE_HARDENING.md` agree on what is current, what is
      deferred, and what belongs to launch gating.

## Workstreams

### 1. Product Truth Pass

- align the top-level story on the current wedge: flowchart-only Mermaid,
  browser-free terminal output, watch mode, partial TUI, and critic/repair
- remove stale wording that understates implemented edge kinds or overstates
  future parity ambitions
- keep unsupported syntax visible without letting it dominate the launch story

### 2. UX and Emulator Behavior Pass

- document `--tui` as partial for first beta unless current execution work
  explicitly graduates it
- explain that alternate-screen behavior, wheel scrolling, keybinding capture,
  and Unicode-width policy can vary across emulators
- position `--watch` as the safer mode when users want normal scrollback and
  fewer fullscreen-emulator surprises

### 3. Mermaid Compatibility Pass

- document the narrow supported syntax honestly
- call out the most likely current mismatch points copied from Mermaid docs:
  edge IDs, `@{}` shapes, markdown-aware labels, nested subgraphs, and styling
- preserve the beta rule from the parent plan: this is framing work, not a
  parity-expansion commitment

### 4. Planning and Launch Narrative Pass

- align `planning/PLAN.md` with the parent initiative and child plans
- keep deferred work explicitly post-beta
- tighten the launch copy around “focused Mermaid flowchart renderer for
  terminals” and “local workflow companion for Mermaid docs workflows”

## Execution Plan

1. Build a current-state source matrix:
   compare `README.md`, `docs/reference.md`, CLI help, and roadmap language for
   contradictions or stale claims.
2. Resolve the public-truth decisions:
   choose the first-beta label for `--tui`, the explicit caveats for `--watch`,
   and the level of Mermaid-gap disclosure that is honest without derailing the
   product story.
3. Update public docs:
   README, reference docs, and CLI help must converge first.
4. Update planning artifacts:
   `planning/PLAN.md`, the parent initiative, and OSS hardening references
   should match the same reality.
5. Run a fresh-reader pass:
   confirm that a new reader would not confuse current capabilities with
   deferred parity or polished post-beta ambitions.

## Dependencies

- Stage 2 stabilization must classify any remaining visible rendering issues as
  fixed or explicit beta limitations.
- `planning/PRE_OSS_COORDINATION.md` remains the sequencing authority.
- `planning/OPEN_SOURCE_HARDENING.md` depends on this plan for the final public
  story around supported scope and feature maturity.

## Risks & Mitigations

- Public docs still over-promise Mermaid support
  -> Keep the supported wedge narrow and move parity breadth into explicit
     “not yet” language.
- Public docs become too defensive and undersell working capabilities
  -> Lead with the coherent shipped wedge, then state caveats concretely.
- Emulator caveats sprawl into generic terminal support writing
  -> Document only the behavior that changes user expectations for `--tui`,
     `--watch`, or Unicode-sensitive output.
- Planning docs drift again after doc updates land
  -> Treat `planning/PLAN.md` sync as part of this plan’s exit criteria, not a
     later cleanup.

## Open Questions

- Should first-beta wording call `--tui` “partial,” “experimental,” or both?
- Which emulator caveats belong in top-level docs versus deeper reference docs?
- Should Mermaid lexical footguns and syntax gaps live in the main reference, a
  limitations section, or both?
- How much internal planning detail should remain public-facing versus
  intentionally internal-only?

## Evidence To Gather

- A contradiction list across README, reference docs, CLI help, and roadmap
  summary before editing starts
- The exact user-facing syntax gaps that matter most from pulse intake
- A short portability note grounded in actual emulator-documented behavior
- A fresh-reader pass confirming the post-edit public story is coherent

## Exit Condition

This project is done when Stage 3 in `planning/PRE_OSS_COORDINATION.md` is
substantively complete and Stage 4 can inherit a stable, truthful public story
instead of compensating for doc drift.

## Outcome

Completed on 2026-04-01.

- Public docs now describe the shipped beta wedge consistently:
  focused Mermaid flowchart rendering, safer `--watch`, partial `--tui`, and
  explicit syntax/Unicode/emulator caveats.
- CLI help and roadmap wording were brought into alignment with the same story.
- The flaky default unicode BT collision cleanup path was stabilized during
  run closeout so the Stage 3 handoff ends with a green verification baseline.
