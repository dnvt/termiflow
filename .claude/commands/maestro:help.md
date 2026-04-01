# /maestro:help — Command Reference

Quick reference for all Maestro commands. Use this when you're
not sure which command to use or what flags are available.

**Usage:** `/maestro:help` or `/maestro:help {command-name}` or
`/maestro:help [description of what you want to do]`

## Context Loading

None — no files are read in either mode.

- No arguments → reference mode (command index and canonical
  chains)
- Any text that is not a command name → routing mode (intent
  signal map → copy-pastable command chain)

## Command Index

| Command | Purpose | Key flags |
| ------- | ------- | --------- |
| `/maestro:start` | Initialize session, load context, resume from checkpoint | `--init`, `--inbox` |
| `/maestro:think` | Brainstorm, explore, challenge, or synthesize ideas | `--mode`, `--deep` |
| `/maestro:plan` | Decompose a goal into a structured, actionable plan | — |
| `/maestro:run` | Execute a planned deliverable and wrap the session | `--deep` |
| `/maestro:review` | Critically evaluate a plan, decision, or artifact | `--lens`, `--deep` |
| `/maestro:simmer` | Refine an artifact iteratively and save the full trajectory bundle | `--iterations` |
| `/maestro:commit` | Save current thinking, decisions, and plans | — |
| `/maestro:decide` | Structure and record a decision with options and rationale | — |
| `/maestro:sync` | Synchronize state across context files and roadmap | `--docs`, `--roadmap`, `--inbox`, `--deep` |
| `/maestro:ingest` | Triage research, findings, or external input | `--batch`, `--pulse`, `--deep` |
| `/maestro:research` | Gather evidence and intelligence on a topic | `--deep` |
| `/maestro:health` | Audit workflow surface; repair issues in `--fix` mode | `--fix`, `--history`, `--gate`, `--deep` |
| `/maestro:push` | Push branch and update or create PR | — |
| `/maestro:publish` | Publish Maestro updates to the shared core | — |
| `/maestro:pulse` | Scheduled intelligence research pulse | — |

**Backing Skills:**

| Skill | Used By |
| ----- | ------- |
| `simmer` | `/maestro:simmer` for multi-round refinement |

## Core Judgment Loop

- Explore the problem or generate options → `/maestro:think`
- Commit to a choice and write the DEC record → `/maestro:decide`
- Refine an existing artifact over multiple rounds →
  `/maestro:simmer`

## When To Use What

**Starting a session:**
`/maestro:start` → then think, plan, decide, or run

**Thinking:**
- Exploring a new topic → `/maestro:think --mode diverge`
- Too many options → `/maestro:think --mode converge`
- Too confident → `/maestro:think --mode challenge`
- Need to connect threads → `/maestro:think --mode synthesize`

**Deciding:**
- Explicit trade-off → `/maestro:decide`
- Need evidence first → `/maestro:research` → `/maestro:decide`

**Building:**
- Scope the work → `/maestro:plan`
- Do the work → `/maestro:run`
- Check the work → `/maestro:review`

**Saving and shipping:**
- Save progress → `/maestro:commit`
- Keep state coherent → `/maestro:sync`
- Push to remote → `/maestro:push`

**Refining:**
- Improve an existing artifact over multiple rounds →
  `/maestro:simmer`

**Maintenance:**
- Audit the surface → `/maestro:health`
- Fix issues → `/maestro:health --fix`
- External input → `/maestro:ingest`
- Pulse findings only → `/maestro:ingest --pulse`
- Scheduled scans → `/maestro:pulse`

## Canonical Chains

```
Exploration:  start → think → decide → commit
Judgment:     think → decide → simmer → commit
Execution:    plan → run → review → commit → push
Research:     research → ingest → decide → commit
Maintenance:  health → health --fix (if issues) → commit
Async:        start --inbox → sync --inbox → commit
```

## Intent Routing

When called with a description (not a command name), map intent
signals to the most efficient command chain. Output each step as
a copy-pastable line with one-line rationale. No files read.

### Output Format

```
For: [1-line paraphrase of the intent]

1. /maestro:[command] [arg]   → [one-line rationale]
2. /maestro:[command]         → [one-line rationale]
3. /maestro:[command]         → [one-line rationale]
```

### Intent Signal Map

| Signal words / intent                                    | Recommended chain                                               |
| -------------------------------------------------------- | --------------------------------------------------------------- |
| explore, brainstorm, not sure where to start             | `think → plan or decide → commit`                              |
| research, investigate, learn about, find out             | `research → ingest → decide → commit`                          |
| process, triage, analyze — have findings already         | `ingest → decide or plan → commit`                             |
| decide, choose, trade-off, weigh options, compare        | `think --mode converge → decide → commit`                      |
| plan, scope, decompose, structure the work               | `think → plan → commit`                                        |
| execute, produce, build a deliverable, write a document  | `plan → run → review → commit`                                 |
| review, audit, critique a specific artifact              | `review → decide (if issues) → commit`                         |
| save, checkpoint, wrap up, end of session                | `commit`                                                       |
| capture, log, quick note, add to inbox                   | `start --inbox`                                                |
| sync, clean up inbox, process async notes                | `sync --inbox → commit`                                        |
| start session, resume, kick off the day                  | `start → [next command based on what's active]`                |
| push, ship, open PR, share branch                        | `commit → push`                                                |
| refine, hone, iterate, polish an artifact                | `simmer → commit` (route to `/maestro:simmer`)                 |
| health check, audit workflow surface                     | `health → health --fix (if issues) → commit`                   |
| weekly pulse, scheduled scan, run recurring research     | `pulse → ingest --pulse → commit`                              |
| full loop, end-to-end, the whole thing                   | `start → think → plan → run → review → commit → push`         |

### Ambiguity Handling

If the description maps to two equally likely chains, show both
with a recommended default:

```
For: [paraphrase] — two paths (Path A recommended if unsure):

Path A (recommended) — if you're still figuring out the approach:
1. /maestro:think    → clarify the problem and options
2. /maestro:plan     → decompose once the approach is clear
3. /maestro:commit   → save the output

Path B — if the scope is already clear:
1. /maestro:plan     → go straight to decomposition
2. /maestro:run      → execute the deliverable
3. /maestro:commit   → save and wrap
```

If the description is too vague to route at all, ask one
forced-choice question:

```
One question before routing:
A) You're figuring out what to do → start with /maestro:think
B) You know what to do and need to execute → start with /maestro:plan

Which fits?
```

## Compact Help Per Command

For per-command flag details, run any command with `--help`:

```
/maestro:start --help
/maestro:think --help
/maestro:plan --help
/maestro:run --help
/maestro:review --help
/maestro:simmer --help
/maestro:commit --help
/maestro:decide --help
/maestro:sync --help
/maestro:ingest --help
/maestro:research --help
```

## Workflow Chain

**Before**: None — `/maestro:help` is a reference command, run anytime.
**After**: Run whichever command matches your situation.

**Related**: Every command in the index above.

## Boundaries

- This command never reads or writes files
- For full documentation, open the command file in `.claude/commands/`

---

END OF COMMAND
