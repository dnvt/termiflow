# TermiFlow

> Interactive TUI graph explorer - **jq for diagrams**

Render Mermaid flowcharts as ASCII/Unicode diagrams in your terminal.

Current status: print-to-stdout mode is implemented (jq-style); `--tui` is stubbed and will land later.

## Features

- **Mermaid-Lite parser** - Supports common flowchart syntax (`graph TD`, nodes, edges) with strict/lenient modes
- **9 border styles** - `ascii`, `unicode`, `double`, `rounded`, `heavy`, `dots`, `plus`, `stars`, `blocks`
- **Composite styling** - Mix and match style components: `corner:dots,border:heavy`
- **Multiline labels (experimental)** - `--wrap` with `--max-lines` for taller boxes
- **Subgraphs** - Single-level `subgraph ... end` containers with titles and portal-aware border piercing
- **9 node shapes** - Rectangle, rounded, diamond, circle, stadium, hexagon, database, subroutine, asymmetric
- **Edge labels** - Pipe syntax `A -->|label| B` and text syntax `A -- label --> B`
- **Pipe-friendly** - Reads stdin / writes stdout by default
- **Cycle detection** - Back-edges rendered in gutter with warnings (or skipped when clipped)
- **Config precedence** - CLI > in-file `%% termiflow:` directive > `~/.config/termiflow/config.toml`

## Installation

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

# Render TermiFlow JSON graph schema directly
cat graph.json | tw --from-json

# Choose a style or composite style
tw --style "corner:dots,border:heavy" diagram.md

# Wrap long labels across multiple lines (experimental)
tw --wrap --max-lines 3 diagram.md

# Output trimming/padding
tw --pad 1 diagram.md
tw --no-crop diagram.md

# Tighter layout spacing
tw --compact diagram.md

# Precompute routes for fan-in/out edges
tw --route-all diagram.md

# Strict mode (fail on warnings)
tw --strict diagram.md

# Interactive mode (not yet implemented - will exit with message)
tw --tui diagram.md
```

## Docs

- Pipelines (Terraform/Docker Compose/npm → Mermaid → TermiFlow): `docs/pipelines.md`
- CLI + syntax reference (flags, config, supported syntax): `docs/reference.md`
- Implementation notes/specs: `planning/`

## License

MIT
