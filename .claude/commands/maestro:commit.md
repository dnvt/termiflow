# /maestro:commit — Checkpoint & Capture Work

Saves current thinking, decisions, and plans to persistent files.
This is the "save game" command — use it at natural stopping
points.

**Usage:** `/maestro:commit`

## Context Loading

- IF `--help` is present: REPORT purpose, no flags (run as-is),
  inputs, outputs, examples, and related commands, then STOP

1. Load `.claude/skills/writing-tone/SKILL.md` when producing
   checkpoint summaries or durable updates

## Exit Criteria

- [ ] All new decisions captured in `decisions/` as
      DEC-NNN records
- [ ] Active plans updated with current status
- [ ] `context/current-task.md` updated with the active
      focus
- [ ] Session checkpoint written to
      `context/session-checkpoint.md`
- [ ] Any new thinking captures saved to `analysis/` with
      date prefix
- [ ] Any substantive review outputs saved to
      `analysis/reviews/`
- [ ] `planning/PLAN.md` updated if priorities changed
- [ ] Git commit with descriptive message (if in a git repo)

## Workflow

### 1. Gather Unsaved Work

Scan the current session for:

- Decisions made but not yet recorded → create DEC-NNN files
- Plans discussed but not yet written → create/update plan files
- Plans whose `Status` or `Roadmap Slot` changed → sync
  `planning/PLAN.md`
- Brainstorm outputs not yet captured → save to
  `analysis/`
- Reviews worth revisiting → save to `analysis/reviews/`
- Status changes to existing work → update progress/

### 2. Update Current Task + Session Checkpoint

Refresh `context/current-task.md` with:

- Current focus
- Why it matters now
- Immediate next action
- Key linked files or decisions

Capture session state per the context-hygiene rule in
`context/session-checkpoint.md`:

- Where we are
- Decisions made (with DEC references)
- Open questions
- Next session prompt

### 3. Codify Check

Before closing, ask:

- Did we establish any pattern worth remembering? → update
  MEMORY.md
- Did we make a recurring mistake? → note it
- Is there a reusable framework from this session? → save as
  template

### 4. Stage & Commit

If in a git repo:

- Stage logically (decisions, plans, thinking, context separately
  if meaningful)
- Commit message:
  `session: {brief summary of what was decided/explored}`

### 5. Report

```
Checkpoint saved:
  Decisions:   {N} new, {M} updated
  Plans:       {list}
  Thinking:    {list of captures}
  Reviews:     {list of review captures}
  Current:     context/current-task.md
  Checkpoint:  context/session-checkpoint.md
  Next:        {what to start with next time}
```

## Workflow Chain

- Usually follows `/maestro:think`, `/maestro:plan`,
  `/maestro:review`, `/maestro:decide`, `/maestro:research`, or
  `/maestro:sync`
- If commit exposes missing structure → return to the command
  that should have produced it

## Boundaries

- Don't rewrite existing documents wholesale — update
  incrementally
- Don't create files for work that hasn't happened yet (that's
  /maestro:plan)
- Don't commit half-formed ideas as decisions — leave them in
  `analysis/`

---

END OF COMMAND
