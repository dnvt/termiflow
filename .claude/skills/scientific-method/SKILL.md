---
name: scientific-method
description:
  Use when a plan, decision, review, or research thread needs
  explicit hypotheses, evidence quality, measurement, or
  experiment design.
version: 1.0.0
allowed-tools: [Read, Grep, Glob, Write]
---

# Scientific Method

Adds engineering and scientific rigor to strategic work. Use this
skill when the problem is not just "what should we say?" but
"what do we actually know, what are we assuming, and how would we
learn fast enough to change course?"

## Evidence Ladder

Rate evidence before leaning on it:

1. **Direct observation** — measured behavior, real user signal,
   actual results
2. **Primary source** — original research, interviews, raw data,
   first-hand logs
3. **Secondary synthesis** — thoughtful summaries or credible
   analysts
4. **Anecdote / opinion** — interesting, but not strong enough to
   carry the decision alone

When evidence is mixed, say so.

## Experiment Log

For meaningful uncertainty, capture:

```markdown
## Experiment Log

**Hypothesis:** {what we believe}
**Intervention:** {what we will test or change}
**Expected Observation:** {what would support or refute it}
**Actual Observation:** {what happened}
**Conclusion:** {continue, pivot, stop, or defer}
```

The point is not ceremony. The point is to keep the work
falsifiable and learnable.

## Measurement Discipline

### Measure What Matters

- Prefer outcome measures over activity counts
- If using a proxy, name it as a proxy
- Distinguish leading indicators from true success criteria

### Decide What Would Change Your Mind

Before acting, ask:

- What result would confirm this?
- What result would weaken it?
- What result would kill it?

### Capture Negative Results

Dead ends, failed tests, and weak signals are still evidence.
Write them down so they stop being rediscovered as "new" ideas.

## Decision Matrix Trigger

Escalate to an explicit matrix when:

- the decision is hard to reverse
- several viable options exist
- trade-offs span multiple dimensions
- the team keeps relitigating the same question

Use rows such as:

- impact
- effort
- reversibility
- evidence strength
- strategic alignment
- downside risk

## When to Use This Skill

- A plan depends on a strong but untested assumption
- Research should end in an experiment or a real decision
- A review needs to check whether claims are evidence-backed
- A run is intended to learn, not just produce a deliverable
- The team needs a tighter line between facts, beliefs, and bets

## Integration Points

- `/maestro:plan` uses this skill for high-uncertainty plans and
  bets
- `/maestro:research` uses it to rate evidence quality and name
  what remains uncertain
- `/maestro:review` uses it to audit claims, success criteria, and
  hidden assumptions
- `/maestro:run` uses it when the deliverable is an experiment,
  memo, or evidence-backed recommendation
- `/maestro:decide` pairs with this skill when the decision should
  include what evidence would reopen it

## Companion Resources

- `templates/experiment-log-template.md` — compact experiment
  section for plans, reviews, or research notes
- `examples/experiment-note-example.md` — worked example of a
  hypothesis turning into a decision
