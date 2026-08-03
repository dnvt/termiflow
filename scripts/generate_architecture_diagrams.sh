#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Generate maintained Mermaid architecture projections from the facts manifest.

Usage:
  scripts/generate_architecture_diagrams.sh [--facts PATH] --out DIR

Options:
  --facts PATH  Facts manifest (default: docs/architecture/facts.json)
  --out DIR     Output directory for generated .mmd files (required)
  -h, --help    Show this help
EOF
}

root_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
facts_path="$root_dir/docs/architecture/facts.json"
out_dir=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --facts)
      facts_path="${2:-}"
      [[ -n "$facts_path" ]] || { echo "--facts requires a path" >&2; exit 2; }
      shift 2
      ;;
    --out)
      out_dir="${2:-}"
      [[ -n "$out_dir" ]] || { echo "--out requires a directory" >&2; exit 2; }
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

[[ -n "$out_dir" ]] || { echo "--out is required" >&2; exit 2; }
[[ -f "$facts_path" ]] || { echo "facts manifest not found: $facts_path" >&2; exit 1; }
command -v jq >/dev/null 2>&1 || { echo "jq is required" >&2; exit 1; }

mkdir -p "$out_dir"
facts_sha256="$(shasum -a 256 "$facts_path" | awk '{print $1}')"
schema="$(jq -r '.schema // empty' "$facts_path")"
[[ "$schema" == "termiflow.architecture_facts.v1" ]] || {
  echo "unexpected facts schema: $schema" >&2
  exit 1
}

header() {
  local direction="$1"
  printf '%% Generated file; edit docs/architecture/facts.json instead.\n'
  printf '%% termiflow-facts-schema: %s\n' "$schema"
  printf '%% termiflow-facts-sha256: %s\n' "$facts_sha256"
  printf '%% termiflow-generator: scripts/generate_architecture_diagrams.sh\n'
  printf 'flowchart %s\n' "$direction"
}

flow_file="$out_dir/architecture-flow.mmd"
{
  header LR
  jq -r '.boundaries[] | "  " + (.id | gsub("-"; "_")) + "[" + .label + "]"' "$facts_path"
  jq -r '.layers[] | "  " + (.id | gsub("-"; "_")) + "[" + .label + "]"' "$facts_path"
  jq -r '.flows[] | "  " + (.from | gsub("-"; "_")) + " -->|" + .data + "| " + (.to | gsub("-"; "_"))' "$facts_path"
} > "$flow_file"

ownership_file="$out_dir/render-ownership.mmd"
{
  header TD
  jq -r '.layers[] | "  " + (.id | gsub("-"; "_")) + "[" + .label + "]"' "$facts_path"
  jq -r 'range(0; (.layers | length) - 1) as $i | "  " + (.layers[$i].id | gsub("-"; "_")) + " --> " + (.layers[$i + 1].id | gsub("-"; "_"))' "$facts_path"
} > "$ownership_file"

printf '%s\n' "Generated architecture diagrams from facts: $facts_sha256"
printf '%s\n' "$flow_file" "$ownership_file"
