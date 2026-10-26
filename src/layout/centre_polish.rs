//! Final centre polish for vertical layouts.
//!
//! Barycenter balancing centres a fan-out parent over the average of its
//! children. With unequal child widths, or a child packed against its
//! neighbour, the parent can end up one or two columns off its middle child,
//! and the renderer then draws the trunk landing beside the middle branch
//! (`┴┬`). A full bend reads fine and a straight line reads best; a one- or
//! two-column miss reads as a mistake.
//!
//! This pass shifts top-level nodes by at most two columns when that lines
//! up another pair exactly, loses no existing alignment and adds no near
//! miss. A node moves together with the straight one-to-one chain it is
//! already aligned with, and every moved box keeps its gap to the other
//! boxes and to the subgraph envelopes.

use std::collections::{BTreeSet, HashMap};

use crate::geom::{Point, Rect};
use crate::graph::{Direction, Graph};
use crate::portals::compute_envelopes;

use super::CoarseLayoutConfig;

/// Two nodes whose centres should share a column.
struct Pair {
    a: String,
    b: String,
    /// A one-to-one edge, drawn as a straight line when aligned.
    chain: bool,
    /// Worth moving a node for: a one-to-one edge or a fan and its middle
    /// box. Any other edge only has to stay as straight as it was.
    goal: bool,
    /// A fan's middle box that the renderer may join on its left centre
    /// column: a flat, even-width box has two.
    flexible: Option<String>,
}

fn has_two_centre_columns(graph: &Graph, id: &str) -> bool {
    use crate::graph::NodeShape;

    graph.get_node(id).is_some_and(|node| {
        matches!(
            node.shape,
            NodeShape::Rectangle | NodeShape::Rounded | NodeShape::Stadium | NodeShape::Subroutine
        ) && node.width >= 4
            && node.width.is_multiple_of(2)
    })
}

/// Every edge (a straight one must stay straight), plus odd fan-outs and
/// their middle child and odd fan-ins and their middle source.
fn alignment_pairs(graph: &Graph, rects: &HashMap<String, Rect>) -> Vec<Pair> {
    let edges: Vec<_> = graph.edges.iter().filter(|e| !e.is_back_edge).collect();
    let out_degree = |id: &str| edges.iter().filter(|e| e.from == id).count();
    let in_degree = |id: &str| edges.iter().filter(|e| e.to == id).count();
    let centre = |id: &str| rects.get(id).map(|r| r.x * 2 + r.width);
    // Only a fan whose ends share a rank is drawn as one bus with a middle branch.
    let one_rank = |ids: &[&str]| {
        let rows: Vec<_> = ids
            .iter()
            .filter_map(|id| rects.get(*id).map(|r| r.y))
            .collect();
        rows.len() == ids.len() && rows.windows(2).all(|w| w[0] == w[1])
    };
    let flexible = |id: &str| has_two_centre_columns(graph, id).then(|| id.to_owned());

    let mut pairs = Vec::new();
    for edge in &edges {
        pairs.push(Pair {
            a: edge.from.clone(),
            b: edge.to.clone(),
            chain: out_degree(&edge.from) == 1 && in_degree(&edge.to) == 1,
            goal: out_degree(&edge.from) == 1 && in_degree(&edge.to) == 1,
            flexible: None,
        });
    }
    for node in &graph.nodes {
        let mut children: Vec<&str> = edges
            .iter()
            .filter(|e| e.from == node.id)
            .map(|e| e.to.as_str())
            .collect();
        children.sort_by_key(|id| (centre(id), *id));
        children.dedup();
        if children.len() >= 3 && children.len() % 2 == 1 && one_rank(&children) {
            let middle = children[children.len() / 2];
            // The renderer joins a plain branch into a single-entry box only.
            let plain_branch = in_degree(middle) == 1
                && edges
                    .iter()
                    .any(|e| e.from == node.id && e.to == middle && e.label.is_none());
            pairs.push(Pair {
                a: node.id.clone(),
                b: middle.to_owned(),
                chain: false,
                goal: true,
                flexible: flexible(middle).filter(|_| plain_branch),
            });
        }
        let mut parents: Vec<&str> = edges
            .iter()
            .filter(|e| e.to == node.id)
            .map(|e| e.from.as_str())
            .collect();
        parents.sort_by_key(|id| (centre(id), *id));
        parents.dedup();
        if parents.len() >= 3 && parents.len() % 2 == 1 && one_rank(&parents) {
            let middle = parents[parents.len() / 2];
            pairs.push(Pair {
                a: middle.to_owned(),
                b: node.id.clone(),
                chain: false,
                goal: true,
                // The renderer moves a single-exit source only.
                flexible: flexible(middle).filter(|_| out_degree(middle) == 1),
            });
        }
    }
    pairs
}

/// Column offset between the centres routes use.
fn centre_gap(pair: &Pair, rects: &HashMap<String, Rect>) -> Option<usize> {
    let centre = |rect: &Rect| rect.x + rect.width / 2;
    let (a, b) = (rects.get(&pair.a)?, rects.get(&pair.b)?);
    let gap = centre(a).abs_diff(centre(b));
    let left_gap = match pair.flexible.as_deref() {
        Some(id) if id == pair.a => (centre(a) - 1).abs_diff(centre(b)),
        Some(_) => centre(a).abs_diff(centre(b) - 1),
        None => gap,
    };
    Some(gap.min(left_gap))
}

/// (pairs aligned exactly, near misses of one or two columns) among `touched`.
fn score(
    pairs: &[Pair],
    touched: &[usize],
    rects: &HashMap<String, Rect>,
) -> (BTreeSet<usize>, usize) {
    let mut aligned = BTreeSet::new();
    let mut near = 0;
    for &index in touched {
        match centre_gap(&pairs[index], rects) {
            Some(0) => {
                aligned.insert(index);
            }
            Some(1 | 2) => near += 1,
            _ => {}
        }
    }
    (aligned, near)
}

/// The node plus every node joined to it by an aligned one-to-one chain.
fn move_unit(start: &str, pairs: &[Pair], rects: &HashMap<String, Rect>) -> Vec<String> {
    let mut unit = vec![start.to_owned()];
    let mut index = 0;
    while index < unit.len() {
        let id = unit[index].clone();
        for pair in pairs
            .iter()
            .filter(|pair| pair.chain && centre_gap(pair, rects) == Some(0))
        {
            let other = if pair.a == id {
                &pair.b
            } else if pair.b == id {
                &pair.a
            } else {
                continue;
            };
            if !unit.contains(other) {
                unit.push(other.clone());
            }
        }
        index += 1;
    }
    unit.sort();
    unit
}

fn keeps_clear(
    unit: &[String],
    moved: &HashMap<String, Rect>,
    rects: &HashMap<String, Rect>,
    envelopes: &[Rect],
    gap: usize,
) -> bool {
    moved.values().all(|rect| {
        let rows_overlap = |other: &Rect| other.y < rect.bottom() && rect.y < other.bottom();
        let boxes_clear = rects.iter().all(|(other_id, other)| {
            unit.contains(other_id)
                || !rows_overlap(other)
                || rect.right() + gap <= other.x
                || other.right() + gap <= rect.x
        });
        let frames_clear = envelopes
            .iter()
            .all(|env| !rows_overlap(env) || rect.right() < env.x || env.right() < rect.x);
        boxes_clear && frames_clear
    })
}

/// Nudge top-level nodes of a TD/BT placement onto the centre column of the
/// node they should line up with.
pub(super) fn polish_vertical_centres(
    graph: &Graph,
    config: &CoarseLayoutConfig,
    positions: &mut HashMap<String, Point>,
    rects: &mut HashMap<String, Rect>,
    prior_positions: Option<&HashMap<String, Point>>,
) {
    if !matches!(
        graph.direction,
        Direction::TD | Direction::TB | Direction::BT
    ) {
        return;
    }
    let pairs = alignment_pairs(graph, rects);
    if pairs.iter().all(|pair| pair.chain) {
        return;
    }
    // Subgraph members stay put, so the envelopes do not move either.
    let envelopes: Vec<Rect> = compute_envelopes(graph, rects, config.subgraph_gutter)
        .into_values()
        .map(|env| env.outer)
        .collect();
    let gap = config.min_horizontal_spacing;
    let movable = |id: &String| {
        graph.get_node_subgraph(id).is_none()
            && !prior_positions.is_some_and(|prior| prior.contains_key(id))
    };

    let mut starts: Vec<String> = pairs
        .iter()
        .filter(|pair| !pair.chain)
        .flat_map(|pair| [pair.a.clone(), pair.b.clone()])
        .filter(movable)
        .collect();
    starts.sort_by_key(|id| (rects.get(id).map(|r| (r.y, r.x)), id.clone()));
    starts.dedup();

    for _ in 0..4 {
        let mut changed = false;
        for start in &starts {
            let unit = move_unit(start, &pairs, rects);
            if !unit.iter().all(movable) {
                continue;
            }
            let touched: Vec<usize> = (0..pairs.len())
                .filter(|&index| unit.contains(&pairs[index].a) || unit.contains(&pairs[index].b))
                .collect();
            let (aligned_before, near_before) = score(&pairs, &touched, rects);
            let goals = |aligned: &BTreeSet<usize>| {
                aligned.iter().filter(|&&index| pairs[index].goal).count()
            };

            let mut best: Option<(usize, usize, HashMap<String, Rect>)> = None;
            for shift in [-1isize, 1, -2, 2] {
                let moved: Option<HashMap<String, Rect>> = unit
                    .iter()
                    .map(|id| {
                        let rect = *rects.get(id)?;
                        let x = rect.x.checked_add_signed(shift)?;
                        Some((id.clone(), Rect { x, ..rect }))
                    })
                    .collect();
                let Some(moved) = moved else {
                    continue;
                };
                if !keeps_clear(&unit, &moved, rects, &envelopes, gap) {
                    continue;
                }
                let mut trial = rects.clone();
                trial.extend(moved.clone());
                let (aligned_after, near_after) = score(&pairs, &touched, &trial);
                let improves = aligned_after.is_superset(&aligned_before)
                    && goals(&aligned_after) > goals(&aligned_before)
                    && near_after <= near_before;
                if improves
                    && best.as_ref().is_none_or(|(aligned, near, _)| {
                        (aligned_after.len(), std::cmp::Reverse(near_after))
                            > (*aligned, std::cmp::Reverse(*near))
                    })
                {
                    best = Some((aligned_after.len(), near_after, moved));
                }
            }
            if let Some((_, _, moved)) = best {
                for (id, rect) in moved {
                    if let Some(point) = positions.get_mut(&id) {
                        point.x = rect.x;
                    }
                    rects.insert(id, rect);
                }
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}
