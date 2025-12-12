# TermiFlow Strategic Roadmap

> "jq for diagrams" - pipe-friendly, terminal-native Mermaid rendering

## Current State (v0.1.x)

- 9 composite styles (ascii, unicode, heavy, double, rounded, dots, plus, stars, blocks)
- Expanded edge routing with vertical stems and junctions (RFC-001)
- Two-pass parser with forward references
- Waterfall layout with cycle detection
- 64 tests passing

---

## Hackweek Sprint (Priority Focus)

**Goal:** Maximum feature impact - expand diagram capabilities and visual polish.

### H1. Edge Labels
**Priority:** P0 | **Complexity:** Medium | **Impact:** High

Support `-->|label|` syntax for edge annotations.

```mermaid
graph TD
    A[Start] -->|validate| B[Process]
    B -->|success| C[Done]
    B -->|error| D[Retry]
```

**Implementation approach:**
- Parser: Recognize `-->|text|`, `-- text -->`, and styled variants
- Layout: Add label row between connected nodes
- Render: Center label on vertical edge segment

**Acceptance criteria:**
- [ ] Parse all Mermaid edge label syntaxes
- [ ] Render labels centered on edge paths
- [ ] Handle multi-word labels with proper width
- [ ] Golden tests for labeled edges

---

### H2. Node Shapes
**Priority:** P0 | **Complexity:** Medium | **Impact:** High

Support common Mermaid node shapes for visual variety.

| Syntax | Shape | Unicode | ASCII |
|--------|-------|---------|-------|
| `[text]` | Rectangle | `┌─┐ │ └─┘` | `+-+ \| +-+` (current) |
| `(text)` | Rounded | `╭─╮ │ ╰─╯` | `/-\ \| \-/` |
| `{text}` | Diamond | `◇` centered | `/\ \/` |
| `((text))` | Circle | `(  )` | `(  )` |
| `([text])` | Stadium | `(══)` | `(==)` |
| `>text]` | Flag/Asymmetric | `▷──┐` | `>--+` |

**Implementation approach:**
- Parser: Detect shape delimiters, store `NodeShape` enum in Node
- Render: Shape-specific `draw_*()` functions
- Layout: Calculate bounding box per shape type

**Acceptance criteria:**
- [ ] Parse all shape syntaxes
- [ ] Render each shape in unicode and ascii modes
- [ ] Edge connections attach to correct anchor points
- [ ] Golden tests for each shape

---

### H3. Sequence Diagrams
**Priority:** P0 | **Complexity:** High | **Impact:** Very High

New diagram type - huge feature expansion.

```mermaid
sequenceDiagram
    participant Alice
    participant Bob
    Alice->>Bob: Hello Bob!
    Bob-->>Alice: Hi Alice!
    Alice->>Bob: How are you?
    Bob-->>Alice: Good thanks!
```

**Target output (Unicode):**
```
┌───────┐            ┌───────┐
│ Alice │            │  Bob  │
└───┬───┘            └───┬───┘
    │                    │
    │  Hello Bob!        │
    │───────────────────>│
    │                    │
    │  Hi Alice!         │
    │<- - - - - - - - - -│
    │                    │
```

**Target output (ASCII):**
```
+-------+            +-------+
| Alice |            |  Bob  |
+---+---+            +---+---+
    |                    |
    |  Hello Bob!        |
    |------------------>|
    |                    |
    |  Hi Alice!         |
    |<- - - - - - - - - -|
    |                    |
```

**Architecture:**
```
src/
├── parser.rs          (add diagram type detection)
├── diagrams/
│   ├── mod.rs         (DiagramType enum, dispatch)
│   ├── flowchart/     (existing code, reorganized)
│   │   ├── parser.rs
│   │   ├── layout.rs
│   │   └── render.rs
│   └── sequence/      (NEW)
│       ├── parser.rs  (~150 lines)
│       ├── layout.rs  (~100 lines)
│       └── render.rs  (~200 lines)
└── lib.rs             (dispatch based on diagram type)
```

**Parser requirements:**
- Detect `sequenceDiagram` header
- Parse `participant Name` and `actor Name`
- Parse messages: `->>` (solid), `-->>` (dashed), `-x` (lost), `-)` (async)
- Parse message text after `:`

**Layout requirements:**
- Horizontal participant spacing (equal or content-aware)
- Vertical message ordering (time flows down)
- Calculate lifeline column positions

**Render requirements:**
- Participant boxes at top
- Dashed vertical lifelines
- Horizontal arrows with labels
- Arrow heads: `>`, `>>`, `x`, `)`

**Acceptance criteria:**
- [ ] Parse basic sequence diagram syntax
- [ ] Render participants with lifelines
- [ ] Render solid and dashed message arrows
- [ ] Message labels positioned correctly
- [ ] Unicode and ASCII style support
- [ ] Golden tests for sequence diagrams

---

### H4. Subgraphs
**Priority:** P1 | **Complexity:** High | **Impact:** High

Group nodes visually within flowcharts.

```mermaid
graph TD
    subgraph Backend
        A[API] --> B[Database]
    end
    subgraph Frontend
        C[Web] --> D[Mobile]
    end
    C --> A
```

**Target output:**
```
┌─────────────────────┐
│       Backend       │
│  ┌───────┐          │
│  │  API  │          │
│  └───┬───┘          │
│      │              │
│      ↓              │
│  ┌──────────┐       │
│  │ Database │       │
│  └──────────┘       │
└─────────────────────┘
```

**Implementation approach:**
- Parser: Detect `subgraph name ... end` blocks
- Layout: Hierarchical positioning (subgraph as container)
- Render: Outer box with title, inner nodes positioned within

**Challenges:**
- Edge routing across subgraph boundaries
- Nested subgraphs (stretch goal; currently warns/ignored)
- Subgraph title positioning

**Acceptance criteria:**
- [x] Parse subgraph syntax (single-level)
- [x] Render subgraph as containing box + title
- [x] Position nodes within subgraph bounds (layout envelopes + gutters)
- [x] Route edges across subgraph boundaries (portal-aware border piercing)
- [x] Golden test inputs exist (expected outputs may need regeneration)

**Remaining work:**
- Nested subgraph support (or a clearer error mode)
- Per-subgraph styling controls and richer title/layout rules
- More deterministic cross-subgraph label placement in dense diagrams

---

### H5. LR/RL Orientation Polish
**Priority:** P2 | **Complexity:** Medium | **Impact:** Medium

Improve horizontal layout quality.

```mermaid
graph LR
    A[Input] --> B[Process] --> C[Output]
```

**Current issues:**
- Continued polish needed for dense LR/RL graphs (tight elbows, label placement, subgraph border interactions)

**Tasks:**
- [x] Audit layout for orientation assumptions
- [x] Implement horizontal edge routing
- [x] Use correct junction chars for horizontal flow (`├`, `┤`, `┬`, `┴`)
- [x] Golden test inputs exist (expected outputs may need regeneration)

---

## Post-Hackweek (Distribution)

These are important but deferred to focus hackweek on features:

### P1. Publish to crates.io
**Priority:** P1 (post-hackweek) | **Complexity:** Low

`cargo install termiflow`

- [ ] Verify Cargo.toml metadata
- [ ] `cargo publish --dry-run`
- [ ] Publish v0.2.0

### P2. GitHub Actions CI/CD
**Priority:** P2 (post-hackweek) | **Complexity:** Medium

- [ ] CI: test on push/PR
- [ ] Release: build binaries on tag
- [ ] Cross-compile: macOS, Linux (x86_64, aarch64)

### P3. Homebrew Tap
**Priority:** P3 (post-hackweek) | **Complexity:** Medium

`brew install dnvt/tap/termiflow`

---

## Future Considerations (v0.4.0+)

### State Diagrams
```mermaid
stateDiagram-v2
    [*] --> Active
    Active --> Inactive: timeout
    Inactive --> Active: wake
    Active --> [*]: done
```

### Class Diagrams
```mermaid
classDiagram
    Animal <|-- Duck
    Animal : +int age
    Animal : +String gender
```

### Watch Mode
`termiflow --watch diagram.md`

### Per-Element Styling
```mermaid
graph TD
    A[Start]:::highlight --> B[End]
    classDef highlight fill:#f9f
```

### Theme Presets
`--theme=github-dark`, `--theme=monokai`

---

## Hackweek Execution Order

```
Day 1: Edge Labels (H1)
       └── Parser → Layout → Render → Tests

Day 2: Node Shapes (H2)
       └── Parser shapes → Render functions → Tests

Day 3-4: Sequence Diagrams (H3)
         └── Architecture refactor → Parser → Layout → Render → Tests

Day 5: Subgraphs (H4) OR LR Polish (H5)
       └── Based on progress and energy
```

**Demo targets:**
- End of Day 2: "Flowcharts with labels and shapes"
- End of Day 4: "Two diagram types working!"
- End of Day 5: "Grouped nodes or horizontal layouts"

---

## Architecture Evolution

**Current (flowchart-only):**
```
src/
├── parser.rs      (flowchart parser)
├── layout.rs      (flowchart layout)
├── render/        (flowchart render)
├── graph.rs       (flowchart data)
└── lib.rs         (entry point)
```

**Target (multi-diagram):**
```
src/
├── lib.rs                    (public API, diagram dispatch)
├── bin/termiflow.rs          (CLI)
├── bin/tw.rs                 (CLI alias)
├── config.rs                 (configuration)
├── style.rs                  (shared styling)
├── diagrams/
│   ├── mod.rs                (DiagramType enum)
│   ├── flowchart/
│   │   ├── mod.rs
│   │   ├── parser.rs
│   │   ├── graph.rs
│   │   ├── layout.rs
│   │   └── render.rs
│   └── sequence/
│       ├── mod.rs
│       ├── parser.rs
│       ├── model.rs          (Participant, Message)
│       ├── layout.rs
│       └── render.rs
└── render/
    ├── mod.rs                (shared Canvas)
    └── canvas.rs
```

---

*Last updated: December 9, 2024*
*Focus: Hackweek Sprint*
