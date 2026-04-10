# /maestro:run - Scoped Work Execution

EXECUTE IMMEDIATELY: implement the requested scope in code, validate it against
the real repo state, and report evidence.

## Context Loading

- IF `--help` is present: REPORT purpose, flags, inputs, outputs, examples, and
  related commands, then STOP
- LOAD specification from `planning/**/*{id}*`
- READ `context/current-task.md` and `context/blockers.md`
- READ related decisions from `decisions/` when they constrain the scope
- ISSUE CHECK — search GitHub for an existing issue:
  ```bash
  gh issue list --repo dnvt/termiflow --search "in:title {Type} {ID}" --json number,title --limit 3
  ```
  If found → note issue number. If not → create from spec. Print:
  `Linked to GitHub issue #{N}`
- SELECT the smallest useful capability set for the scope: implementation,
  error handling, testing, review, and architecture only when the work actually
  needs them

## Exit Criteria (ALL must be true before reporting done)

- All acceptance criteria from the specification are satisfied
- `cargo test` passes
- `cargo clippy` is clean
- `cargo fmt --check` is clean
- Zero `.unwrap()` / `.expect()` / `panic!()` in production code paths
- Coverage >95% for modified crates
- Performance contracts maintained (see maestro.toml)
- If spec has a design packet: Experiment Log records actual outcomes

## Boundaries

- Only modify files within the scope of the work ID
- Do not change public API signatures without documenting in the report
- Create a design packet (`docs/design/`) only if work is uncertain, risky, or
  spans 3+ files across 2+ crates
- Use existing `just` or CLI commands — do not create ad hoc scripts
- Use the real repo state, not assumed command availability
- Do not leave stale command names, count drift, or split-brain docs

## Auto-Chain: Verify (runs automatically after implementation)

After all implementation work is done, run the verification loop inline. Do NOT
stop and report — iterate until all checks pass:

```bash
git status --short
cargo clippy
cargo test
cargo fmt --check
```

**Autonomous fix loop**: If verification fails, analyze the error, fix the root
cause, and re-verify. Repeat until all checks pass OR 3 iterations fail on the
same issue. After 3 failures: report the blocker with evidence.

**Doom loop guard**: If the same file has been edited 5+ times without progress,
reconsider the approach entirely.

## Auto-Chain: Review (runs automatically after verify passes)

Spawn a **separate review agent** (``) to review the diff. The
reviewer did NOT write the code — this avoids self-evaluation bias:

- Security: no `.unwrap()`, no hardcoded secrets, input validation
- Performance: no unnecessary allocations, no blocking in async
- Quality: single responsibility, reasonable function size, clear naming
- Architecture: crate boundaries respected, no circular deps
- Tests: happy path, error cases, edge cases covered

Render verdict: APPROVED / APPROVED WITH SUGGESTIONS / CHANGES REQUESTED. If
changes requested, fix and re-verify before reporting done.

## Auto-Chain: Domain Checks (flag-triggered)

| Flag     | Check                    | Capability Loaded       |
| -------- | ------------------------ | ----------------------- |
| `--sec`  | Security audit           | Security audit          |
| `--perf` | Performance contracts    | Performance validation  |
| `--test` | Test infra + coverage    | Test coverage           |
| `--deep` | All above + architecture | Full decomposition      |

When `--deep`: decompose into observe → hypothesize → implement → measure. Use
the Agent tool with specialist subagent_type for parallel work.

For workflow audits or source-surface changes, prefer
`/maestro:health` after edits.

## Report

When done, report: modified files, commands run with output, remaining risks,
and evidence that exit criteria are met.

## Session Wrap (Auto-Triggered)

After reporting, ALWAYS execute the session wrap protocol:

1. Gather session state (branch, commits, working tree)
2. Write checkpoint to `context/session-checkpoint.md`
3. Run codify check (pattern/rule/context updates)
4. Report: "Session wrapped. Resume with `/maestro:start`."

This is not optional — every `/maestro:run` ends with a session checkpoint.

## Usage

```bash
/maestro:run task:1e15
/maestro:run feat:1e15 --deep
/maestro:run feat:1e15 --sec --perf
run /maestro:run --deep audit maestro commands and skills
/maestro:run --help
```

## Workflow Chain

**Before**:

- `/maestro:think` if the scope or framing is still fuzzy
- `/maestro:decide` if a trade-off is blocking implementation
- `/maestro:research` if evidence is missing
- `/maestro:start` if session context is unclear

**After** (auto-chained — no manual invocation needed):

- Verify → Review → Session Wrap (automatic)
- `/maestro:review {scope}` (optional deep validation by separate agent)
- `/maestro:commit` → `/maestro:push`

## Reference

- `.claude/guides/maestro-run-guide.md`

---

END OF COMMAND
