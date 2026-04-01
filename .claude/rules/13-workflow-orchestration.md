# Workflow Orchestration

## Core Principle

Commands, skills, and agents are one connected system. Treat each
command as a deliberate handoff point: know what it needs before
running, what it is likely to produce, and what should happen
next.

## Proactive Trigger Map

| Signal                                    | Action                        |
| ----------------------------------------- | ----------------------------- |
| Missing scaffold or baseline files        | `/maestro:start --init`       |
| Session begins                            | `/maestro:start`              |
| Need a fast place to capture async notes  | `/maestro:start --inbox`      |
| User explores a new topic                 | `/maestro:think` (diverge)    |
| User has 3+ options                       | `/maestro:think` (converge)   |
| User is very confident                    | `/maestro:think` (challenge)  |
| Multiple threads discussed                | `/maestro:think` (synthesize) |
| Decision point identified                 | `/maestro:decide`             |
| Goal needs decomposition                  | `/maestro:plan`               |
| Artifact or deliverable needed            | `/maestro:run`                |
| Plan, decision, or artifact needs critique| `/maestro:review`             |
| Important uncertainty or an untested bet  | `scientific-method` skill     |
| Work needs saving                         | `/maestro:commit`             |
| End of session                            | `/maestro:commit`             |
| Weekly coherence or inbox cleanup         | `/maestro:sync`               |
| Raw notes need processing                 | `/maestro:sync --inbox`       |
| Need evidence for a decision              | `/maestro:research`           |
| External findings need triage             | `/maestro:ingest`             |
| Recurring scan is due                     | `/maestro:pulse`              |
| Workflow drift or breakage                | `/maestro:health`             |
| Safe workflow repair                      | `/maestro:health --fix`       |
| Artifact needs polishing over rounds      | `/maestro:simmer`             |

## Command To Skill Activation

Load the smallest useful skill set that matches the work. If a
command references a skill that is not installed under
`.claude/skills/`, report the gap explicitly.

| Command             | Always Load             | Conditional                                                                     |
| ------------------- | ----------------------- | ------------------------------------------------------------------------------- |
| `/maestro:start`    | —                       | `--init` for bootstrap, `--inbox` for daily async capture prep                  |
| `/maestro:think`    | brainstorming           | design-thinking, product-strategy, growth-strategy, writing-tone                |
| `/maestro:plan`     | planning, writing-tone  | scientific-method, product-strategy, growth-strategy, leadership, design-thinking |
| `/maestro:run`      | writing-tone            | scientific-method, design-thinking, product-strategy, pack-specific execution skills |
| `/maestro:review`   | decision-logging, writing-tone | scientific-method, product-strategy, design-thinking, leadership          |
| `/maestro:simmer`   | simmer, writing-tone    | design-thinking, product-strategy                                               |
| `/maestro:decide`   | decision-logging        | scientific-method, leadership, writing-tone                                     |
| `/maestro:research` | scientific-method, brainstorming | product-strategy, growth-strategy, design-thinking                        |
| `/maestro:ingest`   | decision-logging        | scientific-method, product-strategy                                              |
| `/maestro:commit`   | —                       | writing-tone                                                                    |
| `/maestro:sync`     | —                       | `--inbox` for async capture cleanup                                             |

## Canonical Chains

| Chain                  | Sequence                                                                              |
| ---------------------- | ------------------------------------------------------------------------------------- |
| Exploration            | `start → think → decide → commit`                                                    |
| Judgment               | `think → decide → simmer → commit`                                                   |
| Execution              | `plan → run → review → commit → push`                                                |
| Research               | `research → ingest → decide → commit`                                                |
| Maintenance            | `health → health --fix (if issues) → commit`                                         |
| Async                  | `start --inbox → sync --inbox → commit`                                              |
| Scheduled Intelligence | `pulse → ingest --pulse → decide or plan → commit`                                   |

`/maestro:run` may include internal verification and inline review
loops defined by the active command or pack. That does not
replace a separate `/maestro:review` when the user wants an
independent validation pass.

## Pack Awareness

Optional packs extend the portable core with domain-specific
commands, rules, skills, guides, and agents.

- Shared core stays portable across repos and users
- Packs may encode domain expertise, but should not own private
  runtime state
- Repo-local `.maestro-project/` overrides still win over shared
  core and packs
- Pack assets are active only when enabled in `maestro.toml`

## Agent Use

- Use deep mode or specialist agents only when the task benefits
  from independent challenge, evidence gathering, or synthesis
- Prefer pack-specific agents only when the active pack installs
  them
- Keep parallelism bounded: a few focused agents beat many shallow
  ones

## Maintenance Rule

When recurring confusion shows up in command usage, session
history, AGENTS docs, or review output, treat it as a Maestro
design problem. Update the relevant command, guide, skill, rule,
generator, or config docs instead of accepting the drift.

## Boundaries

- Suggest the next command, but do not auto-execute another slash
  command without the user's go-ahead
- Load only the skills that materially sharpen the current task
- Do not reference inactive pack capabilities as if they exist
- Keep the workflow legible: one authoritative rule family per
  concern, not parallel conflicting instruction sets
