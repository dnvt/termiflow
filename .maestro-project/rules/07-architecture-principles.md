---
paths:
  - src/**
  - CLAUDE.md
---

# Architecture Principles

Before making structural changes (new modules, new traits, module reorganization),
read `CLAUDE.md` — it documents the full data flow and key abstractions.

## Quick Reference

- **Single responsibility**: one module = one concern (parser, layout, render, style, config are distinct)
- **Type safety**: Enums for states, newtypes for identifiers, parse at boundaries
- **Composition**: No God structs; small focused traits (2–4 methods)
- **Dependency rule**: Lower layers never import from higher layers — render/ does not call parser; layout does not call render
- **Deep modules**: Simple public API hiding complex internals (`lib.rs` re-exports only what callers need)
- **Fail fast**: `?` propagation everywhere, `Result<T,E>` only, zero silent failures

## TermiFlow Pipeline Constraint

The render pipeline is strictly one-way:

```
parser → graph → layout → canvas → output
```

A stage may not reach backward (e.g., render must not re-parse; layout must not render).
New features that require two-way flow need explicit design discussion.

## Before Adding a New Module

1. Does it have exactly one responsibility?
2. Which stage of the pipeline does it serve?
3. Does it introduce a dependency that violates layer order?
4. Are its public types minimal?

## Before Adding a New Trait

1. Is it small and focused (2–4 methods)?
2. Is it defined in the module that owns the abstraction?
3. Does `OrientedCoords` already handle the direction-agnostic version of this?
