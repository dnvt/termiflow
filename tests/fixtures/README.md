# Test Fixtures

Golden test fixtures for TermiFlow diagram rendering.

While iterating on routing/layout, keep `expected/` immutable until the change
is understood. Use the guarded updater in check mode first, then require an
explicit intent to approve any snapshot write. For perceptual review, render a
timestamped packet with `scripts/visual_audit.sh`.

## Structure

```
fixtures/
├── inputs/          # Mermaid-lite input diagrams (.md)
├── holdouts/        # Evaluator-owned schema inputs (never golden-tested)
│   └── inputs/
├── expected/        # Expected output files (.unicode.txt, .ascii.txt) (generated)
└── README.md        # This file (source of truth)
```

The agent-facing perceptual procedure lives in
`skills/termiflow-visual-review/SKILL.md`; it is Rust/Bash-only and keeps the
machine pre-screen separate from one-frame human-visible review. Every existing
input under `inputs/` is in scope for the full visual matrix and one-frame
perceptual ledger; canaries and holdouts do not replace ordinary fixture rows.

The versioned schema contract lives in `fixture_spec.json`. Each named queue
owns its review canaries, negative variants, and evaluator-owned holdouts;
holdout rows are emitted for evaluation but never receive a checked-in golden
target. Holdout source files live outside the primary input corpus so legacy
golden and metadata sweeps cannot accidentally promote them.

Note: golden tests (`cargo test --features golden`) load expected files from disk at runtime.
Regenerate them after intentional rendering changes.

## Naming Convention

```
[category]_[name]_[direction].md
```

- **Categories**: `flow`, `edge`, `label`, `shape`, `parse`, `config`, `subgraph`, `error`
- **Direction**: `td` (top-down), `lr` (left-right), `bt` (bottom-top), `rl` (right-left)

## Test Inventory

- Most non-error fixture families exist in **all four directions** (TD, LR, BT,
  RL). `warn_classDef_td` and `warn_malformed_td` are intentionally TD-only
  warning fixtures.
- Error fixtures omit the direction suffix and are validated against stderr output.

## Test Counts

- **241 input files** (220 clean successes, 18 successes with warnings, 3
  expected errors); the typed contracts live in `metadata.json`
- **482 expected outputs** (ASCII + Unicode per input)
- **964 ordinary visual rows** (ASCII + Unicode × default + optimized per
  input; expected-error rows follow their declared error policy)
- **4 directions tested**: TD, LR, BT, RL

The complete packet currently contains **952 renderable review rows** plus
**12 expected-error policy rows**. Every renderable row receives a
hash-bound `termiflow.route_clarity.v1` report before it can enter the
one-frame reviewer queue. Route `risk` and `inconclusive` statuses are
conservative signals, not approvals. The reviewer must eventually inspect
and record one decision for every renderable row; canaries, holdouts,
residual-only queues, and machine-clean prescreens do not replace that
full-corpus pass.

Renderer-wide work has a second full-corpus policy lane. The canonical packet
injects the requested `--style` for comparable ASCII/Unicode homologs. The
authored-policy packet is generated with `--respect-input-style` so native
`%% termiflow:` style, wrapping, spacing, and composite directives are not
overridden. It covers the same 241 inputs, 964 rows, 952 renderable decisions,
and 12 expected-error policy decisions. The 20 directive-bearing inputs are
high-risk, but every other input remains a no-override control; a row from
one lane cannot cover the other.

## Checking and Updating Expected Outputs

To regenerate expected outputs after code changes:

```bash
# Check snapshots without writing (default safety mode)
scripts/regenerate_golden.sh --check

# After an intentional, reviewed rendering change only:
scripts/regenerate_golden.sh --approve --intent "describe the rendering change"

# Manual visual sweep (does not touch `expected/`)
scripts/render_fixtures.sh --ascii --unicode

# Validate a completed full packet against the known quality baseline
scripts/visual_validate.sh --packet /path/to/packet --strict-quality

# Supplemental native-policy packet: do not inject --style into inputs
scripts/visual_audit.sh --respect-input-style \
  --out /tmp/termiflow-visual-no-override \
  --styles ascii,unicode --modes default,optimized
scripts/visual_validate.sh --packet /tmp/termiflow-visual-no-override \
  --strict-quality

# Validate or materialize the canonical smoke queue (16 review rows)
cargo run --features qa --bin termiflow-qa -- schema \
  --spec tests/fixtures/fixture_spec.json \
  --queue canonical-smoke --check
cargo run --features qa --bin termiflow-qa -- schema \
  --spec tests/fixtures/fixture_spec.json \
  --queue canonical-smoke \
  --emit-manifest /tmp/canonical-smoke-manifest.json

# Materialize the junction canary queue; its 16 holdout rows stay evaluator-owned
cargo run --features qa --bin termiflow-qa -- schema \
  --spec tests/fixtures/fixture_spec.json \
  --queue junction-quad \
  --emit-manifest /tmp/junction-quad-manifest.json
cargo run --features qa --bin termiflow-qa -- golden \
  --manifest /tmp/junction-quad-manifest.json --check

# Compose the schema-bound candidate/packet/holdout boundary. This stops before
# perceptual decisions and golden approval; both remain explicit next steps.
scripts/schema_visual_cycle.sh \
  --queue junction-quad \
  --manifest /tmp/junction-quad-cycle-manifest.json \
  --golden-report /tmp/junction-quad-golden-report.json \
  --packet /tmp/junction-quad-cycle-packet \
  --holdout-packet /tmp/junction-quad-cycle-holdout-packet \
  --holdout-receipt /tmp/junction-quad-cycle-holdout-receipt.json \
  --summary /tmp/junction-quad-schema-cycle.json

# Execute the evaluator-owned holdout queue without creating goldens
cargo run --features qa --bin termiflow-qa -- holdout \
  --spec tests/fixtures/fixture_spec.json \
  --queue junction-quad \
  --out /tmp/junction-quad-holdout-packet \
  --receipt /tmp/junction-quad-holdout-receipt.json

# Add structural machine coverage for rows with no warning, error, fallback, or critic signal
scripts/review_visual_packet.sh --packet /path/to/packet --decisions /tmp/review-decisions.jsonl --prescreen-clean

# Review residual frames one at a time; each record binds frame/evidence hashes
scripts/review_visual_packet.sh --packet /path/to/packet --decisions /tmp/review-decisions.jsonl --next
scripts/review_visual_packet.sh --packet /path/to/packet --decisions /tmp/review-decisions.jsonl --record /tmp/one-review.json
# Records may also be supplied as one JSON object on stdin, which is useful for
# an automated one-frame review loop without creating a private record file.
printf '%s' '{"...":"one review decision"}' | scripts/review_visual_packet.sh \
  --packet /path/to/packet --decisions /tmp/review-decisions.jsonl --record -
# Carry prior human watches/failures and hypotheses into the regenerated queue
scripts/review_visual_packet.sh --packet /path/to/packet \
  --decisions /tmp/full-review-decisions.jsonl \
  --history /tmp/visual-review-history.jsonl --next
# For the authoritative full-corpus perceptual pass, start with a fresh
# decisions file for each packet/policy lane and omit --prescreen-clean; add
# --fresh to every review command. Drain both the canonical and the
# --respect-input-style packet; the latter's effective policy must be checked
# in the evidence before judging style fidelity.
# Fresh mode rejects machine structural and carry-forward decisions. Repeat
# --fresh --next and --fresh --record until every existing input/style/mode
# frame has a separate decision. Do not stop after residual, canary, or
# machine-clean rows.
scripts/review_visual_packet.sh --packet /path/to/packet --decisions /tmp/full-review-decisions.jsonl --fresh --next
scripts/review_visual_packet.sh --packet /path/to/packet --decisions /tmp/full-review-decisions.jsonl --fresh --record /tmp/one-review.json
scripts/review_visual_packet.sh --packet /path/to/packet --decisions /tmp/review-decisions.jsonl --fresh --validate

# Convert the completed one-frame ledger into typed, hash-bound learning
# classes and grouped falsifiable hypotheses. Strict mode requires every
# renderable row to be classified; run once per policy lane.
scripts/visual_learning.sh \
  --packet /path/to/packet \
  --decisions /tmp/full-review-decisions.jsonl \
  --output /tmp/full-review-learning.json \
  --strict

# Govern expected-error rows with a separate policy ledger. These rows are
# reviewed for exit status, stdout, stderr, and the declared error contract;
# they are not perceptual golden decisions.
scripts/review_expected_errors.sh \
  --packet /path/to/packet --records /tmp/expected-errors.jsonl --next
printf '%s' '{"...":"one expected-error policy record"}' | \
  scripts/review_expected_errors.sh \
  --packet /path/to/packet --records /tmp/expected-errors.jsonl --record -
scripts/review_expected_errors.sh \
  --packet /path/to/packet --records /tmp/expected-errors.jsonl --validate

# Close one explicit fix/hold/lesson cycle without changing goldens
scripts/visual_cycle.sh \
  --packet /path/to/packet \
  --decisions /tmp/review-decisions.jsonl \
  --queue-manifest /tmp/junction-quad-manifest.json \
  --holdout-receipt /tmp/junction-quad-holdout-receipt.json \
  --holdout-decisions /tmp/junction-quad-holdout-decisions.jsonl \
  --record /tmp/visual-cycle.json \
  --output /tmp/visual-cycle-receipt.json \
  --history /tmp/visual-review-history.jsonl

# Single test
cargo run -- --print tests/fixtures/inputs/flow_simple_td.md > tests/fixtures/expected/flow_simple_td.unicode.txt
cargo run -- --print --style=ascii tests/fixtures/inputs/flow_simple_td.md > tests/fixtures/expected/flow_simple_td.ascii.txt

# All tests for a direction
for f in tests/fixtures/inputs/*_td.md; do
  base=$(basename "$f" .md)
  cargo run -- --print "$f" > "tests/fixtures/expected/${base}.unicode.txt"
  cargo run -- --print --style=ascii "$f" > "tests/fixtures/expected/${base}.ascii.txt"
done
```

## Golden Test Philosophy

1. **Same graph structure** for all directions to verify rendering algorithm
2. **Two formats** (unicode + ascii) to catch style-specific bugs
3. **Expected outputs are source of truth** - regenerate after intentional changes
4. **Fail fast** - any mismatch indicates a rendering regression
5. **Evidence first** - `scripts/visual_audit.sh` records output, dimensions,
   critic/oracle evidence, identities, and reviewable frame paths without
   touching checked-in goldens
6. **Baseline drift is explicit** - `quality_baseline.json` records only
   reviewed, actionable exceptions; new findings or missing exceptions fail
   strict validation
7. **Frame review is sequential** - inspect one frame, bind its frame/evidence
   hashes, state the observation, propose a falsifiable hypothesis, identify
   related fixtures, and choose the next targeted command before moving on
8. **The loop is self-improving** - generate schema-bound golden canaries,
   inspect rendered frames with machine evidence plus human-eye row/column and
   glyph observations, classify each decision as a confirmed flaw, topology
   ambiguity, or inconclusive review, group repeated observations into
   falsifiable hypotheses, run homolog and evaluator-owned holdout checks, then
   preserve the lesson and next target in the cycle receipt. A strict learning
   report is required before a lane can be treated as complete.

## Visual cycle receipt

`tests/fixtures/visual_cycle_record.schema.json` defines the record consumed by
`scripts/visual_cycle.sh`. The command first requires strict packet validation
and complete one-frame perceptual coverage, then verifies the record against
the exact packet completion/manifest/identity/checksum hashes and decision-log
hash. Its required `scope` object repeats the queue, packet, review, holdout,
fix, homolog result, golden-approval, and lesson bindings so a record cannot
look complete while its top-level evidence refers to another run. A record
must include the human-eye observation with row/column/glyph details,
owner-layer hypothesis, predicted observation, falsifier, next command, fix or
explicit hold disposition, homolog result, holdout result, and a hash-bound
durable lesson artifact.

The receipt is process evidence, not golden approval. Machine critic findings
and perceptual decisions remain separate evidence layers. A `hold` or
`falsified` cycle may keep a holdout blocked or unrun when the reason and next
command are explicit; a `fixed` cycle requires a localized fix and a passed
holdout. The wrapper never appends decisions, changes source, updates
expected outputs, or promotes a baseline.

`schema_visual_cycle.sh` is the reusable boundary before that sequential
review: it materializes the canonical Mermaid queue, checks exact golden
candidates, creates strict main and evaluator-owned holdout packets, and emits
`termiflow.schema_visual_cycle.v1`. It never appends decisions or approves
goldens. Use its emitted one-frame review command, then `visual_cycle.sh`, and
only afterward use the separate intent-bound golden approval command for an
intentional rendering change.

There is intentionally no structural-review escape hatch. A clean machine
pre-screen is not a human-eye decision; a deliberate full perceptual pass uses
a fresh decisions file and drains `--next` one frame at a time until
`--validate` succeeds.

## Direction Semantics

- **TD/LR**: Flow proceeds in natural reading direction
- **BT**: Same as TD but rendered bottom-to-top (inverted)
- **RL**: Same as LR but rendered right-to-left (mirrored)

---
Last updated: August 2, 2026
