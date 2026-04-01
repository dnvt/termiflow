---
name: skill-creator
description:
  Use when creating or updating maestro skills, standardizing
  skill structure, or adding support resources.
version: 1.0.0
allowed-tools: [Read, Grep, Glob, Write, Bash]
---

# Skill Creator

Creates and updates skills that follow the maestro format. Use
this skill when the task is defining a new skill, tightening an
existing one, or deciding whether supporting templates, examples,
or scripts belong with it.

## Skill Structure

```
skills/{skill-name}/
├── SKILL.md          # Required: skill definition
├── examples/         # Optional: example artifacts
├── templates/        # Optional: reusable templates
└── scripts/          # Optional: automation scripts
```

## SKILL.md Format

```markdown
---
name: { skill-name }
description: { one-line description — used for skill selection }
version: { semver }
allowed-tools: [{ list of tools this skill needs }]
---

# {Skill Name}

{2-3 sentence overview of what this skill covers and when to use
it.}

## {Core Content Sections}

{Domain knowledge, frameworks, procedures, patterns.}

## When to Use This Skill

{Explicit triggers — what signals should cause this skill to be
loaded?}

## Integration Points

{How this skill connects to commands, other skills, and agents.}
```

## Quality Checklist

Before finalizing a new skill, verify:

- [ ] Name is descriptive and kebab-case
- [ ] Description fits in one line (used for selection, not
      education)
- [ ] Content is actionable, not just informational
- [ ] Includes "When to Use" section with clear triggers
- [ ] Includes "Integration Points" linking to commands/agents
- [ ] No duplicated content from existing skills
- [ ] Size is proportional: 3-12K characters ideal
- [ ] Frontmatter `allowed-tools` reflects actual needs

## Naming Conventions

- Use kebab-case: `growth-strategy`, not `growthStrategy`
- Name for the domain, not the method: `product-strategy` not
  `rice-scoring`
- Avoid generic names: `planning` is too broad,
  `initiative-planning` is better

## When NOT to Create a Skill

- If the knowledge fits in an existing skill, extend it instead
- If it's a one-time procedure, put it in `.maestro/guides/`
  instead
- If it's a behavioral rule, put it in a rule instead
- If it's project-specific, it may belong in a project override
- If it is portable methodology for multiple repos, upstream it
  to the shared core instead of forking it locally

## When to Use This Skill

- A new recurring domain or workflow deserves its own reusable
  skill
- Existing skills have drifted in structure, metadata, or trigger
  clarity
- A fragile workflow might benefit from bundled examples,
  templates, or scripts
- A capability seems too project-specific for a rule but too
  reusable for a one-off guide

## Integration Points

After creating a skill:

1. Put the skill in the right source layer:
   shared core, active pack, or `.maestro-project/skills/`
2. Update the workflow-orchestration rule if command-to-skill
   routing changes
3. Run `generate.sh` to propagate
4. Update the relevant smoke tests or maintainer docs if the change
   affects workflow behavior
