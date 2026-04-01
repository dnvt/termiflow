---
name: decision-logging
description: Use when creating, reviewing, or linking decision records, trade-off matrices, and decision snapshots.
version: 1.0.0
allowed-tools: [Read, Glob, Grep, Edit, Write]
---

# Decision Logging

Capture decisions so they stay inspectable, comparable, and revisitable. Use
this skill when a choice changes the plan, architecture, experiment path, or
operating assumptions and future sessions will need to understand why.

## Core Record

For durable decisions, use the structure from Rule 17:

- Title
- Status
- Date
- Decider
- Context
- Decision
- Consequences

For non-trivial decisions, add:

### Options Considered

List the real options, not straw men. Preserve the actual trade-offs.

### Criteria

State what matters for this call. Common criteria:

- fitness for the problem
- latency or throughput impact
- implementation complexity
- failure modes and recovery
- cost
- extensibility
- time-to-demo or time-to-validation risk

### Revisit Trigger

Name what new evidence would reopen the decision.

## Canonical Locations

- `decisions/DEC-{NNN}.md` for durable decision records
- Related plans, reviews, and checkpoints should link back to the
  decision rather than restating it inline

## Decision Matrix

When the choice is costly to reverse or affects cross-crate behavior,
performance contracts, or public APIs, create a simple decision
matrix that compares the status quo and 2-3 genuine alternatives.

## Decision Quality Checks

Before finalizing, verify:

- The question is framed correctly, not just the visible symptom
- The decision is the right size and not bundling several separate calls
- Door type is explicit: one-way or two-way
- Options are genuinely viable
- Trade-offs are visible, not hidden inside the chosen option
- The outcome links back to the relevant spec, design packet, or review

## When To Use This Skill

- A design packet reaches a real trade-off point
- A plan or review depends on prior rationale
- A research run changes what should be done next
- A decision keeps resurfacing across sessions and needs one canonical record

## Integration Points

- `/maestro:decide` is the primary decision workflow
- `/maestro:plan` should cite decisions that constrain scope or sequencing
- `/maestro:review` should surface decision drift when implementation diverges
- `/maestro:sync` should keep decision references coherent across docs

## Companion Resources

- `examples/DEC-EXAMPLE.md` - reference decision with options, criteria, and a
  revisit trigger
