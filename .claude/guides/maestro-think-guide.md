# Maestro Think Guide

**Purpose**: Use `/maestro:think` to improve framing, not to generate filler.
The command exists to help the repo make better technical and strategic choices
before momentum turns into rework.

## When To Use It

Use `/maestro:think` when:

- the user is exploring and the frame is still unstable
- the team has one apparent answer but low confidence in the assumptions
- multiple notes, reviews, or design packets need to be synthesized
- a plan exists but the real question still feels underdefined

Skip it when the task is already clear and the next step is straightforward
execution.

## Mode Selection

### Diverge

Best when the option space is too small or anchored.

Useful prompts:

- What are three materially different approaches?
- What if the main constraint disappeared?
- What if we had to solve this with one tenth of the resources?

### Converge

Best when there are too many plausible directions.

Useful prompts:

- Which option wins on speed of learning?
- Which option is easiest to reverse?
- Which option best protects the performance contract?

### Challenge

Best when the team is confident and may be missing failure modes.

Useful prompts:

- What would have to be true for the leading option to fail?
- Which assumption has the weakest evidence?
- What is being treated as a one-way door that probably is not?

### Synthesize

Best when the relevant context is scattered across plans, reviews, and
experiment logs.

Useful prompts:

- What pattern shows up across these artifacts?
- Which contradictions are still unresolved?
- What does the current evidence support, not just suggest?

### Structure

Best when the topic is real but messy.

Useful outputs:

- concise trade-off tables
- buckets of in-scope vs out-of-scope
- ordered hypotheses
- next-step ladders

## Good Outputs

A strong think session ends with:

- a clearer problem statement
- visible assumptions
- a named next command
- a capture saved only when the session materially changed understanding

## Anti-Patterns

- treating `/maestro:think` like `/maestro:plan`
- generating frameworks with no operational value
- converging before real alternatives exist
- using a challenge session to attack rather than to stress-test

## Seamless Hand-Offs

- Use `/maestro:decide` when the main job becomes choosing.
- Use `/maestro:plan` when the direction is clear and decomposition is next.
- Use `/maestro:research` when the blockage is evidence, not reasoning.
- Use `/maestro:run` when the design is ready and low ambiguity remains.
