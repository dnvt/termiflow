# Memory and Agent Usage

## Before Starting Non-Trivial Work

1. Scan the MEMORY.md topic index for relevant prior findings
2. If a topic file matches the work area, Read it before proceeding
3. Check `context/current-task.md` for active task context
4. Check if a skill or `just` command already handles the task before building
   manually

## When to Use Explore Agents

Spawn an Explore agent (Task tool, subagent_type=Explore) instead of manual
Grep/Glob chains when:

- The question spans 3+ files across 2+ modules or subsystems
- You need to understand a flow end-to-end across components
- You need to trace how a concept or interface is used across the repo

Do NOT spawn agents for:

- Single-file lookups or focused search within one known module
- Tasks completable in fewer than 3 tool calls

## Parallel Agent Pattern

When 2+ independent questions need answers, spawn parallel Explore agents in a
single message:

- Each agent gets a focused, specific prompt
- Collect all results before synthesizing
- Prefer 2 focused agents over 1 broad agent

## After Completing Significant Work

Update memory when you discover:

- A reusable pattern or codebase insight (write to relevant topic file)
- A correction to a previous memory entry (update in place)
- A new area of understanding not yet captured (create topic file, update index)

Do NOT update memory for:

- Routine code changes with no novel insight
- Information already recorded in progress/ or docs/ files
