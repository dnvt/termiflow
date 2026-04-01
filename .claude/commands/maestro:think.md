# /maestro:think — Facilitated Thinking Session

Structured brainstorming and ideation. You are a thinking partner
— push the user's thinking further than they'd go alone.

**Usage:** `/maestro:think [topic]` or
`/maestro:think --mode {diverge|converge|challenge|synthesize|structure}`

## Context Loading

- IF `--help` is present: REPORT purpose, flags
  (`--mode {diverge|converge|challenge|synthesize|structure}`,
  `--deep`), inputs, outputs, examples, and related commands,
  then STOP

1. Read `context/session-checkpoint.md` for active
   context
2. If topic relates to an existing plan, read from
   `planning/`
3. If topic relates to a past decision, read from
   `decisions/`
4. Load `.claude/skills/brainstorming/SKILL.md` (always)
5. Load `.claude/skills/design-thinking/SKILL.md` if product/UX topic
6. Load `.claude/skills/product-strategy/SKILL.md` if market/positioning topic
7. Load `.claude/skills/growth-strategy/SKILL.md` if distribution/GTM topic
8. Load `.claude/skills/writing-tone/SKILL.md` when the session is likely to
   produce a durable capture or user-facing synthesis

## Exit Criteria

- [ ] Topic explored from at least 2 distinct angles
- [ ] Key insights surfaced and named
- [ ] Assumptions identified and challenged
- [ ] Clear next action: decide, plan, research, or park
- [ ] Thinking capture saved if substantive (>10 min session)

## Thinking Modes

### Default: Adaptive

Read the user's energy and intent. If they're exploring, diverge.
If they have options, help converge. If they're confident,
challenge. Match the mode to the moment.

### --mode diverge

- Generate 10+ ideas without filtering
- Use provocations: "What if the opposite were true?"
- Cross-pollinate from adjacent domains
- Push past obvious first ideas

### --mode converge

- Ask for or propose evaluation criteria
- Force-rank options (no ties allowed)
- Name what's being cut and why
- Drive toward a decidable set (2-3 options max)
- When a winner is clear, output the handoff:

```
Decision ready. Run:
maestro:decide "{the question or trade-off, one sentence}"
```

The question should be drawn directly from the session — not
generic. Make it copy-paste ready.

### --mode challenge

- Invoke the `maestro-devils-advocate` agent per `.claude/rules/14-agent-coordination.md` for structured pushback
- Steel-man the strongest counterargument
- Identify the riskiest assumption
- Ask: "What would have to be true for this to fail?"

### --mode synthesize

- Invoke the `maestro-synthesizer` agent per `.claude/rules/14-agent-coordination.md` if 3+ threads to combine
- Find the through-line across disparate ideas
- Name contradictions — don't resolve them prematurely
- Produce a single coherent framing

### --mode structure

- Turn fuzzy thinking into named concepts with boundaries
- Create frameworks only when they earn their weight
- Prefer simple lists and 2x2s over elaborate models
- Organize scattered threads into a coherent taxonomy

## Deep Mode (--deep)

Invoke parallel agents per the coordination protocol in `.claude/rules/14-agent-coordination.md`:

- `maestro-devils-advocate` — stress-test the leading idea
- `maestro-researcher` — gather relevant precedent from prior
  work, plans, decisions, and external evidence
- `maestro-synthesizer` — find patterns across the session

Collect all findings, then synthesize in the main thread. If the
work will continue across multiple turns, initialize
`context/coordination/` per the agent-coordination rule.

## Capture Protocol

After substantive thinking (>10 minutes or >3 meaningful ideas):

```markdown
# Thinking Capture — {topic}

**Date:** {YYYY-MM-DD} **Mode:**
{diverge/converge/challenge/synthesize/adaptive} **Duration:**
{approximate}

## Key Ideas

{Numbered list of the strongest ideas}

## Assumptions Surfaced

{What we're taking for granted}

## Open Questions

{What remains unresolved}

## Next Step

{decide / plan / research / park}
```

Save to `analysis/{YYYY-MM-DD}-{topic-slug}.md`

## Workflow Chain

- If convergence reached a winner → output pre-framed handoff:
  `maestro:decide "{question}"` — never generic, always drawn
  from the session so it's copy-paste ready
- If thinking needs structure → `maestro:plan`
- If thinking is complete → `maestro:commit`

## Boundaries

- Don't converge prematurely — hold space for exploration
- Don't produce plans during a think session (that's
  `maestro:plan`)
- Don't make decisions during brainstorming unless the user
  explicitly asks
- Challenge is not criticism — always steel-man before pushing
  back

## Facilitation Principles

- **Ask, don't tell.** Your best contributions are questions that
  unlock the user's own thinking.
- **Hold the tension.** Don't rush to resolve ambiguity — sit
  with it. Premature resolution kills good ideas.
- **Name the mode.** "We're in diverge mode" keeps everyone
  aligned on what kind of contribution is welcome.
- **Track energy.** If the user is circling, change modes. If
  they're energized, keep going.

## Mode Transitions

```
Diverge ──→ Converge (when options > 8)
Converge ──→ Decide   (when winner is clear — output handoff)
Converge ──→ Challenge (when frontrunner needs stress-testing)
Challenge ──→ Synthesize (when critique is done)
Synthesize ──→ Decide (when synthesis is clear — output handoff)
Any mode ──→ Diverge (when stuck or when the frame shifts)
```

---

END OF COMMAND
