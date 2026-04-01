# Harness Engineering: Self-Verification & Backpressure

## Principle

Give the agent feedback loops, not just instructions. The more backpressure you
capture, the more autonomy the agent can safely exercise.

## Upstream Backpressure (Before Implementation)

These guide the agent toward correct implementations:

- Existing code patterns in the codebase (read before writing)
- Architecture rules in `CLAUDE.md`
- Performance contracts (per maestro.toml)
- Five critical patterns (`.claude/rules/01-core-patterns.md`)
- Type system and trait boundaries enforce correct structure

## Downstream Backpressure (After Implementation)

These reject invalid work automatically:

- git status --short — type errors caught immediately
- cargo clippy — lint violations blocked
- cargo test — behavior regressions caught
- cargo fmt --check — style violations caught
- Post-tool hooks: `check-unwrap.sh`, `format-rust.sh`, `validate-syntax.sh`
- `const_assert!` — performance contracts enforced at compile time

## Verification Loop (Ralph Wiggum Pattern)

When verification fails, iterate — do NOT stop and report failure:

1. Run verification (`just check && just test`)
2. If failure: analyze error, fix root cause, re-verify
3. Repeat until all checks pass OR 3 iterations fail on the same issue
4. After 3 failures on same issue: reconsider approach entirely

## Doom Loop Detection

If you've edited the same file 5+ times without tests passing, STOP and:

1. Re-read the specification and acceptance criteria
2. Check if your approach is fundamentally wrong
3. Consider an alternative implementation strategy
4. If still stuck: report the blocker with evidence

## Pre-Completion Checklist (Implicit Gate)

Before reporting ANY task as done, silently verify ALL of these. If any fail,
fix before reporting — do NOT report failure, fix it (Ralph Wiggum pattern):

- [ ] git status --short passes
- [ ] cargo test passes
- [ ] cargo clippy clean
- [ ] No `.unwrap()` in modified production files
- [ ] Modified files are within scope boundaries
- [ ] Codify check done (pattern/rule/context update if needed)

This checklist is implicit — do not print it, just verify it. The post-tool
hooks (`check-unwrap.sh`, `format-rust.sh`, `validate-syntax.sh`) enforce a
subset automatically. The rest is your responsibility.

## Proactive Workflow Awareness

After completing work, consult `.claude/rules/13-workflow-orchestration.md` to
determine what to trigger next. Do not wait to be asked. If verification passed
and the change is ready, suggest `/maestro:commit` and then `/maestro:push`.

## Feedback Codifier Pattern

When a PR review catches something the agent missed, or when a mistake is
repeated across sessions:

1. Extract the lesson as a concrete rule
2. Add it to the appropriate `.claude/rules/` file
3. If it's a new pattern: add to `MEMORY.md` topic index
4. If it's a codebase-specific convention: add to `docs/` folder

The next agent session inherits the lesson automatically. This is what makes the
system self-improving — every mistake becomes a permanent fix.

## Context Quality Hierarchy

When constructing context for subagents or planning:

1. **Incorrect information** is worst (removes before adding)
2. **Missing information** is bad (search before assuming)
3. **Excessive noise** wastes tokens (keep context focused)

Optimize for correctness first, then completeness, then brevity.
