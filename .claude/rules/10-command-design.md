# Command Design: Constraints Over Instructions

## Principle

Define **exit criteria and boundaries**, not step-by-step checklists. Agents
fixate on numbered lists and ignore anything not on them. Constraints let the
agent find its own path to the goal.

## Command Structure (L6 Pattern)

Commands should contain these sections in order:

1. **Context Loading** — what to read before starting (brief)
2. **Exit Criteria** — ALL conditions that must be true when done
3. **Boundaries** — what NOT to do, scope limits
4. **Verification** — how to confirm exit criteria are met
5. **Report** — what to output when done

## Anti-Patterns

- Numbered step lists ("1. Do X, 2. Do Y, 3. Do Z") — agent fixates on list
- Mixing instructions with constraints — keep them separate
- Instructions without exit criteria — no way to know when done
- Over-specifying HOW instead of defining WHAT done looks like

## Compounding Loop

Every `/maestro:commit` includes a codify check:

1. Discovered a reusable pattern? → Update `MEMORY.md` or `.claude/rules/`
2. Encountered a repeatable mistake? → Add or update a rule
3. Context needed for future sessions? → Update `context/current-task.md`

## Decoupled Review

For `/maestro:review`: spawn a separate agent that did NOT write the code. Same
model reviewing its own work produces biased self-evaluation.

## Session Wrap (Mandatory)

Every major workflow command (`/maestro:run`, `/maestro:review`) MUST end by
executing the session wrap protocol inline. This is not optional:

1. Gather session state (branch, commits, working tree)
2. Write checkpoint to `context/session-checkpoint.md`
3. Run codify check (pattern/rule/context updates)
4. Report: "Session wrapped. Resume with `/maestro:start`."

This ensures cross-session continuity and prevents context loss.
