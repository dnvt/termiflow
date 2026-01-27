# Test Fixtures

Golden test fixtures for TermiFlow diagram rendering.

While iterating on routing/layout, you may intentionally delete or regenerate
`expected/` outputs. In that mode, prefer manual review by rendering all inputs
to a timestamped directory via `scripts/render_fixtures.sh`.

## Structure

```
fixtures/
├── inputs/          # Mermaid-lite input diagrams (.md)
├── expected/        # Expected output files (.unicode.txt, .ascii.txt) (generated)
└── README.md        # This file (source of truth)
```

Note: golden tests (`cargo test --features golden`) load expected files from disk at runtime.
Regenerate them after intentional rendering changes.

## Naming Convention

```
[category]_[name]_[direction].md
```

- **Categories**: `flow`, `edge`, `label`, `shape`, `parse`, `config`, `subgraph`, `error`
- **Direction**: `td` (top-down), `lr` (left-right), `bt` (bottom-top), `rl` (right-left)

## Test Inventory

- All non-error fixtures exist in **all four directions** (TD, LR, BT, RL).
- Error fixtures omit the direction suffix and are validated against stderr output.

## Test Counts

- **101 input files** (100 directional + 1 error)
- **202 expected outputs** (ascii + unicode per input)
- **4 directions tested**: TD, LR, BT, RL

## Regenerating Expected Outputs

To regenerate expected outputs after code changes:

```bash
# Regenerate golden snapshots (writes `tests/fixtures/expected/`)
scripts/regenerate_golden.sh

# Manual visual sweep (does not touch `expected/`)
scripts/render_fixtures.sh --ascii --unicode

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

## Direction Semantics

- **TD/LR**: Flow proceeds in natural reading direction
- **BT**: Same as TD but rendered bottom-to-top (inverted)
- **RL**: Same as LR but rendered right-to-left (mirrored)

---
Last updated: December 10, 2024
