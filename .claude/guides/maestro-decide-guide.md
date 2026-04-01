# Maestro Decide Guide

**Purpose**: `/maestro:decide` turns vague trade-offs into durable decisions the
repo can actually operate from.

## Decision Hygiene In Practice

The workflow should produce two artifacts:

1. a durable decision record in `decisions/DEC-{NNN}.md`
2. links from the relevant plan, review, or checkpoint back to
   that decision record

This keeps the rationale durable without maintaining a second
rolling decision log.

## When To Use It

Use `/maestro:decide` when:

- a plan has multiple valid paths and one must be chosen
- a review exposes implementation drift from intent
- an experiment result changes the gating logic
- architecture, API, performance, or sequencing trade-offs need a durable call

## Door Type

### Two-Way Door

- reversible
- cheap to revisit
- optimize for speed of learning

### One-Way Door

- expensive to reverse
- likely to shape multiple future steps
- justify with explicit options and criteria

## Decision Matrix Triggers

Use a decision matrix when the decision:

- affects multiple crates
- changes a public or shared contract
- could weaken a quality or performance gate
- meaningfully changes cost, latency, or operational risk

## Strong Decision Records

Strong records:

- state the question clearly
- preserve the real options
- show the trade-off, not just the winner
- define what would reopen the choice

Weak records:

- read like conclusions without a question
- hide rejected alternatives
- claim certainty without evidence
- omit consequences

## Typical Hand-Offs

- Decide -> Plan when the chosen path needs decomposition
- Decide -> Run when the decision unblocks direct implementation
- Decide -> Research when the real blocker is missing evidence
