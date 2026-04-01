# /maestro:sync — Synchronize State Across Tools

Ensures coherence between local files, tracking tools, and
external systems. Single source of truth maintenance.

**Usage:**
`/maestro:sync [--docs] [--roadmap] [--tools] [--inbox] [--deep]`

## Context Loading

- IF `--help` is present: REPORT purpose, flags (`--docs`,
  `--roadmap`, `--tools`, `--inbox`, `--deep`), inputs, outputs,
  examples, and related commands, then STOP

1. Read `planning/PLAN.md` for current priorities
2. Scan `decisions/` for recent decisions
3. Scan `planning/` for active plans
4. Scan `planning/` for status updates
5. Read `context/session-checkpoint.md`
6. If `--inbox` is present:
   - scan `inbox/` newest first and read `inbox/README.md`
   - read `context/current-task.md` if it exists
   - read `context/blockers.md` if it exists
   - read `analysis/ingest/INDEX.md` if it exists

## Exit Criteria

- [ ] All selected domains verified for coherence
- [ ] Stale items flagged or archived
- [ ] Terminology consistent across documents
- [ ] No contradictions between plans, decisions, and roadmap
- [ ] Inbox items processed if `--inbox`
- [ ] Sync report produced

## Domain Flags

### --docs (Documentation Coherence)

- Verify plans reference current decisions (not superseded ones)
- Verify every plan has either `Decision Links` or explicit
  rationale
- Verify durable plans declare `Status` and `Roadmap Slot`
- Check that thinking captures older than 30 days are either
  converted to `planning/` or `decisions/`, or archived to
  `analysis/archive/`
- Ensure decision records are sequentially numbered without gaps
- Flag any document referencing work that's been completed or
  abandoned

### --roadmap (Roadmap Accuracy)

- Verify roadmap items match active plans
- Verify plan `Roadmap Slot` values match actual roadmap section
- Verify plan `Status` values match how the roadmap describes them
- Check timeline estimates against actual progress
- Flag items with no owner or stale status
- Ensure priorities reflect most recent decisions

### --tools (External Tool Sync)

If Notion is enabled:

- Compare local decisions/plans against Notion pages
- Flag items that exist in one but not the other
- Suggest which direction to sync (local → Notion or Notion →
  local)

If other tools are configured, check consistency similarly.

### --inbox (Async Capture Cleanup)

- Scan active inbox files in `inbox/`, newest first
- Determine each file's canonical date from frontmatter,
  filename, or file metadata
- Parse tasks, ideas, notes, 1:1s, and carry-forward items
- Classify each relevant item into one of:
  - promote to durable system state
  - carry forward to the next daily inbox file
  - archive as raw context only
  - flag for confirmation
- Promote only low-risk summaries into:
  - `planning/`
  - `decisions/`
  - `context/current-task.md`
  - `context/blockers.md`
  - `analysis/ingest/INDEX.md`
  - `planning/PLAN.md`
- Load the current contents of every destination file before
  updating it
- Update `## Processed During Sync` in each inbox file
- Mark each file as `synced` or `needs-review`
- Archive fully processed files to `inbox/archive/`

### --deep (All Domains + Cross-Validation)

Run all three domains plus inbox when active inbox files exist,
then:

- Cross-validate decisions against plans (every plan should trace
  to a decision or explicit rationale)
- Check for orphaned work (plans with no parent initiative)
- Verify naming conventions are consistent
- Check that promoted inbox items were summarized rather than
  copied verbatim

### Auto-Detect (no flags)

Analyze recent changes to determine which domains need sync:

- New decisions → check plan alignment
- Updated plans → check roadmap alignment
- Stale checkpoint → flag for refresh
- Active unsynced inbox files → include `--inbox`

## Sync Report

```
Sync Report — {date}
===================

Documents:
  {N} current, {M} stale (>30d), {K} archived

Roadmap:
  {N} on track, {M} at risk, {K} stale

Inbox:
  {files processed, promoted, carried forward, archived, flagged}

Coherence:
  {list of contradictions or "all consistent"}

Actions Taken:
  {list of auto-fixes applied}

Needs Attention:
  {items requiring human decision}
```

## Workflow Chain

- If sync finds contradictions → `maestro:decide`
- If sync archives work → `maestro:commit`
- If sync finds new project work → `maestro:plan`
- Regular cadence: run weekly or after major decisions

## Boundaries

- Don't auto-resolve contradictions — flag them for the user
- Don't delete files — archive to the owning area's `archive/`
  directory with a date suffix
- Don't sync to external tools without user confirmation
- Sync is read-heavy, write-light — prefer flagging over fixing
- Don't silently convert ambiguous inbox notes into decisions or
  plans
- Don't write into `context/current-task.md`,
  `context/blockers.md`, or
  `analysis/ingest/INDEX.md` without loading its current contents first
- Prefer concise summaries over verbatim transfer from raw inbox
  notes

---

END OF COMMAND
