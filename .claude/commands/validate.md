# /validate - Full Validation Suite

Run complete validation: tests, lints, and format checks.

## Instructions

Run all checks in sequence, reporting results:

### 1. Format Check
```bash
cargo fmt --check
```
If fails: Run `cargo fmt` to fix, then report what changed.

### 2. Clippy Lint
```bash
cargo clippy 2>&1
```
Report warning count and any new/notable warnings.

### 3. Test Suite
```bash
cargo test 2>&1
```
Report: total tests, passed, failed, ignored.

### 4. Golden Tests (if fixtures exist)
```bash
cargo test --test golden 2>&1
```
Report any fixture mismatches.

### 5. Build Release
```bash
cargo build --release 2>&1
```
Verify release build succeeds.

### Report Format

```
## Validation Report

| Check | Status | Details |
|-------|--------|---------|
| Format | ✅/❌ | [clean/N files changed] |
| Clippy | ✅/⚠️ | [N warnings] |
| Tests | ✅/❌ | [X passed, Y failed] |
| Golden | ✅/❌ | [X fixtures validated] |
| Release | ✅/❌ | [built/failed] |

**Overall**: [PASS/FAIL]

[If any failures, list specific issues]
```

### Pre-Commit Checklist
Before committing, ensure:
- [ ] `cargo fmt` - Code formatted
- [ ] `cargo clippy` - No new warnings
- [ ] `cargo test` - All tests pass
- [ ] Manual smoke test of changed functionality
