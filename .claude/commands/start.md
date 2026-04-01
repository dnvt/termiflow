# /start - TermiFlow Session Initialization

Initialize a development session with codebase context and health check.

## Instructions

Perform the following initialization sequence:

### 1. Verify Build Health
Run `cargo build` and `cargo test` to ensure the codebase is in a working state. Report any failures immediately.

### 2. Check Git Status
Run `git status` and `git log --oneline -5` to understand:
- Current branch
- Any uncommitted changes
- Recent commit history

### 3. Summarize Current State
Provide a brief summary including:
- Test count and pass/fail status
- Any clippy warnings (`cargo clippy 2>&1 | grep -c warning` for count)
- Current working features based on recent commits

### 4. Review Active Planning
Check `planning/PLAN.md` for:
- Current phase and next steps
- Any blocked items
- Quick wins available

### 5. Ready Report
Output a ready report in this format:

```
## TermiFlow Session Ready

**Branch**: [branch name]
**Tests**: [X] passing
**Warnings**: [Y] clippy warnings
**Status**: [Clean/Dirty] working tree

**Current Phase**: [from PLAN.md]
**Next Steps**: [1-2 immediate actions]

Ready to work on: [suggested focus area]
```

## Context Files
- `CLAUDE.md` - Architecture and commands reference
- `planning/PLAN.md` - Current roadmap
- `tests/fixtures/README.md` - Golden test documentation
