//! Transactional routing for a source branching through an intermediate
//! database before converging on a terminal database.
//!
//! The generic convergence/fan-out passes can draw the direct source-to-final
//! edge through the intermediate target's entry row.  That makes the
//! intermediate arrow look like a mid-route junction even when the semantic
//! edge count is correct.  This module claims only the small three-node,
//! three-edge topology for which an outer bypass lane can be proven.

use crate::graph::{Direction, EdgeKind, Graph, Node, NodeShape};
use crate::orientation::OrientedCoords;
use crate::style::StyleChars;

use super::super::canvas::Canvas;
use super::super::fan_in_identity::{target_port_columns, target_port_count, target_port_rows};
use super::edge_primitives::{edge_entry_point, edge_exit_point};
use super::{edge_route_owner_id, set_route_char, set_route_edge_char, RouteOwner};

struct DatabaseScene<'a> {
    source: &'a Node,
    intermediate: &'a Node,
    target: &'a Node,
}

/// Route the strict intermediate-database scene, returning `true` only after
/// all three edge paths pass the same reservation checks and are committed.
pub(crate) fn route_database_intermediate_scene(
    canvas: &mut Canvas,
    style: &StyleChars,
    direction: Direction,
    graph: &Graph,
) -> bool {
    let Some(scene) = database_scene(graph) else {
        return false;
    };
    let Some(port_count) = target_port_count(graph, &scene.target.id) else {
        return false;
    };
    if port_count != 2 {
        return false;
    }

    let coords = OrientedCoords::new(direction);
    let target_ports = match direction {
        Direction::TD | Direction::TB | Direction::BT => {
            target_port_columns(scene.target.x, scene.target.width, port_count)
        }
        Direction::LR | Direction::RL => {
            target_port_rows(scene.target.y, scene.target.height, port_count)
        }
    };
    if target_ports.len() != 2 {
        return false;
    }

    let mut planning_canvas = canvas.clone();
    ensure_outer_lane_capacity(&mut planning_canvas, graph, &coords);

    let source_exit = edge_exit_point(scene.source, direction);
    let intermediate_entry = edge_entry_point(scene.intermediate, direction);
    let intermediate_exit = edge_exit_point(scene.intermediate, direction);
    let target_base = edge_entry_point(scene.target, direction);
    let intermediate_secondary = coords.secondary_coord(intermediate_exit.0, intermediate_exit.1);
    let Some(side) = outer_secondary_lane(&planning_canvas, graph, &coords) else {
        return false;
    };
    // The bypass approaches the final target from the selected outer side.
    // Give the direct intermediate→target edge the opposite-side port so its
    // secondary jog can become a dedicated primary lane rather than sharing
    // the bypass's final target row.
    let direct_port_index = if side > intermediate_secondary { 0 } else { 1 };
    let bypass_port_index = 1usize.saturating_sub(direct_port_index);
    let direct_target = coords.with_secondary(
        target_base.0,
        target_base.1,
        target_ports[direct_port_index],
    );
    let bypass_target = coords.with_secondary(
        target_base.0,
        target_base.1,
        target_ports[bypass_port_index],
    );
    let direct_to_intermediate = primary_first_path(source_exit, intermediate_entry, &coords);
    let intermediate_to_target = secondary_first_path(intermediate_exit, direct_target, &coords);
    let source_to_target_bypass = outer_bypass_path(
        source_exit,
        bypass_target,
        side,
        &coords,
        scene.source,
        scene.intermediate,
    );
    let paths = [
        direct_to_intermediate,
        intermediate_to_target,
        source_to_target_bypass,
    ];

    if !paths_are_reserved(&paths, source_exit, &planning_canvas, graph) {
        return false;
    }

    canvas.set_write_stage("edge-route-database-intermediate");
    let mut trial = planning_canvas;
    trial.set_write_stage("edge-route-database-intermediate");
    paint_path(
        &mut trial,
        style,
        graph,
        &scene.source.id,
        &scene.intermediate.id,
        &paths[0],
        direction,
    );
    paint_path(
        &mut trial,
        style,
        graph,
        &scene.intermediate.id,
        &scene.target.id,
        &paths[1],
        direction,
    );
    paint_path(
        &mut trial,
        style,
        graph,
        &scene.source.id,
        &scene.target.id,
        &paths[2],
        direction,
    );

    // The two paths deliberately leave the source through one shared exit
    // cell. Restore the three-arm tee after overlap resolution so the source
    // split is explicit rather than a generic four-way cross.
    let source_owner_id = edge_route_owner_id(graph, &scene.source.id, &scene.intermediate.id);
    let source_owner = RouteOwner {
        kind: crate::render::semantic::CellOwnerKind::EdgeSegment,
        id: source_owner_id.as_str(),
    };
    let branch_index = shared_source_branch_index(&paths, source_exit);
    let bypass_secondary = paths[2]
        .get(branch_index.saturating_add(1))
        .map(|point| coords.secondary_coord(point.0, point.1))
        .unwrap_or_else(|| coords.secondary_coord(source_exit.0, source_exit.1));
    let branch_point = paths[0][branch_index];
    set_route_char(
        &mut trial,
        branch_point.0,
        branch_point.1,
        source_branch_junction(&coords, branch_point, bypass_secondary, style),
        Some(source_owner),
    );

    *canvas = trial;
    true
}

/// Restore a single source exit marker after the node shape pass.  The
/// generic rectangle drawer infers a second border junction when the outer
/// bypass turns immediately below/above the source; that is a route corner,
/// not another source port.
pub(crate) fn repair_database_source_border(
    canvas: &mut Canvas,
    style: &StyleChars,
    direction: Direction,
    graph: &Graph,
) {
    let Some(scene) = database_scene(graph) else {
        return;
    };
    if !matches!(direction, Direction::TD | Direction::TB | Direction::BT) {
        return;
    }

    let source = scene.source;
    let border_y = if direction == Direction::BT {
        source.y
    } else {
        source.bottom_y().saturating_sub(1)
    };
    if border_y >= canvas.height {
        return;
    }

    canvas.set(
        source.x,
        border_y,
        if direction == Direction::BT {
            style.tl
        } else {
            style.bl
        },
    );
    for x in source.x.saturating_add(1)..source.x.saturating_add(source.width.saturating_sub(1)) {
        let ch = if x == source.center_x() {
            if direction == Direction::BT {
                style.junction_up
            } else {
                style.junction_down
            }
        } else {
            style.h
        };
        canvas.set(x, border_y, ch);
    }
    let right_x = source.x.saturating_add(source.width.saturating_sub(1));
    if right_x < canvas.width {
        canvas.set(
            right_x,
            border_y,
            if direction == Direction::BT {
                style.tr
            } else {
                style.br
            },
        );
    }
}

fn database_scene(graph: &Graph) -> Option<DatabaseScene<'_>> {
    if graph.nodes.len() != 3
        || graph.edges.len() != 3
        || !graph.subgraphs.is_empty()
        || graph.has_cycles()
        || graph.edges.iter().any(|edge| {
            edge.is_back_edge
                || edge.kind != EdgeKind::Arrow
                || edge.label.is_some()
                || edge.from == edge.to
        })
    {
        return None;
    }

    let source = graph.nodes.iter().find(|node| {
        node.shape == NodeShape::Rectangle
            && graph
                .edges
                .iter()
                .filter(|edge| edge.from == node.id)
                .count()
                == 2
            && graph.edges.iter().all(|edge| edge.to != node.id)
    })?;
    let intermediate = graph.nodes.iter().find(|node| {
        node.shape == NodeShape::Database
            && graph
                .edges
                .iter()
                .filter(|edge| edge.from == node.id)
                .count()
                == 1
            && graph.edges.iter().filter(|edge| edge.to == node.id).count() == 1
    })?;
    let target = graph.nodes.iter().find(|node| {
        node.shape == NodeShape::Database
            && node.id != intermediate.id
            && graph
                .edges
                .iter()
                .filter(|edge| edge.from == node.id)
                .count()
                == 0
            && graph.edges.iter().filter(|edge| edge.to == node.id).count() == 2
    })?;

    let has_edge = |from: &str, to: &str| {
        graph
            .edges
            .iter()
            .any(|edge| edge.from == from && edge.to == to)
    };
    (has_edge(&source.id, &intermediate.id)
        && has_edge(&source.id, &target.id)
        && has_edge(&intermediate.id, &target.id))
    .then_some(DatabaseScene {
        source,
        intermediate,
        target,
    })
}

fn outer_secondary_lane(canvas: &Canvas, graph: &Graph, coords: &OrientedCoords) -> Option<usize> {
    let (min_secondary, max_secondary) = graph.nodes.iter().fold(
        (usize::MAX, 0usize),
        |(min_secondary, max_secondary), node| {
            let start = coords.secondary_coord(node.x, node.y);
            let end = match coords.secondary {
                crate::orientation::Axis::Horizontal => node.x.saturating_add(node.width),
                crate::orientation::Axis::Vertical => node.y.saturating_add(node.height),
            };
            (min_secondary.min(start), max_secondary.max(end))
        },
    );
    let limit = match coords.secondary {
        crate::orientation::Axis::Horizontal => canvas.width,
        crate::orientation::Axis::Vertical => canvas.height,
    };
    let high = max_secondary.saturating_add(1);
    if high < limit {
        return Some(high);
    }

    let low = min_secondary.saturating_sub(2);
    (low < limit).then_some(low)
}

fn ensure_outer_lane_capacity(canvas: &mut Canvas, graph: &Graph, coords: &OrientedCoords) {
    let max_secondary = graph.nodes.iter().fold(0usize, |max_secondary, node| {
        let end = match coords.secondary {
            crate::orientation::Axis::Horizontal => node.x.saturating_add(node.width),
            crate::orientation::Axis::Vertical => node.y.saturating_add(node.height),
        };
        max_secondary.max(end)
    });
    let required_limit = max_secondary.saturating_add(2);
    match coords.secondary {
        crate::orientation::Axis::Horizontal => canvas.ensure_width(required_limit),
        crate::orientation::Axis::Vertical => canvas.ensure_height(required_limit),
    }
}

fn primary_first_path(
    start: (usize, usize),
    end: (usize, usize),
    coords: &OrientedCoords,
) -> Vec<(usize, usize)> {
    let mut path = Vec::new();
    let mut primary_turn = end;
    coords.set_secondary(
        &mut primary_turn.0,
        &mut primary_turn.1,
        coords.secondary_coord(start.0, start.1),
    );
    append_axis_line(&mut path, start, primary_turn, coords.primary);
    append_axis_line(&mut path, primary_turn, end, coords.secondary);
    path
}

fn secondary_first_path(
    start: (usize, usize),
    end: (usize, usize),
    coords: &OrientedCoords,
) -> Vec<(usize, usize)> {
    let mut path = Vec::new();
    let mut secondary_turn = start;
    coords.set_secondary(
        &mut secondary_turn.0,
        &mut secondary_turn.1,
        coords.secondary_coord(end.0, end.1),
    );
    append_axis_line(&mut path, start, secondary_turn, coords.secondary);
    append_axis_line(&mut path, secondary_turn, end, coords.primary);
    path
}

fn outer_bypass_path(
    start: (usize, usize),
    end: (usize, usize),
    side: usize,
    coords: &OrientedCoords,
    source: &Node,
    _intermediate: &Node,
) -> Vec<(usize, usize)> {
    let mut path = Vec::new();
    let (first_turn, first_primary, first_axis, second_axis) = if matches!(
        coords.direction,
        Direction::TD | Direction::TB | Direction::BT
    ) {
        let source_secondary = coords.secondary_coord(start.0, start.1);
        let pre_secondary = if side > source_secondary {
            source.x.saturating_add(source.width.saturating_sub(3))
        } else {
            source.x.saturating_add(2)
        };
        // Keep one quiet primary-axis cell after the source exit before the
        // bypass leaves the source stem. Starting the bypass on the border
        // makes the source tee read like a stray `+---+` hook; the direct
        // source→intermediate edge already owns this short prefix, so the
        // later branch remains collision-safe and the intermediate arrow
        // still has its own entry cell.
        let first_turn = coords.advance(start.0, start.1, 1);
        let first_primary = coords.with_secondary(first_turn.0, first_turn.1, pre_secondary);
        (first_turn, first_primary, coords.primary, coords.secondary)
    } else {
        // Leave two quiet cells in the source stem before leaving the source
        // axis. The direct source→intermediate edge shares this prefix, so a
        // two-cell stem makes the source-owned tee read as an intentional
        // branch rather than a one-cell `+-+` hook glued to the node corner.
        let first_turn = coords.advance(start.0, start.1, 2);
        let first_primary = coords.with_secondary(first_turn.0, first_turn.1, side);
        (
            first_turn,
            first_primary,
            crate::orientation::Axis::Horizontal,
            crate::orientation::Axis::Vertical,
        )
    };
    let start_side = coords.with_secondary(first_primary.0, first_primary.1, side);
    let target_before = coords.retreat(end.0, end.1, 1);
    let end_side = coords.with_secondary(target_before.0, target_before.1, side);
    let approach_side = coords.with_secondary(
        target_before.0,
        target_before.1,
        coords.secondary_coord(end.0, end.1),
    );
    append_axis_line(&mut path, start, first_turn, first_axis);
    append_axis_line(&mut path, first_turn, first_primary, second_axis);
    append_axis_line(&mut path, first_primary, start_side, coords.secondary);
    append_axis_line(&mut path, start_side, end_side, coords.primary);
    append_axis_line(&mut path, end_side, approach_side, coords.secondary);
    append_axis_line(&mut path, approach_side, end, coords.primary);
    path
}

fn append_axis_line(
    path: &mut Vec<(usize, usize)>,
    start: (usize, usize),
    end: (usize, usize),
    axis: crate::orientation::Axis,
) {
    if path.last().copied() != Some(start) {
        path.push(start);
    }
    match axis {
        crate::orientation::Axis::Horizontal if start.0 <= end.0 => {
            for x in start.0..=end.0 {
                push_unique(path, (x, start.1));
            }
        }
        crate::orientation::Axis::Horizontal => {
            for x in (end.0..=start.0).rev() {
                push_unique(path, (x, start.1));
            }
        }
        crate::orientation::Axis::Vertical if start.1 <= end.1 => {
            for y in start.1..=end.1 {
                push_unique(path, (start.0, y));
            }
        }
        crate::orientation::Axis::Vertical => {
            for y in (end.1..=start.1).rev() {
                push_unique(path, (start.0, y));
            }
        }
    }
}

fn push_unique(path: &mut Vec<(usize, usize)>, point: (usize, usize)) {
    if path.last().copied() != Some(point) {
        path.push(point);
    }
}

fn paths_are_reserved(
    paths: &[Vec<(usize, usize)>; 3],
    shared_source_exit: (usize, usize),
    canvas: &Canvas,
    graph: &Graph,
) -> bool {
    let shared_source_points = shared_source_points(paths, shared_source_exit);
    let mut reserved = std::collections::HashSet::new();
    for (path_index, path) in paths.iter().enumerate() {
        for point in path {
            // The direct and bypass edges may share a short source stem. The
            // bypass is the only path allowed to reuse those cells; any
            // collision with the intermediate→target path still rejects the
            // transaction.
            let is_shared_source_prefix = path_index == 2 && shared_source_points.contains(point);
            if !is_shared_source_prefix && !reserved.insert(*point) {
                return false;
            }
            let (x, y) = *point;
            let inside_node = graph.nodes.iter().any(|node| {
                x >= node.x
                    && x < node.x.saturating_add(node.width)
                    && y >= node.y
                    && y < node.bottom_y()
            });
            if (x >= canvas.width || y >= canvas.height)
                || (canvas.get(x, y) != ' ' && *point != shared_source_exit)
                || inside_node
            {
                return false;
            }
        }
    }
    true
}

fn shared_source_points(
    paths: &[Vec<(usize, usize)>; 3],
    source_exit: (usize, usize),
) -> std::collections::HashSet<(usize, usize)> {
    let mut shared = std::collections::HashSet::new();
    if paths[0].first().copied() != Some(source_exit)
        || paths[2].first().copied() != Some(source_exit)
    {
        return shared;
    }

    let mut index = 0usize;
    while paths[0].get(index) == paths[2].get(index) {
        if let Some(point) = paths[0].get(index).copied() {
            shared.insert(point);
        }
        index = index.saturating_add(1);
        if index >= paths[0].len() || index >= paths[2].len() {
            break;
        }
    }
    shared
}

fn shared_source_branch_index(
    paths: &[Vec<(usize, usize)>; 3],
    source_exit: (usize, usize),
) -> usize {
    if paths[0].first().copied() != Some(source_exit)
        || paths[2].first().copied() != Some(source_exit)
    {
        return 0;
    }

    let mut index = 0usize;
    while index.saturating_add(1) < paths[0].len()
        && index.saturating_add(1) < paths[2].len()
        && paths[0][index.saturating_add(1)] == paths[2][index.saturating_add(1)]
    {
        index = index.saturating_add(1);
    }
    index
}

fn paint_path(
    canvas: &mut Canvas,
    style: &StyleChars,
    graph: &Graph,
    from_id: &str,
    to_id: &str,
    path: &[(usize, usize)],
    direction: Direction,
) {
    let owner_id = edge_route_owner_id(graph, from_id, to_id);
    let owner = RouteOwner {
        kind: crate::render::semantic::CellOwnerKind::EdgeSegment,
        id: owner_id.as_str(),
    };
    for pair in path.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let glyph = if start.0 == end.0 {
            style.edge_v
        } else {
            style.edge_h
        };
        set_route_edge_char(canvas, start.0, start.1, glyph, style, Some(owner));
        set_route_edge_char(canvas, end.0, end.1, glyph, style, Some(owner));
    }
    for window in path.windows(3) {
        let previous = window[0];
        let current = window[1];
        let next = window[2];
        let previous_axis = if previous.0 == current.0 {
            crate::orientation::Axis::Vertical
        } else {
            crate::orientation::Axis::Horizontal
        };
        let next_axis = if current.0 == next.0 {
            crate::orientation::Axis::Vertical
        } else {
            crate::orientation::Axis::Horizontal
        };
        if previous_axis != next_axis {
            set_route_char(
                canvas,
                current.0,
                current.1,
                corner_for_turn(previous, current, next, style),
                Some(owner),
            );
        }
    }

    if let Some(&(arrow_x, arrow_y)) = path.last() {
        let coords = OrientedCoords::new(direction);
        set_route_char(
            canvas,
            arrow_x,
            arrow_y,
            coords.arrow_end(style),
            Some(owner),
        );
    }
}

fn corner_for_turn(
    previous: (usize, usize),
    current: (usize, usize),
    next: (usize, usize),
    style: &StyleChars,
) -> char {
    let up = previous.1 < current.1 || next.1 < current.1;
    let down = previous.1 > current.1 || next.1 > current.1;
    let left = previous.0 < current.0 || next.0 < current.0;
    let right = previous.0 > current.0 || next.0 > current.0;
    match (up, down, left, right) {
        (false, true, false, true) => style.corner_dl,
        (false, true, true, false) => style.corner_dr,
        (true, false, false, true) => style.corner_ul,
        (true, false, true, false) => style.corner_ur,
        _ => style.cross,
    }
}

fn source_branch_junction(
    coords: &OrientedCoords,
    source_exit: (usize, usize),
    bypass_secondary: usize,
    style: &StyleChars,
) -> char {
    let source_secondary = coords.secondary_coord(source_exit.0, source_exit.1);
    let bypass_before = bypass_secondary < source_secondary;
    match coords.direction {
        Direction::TD | Direction::TB | Direction::BT => {
            if bypass_before {
                style.junction_left
            } else {
                style.junction_right
            }
        }
        Direction::LR | Direction::RL => {
            if bypass_before {
                style.junction_up
            } else {
                style.junction_down
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        edge_entry_point, edge_exit_point, outer_bypass_path, route_database_intermediate_scene,
    };
    use crate::graph::{Direction, Edge, Graph, Node, NodeShape};
    use crate::orientation::OrientedCoords;
    use crate::render::canvas::Canvas;
    use crate::style::ASCII_CHARS;

    fn scene(direction: Direction) -> Graph {
        let mut graph = Graph::new();
        graph.direction = direction;
        let mut source = Node::new("A", "A");
        let mut intermediate = Node::with_shape("B", "B", NodeShape::Database);
        let mut target = Node::with_shape("C", "C", NodeShape::Database);
        source.width = 5;
        target.height = 5;
        match direction {
            Direction::TD | Direction::TB => {
                source.x = 0;
                source.y = 0;
                intermediate.x = 2;
                intermediate.y = 6;
                target.x = 0;
                target.y = 15;
            }
            Direction::BT => {
                target.x = 0;
                target.y = 0;
                intermediate.x = 2;
                intermediate.y = 9;
                source.x = 0;
                source.y = 18;
            }
            Direction::LR => {
                source.x = 0;
                source.y = 0;
                intermediate.x = 24;
                intermediate.y = 0;
                target.x = 43;
                target.y = 0;
            }
            Direction::RL => {
                target.x = 0;
                target.y = 0;
                intermediate.x = 24;
                intermediate.y = 0;
                source.x = 45;
                source.y = 0;
            }
        }
        graph.add_node(source);
        graph.add_node(intermediate);
        graph.add_node(target);
        graph.add_edge(Edge::new("A", "C"));
        graph.add_edge(Edge::new("A", "B"));
        graph.add_edge(Edge::new("B", "C"));
        graph
    }

    #[test]
    fn claims_only_the_three_edge_intermediate_database_scene() {
        for direction in [
            Direction::TD,
            Direction::TB,
            Direction::BT,
            Direction::LR,
            Direction::RL,
        ] {
            let graph = scene(direction);
            let mut canvas = Canvas::new(90, 40);
            assert!(
                route_database_intermediate_scene(&mut canvas, &ASCII_CHARS, direction, &graph,),
                "scene should be claimed for {direction:?}"
            );
            assert_eq!(
                canvas
                    .to_string()
                    .chars()
                    .filter(|ch| matches!(ch, 'v' | '^' | '<' | '>'))
                    .count(),
                3,
                "three edges need three arrowheads at {direction:?}"
            );
        }
    }

    #[test]
    fn horizontal_database_bypass_reserves_a_two_cell_source_tee_stem() {
        for direction in [Direction::LR, Direction::RL] {
            let graph = scene(direction);
            let source = graph.get_node("A").expect("source node");
            let intermediate = graph.get_node("B").expect("intermediate node");
            let target = graph.get_node("C").expect("target node");
            let coords = OrientedCoords::new(direction);
            let start = edge_exit_point(source, direction);
            let end = edge_entry_point(target, direction);
            let source_secondary = coords.secondary_coord(start.0, start.1);
            let side = source_secondary.saturating_add(5);
            let path = outer_bypass_path(start, end, side, &coords, source, intermediate);

            let quiet_stem = path
                .iter()
                .take_while(|point| coords.secondary_coord(point.0, point.1) == source_secondary)
                .count();
            assert!(
                quiet_stem >= 3,
                "{direction:?} database bypass needs source plus two quiet stem cells, got {quiet_stem}"
            );
        }
    }
}
