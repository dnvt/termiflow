# Maestro Research Guide

**Purpose**: `/maestro:research` is for targeted engineering and scientific
investigation, not for generic internet wandering.

## What Good Research Looks Like

Good research is:

- question-led
- proportionate to the decision it informs
- explicit about source quality
- connected back to current repo work

Bad research is:

- broad without a decision or plan in mind
- padded with low-signal sources
- detached from current architecture or roadmap reality

## Evidence Mix

Start with the cheapest high-signal sources:

1. prior repo analysis
2. design packets and experiment logs
3. benchmark or telemetry evidence
4. primary external sources
5. practitioner examples or comparable systems

Do not skip existing repo knowledge and re-research what is already known.

## Research Questions That Fit

- What does the literature say about a specific loss design?
- How do comparable systems handle telemetry freshness or checkpoint selection?
- What prior experiments inside the repo already constrain this choice?
- What evidence would justify changing a gate, threshold, or workflow policy?

## Output Discipline

A research brief should end with an action recommendation:

- decide
- plan
- ingest
- monitor
- no action

If the work is mostly external intelligence collection on an ongoing cadence,
prefer `/maestro:pulse`. If it needs taxonomy placement and triage, hand off to
`/maestro:ingest`.
