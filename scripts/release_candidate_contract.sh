#!/usr/bin/env bash
set -euo pipefail

# Local, network-free contract test for the candidate boundary. It constructs a
# clean detached worktree from the current in-flight patch, so the shared
# checkout remains untouched and the test exercises the same clean-tree rule CI
# will enforce.

root="$(git rev-parse --show-toplevel)"
test_root="$(mktemp -d "${TMPDIR:-/tmp}/termiflow-release-contract.XXXXXX")"
cleanup() {
  git -C "$root" worktree remove --force "$test_root/repo" >/dev/null 2>&1 || true
  rm -rf -- "$test_root"
}
trap cleanup EXIT

patch_file="$test_root/in-flight.patch"
patch_index="$test_root/index"
GIT_INDEX_FILE="$patch_index" git -C "$root" read-tree HEAD
GIT_INDEX_FILE="$patch_index" git -C "$root" add -A -- .
GIT_INDEX_FILE="$patch_index" git -C "$root" diff --cached --binary --full-index HEAD -- . > "$patch_file"
git -C "$root" worktree add --detach "$test_root/repo" HEAD >/dev/null
if [[ -s "$patch_file" ]]; then
  git -C "$test_root/repo" apply --whitespace=nowarn "$patch_file"
fi
cp "$root/scripts/release_candidate.sh" "$test_root/repo/scripts/release_candidate.sh"
cp "$root/scripts/package_contract.sh" "$test_root/repo/scripts/package_contract.sh"
chmod +x "$test_root/repo/scripts/release_candidate.sh" "$test_root/repo/scripts/package_contract.sh"
git -C "$test_root/repo" add -A
git -C "$test_root/repo" -c user.name='TermiFlow contract test' -c user.email='contract-test@localhost' commit --allow-empty --no-verify -m 'candidate contract fixture' >/dev/null

candidate="$test_root/candidate.json"
repo="$test_root/repo"
scripts="$repo/scripts"
cd "$repo"
release_version="$(cargo metadata --locked --no-deps --format-version 1 | jq -r '.packages[0].version')"

"$scripts/release_candidate.sh" prepare --tag "v$release_version" --boundary "$candidate"
"$scripts/release_candidate.sh" verify --tag "v$release_version" --boundary "$candidate"

mutated_candidate="$test_root/mutated-candidate.json"
jq '.candidate.sha=("0" * 40)' "$candidate" > "$mutated_candidate"
if "$scripts/release_candidate.sh" verify --tag v0.2.2 --boundary "$mutated_candidate"; then
  printf '%s\n' 'candidate contract: source mismatch was accepted' >&2
  exit 1
fi

archives="$test_root/archives"
fragments="$test_root/fragments"
mkdir -p "$archives" "$fragments"
for target in aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu; do
  archive="$archives/termiflow-v${release_version}-${target}.tar.gz"
  printf '%s\n' "fixture archive for $target" > "$archive"
  "$scripts/release_candidate.sh" record \
    --boundary "$candidate" \
    --target "$target" \
    --archive "$archive" \
    --fragment "$fragments/fragment-${target}.json"
done

mutated_archive="$archives/termiflow-v${release_version}-aarch64-apple-darwin.tar.gz"
printf '%s\n' 'tampered archive' > "$mutated_archive"
if "$scripts/release_candidate.sh" finalize \
  --boundary "$candidate" \
  --package "$repo/$(jq -r '.package.path' "$candidate")" \
  --fragments-dir "$fragments" \
  --archives-dir "$archives"; then
  printf '%s\n' 'candidate contract: archive mismatch was accepted' >&2
  exit 1
fi

printf '%s\n' 'fixture archive for aarch64-apple-darwin' > "$mutated_archive"
"$scripts/release_candidate.sh" finalize \
  --boundary "$candidate" \
  --package "$repo/$(jq -r '.package.path' "$candidate")" \
  --fragments-dir "$fragments" \
  --archives-dir "$archives"
env -u GITHUB_REF_NAME "$scripts/release_preflight.sh" --boundary "$candidate" --publish

printf '%s\n' 'release candidate contract: PASS'
