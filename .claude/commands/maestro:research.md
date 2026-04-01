# /maestro:research - Targeted Engineering Research

EXECUTE IMMEDIATELY: investigate a focused question using repo evidence,
existing analyses, and external sources as needed, then save a decision-ready
research brief.

## Context Loading

- IF `--help` is present: REPORT purpose, flags, inputs, outputs, examples, and
  related commands, then STOP
- READ `context/current-task.md`
- READ related decisions from `decisions/` when they constrain the question
- READ `planning/PLAN.md` if the question affects sequencing or scope
- SEARCH `analysis/` for related prior work before gathering new evidence
- CHECK related design packets and decision documents when they exist
- LOAD `brainstorming` to sharpen the question and sub-questions
- LOAD `decision-logging` if the research is supporting an active decision
- LOAD `writing-tone` for the final brief

## Exit Criteria (ALL must be true)

- The research question is explicit before evidence gathering starts
- Evidence was collected from multiple sources or artifact types
- Findings are attributed and relevance is clear
- Convergence, divergence, and notable gaps are identified
- A research brief is saved to `analysis/research/`
- The brief ends with a concrete recommendation: decide, plan, ingest, monitor,
  or no action

## Research Protocol

### 1. Frame The Question

Turn the request into 3-5 answerable sub-questions.

### 2. Gather Evidence

Use the lightest evidence mix that can answer the question well:

- existing repo analysis
- design packets and experiment logs
- benchmarks and metrics
- primary external sources
- practitioner commentary or comparable systems

### 3. Evaluate Source Quality

For each meaningful source, assess:

- credibility
- recency
- evidence strength
- relevance
- novelty

### 4. Synthesize

Do not list raw findings only. Explain:

- where the evidence agrees
- where it conflicts
- what surprised you
- what this changes in the current work

## Deep Mode (--deep)

Spawn 3 focused explore agents in parallel:

1. Primary evidence and prior-art scan
2. Practitioner patterns and implementation precedents
3. Counterargument and weak-signal review

The main thread synthesizes the results into one brief.

## Output Format

Save to
`analysis/research/{YYYY-MM-DD}-{topic-slug}-research.md`.

```markdown
# Research Brief: {Topic}

**Date:** {YYYY-MM-DD}
**Prompted by:** {task, plan, or decision}
**Sub-questions:** ...

## Executive Summary

...

## Findings

### Finding 1

**Source:** ...
**Relevance:** ...
**Confidence:** High | Medium | Low
**Detail:** ...

## Convergence And Divergence

...

## Recommendation

- Decide
- Plan
- Ingest
- Monitor
- No action
```

If the output is external intelligence that belongs in the intake taxonomy,
handoff to `/maestro:ingest`.

## Usage

```bash
/maestro:research batch_size scaling for Path A fine-tuning
/maestro:research --deep speech scoring ordinal loss literature
/maestro:research compare telemetry freshness patterns across training systems
/maestro:research --help
```

## Workflow Chain

**Before**: evidence gap, disputed assumption, unfamiliar design space

**After**:

- IF the findings require intake triage → `/maestro:ingest`
- IF the findings make a choice possible → `/maestro:decide`
- IF the findings reshape scope → `/maestro:plan`
- IF the findings clarify implementation → `/maestro:run`

**Related**: `/maestro:pulse`, `/maestro:ingest`, `/maestro:decide`

## Reference

- `.claude/guides/maestro-research-guide.md`

---

END OF COMMAND
