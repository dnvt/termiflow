#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Generate Mermaid review diagrams without editing files by hand.

Usage:
  scripts/generate_review_diagrams.sh [--out DIR] [--render]

Options:
  --out DIR   Output directory (default: artifacts/review-diagrams)
  --render    Also render ASCII/Unicode output and run the visual audit
  -h, --help  Show this help
EOF
}

out_dir="artifacts/review-diagrams"
render=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out)
      out_dir="${2:-}"
      if [[ -z "$out_dir" ]]; then
        echo "--out requires a directory" >&2
        exit 2
      fi
      shift 2
      ;;
    --render)
      render=true
      shift
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

mkdir -p "$out_dir"

write_diagram() {
  local name="$1"
  shift
  printf '%s\n' "$@" > "$out_dir/$name"
}

direction_suffix() {
  case "$1" in
    TD) printf '%s' td ;;
    LR) printf '%s' lr ;;
    BT) printf '%s' bt ;;
    RL) printf '%s' rl ;;
    *)
      echo "unsupported direction: $1" >&2
      exit 2
      ;;
  esac
}

for direction in TD LR BT RL; do
  suffix="$(direction_suffix "$direction")"

  write_diagram "01-basic-${suffix}.md" \
    "flowchart $direction" \
    '  A[Parse] --> B[Layout]' \
    '  B --> C[Render]' \
    '  C --> D[Output]'

  write_diagram "02-branching-${suffix}.md" \
    "flowchart $direction" \
    '  A[Request] --> B{Valid?}' \
    '  B -->|yes| C[Render]' \
    '  B -->|no| D[Report error]' \
    '  C --> E[Terminal]' \
    '  D --> E'

  write_diagram "03-subgraphs-${suffix}.md" \
    "graph $direction" \
    'subgraph SG1 [Input]' \
    '  A[Mermaid] --> B[Parser]' \
    'end' \
    'subgraph SG2 [Renderer]' \
    '  C[Layout] --> D[Routing]' \
    '  D --> E[Canvas]' \
    'end' \
    'B --> C' \
    'E --> F[Terminal]'

  write_diagram "04-dense-${suffix}.md" \
    "flowchart $direction" \
    '  A[Client] --> C[API]' \
    '  B[Worker] --> C' \
    '  C --> D[(Database)]' \
    '  C --> E[Cache]' \
    '  D --> F[Response]' \
    '  E --> F'
done

write_diagram "05-long-labels-td.md" \
  '%% termiflow: wrap=true' \
  '%% termiflow: max_lines=3' \
  'flowchart TD' \
  '  A[Very long label that should wrap across multiple lines]' \
  '  A --> B[Another long label for readability testing]'

write_diagram "06-edge-kinds-lr.md" \
  'flowchart LR' \
  '  A([Start]) ==> B{Decision}' \
  '  B -->|yes| C[(Database)]' \
  '  B -.->|retry| D[[Worker]]' \
  '  C --> E((Done))' \
  '  D --x E'

write_diagram "07-unicode-labels-td.md" \
  'flowchart TD' \
  '  A[Start ✓] --> B[日本語]' \
  '  B --> C[Done]' \
  '  C --> D[Terminal]'

if "$render"; then
  if ! command -v tw >/dev/null 2>&1; then
    echo "--render requires tw in PATH" >&2
    exit 1
  fi

  render_dir="$out_dir/rendered"
  mkdir -p "$render_dir"

  for diagram in "$out_dir"/*.md; do
    stem="$(basename "${diagram%.md}")"
    tw --strict --print "$diagram" > "$render_dir/${stem}.unicode.txt"
    tw --strict --style ascii --print "$diagram" > "$render_dir/${stem}.ascii.txt"
    tw --strict --audit --debug-critic --optimize-render --print "$diagram" \
      > /dev/null \
      2> "$render_dir/${stem}.audit.log"
  done

  echo "Rendered outputs and audit logs: $render_dir"
fi

echo "Generated Mermaid diagrams: $out_dir"
find "$out_dir" -maxdepth 1 -type f -name '*.md' -print | sort
