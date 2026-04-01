---
# DEC-001: Suppress `too_many_arguments` on Internal Shape Draw Functions

**Status:** Decided
**Date:** 2026-04-01
**Decider:** Maintainer

## Context

Clippy flagged 11 internal `draw_*` functions in `src/render/shapes.rs` for exceeding the 7-argument limit (8–16 args each). These functions are not public API — they are low-level canvas drawing helpers that mirror box-drawing conventions (top-left corner char, top-right, bottom-left, etc.).

Two options were considered:
1. Suppress with `#[allow(clippy::too_many_arguments)]` at each function site
2. Refactor arguments into a parameter struct (e.g., `DrawBoxParams`)

## Decision

Use `#[allow(clippy::too_many_arguments)]` at each function, not a parameter struct.

## Why

- The argument lists are symmetrical and self-documenting (each char maps to a specific glyph role)
- A parameter struct would add boilerplate construction at every call site with no functional benefit
- These are private functions with stable call signatures — not public API
- The same pattern is used by `apply_balance_pass` and `lane_route` in `src/layout.rs` for identical reasons

## Consequences

- Clippy is clean (0 warnings)
- Call sites are unchanged
- If a shape function grows beyond ~20 arguments, revisit with a struct
