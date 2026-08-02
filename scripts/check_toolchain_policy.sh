#!/usr/bin/env bash
# Fail-closed repository alignment checks for Rust, CI, and release policy.
# This is intentionally offline: it does not claim that external registries or
# upstream releases are still current after the dated currency audit.
set -euo pipefail

root_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root_dir"

ci_workflow="$root_dir/.github/workflows/ci.yml"
release_workflow="$root_dir/.github/workflows/release.yml"
toolchain_file="$root_dir/rust-toolchain.toml"
cargo_file="$root_dir/Cargo.toml"

fail() {
  printf 'toolchain policy: ERROR: %s\n' "$*" >&2
  exit 1
}

for required_file in "$ci_workflow" "$release_workflow" "$toolchain_file" "$cargo_file"; do
  [[ -f "$required_file" ]] || fail "required file is missing: ${required_file#$root_dir/}"
done

stable_channel=$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\([^"]*\)".*$/\1/p' "$toolchain_file" | head -n 1)
[[ -n "$stable_channel" ]] || fail "rust-toolchain.toml has no exact channel"
[[ "$stable_channel" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "stable channel must be an exact x.y.z release, observed '$stable_channel'"

components=$(sed -n 's/^[[:space:]]*components[[:space:]]*=[[:space:]]*\[\(.*\)\].*$/\1/p' "$toolchain_file" | head -n 1)
[[ "$components" == *rustfmt* ]] || fail "rust-toolchain.toml must include rustfmt"
[[ "$components" == *clippy* ]] || fail "rust-toolchain.toml must include clippy"

msrv=$(sed -n 's/^[[:space:]]*rust-version[[:space:]]*=[[:space:]]*"\([^"]*\)".*$/\1/p' "$cargo_file" | head -n 1)
[[ -n "$msrv" ]] || fail "Cargo.toml has no exact rust-version"
[[ "$msrv" =~ ^[0-9]+\.[0-9]+(\.[0-9]+)?$ ]] || fail "Cargo.toml rust-version must be numeric, observed '$msrv'"

action_records=$(sed -n \
  -e 's/^[[:space:]]*-[[:space:]]*uses:[[:space:]]*\([^@[:space:]]*\)@\([^[:space:]]*\).*$/\1\t\2/p' \
  -e 's/^[[:space:]]*uses:[[:space:]]*\([^@[:space:]]*\)@\([^[:space:]]*\).*$/\1\t\2/p' \
  "$ci_workflow" "$release_workflow")
[[ -n "$action_records" ]] || fail "no versioned workflow actions were found"

while IFS=$'\t' read -r action ref; do
  [[ -n "$action" && -n "$ref" ]] || fail "workflow action reference is malformed"
  if [[ ! "$ref" =~ ^[0-9]+\.[0-9]+(\.[0-9]+)?$ && ! "$ref" =~ ^v?[0-9]+\.[0-9]+\.[0-9]+$ && ! "$ref" =~ ^[0-9a-fA-F]{40}$ ]]; then
    fail "workflow action $action uses floating or unsupported ref '$ref'"
  fi
done <<< "$action_records"

action_conflicts=$(printf '%s\n' "$action_records" | awk -F '\t' '
  {
    if ($1 == "dtolnay/rust-toolchain") {
      next
    }
    if ($1 in refs && refs[$1] != $2) {
      print $1 ": " refs[$1] " vs " $2
    }
    refs[$1] = $2
  }
')
[[ -z "$action_conflicts" ]] || fail "workflow action pins conflict: $action_conflicts"

stable_toolchain_refs=0
msrv_toolchain_refs=0
while IFS=$'\t' read -r action ref; do
  [[ "$action" == "dtolnay/rust-toolchain" ]] || continue
  if [[ "$ref" == "$stable_channel" ]]; then
    stable_toolchain_refs=$((stable_toolchain_refs + 1))
  elif [[ "$ref" == "$msrv" ]]; then
    msrv_toolchain_refs=$((msrv_toolchain_refs + 1))
  else
    fail "dtolnay/rust-toolchain ref '$ref' does not match stable '$stable_channel' or MSRV '$msrv'"
  fi
done <<< "$action_records"
(( stable_toolchain_refs > 0 )) || fail "no workflow uses the declared stable toolchain '$stable_channel'"
(( msrv_toolchain_refs > 0 )) || fail "no workflow uses the declared MSRV '$msrv'"

tool_records=$(grep -hE 'cargo install[[:space:]]+(cargo-deny|cargo-audit|cross)([[:space:]]|$)' "$ci_workflow" "$release_workflow" || true)
[[ -n "$tool_records" ]] || fail "no cargo-deny, cargo-audit, or cross install pins were found"

for tool in cargo-deny cargo-audit cross; do
  tool_lines=$(printf '%s\n' "$tool_records" | grep -E "cargo install[[:space:]]+$tool([[:space:]]|$)" || true)
  [[ -n "$tool_lines" ]] || fail "missing exact install pin for $tool"
  tool_line_count=$(printf '%s\n' "$tool_lines" | wc -l | tr -d ' ')
  tool_versions=$(printf '%s\n' "$tool_lines" | sed -n 's/.*--version[[:space:]][[:space:]]*\([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\).*/\1/p')
  [[ -n "$tool_versions" ]] || fail "$tool install must include an exact semver --version"
  tool_version_count=$(printf '%s\n' "$tool_versions" | wc -l | tr -d ' ')
  [[ "$tool_version_count" -eq "$tool_line_count" ]] || fail "$tool install pins must each include an exact semver --version"
  while IFS= read -r version; do
    [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "$tool has a non-semver version '$version'"
  done <<< "$tool_versions"
  locked_line_count=$(printf '%s\n' "$tool_lines" | grep -c -- '--locked' || true)
  [[ "$locked_line_count" -eq "$tool_line_count" ]] || fail "$tool install pins must each include --locked"
  if [[ $(printf '%s\n' "$tool_versions" | sort -u | wc -l | tr -d ' ') -ne 1 ]]; then
    fail "$tool has conflicting install versions: $(printf '%s\n' "$tool_versions" | sort -u | tr '\n' ' ')"
  fi
done

while IFS= read -r line; do
  [[ -n "$line" ]] || continue
  [[ "$line" =~ name:[[:space:]]*cargo ]] && continue
  [[ "$line" =~ name:[[:space:]]*cross ]] && continue
  [[ "$line" =~ ^[[:space:]]*# ]] && continue
  [[ "$line" =~ (^|[[:space:]])(cargo|cross)[[:space:]]+(clippy|test|bench|doc|build|package|publish|install|deny|metadata|tree|update|check)([[:space:]]|$) ]] || continue
  [[ "$line" == *--locked* ]] && continue
  [[ "$line" =~ (^|[[:space:]])cargo[[:space:]]+fmt([[:space:]]|$) ]] && continue
  [[ "$line" =~ (^|[[:space:]])cargo[[:space:]]+audit([[:space:]]|$) ]] && continue
  fail "graph/build/test/doc/package/publish command lacks --locked: $line"
done < <(grep -hE '(^|[[:space:]])(cargo|cross)[[:space:]]+(clippy|test|bench|doc|build|package|publish|install|deny|metadata|tree|update|check|fmt|audit)([[:space:]]|$)' "$ci_workflow" "$release_workflow" || true)

printf 'toolchain policy: PASS\n'
printf '  stable=%s msrv=%s components=rustfmt,clippy\n' "$stable_channel" "$msrv"
printf '  toolchain workflow refs: stable=%s msrv=%s\n' "$stable_toolchain_refs" "$msrv_toolchain_refs"
printf '  action pins:\n%s\n' "$(printf '%s\n' "$action_records" | sort -u | sed 's/\t/@/')"
printf '  cargo tool pins:\n%s\n' "$(printf '%s\n' "$tool_records" | sed 's/^[[:space:]]*run:[[:space:]]*//' | sort -u)"
printf '  scope=internal-alignment; external-latest-audit=separate\n'
