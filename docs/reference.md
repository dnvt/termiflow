# Reference

## Binaries

- `termiflow`: main CLI
- `tw`: short alias

## Modes

- Default: print-to-stdout (jq-style). Reads a file (if provided) or stdin.
- `--tui`: alternate-screen live preview with auto-reload, panning, and findings overlay. Partial first-beta mode: raw-mode input, wheel scrolling, and some fullscreen keybindings depend on the terminal emulator.
- `--watch`: primary-screen watch mode with low-flicker inline redraw in normal scrollback. This is the safer live-preview mode when you want normal scrollback and fewer fullscreen-emulator surprises.
- `--print [FILE]`: explicit print mode (optional file argument; `-` means stdin).

## Common Flags

- `--style`, `-s`: base style (`ascii`, `unicode`, `double`, `rounded`, `heavy`, `dots`, `plus`, `stars`, `blocks`) or composite (e.g. `corner:dots,border:heavy`).
- `--max-label`: label width budget in columns (default 20). Affects truncation and box sizing.
- `--wrap`: enable multiline label wrapping (experimental; default off).
- `--max-lines`: max label lines when wrapping is enabled (default 1).
- `--crop` / `--no-crop`: crop empty margins around output (default on).
- `--pad N`: add padding (spaces/lines) around output (default 0).
- `--compact`: use a tighter layout spacing (less whitespace).
- `--fit-terminal`: constrain the canvas to current terminal dimensions.
- `--optimize-render`: enable bounded render/layout repair after the initial draw.
- `--render-repair-passes N`: max render repair passes when optimization is enabled.
- `--layout-repair-passes N`: max layout retry passes when optimization is enabled.
- `--audit`: emit a compact visual audit summary to stderr.
- `--debug-critic`: emit critic findings for the rendered frame.
- `--strict`: treat parse warnings as errors.

Composite components: `corner`, `border`, `arrow`, `edge`, `junction`, `back`, `subgraph`.

## Supported Mermaid (Flowchart Only)

TermiFlow supports Mermaid flowcharts only. It is a focused renderer for local
Mermaid docs workflows, not a full Mermaid implementation. Accepted headers:

- `graph TD|LR|TB|BT|RL`
- `flowchart TD|LR|TB|BT|RL` (common generator output)

Supported patterns:

- Nodes (multiple shapes): `A[Label]`, `B{Decision}`, `C((Circle))`, `D(((Event)))`, `E[(Database)]`, `F[[Subroutine]]`, `G([Stadium])`, `H{{Hex}}`, …
- Grouped edges: `A & B --> C`, `A --> B & C`
- Edges: `A --> B`, `A ---> B`, `A --- B`, `A ==> B`, `A -.-> B`, `A <--> B`, `A --o B`, `A --x B`
- Edge labels: `A -->|label| B` and `A -- label --> B`
- Subgraphs (single-level): `subgraph ... end` (nested subgraphs warn; `--strict` makes warnings fatal)
- Per-diagram directives: `%% termiflow: style=...`, `%% termiflow: max_label=...`
- Multiline: `%% termiflow: wrap=true`, `%% termiflow: max_lines=3`
- Click targets: `click ID "file.md"` (parsed; currently informational only)

## Current Gaps And Caveats

- Mermaid styling/classes (`style`, `classDef`, `:::`). `:::` suffixes are stripped with a warning so edges still parse, but no styling is applied.
- Mermaid flowchart edge IDs
- Mermaid `@{}` shape family
- Mermaid markdown-aware labels / markdown strings
- Nested subgraphs (warn and ignore inner subgraphs today)
- A line containing only `end` closes the current subgraph. Avoid bare lowercase `end` as generated content inside flowcharts.
- Non-flowchart diagram types (sequence/class/state/ER/gantt/etc.)

## Unicode And Terminal Portability

- `--watch` is the safer live-preview mode when you want normal scrollback and fewer alternate-screen surprises.
- `--tui` uses raw mode plus the alternate screen. Depending on the emulator, wheel input can be translated into arrow keys and some fullscreen keybindings may stay bound by the terminal.
- Unicode output follows the `unicode-width` policy used by the renderer, but actual width for emoji, CJK, and ambiguous-width characters can still vary with emulator configuration.
- Use `--style ascii` for the most portable output when you need predictable cross-terminal rendering.

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
- `TERMIFLOW_DEBUG_CRITIC=1`: emit structural critic findings even without `--debug-critic`.
- `TERMIFLOW_OPTIMIZE_RENDER=1`: force render optimization without passing `--optimize-render`.
