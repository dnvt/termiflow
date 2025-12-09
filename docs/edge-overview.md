```mermaid
graph TD
    ROOT[Edge Routing] --> EXP[route_expanded_edge]
    ROOT --> FWD[route_edge]
    ROOT --> BACK[route_back_edge]
    ROOT --> HELP[helpers]

    EXP --> FAN1[1 target:\nstraight or L\n(stem + span + drop + arrow)]
    EXP --> FANM[many targets:\nstem → span (junction) → drops → arrows\nspan covers src+targets to avoid disconnection]

    FWD --> CASES["cases:\n- straight (aligned, clear path)\n- reuse existing vertical\n- L-shaped with mid_y spread"]
    FWD --> BLOCK[blocked check vs other nodes\ntries adjacent x reuse]

    BACK --> GUTTER["draw in gutter:\nhoriz to right, vertical, arrow left"]

    HELP --> CENTER[center_x(node)]
    HELP --> MIDY[calculate_mid_y(edge_index)]
    HELP --> CORNER[corner_char(from_x,to_x,is_source)]
```
