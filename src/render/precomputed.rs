//! Explicit precomputed-route projection.
//!
//! This module owns the route-plan-to-canvas adapter for callers that provide
//! axis-aligned segments. Fallback routing remains in the legacy pipeline.

use std::collections::HashSet;

use super::canvas;
use super::provenance::edge_owner_id;
use super::scene::{Scene, SceneIntent, SceneRecorder};
use super::semantic::{CellOwnerKind, CellRole};
use super::subgraph_title_y;
use super::Canvas;
use crate::geom::Segment;
use crate::graph::{Direction, EdgeKind, Graph};
use crate::indexed_graph::EdgeId;
use crate::layout_snapshot::LayoutSnapshot;
use crate::style::StyleChars;

// ============================================================================
// Precomputed Edge Route Rendering (experimental)
// ============================================================================

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

const PRECOMPUTED_ROUTE_Z_INDEX: u8 = 5;

#[derive(Copy, Clone)]
struct PrecomputedRouteOwner<'a> {
    kind: CellOwnerKind,
    id: &'a str,
}

fn dir_from_segment(seg: &Segment) -> Option<Dir> {
    if seg.from.x == seg.to.x {
        if seg.to.y > seg.from.y {
            Some(Dir::Down)
        } else if seg.to.y < seg.from.y {
            Some(Dir::Up)
        } else {
            None
        }
    } else if seg.from.y == seg.to.y {
        if seg.to.x > seg.from.x {
            Some(Dir::Right)
        } else if seg.to.x < seg.from.x {
            Some(Dir::Left)
        } else {
            None
        }
    } else {
        None
    }
}

fn opposite_dir(d: Dir) -> Dir {
    match d {
        Dir::Up => Dir::Down,
        Dir::Down => Dir::Up,
        Dir::Left => Dir::Right,
        Dir::Right => Dir::Left,
    }
}

fn corner_for_turn(prev: Dir, next: Dir, chars: &StyleChars) -> Option<char> {
    use Dir::*;
    let a = opposite_dir(prev);
    let b = next;
    // Corner character needed based on the two arms (where we came from, where we're going):
    // ┘ (corner_ur) = UP + LEFT arms
    // └ (corner_ul) = UP + RIGHT arms
    // ┐ (corner_dr) = DOWN + LEFT arms
    // ┌ (corner_dl) = DOWN + RIGHT arms
    match (a, b) {
        (Up, Left) | (Left, Up) => Some(chars.corner_ur), // ┘
        (Up, Right) | (Right, Up) => Some(chars.corner_ul), // └
        (Down, Left) | (Left, Down) => Some(chars.corner_dr), // ┐
        (Down, Right) | (Right, Down) => Some(chars.corner_dl), // ┌
        _ => None,
    }
}

fn arrow_for_dir(dir: Dir, chars: &StyleChars) -> char {
    match dir {
        Dir::Up => chars.arrow_up,
        Dir::Down => chars.arrow_down,
        Dir::Left => chars.arrow_left,
        Dir::Right => chars.arrow_right,
    }
}

pub(super) fn is_subgraph_title_cell(graph: &Graph, x: usize, y: usize) -> bool {
    graph.subgraphs.iter().any(|sg| {
        if sg.title.is_none() || !sg.bounds.is_valid() {
            return false;
        }
        let title_y = subgraph_title_y(&sg.bounds, graph.direction);
        y == title_y && x >= sg.bounds.x && x < sg.bounds.x.saturating_add(sg.bounds.width)
    })
}

#[allow(clippy::too_many_arguments)]
fn draw_segment(
    seg: &Segment,
    dir: Dir,
    canvas: &mut Canvas,
    chars: &StyleChars,
    skip_start: bool,
    skip_end: bool,
    graph: &Graph,
    owner: PrecomputedRouteOwner<'_>,
) {
    match dir {
        Dir::Left | Dir::Right => {
            let (min, max) = if seg.from.x <= seg.to.x {
                (seg.from.x, seg.to.x)
            } else {
                (seg.to.x, seg.from.x)
            };

            // Apply adjustments based on which end is 'start' and 'end'
            // If moving Right (from=min), skip_start increases min, skip_end decreases max
            // If moving Left (from=max), skip_start decreases max, skip_end increases min

            let (draw_start, draw_end) = if seg.from.x == min {
                // Moving Right
                (
                    min + if skip_start { 1 } else { 0 },
                    max.saturating_sub(if skip_end { 1 } else { 0 }),
                )
            } else {
                // Moving Left
                (
                    min + if skip_end { 1 } else { 0 },
                    max.saturating_sub(if skip_start { 1 } else { 0 }),
                )
            };

            if draw_start <= draw_end {
                for x in draw_start..=draw_end {
                    if is_subgraph_title_cell(graph, x, seg.from.y) {
                        continue;
                    }
                    set_precomputed_route_edge_char(
                        canvas,
                        x,
                        seg.from.y,
                        chars.edge_h,
                        chars,
                        owner,
                    );
                }
            }
        }
        Dir::Up | Dir::Down => {
            let (min, max) = if seg.from.y <= seg.to.y {
                (seg.from.y, seg.to.y)
            } else {
                (seg.to.y, seg.from.y)
            };

            let (draw_start, draw_end) = if seg.from.y == min {
                // Moving Down
                (
                    min + if skip_start { 1 } else { 0 },
                    max.saturating_sub(if skip_end { 1 } else { 0 }),
                )
            } else {
                // Moving Up
                (
                    min + if skip_end { 1 } else { 0 },
                    max.saturating_sub(if skip_start { 1 } else { 0 }),
                )
            };

            if draw_start <= draw_end {
                for y in draw_start..=draw_end {
                    if is_subgraph_title_cell(graph, seg.from.x, y) {
                        continue;
                    }
                    set_precomputed_route_edge_char(
                        canvas,
                        seg.from.x,
                        y,
                        chars.edge_v,
                        chars,
                        owner,
                    );
                }
            }
        }
    }
}

pub(super) fn draw_routes(
    graph: &Graph,
    layout_snapshot: &LayoutSnapshot,
    canvas: &mut Canvas,
    chars: &StyleChars,
    recorder: &mut SceneRecorder,
    skipped_edge_indices: &HashSet<usize>,
) {
    let debug_timing = crate::runtime::current().diagnostics.timing;
    let mut edge_ids: Vec<EdgeId> = layout_snapshot.route_ids().collect();
    edge_ids.sort_unstable();
    let mut marker_scene = Scene::new();

    for edge_id in edge_ids {
        let edge_idx = edge_id.index();
        if skipped_edge_indices.contains(&edge_idx) {
            continue;
        }
        let Some(route) = layout_snapshot.route(edge_id) else {
            continue;
        };
        if route.segments.is_empty() {
            continue;
        }

        let Some(edge) = graph.edges.get(edge_idx) else {
            continue;
        };
        let owner_id = edge_owner_id(edge_idx, edge);
        let owner = PrecomputedRouteOwner {
            kind: if edge.is_back_edge {
                CellOwnerKind::CycleEdge
            } else {
                CellOwnerKind::EdgeSegment
            },
            id: owner_id.as_str(),
        };
        let (Some(from), Some(to)) = (graph.get_node(&edge.from), graph.get_node(&edge.to)) else {
            continue;
        };
        if !canvas.is_visible(from) || !canvas.is_visible(to) {
            continue;
        }

        // Apply edge-kind-specific shaft characters.
        let mut route_chars = *chars;
        match edge.kind {
            EdgeKind::Arrow
            | EdgeKind::Open
            | EdgeKind::Bidirectional
            | EdgeKind::CircleEnd
            | EdgeKind::CrossEnd => {} // use default edge chars
            EdgeKind::Thick => {
                // Heavy/bold shaft chars
                route_chars.edge_h = '━';
                route_chars.edge_v = '┃';
            }
            EdgeKind::Dotted => {
                route_chars.edge_h = chars.dotted_h;
                route_chars.edge_v = chars.dotted_v;
            }
        }
        // Back-edges always override with cycle styling.
        if edge.is_back_edge {
            route_chars.edge_h = chars.back_h;
            route_chars.edge_v = chars.back_v;
        }

        // Track if we need to draw a corner for perpendicular first segment
        let mut needs_start_corner: Option<(usize, usize, char)> = None;

        // Draw stem from source node exit to route start if the route starts
        // with a perpendicular segment (horizontal in TD/BT, vertical in LR/RL).
        // This handles cases where the route detours before dropping to target.
        if let Some(first_seg) = route.segments.first() {
            let first_dir = dir_from_segment(first_seg);
            let route_start = first_seg.from;
            let src_center_x = from.center_x();
            let src_center_y = from.center_y();

            match graph.direction {
                Direction::TD | Direction::TB => {
                    // In TD/BT, first segment should be vertical (Down/Up)
                    // If it's horizontal (Left/Right), we need a connecting stem and corner
                    if matches!(first_dir, Some(Dir::Left) | Some(Dir::Right)) {
                        // Box border is at y = from.y + from.height - 1
                        // We need to draw from box border down to route start to create junction
                        let box_border_y = from.y + from.height - 1;
                        if debug_timing {
                            eprintln!(
                                "  TD horizontal-first: src_center_x={} box_border_y={} route_start.y={}",
                                src_center_x, box_border_y, route_start.y
                            );
                        }
                        // Draw vertical stem from box border to the route start row (exclusive)
                        // This will create a junction on the box border via resolve_overlap
                        for y in box_border_y..route_start.y {
                            if debug_timing {
                                eprintln!("    drawing stem at ({src_center_x}, {y})");
                            }
                            set_precomputed_route_edge_char(
                                canvas,
                                src_center_x,
                                y,
                                route_chars.edge_v,
                                &route_chars,
                                owner,
                            );
                        }
                        // Queue corner to be drawn AFTER segments (so it overwrites)
                        // At the source center, we need a corner character that connects:
                        // - UP (to the box border junction above)
                        // - LEFT/RIGHT (horizontal segment to turn point)
                        // Use corner characters ┘ (up/left) or └ (up/right) - no down arm needed
                        let corner = if first_dir == Some(Dir::Left) {
                            route_chars.corner_ur // ┘ - connects up, left
                        } else {
                            route_chars.corner_ul // └ - connects up, right
                        };
                        if debug_timing {
                            eprintln!(
                                "    needs_start_corner=({}, {}, '{}')",
                                src_center_x, route_start.y, corner
                            );
                        }
                        needs_start_corner = Some((src_center_x, route_start.y, corner));
                    }
                }
                Direction::BT => {
                    if matches!(first_dir, Some(Dir::Left) | Some(Dir::Right)) {
                        // Box top border is at y = from.y (for BT, edges exit from top)
                        // Draw vertical stem from route start up to box border (inclusive)
                        let box_border_y = from.y;
                        for y in (route_start.y + 1)..=box_border_y {
                            set_precomputed_route_edge_char(
                                canvas,
                                src_center_x,
                                y,
                                route_chars.edge_v,
                                &route_chars,
                                owner,
                            );
                        }
                        let corner = if first_dir == Some(Dir::Left) {
                            route_chars.corner_ur // ┘ - going left from here
                        } else {
                            route_chars.corner_ul // └ - going right from here
                        };
                        needs_start_corner = Some((src_center_x, route_start.y, corner));
                    }
                }
                Direction::LR => {
                    if matches!(first_dir, Some(Dir::Up) | Some(Dir::Down)) {
                        let exit_x = from.x + from.width;
                        for x in exit_x..route_start.x {
                            set_precomputed_route_edge_char(
                                canvas,
                                x,
                                src_center_y,
                                route_chars.edge_h,
                                &route_chars,
                                owner,
                            );
                        }
                    }
                }
                Direction::RL => {
                    if matches!(first_dir, Some(Dir::Up) | Some(Dir::Down)) {
                        let exit_x = from.x.saturating_sub(1);
                        for x in (route_start.x + 1)..=exit_x {
                            set_precomputed_route_edge_char(
                                canvas,
                                x,
                                src_center_y,
                                route_chars.edge_h,
                                &route_chars,
                                owner,
                            );
                        }
                    }
                }
            }
        }

        for i in 0..route.segments.len() {
            let seg = &route.segments[i];
            let Some(dir) = dir_from_segment(seg) else {
                continue;
            };

            let mut next_dir = None;
            if i + 1 < route.segments.len() {
                next_dir = dir_from_segment(&route.segments[i + 1]);
            }

            let is_turn = if let Some(nd) = next_dir {
                nd != dir
            } else {
                false
            };

            let skip_start = i > 0;
            let skip_end = is_turn;

            draw_segment(
                seg,
                dir,
                canvas,
                &route_chars,
                skip_start,
                skip_end,
                graph,
                owner,
            );

            if is_turn {
                if let Some(nd) = next_dir {
                    if let Some(corner) = corner_for_turn(dir, nd, &route_chars) {
                        if !is_subgraph_title_cell(graph, seg.to.x, seg.to.y) {
                            set_precomputed_route_edge_char(
                                canvas,
                                seg.to.x,
                                seg.to.y,
                                corner,
                                &route_chars,
                                owner,
                            );
                        }
                    }
                }
            }
        }

        if let Some(last_seg) = route.segments.last() {
            let dir = dir_from_segment(last_seg).unwrap_or(match graph.direction {
                Direction::TD | Direction::TB => Dir::Down,
                Direction::BT => Dir::Up,
                Direction::LR => Dir::Right,
                Direction::RL => Dir::Left,
            });
            // Determine the terminal cell character based on edge kind.
            if !is_subgraph_title_cell(graph, last_seg.to.x, last_seg.to.y) {
                let tip = if edge.kind == EdgeKind::Open {
                    // Open links: draw shaft char (no end marker)
                    match dir {
                        Dir::Left | Dir::Right => route_chars.edge_h,
                        Dir::Up | Dir::Down => route_chars.edge_v,
                    }
                } else if edge.kind == EdgeKind::CircleEnd {
                    chars.circle_end // non-directional circle marker
                } else if edge.kind == EdgeKind::CrossEnd {
                    chars.cross_end // non-directional cross marker
                } else {
                    arrow_for_dir(dir, &route_chars)
                };
                let role = if canvas::is_arrow(tip) {
                    CellRole::ArrowTip
                } else if matches!(edge.kind, EdgeKind::CircleEnd | EdgeKind::CrossEnd) {
                    CellRole::EndpointMarker
                } else if canvas::is_horizontal(tip, &route_chars) {
                    CellRole::Horizontal
                } else if canvas::is_vertical(tip, &route_chars) {
                    CellRole::Vertical
                } else {
                    CellRole::Text
                };
                marker_scene.push(SceneIntent::owned(
                    last_seg.to.x,
                    last_seg.to.y,
                    tip,
                    owner.kind,
                    owner.id,
                    role,
                    PRECOMPUTED_ROUTE_Z_INDEX.saturating_add(1),
                ));
            }
        }

        // For bidirectional edges, draw a reverse arrowhead at the route start.
        if edge.kind == EdgeKind::Bidirectional {
            if let Some(first_seg) = route.segments.first() {
                if let Some(fwd) = dir_from_segment(first_seg) {
                    let rev = match fwd {
                        Dir::Up => Dir::Down,
                        Dir::Down => Dir::Up,
                        Dir::Left => Dir::Right,
                        Dir::Right => Dir::Left,
                    };
                    let rev_arrow = arrow_for_dir(rev, &route_chars);
                    marker_scene.push(SceneIntent::owned(
                        first_seg.from.x,
                        first_seg.from.y,
                        rev_arrow,
                        owner.kind,
                        owner.id,
                        CellRole::ArrowTip,
                        PRECOMPUTED_ROUTE_Z_INDEX.saturating_add(1),
                    ));
                }
            }
        }

        // Draw start corner AFTER segments so it overwrites the horizontal line
        if let Some((x, y, corner)) = needs_start_corner {
            set_precomputed_route_char(canvas, x, y, corner, owner);
        }
    }

    if !marker_scene.is_empty() {
        marker_scene.resolve_with_recorder(canvas, chars, recorder, "precomputed-route-markers");
    }
}

fn set_precomputed_route_char(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    ch: char,
    owner: PrecomputedRouteOwner<'_>,
) {
    canvas.set_owned(x, y, ch, owner.kind, owner.id, PRECOMPUTED_ROUTE_Z_INDEX);
}

fn set_precomputed_route_edge_char(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    ch: char,
    chars: &StyleChars,
    owner: PrecomputedRouteOwner<'_>,
) {
    canvas.set_edge_char_owned(
        x,
        y,
        ch,
        chars,
        owner.kind,
        owner.id,
        PRECOMPUTED_ROUTE_Z_INDEX,
    );
}
