---
schema: termiflow.visual_quality.program.v1
status: active
current_slice: S4_ready
owner: Maintainer
created: 2026-07-10
agent_dispatch_trace:
  command: "maestro:run"
  main_lane_role: orchestrator
  lanes_required: ["Implementation", "Verifier"]
  lanes_spawned: ["Implementation", "Verifier"]
  parallel: true
  synthesis_owner: main_thread
  evidence_reviewed: true
  contradictions_resolved: true
  final_artifact: "planning/VISUAL_QUALITY_STEWARDSHIP.md"
---

# Visual Quality Stewardship

## Purpose

This is the tracked contract for TermiFlow's recurring visual-quality program.
It turns “perfect diagrams” into a finite, falsifiable standard: release-worthy
quality over a versioned evaluation matrix, verified by independent oracles,
two clean review sweeps on unchanged source, and explicit maintainer sign-off.

The detailed implementation plan and reviews live in Maestro's private rootless
sidecar. This file owns the repository-visible scope, gates, and slice status.

## Quality Order

Evaluate changes lexicographically:

1. semantic correctness
2. geometric integrity and containment
3. route legibility
4. text integrity and ownership
5. readability, spacing, compactness, and polish

A new defect at an equal or earlier layer rejects a change even if total
finding count falls. Findings are deduplicated by root cause and compared only
within a frozen corpus/oracle epoch.

## Severity

- **P0:** crash, nondeterminism, wrong/missing semantics, or data loss
- **P1:** overlap, illegal overwrite, broken containment/border, disconnected or
  misdirected route, meaningful clipping, or unreadable ownership
- **P2:** material traceability, alignment, spacing, or efficiency defect
- **P3:** preference or polish without correctness/readability impact

Critic severities are evidence, not automatic P-level mappings. Completion
requires zero critic Errors. Warnings must be resolved or become explicit
maintainer-owned out-of-matrix exceptions with an owner and expiry. Info may
remain only as documented P3 evidence.

## Evaluation Matrix

The matrix is discovered dynamically; counts below describe the S0 baseline.

### Tier A — Complete primary-style review

- 234 valid fixtures × ASCII/Unicode × default/optimized = 936 renders
- complete structured visual review
- 3 expected-error fixtures in a separate negative-test lane

### Tier B — Exhaustive machine matrix

- 234 valid fixtures × 9 base styles × default/optimized = 4,212 renders
- every row checks status, classified stderr, output presence/hash, critic and
  independent invariants, and normalized geometry where available

### Tier C — Development and evaluator-owned closure holdouts

- seed/version/hash are pre-registered
- implementation does not inspect closure outputs before validation
- discovered failures are permanently promoted to regression coverage
- one closure-holdout hash stays frozen across final sweeps

### Tier D — Metamorphic checks

- TD↔BT and LR↔RL semantic/containment parity
- ASCII↔Unicode topology/ownership equivalence
- default→optimized semantic identity and quality non-regression
- repeated-run determinism

## Evidence Independence

Goldens prove stability, not visual quality. The built-in critic and
GeometryTrace are useful but share assumptions with the renderer. Acceptance
must also use:

- expectations derived independently from parsed graph semantics
- a final-text/cell-grid topology checker that does not consume renderer
  ownership metadata
- mutation tests that corrupt final frames and pre-render geometry separately
- structured visual decisions tied to immutable frame hashes

## Worktree Ownership

Automation never assumes that a sidecar lease excludes the user or other
agents.

- Prefer a dedicated clean worktree for mutation slices.
- Otherwise use a write-set allowlist and per-path compare-and-swap preimage
  hashes immediately before every patch.
- Verify expected postimage hashes after each patch.
- Never target a pre-existing dirty source path.
- Pause on overlap; never reset, discard, or overwrite unrelated work.
- Reverse a cycle-owned patch only when its exact recorded postimage still
  matches.
- Never stage, commit, push, publish, or release without separate authority.

At S0, these pre-existing paths are excluded from automation ownership:

- `planning/OPEN_SOURCE_HARDENING.md`
- `planning/PLAN.md`
- `planning/PRE_OSS_COORDINATION.md`
- all current untracked `.agents/skills/**` files

## Artifact Boundary

Tracked:

- this program contract
- future accepted harness/oracle/regression tooling
- ordinary regression fixtures and tests intended for contributors

Private or ignored:

- runtime ledger, append-only events, leases, evaluator holdout manifest, and
  current checkpoint under the resolved Maestro context directory
- generated frames, manifests, logs, review sheets, and packets under
  `artifacts/visual-audit/`

Private audit/holdout/state data must not enter the published crate. Package
contents are an explicit verification gate.

## Reference Display Profiles

### Logical machine profile

- raw non-ANSI cell grid
- repository `DisplayProfile`
- `unicode-width 0.2.2`
- `unicode-segmentation 1.13.3`
- locale `en_US.UTF-8`
- ASCII is the portable visual baseline

### Reference Unicode visual profile

- macOS 26.6 build 25G5057c
- Terminal.app 2.15
- Menlo Regular from `/System/Library/Fonts/Menlo.ttc`
- fixed cell grid, UTF-8 locale, ANSI disabled/stripped
- exact font size, cell dimensions, viewport, and screenshot/raster parameters
  must be captured when the review-packet renderer is implemented

The current automation shell reports `TERM=dumb`; it is build evidence only and
cannot provide final Unicode visual sign-off.

## Recurring State

The current private ledger is bootstrap evidence for current execution state;
it is not yet a tamper-evident or reconstructible event chain. S5 will
implement that capability. Runtime states are `active`, `paused_conflict`,
`needs_decision`, `needs_review`, `awaiting_signoff`, and `accepted`.

The future executor must use a lease plus per-path CAS, atomic phase events,
write-once finalized packets, resumable per-row review records, an eight-
iteration cap, plateau rules, and state-specific exit codes. Scheduling is not
authorized by this S0 slice.

## Slice Sequence

- **S0:** contract and evidence-only baseline
- **S0R:** resolve S0 review blockers: snapshot classification, path-level CAS,
  and bootstrap-ledger status
- **S0B:** behavior-neutral strict-Clippy and golden-runner baseline
- **S1:** fail-closed corpus runner
- **S2:** structured critic plus independent geometry/text oracles
- **S3:** taxonomy, holdouts, and metamorphic coverage
- **S4:** review packet and baseline visual sweep
- **S5:** resumable executor and separately authorized recurrence
- **S6:** bounded one-defect repair cycles
- **S7:** optional evidence-gated consolidation
- **S8:** complete verification and two independent/reconciled sweeps
- **S9:** maintainer packet and explicit sign-off

## S0 Evidence Summary

- dynamic inventory: 237 inputs, 234 valid, 3 expected errors, 474 expected
  ASCII/Unicode outputs, and no missing expected pair
- valid direction counts: TD 60, LR 58, BT 58, RL 58; the two warning fixtures
  account for the TD-only imbalance
- nine base styles confirmed
- `cargo fmt --check`: pass
- `cargo test`: pass, 487 tests
- `cargo test --features golden`: fail in ASCII and Unicode for
  `collision_edge_along_border_td`
- strict all-target/all-feature Clippy: fail at 25 diagnostic sites
- package listing succeeds with 70 entries but includes 9 unrelated
  `.agents/**` files
- current render scripts are not trustworthy gates: they hardcode an absent
  target path and swallow per-case failures
- provisional one-off Tier A: 936/936 successful, 28.55s, 4,112 KiB, 455
  unique frame hashes, 72 nonempty stderr logs
- provisional one-off Tier B: 4,212/4,212 successful, 128.88s, 18,584 KiB,
  1,855 unique frame hashes, 324 nonempty stderr logs
- these cost measurements are baseline evidence, not acceptance gates; S1 must
  reproduce them with a durable fail-closed manifest and classified stderr

## S0 Acceptance

S0 is complete when this contract and private S0 artifacts record:

- dynamic inventory and corpus hashes
- exact dirty-state identity and protected paths
- severity/epoch semantics and ledger/event schemas
- tracked/private/package boundaries
- evaluator-owned holdout policy
- logical and Unicode reference profiles
- baseline verification, runtime/storage limits, and known blockers
- exact next command and no renderer behavior change

## S0R Remediation

Twenty-six TD collision/subgraph snapshots were stale relative to commit
`57d6fdb`, which intentionally moved diagonal cross-subgraph bridges above the
arrow row. Repeated render probes then revealed a P0 nondeterminism defect in
subgraph-overlap resolution: `HashMap` iteration and equal-shift selection were
not stable. `src/layout.rs` now sorts subgraph IDs and uses a lexicographic
tie-break for equal shifts. The repeated-render regression test, multi-process
hash probes, full corpus comparison, and golden suite verify the stable output
used for the deliberate snapshot reconciliation.

S0R records a per-path ownership manifest for all protected user paths and a
separate preimage/postimage manifest for every reconciled snapshot. The ledger
and event files are explicitly bootstrap-only until S5 supplies canonical event
hashing and reconstruction. `subgraph_outside_td` and `subgraph_single_td`
remain open arrow-shaft findings for a later visual repair slice.

S0R, S0B, S1, and S2 are accepted with deferred findings. S3 is authorized,
but this does not make the renderer quality gates green.

## S1 Corpus Runner

`scripts/visual_audit.sh` is the fail-closed primary-matrix runner. It discovers
the configured Cargo binary from JSON messages, records JSONL rows in a staged
artifact directory, and atomically publishes only a fully validated run. Its
initial complete evidence has 936 successful primary rows, 12 separate
expected-error rows, and 72 explicitly classified warning rows. It never
writes `tests/fixtures/expected/`.
