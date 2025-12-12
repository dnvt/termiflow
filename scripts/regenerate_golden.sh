#!/usr/bin/env bash
set -euo pipefail

# Regenerate golden expected outputs for `cargo test --features golden`.
#
# Writes to: tests/fixtures/expected/
#
# Notes:
# - Golden tests use `include_str!()`, so expected files must exist at build time.
# - Output is cropped by default (unless `--no-crop` is passed).

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${root_dir}"

mkdir -p tests/fixtures/expected

echo "Building…" >&2
cargo build -q

bin="target/debug/termiflow"
if [[ ! -x "${bin}" ]]; then
  echo "Binary not found: ${bin}" >&2
  exit 1
fi

for f in tests/fixtures/inputs/*.md; do
  base="$(basename "${f%.md}")"

  # Special-case: this fixture has legacy expected filenames in `tests/golden.rs`.
  if [[ "${base}" == "subgraph_basic_td" ]]; then
    "${bin}" --print --style unicode "${f}" > "tests/fixtures/expected/${base}_unicode.txt" 2> /dev/null || true
    "${bin}" --print --style ascii "${f}" > "tests/fixtures/expected/${base}_ascii.txt" 2> /dev/null || true
    continue
  fi

  if [[ "${base}" == "error_sequence" ]]; then
    "${bin}" --print --style unicode "${f}" 1> /dev/null 2> "tests/fixtures/expected/${base}.unicode.txt" || true
    "${bin}" --print --style ascii "${f}" 1> /dev/null 2> "tests/fixtures/expected/${base}.ascii.txt" || true
    continue
  fi

  "${bin}" --print --style unicode "${f}" > "tests/fixtures/expected/${base}.unicode.txt" 2> /dev/null || true
  "${bin}" --print --style ascii "${f}" > "tests/fixtures/expected/${base}.ascii.txt" 2> /dev/null || true
done

echo "Regenerated: tests/fixtures/expected" >&2

