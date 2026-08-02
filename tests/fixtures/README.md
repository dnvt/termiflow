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
├── expected/        # Expected output files (.unicode.txt, .ascii.txt) (generated)
└── README.md        # This file (source of truth)
```

The agent-facing perceptual procedure lives in
`skills/termiflow-visual-review/SKILL.md`; it is Rust/Bash-only and keeps the
machine pre-screen separate from one-frame human-visible review.

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

- **237 input files** (216 clean successes, 18 successes with warnings, 3
  expected errors); the typed contracts live in `metadata.json`
- **474 expected outputs** (ASCII + Unicode per input)
- **4 directions tested**: TD, LR, BT, RL

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

# Add structural machine coverage for rows with no warning, error, fallback, or critic signal
scripts/review_visual_packet.sh --packet /path/to/packet --decisions /tmp/review-decisions.jsonl --prescreen-clean

# Review residual frames one at a time; each record binds frame/evidence hashes
scripts/review_visual_packet.sh --packet /path/to/packet --decisions /tmp/review-decisions.jsonl --next
scripts/review_visual_packet.sh --packet /path/to/packet --decisions /tmp/review-decisions.jsonl --record /tmp/one-review.json
# For a full perceptual pass, start with a fresh decisions file and omit
# --prescreen-clean; repeat --next and --record until every frame is reviewed.
scripts/review_visual_packet.sh --packet /path/to/packet --decisions /tmp/full-review-decisions.jsonl --next
scripts/review_visual_packet.sh --packet /path/to/packet --decisions /tmp/full-review-decisions.jsonl --record /tmp/one-review.json
scripts/review_visual_packet.sh --packet /path/to/packet --decisions /tmp/review-decisions.jsonl --validate

# Close one explicit fix/hold/lesson cycle without changing goldens
scripts/visual_cycle.sh \
  --packet /path/to/packet \
  --decisions /tmp/review-decisions.jsonl \
  --record /tmp/visual-cycle.json \
  --output /tmp/visual-cycle-receipt.json

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

## Visual cycle receipt

`tests/fixtures/visual_cycle_record.schema.json` defines the record consumed by
`scripts/visual_cycle.sh`. The command first requires strict packet validation
and complete one-frame perceptual coverage, then verifies the record against
the exact packet completion/manifest/identity/checksum hashes and decision-log
hash. A record must include the human-eye observation with row/column/glyph
details, owner-layer hypothesis, predicted observation, falsifier, next
command, fix or explicit hold disposition, homologs, holdout result, and a
hash-bound durable lesson artifact.

The receipt is process evidence, not golden approval. Machine critic findings
and perceptual decisions remain separate evidence layers. A `hold` or
`falsified` cycle may keep a holdout blocked or unrun when the reason and next
command are explicit; a `fixed` cycle requires a localized fix and a passed
holdout. The wrapper never appends decisions, changes source, updates
expected outputs, or promotes a baseline.

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
