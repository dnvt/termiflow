# /maestro:learn — Map-First Learning Command

Takes a source — PR, branch, file, directory, or concept — and
produces a structural map with ASCII diagrams, layered depth on
demand, and a single living artifact that improves through
conversation.

**Usage:**

```
/maestro:learn <source>                      # PR number, branch, file, dir, or concept
/maestro:learn <source> --depth <level>      # orient | working | deep
/maestro:learn <source> --dive <anchor>      # drill into a named concept
/maestro:learn <source> --share              # clean output for sharing
/maestro:learn --help                        # show this help
```

## Context Loading

- IF `--help` is present: REPORT purpose, flags, inputs, outputs,
  examples, and related commands, then STOP

1. Read `maestro.toml` `[learn]` section for output directory and
   `tw` binary path
2. Read `context/current-task.md` for active focus
3. If source is a PR or branch, read the diff and PR description
4. If an existing artifact exists for this source, read it (for
   `--dive` or re-runs)
5. Load `.claude/skills/writing-tone/SKILL.md` if `--share` is
   present (not needed for orient/working/deep without share)

## Exit Criteria

- [ ] Source identified and read in full before any output
- [ ] 5-10 core abstractions identified from the source
- [ ] Structural map rendered via `tw` (or raw Mermaid on `tw`
      failure)
- [ ] Requested depth level produced (default: orient)
- [ ] Artifact saved to configured output directory
- [ ] Commit hash stamped in artifact header
- [ ] "Ask me anything — use `--dive <anchor>` to go deeper"
      displayed (unless `--share`)

---

## Source Detection

Identify the source type from the argument:

| Pattern | Type | How to read |
|---------|------|-------------|
| `#123` or bare number | PR | `gh pr view <N> --json title,body,files` + `gh pr diff <N>` |
| Branch name (no `/` prefix, exists in git) | Branch | `git diff main..<branch>` + `git log main..<branch> --oneline` |
| Path ending in `/` or existing directory | Directory | Read all files recursively (respect `.gitignore`) |
| Path to existing file | File | Read the file |
| Quoted string or unrecognized | Concept | Web search + scan codebase for related code |

For PRs and branches: check diff size BEFORE reading the full
content. Run `gh pr diff <N> | wc -l` or `git diff main..<branch>
| wc -l` first. If over 3000 lines or 30 files, suggest narrowing
to a specific directory or concern before proceeding.

If size is acceptable, read the entire diff before producing any
output. The command's value is editorial judgment about what
matters — which requires reading everything first.

---

## Map Production

Always runs first, regardless of depth level. The map is done
when ALL of these are true:

- 5-10 core abstractions identified from the source (not from
  file names — from concepts, patterns, data flows, decisions)
- Relationships between abstractions described in Mermaid syntax
- Mermaid rendered via `tw` (heredoc, not echo — avoids
  single-quote escaping issues):
  ```bash
  <tw_binary> <<'MERMAID'
  graph TD
      A[concept]-->B[concept]
  MERMAID
  ```
  Where `<tw_binary>` is from `maestro.toml` `[learn].tw_binary`.
  On `tw` failure: warn, show raw Mermaid, continue.
- Each abstraction named as a dive anchor

For PRs: abstractions come from the changeset, not the whole
codebase. What did this PR add, change, or introduce?

For files/directories: abstractions come from the code structure,
key interfaces, and domain concepts.

For concepts: abstractions come from research. Cap at 3 web
searches and 5 codebase scans — concepts are bounded, not
open-ended.

---

## Depth Levels (Escalation Ladder)

Only generate the requested level. Never front-load all three.

| Level | Flag | What you produce | When to use |
|-------|------|-----------------|-------------|
| L0 | `--depth orient` (default) | Map + one sentence per abstraction + "why does this exist?" + gotchas | First encounter with unfamiliar territory |
| L1 | `--depth working` | Module interfaces, key functions, data flow between abstractions, gotchas with code examples. **Every key function and type must include a `path:line_number` pointer** so the reader can navigate directly to the source. | Need to make changes confidently |
| L2 | `--depth deep` | Implementation tradeoffs, edge cases, historical context, alternatives considered, "why not X?". **Retain all L1 file pointers. Add links to planning docs, RFCs, decision records, or external articles** where they exist and informed the design. | Need to understand architectural decisions |

Each level REPLACES the prior level's content — the artifact
reads as a coherent document at the requested depth, not layers
stacked on top of each other.

When a depth level is requested and an artifact already exists
for this source: read the existing artifact first, then rewrite
each section at the new depth. Preserve any insights, gotchas,
or relationship discoveries from the prior version — they were
earned. The rewrite deepens, it does not discard.

---

## Dive Behavior (--dive)

When `--dive <anchor>` is used:

1. Read the existing artifact
2. Find the section for `<anchor>`
3. Expand it to the next depth level (orient → working → deep).
   Apply the pointer and reference requirements for the new level:
   L1 requires `path:line_number` for every key function and type;
   L2 retains those and adds links to planning docs, RFCs, or
   external articles where they exist.
4. Rewrite that section for clarity and coherence — do not just
   append more text below the existing content
5. If the dive reveals new relationships, update the map diagram
   (re-render through `tw`)
6. Save back to the SAME file

The artifact is a single source of truth. It improves over the
conversation, not grows.

**Preservation rule:** the rewritten section must cover everything
the prior version covered, plus the new depth. "Rewrite for
coherence" does not mean "cut things that seem obvious now." If
the orient version said "this exists because X" and the working
version explains how it works, the "because X" framing must
survive in the rewrite.

If the anchor is already at L2 (deep), produce a **handoff
block** instead of more prose:

1. State that the anchor is at L2.
2. Identify 2-4 sub-concepts that surfaced during the deep dive
   and are worth their own treatment. These should be specific
   (a type lifecycle, an algorithm, a special-case branch) — not
   generic ("learn more about X").
3. For each sub-concept, suggest a scoped `maestro:learn` command
   with a concrete source argument (a file path, a quoted concept
   string, or a PR number if relevant).
4. Do NOT append further prose to the section. The section stays
   at L2. The handoff block is printed to the user, not saved.

**Handoff block format:**

```
**[Anchor] is at L2 (deep) — no further depth here.**

Sub-concepts worth a dedicated run:
  [Sub-concept A] — /maestro:learn src/path/to/file.rs
  [Sub-concept B] — /maestro:learn "concept name"
  [Sub-concept C] — /maestro:learn src/other/file.rs

Each will produce a fresh map with its own anchor set and depth
ladder. Start the new run at orient; dive from there.
```

Do not generate this block for anchors that are not yet at L2.
Only trigger it when the current dive *would* push past L2.

---

## Share Mode (--share)

Produces output suitable for sharing with a teammate.

When `--share` is present:

- Strip the "Ask me anything" prompt
- Strip generation metadata (date, commit hash)
- Rewrite all prose using the `writing-tone` skill — concise,
  direct, Francois's voice. Not robotic AI prose.
- Keep the map diagram and all `tw`-rendered visuals
- Keep dive anchor sections at their current depth
- Hard cap: 200 lines. If over, cut the least essential
  sections and note what was cut.
- Save to the same artifact file (overwrite)

---

## Artifact Format

Save to: `<learn.output_dir>/{date}-{source-slug}.md`

The `output_dir` is read from `maestro.toml` `[learn].output_dir`.
If not configured, default to `learning/`.

Ensure the output directory exists and is in `.gitignore` (if
`[learn].gitignore = true`, which is the default).

```markdown
# Learn: {source description}

**Source:** {PR #N | branch name | file path | concept}
**Generated:** {YYYY-MM-DD}  **Commit:** {short hash}  **Depth:** {level}

## Map

{tw-rendered diagram}

```mermaid
{mermaid source for portability}
```

**Dive anchors:** [{anchor1}] [{anchor2}] [{anchor3}] ...

## {Anchor 1}: {Name}

{Content at requested depth}

## {Anchor 2}: {Name}

{Content at requested depth}

...

## Gotchas

{Non-obvious things that will bite you — always included}

---

*Ask me anything — use `/maestro:learn <source> --dive <anchor>` to go deeper.*
```

---

## Diagram Pipeline

All diagrams go through `tw`. The command writes Mermaid syntax,
pipes it through the binary, and stores BOTH in the artifact:

1. The `tw`-rendered ASCII/Unicode output (for reading)
2. The Mermaid source in a fenced `mermaid` block (for
   portability and re-rendering)

When an artifact is rewritten (via `--dive` or depth change),
re-render all diagrams through `tw`.

### tw Failure Handling

```
if tw exits non-zero:
  print "⚠ tw rendering failed: {stderr}"
  print "Showing raw Mermaid. Fix tw or re-run later."
  use fenced mermaid block only (no rendered diagram)
  continue — do not abort
```

### Diagram Style Guide

- Use `graph TD` (top-down) for hierarchies and module structure
- Use `graph LR` (left-right) for data flows and sequences
- Keep nodes to 10 or fewer per diagram — split into multiple
  diagrams if the map is complex
- Node labels should be short: concept name only, not descriptions
- Use edge labels sparingly — only when the relationship type
  isn't obvious

---

## Configuration (maestro.toml)

```toml
[learn]
output_dir = "learning"         # where artifacts are saved
tw_binary = "/path/to/tw"       # termiflow binary for diagram rendering
default_depth = "orient"        # orient | working | deep
gitignore = true                # add output_dir to .gitignore
```

If the `[learn]` section is missing, use these defaults:
- `output_dir`: `"learning"`
- `tw_binary`: search `$PATH` for `tw`, then fall back to raw
  Mermaid
- `default_depth`: `"orient"`
- `gitignore`: `true`

---

## Verification

Before saving the artifact, check:

- Does every abstraction in the map correspond to something real
  in the source? (No invented concepts)
- Does every dive anchor name appear as a section heading in the
  artifact?
- Did the depth level produce content appropriate to its
  description? (orient = why it exists; working = how to change
  it; deep = why it was built this way)
- If this is a dive rewrite: does the new section still cover
  what the prior version covered? (preservation rule)
- If `tw` rendered: does the diagram show the same abstractions
  as the text?
- **L1 check:** Does every key function, type, and interface in
  the section have a `path:line_number` pointer? A claim about
  code without a pointer is not L1.
- **L2 check:** Are all L1 pointers retained? Are planning docs,
  RFCs, decision records, or external references linked where
  they exist and informed the design? A design decision with no
  evidence trail is not L2.

If any check fails, fix before saving.

---

## Report

After producing the artifact:

```
Learn complete:
  Source:    {what was learned}
  Artifact: {file path}
  Depth:    {orient | working | deep}
  Map:      {N} abstractions, {M} relationships
  Diagrams: {N} rendered via tw | raw Mermaid (tw unavailable)
  Anchors:  [{list of dive anchors}]

Ask me anything — use --dive <anchor> to go deeper.
```

---

## Workflow Chain

**Before**: User encounters unfamiliar code, PR, or concept

**After**:
- Conversation continues — user asks questions, uses `--dive`
- If learning surfaces a decision → `/maestro:decide`
- If learning reveals a gap → `/maestro:research`
- When done → `/maestro:commit` to save progress

**Related**: `/maestro:think` (for exploring your own ideas),
`/maestro:research` (for evidence gathering),
`/maestro:review` (for critique, not understanding)

---

## Boundaries

- Read the ENTIRE source before producing output. The value is
  editorial judgment about what matters — not speed.
- Do not produce all depth levels at once. Generate only what
  was requested.
- Do not append to the artifact on dive — rewrite the section.
- Do not create multiple artifacts for the same source. One file,
  improving over time.
- Check source size before reading: >3000 lines or >30 files →
  suggest narrowing. Always count first, read second.
- If `tw` is not available and Mermaid is the fallback, say so
  clearly — don't pretend the raw syntax is a diagram.

---

END OF COMMAND
