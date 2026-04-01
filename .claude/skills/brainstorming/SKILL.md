---
name: brainstorming
description: Use when starting any non-trivial work that could benefit from design exploration before implementation — new features, architectural changes, unclear requirements, or when the user says "brainstorm", "explore options", "think about", or "design"
---

# Brainstorming

**Refine ideas through structured exploration before implementation, planning,
or decision-making hardens the wrong frame.**

---

## When to Use

- User requests design exploration ("brainstorm", "think about", "what if")
- Starting a new feature or epic with unclear requirements
- Architectural decisions spanning 2+ crates
- Any work where jumping straight to code would be premature
- Before `/maestro:run` or `/maestro:plan` when the scope or approach is
  uncertain
- Before `/maestro:decide` when the option set is still too narrow

## Core Principle

**No implementation until the problem and option space are clear enough to earn
it.** Apparent simplicity often hides the strongest untested assumptions.

---

## Thinking Modes

Use these modes deliberately instead of treating all ideation the same:

### Diverge

- Generate more options before judging
- Use constraints, reversals, and analogies
- Push past the first obvious answers

### Converge

- Apply explicit criteria
- Force-rank options
- Name what is being cut and why

### Challenge

- Steel-man the strongest opposing case
- Ask what would have to be true for the leading idea to fail
- Surface hidden dependencies and weak assumptions

### Synthesize

- Connect threads across design packets, review findings, and prior analyses
- Name contradictions before resolving them
- Produce one coherent framing from scattered evidence

### Structure

- Turn fuzzy ideas into clear buckets and boundaries
- Prefer simple matrices, lists, and ordered hypotheses
- Avoid frameworks that are harder to explain than the idea itself

---

## The Process

### Phase 1: Explore Context (Silent)

Before asking any questions, silently gather context:

- Read relevant crate source code, specs, and progress files
- Check `context/current-task.md` for active work when it exists
- Understand the architecture boundaries (see `ARCHITECTURE.md`)
- Identify related prior decisions in `decisions/`
- Check for existing design packets, decision matrices, or analysis notes

### Phase 2: Clarify (Interactive — One Question at a Time)

Ask the user clarifying questions to narrow the design space:

- **One question per message** — avoid overwhelming with multi-part questions
- **Prefer multiple choice** — easier to answer than open-ended
- **YAGNI ruthlessly** — if a feature isn't needed now, cut it
- **Stop when clear** — don't ask questions you can answer from the codebase

Common clarification areas:

- Scope boundaries (which crates? which API surface?)
- Performance constraints (is this on the hot path?)
- User-facing vs. internal change
- Compatibility requirements (breaking change acceptable?)

### Phase 3: Propose Approaches (2-3 Options)

Present 2-3 concrete approaches with trade-offs:

```markdown
## Approach A: {Name}

**How**: {1-2 sentence implementation summary} **Pros**: {key advantages}
**Cons**: {key disadvantages} **Effort**: {relative: small/medium/large}
**Risk**: {what could go wrong}

## Approach B: {Name}

...

## Recommendation: {A or B} because {one-sentence rationale}
```

Let the user choose or propose a hybrid. Do not proceed until they approve.

### Phase 4: Design Summary

After approach approval, present a concise design:

```markdown
## Design: {Feature Name}

**Approach**: {chosen approach} **Crates affected**: {list} **API changes**:
{new endpoints, modified traits, etc.} **Key decisions**: {2-3 bullets}
**Acceptance criteria**: {measurable outcomes}
```

### Phase 5: Handoff

Once the user approves the design:

- **For epic-level scope** → invoke `planning` skill to generate the full
  spec
- **For a concrete trade-off** → switch to `/maestro:decide`
- **For an evidence gap** → switch to `/maestro:research`
- **For feature/task scope** → proceed directly to `/maestro:run`
- **If uncertain** → create a design packet in `docs/design/` before
  implementing

---

## Techniques

### Constraint Shift

- What if we had to ship in one week?
- What if we had one tenth of the budget?
- What if this had to work with current telemetry only?

### Reversal

- How would we make this fail?
- What would guarantee a performance regression?
- What would make the resulting system impossible to operate?

Reverse those answers into design signals.

### First Principles

Ask:

1. What do we know from evidence?
2. What are we assuming?
3. Which options survive if we keep only the evidence-backed truths?

### 10x / 0.1x Thinking

- What breaks if scale increases tenfold?
- What would we cut if resources dropped by ninety percent?

### Worst Possible Idea

Generate obviously bad ideas to break anchoring and expose implicit quality
criteria.

## Rationalization Prevention

| Excuse                             | Reality                                                                                 |
| ---------------------------------- | --------------------------------------------------------------------------------------- |
| "This is too simple to brainstorm" | Simple changes have hidden assumptions. 5 minutes of design saves 30 minutes of rework. |
| "I already know the approach"      | Present it anyway. The user may have context you don't.                                 |
| "The user wants speed"             | Rework is slower than design. Ask one question, not zero.                               |
| "It's just a bug fix"              | Bug fixes don't need brainstorming. But "fix" that changes behavior does.               |

## Red Flags (Stop and Reconsider)

- Writing code before the user approved an approach
- Asking 3+ questions in one message
- Proposing only one approach (always offer alternatives)
- Skipping the design summary for non-trivial changes
- Adding features the user didn't ask for (YAGNI)

---

## Integration with Other Skills

- **planning**: Brainstorming feeds into planning for large scope work
- **decision-logging**: use when the discussion turns into a durable choice
- **milestone-execution**: For feature-level work, brainstorming → direct
  implementation
- **architecture-review**: Invoke when brainstorming reveals cross-crate
  implications
- **tdd-workflow**: After brainstorming, implementation follows TDD
- **writing-tone**: use for captures, summaries, and user-facing synthesis

---

**Skill Version**: 1.0.0 **Last Updated**: 2026-03-18 **Related Skills**:
planning, decision-logging, milestone-execution, architecture-review
