# Layout & Routing Spike (Implemented)

Branch: `feat/layout-routing-spike` (based on `feat/mergin-subgraphs`)

## Status
**Implemented.** This “coarse layout” strategy is now the default engine (used by `layout::coarse_waterfall`) and supports subgraphs, portal-aware crossings, centering, and all four orientations (TD, LR, BT, RL).

## Why
- The legacy waterfall layout was deterministic but tightly coupled to edge routing and spacing constants; subgraphs added another axis of complexity it couldn't handle.
- We needed a clearer geometry model, strict spacing rules, and collision handling so box positioning, connections, and subgraph bounds stay predictable across directions.
- Goal achieved: a composable pipeline with an explicit geometry model and A* grid routing.

## Architecture
- **Geometry model**
  - `Rect { x, y, width, height }` for nodes and subgraphs.
  - `EdgeRoute` as a list of orthogonal `Segment`s.
  - `OccupancyGrid` for collision detection and pathfinding.
- **Pipeline**
  1) **Measure** nodes (text width, padding).
  2) **Layer** (Sugiyama/longest-path) with rank reuse for cycles.
  3) **Place** on a coarse grid: primary axis by layer index; secondary axis by slot.
  4) **Center** layers along the secondary axis to align the diagram visually.
  5) **Flip** coordinates for BT/RL orientations to match flow direction.
  6) **Shift** layout to accommodate subgraph gutters if present.
  7) **Route edges** using Manhattan A* on the occupancy grid with obstacles from inflated rects and subgraph borders.
     - Note: the renderer intentionally owns fan-in/fan-out junction aesthetics and cross-subgraph portal piercing; the layout may leave those edges unrouted.
- **Subgraphs**
  - Bounds calculated from member rects + padding + title band.
  - Subgraphs are drawn with heavy borders and titles.
  - "Portals" are carved into the occupancy grid to allow edges to enter/exit nodes and subgraphs.

## Debugging

- `TERMIFLOW_DISABLE_PORTALS=1` disables portal carving (layout + render) for debugging border artifacts.

## API (direction-agnostic)
```rust
pub struct LayoutInput<'a> {
    pub graph: &'a Graph,
    pub prior_positions: Option<HashMap<String, Point>>,
}

pub struct LayoutOutput {
    pub positions: HashMap<String, Point>,
    pub subgraph_bounds: HashMap<String, SubgraphBounds>,
    pub routes: HashMap<usize, EdgeRoute>,
    pub canvas: Rect,
    pub warnings: Vec<String>,
}

pub fn layout(input: LayoutInput, config: CoarseLayoutConfig) -> Result<LayoutOutput>;
```

## Risks / Open Questions (Resolved)
- **Router complexity:** Resolved by using a coarse grid (character-level but integer coordinates) and A*. Performance is acceptable for typical terminal diagrams.
- **Subgraph gutters:** Implemented with `subgraph_gutter` config (default: 2). Nodes are shifted to make room for the border.
- **Artifacts:** "Corner stomping" artifacts (`├┬`) were resolved by making the renderer aware of pre-existing corners when drawing segments.
- **Orientation:** BT and RL are handled via coordinate flipping at the end of the layout phase, allowing the core logic to assume Top-Down/Left-Right flow.
