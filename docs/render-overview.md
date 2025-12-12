```mermaid
graph TD
    R[render(graph, config)] --> SIZE[calc width/height + gutter]
    R --> CANVAS[Canvas::new(width,height)]
    R --> CHARS[chars := composite_style.to_style_chars]
    R --> SUBGRAPHS[draw subgraph borders + titles]
    SUBGRAPHS --> PORTALS[collect portal slots\n+ carve openings (optional)]
    R --> VISIBLE[visible_nodes := filter is_visible]
    R --> GROUP[group edges\nforward by source, collect back-edges]
    GROUP --> PRE[draw precomputed routes\n(Graph.edge_routes)]
    GROUP --> CONVERGE[route_convergent_edges\n(N→1)]
    GROUP --> DIVERGE[route_divergent_edges\n(1→N, incl cross-subgraph)]
    GROUP --> BACK[back-edges → route_back_edge]
    R --> DRAW[draw nodes → draw_node]
    R --> REINFORCE[reinforce portal piercings\n(after edges + boxes)]

    subgraph Edge Routing
      CONVERGE --> STEM[stem → junction span → drops → arrows]
      DIVERGE --> STEM
      BACK --> GUTTER[gutter routing on right margin]
    end

    subgraph Canvas & Helpers
      CANVAS --> SET[set/get/set_edge_char\n(overlap resolution)]
      DIVERGE --> EDGEHELP[OrientedCoords helpers\ncenter_x/center_y, corners, spans]
      DRAW --> SHAPES[shape dispatch:\nrectangle/rounded/diamond/etc.]
    end
```
