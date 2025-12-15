# Reference

## Binaries

- `termiflow`: main CLI
- `tw`: short alias

## Modes

- Default: print-to-stdout (jq-style). Reads a file (if provided) or stdin.
- `--tui`: interactive mode (not yet implemented; exits with a message).
- `--print [FILE]`: explicit print mode (optional file argument; `-` means stdin).
- `--from-json`: parse input as TermiFlow JSON graph schema (instead of Mermaid).

## Common Flags

- `--style`, `-s`: base style (`ascii`, `unicode`, `double`, `rounded`, `heavy`, `dots`, `plus`, `stars`, `blocks`) or composite (e.g. `corner:dots,border:heavy`).
- `--max-label`: label width budget in columns (default 20). Affects truncation and box sizing.
- `--wrap`: enable multiline label wrapping (experimental; default off).
- `--max-lines`: max label lines when wrapping is enabled (default 1).
- `--crop` / `--no-crop`: crop empty margins around output (default on).
- `--pad N`: add padding (spaces/lines) around output (default 0).
- `--compact`: use a tighter layout spacing (less whitespace).
- `--route-all`: precompute routes for fan-in/out edges (renderer normally owns junctions).
- `--strict`: treat parse warnings as errors.

Composite components: `corner`, `border`, `arrow`, `edge`, `junction`, `back`, `subgraph`.

## Supported Mermaid (Flowchart Only)

TermiFlow supports Mermaid flowcharts only. Accepted headers:

- `graph TD|LR|TB|BT|RL`
- `flowchart TD|LR|TB|BT|RL` (common generator output)

Supported patterns:

- Nodes (multiple shapes): `A[Label]`, `B{Decision}`, `C((Circle))`, `D[(Database)]`, `E[[Subroutine]]`, …
- Edges: `A --> B`, `A ---> B`
- Edge labels: `A -->|label| B` and `A -- label --> B`
- Subgraphs (single-level): `subgraph ... end` (nested subgraphs warn; `--strict` makes warnings fatal)
- Per-diagram directives: `%% termiflow: style=...`, `%% termiflow: max_label=...`
- Multiline: `%% termiflow: wrap=true`, `%% termiflow: max_lines=3`
- Click targets: `click ID "file.md"` (parsed; currently informational)

Not supported (yet):

- Mermaid styling/classes (`style`, `classDef`, `:::`)
- Non-flowchart diagram types (sequence/class/state/ER/gantt/etc.)

## Configuration

Precedence: CLI flags > in-file `%% termiflow:` directives > config file.

Config file locations:

- macOS: `~/Library/Application Support/termiflow/config.toml`
- Linux: `~/.config/termiflow/config.toml`
- Windows: `%APPDATA%\\termiflow\\config.toml`

Example:

```toml
style = "unicode"
max_label_width = 25
wrap = true
max_lines = 3
crop = true
pad = 0
```

## Debug

- `TERMIFLOW_DISABLE_PORTALS=1`: disable carving openings in subgraph borders.
- `TERMIFLOW_DEBUG_TIMING=1`: print coarse timing/routing stats to stderr.
- `TERMIFLOW_DEBUG_ROUTES=1`: dump precomputed edge route segments to stderr.
