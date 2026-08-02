#!/usr/bin/env bash
set -euo pipefail

# Release-boundary preflight. It is intentionally fail-closed: an unset
# candidate, unresolved version, dirty candidate worktree, or incomplete claim
# scan cannot be treated as publishable.

root="$(git rev-parse --show-toplevel)"
boundary="${TERMIFLOW_RELEASE_BOUNDARY:-${root}/.maestro/state/context/2026-08-02-termiflow-release-boundary.json}"
mode="candidate"

if [[ "${1:-}" == "--publish" ]]; then
  mode="publish"
elif [[ -n "${1:-}" ]]; then
  printf 'usage: %s [--publish]\n' "$0" >&2
  exit 2
fi

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

state="$(jq -r '.state // "missing"' "$boundary")"
candidate_sha="$(jq -r '.candidate.sha // ""' "$boundary")"
version_decision="$(jq -r '.candidate.version_decision // "missing"' "$boundary")"
publication_allowed="$(jq -r '.publication_allowed // false' "$boundary")"
claim_scan="$(jq -r '.ai_claim_scan.status // "missing"' "$boundary")"
current_sha="$(git rev-parse HEAD)"

if [[ -z "$candidate_sha" || "$candidate_sha" == "null" ]]; then
  fail "candidate SHA is not fixed"
elif [[ "$current_sha" != "$candidate_sha" ]]; then
  fail "HEAD $current_sha does not match candidate SHA $candidate_sha"
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
