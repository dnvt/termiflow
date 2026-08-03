#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
facts="$root_dir/docs/architecture/facts.json"
generated="$root_dir/docs/architecture/generated"
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/termiflow-architecture-facts-contract.XXXXXX")"
trap 'rm -rf -- "$tmp_dir"' EXIT

expect_failure() {
  if "$@" >/dev/null 2>&1; then
    echo "architecture facts contract: expected failure was accepted: $*" >&2
    exit 1
  fi
}

scripts="$root_dir/scripts"

jq '.layers[0].owner = "src/not-a-real-owner.rs"' "$facts" > "$tmp_dir/missing-owner.json"
expect_failure "$scripts/check_architecture_facts.sh" --facts "$tmp_dir/missing-owner.json" --generated-dir "$generated"

jq '.capabilities.unsupported_without_silent_degradation = []' "$facts" > "$tmp_dir/missing-capability.json"
expect_failure "$scripts/check_architecture_facts.sh" --facts "$tmp_dir/missing-capability.json" --generated-dir "$generated"

jq '.source.extra_path = "/Users/example/local"' "$facts" > "$tmp_dir/local-path.json"
expect_failure "$scripts/check_architecture_facts.sh" --facts "$tmp_dir/local-path.json" --generated-dir "$generated"

jq '.source.mode = "changed"' "$facts" > "$tmp_dir/stale-facts.json"
expect_failure "$scripts/check_architecture_facts.sh" --facts "$tmp_dir/stale-facts.json" --generated-dir "$generated"

cp -R "$generated" "$tmp_dir/generated-missing"
rm -- "$tmp_dir/generated-missing/render-ownership.mmd"
expect_failure "$scripts/check_architecture_facts.sh" --facts "$facts" --generated-dir "$tmp_dir/generated-missing"

"$scripts/check_architecture_facts.sh"
printf '%s\n' 'architecture facts contract: PASS'
