use super::envelope_stage::resolve_subgraph_envelopes;
use super::normalization::normalize_orientation_and_gutters;
use super::routing_stage::route_stage;
use super::*;

pub fn layout(input: LayoutInput, config: CoarseLayoutConfig) -> Result<LayoutOutput> {
    let coords = OrientedCoords::new(input.graph.direction);
    let debug_timing = std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok();

    // 1) Layer assignment (lenient Kahn) and ordering.
    let t_layers = std::time::Instant::now();
    let mut layers = assign_layers(input.graph);

    // 1.5) Optimize layer order to minimize crossings (adaptive algorithm with convergence)
    let minimizer = CrossingMinimizer::new();
    let final_crossings = minimizer.minimize(input.graph, &mut layers);
    if debug_timing {
        eprintln!(
            "termiflow: layers {:?} ({} layers, {} crossings)",
            t_layers.elapsed(),
            layers.len(),
            final_crossings
        );
    }

    // 2) Place nodes on coarse grid.
    let t_place = std::time::Instant::now();
    let mut placement = place_nodes(
        input.graph,
        &layers,
        &coords,
        &config,
        input.prior_positions.as_ref(),
    );
    if debug_timing {
        eprintln!(
            "termiflow: placement {:?} (canvas {}x{})",
            t_place.elapsed(),
            placement.canvas.width,
            placement.canvas.height
        );
    }

    // 2.25) Resolve horizontal subgraph overlaps for LR/RL before flipping coordinates.
    if matches!(input.graph.direction, Direction::LR | Direction::RL)
        && !input.graph.subgraphs.is_empty()
    {
        for _ in 0..8 {
            let mut required_shift_by_id: HashMap<String, usize> = HashMap::new();

            let mut subgraph_min_rank: HashMap<&str, usize> = HashMap::new();
            for sg in &input.graph.subgraphs {
                let min_rank = subgraph_tree_rank_range(input.graph, &placement.ranks, &sg.id)
                    .map(|(min_rank, _)| min_rank);
                if let Some(r) = min_rank {
                    subgraph_min_rank.insert(sg.id.as_str(), r);
                }
            }

            let envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);

            let mut sg_ids: Vec<&String> = envelopes.keys().collect();
            sg_ids.sort_unstable_by(|left, right| left.as_str().cmp(right.as_str()));
            for i in 0..sg_ids.len() {
                for j in (i + 1)..sg_ids.len() {
                    let env1 = &envelopes[sg_ids[i]];
                    let env2 = &envelopes[sg_ids[j]];
                    let intersects = env1.outer.x < env2.outer.right()
                        && env1.outer.right() > env2.outer.x
                        && env1.outer.y < env2.outer.bottom()
                        && env1.outer.bottom() > env2.outer.y;
                    if !intersects {
                        continue;
                    }
                    let nested = rect_fully_inside(env1.outer, env2.outer)
                        || rect_fully_inside(env2.outer, env1.outer);
                    if nested
                        && subgraphs_have_declared_hierarchy(
                            input.graph,
                            sg_ids[i].as_str(),
                            sg_ids[j].as_str(),
                        )
                    {
                        continue;
                    }

                    let r1 = subgraph_min_rank.get(sg_ids[i].as_str()).copied();
                    let r2 = subgraph_min_rank.get(sg_ids[j].as_str()).copied();
                    let (Some(rank1), Some(rank2)) = (r1, r2) else {
                        continue;
                    };
                    // Shift the later-ranked subgraph to the right until it clears the earlier one.
                    let (late_id, early_env, late_env) = if rank1 <= rank2 {
                        (sg_ids[j].as_str(), env1, env2)
                    } else {
                        (sg_ids[i].as_str(), env2, env1)
                    };

                    let required_left = early_env.outer.right().saturating_add(1);
                    if late_env.outer.x < required_left {
                        let delta = required_left - late_env.outer.x;
                        required_shift_by_id
                            .entry(late_id.to_string())
                            .and_modify(|d| *d = (*d).max(delta))
                            .or_insert(delta);
                    }
                }
            }

            let Some((late_id, delta_x)) = required_shift_by_id
                .iter()
                .max_by(|(left_id, left_delta), (right_id, right_delta)| {
                    left_delta
                        .cmp(right_delta)
                        .then_with(|| right_id.cmp(left_id))
                })
                .map(|(id, delta)| (id.clone(), *delta))
            else {
                break;
            };

            shift_nodes_in_subgraph(
                input.graph,
                &mut placement.positions,
                &mut placement.node_rects,
                &late_id,
                delta_x,
            );
        }
    }

    // 2.26) Resolve vertical subgraph overlaps for TD/BT
    if matches!(
        input.graph.direction,
        Direction::TD | Direction::TB | Direction::BT
    ) && !input.graph.subgraphs.is_empty()
    {
        for _ in 0..8 {
            let envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            let mut shifts: HashMap<String, usize> = HashMap::new();

            // Compute minimum rank for each subgraph to determine "earlier" vs "later"
            let mut subgraph_min_rank: HashMap<&str, usize> = HashMap::new();
            for sg in &input.graph.subgraphs {
                let min_rank = subgraph_tree_rank_range(input.graph, &placement.ranks, &sg.id)
                    .map(|(min_rank, _)| min_rank);
                if let Some(r) = min_rank {
                    subgraph_min_rank.insert(sg.id.as_str(), r);
                }
            }

            // Check all sibling pairs for vertical overlap
            let mut sg_ids: Vec<&String> = envelopes.keys().collect();
            sg_ids.sort_unstable_by(|left, right| left.as_str().cmp(right.as_str()));
            for i in 0..sg_ids.len() {
                for j in (i + 1)..sg_ids.len() {
                    let env1 = &envelopes[sg_ids[i]];
                    let env2 = &envelopes[sg_ids[j]];

                    // Must overlap horizontally to collide vertically
                    let h_overlap =
                        env1.outer.x < env2.outer.right() && env2.outer.x < env1.outer.right();
                    let v_overlap =
                        env1.outer.y < env2.outer.bottom() && env2.outer.y < env1.outer.bottom();

                    if !h_overlap || !v_overlap {
                        continue;
                    }

                    // Skip nested subgraphs
                    let nested = rect_fully_inside(env1.outer, env2.outer)
                        || rect_fully_inside(env2.outer, env1.outer);
                    if nested
                        && subgraphs_have_declared_hierarchy(
                            input.graph,
                            sg_ids[i].as_str(),
                            sg_ids[j].as_str(),
                        )
                    {
                        continue;
                    }

                    // Determine which subgraph is "later" (higher rank = drawn later)
                    let r1 = subgraph_min_rank.get(sg_ids[i].as_str()).copied();
                    let r2 = subgraph_min_rank.get(sg_ids[j].as_str()).copied();
                    let (Some(rank1), Some(rank2)) = (r1, r2) else {
                        continue;
                    };

                    // Shift the later-ranked subgraph down until it clears the earlier one
                    let (late_id, early_env, late_env) = if rank1 <= rank2 {
                        (sg_ids[j].as_str(), env1, env2)
                    } else {
                        (sg_ids[i].as_str(), env2, env1)
                    };

                    let required_top = early_env.outer.bottom().saturating_add(1);
                    if late_env.outer.y < required_top {
                        let delta = required_top - late_env.outer.y;
                        shifts
                            .entry(late_id.to_string())
                            .and_modify(|d| *d = (*d).max(delta))
                            .or_insert(delta);
                    }
                }
            }

            let Some((sg_id, delta)) = shifts
                .iter()
                .max_by(|(left_id, left_delta), (right_id, right_delta)| {
                    left_delta
                        .cmp(right_delta)
                        .then_with(|| right_id.cmp(left_id))
                })
                .map(|(id, d)| (id.clone(), *d))
            else {
                break;
            };

            // Shift all nodes in the subgraph down
            shift_nodes_in_subgraph_y(
                input.graph,
                &mut placement.positions,
                &mut placement.node_rects,
                &sg_id,
                delta,
            );
        }
    }

    normalize_orientation_and_gutters(input.graph, &config, &mut placement);
    let subgraph_envelopes =
        resolve_subgraph_envelopes(&input, &config, &mut placement, debug_timing);

    let routes = route_stage(
        input.graph,
        &config,
        &coords,
        &placement,
        &subgraph_envelopes,
        debug_timing,
    );
    let warnings = Vec::new();

    Ok(LayoutOutput {
        positions: placement.positions,
        subgraph_envelopes,
        routes,
        canvas: placement.canvas,
        warnings,
        ranks: placement.ranks,
    })
}
