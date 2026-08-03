#!/usr/bin/env bash
set -euo pipefail

# Validate the published source package as a consumer would see it. The
# repository deliberately keeps maintainer QA tests and their large fixture
# corpus out of the crate; repository CI runs those tests from the checkout.

usage() {
  cat >&2 <<'USAGE'
Usage: scripts/package_contract.sh [--output PATH]

Build and inspect the Cargo package, extract it into a private temporary
directory, and run the package's consumer-facing all-targets test surface.
USAGE
}

output=""
while (($# > 0)); do
  case "$1" in
    --output)
      (($# >= 2)) || { usage; exit 2; }
      output="$2"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

root="$(git rev-parse --show-toplevel)"
cd "$root"
version="$(cargo metadata --locked --no-deps --format-version 1 | jq -r '.packages[0].version')"
[[ -n "$version" && "$version" != "null" ]] || {
  echo "package contract: Cargo version is missing" >&2
  exit 1
}

cargo package --locked --allow-dirty
crate="$(find "$root/target" -type f -path '*/package/termiflow-*.crate' -print | sort | tail -n 1)"
[[ -n "$crate" && -f "$crate" ]] || {
  echo "package contract: packaged crate was not found" >&2
  exit 1
}

if tar -tzf "$crate" | grep -Eq '(^|/)(tests|benches|scripts)/|(^|/)src/qa/|(^|/)src/bin/termiflow_qa\.rs$'; then
  echo "package contract: maintainer-only sources leaked into the crate" >&2
  exit 1
fi

package_root="$(mktemp -d "${TMPDIR:-/tmp}/termiflow-package.XXXXXX")"
cleanup() {
  rm -rf -- "$package_root"
}
trap cleanup EXIT
tar -xzf "$crate" -C "$package_root"
extracted="$package_root/termiflow-$version"
[[ -f "$extracted/Cargo.toml" ]] || {
  echo "package contract: extracted Cargo.toml is missing" >&2
  exit 1
}

CARGO_TARGET_DIR="$package_root/target" cargo test --locked --all-targets --no-default-features \
  --manifest-path "$extracted/Cargo.toml"

crate_sha256="$(shasum -a 256 "$crate" | awk '{print $1}')"
report="$(jq -n \
  --arg schema 'termiflow.package_contract.v1' \
  --arg version "$version" \
  --arg crate "$crate" \
  --arg sha256 "$crate_sha256" \
  '{schema:$schema,version:$version,crate:$crate,sha256:$sha256,maintainer_sources:"excluded",maintainer_fixture_feature:"repository-only",extracted_all_targets:"passed"}')"
if [[ -n "$output" ]]; then
  mkdir -p "$(dirname -- "$output")"
  printf '%s\n' "$report" >"$output"
else
  printf '%s\n' "$report"
fi
