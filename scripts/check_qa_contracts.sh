#!/usr/bin/env bash
set -euo pipefail

root_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
policy_matrix="$root_dir/docs/architecture/effective-policy-matrix.json"
capability_matrix="$root_dir/docs/architecture/persistence-capability-matrix.json"
config_source="$root_dir/src/config.rs"
persist_source="$root_dir/src/qa/persist.rs"

fail() {
  printf 'qa contracts: ERROR: %s\n' "$*" >&2
  exit 1
}

command -v jq >/dev/null 2>&1 || fail "jq is required"
[[ -f "$policy_matrix" ]] || fail "missing effective-policy matrix"
[[ -f "$capability_matrix" ]] || fail "missing persistence-capability matrix"
jq empty "$policy_matrix" "$capability_matrix" || fail "contract JSON is invalid"

policy_fields=$(jq -r '.fields[].field' "$policy_matrix")
[[ -n "$policy_fields" ]] || fail "policy matrix has no fields"
code_policy_fields=$(awk '
  /^pub const EFFECTIVE_POLICY_CONTRACT_FIELDS/ { in_fields = 1; next }
  in_fields && index($0, "];") { exit }
  in_fields { print }
' "$config_source" | sed -n 's/.*"\([^"]*\)".*/\1/p')
[[ -n "$code_policy_fields" ]] || fail "code-owned policy field list is missing"
if [[ "$(printf '%s\n' "$policy_fields" | sort)" != "$(printf '%s\n' "$code_policy_fields" | sort)" ]]; then
  fail "effective-policy matrix fields drift from src/config.rs"
fi

capability_targets=$(jq -r '.targets[].target' "$capability_matrix")
code_capability_targets=$(grep -E '^pub\(crate\) const PERSISTENCE_CAPABILITY_TARGETS' "$persist_source" \
  | grep -o '"[^"]*"' | tr -d '"')
[[ -n "$code_capability_targets" ]] || fail "code-owned capability target list is missing"
if [[ "$(printf '%s\n' "$capability_targets" | sort)" != "$(printf '%s\n' "$code_capability_targets" | sort)" ]]; then
  fail "persistence capability matrix targets drift from src/qa/persist.rs"
fi
grep -q 'RENAME_NOREPLACE' "$persist_source" || fail "Linux no-replace primitive is not code-owned"
grep -q 'RENAME_EXCL' "$persist_source" || fail "macOS no-replace primitive is not code-owned"
if awk '/pub\(crate\) fn publish_directory_with_ops/,/^}/' "$persist_source" | grep -q 'fs::rename'; then
  fail "final directory publication contains an ordinary rename fallback"
fi

for point in stage-created writing ready before-publish after-publish; do
  grep -q '"'"$point"'"' "$root_dir/tests/qa_persistence.rs" \
    || fail "subprocess pause inventory is missing $point"
done
grep -q 'two_process_visual_packet_claims_have_one_winner' "$root_dir/tests/qa_persistence.rs" \
  || fail "independent-process directory race inventory is missing"

canonical=$(mktemp "${TMPDIR:-/tmp}/termiflow-qa-contracts.XXXXXX")
trap 'rm -f -- "$canonical"' EXIT
jq -S -c '{policy_schema:.policy_schema,fields:[.fields[].field],capabilities:(input.targets|map({target,status,primitive}))}' \
  "$policy_matrix" "$capability_matrix" > "$canonical"
digest=$(shasum -a 256 "$canonical" | awk '{print $1}')
printf 'qa contracts: PASS\n'
printf '  policy_fields=%s capability_targets=%s\n' "$(printf '%s\n' "$policy_fields" | wc -l | tr -d ' ')" "$(printf '%s\n' "$capability_targets" | wc -l | tr -d ' ')"
printf '  drift_receipt_sha256=%s\n' "$digest"
