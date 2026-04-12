//! Local render repair loop.
//!
//! Repairs operate on the already-rendered canvas. This keeps Phase 6 bounded:
//! it can improve local glyph correctness and portal/border integrity without
//! adding a second full layout engine.

use super::canvas::{is_horizontal, is_vertical, Canvas};
use super::critic::{analyze, CriticReport, FindingCode};
use super::provenance::{refresh_provenance, EdgeLabelPlacement};
use super::semantic::{CellOwnerKind, CellRole, SemanticFrame};
use super::subgraph_title_y;
use super::topology::{canonical_routing_glyph, canvas_connections};
use crate::graph::{Direction, Graph};
use crate::portals::PortalSlots;
use crate::style::StyleChars;
use std::collections::HashMap;

/// Apply a bounded number of local repair passes to the canvas.
#[allow(clippy::too_many_arguments)]
pub fn optimize_canvas(
    graph: &Graph,
    canvas: &mut Canvas,
    direction: Direction,
    chars: &StyleChars,
    subgraph_chars: &StyleChars,
    portal_slots: &HashMap<String, PortalSlots>,
    edge_label_placements: &[EdgeLabelPlacement],
    max_passes: usize,
) -> CriticReport {
    refresh_provenance(
        canvas,
        graph,
        chars,
        portal_slots,
        direction,
        edge_label_placements,
    );
    let mut last_report = analyze(graph, &SemanticFrame::from_canvas(canvas), direction, chars);

    for _ in 0..max_passes {
        let mut changed = false;

        for finding in &last_report.findings {
            match finding.code {
                FindingCode::UnusedPortalOpening => {
                    for &(x, y) in &finding.cells {
                        if apply_portal_fill(graph, canvas, x, y, subgraph_chars) {
                            changed = true;
                        }
                    }
                }
                FindingCode::JunctionTopologyMismatch => {
                    for &(x, y) in &finding.cells {
                        if normalize_routing_glyph(canvas, x, y, chars) {
                            changed = true;
                        }
                    }
                }
                FindingCode::RouteTopologyMismatch => {
                    for &(x, y) in &finding.cells {
                        if normalize_routing_glyph(canvas, x, y, chars) {
                            changed = true;
                        }
                    }
                }
                FindingCode::ArrowWithoutVisibleShaft => {
                    for &(x, y) in &finding.cells {
                        if extend_arrow_shaft(canvas, x, y, chars) {
                            changed = true;
                        }
                    }
                }
                FindingCode::SubgraphTitleCorrupted => {
                    for owner_id in &finding.owner_ids {
                        if restore_subgraph_title(graph, canvas, owner_id, direction) {
                            changed = true;
                        }
                    }
                }
                FindingCode::EdgeLabelCollidesWithNode => {
                    // Canvas-level repair: clear label cells that intrude into a
                    // node bounding box.  The label is already corrupted at this
                    // point; removing the cells is better than leaving garbled
                    // text on top of the node border.  Layout repair (in lib.rs)
                    // will attempt to prevent the collision in the next pass.
                    for &(x, y) in &finding.cells {
                        if canvas.get(x, y) != ' ' {
                            canvas.set(x, y, ' ');
                            changed = true;
                        }
                    }
                }
                _ => {}
            }
        }

        if !changed {
            break;
        }

        refresh_provenance(
            canvas,
            graph,
            chars,
            portal_slots,
            direction,
            edge_label_placements,
        );
        last_report = analyze(graph, &SemanticFrame::from_canvas(canvas), direction, chars);
    }

    last_report
}

/// Apply a cheap deterministic topology cleanup to already-drawn routing glyphs.
///
/// Unlike the full optimize loop, this does not consult the critic or mutate
/// labels/portals. It only normalizes edge-owned cells to the canonical glyph
/// implied by their visible neighbors. This keeps default rendering free of
/// obvious tee/corner mismatches without enabling heavier repair behavior.
pub fn stabilize_routing_topology(canvas: &mut Canvas, chars: &StyleChars) -> bool {
    let mut updates = Vec::new();

    for y in 0..canvas.height {
        for x in 0..canvas.width {
            let Some(meta) = canvas.get_meta(x, y).cloned() else {
                continue;
            };
            if !matches!(
                meta.owner_kind,
                CellOwnerKind::EdgeSegment | CellOwnerKind::CycleEdge | CellOwnerKind::Junction
            ) {
                continue;
            }
            if meta.role == CellRole::ArrowTip {
                continue;
            }

            let Some(replacement) =
                canonical_routing_glyph(canvas_connections(canvas, x, y), chars, meta.owner_kind)
            else {
                continue;
            };

            let current = canvas.get(x, y);
            // Don't downgrade styled variants (e.g. thick ┃→│ or ━→─).
            if (is_vertical(replacement, chars) && is_vertical(current, chars))
                || (is_horizontal(replacement, chars) && is_horizontal(current, chars))
            {
                continue;
            }

            if current != replacement {
                updates.push((x, y, replacement, meta));
            }
        }
    }

    let changed = !updates.is_empty();
    for (x, y, replacement, meta) in updates {
        if let Some(owner_id) = meta.owner_id.as_deref() {
            canvas.set_owned(x, y, replacement, meta.owner_kind, owner_id, meta.z_index);
        } else {
            canvas.set(x, y, replacement);
        }
    }

    changed
}

/// Apply a conservative default cleanup to edge-owned cells.
///
/// This only corrects cells whose visible neighbors clearly imply a straight
/// segment. It avoids the broader tee/corner/cross normalization that can
/// shift the baseline output of many fixtures, while still fixing obviously
/// wrong stray corners embedded in vertical or horizontal runs.
pub fn stabilize_straight_segments(canvas: &mut Canvas, chars: &StyleChars) -> bool {
    let mut updates = Vec::new();

    for y in 0..canvas.height {
        for x in 0..canvas.width {
            let Some(meta) = canvas.get_meta(x, y).cloned() else {
                continue;
            };
            if !matches!(
                meta.owner_kind,
                CellOwnerKind::EdgeSegment | CellOwnerKind::CycleEdge | CellOwnerKind::Junction
            ) {
                continue;
            }
            if meta.role == CellRole::ArrowTip {
                continue;
            }

            let Some(replacement) =
                canonical_routing_glyph(canvas_connections(canvas, x, y), chars, meta.owner_kind)
            else {
                continue;
            };
            if replacement != chars.h && replacement != chars.v {
                continue;
            }

            let current = canvas.get(x, y);
            // Don't downgrade styled variants (e.g. thick ┃→│ or ━→─).
            // If the current char is already a valid straight-line char in the
            // same axis as the replacement, leave it untouched.
            if (replacement == chars.v && is_vertical(current, chars))
                || (replacement == chars.h && is_horizontal(current, chars))
            {
                continue;
            }

            if current != replacement {
                updates.push((x, y, replacement, meta));
            }
        }
    }

    let changed = !updates.is_empty();
    for (x, y, replacement, meta) in updates {
        if let Some(owner_id) = meta.owner_id.as_deref() {
            canvas.set_owned(x, y, replacement, meta.owner_kind, owner_id, meta.z_index);
        } else {
            canvas.set(x, y, replacement);
        }
    }

    changed
}

/// Normalize cells explicitly marked as junctions.
///
/// This is narrower than full topology stabilization: it only touches cells
/// that were already classified as junctions, so it can fix obviously wrong tee
/// or corner picks without rewriting ordinary edge segments across the frame.
pub fn stabilize_junction_cells(canvas: &mut Canvas, chars: &StyleChars) -> bool {
    let mut updates = Vec::new();

    for y in 0..canvas.height {
        for x in 0..canvas.width {
            let Some(meta) = canvas.get_meta(x, y).cloned() else {
                continue;
            };
            if meta.owner_kind != CellOwnerKind::Junction {
                continue;
            }
            if meta.role == CellRole::ArrowTip {
                continue;
            }

            let Some(replacement) =
                canonical_routing_glyph(canvas_connections(canvas, x, y), chars, meta.owner_kind)
            else {
                continue;
            };

            if canvas.get(x, y) != replacement {
                updates.push((x, y, replacement, meta));
            }
        }
    }

    let changed = !updates.is_empty();
    for (x, y, replacement, meta) in updates {
        if let Some(owner_id) = meta.owner_id.as_deref() {
            canvas.set_owned(x, y, replacement, meta.owner_kind, owner_id, meta.z_index);
        } else {
            canvas.set(x, y, replacement);
        }
    }

    changed
}

/// Repair only edge-owned cells whose current glyph class disagrees with the
/// visible connection pattern.
///
/// This is the default-render compromise:
/// - do fix obvious degree mismatches such as `-` where a junction is needed
/// - do fix wrong corner/junction swaps like `┤` where only a corner fits
/// - do fix same-class corner orientation when topology makes the correct turn unambiguous
/// - do not rewrite same-class tee orientation unless a cross is involved
pub fn stabilize_degree_mismatches(canvas: &mut Canvas, chars: &StyleChars) -> bool {
    let mut updates = Vec::new();

    for y in 0..canvas.height {
        for x in 0..canvas.width {
            let Some(meta) = canvas.get_meta(x, y).cloned() else {
                continue;
            };
            if !matches!(
                meta.owner_kind,
                CellOwnerKind::EdgeSegment | CellOwnerKind::CycleEdge | CellOwnerKind::Junction
            ) {
                continue;
            }
            if meta.role == CellRole::ArrowTip {
                continue;
            }

            let Some(replacement) =
                canonical_routing_glyph(canvas_connections(canvas, x, y), chars, meta.owner_kind)
            else {
                continue;
            };
            if canvas.get(x, y) == replacement {
                continue;
            }

            let Some(current_class) = glyph_class(canvas.get(x, y), chars) else {
                continue;
            };
            let Some(replacement_class) = glyph_class(replacement, chars) else {
                continue;
            };

            let current = canvas.get(x, y);
            let should_update = current_class != replacement_class
                || (current_class == GlyphClass::Corner && replacement_class == GlyphClass::Corner)
                || (current_class == GlyphClass::Junction
                    && replacement_class == GlyphClass::Junction
                    && (current == chars.cross || replacement == chars.cross));

            if should_update {
                updates.push((x, y, replacement, meta));
            }
        }
    }

    let changed = !updates.is_empty();
    for (x, y, replacement, meta) in updates {
        if let Some(owner_id) = meta.owner_id.as_deref() {
            canvas.set_owned(x, y, replacement, meta.owner_kind, owner_id, meta.z_index);
        } else {
            canvas.set(x, y, replacement);
        }
    }

    changed
}

/// Backfill a single missing shaft cell immediately behind edge-owned arrows.
///
/// This is a narrow default-render cleanup: it only fills a blank cell directly
/// behind an existing arrow tip, leaving broader route reshaping to the
/// optimize loop.
pub fn stabilize_arrow_shafts(canvas: &mut Canvas, chars: &StyleChars) -> bool {
    let mut arrows = Vec::new();

    for y in 0..canvas.height {
        for x in 0..canvas.width {
            let ch = canvas.get(x, y);
            if !matches!(
                ch,
                '>' | '→' | '▶' | '<' | '←' | '◀' | '^' | '↑' | '▲' | 'v' | '↓' | '▼'
            ) {
                continue;
            }

            let owner_kind = canvas
                .get_meta(x, y)
                .map(|meta| meta.owner_kind)
                .unwrap_or_default();

            if matches!(
                owner_kind,
                CellOwnerKind::ArrowHead | CellOwnerKind::EdgeSegment | CellOwnerKind::CycleEdge
            ) {
                arrows.push((x, y));
            }
        }
    }

    let mut changed = false;
    for (x, y) in arrows {
        if extend_arrow_shaft(canvas, x, y, chars) {
            changed = true;
        }
    }

    changed
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GlyphClass {
    Straight,
    Corner,
    Junction,
}

fn glyph_class(ch: char, chars: &StyleChars) -> Option<GlyphClass> {
    if matches!(ch, c if c == chars.edge_h || c == chars.edge_v || c == chars.back_h || c == chars.back_v)
    {
        return Some(GlyphClass::Straight);
    }

    if matches!(
        ch,
        c if c == chars.corner_dr
            || c == chars.corner_dl
            || c == chars.corner_ur
            || c == chars.corner_ul
    ) {
        return Some(GlyphClass::Corner);
    }

    if matches!(
        ch,
        c if c == chars.junction_down
            || c == chars.junction_up
            || c == chars.junction_right
            || c == chars.junction_left
            || c == chars.cross
    ) {
        return Some(GlyphClass::Junction);
    }

    None
}

fn apply_portal_fill(
    graph: &Graph,
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    subgraph_chars: &StyleChars,
) -> bool {
    for sg in &graph.subgraphs {
        let bounds = &sg.bounds;
        let on_top_or_bottom = (y == bounds.y || y == bounds.y + bounds.height.saturating_sub(1))
            && x >= bounds.x
            && x < bounds.x + bounds.width;
        let on_left_or_right = (x == bounds.x || x == bounds.x + bounds.width.saturating_sub(1))
            && y >= bounds.y
            && y < bounds.y + bounds.height;

        if on_top_or_bottom {
            canvas.set(x, y, subgraph_chars.h);
            return true;
        }
        if on_left_or_right {
            canvas.set(x, y, subgraph_chars.v);
            return true;
        }
    }
    false
}

fn restore_subgraph_title(
    graph: &Graph,
    canvas: &mut Canvas,
    subgraph_id: &str,
    direction: Direction,
) -> bool {
    let Some(subgraph) = graph.get_subgraph(subgraph_id) else {
        return false;
    };
    let Some(title) = subgraph.title.as_deref() else {
        return false;
    };
    if !subgraph.bounds.is_valid() {
        return false;
    }

    let title_fmt = crate::graph::subgraph_title_text(title);
    let Some(start_x) = crate::graph::subgraph_title_start_x(
        subgraph.bounds.x,
        subgraph.bounds.width,
        title,
        direction,
    ) else {
        return false;
    };
    let title_y = subgraph_title_y(&subgraph.bounds, direction);

    let mut changed = false;
    for (idx, ch) in title_fmt.chars().enumerate() {
        let x = start_x + idx;
        if x >= canvas.width || title_y >= canvas.height {
            continue;
        }
        if canvas.get(x, title_y) != ch {
            canvas.set(x, title_y, ch);
            changed = true;
        }
    }

    changed
}

fn normalize_routing_glyph(canvas: &mut Canvas, x: usize, y: usize, chars: &StyleChars) -> bool {
    let owner_kind = canvas
        .get_meta(x, y)
        .map(|cell| cell.owner_kind)
        .unwrap_or_default();
    let Some(replacement) =
        canonical_routing_glyph(canvas_connections(canvas, x, y), chars, owner_kind)
    else {
        return false;
    };
    if canvas.get(x, y) != replacement {
        canvas.set(x, y, replacement);
        true
    } else {
        false
    }
}

fn extend_arrow_shaft(canvas: &mut Canvas, x: usize, y: usize, chars: &StyleChars) -> bool {
    let arrow = canvas.get(x, y);
    let (px, py, shaft) = match arrow {
        '>' | '→' | '▶' => (x.saturating_sub(1), y, chars.edge_h),
        '<' | '←' | '◀' => (x + 1, y, chars.edge_h),
        'v' | '↓' | '▼' => (x, y.saturating_sub(1), chars.edge_v),
        '^' | '↑' | '▲' => (x, y + 1, chars.edge_v),
        _ => return false,
    };

    if px < canvas.width && py < canvas.height && canvas.get(px, py) == ' ' {
        canvas.set(px, py, shaft);
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{BaseStyle, CompositeStyle};

    fn unicode_chars() -> StyleChars {
        CompositeStyle::default().to_style_chars(BaseStyle::Unicode)
    }

    #[test]
    fn normalize_routing_glyph_replaces_cross_with_tee_when_only_three_arms_exist() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(3, 3);
        canvas.set(1, 0, chars.edge_v);
        canvas.set(1, 1, chars.cross);
        canvas.set(0, 1, chars.edge_h);
        canvas.set(2, 1, chars.edge_h);

        assert!(normalize_routing_glyph(&mut canvas, 1, 1, &chars));
        assert_eq!(canvas.get(1, 1), chars.junction_up);
    }

    #[test]
    fn extend_arrow_shaft_backfills_missing_segment() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(3, 1);
        canvas.set(2, 0, '>');

        assert!(extend_arrow_shaft(&mut canvas, 2, 0, &chars));
        assert_eq!(canvas.get(1, 0), chars.edge_h);
    }

    #[test]
    fn stabilize_arrow_shafts_repairs_owned_left_arrow() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(4, 1);
        canvas.set_owned(1, 0, '←', CellOwnerKind::ArrowHead, "edge:0:A->B", 3);

        assert!(stabilize_arrow_shafts(&mut canvas, &chars));
        assert_eq!(canvas.get(2, 0), chars.edge_h);
    }

    #[test]
    fn normalize_routing_glyph_replaces_dangling_corner_with_vertical() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(3, 3);
        canvas.set(1, 1, chars.corner_ur);
        canvas.set(1, 2, chars.edge_v);

        assert!(normalize_routing_glyph(&mut canvas, 1, 1, &chars));
        assert_eq!(canvas.get(1, 1), chars.edge_v);
    }

    #[test]
    fn restore_subgraph_title_rewrites_corrupted_title_cells() {
        let mut graph = Graph::new();
        graph.direction = Direction::TD;
        let mut subgraph = crate::graph::Subgraph::new("sg", Some("Flow".to_string()));
        subgraph.bounds = crate::graph::Rectangle::new(0, 0, 14, 5);
        graph.add_subgraph(subgraph);

        let mut canvas = Canvas::new(14, 5);
        canvas.set(3, 0, '─');
        canvas.set(4, 0, '─');
        canvas.set(5, 0, '─');

        assert!(restore_subgraph_title(
            &graph,
            &mut canvas,
            "sg",
            Direction::TD
        ));
        let rendered = canvas.to_string_cropped(0);
        assert!(rendered.contains("Flow"));
    }

    #[test]
    fn restore_subgraph_title_rewrites_corrupted_bt_bottom_title_cells() {
        let mut graph = Graph::new();
        graph.direction = Direction::BT;
        let mut subgraph = crate::graph::Subgraph::new("sg", Some("Flow".to_string()));
        subgraph.bounds = crate::graph::Rectangle::new(0, 0, 14, 5);
        graph.add_subgraph(subgraph);

        let mut canvas = Canvas::new(14, 5);
        canvas.set(3, 4, '─');
        canvas.set(4, 4, '─');
        canvas.set(5, 4, '─');

        assert!(restore_subgraph_title(
            &graph,
            &mut canvas,
            "sg",
            Direction::BT
        ));
        let rendered = canvas.to_string_cropped(0);
        assert!(rendered.contains("Flow"));
    }
}
