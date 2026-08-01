#!/usr/bin/env sh
# Portable Bash entry point for the Rust visual-audit packet runner.
set -eu

root_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root_dir"
exec cargo run --quiet --features qa --bin termiflow-qa -- visual-audit "$@"
