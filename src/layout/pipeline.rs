use super::envelope_stage::resolve_subgraph_envelopes;
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
    resolve_subgraph_envelopes(&input, &config, &mut placement, debug_timing);
    reserve_sibling_subgraph_target_corridor(input.graph, &mut placement, &config);
    reserve_nested_bt_external_entry_lanes(input.graph, &mut placement, &config);
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
