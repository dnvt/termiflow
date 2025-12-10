# Test Fixtures

Golden test fixtures for TermiFlow diagram rendering.

## Structure

```
fixtures/
├── inputs/          # Mermaid-lite input diagrams (.md)
├── expected/        # Expected output files (.unicode.txt, .ascii.txt)
└── README.md        # This file (source of truth)
```

## Naming Convention

```
[category]_[name]_[direction].md
```

- **Categories**: `flow`, `edge`, `label`, `shape`, `parse`, `config`, `error`
- **Direction**: `td` (top-down), `lr` (left-right), `bt` (bottom-top), `rl` (right-left)

## Test Inventory

### Direction-Consistent Tests (Same structure across all 4 directions)

| Test Name | Description | Directions |
|-----------|-------------|------------|
| `flow_simple` | Linear 3-node flow (Start → Process → End) | TD, LR, BT, RL |
| `edge_complex` | 5-node graph with divergent and convergent edges | TD, LR, BT, RL |
| `edge_branch` | API Gateway → Services → DB/Cache with multiple branches | TD, LR, BT, RL |
| `edge_converge` | 2 sources merging to 1 target | TD, LR, BT, RL |

### TD-Only Tests (Special features)

| Test Name | Description |
|-----------|-------------|
| `config_style_td` | Composite style configuration |
| `flow_branch_td` | Basic branching |
| `flow_chain_td` | Linear chain |
| `label_basic_td` | Edge labels (pipe and text syntax) |
| `parse_forward_td` | Forward reference parsing |
| `shape_all_td` | All 9 node shapes |
| `shape_database_td` | Database cylinder shape |

### Error Tests

| Test Name | Description |
|-----------|-------------|
| `error_sequence` | Unsupported diagram type |
| `error_subgraph_td` | Subgraph syntax (not supported) |

## Test Counts

- **25 input files** → **50 golden tests** (25 × 2 formats: unicode + ascii)
- **4 directions tested**: TD, LR, BT, RL
- **Direction-consistent tests**: 4 test families × 4 directions = 16 directional tests

## Regenerating Expected Outputs

To regenerate expected outputs after code changes:

```bash
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
