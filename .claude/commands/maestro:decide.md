# /maestro:decide — Structured Decision Making

Facilitates decisions with explicit options, criteria, and
rationale. Produces a decision record that future-you can
understand.

**Usage:** `/maestro:decide [question]`

## Context Loading

- IF `--help` is present: REPORT purpose, no flags (pass the
  decision question as an argument), inputs, outputs, examples,
  and related commands, then STOP

1. Read `context/session-checkpoint.md`
2. Read `decisions/` for related past decisions
3. Read relevant plans from `planning/`
4. Load `.claude/skills/scientific-method/SKILL.md` if the
   decision depends on evidence quality, a bet, or a proposed
   experiment
5. Load `.claude/skills/decision-logging/SKILL.md` (always)
6. Load `.claude/skills/leadership/SKILL.md` if decision involves
   people/team
7. Load `.claude/skills/writing-tone/SKILL.md` for the final
   record and summary

## Exit Criteria

- [ ] Decision framed as a clear question
- [ ] At least 2 real options considered (no straw men)
- [ ] Evaluation criteria explicit
- [ ] Door type identified (one-way or two-way)
- [ ] Decision made OR explicitly deferred with trigger
- [ ] If evidence is weak or mixed, that is stated explicitly
- [ ] Decision record saved to `decisions/DEC-{NNN}.md`

## Decision Facilitation Flow

### 1. Frame the Question

Help the user articulate what they're actually deciding. Often
the presenting question isn't the real one.

Ask:

- "What changes depending on the answer?"
- "Why does this need deciding now?"
- "Is this actually one decision or several bundled together?"

### 2. Identify Door Type

**Two-way door** (reversible): Decide in minutes. Action >
analysis. Use lightweight process.

**One-way door** (irreversible or very costly to reverse): Slow
down. Get more input. Use full decision record.

### 3. Generate Options

Minimum 2, maximum 5. Each option must be genuinely viable — not
a straw man to make the preferred option look good.

Include "do nothing" as an explicit option when relevant.

### 4. Define Criteria

Ask the user what matters. If they're unsure, propose:

- Impact (how much does this move the needle?)
- Effort (what does this cost in time/energy/money?)
- Reversibility (how easy to undo if wrong?)
- Alignment (how well does this fit our strategy?)
- Risk (what's the downside scenario?)

### 5. Evaluate

Score or rank each option against criteria. Be explicit about
trade-offs — there's always a trade-off.

If the decision depends on uncertain evidence, also ask:

- What's the strongest evidence we actually have?
- What's still assumption?
- What result would make us revisit this?

### 6. Decide or Defer

If deciding: record the decision with rationale. If deferring:
record what new information would enable the decision, and set a
deadline.

## Decision Record

Save to `decisions/DEC-{NNN}.md` using the format from
the decision-hygiene rule. For complex decisions, use the
optional expansions in the `decision-logging` skill (Options
Considered, Revisit Trigger).

To determine NNN: scan existing files, increment the highest
number.

## Workflow Chain

- After deciding → `/maestro:commit` to save the record
- If decision requires planning → `/maestro:plan`
- If decision needs more thinking first → `/maestro:think`

## Boundaries

- Don't decide for the user — facilitate, present, recommend
- Don't skip options generation even if the user "already knows"
  (the process surfaces blind spots)
- Don't combine decisions — if you find 2 decisions bundled,
  separate them and handle sequentially
- Don't defer without a trigger — "we'll decide later" is not a
  plan

---

END OF COMMAND
