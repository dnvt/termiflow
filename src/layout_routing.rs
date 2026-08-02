//! Subgraph portal preparation and obstacle-aware Manhattan routing.
//!
//! This private module owns the route-planning state used by the coarse layout
//! engine. Its parent-facing surface is deliberately limited to the calls made
//! by the layout pipeline; the search implementation remains private here.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use crate::geom::{EdgeRoute, Point, Rect};
use crate::graph::{Direction, Graph};
use crate::orientation::{Axis, OrientedCoords};
use crate::portals::SubgraphEnvelope;

// -----------------------------------------------------------------------------
// Subgraphs
// -----------------------------------------------------------------------------

pub(super) fn gutters_to_avoid(
    graph: &Graph,
    subgraph_envelopes: &HashMap<String, SubgraphEnvelope>,
    _edge_idx: usize,
    from: &str,
    to: &str,
) -> Vec<Rect> {
    // Skip gutters that contain either endpoint to avoid blocking exits.
    let mut avoid = Vec::new();
    for (sg_id, bounds) in subgraph_envelopes {
        let contains_endpoint = graph
            .node_subgraph
            .get(from)
            .map(|id| id == sg_id)
            .unwrap_or(false)
            || graph
                .node_subgraph
                .get(to)
                .map(|id| id == sg_id)
                .unwrap_or(false);
        if !contains_endpoint {
            avoid.push(bounds.outer);
        }
    }
    avoid
}

pub(super) fn mark_subgraph_rings(
    grid: &mut OccupancyGrid,
    subgraphs: &HashMap<String, SubgraphEnvelope>,
) {
    for bounds in subgraphs.values() {
        let outer = bounds.outer;
        let inner = bounds.inner;
        if outer.is_empty() || inner.is_empty() {
            continue;
        }

        // Top band
        if inner.y > outer.y {
            grid.mark_rect(&Rect::new(
                outer.x,
                outer.y,
                outer.width,
                inner.y.saturating_sub(outer.y),
            ));
        }
        // Bottom band
        if outer.bottom() > inner.bottom() {
            grid.mark_rect(&Rect::new(
                outer.x,
                inner.bottom(),
                outer.width,
                outer.bottom().saturating_sub(inner.bottom()),
            ));
        }
        // Left band
        if inner.x > outer.x {
            grid.mark_rect(&Rect::new(
                outer.x,
                inner.y,
                inner.x.saturating_sub(outer.x),
                inner.height,
            ));
        }
        // Right band
        if outer.right() > inner.right() {
            grid.mark_rect(&Rect::new(
                inner.right(),
                inner.y,
                outer.right().saturating_sub(inner.right()),
                inner.height,
            ));
        }
    }
}

pub(super) fn carve_node_portals(
    grid: &mut OccupancyGrid,
    node_rects: &HashMap<String, Rect>,
    coords: &OrientedCoords,
    padding: usize,
    graph: &Graph,
    subgraph_envelopes: &HashMap<String, SubgraphEnvelope>,
) {
    let ring_zones: Vec<&SubgraphEnvelope> = subgraph_envelopes.values().collect();

    for (node_id, rect) in node_rects {
        let entry = edge_entry_point(*rect, coords.direction);
        let exit = edge_exit_point(*rect, coords.direction);

        let (allowed_rect, in_subgraph) = graph
            .get_node_subgraph(node_id)
            .and_then(|sg_id| subgraph_envelopes.get(sg_id))
            .map(|b| (b.inner.inflate(padding.max(1)), true))
            .unwrap_or_else(|| (Rect::new(0, 0, grid.width, grid.height), false));

        // Determine clearing direction based on layout direction
        // Entry clears OUTWARDS from the box (opposite to flow into box)
        // Exit clears OUTWARDS from the box (with flow out of box)
        let (entry_dir, exit_dir) = match coords.direction {
            Direction::TD | Direction::TB => ((0, -1), (0, 1)),
            Direction::BT => ((0, 1), (0, -1)),
            Direction::LR => ((-1, 0), (1, 0)),
            Direction::RL => ((1, 0), (-1, 0)),
        };

        for i in 0..=padding {
            // Clear entry path
            if !in_subgraph {
                let ex = if entry_dir.0 < 0 {
                    entry.x.saturating_sub((-entry_dir.0 * i as isize) as usize)
                } else {
                    entry.x.saturating_add((entry_dir.0 * i as isize) as usize)
                };
                let ey = if entry_dir.1 < 0 {
                    entry.y.saturating_sub((-entry_dir.1 * i as isize) as usize)
                } else {
                    entry.y.saturating_add((entry_dir.1 * i as isize) as usize)
                };
                let entry_point = Point::new(ex, ey);
                let in_ring = ring_zones
                    .iter()
                    .any(|b| b.outer.contains(entry_point) && !b.inner.contains(entry_point));
                if allowed_rect.contains(entry_point) && !in_ring {
                    grid.clear_point(entry_point);
                }
            }

            // Clear exit path
            let xx = if exit_dir.0 < 0 {
                exit.x.saturating_sub((-exit_dir.0 * i as isize) as usize)
            } else {
                exit.x.saturating_add((exit_dir.0 * i as isize) as usize)
            };
            let xy = if exit_dir.1 < 0 {
                exit.y.saturating_sub((-exit_dir.1 * i as isize) as usize)
            } else {
                exit.y.saturating_add((exit_dir.1 * i as isize) as usize)
            };
            let exit_point = Point::new(xx, xy);
            let in_ring = ring_zones
                .iter()
                .any(|b| b.outer.contains(exit_point) && !b.inner.contains(exit_point));
            if allowed_rect.contains(exit_point) && !in_ring {
                grid.clear_point(exit_point);
            }
        }
    }
}

pub(super) fn carve_subgraph_portals(
    grid: &mut OccupancyGrid,
    subgraphs: &HashMap<String, SubgraphEnvelope>,
    gutter: usize,
) {
    let debug_timing = std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok();

    let span = gutter.max(1) * 2 + 1;
    for (sg_id, bounds) in subgraphs {
        let portals = &bounds.portals;
        let clamp_h = |x: usize| {
            let min = bounds.outer.x.saturating_add(1);
            let max = bounds.outer.right().saturating_sub(2);
            x.clamp(min, max)
        };
        let clamp_v = |y: usize| {
            let min = bounds.outer.y.saturating_add(1);
            let max = bounds.outer.bottom().saturating_sub(2);
            y.clamp(min, max)
        };
        let half = span / 2;

        for &x in &portals.top {
            let cx = clamp_h(x);
            let start_x = cx.saturating_sub(half);
            let end_x = start_x + span;
            for y in bounds.outer.y..=bounds.inner.y {
                for xi in start_x..end_x {
                    grid.clear_point(Point::new(xi, y));
                }
            }
        }
        for &x in &portals.bottom {
            let cx = clamp_h(x);
            let start_x = cx.saturating_sub(half);
            let end_x = start_x + span;
            for y in bounds.inner.bottom()..=bounds.outer.bottom().saturating_sub(1) {
                for xi in start_x..end_x {
                    grid.clear_point(Point::new(xi, y));
                }
            }
        }
        for &y in &portals.left {
            let cy = clamp_v(y);
            let start_y = cy.saturating_sub(half);
            let end_y = start_y + span;
            for x in bounds.outer.x..=bounds.inner.x {
                for yi in start_y..end_y {
                    grid.clear_point(Point::new(x, yi));
                }
            }
        }
        for &y in &portals.right {
            let cy = clamp_v(y);
            let start_y = cy.saturating_sub(half);
            let end_y = start_y + span;
            for x in bounds.inner.right()..=bounds.outer.right().saturating_sub(1) {
                for yi in start_y..end_y {
                    grid.clear_point(Point::new(x, yi));
                }
            }
        }

        if debug_timing {
            eprintln!(
                "subgraph {} portals top={:?} bottom={:?} left={:?} right={:?}",
                sg_id, portals.top, portals.bottom, portals.left, portals.right
            );
        }
    }
}

pub(super) enum PortalUse {
    Enter,
    Exit,
}

fn median_slot(slots: &HashSet<usize>, fallback: usize) -> usize {
    if slots.is_empty() {
        return fallback;
    }
    let mut vals: Vec<usize> = slots.iter().copied().collect();
    vals.sort_unstable();
    vals[vals.len() / 2]
}

pub(super) fn portal_point(
    bounds: &SubgraphEnvelope,
    how: PortalUse,
    direction: Direction,
) -> Option<Point> {
    match (direction, how) {
        (Direction::TD | Direction::TB, PortalUse::Enter) => {
            let x = median_slot(&bounds.portals.top, bounds.outer.x + bounds.outer.width / 2);
            Some(Point::new(x, bounds.outer.y.saturating_add(1)))
        }
        (Direction::TD | Direction::TB, PortalUse::Exit) => {
            let x = median_slot(
                &bounds.portals.bottom,
                bounds.outer.x + bounds.outer.width / 2,
            );
            Some(Point::new(x, bounds.outer.bottom().saturating_sub(1)))
        }
        (Direction::BT, PortalUse::Enter) => {
            let x = median_slot(
                &bounds.portals.bottom,
                bounds.outer.x + bounds.outer.width / 2,
            );
            Some(Point::new(x, bounds.outer.bottom().saturating_sub(1)))
        }
        (Direction::BT, PortalUse::Exit) => {
            let x = median_slot(&bounds.portals.top, bounds.outer.x + bounds.outer.width / 2);
            Some(Point::new(x, bounds.outer.y))
        }
        (Direction::LR, PortalUse::Enter) => {
            let y = median_slot(
                &bounds.portals.left,
                bounds.outer.y + bounds.outer.height / 2,
            );
            Some(Point::new(bounds.outer.x, y))
        }
        (Direction::LR, PortalUse::Exit) => {
            let y = median_slot(
                &bounds.portals.right,
                bounds.outer.y + bounds.outer.height / 2,
            );
            Some(Point::new(bounds.outer.right().saturating_sub(1), y))
        }
        (Direction::RL, PortalUse::Enter) => {
            let y = median_slot(
                &bounds.portals.right,
                bounds.outer.y + bounds.outer.height / 2,
            );
            Some(Point::new(bounds.outer.right().saturating_sub(1), y))
        }
        (Direction::RL, PortalUse::Exit) => {
            let y = median_slot(
                &bounds.portals.left,
                bounds.outer.y + bounds.outer.height / 2,
            );
            Some(Point::new(bounds.outer.x, y))
        }
    }
}

fn push_route_leg(route: &mut EdgeRoute, from: Point, to: Point) {
    if from != to {
        route.push_segment(from, to);
    }
}

fn build_horizontal_cross_subgraph_fanin_route(
    start: Point,
    portal: Point,
    arrow: Point,
    direction: Direction,
    inner_lane_x: usize,
    outer_lane_x: usize,
) -> EdgeRoute {
    let mut route = EdgeRoute::new();
    let mut cursor = start;

    let source_lane = Point::new(inner_lane_x, cursor.y);
    push_route_leg(&mut route, cursor, source_lane);
    cursor = source_lane;

    let merge_lane = Point::new(inner_lane_x, portal.y);
    push_route_leg(&mut route, cursor, merge_lane);
    cursor = merge_lane;

    push_route_leg(&mut route, cursor, portal);
    cursor = portal;

    if portal.y == arrow.y {
        push_route_leg(&mut route, cursor, arrow);
        return route;
    }

    let outside_lane = Point::new(outer_lane_x, portal.y);
    push_route_leg(&mut route, cursor, outside_lane);

    let outside_turn = Point::new(outer_lane_x, arrow.y);
    push_route_leg(&mut route, outside_lane, outside_turn);

    match direction {
        Direction::LR | Direction::RL => push_route_leg(&mut route, outside_turn, arrow),
        _ => unreachable!(),
    }

    route
}

pub(super) fn route_selective_horizontal_cross_subgraph_fanin_groups(
    graph: &Graph,
    node_rects: &HashMap<String, Rect>,
    subgraph_envelopes: &HashMap<String, SubgraphEnvelope>,
    incoming_counts: &HashMap<&str, usize>,
    routes: &mut HashMap<usize, EdgeRoute>,
    grid: &mut OccupancyGrid,
) {
    if !matches!(graph.direction, Direction::LR | Direction::RL) {
        return;
    }

    let mut grouped_by_target: HashMap<&str, Vec<usize>> = HashMap::new();
    for (edge_idx, edge) in graph.edges.iter().enumerate() {
        if edge.is_back_edge || edge.label.is_some() {
            continue;
        }
        if incoming_counts.get(edge.to.as_str()).copied().unwrap_or(0) < 2 {
            continue;
        }
        grouped_by_target
            .entry(edge.to.as_str())
            .or_default()
            .push(edge_idx);
    }

    for (target_id, edge_indices) in grouped_by_target {
        let Some(target) = graph.get_node(target_id) else {
            continue;
        };
        let Some(first_edge) = edge_indices
            .first()
            .and_then(|edge_idx| graph.edges.get(*edge_idx))
        else {
            continue;
        };
        let Some(source_sg_id) = graph.get_node_subgraph(&first_edge.from) else {
            continue;
        };
        if graph.get_node_subgraph(target_id) == Some(source_sg_id) {
            continue;
        }

        let Some(env) = subgraph_envelopes.get(source_sg_id) else {
            continue;
        };
        let Some(portal) = portal_point(env, PortalUse::Exit, graph.direction) else {
            continue;
        };

        let target_rect = node_rects
            .get(target_id)
            .copied()
            .unwrap_or_else(|| Rect::new(target.x, target.y, target.width, target.height));
        let arrow = edge_entry_point(target_rect, graph.direction);

        let mut starts = Vec::new();
        let mut all_from_same_subgraph = true;
        for edge_idx in &edge_indices {
            let Some(edge) = graph.edges.get(*edge_idx) else {
                all_from_same_subgraph = false;
                break;
            };
            if graph.get_node_subgraph(&edge.from) != Some(source_sg_id)
                || !graph.edge_crosses_subgraph_boundary(&edge.from, &edge.to)
            {
                all_from_same_subgraph = false;
                break;
            }
            let Some(source) = graph.get_node(&edge.from) else {
                all_from_same_subgraph = false;
                break;
            };
            let source_rect = node_rects
                .get(edge.from.as_str())
                .copied()
                .unwrap_or_else(|| Rect::new(source.x, source.y, source.width, source.height));
            starts.push((*edge_idx, edge_exit_point(source_rect, graph.direction)));
        }
        if !all_from_same_subgraph || starts.len() < 2 {
            continue;
        }

        let min_source_y = starts
            .iter()
            .map(|(_, start)| start.y)
            .min()
            .unwrap_or(portal.y);
        let max_source_y = starts
            .iter()
            .map(|(_, start)| start.y)
            .max()
            .unwrap_or(portal.y);
        if portal.y <= min_source_y || portal.y >= max_source_y {
            continue;
        }

        let Some((inner_lane_x, outer_lane_x)) = (match graph.direction {
            Direction::LR => {
                let max_exit_x = starts
                    .iter()
                    .map(|(_, start)| start.x)
                    .max()
                    .unwrap_or(portal.x);
                let desired_inner_lane_x = max_exit_x.saturating_add(1);
                let inner_lane_x = desired_inner_lane_x
                    .min(portal.x.saturating_sub(2))
                    .max(max_exit_x);
                let outer_lane_x = arrow.x.saturating_sub(1);
                (inner_lane_x < portal.x && outer_lane_x > portal.x)
                    .then_some((inner_lane_x, outer_lane_x))
            }
            Direction::RL => {
                let min_exit_x = starts
                    .iter()
                    .map(|(_, start)| start.x)
                    .min()
                    .unwrap_or(portal.x);
                let desired_inner_lane_x = min_exit_x.saturating_sub(1);
                let inner_lane_x = desired_inner_lane_x
                    .max(portal.x.saturating_add(2))
                    .min(min_exit_x);
                let outer_lane_x = arrow.x.saturating_add(1);
                (inner_lane_x > portal.x && outer_lane_x < portal.x)
                    .then_some((inner_lane_x, outer_lane_x))
            }
            _ => None,
        }) else {
            continue;
        };

        for (edge_idx, start) in starts {
            let route = build_horizontal_cross_subgraph_fanin_route(
                start,
                portal,
                arrow,
                graph.direction,
                inner_lane_x,
                outer_lane_x,
            );
            if route.segments.is_empty() {
                continue;
            }
            grid.mark_path(&route);
            routes.insert(edge_idx, route);
        }
    }
}

// -----------------------------------------------------------------------------
// Routing
// -----------------------------------------------------------------------------

const WEIGHT_FREE: u8 = 1;
const WEIGHT_EDGE: u8 = 10;
const WEIGHT_OBSTACLE: u8 = 255;
const COST_BEND: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    fn from_vec(dx: isize, dy: isize) -> Option<Self> {
        match (dx, dy) {
            (0, -1) => Some(Dir::Up),
            (0, 1) => Some(Dir::Down),
            (-1, 0) => Some(Dir::Left),
            (1, 0) => Some(Dir::Right),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct OccupancyGrid {
    pub(super) width: usize,
    pub(super) height: usize,
    weights: Vec<u8>,
}

impl OccupancyGrid {
    pub(super) fn new(width: usize, height: usize) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            width,
            height,
            weights: vec![WEIGHT_FREE; width * height],
        }
    }

    fn in_bounds(&self, p: Point) -> bool {
        p.x < self.width && p.y < self.height
    }

    fn idx(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }

    pub(super) fn mark_rect(&mut self, rect: &Rect) {
        if rect.is_empty() {
            return;
        }
        let x_end = rect.right().min(self.width);
        let y_end = rect.bottom().min(self.height);
        let x_start = rect.x.min(self.width);
        let y_start = rect.y.min(self.height);

        for y in y_start..y_end {
            let row_offset = y * self.width;
            for x in x_start..x_end {
                self.weights[row_offset + x] = WEIGHT_OBSTACLE;
            }
        }
    }

    pub(super) fn clear_point(&mut self, p: Point) {
        if self.in_bounds(p) {
            let idx = self.idx(p.x, p.y);
            self.weights[idx] = WEIGHT_FREE;
        }
    }

    fn cost_at(&self, p: Point) -> u8 {
        if !self.in_bounds(p) {
            return WEIGHT_OBSTACLE;
        }
        self.weights[self.idx(p.x, p.y)]
    }

    pub(super) fn mark_path(&mut self, route: &EdgeRoute) {
        for seg in &route.segments {
            // Determine direction and range
            if seg.from.x == seg.to.x {
                // Vertical
                let (min_y, max_y) = if seg.from.y < seg.to.y {
                    (seg.from.y, seg.to.y)
                } else {
                    (seg.to.y, seg.from.y)
                };
                for y in min_y..=max_y {
                    if y < self.height {
                        let idx = self.idx(seg.from.x, y);
                        // Don't overwrite hard obstacles, but do overwrite free/edge
                        if self.weights[idx] != WEIGHT_OBSTACLE {
                            self.weights[idx] = WEIGHT_EDGE;
                        }
                    }
                }
            } else {
                // Horizontal
                let (min_x, max_x) = if seg.from.x < seg.to.x {
                    (seg.from.x, seg.to.x)
                } else {
                    (seg.to.x, seg.from.x)
                };
                for x in min_x..=max_x {
                    if x < self.width {
                        let idx = self.idx(x, seg.from.y);
                        if self.weights[idx] != WEIGHT_OBSTACLE {
                            self.weights[idx] = WEIGHT_EDGE;
                        }
                    }
                }
            }
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
struct PathNode {
    cost: usize,
    estimate: usize,
    point: Point,
    arrival_dir: Option<Dir>,
}

impl Ord for PathNode {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap behavior using BinaryHeap
        (other.cost + other.estimate).cmp(&(self.cost + self.estimate))
    }
}

impl PartialOrd for PathNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn manhattan(a: Point, b: Point) -> usize {
    a.x.abs_diff(b.x) + a.y.abs_diff(b.y)
}

fn add_manhattan_segment(route: &mut EdgeRoute, from: Point, to: Point, direction: Direction) {
    if from == to {
        return;
    }
    if from.x == to.x || from.y == to.y {
        route.push_segment(from, to);
        return;
    }

    let mid = match direction {
        Direction::TD | Direction::TB | Direction::BT => Point::new(to.x, from.y),
        Direction::LR | Direction::RL => Point::new(from.x, to.y),
    };
    route.push_segment(from, mid);
    route.push_segment(mid, to);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn lane_route(
    start: Point,
    end: Point,
    from_rect: Rect,
    to_rect: Rect,
    direction: Direction,
    out_count: usize,
    in_count: usize,
    pad: usize,
) -> Option<EdgeRoute> {
    if out_count < 2 && in_count < 2 {
        return None;
    }

    let mut route = EdgeRoute::new();
    match direction {
        Direction::TD | Direction::TB => {
            if out_count > 1 {
                let lane_y = from_rect.bottom().saturating_add(pad);
                let mid_a = Point::new(start.x, lane_y);
                let mid_b = Point::new(end.x, lane_y);
                route.push_segment(start, mid_a);
                route.push_segment(mid_a, mid_b);
                route.push_segment(mid_b, end);
                return Some(route);
            }
            if in_count > 1 {
                let lane_y = to_rect.y.saturating_sub(pad);
                let mid_a = Point::new(start.x, lane_y);
                let mid_b = Point::new(end.x, lane_y);
                route.push_segment(start, mid_a);
                route.push_segment(mid_a, mid_b);
                route.push_segment(mid_b, end);
                return Some(route);
            }
        }
        Direction::BT => {
            if out_count > 1 {
                let lane_y = from_rect.y.saturating_sub(pad);
                let mid_a = Point::new(start.x, lane_y);
                let mid_b = Point::new(end.x, lane_y);
                route.push_segment(start, mid_a);
                route.push_segment(mid_a, mid_b);
                route.push_segment(mid_b, end);
                return Some(route);
            }
            if in_count > 1 {
                let lane_y = to_rect.bottom().saturating_add(pad);
                let mid_a = Point::new(start.x, lane_y);
                let mid_b = Point::new(end.x, lane_y);
                route.push_segment(start, mid_a);
                route.push_segment(mid_a, mid_b);
                route.push_segment(mid_b, end);
                return Some(route);
            }
        }
        Direction::LR => {
            if out_count > 1 {
                let lane_x = from_rect.right().saturating_add(pad);
                let mid_a = Point::new(lane_x, start.y);
                let mid_b = Point::new(lane_x, end.y);
                route.push_segment(start, mid_a);
                route.push_segment(mid_a, mid_b);
                route.push_segment(mid_b, end);
                return Some(route);
            }
            if in_count > 1 {
                let lane_x = to_rect.x.saturating_sub(pad);
                let mid_a = Point::new(lane_x, start.y);
                let mid_b = Point::new(lane_x, end.y);
                route.push_segment(start, mid_a);
                route.push_segment(mid_a, mid_b);
                route.push_segment(mid_b, end);
                return Some(route);
            }
        }
        Direction::RL => {
            if out_count > 1 {
                let lane_x = from_rect.x.saturating_sub(pad);
                let mid_a = Point::new(lane_x, start.y);
                let mid_b = Point::new(lane_x, end.y);
                route.push_segment(start, mid_a);
                route.push_segment(mid_a, mid_b);
                route.push_segment(mid_b, end);
                return Some(route);
            }
            if in_count > 1 {
                let lane_x = to_rect.right().saturating_add(pad);
                let mid_a = Point::new(lane_x, start.y);
                let mid_b = Point::new(lane_x, end.y);
                route.push_segment(start, mid_a);
                route.push_segment(mid_a, mid_b);
                route.push_segment(mid_b, end);
                return Some(route);
            }
        }
    }

    None
}

pub(super) fn fallback_manhattan_route(
    start: Point,
    end: Point,
    direction: Direction,
) -> EdgeRoute {
    let mut route = EdgeRoute::new();
    add_manhattan_segment(&mut route, start, end, direction);
    route
}

fn route_with_obstacles(
    start: Point,
    end: Point,
    grid: &mut OccupancyGrid,
    avoid_rects: &[Rect],
    coords: &OrientedCoords,
) -> Option<EdgeRoute> {
    let debug_timing = std::env::var("TERMIFLOW_DEBUG_TIMING").is_ok();
    if start == end {
        let mut route = EdgeRoute::new();
        route.push_segment(start, end);
        return Some(route);
    }

    let mut came_from: HashMap<Point, Point> = HashMap::new();
    let mut best_cost: HashMap<(Point, Option<Dir>), usize> = HashMap::new();
    // Track overall best cost to each point (regardless of direction) for came_from updates
    let mut best_cost_to_point: HashMap<Point, usize> = HashMap::new();
    let mut open = BinaryHeap::new();

    open.push(PathNode {
        cost: 0,
        estimate: manhattan(start, end),
        point: start,
        arrival_dir: None,
    });

    // Initial cost for start point (any direction)
    best_cost.insert((start, None), 0);
    best_cost_to_point.insert(start, 0);

    let mut found_end = false;
    let mut steps: usize = 0;
    let max_steps = grid
        .width
        .saturating_mul(grid.height)
        .saturating_mul(10)
        .max(10_000);

    while let Some(current) = open.pop() {
        steps += 1;
        if steps > max_steps {
            eprintln!(
                "termiflow: warning: routing aborted after {steps} steps ({start:?} -> {end:?})"
            );
            break;
        }
        if debug_timing && steps.is_multiple_of(500) {
            eprintln!(
                "    routing step {} at {:?} (open={})",
                steps,
                current.point,
                open.len()
            );
        }
        if current.point == end {
            found_end = true;
            break;
        }

        let neighbors = ordered_neighbors(current.point, end, coords);
        if debug_timing && steps <= 1 {
            for next in &neighbors {
                let cost = grid.cost_at(*next);
                let blocked = avoid_rects.iter().any(|r| r.contains(*next));
                eprintln!("    neighbor {next:?} cost={cost} blocked_by_rect={blocked}");
            }
        }
        for next in neighbors {
            // Check hard obstacles (rects)
            if avoid_rects.iter().any(|r| r.contains(next)) && next != end {
                continue;
            }

            // Check grid cost
            let cell_cost = grid.cost_at(next);
            if cell_cost == WEIGHT_OBSTACLE && next != end {
                continue;
            }

            // Calculate movement direction
            let dx = next.x as isize - current.point.x as isize;
            let dy = next.y as isize - current.point.y as isize;
            let move_dir = Dir::from_vec(dx, dy);

            // Calculate new cost
            let mut new_cost = current.cost + cell_cost as usize;

            // Add bend penalty
            if let (Some(prev), Some(curr)) = (current.arrival_dir, move_dir) {
                if prev != curr {
                    new_cost += COST_BEND;
                }
            }

            let key = (next, move_dir);
            let known = best_cost.get(&key).copied().unwrap_or(usize::MAX);

            if new_cost < known {
                best_cost.insert(key, new_cost);
                // Only update came_from if this is the best overall path to this point
                let best_to_next = best_cost_to_point.get(&next).copied().unwrap_or(usize::MAX);
                if new_cost < best_to_next {
                    best_cost_to_point.insert(next, new_cost);
                    came_from.insert(next, current.point);
                }
                open.push(PathNode {
                    cost: new_cost,
                    estimate: manhattan(next, end),
                    point: next,
                    arrival_dir: move_dir,
                });
            }
        }
    }

    if !found_end {
        if debug_timing {
            eprintln!("    routing failed after {steps} steps");
        }
        return None;
    }

    if debug_timing {
        eprintln!("    routing succeeded after {steps} steps");
    }

    let mut path: Vec<Point> = Vec::new();
    let mut current = end;
    path.push(current);
    let mut visited: HashSet<Point> = HashSet::new();
    visited.insert(current);
    while let Some(prev) = came_from.get(&current) {
        if !visited.insert(*prev) {
            break;
        }
        current = *prev;
        path.push(current);
        if current == start {
            break;
        }
    }
    path.reverse();

    let route = compress_path(&path);

    // Mark the successful route on the grid to repel future edges
    grid.mark_path(&route);

    Some(route)
}

pub(super) fn route_with_obstacles_v2(
    start: Point,
    end: Point,
    grid: &mut OccupancyGrid,
    avoid_rects: &[Rect],
    coords: &OrientedCoords,
) -> Option<EdgeRoute> {
    if let Some(route) = route_with_obstacles(start, end, grid, avoid_rects, coords) {
        return Some(route);
    }
    route_with_detours(start, end, grid, avoid_rects, coords)
}

fn route_with_detours(
    start: Point,
    end: Point,
    grid: &mut OccupancyGrid,
    avoid_rects: &[Rect],
    coords: &OrientedCoords,
) -> Option<EdgeRoute> {
    if start == end {
        return Some(EdgeRoute::new());
    }

    let in_avoid = |p: Point| -> bool { avoid_rects.iter().any(|r| r.contains(p)) };
    let in_bounds = |p: Point| -> bool { p.x < grid.width && p.y < grid.height };

    let (start_primary, end_primary) = match coords.primary {
        Axis::Horizontal => (start.x, end.x),
        Axis::Vertical => (start.y, end.y),
    };
    let (p_min, p_max) = if start_primary <= end_primary {
        (start_primary, end_primary)
    } else {
        (end_primary, start_primary)
    };

    // Try a small set of primary-axis "dogleg" rows/cols near the midpoint and endpoints.
    let mid = p_min + (p_max.saturating_sub(p_min) / 2);
    let mut candidates: Vec<usize> = vec![
        mid,
        mid.saturating_add(1),
        mid.saturating_sub(1),
        mid.saturating_add(2),
        mid.saturating_sub(2),
        p_min.saturating_add(1),
        p_max.saturating_sub(1),
    ];
    candidates.sort_unstable();
    candidates.dedup();

    for primary in candidates {
        let (p1, p2) = match coords.primary {
            Axis::Vertical => (Point::new(start.x, primary), Point::new(end.x, primary)),
            Axis::Horizontal => (Point::new(primary, start.y), Point::new(primary, end.y)),
        };
        if !in_bounds(p1) || !in_bounds(p2) {
            continue;
        }
        if (p1 != start && p1 != end && in_avoid(p1)) || (p2 != start && p2 != end && in_avoid(p2))
        {
            continue;
        }

        // Use a cloned grid so failed attempts don't "burn in" partial routes.
        let mut trial = grid.clone();
        trial.clear_point(p1);
        trial.clear_point(p2);

        let mut combined = EdgeRoute::new();
        let legs = [(start, p1), (p1, p2), (p2, end)];
        let mut ok = true;
        for (a, b) in legs {
            if a == b {
                continue;
            }
            let Some(route) = route_with_obstacles(a, b, &mut trial, avoid_rects, coords) else {
                ok = false;
                break;
            };
            for s in route.segments {
                combined.push_segment(s.from, s.to);
            }
        }

        if ok && !combined.segments.is_empty() {
            return Some(combined);
        }
    }

    None
}

fn ordered_neighbors(current: Point, goal: Point, coords: &OrientedCoords) -> Vec<Point> {
    let dx = goal.x as isize - current.x as isize;
    let dy = goal.y as isize - current.y as isize;

    let primary_first = if coords.primary == Axis::Horizontal {
        vec![
            (dx.signum(), 0),
            (0, dy.signum()),
            (-dx.signum(), 0),
            (0, -dy.signum()),
        ]
    } else {
        vec![
            (0, dy.signum()),
            (dx.signum(), 0),
            (0, -dy.signum()),
            (-dx.signum(), 0),
        ]
    };

    let mut neighbors = Vec::new();
    for (sx, sy) in primary_first {
        if sx == 0 && sy == 0 {
            continue;
        }
        let nx = if sx.is_negative() {
            current.x.saturating_sub(sx.unsigned_abs())
        } else {
            current.x.saturating_add(sx as usize)
        };
        let ny = if sy.is_negative() {
            current.y.saturating_sub(sy.unsigned_abs())
        } else {
            current.y.saturating_add(sy as usize)
        };
        let next = Point::new(nx, ny);
        if next != current {
            neighbors.push(next);
        }
    }
    neighbors
}

fn compress_path(points: &[Point]) -> EdgeRoute {
    let mut route = EdgeRoute::new();
    if points.is_empty() {
        return route;
    }
    if points.len() == 1 {
        route.push_segment(points[0], points[0]);
        return route;
    }

    let mut seg_start = points[0];
    let mut last_dir = (0isize, 0isize);
    for window in points.windows(2) {
        let a = window[0];
        let b = window[1];
        let dir = (b.x as isize - a.x as isize, b.y as isize - a.y as isize);
        let norm = (dir.0.signum(), dir.1.signum());
        if last_dir != norm && last_dir != (0, 0) {
            route.push_segment(seg_start, a);
            seg_start = a;
        }
        last_dir = norm;
    }
    route.push_segment(seg_start, *points.last().unwrap());
    route
}

pub(super) fn edge_exit_point(rect: Rect, direction: Direction) -> Point {
    match direction {
        Direction::TD | Direction::TB => Point::new(rect.x + rect.width / 2, rect.y + rect.height),
        Direction::BT => Point::new(rect.x + rect.width / 2, rect.y.saturating_sub(1)),
        Direction::LR => Point::new(rect.x + rect.width, rect.y + rect.height / 2),
        Direction::RL => Point::new(rect.x.saturating_sub(1), rect.y + rect.height / 2),
    }
}

pub(super) fn edge_entry_point(rect: Rect, direction: Direction) -> Point {
    match direction {
        Direction::TD | Direction::TB => {
            Point::new(rect.x + rect.width / 2, rect.y.saturating_sub(1))
        }
        Direction::BT => Point::new(rect.x + rect.width / 2, rect.y + rect.height),
        Direction::LR => Point::new(rect.x.saturating_sub(1), rect.y + rect.height / 2),
        Direction::RL => Point::new(rect.x + rect.width, rect.y + rect.height / 2),
    }
}
