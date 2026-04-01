# Decision Hygiene

## Core Principle

Undocumented decisions are unstable decisions. If a choice
changes the work, risk, or sequencing in a meaningful way,
capture it where future sessions can find and evaluate it.

## Decision Record Format

Every durable decision should include:

```markdown
# DEC-{NNN}: {Title}

**Status:** Proposed | Decided | Deferred | Superseded
**Date:** YYYY-MM-DD
**Decider:** Who made the call
**Context:** Why this decision is needed now
**Decision:** What was chosen and why
**Consequences:** What changes, what is deferred, what becomes riskier
```

## Where Decisions Live

- Durable decision records live in `decisions/DEC-{NNN}.md`
- Plans, reviews, and checkpoints should link back to those
  records instead of repeating the rationale inline

## Two-Way vs One-Way Doors

**Two-way doors** are reversible. Decide fast, learn, and revisit
if the data changes.

**One-way doors** are expensive to unwind. Slow down, compare
options explicitly, and save the rationale in a durable record.

Most blocked decisions are not truly one-way. Call that out when
analysis has outgrown the actual risk.

## Quality Checks

- At least two real options were considered when the decision was
  non-trivial
- Criteria were explicit
- Revisit trigger is stated for reversible or evidence-sensitive
  decisions
- Related plans and review artifacts link back to the decision
