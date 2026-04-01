# Agent Coordination

## Core Principle

Parallel agents are useful only when their outputs converge into
one coherent answer. Coordinate through shared state and bounded
prompts, not by letting agents contaminate each other's framing.

## When To Use Multiple Agents

- 1 agent: default. Most sessions need focus, not spread
- 2 agents: when you need both construction and challenge
- 3+ agents: only when the threads are genuinely independent and
  will be synthesized later

For 1-2 agents, collect results directly. Use the file protocol
below only when coordination itself needs to survive across
multiple turns or be handed off cleanly.

## Coordination Files

When durable coordination is warranted, use:

```text
context/coordination/
├── active-agents.md
├── findings-ledger.md
└── handoff-queue.md
```

- `active-agents.md`: who is working on what
- `findings-ledger.md`: structured findings, append-only
- `handoff-queue.md`: optional follow-up work discovered by an
  agent

## Protocol

### 1. Initialize

Create durable coordination files only when the work spans
multiple turns, needs a handoff, or the findings themselves are
worth preserving.

### 2. Dispatch

Each agent gets:

- a focused, bounded prompt
- only the context it needs
- a clear output format

### 3. Collect

Update the registry as agents complete and append findings in a
structured format:

```markdown
## [Agent Name] — {Focus Area}

**Finding:** {What they discovered}
**Confidence:** High | Medium | Low
**Depends on:** {Open questions, if any}
**Contradicts:** {Flag if this conflicts with another finding}
```

### 4. Synthesize

The main thread must:

- wait for all required agents before synthesis
- resolve contradictions explicitly
- deduplicate overlapping findings
- group outcomes into fix now, investigate, or defer

### 5. Cleanup

After synthesis:

- remove expired coordination files when they are no longer useful, or
- move them to `context/coordination/archive/` if the record is
  worth keeping

Do not leave stale coordination files in the active directory.

## Handoff Rule

Agents pass findings, not full reasoning chains. Follow-up agents
should receive the task plus the relevant findings from prior
agents, not the prior agent's whole internal framing.

## Anti-Patterns

- Coordination overhead exceeds thinking value
- All agents receive the same framing
- Synthesis begins before all required agents finish
- Agent sprawl: more than 4 parallel agents
- Durable clutter from expired coordination files

## Conflict Resolution

- If multiple agents flag the same critical issue, treat it as
  high-confidence
- If agents disagree, prefer the one with stronger evidence and
  report the disagreement
- Never silently average away a contradiction
