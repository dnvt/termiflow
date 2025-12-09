```mermaid
graph TD
    R[render(graph, config)] --> SIZE[calc width/height + gutter]
    R --> CANVAS[Canvas::new(width,height)]
    R --> CHARS[chars := composite_style.to_style_chars]
    R --> VISIBLE[visible_nodes := filter is_visible]
    R --> GROUP[group edges\nforward by source, collect back-edges]
    GROUP --> EXPANDED[expanded forward edges\nsort sources/targets → route_expanded_edge]
    GROUP --> BACK[back-edges → route_back_edge]
    R --> DRAW[draw nodes → draw_node]

    subgraph Edge Routing
      EXPANDED --> STEM[stem → junction span → drops → arrows]
      BACK --> GUTTER[gutter routing on right margin]
    end

    subgraph Canvas & Helpers
      CANVAS --> SET[set/get/set_edge_char\n(overlap resolution)]
      EXPANDED --> EDGEHELP[center_x, corner_char, mid_y]
      DRAW --> SHAPES[shape dispatch:\nrectangle/rounded/diamond/etc.]
    end
```
