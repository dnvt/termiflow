#!/usr/bin/env sh
# Produce an immutable, hash-bound classification of one-frame decisions.
set -eu

root_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root_dir"
exec cargo run --locked --quiet --features qa --bin termiflow-qa -- learn "$@"
