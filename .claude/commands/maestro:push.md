# /maestro:push - Push Branch and Update/Create PR

EXECUTE IMMEDIATELY: push the current branch and create or update a PR using the
repo template, then report the PR URL.

## Context Loading

- IF `--help` is present: REPORT purpose, flags, inputs, outputs, examples, and
  related commands, then STOP
- READ working tree state and current branch
- READ `.github/pull_request_template.md` for PR body structure
- READ `context/current-task.md` for task context in PR description
- CHECK for existing PR on this branch

## Exit Criteria (ALL must be true)

- Working tree is clean (no uncommitted changes)
- Branch is not `main` or `master` (unless user explicitly confirms)
- Branch is pushed to remote with upstream tracking
- PR exists on GitHub (created or updated) with template-based body
- PR body includes Summary derived from actual commits (not memory)
- Issue link attempted (non-blocking)

## Verification

```bash
git status --porcelain              # must be empty
git branch --show-current           # must not be main/master
git log --oneline origin/HEAD..HEAD # must be empty after push
gh pr view --json number,url        # must return valid PR
```

## Boundaries

- Never force push
- Never edit PR body without including template sections
- Never mark tests or performance checkboxes without evidence
- Verify PR description against actual diff — read the diff, don't summarize
  from memory
- Do not auto-close issues — use `Refs #N`, not `Closes #N`, unless explicitly
  told work is complete

## Pre-Push Gate (Auto)

Before pushing, inspect the diff against the base branch and run targeted checks
based on what actually changed. This replaces the former `/ready` command.

```bash
git diff --name-only $(git merge-base HEAD main)..HEAD
```

| Files Changed              | Check Triggered                   |
| -------------------------- | --------------------------------- |
| `src/*/src/**`  | Compile + clippy + test           |
| `**/auth*`, `**/privacy*`  | Zero-unwrap scan + auth patterns  |
| `**/scoring*`, `**/gop*`   | Performance contract validation   |
| `**/streaming*`, `**/ws*`  | WebSocket contract validation     |
| `Cargo.toml`, `Cargo.lock` | Dependency vulnerability check    |
| Any Rust files             | Zero-unwrap scan on changed files |

Run all applicable checks. If any FAIL, report blocking issues and do NOT push.
Use `--no-gate` to skip the pre-push gate (e.g., docs-only changes).

```
PRE-PUSH GATE: READY | NOT READY
Blocking: {list or "None"}
```

## Execution

### Push

```bash
git push -u origin HEAD
```

If upstream already exists, push without `-u`.

### Base Branch

Ask the user for the PR base branch. Default to `main` if
unspecified.

### Detect Existing PR

```bash
gh pr view --json number,title,body,baseRefName,headRefName
```

If PR exists → update. If not → create.

### Build PR Body

Use `.github/pull_request_template.md` as source:

- Fill **Summary** from latest commits
- Add **Progress Update** with timestamp and commit bullets, cross-referenced
  with `context/current-task.md`
- Leave checkboxes unchecked unless explicitly confirmed
- For existing PRs: preserve existing body, append progress update

### Create or Update

```bash
# New PR
gh pr create --base <base> --title "<title>" --body "<body>"

# Existing PR
gh pr edit <number> --body "<updated_body>"
```

Title: use most recent commit summary or branch convention. Keep conventional
format if already used (e.g., `feat(api): ...`).

### Issue Link (Non-Blocking)

1. Extract work ID from branch or commits (patterns: `feat:1e5`, `epic/2s`)
2. Search:
   `gh issue list --search "in:title {ID}" --json number,title --limit 3`
3. If found and not already referenced: add `Refs #{N}` to PR body
4. If not found: suggest creating an issue via `gh issue create`

### Version Advisory (Non-Blocking)

If 10+ commits since last tag, print version advisory (non-blocking).

## Report

```
PR #{number}: {title}
URL: {url}
Base: {base} ← {branch}
Issue: #{N} linked | No issue found
Version: {advisory or "N/A"}
```

## Usage

```bash
/maestro:push                # Auto-detect and gate
/maestro:push --no-gate      # Skip pre-push gate (docs-only, etc.)
/maestro:push --deep         # Full pre-push gate: all domain checks + dep audit
/maestro:push --help         # Show compact help and stop
```

`--deep`: Runs every check in the detection table regardless of diff scope —
compile, clippy, test, security scan, perf contracts, dep audit, unwrap scan,
and docs. Use before important PRs.

## Workflow Chain

**Before**: `/maestro:commit` completed, working tree clean **After**:

- IF PR created/updated → monitor CI, respond to review feedback
- IF dirty tree → `/maestro:commit` first
- IF CI fails → fix → `/maestro:commit` → re-run `/maestro:push`

**Related**: `/maestro:commit`, `/maestro:run`

---

END OF COMMAND
