# /maestro:start — Initialize Session Context

Loads project state and resumes from last checkpoint. Run this at
the beginning of every session.

Use `--init` only when the baseline scaffold is missing or has
drifted. Use `--inbox` when you want the day's async capture file
ready before work starts.

**Usage:** `/maestro:start`, `/maestro:start --init`,
`/maestro:start --inbox`, or `/maestro:start --init --inbox`

## Context Loading

- IF `--help` is present: REPORT purpose, flags (`--init`,
  `--inbox`), inputs, outputs, examples, and related commands,
  then STOP

1. Check for baseline files:
   - `planning/PLAN.md`
   - `MEMORY.md`
   - `context/current-task.md`
   - If any are missing and `--init` is not present, flag the
     scaffold as incomplete and suggest `maestro:start --init`
2. If `--init` is present, create the missing baseline files and
   directories from the bootstrap set before continuing
3. If `--inbox` is present:
   - ensure `inbox/` and `inbox/archive/` exist
   - ensure `inbox/README.md` and `inbox/archive/README.md` exist
   - determine today's file path: `inbox/{YYYY-MM-DD}.md`
   - if the file exists, load it and count unresolved
     carry-forward items
   - if the file does not exist, create it from
     the active inbox template by replacing `YYYY-MM-DD` with
     today's date:
     - prefer `.maestro-project/templates/inbox/daily.md` when
       present
     - otherwise use `.maestro/templates/inbox/daily.md`
   - if creating today's file, scan all active unsynced inbox
     files, newest first, and carry forward only unresolved
     tasks, blockers, and explicit follow-ups
4. Read `context/current-task.md` if it exists
5. Read `context/session-checkpoint.md` if it exists
   - Resume from where we left off
   - Load open questions and next-session prompt
6. Read `planning/PLAN.md` for current priorities
7. Scan `decisions/` for recent decisions (last 5 by
   date)
8. Scan `planning/` for active plans
9. Check `context/blockers.md` if it exists
10. Load MEMORY.md topic index

## Exit Criteria

- [ ] Current state understood and summarized
- [ ] Active work streams identified
- [ ] Open questions from last session surfaced
- [ ] Blockers flagged if any
- [ ] Missing baseline files flagged if any, or created if
      `--init`
- [ ] Inbox prepared if `--inbox`
- [ ] Ready to work — next action is clear

## Init Mode (--init)

Create if missing:

- `planning/PLAN.md`
- `MEMORY.md`
- `context/current-task.md`
- `context/session-checkpoint.md`
- `context/blockers.md`
- `analysis/ingest/`
- `analysis/ingest/INDEX.md`
- `context/pulse-config.md`
- `context/coordination/`
- `context/archive/`
- `analysis/research/`
- `analysis/reviews/`
- `analysis/archive/`
- `inbox/`
- `inbox/archive/`

Also create lightweight README/index files for
`context/`, `decisions/`, `planning/`,
`planning/`, and `analysis/` if they are
missing,
plus `inbox/README.md` and `inbox/archive/README.md`.

Rules:

- Create only missing files and directories.
- Never overwrite populated files just to match the template.
- After bootstrapping, continue into the normal start report.

## Inbox Mode (--inbox)

Use this when the session should begin with a ready-to-capture
daily note.

Rules:

- Default file path: `inbox/{YYYY-MM-DD}.md`
- Prefer one file per day over multiple loose note files
- Carry forward only explicit unfinished tasks, blockers, and
  follow-ups
- Carry forward from all active unsynced inbox files, newest
  first
- Never carry forward full raw sections or narrative notes
  wholesale
- If the prior inbox file looks fully processed, don't carry
  anything forward
- If the prior file has no explicit date, infer it from the
  filename first, then file metadata
- `--inbox` may create inbox scaffolding and the daily file even
  when `--init` is not present
- `--inbox` must not rewrite durable files such as plans,
  decisions, roadmap, or checkpoints

## Report

```
Designer — Session Initialized
======================================
Resumed from: {checkpoint date or "fresh start"}

Active Work:
  {list of active plans/initiatives}

Recent Decisions:
  {last 3-5 decisions with DEC-NNN}

Scaffold:
  {complete or list missing baseline files}

Bootstrap:
  {created/reused summary or "not requested"}

Open Questions:
  {from checkpoint or "none"}

Blockers:
  {from blockers.md or "none"}

Inbox:
  {today's file path, created/reused, carry-forward count, sync status}

Ready: {suggested next command or topic}
```

## Workflow Chain

After start, the user typically moves to one of:

- `maestro:think` — brainstorm or explore a topic
- `maestro:plan` — decompose an initiative
- `maestro:review` — evaluate existing work
- `maestro:decide` — make a pending decision
- `maestro:sync --inbox` — clean up and externalize async notes

## Boundaries

- Without `--init` or `--inbox`, don't create files during start
  — only read
- With `--init`, create only missing scaffold files and then
  continue to the start report
- With `--inbox`, create only inbox scaffolding and today's inbox
  file if needed
- If checkpoint is stale (>7 days), flag it but still load

## Common Patterns

- **Morning kickoff:** Load everything, review what's active, set
  intention
- **Async kickoff:** Run with `--inbox`, capture the day in
  `inbox/{YYYY-MM-DD}.md`
- **Quick follow-up:** Minimal context, pick up one specific
  thread
- **Deep session:** Full context load, plan for extended work
- **Weekly review:** Load all + sync check (suggest
  `maestro:sync` after)

---

END OF COMMAND
