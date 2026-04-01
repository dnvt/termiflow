# /maestro:health - System Health Audit

EXECUTE IMMEDIATELY: validate AI workflow integrity and, when configured,
operating cadence or gate readiness. Report health status with actionable
findings.

## Context Loading

- IF `--help` is present: REPORT purpose, flags, inputs, outputs, examples, and
  related commands, then STOP
- READ `.claude/commands/` for the generated command surface
- READ `.claude/commands/README.md` only if present
- READ `.claude/rules/13-workflow-orchestration.md` for command and skill routing
- READ `context/current-task.md` for freshness check
- CHECK `context/session-checkpoint.md` for unconsumed checkpoints

## Exit Criteria (ALL must be true)

- All command files have Workflow Chain sections with Before/After/Related
- All command files expose compact `--help` coverage
- All command files end with END OF COMMAND marker
- All skills referenced in commands have matching SKILL.md directories
- Optional inventory docs match files on disk when they exist
- Justfile recipes referenced in commands actually exist
- `current-task.md` updated within 7 days
- No unconsumed session checkpoint (or flagged as warning)
- If operating cadence is configured or `--gate`: cadence requirements
  are evaluated where observable, or missing evaluators are reported explicitly

## Verification

```bash
# Command integrity
for f in .claude/commands/*.md; do
    [ "$(basename "$f")" != "README.md" ] && ! grep -q "## Workflow Chain" "$f" && echo "MISSING: $f"
done

# Skill cross-reference
grep -roh '`[a-z-]*`' .claude/commands/*.md | sort -u

# Optional inventory docs
if [ -f .claude/commands/README.md ]; then
    diff <(ls -1 .claude/commands/*.md | grep -v README | sed 's|.*/||;s|\.md||' | sort) \
         <(grep -oE '/maestro:[a-z-]+' .claude/commands/README.md | sed 's|^/||' | sort -u)
fi
```

## Boundaries

- Default mode is read-only — report findings, do not auto-fix commands or skills
- `--fix` mode applies only safe, non-destructive repairs
- Never delete user or repo state
- Never silently overwrite ambiguous files
- Report actionable findings with file paths

## Execution

### Default Mode

Run Steps 1-6 sequentially, then produce the report.

### --fix Mode (Safe Repair)

Run the full audit (Steps 1-6), then apply safe, non-destructive repairs:

**Permitted repairs:**
- Add missing END OF COMMAND markers to command files
- Add missing Workflow Chain stub sections to command files
- Restore missing skill SKILL.md stubs when the skill name is unambiguous
- Scaffold missing state indexes or trackers when their shape is already defined
- Repair README or inventory drift when source-of-truth is unambiguous

**Not permitted automatically:**
- Deleting state files or context files
- Moving user content to new locations
- Changing policy boundaries or decision records
- Inventing new source-of-truth locations without a plan

Report each fix applied with the before/after change.

### --history Mode (Session Retrospective)

Review available session traces (`context/session-checkpoint.md` history,
`.claude/state/` if present) and extract an improvement backlog:
- Repeated misunderstandings or loops
- Dropped context or stale assumptions
- Moments where a command, skill, or template should have existed but didn't

Produce improvement backlog grouped by: command changes, new skills, new
guides, rule changes.

### --deep Mode (Multi-Agent)

Invoke 3 parallel Explore agents per the coordination protocol
in `.claude/rules/14-agent-coordination.md`:

**Agent 1 — Command & Skill Validator**: Validate all command files for Workflow
Chain sections, END OF COMMAND markers, Usage sections, and `--help` coverage.
Cross-reference skill names in commands against actual SKILL.md directories.
Report discrepancies as file:line.

**Agent 2 — Context & Task Freshness**: Check `current-task.md` age,
`session-checkpoint.md` existence, `planning/PLAN.md` freshness, and
whether active plans in `planning/` are reflected coherently in the
roadmap and context files.

**Agent 3 — Justfile & Inventory Docs**: Extract all `just {recipe}`
references from commands and CLAUDE.md. Compare against available recipes. If
inventory docs exist, check that they match files on disk.

**HARD GATE**: Collect ALL 3 agent results before synthesis.

### Step 1: Command Integrity

```bash
ls -1 .claude/commands/*.md | grep -v README | grep -v archive
for f in .claude/commands/*.md; do
    [ "$(basename "$f")" != "README.md" ] && ! grep -q "## Workflow Chain" "$f" && echo "MISSING workflow chain: $f"
done
```

Verify each command has: Purpose line, Workflow Chain, Usage section, `--help`
surface, END OF COMMAND marker, valid skill references.

### Step 2: Skill Completeness

```bash
ls -1 .claude/skills/*/SKILL.md 2>/dev/null
for f in .claude/skills/*/SKILL.md; do
    ! grep -q "description:" "$f" && echo "MISSING description: $f"
done
```

### Step 3: Task Freshness

```bash
stat -c "%y" context/current-task.md 2>/dev/null | cut -d' ' -f1
[ -f context/session-checkpoint.md ] && echo "WARNING: Unconsumed checkpoint"
```

Flag if `current-task.md` not updated in >7 days. Also check whether plans
marked `Status: Active` in `planning/` are represented coherently in
`planning/PLAN.md`.

### Step 4: Justfile Recipe Validation

Cross-check `just {recipe}` references in commands against available recipes.

### Step 5: Inventory Docs (Optional)

Treat `.claude/commands/*.md` on disk as the source of truth.

If `.claude/commands/README.md` exists, compare its command inventory against
files on disk. If it does not exist, report `N/A` instead of failing the audit.

### Step 6: Operating Cadence (Optional)

If operating cadence is configured, evaluate only what the repo defines
explicitly:

- Read cadence tracks from `maestro.toml`
- Look for an observable artifact, checklist, or command for each track in
  repo-local docs, progress files, or active pack guides
- If a track has no observable evaluation path, report it as a design gap
  instead of inventing a repo-specific check
- Flag stale cadence evidence with severity: fresh (< 2 weeks), stale
  (2-4 weeks), overdue (> 4 weeks) when dates are available

### --gate Mode (Gate Readiness)

Evaluate gate requirements against actual state:

- Read gate requirements from `maestro.toml` if present, otherwise ask the
  user to define them
- Check each requirement against actual artifacts and metrics
- Report gate readiness percentage and blocking items

If gates are not met: recommend narrowing the wedge and polishing packaging.

## Report

```markdown
## System Health Report

**Date**: {ISO date}

### Command Integrity

- Total commands: {count}
- With workflow chains: {count}/{total}
- With `--help`: {count}/{total}
- With END OF COMMAND: {count}/{total}
- Issues: {list or "None"}

### Skill Completeness

- Total skills: {count}
- With descriptions: {count}/{total}
- Issues: {list or "None"}

### Task Freshness

- current-task.md: {age} days old — Fresh | Stale
- Session checkpoint: Present | Absent

### Justfile Integration

- Referenced recipes: {count}
- Missing recipes: {list or "None"}

### Inventory Docs

- Command source of truth: `.claude/commands/*.md`
- Inventory docs checked: `.claude/commands/README.md` | `N/A`
- Inventory mismatches: {list or "None"}

### Operating Cadence (if configured)

- {Track Name}: ✅/⚠️/❌ ({evidence summary or "no evaluator defined"})

### Gate Readiness (if --gate)

- Gate {N}: {percentage}% ready
- Blocking: {items or "None"}

### Overall Health: Healthy | {N} issues found

### Top 3 Actions: {recommended next steps}
```

If issues were found, output after the report:

```
Issues found. Run:
/maestro:health --fix
```

## Capabilities Invoked

- command and recipe validation
- optional cadence and gate checks when configured

## Usage

```bash
/maestro:health            # Full system audit (+ cadence if configured)
/maestro:health --fix      # Audit then apply safe, non-destructive repairs
/maestro:health --history  # Session retrospective and improvement backlog
/maestro:health --deep     # Multi-agent parallel audit
/maestro:health --gate     # Gate readiness evaluation
/maestro:health --help     # Show compact help and available modes
```

## Workflow Chain

**Before**: None (can run anytime — recommended at session start or sprint
planning) **After**:

- IF issues found → re-run with `--fix` OR fix manually → `maestro:commit`
- IF cadence gaps → update the repo-local cadence definition or maintainer
  guide before treating the track as enforceable
- IF healthy → continue working

**Related**: `maestro:start`

---

END OF COMMAND
