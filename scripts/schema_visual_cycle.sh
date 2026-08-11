#!/usr/bin/env bash
set -euo pipefail

# Compose one schema-bound candidate/packet/holdout boundary. This command
# intentionally stops before perceptual decisions and golden approval: those
# are separate one-frame and explicit-intent actions.

root_dir=$(CDPATH="" cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root_dir"

usage() {
  cat >&2 <<'USAGE'
Usage: scripts/schema_visual_cycle.sh --queue ID \
  --manifest PATH --golden-report PATH --packet DIR \
  --holdout-packet DIR --holdout-receipt PATH [options]

Materialize and validate one canonical Mermaid fixture queue, check its golden
candidates, build strict main and evaluator-owned holdout packets, and emit a
summary that points to the separate one-frame review and approval commands.

Required output paths must not already exist. This command never appends visual
decisions, updates goldens, changes the quality baseline, or approves a cycle.

Options:
  --spec PATH              Fixture spec (default: tests/fixtures/fixture_spec.json)
  --queue ID               Named schema queue (required)
  --manifest PATH          New materialized queue manifest (required)
  --golden-report PATH     New golden check report (required)
  --packet DIR             New main visual packet directory (required)
  --holdout-packet DIR     New evaluator holdout packet directory (required)
  --holdout-receipt PATH   New evaluator holdout receipt (required)
  --binary PATH            Prebuilt termiflow binary
  --display-profile ID     Display profile (default: terminal-grid-v1)
  --timeout-seconds N      Per-row timeout (default: 60)
  --summary PATH           Also write the new JSON summary to PATH
  -h, --help               Show this help
USAGE
}

die() {
  echo "schema visual cycle: $*" >&2
  exit 1
}

spec_path="tests/fixtures/fixture_spec.json"
queue_id=""
manifest_path=""
golden_report_path=""
packet_path=""
holdout_packet_path=""
holdout_receipt_path=""
binary_path=""
display_profile="terminal-grid-v1"
timeout_seconds="60"
summary_path=""

while (($# > 0)); do
  case "$1" in
    --spec)
      (($# >= 2)) || die "--spec requires a path"
      spec_path="$2"
      shift 2
      ;;
    --queue)
      (($# >= 2)) || die "--queue requires an ID"
      queue_id="$2"
      shift 2
      ;;
    --manifest)
      (($# >= 2)) || die "--manifest requires a path"
      manifest_path="$2"
      shift 2
      ;;
    --golden-report)
      (($# >= 2)) || die "--golden-report requires a path"
      golden_report_path="$2"
      shift 2
      ;;
    --packet)
      (($# >= 2)) || die "--packet requires a directory"
      packet_path="$2"
      shift 2
      ;;
    --holdout-packet)
      (($# >= 2)) || die "--holdout-packet requires a directory"
      holdout_packet_path="$2"
      shift 2
      ;;
    --holdout-receipt)
      (($# >= 2)) || die "--holdout-receipt requires a path"
      holdout_receipt_path="$2"
      shift 2
      ;;
    --binary)
      (($# >= 2)) || die "--binary requires a path"
      binary_path="$2"
      shift 2
      ;;
    --display-profile)
      (($# >= 2)) || die "--display-profile requires an ID"
      display_profile="$2"
      shift 2
      ;;
    --timeout-seconds)
      (($# >= 2)) || die "--timeout-seconds requires a number"
      timeout_seconds="$2"
      shift 2
      ;;
    --summary)
      (($# >= 2)) || die "--summary requires a path"
      summary_path="$2"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      usage
      die "unknown argument: $1"
      ;;
  esac
done

[[ -n "$queue_id" ]] || die "--queue is required"
[[ -n "$manifest_path" ]] || die "--manifest is required"
[[ -n "$golden_report_path" ]] || die "--golden-report is required"
[[ -n "$packet_path" ]] || die "--packet is required"
[[ -n "$holdout_packet_path" ]] || die "--holdout-packet is required"
[[ -n "$holdout_receipt_path" ]] || die "--holdout-receipt is required"
command -v jq >/dev/null 2>&1 || die "jq is required"

resolve_file() {
  local candidate="$1"
  local label="$2"
  if [[ "$candidate" != /* ]]; then
    candidate="$root_dir/$candidate"
  fi
  [[ -f "$candidate" && ! -L "$candidate" ]] || die "$label is not a regular file: $candidate"
  local parent
  parent="$(CDPATH="" cd -P -- "$(dirname -- "$candidate")" && pwd -P)" \
    || die "cannot resolve $label parent: $candidate"
  printf '%s/%s\n' "$parent" "$(basename -- "$candidate")"
}

resolve_new_file() {
  local candidate="$1"
  local label="$2"
  if [[ "$candidate" != /* ]]; then
    candidate="$root_dir/$candidate"
  fi
  [[ ! -e "$candidate" && ! -L "$candidate" ]] || die "$label already exists: $candidate"
  local parent
  parent="$(CDPATH="" cd -P -- "$(dirname -- "$candidate")" && pwd -P)" \
    || die "$label parent does not exist: $candidate"
  printf '%s/%s\n' "$parent" "$(basename -- "$candidate")"
}

resolve_new_directory() {
  local candidate="$1"
  local label="$2"
  if [[ "$candidate" != /* ]]; then
    candidate="$root_dir/$candidate"
  fi
  [[ ! -e "$candidate" && ! -L "$candidate" ]] || die "$label already exists: $candidate"
  local parent
  parent="$(CDPATH="" cd -P -- "$(dirname -- "$candidate")" && pwd -P)" \
    || die "$label parent does not exist: $candidate"
  printf '%s/%s\n' "$parent" "$(basename -- "$candidate")"
}

spec_file="$(resolve_file "$spec_path" spec)"
manifest_file="$(resolve_new_file "$manifest_path" manifest)"
golden_report_file="$(resolve_new_file "$golden_report_path" golden-report)"
packet_dir="$(resolve_new_directory "$packet_path" packet)"
holdout_packet_dir="$(resolve_new_directory "$holdout_packet_path" holdout-packet)"
holdout_receipt_file="$(resolve_new_file "$holdout_receipt_path" holdout-receipt)"
if [[ -n "$binary_path" ]]; then
  binary_file="$(resolve_file "$binary_path" binary)"
else
  binary_file=""
fi
if [[ -n "$summary_path" ]]; then
  summary_file="$(resolve_new_file "$summary_path" summary)"
else
  summary_file=""
fi

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/termiflow-schema-cycle.XXXXXX")"
cleanup() {
  rm -rf -- "$tmp_dir"
}
trap cleanup EXIT

run_logged() {
  local label="$1"
  local stdout_log="$tmp_dir/$label.stdout"
  local stderr_log="$tmp_dir/$label.stderr"
  shift
  if ! "$@" >"$stdout_log" 2>"$stderr_log"; then
    cat "$stderr_log" >&2
    cat "$stdout_log" >&2
    die "$label failed"
  fi
}

qa=(cargo run --locked --quiet --features qa --bin termiflow-qa --)
schema_cmd=("${qa[@]}" schema --spec "$spec_file" --queue "$queue_id" --emit-manifest "$manifest_file")
run_logged schema "${schema_cmd[@]}"

golden_cmd=("${qa[@]}" golden --manifest "$manifest_file" --check --report "$golden_report_file")
if [[ -n "$binary_file" ]]; then
  golden_cmd+=(--binary "$binary_file")
fi
run_logged golden "${golden_cmd[@]}"

audit_cmd=(scripts/visual_audit.sh --schema-manifest "$manifest_file" --out "$packet_dir" --display-profile "$display_profile" --timeout-seconds "$timeout_seconds")
if [[ -n "$binary_file" ]]; then
  audit_cmd+=(--binary "$binary_file")
fi
run_logged visual-audit "${audit_cmd[@]}"
run_logged visual-validate scripts/visual_validate.sh --packet "$packet_dir" --queue-manifest "$manifest_file" --strict-quality

holdout_cmd=("${qa[@]}" holdout --spec "$spec_file" --queue "$queue_id" --out "$holdout_packet_dir" --receipt "$holdout_receipt_file" --display-profile "$display_profile" --timeout-seconds "$timeout_seconds")
if [[ -n "$binary_file" ]]; then
  holdout_cmd+=(--binary "$binary_file")
fi
run_logged holdout "${holdout_cmd[@]}"
run_logged holdout-validate scripts/visual_validate.sh --packet "$holdout_packet_dir" --queue-manifest "$manifest_file" --holdout --strict-quality

hash_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d ' ' -f 1
  else
    shasum -a 256 "$1" | cut -d ' ' -f 1
  fi
}

manifest_sha256="$(hash_file "$manifest_file")"
golden_report_sha256="$(hash_file "$golden_report_file")"
holdout_receipt_sha256="$(hash_file "$holdout_receipt_file")"
packet_identity_sha256="$(hash_file "$packet_dir/identity.json")"
packet_complete_sha256="$(hash_file "$packet_dir/COMPLETE.json")"
holdout_identity_sha256="$(hash_file "$holdout_packet_dir/identity.json")"
holdout_complete_sha256="$(hash_file "$holdout_packet_dir/COMPLETE.json")"

summary_json="$(jq -n \
  --arg queue_id "$(jq -er '.queue_id | strings' "$manifest_file")" \
  --arg queue_sha256 "$(jq -er '.queue_sha256 | strings' "$manifest_file")" \
  --arg spec_sha256 "$(jq -er '.spec_sha256 | strings' "$manifest_file")" \
  --arg manifest_path "$manifest_file" \
  --arg manifest_sha256 "$manifest_sha256" \
  --arg golden_report_path "$golden_report_file" \
  --arg golden_report_sha256 "$golden_report_sha256" \
  --arg packet_path "$packet_dir" \
  --arg packet_identity_sha256 "$packet_identity_sha256" \
  --arg packet_complete_sha256 "$packet_complete_sha256" \
  --arg holdout_packet_path "$holdout_packet_dir" \
  --arg holdout_identity_sha256 "$holdout_identity_sha256" \
  --arg holdout_complete_sha256 "$holdout_complete_sha256" \
  --arg holdout_receipt_path "$holdout_receipt_file" \
  --arg holdout_receipt_sha256 "$holdout_receipt_sha256" \
  --arg review_command "scripts/review_visual_packet.sh --packet $packet_dir --decisions <DECISIONS.jsonl> --next" \
  --arg cycle_command "scripts/visual_cycle.sh --packet $packet_dir --decisions <DECISIONS.jsonl> --queue-manifest $manifest_file --holdout-receipt $holdout_receipt_file --holdout-decisions <HOLDOUT-DECISIONS.jsonl> --record <CYCLE.json> --output <CYCLE-RECEIPT.json>" \
  --arg approval_command "scripts/regenerate_golden.sh --approve --intent <EXPLICIT-RENDERING-INTENT>" \
  '{schema:"termiflow.schema_visual_cycle.v1",status:"ready_for_perceptual_review",queue_id:$queue_id,queue_sha256:$queue_sha256,spec_sha256:$spec_sha256,manifest:{path:$manifest_path,sha256:$manifest_sha256},golden:{status:"current",report_path:$golden_report_path,report_sha256:$golden_report_sha256,approval:"separate_explicit_command"},packet:{path:$packet_path,identity_sha256:$packet_identity_sha256,complete_sha256:$packet_complete_sha256},holdout:{packet_path:$holdout_packet_path,identity_sha256:$holdout_identity_sha256,complete_sha256:$holdout_complete_sha256,receipt_path:$holdout_receipt_path,receipt_sha256:$holdout_receipt_sha256},review_required:true,commands:{review_one_frame:$review_command,close_cycle:$cycle_command,approve_golden:$approval_command}}')"

summary_schema="$root_dir/tests/fixtures/schema_visual_cycle_summary.schema.json"
[[ -f "$summary_schema" && ! -L "$summary_schema" ]] || die "summary schema is missing"
jq -e '.["$id"] == "termiflow.schema_visual_cycle.v1" and .type == "object"' "$summary_schema" >/dev/null \
  || die "summary schema is invalid"
jq -e '
  .schema == "termiflow.schema_visual_cycle.v1" and
  .status == "ready_for_perceptual_review" and
  (.queue_id | type == "string" and length > 0) and
  (.queue_sha256 | test("^[0-9a-f]{64}$")) and
  (.spec_sha256 | test("^[0-9a-f]{64}$")) and
  (.manifest.sha256 | test("^[0-9a-f]{64}$")) and
  (.golden.status == "current" and .golden.approval == "separate_explicit_command") and
  (.packet.identity_sha256 | test("^[0-9a-f]{64}$")) and
  (.holdout.receipt_sha256 | test("^[0-9a-f]{64}$")) and
  .review_required == true
' <<<"$summary_json" >/dev/null || die "summary failed its schema-bound shape check"

if [[ -n "$summary_file" ]]; then
  printf '%s\n' "$summary_json" > "$summary_file"
fi
printf '%s\n' "$summary_json"
