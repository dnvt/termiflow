# /debug - Debug Rendering Issues

Diagnose rendering problems with detailed debug output.

## Arguments
- `$ARGUMENTS` - Required: file path or inline diagram

## Instructions

### 1. Enable Debug Environment Variables
```bash
export TERMIFLOW_DEBUG_TIMING=1
export TERMIFLOW_DEBUG_ROUTES=1
```

### 2. Run with Debug Output
```bash
TERMIFLOW_DEBUG_TIMING=1 TERMIFLOW_DEBUG_ROUTES=1 cargo run --bin tw -- "$ARGUMENTS" 2>&1
```

### 3. Check Parser Output
Test parsing in isolation:
```bash
cargo test parser -- --nocapture 2>&1 | head -50
```

### 4. Isolate the Issue

**If edges are wrong:**
- Check `src/render/edge.rs` for routing logic
- Look at `route_divergent_edges()` or `route_convergent_edges()`

**If boxes are wrong:**
- Check `src/render/shapes.rs` for box drawing
- Look at junction placement logic

**If layout is wrong:**
- Check `src/layout.rs` for positioning
- Look at `coarse_waterfall()` and layer assignment

**If style is wrong:**
- Check `src/style.rs` for character mappings
- Verify `CompositeStyle::parse()` for mixed styles

### 5. Debug Flags Reference

| Env Variable | Purpose |
|--------------|---------|
| `TERMIFLOW_DEBUG_TIMING=1` | Print timing stats to stderr |
| `TERMIFLOW_DEBUG_ROUTES=1` | Dump computed edge routes to stderr |
| `TERMIFLOW_DISABLE_PORTALS=1` | Disable subgraph border piercing |

### 6. Common Issues Checklist

- [ ] **Arrows pointing wrong way**: Check `OrientedCoords` direction in `orientation.rs`
- [ ] **Missing junctions**: Check `resolve_overlap()` in `canvas.rs`
- [ ] **Edges not connecting**: Check stem length constants in `edge.rs`
- [ ] **Labels overlapping**: Check `draw_edge_label()` positioning
- [ ] **Subgraph borders broken**: Check portal carving in `portals.rs`
