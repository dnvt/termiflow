# /maestro:plan — Strategic Planning & Decomposition

Turns goals into structured, actionable plans. Scopes
automatically based on what the user describes.

**Usage:** `/maestro:plan [topic]` or
`/maestro:plan --scope {vision|strategy|initiative|project|task}` or
`/maestro:plan [topic] --deep`

## Context Loading

- IF `--help` is present: REPORT purpose, flags
  (`--scope {vision|strategy|initiative|project|task}`, `--deep`),
  inputs, outputs, examples, and related commands, then STOP

1. Read `planning/PLAN.md` for strategic alignment
2. Read `context/session-checkpoint.md`
3. Scan `planning/` for related active plans
4. Scan `decisions/` for constraints from past decisions
5. Load `.claude/skills/planning/SKILL.md` (always)
6. Load `.claude/skills/scientific-method/SKILL.md` if the plan
   contains a real bet, meaningful uncertainty, or a need for
   measurement
7. Load `.claude/skills/product-strategy/SKILL.md` if
   product-level scope
8. Load `.claude/skills/design-thinking/SKILL.md` if user-facing
   scope
9. Load `.claude/skills/growth-strategy/SKILL.md` if the plan
   includes GTM, adoption, or channel questions
10. Load `.claude/skills/leadership/SKILL.md` if team/org scope
11. Load `.claude/skills/writing-tone/SKILL.md` for the durable
    plan output

## Exit Criteria

- [ ] Scope level identified and confirmed with user
- [ ] Plan follows the appropriate scope template
- [ ] Dependencies and risks named
- [ ] Success criteria are measurable (or at least observable)
- [ ] Open questions listed — not hidden
- [ ] Plan traces back to a decision or explicit rationale
- [ ] Plan declares a lifecycle `Status` and `Roadmap Slot`
- [ ] If uncertainty is material, the plan includes an
      Experiment Log or evidence agenda
- [ ] Plan saved to `planning/`
- [ ] `planning/PLAN.md` updated or explicitly confirmed unchanged

## Scope Detection

Detect scope from the user's language:

| Signal                                       | Scope      | Template                               |
| -------------------------------------------- | ---------- | -------------------------------------- |
| "Where are we going", "vision", "north star" | Vision     | 1-3 year horizon, 3-5 strategic themes |
| "Strategy", "approach", "how we win"         | Strategy   | 6-12 month horizon, 3-5 initiatives    |
| "Initiative", "program", "workstream"        | Initiative | 1-3 month horizon, 3-7 projects        |
| "Project", "feature", "deliverable"          | Project    | 2-6 week horizon, 5-15 tasks           |
| "Task", "action item", "next step"           | Task       | 1-5 day horizon, single deliverable    |

## Scope Templates

### Vision

```markdown
# Vision: {Name}

**Horizon:** {timeframe} **Owner:** {who} **Decision Links:**
{DEC-NNN or explicit rationale}

## Where We're Going

{2-3 paragraphs: the future state we're building toward}

## Strategic Themes

1. {Theme} — {one-line description} ...

## What We're NOT Doing

{Explicit exclusions}

## How We'll Know We're Succeeding

{Observable signals, not vanity metrics}
```

### Strategy

```markdown
# Strategy: {Name}

**Horizon:** {timeframe} **Aligned to:** {vision or north star}
**Decision Links:** {DEC-NNN or explicit rationale}

## Current State

{Where we are now — honest assessment}

## Target State

{Where we want to be}

## Initiatives

1. {Initiative} — {objective}, {key result} ...

## Key Bets & Risks

{What we're betting on and what could go wrong}

## Resource Constraints

{People, money, time, attention}
```

### Initiative / Project / Task

```markdown
# {Scope}: {Name}

**Status:** Draft | Active | Parked | Done
**Roadmap Slot:** Current Focus | Active Workstreams | Backlog | Completed
**Owner:** {who} **Timeline:** {start} → {target end}
**Aligned to:** {parent scope}
**Decision Links:** {DEC-NNN or explicit rationale}

## Objective

{One sentence: what does done look like?}

## Scope

**In:** {what's included} **Out:** {what's explicitly excluded}

## Success Criteria

- [ ] {Measurable or observable criterion} ...

## Dependencies

- {What needs to happen first or in parallel}

## Risks & Mitigations

- {Risk} → {mitigation or acceptance}

## Open Questions

- {Things we need to figure out before or during execution}
```

## Workflow Chain

- After planning, user typically goes to `/maestro:think` for
  specifics or starts executing
- If planning surfaces a needed decision → `/maestro:decide`
- When plan is ready to save → `/maestro:commit`

## Lifecycle & Roadmap Sync

Every durable plan should declare both:

- `Status`: `Draft`, `Active`, `Parked`, or `Done`
- `Roadmap Slot`: `Current Focus`, `Active Workstreams`,
  `Backlog`, or `Completed`

Use these rules:

- `Draft` plans may stay off-roadmap unless they already shape
  current priorities
- `Active` plans should appear in `planning/PLAN.md`
- `Parked` plans belong in backlog unless deliberately hidden
- `Done` plans should move to the completed section or be removed
  from active sections
- Vision, strategy, and initiative plans usually deserve direct
  roadmap entries; projects only when strategically important;
  tasks usually stay inside the parent plan

## Evidence & Experiment Design

For plans with real uncertainty, include:

```markdown
## Evidence To Gather

- {what we need to observe before confidence increases}

## Experiment Log

**Hypothesis:** {what we believe}
**Intervention:** {what we will test or change}
**Expected Observation:** {what would support or refute it}
**Actual Observation:** {filled after execution}
**Conclusion:** {continue, pivot, stop, or defer}
```

If the plan is straightforward and low-risk, don't add fake
scientific theater. Use this only when it sharpens the work.

## Deep Mode (--deep)

Use a wider planning pass:

- Load more sibling artifacts and prior thinking before writing
- Stress-test assumptions, dependencies, and sequencing before
  finalizing
- Expand risks, open questions, and non-goals instead of keeping
  them implicit
- Strengthen the evidence plan and define what would change the
  recommendation
- If the client supports parallel agents, use them sparingly for
  challenge, evidence, or synthesis work

## Boundaries

- Plans are living documents — don't overspecify at creation time
- Don't plan what you should be deciding (if there's a fork, use
  /maestro:decide)
- Don't create child plans during parent planning — note them as
  "to decompose"
- Don't fill in details the user hasn't thought about — ask
  instead

---

END OF COMMAND
