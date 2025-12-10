# Layout & Routing Spike (Draft)

Branch: `feat/layout-routing-spike` (based on `feat/mergin-subgraphs`)

## Why
- Current waterfall layout is deterministic but tightly coupled to edge routing and spacing constants; subgraphs add another axis of complexity.
- We need a clearer geometry model, spacing rules, and collision handling so box positioning, connections, and subgraph bounds stay predictable across directions.
- Goal: a small, composable pipeline we can iterate on (first a simple grid/layered approach, then smarter routing/collision handling).

## Working Assumptions
- Keep direction-agnostic math via `OrientedCoords`.
- Single-level subgraphs for now; nested is out of scope.
- ASCII and Unicode must both stay legible; do not regress compactness too much.
- Determinism > perfection. Stability across edits is a key UX requirement.

## Architecture Sketch
- **Geometry model**
  - `Rect { center, size }` for nodes and subgraphs; `Port { offset, side }` for connection points.
  - `EdgeRoute` as a list of oriented segments (grid-aligned first, optional smoothing later).
  - Spatial index (quadtree or uniform grid) keyed by inflated rects for spacing/collision tests.
- **Pipeline**
  1) **Measure** nodes/subgraphs (text width, padding, label space).
  2) **Layer** (Sugiyama/longest-path) with rank reuse for cycles.
  3) **Order** within layer (median heuristic; keep user-locked nodes fixed when present).
  4) **Place** on a coarse grid: primary axis by layer index; secondary axis by ordered slot; apply min col/row spacing and subgraph gutters.
  5) **Route edges** separately: Manhattan on the coarse grid with obstacles from inflated rects; prefer V-then-H (or H-then-V) based on direction; allow short doglegs around occupied cells; round corners at render time.
  6) **Tidy**: local overlap pushes within a layer, then re-route only affected edges; optional edge bundling per layer.
  7) **Stabilize**: reuse previous coordinates when available; only relax locally after edits.
- **Subgraphs**
  - Bounds from member rects + padding + title band.
  - Entry/exit ports on the primary edge (direction-aware); route internal edges first, then external edges that target entry/exit bands as obstacles.
  - Enforce a gutter between subgraph bounds and external nodes/edges.
- **Spacing & collisions**
  - Inflate rects by padding + min edge clearance before collision checks.
  - Broadphase via quadtree; narrow phase resolves overlaps with small secondary-axis pushes (stay within layer to keep rank semantics).
  - Keep a cheap grid occupancy map for routing; reroute edges that cross newly occupied cells.
- **Labels/ports**
  - Reserve space for edge labels on the first horizontal/vertical span after the source stem.
  - Allow port hints (top/left/right/bottom) to bias entry/exit side when we add port syntax.

## Minimal Prototype Plan
1) Extract/define `Rect`, `Port`, `EdgeRoute` types in `graph` or a `geom` module; keep adapters to existing `Node`/`Edge`.
2) Add a coarse grid allocator that maps layer/slot to coordinates using existing spacing constants; emit rects and occupancy map.
3) Implement Manhattan router with obstacle awareness on the occupancy grid; start with V-then-H (primary then secondary) and dogleg fallback.
4) Add overlap resolution within a layer (secondary-axis pushes) and reroute affected edges.
5) Integrate subgraph bounds, entry/exit bands, and gutters; reroute external edges against those obstacles.
6) Add stability hooks: optional prior positions in `Graph`, lockable nodes, and minimal-move heuristics.
7) Tests: golden layouts for TD/LR/BT/RL with and without subgraphs; focused unit tests for router detours and collision pushes.

## API Sketch (direction-agnostic)
```rust
pub struct LayoutInput<'a> {
    pub graph: &'a Graph,
    pub prior_positions: Option<HashMap<NodeId, Point>>,
}

pub struct LayoutOutput {
    pub positions: HashMap<NodeId, Point>,
    pub subgraph_bounds: HashMap<SubgraphId, Rect>,
    pub routes: HashMap<EdgeId, EdgeRoute>,
    pub warnings: Vec<String>,
}

pub trait LayoutEngine {
    fn layout(&self, input: LayoutInput) -> Result<LayoutOutput>;
}
```

## Risks / Open Questions
- Router complexity: A* on a fine grid could explode; start coarse and only refine when congested.
- Stability vs. compactness: Keeping prior positions may lock in suboptimal spacing; need heuristics to cap drift.
- Subgraph gutters: How much padding is enough to keep labels and edges from colliding without over-expanding?
- Performance: Spatial index adds overhead; ensure O(n log n) with modest constants for typical diagrams.
