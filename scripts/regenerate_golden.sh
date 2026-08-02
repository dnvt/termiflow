#!/usr/bin/env sh
# Compatibility entry point for guarded Rust golden checking/updating.
set -eu

root_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root_dir"
exec cargo run --locked --quiet --features qa --bin termiflow-qa -- golden "$@"
