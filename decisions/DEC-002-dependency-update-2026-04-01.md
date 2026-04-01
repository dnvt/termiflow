---
# DEC-002: Bump ratatui 0.29→0.30 and toml 0.9→1

**Status:** Decided
**Date:** 2026-04-01
**Decider:** Maintainer

## Context

Running `/maestro:run --deep` to update all dependencies to latest versions. `cargo update` handles lock-level bumps automatically. Two direct dependencies required Cargo.toml version constraint changes:

- `ratatui`: 0.29 → 0.30 (new minor release)
- `toml`: 0.9 → 1 (major version, spec 1.1.0)

## Decision

Bump both in `Cargo.toml`. Keep `crossterm = "0.29"` (already at latest). Keep `lazy_static = "1.5"` (migration to `OnceLock` deferred — low priority, no functional difference).

## Why

- Both compiled cleanly with no breaking API changes in our usage
- 299 tests passed after bump
- Staying current reduces future migration delta
- `toml` v1 is spec-compliant TOML 1.1.0; our config parsing is unaffected

## Consequences

- Cargo.lock now pinned to ratatui 0.30.0 and toml 1.1.1
- 59 transitive packages updated in the lock file
- `lazy_static` modernization (→ `OnceLock`) deferred; no regression risk

## Revisit Trigger

Next time a `lazy_static` regex cache causes a lint or MSRV concern.
