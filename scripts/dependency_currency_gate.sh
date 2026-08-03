#!/usr/bin/env bash
set -euo pipefail

# Dated, fail-closed Cargo/Rust currency evidence. This command observes the
# reachable graph and records non-root Cargo lock metadata; it never edits
# Cargo.toml or Cargo.lock.

usage() {
  cat >&2 <<'USAGE'
Usage: scripts/dependency_currency_gate.sh [--receipt PATH] [--allow-dirty]

Run compatible and aggressive cargo-outdated checks, Cargo graph/audit checks,
and emit a termiflow.dependency_currency_receipt.v1 JSON receipt. A dirty
worktree is always recorded as blocked; --allow-dirty is for exploratory
evidence only. The command never updates the manifest or lockfile.
USAGE
}

receipt_path="target/dependency-currency/termiflow-dependency-currency-receipt.json"
allow_dirty=false

while (($# > 0)); do
  case "$1" in
    --receipt)
      (($# >= 2)) || { echo "--receipt requires a path" >&2; exit 2; }
      receipt_path="$2"
      shift 2
      ;;
    --allow-dirty)
      allow_dirty=true
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      usage
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

root_dir="$(git rev-parse --show-toplevel)"
cd "$root_dir"

if [[ "$receipt_path" != /* ]]; then
  receipt_path="$root_dir/$receipt_path"
fi
receipt_dir="$(dirname -- "$receipt_path")"
mkdir -p "$receipt_dir"
[[ ! -L "$receipt_path" ]] || {
  echo "dependency currency: receipt path must not be a symlink: $receipt_path" >&2
  exit 2
}
[[ ! -e "$receipt_path" ]] || {
  echo "dependency currency: receipt already exists; refusing to rerun into an authoritative path: $receipt_path" >&2
  exit 2
}

receipt_stage=""
cleanup_receipt_stage() {
  if [[ -n "${receipt_stage:-}" && -e "$receipt_stage" ]]; then
    rm -f -- "$receipt_stage"
  fi
}
trap cleanup_receipt_stage EXIT

for command_name in jq cargo rustc rustup cargo-outdated cargo-deny cargo-audit; do
  command -v "$command_name" >/dev/null 2>&1 || {
    echo "dependency currency: required command is missing: $command_name" >&2
    exit 2
  }
done

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

hash_path_list() {
  local path
  while IFS= read -r -d '' path; do
    [[ -f "$path" && ! -L "$path" ]] || {
      echo "dependency currency: source path is not a regular file: $path" >&2
      return 1
    }
    printf '%s\0' "$path"
    git hash-object -- "$path"
    printf '\0'
  done < "$1" | hash_stream
}

capture_source() {
  local output="$1"
  local dirty_text dirty_json untracked_sha tracked_sha untracked_paths_json

  git ls-files --others --exclude-standard -z > "$receipt_dir/untracked-paths.log"
  git ls-files -z > "$receipt_dir/tracked-paths.log"
  dirty_text="$(git status --porcelain=v1 --untracked-files=all)"
  if [[ -n "$dirty_text" ]]; then
    dirty_json=true
  else
    dirty_json=false
  fi
  tracked_sha="$(hash_path_list "$receipt_dir/tracked-paths.log")"
  untracked_sha="$(hash_path_list "$receipt_dir/untracked-paths.log")"
  untracked_paths_json="$(jq -R -s 'split("\u0000") | map(select(length > 0))' < "$receipt_dir/untracked-paths.log")"

  local unsigned
  unsigned="$(jq -cn \
    --arg commit "$(git rev-parse HEAD)" \
    --arg tracked "$tracked_sha" \
    --arg diff "$(git diff --binary HEAD -- | hash_stream)" \
    --arg staged "$(git diff --cached --binary -- | hash_stream)" \
    --arg untracked "$untracked_sha" \
    --argjson paths "$untracked_paths_json" \
    --argjson dirty "$dirty_json" \
    '{commit:$commit, worktree_dirty:$dirty, tracked_worktree_sha256:$tracked,
      tracked_diff_sha256:$diff, staged_diff_sha256:$staged,
      untracked_files_sha256:$untracked, untracked_paths:$paths}')"
  jq -c --arg identity "$(printf '%s' "$unsigned" | hash_stream)" \
    '. + {identity_sha256:$identity}' <<< "$unsigned" > "$output"
}

run_logged() {
  local stdout_log="$1"
  local stderr_log="$2"
  shift 2
  "$@" > "$stdout_log" 2> "$stderr_log"
  local result=$?
  return "$result"
}

command_record() {
  local status="$1"
  local exit_code="$2"
  local stdout_log="$3"
  local stderr_log="$4"
  local argv_json="$5"
  jq -cn \
    --arg status "$status" \
    --argjson exit_code "$exit_code" \
    --arg stdout_sha256 "$(hash_file "$stdout_log")" \
    --arg stderr_sha256 "$(hash_file "$stderr_log")" \
    --arg stdout_path "$(basename "$stdout_log")" \
    --arg stderr_path "$(basename "$stderr_log")" \
    --argjson argv "$argv_json" \
    '{status:$status, exit_code:$exit_code, argv:$argv,
      stdout_sha256:$stdout_sha256, stderr_sha256:$stderr_sha256,
      stdout_path:$stdout_path, stderr_path:$stderr_path}'
}

outdated_candidate_count() {
  jq -r 'if type == "array" then
    ([.[] | (.dependencies // []) | length] | add // 0)
  elif type == "object" then
    ((.dependencies // []) | length)
  else 0 end' "$1"
}

classify_outdated() {
  local output="$1"
  local exit_code="$2"
  if ! jq -e 'type == "object" or type == "array"' "$output" >/dev/null 2>&1; then
    printf '%s\n' error
    return
  fi
  local candidates
  candidates="$(outdated_candidate_count "$output")"
  if (( candidates > 0 )); then
    printf '%s\n' candidate
  elif (( exit_code == 0 )); then
    printf '%s\n' current
  else
    printf '%s\n' error
  fi
}

classify_pass() {
  local exit_code="$1"
  if (( exit_code == 0 )); then
    printf '%s\n' pass
  else
    printf '%s\n' error
  fi
}

capture_source "$receipt_dir/source-before.json"
worktree_dirty="$(jq -r '.worktree_dirty' "$receipt_dir/source-before.json")"
if [[ "$worktree_dirty" == true && "$allow_dirty" != true ]]; then
  echo "dependency currency: worktree is dirty; pass --allow-dirty only for exploratory evidence" >&2
  exit 2
fi

rustc_log="$receipt_dir/rustc-vv.txt"
cargo_log="$receipt_dir/cargo-version.txt"
rustup_log="$receipt_dir/rustup-active-toolchain.txt"
rustup_check_log="$receipt_dir/rustup-check.txt"
rustup_check_stderr="$receipt_dir/rustup-check.stderr.log"
outdated_root_stdout="$receipt_dir/outdated-root.json"
outdated_root_stderr="$receipt_dir/outdated-root.stderr.log"
outdated_features_stdout="$receipt_dir/outdated-features.json"
outdated_features_stderr="$receipt_dir/outdated-features.stderr.log"
update_stdout="$receipt_dir/cargo-update.stdout.log"
update_stderr="$receipt_dir/cargo-update.stderr.log"
metadata_log="$receipt_dir/cargo-metadata.json"
metadata_stderr="$receipt_dir/cargo-metadata.stderr.log"
tree_log="$receipt_dir/cargo-tree.txt"
tree_stderr="$receipt_dir/cargo-tree.stderr.log"
duplicates_log="$receipt_dir/cargo-duplicates.txt"
duplicates_stderr="$receipt_dir/cargo-duplicates.stderr.log"
deny_stdout="$receipt_dir/cargo-deny.stdout.log"
deny_stderr="$receipt_dir/cargo-deny.stderr.log"
audit_stdout="$receipt_dir/cargo-audit.stdout.log"
audit_stderr="$receipt_dir/cargo-audit.stderr.log"

rustc -Vv > "$rustc_log"
cargo --version > "$cargo_log"
rustup show active-toolchain > "$rustup_log"

set +e
run_logged "$rustup_check_log" "$rustup_check_stderr" rustup check
rustup_check_exit=$?
run_logged "$outdated_root_stdout" "$outdated_root_stderr" \
  cargo outdated --workspace --aggressive --format json --color never --exit-code 1
outdated_root_exit=$?
run_logged "$outdated_features_stdout" "$outdated_features_stderr" \
  cargo outdated --workspace --features 'golden qa' --aggressive --format json --color never --exit-code 1
outdated_features_exit=$?
run_logged "$update_stdout" "$update_stderr" \
  cargo update --workspace --dry-run --locked --verbose
update_exit=$?
run_logged "$metadata_log" "$metadata_stderr" \
  cargo metadata --locked --all-features --format-version 1
metadata_exit=$?
run_logged "$tree_log" "$tree_stderr" \
  cargo tree --workspace --locked --all-features --target all --edges normal,build,dev --format '{p}'
tree_exit=$?
run_logged "$duplicates_log" "$duplicates_stderr" \
  cargo tree --workspace --locked --all-features --target all --edges normal,build,dev --duplicates --format '{p}'
duplicates_exit=$?
run_logged "$deny_stdout" "$deny_stderr" \
  cargo deny --locked check advisories bans licenses sources
deny_exit=$?
run_logged "$audit_stdout" "$audit_stderr" \
  cargo audit --deny warnings
audit_exit=$?
set -e

root_outdated_status="$(classify_outdated "$outdated_root_stdout" "$outdated_root_exit")"
features_outdated_status="$(classify_outdated "$outdated_features_stdout" "$outdated_features_exit")"
update_status="$(classify_pass "$update_exit")"
metadata_status="$(classify_pass "$metadata_exit")"
tree_status="$(classify_pass "$tree_exit")"
duplicates_status="$(classify_pass "$duplicates_exit")"
deny_status="$(classify_pass "$deny_exit")"
audit_status="$(classify_pass "$audit_exit")"

stable_channel="$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\([^"]*\)".*$/\1/p' rust-toolchain.toml | head -n 1)"
rustup_latest="$(sed -n \
  -e '1s/.* -> \([0-9][0-9.]*\).*/\1/p' \
  -e '1s/.*up to date: \([0-9][0-9.]*\).*/\1/p' \
  "$rustup_check_log" | head -n 1)"
rustup_check_command_status=error
rust_currency_status=error
if [[ -n "$rustup_latest" && "$rustup_latest" == "$stable_channel" ]]; then
  rust_currency_status=pass
  if (( rustup_check_exit == 0 )); then
    rustup_check_command_status=pass
  else
    # rustup check returns 100 when any installed channel has an update; the
    # stable observation remains authoritative for this stable-only policy.
    rustup_check_command_status=stable-current
  fi
fi
if [[ "$rust_currency_status" != pass ]]; then
  rust_currency_status=error
fi

metadata_valid=false
if [[ "$metadata_status" == pass ]] && jq -e '.packages and .resolve' "$metadata_log" >/dev/null 2>&1; then
  metadata_valid=true
fi

direct_requirements_json='[]'
if [[ "$metadata_valid" == true ]]; then
  direct_requirements_json="$(jq --arg root_package termiflow --arg default_kind normal '[.packages[] | select(.name == $root_package) | .dependencies[]
    | select(.source != null)
    | {name, req, kind:(.kind // $default_kind), optional:(.optional // false), target:(.target // null)}]
    ' "$metadata_log")"
fi

reachable_lines="$receipt_dir/reachable-packages.tsv"
reachable_json_file="$receipt_dir/reachable-packages.json"
if [[ "$tree_status" == pass ]]; then
  sed -E 's/^[^[:alnum:]_.-]*//' "$tree_log" \
    | awk '$1 ~ /^[[:alnum:]_.-]+$/ && $2 ~ /^v[0-9]/ { print $1 "\t" substr($2, 2) }' \
    | sort -u > "$reachable_lines"
  jq -R -s 'split("\n") | map(select(length > 0) | split("\t")
    | {name:.[0], version:.[1]})' "$reachable_lines" > "$reachable_json_file"
else
  printf '[]\n' > "$reachable_json_file"
fi

non_root_lock_json='[]'
if [[ "$metadata_valid" == true ]]; then
  non_root_lock_json="$(jq --slurpfile reachable "$reachable_json_file" \
    '[.packages[] as $package
      | select(([$reachable[0][] | select(.name == $package.name and .version == $package.version)] | length) == 0)
      | {name:$package.name, version:$package.version, source:($package.source // "path")}]' \
    "$metadata_log")"
fi

duplicate_lines_json='[]'
if [[ "$duplicates_status" == pass ]]; then
  duplicate_lines_json="$(jq -R -s 'split("\n") | map(select(length > 0))' "$duplicates_log")"
fi

root_candidate_count="$(outdated_candidate_count "$outdated_root_stdout" 2>/dev/null || printf '0')"
features_candidate_count="$(outdated_candidate_count "$outdated_features_stdout" 2>/dev/null || printf '0')"
non_root_count="$(jq 'length' <<< "$non_root_lock_json")"

findings_file="$receipt_dir/findings.json"
printf '[]\n' > "$findings_file"
add_finding() {
  local message="$1"
  jq --arg message "$message" '. + [$message]' "$findings_file" > "$findings_file.next"
  mv -- "$findings_file.next" "$findings_file"
}

[[ "$worktree_dirty" == true ]] && add_finding "worktree is dirty; this receipt is exploratory and blocked"
[[ "$rust_currency_status" != pass ]] && add_finding "rustup latest-stable observation does not match the pinned exact stable channel"
(( root_candidate_count > 0 )) && add_finding "cargo outdated found direct or reachable latest candidates"
(( features_candidate_count > 0 )) && add_finding "cargo outdated found feature-activated latest candidates"
[[ "$root_outdated_status" == error ]] && add_finding "root cargo outdated command or JSON result was unavailable"
[[ "$features_outdated_status" == error ]] && add_finding "feature cargo outdated command or JSON result was unavailable"
[[ "$update_status" != pass ]] && add_finding "Cargo compatible update probe failed"
[[ "$metadata_status" != pass ]] && add_finding "Cargo metadata failed"
[[ "$tree_status" != pass ]] && add_finding "reachable Cargo tree failed"
[[ "$duplicates_status" != pass ]] && add_finding "duplicate-version Cargo tree failed"
[[ "$deny_status" != pass ]] && add_finding "cargo deny failed"
[[ "$audit_status" != pass ]] && add_finding "cargo audit failed"
(( non_root_count > 0 )) && add_finding "Cargo reports $non_root_count lock records outside the activated reachable tree; these are recorded, not manually deleted"

capture_source "$receipt_dir/source-after.json"
source_before_identity="$(jq -r '.identity_sha256' "$receipt_dir/source-before.json")"
source_after_identity="$(jq -r '.identity_sha256' "$receipt_dir/source-after.json")"
[[ "$source_before_identity" == "$source_after_identity" ]] \
  || add_finding "source identity changed while the currency probe ran"

overall_status=pass
if [[ "$worktree_dirty" == true || "$source_before_identity" != "$source_after_identity" ||
  "$rust_currency_status" != pass ||
  "$root_outdated_status" != current || "$features_outdated_status" != current ||
  "$update_status" != pass || "$metadata_status" != pass || "$tree_status" != pass ||
  "$duplicates_status" != pass || "$deny_status" != pass || "$audit_status" != pass ]]; then
  overall_status=blocked
fi

manifest_sha256="$(hash_file Cargo.toml)"
lock_sha256="$(hash_file Cargo.lock)"
toolchain_sha256="$(hash_file rust-toolchain.toml)"
rustc_release="$(sed -n 's/^release: //p' "$rustc_log" | head -n 1)"
rustc_host="$(sed -n 's/^host: //p' "$rustc_log" | head -n 1)"
msrv="$(sed -n 's/^[[:space:]]*rust-version[[:space:]]*=[[:space:]]*"\([^"]*\)".*$/\1/p' Cargo.toml | head -n 1)"
observed_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

root_command_json='["cargo","outdated","--workspace","--aggressive","--format","json","--color","never","--exit-code","1"]'
features_command_json='["cargo","outdated","--workspace","--features","golden qa","--aggressive","--format","json","--color","never","--exit-code","1"]'
update_command_json='["cargo","update","--workspace","--dry-run","--locked","--verbose"]'
metadata_command_json='["cargo","metadata","--locked","--all-features","--format-version","1"]'
tree_command_json='["cargo","tree","--workspace","--locked","--all-features","--target","all","--edges","normal,build,dev","--format","{p}"]'
duplicates_command_json='["cargo","tree","--workspace","--locked","--all-features","--target","all","--edges","normal,build,dev","--duplicates","--format","{p}"]'
deny_command_json='["cargo","deny","--locked","check","advisories","bans","licenses","sources"]'
audit_command_json='["cargo","audit","--deny","warnings"]'
rustup_check_command_json='["rustup","check"]'

receipt_stage="$(mktemp "${receipt_path}.tmp.XXXXXX")"
jq -n \
  --arg schema "termiflow.dependency_currency_receipt.v1" \
  --arg observed_at "$observed_at" \
  --argjson source "$(cat "$receipt_dir/source-after.json")" \
  --arg allow_dirty "$allow_dirty" \
  --arg manifest_sha256 "$manifest_sha256" \
  --arg lock_sha256 "$lock_sha256" \
  --arg toolchain_sha256 "$toolchain_sha256" \
  --arg rustc_release "$rustc_release" \
  --arg rustc_host "$rustc_host" \
  --arg msrv "$msrv" \
  --arg stable_channel "$stable_channel" \
  --arg rustc_verbose_sha256 "$(hash_file "$rustc_log")" \
  --arg cargo_version_sha256 "$(hash_file "$cargo_log")" \
  --arg rustup_active_toolchain "$(tr '\n' ' ' < "$rustup_log" | sed 's/[[:space:]]*$//')" \
  --arg rustup_latest "$rustup_latest" \
  --arg rust_currency_status "$rust_currency_status" \
  --argjson rustup_check_command "$(command_record "$rustup_check_command_status" "$rustup_check_exit" "$rustup_check_log" "$rustup_check_stderr" "$rustup_check_command_json")" \
  --argjson root_command "$(command_record "$root_outdated_status" "$outdated_root_exit" "$outdated_root_stdout" "$outdated_root_stderr" "$root_command_json")" \
  --argjson features_command "$(command_record "$features_outdated_status" "$outdated_features_exit" "$outdated_features_stdout" "$outdated_features_stderr" "$features_command_json")" \
  --argjson update_command "$(command_record "$update_status" "$update_exit" "$update_stdout" "$update_stderr" "$update_command_json")" \
  --argjson metadata_command "$(command_record "$metadata_status" "$metadata_exit" "$metadata_log" "$metadata_stderr" "$metadata_command_json")" \
  --argjson tree_command "$(command_record "$tree_status" "$tree_exit" "$tree_log" "$tree_stderr" "$tree_command_json")" \
  --argjson duplicates_command "$(command_record "$duplicates_status" "$duplicates_exit" "$duplicates_log" "$duplicates_stderr" "$duplicates_command_json")" \
  --argjson deny_command "$(command_record "$deny_status" "$deny_exit" "$deny_stdout" "$deny_stderr" "$deny_command_json")" \
  --argjson audit_command "$(command_record "$audit_status" "$audit_exit" "$audit_stdout" "$audit_stderr" "$audit_command_json")" \
  --argjson direct_requirements "$direct_requirements_json" \
  --argjson reachable_packages "$(cat "$reachable_json_file")" \
  --argjson non_root_lock_metadata "$non_root_lock_json" \
  --arg duplicate_tree_sha256 "$(hash_file "$duplicates_log")" \
  --argjson duplicate_tree_lines "$duplicate_lines_json" \
  --argjson findings "$(cat "$findings_file")" \
  --argjson root_candidate_count "$root_candidate_count" \
  --argjson features_candidate_count "$features_candidate_count" \
  --arg overall_status "$overall_status" \
  '{schema:$schema, observed_at:$observed_at,
    source:($source + {allow_dirty:($allow_dirty == "true"),
      manifest_sha256:$manifest_sha256, lock_sha256:$lock_sha256,
      toolchain_sha256:$toolchain_sha256}),
    toolchain:{rustc_release:$rustc_release, host:$rustc_host, msrv:$msrv,
      rustc_verbose_sha256:$rustc_verbose_sha256,
      cargo_version_sha256:$cargo_version_sha256,
      rustup_active_toolchain:$rustup_active_toolchain},
    commands:{rustup_check:$rustup_check_command,
      cargo_outdated_root:$root_command,
      cargo_outdated_features:$features_command, cargo_update:$update_command,
      cargo_metadata:$metadata_command, cargo_tree:$tree_command,
      cargo_duplicates:$duplicates_command, cargo_deny:$deny_command,
      cargo_audit:$audit_command},
    latest:{rust_status:$rust_currency_status, pinned_stable:$stable_channel,
      observed_stable:$rustup_latest, root_status:$root_command.status,
      feature_status:$features_command.status,
      root_candidate_count:$root_candidate_count,
      feature_candidate_count:$features_candidate_count,
      major_versions_checked:true},
    graph:{metadata_sha256:$metadata_command.stdout_sha256,
      reachable_tree_sha256:$tree_command.stdout_sha256,
      reachable_package_count:($reachable_packages | length),
      non_root_lock_metadata_count:($non_root_lock_metadata | length)},
    direct_requirements:$direct_requirements,
    reachable_packages:$reachable_packages,
    duplicate_tree:{sha256:$duplicate_tree_sha256, lines:$duplicate_tree_lines},
    non_root_lock_metadata:$non_root_lock_metadata,
    findings:$findings,
    status:$overall_status}' > "$receipt_stage"

"$root_dir/scripts/publish_receipt.sh" "$receipt_stage" "$receipt_path"
receipt_stage=""

if ! jq -e '
  .schema == "termiflow.dependency_currency_receipt.v1" and
  (.source.commit | strings | length > 0) and
  (.source.identity_sha256 | strings | test("^[0-9a-f]{64}$")) and
  (.toolchain.rustc_release | strings | length > 0) and
  (.latest.major_versions_checked == true) and
  (.direct_requirements | type == "array") and
  (.reachable_packages | type == "array") and
  (.duplicate_tree.lines | type == "array") and
  (.findings | type == "array") and
  (.status == "pass" or .status == "blocked")
' "$receipt_path" >/dev/null; then
  echo "dependency currency: emitted receipt failed structural validation: $receipt_path" >&2
  exit 1
fi

echo "dependency currency receipt: $receipt_path"
echo "dependency currency status: $overall_status"
if [[ "$overall_status" != pass ]]; then
  exit 1
fi
