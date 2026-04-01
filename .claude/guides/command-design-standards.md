# Command Design Standards

**Purpose**: Ensure all maestro commands are reliable, discoverable, and
executable by Claude Code.

**Created**: 2025-10-10 (Based on `/maestro:review` command execution failure
analysis)

---

## Core Principles

### 1. Commands Are Imperative, Not Informational

**Problem**: Commands that look like documentation don't trigger execution.

**Solution**: Clear execution signals at the top of every command file.

**Template**:

```markdown
# /{command-name} - Brief Title

**⚠️ COMMAND EXECUTION MODE ⚠️**

When this command is invoked, you MUST:

1. **STOP** all work in progress immediately
2. **LOAD** required context/specifications
3. **EXECUTE** the workflow below
4. **REPORT** results/findings
5. **DO NOT** resume previous work until command completes
```

### 2. Keep Commands Short (100-250 Lines Max)

**Rationale**: Long specifications compete with task execution in context
window.

**Length Targets**:

- **Core command**: 100-250 lines (essential workflow only)
- **Detailed guide**: 300-500 lines (philosophy, examples, patterns)
- **Checklists**: 30-50 lines each (quick reference)
- **Templates**: 50-100 lines (output formats)

**Enforcement**: If command exceeds 250 lines, split into:

- `.claude/commands/{name}.md` - Core workflow
- `.claude/guides/{name}-guide.md` - Detailed examples
- `.claude/references/{name}-checklist.md` - Quick checklists

### 3. Use Imperative Language Throughout

**Forbidden** (Passive/Descriptive):

- "This command validates completion"
- "The workflow involves..."
- "You should check..."

**Required** (Imperative/Active):

- "STOP all work immediately"
- "EXECUTE the 9-phase workflow"
- "LOAD the specification file"
- "REPORT completion status"

**Why**: Imperative language creates unmistakable execution signals.

### 4. Separate Core Workflow from Educational Content

**Command Structure**:

```
.claude/commands/prosody-example.md (150 lines)
├── Execution header (imperative signals)
├── Purpose (2-3 sentences)
├── Usage examples
├── Workflow (numbered phases with clear steps)
└── References to detailed guides

.claude/guides/example-guide.md (400 lines)
├── Philosophy
├── Detailed examples
├── Common patterns
├── Troubleshooting
└── Best practices

.claude/references/example-checklist.md (40 lines)
├── Criterion 1
├── Criterion 2
└── Criterion 3
```

### 5. Explicit Phase-Based Workflows

**Pattern**: Number all workflow phases clearly.

**Example**:

```markdown
## 9-Phase Workflow

### Phase 1: Load Context

[steps]

### Phase 2: Assess Completion

[steps]

### Phase 3: Analyze Documentation

[steps]
```

**Benefits**:

- Clear progression through workflow
- Easy to track current phase
- Natural checkpoints for validation

### 6. Include Stop Conditions

**Pattern**: Tell Claude when to STOP and not proceed.

**Example**:

```markdown
### Phase 6: Generate Remediation Plan (if incomplete)

[remediation steps]

**STOP HERE** - Do not proceed to cleanup phases.
```

**Why**: Prevents Claude from continuing workflow when conditions aren't met.

### 7. Every Command Must Support `--help`

Every command must define a compact help mode.

When invoked with `--help`, the command should:

- state its purpose in one sentence
- explain when to use it
- list supported flags and what each flag changes
- show expected inputs
- show expected outputs
- provide realistic examples
- point to related next commands

The help view should be enough for day-to-day discovery without forcing the
user into the full guide.

### 8. Prefer Flags Over New Top-Level Commands When The Workflow Is The Same

If a smaller command is really a mode of a larger workflow, prefer a flag or
submode instead of creating a new top-level verb.

Create a separate command only when:

- it has a distinct purpose
- it is used independently often enough to justify discovery cost
- it meaningfully changes the workflow shape rather than just its focus

---

## Command Structure Template

````markdown
# /{command-name} - Brief Title (3-5 words)

**⚠️ COMMAND EXECUTION MODE ⚠️**

When this command is invoked, you MUST:

1. **STOP** all work in progress immediately
2. **LOAD** [what context to load]
3. **EXECUTE** the [N]-phase workflow below
4. **REPORT** [what to report]
5. **DO NOT** resume previous work until command completes

---

## Purpose

[2-3 sentence description of what this command does]

**Philosophy**: [Core principle, if applicable]

---

## Usage

```bash
/{command-name} {args}

# Examples:
/{command-name} example:1     # Description
/{command-name} example:2     # Description
```
````

---

## [N]-Phase Workflow

### Phase 1: [Name]

[Clear, numbered steps]

---

### Phase 2: [Name]

[Clear, numbered steps]

---

[... continue for all phases ...]

---

## Reference Documents

- **Detailed guide**: `.claude/guides/{name}-guide.md`
- **Checklists**: `.claude/references/{name}-checklist.md`
- **Templates**: `.claude/references/{name}-template.md`

---

## Quality Enforcement

- [Key requirement 1]
- [Key requirement 2]
- [Key requirement 3]

---

**Status**: [Brief status note] **Execution Time**: [Estimated time]
**Outcome**: [Expected outcome]

---

**REMEMBER**: This is an EXECUTION command. When invoked, STOP current work and
EXECUTE this workflow immediately.

````
---

## Checklist Structure Template

```markdown
# {Command Name} Checklist

## Section 1

- [ ] Criterion 1
- [ ] Criterion 2
- [ ] Criterion 3

## Section 2

- [ ] Criterion 4
- [ ] Criterion 5

[... additional sections as needed ...]
````

**Characteristics**:

- Clear checkbox format (`- [ ]`)
- Grouped into logical sections
- 20-30 criteria max (focused)
- Actionable items only

---

## Report Template Structure

```markdown
# {Work Type} {Work ID} - Report Title

## SECTION 1: SUMMARY

[High-level overview]

---

## SECTION 2: DETAILED FINDINGS

[Detailed information]

---

## SECTION 3: NEXT STEPS

[Actionable items]

---

**Status**: [Final status]
```

**Characteristics**:

- Clear section headers
- Scannable format (bullets, checkboxes)
- Summary at top
- Next steps at bottom

---

## File Naming Conventions

### Commands

- **Pattern**: `{verb}-{noun}.md` or `prosody-{action}.md`
- **Examples**: `maestro:review.md`, `maestro:run.md`, `prosody-validate.md`
- **Location**: `.claude/commands/`

### Guides

- **Pattern**: `{topic}-guide.md`
- **Examples**: `review-guide.md`, `execution-guide.md`
- **Location**: `.claude/guides/`

### Checklists

- **Pattern**: `{command-name}-checklist-{variant}.md`
- **Examples**: `review-checklist-task.md`, `review-checklist-feature.md`
- **Location**: `.claude/references/`

### Templates

- **Pattern**: `{output-type}-template.md`
- **Examples**: `completion-report-template.md`, `remediation-plan-template.md`
- **Location**: `.claude/references/`

---

## Command Length Audit Checklist

When reviewing or creating commands, verify:

- [ ] Core command file <250 lines
- [ ] Imperative execution header present
- [ ] Clear phase-based workflow (numbered)
- [ ] Stop conditions specified where needed
- [ ] References to detailed guides (not inline)
- [ ] Checklists in separate files
- [ ] Templates in separate files
- [ ] No extensive examples in core command
- [ ] No lengthy philosophy sections inline

**If any fail**: Refactor to split content appropriately.

---

## Migration Guide: Existing Commands

### For Long Commands (>300 lines):

1. **Identify sections**:
   - Core workflow (keep in command)
   - Philosophy/examples (move to guide)
   - Checklists (move to references)
   - Templates (move to references)

2. **Split files**:
   - Create `.claude/guides/{name}-guide.md`
   - Create `.claude/references/{name}-*.md` files
   - Update core command to reference new files

3. **Add imperative header**:
   - Copy template from this document
   - Customize for specific command

4. **Test**:
   - Verify command triggers execution
   - Confirm references load correctly

### Priority Order (Most Critical First):

1. `/maestro:review` ✅ (Completed 2025-10-10)
2. `/maestro:run` (Long, complex workflow)
3. Legacy orchestration meta-command (retired `/prosody-execute`)
4. `/prosody-validate-state` (Long workflow)

---

## Examples: Good vs Bad

### ❌ BAD: Passive, Long, Educational

```markdown
# /example-command

This command helps you validate completion of work items.

The philosophy behind this command is to ensure...

[500 lines of examples, philosophy, patterns]

To use this command, you would typically...
```

**Problems**:

- No execution signal
- Passive language ("helps you", "would typically")
- Too long (500+ lines)
- Educational content inline

### ✅ GOOD: Imperative, Short, Workflow-Focused

```markdown
# /example-command - Work Validation

**⚠️ COMMAND EXECUTION MODE ⚠️**

When this command is invoked, you MUST:

1. **STOP** all work immediately
2. **LOAD** work specification
3. **EXECUTE** 5-phase workflow
4. **REPORT** validation results
5. **DO NOT** resume until complete

## 5-Phase Workflow

### Phase 1: Load Specification

[Clear steps]

### Phase 2: Validate Criteria

[Clear steps]

[... phases 3-5 ...]

## References

- Guide: `.claude/guides/example-guide.md`
- Checklist: `.claude/references/example-checklist.md`

**REMEMBER**: EXECUTE immediately when invoked.
```

**Strengths**:

- Clear execution signal
- Imperative language throughout
- Short (150 lines)
- References detailed content

---

## Validation Checklist for New Commands

Before committing a new command, verify:

- [ ] Imperative execution header present (⚠️ COMMAND EXECUTION MODE ⚠️)
- [ ] Core workflow <250 lines
- [ ] Numbered phases with clear steps
- [ ] Stop conditions where applicable
- [ ] Detailed content moved to guides/references
- [ ] Imperative language throughout (STOP, EXECUTE, LOAD, REPORT)
- [ ] Clear purpose statement (2-3 sentences)
- [ ] Usage examples provided
- [ ] References to detailed guides
- [ ] Final "REMEMBER" execution reminder

---

## Enforcement

All commands in `.claude/commands/` MUST follow these standards.

**Audit Schedule**:

- New commands: Before commit
- Existing commands: During next major update
- Full audit: Once per epic

**Quality Gate**:

- PR reviews check command length
- Maestro command updates require standards compliance
- Legacy commands updated as needed (not blocking)

---

**Status**: Standards v1.0 (Based on `/maestro:review` redesign) **Next
Review**: After 5 command updates **Owner**: Maestro orchestration system

---

**Last Updated**: 2025-10-10
