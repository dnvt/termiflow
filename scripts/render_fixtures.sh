#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Render all fixture inputs into a timestamped output directory.

Usage:
  scripts/render_fixtures.sh [--out DIR] [--style STYLE] [--ascii] [--unicode]

Defaults:
  --out   artifacts/fixtures/<timestamp>
  --style unicode
  renders only the selected style (use --ascii/--unicode to render both)

Examples:
  scripts/render_fixtures.sh
  scripts/render_fixtures.sh --ascii --unicode
  scripts/render_fixtures.sh --style ascii
  scripts/render_fixtures.sh --out /tmp/termiflow-fixtures --ascii --unicode
EOF
}

out_dir=""
style="unicode"
render_ascii=false
render_unicode=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out)
      out_dir="${2:-}"
      shift 2
      ;;
    --style)
      style="${2:-}"
      shift 2
      ;;
    --ascii)
      render_ascii=true
      shift
      ;;
    --unicode)
      render_unicode=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown arg: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "${out_dir}" ]]; then
  ts="$(date +"%Y%m%d-%H%M%S")"
  out_dir="artifacts/fixtures/${ts}"
fi

if ! ${render_ascii} && ! ${render_unicode}; then
  case "${style}" in
    ascii) render_ascii=true ;;
    unicode) render_unicode=true ;;
    *)
      echo "Unsupported style: ${style} (expected: ascii|unicode)" >&2
      exit 2
      ;;
  esac
fi

mkdir -p "${out_dir}"

echo "Building…" >&2
cargo build -q

bin="target/debug/termiflow"
if [[ ! -x "${bin}" ]]; then
  echo "Binary not found: ${bin}" >&2
  exit 1
fi

inputs=(tests/fixtures/inputs/*.md)
if [[ ${#inputs[@]} -eq 0 ]]; then
  echo "No inputs found in tests/fixtures/inputs/*.md" >&2
  exit 1
fi

echo "Rendering ${#inputs[@]} fixtures to ${out_dir}" >&2

for f in "${inputs[@]}"; do
  base="$(basename "${f%.md}")"

  if ${render_unicode}; then
    "${bin}" --print "${f}" > "${out_dir}/${base}.unicode.txt" 2> "${out_dir}/${base}.unicode.log" || true
  fi
  if ${render_ascii}; then
    "${bin}" --print --style=ascii "${f}" > "${out_dir}/${base}.ascii.txt" 2> "${out_dir}/${base}.ascii.log" || true
  fi
done

{
  echo "Rendered at: $(date)"
  echo "Output dir: ${out_dir}"
  echo
  echo "Inputs:"
  for f in "${inputs[@]}"; do
    echo "  - ${f}"
  done
} > "${out_dir}/INDEX.txt"

echo "Done." >&2
