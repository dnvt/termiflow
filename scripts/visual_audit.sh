#!/usr/bin/env bash
# Fail-closed fixture-matrix runner. It never writes golden snapshots.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/visual_audit.sh [options]

Options:
  --out DIR             Final artifact directory (default: artifacts/visual-audit/<timestamp>)
  --styles CSV          Styles to render (default: ascii,unicode)
  --modes CSV           Modes to render (default: default,optimized)
  --binary PATH         Use a prebuilt binary (test seam; skips Cargo discovery)
  --input-root DIR      Fixture directory (default: tests/fixtures/inputs)
  --help                Show this help

The runner builds/discovers the configured target with Cargo JSON messages,
writes an atomic JSONL manifest, and exits nonzero on any unexpected row.
EOF
}

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"

out_dir=""
styles_csv="ascii,unicode"
modes_csv="default,optimized"
binary=""
input_root="tests/fixtures/inputs"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) out_dir="${2:?--out requires a directory}"; shift 2 ;;
    --styles) styles_csv="${2:?--styles requires a CSV value}"; shift 2 ;;
    --modes) modes_csv="${2:?--modes requires a CSV value}"; shift 2 ;;
    --binary) binary="${2:?--binary requires a path}"; shift 2 ;;
    --input-root) input_root="${2:?--input-root requires a directory}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [[ -z "$out_dir" ]]; then
  out_dir="artifacts/visual-audit/$(date -u +%Y%m%dT%H%M%SZ)"
fi
case "$out_dir" in
  tests/fixtures/expected|tests/fixtures/expected/*)
    echo "refusing to write golden fixture directory: $out_dir" >&2
    exit 2
    ;;
esac
[[ -d "$input_root" ]] || { echo "missing input root: $input_root" >&2; exit 2; }
[[ ! -e "$out_dir" ]] || { echo "artifact directory already exists: $out_dir" >&2; exit 2; }

IFS=',' read -r -a styles <<< "$styles_csv"
IFS=',' read -r -a modes <<< "$modes_csv"
for style in "${styles[@]}"; do
  case "$style" in ascii|unicode|double|rounded|heavy|dots|plus|stars|blocks) ;; *) echo "unsupported style: $style" >&2; exit 2;; esac
done
for mode in "${modes[@]}"; do
  case "$mode" in default|optimized) ;; *) echo "unsupported mode: $mode" >&2; exit 2;; esac
done

stage_dir="${out_dir}.staging.$$"
mkdir -p "$stage_dir/frames" "$stage_dir/logs"
cleanup() { rm -rf "$stage_dir"; }
trap cleanup EXIT

if [[ -z "$binary" ]]; then
  cargo_messages="$stage_dir/cargo-build.jsonl"
  cargo build --bin termiflow --message-format=json > "$cargo_messages"
  binary="$(python3 - "$cargo_messages" <<'PY'
import json, sys
for line in open(sys.argv[1], encoding="utf-8"):
    event = json.loads(line)
    target = event.get("target", {})
    if event.get("reason") == "compiler-artifact" and target.get("name") == "termiflow" and "bin" in target.get("kind", []):
        executable = event.get("executable")
        if executable:
            print(executable)
            break
PY
)"
fi
[[ -n "$binary" && -x "$binary" ]] || { echo "discovered binary is not executable: $binary" >&2; exit 1; }

mapfile -d '' inputs < <(find "$input_root" -type f -name '*.md' -print0 | sort -z)
(( ${#inputs[@]} > 0 )) || { echo "no fixture inputs" >&2; exit 1; }

manifest="$stage_dir/manifest.jsonl"
summary="$stage_dir/summary.json"
expected_rows=0
actual_rows=0
primary_rows=0
failures=0
warning_rows=0
error_rows=0

write_row() {
  local input="$1" base="$2" style="$3" mode="$4" classification="$5" status="$6" stdout_path="$7" stderr_path="$8" expected_stderr_path="$9"
  python3 - "$manifest" "$input" "$base" "$style" "$mode" "$classification" "$status" "$stdout_path" "$stderr_path" "$expected_stderr_path" "$binary" <<'PY'
import hashlib, json, os, sys
manifest, input_path, base, style, mode, classification, status, stdout_path, stderr_path, expected_path, binary = sys.argv[1:]
def digest(path):
    return hashlib.sha256(open(path, "rb").read()).hexdigest()
def size(path):
    return os.path.getsize(path)
row = {
    "schema": "termiflow.visual_audit.row.v1",
    "case_id": hashlib.sha256(open(input_path, "rb").read() + b"\0" + style.encode() + b"\0" + mode.encode()).hexdigest(),
    "input": input_path, "fixture": base, "style": style, "mode": mode,
    "classification": classification, "status": int(status),
    "argv": [binary, "--print", "--style", style] + (["--optimize-render"] if mode == "optimized" else []) + [input_path],
    "stdout": {"path": stdout_path, "sha256": digest(stdout_path), "bytes": size(stdout_path)},
    "stderr": {"path": stderr_path, "sha256": digest(stderr_path), "bytes": size(stderr_path)},
    "expected_stderr": expected_path or None,
}
with open(manifest, "a", encoding="utf-8") as f:
    f.write(json.dumps(row, sort_keys=True) + "\n")
PY
}

for input in "${inputs[@]}"; do
  base="$(basename "${input%.md}")"
  is_error=false
  [[ "$base" == error_* ]] && is_error=true
  for style in "${styles[@]}"; do
    for mode in "${modes[@]}"; do
      if "$is_error"; then
        expected_rows=$((expected_rows + 1))
      else
        expected_rows=$((expected_rows + 1))
        primary_rows=$((primary_rows + 1))
      fi
      stem="${base}.${style}.${mode}"
      stdout_path="$stage_dir/frames/$stem.txt"
      stderr_path="$stage_dir/logs/$stem.log"
      args=(--print --style "$style")
      [[ "$mode" == optimized ]] && args+=(--optimize-render)
      set +e
      "$binary" "${args[@]}" "$input" > "$stdout_path" 2> "$stderr_path"
      status=$?
      set -e
      classification="success"
      expected_stderr_path=""
      if "$is_error"; then
        classification="expected_error"
        expected_stderr_path="tests/fixtures/expected/${base}.${style}.txt"
        if [[ "$status" -eq 0 || -s "$stdout_path" || ! -f "$expected_stderr_path" ]] || ! cmp -s "$stderr_path" "$expected_stderr_path"; then
          echo "unexpected error-fixture result: $stem" >&2
          failures=$((failures + 1))
        fi
        error_rows=$((error_rows + 1))
      elif [[ "$base" == cycle_* ]]; then
        classification="success_with_warning"
        if [[ "$status" -ne 0 || ! -s "$stdout_path" ]] || ! grep -qx 'termiflow: warning: Cycle detected, rendering back-edges in gutter' "$stderr_path"; then
          echo "unexpected cycle warning result: $stem" >&2
          failures=$((failures + 1))
        fi
        warning_rows=$((warning_rows + 1))
      elif [[ "$base" == warn_classDef_td ]]; then
        classification="success_with_warning"
        if [[ "$status" -ne 0 || ! -s "$stdout_path" ]] || [[ "$(wc -l < "$stderr_path" | tr -d ' ')" -ne 2 ]] || ! grep -qx 'termiflow: warning: line 3: Mermaid classes not supported' "$stderr_path"; then
          echo "unexpected classDef warning result: $stem" >&2
          failures=$((failures + 1))
        fi
        warning_rows=$((warning_rows + 1))
      elif [[ "$base" == warn_malformed_td ]]; then
        classification="success_with_warning"
        if [[ "$status" -ne 0 || ! -s "$stdout_path" ]] || [[ "$(wc -l < "$stderr_path" | tr -d ' ')" -ne 1 ]]; then
          echo "unexpected malformed warning result: $stem" >&2
          failures=$((failures + 1))
        fi
        warning_rows=$((warning_rows + 1))
      elif [[ "$status" -ne 0 || ! -s "$stdout_path" || -s "$stderr_path" ]]; then
        echo "unexpected success-fixture result: $stem" >&2
        failures=$((failures + 1))
      fi
      write_row "$input" "$base" "$style" "$mode" "$classification" "$status" "$stdout_path" "$stderr_path" "$expected_stderr_path"
      actual_rows=$((actual_rows + 1))
    done
  done
done

if [[ "$actual_rows" -ne "$expected_rows" ]]; then
  echo "row-count mismatch: expected $expected_rows, got $actual_rows" >&2
  failures=$((failures + 1))
fi
python3 - "$summary" "$binary" "$expected_rows" "$actual_rows" "$primary_rows" "$error_rows" "$warning_rows" "$failures" "$styles_csv" "$modes_csv" <<'PY'
import json, sys
path, binary, expected, actual, primary, errors, warnings, failures, styles, modes = sys.argv[1:]
with open(path, "w", encoding="utf-8") as f:
    json.dump({"schema":"termiflow.visual_audit.summary.v1", "binary":binary,
               "expected_rows":int(expected), "actual_rows":int(actual), "primary_rows":int(primary),
               "expected_error_rows":int(errors), "warning_rows":int(warnings),
               "failures":int(failures), "styles":styles.split(','), "modes":modes.split(',')}, f, sort_keys=True)
    f.write("\n")
PY

if [[ "$failures" -ne 0 ]]; then
  echo "visual audit failed with $failures unexpected row(s); staging retained at $stage_dir" >&2
  trap - EXIT
  exit 1
fi
mv "$stage_dir" "$out_dir"
trap - EXIT
echo "visual audit complete: $out_dir ($actual_rows rows)" >&2
