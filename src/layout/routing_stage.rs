//! Occupancy preparation and pre-routing orchestration for coarse layout.

use std::collections::HashMap;

use crate::geom::EdgeRoute;
use crate::graph::Graph;
use crate::orientation::OrientedCoords;
use crate::portals::SubgraphEnvelope;

use super::layout_routing;
use super::placement::Placement;
use super::CoarseLayoutConfig;

/// Seed routing obstacles and pre-route edges that do not belong to the renderer.
///
/// Low-level grid and route algorithms remain in `layout_routing`; this module
/// owns only the stage sequencing and the local mutable occupancy state.
pub(super) fn route_stage(
    graph: &Graph,
    config: &CoarseLayoutConfig,
    coords: &OrientedCoords,
    placement: &Placement,
    subgraph_envelopes: &HashMap<String, SubgraphEnvelope>,
    debug_timing: bool,
) -> HashMap<usize, EdgeRoute> {
    // 4) Occupancy grid seeded with node padding and subgraph gutters (with carved portals).
    let t_grid = std::time::Instant::now();
    let mut grid = layout_routing::OccupancyGrid::new(
        placement.canvas.right()
            + config.min_horizontal_spacing
            + config.subgraph_gutter
            + config.min_horizontal_spacing,
        placement.canvas.bottom()
            + config.min_vertical_spacing
            + config.subgraph_gutter
            + config.min_vertical_spacing,
    );
    for rect in placement.node_rects.values() {
        grid.mark_rect(&rect.inflate(config.node_padding));
    }
    layout_routing::carve_node_portals(
        &mut grid,
        &placement.node_rects,
        coords,
        config.node_padding,
        graph,
        subgraph_envelopes,
    );
    // No additional carving for fan-outs; deterministic lanes are built during routing.
    layout_routing::mark_subgraph_rings(&mut grid, subgraph_envelopes);
    if config.enable_portals {
        layout_routing::carve_subgraph_portals(
            &mut grid,
            subgraph_envelopes,
            config.subgraph_gutter,
        );
    }
    if debug_timing {
        eprintln!(
            "termiflow: grid {:?} ({}x{})",
            t_grid.elapsed(),
            grid.width,
            grid.height
        );
    }

    // 5) Route edges with Manhattan + obstacle avoidance.
    let mut routes: HashMap<usize, EdgeRoute> = HashMap::new();
    let t_route = std::time::Instant::now();
    let mut outgoing_counts: HashMap<&str, usize> = HashMap::new();
    let mut incoming_counts: HashMap<&str, usize> = HashMap::new();
    for edge in graph.edges.iter().filter(|e| !e.is_back_edge) {
        *outgoing_counts.entry(edge.from.as_str()).or_default() += 1;
        *incoming_counts.entry(edge.to.as_str()).or_default() += 1;
    }
    layout_routing::route_selective_horizontal_cross_subgraph_fanin_groups(
        graph,
        &placement.node_rects,
        subgraph_envelopes,
        &incoming_counts,
        &mut routes,
        &mut grid,
    );
    for (edge_idx, edge) in graph.edges.iter().enumerate() {
        if edge.is_back_edge {
            // Skip routing here; back-edges are handled by the cycle renderer.
            continue;
        }
        if routes.contains_key(&edge_idx) {
            continue;
        }

        if debug_timing {
            eprintln!("termiflow: route edge {} -> {}", edge.from, edge.to);
        }
        let from_rect = placement
            .node_rects
            .get(&edge.from)
            .cloned()
            .unwrap_or_default();
        let to_rect = placement
            .node_rects
            .get(&edge.to)
            .cloned()
            .unwrap_or_default();

        let out_degree = outgoing_counts
            .get(edge.from.as_str())
            .copied()
            .unwrap_or(0);
        let in_degree = incoming_counts.get(edge.to.as_str()).copied().unwrap_or(0);

        // Convergent edges (multiple sources into one target) render best when the renderer
        // owns the junction, so skip pre-routing here.
        if in_degree > 1 {
            if debug_timing {
                eprintln!("  skip edge {edge_idx} due to convergent routing");
            }
            continue;
        }

        // Fan-outs look best when the renderer owns the shared junction.
        if out_degree > 1 {
            if debug_timing {
                eprintln!("  skip edge {edge_idx} fan-out handled in renderer");
            }
            continue;
        }

        // Labeled fan-out / fan-in edges are better handled in the renderer so labels
        // can sit on clean junctions instead of fighting precomputed paths.
        if edge.label.is_some() && (out_degree > 1 || in_degree > 1) {
            if debug_timing {
                eprintln!("  skip edge {edge_idx} labeled fan-out/fan-in");
            }
            continue;
        }

        let crosses_subgraph = graph.edge_crosses_subgraph_boundary(&edge.from, &edge.to);

        // Leave fan-out / fan-in edges that cross subgraph boundaries to the renderer so
        // they can share junctions cleanly instead of overlapping pre-routed lanes.
        if crosses_subgraph && (out_degree > 1 || in_degree > 1) {
            if debug_timing {
                eprintln!("  skip edge {edge_idx} cross-subgraph fan routing");
            }
            continue;
        }

        // Any edge that crosses a subgraph boundary is rendered with portal-aware logic;
        // skip pre-routing to avoid stale paths that don't honor portals.
        if crosses_subgraph {
            continue;
        }

        // Compute avoid gutters (all subgraphs except those containing endpoints).
        let avoid_rects = layout_routing::gutters_to_avoid(
            graph,
            subgraph_envelopes,
            edge_idx,
            &edge.from,
            &edge.to,
        );

        let from_sg = graph.get_node_subgraph(&edge.from);
        let to_sg = graph.get_node_subgraph(&edge.to);

        let start = layout_routing::edge_exit_point(from_rect, graph.direction);
        let end = layout_routing::edge_entry_point(to_rect, graph.direction);

        if debug_timing {
            eprintln!(
                "  start {:?} end {:?} avoid {}",
                start,
                end,
                avoid_rects.len()
            );
        }

        // Ensure endpoints are traversable even if padding or rings marked them as obstacles.
        grid.clear_point(start);
        grid.clear_point(end);

        // Deterministic fan-out / fan-in lanes for simple non-subgraph cases.
        if edge.label.is_none() {
            if let Some(route) = layout_routing::lane_route(
                start,
                end,
                from_rect,
                to_rect,
                graph.direction,
                out_degree,
                in_degree,
                config.node_padding.max(1),
            ) {
                grid.mark_path(&route);
                if debug_timing {
                    eprintln!("  lane route stored for edge {edge_idx}");
                }
                routes.insert(edge_idx, route);
                continue;
            }
        }

        // Build waypoints: start → (portal exit?) → (portal enter?) → end.
        let mut checkpoints = vec![start];
        if config.enable_portals && from_sg != to_sg {
            if let Some(id) = from_sg {
                if let Some(env) = subgraph_envelopes.get(id) {
                    if let Some(p) = layout_routing::portal_point(
                        env,
                        layout_routing::PortalUse::Exit,
                        graph.direction,
                    ) {
                        checkpoints.push(p);
                        grid.clear_point(p);
                    }
                }
            }
            if let Some(id) = to_sg {
                if let Some(env) = subgraph_envelopes.get(id) {
                    if let Some(p) = layout_routing::portal_point(
                        env,
                        layout_routing::PortalUse::Enter,
                        graph.direction,
                    ) {
                        checkpoints.push(p);
                        grid.clear_point(p);
                    }
                }
            }
        }
        checkpoints.push(end);

        let mut combined = EdgeRoute::new();
        for pair in checkpoints.windows(2) {
            let (seg_start, seg_end) = (pair[0], pair[1]);
            if let Some(route) = layout_routing::route_with_obstacles_v2(
                seg_start,
                seg_end,
                &mut grid,
                &avoid_rects,
                coords,
            ) {
                grid.mark_path(&route);
                for s in route.segments {
                    combined.push_segment(s.from, s.to);
                }
            } else {
                let route =
                    layout_routing::fallback_manhattan_route(seg_start, seg_end, graph.direction);
                grid.mark_path(&route);
                for s in route.segments {
                    combined.push_segment(s.from, s.to);
                }
            }
        }

        if debug_timing {
            eprintln!(
                "  stored route {} with {} segments (checkpoints={})",
                edge_idx,
                combined.segments.len(),
                checkpoints.len()
            );
        }
        routes.insert(edge_idx, combined);
    }
    if debug_timing {
        eprintln!(
            "termiflow: routing {:?} ({} edges)",
            t_route.elapsed(),
            graph.edges.len()
        );
        eprintln!("termiflow: stored routes {}", routes.len());
    }

    routes
}
