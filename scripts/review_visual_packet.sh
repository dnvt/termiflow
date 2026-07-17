#!/usr/bin/env bash
# Interactive terminal reviewer for an immutable visual-audit packet.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/review_visual_packet.sh --packet DIR --decisions FILE [options]

Options:
  --packet DIR        Immutable visual-audit run (contains manifest.jsonl)
  --decisions FILE    JSONL destination for review decisions
  --fixture NAME      Review only one fixture base name
  --style STYLE       Review only one style (ascii or unicode)
  --mode MODE         Review only one mode (default or optimized)
  --reviewer NAME     Reviewer identity (default: $USER)
  --help              Show this help

Keys at each frame: p=pass, f=fail, w=watch, s=skip, q=quit.
For fail/watch, enter a concise evidence note. Existing case IDs are skipped.
EOF
}

packet="" decisions="" fixture="" style="" mode="" reviewer="${USER:-reviewer}"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --packet) packet="${2:?}"; shift 2 ;;
    --decisions) decisions="${2:?}"; shift 2 ;;
    --fixture) fixture="${2:?}"; shift 2 ;;
    --style) style="${2:?}"; shift 2 ;;
    --mode) mode="${2:?}"; shift 2 ;;
    --reviewer) reviewer="${2:?}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

[[ -n "$packet" && -n "$decisions" ]] || { usage >&2; exit 2; }
manifest="$packet/manifest.jsonl"
[[ -f "$manifest" ]] || { echo "missing manifest: $manifest" >&2; exit 2; }
[[ -r /dev/tty && -w /dev/tty ]] || {
  echo "interactive review requires an attached terminal (/dev/tty is unavailable)" >&2
  exit 2
}
mkdir -p "$(dirname "$decisions")"
touch "$decisions"
queue="$(mktemp "${TMPDIR:-/tmp}/termiflow-review-queue.XXXXXX")"
trap 'rm -f "$queue"' EXIT

python3 - "$manifest" "$decisions" "$fixture" "$style" "$mode" "$packet" <<'PY' > "$queue"
import json, sys
manifest, decisions, fixture, style, mode, packet = sys.argv[1:]
seen = {json.loads(line)["case_id"] for line in open(decisions) if line.strip()}
for line in open(manifest):
    row = json.loads(line)
    if row["classification"] == "expected_error" or row["case_id"] in seen:
        continue
    if fixture and row["fixture"] != fixture: continue
    if style and row["style"] != style: continue
    if mode and row["mode"] != mode: continue
    # S1 manifests retain the staging path used before atomic publication.
    # The review packet is portable by rebasing each artifact to its final root.
    frame = f'{packet}/frames/{row["fixture"]}.{row["style"]}.{row["mode"]}.txt'
    print("\t".join([row["case_id"], row["fixture"], row["style"], row["mode"], frame]))
PY
while IFS=$'\t' read -r case_id case_fixture case_style case_mode frame; do
  clear
  printf 'Fixture: %s | %s | %s\n\n' "$case_fixture" "$case_style" "$case_mode"
  sed -n '1,240p' "$frame"
  printf '\nDecision [p/f/w/s/q]: '
  read -r decision </dev/tty
  case "$decision" in
    q) exit 0 ;;
    p) verdict="pass"; note="" ;;
    f) verdict="fail"; printf 'Evidence note: '; read -r note </dev/tty ;;
    w) verdict="watch"; printf 'Evidence note: '; read -r note </dev/tty ;;
    s) continue ;;
    *) echo "invalid decision; frame not recorded" >&2; continue ;;
  esac
  python3 - "$decisions" "$case_id" "$case_fixture" "$case_style" "$case_mode" "$frame" "$verdict" "$note" "$reviewer" <<'PY'
import datetime, json, sys
path, case_id, fixture, style, mode, frame, verdict, note, reviewer = sys.argv[1:]
record = {"schema":"termiflow.visual_review.decision.v1", "case_id":case_id,
          "fixture":fixture, "style":style, "mode":mode, "frame":frame,
          "verdict":verdict, "evidence":note, "reviewer":reviewer,
          "timestamp":datetime.datetime.now(datetime.timezone.utc).isoformat()}
with open(path, "a", encoding="utf-8") as out:
    out.write(json.dumps(record, sort_keys=True) + "\n")
PY
done < "$queue"
