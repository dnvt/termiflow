# AGENTS.md

Agent instructions for AI coding assistants (Claude Code, Codex CLI, etc.) working on TermiFlow.

## Quick Reference

### Project Commands

| Command | Purpose | Usage |
|---------|---------|-------|
| `/start` | Initialize session | Run first to check health and context |
| `/audit` | Review fixtures for ASCII issues | `/audit` or `/audit subgraph` |
| `/render` | Quick diagram test | `/render graph TD; A-->B` |
| `/validate` | Full test suite | Run before commits |
| `/fixture` | Manage golden tests | `/fixture list`, `/fixture show flow_simple` |
| `/debug` | Diagnose issues | `/debug diagram.md` |

### Maestro Workflow Commands

| Command | Purpose |
|---------|---------|
| `/maestro:start` | Load session context and resume from checkpoint |
| `/maestro:plan` | Decompose a feature or initiative into a structured plan |
| `/maestro:run` | Execute a planned deliverable with verification |
| `/maestro:review` | Critical review of plans, decisions, or implementations |
| `/maestro:research` | Deep research on a topic (renderer algorithms, Mermaid spec, etc.) |
| `/maestro:decide` | Structured decision with options, criteria, and rationale |
| `/maestro:commit` | Checkpoint session state and capture decisions |
| `/maestro:think` | Brainstorm, converge, or challenge an approach |
| `/maestro:simmer` | Iterative refinement of a document or design |
| `/maestro:sync` | Coherence check across plans and decisions |

Maestro commands are defined in `.claude/commands/maestro:*.md` (generated — do not edit directly; edit `.maestro-project/` overrides instead).

## Project Context

**TermiFlow** renders Mermaid flowcharts as ASCII/Unicode terminal art.

```
Mermaid text → Parser → Graph → Layout → Canvas → Output
```

**Key files:**
- `src/parser.rs` - Two-pass Mermaid parser
- `src/layout.rs` - Waterfall positioning algorithm
- `src/render/edge.rs` - Direction-agnostic edge routing
- `src/orientation.rs` - TD/LR/BT/RL coordinate abstraction
- `src/style.rs` - 9 styles + composite mixing

**Test fixtures:** `tests/fixtures/` with 101 inputs × 2 styles = 202 golden outputs

## Workflow for Agents

### Starting a Session
```bash
# Always start with health check
cargo test
cargo clippy
```

### Making Changes

1. **Understand scope**: Read relevant source files before editing
2. **Run tests frequently**: `cargo test [module_name]`
3. **Check rendering**: `echo 'graph TD; A-->B' | cargo run --bin tw --`
4. **Validate before commit**: `cargo fmt && cargo clippy && cargo test`

### Fixture Updates

After intentional rendering changes:
```bash
cargo test --features golden -- --ignored  # Regenerate
git diff tests/fixtures/expected/          # Review changes
```

## Common Tasks

### Add a Feature
1. Implement in relevant module
2. Add unit tests inline (`#[cfg(test)]`)
3. Add golden fixture if affects output
4. Update `CLAUDE.md` if architectural

### Fix a Bug
1. Write failing test first
2. Fix the code
3. Verify test passes
4. Check no regression in golden fixtures

### Debug Rendering
```bash
TERMIFLOW_DEBUG_ROUTES=1 cargo run --bin tw -- diagram.md 2>&1
```

## Architecture Quick Reference

### Direction Abstraction
`OrientedCoords` in `orientation.rs` maps logical operations to physical coordinates:
- `advance(x, y, dist)` - Move in flow direction
- `retreat(x, y, dist)` - Move against flow
- Primary axis = flow direction, Secondary axis = branching

### Edge Routing
Two main functions in `render/edge.rs`:
- `route_divergent_edges()` - One source → multiple targets
- `route_convergent_edges()` - Multiple sources → one target

### Composite Styling
`CompositeStyle` allows mixing: `corner:dots,border:heavy`

Components: `corner`, `border`, `arrow`, `edge`, `junction`, `back`, `subgraph`

### Config Precedence
CLI flags > `%% termiflow:` directives > `~/.config/termiflow/config.toml`

## Testing Checklist

- [ ] `cargo test` - All 147 tests pass
- [ ] `cargo clippy` - No new warnings
- [ ] `cargo fmt --check` - Code formatted
- [ ] Golden fixtures match expected output
- [ ] All 4 directions work (TD, LR, BT, RL)
- [ ] Both styles work (ascii, unicode)

## Files to Read First

1. `CLAUDE.md` - Complete architecture and commands
2. `README.md` - User-facing features
3. `docs/reference.md` - CLI flags and syntax
4. `planning/PLAN.md` - Current roadmap
5. `tests/fixtures/README.md` - Golden test documentation

## Command Details

Commands are defined in `.claude/commands/` and can be invoked as `/command` in Claude Code or run as scripts in Codex.

### /start
Initializes a session with:
- Build and test verification
- Git status check
- Planning context from `PLAN.md`
- Ready report with suggested focus

### /audit [family]
Reviews fixtures for ASCII/Unicode rendering issues:
- Box corner alignment
- Junction character correctness
- Arrow direction verification
- Label positioning
- Edge continuity

### /render <diagram>
Quick rendering test. Accepts file path or inline Mermaid.

### /validate
Full pre-commit validation:
- Format check
- Clippy lint
- Test suite
- Golden tests
- Release build

### /fixture <subcommand>
Golden test management:
- `list` - Show all fixtures
- `show <name>` - Render a fixture
- `diff <name>` - Compare actual vs expected
- `regen` - Regenerate all (careful!)
- `add <name>` - Create new fixture

### /debug <diagram>
Debug rendering with:
- Timing stats
- Route dumps
- Issue isolation guidance

## Maestro Setup

Maestro provides a portable workflow layer (planning, review, research, decisions)
via the `.maestro/` git submodule. Generated files live in `.claude/` and are
tracked in git. Do not edit `.claude/commands/maestro:*.md` directly — regenerate
via `bash .maestro/generate.sh` after updating `.maestro-project/`.

### Portability Model

```
.maestro/           ← shared core (git submodule, read-only here)
.maestro-project/   ← termiflow-local overrides (rules, hooks, templates)
maestro.toml        ← activates packs, sets verification commands and paths
.claude/            ← generated runtime surface (tracked in git)
```

### Skills and Session Context

- Session state: `context/session-checkpoint.md` (created on first Maestro session)
- Planning: `planning/PLAN.md` (existing — Maestro roadmap points here)
- Research/decisions: `analysis/` (created as needed)

### After Submodule Update

```bash
bash .maestro/generate.sh   # regenerate .claude/ from updated submodule
git add .claude/ maestro.toml .maestro
git commit -m "chore: update maestro submodule and regenerate"
```
