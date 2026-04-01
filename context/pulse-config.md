# TermiFlow Pulse Config

**Updated:** 2026-04-01
**Purpose:** Configure the fixed 12-slot `/maestro:pulse` scheduler around
TermiFlow's actual product surface: Mermaid compatibility, terminal rendering,
quality repair, TUI/watch UX, portability, testing, and release hygiene.

## Operating Cadence

The scheduler in `/maestro:pulse` is fixed. This file assigns the best
TermiFlow-specific topics to those slots.

Recommended operating rhythm:

- Weekdays: run `/maestro:pulse` up to 3 times to clear both daily pulses plus
  the weekday-specific pulse.
- Weekends: run `/maestro:pulse` up to 2 times to clear the two daily pulses.
- Calendar trigger days (`1`, `8`, `15`, `21`, `22`): run one additional pulse
  if you want to stay current on biweekly/monthly tracks.
- If time is tight, clear OVERDUE pulses first, then pulse `1`, then pulse `2`.

## Feed Targets

Every finding should map to exactly one primary feed target and optionally one
secondary target.

| Feed Target | What It Covers | Primary Files / Plans |
|-------------|----------------|------------------------|
| `parser-parity` | Mermaid syntax, directives, shapes, edge kinds, click targets | `src/parser.rs`, `src/graph.rs`, `docs/reference.md`, `planning/AUDIT-mermaid-parity.md` |
| `layout-routing` | Waterfall layout, crossings, orthogonal routing, edge labels, aspect ratio | `src/layout.rs`, `src/crossing.rs`, `src/render/edge.rs`, `src/orientation.rs` |
| `render-feedback` | Semantic frame, critic, provenance, repair loop, visual audit | `planning/PHASE6_RENDER_FEEDBACK_ENGINE.md`, `src/render/{critic,provenance,repair,semantic,topology}.rs`, `tests/visual_audit.rs` |
| `terminal-ux` | TUI mode, watch mode, frame diffing, viewporting, live reload | `src/tui/`, `src/bin/common/mod.rs`, `README.md` |
| `unicode-portability` | Unicode width, emoji/CJK handling, terminal emulator behavior, style fidelity | `src/measure.rs`, `src/style.rs`, `tests/fixtures/`, `tests/default_print.rs` |
| `qa-fixtures` | Golden fixtures, snapshot strategy, fuzzing, regression harnesses | `tests/golden.rs`, `tests/fixtures/`, `tests/cli_flags.rs`, `tests/render_options_api.rs` |
| `release-distribution` | Cargo/dependency health, packaging, docs accuracy, distribution channels | `Cargo.toml`, `README.md`, `docs/reference.md`, CI/release workflow |
| `roadmap-positioning` | Product direction, differentiation, prioritization, adoption signals | `planning/PLAN.md`, `planning/AUDIT-mermaid-parity.md`, `README.md` |

## Global Quality Signals

### INGEST Immediately

- Directly changes or invalidates Mermaid flowchart syntax that TermiFlow
  supports or plans to support.
- Provides a reproducible rendering or portability failure with concrete input,
  output, fixture, issue, or benchmark evidence.
- Demonstrates a technique with measured wins for routing quality, diff
  rendering, repaint cost, snapshot reliability, or parser robustness.
- Identifies a security advisory, dependency break, or packaging change that can
  affect users within the next release cycle.
- Shows clear adoption pressure: repeated user demand, upstream issue volume, or
  competitor shipping something that overlaps a named roadmap gap.

### MONITOR

- Plausibly relevant work with incomplete benchmarks, unclear fit, or early
  ecosystem motion.
- Upstream discussion without shipped behavior, merged code, or operational
  evidence.
- Techniques that are promising but would require architectural work beyond the
  current roadmap window.

### SKIP

- Browser-only theming, animation, or styling work with no terminal relevance.
- Marketing material without technical detail, source code, or reproducible
  examples.
- Non-flowchart Mermaid features unless they affect shared parser assumptions or
  CLI ergonomics.
- Incremental work that does not change parity, quality, performance, or
  release risk for TermiFlow.

## Pulse Portfolio

### Pulse 1: Mermaid Upstream and Generator Drift

- Frequency: Daily
- Schedule: Every day
- Primary feed targets: `parser-parity`, `roadmap-positioning`
- Use when: you want the fastest signal for syntax drift, new flowchart
  features, or generator output that could break TermiFlow parsing.

High-signal criteria:

- New Mermaid flowchart syntax, edge kinds, shapes, label rules, click/style
  behavior, or directive changes.
- Generator tools producing Mermaid text that TermiFlow currently misparses.
- Upstream bugs or regressions with reproducible flowchart examples.

Search prompt:

```text
Research goal: detect upstream Mermaid flowchart and generator changes that can
create parser gaps or roadmap opportunities for TermiFlow.

Prioritize:
- Mermaid flowchart release notes, changelogs, roadmap notes, and spec updates
- GitHub issues/discussions around flowchart shapes, edge kinds, subgraphs,
  labels, directives, click, classDef/style, markdown strings, and parser drift
- Tools that emit Mermaid flowcharts (infra docs, architecture generators,
  diagram exporters) when their output stresses our parser

Example queries:
- Mermaid flowchart release notes edge shapes labels classDef 2026
- site:github.com mermaid-js mermaid flowchart issue shape edge label
- Mermaid generator flowchart output graph TD parser compatibility

Ignore:
- Sequence/state/class diagram work unless it affects shared parser rules
- Browser-only themes, animations, and CSS polish
```

### Pulse 2: Terminal Rendering, TUI, and Emulator Drift

- Frequency: Daily
- Schedule: Every day
- Primary feed targets: `terminal-ux`, `unicode-portability`
- Use when: you want fast awareness of terminal ecosystem changes that affect
  watch mode, TUI mode, ANSI behavior, and display fidelity.

High-signal criteria:

- ratatui, crossterm, portable terminal, or emulator changes that affect redraw,
  viewporting, keyboard handling, or alternate-screen behavior.
- Terminal emulator bugs or fixes around ANSI, Unicode width, cursor movement,
  OSC 8 links, or repaint artifacts.
- New patterns for incremental redraw or low-flicker watch mode output.

Search prompt:

```text
Research goal: monitor the Rust terminal UI and terminal-emulator ecosystem for
changes that affect TermiFlow's TUI, watch mode, or output correctness.

Prioritize:
- ratatui, crossterm, unicode-width, terminal emulator release notes/issues
- terminal diff rendering, partial repaint, viewport management, alternate
  screen behavior, and keyboard input handling
- real-world bug reports involving ANSI escape handling, wrapping, cursor math,
  and Unicode display correctness

Example queries:
- ratatui crossterm release notes terminal redraw unicode 2026
- terminal emulator ansi cursor movement unicode width bug 2026
- rust tui incremental repaint alternate screen viewport

Ignore:
- web canvas rendering work with no terminal analogue
- generic text editor TUI posts without concrete rendering techniques
```

### Pulse 3: Graph Layout, Routing, and Edge Label Placement

- Frequency: Weekly
- Schedule: Tuesday
- Primary feed targets: `layout-routing`, `render-feedback`
- Use when: you want external algorithms and heuristics that can improve
  readability, crossings, aspect ratio, or label placement.

High-signal criteria:

- Orthogonal routing, junction placement, crossing minimization, or label
  placement methods with concrete evaluation criteria.
- Open-source layout engines or papers with techniques adaptable to ASCII or
  Unicode diagrams.
- Readability metrics or scoring models that align with the critic/repair loop.

Search prompt:

```text
Research goal: find practical layout and routing ideas that improve TermiFlow's
deterministic readability without requiring a wholesale rewrite.

Prioritize:
- orthogonal edge routing, Sugiyama variants, barycenter/median refinements,
  crossing minimization, edge-label placement, and diagram readability metrics
- implementations in Rust or well-documented OSS that expose concrete heuristics
- scoring methods that could feed render-feedback penalties or repair candidates

Example queries:
- orthogonal edge routing label placement graph layout readability 2026
- Sugiyama crossing minimization edge label placement open source
- terminal ascii graph layout routing junction readability

Ignore:
- force-directed or interactive graph editors unless they surface reusable
  deterministic heuristics
```

### Pulse 4: Competitive CLI, Renderer, and Diagram UX Scan

- Frequency: Weekly
- Schedule: Monday
- Primary feed targets: `roadmap-positioning`, `terminal-ux`
- Use when: you need a weekly scan of adjacent CLI and renderer products to
  calibrate what TermiFlow should copy, ignore, or differentiate on.

High-signal criteria:

- Terminal-first diagram tools shipping watch mode, preview UX, diff views, or
  diagram audit/repair workflows.
- Mermaid CLI, D2, Graphviz, or adjacent tools closing gaps in areas that match
  our roadmap.
- Repeated user pain around browser dependency, onboarding, or terminal-native
  workflows that TermiFlow can exploit.

Search prompt:

```text
Research goal: track adjacent diagram tools and terminal UX patterns that should
shape TermiFlow's product direction and prioritization.

Prioritize:
- Mermaid CLI, D2, Graphviz frontends, textual diagram tools, terminal preview
  tools, and live-reload CLI workflows
- shipping features for watch mode, interactive navigation, diagnostics,
  low-flicker redraw, and export pipelines
- user complaints or adoption wins that clarify where terminal-native value is
  strongest

Example queries:
- mermaid cli watch live preview terminal release notes 2026
- d2 graphviz terminal diagram preview watch mode
- ascii diagram cli live reload renderer comparison

Ignore:
- whiteboard/SaaS diagram products with no CLI or automation surface
```

### Pulse 5: Unicode Width, CJK/Emoji, and Portability

- Frequency: Weekly
- Schedule: Wednesday
- Primary feed targets: `unicode-portability`, `qa-fixtures`
- Use when: you want to stay ahead of the hardest correctness failures in
  terminal output across platforms and fonts.

High-signal criteria:

- Real terminal-width bugs, CJK or emoji rendering changes, wcwidth updates, or
  portability caveats that could break fixtures.
- Cross-platform observations involving Windows Terminal, iTerm2, Kitty, Alacritty,
  Ghostty, tmux, or shells that alter rendering behavior.
- Techniques for testable width normalization or platform-aware fixture policy.

Search prompt:

```text
Research goal: find high-value signals about Unicode width and terminal
portability that can improve TermiFlow's rendering correctness and fixture
strategy.

Prioritize:
- Unicode width, emoji/CJK, grapheme cluster, and terminal emulator behavior
- Rust libraries and issue threads around width calculation and display drift
- cross-platform rendering discrepancies that can be reproduced in tests

Example queries:
- unicode width emoji cjk terminal rust issue 2026
- wcwidth terminal emulator discrepancy windows terminal kitty iTerm2
- rust unicode-width grapheme terminal rendering bug

Ignore:
- typography discussions with no terminal cell-model relevance
```

### Pulse 6: Testing, Fuzzing, and Parser Hardening

- Frequency: Weekly
- Schedule: Thursday
- Primary feed targets: `qa-fixtures`, `parser-parity`
- Use when: you want better regression detection, fixture strategy, and input
  hardening for a text-based renderer.

High-signal criteria:

- Snapshot/golden testing patterns that reduce noisy diffs and improve review.
- Parser fuzzing or corpus-mining approaches that surface malformed or
  generator-produced edge cases.
- Rust testing tools or practices relevant to CLI output, cross-platform I/O,
  and property testing.

Search prompt:

```text
Research goal: improve the reliability of TermiFlow's test harnesses, fixtures,
and parser robustness.

Prioritize:
- Rust snapshot and golden-test strategies for CLI renderers
- parser fuzzing, property testing, corpus minimization, and malformed-input
  regression workflows
- output normalization and fixture-review techniques that keep diffs readable

Example queries:
- rust cli snapshot testing golden fixtures parser fuzzing 2026
- rust property testing parser malformed input corpus minimization
- terminal renderer golden test strategy unicode snapshot

Ignore:
- browser screenshot testing unless the core idea cleanly transfers to text
  renderers
```

### Pulse 7: Dependency, Security, Packaging, and Release Engineering

- Frequency: Weekly
- Schedule: Friday
- Primary feed targets: `release-distribution`, `terminal-ux`
- Use when: you want steady awareness of breakage risk and easier distribution
  for users who install or automate TermiFlow.

High-signal criteria:

- RustSec advisories, dependency changes, or packaging/tooling shifts that
  affect a Rust CLI with TUI features.
- Improvements for Homebrew, cargo install, cross-platform builds, release
  automation, or shell completion distribution.
- Breaking changes in ratatui/crossterm or supporting crates that can alter UX.

Search prompt:

```text
Research goal: watch the operational surface of TermiFlow as a Rust CLI:
security, dependency health, packaging, and distribution.

Prioritize:
- RustSec advisories and dependency release notes for crates we use or may use
- cargo install, Homebrew, release automation, binary packaging, and shell
  completion/distribution practices for Rust CLIs
- CI and cross-platform packaging patterns that reduce user friction

Example queries:
- RustSec crossterm ratatui unicode-width release advisory 2026
- rust cli homebrew cargo install release automation 2026
- cross platform rust tui binary packaging windows mac linux

Ignore:
- deployment patterns for server apps that do not affect CLI distribution
```

### Pulse 8: Mermaid Parity Backlog and Feature Prioritization

- Frequency: Biweekly
- Schedule: 1st and 15th
- Primary feed targets: `parser-parity`, `roadmap-positioning`
- Use when: you want a deliberate parity scan to decide which Mermaid gaps are
  worth building next.

High-signal criteria:

- Upstream or user demand indicating that a specific parity gap is becoming
  urgent: edge kinds, nested subgraphs, per-element styling, new shapes, or
  richer labels.
- Strong examples showing which missing features block practical adoption.
- Evidence that a backlog item should move up or down in `planning/PLAN.md`.

Search prompt:

```text
Research goal: reassess TermiFlow's Mermaid parity backlog based on actual
ecosystem demand and implementation leverage.

Prioritize:
- usage evidence for unsupported features in flowcharts
- Mermaid examples, docs, generators, and issues that reveal which gaps matter
  most in automated or terminal-friendly workflows
- opportunities where one implementation unlocks multiple parity gaps

Example queries:
- Mermaid flowchart unsupported feature classDef nested subgraph edge kinds
- Mermaid flowchart generator output dotted thick open link 2026
- Mermaid parity terminal renderer missing feature demand

Ignore:
- parity work for non-flowchart diagram families unless it changes parser scope
  decisions
```

### Pulse 9: Strategic Roadmap and Positioning

- Frequency: Monthly
- Schedule: 21st
- Primary feed targets: `roadmap-positioning`, `release-distribution`
- Use when: you need a monthly strategy check on where TermiFlow should invest
  next relative to the market and its own branch reality.

High-signal criteria:

- Clear evidence that a roadmap item should be reprioritized or retired.
- Adoption signals that suggest tightening the product story around terminal
  Mermaid, watch mode, audit/repair, or CI-friendly output.
- Gaps in README or docs positioning relative to how adjacent tools are selling
  themselves.

Search prompt:

```text
Research goal: understand where TermiFlow should differentiate and what the
next roadmap bets should be.

Prioritize:
- product positioning and adoption signals for terminal-native diagram tooling
- workflows in docs, blogs, demos, and issue threads that show how teams use
  Mermaid in CI, docs-as-code, or terminal environments
- adjacent tools' messaging around preview, audit, collaboration, export, and
  automation

Example queries:
- terminal mermaid workflow docs as code ci preview blog 2026
- diagram cli product positioning watch mode audit render quality
- mermaid terminal renderer adoption pain points

Ignore:
- generic AI or whiteboard tooling with no relation to text-first diagrams
```

### Pulse 10: Live Preview, Watch Mode, and Editor Integration

- Frequency: Biweekly
- Schedule: 8th and 22nd
- Primary feed targets: `terminal-ux`, `release-distribution`
- Use when: you want to keep improving the hands-on authoring loop around
  TermiFlow, not just the final render.

High-signal criteria:

- Better file watching, live reload, viewport navigation, diagnostics panes, or
  editor hooks that would make `--watch` and `--tui` materially better.
- Proven patterns for diffing terminal frames, event handling, or preview
  integration with editors and shells.
- Opportunities for low-friction embedding in docs or developer workflows.

Search prompt:

```text
Research goal: improve the authoring loop around TermiFlow through better live
preview, watch mode, and editor/shell integration.

Prioritize:
- file watching, live reload, viewport navigation, keyboard affordances, and
  inline findings panels
- editor plugin or shell integration patterns for terminal-first tools
- practical diff rendering or redraw patterns that reduce flicker and CPU load

Example queries:
- rust file watcher tui live preview terminal editor integration 2026
- terminal diff renderer watch mode low flicker rust
- ratatui live reload viewport keyboard navigation preview

Ignore:
- browser iframe preview features with no shell or editor analogue
```

### Pulse 11: Performance, Incremental Repaint, and Frame Diffing

- Frequency: Weekly
- Schedule: Friday
- Primary feed targets: `render-feedback`, `terminal-ux`
- Use when: you want focused performance ideas for large diagrams, repaint
  costs, and repair-loop efficiency.

High-signal criteria:

- Benchmarked incremental redraw, semantic diffing, canvas compaction, or
  repair-loop scoring strategies.
- Data-structure or profiling insights relevant to large diagrams, dense
  routing, or frequent re-rendering.
- Practical terminal presenter techniques that fit the existing `src/tui/`
  modules.

Search prompt:

```text
Research goal: find performant techniques for TermiFlow's render-feedback loop,
large-diagram rendering, and terminal frame presentation.

Prioritize:
- incremental repaint, frame diffing, sparse update regions, and semantic frame
  comparison
- profiling reports or optimization notes for graph layout, routing, and text
  canvas rendering
- strategies that keep deterministic behavior while reducing redraw or repair
  cost

Example queries:
- terminal frame diff incremental repaint rust tui performance 2026
- graph layout routing performance profiling rust renderer
- ascii canvas sparse updates diff rendering

Ignore:
- GPU rendering or browser canvas optimization without a text-grid analogue
```

### Pulse 12: Pulse Portfolio Maintenance and Horizon Scan

- Frequency: Monthly
- Schedule: 1st
- Primary feed targets: `roadmap-positioning`, `release-distribution`
- Use when: you want to recalibrate the pulse portfolio itself and catch slow
  ecosystem shifts that deserve a new watch topic.

High-signal criteria:

- Evidence that an existing pulse topic is stale, too broad, or missing an
  important new thread.
- Major ecosystem shifts in Mermaid, Rust terminal tooling, or CLI distribution
  that should alter pulse prompts or roadmap focus.
- Repeated findings from recent pulses that suggest combining, splitting, or
  reprioritizing topics.

Search prompt:

```text
Research goal: reassess whether the current 12-pulse portfolio still matches
TermiFlow's codebase, roadmap, and ecosystem reality.

Prioritize:
- major roadmap and ecosystem changes across Mermaid, Rust TUI, Unicode
  portability, testing, and release engineering
- patterns from recent pulse outputs that suggest adding, pruning, or rewording
  pulse topics
- slow-moving but important changes in distribution channels, platform support,
  and user expectations for terminal-native tools

Example queries:
- Mermaid roadmap terminal renderer ecosystem 2026
- rust tui ecosystem trends release engineering cli distribution 2026
- terminal unicode portability trends 2026

Ignore:
- one-off novelty posts that do not imply a lasting research track
```
