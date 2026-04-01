# Maestro Start Guide

`/maestro:start` is for loading the current repo context before doing work.

## What To Load

- core project docs from `maestro.toml`
- current task and blockers
- latest session checkpoint, if present
- generated rules, skills, and agents
- current git state

## Readiness Check

Before moving on, you should know:

- what the active task is
- what constraints or rules apply
- what verification is expected
- which commands or packs are relevant next

If the context is still fuzzy after `/maestro:start`, go to `/maestro:think`
before implementation.
