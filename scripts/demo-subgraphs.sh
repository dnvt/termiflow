#!/bin/bash
# Demo all subgraph fixtures

set -e

echo "=== Subgraph Basic (Unicode) ==="
cargo run --quiet -- --print tests/fixtures/inputs/subgraph_basic.md
echo ""

echo "=== Subgraph Basic (ASCII) ==="
cargo run --quiet -- --print --style ascii tests/fixtures/inputs/subgraph_basic.md
echo ""

echo "=== Subgraph Cross Edges (Unicode) ==="
cargo run --quiet -- --print tests/fixtures/inputs/subgraph_cross_edges.md
echo ""

echo "=== Subgraph Cross Edges (ASCII) ==="
cargo run --quiet -- --print --style ascii tests/fixtures/inputs/subgraph_cross_edges.md
echo ""

echo "=== Unsupported (now renders subgraph) ==="
cargo run --quiet -- --print tests/fixtures/inputs/unsupported.md
