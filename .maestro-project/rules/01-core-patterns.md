# Rust Patterns — TermiFlow

## Zero Tolerance (non-test code)

FORBIDDEN in `src/` outside `#[cfg(test)]`:
- `.unwrap()` — use `?`, `if let`, or explicit match
- `.expect()` — same; exception: one-time startup failures where the message is meaningful
- `panic!()`, `todo!()`, `unimplemented!()` — no stubs in shipping code
- `unsafe` — requires explicit comment justifying why it's safe and why safe alternatives don't work

## Correctness Patterns

- Float comparisons: use epsilon (`(a - b).abs() < EPSILON`), never `==`
- Array access: prefer `.get(i)` with explicit handling over `[i]` when index may be out of range
- Lock guards: clone the data you need immediately, drop the guard — do not hold across `.await` or non-trivial work
- Struct construction: use struct literals; avoid default() + field reassignment

## TermiFlow-Specific

- Canvas coordinates are `(col, row)` (x = column, y = row) — do not confuse with `(row, col)`
- `OrientedCoords` is the abstraction for direction-agnostic layout — use it rather than duplicating TD/LR/BT/RL branches
- Render pipeline is one-way: parser → graph → layout → canvas → output. Do not reach backward.
- Style components in `CompositeStyle` are independent — mixing is intentional, not a fallback

## Linting

All lint configuration lives in the root `Cargo.toml`. Do not add `#![allow(...)]` or `#![deny(...)]`
directives to individual files without a comment explaining why a file-level override is needed.
