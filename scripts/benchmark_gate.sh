#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: scripts/benchmark_gate.sh [--receipt PATH] [--sample-size N] [--allow-dirty]

Run the locked Criterion rendering smoke benchmark and emit a
termiflow.benchmark_receipt.v1 JSON receipt. Dirty worktrees are rejected
unless --allow-dirty is explicitly supplied.
USAGE
}

receipt_path="target/benchmark/termiflow-benchmark-receipt.json"
sample_size=10
allow_dirty=false

while (($# > 0)); do
  case "$1" in
    --receipt)
      (($# >= 2)) || { echo "--receipt requires a path" >&2; exit 2; }
      receipt_path="$2"
      shift 2
      ;;
    --sample-size)
      (($# >= 2)) || { echo "--sample-size requires a positive integer" >&2; exit 2; }
      sample_size="$2"
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
      echo "unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

[[ "$sample_size" =~ ^[1-9][0-9]*$ && "$sample_size" -ge 10 ]] || {
  echo "--sample-size must be an integer greater than or equal to 10: $sample_size" >&2
  exit 2
}
command -v jq >/dev/null 2>&1 || {
  echo "benchmark gate requires jq to emit its JSON receipt" >&2
  exit 2
}

root="$(git rev-parse --show-toplevel)"
cd "$root"
if [[ "$receipt_path" != /* ]]; then
  receipt_path="$root/$receipt_path"
fi
receipt_dir="$(dirname "$receipt_path")"
mkdir -p "$receipt_dir"

stdout_log="$receipt_dir/benchmark.stdout.log"
stderr_log="$receipt_dir/benchmark.stderr.log"
metadata_log="$receipt_dir/cargo-metadata.json"
rustc_log="$receipt_dir/rustc-vv.txt"
cargo_log="$receipt_dir/cargo-version.txt"
source_status_log="$receipt_dir/source-status.txt"
source_before_log="$receipt_dir/source-before.json"
source_after_log="$receipt_dir/source-after.json"
untracked_paths_log="$receipt_dir/untracked-paths.txt"
tracked_paths_log="$receipt_dir/tracked-paths.txt"
workload_paths_log="$receipt_dir/workload-paths.txt"

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
      echo "source path is not a regular file: $path" >&2
      return 1
    }
    printf '%s\0' "$path"
    git hash-object -- "$path"
    printf '\0'
  done < "$1" | hash_stream
}

capture_source() {
  local output="$1"
  local dirty_text dirty_json untracked_sha workload_sha untracked_paths_json

  git ls-files --others --exclude-standard -z > "$untracked_paths_log"
  git ls-files -z > "$tracked_paths_log"
  git ls-files -z -- \
    Cargo.toml Cargo.lock rust-toolchain.toml rust-toolchain \
    benches tests/fixtures/inputs tests/fixtures/metadata.json \
    tests/fixtures/fixture_spec.json > "$workload_paths_log"
  [[ -s "$workload_paths_log" ]] || {
    echo "benchmark workload file set is empty" >&2
    return 1
  }

  dirty_text="$(git status --porcelain=v1 --untracked-files=all)"
  if [[ -n "$dirty_text" ]]; then
    dirty_json=true
  else
    dirty_json=false
  fi
  untracked_sha="$(hash_path_list "$untracked_paths_log")"
  workload_sha="$(hash_path_list "$workload_paths_log")"
  untracked_paths_json="$(jq -R -s 'split("\u0000") | map(select(length > 0))' < "$untracked_paths_log")"

  local unsigned
  unsigned="$(jq -cn \
    --arg commit "$(git rev-parse HEAD)" \
    --arg tracked "$(hash_path_list "$tracked_paths_log")" \
    --arg diff "$(git diff --binary HEAD -- | hash_stream)" \
    --arg staged "$(git diff --cached --binary -- | hash_stream)" \
    --arg untracked "$untracked_sha" \
    --arg workload "$workload_sha" \
    --argjson paths "$untracked_paths_json" \
    --argjson dirty "$dirty_json" \
    '{source_commit:$commit, worktree_dirty:$dirty, tracked_worktree_sha256:$tracked, tracked_diff_sha256:$diff, staged_diff_sha256:$staged, untracked_files_sha256:$untracked, untracked_paths:$paths, workload_sha256:$workload}')"
  jq -c --arg identity "$(printf '%s' "$unsigned" | hash_stream)" \
    '. + {source_identity_sha256:$identity}' <<< "$unsigned" > "$output"
}

capture_source "$source_before_log"
if [[ "$(jq -r '.worktree_dirty' "$source_before_log")" == true && "$allow_dirty" != true ]]; then
  echo "benchmark gate requires a clean worktree; pass --allow-dirty only for exploratory evidence" >&2
  exit 2
fi

git status --porcelain=v1 --untracked-files=all > "$source_status_log"
cargo metadata --locked --format-version 1 > "$metadata_log"
rustc -Vv > "$rustc_log"
cargo --version > "$cargo_log"

manifest_sha256="$(hash_file Cargo.toml)"
lock_sha256="$(hash_file Cargo.lock)"
metadata_sha256="$(hash_file "$metadata_log")"
rust_toolchain_sha256=""
if [[ -f rust-toolchain.toml ]]; then
  rust_toolchain_sha256="$(hash_file rust-toolchain.toml)"
elif [[ -f rust-toolchain ]]; then
  rust_toolchain_sha256="$(hash_file rust-toolchain)"
fi
rustc_sha256="$(hash_file "$rustc_log")"
cargo_sha256="$(hash_file "$cargo_log")"
criterion_version="$(cargo tree --locked --all-features --format '{p}' | sed -n 's/.*criterion v//p' | head -n 1)"

started_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
if ! cargo bench --locked --bench rendering --all-features -- --noplot --sample-size "$sample_size" > "$stdout_log" 2> "$stderr_log"; then
  echo "benchmark command failed; logs retained under $receipt_dir" >&2
  exit 1
fi
finished_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

capture_source "$source_after_log"
before_sha="$(jq -r '.source_identity_sha256' "$source_before_log")"
after_sha="$(jq -r '.source_identity_sha256' "$source_after_log")"
if [[ "$before_sha" != "$after_sha" ]]; then
  echo "source state changed during benchmark; receipt rejected" >&2
  exit 1
fi

stdout_sha256="$(hash_file "$stdout_log")"
stderr_sha256="$(hash_file "$stderr_log")"
stdout_bytes="$(wc -c < "$stdout_log" | tr -d '[:space:]')"
stderr_bytes="$(wc -c < "$stderr_log" | tr -d '[:space:]')"
metadata_bytes="$(wc -c < "$metadata_log" | tr -d '[:space:]')"
rustc_release="$(sed -n 's/^release: //p' "$rustc_log" | head -n 1)"
target="$(sed -n 's/^host: //p' "$rustc_log" | head -n 1)"
host_os="$(uname -s)"
host_arch="$(uname -m)"
command_json="$(jq -cn --arg sample "$sample_size" \
  '["cargo","bench","--locked","--bench","rendering","--all-features","--","--noplot","--sample-size",$sample]')"

jq -n \
  --arg schema "termiflow.benchmark_receipt.v1" \
  --arg started "$started_at" \
  --arg finished "$finished_at" \
  --argjson command "$command_json" \
  --arg criterion "$criterion_version" \
  --arg rustc "$rustc_release" \
  --arg target "$target" \
  --arg os "$host_os" \
  --arg arch "$host_arch" \
  --argjson source "$(cat "$source_after_log")" \
  --arg source_sha "$after_sha" \
  --arg manifest "$manifest_sha256" \
  --arg lock "$lock_sha256" \
  --arg metadata "$metadata_sha256" \
  --arg metadata_path "$(basename "$metadata_log")" \
  --arg metadata_bytes "$metadata_bytes" \
  --arg toolchain "$rust_toolchain_sha256" \
  --arg rustc_sha "$rustc_sha256" \
  --arg cargo_sha "$cargo_sha256" \
  --arg workload "$(jq -r '.workload_sha256' "$source_after_log")" \
  --arg stdout "$stdout_sha256" \
  --arg stderr "$stderr_sha256" \
  --arg stdout_path "$(basename "$stdout_log")" \
  --arg stderr_path "$(basename "$stderr_log")" \
  --arg stdout_bytes "$stdout_bytes" \
  --arg stderr_bytes "$stderr_bytes" \
  --argjson allow_dirty "$allow_dirty" \
  '{schema:$schema, status:"passed", comparability:(if $allow_dirty then "exploratory-dirty" else "clean" end), started_at:$started, finished_at:$finished, command:$command, benchmark:{name:"rendering", criterion_version:$criterion, all_features:true}, host:{os:$os, arch:$arch, target:$target, rustc_release:$rustc}, source_identity:$source, source_identity_sha256:$source_sha, build:{cargo_manifest_sha256:$manifest, cargo_lock_sha256:$lock, cargo_metadata_sha256:$metadata, cargo_metadata_path:$metadata_path, cargo_metadata_bytes:($metadata_bytes|tonumber), rust_toolchain_sha256:(if $toolchain == "" then null else $toolchain end), rustc_verbose_sha256:$rustc_sha, cargo_version_sha256:$cargo_sha}, workload_sha256:$workload, logs:{stdout:{path:$stdout_path, bytes:($stdout_bytes|tonumber), sha256:$stdout}, stderr:{path:$stderr_path, bytes:($stderr_bytes|tonumber), sha256:$stderr}}}' > "$receipt_path"

echo "benchmark receipt: $receipt_path"
