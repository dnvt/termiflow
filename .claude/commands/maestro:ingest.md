# /maestro:ingest — Research Triage & Integration

Takes raw research, articles, competitor intel, user feedback, or
any external input and classifies it against your plans, roadmap,
and decisions. Turns noise into signal.

**Usage:**

- `/maestro:ingest [source]` — triage a specific article,
  research doc, or finding
- `/maestro:ingest --pulse` — triage pulse outputs in
  `analysis/research-pulse/` that are not yet recorded in
  `analysis/ingest/INDEX.md`
- `/maestro:ingest --batch` — triage all un-ingested research
  from `analysis/research/`
- `/maestro:ingest --deep` — full cross-referencing against all
  plans and decisions

## Context Loading

- IF `--help` is present: REPORT purpose, flags (`--batch`,
  `--pulse`, `--deep`), inputs, outputs, examples, and related commands,
  then STOP

1. Read `planning/PLAN.md` — the primary filter for relevance
2. Scan `planning/` — active plans that findings might
   inform
3. Scan `decisions/` — pending or recent decisions
   needing evidence
4. Read `analysis/research/` — recent research outputs
5. If `--pulse`, read `analysis/research-pulse/` — pulse outputs
   eligible for pulse-only triage
6. Read `analysis/ingest/INDEX.md` if it exists — past
   triage decisions
7. Load `.claude/skills/scientific-method/SKILL.md` if the finding quality or
   evidence strength is part of the judgment
8. Load `.claude/skills/decision-logging/SKILL.md` (ingestion decisions are
   decisions)
9. Load `.claude/skills/product-strategy/SKILL.md` if findings relate to
   market/positioning

## Exit Criteria

- [ ] Every input classified with a verdict and rationale
- [ ] High-relevance findings linked to specific plans or
      decisions
- [ ] Ingest index updated
- [ ] No finding left with "maybe" — force a classification
- [ ] Evidence quality or uncertainty noted when it affects the
      verdict
- [ ] Recommended follow-up actions listed

## Classification System

Every finding gets one of four verdicts:

### INTEGRATE — Act on this

The finding directly informs an active plan or pending decision.

**Action:** Create or update a plan, inform a decision, or add to
roadmap. **Threshold:** Clear relevance to something we're
actively working on.

### MONITOR — Watch this space

Interesting and potentially relevant, but we don't need to act
now.

**Action:** Add to a monitoring list with a revisit trigger.
**Threshold:** Relevant to our domain but not to current
priorities.

### REFERENCE — File for later

Useful background knowledge that might matter in the future.

**Action:** Save in a reference location. No active tracking.
**Threshold:** Credible, related to our space, but no near-term
action.

### DISMISS — Not relevant

Doesn't apply to our work, or too low-quality to be useful.

**Action:** Note the dismissal rationale. Don't save.
**Threshold:** Off-topic, outdated, unsubstantiated, or
redundant.

## Triage Protocol

### 1. Read the Input

For each piece of input:

- What is the core claim or insight?
- What evidence supports it?
- How credible is the source?
- Is this direct evidence, a primary source, a synthesis, or an
  anecdote?

### 2. Check Alignment

Compare against current context:

- Does this relate to any active plan in `planning/`?
- Does this inform any pending decision in `decisions/`?
- Does this connect to a roadmap priority?
- Does this challenge an assumption we're currently operating
  under?

### 3. Classify

Assign one of: INTEGRATE / MONITOR / REFERENCE / DISMISS.

Rules:

- If it changes what we should do → INTEGRATE
- If it changes what we should watch → MONITOR
- If it adds background depth → REFERENCE
- If it doesn't pass the "so what?" test → DISMISS
- When torn between INTEGRATE and MONITOR → ask: "If we ignore
  this for 30 days, what's the downside?" If the answer is
  "nothing," it's MONITOR.

### 4. Connect

For INTEGRATE findings:

- Name the specific plan or decision it affects
- Describe what changes as a result
- Propose the concrete next step

For MONITOR findings:

- Define the trigger that would upgrade it to INTEGRATE
- Set a revisit date

## Batch Mode (--batch)

Scan `analysis/research/` for files not yet in the intake
index. Triage each one. Produce a batch summary.

## Pulse Mode (--pulse)

Scan `analysis/research-pulse/` for pulse output files not yet
represented in the intake index. Triage only those pulse outputs.
This is the explicit pulse-only alias; unlike `--batch`, it does
not scan the general research directory.

## Deep Mode (--deep)

Invoke the `maestro-synthesizer` agent per `.claude/rules/14-agent-coordination.md` to cross-reference all findings against:

- The full roadmap (not just current priorities)
- All active and recent decisions
- All active plans

Look for:

- Patterns across multiple research sessions
- Contradictions between findings and current plans
- Gaps in our research coverage

## Ingest Index

Maintain `analysis/ingest/INDEX.md` under `analysis/ingest/`:

```markdown
# Intake Index

## Recent Intake

| Date   | Source               | Verdict   | Linked To       | Summary     |
| ------ | -------------------- | --------- | --------------- | ----------- |
| {date} | {source file or URL} | INTEGRATE | {plan/decision} | {one line}  |
| {date} | {source}             | MONITOR   | —               | {one line}  |
| {date} | {source}             | DISMISS   | —               | {rationale} |

## Monitoring List

| Finding         | Source          | Revisit Trigger | Added  |
| --------------- | --------------- | --------------- | ------ |
| {what to watch} | {from research} | {condition}     | {date} |
```

## Ingest Report

```
Ingest Report — {date}
=====================

Input: {what was triaged}

Results:
  INTEGRATE: {N} findings → linked to {durable plans or decisions}
  MONITOR:   {N} findings → revisit triggers set
  REFERENCE: {N} findings → filed
  DISMISS:   {N} findings → rationale noted

Key Actions:
  1. {specific next step for top INTEGRATE finding}
  2. {specific next step}

Monitoring Watchlist: {N} items active
```

## Workflow Chain

- If ingest produces INTEGRATE items → `maestro:plan` or
  `maestro:decide`
- If ingest updates monitoring list → `maestro:commit`
- If batch ingest reveals patterns → `maestro:think` (synthesize
  mode)
- Triggered after `maestro:research` and `maestro:pulse`

## Boundaries

- Ingest evaluates — it doesn't generate new research (that's
  `maestro:research`)
- `--pulse` is narrower than `--batch` — it only sweeps pulse
  outputs
- Don't create plans during ingest — flag that a plan update is
  needed
- Don't make decisions during ingest — flag that a decision is
  needed
- Force a classification — "maybe" and "interesting" are not
  verdicts
- Be honest about dismissals — document why, don't just ignore
- The monitoring list is not a graveyard — review and prune
  monthly

---

END OF COMMAND
