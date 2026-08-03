#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: scripts/publish_receipt.sh STAGED_FILE FINAL_FILE

Claim an absent same-directory final receipt with a hard link. The staged file
is removed only after the claim succeeds; overwrite-prone mv/rename fallback is
intentionally unsupported.
USAGE
}

die() {
  echo "receipt publication: $*" >&2
  exit 1
}

(($# == 2)) || {
  usage
  exit 2
}

staged="$1"
final="$2"
[[ -f "$staged" && ! -L "$staged" ]] || die "staged path is not a regular file: $staged"
[[ ! -e "$final" && ! -L "$final" ]] || die "final receipt already exists: $final"

staged_dir="$(cd -- "$(dirname -- "$staged")" && pwd -P)"
final_dir="$(cd -- "$(dirname -- "$final")" && pwd -P)"
[[ "$staged_dir" == "$final_dir" ]] || die "staged and final paths must share a directory"

ln "$staged" "$final" || die "same-directory hard-link claim failed; final receipt was not overwritten"
if ! rm -- "$staged"; then
  die "receipt was claimed but staged cleanup failed; remove only the staged path after inspection: $staged"
fi

echo "receipt published: $final"
