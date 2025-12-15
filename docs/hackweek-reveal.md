# Hackweek Reveal Script (TermiFlow / `tw`)

Goal: turn Mermaid flowcharts into readable terminal diagrams, with a jq-like pipe workflow.

## Setup (pick one)

- Installed: `tw --help`
- From repo: `alias tw='cargo run --quiet --bin tw --'`

Recommended flags for demos/screenshots:

```bash
alias twd='tw --wrap --max-lines 3 --pad 1 --compact'
```

---

## 0) Why

I love making diagrams to think and explain… but:
- ASCII diagrams are tedious and don’t scale
- Figma/Excalidraw is great, but I end up “designing” instead of thinking

## 1) Mermaid is a great *authoring* language

Start with a tiny flowchart:
- Input: `tests/fixtures/inputs/flow_simple_td.md`

```bash
cat tests/fixtures/inputs/flow_simple_td.md
```

## 2) …but Mermaid text is not very readable

It’s optimized for machines and renderers, not the human eye in a terminal.

## 3) …and you usually need an embedder to “see it”

GitHub/VS Code plugins/web renderers are great, but they’re not always available in:
- SSH sessions
- CI logs
- CLI pipelines

## 4) Enter TermiFlow (`tw`): render Mermaid to the terminal

```bash
cat tests/fixtures/inputs/flow_simple_td.md | tw
```

Bonus: make it presentation-friendly (wrap + pad + compact):

```bash
cat tests/fixtures/inputs/flow_simple_td.md | twd
```

## 5) Shapes: more expressive than boxes

- Input: `tests/fixtures/inputs/shape_all_td.md`

```bash
cat tests/fixtures/inputs/shape_all_td.md | tw
```

## 6) Labels: keep context on the edges

- Input: `tests/fixtures/inputs/label_basic_td.md`

```bash
cat tests/fixtures/inputs/label_basic_td.md | tw
```

## 7) Directions: TD / LR / RL / BT (same graph, different view)

- TD: `tests/fixtures/inputs/flow_simple_td.md`
- LR: `tests/fixtures/inputs/flow_simple_lr.md`
- RL: `tests/fixtures/inputs/flow_simple_rl.md`
- BT: `tests/fixtures/inputs/flow_simple_bt.md`

```bash
cat tests/fixtures/inputs/flow_simple_lr.md | tw
```

## 8) Real routing: branching + convergence

- Branching: `tests/fixtures/inputs/edge_branch_td.md`
- Converge: `tests/fixtures/inputs/edge_converge_td.md`
- Complex: `tests/fixtures/inputs/edge_complex_td.md`

```bash
cat tests/fixtures/inputs/edge_complex_td.md | tw
```

## 9) Subgraphs: “containers” for architecture diagrams

- Input: `tests/fixtures/inputs/subgraph_complex_td.md`

```bash
cat tests/fixtures/inputs/subgraph_complex_td.md | tw
```

## 10) The fun part: pipelines (other sources → Mermaid → `tw`)

Fully local demo (JSON graph → Mermaid → render):

```bash
python3 examples/graph_to_mermaid.py examples/inputs/microservices_graph.json | twd
```

Even better: skip Mermaid entirely (JSON graph → render):

```bash
cat examples/inputs/microservices_graph.json | tw --from-json | twd
```

Local “real codebase” pipeline (Cargo workspace → render):

```bash
cargo metadata --format-version 1 \
  | python3 examples/cargo_metadata_to_graph.py \
  | tw --from-json | twd
```

More pipelines (Terraform / Docker Compose / npm): `docs/pipelines.md`

## 11) What’s not shipped yet

- `--tui` interactive mode (stubbed)
- Mermaid styling/classes (`style`, `classDef`, `:::`)
- Non-flowchart Mermaid diagram types (sequence/class/state/ER/…)

## 12) Tiny “how it works” (one sentence each)

- Parse: Mermaid-lite → graph model (nodes/edges/subgraphs)
- Layout: coarse layered placement + obstacle-aware routing
- Render: draw boxes/edges/labels onto a char canvas (handles overlaps/junctions)
