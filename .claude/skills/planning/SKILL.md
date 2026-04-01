---
name: planning
version: 3.0.0
description: Create durable roadmap-aligned plans for knowledge-work repos using the plan output directory and roadmap path configured by Maestro.
allowed-tools: [Read, Write, Edit, Glob, Grep, Bash, Agent]
---

# Planning

## Purpose

Produce durable plans at the right level of scope, save them in the configured
plans directory, and keep the roadmap relationship explicit instead of implicit.

This skill is for knowledge-work planning, not engineering backlog management.
It assumes a lightweight repo shape:

- a roadmap document
- a directory of durable plans
- decision records
- optional thinking and research inputs

## Inputs Expected From The Command

The calling command should already have resolved:

- `PLANS_DIR`: where durable plans live
- `ROADMAP_PATH`: the roadmap file to align against
- `SCOPE`: vision, strategy, initiative, project, or task
- related plans and decisions
- whether the plan is a new artifact or an update to an existing one

If any of those are missing, infer conservatively from repo context rather than
inventing a new planning system.

## Scope Model

| Scope | Typical Horizon | What It Produces |
| ----- | --------------- | ---------------- |
| Vision | 1-3 years | future state, themes, non-goals |
| Strategy | 6-12 months | target state, initiatives, bets |
| Initiative | 1-3 months | objective, scope, sequencing, dependencies |
| Project | 2-6 weeks | concrete deliverable plan |
| Task | 1-5 days | focused execution slice |

When ambiguous, ask once which scope the user intends.

## Plan Lifecycle Convention

Every durable plan should declare both of these fields near the top:

- `Status`: `Draft`, `Active`, `Parked`, or `Done`
- `Roadmap Slot`: `Current Focus`, `Active Workstreams`, `Backlog`, or
  `Completed`

Use them this way:

- `Draft`: being shaped; may stay off-roadmap unless it already affects active
  priorities
- `Active`: committed and in progress; should usually appear on the roadmap
- `Parked`: intentionally not active; should usually live in backlog
- `Done`: complete; should move to completed or drop out of active roadmap

Roadmap slot is how the plan maps into the roadmap document. It is not extra
metadata for its own sake.

## Roadmap Sync Rules

Not every plan deserves a first-class roadmap line. Keep the roadmap legible.

- Vision, strategy, and initiative plans usually deserve direct roadmap entries
- Projects deserve roadmap entries when they materially affect current
  priorities, not by default
- Tasks usually stay inside their parent plan and should not clutter the roadmap

When syncing a plan to the roadmap:

1. Update an existing entry instead of creating duplicates
2. Link to the plan path directly
3. Place it under the declared `Roadmap Slot`
4. Reflect the plan's current `Status`
5. Remove or move stale entries when a plan changes slot

## Output Location Rules

Default to one markdown file per plan in `PLANS_DIR` using a clear slug:

- `{plans_dir}/{topic-slug}.md`

Reuse an existing plan path when:

- the plan already exists
- there is an obvious parent/child convention already in use
- the user explicitly asked to update a specific plan

Do not invent a deep folder taxonomy unless the repo already uses one.

## Shared Header Convention

All plans should start from this lightweight frame:

```markdown
# {Scope}: {Name}

**Status:** Draft | Active | Parked | Done
**Roadmap Slot:** Current Focus | Active Workstreams | Backlog | Completed
**Owner:** {who}
**Timeline:** {start} → {target end}
**Aligned To:** {parent scope, roadmap theme, or north star}
**Decision Links:** {DEC-NNN or explicit rationale}
```

If a field is genuinely unknown, say so plainly rather than omitting it.

## Scope Templates

### Vision

Use:

- future state
- strategic themes
- explicit non-goals
- observable success signals

### Strategy

Use:

- current state
- target state
- initiatives
- key bets and risks
- resource constraints

### Initiative / Project / Task

Use:

- objective
- scope in/out
- success criteria
- dependencies
- risks and mitigations
- open questions

For uncertain work, add:

```markdown
## Evidence To Gather

- {what would materially increase confidence}

## Experiment Log

**Hypothesis:** {what we believe}
**Intervention:** {what we will try}
**Expected Observation:** {what would support or weaken it}
**Actual Observation:** {fill during execution}
**Conclusion:** {continue, pivot, stop, or defer}
```

## Context Discipline

Before writing or updating a plan:

1. Read the roadmap
2. Read relevant sibling or parent plans
3. Read decisions that constrain the plan
4. Check whether a durable plan already exists for this work
5. Prefer updating an existing plan over creating a duplicate

## Process

1. Detect the scope
2. Choose or confirm the output path in `PLANS_DIR`
3. Draft or update the plan using the shared header convention
4. Make the `Status` and `Roadmap Slot` explicit
5. Sync the roadmap when the plan meaningfully changes active priorities
6. Report:
   - plan path
   - scope
   - status
   - roadmap action taken

## Quality Bar

A good plan is:

- specific enough to execute
- honest about uncertainty
- linked to a decision or clear rationale
- explicit about scope and dependencies
- legible in the roadmap system

## Anti-Patterns

- backlog hierarchies copied from another repo without need
- roadmap entries with no durable plan behind them
- durable plans with no status or roadmap slot
- creating a fresh plan when an existing one should be updated
- stuffing task-level detail into vision or strategy plans
- adding roadmap entries for every small task
