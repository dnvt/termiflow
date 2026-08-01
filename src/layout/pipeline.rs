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

    // 2.5) Flip coordinates for BT/RL to match flow direction
    // Calculate strict content bounds
    let max_x = placement
        .node_rects
        .values()
        .map(|r| r.right())
        .max()
        .unwrap_or(0);
    let max_y = placement
        .node_rects
        .values()
        .map(|r| r.bottom())
        .max()
        .unwrap_or(0);

    if input.graph.direction == Direction::BT {
        for (id, p) in placement.positions.iter_mut() {
            let h = placement
                .node_rects
                .get(id)
                .map(|r| r.height)
                .unwrap_or(BOX_HEIGHT);
            p.y = max_y.saturating_sub(p.y).saturating_sub(h);
        }
        for r in placement.node_rects.values_mut() {
            r.y = max_y.saturating_sub(r.y).saturating_sub(r.height);
        }
    } else if input.graph.direction == Direction::RL {
        // Easier: Iterate keys of positions (node ids)
        for (id, p) in placement.positions.iter_mut() {
            if let Some(r) = placement.node_rects.get_mut(id) {
                let new_x = max_x.saturating_sub(r.x + r.width);
                p.x = new_x;
                r.x = new_x;
            }
        }
    }

    // Shift nodes to make room for subgraph gutters if any subgraphs exist
    if !input.graph.subgraphs.is_empty() {
        let shift = config.subgraph_gutter;
        for p in placement.positions.values_mut() {
            p.x += shift;
            p.y += shift;
        }
        for r in placement.node_rects.values_mut() {
            r.x += shift;
            r.y += shift;
        }
        // Canvas grows by the shift amount (padding on both sides)
        placement.canvas.width = max_x + shift * 2;
        placement.canvas.height = max_y + shift * 2;
    } else {
        // Tighten canvas to content if no subgraphs (optional, but cleaner)
        placement.canvas.width = max_x;
        placement.canvas.height = max_y;
    }

    reserve_nested_horizontal_subgraph_headroom(
        input.graph,
        &mut placement.positions,
        &mut placement.node_rects,
        config.subgraph_gutter,
        &mut placement.canvas.height,
    );

    if matches!(input.graph.direction, Direction::LR | Direction::RL)
        && !input.graph.subgraphs.is_empty()
    {
        for _ in 0..8 {
            let envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            let mut required_env_shift: Option<(usize, usize)> = None;
            let mut external_node_shifts: HashMap<String, usize> = HashMap::new();

            for (subgraph_id, env) in &envelopes {
                for edge in input.graph.edges.iter().filter(|edge| !edge.is_back_edge) {
                    let (Some(from_rect), Some(to_rect)) = (
                        placement.node_rects.get(&edge.from),
                        placement.node_rects.get(&edge.to),
                    ) else {
                        continue;
                    };

                    let from_inside_tree = input
                        .graph
                        .is_node_in_subgraph_tree(&edge.from, subgraph_id);
                    let to_inside_tree =
                        input.graph.is_node_in_subgraph_tree(&edge.to, subgraph_id);
                    if from_inside_tree == to_inside_tree {
                        continue;
                    }

                    let inside_rect = if from_inside_tree {
                        *from_rect
                    } else {
                        *to_rect
                    };
                    if !rect_fully_inside(env.outer, inside_rect) {
                        continue;
                    }

                    let external_rect = if from_inside_tree {
                        *to_rect
                    } else {
                        *from_rect
                    };
                    let external_is_subgraph = if from_inside_tree {
                        input.graph.get_node_subgraph(&edge.to).is_some()
                    } else {
                        input.graph.get_node_subgraph(&edge.from).is_some()
                    };
                    if external_is_subgraph {
                        continue;
                    }

                    if external_rect.x < env.outer.x {
                        let overlaps_left_wall = external_rect.right() > env.outer.x;
                        if overlaps_left_wall {
                            let required_env_x = external_rect.right().saturating_add(2);
                            let threshold_x = env.outer.x;
                            let delta_x = required_env_x - env.outer.x;
                            match required_env_shift {
                                Some((best_x, best_delta)) => {
                                    if threshold_x < best_x
                                        || (threshold_x == best_x && delta_x > best_delta)
                                    {
                                        required_env_shift = Some((threshold_x, delta_x));
                                    }
                                }
                                None => required_env_shift = Some((threshold_x, delta_x)),
                            }
                        }
                    } else {
                        let overlaps_right_wall = external_rect.x < env.outer.right();
                        if overlaps_right_wall {
                            let required_external_x = env.outer.right().saturating_add(2);
                            let external_node_id = if from_inside_tree {
                                edge.to.clone()
                            } else {
                                edge.from.clone()
                            };
                            let delta_x = required_external_x - external_rect.x;
                            external_node_shifts
                                .entry(external_node_id)
                                .and_modify(|existing| *existing = (*existing).max(delta_x))
                                .or_insert(delta_x);
                        }
                    }
                }
            }

            if required_env_shift.is_none() && external_node_shifts.is_empty() {
                break;
            }

            if let Some((threshold_x, delta_x)) = required_env_shift {
                shift_nodes_from_x(
                    &mut placement.positions,
                    &mut placement.node_rects,
                    threshold_x,
                    delta_x,
                );
            }
            if !external_node_shifts.is_empty() {
                shift_nodes_by_id_x(
                    &mut placement.positions,
                    &mut placement.node_rects,
                    &external_node_shifts,
                );
            }

            let max_right = placement
                .node_rects
                .values()
                .map(|rect| rect.right())
                .max()
                .unwrap_or(placement.canvas.right());
            placement.canvas.width = placement.canvas.width.max(max_right);
        }
    }

    rebalance_side_by_side_horizontal_top_level_sibling_gaps(
        input.graph,
        &mut placement.positions,
        &mut placement.node_rects,
        config.subgraph_gutter,
        &mut placement.canvas.width,
    );

    // 3) Subgraph bounds + gutters.
    let mut subgraph_envelopes =
        compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
    adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);

    if matches!(input.graph.direction, Direction::LR | Direction::RL)
        && !subgraph_envelopes.is_empty()
    {
        let gap = nested_horizontal_follow_gap(&config);
        for _ in 0..8 {
            let mut required_shift_by_id: HashMap<String, isize> = HashMap::new();

            for child_subgraph in input
                .graph
                .subgraphs
                .iter()
                .filter(|subgraph| subgraph.parent_id.is_some())
            {
                let Some(parent_id) = child_subgraph.parent_id.as_deref() else {
                    continue;
                };
                let (Some(parent_env), Some(child_env)) = (
                    subgraph_envelopes.get(parent_id),
                    subgraph_envelopes.get(&child_subgraph.id),
                ) else {
                    continue;
                };
                if !rect_fully_inside(parent_env.outer, child_env.outer) {
                    continue;
                }

                let Some(target_left) = preferred_declared_nested_horizontal_left(
                    input.graph,
                    &placement.node_rects,
                    parent_id,
                    &child_subgraph.id,
                    parent_env,
                    child_env,
                    input.graph.direction,
                    gap,
                ) else {
                    continue;
                };

                if target_left == child_env.outer.x {
                    continue;
                }

                let delta = target_left as isize - child_env.outer.x as isize;
                required_shift_by_id
                    .entry(child_subgraph.id.clone())
                    .and_modify(|existing| {
                        if delta.abs() > existing.abs() {
                            *existing = delta;
                        }
                    })
                    .or_insert(delta);
            }

            let Some((subgraph_id, delta_x)) = required_shift_by_id
                .iter()
                .max_by_key(|(_, delta)| delta.abs())
                .map(|(id, delta)| (id.clone(), *delta))
            else {
                break;
            };

            shift_nodes_in_subgraph_tree_x_signed(
                input.graph,
                &mut placement.positions,
                &mut placement.node_rects,
                &subgraph_id,
                delta_x,
            );

            let max_right = placement
                .node_rects
                .values()
                .map(|rect| rect.right())
                .max()
                .unwrap_or(placement.canvas.right());
            placement.canvas.width = placement.canvas.width.max(max_right);

            subgraph_envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
        }

        for _ in 0..8 {
            let envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            let mut required_env_shift: Option<(usize, usize)> = None;
            let mut external_node_shifts: HashMap<String, usize> = HashMap::new();

            for (subgraph_id, env) in &envelopes {
                for edge in input.graph.edges.iter().filter(|edge| !edge.is_back_edge) {
                    let (Some(from_rect), Some(to_rect)) = (
                        placement.node_rects.get(&edge.from),
                        placement.node_rects.get(&edge.to),
                    ) else {
                        continue;
                    };

                    let from_inside_tree = input
                        .graph
                        .is_node_in_subgraph_tree(&edge.from, subgraph_id);
                    let to_inside_tree =
                        input.graph.is_node_in_subgraph_tree(&edge.to, subgraph_id);
                    if from_inside_tree == to_inside_tree {
                        continue;
                    }

                    let inside_rect = if from_inside_tree {
                        *from_rect
                    } else {
                        *to_rect
                    };
                    if !rect_fully_inside(env.outer, inside_rect) {
                        continue;
                    }

                    let external_rect = if from_inside_tree {
                        *to_rect
                    } else {
                        *from_rect
                    };
                    let external_is_subgraph = if from_inside_tree {
                        input.graph.get_node_subgraph(&edge.to).is_some()
                    } else {
                        input.graph.get_node_subgraph(&edge.from).is_some()
                    };
                    if external_is_subgraph {
                        continue;
                    }

                    if external_rect.x < env.outer.x {
                        let overlaps_left_wall = external_rect.right() > env.outer.x;
                        if overlaps_left_wall {
                            let required_env_x = external_rect.right().saturating_add(2);
                            let threshold_x = env.outer.x;
                            let delta_x = required_env_x - env.outer.x;
                            match required_env_shift {
                                Some((best_x, best_delta)) => {
                                    if threshold_x < best_x
                                        || (threshold_x == best_x && delta_x > best_delta)
                                    {
                                        required_env_shift = Some((threshold_x, delta_x));
                                    }
                                }
                                None => required_env_shift = Some((threshold_x, delta_x)),
                            }
                        }
                    } else {
                        let overlaps_right_wall = external_rect.x < env.outer.right();
                        if overlaps_right_wall {
                            let required_external_x = env.outer.right().saturating_add(2);
                            let external_node_id = if from_inside_tree {
                                edge.to.clone()
                            } else {
                                edge.from.clone()
                            };
                            let delta_x = required_external_x - external_rect.x;
                            external_node_shifts
                                .entry(external_node_id)
                                .and_modify(|existing| *existing = (*existing).max(delta_x))
                                .or_insert(delta_x);
                        }
                    }
                }
            }

            if required_env_shift.is_none() && external_node_shifts.is_empty() {
                break;
            }

            if let Some((threshold_x, delta_x)) = required_env_shift {
                shift_nodes_from_x(
                    &mut placement.positions,
                    &mut placement.node_rects,
                    threshold_x,
                    delta_x,
                );
            }
            if !external_node_shifts.is_empty() {
                shift_nodes_by_id_x(
                    &mut placement.positions,
                    &mut placement.node_rects,
                    &external_node_shifts,
                );
            }

            let max_right = placement
                .node_rects
                .values()
                .map(|rect| rect.right())
                .max()
                .unwrap_or(placement.canvas.right());
            placement.canvas.width = placement.canvas.width.max(max_right);

            subgraph_envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
        }
    }

    if is_vertical_flow(input.graph.direction) && !subgraph_envelopes.is_empty() {
        let route_budgeted_subgraphs = route_budgeted_subgraphs(input.graph);
        for _ in 0..8 {
            let mut widened_any = false;
            for subgraph_id in &route_budgeted_subgraphs {
                if widen_subgraph_for_internal_route_span(
                    input.graph,
                    &mut placement.positions,
                    &mut placement.node_rects,
                    subgraph_id,
                    config.min_horizontal_spacing,
                ) > 0
                {
                    widened_any = true;
                }
                if widen_subgraph_for_outgoing_route_pressure(
                    input.graph,
                    &mut placement.positions,
                    &mut placement.node_rects,
                    subgraph_id,
                ) > 0
                {
                    widened_any = true;
                }
            }
            if widened_any {
                let max_right = placement
                    .node_rects
                    .values()
                    .map(|r| r.right())
                    .max()
                    .unwrap_or(placement.canvas.right());
                placement.canvas.width = placement.canvas.width.max(max_right);
                subgraph_envelopes =
                    compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
                adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
            }

            let mut required_shift_by_id: HashMap<String, isize> = HashMap::new();

            let sg_ids: Vec<&String> = subgraph_envelopes.keys().collect();
            for parent_id in &sg_ids {
                let Some(parent_env) = subgraph_envelopes.get(*parent_id) else {
                    continue;
                };
                for child_id in &sg_ids {
                    if parent_id == child_id {
                        continue;
                    }
                    let Some(child_env) = subgraph_envelopes.get(*child_id) else {
                        continue;
                    };
                    if !rect_fully_inside(parent_env.outer, child_env.outer) {
                        continue;
                    }
                    let child_has_external_outgoing = input.graph.edges.iter().any(|edge| {
                        !edge.is_back_edge
                            && input.graph.is_node_in_subgraph_tree(&edge.from, child_id)
                            && !input.graph.is_node_in_subgraph_tree(&edge.to, child_id)
                    });
                    if !child_has_external_outgoing {
                        continue;
                    }

                    let preferred_center_x = preferred_subgraph_center_x(
                        input.graph,
                        &placement.node_rects,
                        child_id,
                        rect_center_x(child_env.outer),
                    );
                    let route_pressure_shift = outgoing_route_pressure_shift_x(
                        input.graph,
                        &placement.node_rects,
                        child_id,
                    );
                    let preferred_left =
                        preferred_center_x.saturating_sub(child_env.outer.width / 2);

                    let mut min_left = 0usize;
                    let mut max_left: Option<usize> = None;

                    for (node_id, node_rect) in placement.node_rects.iter() {
                        if input.graph.is_node_in_subgraph_tree(node_id, child_id) {
                            continue;
                        }
                        if !rect_fully_inside(parent_env.outer, *node_rect)
                            || !rects_overlap_vertically(*node_rect, child_env.outer)
                        {
                            continue;
                        }

                        if node_rect.right() <= child_env.outer.x {
                            min_left = min_left.max(node_rect.right().saturating_add(1));
                        } else if node_rect.x >= child_env.outer.right() {
                            let candidate = node_rect
                                .x
                                .saturating_sub(child_env.outer.width.saturating_add(1));
                            max_left =
                                Some(max_left.map_or(candidate, |limit| limit.min(candidate)));
                        } else {
                            min_left = min_left.max(node_rect.right().saturating_add(1));
                        }
                    }

                    let unclamped_left = if let Some(limit) = max_left {
                        preferred_left.clamp(min_left, limit.max(min_left))
                    } else {
                        preferred_left.max(min_left)
                    };
                    let target_left = if let Some(limit) = max_left {
                        unclamped_left
                            .saturating_add(route_pressure_shift)
                            .min(limit.max(unclamped_left))
                    } else {
                        unclamped_left.saturating_add(route_pressure_shift)
                    };

                    if target_left != child_env.outer.x {
                        let delta = target_left as isize - child_env.outer.x as isize;
                        required_shift_by_id
                            .entry((**child_id).clone())
                            .and_modify(|existing| {
                                if delta.abs() > existing.abs() {
                                    *existing = delta;
                                }
                            })
                            .or_insert(delta);
                    }
                }
            }

            let Some((sg_id, delta_x)) = required_shift_by_id
                .iter()
                .max_by_key(|(_, delta)| delta.abs())
                .map(|(id, delta)| (id.clone(), *delta))
            else {
                if widened_any {
                    continue;
                }
                break;
            };

            shift_nodes_in_subgraph_tree_x_signed(
                input.graph,
                &mut placement.positions,
                &mut placement.node_rects,
                &sg_id,
                delta_x,
            );

            let max_right = placement
                .node_rects
                .values()
                .map(|r| r.right())
                .max()
                .unwrap_or(placement.canvas.right());
            placement.canvas.width = placement.canvas.width.max(max_right);

            subgraph_envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
        }
    }

    // Ensure we have at least one row between a subgraph bottom border and any
    // external target box below it. Otherwise the renderer's arrow would land on
    // the border row (missing the arrow at the target entry point).
    if matches!(input.graph.direction, Direction::TD | Direction::TB)
        && !subgraph_envelopes.is_empty()
    {
        for _ in 0..8 {
            let mut required_shift_by_rank: HashMap<usize, usize> = HashMap::new();

            let mut subgraph_min_rank: HashMap<&str, usize> = HashMap::new();
            for sg in &input.graph.subgraphs {
                let min_rank = subgraph_tree_rank_range(input.graph, &placement.ranks, &sg.id)
                    .map(|(min_rank, _)| min_rank);
                if let Some(r) = min_rank {
                    subgraph_min_rank.insert(sg.id.as_str(), r);
                }
            }

            let mut incoming_into_subgraph_from: HashMap<(String, String), usize> = HashMap::new();
            for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                let (_, enter_subgraphs) =
                    input.graph.edge_boundary_crossings(&edge.from, &edge.to);
                for to_sg in enter_subgraphs {
                    *incoming_into_subgraph_from
                        .entry((edge.from.clone(), to_sg.to_string()))
                        .or_default() += 1;
                }
            }

            // Ensure declared parents keep a visible title/border band above nested children.
            for child_subgraph in input
                .graph
                .subgraphs
                .iter()
                .filter(|subgraph| subgraph.parent_id.is_some())
            {
                let Some(parent_id) = child_subgraph.parent_id.as_deref() else {
                    continue;
                };
                let (Some(parent_env), Some(child_env)) = (
                    subgraph_envelopes.get(parent_id),
                    subgraph_envelopes.get(&child_subgraph.id),
                ) else {
                    continue;
                };
                let Some(&shift_rank) = subgraph_min_rank.get(child_subgraph.id.as_str()) else {
                    continue;
                };

                let parent_has_title = input
                    .graph
                    .get_subgraph(parent_id)
                    .and_then(|subgraph| subgraph.title.as_ref())
                    .is_some();
                let required_child_top =
                    parent_env
                        .outer
                        .y
                        .saturating_add(if parent_has_title { 3 } else { 2 });
                if child_env.outer.y >= required_child_top {
                    continue;
                }

                let delta = required_child_top - child_env.outer.y;
                required_shift_by_rank
                    .entry(shift_rank)
                    .and_modify(|existing| *existing = (*existing).max(delta))
                    .or_insert(delta);
            }

            // Ensure enough clearance above a subgraph top border for incoming edges.
            for (sg_id, env) in subgraph_envelopes.iter() {
                let Some(&shift_rank) = subgraph_min_rank.get(sg_id.as_str()) else {
                    continue;
                };
                for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                    let (_, enter_subgraphs) =
                        input.graph.edge_boundary_crossings(&edge.from, &edge.to);
                    if !enter_subgraphs.contains(&sg_id.as_str()) {
                        continue;
                    }
                    // Don't apply this spacing rule for edges whose source already sits inside
                    // another subgraph (nested compositions). Those are handled by internal
                    // subgraph padding and routing, and enforcing "outside" clearance here
                    // can cause runaway vertical expansion.
                    if input.graph.get_node_subgraph(&edge.from).is_some() {
                        continue;
                    }
                    let Some(from_rect) = placement.node_rects.get(&edge.from) else {
                        continue;
                    };
                    // Single incoming edge: one connector row is enough.
                    // Fan-out entry (same external source → multiple targets): keep two rows so
                    // the trunk can be visible before entering the subgraph.
                    let incoming_count = incoming_into_subgraph_from
                        .get(&(edge.from.clone(), sg_id.clone()))
                        .copied()
                        .unwrap_or(1);
                    let clearance = if incoming_count > 1 { 2 } else { 1 };
                    let required_border_y = from_rect.bottom().saturating_add(clearance);
                    if env.outer.y < required_border_y {
                        let delta = required_border_y - env.outer.y;
                        required_shift_by_rank
                            .entry(shift_rank)
                            .and_modify(|d| *d = (*d).max(delta))
                            .or_insert(delta);
                    }
                }
            }

            // Ensure at least one empty row between stacked subgraphs when an edge crosses
            // from one to the next (so the connector is visible outside both borders).
            for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                let (Some(from_sg), Some(to_sg)) = (
                    input.graph.get_node_subgraph(&edge.from),
                    input.graph.get_node_subgraph(&edge.to),
                ) else {
                    continue;
                };
                if from_sg == to_sg {
                    continue;
                }
                let (Some(from_env), Some(to_env)) = (
                    subgraph_envelopes.get(from_sg),
                    subgraph_envelopes.get(to_sg),
                ) else {
                    continue;
                };
                // Only skip if subgraphs are truly nested (one fully inside the other).
                // Overlapping-but-not-nested subgraphs need spacing applied.
                let is_nested = rect_fully_inside(from_env.outer, to_env.outer)
                    || rect_fully_inside(to_env.outer, from_env.outer);
                if is_nested {
                    continue;
                }
                let required_to_top = from_env.outer.bottom().saturating_add(1);
                if to_env.outer.y >= required_to_top {
                    continue;
                }
                let Some(&shift_rank) = subgraph_min_rank.get(to_sg) else {
                    continue;
                };
                let delta = required_to_top - to_env.outer.y;
                required_shift_by_rank
                    .entry(shift_rank)
                    .and_modify(|d| *d = (*d).max(delta))
                    .or_insert(delta);
            }

            for env in subgraph_envelopes.values() {
                for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                    let (Some(from_rect), Some(to_rect)) = (
                        placement.node_rects.get(&edge.from),
                        placement.node_rects.get(&edge.to),
                    ) else {
                        continue;
                    };

                    if !rect_fully_inside(env.outer, *from_rect) {
                        continue;
                    }
                    if rect_fully_inside(env.outer, *to_rect) {
                        continue;
                    }
                    // If the destination is inside another subgraph, let that subgraph's
                    // padding handle arrow/label clearance. This rule is specifically for
                    // edges that exit a subgraph into open (non-subgraph) space.
                    if input.graph.get_node_subgraph(&edge.to).is_some() {
                        continue;
                    }
                    let required_target_y = env.outer.bottom().saturating_add(1);
                    if to_rect.y >= required_target_y {
                        continue;
                    }
                    let Some(rank) = placement.ranks.get(&edge.to) else {
                        continue;
                    };
                    let delta = required_target_y - to_rect.y;
                    required_shift_by_rank
                        .entry(*rank)
                        .and_modify(|d| *d = (*d).max(delta))
                        .or_insert(delta);
                }
            }

            let Some((&min_rank, &delta_y)) = required_shift_by_rank.iter().min_by_key(|(r, _)| *r)
            else {
                break;
            };

            shift_nodes_from_rank_td(
                &mut placement.positions,
                &mut placement.node_rects,
                &placement.ranks,
                min_rank,
                delta_y,
            );

            let max_bottom = placement
                .node_rects
                .values()
                .map(|r| r.bottom())
                .max()
                .unwrap_or(placement.canvas.bottom());
            placement.canvas.height = placement.canvas.height.max(max_bottom);

            subgraph_envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
        }
    }

    compact_stacked_vertical_top_level_sibling_subgraphs(
        input.graph,
        &mut placement.positions,
        &mut placement.node_rects,
        config.subgraph_gutter,
        &mut placement.canvas.height,
    );
    subgraph_envelopes =
        compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
    adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);

    // BT: ensure clearance above subgraph top borders (for outgoing edges to external
    // targets above) and between stacked subgraphs (so connectors don't overwrite
    // titles/corners on adjacent borders).
    if input.graph.direction == Direction::BT && !subgraph_envelopes.is_empty() {
        for _ in 0..8 {
            let mut required_shift_by_rank: HashMap<usize, usize> = HashMap::new();

            let mut subgraph_max_rank: HashMap<&str, usize> = HashMap::new();
            for sg in &input.graph.subgraphs {
                let max_rank = subgraph_tree_rank_range(input.graph, &placement.ranks, &sg.id)
                    .map(|(_, max_rank)| max_rank);
                if let Some(r) = max_rank {
                    subgraph_max_rank.insert(sg.id.as_str(), r);
                }
            }

            // Keep at least one connector row between an external target box above and the
            // subgraph top border it is connected to.
            for (sg_id, env) in subgraph_envelopes.iter() {
                let Some(&shift_rank) = subgraph_max_rank.get(sg_id.as_str()) else {
                    continue;
                };
                for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                    let (exit_subgraphs, _) =
                        input.graph.edge_boundary_crossings(&edge.from, &edge.to);
                    if !exit_subgraphs.contains(&sg_id.as_str()) {
                        continue;
                    }
                    let Some(to_rect) = placement.node_rects.get(&edge.to) else {
                        continue;
                    };
                    // Only when the destination is above this envelope.
                    if to_rect.bottom() > env.outer.y.saturating_add(1) {
                        continue;
                    }
                    let required_border_y = to_rect.bottom().saturating_add(1);
                    if env.outer.y >= required_border_y {
                        continue;
                    }
                    let delta = required_border_y - env.outer.y;
                    required_shift_by_rank
                        .entry(shift_rank)
                        .and_modify(|d| *d = (*d).max(delta))
                        .or_insert(delta);
                }
            }

            // Ensure at least one connector row between a subgraph bottom border and any
            // external source node that feeds into content inside that envelope. In BT this
            // matters for both direct targets and visually nested parent envelopes; otherwise
            // an enlarged outer border can land on top of the lower source box.
            for (sg_id, env) in subgraph_envelopes.iter() {
                let Some(subgraph) = input.graph.get_subgraph(sg_id) else {
                    continue;
                };
                if subgraph.parent_id.is_none() && subgraph.child_ids.is_empty() {
                    continue;
                }
                for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                    let Some(from_rect) = placement.node_rects.get(&edge.from) else {
                        continue;
                    };
                    if input.graph.is_node_in_subgraph_tree(&edge.from, sg_id) {
                        continue;
                    }
                    if !input.graph.is_node_in_subgraph_tree(&edge.to, sg_id) {
                        continue;
                    }
                    if !rect_fully_inside(env.outer, *from_rect) {
                        continue;
                    }
                    // The source node must start at least one row below the outer envelope
                    // bottom so there is room for the routing connector between them.
                    let required_source_y = env.outer.bottom().saturating_add(1);
                    if from_rect.y >= required_source_y {
                        continue;
                    }
                    let Some(&rank) = placement.ranks.get(&edge.from) else {
                        continue;
                    };
                    let delta = required_source_y - from_rect.y;
                    required_shift_by_rank
                        .entry(rank)
                        .and_modify(|d| *d = (*d).max(delta))
                        .or_insert(delta);
                }
            }

            // Ensure at least one empty row between stacked subgraphs when an edge crosses
            // from the lower subgraph to the upper one (BT flows upward).
            for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                let (Some(from_sg), Some(to_sg)) = (
                    input.graph.get_node_subgraph(&edge.from),
                    input.graph.get_node_subgraph(&edge.to),
                ) else {
                    continue;
                };
                if from_sg == to_sg {
                    continue;
                }
                let (Some(from_env), Some(to_env)) = (
                    subgraph_envelopes.get(from_sg),
                    subgraph_envelopes.get(to_sg),
                ) else {
                    continue;
                };
                // In BT, `to_sg` is visually above `from_sg` (smaller y). Only skip if
                // subgraphs are truly nested (one fully inside the other).
                let is_nested = rect_fully_inside(from_env.outer, to_env.outer)
                    || rect_fully_inside(to_env.outer, from_env.outer);
                if is_nested {
                    continue;
                }
                let required_from_top = to_env.outer.bottom().saturating_add(1);
                if from_env.outer.y >= required_from_top {
                    continue;
                }
                let Some(&shift_rank) = subgraph_max_rank.get(from_sg) else {
                    continue;
                };
                let delta = required_from_top - from_env.outer.y;
                required_shift_by_rank
                    .entry(shift_rank)
                    .and_modify(|d| *d = (*d).max(delta))
                    .or_insert(delta);
            }

            let Some((&max_rank, &delta_y)) = required_shift_by_rank.iter().max_by_key(|(r, _)| *r)
            else {
                break;
            };

            shift_nodes_up_to_rank_bt(
                &mut placement.positions,
                &mut placement.node_rects,
                &placement.ranks,
                max_rank,
                delta_y,
            );

            let max_bottom = placement
                .node_rects
                .values()
                .map(|r| r.bottom())
                .max()
                .unwrap_or(placement.canvas.bottom());
            placement.canvas.height = placement.canvas.height.max(max_bottom);

            subgraph_envelopes =
                compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
            adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);
        }

        let mut incoming_into_subgraph_from: HashMap<(String, String), usize> = HashMap::new();
        for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
            let (_, enter_subgraphs) = input.graph.edge_boundary_crossings(&edge.from, &edge.to);
            for to_sg in enter_subgraphs {
                *incoming_into_subgraph_from
                    .entry((edge.from.clone(), to_sg.to_string()))
                    .or_default() += 1;
            }
        }

        let mut source_shifts: HashMap<String, usize> = HashMap::new();
        for (subgraph_id, env) in subgraph_envelopes.iter() {
            let has_title = input
                .graph
                .get_subgraph(subgraph_id)
                .and_then(|subgraph| subgraph.title.as_ref())
                .is_some();
            let contains_child_envelope = subgraph_envelopes.iter().any(|(other_id, other_env)| {
                other_id != subgraph_id && rect_fully_inside(env.outer, other_env.outer)
            });
            if !contains_child_envelope && !has_title {
                continue;
            }
            let required_source_y = env.outer.bottom().saturating_add(1);
            for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
                let (Some(from_rect), Some(to_rect)) = (
                    placement.node_rects.get(&edge.from),
                    placement.node_rects.get(&edge.to),
                ) else {
                    continue;
                };
                if rect_fully_inside(env.outer, *from_rect)
                    || !rect_fully_inside(env.outer, *to_rect)
                {
                    continue;
                }
                if input.graph.get_node_subgraph(&edge.from).is_some() {
                    continue;
                }
                if !contains_child_envelope
                    && incoming_into_subgraph_from
                        .get(&(edge.from.clone(), subgraph_id.clone()))
                        .copied()
                        .unwrap_or(1)
                        <= 1
                {
                    continue;
                }
                let overlaps_envelope_horizontally =
                    from_rect.x < env.outer.right() && env.outer.x < from_rect.right();
                if !overlaps_envelope_horizontally || from_rect.y >= required_source_y {
                    continue;
                }

                let delta = required_source_y - from_rect.y;
                source_shifts
                    .entry(edge.from.clone())
                    .and_modify(|existing| *existing = (*existing).max(delta))
                    .or_insert(delta);
            }
        }

        if !source_shifts.is_empty() {
            shift_nodes_by_id_y(
                &mut placement.positions,
                &mut placement.node_rects,
                &source_shifts,
            );
            let max_bottom = placement
                .node_rects
                .values()
                .map(|r| r.bottom())
                .max()
                .unwrap_or(placement.canvas.bottom());
            placement.canvas.height = placement.canvas.height.max(max_bottom);
        }
    }

    // Warn about overlapping (but not nested) subgraphs that couldn't be resolved.
    if debug_timing && subgraph_envelopes.len() > 1 {
        let sg_ids: Vec<&String> = subgraph_envelopes.keys().collect();
        for i in 0..sg_ids.len() {
            for j in (i + 1)..sg_ids.len() {
                let env1 = &subgraph_envelopes[sg_ids[i]];
                let env2 = &subgraph_envelopes[sg_ids[j]];
                // Check if they intersect
                let intersects = env1.outer.x < env2.outer.right()
                    && env1.outer.right() > env2.outer.x
                    && env1.outer.y < env2.outer.bottom()
                    && env1.outer.bottom() > env2.outer.y;
                if intersects {
                    let nested = rect_fully_inside(env1.outer, env2.outer)
                        || rect_fully_inside(env2.outer, env1.outer);
                    if !nested {
                        eprintln!(
                            "termiflow: warning: subgraphs {} and {} overlap",
                            sg_ids[i], sg_ids[j]
                        );
                    }
                }
            }
        }
    }

    rebalance_titled_vertical_subgraph_content_x(
        input.graph,
        &mut placement.positions,
        &mut placement.node_rects,
        config.subgraph_gutter,
        &mut placement.canvas.width,
    );
    rebalance_titled_vertical_subgraph_content_y(
        input.graph,
        &mut placement.positions,
        &mut placement.node_rects,
        config.subgraph_gutter,
        &mut placement.canvas.height,
    );
    subgraph_envelopes =
        compute_envelopes(input.graph, &placement.node_rects, config.subgraph_gutter);
    adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);

    enforce_declared_nested_envelopes(input.graph, &mut subgraph_envelopes);
    adjust_portal_slots_for_title(&mut subgraph_envelopes, input.graph);

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
        &coords,
        config.node_padding,
        input.graph,
        &subgraph_envelopes,
    );
    // No additional carving for fan-outs; deterministic lanes are built during routing.
    layout_routing::mark_subgraph_rings(&mut grid, &subgraph_envelopes);
    if config.enable_portals {
        layout_routing::carve_subgraph_portals(
            &mut grid,
            &subgraph_envelopes,
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
    let warnings = Vec::new();
    let t_route = std::time::Instant::now();
    let mut outgoing_counts: HashMap<&str, usize> = HashMap::new();
    let mut incoming_counts: HashMap<&str, usize> = HashMap::new();
    for edge in input.graph.edges.iter().filter(|e| !e.is_back_edge) {
        *outgoing_counts.entry(edge.from.as_str()).or_default() += 1;
        *incoming_counts.entry(edge.to.as_str()).or_default() += 1;
    }
    layout_routing::route_selective_horizontal_cross_subgraph_fanin_groups(
        input.graph,
        &placement.node_rects,
        &subgraph_envelopes,
        &incoming_counts,
        &mut routes,
        &mut grid,
    );
    for (edge_idx, edge) in input.graph.edges.iter().enumerate() {
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
                eprintln!("  skip edge {} due to convergent routing", edge_idx);
            }
            continue;
        }

        // Fan-outs look best when the renderer owns the shared junction.
        if out_degree > 1 {
            if debug_timing {
                eprintln!("  skip edge {} fan-out handled in renderer", edge_idx);
            }
            continue;
        }

        // Labeled fan-out / fan-in edges are better handled in the renderer so labels
        // can sit on clean junctions instead of fighting precomputed paths.
        if edge.label.is_some() && (out_degree > 1 || in_degree > 1) {
            if debug_timing {
                eprintln!("  skip edge {} labeled fan-out/fan-in", edge_idx);
            }
            continue;
        }

        let crosses_subgraph = input
            .graph
            .edge_crosses_subgraph_boundary(&edge.from, &edge.to);

        // Leave fan-out / fan-in edges that cross subgraph boundaries to the renderer so
        // they can share junctions cleanly instead of overlapping pre-routed lanes.
        if crosses_subgraph && (out_degree > 1 || in_degree > 1) {
            if debug_timing {
                eprintln!("  skip edge {} cross-subgraph fan routing", edge_idx);
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
            input.graph,
            &subgraph_envelopes,
            edge_idx,
            &edge.from,
            &edge.to,
        );

        let from_sg = input.graph.get_node_subgraph(&edge.from);
        let to_sg = input.graph.get_node_subgraph(&edge.to);

        let start = layout_routing::edge_exit_point(from_rect, input.graph.direction);
        let end = layout_routing::edge_entry_point(to_rect, input.graph.direction);

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
                input.graph.direction,
                out_degree,
                in_degree,
                config.node_padding.max(1),
            ) {
                grid.mark_path(&route);
                if debug_timing {
                    eprintln!("  lane route stored for edge {}", edge_idx);
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
                        input.graph.direction,
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
                        input.graph.direction,
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
                &coords,
            ) {
                grid.mark_path(&route);
                for s in route.segments {
                    combined.push_segment(s.from, s.to);
                }
            } else {
                let route = layout_routing::fallback_manhattan_route(
                    seg_start,
                    seg_end,
                    input.graph.direction,
                );
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
            input.graph.edges.len()
        );
        eprintln!("termiflow: stored routes {}", routes.len());
    }

    Ok(LayoutOutput {
        positions: placement.positions,
        subgraph_envelopes,
        routes,
        canvas: placement.canvas,
        warnings,
        ranks: placement.ranks,
    })
}
