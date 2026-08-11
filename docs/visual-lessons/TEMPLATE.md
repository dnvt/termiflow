# Visual lesson: <short human-readable symptom>

Status: Observation | Hypothesis | Falsified | Promoted rule | Watch
Source epoch: <cycle or commit identifier>
Fixture family: `<fixture stem or family>`

## Observation

Describe exactly what a human sees, including one-cell, one-row, arrowhead,
border, spacing, label, or route-ownership details. Do not begin with the
suspected implementation layer.

## Hypothesis and falsifier

State the smallest code or policy change that should explain the observation.
Name the result that would falsify it, including negative controls and the
homolog directions/styles/modes that must remain safe.

## Evidence

- Focused regression or independent oracle: `<repository path and command>`
- Complete-corpus result: `<inputs / packet rows / renderable rows / errors>`
- Human-eye result: `<one-frame decisions and remaining watches>`
- Golden result: `<check or explicit approval record>`

Transient packets, ledgers, private Maestro state, model prompts, and local
absolute paths do not belong here. Record their cycle identifier and digest in
private QA state when needed; keep this page reproducible from repository
paths and commands.

## Promoted rule or next experiment

Record the reusable renderer rule, or the smallest next experiment with its
acceptance predicate. A clean machine critic is not sufficient to close a
human-eye watch.
