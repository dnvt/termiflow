# Initiative: Pre-OSS Coordination

**Status:** Active
**Roadmap Slot:** Current Focus
**Owner:** Maintainer
**Timeline:** 2026-04-01 -> 2026-05-01
**Aligned To:** Pre-1.0 polish and first public TermiFlow beta
**Decision Links:** `DEC-003` for launch sequencing; rationale in `analysis/2026-04-01-pre-oss-coordination.md` and `analysis/2026-04-01-open-source-strategy.md`

## Objective

Coordinate TermiFlow's next month of work so the existing product becomes a
credible public beta: keep what already works stable, fix the defects that
weaken trust, align plans and docs with current reality, and place OSS launch
inside that sequence instead of alongside it.

## Source Of Truth

This file is the canonical operating plan for pre-OSS work.

- `planning/PLAN.md` is the roadmap summary only.
- `planning/DOC_TRUTH_ALIGNMENT.md` is the child plan for pipeline stage 3.
- `context/current-task.md` should point to the current execution slice, not
  replace this document.
- `planning/phase2/` docs, `planning/PHASE6_RENDER_FEEDBACK_ENGINE.md`, and
  `planning/AUDIT-mermaid-parity.md` are supporting or future-planning inputs,
  not the live operating queue.

If another document disagrees with this one about sequencing, this document
wins until it is updated.

## Product Truth Snapshot

### Working Now

- Core flowchart pipeline works: parser, layout, render, config, and CLI.
- Shipped wedge is coherent: flowchart-only Mermaid, 9 styles, 14 node shapes,
  multiple flowchart edge kinds, edge labels, single-level subgraphs, watch
  mode, partial TUI, and critic/repair capabilities.
- Verification is currently healthy: `cargo test` passes 311 tests and
  `cargo clippy` is clean.

### Still Broken Or Trust-Reducing

- LR and sibling-subgraph rendering edge cases still need either fixes or
  explicit beta framing.
- OSS packaging and repo hygiene are not launch-ready.

### Still Planned But Not Beta-Critical

- Nested subgraphs
- Mermaid edge IDs, `@{}` shapes, and markdown-aware labels
- TUI UX polish
- Per-element styling / Phase 2
- New diagram types

## Pipeline At A Glance

1. Stabilize the worktree and cut a focused beta branch
2. Stabilize the shipped product
3. Align docs and planning with reality
4. Execute OSS hardening
5. Launch the public repo beta
6. Reassess crates.io and post-beta expansion

OSS lives in stage 5. It does not happen before stabilization and truth
alignment. It also does not wait for broad parity expansion.

## Canonical Pipeline

### Stage 1. Stabilize The Worktree

**Goal:** Get to a coherent branch/release-candidate state so the beta effort is
not mixed with unrelated uncommitted work.

**Current State:** Large pre-existing work remains uncommitted across Phase 5/6
and fixture expansion.

**Tasks:**
- checkpoint or separate the current dirty batch into a clear branch boundary
- avoid mixing beta-hardening edits with unrelated exploratory work

**Exit:** We have a focused pre-OSS branch or an equivalent stable worktree.

### Stage 2. Stabilize The Shipped Product

**Goal:** Remove the defects that most obviously undermine confidence in the
existing wedge.

**Must Fix Before OSS:**
- P1: BT titled subgraph placement and border cleanup
- classify LR border gap and sibling-subgraph collision as either launch
  blockers or explicit beta limitations
- add tests for blind-spot modules where risk is concentrated

**Nice To Have Before OSS:**
- cycle fixture cleanup where it increases confidence cheaply
- open links `---` if it stays low-risk and does not delay stabilization

**Exit:** No known high-severity defect remains in the promised beta path.

### Stage 3. Align Docs And Planning With Reality

**Goal:** Make the repo tell the truth about the product and the roadmap.

**Ownering Plan:** `planning/DOC_TRUTH_ALIGNMENT.md`

**Tasks:**
- align `README.md`, `docs/reference.md`, and CLI help
- label `--tui` honestly as partial or experimental
- treat Phase 6 docs as partially implemented historical design, not current
  roadmap
- keep deferred work explicitly outside the beta gate

**Exit:** Public docs, roadmap summary, and active plans agree on what works,
what is partial, and what is deferred.

### Stage 4. Execute OSS Hardening

**Goal:** Make the repo and package publicly defensible.

**Ownering Plan:** `planning/OPEN_SOURCE_HARDENING.md`

**Tasks:**
- clean package boundaries and manifest metadata
- add dependency-governance basics (`deny.toml`, current audit tooling, explicit
  metadata)
- add top-level license and baseline community docs
- add GitHub Actions for format, clippy, and tests
- define the public repo surface and launch checklist

**Sequencing Rule:** Follow `DEC-003`: public repo first, crates.io later.

**Exit:** Repo-launch gates are met.

### Stage 5. Launch The Public Repo Beta

**Goal:** Put the narrow product in public with a trustworthy story.

**Launch Gates:**
- stage 2 defects are closed or explicitly framed as beta limitations
- stage 3 doc alignment is complete
- stage 4 repo/package gates are complete
- the public promise stays narrow: focused terminal flowchart renderer

**Exit:** Public repo is live and the first external feedback loop begins.

### Stage 6. Reassess After Launch

**Goal:** Choose the next move based on actual feedback, not pre-launch
ambition.

**Possible Next Moves:**
- crates.io publish
- parity expansion
- TUI polish
- targeted stabilization from public feedback

**Exit:** A new post-beta priority order is decided.

## Task Buckets

### Bucket A: Must Be True Before OSS

- stable enough worktree to cut a focused beta branch
- BT border corruption fixed
- launch-blocker classification for LR / sibling-subgraph issues
- blind-spot tests added where needed
- docs and plans aligned
- OSS hardening gates complete

### Bucket B: Good To Do If Cheap

- open links `---`
- low-cost fixture and audit cleanup
- selective documentation polish beyond the beta promise

### Bucket C: Planned But Explicitly Post-Beta

- dotted / thick edge parity
- nested subgraphs
- per-element styling
- advanced TUI polish
- new diagram families

## Scope

**In:** Stabilization, truth-aligned docs, roadmap cleanup, OSS hardening, and
public beta gating.

**Out:** Full Mermaid parity, 1.0 release work, Homebrew tap, broad release
automation, and major new feature families.

## Success Criteria

- [ ] The currently shipped wedge is stable enough that known high-severity
      rendering defects are either fixed or explicitly removed from the public
      beta promise.
- [ ] Public docs, CLI help, and planning documents agree on what works,
      what is partial, and what is deferred.
- [ ] The OSS hardening sprint reaches repo-launch readiness.
- [ ] Deferred roadmap work is clearly documented as post-beta, not silently
      competing with beta-critical work.
- [ ] A public repo launch window is identified and tied to observable gates,
      not vague confidence.

## OSS Beta Gates

- stable branch / worktree boundary
- No known high-severity defects in the promised beta path
- README and reference docs match actual behavior
- package boundary and manifest metadata are cleaned up
- top-level `LICENSE` and basic community docs exist
- GitHub Actions is green on the public branch

## Current Focus

The current execution slice is Stage 4:
`planning/OPEN_SOURCE_HARDENING.md`.

Immediate follow-up after that:
1. execute OSS hardening against the now-aligned public story
2. keep remaining visible rendering issues either fixed or explicitly framed as
   beta limitations
3. return to blind-spot tests where they materially affect launch confidence

## Child Plans And Supporting Inputs

### Child Plans

- `planning/BT_SUBGRAPH_TITLE_POSITION.md`
  Purpose: completed stage 2 task record for BT titled subgraph behavior
- `planning/DOC_TRUTH_ALIGNMENT.md`
  Purpose: stage 3 execution plan for public docs, roadmap truth, and launch-story alignment
- `planning/OPEN_SOURCE_HARDENING.md`
  Purpose: stage 4 execution plan

### Supporting Inputs

- `planning/RENDERING_ISSUES_AUDIT.md`
  Purpose: defect inventory for stage 2
- `planning/AUDIT-mermaid-parity.md`
  Purpose: longer-horizon parity backlog, not beta gate
- `planning/PHASE6_RENDER_FEEDBACK_ENGINE.md`
  Purpose: historical / partially-implemented design packet
- `planning/phase2/PHASE2_PLAN.md`
  Purpose: deferred styling initiative

## Dependencies

- The dirty worktree must be stabilized enough to cut a focused beta branch.
- `planning/OPEN_SOURCE_HARDENING.md` must complete its repo/package gates.
- Remaining high-severity rendering issues need either fixes or explicit
  non-goal framing before launch.

## Risks & Mitigations

- Too many active lanes at once
  -> Use the four-lane model and keep only one lane as the current focus.
- Feature expansion crowds out stabilization
  -> Treat parity work as post-beta unless it directly fixes a launch blocker.
- Public launch happens with contradictory docs
  -> Make doc alignment a gate, not a cleanup chore.
- OSS timing slips because "one more feature" feels tempting
  -> Hold the gate at credibility, not completeness.

## Open Questions

- Which specific rendering issues, if still unresolved, are acceptable to frame
  as known beta limitations rather than launch blockers?
- Should `--tui` be labeled partial or experimental for the public beta?
- Which existing planning docs belong in the public repo surface on day one?

## Evidence To Gather

- Fresh renders for the known high-severity defect fixtures
- Final package contents after boundary cleanup
- Clean-reader review of README and reference docs
- First green GitHub Actions run on the public branch

## Experiment Log

**Hypothesis:** Shipping a narrow public beta after stabilization and repo
hardening will produce better learning than waiting for a larger parity push.

**Intervention:** Sequence work as worktree boundary -> stabilize -> align ->
harden -> public repo launch.

**Expected Observation:** External feedback centers on real usage, installation,
and unsupported syntax edges, rather than basic trust failures about broken
rendering or confusing docs.

**Actual Observation:** Pending.

**Conclusion:** Pending.
