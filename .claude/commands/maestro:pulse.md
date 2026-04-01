# /maestro:pulse - Research Intelligence Pulse

**COMMAND EXECUTION MODE**

When this command is invoked, you MUST:

1. **IF** `--help` is present: report purpose, flags, inputs, outputs, examples,
   and related commands, then STOP
2. **STOP** all work in progress immediately
3. **SELECT** the optimal pulse for today
4. **EXECUTE** deep web research for that pulse
5. **SAVE** structured findings
6. **DO NOT** resume previous work until pulse completes

---

## Actions (Execute Now)

1. **READ** `.claude/state/pulse-tracker.json` for run history
2. **IF** `--status` flag: REPORT pulse schedule table and STOP
3. **IF** argument is a number (1-12): SELECT that pulse directly
4. **ELSE**: COMPUTE best pulse using selection algorithm below
5. **LOAD** pulse definition from `context/pulse-config.md`
6. **EXECUTE** deep web research (minimum 3 searches, refine queries)
7. **EVALUATE** each finding against quality signals (see pulse config)
8. **WRITE** output to `analysis/research-pulse/{YYYY-MM-DD}-pulse-{N}.md`
9. **UPDATE** `.claude/state/pulse-tracker.json` with run timestamp + counts
10. **REPORT** summary: findings count, top recommendation, next pulse due

## Pulse Selection Algorithm

```
today = current date (YYYY-MM-DD)
dow = day of week (Mon=0 ... Sun=6)
dom = day of month (1-31)

candidates = []

# Daily pulses (1, 2) — due every day
IF pulse 1 not run today: candidates += pulse 1
IF pulse 2 not run today: candidates += pulse 2

# Weekly pulses — due on their weekday
IF dow == Mon AND pulse 4 not run this week: candidates += pulse 4
IF dow == Tue AND pulse 3 not run this week: candidates += pulse 3
IF dow == Wed AND pulse 5 not run this week: candidates += pulse 5
IF dow == Thu AND pulse 6 not run this week: candidates += pulse 6
IF dow == Fri AND pulse 7 not run this week: candidates += pulse 7
IF dow == Fri AND pulse 11 not run this week: candidates += pulse 11

# Biweekly pulses — due on specific days of month
IF dom IN [1, 15] AND pulse 8 not run this period: candidates += pulse 8
IF dom IN [8, 22] AND pulse 10 not run this period: candidates += pulse 10

# Monthly pulses — due on specific day of month
IF dom == 21 AND pulse 9 not run this month: candidates += pulse 9
IF dom == 1 AND pulse 12 not run this month: candidates += pulse 12

# Add any OVERDUE pulses (past schedule, never run or stale)
FOR each pulse NOT in candidates:
  IF pulse is overdue (past its schedule window): candidates += pulse (PRIORITY)

# Sort: overdue first, then by staleness (oldest last_run first)
# Pick the top candidate
```

## Usage

```bash
/maestro:pulse                  # Auto-select best pulse for today
/maestro:pulse 1                # Force Pulse 1 (Research Papers)
/maestro:pulse 6                # Force Pulse 6 (ML Training)
/maestro:pulse --status         # Show schedule status for all 12 pulses
/maestro:pulse --deep           # Extended search: 5+ queries, cross-reference findings
/maestro:pulse --help           # Show compact help and stop
```

`--deep`: Runs 5+ web searches per pulse (vs default 3), cross-references
findings against active plans and roadmap, and routes high-signal results through
`/maestro:ingest --pulse`.

## Output Template

Write to `analysis/research-pulse/{YYYY-MM-DD}-pulse-{N}.md`:

```markdown
# Pulse {N}: {Pulse Name}

**Date**: {YYYY-MM-DD} **Bucket**: {taxonomy bucket} **Findings**: {count}
**High-Signal**: {count}

---

## Finding 1: {Title}

**URL**: {url} **Impact**: HIGH | MEDIUM | LOW **Feeds**: {project
plan/initiative/epic} **Summary**: {2-3 sentences on what this is and why it
matters} **Recommendation**: INGEST | MONITOR | SKIP **Rationale**: {Why this
recommendation}

---

## Finding 2: {Title}

...

---

## Pulse Summary

- **Total findings**: {N}
- **INGEST recommendations**: {N} (route through `/maestro:ingest --pulse`)
- **MONITOR recommendations**: {N}
- **Next pulse due**: Pulse {N} ({name}) — {schedule}
```

## Critical Constraints

- MUST: Run minimum 3 web searches per pulse execution
- MUST: Include verifiable URLs for every finding (no fabricated references)
- MUST: Map every finding to a specific plan, initiative, or epic
- MUST: Update tracker JSON after successful output write
- MUST NOT: Run more than one pulse per invocation (deep > wide)
- MUST NOT: Fabricate paper titles, authors, or benchmark numbers
- MUST NOT: Skip the quality signal evaluation step

## Status Report Format (for `--status`)

```
PULSE SCHEDULE STATUS — {today's date}

# | Name                          | Freq     | Schedule | Last Run   | Status
--|-------------------------------|----------|----------|------------|--------
1 | Research Papers & Preprints   | Daily    | Every day| 2026-02-15 | DUE
2 | Speech Repos & Open-Source    | Daily    | Every day| 2026-02-16 | OK
3 | Competitor Product Intel      | Weekly   | Tuesday  | 2026-02-11 | OK
...

RECOMMENDED NEXT: Pulse 1 (Research Papers & Preprints) — last run 1 day ago
```

## Reference

- Pulse definitions: `context/pulse-config.md`
- Active research pulse capability or repo-local equivalent
- Guide: `.claude/guides/pulse-guide.md`
- Run history: `.claude/state/pulse-tracker.json`
- Output directory: `analysis/research-pulse/`

---

## Workflow Chain

**Before**: Research pulse due or user requests a specific pulse **After**:

- IF high-signal findings exist → `/maestro:ingest --pulse`
- IF a single thread deserves a deeper manual investigation → `/maestro:research`
- IF roadmap impact identified → update docs, `/maestro:commit`
- IF no actionable findings → keep monitoring cadence

**Related**: `/maestro:research`, `/maestro:ingest`, `/maestro:commit`

---

END OF COMMAND
