# /fixture - Golden Fixture Management

Manage golden test fixtures: view, compare, regenerate.

## Arguments
- `$ARGUMENTS` - Subcommand: `list`, `show <name>`, `diff <name>`, `regen`, `add <name>`

## Instructions

### `list` - List all fixtures
```bash
ls tests/fixtures/inputs/*.md | wc -l
echo "---"
ls tests/fixtures/inputs/ | sed 's/_[tlbr][drlb]\.md$//' | sort -u
```
Show count and unique fixture families.

### `show <name>` - Display a fixture
Render the specified fixture in both styles:
```bash
# Input
cat tests/fixtures/inputs/$ARGUMENTS_td.md

# Unicode output
cargo run --bin tw -- tests/fixtures/inputs/$ARGUMENTS_td.md

# ASCII output
cargo run --bin tw -- --style ascii tests/fixtures/inputs/$ARGUMENTS_td.md
```

### `diff <name>` - Compare actual vs expected
```bash
# Generate actual output
cargo run --bin tw -- tests/fixtures/inputs/$ARGUMENTS_td.md > /tmp/actual_unicode.txt
cargo run --bin tw -- --style ascii tests/fixtures/inputs/$ARGUMENTS_td.md > /tmp/actual_ascii.txt

# Compare
diff tests/fixtures/expected/$ARGUMENTS_td_unicode.txt /tmp/actual_unicode.txt
diff tests/fixtures/expected/$ARGUMENTS_td_ascii.txt /tmp/actual_ascii.txt
```
Report differences or "identical".

### `regen` - Regenerate all fixtures
**Warning**: Only do this after intentional rendering changes!

```bash
cargo test --features golden -- --ignored 2>&1
```

Then verify the changes make sense:
```bash
git diff tests/fixtures/expected/ | head -100
```

### `add <name>` - Create a new fixture

1. Create input file: `tests/fixtures/inputs/<name>_td.md`
2. Create variants for other directions: `_lr.md`, `_bt.md`, `_rl.md`
3. Generate expected outputs:
```bash
for dir in td lr bt rl; do
  cargo run --bin tw -- tests/fixtures/inputs/<name>_$dir.md > tests/fixtures/expected/<name>_${dir}_unicode.txt
  cargo run --bin tw -- --style ascii tests/fixtures/inputs/<name>_$dir.md > tests/fixtures/expected/<name>_${dir}_ascii.txt
done
```
4. Run `cargo test --test golden` to verify.

### Fixture Naming Convention
```
[category]_[name]_[direction].md

Categories: flow, edge, label, shape, parse, config, subgraph, scale, crossing, error
Directions: td, lr, bt, rl
```

### Expected Output Naming
```
[category]_[name]_[direction]_[style].txt

Styles: unicode, ascii
```
