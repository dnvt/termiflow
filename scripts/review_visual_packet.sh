#!/bin/sh
# Portable Bash entry point for hash-bound, one-frame-at-a-time Rust visual review.
set -eu

root_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root_dir"
exec cargo run --quiet --features qa --bin termiflow-qa -- review "$@"
