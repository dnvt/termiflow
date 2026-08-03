# Releasing TermiFlow

Releases are tag-driven, candidate-bound, and reproducible from public
repository evidence. The release job must not depend on a developer's private
Maestro state directory.

## Maintainer checklist

1. Update `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, and README installation
   text when the version changes.
2. Run the contributor quality gates from [`CONTRIBUTING.md`](../CONTRIBUTING.md)
   on a clean checkout.
3. Confirm architecture facts and generated diagrams:

   ```sh
   scripts/check_architecture_facts.sh
   scripts/architecture_facts_contract.sh
   ```

4. Create an exact `vX.Y.Z` tag matching `Cargo.toml`. The candidate job runs:

   ```sh
   scripts/release_candidate.sh prepare \
     --tag vX.Y.Z \
     --boundary "$RUNNER_TEMP/termiflow-release-candidate/candidate.json"
   ```

   The boundary binds the tag, source/tree, lockfile, Rust channel, package
   contract, and supported target matrix.
5. Each target build verifies the downloaded boundary before compiling and
   records its archive digest fragment. The release job finalizes all fragments
   and runs:

   ```sh
   scripts/release_candidate.sh finalize \
     --boundary candidate.json \
     --package termiflow-X.Y.Z.crate \
     --fragments-dir fragments \
     --archives-dir archives
   scripts/release_preflight.sh --boundary candidate.json --publish
   ```

6. Publish only after preflight passes. The GitHub release includes the target
   archives and finalized candidate manifest.

## Recovery and rollback

A missing, mismatched, dirty, or incomplete candidate is a failed release
attempt. Do not repair a candidate in place; create a new tag/boundary after
fixing the source. Inspect the candidate JSON and archive fragments as the
evidence trail before retrying.
