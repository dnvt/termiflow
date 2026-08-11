#!/bin/sh
# Portable entry point for the typed expected-error policy ledger.
set -eu

root_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root_dir"
exec cargo run --locked --quiet --features qa --bin termiflow-qa -- error-policy "$@"
