#!/usr/bin/env bash
set -euo pipefail

root_dir=$(CDPATH="" cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root_dir"

usage() {
  cat >&2 <<'USAGE'
Usage: scripts/visual_cycle.sh --packet DIR --decisions FILE \
  --record FILE --output FILE

Validate a strict visual packet and its complete one-frame review ledger, then
emit one hash-bound termiflow.visual_cycle.v1 receipt from an explicit cycle
record. This command never appends decisions, updates goldens, or changes
baselines. The output path must not already exist.
USAGE
}

die() {
  echo "visual cycle: $*" >&2
  exit 1
}

packet_path=""
decisions_path=""
record_path=""
output_path=""

while (($# > 0)); do
  case "$1" in
    --packet)
      (($# >= 2)) || die "--packet requires a directory"
      packet_path="$2"
      shift 2
      ;;
    --decisions)
      (($# >= 2)) || die "--decisions requires a file"
      decisions_path="$2"
      shift 2
      ;;
    --record)
      (($# >= 2)) || die "--record requires a file"
      record_path="$2"
      shift 2
      ;;
    --output)
      (($# >= 2)) || die "--output requires a file"
      output_path="$2"
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

[[ -n "$packet_path" ]] || die "--packet is required"
[[ -n "$decisions_path" ]] || die "--decisions is required"
[[ -n "$record_path" ]] || die "--record is required"
[[ -n "$output_path" ]] || die "--output is required"
command -v jq >/dev/null 2>&1 || die "jq is required"

hash_stream() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum | cut -d ' ' -f 1
  else
    shasum -a 256 | cut -d ' ' -f 1
  fi
}

hash_file() {
  hash_stream < "$1"
}

resolve_file() {
  local candidate="$1"
  local label="$2"
  if [[ "$candidate" != /* ]]; then
    candidate="$root_dir/$candidate"
  fi
  [[ -f "$candidate" ]] || die "$label does not exist: $candidate"
  [[ ! -L "$candidate" ]] || die "$label must not be a symlink: $candidate"
  local parent
  parent="$(CDPATH="" cd -P -- "$(dirname -- "$candidate")" && pwd -P)" \
    || die "cannot resolve $label parent: $candidate"
  printf '%s/%s\n' "$parent" "$(basename -- "$candidate")"
}

resolve_directory() {
  local candidate="$1"
  local label="$2"
  if [[ "$candidate" != /* ]]; then
    candidate="$root_dir/$candidate"
  fi
  [[ -d "$candidate" ]] || die "$label does not exist: $candidate"
  [[ ! -L "$candidate" ]] || die "$label must not be a symlink: $candidate"
  CDPATH="" cd -P -- "$candidate" && pwd -P
}

resolve_output() {
  local candidate="$1"
  if [[ "$candidate" != /* ]]; then
    candidate="$root_dir/$candidate"
  fi
  [[ ! -e "$candidate" && ! -L "$candidate" ]] \
    || die "output already exists: $candidate"
  local parent
  parent="$(CDPATH="" cd -P -- "$(dirname -- "$candidate")" && pwd -P)" \
    || die "output parent does not exist: $candidate"
  printf '%s/%s\n' "$parent" "$(basename -- "$candidate")"
}

packet_dir="$(resolve_directory "$packet_path" packet)"
decisions_file="$(resolve_file "$decisions_path" decisions)"
record_file="$(resolve_file "$record_path" cycle-record)"
output_file="$(resolve_output "$output_path")"

for required in COMPLETE.json manifest.jsonl identity.json PACKET.sha256; do
  [[ -f "$packet_dir/$required" && ! -L "$packet_dir/$required" ]] \
    || die "packet is missing regular $required"
done

schema_file="$root_dir/tests/fixtures/visual_cycle_record.schema.json"
[[ -f "$schema_file" && ! -L "$schema_file" ]] \
  || die "cycle record schema is missing"
jq -e '."$id" == "termiflow.visual_cycle_record.v1" and .type == "object"' \
  "$schema_file" >/dev/null \
  || die "cycle record schema is invalid"

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/termiflow-visual-cycle.XXXXXX")"
trap 'rm -rf -- "$tmp_dir"' EXIT

if ! "$root_dir/scripts/visual_validate.sh" \
  --packet "$packet_dir" --strict-quality \
  >"$tmp_dir/visual.stdout" 2>"$tmp_dir/visual.stderr"; then
  cat "$tmp_dir/visual.stderr" >&2
  cat "$tmp_dir/visual.stdout" >&2
  die "strict visual validation failed"
fi

if ! "$root_dir/scripts/review_visual_packet.sh" \
  --packet "$packet_dir" --decisions "$decisions_file" --validate \
  >"$tmp_dir/review.stdout" 2>"$tmp_dir/review.stderr"; then
  cat "$tmp_dir/review.stderr" >&2
  cat "$tmp_dir/review.stdout" >&2
  die "perceptual review coverage failed"
fi

reviewed="$(jq -er '.reviewed | numbers' "$tmp_dir/review.stdout")" \
  || die "review validator did not emit coverage JSON"
review_schema="$(jq -er '.schema | strings' "$tmp_dir/review.stdout")" \
  || die "review validator did not emit a schema"
[[ "$review_schema" == "termiflow.visual_review.coverage.v1" ]] \
  || die "unexpected review coverage schema: $review_schema"
expected_rows="$(jq -s '[.[] | select(.classification != "expected_error")] | length' \
  "$packet_dir/manifest.jsonl")" \
  || die "cannot count reviewable manifest rows"
(( reviewed == expected_rows )) \
  || die "review coverage is incomplete: $reviewed of $expected_rows"

complete_sha256="$(hash_file "$packet_dir/COMPLETE.json")"
manifest_sha256="$(hash_file "$packet_dir/manifest.jsonl")"
identity_sha256="$(hash_file "$packet_dir/identity.json")"
packet_checksum_sha256="$(hash_file "$packet_dir/PACKET.sha256")"
decisions_sha256="$(hash_file "$decisions_file")"
record_sha256="$(hash_file "$record_file")"
schema_sha256="$(hash_file "$schema_file")"

source_commit="$(jq -er '.source_commit | strings' "$packet_dir/identity.json")" \
  || die "packet identity has no source_commit"
worktree_dirty="$(jq -r '.worktree_dirty | select(type == "boolean")' \
  "$packet_dir/identity.json")" \
  || die "cannot read packet identity worktree_dirty flag"
[[ "$worktree_dirty" == true || "$worktree_dirty" == false ]] \
  || die "packet identity has no boolean worktree_dirty flag"
effective_sha256="$(jq -er '.provenance.effective_sha256 | strings' \
  "$packet_dir/identity.json")" \
  || die "packet identity has no effective source identity"

lesson_path="$(jq -er '.lesson.path | strings | select(length > 0)' "$record_file")" \
  || die "cycle record has no lesson.path"
[[ "$lesson_path" != /* ]] || die "lesson.path must be repository-relative"
case "$lesson_path" in
  ""|.|..|./*|../*|*/../*|*/..|*/./*)
    die "lesson.path contains an unsafe traversal: $lesson_path"
    ;;
esac
lesson_file="$(resolve_file "$lesson_path" lesson)"
[[ "$lesson_file" == "$root_dir/"* ]] \
  || die "lesson.path escapes the repository: $lesson_path"
lesson_sha256="$(hash_file "$lesson_file")"

if ! jq -e \
  --arg complete "$complete_sha256" \
  --arg manifest "$manifest_sha256" \
  --arg identity "$identity_sha256" \
  --arg checksum "$packet_checksum_sha256" \
  --arg decisions "$decisions_sha256" \
  --argjson reviewed "$reviewed" \
  --argjson expected "$expected_rows" \
  --arg lesson_sha "$lesson_sha256" '
    def nonempty_string:
      type == "string" and length > 0;
    def nonempty_strings:
      type == "array" and length > 0 and all(.[]; nonempty_string);
    def has_only($allowed):
      (keys | sort) == ($allowed | sort);
    def enum($values; $value):
      ($values | index($value)) != null;
    has_only(["schema", "cycle_id", "disposition", "packet", "review",
      "observation", "observation_details", "owner_layer", "hypothesis",
      "expected_observation_if_true", "falsifier", "next_command", "fix",
      "homologs", "holdout", "lesson", "golden_approval"]) and
    (.packet | has_only(["complete_sha256", "manifest_sha256",
      "identity_sha256", "packet_checksum_sha256"])) and
    (.review | has_only(["decisions_sha256", "reviewed", "expected_rows"])) and
    (.fix | has_only(["status", "summary"])) and
    (.holdout | has_only(["status", "result"])) and
    (.lesson | has_only(["kind", "path", "sha256", "rule", "next_review"])) and
    (.golden_approval | has_only(["status"])) and
    (.schema == "termiflow.visual_cycle_record.v1") and
    (.cycle_id | nonempty_string) and
    (enum(["fixed", "hold", "falsified"]; .disposition)) and
    (.packet | type == "object") and
    (.packet.complete_sha256 == $complete) and
    (.packet.manifest_sha256 == $manifest) and
    (.packet.identity_sha256 == $identity) and
    (.packet.packet_checksum_sha256 == $checksum) and
    (.review | type == "object") and
    (.review.decisions_sha256 == $decisions) and
    (.review.reviewed == $reviewed) and
    (.review.expected_rows == $expected) and
    (.observation | nonempty_string) and
    (.observation_details | type == "array" and length > 0 and all(.[];
      type == "object" and has_only(["row", "column", "glyph", "note"]) and
      (.row | type == "number") and (.column | type == "number") and
      (.row >= 0) and (.column >= 0) and
      (.row == (.row | floor)) and (.column == (.column | floor)) and
      (.glyph | type == "string") and (.note | nonempty_string))) and
    (.owner_layer | nonempty_string) and
    (.hypothesis | nonempty_string) and
    (.expected_observation_if_true | nonempty_string) and
    (.falsifier | nonempty_string) and
    (.next_command | nonempty_string) and
    (.fix | type == "object") and
    (enum(["localized", "not_applicable", "none"]; .fix.status)) and
    (.fix.summary | nonempty_string) and
    (.homologs | nonempty_strings) and
    (.holdout | type == "object") and
    (enum(["passed", "failed", "blocked", "not_run"]; .holdout.status)) and
    (.holdout.result | nonempty_string) and
    (.lesson | type == "object") and
    (enum(["renderer", "fixture", "oracle", "taxonomy", "skill", "script", "review"]; .lesson.kind)) and
    (.lesson.path | nonempty_string) and
    (.lesson.sha256 == $lesson_sha) and
    (.lesson.rule | nonempty_string) and
    (.lesson.next_review | nonempty_string) and
    (.golden_approval | type == "object") and
    (enum(["not_requested", "separate_review"]; .golden_approval.status)) and
    ((.disposition != "fixed") or
      (.fix.status == "localized" and .holdout.status == "passed"))
  ' "$record_file" >/dev/null; then
  die "cycle record failed its fail-closed contract"
fi

cycle_id="$(jq -er '.cycle_id | strings' "$record_file")"
disposition="$(jq -er '.disposition | strings' "$record_file")"
lesson_kind="$(jq -er '.lesson.kind | strings' "$record_file")"
lesson_rule="$(jq -er '.lesson.rule | strings' "$record_file")"
lesson_next_review="$(jq -er '.lesson.next_review | strings' "$record_file")"
holdout_status="$(jq -er '.holdout.status | strings' "$record_file")"
fix_status="$(jq -er '.fix.status | strings' "$record_file")"

jq -S -n \
  --arg schema "termiflow.visual_cycle.v1" \
  --arg cycle_id "$cycle_id" \
  --arg source_commit "$source_commit" \
  --arg effective_sha256 "$effective_sha256" \
  --argjson worktree_dirty "$worktree_dirty" \
  --arg complete_sha256 "$complete_sha256" \
  --arg manifest_sha256 "$manifest_sha256" \
  --arg identity_sha256 "$identity_sha256" \
  --arg packet_checksum_sha256 "$packet_checksum_sha256" \
  --arg decisions_sha256 "$decisions_sha256" \
  --arg record_sha256 "$record_sha256" \
  --arg schema_sha256 "$schema_sha256" \
  --argjson reviewed "$reviewed" \
  --argjson expected_rows "$expected_rows" \
  --arg disposition "$disposition" \
  --arg fix_status "$fix_status" \
  --arg holdout_status "$holdout_status" \
  --arg lesson_kind "$lesson_kind" \
  --arg lesson_path "$lesson_path" \
  --arg lesson_sha256 "$lesson_sha256" \
  --arg lesson_rule "$lesson_rule" \
  --arg lesson_next_review "$lesson_next_review" \
  '{
    schema: $schema,
    status: "accepted",
    cycle_id: $cycle_id,
    source: {
      source_commit: $source_commit,
      effective_sha256: $effective_sha256,
      worktree_dirty: $worktree_dirty
    },
    packet: {
      complete_sha256: $complete_sha256,
      manifest_sha256: $manifest_sha256,
      identity_sha256: $identity_sha256,
      packet_checksum_sha256: $packet_checksum_sha256
    },
    review: {
      decisions_sha256: $decisions_sha256,
      reviewed: $reviewed,
      expected_rows: $expected_rows,
      strict_quality_validated: true
    },
    record: {
      sha256: $record_sha256,
      schema_sha256: $schema_sha256,
      disposition: $disposition,
      fix_status: $fix_status,
      holdout_status: $holdout_status
    },
    lesson: {
      kind: $lesson_kind,
      path: $lesson_path,
      sha256: $lesson_sha256,
      rule: $lesson_rule,
      next_review: $lesson_next_review
    },
    golden_approval: "separate_review_required"
  }' > "$tmp_dir/receipt.json"

mv -- "$tmp_dir/receipt.json" "$output_file"
echo "visual cycle receipt: $output_file"
