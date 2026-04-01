---
paths:
  - apps/**
  - sdk/**
---

# TypeScript Patterns

## Runtime & Tools

- **Bun** runtime for all TypeScript execution
- **Lit** web components for studio app (`apps/studio/`)
- TypeScript strict mode enabled

## Conventions

- Use generic `querySelector<T>()` instead of type casts
- Type guards over `as` assertions
- Tests via Bun test runner (`bun test`)
- SDK uses Zod schemas for validation (`sdk/node/src/schemas/`)
