# TermiFlow

> Terminal-native Mermaid flowchart renderer and local preview tool

Render Mermaid flowcharts as ASCII/Unicode diagrams in your terminal without a
browser.

Current status: print mode and primary-screen watch mode (`--watch`) are ready
for the first public beta. Alternate-screen live preview (`--tui`) works today,
but it remains a partial mode whose input and scroll behavior depends on the
terminal emulator.

## Examples

**Pipeline — unicode style:**

```
$ echo 'graph LR; A[Parse] --> B[Layout]; B --> C[Render]; C --> D[Output]' | tw
```

```
┌─────────┐        ┌──────────┐        ┌──────────┐        ┌──────────┐
│  Parse  ├───────→│  Layout  ├───────→│  Render  ├───────→│  Output  │
└─────────┘        └──────────┘        └──────────┘        └──────────┘
```

**Same diagram — `--style ascii` for maximum portability:**

```
$ echo 'graph LR; A[Parse] --> B[Layout]; B --> C[Render]; C --> D[Output]' | tw --style ascii
```

```
+---------+        +----------+        +----------+        +----------+
|  Parse  +------->|  Layout  +------->|  Render  +------->|  Output  |
+---------+        +----------+        +----------+        +----------+
```

**Decision flow with branching:**

```
$ printf 'graph TD\n    A[Build]-->B[Test]\n    B-->C{Pass?}\n    C-->|yes|D[Stage]\n    C-->|no|E[Fail]\n    D-->F[Deploy]' | tw
```

```
        ┌─────────┐
        │  Build  │
        └────┬────┘
             │
             ↓
        ┌────────┐
        │  Test  │
        └────┬───┘
             │
             ↓
             ◇
        <  Pass?  >
             ┬
             │
      ┌──────┴───────┐
     yes            no
      ↓              ↓
 ┌─────────┐    ┌────────┐
 │  Stage  │    │  Fail  │
 └────┬────┘    └────────┘
      │
      ↓
┌──────────┐
│  Deploy  │
└──────────┘
```

**Subgraphs — CI/CD pipeline with containers:**

```
$ printf 'graph LR\n    subgraph CI\n        A[Push]-->B[Lint]-->C[Test]\n    end\n    subgraph CD\n        D[Build]-->E[Deploy]\n    end\n    C-->D' | tw
```

```
┏[  CI  ]━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓   ┏[  CD  ]━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
┃                                                  ┃   ┃                                   ┃
┃ ┌────────┐        ┌────────┐        ┌────────┐   ┃   ┃  ┌─────────┐        ┌──────────┐  ┃
┃ │  Push  ├───────→│  Lint  ├───────→│  Test  ├─────────→│  Build  ├───────→│  Deploy  │  ┃
┃ └────────┘        └────────┘        └────────┘   ┃   ┃  └─────────┘        └──────────┘  ┃
┃                                                  ┃   ┃                                   ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛   ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
```

## Features

- **Focused Mermaid flowchart parser** - Flowchart-only wedge for local docs workflows, not full Mermaid parity
- **Supported edge kinds** - `-->`, `---`, `==>`, `-.->`, `<-->`, `--o`, `--x`, plus pipe/text labels
- **9 border styles** - `ascii`, `unicode`, `double`, `rounded`, `heavy`, `dots`, `plus`, `stars`, `blocks`
- **Composite styling** - Mix and match style components: `corner:dots,border:heavy`
- **Multiline labels** - `--wrap` with `--max-lines` for taller boxes
- **Subgraphs** - Nested `subgraph ... end` containers with titles, ancestor-aware portal piercing, and clean multi-direction containment
- **14 node shapes** - Rectangle, rounded, diamond, circle, double-circle, database, subroutine, and trapezoid/parallelogram variants
- **Edge labels** - Pipe syntax `A -->|label| B` and text syntax `A -- label --> B`
- **Pipe-friendly** - Reads stdin / writes stdout by default
- **JSON input mode** - `--from-json` for TermiFlow's lightweight graph schema
- **Cycle detection** - Back-edges rendered in gutter with warnings (or skipped when clipped)
- **Config precedence** - CLI > in-file `%% termiflow:` directive > `~/.config/termiflow/config.toml`
- **Live preview** - `--watch` for low-flicker inline redraws in normal scrollback; `--tui` for partial alternate-screen panning/reload/findings
- **Visual audit + repair** - `--audit`, `--optimize-render`, render/layout repair passes, and critic output for polishing difficult ASCII

## Installation

### Homebrew (macOS and Linux — no Rust required)

```bash
brew install dnvt/termiflow/termiflow
```

### crates.io

```bash
cargo install termiflow
```

### From source

```bash
cargo install --path .
```

All three options install both `termiflow` and `tw`.

## Quickstart

```bash
# Render a Mermaid flowchart file
tw diagram.md

# Pipe a generated Mermaid flowchart into TermiFlow
some-generator | tw

# Render the lightweight JSON graph schema instead of Mermaid
cat graph.json | tw --from-json

# Choose a style or composite style
tw --style "corner:dots,border:heavy" diagram.md

# Wrap long labels across multiple lines
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
- Wrapping, truncation, preview frames, and status rows all follow the same
  display-width policy. The final rendered canvas is still char-backed, so some
  multi-codepoint grapheme composition can still vary by terminal.
- Unicode width for emoji, CJK, and ambiguous-width characters can vary across
  terminals and emulator config. Use `--style ascii` for the most portable
  output.
- For current Mermaid syntax gaps such as `style`, `classDef`, edge IDs,
  `@{}` shapes, and markdown labels, see `docs/reference.md`.

## Docs

- Pipelines (Terraform/Docker Compose/npm → Mermaid → TermiFlow): `docs/pipelines.md`
- CLI + syntax reference (flags, config, supported syntax): `docs/reference.md`

## License

MIT
