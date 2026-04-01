# DEC-003: Launch the Public Repo Before Publishing to crates.io

**Status:** Decided
**Date:** 2026-04-01
**Decider:** Maintainer

## Context

The open-source hardening sprint needs a sequencing decision: should TermiFlow
launch repo-first, crates.io-first, or both together?

This decision matters now because the hardening plan includes package boundary
work, release metadata, CI, and launch docs. Those tasks depend on whether the
first public milestone is a source-visible repo launch, a publishable crate, or
both on the same day.

Current evidence is strong on readiness gaps because it comes from direct repo
inspection:

- `cargo package --list --allow-dirty` currently fails because Cargo tries to
  package `.claude/commands/maestro:*.md` files with `:` in the filename.
- `Cargo.toml` is missing public release metadata such as `repository`,
  `homepage`, and `documentation`.
- The repo has no top-level `LICENSE` file yet, despite declaring `license =
  "MIT"` in the manifest.
- Public docs are not fully aligned on feature status, especially around
  `--tui`.

Evidence is weaker on downstream adoption impact because no external user
feedback exists yet.

## Door Type

One-way enough to deserve a durable record. The exact sequencing can be revised
later, but the first public launch sets expectations and creates a durable
external first impression.

## Decision

Launch the public repo first. Do not make crates.io publication part of the
first public release gate.

Treat crates.io publication as a follow-on step inside the same hardening
initiative, to be executed only after package boundaries, manifest metadata,
public docs, and baseline CI are clean.

## Why

- Repo readiness is closer than crates.io readiness.
- A public repo lets us validate the product surface, docs, and community
  expectations without coupling that milestone to Cargo packaging quality.
- crates.io publication has a higher first-impression bar for a CLI tool
  because broken metadata, confusing package contents, or mismatched docs create
  immediate install friction.
- "Both together" is attractive only after the current packaging and doc drift
  issues are resolved; today it would couple two launch risks into one event.
- crates.io-first would optimize for install convenience at the cost of higher
  launch fragility.

## Options Considered

1. **Public repo first, then crates.io.**
   Pros: lowest coupling, fastest path to a credible public beta, easier to
   correct docs and boundaries before package distribution.
   Cons: early users install from source or clone/build until the crate lands.
2. **crates.io first, then public repo.**
   Pros: strongest install story on day one for Rust users.
   Cons: blocked by current packaging issues and creates a worse failure mode if
   docs or package contents are not ready.
3. **Launch repo and crates.io together.**
   Pros: cleanest public story when everything is ready.
   Cons: raises the launch bar and couples packaging, docs, CI, and repo
   curation into a single go / no-go event.

## Criteria

- time to credible public launch
- first-impression risk
- reversibility
- packaging readiness
- documentation coherence
- distribution convenience

## Consequences

- The open-source hardening sprint should sequence repo curation, public docs,
  and CI before any crates.io publish work is considered complete.
- The launch checklist should treat `cargo package --allow-dirty` and
  `cargo publish --dry-run` as crate-release gates, not repo-launch gates.
- Public docs should initially assume repo/source installation is acceptable.
- If the hardening sprint completes quickly, crates.io can still follow soon
  after the public repo launch; this decision does not defer it indefinitely.

## Revisit Trigger

Reopen this decision when all of the following are true:

- `cargo package --allow-dirty` succeeds and the package contents are reviewed
- `cargo publish --dry-run` succeeds
- public docs are consistent on supported scope and feature status
- baseline GitHub Actions are green on the public branch

At that point, reassess whether crates.io should ship immediately as phase two
of the same beta launch.
