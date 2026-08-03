#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Validate the portable architecture facts source and generated Mermaid files.

Usage:
  scripts/check_architecture_facts.sh [--facts PATH] [--generated-dir DIR]

Options:
  --facts PATH          Facts manifest (default: docs/architecture/facts.json)
  --generated-dir DIR   Generated Mermaid directory (default: docs/architecture/generated)
  -h, --help            Show this help
EOF
}

root_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
facts_path="$root_dir/docs/architecture/facts.json"
generated_dir="$root_dir/docs/architecture/generated"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --facts)
      facts_path="${2:-}"
      [[ -n "$facts_path" ]] || { echo "--facts requires a path" >&2; exit 2; }
      shift 2
      ;;
    --generated-dir)
      generated_dir="${2:-}"
      [[ -n "$generated_dir" ]] || { echo "--generated-dir requires a directory" >&2; exit 2; }
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

fail() {
  echo "architecture facts: $1" >&2
  exit 1
}

[[ -f "$facts_path" ]] || fail "facts manifest not found: $facts_path"
[[ -d "$generated_dir" ]] || fail "generated directory not found: $generated_dir"
command -v jq >/dev/null 2>&1 || fail "jq is required"
command -v shasum >/dev/null 2>&1 || fail "shasum is required"

jq empty "$facts_path" || fail "facts manifest is invalid JSON"
[[ "$(jq -r '.schema // empty' "$facts_path")" == "termiflow.architecture_facts.v1" ]] || fail "facts schema is not termiflow.architecture_facts.v1"
[[ "$(jq -r '.version // 0' "$facts_path")" == "1" ]] || fail "facts version is not 1"

if rg -n '"(checkout|branch|evidence_path)"|/Users/|/private/|/tmp/|/Developer/' \
  "$facts_path" "$root_dir/docs/architecture/baseline-manifest.json" "$root_dir/docs/architecture/capability-matrix.json" >/dev/null; then
  fail "architecture facts contain machine-local paths or historical branch fields"
fi

[[ "$(jq -r '.source.owner // empty' "$facts_path")" == "docs/architecture/facts.json" ]] || fail "facts source owner is missing"
[[ "$(jq -r '.provenance.generator // empty' "$facts_path")" == "scripts/generate_architecture_diagrams.sh" ]] || fail "facts generator provenance is missing"
[[ "$(jq -r '.capabilities.unsupported_without_silent_degradation | length' "$facts_path")" -gt 0 ]] || fail "unsupported capabilities are not declared"
[[ "$(jq -r '.diagnostics | length' "$facts_path")" -gt 0 ]] || fail "diagnostics are not declared"

while IFS= read -r owner; do
  [[ -n "$owner" ]] || continue
  [[ -e "$root_dir/$owner" ]] || fail "source owner does not exist: $owner"
done < <(jq -r '.. | objects | .owner? // empty' "$facts_path")

while IFS= read -r inventory; do
  [[ -n "$inventory" ]] || continue
  [[ -f "$root_dir/$inventory" ]] || fail "supporting inventory does not exist: $inventory"
  case "$inventory" in
    *.json) jq empty "$root_dir/$inventory" || fail "supporting inventory is invalid JSON: $inventory" ;;
  esac
done < <(jq -r '.source.supporting_inventories[]?' "$facts_path")

facts_sha256="$(shasum -a 256 "$facts_path" | awk '{print $1}')"
while IFS= read -r relative_path; do
  [[ -n "$relative_path" ]] || continue
  name="$(basename "$relative_path")"
  generated="$generated_dir/$name"
  [[ -f "$generated" ]] || fail "generated output is missing: $relative_path"
  rg -q "^% termiflow-facts-sha256: $facts_sha256$" "$generated" || fail "generated output has stale facts digest: $relative_path"
  rg -q '^% termiflow-generator: scripts/generate_architecture_diagrams.sh$' "$generated" || fail "generated output has no generator marker: $relative_path"
  rg -q '^flowchart (TD|TB|LR|RL|BT)$' "$generated" || fail "generated output has no supported flowchart header: $relative_path"
  if command -v tw >/dev/null 2>&1; then
    tw --strict --print "$generated" >/dev/null || fail "strict Mermaid validation failed: $relative_path"
  fi
done < <(jq -r '.provenance.generated_outputs[]' "$facts_path")

temporary_dir="$(mktemp -d "${TMPDIR:-/tmp}/termiflow-architecture-facts.XXXXXX")"
trap 'rm -rf -- "$temporary_dir"' EXIT
"$root_dir/scripts/generate_architecture_diagrams.sh" --facts "$facts_path" --out "$temporary_dir/generated" >/dev/null
for generated in "$generated_dir"/*.mmd; do
  name="$(basename "$generated")"
  cmp -s "$generated" "$temporary_dir/generated/$name" || fail "generated output drifted: $name"
done

printf 'architecture facts: PASS (%s)\n' "$facts_sha256"
