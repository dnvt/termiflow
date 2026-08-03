## Summary

<!-- What changed, and why? Keep this focused on one reviewable intent. -->

## Compatibility checklist

- [ ] Public graph/parser/config behavior is preserved or the decision is linked.
- [ ] Approved golden and visual outputs are unchanged unless intentionally reviewed.
- [ ] No private Maestro state, generated projection, or local path is required.
- [ ] New facts/diagrams are generated from `docs/architecture/facts.json`.

## Verification

<!-- List the focused and full commands you ran, including any negative tests. -->

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --locked --all-targets --all-features -- -D warnings`
- [ ] `cargo test --locked --all-targets --all-features`
- [ ] Relevant package, release-candidate, facts, or visual checks

## Notes for reviewers

<!-- Risks, deferred work, migration notes, or deliberate output changes. -->
