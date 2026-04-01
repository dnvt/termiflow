# Maestro Run Guide

`/maestro:run` is for scoped execution with evidence, verification, and review.

## Default Loop

1. Load the task or requested scope
2. Implement the smallest defensible change
3. Run the configured verification commands
4. Fix failures until the result is stable
5. Trigger review before reporting done

## Good Run Behavior

- work from actual repo state, not assumptions
- keep edits inside the requested scope
- report commands actually run
- surface blockers with evidence, not vague summaries

If the work becomes mostly about learning or trade-offs, stop and switch to
`/maestro:think`, `/maestro:research`, or `/maestro:decide`.
