# TermiFlow Documentation

## 📚 Documentation Structure

### Core Documentation
- **[SPEC.md](./SPEC.md)** - Complete technical specification for Phase 1
- **[COMPOSITE_STYLES.md](./COMPOSITE_STYLES.md)** - Composite styling system guide
- **[../DEMO.md](../DEMO.md)** - Demo guide for hackweek presentation
- **[../README.md](../README.md)** - Project overview and usage

### Phase 2: Per-Element Styling
- **[PHASE2_PLAN.md](./PHASE2_PLAN.md)** - Architecture and design decisions
- **[PHASE2_IMPLEMENTATION.md](./PHASE2_IMPLEMENTATION.md)** - Implementation guide with code examples
- **[PHASE2_QUICK_REFERENCE.md](./PHASE2_QUICK_REFERENCE.md)** - User syntax guide
- **[PHASE2_SUMMARY.md](./PHASE2_SUMMARY.md)** - Executive summary

## 🚀 Current Status

### ✅ Phase 1: Complete
- Two-pass Mermaid parser with forward references
- Waterfall layout with cycle detection
- Multi-style rendering (9 styles: ASCII, Unicode, Double, Rounded, Heavy, Dots, Plus, Stars, Blocks)
- Composite styling system (mix & match components)
- 3-tier configuration system
- Arrow placement rules (vertical only)
- 44 passing tests

### 🔄 Phase 2: Planned
- Parse Mermaid's native style syntax
- Per-node and per-edge styling
- 100% Mermaid compatibility guaranteed
- ANSI color support

### 📋 Phase 3: Future
- Interactive TUI with ratatui
- Click target navigation
- Large graph optimizations

## 🏗️ Architecture Overview

```
Parser (Two-pass) → Layout (Topological) → Canvas (2D Grid) → Output
                                               ↑
                                          Style System
```

## 🎯 Key Design Principles

1. **Mermaid Compatibility First** - Every diagram must work in GitHub/Mermaid.live
2. **Progressive Enhancement** - Terminal features enhance, never break
3. **Performance** - <1ms parse time for 100+ nodes
4. **Extensibility** - Clean separation of concerns

## 📊 Test Coverage

| Component | Tests | Status |
|-----------|-------|--------|
| Parser | 20+ | ✅ Comprehensive |
| Layout | 5 | ✅ Basic coverage |
| Canvas | 3 | ⚠️ Minimal |
| Style | 8 | ✅ Good coverage |
| Integration | 10 | ✅ End-to-end |

## 🔍 Known Issues

1. Junction characters defined but not fully utilized at merge points
2. Some helper functions unused (kept for Phase 2)
3. `--debug-layout` flag mentioned but not implemented
4. Max-label affects display but not box width

## 🛠️ Development Setup

```bash
# Build
cargo build --release

# Test
cargo test

# Run
cargo run -- --print tests/fixtures/inputs/simple.md

# Benchmark
time cargo run -- --print tests/fixtures/inputs/large.md
```

## 📈 Performance Metrics

| Metric | Value | Notes |
|--------|-------|-------|
| Parse Time | <1ms | 100-node graphs |
| Memory Usage | O(n) | Linear scaling |
| Max Graph Size | 1000+ nodes | Tested successfully |
| Render Time | <5ms | Including canvas generation |

## 🤝 Contributing

See main README for contribution guidelines. Key areas for improvement:

1. Junction rendering at edge merge points
2. Canvas test coverage
3. TUI implementation (Phase 3)
4. Edge label support (Phase 2.5)

## 📝 Documentation Maintenance

When updating documentation:
1. Keep SPEC.md as source of truth for current implementation
2. Update DEMO.md for user-facing examples
3. Phase docs in `docs/` for future work
4. Run all examples to verify they work