//! Canvas - 2D character grid for diagram rendering.
//!
//! Provides the core `Canvas` type and character classification utilities
//! for detecting line types, junctions, and resolving overlapping characters.

use crate::graph::Node;
use crate::style::{display_char_width, StyleChars, ASCII_CHARS, UNICODE_CHARS};

use super::fallback_route::{
    FallbackPaint, FallbackRoutePlan, FallbackRouteRejection, FallbackRouteTrace,
};
use super::semantic::{CellMeta, CellOwnerKind, CellRole};

// ============================================================================
// Character Classification
// ============================================================================

/// Horizontal line characters across all supported styles.
///
/// The style-specific fields cover custom fallback shafts (for example `=`
/// and `.` in ASCII Thick/Dotted routes); the explicit variants keep this
/// predicate useful for canvases that combine multiple route styles.
pub fn is_horizontal(c: char, style: &StyleChars) -> bool {
    matches!(
        c,
        '-' | '=' | '.' | '─' | '═' | '━' | '╌' | '╴' | '╶' | '╸' | '╺' | '█'
    ) || c == style.edge_h
        || c == style.back_h
        || c == style.dotted_h
}

/// Vertical line characters across all supported styles.
pub fn is_vertical(c: char, style: &StyleChars) -> bool {
    matches!(
        c,
        '|' | ':' | '│' | '║' | '┃' | '╎' | '┆' | '┊' | '┋' | '╏' | '█'
    ) || c == style.edge_v
        || c == style.back_v
        || c == style.dotted_v
}

fn is_any_horizontal(c: char) -> bool {
    is_horizontal(c, &ASCII_CHARS) || is_horizontal(c, &UNICODE_CHARS)
}

fn is_any_vertical(c: char) -> bool {
    is_vertical(c, &ASCII_CHARS) || is_vertical(c, &UNICODE_CHARS)
}

/// Arrow characters (endpoints - never overwritten)
pub fn is_arrow(c: char) -> bool {
    matches!(
        c,
        'v' | '^' | '<' | '>'           // ASCII
        | '↓' | '↑' | '←' | '→'         // Unicode thin arrows
        | '▼' | '▲' | '◀' | '▶' // Unicode filled arrows
    )
}

/// Dedicated portal crossing marker for the active style.
pub fn is_portal_marker(c: char, style: &StyleChars) -> bool {
    c == style.portal_pierce
}

/// Non-directional edge terminal markers for the active style.
pub(crate) fn is_endpoint_marker(c: char, style: &StyleChars) -> bool {
    c == style.circle_end || c == style.cross_end
}

/// Corner characters for the given style
pub fn is_corner(c: char, s: &StyleChars) -> bool {
    c == s.corner_dr || c == s.corner_dl || c == s.corner_ur || c == s.corner_ul
}

/// Junction characters (T-junctions and crosses - preserved once created)
pub fn is_junction(c: char, s: &StyleChars) -> bool {
    c == s.junction_down
        || c == s.junction_up
        || c == s.junction_left
        || c == s.junction_right
        || c == s.cross
}

pub(crate) fn is_explicit_crossing_marker(c: char, s: &StyleChars) -> bool {
    c == s.cross_end
}

/// Box label content (alphanumeric + punctuation)
pub fn is_box_char(c: char, _style: &StyleChars) -> bool {
    c.is_alphanumeric()
        || matches!(
            c,
            '(' | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '_'
                | '.'
                | ','
                | ':'
                | ';'
                | '!'
                | '?'
                | '\''
                | '"'
                | '`'
                | '@'
                | '#'
                | '$'
                | '%'
                | '&'
                | '*'
                | '='
                | '+'
                | '/'
                | '\\'
                | '-'
        )
}

// Corner direction helpers (which way does the corner "open"?)
pub fn is_corner_up(c: char, s: &StyleChars) -> bool {
    c == s.corner_ul || c == s.corner_ur
}
pub fn is_corner_down(c: char, s: &StyleChars) -> bool {
    c == s.corner_dl || c == s.corner_dr
}
pub fn is_corner_left(c: char, s: &StyleChars) -> bool {
    c == s.corner_dl || c == s.corner_ul
}
pub fn is_corner_right(c: char, s: &StyleChars) -> bool {
    c == s.corner_dr || c == s.corner_ur
}

// ============================================================================
// Overlap Resolution
// ============================================================================

/// Resolve what character to draw when two characters overlap.
/// Creates junctions/crosses where appropriate, preserves sacred characters.
///
/// Note: Parallel edges (both horizontal or both vertical) do NOT create
/// crossing indicators. This is intentional - they are visually distinguishable
/// by separation, and crosses would create ambiguity about edge connectivity.
pub fn resolve_overlap(existing: char, new: char, s: &StyleChars) -> char {
    // Empty space - just use new character
    if existing == ' ' || existing == '\0' {
        return new;
    }

    // Arrows are endpoints - never overwrite
    if is_arrow(existing) {
        return existing;
    }

    // Junctions are already merged - preserve them
    if is_junction(existing, s) {
        return existing;
    }

    // Identical characters - no change needed
    if existing == new {
        return existing;
    }

    // Corner + line = junction (existing corner)
    if is_corner(existing, s) {
        if is_vertical(new, s) {
            return if is_corner_left(existing, s) {
                s.junction_right // ├
            } else if is_corner_right(existing, s) {
                s.junction_left // ┤
            } else {
                s.cross
            };
        }
        if is_horizontal(new, s) {
            return if is_corner_up(existing, s) {
                s.junction_up // ┴
            } else if is_corner_down(existing, s) {
                s.junction_down // ┬
            } else {
                s.cross
            };
        }
        // Two corners = junction (edges converging)
        // Use arm-counting to determine the correct junction type based on
        // which directions both corners open toward.
        //
        // Note: is_corner_left/right indicate which SIDE the corner is on, not
        // which direction the arm points. Corners on the left side have their
        // horizontal arm pointing RIGHT, and vice versa.
        if is_corner(new, s) {
            // Count all directional arms from both corners
            let has_up_arm = is_corner_up(existing, s) || is_corner_up(new, s);
            let has_down_arm = is_corner_down(existing, s) || is_corner_down(new, s);
            // Corners on right side (is_corner_right) have arm going LEFT
            let has_left_arm = is_corner_right(existing, s) || is_corner_right(new, s);
            // Corners on left side (is_corner_left) have arm going RIGHT
            let has_right_arm = is_corner_left(existing, s) || is_corner_left(new, s);

            let arm_count = [has_up_arm, has_down_arm, has_left_arm, has_right_arm]
                .iter()
                .filter(|&&b| b)
                .count();

            if arm_count >= 4 {
                return s.cross; // ┼ - all four directions
            }
            if arm_count == 3 {
                // Three-way junction - determine which direction is missing
                if !has_up_arm {
                    return s.junction_down; // ┬ - no up arm
                }
                if !has_down_arm {
                    return s.junction_up; // ┴ - no down arm
                }
                if !has_left_arm {
                    return s.junction_right; // ├ - no left arm
                }
                if !has_right_arm {
                    return s.junction_left; // ┤ - no right arm
                }
            }
            // Two arms - this is actually a corner situation (shouldn't happen
            // for two overlapping corners, but fall through to cross as safety)
            return s.cross;
        }
    }

    // Line + corner = junction (existing line, new corner)
    if is_horizontal(existing, s) && is_corner(new, s) {
        return if is_corner_up(new, s) {
            s.junction_up // ┴
        } else if is_corner_down(new, s) {
            s.junction_down // ┬
        } else {
            s.cross
        };
    }
    if is_vertical(existing, s) && is_corner(new, s) {
        return if is_corner_left(new, s) {
            s.junction_right // ├
        } else if is_corner_right(new, s) {
            s.junction_left // ┤
        } else {
            s.cross
        };
    }

    // Perpendicular lines crossing = cross
    if (is_horizontal(existing, s) && is_vertical(new, s))
        || (is_vertical(existing, s) && is_horizontal(new, s))
    {
        return s.cross;
    }

    // Box content (labels) - preserve
    if is_box_char(existing, s) {
        return existing;
    }

    // Default: new character wins
    new
}

// ============================================================================
// Canvas Structure
// ============================================================================

/// 2D character canvas for rendering diagrams.
///
/// The canvas is a grid of characters that can be drawn to and then
/// converted to a string for display.
#[derive(Clone)]
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    grid: Vec<Vec<char>>,
    combining_grid: Vec<Vec<String>>,
    meta_grid: Vec<Vec<CellMeta>>,
    explicit_crossings_enabled: bool,
    explicit_crossing_cells: Vec<(usize, usize)>,
    write_stage_grid: Vec<Vec<String>>,
    current_write_stage: String,
    fallback_route_plans: Vec<FallbackRoutePlan>,
    fallback_route_rejections: Vec<FallbackRouteRejection>,
}

impl Canvas {
    /// Create a new canvas filled with spaces.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            grid: vec![vec![' '; width]; height],
            combining_grid: vec![vec![String::new(); width]; height],
            meta_grid: vec![vec![CellMeta::default(); width]; height],
            explicit_crossings_enabled: false,
            explicit_crossing_cells: Vec::new(),
            write_stage_grid: vec![vec!["init".to_owned(); width]; height],
            current_write_stage: "init".to_owned(),
            fallback_route_plans: Vec::new(),
            fallback_route_rejections: Vec::new(),
        }
    }

    /// Set a deterministic diagnostic stage for subsequent canvas writes.
    ///
    /// The stage is provenance-only: it never changes the visible character
    /// grid and is consumed by the QA portal trace.
    pub(crate) fn set_write_stage(&mut self, stage: &str) {
        self.current_write_stage.clear();
        self.current_write_stage.push_str(stage);
    }

    /// Enable the narrowly scoped interior-crossing lowering selected by the
    /// topology gate in the render pipeline.  The flag is private to the
    /// renderer so public style/API semantics remain unchanged.
    pub(crate) fn set_explicit_crossings_enabled(&mut self, enabled: bool) {
        self.explicit_crossings_enabled = enabled;
    }

    /// Grow the canvas to the requested width while preserving all existing
    /// cells and their semantic metadata.
    pub fn ensure_width(&mut self, width: usize) {
        if width <= self.width {
            return;
        }
        for row in &mut self.grid {
            row.resize(width, ' ');
        }
        for row in &mut self.combining_grid {
            row.resize(width, String::new());
        }
        for row in &mut self.meta_grid {
            row.resize(width, CellMeta::default());
        }
        for row in &mut self.write_stage_grid {
            row.resize(width, self.current_write_stage.clone());
        }
        self.width = width;
    }

    /// Grow the canvas to the requested height while preserving all existing
    /// cells and their semantic metadata.
    pub fn ensure_height(&mut self, height: usize) {
        if height <= self.height {
            return;
        }
        let old_height = self.height;
        let width = self.width;
        let stage = self.current_write_stage.clone();
        self.grid
            .extend((old_height..height).map(|_| vec![' '; width]));
        self.combining_grid
            .extend((old_height..height).map(|_| vec![String::new(); width]));
        self.meta_grid
            .extend((old_height..height).map(|_| vec![CellMeta::default(); width]));
        self.write_stage_grid
            .extend((old_height..height).map(|_| vec![stage.clone(); width]));
        self.height = height;
    }

    /// Set a character at position (x, y).
    pub fn set(&mut self, x: usize, y: usize, c: char) {
        self.set_inferred(x, y, c);
    }

    /// Get character at position (x, y).
    pub fn get(&self, x: usize, y: usize) -> char {
        if x < self.width && y < self.height {
            self.grid[y][x]
        } else {
            ' '
        }
    }

    /// Set edge character with smart crossing/junction detection.
    ///
    /// Priority (highest first):
    /// 1. Preserve arrows and box content
    /// 2. Preserve existing junctions
    /// 3. Create junctions when lines/corners overlap
    /// 4. New character wins for empty space
    pub fn set_edge_char(&mut self, x: usize, y: usize, new_char: char, s: &StyleChars) {
        if self
            .get_meta(x, y)
            .is_some_and(|meta| meta.role == CellRole::EndpointMarker)
        {
            return;
        }
        let existing = self.get(x, y);
        let final_char = resolve_overlap(existing, new_char, s);
        self.set_inferred(x, y, final_char);
    }

    /// Set a character and infer a generic semantic classification from the glyph.
    pub fn set_inferred(&mut self, x: usize, y: usize, c: char) {
        if x < self.width && y < self.height {
            // A variation selector or combining mark has no terminal-cell
            // width of its own. Keep it attached to the nearest preceding
            // visible cell instead of replacing the base glyph in the grid.
            // The separate combining stream lets the canvas retain the
            // complete grapheme while routing and geometry continue to use
            // one logical cell per base glyph.
            if c != '\0' && display_char_width(c) == 0 {
                let target = if self.grid[y][x] != ' ' && self.grid[y][x] != '\0' {
                    Some(x)
                } else if x > 0 && self.grid[y][x - 1] != ' ' && self.grid[y][x - 1] != '\0' {
                    Some(x - 1)
                } else {
                    None
                };
                if let Some(target) = target {
                    self.combining_grid[y][target].push(c);
                    self.write_stage_grid[y][target] = self.current_write_stage.clone();
                }
                return;
            }
            if matches!(self.grid[y][x], 'x' | '✕')
                && self.meta_grid[y][x].role == CellRole::Junction
            {
                return;
            }
            self.combining_grid[y][x].clear();
            self.grid[y][x] = c;
            self.meta_grid[y][x] = infer_meta(c);
            self.write_stage_grid[y][x] = self.current_write_stage.clone();
        }
    }

    /// Set a character with explicit semantic ownership.
    pub fn set_owned(
        &mut self,
        x: usize,
        y: usize,
        c: char,
        owner_kind: CellOwnerKind,
        owner_id: &str,
        z_index: u8,
    ) {
        self.set_owned_internal(x, y, c, owner_kind, owner_id, None, z_index);
    }

    /// Set a character with explicit semantic ownership and role.
    ///
    /// Route terminal markers use this path so later topology passes can
    /// distinguish them from ordinary shafts and directional arrowheads.
    // The explicit cell-write API intentionally mirrors the ownership fields
    // carried by CellMeta; keep the public renderer primitive stable.
    #[allow(clippy::too_many_arguments)]
    pub fn set_owned_with_role(
        &mut self,
        x: usize,
        y: usize,
        c: char,
        owner_kind: CellOwnerKind,
        owner_id: &str,
        role: CellRole,
        z_index: u8,
    ) {
        self.set_owned_internal(x, y, c, owner_kind, owner_id, Some(role), z_index);
    }

    #[allow(clippy::too_many_arguments)]
    fn set_owned_internal(
        &mut self,
        x: usize,
        y: usize,
        c: char,
        owner_kind: CellOwnerKind,
        owner_id: &str,
        role: Option<CellRole>,
        z_index: u8,
    ) {
        let existing = self.get(x, y);
        let existing_meta = self.get_meta(x, y).cloned();
        if existing_meta.as_ref().is_some_and(|meta| {
            meta.role == CellRole::EndpointMarker && owner_kind != CellOwnerKind::PortalOpening
        }) {
            return;
        }
        if matches!(existing, 'x' | '✕')
            && existing_meta
                .as_ref()
                .is_some_and(|meta| meta.role == CellRole::Junction)
        {
            return;
        }
        let explicit_crossing = self.explicit_crossings_enabled
            && owner_kind == CellOwnerKind::EdgeSegment
            && existing_meta.as_ref().is_some_and(|meta| {
                meta.owner_kind == CellOwnerKind::EdgeSegment
                    && meta.owner_id.as_deref().is_some_and(|existing_owner| {
                        existing_owner != owner_id
                            && ((is_any_horizontal(existing) && is_any_vertical(c))
                                || (is_any_vertical(existing) && is_any_horizontal(c)))
                    })
            });
        if x < self.width && y < self.height {
            self.combining_grid[y][x].clear();
            let final_char = if explicit_crossing {
                self.explicit_crossing_cells.push((x, y));
                if existing.is_ascii() && c.is_ascii() {
                    'x'
                } else {
                    '✕'
                }
            } else {
                c
            };
            self.grid[y][x] = final_char;
            let mut meta = infer_owned_meta(final_char, owner_kind, owner_id, z_index);
            if let Some(role) = role {
                meta.role = role;
            }
            if explicit_crossing {
                meta.role = CellRole::Junction;
            }
            self.meta_grid[y][x] = meta;
            self.write_stage_grid[y][x] = self.current_write_stage.clone();
        }
    }

    /// Set an edge character with overlap resolution and explicit ownership.
    #[allow(clippy::too_many_arguments)]
    pub fn set_edge_char_owned(
        &mut self,
        x: usize,
        y: usize,
        new_char: char,
        s: &StyleChars,
        owner_kind: CellOwnerKind,
        owner_id: &str,
        z_index: u8,
    ) {
        let existing = self.get(x, y);
        let existing_meta = self.get_meta(x, y).cloned();
        if existing_meta.as_ref().is_some_and(|meta| {
            meta.role == CellRole::EndpointMarker && owner_kind != CellOwnerKind::PortalOpening
        }) {
            return;
        }
        if existing_meta.as_ref().is_some_and(|meta| {
            meta.role == CellRole::Junction && is_explicit_crossing_marker(existing, s)
        }) {
            return;
        }
        let explicit_crossing = self.explicit_crossings_enabled
            && owner_kind == CellOwnerKind::EdgeSegment
            && existing_meta.as_ref().is_some_and(|meta| {
                meta.owner_kind == CellOwnerKind::EdgeSegment
                    && meta.owner_id.as_deref().is_some_and(|existing_owner| {
                        existing_owner != owner_id
                            && ((is_horizontal(existing, s) && is_vertical(new_char, s))
                                || (is_vertical(existing, s) && is_horizontal(new_char, s)))
                    })
            });
        let final_char = if explicit_crossing {
            self.explicit_crossing_cells.push((x, y));
            s.cross_end
        } else {
            resolve_overlap(existing, new_char, s)
        };
        if x < self.width && y < self.height {
            self.combining_grid[y][x].clear();
            self.grid[y][x] = final_char;
            let existing_meta = &self.meta_grid[y][x];
            let final_role = infer_role(final_char);
            let should_preserve_existing = final_char == existing
                && !matches!(
                    final_role,
                    CellRole::Horizontal
                        | CellRole::Vertical
                        | CellRole::Corner
                        | CellRole::Junction
                        | CellRole::ArrowTip
                        | CellRole::EndpointMarker
                );

            if should_preserve_existing {
                return;
            }

            let mut meta = infer_owned_meta(final_char, owner_kind, owner_id, z_index);
            if explicit_crossing {
                // Keep the crossing in the edge-owner provenance stream while
                // preventing generic repair passes from treating the marker as
                // a canonical tee/cross that must be rewritten.
                meta.role = CellRole::Junction;
            }
            if meta.z_index >= existing_meta.z_index {
                self.meta_grid[y][x] = meta;
                self.write_stage_grid[y][x] = self.current_write_stage.clone();
            }
        }
    }

    /// Keep an explicit crossing only when both perpendicular paths continue
    /// through the cell in the final routed canvas.  A route ending at an
    /// arrow is a true junction/attachment and must retain the ordinary cross
    /// lowering instead of looking like an unowned pass-through.
    pub(crate) fn finalize_explicit_crossings(&mut self, s: &StyleChars) {
        let candidates = std::mem::take(&mut self.explicit_crossing_cells);
        for (x, y) in candidates {
            if self.get(x, y) != s.cross_end || self.is_interior_crossing(x, y, s) {
                continue;
            }
            let owner = self.get_meta(x, y).and_then(|meta| meta.owner_id.clone());
            // `set_owned` intentionally protects an explicit marker
            // from ordinary later writes.  Finalization is the one
            // semantic downgrade that is allowed to replace it: the
            // endpoint guard has proved that this overlap is not a
            // pass-through and must become a real junction.
            if x < self.width && y < self.height {
                self.combining_grid[y][x].clear();
                self.grid[y][x] = s.cross;
                self.meta_grid[y][x] = infer_owned_meta(
                    s.cross,
                    CellOwnerKind::Junction,
                    owner.as_deref().unwrap_or("crossing"),
                    5,
                );
                self.write_stage_grid[y][x] = self.current_write_stage.clone();
            }
        }
    }

    fn is_interior_crossing(&self, x: usize, y: usize, s: &StyleChars) -> bool {
        let neighbors = [
            self.get(x.saturating_sub(1), y),
            self.get(x.saturating_add(1), y),
            self.get(x, y.saturating_sub(1)),
            self.get(x, y.saturating_add(1)),
        ];
        if neighbors.iter().any(|ch| is_arrow(*ch)) {
            return false;
        }

        let supports_horizontal =
            |ch: char| is_horizontal(ch, s) || is_corner(ch, s) || is_junction(ch, s);
        let supports_vertical =
            |ch: char| is_vertical(ch, s) || is_corner(ch, s) || is_junction(ch, s);

        supports_horizontal(neighbors[0])
            && supports_horizontal(neighbors[1])
            && supports_vertical(neighbors[2])
            && supports_vertical(neighbors[3])
    }

    /// Rebuild metadata for every cell from the current visible glyph grid.
    pub fn refresh_inferred_meta(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.meta_grid[y][x] = infer_meta(self.grid[y][x]);
            }
        }
    }

    /// Update semantic metadata without changing the visible character.
    pub fn set_meta_only(
        &mut self,
        x: usize,
        y: usize,
        owner_kind: CellOwnerKind,
        owner_id: Option<&str>,
        role: CellRole,
        z_index: u8,
    ) {
        if x < self.width && y < self.height {
            let ch = self.grid[y][x];
            if z_index >= self.meta_grid[y][x].z_index {
                self.meta_grid[y][x] = CellMeta {
                    ch,
                    owner_kind,
                    owner_id: owner_id.map(ToOwned::to_owned),
                    role,
                    z_index,
                };
                self.write_stage_grid[y][x] = self.current_write_stage.clone();
            }
        }
    }

    /// Get semantic metadata at position (x, y).
    pub fn get_meta(&self, x: usize, y: usize) -> Option<&CellMeta> {
        if x < self.width && y < self.height {
            Some(&self.meta_grid[y][x])
        } else {
            None
        }
    }

    pub(crate) fn write_stage_at(&self, x: usize, y: usize) -> Option<&str> {
        self.write_stage_grid
            .get(y)
            .and_then(|row| row.get(x))
            .map(String::as_str)
    }

    /// Return the non-space cells changed by a route-only simulation relative
    /// to the same canvas before edge lowering.  The BT scene planner uses
    /// this to turn the renderer's own overlap resolution into an explicit,
    /// hashable reservation without replaying unowned heuristics in the main
    /// canvas.
    pub(crate) fn non_space_delta(&self, baseline: &Canvas) -> Vec<FallbackPaint> {
        if self.width != baseline.width || self.height != baseline.height {
            return Vec::new();
        }
        let mut paints = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                let glyph = self.grid[y][x];
                if glyph != ' ' && glyph != baseline.grid[y][x] {
                    paints.push(FallbackPaint {
                        point: super::fallback_route::FallbackPoint { x, y },
                        glyph,
                    });
                }
            }
        }
        paints
    }

    /// Register a validated fallback route before lowering it onto the grid.
    pub(crate) fn record_fallback_route_plan(&mut self, plan: FallbackRoutePlan) {
        self.fallback_route_plans.push(plan);
    }

    pub(crate) fn record_fallback_route_rejection(
        &mut self,
        owner_id: impl Into<String>,
        strategy: impl Into<String>,
        reason: impl Into<String>,
    ) {
        self.fallback_route_rejections.push(FallbackRouteRejection {
            owner_id: owner_id.into(),
            strategy: strategy.into(),
            reason: reason.into(),
        });
    }

    /// Return final-canvas observations for every fallback route planned during
    /// edge routing.  The observation intentionally happens late so border and
    /// title cleanup cannot hide a broken route from the audit packet.
    pub(crate) fn fallback_route_traces(&self) -> Vec<FallbackRouteTrace> {
        self.fallback_route_plans
            .iter()
            .map(|plan| plan.trace_on(self))
            .collect()
    }

    pub(crate) fn fallback_route_rejections(&self) -> Vec<FallbackRouteRejection> {
        self.fallback_route_rejections.clone()
    }

    /// Return whether a later repair/restore stage is about to touch a cell
    /// promised by a fallback route plan.
    pub(crate) fn fallback_route_claims_cell(&self, x: usize, y: usize) -> bool {
        self.fallback_route_plans
            .iter()
            .any(|plan| plan.claims_cell(x, y))
    }

    /// Return whether a later route writer would trespass on a previously
    /// committed fallback scene. Scene-owned edges and the scene owner itself
    /// remain allowed; unrelated routes must leave the planned cell intact so
    /// final provenance cannot silently turn a scene corner into a straight
    /// segment.
    pub(crate) fn fallback_route_cell_owned_by_other(
        &self,
        x: usize,
        y: usize,
        owner_id: Option<&str>,
    ) -> bool {
        self.fallback_route_plans.iter().any(|plan| {
            plan.claims_cell(x, y)
                && !owner_id.is_some_and(|owner| {
                    owner == plan.owner_id
                        || plan.covered_edge_ids.iter().any(|edge_id| edge_id == owner)
                })
        })
    }

    pub(crate) fn fallback_route_covers_edge(&self, owner_id: &str) -> bool {
        self.fallback_route_plans.iter().any(|plan| {
            plan.covered_edge_ids
                .iter()
                .any(|covered_id| covered_id == owner_id)
        })
    }

    pub(crate) fn fallback_route_has_scene_boundary(&self, boundary_id: &str) -> bool {
        self.fallback_route_plans.iter().any(|plan| {
            !plan.covered_edge_ids.is_empty()
                && plan
                    .boundary_claims
                    .iter()
                    .any(|claim| claim.boundary_id == boundary_id)
        })
    }

    pub(crate) fn fallback_route_claims_boundary(
        &self,
        boundary_id: &str,
        side: &str,
        x: usize,
        y: usize,
    ) -> bool {
        self.fallback_route_plans
            .iter()
            .any(|plan| plan.claims_boundary(boundary_id, side, x, y))
    }

    /// Promote final fallback boundary pierces to portal ownership without
    /// changing their already-lowered glyph.  This keeps the critic from
    /// interpreting a deliberate clean wall crossing as a junction.
    pub(crate) fn finalize_fallback_route_claims(&mut self) {
        let claims = self
            .fallback_route_plans
            .iter()
            .flat_map(|plan| plan.boundary_claims.iter())
            .map(|claim| {
                (
                    claim.x,
                    claim.y,
                    claim.boundary_id.clone(),
                    claim.expected_glyph,
                )
            })
            .collect::<Vec<_>>();
        for (x, y, owner_id, expected_glyph) in claims {
            if self.get(x, y) == expected_glyph {
                self.set_meta_only(
                    x,
                    y,
                    CellOwnerKind::PortalOpening,
                    Some(&owner_id),
                    CellRole::Portal,
                    6,
                );
            }
        }
    }

    /// Capture explicit edge-related metadata that should survive a metadata refresh.
    pub fn explicit_edge_meta(&self) -> Vec<(usize, usize, CellMeta)> {
        let mut preserved = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                let meta = &self.meta_grid[y][x];
                let owned_route = matches!(
                    meta.owner_kind,
                    CellOwnerKind::EdgeSegment
                        | CellOwnerKind::ArrowHead
                        | CellOwnerKind::CycleEdge
                );
                let route_role = matches!(
                    meta.role,
                    CellRole::Horizontal
                        | CellRole::Vertical
                        | CellRole::Corner
                        | CellRole::Junction
                        | CellRole::ArrowTip
                        | CellRole::EndpointMarker
                        | CellRole::Portal
                );
                if meta.owner_id.is_some() && meta.z_index > 0 && (owned_route || route_role) {
                    preserved.push((x, y, meta.clone()));
                }
            }
        }
        preserved
    }

    /// Check if a node is within visible canvas bounds.
    pub fn is_visible(&self, node: &Node) -> bool {
        node.x + node.width <= self.width && node.y + node.height <= self.height
    }

    /// Convert the canvas to a string, cropping empty margins and optionally padding.
    ///
    /// Cropping trims any fully-empty rows/columns (spaces only) around the content.
    /// Padding adds blank rows and left/right spaces around every line.
    pub fn to_string_cropped(&self, pad: usize) -> String {
        if self.width == 0 || self.height == 0 {
            return String::new();
        }

        let mut found = false;
        let mut min_x = self.width;
        let mut max_x = 0usize;
        let mut min_y = self.height;
        let mut max_y = 0usize;

        for (y, row) in self.grid.iter().enumerate() {
            for (x, c) in row.iter().enumerate() {
                if *c != ' ' && *c != '\0' {
                    found = true;
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                }
            }
        }

        if !found {
            return String::new();
        }

        let mut lines: Vec<String> = Vec::with_capacity(max_y.saturating_sub(min_y) + 1);
        for y in min_y..=max_y {
            let slice = &self.grid[y][min_x..=max_x];
            let mut line = String::new();
            for (offset, c) in slice.iter().enumerate() {
                if *c == '\0' {
                    continue;
                }
                line.push(*c);
                line.push_str(&self.combining_grid[y][min_x + offset]);
            }
            let line = line.trim_end().to_string();
            lines.push(line);
        }

        pad_lines(&lines, pad)
    }
}

fn infer_meta(c: char) -> CellMeta {
    let role = infer_role(c);

    let owner_kind = match role {
        CellRole::Empty => CellOwnerKind::Empty,
        CellRole::ArrowTip => CellOwnerKind::ArrowHead,
        CellRole::Junction => CellOwnerKind::Junction,
        CellRole::Horizontal | CellRole::Vertical | CellRole::Corner | CellRole::EndpointMarker => {
            CellOwnerKind::EdgeSegment
        }
        CellRole::Text
        | CellRole::Unknown
        | CellRole::Fill
        | CellRole::Border
        | CellRole::Portal => CellOwnerKind::Unknown,
    };

    CellMeta {
        ch: c,
        owner_kind,
        owner_id: None,
        role,
        z_index: 0,
    }
}

fn infer_role(c: char) -> CellRole {
    if c == ' ' || c == '\0' {
        CellRole::Empty
    } else if is_arrow(c) {
        CellRole::ArrowTip
    } else if matches!(
        c,
        '┌' | '┐' | '└' | '┘' | '╔' | '╗' | '╚' | '╝' | '╭' | '╮' | '╰' | '╯'
    ) {
        CellRole::Corner
    } else if matches!(c, '-' | '─' | '═' | '━' | '█') {
        CellRole::Horizontal
    } else if matches!(c, '|' | ':' | '│' | '║' | '┃') {
        CellRole::Vertical
    } else if matches!(
        c,
        '+' | '┼'
            | '╬'
            | '╋'
            | '├'
            | '┤'
            | '┬'
            | '┴'
            | '╠'
            | '╣'
            | '╦'
            | '╩'
            | '┣'
            | '┫'
            | '┳'
            | '┻'
    ) {
        CellRole::Junction
    } else {
        CellRole::Text
    }
}

fn infer_owned_meta(c: char, owner_kind: CellOwnerKind, owner_id: &str, z_index: u8) -> CellMeta {
    let role = if owner_kind == CellOwnerKind::PortalOpening {
        CellRole::Portal
    } else {
        infer_role(c)
    };
    let final_owner_kind = match (owner_kind, role) {
        (CellOwnerKind::CycleEdge, CellRole::ArrowTip) => CellOwnerKind::CycleEdge,
        (_, CellRole::ArrowTip) => CellOwnerKind::ArrowHead,
        _ => owner_kind,
    };

    CellMeta {
        ch: c,
        owner_kind: final_owner_kind,
        owner_id: Some(owner_id.to_string()),
        role,
        z_index,
    }
}

impl std::fmt::Display for Canvas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let output = self
            .grid
            .iter()
            .enumerate()
            .map(|(y, row)| {
                let mut line = String::new();
                for (x, c) in row.iter().enumerate() {
                    if *c == '\0' {
                        continue;
                    }
                    line.push(*c);
                    line.push_str(&self.combining_grid[y][x]);
                }
                line.trim_end().to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        write!(f, "{output}")
    }
}

fn pad_lines(lines: &[String], pad: usize) -> String {
    if pad == 0 {
        return lines.join("\n");
    }

    let prefix = " ".repeat(pad);
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + pad * 2);

    for _ in 0..pad {
        out.push(String::new());
    }
    for line in lines {
        if line.is_empty() {
            out.push(String::new());
        } else {
            out.push(format!("{prefix}{line}"));
        }
    }
    for _ in 0..pad {
        out.push(String::new());
    }

    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{BaseStyle, CompositeStyle};

    fn unicode_chars() -> StyleChars {
        CompositeStyle::default().to_style_chars(BaseStyle::Unicode)
    }

    fn ascii_chars() -> StyleChars {
        CompositeStyle::default().to_style_chars(BaseStyle::Ascii)
    }

    // ==========================================================================
    // Character Classification Tests
    // ==========================================================================

    #[test]
    fn test_is_horizontal_unicode() {
        let s = unicode_chars();
        assert!(is_horizontal('─', &s));
        assert!(is_horizontal('═', &s));
        assert!(is_horizontal('━', &s));
        assert!(is_horizontal('╌', &s));
        assert!(!is_horizontal('│', &s));
        assert!(!is_horizontal('a', &s));
    }

    #[test]
    fn test_is_vertical_unicode() {
        let s = unicode_chars();
        assert!(is_vertical('│', &s));
        assert!(is_vertical('║', &s));
        assert!(is_vertical('┃', &s));
        assert!(is_vertical('╎', &s));
        assert!(!is_vertical('─', &s));
        assert!(!is_vertical('a', &s));
    }

    #[test]
    fn test_is_styled_ascii_route_shaft() {
        let mut s = ascii_chars();
        s.edge_h = '=';
        s.dotted_h = '.';
        assert!(is_horizontal('=', &s));
        assert!(is_horizontal('.', &s));
        assert!(is_vertical(':', &s));
    }

    #[test]
    fn explicit_edge_meta_preserves_custom_fallback_markers() {
        let mut canvas = Canvas::new(3, 1);
        canvas.set_owned(0, 0, '=', CellOwnerKind::EdgeSegment, "edge:0:A->B", 5);
        canvas.set_owned(1, 0, 'o', CellOwnerKind::EdgeSegment, "edge:1:A->C", 5);
        canvas.set_owned(2, 0, 'x', CellOwnerKind::EdgeSegment, "edge:2:A->D", 5);

        let preserved = canvas.explicit_edge_meta();
        assert_eq!(preserved.len(), 3);
        assert!(preserved.iter().all(|(_, _, meta)| meta.owner_id.is_some()));
    }

    #[test]
    fn combining_marks_do_not_replace_their_base_cell() {
        let mut canvas = Canvas::new(3, 1);
        canvas.set(0, 0, '⚙');
        canvas.set(1, 0, '\u{fe0f}');

        assert_eq!(canvas.get(0, 0), '⚙');
        assert_eq!(canvas.to_string_cropped(0), "⚙️");
    }

    #[test]
    fn endpoint_marker_role_is_protected_from_generic_edge_writes() {
        let chars = unicode_chars();
        let mut canvas = Canvas::new(3, 3);
        canvas.set_owned_with_role(
            1,
            1,
            chars.circle_end,
            CellOwnerKind::EdgeSegment,
            "edge:4:A->F",
            CellRole::EndpointMarker,
            5,
        );

        canvas.set_edge_char(1, 1, chars.edge_v, &chars);
        canvas.set_edge_char_owned(
            1,
            1,
            chars.edge_v,
            &chars,
            CellOwnerKind::EdgeSegment,
            "edge:4:A->F",
            5,
        );
        canvas.set_owned(
            1,
            1,
            chars.edge_v,
            CellOwnerKind::EdgeSegment,
            "edge:4:A->F",
            5,
        );
        let marker_meta = canvas.get_meta(1, 1).expect("endpoint marker metadata");
        assert_eq!(canvas.get(1, 1), chars.circle_end);
        assert_eq!(marker_meta.role, CellRole::EndpointMarker);
        assert_eq!(marker_meta.owner_id.as_deref(), Some("edge:4:A->F"));
        canvas.set_owned(
            1,
            1,
            chars.edge_h,
            CellOwnerKind::PortalOpening,
            "subgraph:portal",
            6,
        );

        let meta = canvas.get_meta(1, 1).expect("endpoint marker metadata");
        assert_eq!(canvas.get(1, 1), chars.edge_h);
        assert_eq!(meta.owner_kind, CellOwnerKind::PortalOpening);
        assert_eq!(meta.owner_id.as_deref(), Some("subgraph:portal"));
    }

    #[test]
    fn test_is_arrow() {
        assert!(is_arrow('v'));
        assert!(is_arrow('^'));
        assert!(is_arrow('<'));
        assert!(is_arrow('>'));
        assert!(is_arrow('▼'));
        assert!(is_arrow('↓'));
        assert!(!is_arrow('─'));
        assert!(!is_arrow('a'));
    }

    #[test]
    fn test_is_corner() {
        let s = unicode_chars();
        assert!(is_corner('┌', &s)); // corner_dl
        assert!(is_corner('┐', &s)); // corner_dr
        assert!(is_corner('└', &s)); // corner_ul
        assert!(is_corner('┘', &s)); // corner_ur
        assert!(!is_corner('─', &s));
        assert!(!is_corner('│', &s));
    }

    #[test]
    fn test_is_junction() {
        let s = unicode_chars();
        assert!(is_junction('┬', &s)); // junction_down
        assert!(is_junction('┴', &s)); // junction_up
        assert!(is_junction('┼', &s)); // cross
        assert!(!is_junction('─', &s));
        assert!(!is_junction('└', &s));
    }

    // ==========================================================================
    // Overlap Resolution Tests
    // ==========================================================================

    #[test]
    fn test_overlap_empty_space_takes_new() {
        let s = unicode_chars();
        assert_eq!(resolve_overlap(' ', '│', &s), '│');
        assert_eq!(resolve_overlap(' ', '─', &s), '─');
        assert_eq!(resolve_overlap('\0', '┌', &s), '┌');
    }

    #[test]
    fn test_overlap_arrows_never_overwritten() {
        let s = unicode_chars();
        assert_eq!(resolve_overlap('↓', '│', &s), '↓');
        assert_eq!(resolve_overlap('▼', '─', &s), '▼');
        assert_eq!(resolve_overlap('v', '|', &s), 'v');
    }

    #[test]
    fn test_overlap_junctions_preserved() {
        let s = unicode_chars();
        assert_eq!(resolve_overlap('┬', '│', &s), '┬');
        assert_eq!(resolve_overlap('┴', '─', &s), '┴');
        assert_eq!(resolve_overlap('┼', '│', &s), '┼');
    }

    #[test]
    fn test_overlap_corner_plus_vertical_creates_junction() {
        let s = unicode_chars();
        // Left-opening corner + vertical = right-pointing junction (├)
        assert_eq!(resolve_overlap('└', '│', &s), '├');
        assert_eq!(resolve_overlap('┌', '│', &s), '├');
        // Right-opening corner + vertical = left-pointing junction (┤)
        assert_eq!(resolve_overlap('┘', '│', &s), '┤');
        assert_eq!(resolve_overlap('┐', '│', &s), '┤');
    }

    #[test]
    fn test_overlap_corner_plus_horizontal_creates_junction() {
        let s = unicode_chars();
        // Up-opening corner + horizontal = up-pointing junction (┴)
        assert_eq!(resolve_overlap('└', '─', &s), '┴');
        assert_eq!(resolve_overlap('┘', '─', &s), '┴');
        // Down-opening corner + horizontal = down-pointing junction (┬)
        assert_eq!(resolve_overlap('┌', '─', &s), '┬');
        assert_eq!(resolve_overlap('┐', '─', &s), '┬');
    }

    #[test]
    fn test_overlap_perpendicular_lines_create_cross() {
        let s = unicode_chars();
        assert_eq!(resolve_overlap('│', '─', &s), '┼');
        assert_eq!(resolve_overlap('─', '│', &s), '┼');
    }

    #[test]
    fn test_overlap_box_content_preserved() {
        let s = unicode_chars();
        assert_eq!(resolve_overlap('A', '│', &s), 'A');
        assert_eq!(resolve_overlap('1', '─', &s), '1');
        assert_eq!(resolve_overlap('_', '┌', &s), '_');
    }

    #[test]
    fn test_overlap_two_corners_creates_junction() {
        let s = unicode_chars();
        // Two up-opening corners (└ and ┘) combine to junction_up (┴)
        // └ = up+right, ┘ = up+left → combined: up+right+left = ┴
        assert_eq!(resolve_overlap('└', '┘', &s), '┴');
        assert_eq!(resolve_overlap('┘', '└', &s), '┴');

        // Two down-opening corners (┌ and ┐) combine to junction_down (┬)
        // ┌ = down+right, ┐ = down+left → combined: down+right+left = ┬
        assert_eq!(resolve_overlap('┌', '┐', &s), '┬');
        assert_eq!(resolve_overlap('┐', '┌', &s), '┬');

        // Opposite corners (└ and ┐) combine to cross or specific junction
        // └ = up+right, ┐ = down+left → combined: all 4 = cross
        assert_eq!(resolve_overlap('└', '┐', &s), '┼');
        assert_eq!(resolve_overlap('┐', '└', &s), '┼');

        // Same-side corners combine to appropriate junction
        // └ = up+right, ┌ = down+right → combined: up+down+right = ├
        assert_eq!(resolve_overlap('└', '┌', &s), '├');
        assert_eq!(resolve_overlap('┌', '└', &s), '├');

        // ┘ = up+left, ┐ = down+left → combined: up+down+left = ┤
        assert_eq!(resolve_overlap('┘', '┐', &s), '┤');
        assert_eq!(resolve_overlap('┐', '┘', &s), '┤');
    }

    // ==========================================================================
    // Canvas Operations Tests
    // ==========================================================================

    #[test]
    fn test_canvas_new_filled_with_spaces() {
        let canvas = Canvas::new(10, 5);
        assert_eq!(canvas.width, 10);
        assert_eq!(canvas.height, 5);
        assert_eq!(canvas.get(0, 0), ' ');
        assert_eq!(canvas.get(9, 4), ' ');
    }

    #[test]
    fn test_canvas_set_get() {
        let mut canvas = Canvas::new(10, 5);
        canvas.set(3, 2, 'X');
        assert_eq!(canvas.get(3, 2), 'X');
        assert_eq!(canvas.get(0, 0), ' ');
    }

    #[test]
    fn test_canvas_out_of_bounds_returns_space() {
        let canvas = Canvas::new(10, 5);
        assert_eq!(canvas.get(100, 100), ' ');
    }

    #[test]
    fn test_canvas_set_out_of_bounds_ignored() {
        let mut canvas = Canvas::new(10, 5);
        canvas.set(100, 100, 'X'); // Should not panic
        assert_eq!(canvas.get(100, 100), ' ');
    }

    #[test]
    fn test_canvas_set_edge_char_with_overlap_resolution() {
        let mut canvas = Canvas::new(10, 5);
        let s = unicode_chars();

        // First edge: vertical line
        canvas.set_edge_char(5, 2, '│', &s);
        assert_eq!(canvas.get(5, 2), '│');

        // Second edge: horizontal line crossing -> creates cross
        canvas.set_edge_char(5, 2, '─', &s);
        assert_eq!(canvas.get(5, 2), '┼');
    }

    #[test]
    fn test_canvas_is_visible() {
        let canvas = Canvas::new(80, 40);

        let visible_node = Node {
            id: "A".into(),
            label: "Test".into(),
            label_lines: Vec::new(),
            shape: crate::graph::NodeShape::Rectangle,
            click_target: None,
            x: 0,
            y: 0,
            width: 10,
            height: crate::style::BOX_HEIGHT,
            rank: 0,
        };
        assert!(canvas.is_visible(&visible_node));

        let clipped_node = Node {
            id: "B".into(),
            label: "Clipped".into(),
            label_lines: Vec::new(),
            shape: crate::graph::NodeShape::Rectangle,
            click_target: None,
            x: 75,
            y: 0,
            width: 10, // x + width = 85 > 80
            height: crate::style::BOX_HEIGHT,
            rank: 0,
        };
        assert!(!canvas.is_visible(&clipped_node));
    }

    #[test]
    fn test_canvas_display_trims_trailing_spaces() {
        let mut canvas = Canvas::new(10, 3);
        canvas.set(0, 0, 'A');
        canvas.set(2, 1, 'B');

        let output = format!("{canvas}");
        let lines: Vec<&str> = output.lines().collect();

        assert_eq!(lines[0], "A"); // Trimmed from "A         "
        assert_eq!(lines[1], "  B"); // Trimmed from "  B       "
    }

    // ==========================================================================
    // ASCII Style Tests
    // ==========================================================================

    #[test]
    fn test_ascii_overlap_resolution() {
        let s = ascii_chars();

        // ASCII uses different characters
        assert_eq!(resolve_overlap(' ', '|', &s), '|');
        assert_eq!(resolve_overlap(' ', '-', &s), '-');

        // Perpendicular creates cross
        assert_eq!(resolve_overlap('|', '-', &s), '+');
        assert_eq!(resolve_overlap('-', '|', &s), '+');
    }
}
