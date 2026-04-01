# TermiFlow

> Terminal-native Mermaid flowchart renderer and local preview tool

Render Mermaid flowcharts as ASCII/Unicode diagrams in your terminal without a
browser.

Current status: print mode and primary-screen watch mode (`--watch`) are ready
for the first public beta. Alternate-screen live preview (`--tui`) works today,
but it remains a partial mode whose input and scroll behavior depends on the
terminal emulator.

## Features

- **Focused Mermaid flowchart parser** - Flowchart-only wedge for local docs workflows, not full Mermaid parity
- **Supported edge kinds** - `-->`, `---`, `==>`, `-.->`, `<-->`, `--o`, `--x`, plus pipe/text labels
- **9 border styles** - `ascii`, `unicode`, `double`, `rounded`, `heavy`, `dots`, `plus`, `stars`, `blocks`
- **Composite styling** - Mix and match style components: `corner:dots,border:heavy`
- **Multiline labels (experimental)** - `--wrap` with `--max-lines` for taller boxes
- **Subgraphs** - Single-level `subgraph ... end` containers with titles and portal-aware border piercing
- **14 node shapes** - Rectangle, rounded, diamond, circle, double-circle, database, subroutine, and trapezoid/parallelogram variants
- **Edge labels** - Pipe syntax `A -->|label| B` and text syntax `A -- label --> B`
- **Pipe-friendly** - Reads stdin / writes stdout by default
- **Cycle detection** - Back-edges rendered in gutter with warnings (or skipped when clipped)
- **Config precedence** - CLI > in-file `%% termiflow:` directive > `~/.config/termiflow/config.toml`
- **Live preview** - `--watch` for low-flicker inline redraws in normal scrollback; `--tui` for partial alternate-screen panning/reload/findings
- **Visual audit + repair** - `--audit`, `--optimize-render`, render/layout repair passes, and critic output for polishing difficult ASCII

## Installation (from source)

```bash
cargo install --path .
```

This installs both `termiflow` and `tw` (a short alias). To install only `termiflow`:

```bash
cargo install --path . --bin termiflow
```

## Quickstart

```bash
# Render a Mermaid flowchart file
tw diagram.md

# Pipe a generated Mermaid flowchart into TermiFlow
some-generator | tw

# Choose a style or composite style
tw --style "corner:dots,border:heavy" diagram.md

# Wrap long labels across multiple lines (experimental)
tw --wrap --max-lines 3 diagram.md

# Output trimming/padding
tw --pad 1 diagram.md
tw --no-crop diagram.md

# Tighter layout spacing
tw --compact diagram.md

# Live preview modes
tw --tui diagram.md
tw --watch diagram.md

# Audit / repair difficult diagrams
tw --audit --optimize-render diagram.md

# Strict mode (fail on warnings)
tw --strict diagram.md
```

## Compatibility Notes

- `--watch` is the safer live-preview mode if you want normal scrollback and
  fewer fullscreen-emulator surprises.
- `--tui` uses raw mode plus the alternate screen; wheel scrolling and some
  fullscreen keybindings can be translated or intercepted by the terminal
  emulator.
- Unicode width for emoji, CJK, and ambiguous-width characters can vary across
  terminals and emulator config. Use `--style ascii` for the most portable
  output.
- For current Mermaid syntax gaps such as `style`, `classDef`, edge IDs,
  `@{}` shapes, markdown labels, and nested subgraphs, see `docs/reference.md`.

## Docs

- Pipelines (Terraform/Docker Compose/npm → Mermaid → TermiFlow): `docs/pipelines.md`
- CLI + syntax reference (flags, config, supported syntax): `docs/reference.md`
- Implementation notes/specs: `planning/`

## License

MIT
