#!/usr/bin/env bash
set -euo pipefail

# Release-boundary preflight. It is intentionally fail-closed: an unset
# candidate, unresolved version, dirty candidate worktree, or incomplete claim
# scan cannot be treated as publishable.

root="$(git rev-parse --show-toplevel)"
boundary="${TERMIFLOW_RELEASE_BOUNDARY:-${root}/.maestro/state/context/2026-08-02-termiflow-release-boundary.json}"
mode="candidate"

while (($# > 0)); do
  case "$1" in
    --boundary)
      (($# >= 2)) || { printf 'usage: %s [--boundary PATH] [--publish]\n' "$0" >&2; exit 2; }
      boundary="$2"
      shift 2
      ;;
    --publish)
      mode="publish"
      shift
      ;;
    --help|-h)
      printf 'usage: %s [--boundary PATH] [--publish]\n' "$0"
      exit 0
      ;;
    *)
      printf 'usage: %s [--boundary PATH] [--publish]\n' "$0" >&2
      exit 2
      ;;
  esac
done

failures=0
fail() {
  printf 'release preflight: %s\n' "$1" >&2
  failures=$((failures + 1))
}

if [[ ! -f "$boundary" ]]; then
  fail "boundary artifact not found: $boundary"
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  fail "jq is required to evaluate the release boundary"
  exit 1
fi

if ! jq empty "$boundary" >/dev/null 2>&1; then
  fail "boundary artifact is not valid JSON"
fi
if [[ "$(jq -r '.schema // ""' "$boundary")" != "termiflow.release_candidate.v1" ]]; then
  fail "boundary schema is not termiflow.release_candidate.v1"
fi
if [[ "$(jq -r '.version // 0' "$boundary")" != "1" ]]; then
  fail "boundary schema version is not 1"
fi

state="$(jq -r '.state // "missing"' "$boundary")"
candidate_sha="$(jq -r '.candidate.sha // ""' "$boundary")"
candidate_tree_sha="$(jq -r '.candidate.tree_sha // ""' "$boundary")"
candidate_tag="$(jq -r '.candidate.tag // ""' "$boundary")"
candidate_version="$(jq -r '.candidate.version // ""' "$boundary")"
candidate_lock_sha="$(jq -r '.candidate.lock_sha256 // ""' "$boundary")"
candidate_toolchain_sha="$(jq -r '.candidate.toolchain_sha256 // ""' "$boundary")"
candidate_toolchain_channel="$(jq -r '.candidate.toolchain_channel // ""' "$boundary")"
version_decision="$(jq -r '.candidate.version_decision // "missing"' "$boundary")"
publication_allowed="$(jq -r '.publication_allowed // false' "$boundary")"
claim_scan="$(jq -r '.ai_claim_scan.status // "missing"' "$boundary")"
current_sha="$(git rev-parse HEAD)"

sha256_file() {
  shasum -a 256 "$1" | awk '{print $1}'
}

if [[ -z "$candidate_sha" || "$candidate_sha" == "null" ]]; then
  fail "candidate SHA is not fixed"
elif [[ "$current_sha" != "$candidate_sha" ]]; then
  fail "HEAD $current_sha does not match candidate SHA $candidate_sha"
fi

if [[ "$candidate_tree_sha" != "$(git rev-parse HEAD^{tree})" ]]; then
  fail "HEAD tree does not match candidate tree SHA"
fi

if [[ ! "$candidate_tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ || "${candidate_tag#v}" != "$candidate_version" ]]; then
  fail "candidate tag/version binding is invalid"
fi
if [[ -n "${GITHUB_REF_NAME:-}" && "$GITHUB_REF_NAME" != "$candidate_tag" ]]; then
  fail "GITHUB_REF_NAME $GITHUB_REF_NAME does not match candidate tag $candidate_tag"
fi

if [[ ! -f Cargo.lock || "$candidate_lock_sha" != "$(sha256_file Cargo.lock)" ]]; then
  fail "Cargo.lock does not match candidate digest"
fi
if [[ ! -f rust-toolchain.toml || "$candidate_toolchain_sha" != "$(sha256_file rust-toolchain.toml)" ]]; then
  fail "rust-toolchain.toml does not match candidate digest"
fi
stable_channel="$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\([^"]*\)".*$/\1/p' rust-toolchain.toml | head -n 1)"
if [[ "$candidate_toolchain_channel" != "$stable_channel" ]]; then
  fail "toolchain channel does not match candidate"
fi

if [[ "$(jq -r '.package.extracted_all_targets // "missing"' "$boundary")" != "passed" ]]; then
  fail "extracted package contract is not passed"
fi
target_count="$(jq '.candidate.targets // [] | length' "$boundary")"
if [[ "$target_count" -lt 1 ]]; then
  fail "candidate target matrix is empty"
fi

if [[ "$version_decision" == "deferred_until_renderer_compatibility_report" || "$version_decision" == "missing" ]]; then
  fail "release version decision is unresolved"
fi

if [[ "$claim_scan" != "passed" ]]; then
  fail "AI/public-claims scan is not passed"
fi

if [[ -n "$(git status --porcelain=v1)" ]]; then
  fail "candidate worktree is dirty; use explicit path-scoped staging and commit"
fi

if [[ "$state" == "publication-ready" ]]; then
  archive_count="$(jq '.archives // [] | length' "$boundary")"
  if [[ "$archive_count" != "$target_count" ]]; then
    fail "publication-ready boundary has $archive_count archives for $target_count targets"
  fi
  if ! jq -e 'all(.archives[]; (.target | type == "string") and (.sha256 | test("^[0-9a-f]{64}$")) and (.bytes | type == "number"))' "$boundary" >/dev/null; then
    fail "publication-ready archive records are incomplete"
  fi
fi

if [[ "$mode" == "publish" ]]; then
  if [[ "$state" != "publication-ready" ]]; then
    fail "boundary state is $state, expected publication-ready"
  fi
  if [[ "$publication_allowed" != "true" ]]; then
    fail "publication_allowed is not true"
  fi
else
  if [[ "$state" != "candidate-ready" && "$state" != "publication-ready" ]]; then
    fail "boundary state is $state, expected candidate-ready or publication-ready"
  fi
fi

if (( failures > 0 )); then
  printf 'release preflight: %d blocking condition(s)\n' "$failures" >&2
  exit 1
fi

printf 'release preflight: %s gate passed for %s\n' "$mode" "$current_sha"
