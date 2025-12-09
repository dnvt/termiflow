```mermaid
graph TD
    C[Canvas] --> CLASSIFY[character helpers\nis_horizontal/is_vertical/is_arrow\nis_corner/is_junction/is_box_char]
    C --> OVERLAP[resolve_overlap(existing,new)\n→ junctions/crosses/sacred chars preserved]
    C --> GRID[set/get/set_edge_char\n(uses resolve_overlap)]
    C --> VISIBLE[is_visible(node)]
    C --> DISPLAY[Display impl → to string]

    CLASSIFY --> CORNERDIR[corner dir helpers\nis_corner_up/down/left/right]
    OVERLAP --> RULES[Sacred: arrows/box chars/junctions\nCorner+line → junction\nPerpendicular → cross\nElse new wins]
```
