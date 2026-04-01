# /maestro:simmer — Iterative Refinement

Refine an existing artifact over multiple rounds. This is the
first-class Maestro entrypoint for polishing a document, prompt,
plan, spec, or bounded workspace without losing the score
trajectory.

**Usage:** `/maestro:simmer {artifact-or-path}` or
`/maestro:simmer {artifact-or-path} --iterations {N}`

## Context Loading

- IF `--help` is present: REPORT purpose, flags
  (`--iterations {N}`), inputs, outputs, examples, and related
  commands, then STOP

1. Read `context/current-task.md` for the active focus
2. Read `context/session-checkpoint.md` for continuity
3. Resolve the artifact to refine:
   - explicit path from the command input, OR
   - pasted artifact in the prompt, OR
   - the most recent plan / review / deliverable referenced in
     current context
4. Read the artifact itself before refining
5. Load `.claude/skills/writing-tone/SKILL.md` for prose or
   communication artifacts
6. Load `.claude/skills/design-thinking/SKILL.md` if the artifact
   is UX, product, or workflow related
7. Load `.claude/skills/product-strategy/SKILL.md` if the
   artifact is positioning, roadmap, or strategy related
8. Load `.claude/skills/simmer/SKILL.md` (always)

## Exit Criteria

- [ ] Artifact and refinement goal are unambiguous
- [ ] Refinement bundle created under `analysis/simmer/`
- [ ] Setup brief captured with explicit criteria
- [ ] `trajectory.md` records each iteration
- [ ] `result.md` contains the best candidate
- [ ] `summary.md` explains what changed and why the best
      iteration won
- [ ] Source artifact is only overwritten if the user explicitly
      asked for in-place replacement

## Execution Flow

### 1. Resolve Target And Bundle

Identify what is being refined and create a durable run bundle:

- Derive a short slug from the artifact name or topic
- Create `analysis/simmer/{YYYY-MM-DD}-{slug}/`
- Record the original source path, if any
- Use that bundle path as the authoritative `OUTPUT_DIR` for the
  full refinement run

### 2. Set Up The Refinement Brief

Prefer explicit user criteria when they exist. Otherwise let the
setup skill infer the rubric and propose it.

Force these defaults unless the user overrode them:

- `OUTPUT_DIR: analysis/simmer/{YYYY-MM-DD}-{slug}/`
- `ITERATIONS: 3`

Write the final setup brief to `setup.md` inside the run bundle.

### 3. Run The Simmer Loop

Use the simmer skill family to execute the loop:

- `simmer-setup` resolves artifact type, criteria, and mode
- `simmer-generator` produces the next candidate
- `simmer-judge` or `simmer-judge-board` scores it
- `simmer-reflect` updates the trajectory and best-so-far state

Operational requirements:

- Update `trajectory.md` after every judged iteration
- Keep the best candidate separate from the latest candidate
- For workspace refinement, save touched-file snapshots under
  `snapshots/iteration-N/` inside the bundle before replacing the
  current best state
- If a later iteration regresses, continue from the best
  snapshot/candidate rather than the regressed one

### 4. Finalize

At the end of the run:

- Write the winning artifact to `result.md`
- Write `summary.md` with the start state, winning iteration,
  final score, and recommended next step
- If the user explicitly asked to apply the refined result back to
  the source artifact, do that as a final step after the bundle is
  complete

## Report

```
Simmer complete:
  Target:   {artifact or path}
  Bundle:   analysis/simmer/{YYYY-MM-DD}-{slug}/
  Best:     iteration {N} ({score}/10)
  Result:   {result path}
  Applied:  yes | no
  Next:     {suggested follow-up}
```

## Workflow Chain

- Before simmer: `maestro:think`, `maestro:review`, or
  `maestro:run`
- After simmer: `maestro:commit` to checkpoint the refined output

## Boundaries

- Do not start refining until the target artifact is clear
- Do not overwrite the original artifact without explicit user
  intent
- Keep the run bundle authoritative even if you also update the
  source artifact
- Prefer the configured output directory from `maestro.toml`
  rather than ad hoc scratch paths
- If refinement exposes a deeper scope problem, stop and route to
  `maestro:plan` or `maestro:decide`

---

END OF COMMAND
