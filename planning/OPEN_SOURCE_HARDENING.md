# Project: Open-Source Hardening Sprint

**Status:** Active
**Roadmap Slot:** Active Workstreams
**Owner:** Maintainer
**Timeline:** 2026-04-01 -> 2026-04-15
**Aligned To:** Pre-1.0 polish and first public TermiFlow release
**Decision Links:** `DEC-003` for launch sequencing; rationale in `analysis/2026-04-01-open-source-strategy.md`; revives the distribution thread archived in `planning/archive/ROADMAP.md`

## Parent Plan

`planning/PRE_OSS_COORDINATION.md` is the canonical source of truth. This file
owns pipeline stage 4 only: OSS hardening.

## Objective

Prepare TermiFlow for a credible first public OSS release without waiting for
full Mermaid parity. "Done" means the repository, crate packaging, release
metadata, CI, and launch docs are coherent enough for an initial public beta.

## Scope

**In:** Public repo boundary, package include/exclude rules, Cargo metadata,
license and contribution docs, baseline GitHub Actions, public-facing docs
consistency, release checklist, and launch narrative.

**Out:** New renderer features, full Mermaid compatibility, Homebrew tap,
cross-platform binary distribution beyond a minimal first release, and a full
rewrite of `planning/PLAN.md`.

## Success Criteria

- [ ] A public artifact boundary is documented: what ships in the public repo,
      what stays as local workflow scaffolding, and what is excluded from the
      published crate.
- [ ] `cargo package --allow-dirty` succeeds and the package contents are
      reviewed before any publish attempt.
- [ ] `Cargo.toml` includes public release metadata: `repository`, `homepage`,
      `documentation`, and any needed `include` / `exclude` policy.
- [ ] The repo has baseline OSS hygiene: top-level `LICENSE`,
      `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, and a short security /
      disclosure path.
- [ ] GitHub Actions validates the repo on push / PR with at least format,
      clippy, and tests.
- [ ] Public docs agree on the supported scope and on the status of
      `--watch`, `--tui`, `--audit`, and optimization-related flags.
- [ ] A launch checklist exists for the first public beta, including the
      decision on repo-first vs crates.io-first sequencing.

## Workstreams

### 1. Public Boundary and Repo Curation

- Decide whether the public repo is this repo with curated contents, or a
  cleaned public mirror.
- Identify which directories are product surface vs local workflow state:
  `src/`, `tests/`, `examples/`, `docs/`, selected `planning/` docs vs
  `.claude/`, `.maestro/`, `.maestro-project/`, `context/`, `analysis/`,
  `inbox/`.
- Make the boundary executable, not just described: crate `exclude` / `include`
  rules, doc wording, and any repo cleanup needed before launch.

### 2. Packaging and Release Metadata

- Fix the current Cargo packaging blocker caused by files such as
  `.claude/commands/maestro:*.md`.
- Add missing manifest metadata and confirm naming, description, versioning, and
  release expectations for a 0.x public beta.
- Add a repo-local `deny.toml` so `cargo-deny` becomes actionable instead of
  noisy on normal MIT / Apache dependencies.
- Refresh audit tooling deliberately and pin the expected audit surface in CI;
  the current `cargo-deny 0.18.3` install already failed on a RustSec advisory
  using `CVSS:4.0`.
- Run and review `cargo package --allow-dirty` and `cargo publish --dry-run`
  before any real publish.

### 3. OSS Repo Hygiene and Automation

- Add baseline GitHub Actions for format, clippy, and tests.
- Decide whether release automation is needed for day one or should wait until
  after the first public beta.
- Add the minimum community docs that reduce friction for outside contributors
  and issue reporters.

### 4. Launch Docs and Positioning

- Tighten the public story to "focused Mermaid flowchart renderer for terminals"
  rather than "full Mermaid implementation."
- Reconcile doc drift across `README.md`, `docs/reference.md`, demo notes, and
  CLI help.
- Produce a short launch packet: release checklist, release notes skeleton, and
  a compact "what works / what is experimental / what is out of scope" section.

## Pulse-Driven Hardening Deltas

The April 1 pulse ingest changed this plan in two concrete ways:

1. packaging and governance work is now more specific than the original sprint
   outline:
   manifest metadata, `deny.toml`, and audit-tool freshness are explicit hardening
   tasks rather than implied cleanup.
2. the public story is sharper:
   TermiFlow should launch as a terminal-native Mermaid flowchart companion for
   local docs workflows, not as a claim of broad Mermaid completeness.

## Execution Plan

1. Define the public release model:
   same repo vs public mirror, and which docs are intentionally public.
   Launch sequencing follows `DEC-003`: public repo first, crates.io after the
   packaging and docs gates pass.
2. Make packaging pass:
   fix manifest metadata, add package boundary rules, and get
   `cargo package --allow-dirty` clean. Add `deny.toml` and make the audit tool
   versions explicit enough that dependency governance is trustworthy.
3. Add repo hygiene:
   license, contributing path, conduct, security contact, and CI.
4. Align public docs:
   supported scope, feature status, install path, and launch messaging.
5. Run release readiness review:
   one final package-content review and a go / no-go checklist for public beta.

## Dependencies

- The current dirty worktree needs to be stabilized enough to branch or cut a
  clean public-release candidate.
- A decision is needed on whether local Maestro / agent scaffolding remains in
  the public repo or moves out of the public surface.
- GitHub repository settings, branch protections, and any release secrets must
  be available before CI / release automation can be finalized.

## Risks & Mitigations

- Dirty worktree makes launch scope ambiguous
  -> Cut a dedicated OSS-hardening branch and avoid mixing it with unrelated
     feature work.
- Internal workflow artifacts leak into the public package or confuse users
  -> Use explicit package boundaries and keep public docs focused on the
     product.
- Public docs over-promise support or maturity
  -> Treat unsupported Mermaid syntax and experimental UX as explicit
     non-goals.
- Release automation becomes a time sink
  -> Start with source release + CI; defer Homebrew tap and broad binary
     distribution until after the first public beta.
- Launch waits on full parity
  -> Hold the launch bar at "credible narrow product" instead of "complete
     Mermaid renderer."

## Open Questions

- Should `--tui` and `--watch` be labeled stable or experimental in the first
  public beta?
- Which planning / architecture docs should remain public for credibility, and
  which should be moved out to reduce noise?
- Is the primary early user a CLI user, a library integrator, or both?
- Is `0.1.x` or `0.2.0` the right first public version number?

## Evidence To Gather

- Final `cargo package` contents after boundary rules are added.
- A clean `cargo deny check bans licenses sources` run using a repo-local
  policy file.
- A current advisory check with audit tooling new enough to parse the active
  RustSec database.
- Fresh-reader pass on the README and CLI reference for install and expectation
  clarity.
- CI runtime and failure modes on at least one clean GitHub Actions run.
- A short risk review of public-facing files to confirm no private workflow or
  misleading internal state is being surfaced as product.

## Experiment Log

**Hypothesis:** A narrow public beta for the renderer / CLI will create useful
feedback faster than waiting for broader Mermaid parity.

**Intervention:** Launch a public repo with coherent docs and a publishable
crate, while labeling experimental areas clearly.

**Expected Observation:** Early feedback clusters around installation, docs,
unsupported syntax, and packaging friction more than around core rendering
credibility.

**Actual Observation:** Pending.

**Conclusion:** Pending.
