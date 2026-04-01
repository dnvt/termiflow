# /maestro:publish - Publish Maestro & Sync Consumers

EXECUTE IMMEDIATELY: commit and push changes in the `.maestro/` submodule, then
update all consuming repos that use it.

## Context Loading

- IF `--help` is present: REPORT purpose, flags, inputs, outputs, examples, and
  related commands, then STOP
- READ `.maestro/` submodule status: `cd .maestro && git status --short`
- READ `.maestro/` recent commits: `cd .maestro && git log --oneline -5`
- IDENTIFY consumer repos by scanning sibling directories for `.gitmodules`
  entries containing `.maestro`:
  ```bash
  for dir in "$(dirname "$(pwd)")"/*/.gitmodules; do
    grep -l '\.maestro' "$dir" 2>/dev/null
  done
  ```

## Exit Criteria (ALL must be true)

- All `.maestro/` changes are committed with a descriptive message
- `.maestro/` is pushed to `origin main`
- Each consumer repo has its submodule pointer updated to the new commit
- Each consumer repo has `.claude/` regenerated via `.maestro/generate.sh`
- Summary report shows per-consumer status (updated / failed / already current)

## Boundaries

- Do NOT commit anything in consumer repos — only update submodule + regenerate.
  Report what's ready to commit and let the user decide.
- Do NOT force-push or rebase the maestro repo
- Do NOT modify files outside `.maestro/` and `.claude/` in consumers
- If `.maestro/` has no changes AND `--sync-only` was not specified, STOP and
  report "nothing to publish"
- If a consumer's `generate.sh` fails, report the failure and continue to the
  next consumer

## Process

### Step 1: Commit `.maestro/` changes

```bash
cd .maestro
git add -A
git status --short  # Show what will be committed
```

Ask the user to confirm or provide a commit message. If the user provided one
inline (e.g., `/maestro:publish "message here"`), use it directly.

### Step 2: Push to origin

```bash
cd .maestro
git push origin main
```

### Step 3: Update each consumer

For each discovered consumer repo:

```bash
cd <consumer>/.maestro
git fetch origin main --quiet
git checkout main --quiet
git pull origin main --quiet
cd <consumer>
.maestro/generate.sh
git status --short .maestro .claude/
```

### Step 4: Report

```
## Maestro Published

Commit: <hash> "<message>"
Pushed: origin/main

### Consumers
| Repo | Submodule | Regenerated | Files Changed |
|------|-----------|-------------|---------------|
| repo-a | ✅ | ✅ | N files |
| repo-b | ⚠️ | ✅ | N files |

Next: commit submodule pointer in each consumer.
```

## Flags

| Flag           | Behavior                                           |
| -------------- | -------------------------------------------------- |
| `--help`       | Show compact command help and stop                 |
| `--sync-only`  | Skip commit/push, only update consumers            |
| `--dry-run`    | Show what would happen without making changes      |
| (no flag)      | Full publish: commit + push + sync consumers       |

## Usage

```bash
/maestro:publish "feat: add planning command, rename epic-planning"
/maestro:publish --sync-only
/maestro:publish --dry-run
/maestro:publish --help
```

## Workflow Chain

**Before**: Edit `.maestro/` files (skills, commands, templates, guides, agents)

**After**: Commit submodule pointer update in each consumer:
```bash
cd <consumer> && git add .maestro && git commit -m "chore: update maestro submodule"
```

**Related**: `maestro:sync`, `maestro:health`

---

END OF COMMAND
