use super::envelope_stage::{
    clear_feeders_off_frame_walls, finalize_bt_parallel_first_rail, resolve_subgraph_envelopes,
};
use super::normalization::normalize_orientation_and_gutters;
use super::routing_stage::route_stage;
use super::*;

use crate::render::sibling_subgraph_fan_in_identity;

pub fn layout(input: LayoutInput, config: CoarseLayoutConfig) -> Result<LayoutOutput> {
    let coords = OrientedCoords::new(input.graph.direction);
    let debug_timing = crate::runtime::current().diagnostics.timing;

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
    super::centre_polish::polish_vertical_centres(
        input.graph,
        &config,
        &mut placement.positions,
        &mut placement.node_rects,
        input.prior_positions.as_ref(),
    );
    clear_skip_edge_columns(input.graph, &mut placement);
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
            let mut required_drop_by_id: HashMap<String, usize> = HashMap::new();

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
                    // Shift the later-ranked subgraph, with its nested frames, to the
                    // right until it clears the earlier one. Parallel lanes of the same
                    // rank (one node fans into both, or both feed one place) stack
                    // instead: drawn side by side, each lane's lines ran along or
                    // through the other (gen_c_124, gen_a_132, gen_c_070).
                    let (late_id, early_env, late_env) = if rank1 <= rank2 {
                        (sg_ids[j].as_str(), env1, env2)
                    } else {
                        (sg_ids[i].as_str(), env2, env1)
                    };

                    if rank1 == rank2
                        && frames_are_parallel_lanes(input.graph, sg_ids[i], sg_ids[j])
                    {
                        let required_top = early_env.outer.bottom().saturating_add(1);
                        if late_env.outer.y < required_top {
                            let delta = required_top - late_env.outer.y;
                            required_drop_by_id
                                .entry(late_id.to_string())
                                .and_modify(|d| *d = (*d).max(delta))
                                .or_insert(delta);
                        }
                        continue;
                    }
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

            if let Some((late_id, delta_y)) = required_drop_by_id
                .iter()
                .max_by(|(left_id, left_delta), (right_id, right_delta)| {
                    left_delta
                        .cmp(right_delta)
                        .then_with(|| right_id.cmp(left_id))
                })
                .map(|(id, delta)| (id.clone(), *delta))
            {
                shift_nodes_in_subgraph_tree_y_signed(
                    input.graph,
                    &mut placement.positions,
                    &mut placement.node_rects,
                    &late_id,
                    delta_y as isize,
                );
                continue;
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

            shift_nodes_in_subgraph_tree_x_signed(
                input.graph,
                &mut placement.positions,
                &mut placement.node_rects,
                &late_id,
                delta_x as isize,
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
            // BT top-level frames that send edges both ways interleave in rank,
            // so they go side by side instead of one below the other.
            let mut side_shifts: HashMap<String, usize> = HashMap::new();
            let sends = |from: &str, to: &str| {
                input.graph.edges.iter().any(|edge| {
                    !edge.is_back_edge
                        && input.graph.is_node_in_subgraph_tree(&edge.from, from)
                        && input.graph.is_node_in_subgraph_tree(&edge.to, to)
                })
            };

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

                    // Shift the later-ranked subgraph down until it clears the earlier one.
                    // On a tie, the frame feeding the other one comes first.
                    let first_feeds_second = sends(sg_ids[i], sg_ids[j]);
                    let second_feeds_first = sends(sg_ids[j], sg_ids[i]);
                    let first_is_early = rank1 < rank2
                        || rank1 == rank2 && (first_feeds_second || !second_feeds_first);
                    let (late_id, early_env, late_env) = if first_is_early {
                        (sg_ids[j].as_str(), env1, env2)
                    } else {
                        (sg_ids[i].as_str(), env2, env1)
                    };

                    let top_level = |id: &str| {
                        input
                            .graph
                            .get_subgraph(id)
                            .is_some_and(|subgraph| subgraph.parent_id.is_none())
                    };
                    // BT frames with members on a shared rank and no edge between
                    // them cannot stack without one wrapping the other's members.
                    let spans =
                        |id: &str| subgraph_tree_rank_range(input.graph, &placement.ranks, id);
                    let share_a_rank = match (spans(sg_ids[i]), spans(sg_ids[j])) {
                        (Some((min1, max1)), Some((min2, max2))) => min1 <= max2 && min2 <= max1,
                        _ => false,
                    };
                    // A back edge into a frame side by side with another runs along
                    // the shared row, and BT has no detour row for it.
                    let cycles_through = |id: &str| {
                        input.graph.edges.iter().any(|edge| {
                            edge.is_back_edge
                                && (input.graph.is_node_in_subgraph_tree(&edge.from, id)
                                    || input.graph.is_node_in_subgraph_tree(&edge.to, id))
                        })
                    };
                    // An outside node ranked inside a frame's span and joined to it
                    // weaves through it (gen_a_108: SW -> Online -> Queue -> IDB);
                    // side by side, that node lands on the frame's border.
                    let woven = |id: &str| {
                        let Some((low, high)) = spans(id) else {
                            return false;
                        };
                        input.graph.edges.iter().any(|edge| {
                            [(&edge.from, &edge.to), (&edge.to, &edge.from)].iter().any(
                                |(outside, inside)| {
                                    !input.graph.is_node_in_subgraph_tree(outside, id)
                                        && input.graph.is_node_in_subgraph_tree(inside, id)
                                        && placement
                                            .ranks
                                            .get(outside.as_str())
                                            .is_some_and(|&rank| low < rank && rank < high)
                                },
                            )
                        })
                    };
                    let (cycles_i, cycles_j) =
                        (cycles_through(sg_ids[i]), cycles_through(sg_ids[j]));
                    let unwoven = !woven(sg_ids[i]) && !woven(sg_ids[j]);
                    // Back edges run to the right gutter, so side by side the frame a
                    // back edge reaches stands right of the other (gen_c_095).
                    if input.graph.direction == Direction::BT
                        && top_level(sg_ids[i])
                        && top_level(sg_ids[j])
                        && share_a_rank
                        && unwoven
                        && cycles_i != cycles_j
                        && !sends(sg_ids[i], sg_ids[j])
                        && !sends(sg_ids[j], sg_ids[i])
                    {
                        let (right_id, left_env, right_env) = if cycles_i {
                            (sg_ids[i].as_str(), env2, env1)
                        } else {
                            (sg_ids[j].as_str(), env1, env2)
                        };
                        let required_left = left_env.outer.right().saturating_add(1);
                        if right_env.outer.x < required_left {
                            let delta = required_left - right_env.outer.x;
                            side_shifts
                                .entry(right_id.to_string())
                                .and_modify(|d| *d = (*d).max(delta))
                                .or_insert(delta);
                        }
                        continue;
                    }
                    let share_a_rank = share_a_rank && !cycles_i && !cycles_j && unwoven;
                    if input.graph.direction == Direction::BT
                        && top_level(sg_ids[i])
                        && top_level(sg_ids[j])
                        && ((share_a_rank
                            && !sends(sg_ids[i], sg_ids[j])
                            && !sends(sg_ids[j], sg_ids[i]))
                            || sends(sg_ids[i], sg_ids[j]) && sends(sg_ids[j], sg_ids[i]))
                    {
                        let required_left = early_env.outer.right().saturating_add(1);
                        if late_env.outer.x < required_left {
                            let delta = required_left - late_env.outer.x;
                            side_shifts
                                .entry(late_id.to_string())
                                .and_modify(|d| *d = (*d).max(delta))
                                .or_insert(delta);
                        }
                        continue;
                    }

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

            if let Some((sg_id, delta)) = side_shifts
                .iter()
                .max_by(|(left_id, left_delta), (right_id, right_delta)| {
                    left_delta
                        .cmp(right_delta)
                        .then_with(|| right_id.cmp(left_id))
                })
                .map(|(id, d)| (id.clone(), *d))
            {
                shift_nodes_in_subgraph_tree_x_signed(
                    input.graph,
                    &mut placement.positions,
                    &mut placement.node_rects,
                    &sg_id,
                    delta as isize,
                );
                // Outside nodes whose every edge joins the moved frame go with
                // it; left behind, their straight edges into it gain jogs.
                for node in &input.graph.nodes {
                    if input.graph.get_node_subgraph(&node.id).is_some() {
                        continue;
                    }
                    let mut edges = input
                        .graph
                        .edges
                        .iter()
                        .filter(|edge| edge.from == node.id || edge.to == node.id)
                        .peekable();
                    let satellite = edges.peek().is_some()
                        && edges.all(|edge| {
                            let other = if edge.from == node.id {
                                &edge.to
                            } else {
                                &edge.from
                            };
                            input.graph.is_node_in_subgraph_tree(other, &sg_id)
                        });
                    if !satellite {
                        continue;
                    }
                    if let Some(point) = placement.positions.get_mut(&node.id) {
                        point.x += delta;
                    }
                    if let Some(rect) = placement.node_rects.get_mut(&node.id) {
                        rect.x += delta;
                    }
                }
                continue;
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

            // In BT, an outside node that follows the frame, fed only by one of
            // its members or by such a node, stays past it: left behind, the
            // shifted member would land on or past it and the flip would leave
            // it under it (gen_a_228's Offer under Interview Loop, gen_a_248's
            // Controller under Planning and the actuators under Controller). A
            // node outside any frame or in one enclosing this one moves alone
            // without splitting its own frame. A target several members feed
            // keeps the corridor beside the frames its fan-in is planned for
            // (subgraph_complex_bt). TD/LR passes after this one already part
            // such a node.
            let encloses_frame = |own: &str| {
                let mut current = input
                    .graph
                    .get_subgraph(&sg_id)
                    .and_then(|subgraph| subgraph.parent_id.as_deref());
                while let Some(id) = current {
                    if id == own {
                        return true;
                    }
                    current = input
                        .graph
                        .get_subgraph(id)
                        .and_then(|subgraph| subgraph.parent_id.as_deref());
                }
                false
            };
            let mut followers: Vec<String> = Vec::new();
            let mut grew = input.graph.direction == Direction::BT;
            while grew {
                let before = followers.len();
                for node in &input.graph.nodes {
                    if followers.contains(&node.id)
                        || input.graph.is_node_in_subgraph_tree(&node.id, &sg_id)
                        || !input
                            .graph
                            .get_node_subgraph(&node.id)
                            .is_none_or(encloses_frame)
                    {
                        continue;
                    }
                    let mut incoming = input
                        .graph
                        .edges
                        .iter()
                        .filter(|edge| !edge.is_back_edge && edge.to == node.id);
                    let (Some(edge), None) = (incoming.next(), incoming.next()) else {
                        continue;
                    };
                    let carried = input.graph.is_node_in_subgraph_tree(&edge.from, &sg_id)
                        || followers.contains(&edge.from);
                    let overtaken = match (
                        placement.node_rects.get(&edge.from),
                        placement.node_rects.get(&node.id),
                    ) {
                        (Some(source), Some(rect)) => {
                            source.y < rect.y && source.y + delta >= rect.y
                        }
                        _ => false,
                    };
                    if carried && overtaken {
                        followers.push(node.id.clone());
                    }
                }
                grew = followers.len() > before;
            }

            // Shift every node in the subgraph tree down: a frame holding only
            // nested frames has no direct members to move.
            shift_nodes_in_subgraph_tree_y_signed(
                input.graph,
                &mut placement.positions,
                &mut placement.node_rects,
                &sg_id,
                delta as isize,
            );
            for id in followers {
                if let Some(point) = placement.positions.get_mut(&id) {
                    point.y += delta;
                }
                if let Some(rect) = placement.node_rects.get_mut(&id) {
                    rect.y += delta;
                }
            }
        }
    }

    normalize_orientation_and_gutters(input.graph, &config, &mut placement);
    resolve_subgraph_envelopes(&input, &config, &mut placement, debug_timing);
    reserve_sibling_subgraph_target_corridor(input.graph, &mut placement, &config);
    reserve_nested_bt_external_entry_lanes(input.graph, &mut placement, &config);
    clear_frame_border_centerlines(input.graph, &mut placement, &config);
    let mut subgraph_envelopes =
        resolve_subgraph_envelopes(&input, &config, &mut placement, debug_timing);
    if let Some(envelopes) =
        clear_feeders_off_frame_walls(input.graph, &config, &mut placement, &subgraph_envelopes)
    {
        subgraph_envelopes = envelopes;
    }
    // H154 is the final fixed-envelope owner for the exact direct three-rail
    // BT scene. It runs once after the second bounded envelope resolution so a
    // relative lane preference cannot be consumed twice by the layout loop.
    finalize_bt_parallel_first_rail(
        input.graph,
        &config,
        &mut placement,
        &mut subgraph_envelopes,
    );
    reserve_title_turn_rows(input.graph, &mut placement, &mut subgraph_envelopes);
    reserve_bt_back_edge_exit_rows(input.graph, &mut placement, &mut subgraph_envelopes);
    sink_lr_back_edge_ends_below_twins(input.graph, &mut placement);
    reserve_lr_back_edge_top_rows(input.graph, &mut placement);
    reserve_lr_back_edge_lane_rows(input.graph, &config, &mut placement);

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

/// Reserve the primary-axis room required by the bounded sibling-subgraph
/// target-entry scene.  External terminal targets are safe to move here: the
/// topology gate proves they have no outgoing edges and the two selected
/// source edges are the only incoming scene ownership.  Keeping this in the
/// placement stage means the envelope, route planner, and renderer all see
/// the same corridor instead of repairing a cramped frame after painting.
fn reserve_sibling_subgraph_target_corridor(
    graph: &Graph,
    placement: &mut placement::Placement,
    config: &CoarseLayoutConfig,
) {
    let required = sibling_subgraph_fan_in_identity::required_primary_gap(
        sibling_subgraph_fan_in_identity::TARGET_PORT_COUNT,
    );
    if required == 0 {
        return;
    }

    let envelopes = compute_envelopes(graph, &placement.node_rects, config.subgraph_gutter);
    for scene in sibling_subgraph_fan_in_identity::scenes(graph) {
        let Some(source) = envelopes
            .get(&scene.source_subgraph_id)
            .map(|env| env.outer)
        else {
            continue;
        };
        let Some(target) = placement.node_rects.get(&scene.target_id).copied() else {
            continue;
        };

        let right = target.x >= source.right();
        let left = target.right() <= source.x;
        let below = target.y >= source.bottom();
        let above = target.bottom() <= source.y;
        let vertical_gap = if below {
            target.y.saturating_sub(source.bottom())
        } else if above {
            source.y.saturating_sub(target.bottom())
        } else {
            usize::MAX
        };
        let horizontal_gap = if right {
            target.x.saturating_sub(source.right())
        } else if left {
            source.x.saturating_sub(target.right())
        } else {
            usize::MAX
        };

        let vertical_preferred = matches!(
            graph.direction,
            Direction::TD | Direction::TB | Direction::BT
        );
        let use_horizontal = if right || left {
            if below || above {
                if vertical_preferred {
                    vertical_gap < required || horizontal_gap <= vertical_gap
                } else {
                    true
                }
            } else {
                true
            }
        } else {
            false
        };

        let (dx, dy) = if use_horizontal && right {
            (required.saturating_sub(horizontal_gap) as isize, 0)
        } else if use_horizontal && left {
            (-(required.saturating_sub(horizontal_gap) as isize), 0)
        } else if below {
            (0, required.saturating_sub(vertical_gap) as isize)
        } else if above {
            (0, -(required.saturating_sub(vertical_gap) as isize))
        } else {
            (0, 0)
        };
        if dx == 0 && dy == 0 {
            continue;
        }

        let Some(rect) = placement.node_rects.get_mut(&scene.target_id) else {
            continue;
        };
        rect.x = shift_coordinate(rect.x, dx);
        rect.y = shift_coordinate(rect.y, dy);
        if let Some(position) = placement.positions.get_mut(&scene.target_id) {
            position.x = rect.x;
            position.y = rect.y;
        }
        placement.canvas.width = placement.canvas.width.max(rect.right());
        placement.canvas.height = placement.canvas.height.max(rect.bottom());
    }
}

fn rects_overlap(a: &Rect, b: &Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

/// Columns a fan needs between its members and a frame to draw its bus
/// there rather than inside the frame.
const FAN_BUS_ROOM: usize = 5;

/// The row step that takes an LR/RL node's centre row off a frame border its
/// edge to `other` would run along: the node stands outside the frame, its
/// centre row is the frame's top or bottom border, and the edge has to pass
/// the frame. `fellows` are the other members of a fan `other` draws to the
/// node: a frame holding `other` and none of them leaves the bus outside it
/// when the gap has room, and the node's line then never reaches the frame.
fn border_centerline_step(
    graph: &Graph,
    (node_id, rect): (&str, &Rect),
    (other_id, other): (&str, &Rect),
    fellows: &[&str],
    envelopes: &HashMap<String, SubgraphEnvelope>,
) -> Option<isize> {
    let centre = rect.y + rect.height / 2;
    let mut ids: Vec<&String> = envelopes.keys().collect();
    ids.sort();
    for sg in ids {
        if graph.is_node_in_subgraph_tree(node_id, sg) {
            continue;
        }
        let frame = envelopes[sg].outer;
        let gap = if rect.right() <= frame.x {
            frame.x - rect.right()
        } else if rect.x >= frame.right() {
            rect.x - frame.right()
        } else {
            continue;
        };
        let (a, b) = (rect.x + rect.width / 2, other.x + other.width / 2);
        if a.min(b) >= frame.right() || a.max(b) <= frame.x {
            continue;
        }
        let bus_outside = !fellows.is_empty()
            && gap >= FAN_BUS_ROOM
            && graph.is_node_in_subgraph_tree(other_id, sg)
            && !fellows
                .iter()
                .any(|fellow| graph.is_node_in_subgraph_tree(fellow, sg));
        if bus_outside {
            continue;
        }
        if centre == frame.y {
            return Some(-2);
        }
        if centre + 1 == frame.bottom() {
            return Some(2);
        }
    }
    None
}

/// The first border step any of a node's edges asks for. A node fanning
/// lines out, or taking a fan in, meets them at a bus beside it, so only
/// its single lines count.
fn node_border_step(
    graph: &Graph,
    id: &str,
    rect: &Rect,
    node_rects: &HashMap<String, Rect>,
    envelopes: &HashMap<String, SubgraphEnvelope>,
) -> Option<isize> {
    let out_degree = graph.edges.iter().filter(|e| e.from == id).count();
    let in_degree = graph.edges.iter().filter(|e| e.to == id).count();
    graph.edges.iter().find_map(|edge| {
        let (other_id, fellows): (&str, Vec<&str>) = if edge.from == id && out_degree == 1 {
            let fellows = graph
                .edges
                .iter()
                .filter(|e| e.to == edge.to && e.from != id)
                .map(|e| e.from.as_str())
                .collect();
            (&edge.to, fellows)
        } else if edge.to == id && in_degree == 1 {
            let fellows = graph
                .edges
                .iter()
                .filter(|e| e.from == edge.from && e.to != id)
                .map(|e| e.to.as_str())
                .collect();
            (&edge.from, fellows)
        } else {
            return None;
        };
        let other = node_rects.get(other_id)?;
        border_centerline_step(graph, (id, rect), (other_id, other), &fellows, envelopes)
    })
}

/// In LR/RL, move a node outside every frame two rows off a frame border
/// its centre row lies on, when the edge would otherwise run along that
/// border (gen_a_229's Resolver Cache, gen_a_207's Compactor), and the room
/// is free. A move can free the room another node needs, so the sweep
/// repeats a few times.
fn clear_frame_border_centerlines(
    graph: &Graph,
    placement: &mut placement::Placement,
    config: &CoarseLayoutConfig,
) {
    if !matches!(graph.direction, Direction::LR | Direction::RL) {
        return;
    }
    let mut ids: Vec<String> = graph
        .nodes
        .iter()
        .filter(|node| graph.get_node_subgraph(&node.id).is_none())
        .map(|node| node.id.clone())
        .collect();
    ids.sort();
    for _ in 0..3 {
        let mut moved_any = false;
        for id in &ids {
            let envelopes = compute_envelopes(graph, &placement.node_rects, config.subgraph_gutter);
            let Some(rect) = placement.node_rects.get(id).copied() else {
                continue;
            };
            let Some(step) = node_border_step(graph, id, &rect, &placement.node_rects, &envelopes)
            else {
                continue;
            };
            if step < 0 && rect.y < step.unsigned_abs() {
                continue;
            }
            let moved = Rect {
                y: shift_coordinate(rect.y, step),
                ..rect
            };
            let clear_of_nodes = placement.node_rects.iter().all(|(other_id, other)| {
                other_id == id || !rects_overlap(&moved.inflate(1), other)
            });
            let clear_of_frames = envelopes
                .values()
                .all(|envelope| !rects_overlap(&moved, &envelope.outer));
            if !clear_of_nodes
                || !clear_of_frames
                || node_border_step(graph, id, &moved, &placement.node_rects, &envelopes).is_some()
            {
                continue;
            }
            placement.node_rects.insert(id.clone(), moved);
            if let Some(position) = placement.positions.get_mut(id) {
                position.y = moved.y;
            }
            placement.canvas.height = placement.canvas.height.max(moved.bottom());
            moved_any = true;
        }
        if !moved_any {
            break;
        }
    }
}

fn shift_coordinate(value: usize, delta: isize) -> usize {
    if delta.is_negative() {
        value.saturating_sub(delta.unsigned_abs())
    } else {
        value.saturating_add(delta as usize)
    }
}

/// Align a terminal external BT source with the one title-safe lane shared by
/// a nested boundary chain.  A source node that exits one cell beside that
/// lane has no spare row for a clean elbow: its turn is written onto the
/// source box's top border and becomes a `+-+`/corner fusion.  The route and
/// portal planners already derive the shared lane from the same topology; the
/// placement stage makes the source centerline agree with it when the source
/// is a single-edge terminal and the move is collision-free.
fn reserve_nested_bt_external_entry_lanes(
    graph: &Graph,
    placement: &mut placement::Placement,
    config: &CoarseLayoutConfig,
) {
    if graph.direction != Direction::BT {
        return;
    }

    let envelopes = compute_envelopes(graph, &placement.node_rects, config.subgraph_gutter);
    let bounds = envelopes
        .iter()
        .map(|(id, envelope)| (id.clone(), envelope.outer))
        .collect::<HashMap<_, _>>();

    for edge in &graph.edges {
        if edge.is_back_edge || edge.kind != crate::graph::EdgeKind::Arrow {
            continue;
        }
        if graph.get_node_subgraph(&edge.from).is_some()
            || graph.get_node_subgraph(&edge.to).is_none()
        {
            continue;
        }
        if graph
            .edges
            .iter()
            .filter(|candidate| !candidate.is_back_edge && candidate.from == edge.from)
            .count()
            != 1
        {
            continue;
        }

        let (exit_subgraphs, enter_subgraphs) = graph.edge_boundary_crossings(&edge.from, &edge.to);
        if !exit_subgraphs.is_empty() || enter_subgraphs.len() < 2 {
            continue;
        }
        let Some(source_rect) = placement.node_rects.get(&edge.from).copied() else {
            continue;
        };
        let desired_x = source_rect.x + source_rect.width / 2;
        let Some(lane) = crate::portals::bt_nested_boundary_lane_with_bounds(
            graph,
            &enter_subgraphs,
            desired_x,
            Some(&bounds),
        ) else {
            continue;
        };
        if lane == desired_x {
            continue;
        }

        let candidate = Rect::new(
            lane.saturating_sub(source_rect.width / 2),
            source_rect.y,
            source_rect.width,
            source_rect.height,
        );
        let collides = placement
            .node_rects
            .iter()
            .filter(|(node_id, _)| node_id.as_str() != edge.from)
            .any(|(_, other)| {
                let candidate = candidate.inflate(1);
                let other = other.inflate(1);
                candidate.x < other.right()
                    && other.x < candidate.right()
                    && candidate.y < other.bottom()
                    && other.y < candidate.bottom()
            });
        if collides {
            continue;
        }

        if let Some(rect) = placement.node_rects.get_mut(&edge.from) {
            *rect = candidate;
        }
        if let Some(position) = placement.positions.get_mut(&edge.from) {
            position.x = candidate.x;
            position.y = candidate.y;
        }
        placement.canvas.width = placement.canvas.width.max(candidate.right());
    }
}

/// Two frames are parallel lanes when one outside node, or one other frame,
/// links to both of them: a fan feeding both, or both feeding one place.
fn frames_are_parallel_lanes(graph: &Graph, a: &str, b: &str) -> bool {
    let outside =
        |id: &str| !graph.is_node_in_subgraph_tree(id, a) && !graph.is_node_in_subgraph_tree(id, b);
    let neighbours = |frame: &str| -> Vec<&str> {
        graph
            .edges
            .iter()
            .filter_map(|edge| {
                let from_in = graph.is_node_in_subgraph_tree(&edge.from, frame);
                let to_in = graph.is_node_in_subgraph_tree(&edge.to, frame);
                match (from_in, to_in) {
                    (true, false) => Some(edge.to.as_str()),
                    (false, true) => Some(edge.from.as_str()),
                    _ => None,
                }
            })
            .filter(|id| outside(id))
            .collect()
    };
    let (of_a, of_b) = (neighbours(a), neighbours(b));
    if of_a.iter().any(|id| of_b.contains(id)) {
        return true;
    }
    graph
        .subgraphs
        .iter()
        .filter(|other| {
            !subgraphs_have_declared_hierarchy(graph, &other.id, a)
                && !subgraphs_have_declared_hierarchy(graph, &other.id, b)
                && other.id != a
                && other.id != b
        })
        .any(|other| {
            of_a.iter()
                .any(|id| graph.is_node_in_subgraph_tree(id, &other.id))
                && of_b
                    .iter()
                    .any(|id| graph.is_node_in_subgraph_tree(id, &other.id))
        })
}

/// A TD/TB line entering a titled frame above a target that sits under the
/// title passes the title in a lane beside it, then turns back on the row
/// under the title, above the arrow row. The settled frame gains the rows it
/// lacks, upward, when they and a row above them are free of boxes, of other
/// frames and of an enclosing frame's title (gen_c_081's Second had only its
/// title row).
fn reserve_title_turn_rows(
    graph: &Graph,
    placement: &mut placement::Placement,
    envelopes: &mut HashMap<String, SubgraphEnvelope>,
) {
    if !matches!(graph.direction, Direction::TD | Direction::TB) {
        return;
    }
    let mut ids: Vec<String> = envelopes.keys().cloned().collect();
    ids.sort();
    for id in ids {
        let Some(subgraph) = graph.get_subgraph(&id) else {
            continue;
        };
        let Some(len) = subgraph
            .title
            .as_deref()
            .map(crate::graph::subgraph_title_len)
        else {
            continue;
        };
        let outer = envelopes[&id].outer;
        let entries: Vec<&Rect> = graph
            .edges
            .iter()
            .filter(|edge| {
                !edge.is_back_edge
                    && !graph.is_node_in_subgraph_tree(&edge.from, &id)
                    && graph.is_node_in_subgraph_tree(&edge.to, &id)
            })
            .filter_map(|edge| placement.node_rects.get(&edge.to))
            .filter(|rect| rect.x + rect.width / 2 <= outer.x + len && rect.y > outer.y)
            .collect();
        // The router's lane one cell past the padded title, else the column
        // beside the left wall; a lane off the target's top edge turns.
        let lane = if outer.x + 2 + len + 2 < outer.right() {
            outer.x + 2 + len
        } else {
            outer.x + 1
        };
        let turning: Vec<&&Rect> = entries
            .iter()
            .filter(|rect| lane < rect.x + 1 || lane + 2 > rect.right())
            .collect();
        if turning.is_empty() {
            continue;
        }
        let needed = 4;
        let top = turning
            .iter()
            .map(|rect| rect.y)
            .min()
            .unwrap_or(outer.y)
            .saturating_sub(outer.y);
        if top >= needed {
            continue;
        }
        let extra = needed - top;
        let band = Rect::new(outer.x, outer.y - extra - 1, outer.width, extra + 1);
        let meets = |rect: &Rect| {
            rect.x < band.right()
                && band.x < rect.right()
                && rect.y < band.bottom()
                && band.y < rect.bottom()
        };
        let boxes_clear = placement.node_rects.values().all(|rect| !meets(rect));
        let frames_clear = envelopes.iter().all(|(other_id, envelope)| {
            if other_id == &id {
                return true;
            }
            if graph.is_subgraph_ancestor(other_id, &id) {
                return outer.y - extra >= envelope.outer.y + 3;
            }
            !meets(&envelope.outer)
        });
        if outer.y > extra && boxes_clear && frames_clear {
            let envelope = envelopes.get_mut(&id).expect("envelope");
            envelope.outer.y -= extra;
            envelope.outer.height += extra;
            continue;
        }
        // No room above: insert the rows under the title, moving everything
        // from there down and growing every frame across that row.
        let row = outer.y + 2;
        for (node_id, rect) in placement.node_rects.iter_mut() {
            if rect.y >= row {
                rect.y += extra;
                if let Some(point) = placement.positions.get_mut(node_id) {
                    point.y = rect.y;
                }
            }
        }
        for envelope in envelopes.values_mut() {
            for rect in [&mut envelope.outer, &mut envelope.inner] {
                if rect.y >= row {
                    rect.y += extra;
                } else if rect.bottom() > row {
                    rect.height += extra;
                }
            }
        }
        placement.canvas.height += extra;
    }
}

/// A BT back edge whose run along its source's row would cross a box leaves
/// through the source's top border and runs to the right gutter two rows
/// above the top rank (`plan_bt_cycle_top_exits`). In a graph without frames
/// those two rows are kept free above everything.
fn reserve_bt_back_edge_exit_rows(
    graph: &Graph,
    placement: &mut placement::Placement,
    envelopes: &mut HashMap<String, SubgraphEnvelope>,
) {
    if graph.direction != Direction::BT || !graph.subgraphs.is_empty() {
        return;
    }
    let boxes: Vec<(usize, usize, usize, usize)> = placement
        .node_rects
        .values()
        .map(|rect| (rect.x, rect.y, rect.width, rect.height))
        .collect();
    let blocked = graph
        .edges
        .iter()
        .filter(|edge| edge.is_back_edge && edge.from != edge.to)
        .filter_map(|edge| placement.node_rects.get(&edge.from))
        .any(|from| {
            crate::render::cycle::bt_back_edge_row_blocked(
                (from.x, from.y, from.width, from.height),
                boxes.iter().copied(),
            )
        });
    let top = placement.node_rects.values().map(|rect| rect.y).min();
    let Some(extra) = top
        .filter(|_| blocked)
        .map(|top| 2usize.saturating_sub(top))
    else {
        return;
    };
    if extra == 0 {
        return;
    }
    for (id, rect) in placement.node_rects.iter_mut() {
        rect.y += extra;
        if let Some(point) = placement.positions.get_mut(id) {
            point.y = rect.y;
        }
    }
    for envelope in envelopes.values_mut() {
        envelope.outer.y += extra;
        envelope.inner.y += extra;
    }
    placement.canvas.height += extra;
}

/// In an LR/RL graph without frames, a back edge's lane runs under the boxes
/// and its legs drop from its ends' centre columns; a box stacked under an
/// end in that column hides the leg (gen_a_009: Tune -> Train rises through
/// Validation Set). When that box has the same lines in and out as the end
/// and no back edge of its own, the two trade places: no other line changes.
/// In TD/BT a twin shares its end's rank row, so none stands under it.
fn sink_lr_back_edge_ends_below_twins(graph: &Graph, placement: &mut placement::Placement) {
    if !graph.subgraphs.is_empty() {
        return;
    }
    let forward = |id: &str| {
        let mut ins: Vec<&str> = graph
            .edges
            .iter()
            .filter(|edge| !edge.is_back_edge && edge.to == id)
            .map(|edge| edge.from.as_str())
            .collect();
        let mut outs: Vec<&str> = graph
            .edges
            .iter()
            .filter(|edge| !edge.is_back_edge && edge.from == id)
            .map(|edge| edge.to.as_str())
            .collect();
        ins.sort_unstable();
        outs.sort_unstable();
        (ins, outs)
    };
    let back_ends: Vec<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.is_back_edge)
        .flat_map(|edge| [edge.from.as_str(), edge.to.as_str()])
        .collect();
    let rect = |rect: &crate::geom::Rect| (rect.x, rect.y, rect.width, rect.height.max(3));
    let boxes: Vec<(usize, usize, usize, usize)> =
        placement.node_rects.values().map(rect).collect();
    let under: Vec<&str> = graph
        .edges
        .iter()
        .filter(|edge| edge.is_back_edge)
        .filter(|edge| {
            match (
                placement.node_rects.get(&edge.from),
                placement.node_rects.get(&edge.to),
            ) {
                (Some(from), Some(to)) => {
                    !crate::render::cycle::lr_back_edge_over_the_top(rect(from), rect(to), &boxes)
                }
                _ => false,
            }
        })
        .flat_map(|edge| [edge.from.as_str(), edge.to.as_str()])
        .collect();
    for end in &under {
        let Some(&upper) = placement.node_rects.get(*end) else {
            continue;
        };
        let leg = upper.x + upper.width / 2;
        let Some((twin, lower)) = placement
            .node_rects
            .iter()
            .filter(|(_, rect)| rect.y > upper.y && (rect.x..rect.x + rect.width).contains(&leg))
            .min_by_key(|(_, rect)| rect.y)
            .map(|(id, rect)| (id.clone(), *rect))
        else {
            continue;
        };
        if back_ends.contains(&twin.as_str()) || forward(&twin) != forward(end) {
            continue;
        }
        let gap = lower.y - upper.bottom();
        let moves = [
            (twin, upper.y),
            (end.to_string(), upper.y + lower.height + gap),
        ];
        for (id, y) in moves {
            if let Some(rect) = placement.node_rects.get_mut(&id) {
                rect.y = y;
            }
            if let Some(point) = placement.positions.get_mut(&id) {
                point.y = y;
            }
        }
    }
}

/// In an LR/RL graph without frames, a back edge whose route under the boxes
/// would cross one goes over the top when nothing stands above its ends
/// (`lr_back_edge_over_the_top`). The rows its lane needs are kept free above
/// everything.
fn reserve_lr_back_edge_top_rows(graph: &Graph, placement: &mut placement::Placement) {
    if !matches!(graph.direction, Direction::LR | Direction::RL) || !graph.subgraphs.is_empty() {
        return;
    }
    let rect = |rect: &crate::geom::Rect| (rect.x, rect.y, rect.width, rect.height.max(3));
    let boxes: Vec<(usize, usize, usize, usize)> =
        placement.node_rects.values().map(rect).collect();
    let over = graph
        .edges
        .iter()
        .filter(|edge| edge.is_back_edge && edge.from != edge.to)
        .filter(|edge| {
            match (
                placement.node_rects.get(&edge.from),
                placement.node_rects.get(&edge.to),
            ) {
                (Some(from), Some(to)) => {
                    crate::render::cycle::lr_back_edge_over_the_top(rect(from), rect(to), &boxes)
                }
                _ => false,
            }
        })
        .count();
    let top = placement
        .node_rects
        .values()
        .map(|rect| rect.y)
        .min()
        .unwrap_or(0);
    let extra = crate::render::cycle::lr_top_lane_rows(over).saturating_sub(top);
    if extra == 0 {
        return;
    }
    for (id, rect) in placement.node_rects.iter_mut() {
        rect.y += extra;
        if let Some(point) = placement.positions.get_mut(id) {
            point.y += extra;
        }
    }
    placement.canvas.height += extra;
}

/// In an LR/RL graph without frames, a back edge that cannot go over the top
/// runs on a lane `min_vertical_spacing` rows under its lower end; a box
/// standing above that lane in one of its ends' columns is crossed by that
/// end's leg (gen_d_120: Y -> B into Z, with X and A above). Open rows from
/// that box down so the lane passes clear, with a blank row before the box.
fn reserve_lr_back_edge_lane_rows(
    graph: &Graph,
    config: &CoarseLayoutConfig,
    placement: &mut placement::Placement,
) {
    if !matches!(graph.direction, Direction::LR | Direction::RL) || !graph.subgraphs.is_empty() {
        return;
    }
    let rect = |rect: &crate::geom::Rect| (rect.x, rect.y, rect.width, rect.height.max(3));
    for edge in graph.edges.iter().filter(|edge| edge.is_back_edge) {
        let boxes: Vec<(usize, usize, usize, usize)> =
            placement.node_rects.values().map(rect).collect();
        let (Some(&from), Some(&to)) = (
            placement.node_rects.get(&edge.from),
            placement.node_rects.get(&edge.to),
        ) else {
            continue;
        };
        if crate::render::cycle::lr_back_edge_over_the_top(rect(&from), rect(&to), &boxes) {
            continue;
        }
        let center = |rect: &crate::geom::Rect| rect.x + rect.width / 2;
        let legs = [center(&from), center(&to)];
        let ends_top = from.y.max(to.y);
        let lane = from.bottom().max(to.bottom()) + config.min_vertical_spacing.max(2);
        let Some(top) = placement
            .node_rects
            .values()
            .filter(|rect| rect.y > ends_top)
            .filter(|rect| {
                legs.iter()
                    .any(|&leg| (rect.x..rect.x + rect.width).contains(&leg))
            })
            .map(|rect| rect.y)
            .min()
        else {
            continue;
        };
        let extra = (lane + 2).saturating_sub(top);
        for (id, rect) in placement.node_rects.iter_mut() {
            if rect.y >= top {
                rect.y += extra;
                if let Some(point) = placement.positions.get_mut(id) {
                    point.y += extra;
                }
            }
        }
        placement.canvas.height += extra;
    }
}

/// A fan-in source's line drops straight down its centre column to the
/// merge above its target. An edge that skips ranks has no node of its own
/// on the ranks between, so a box placed there in that column hides the line
/// (gen_a_045's Score above 0.8? -> Approve through Manual Review). In a
/// TD/BT graph without frames, for a fan-in with a labelled line in (an
/// unlabelled one has renderer lanes of its own, fx_shape_database_td),
/// every such box moves just clear of the column, to the nearer side, when
/// all of them can without meeting another box; a box whose own labelled
/// line runs into the target stays, since its turn would take the label
/// (gen_d_023), and so then do the others (gen_a_070).
fn clear_skip_edge_columns(graph: &Graph, placement: &mut placement::Placement) {
    if !matches!(
        graph.direction,
        Direction::TD | Direction::TB | Direction::BT
    ) || !graph.subgraphs.is_empty()
    {
        return;
    }
    let fan_in = |id: &str| {
        let into: Vec<&crate::graph::Edge> = graph
            .edges
            .iter()
            .filter(|edge| !edge.is_back_edge && edge.to == id)
            .collect();
        into.len() > 1 && into.iter().any(|edge| edge.label.is_some())
    };
    let mut skips: Vec<(&str, &str, usize, usize)> = graph
        .edges
        .iter()
        .filter(|edge| !edge.is_back_edge && edge.from != edge.to && fan_in(&edge.to))
        .filter_map(|edge| {
            let from = *placement.ranks.get(&edge.from)?;
            let to = *placement.ranks.get(&edge.to)?;
            (to >= from + 2).then_some((edge.from.as_str(), edge.to.as_str(), from, to))
        })
        .collect();
    skips.sort();
    let mut ids: Vec<String> = placement.node_rects.keys().cloned().collect();
    ids.sort();
    for (from_id, to_id, from_rank, to_rank) in skips {
        let Some(source) = placement.node_rects.get(from_id).copied() else {
            continue;
        };
        let column = source.x + source.width / 2;
        let mut rects = placement.node_rects.clone();
        let mut moves: Vec<(String, Rect)> = Vec::new();
        let mut movable = true;
        for id in &ids {
            if id == from_id || id == to_id {
                continue;
            }
            let Some(rank) = placement.ranks.get(id).copied() else {
                continue;
            };
            let rect = rects[id];
            if rank <= from_rank || rank >= to_rank || column + 1 < rect.x || column > rect.right()
            {
                continue;
            }
            if graph
                .edges
                .iter()
                .any(|edge| edge.from == *id && edge.to == to_id && edge.label.is_some())
            {
                movable = false;
                break;
            }
            let right = Rect {
                x: column + 2,
                ..rect
            };
            let left = (rect.right() + 1)
                .checked_sub(column)
                .and_then(|delta| rect.x.checked_sub(delta))
                .map(|x| Rect { x, ..rect });
            let options = match left {
                Some(left) if left.x.abs_diff(rect.x) < right.x.abs_diff(rect.x) => {
                    [Some(left), Some(right)]
                }
                left => [Some(right), left],
            };
            let Some(moved) = options.into_iter().flatten().find(|moved| {
                rects.iter().all(|(other_id, other)| {
                    other_id == id || !rects_overlap(&moved.inflate(1), other)
                })
            }) else {
                movable = false;
                break;
            };
            rects.insert(id.clone(), moved);
            moves.push((id.clone(), moved));
        }
        if !movable {
            continue;
        }
        for (id, moved) in moves {
            placement.node_rects.insert(id.clone(), moved);
            if let Some(point) = placement.positions.get_mut(&id) {
                point.x = moved.x;
            }
            placement.canvas.width = placement.canvas.width.max(moved.right());
        }
    }
}
