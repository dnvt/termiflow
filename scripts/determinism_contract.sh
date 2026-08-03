#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"

echo "determinism contract: private duplicate/order/route tests"
cargo test --locked --lib render::determinism::tests -- --test-threads=1

echo "determinism contract: repeated-process corpus test"
cargo test --locked --test render_determinism -- --test-threads=1

echo "determinism contract: PASS"
