# Subgraph Migration Analysis

> Historical note: subgraph support has since been implemented on the direction-agnostic codebase.
> Use this document as background context only; it no longer reflects current `main`.

## Executive Summary

Subgraph support is now implemented end-to-end (parse → layout → render), including:
- Parsing `subgraph ... end` blocks (single-level; nested warns/ignored).
- Subgraph bounds + gutters in layout.
- Portal-aware border piercing for cross-subgraph edges in render.

Current implementation touchpoints:
- `src/graph.rs` (`Subgraph`, `Rectangle`, `Graph::subgraphs`, `Graph::node_subgraph`)
- `src/parser.rs` (subgraph parsing + nested warnings)
- `src/layout.rs` (subgraph envelopes + gutters + portal carving in occupancy grid)
- `src/portals.rs` (shared envelope/portal helpers)
- `src/render/mod.rs`, `src/render/edge.rs` (portal carving + cross-subgraph routing)

## Branch Comparison

### Current `main` Has (feat/subgraphs Lacks)
- `src/orientation.rs` - Direction-agnostic coordinate abstraction
- `src/render/cycle.rs` - Extracted cycle edge routing
- `src/render/shapes.rs` - Extracted shape drawing (455 lines)
- `Direction::RL` support (Right-to-Left)
- Compact spacing constants (MINIMAL, LABELED, FANOUT, FANIN)
- `OrientedCoords` methods: `advance()`, `retreat()`, `with_secondary()`
- `Node::center_x()` and `Node::center_y()` methods
- 50 golden tests covering all 4 directions (TD, LR, BT, RL)
- Performance benchmarks

### `feat/subgraphs` Has (main Lacks)
- `Subgraph` struct with bounds calculation
- `Rectangle` struct for bounding boxes
- Subgraph parsing (`subgraph ID [title]` syntax)
- `hierarchical_waterfall()` layout algorithm
- Subgraph entry/exit point calculation for edge routing
- Cross-subgraph edge routing with proper junction placement
- `draw_subgraph()` with dashed borders
- `enable_subgraphs` config option

## Technical Audit

### 1. Data Structures (graph.rs)

**Additions needed:**
```rust
/// Rectangle for bounding boxes
#[derive(Debug, Clone, Default)]
pub struct Rectangle {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

/// Subgraph grouping nodes together (single-level only)
#[derive(Debug, Clone)]
pub struct Subgraph {
    pub id: String,
    pub title: Option<String>,
    pub node_ids: HashSet<String>,
    pub bounds: Rectangle,
    pub rank_range: (usize, usize),
}

// Graph additions:
pub subgraphs: Vec<Subgraph>,
pub node_subgraph: HashMap<String, String>,
```

**Assessment:** Clean addition, no conflicts. Can port directly.

### 2. Parser (parser.rs)

**Additions needed:**
- `RE_SUBGRAPH_BRACKET` regex for `subgraph ID [title]`
- `RE_SUBGRAPH_PLAIN` regex for `subgraph title`
- `RE_SUBGRAPH_END` regex for `end`
- `current_subgraph` tracking during parse
- `parse_with_config()` with `enable_subgraphs` parameter

**Assessment:** Moderate complexity. Parser changes are additive. Need to add RL direction regex (already in main).

### 3. Layout (layout.rs)

**Current feat/subgraphs approach:**
```
hierarchical_waterfall() - 1000+ lines
├── Phase 1: Measurement
│   ├── calculate_node_ranks()
│   ├── calculate_subgraph_metrics()
│   └── find_subgraph_connections()
├── Phase 2: Positioning
│   ├── group_subgraphs_into_columns()
│   ├── Rank-aware Y offsets for subgraph titles
│   └── Subgraph-grouped node positioning
└── Phase 3: Bounds
    └── calculate_subgraph_bounds()
```

**Issues:**
1. **TD/LR only** - No BT/RL support
2. **Hardcoded Y calculations** - Assumes vertical primary axis
3. **Duplicates waterfall()** - Shares 200+ lines of rank calculation
4. **No OrientedCoords usage** - Direction-specific coordinate math

**Migration Strategy:**
- Extract shared rank calculation into reusable function
- Use `OrientedCoords` for direction-agnostic positioning
- Parameterize "Y offset" as "primary offset" using abstraction
- Add BT/RL coordinate transformations

### 4. Edge Routing (render/edge.rs)

**Current feat/subgraphs approach:**
```rust
route_expanded_edge(
    from, to_nodes, canvas, style,
    subgraph_entry_y,  // Where arrows enter subgraph
    subgraph_exit_y,   // Where stems exit subgraph
)
```

**Issues:**
1. **TD-specific** - Uses `y` coordinates, assumes vertical flow
2. **Separate functions** - `route_external_edge_from_exit()` for cross-subgraph
3. **No convergent handling** - Only divergent (1→N) routing

**Our Current Architecture:**
```rust
// render/edge.rs - Direction-agnostic
route_divergent_edges(from, to_nodes, canvas, style, direction)
route_convergent_edges(from_nodes, to, canvas, style, direction)
// Uses OrientedCoords for all coordinate calculations
```

**Migration Strategy:**
- Add `subgraph_entry: HashMap<String, usize>` and `subgraph_exit: HashMap<String, usize>` parameters
- Convert Y-based calculations to direction-agnostic using OrientedCoords
- Make entry/exit calculation direction-aware (entry = primary axis entry, exit = primary axis exit)

### 5. Rendering (render/mod.rs)

**Current feat/subgraphs approach:**
```rust
// Draw order:
1. compute_subgraph_entry_points()
2. compute_subgraph_exit_points()
3. Route internal edges (same subgraph)
4. Route external edges (cross subgraph)
5. draw_subgraph() - dashed borders
6. draw_node() - solid boxes
```

**Issues:**
1. **TD-specific entry/exit** - Assumes top entry, bottom exit
2. **No direction abstraction** - Hardcoded coordinate checks
3. **Separate internal/external routing** - Complex branching logic

**Migration Strategy:**
- Make entry/exit point calculation direction-aware:
  - TD: entry at top (y), exit at bottom
  - LR: entry at left (x), exit at right
  - BT: entry at bottom, exit at top
  - RL: entry at right, exit at left
- Use `OrientedCoords::primary_coord()` instead of `.y`
- Integrate with existing render pipeline

## Migration Plan

### Phase 1: Data Structures (Low Risk)
1. Add `Rectangle` struct to `graph.rs`
2. Add `Subgraph` struct to `graph.rs`
3. Add `subgraphs` and `node_subgraph` to `Graph`
4. Add `enable_subgraphs` to `Config`

### Phase 2: Parser (Medium Risk)
1. Add subgraph regexes
2. Add `current_subgraph` tracking
3. Implement subgraph parsing in pass 2
4. Add nested subgraph warning

### Phase 3: Layout (High Complexity)
1. Extract shared rank calculation from `waterfall()`
2. Create direction-agnostic `SubgraphMetrics` calculation
3. Implement `hierarchical_waterfall()` using `OrientedCoords`
4. Handle subgraph spacing for all 4 directions

### Phase 4: Edge Routing (High Complexity)
1. Add entry/exit point parameters to edge routing
2. Make `compute_subgraph_entry_points()` direction-aware
3. Make `compute_subgraph_exit_points()` direction-aware
4. Integrate with `route_divergent_edges()` and `route_convergent_edges()`

### Phase 5: Rendering (Medium Risk)
1. Add `draw_subgraph()` function
2. Make border drawing direction-aware
3. Add title positioning based on direction
4. Update render pipeline order

### Phase 6: Testing (Required)
1. Create subgraph fixtures for all 4 directions
2. Add golden tests for:
   - `subgraph_basic_td/lr/bt/rl`
   - `subgraph_complex_td/lr/bt/rl`
   - `subgraph_cross_edges_td/lr/bt/rl`

## Key Files to Modify

| File | Changes | Risk |
|------|---------|------|
| `src/graph.rs` | Add Subgraph, Rectangle | Low |
| `src/config.rs` | Add enable_subgraphs | Low |
| `src/parser.rs` | Add subgraph parsing | Medium |
| `src/layout.rs` | Add hierarchical_waterfall | High |
| `src/render/edge.rs` | Add entry/exit handling | High |
| `src/render/mod.rs` | Add draw_subgraph, update pipeline | Medium |
| `src/orientation.rs` | May need new methods | Low |

## Estimated Effort

- Phase 1: 1 hour (data structures)
- Phase 2: 2 hours (parser)
- Phase 3: 4-6 hours (layout - most complex)
- Phase 4: 3-4 hours (edge routing)
- Phase 5: 2 hours (rendering)
- Phase 6: 2 hours (testing)

**Total: 14-17 hours**

## Recommendation

**Do NOT merge feat/subgraphs directly.** Instead:

1. Cherry-pick specific commits for data structures and parser only
2. Reimplement layout algorithm using our OrientedCoords abstraction
3. Reimplement edge routing to use direction-agnostic approach
4. Build comprehensive tests for all 4 directions before merge

The subgraph feature is valuable, but the implementation needs significant refactoring to match our improved architecture.
