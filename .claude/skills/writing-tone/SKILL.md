---
name: writing-tone
description:
  Use when drafting or reviewing written artifacts for voice,
  clarity, and audience-appropriate tone.
version: 1.0.0
allowed-tools: [Read, Grep, Glob]
---

# Writing Tone

Consistent voice across all artifacts, from quick brainstorm
captures to polished strategic documents. Use this skill whenever
clarity, specificity, or audience fit matters as much as the
underlying content.

## Voice Principles

### 1. Clear Over Clever

Say what you mean in the simplest language possible. If a 5th
grader couldn't understand the sentence structure (not the
content), simplify.

Bad: "We should endeavor to leverage our existing synergies to
optimize our go-to-market trajectory."

Good: "We should use what we already have to get to market
faster."

### 2. Confident, Not Arrogant

State your position with conviction, then show your reasoning.
Confidence is "Here's what I think and why." Arrogance is "This
is obviously right."

### 3. Specific Over General

Replace vague claims with specific ones:

- "Many users" → "23% of active users (n=450)"
- "Significantly improved" → "Reduced from 12s to 3s"
- "We believe" → "Based on {evidence}, we expect"

### 4. Active Over Passive

- "The team decided" not "It was decided"
- "We'll launch in March" not "A March launch is anticipated"
- "I recommend" not "It is recommended"

### 5. Short Over Long

- Paragraphs: 3-4 sentences max
- Sentences: 15-20 words average
- Bullet points: one idea per bullet
- Documents: as long as necessary, no longer

## Context-Specific Voice

### Internal (Thinking Sessions, Working Docs)

- Direct and honest. "This doesn't hold up because…" not "Perhaps
  we might consider…"
- Concise. If a bullet point works, don't write a paragraph.
- Opinionated with rationale. "I'd go with Option B because…" not
  "Both options have merit."
- Question-forward. Lead with the question, not the summary of
  what's known.
- No filler. Cut "I think that", "it's worth noting", "let's dive
  into".

### External (Decisions, Plans, Shared Documents)

- Context-first. Start with why before what.
- Precise language. "Increase conversion from 2% to 4%" not
  "improve significantly."
- Acknowledge trade-offs. "We chose X over Y because Z, accepting
  that…"
- No jargon without definition on first use.

### Facilitation (Brainstorming, Debates)

- Curious, not leading. "What would happen if…?" not "Don't you
  think…?"
- Build on ideas. "Yes, and…" not "But…"
- Name the tension. "I notice we're optimizing for speed and
  quality — which takes priority?"
- Hold space for incomplete ideas instead of rushing to resolve.

## Document-Specific Tone

### Brainstorm Captures

- Rough, energetic, incomplete — that's fine
- Use fragments, questions, half-formed thoughts
- Don't polish what's meant to be raw

### Decision Records

- Neutral, precise, complete
- State all options without bias in the description
- Save opinion for the Decision section

### Plans

- Actionable, forward-looking, honest about uncertainty
- Use "will" for committed items, "may" for uncertain
- Own the trade-offs: "We chose X, which means we accept Y"

### Strategic Documents

- Context-first: why before what
- Narrative structure (situation → complication → resolution)
- Quantify where possible, qualify where necessary

### External Communication

- Audience-aware: adjust vocabulary for the reader
- Lead with what matters to them, not what matters to us
- End with a clear ask or next step

## Formatting Rules

- **Headings** communicate structure — if you have 10+ headings,
  your document is probably too long or trying to do too much
- **Bold** for key terms and emphasis — max 2-3 per section
- **Lists** for 3+ parallel items — don't bullet-point everything
- **Tables** for comparisons and structured data
- **Links** to sources, not inline explanations

## Anti-Patterns

- **Weasel words:** "arguably", "it could be said", "many
  believe"
- **Corporate speak:** "synergy", "leverage", "paradigm shift",
  "disrupt"
- **Unnecessary hedging:** "I might suggest that perhaps we could
  consider" → "I suggest"
- **Emoji overload:** Use sparingly if at all in working
  documents
- **ALL CAPS for emphasis:** Use bold or restructure the sentence

## When to Use This Skill

- A document needs to be tightened for clarity, tone, or audience
  alignment
- A strategic artifact should sound decisive without drifting
  into fluff
- Internal and external audiences need distinct voice and
  formatting choices
- Another command produces written output that will be shared or
  persisted

## Integration Points

- `/maestro:run` loads this skill by default for execution
  artifacts
- `/maestro:commit` benefits from this skill when checkpoint
  summaries need to stay concise
- `/maestro:plan`, `/maestro:decide`, and `/maestro:review`
  should use it for durable written outputs
- This skill pairs well with any domain skill that supplies the
  substance but not the voice

## Output Expectations

- Rewrites should preserve intent while increasing directness and
  specificity
- The audience should remain clear from the first paragraph or
  first bullets
- If the original is vague, replace abstraction with concrete
  language rather than just shortening it

## Companion Resources

- `examples/strategy-note-before-after.md` — compact before/after
  rewrite example
