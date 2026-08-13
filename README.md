# TermiFlow

> TermiFlow is a terminal-native Mermaid flowchart renderer.

Render Mermaid flowcharts as ASCII/Unicode diagrams directly in your
terminal—without a browser.

Current status: TermiFlow v0.2.4 is the current public release. Print mode
and primary-screen watch mode (`--watch`) are the stable workflow; alternate-
screen live preview (`--tui`) is available but remains partial because input and
scroll behavior depends on the terminal emulator.

## Examples

**Pipeline — unicode style:**

```
$ printf 'graph LR\n    A[Parse] --> B[Layout]\n    B --> C[Render]\n    C --> D[Output]\n' | tw
```

```
┌─────────┐        ┌──────────┐        ┌──────────┐        ┌──────────┐
│  Parse  ├───────→│  Layout  ├───────→│  Render  ├───────→│  Output  │
└─────────┘        └──────────┘        └──────────┘        └──────────┘
```

**Same diagram — `--style ascii` for maximum portability:**

```
$ printf 'graph LR\n    A[Parse] --> B[Layout]\n    B --> C[Render]\n    C --> D[Output]\n' | tw --style ascii
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
┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓   ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
┃  CI                                              ┃   ┃  CD                               ┃
┃                                                  ┃   ┃                                   ┃
┃ ┌────────┐        ┌────────┐        ┌────────┐   ┃   ┃  ┌─────────┐        ┌──────────┐  ┃
┃ │  Push  ├───────→│  Lint  ├───────→│  Test  ├───┼───┼─→│  Build  ├───────→│  Deploy  │  ┃
┃ └────────┘        └────────┘        └────────┘   ┃   ┃  └─────────┘        └──────────┘  ┃
┃                                                  ┃   ┃                                   ┃
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

### Homebrew (macOS and supported Linux systems — no Rust required)

```bash
brew install dnvt/termiflow/termiflow
```

The Homebrew formula is maintained in the external `dnvt/homebrew-termiflow`
tap; this repository keeps installation instructions only.

The v0.2.4 Linux release binaries are architecture-specific: the x86_64 GNU
archive targets glibc 2.39 or newer, while the aarch64 GNU archive targets
glibc 2.18 or newer. Older or different Linux userspaces can use the Rust
source fallback below.

### GitHub Releases

If Homebrew is unavailable, download a release binary from [GitHub
Releases](https://github.com/dnvt/termiflow/releases).

### From source (requires Rust)

A source checkout requires a Rust toolchain and Cargo:

```bash
# From a checked-out v0.2.4 source tree:
cargo install --locked --path . --bin tw
```

This installs the checkout you are using. For a no-Rust installation, prefer
the v0.2.4 Homebrew formula or the prebuilt binaries linked from the GitHub
release.

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
tw --spacing compact diagram.md

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

- Website: https://termiflow.dnvt.me
- Crates.io: https://crates.io/crates/termiflow
- GitHub repository: https://github.com/dnvt/termiflow
- Homebrew tap: https://github.com/dnvt/homebrew-termiflow
- Pipelines (Terraform/Docker Compose/npm → Mermaid → TermiFlow): `docs/pipelines.md`
- CLI + syntax reference (flags, config, supported syntax): `docs/reference.md`
- Architecture facts and generated diagrams: `docs/architecture/README.md`
- Visual QA lessons and renderer design rules: `docs/visual-lessons/README.md`
- Contributing: `CONTRIBUTING.md`
- Release instructions: `docs/releasing.md`
- Changelog: `CHANGELOG.md`
- Security policy and [private vulnerability reporting](https://github.com/dnvt/termiflow/security/advisories/new): `SECURITY.md`

## Development

The repository’s required checks are documented in `CONTRIBUTING.md`. The
repository includes deterministic golden fixtures, visual audit tooling, and
quality-baseline checks so rendering changes can be reviewed reproducibly; see
`tests/fixtures/README.md` for the contributor workflow.

For a locked local verification pass:

```bash
cargo fmt --check
cargo test --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
scripts/regenerate_golden.sh --check
```

Golden-output changes require explicit review of the generated report and every
changed snapshot; a regenerated snapshot is not approval by itself.

## License

MIT
