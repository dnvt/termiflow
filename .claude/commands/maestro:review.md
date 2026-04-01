# /maestro:review - Code-Based Work Validation

EXECUTE IMMEDIATELY: validate {type}:{id} completion against actual code and
report evidence.

## Core Principle

**CODE OVER DOCS** — never trust documentation alone. Run real commands, read
real source files, verify real implementations exist.

## Context Loading

- IF `--help` is present: REPORT purpose, flags, inputs, outputs, examples, and
  related commands, then STOP
- LOAD specification from `planning/**/*{id}*`
- IDENTIFY acceptance criteria and deliverables
- CHECK for linked design packets and Experiment Log entries

## Exit Criteria (ALL must be true)

- All acceptance criteria from spec are satisfied with code evidence
- `git status --short` succeeds
- `cargo fmt --check && cargo clippy && cargo test` passes (capture exact test count)
- `cargo clippy` is clean
- Zero `.unwrap()` / `.expect()` / `panic!()` in modified production code
- If perf-relevant: performance contracts verified (per maestro.toml)
- If core implementation paths under `src/` changed: relevant integration tests pass
- If design packet exists: Experiment Log records actual outcomes
- Confidence = 100% (no partial completions)

## Boundaries

- This is a VALIDATION command — stop current work and switch to review mode
- Run actual commands and capture output (no assumed success)
- Do not mark complete without passing all checks
- If <100% confidence: generate remediation plan and STOP
- If 100% confidence: report with full evidence

## Verification

```bash
cargo build && cargo clippy        # compile + clippy + fmt
cargo fmt --check && cargo clippy && cargo test          # all workspace tests with count
```

If perf-relevant crates touched: run performance-specific tests. If core crates
under `src/` touched: run integration tests.

## Decoupled Review (Recommended)

Spawn a **separate review agent** that did NOT write the code. This avoids
self-evaluation bias:

```
Agent(subagent_type="",
      prompt="Review the diff for {id} against the spec at {spec_path}.
              You did NOT write this code — evaluate it objectively.
              Check: correctness, zero-unwrap compliance, performance
              contracts, test coverage, critical patterns.
              Report confidence 0-100 with file:line evidence.")
```

The reviewer receives: diff, spec, test results — NOT the implementation
reasoning.

## Domain Flags

| Flag         | Check                                       | Capability Loaded       |
| ------------ | ------------------------------------------- | ----------------------- |
| `--sec`      | Zero-unwrap, auth patterns, GDPR, secrets   | Security audit          |
| `--perf`     | Performance contracts, blocking calls       | Performance validation  |
| `--arch`     | Boundaries, dependency direction, API       | Architecture review     |
| `--quality`  | Critical patterns, best practices, naming   | Quality review          |
| `--coverage` | Test coverage >95%, missing test cases      | Test coverage audit     |

Multiple flags can be combined: `/maestro:review --sec --perf feat:1e15`

When no domain flag is specified, the reviewer uses general code review criteria
(security, performance, quality, architecture, tests) without spawning
specialist agents.

## Deep Mode (--deep)

Spawn 3+ parallel Explore agents for thorough verification:

1. **Acceptance Criteria Verifier** — match each criterion to implementation
   with file:line references
2. **Quality Spot-Check** — zero-unwrap scan, critical patterns, performance
   contract values in modified files
3. **Test Completeness Auditor** — test counts, public function coverage,
   ignored tests, naming conventions
4. **Domain Specialists** — if domain flags are set, spawn additional specialist
   agents (``, `maestro-performance-guardian`, etc.)

HARD GATE: collect ALL agent results before synthesis.

## Report Format

```markdown
## Review Report: {id}

**Specification**: {path}

### Results

- Build: PASS/FAIL
- Tests: {N} passed
- Clippy: Clean / {N} warnings
- Zero Unwrap: 0 violations / {N} violations
- Performance: Contracts met / {issues}
- Smoke Test: OK / FAIL / N/A

### Acceptance Criteria

{Checklist with PASS/FAIL per criterion}

### Confidence: {percentage}%

**Status**: COMPLETE | NEEDS ITERATION
```

---

## Usage

```bash
/maestro:review task:1e27                  # Review task (general)
/maestro:review feat:1e15                  # Review feature
/maestro:review epic:1e                    # Review epic
/maestro:review --deep task:1e27           # Multi-agent thorough review
/maestro:review --sec --perf feat:1e15     # Security + performance focus
/maestro:review --arch --quality epic:1e   # Architecture + best practices
/maestro:review --coverage feat:1e15       # Test coverage audit
/maestro:review --deep --sec --perf --arch # Full deep review with all domains
/maestro:review --help                     # Show compact help and stop
```

---

## Session Wrap (Auto-Triggered)

After delivering the review report, ALWAYS execute the session wrap protocol:

1. Gather session state (branch, commits, working tree)
2. Write checkpoint to `context/session-checkpoint.md`
3. Run codify check (pattern/rule/context updates)
4. Report: "Session wrapped. Resume with `/maestro:start`."

This is not optional — every `/maestro:review` ends with a session checkpoint.

---

**REMEMBER**: This is a VALIDATION command. When invoked, STOP current work and
VALIDATE against actual code with real commands. No assumed success. 100%
confidence required.

---

## Workflow Chain

**Before**: `/maestro:run` (implementation complete) **After**:

- IF 100% confidence → `/maestro:commit` → `/maestro:push`
- IF <100% confidence → fix gaps, re-run `/maestro:review`
- IF unresolved trade-off is the blocker → `/maestro:decide`
- IF missing evidence blocks confidence → `/maestro:research`
- IF performance concerns → `/maestro:run --perf`
- IF security concerns → `/maestro:run --sec`
- IF a runtime or integration path is broken → use the active debugging or domain-specific diagnostic capability

**Related**: `/maestro:run`, `/maestro:decide`, `/maestro:research`

---

END OF COMMAND
