//! Cycle edge routing and node geometry helpers.
//!
//! This module provides:
//! - `route_cycle_edge`: Routes cycle edges through the gutter (right for TD/BT, bottom for LR/RL)
//! - `center_x`, `center_y`: Calculate visual center coordinates of nodes

use std::collections::HashMap;

use crate::geom::Rect;
use crate::graph::{Direction, EdgeKind, Graph, Node};
use crate::spacing::SpacingConfig;
use crate::style::StyleChars;

use super::canvas::Canvas;
use super::semantic::{CellOwnerKind, CellRole};

const CYCLE_Z_INDEX: u8 = 5;

// ============================================================================
// Cycle Edge Routing
// ============================================================================

/// Route a cycle edge through the appropriate gutter.
///
/// For TD/BT: Routes through right gutter (horizontal → vertical → horizontal)
/// For LR/RL: Routes through bottom gutter (vertical → horizontal → vertical)
pub fn route_cycle_edge(
    from: &Node,
    to: &Node,
    canvas: &mut Canvas,
    style: &StyleChars,
    spacing: &SpacingConfig,
    direction: crate::graph::Direction,
    owner_id: Option<&str>,
) {
    route_cycle_edge_on_lane(
        from,
        to,
        canvas,
        style,
        spacing,
        direction,
        owner_id,
        EdgeKind::Arrow,
        None,
        None,
        None,
        None,
        0,
        None,
    );
}

/// Route a cycle edge, using a planned LR/RL gutter lane when one is given,
/// planned TD source and target detour rows, and in TD/BT the gutter column
/// `gutter_inset` column pairs in from the outermost one.
#[allow(clippy::too_many_arguments)]
pub fn route_cycle_edge_on_lane(
    from: &Node,
    to: &Node,
    canvas: &mut Canvas,
    style: &StyleChars,
    spacing: &SpacingConfig,
    direction: crate::graph::Direction,
    owner_id: Option<&str>,
    kind: EdgeKind,
    lane: Option<HorizontalCycleLane>,
    source_detour: Option<usize>,
    target_detour: Option<&[usize]>,
    bottom_exit: Option<CycleBottomExit>,
    gutter_inset: usize,
    local_boxes: Option<&[Rect]>,
) {
    use crate::graph::Direction;

    if !canvas.is_visible(from) || !canvas.is_visible(to) {
        return;
    }

    match direction {
        Direction::TD | Direction::TB | Direction::BT => {
            // Vertical layout: use right gutter
            let cycle_gutter = spacing.cycle_gutter;
            if canvas.width <= cycle_gutter + 2 * gutter_inset {
                return;
            }

            let gutter_x = canvas.width - 2 - 2 * gutter_inset;
            let from_y = from.center_y();
            let to_y = to.center_y();

            // Self-loops need their own three-sided route. The generic
            // same-row path would collapse to a clipped one-sided tail because
            // `from_y == to_y`; reserve a nearby return leg so the human eye
            // can see a closed cycle in TD/BT as well as LR/RL.
            if from.id == to.id {
                let node_right = from.x + from.width;
                let return_x = node_right.saturating_add(2);
                let loop_offset = spacing.row_spacing.max(2);
                let loop_y = match direction {
                    Direction::BT => from.y.saturating_sub(loop_offset),
                    Direction::TD | Direction::TB => from.bottom_y().saturating_add(loop_offset),
                    _ => unreachable!(),
                };

                if return_x >= gutter_x || loop_y >= canvas.height {
                    return;
                }

                let (gutter_source_corner, gutter_loop_corner, return_loop_corner, return_junction) =
                    if matches!(direction, Direction::BT) {
                        (
                            style.corner_ur,
                            style.corner_dr,
                            style.corner_dl,
                            style.junction_up,
                        )
                    } else {
                        (
                            style.corner_dr,
                            style.corner_ur,
                            style.corner_ul,
                            style.junction_down,
                        )
                    };

                // Place corners before lines so overlap resolution cannot turn
                // them into generic crosses.
                set_cycle_char(canvas, gutter_x, from_y, gutter_source_corner, owner_id);
                set_cycle_char(canvas, gutter_x, loop_y, gutter_loop_corner, owner_id);
                set_cycle_char(canvas, return_x, loop_y, return_loop_corner, owner_id);

                let (min_y, max_y) = if loop_y < from_y {
                    (loop_y, from_y)
                } else {
                    (from_y, loop_y)
                };
                for y in min_y.saturating_add(1)..max_y {
                    set_cycle_edge_char(canvas, gutter_x, y, style.back_v, style, owner_id);
                    set_cycle_edge_char(canvas, return_x, y, style.back_v, style, owner_id);
                }

                for x in node_right.saturating_add(1)..gutter_x {
                    set_cycle_edge_char(canvas, x, from_y, style.back_h, style, owner_id);
                }
                for x in return_x.saturating_add(1)..gutter_x {
                    set_cycle_edge_char(canvas, x, loop_y, style.back_h, style, owner_id);
                }

                set_cycle_char(canvas, return_x, from_y, return_junction, owner_id);
                let end = cycle_end_char(kind, style.arrow_left, style.back_h, style);
                set_cycle_end(canvas, node_right, from_y, end, kind, owner_id);
                return;
            }

            // A box on the run along either end's row that no planned detour
            // avoids would be crossed: the loop closes in the nearest clear
            // column right of both ends instead, on their own rows (gen_d_094).
            let local_gutter = local_boxes.and_then(|boxes| {
                local_cycle_gutter(
                    from,
                    to,
                    boxes,
                    canvas,
                    style,
                    gutter_x,
                    source_detour.is_some() || bottom_exit.is_some(),
                    target_detour.is_some(),
                )
            });
            let (gutter_x, source_detour, target_detour, bottom_exit) = match local_gutter {
                Some(x) => (x, None, None, None),
                None => (gutter_x, source_detour, target_detour, bottom_exit),
            };

            // Horizontal line from source to gutter.  A box on the source's row
            // sends it along its planned detour row instead: a one-cell stub,
            // a jog past the rank, then the run to the gutter.  It is taken only
            // when the jog column is blank and the detour row meets nothing but
            // vertical lines it crosses, so it never runs along another route.
            let detour_clear = |detour_y: usize| {
                let jog_x = from.x + from.width + 1;
                detour_y < canvas.height
                    && (from_y.min(detour_y)..=from_y.max(detour_y))
                        .all(|y| canvas.get(jog_x, y) == ' ')
                    && (jog_x + 1..=gutter_x).all(|x| {
                        let c = canvas.get(x, detour_y);
                        c == ' ' || super::canvas::is_vertical(c, style)
                    })
            };
            // A source that also takes a back edge in leaves through its bottom
            // border instead, when that drop and its row are clear.
            let bottom_exit = bottom_exit.filter(|exit| {
                exit.row < canvas.height
                    && exit.x < gutter_x
                    && (from.bottom_y()..=exit.row).all(|y| canvas.get(exit.x, y) == ' ')
                    && (exit.x + 1..=gutter_x).all(|x| {
                        let c = canvas.get(x, exit.row);
                        c == ' ' || super::canvas::is_vertical(c, style)
                    })
            });
            let source_y = match (bottom_exit, source_detour.filter(|y| detour_clear(*y))) {
                (Some(exit), _) => {
                    for y in from.bottom_y()..exit.row {
                        set_cycle_edge_char(canvas, exit.x, y, style.back_v, style, owner_id);
                    }
                    set_cycle_char(canvas, exit.x, exit.row, style.corner_ul, owner_id);
                    for x in exit.x + 1..gutter_x {
                        set_cycle_edge_char(canvas, x, exit.row, style.back_h, style, owner_id);
                    }
                    exit.row
                }
                (None, Some(detour_y)) => {
                    let stub_x = from.x + from.width;
                    let jog_x = stub_x + 1;
                    let (jog_turn, detour_turn) = if detour_y > from_y {
                        (style.corner_dr, style.corner_ul)
                    } else {
                        (style.corner_ur, style.corner_dl)
                    };
                    set_cycle_char(canvas, jog_x, from_y, jog_turn, owner_id);
                    set_cycle_char(canvas, jog_x, detour_y, detour_turn, owner_id);
                    set_cycle_edge_char(canvas, stub_x, from_y, style.back_h, style, owner_id);
                    for y in from_y.min(detour_y) + 1..from_y.max(detour_y) {
                        set_cycle_edge_char(canvas, jog_x, y, style.back_v, style, owner_id);
                    }
                    for x in jog_x + 1..gutter_x {
                        set_cycle_edge_char(canvas, x, detour_y, style.back_h, style, owner_id);
                    }
                    detour_y
                }
                (None, None) => {
                    for x in (from.x + from.width)..gutter_x {
                        set_cycle_edge_char(canvas, x, from_y, style.back_h, style, owner_id);
                    }
                    from_y
                }
            };
            let from_y = source_y;

            // The run from the gutter into the target follows the target's row,
            // unless a box on that row sends it along its planned detour row
            // below the target's rank, with a jog up into the target's side.
            // Taken only over blank cells and lines it crosses, like the source
            // detour.
            let jog_x = to.x + to.width + 1;
            // A planned row is clear when the jog may cross only plain horizontal
            // lines (a merge bus) on its way down and its corners and the row
            // itself meet only blanks and vertical lines.
            let clear_row = |detour_y: usize| {
                detour_y < canvas.height
                    && detour_y > to_y
                    && canvas.get(jog_x, to_y) == ' '
                    && canvas.get(jog_x, detour_y) == ' '
                    && (to_y + 1..detour_y).all(|y| {
                        let c = canvas.get(jog_x, y);
                        c == ' '
                            || (super::canvas::is_horizontal(c, style)
                                && !super::canvas::is_junction(c, style))
                    })
                    && (jog_x + 1..=gutter_x).all(|x| {
                        let c = canvas.get(x, detour_y);
                        c == ' ' || super::canvas::is_vertical(c, style)
                    })
            };
            // Of those, prefer a row with no long run beside it: two detours on
            // adjacent rows read as one doubled line.
            let beside_a_run = |detour_y: usize| {
                (jog_x + 1..=gutter_x)
                    .filter(|&x| {
                        [detour_y.wrapping_sub(1), detour_y + 1]
                            .iter()
                            .any(|&y| super::canvas::is_horizontal(canvas.get(x, y), style))
                    })
                    .count()
                    > 3
            };
            let target_detour = target_detour.and_then(|rows| {
                let open: Vec<usize> = rows.iter().copied().filter(|&y| clear_row(y)).collect();
                open.iter()
                    .copied()
                    .find(|&y| !beside_a_run(y))
                    .or_else(|| open.first().copied())
            });
            let entry_y = target_detour.unwrap_or(to_y);

            // A turn onto a gutter column another loop already runs down joins it
            // with a tee rather than cutting it with a corner (read before this
            // loop draws its own run down the column).
            let joins = |canvas: &Canvas, y: usize| {
                super::canvas::is_vertical(canvas.get(gutter_x, y), style)
            };
            let (source_joins, entry_joins) = (joins(canvas, from_y), joins(canvas, entry_y));

            // Vertical line in gutter
            let (top, bottom) = if from_y < entry_y {
                (from_y, entry_y)
            } else {
                (entry_y, from_y)
            };
            for y in top..=bottom {
                set_cycle_edge_char(canvas, gutter_x, y, style.back_v, style, owner_id);
            }

            // Horizontal line from gutter to target
            match target_detour {
                Some(detour_y) => {
                    set_cycle_char(canvas, jog_x, to_y, style.corner_dr, owner_id);
                    set_cycle_char(canvas, jog_x, detour_y, style.corner_ul, owner_id);
                    for y in to_y + 1..detour_y {
                        set_cycle_edge_char(canvas, jog_x, y, style.back_v, style, owner_id);
                    }
                    for x in jog_x + 1..gutter_x {
                        set_cycle_edge_char(canvas, x, detour_y, style.back_h, style, owner_id);
                    }
                }
                None => {
                    for x in (to.x + to.width)..gutter_x {
                        set_cycle_edge_char(canvas, x, to_y, style.back_h, style, owner_id);
                    }
                }
            }

            // Corners at source-to-gutter and gutter-to-target junctions
            // Path: horizontal right from source → vertical in gutter → horizontal left to target
            let turn = |joins: bool, corner: char| if joins { style.junction_left } else { corner };
            if from_y < entry_y {
                // Downward: right-then-down at source, down-then-left at target
                let source_turn = turn(source_joins, style.corner_dr); // ┐
                let target_turn = turn(entry_joins, style.corner_ur); // ┘
                set_cycle_char(canvas, gutter_x, from_y, source_turn, owner_id);
                set_cycle_char(canvas, gutter_x, entry_y, target_turn, owner_id);
            } else {
                // Upward: right-then-up at source, up-then-left at target
                let source_turn = turn(source_joins, style.corner_ur); // ┘
                let target_turn = turn(entry_joins, style.corner_dr); // ┐
                set_cycle_char(canvas, gutter_x, from_y, source_turn, owner_id);
                set_cycle_char(canvas, gutter_x, entry_y, target_turn, owner_id);
            }

            // Arrow pointing into target - back-edge always enters from right gutter
            let end = cycle_end_char(kind, style.arrow_left, style.back_h, style);
            set_cycle_end(canvas, to.x + to.width, to_y, end, kind, owner_id);
            // A bidirectional back edge also points back into its source.
            if kind == EdgeKind::Bidirectional {
                match bottom_exit {
                    Some(exit) => {
                        set_cycle_char(canvas, exit.x, from.bottom_y(), style.arrow_up, owner_id);
                    }
                    None => set_cycle_char(
                        canvas,
                        from.x + from.width,
                        from.center_y(),
                        style.arrow_left,
                        owner_id,
                    ),
                }
            }
        }
        Direction::LR | Direction::RL => {
            // Horizontal layout: use bottom gutter for back-edge
            // Back-edge goes: down from source → horizontal in gutter → up to target

            // Self-loop detection: route around the right side of the node
            if from.id == to.id {
                let node_right = from.x + from.width;
                let node_bottom = from.bottom_y();
                let loop_offset = spacing.row_spacing.max(2);
                let gutter_y = node_bottom + loop_offset;

                if gutter_y >= canvas.height {
                    return;
                }

                let center_x = from.x + from.width / 2;
                let loop_right = node_right + 2;

                if loop_right >= canvas.width {
                    return;
                }

                // Place corners FIRST to prevent overlap resolution converting them to crosses.
                // Path: right from node -> down -> left -> up into node bottom.
                set_cycle_char(
                    canvas,
                    loop_right,
                    from.center_y(),
                    style.corner_dr,
                    owner_id,
                );
                set_cycle_char(canvas, loop_right, gutter_y, style.corner_ur, owner_id);
                set_cycle_char(canvas, center_x, gutter_y, style.corner_ul, owner_id);

                // Draw lines EXCLUDING corner positions
                // Horizontal bridge from the node exit to the right-side loop column.
                for x in node_right..loop_right {
                    set_cycle_edge_char(canvas, x, from.center_y(), style.back_h, style, owner_id);
                }
                // Vertical down from right of box (below the top corner)
                for y in (from.center_y() + 1)..gutter_y {
                    set_cycle_edge_char(canvas, loop_right, y, style.back_v, style, owner_id);
                }
                // Horizontal in gutter (between the two corners)
                for x in (center_x + 1)..loop_right {
                    set_cycle_edge_char(canvas, x, gutter_y, style.back_h, style, owner_id);
                }
                // Vertical up from gutter to box bottom (above the bottom corner)
                for y in (node_bottom + 1)..gutter_y {
                    set_cycle_edge_char(canvas, center_x, y, style.back_v, style, owner_id);
                }
                // Place the visible arrow on the outside entry row so the later
                // box redraw doesn't erase it.
                if node_bottom < canvas.height {
                    let end = cycle_end_char(kind, style.arrow_up, style.back_v, style);
                    set_cycle_end(canvas, center_x, node_bottom, end, kind, owner_id);
                }
                return;
            }

            // Calculate a gutter position below the nodes
            let nodes_bottom = from.bottom_y().max(to.bottom_y());
            let gutter_y = lane.map_or(
                nodes_bottom + spacing.row_spacing.max(2), // Add spacing below nodes
                |lane| lane.gutter_y,
            );

            if gutter_y >= canvas.height {
                // Not enough space for gutter, skip
                return;
            }

            let from_x = lane.map_or(from.x + from.width / 2, |lane| lane.from_x);
            let to_x = lane.map_or(to.x + to.width / 2, |lane| lane.to_x);

            // Vertical line from source box bottom down to gutter
            for y in from.bottom_y()..=gutter_y {
                set_cycle_edge_char(canvas, from_x, y, style.back_v, style, owner_id);
            }

            // Horizontal line in gutter connecting the two vertical lines
            let (left, right) = if from_x < to_x {
                (from_x, to_x)
            } else {
                (to_x, from_x)
            };
            for x in left..=right {
                set_cycle_edge_char(canvas, x, gutter_y, style.back_h, style, owner_id);
            }

            // Vertical line from gutter up to target box bottom
            for y in to.bottom_y()..=gutter_y {
                set_cycle_edge_char(canvas, to_x, y, style.back_v, style, owner_id);
            }

            // Arrow pointing into target's bottom (where it enters from the gutter)
            // The back-edge enters from below, so we draw an up-arrow at the target's bottom
            let target_bottom_y = to.bottom_y();

            // Place the visible arrow on the outside entry row so the later box
            // redraw doesn't erase it.
            if target_bottom_y < canvas.height {
                let end = cycle_end_char(kind, style.arrow_up, style.back_v, style);
                set_cycle_end(canvas, to_x, target_bottom_y, end, kind, owner_id);
            }
            // A bidirectional back edge also points back up into its source.
            if kind == EdgeKind::Bidirectional && from.bottom_y() < canvas.height {
                set_cycle_char(canvas, from_x, from.bottom_y(), style.arrow_up, owner_id);
            }
        }
    }
}

/// The glyph a back edge ends with at its target: a circle or cross marker by
/// kind, no head for an open link, otherwise `arrow`.
fn cycle_end_char(kind: EdgeKind, arrow: char, shaft: char, style: &StyleChars) -> char {
    match kind {
        EdgeKind::CircleEnd => style.circle_end,
        EdgeKind::CrossEnd => style.cross_end,
        EdgeKind::Open => shaft,
        _ => arrow,
    }
}

/// Write a back edge's target end; a circle or cross marker keeps the
/// endpoint-marker role forward edges give it.
fn set_cycle_end(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    ch: char,
    kind: EdgeKind,
    owner_id: Option<&str>,
) {
    match owner_id {
        Some(owner_id) if matches!(kind, EdgeKind::CircleEnd | EdgeKind::CrossEnd) => {
            canvas.set_owned_with_role(
                x,
                y,
                ch,
                CellOwnerKind::CycleEdge,
                owner_id,
                CellRole::EndpointMarker,
                CYCLE_Z_INDEX,
            );
        }
        _ => set_cycle_char(canvas, x, y, ch, owner_id),
    }
}

fn set_cycle_char(canvas: &mut Canvas, x: usize, y: usize, ch: char, owner_id: Option<&str>) {
    if let Some(owner_id) = owner_id {
        canvas.set_owned(x, y, ch, CellOwnerKind::CycleEdge, owner_id, CYCLE_Z_INDEX);
    } else {
        canvas.set(x, y, ch);
    }
}

fn set_cycle_edge_char(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    ch: char,
    style: &StyleChars,
    owner_id: Option<&str>,
) {
    // A thick or dotted back edge keeps its kind only on cells it has to
    // itself; a cell another edge already uses stays plain, so a shared
    // line never takes one edge's style.
    let ch = if canvas.get(x, y) == ' ' {
        ch
    } else if ch == style.back_h && style.back_h != style.edge_h {
        style.edge_h
    } else if ch == style.back_v && style.back_v != style.edge_v {
        style.edge_v
    } else {
        ch
    };
    if let Some(owner_id) = owner_id {
        canvas.set_edge_char_owned(
            x,
            y,
            ch,
            style,
            CellOwnerKind::CycleEdge,
            owner_id,
            CYCLE_Z_INDEX,
        );
    } else {
        canvas.set_edge_char(x, y, ch, style);
    }
}

/// A planned LR/RL back-edge route: exit and entry columns plus gutter row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HorizontalCycleLane {
    pub from_x: usize,
    pub to_x: usize,
    pub gutter_y: usize,
}

/// Plan, for each TD/TB back edge whose run from its source to the right
/// gutter would cross a box on the source's row, the row it detours along
/// instead: below the source's rank, with one blank row between it and the
/// boxes.  BT keeps its row run: its back-edge sources sit on the top rank,
/// with no room above, and below them run the forward edges.
pub fn plan_vertical_cycle_source_detours(graph: &Graph) -> HashMap<usize, usize> {
    let mut detours = HashMap::new();
    if !matches!(graph.direction, Direction::TD | Direction::TB) {
        return detours;
    }
    let node = |id: &str| graph.nodes.iter().find(|node| node.id == id);
    let last_row = |node: &Node| node.y + node.height.max(3) - 1;
    for (idx, edge) in graph.edges.iter().enumerate() {
        if !edge.is_back_edge || edge.from == edge.to {
            continue;
        }
        let (Some(from), Some(_)) = (node(&edge.from), node(&edge.to)) else {
            continue;
        };
        let run_start = from.x + from.width;
        let jog_x = run_start + 1;
        let row = from.center_y();
        let on_run = |node: &&Node, y: usize| {
            node.id != from.id
                && node.x + node.width > run_start
                && node.y <= y
                && y <= last_row(node)
        };
        let blockers: Vec<&Node> = graph.nodes.iter().filter(|n| on_run(n, row)).collect();
        // The jog needs a blank column between it and the first box.
        if blockers.is_empty() || blockers.iter().any(|n| n.x <= jog_x + 1) {
            continue;
        }
        let mut detour = blockers
            .iter()
            .map(|n| last_row(n))
            .fold(last_row(from), usize::max)
            + 2;
        // Step past any later box the detour row would run into.
        for _ in 0..64 {
            let Some(next) = graph.nodes.iter().find(|n| {
                n.x + n.width > jog_x
                    && n.y.saturating_sub(1) <= detour
                    && detour <= last_row(n) + 1
            }) else {
                break;
            };
            detour = last_row(next) + 2;
        }
        // The jog runs down from the source row to the detour row; a box on
        // a later rank that spans its column would read as the edge's source.
        let jog_blocked = graph.nodes.iter().any(|n| {
            n.id != from.id
                && n.x <= jog_x + 1
                && jog_x <= n.x + n.width
                && n.y <= detour
                && row <= last_row(n)
        });
        if jog_blocked {
            continue;
        }
        detours.insert(idx, detour);
    }
    detours
}

/// Plan, for each TD/TB back edge whose run from the right gutter into its
/// target would cross a box on the target's row, the row it enters along
/// instead: below the target's rank, with one blank row between it and the
/// boxes, and a jog up into the target's side.
pub fn plan_vertical_cycle_target_detours(graph: &Graph) -> HashMap<usize, Vec<usize>> {
    let mut detours = HashMap::new();
    if !matches!(graph.direction, Direction::TD | Direction::TB) {
        return detours;
    }
    let node = |id: &str| graph.nodes.iter().find(|node| node.id == id);
    let last_row = |node: &Node| node.y + node.height.max(3) - 1;
    for (idx, edge) in graph.edges.iter().enumerate() {
        if !edge.is_back_edge || edge.from == edge.to {
            continue;
        }
        let (Some(from), Some(to)) = (node(&edge.from), node(&edge.to)) else {
            continue;
        };
        let run_start = to.x + to.width;
        let jog_x = run_start + 1;
        let row = to.center_y();
        let blockers: Vec<&Node> = graph
            .nodes
            .iter()
            .filter(|n| {
                n.id != to.id && n.x + n.width > run_start && n.y <= row && row <= last_row(n)
            })
            .collect();
        // The jog needs a blank column between it and the first box.
        if blockers.is_empty() || blockers.iter().any(|n| n.x <= jog_x + 1) {
            continue;
        }
        let detour = blockers
            .iter()
            .map(|n| last_row(n))
            .fold(last_row(to), usize::max)
            + 2;
        // A detour row must lie above the source's row, clear of every box it
        // runs past, and the jog column clear of boxes up to it.
        let clear = |detour: usize| {
            let row_blocked = graph.nodes.iter().any(|n| {
                n.x + n.width > jog_x
                    && n.y.saturating_sub(1) <= detour
                    && detour <= last_row(n) + 1
            });
            let jog_blocked = graph.nodes.iter().any(|n| {
                n.id != to.id
                    && n.x <= jog_x + 1
                    && jog_x <= n.x + n.width
                    && n.y <= detour
                    && row <= last_row(n)
            });
            detour < from.center_y() && !row_blocked && !jog_blocked
        };
        // Rows further down are offered too, in order, for the router to take
        // the first a line drawn since (a merge bus, another detour) leaves
        // clear. Never a frame's border row; rows right beside one come last,
        // since a run there reads as a doubled border.
        let borders: Vec<usize> = graph
            .subgraphs
            .iter()
            .filter(|sg| sg.bounds.is_valid() && sg.bounds.x + sg.bounds.width > jog_x)
            .flat_map(|sg| [sg.bounds.y, sg.bounds.y + sg.bounds.height - 1])
            .collect();
        let open: Vec<usize> = (detour..=detour + 6)
            .filter(|&candidate| clear(candidate) && !borders.contains(&candidate))
            .collect();
        let hugs = |row: usize| borders.iter().any(|&border| border.abs_diff(row) == 1);
        let candidates: Vec<usize> = open
            .iter()
            .copied()
            .filter(|&row| !hugs(row))
            .chain(open.iter().copied().filter(|&row| hugs(row)))
            .collect();
        if candidates.is_empty() {
            continue;
        }
        detours.insert(idx, candidates);
    }

    // A jog meets its target beside the target's side cell; another back
    // edge still running along that row, into its own target or out of its
    // source, would pass that cell and turn the jog's corner into a tee.
    // Dropping a detour puts its edge back on the row, so repeat until none
    // changes.
    let row_runs =
        |detours: &HashMap<usize, Vec<usize>>, row: usize, past_x: usize, skip: usize| {
            graph.edges.iter().enumerate().any(|(idx, edge)| {
                idx != skip
                    && edge.is_back_edge
                    && edge.from != edge.to
                    && [(&edge.to, !detours.contains_key(&idx)), (&edge.from, true)]
                        .into_iter()
                        .filter(|(_, on_row)| *on_row)
                        .filter_map(|(id, _)| node(id))
                        .any(|n| n.center_y() == row && n.x + n.width <= past_x)
            })
        };
    loop {
        let crossed: Vec<usize> = detours
            .keys()
            .copied()
            .filter(|&idx| {
                node(&graph.edges[idx].to)
                    .is_some_and(|to| row_runs(&detours, to.center_y(), to.x + to.width + 1, idx))
            })
            .collect();
        if crossed.is_empty() {
            break;
        }
        for idx in crossed {
            detours.remove(&idx);
        }
    }
    detours
}

/// A TD/TB back edge that leaves its source through the bottom border: the
/// border column it drops from and the row it runs to the gutter along.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CycleBottomExit {
    pub x: usize,
    pub row: usize,
}

/// Plan a bottom-border exit for each TD/TB back edge whose source also
/// takes a back edge in (or loops on itself). Both would otherwise use the
/// source's one side cell, where an arrow in hides the exit. The exit drops
/// from the border right of the forward stem to a row below the rank, one
/// blank row clear of the boxes, and runs along it to the gutter. Below a
/// source that loops on itself, the row runs under that loop rather than
/// alongside its bottom.
pub fn plan_vertical_cycle_bottom_exits(
    graph: &Graph,
    spacing: &SpacingConfig,
) -> HashMap<usize, CycleBottomExit> {
    let mut exits = HashMap::new();
    if !matches!(graph.direction, Direction::TD | Direction::TB) {
        return exits;
    }
    let node = |id: &str| graph.nodes.iter().find(|node| node.id == id);
    let last_row = |node: &Node| node.y + node.height.max(3) - 1;
    for (idx, edge) in graph.edges.iter().enumerate() {
        if !edge.is_back_edge || edge.from == edge.to {
            continue;
        }
        let Some(from) = node(&edge.from) else {
            continue;
        };
        let takes_one_in = graph
            .edges
            .iter()
            .any(|other| other.is_back_edge && other.to == from.id);
        if !takes_one_in || !from.shape.has_flat_vertical_entry() {
            continue;
        }
        // On the centre of a box with no forward exit; otherwise right of the
        // forward stem, a blank border cell clear of each side (an ASCII `+`
        // beside the corner `+` reads as a doubled corner), or failing that
        // just inside the corner.
        let leaves_forward = graph
            .edges
            .iter()
            .any(|other| !other.is_back_edge && other.from == from.id);
        let x = if !leaves_forward {
            from.center_x()
        } else {
            let clear = from.x + from.width.saturating_sub(3);
            let tight = from.x + from.width.saturating_sub(2);
            if clear >= from.center_x() + 2 {
                clear
            } else if tight >= from.center_x() + 2 {
                tight
            } else {
                continue;
            }
        };
        let loops_on_itself = graph
            .edges
            .iter()
            .any(|other| other.from == from.id && other.to == from.id);
        // Under the self-loop when the source loops on itself (a row above the
        // loop's bottom would cross both its legs), else just below the rank.
        let under_loop = loops_on_itself.then(|| from.bottom_y() + spacing.row_spacing.max(2) + 2);
        let clear_row = |start: usize| {
            let mut row = start;
            for _ in 0..64 {
                let Some(next) = graph.nodes.iter().find(|n| {
                    n.x + n.width > x && n.y.saturating_sub(1) <= row && row <= last_row(n) + 1
                }) else {
                    break;
                };
                row = last_row(next) + 2;
            }
            let drop_blocked = graph.nodes.iter().any(|n| {
                n.id != from.id
                    && n.x <= x + 1
                    && x <= n.x + n.width
                    && n.y <= row
                    && from.bottom_y() <= last_row(n)
            });
            (!drop_blocked).then_some(row)
        };
        let Some(row) = (match under_loop {
            Some(row) => clear_row(row),
            None => clear_row(last_row(from) + 2),
        }) else {
            continue;
        };
        exits.insert(idx, CycleBottomExit { x, row });
    }
    exits
}

/// A loop in a TD/BT gutter column: its top and bottom rows, whether it is a
/// self-loop, and its edge kind.
type GutterLoop = (usize, usize, (bool, EdgeKind));

/// Plan the right-gutter column of each TD/TB/BT back edge, as an inset in
/// column pairs from the outermost one, so loops whose rows meet never share
/// a column and merge.  Shorter loops take the inner columns so they nest;
/// loops whose rows lie apart share one.  `source_detours` and
/// `target_detours` give the rows a back edge leaves its source and enters
/// its target on when it detours.
pub fn plan_vertical_cycle_gutters(
    graph: &Graph,
    spacing: &SpacingConfig,
    source_detours: &HashMap<usize, usize>,
    target_detours: &HashMap<usize, Vec<usize>>,
) -> HashMap<usize, usize> {
    let mut insets = HashMap::new();
    if !matches!(
        graph.direction,
        Direction::TD | Direction::TB | Direction::BT
    ) {
        return insets;
    }
    let node = |id: &str| graph.nodes.iter().find(|node| node.id == id);
    let loop_offset = spacing.row_spacing.max(2);
    let mut spans: Vec<(usize, usize, usize, usize)> = graph
        .edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| edge.is_back_edge)
        .filter_map(|(idx, edge)| {
            let from = node(&edge.from)?;
            let to = node(&edge.to)?;
            let (top, bottom) = if from.id == to.id {
                match graph.direction {
                    Direction::BT => (from.y.saturating_sub(loop_offset), from.center_y()),
                    _ => (from.center_y(), from.bottom_y() + loop_offset),
                }
            } else {
                let from_y = source_detours.get(&idx).copied().unwrap_or(from.center_y());
                let to_y = target_detours
                    .get(&idx)
                    .and_then(|rows| rows.first().copied())
                    .unwrap_or(to.center_y());
                (from_y.min(to_y), from_y.max(to_y))
            };
            Some((bottom - top, top, bottom, idx))
        })
        .collect();
    if spans.len() < 2 {
        return insets;
    }
    spans.sort_unstable();

    // Inner columns first: each loop takes the innermost column whose loops
    // all lie at least one blank row away from it.  Loops on the same pair of
    // rows already share both runs, so a second column would only draw a
    // ladder; in one style they keep one column and read as a fan-in.
    let kind = |idx: usize| {
        let edge = &graph.edges[idx];
        (edge.from == edge.to, edge.kind)
    };
    let mut columns: Vec<Vec<GutterLoop>> = Vec::new();
    let mut placed = Vec::new();
    for (_, top, bottom, idx) in spans {
        let loop_rows = (top, bottom, kind(idx));
        let column = columns
            .iter()
            .position(|rows| rows.contains(&loop_rows))
            .or_else(|| {
                columns
                    .iter()
                    .position(|rows| rows.iter().all(|&(t, b, _)| bottom + 1 < t || b + 1 < top))
            })
            .unwrap_or_else(|| {
                columns.push(Vec::new());
                columns.len() - 1
            });
        columns[column].push(loop_rows);
        placed.push((idx, column));
    }
    let outermost = columns.len() - 1;
    for (idx, column) in placed {
        insets.insert(idx, outermost - column);
    }
    insets
}

/// A back-edge endpoint at a node: (sort key, edge index, is the exit).
type CycleEndpoint = ((bool, isize, usize), usize, bool);

/// Plan LR/RL back-edge lanes so that no two back edges share a column at a
/// node or a gutter row over overlapping spans, and no gutter run lies on or
/// beside a subgraph's top or bottom border. A back edge without such a
/// conflict keeps the default geometry (box centers, first gutter row). A run
/// that would cross an unrelated box drops below it.
pub fn plan_horizontal_cycle_lanes(
    graph: &Graph,
    spacing: &SpacingConfig,
) -> HashMap<usize, HorizontalCycleLane> {
    let mut lanes = HashMap::new();
    if !matches!(graph.direction, Direction::LR | Direction::RL) {
        return lanes;
    }
    let node = |id: &str| graph.nodes.iter().find(|node| node.id == id);
    let center = |node: &Node| node.x + node.width / 2;
    let back_edges: Vec<(usize, &Node, &Node)> = graph
        .edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| edge.is_back_edge && edge.from != edge.to)
        .filter_map(|(idx, edge)| Some((idx, node(&edge.from)?, node(&edge.to)?)))
        .collect();
    if back_edges.is_empty() {
        return lanes;
    }

    let borders: Vec<(usize, usize, usize)> = graph
        .subgraphs
        .iter()
        .filter(|subgraph| subgraph.bounds.width > 0 && subgraph.bounds.height > 0)
        .flat_map(|subgraph| {
            let bounds = &subgraph.bounds;
            let right = bounds.x + bounds.width - 1;
            [
                (bounds.y, bounds.x, right),
                (bounds.y + bounds.height - 1, bounds.x, right),
            ]
        })
        .collect();

    // Rows first, on box centers: shorter spans take the upper rows so the
    // loops nest. Spans that only touch at a shared node do not conflict;
    // their endpoints are split into separate columns below.
    let mut order: Vec<(usize, usize, usize)> = back_edges
        .iter()
        .map(|&(idx, from, to)| {
            let (left, right) = (center(from).min(center(to)), center(from).max(center(to)));
            (right - left, left, idx)
        })
        .collect();
    order.sort_unstable();
    let mut rows: HashMap<usize, usize> = HashMap::new();
    let mut placed: Vec<(usize, usize, usize)> = Vec::new();
    for (_, left, idx) in order {
        let Some(&(_, from, to)) = back_edges.iter().find(|(edge_idx, _, _)| *edge_idx == idx)
        else {
            continue;
        };
        let right = center(from).max(center(to));
        let mut gutter_y = from.bottom_y().max(to.bottom_y()) + spacing.row_spacing.max(2);
        for _ in 0..64 {
            if let Some(&(border_y, _, _)) = borders.iter().find(|(border_y, b_left, b_right)| {
                border_y.abs_diff(gutter_y) <= 1 && *b_left <= right + 1 && left <= b_right + 1
            }) {
                gutter_y = border_y + 2;
                continue;
            }
            if placed
                .iter()
                .any(|&(row, p_left, p_right)| row == gutter_y && p_left < right && left < p_right)
            {
                gutter_y += 2;
                continue;
            }
            // A box on the gutter row would have the run cross it; step below
            // the box rather than give every back edge the default geometry.
            if let Some(bottom) = graph
                .nodes
                .iter()
                .filter(|other| other.id != from.id && other.id != to.id)
                .filter(|other| {
                    (other.y..other.bottom_y()).contains(&gutter_y)
                        && other.x <= right
                        && left < other.x + other.width
                })
                .map(Node::bottom_y)
                .max()
            {
                gutter_y = bottom + 1;
                continue;
            }
            break;
        }
        placed.push((gutter_y, left, right));
        rows.insert(idx, gutter_y);
    }

    // Columns second. At a node, routes heading left sit on its left side and
    // routes heading right on its right side; within a side the lower (outer)
    // loop takes the outer column so it never crosses an inner loop's run.
    let mut endpoints: HashMap<&str, Vec<CycleEndpoint>> = HashMap::new();
    for &(idx, from, to) in &back_edges {
        let row = rows.get(&idx).copied().unwrap_or(0);
        for (owner, other, is_exit) in [(from, to, true), (to, from, false)] {
            let heads_right = center(other) > center(owner);
            let depth = (row * 4096 + center(owner).abs_diff(center(other))) as isize;
            let outer_first = if heads_right { -depth } else { depth };
            endpoints.entry(owner.id.as_str()).or_default().push((
                (heads_right, outer_first, idx),
                idx,
                is_exit,
            ));
        }
    }
    let mut columns: HashMap<(usize, bool), usize> = HashMap::new();
    let mut unsplittable = false;
    for (node_id, mut ends) in endpoints {
        let Some(owner) = node(node_id) else {
            continue;
        };
        ends.sort_unstable();
        let count = ends.len();
        let usable = owner.width.saturating_sub(4);
        let step = if count < 2 {
            0
        } else if (count - 1) * 2 <= usable {
            2
        } else {
            0
        };
        // Separate rows over one shared column cross each other; a node that
        // cannot split its endpoints keeps every back edge on the default lane.
        unsplittable |= count > 1 && step == 0;
        let start = center(owner).saturating_sub((count - 1) * step / 2);
        for (position, (_, idx, is_exit)) in ends.into_iter().enumerate() {
            columns.insert((idx, is_exit), start + position * step);
        }
    }

    for &(idx, from, to) in &back_edges {
        let Some(&gutter_y) = rows.get(&idx) else {
            continue;
        };
        lanes.insert(
            idx,
            HorizontalCycleLane {
                from_x: columns.get(&(idx, true)).copied().unwrap_or(center(from)),
                to_x: columns.get(&(idx, false)).copied().unwrap_or(center(to)),
                gutter_y,
            },
        );
    }

    // A leg dropping through an unrelated box below its end moves to the
    // nearest column of that end's box that drops clear, with a blank
    // column either side and away from the end's other legs (gen_a_222).
    let mut used: Vec<(&str, usize)> = lanes
        .iter()
        .flat_map(|(idx, lane)| {
            let (_, from, to) = back_edges.iter().find(|(i, _, _)| i == idx).copied()?;
            Some([(from.id.as_str(), lane.from_x), (to.id.as_str(), lane.to_x)])
        })
        .flatten()
        .collect();
    let mut ids: Vec<usize> = lanes.keys().copied().collect();
    ids.sort_unstable();
    for idx in ids {
        let Some(&(_, from, to)) = back_edges.iter().find(|(i, _, _)| *i == idx) else {
            continue;
        };
        let gutter_y = lanes[&idx].gutter_y;
        for (owner, is_exit) in [(from, true), (to, false)] {
            let x = if is_exit {
                lanes[&idx].from_x
            } else {
                lanes[&idx].to_x
            };
            let hits = |x: usize| {
                graph
                    .nodes
                    .iter()
                    .filter(|other| other.id != from.id && other.id != to.id)
                    .any(|other| {
                        (other.x..other.x + other.width).contains(&x)
                            && owner.bottom_y() < other.bottom_y()
                            && other.y <= gutter_y
                    })
            };
            if !hits(x) {
                continue;
            }
            let taken = |c: usize| {
                used.iter()
                    .any(|&(id, other)| id == owner.id && other != x && other.abs_diff(c) <= 1)
            };
            let mut candidates: Vec<usize> =
                (owner.x + 2..(owner.x + owner.width).saturating_sub(2)).collect();
            candidates.sort_by_key(|&c| (c.abs_diff(x), c));
            let Some(moved) = candidates
                .into_iter()
                .find(|&c| !hits(c.saturating_sub(1)) && !hits(c) && !hits(c + 1) && !taken(c))
            else {
                continue;
            };
            if let Some(entry) = used.iter_mut().find(|(id, c)| *id == owner.id && *c == x) {
                entry.1 = moved;
            }
            let lane = lanes.get_mut(&idx).expect("lane");
            if is_exit {
                lane.from_x = moved;
            } else {
                lane.to_x = moved;
            }
        }
    }

    // a layout the gutter cannot fix; extra lanes would only add clutter, so
    // keep the default geometry for every back edge.
    let crosses_a_box = back_edges.iter().any(|&(idx, from, to)| {
        let Some(lane) = lanes.get(&idx) else {
            return false;
        };
        let (left, right) = (lane.from_x.min(lane.to_x), lane.from_x.max(lane.to_x));
        graph
            .nodes
            .iter()
            .filter(|other| other.id != from.id && other.id != to.id)
            .any(|other| {
                let (x0, x1) = (other.x, other.x + other.width);
                let (y0, y1) = (other.y, other.bottom_y());
                let leg =
                    |x: usize, top: usize| (x0..x1).contains(&x) && top < y1 && y0 <= lane.gutter_y;
                leg(lane.from_x, from.bottom_y())
                    || leg(lane.to_x, to.bottom_y())
                    || ((y0..y1).contains(&lane.gutter_y) && x0 <= right && left < x1)
            })
    });
    if crosses_a_box || unsplittable {
        lanes.clear();
    }
    lanes
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Calculate the visual center x-coordinate of a node.
#[inline]
pub fn center_x(node: &Node) -> usize {
    node.center_x()
}

/// Calculate the visual center y-coordinate of a node.
#[inline]
pub fn center_y(node: &Node) -> usize {
    node.center_y()
}

/// The column a TD/BT back edge closes its loop in when its run along the
/// source's or the target's row to the right gutter would cross a box no
/// planned detour avoids: the nearest one right of both ends whose runs on
/// the two rows meet no box and cross only vertical lines, and whose column
/// meets no box (with a blank column beside it) and no other vertical line,
/// with none beside it.
#[allow(clippy::too_many_arguments)]
fn local_cycle_gutter(
    from: &Node,
    to: &Node,
    boxes: &[Rect],
    canvas: &Canvas,
    style: &StyleChars,
    gutter_x: usize,
    source_covered: bool,
    target_covered: bool,
) -> Option<usize> {
    let others: Vec<&Rect> = boxes
        .iter()
        .filter(|rect| (rect.x, rect.y) != (from.x, from.y) && (rect.x, rect.y) != (to.x, to.y))
        .collect();
    let (from_y, to_y) = (from.center_y(), to.center_y());
    let (from_right, to_right) = (from.x + from.width, to.x + to.width);
    let run_meets_box = |start: usize, end: usize, row: usize| {
        others.iter().any(|rect| {
            rect.y <= row && row < rect.bottom() && rect.x < end && start < rect.right()
        })
    };
    let source_blocked = !source_covered && run_meets_box(from_right, gutter_x, from_y);
    let target_blocked = !target_covered && run_meets_box(to_right, gutter_x, to_y);
    if !source_blocked && !target_blocked {
        return None;
    }
    let (top, bottom) = (from_y.min(to_y), from_y.max(to_y));
    let run_clear = |start: usize, end: usize, row: usize| {
        (start..end).all(|x| {
            let c = canvas.get(x, row);
            c == ' ' || super::canvas::is_vertical(c, style)
        })
    };
    (from_right.max(to_right) + 2..gutter_x).find(|&x| {
        !run_meets_box(from_right, x + 2, from_y)
            && !run_meets_box(to_right, x + 2, to_y)
            && !others.iter().any(|rect| {
                rect.x <= x + 1 && x < rect.right() + 1 && rect.y <= bottom && top < rect.bottom()
            })
            && run_clear(from_right, x, from_y)
            && run_clear(to_right, x, to_y)
            && (top..=bottom).all(|y| {
                canvas.get(x, y) == ' '
                    && !super::canvas::is_vertical(canvas.get(x - 1, y), style)
                    && !super::canvas::is_vertical(canvas.get(x + 1, y), style)
            })
    })
}

#[cfg(test)]
mod tests {
    use super::super::canvas::Canvas;
    use super::*;
    use crate::graph::Direction;
    use crate::spacing::SpacingConfig;
    use crate::style::{BaseStyle, CompositeStyle};

    fn make_node(id: &str, x: usize, y: usize, width: usize) -> Node {
        Node {
            id: id.into(),
            label: id.into(),
            label_lines: Vec::new(),
            shape: crate::graph::NodeShape::Rectangle,
            click_target: None,
            x,
            y,
            width,
            height: crate::style::BOX_HEIGHT,
            rank: 0,
        }
    }

    fn unicode_chars() -> StyleChars {
        CompositeStyle::default().to_style_chars(BaseStyle::Unicode)
    }

    // ==========================================================================
    // Cycle Edge Routing Tests
    // ==========================================================================

    #[test]
    fn cycle_edge_routes_through_gutter_td() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(80, 40);

        let src = make_node("S", 10, 15, 7);
        let target = make_node("T", 10, 2, 7); // Target is ABOVE source (back-edge)

        let spacing = SpacingConfig::default_config();
        route_cycle_edge(
            &src,
            &target,
            &mut canvas,
            &chars,
            &spacing,
            Direction::TD,
            None,
        );

        // Back-edge uses right gutter
        let gutter_x = canvas.width - 2;

        // Corners should exist at junction points in gutter
        let src_mid_y = src.center_y();
        let target_mid_y = target.center_y();
        // src is below target (src_mid_y > target_mid_y), so this is an "upward cycle":
        // corner_ur (┘) at source (right-then-up), corner_dr (┐) at target (up-then-left)
        assert_eq!(canvas.get(gutter_x, src_mid_y), chars.corner_ur);
        assert_eq!(canvas.get(gutter_x, target_mid_y), chars.corner_dr);

        // Vertical line should exist between the corners
        let mid_y = (src_mid_y + target_mid_y) / 2;
        assert_eq!(canvas.get(gutter_x, mid_y), chars.back_v);

        // Arrow should point into target
        assert_eq!(
            canvas.get(target.x + target.width, target_mid_y),
            chars.arrow_left
        );
    }

    #[test]
    fn cycle_edge_invisible_nodes_is_noop() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(20, 20); // Small canvas

        // Nodes outside canvas bounds
        let src = make_node("S", 100, 100, 7);
        let target = make_node("T", 100, 50, 7);

        let spacing = SpacingConfig::default_config();
        route_cycle_edge(
            &src,
            &target,
            &mut canvas,
            &chars,
            &spacing,
            Direction::TD,
            None,
        );

        // Nothing should be drawn (gutter would be at x=18)
        assert_eq!(canvas.get(18, 10), ' ');
    }

    #[test]
    fn cycle_edge_lr_target_entry_uses_visible_arrow() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(80, 30);

        let target = make_node("A", 8, 2, 9);
        let source = make_node("B", 34, 2, 9);

        let spacing = SpacingConfig::default_config();
        route_cycle_edge(
            &source,
            &target,
            &mut canvas,
            &chars,
            &spacing,
            Direction::LR,
            None,
        );

        let target_entry_x = target.x + target.width / 2;
        let target_bottom_y = target.bottom_y();

        assert_eq!(canvas.get(target_entry_x, target_bottom_y), chars.arrow_up);
        assert_eq!(
            canvas.get(target_entry_x, target_bottom_y + 1),
            chars.back_v
        );
    }

    #[test]
    fn cycle_self_loop_lr_places_visible_arrow_below_box() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(40, 20);

        let node = make_node("Self", 8, 2, 8);

        let spacing = SpacingConfig::default_config();
        route_cycle_edge(
            &node,
            &node,
            &mut canvas,
            &chars,
            &spacing,
            Direction::LR,
            None,
        );

        let center_x = node.x + node.width / 2;
        let loop_right = node.x + node.width + 2;
        let gutter_y = node.bottom_y() + spacing.row_spacing.max(2);
        assert_eq!(
            canvas.get(node.x + node.width, node.center_y()),
            chars.back_h
        );
        assert_eq!(canvas.get(loop_right, node.center_y()), chars.corner_dr);
        assert_eq!(canvas.get(loop_right, gutter_y), chars.corner_ur);
        assert_eq!(canvas.get(center_x, gutter_y), chars.corner_ul);
        assert_eq!(canvas.get(center_x, node.bottom_y()), chars.arrow_up);
        assert_eq!(canvas.get(center_x, node.bottom_y() + 1), chars.back_v);
    }

    #[test]
    fn cycle_self_loop_bt_places_visible_return_leg() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(40, 20);

        let node = make_node("Self", 8, 10, 8);
        let spacing = SpacingConfig::default_config();
        route_cycle_edge(
            &node,
            &node,
            &mut canvas,
            &chars,
            &spacing,
            Direction::BT,
            None,
        );

        let gutter_x = canvas.width - 2;
        let return_x = node.x + node.width + 2;
        let loop_y = node.y - spacing.row_spacing.max(2);
        assert_eq!(
            canvas.get(node.x + node.width, node.center_y()),
            chars.arrow_left
        );
        assert_eq!(canvas.get(gutter_x, node.center_y()), chars.corner_ur);
        assert_eq!(canvas.get(gutter_x, loop_y), chars.corner_dr);
        assert_eq!(canvas.get(return_x, loop_y), chars.corner_dl);
        assert_eq!(canvas.get(return_x, node.center_y()), chars.junction_up);
        assert_eq!(canvas.get(return_x, node.center_y() - 1), chars.back_v);
    }

    #[test]
    fn cycle_self_loop_td_places_visible_return_leg() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(40, 20);

        let node = make_node("Self", 8, 2, 8);
        let spacing = SpacingConfig::default_config();
        route_cycle_edge(
            &node,
            &node,
            &mut canvas,
            &chars,
            &spacing,
            Direction::TD,
            None,
        );

        let gutter_x = canvas.width - 2;
        let return_x = node.x + node.width + 2;
        let loop_y = node.bottom_y() + spacing.row_spacing.max(2);
        assert_eq!(
            canvas.get(node.x + node.width, node.center_y()),
            chars.arrow_left
        );
        assert_eq!(canvas.get(gutter_x, node.center_y()), chars.corner_dr);
        assert_eq!(canvas.get(gutter_x, loop_y), chars.corner_ur);
        assert_eq!(canvas.get(return_x, loop_y), chars.corner_ul);
        assert_eq!(canvas.get(return_x, node.center_y()), chars.junction_down);
        assert_eq!(canvas.get(return_x, node.center_y() + 1), chars.back_v);
    }

    // ==========================================================================
    // Helper Function Tests
    // ==========================================================================

    #[test]
    fn center_x_calculates_correctly() {
        // Odd width: 7/2 = 3, so center is at x+3
        let node_odd = make_node("A", 10, 0, 7);
        assert_eq!(center_x(&node_odd), 13); // 10 + 3

        // Even width: 8/2 = 4, so center is at x+4
        let node_even = make_node("B", 10, 0, 8);
        assert_eq!(center_x(&node_even), 14); // 10 + 4

        // Width 1: 1/2 = 0, center is at x
        let node_min = make_node("C", 10, 0, 1);
        assert_eq!(center_x(&node_min), 10);
    }
}
