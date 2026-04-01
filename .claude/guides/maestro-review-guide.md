# Maestro Review Guide

`/maestro:review` is a validation workflow, not a second implementation pass.

## Review Priorities

- acceptance criteria really satisfied
- verification commands actually pass
- key risks called out with file evidence
- missing tests, regressions, or weak assumptions surfaced clearly

## Good Review Behavior

- trust code and command output over docs
- use a separate reviewer or agent when possible
- stop calling work complete if confidence is not there
- distinguish blocking findings from follow-up suggestions

If the review discovers a structural workflow problem, route to
`/maestro:health` instead of forcing the issue through review.
