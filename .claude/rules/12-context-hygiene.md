# Context Hygiene

## Core Principle

The context window is an attention budget, not storage. Load what
you need for the current task. Prune what is no longer
load-bearing.

## Context Loading Hierarchy

Use the minimum context level that gets the job done:

1. Quick: `context/current-task.md` only
2. Standard: current task + recent decisions + `planning/PLAN.md`
3. Full: standard + relevant plans + session checkpoint
4. Deep: full + relevant skills + coordination files

## Pruning Rules

- Remove incorrect information before adding more context
- Remove completed items from active context promptly
- Collapse long discussions into a short summary plus a link
- Update or remove stale `MEMORY.md` entries when they stop helping
- Archive durable history in the owning area instead of letting
  active files grow into catch-alls

## Session Checkpoints

Before ending any substantial session, capture:

- where we are
- decisions made
- open questions
- the next-session prompt

Keep `context/current-task.md` and
`context/session-checkpoint.md` aligned. One is the operational
handoff; the other is the session narrative.

## Subagent Context Isolation

When spawning agents:

- give each agent only the context it needs
- pass findings back to the main thread
- do not let agents see each other's work mid-stream unless the
  task specifically requires it

Durable coordination files belong under `context/coordination/`
only when the work needs to survive beyond the current turn.

## Context Quality Priority

1. Remove incorrect information
2. Add missing information
3. Reduce noise
