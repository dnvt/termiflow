#!/bin/sh
# POSIX entry point for strict Rust visual-packet validation.
set -eu

root_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root_dir"
exec cargo run --quiet --features qa --bin termiflow-qa -- visual-validate "$@"
