```mermaid
graph TD
    ROOT[Edge Routing] --> PRE[draw precomputed routes\n(Graph.edge_routes)]
    ROOT --> DIV[route_divergent_edges\n(1→N)]
    ROOT --> CONV[route_convergent_edges\n(N→1)]
    ROOT --> BACK[route_cycle_edge\n(back-edges)]
    ROOT --> HELP[helpers]

    DIV --> ONE[1 target:\nstraight or L\n(stem + turn + arrow)]
    DIV --> MANY[many targets:\nstem → span (junction) → drops → arrows]
    DIV --> XSG[cross-subgraph:\nportal-aware border piercing\n(TD/LR/BT/RL)]

    CONV --> MERGE[many sources:\nstems → shared junction → arrow]

    BACK --> GUTTER["draw in gutter:\nroute through reserved margin\n(skip if clipped)"]

    HELP --> ORIENT[OrientedCoords\n(primary/secondary axes)]
    HELP --> OVERLAP[Canvas::set_edge_char\n(overlap → junction/cross)]
```
