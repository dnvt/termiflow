# TermiFlow Planning & Documentation

Internal plans, specs, and status live here to keep `/docs` consumer-friendly.

## Active Documents

| Document | Purpose |
|----------|---------|
| **[PLAN.md](PLAN.md)** | Current development plan and work queue |
| [AUDIT-mermaid-parity.md](AUDIT-mermaid-parity.md) | Comprehensive Mermaid vs Termiflow feature gaps |
| [AUDIT-termiflow-limits.md](AUDIT-termiflow-limits.md) | Internal limits reference (canvas, labels, routing) |
| [RFC-001-expanded-edge-routing.md](RFC-001-expanded-edge-routing.md) | Edge routing algorithm (implemented) |
| [spec/SPEC.md](spec/SPEC.md) | Technical specification |

## Future Work (Deferred)

| Document | Purpose |
|----------|---------|
| [phase2/](phase2/) | Per-element styling spec (Mermaid `classDef`, `:::` support) |

## Archive

Historical documents kept for reference:

| Document | Status |
|----------|--------|
| [archive/ROADMAP.md](archive/ROADMAP.md) | Superseded by PLAN.md |
| [archive/FUTURE_WORK.md](archive/FUTURE_WORK.md) | Merged into PLAN.md backlog |
| [archive/LAYOUT_ROUTING_SPIKE.md](archive/LAYOUT_ROUTING_SPIKE.md) | Implemented |
| [archive/ROUTING_REVIEW.md](archive/ROUTING_REVIEW.md) | Completed |
| [archive/SUBGRAPH_MIGRATION.md](archive/SUBGRAPH_MIGRATION.md) | Implemented |

## Quick Start

```bash
# Run tests
cargo test

# Regenerate golden fixtures
cargo test --features golden -- --ignored

# Render all fixtures for manual review
./scripts/render_fixtures.sh --ascii --unicode
```

---

*Last updated: 2026-01-27*
