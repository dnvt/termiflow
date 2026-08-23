//! Private plans and final traces for renderer-owned fallback routes.
//!
//! A fallback route is still a first-class render decision.  The plan is
//! captured before it mutates the canvas; the trace is derived from the final
//! canvas after later border, title, stabilization, and portal stages have
//! run.  This keeps visual regressions falsifiable instead of treating an
//! untraced fallback as a successful render by default.

use serde::Serialize;
use std::collections::{BTreeSet, VecDeque};

use super::canvas::Canvas;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct FallbackPoint {
    pub x: usize,
    pub y: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) enum FallbackAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FallbackSegment {
    pub from: FallbackPoint,
    pub to: FallbackPoint,
    pub axis: FallbackAxis,
    pub glyph: char,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FallbackCorner {
    pub point: FallbackPoint,
    pub glyph: char,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FallbackPaint {
    pub point: FallbackPoint,
    pub glyph: char,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FallbackBoundaryClaim {
    pub boundary_id: String,
    pub side: String,
    pub x: usize,
    pub y: usize,
    pub expected_glyph: char,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FallbackAttachment {
    pub role: String,
    pub point: FallbackPoint,
    pub boundary_id: Option<String>,
    pub side: Option<String>,
}

/// One scene-owned target-entry decision shared by route lowering, portal
/// projection, fallback claims, and the final semantic trace.
///
/// This is deliberately narrower than a complete route plan: it records the
/// edge/boundary association and the exact physical entry/arrow coordinates
/// that must not be reconstructed from a node center later in the pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PortalEntryDecision {
    pub edge_id: String,
    pub owner_id: String,
    pub target_node_id: String,
    pub boundary_id: String,
    pub side: String,
    pub portal_x: usize,
    pub portal_y: usize,
    pub arrow_x: usize,
    pub arrow_y: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FallbackRoutePlan {
    pub owner_id: String,
    pub strategy: String,
    pub segments: Vec<FallbackSegment>,
    pub corners: Vec<FallbackCorner>,
    pub paints: Vec<FallbackPaint>,
    pub boundary_claims: Vec<FallbackBoundaryClaim>,
    pub source_attachment: Option<FallbackAttachment>,
    pub target_attachment: Option<FallbackAttachment>,
    pub arrow_attachment: Option<FallbackPoint>,
    pub covered_edge_ids: Vec<String>,
    pub shared_cells: Vec<FallbackPoint>,
    pub entry_decisions: Vec<PortalEntryDecision>,
    pub contract_digest: Option<String>,
}

impl FallbackRoutePlan {
    pub(crate) fn new(owner_id: impl Into<String>, strategy: impl Into<String>) -> Self {
        Self {
            owner_id: owner_id.into(),
            strategy: strategy.into(),
            segments: Vec::new(),
            corners: Vec::new(),
            paints: Vec::new(),
            boundary_claims: Vec::new(),
            source_attachment: None,
            target_attachment: None,
            arrow_attachment: None,
            covered_edge_ids: Vec::new(),
            shared_cells: Vec::new(),
            entry_decisions: Vec::new(),
            contract_digest: None,
        }
    }

    pub(crate) fn set_contract_digest(&mut self, digest: Option<String>) {
        self.contract_digest = digest;
    }

    pub(crate) fn set_scene_coverage(&mut self, edge_ids: impl IntoIterator<Item = String>) {
        self.covered_edge_ids = edge_ids.into_iter().collect();
    }

    /// Record a physical route cell that may legitimately be owned by a
    /// sibling edge, such as the shared source stem before a fan-in branch.
    /// This is evidence-only metadata; it never makes the cell available to a
    /// later route writer.
    pub(crate) fn allow_shared_cell(&mut self, x: usize, y: usize) {
        if !self
            .shared_cells
            .iter()
            .any(|point| point.x == x && point.y == y)
        {
            self.shared_cells.push(FallbackPoint { x, y });
        }
    }

    pub(crate) fn set_source_attachment(
        &mut self,
        boundary_id: impl Into<String>,
        side: impl Into<String>,
        x: usize,
        y: usize,
    ) {
        self.source_attachment = Some(FallbackAttachment {
            role: "source".to_owned(),
            point: FallbackPoint { x, y },
            boundary_id: Some(boundary_id.into()),
            side: Some(side.into()),
        });
    }

    pub(crate) fn set_target_attachment(
        &mut self,
        boundary_id: impl Into<String>,
        side: impl Into<String>,
        x: usize,
        y: usize,
    ) {
        self.target_attachment = Some(FallbackAttachment {
            role: "target".to_owned(),
            point: FallbackPoint { x, y },
            boundary_id: Some(boundary_id.into()),
            side: Some(side.into()),
        });
    }

    pub(crate) fn set_arrow_attachment(&mut self, x: usize, y: usize) {
        self.arrow_attachment = Some(FallbackPoint { x, y });
    }

    pub(crate) fn set_target_entry_decision(&mut self, decision: PortalEntryDecision) {
        self.entry_decisions.push(decision);
    }

    pub(crate) fn push_vertical(&mut self, x: usize, y1: usize, y2: usize, glyph: char) {
        if y1 == y2 {
            return;
        }
        self.segments.push(FallbackSegment {
            from: FallbackPoint { x, y: y1 },
            to: FallbackPoint { x, y: y2 },
            axis: FallbackAxis::Vertical,
            glyph,
        });
    }

    pub(crate) fn push_horizontal(&mut self, y: usize, x1: usize, x2: usize, glyph: char) {
        if x1 == x2 {
            return;
        }
        self.segments.push(FallbackSegment {
            from: FallbackPoint { x: x1, y },
            to: FallbackPoint { x: x2, y },
            axis: FallbackAxis::Horizontal,
            glyph,
        });
    }

    pub(crate) fn push_corner(&mut self, x: usize, y: usize, glyph: char) {
        self.corners.push(FallbackCorner {
            point: FallbackPoint { x, y },
            glyph,
        });
    }

    pub(crate) fn push_paint(&mut self, x: usize, y: usize, glyph: char) {
        self.paints.push(FallbackPaint {
            point: FallbackPoint { x, y },
            glyph,
        });
    }

    pub(crate) fn claim_boundary(
        &mut self,
        boundary_id: impl Into<String>,
        side: impl Into<String>,
        x: usize,
        y: usize,
        expected_glyph: char,
    ) {
        self.boundary_claims.push(FallbackBoundaryClaim {
            boundary_id: boundary_id.into(),
            side: side.into(),
            x,
            y,
            expected_glyph,
        });
    }

    pub(crate) fn validation_error(&self, width: usize, height: usize) -> Option<String> {
        let point_is_valid = |point: FallbackPoint| point.x < width && point.y < height;
        if !self.segments.iter().all(|segment| {
            point_is_valid(segment.from)
                && point_is_valid(segment.to)
                && match segment.axis {
                    FallbackAxis::Horizontal => segment.from.y == segment.to.y,
                    FallbackAxis::Vertical => segment.from.x == segment.to.x,
                }
        }) {
            return Some("segment is out of bounds or not axis-aligned".to_owned());
        }
        if !self
            .corners
            .iter()
            .all(|corner| point_is_valid(corner.point))
        {
            return Some("corner is out of bounds".to_owned());
        }
        if !self
            .boundary_claims
            .iter()
            .all(|claim| claim.x < width && claim.y < height)
        {
            return Some("boundary claim is out of bounds".to_owned());
        }
        if !self.paints.iter().all(|paint| point_is_valid(paint.point)) {
            return Some("paint is out of bounds".to_owned());
        }
        if !self.shared_cells.iter().all(|point| point_is_valid(*point)) {
            return Some("shared route cell is out of bounds".to_owned());
        }
        let mut decision_edges = BTreeSet::new();
        for decision in &self.entry_decisions {
            if decision.owner_id != self.owner_id {
                return Some("target-entry decision has a different route owner".to_owned());
            }
            if !decision_edges.insert(decision.edge_id.as_str()) {
                return Some("route contract contains duplicate target-entry decisions".to_owned());
            }
            if !self
                .covered_edge_ids
                .iter()
                .any(|edge_id| edge_id == &decision.edge_id)
            {
                return Some(
                    "target-entry decision is not covered by the route contract".to_owned(),
                );
            }
            if !self.claims_boundary(
                &decision.boundary_id,
                &decision.side,
                decision.portal_x,
                decision.portal_y,
            ) {
                return Some("target-entry decision has no matching boundary claim".to_owned());
            }
            if !self
                .paints
                .iter()
                .any(|paint| paint.point.x == decision.arrow_x && paint.point.y == decision.arrow_y)
            {
                return Some("target-entry decision has no matching arrow paint".to_owned());
            }
            if !point_is_valid(FallbackPoint {
                x: decision.portal_x,
                y: decision.portal_y,
            }) || !point_is_valid(FallbackPoint {
                x: decision.arrow_x,
                y: decision.arrow_y,
            }) {
                return Some("target-entry decision is out of bounds".to_owned());
            }
        }
        if !self.covered_edge_ids.is_empty() {
            if self.paints.is_empty() {
                return Some("scene route contract has no planned paints".to_owned());
            }
            let unique_ids: BTreeSet<&str> =
                self.covered_edge_ids.iter().map(String::as_str).collect();
            if unique_ids.len() != self.covered_edge_ids.len() {
                return Some("scene route contract contains duplicate edge ids".to_owned());
            }
            return None;
        }

        let has_contract = self.source_attachment.is_some()
            || self.target_attachment.is_some()
            || self.arrow_attachment.is_some();
        if !has_contract {
            return None;
        }
        let (Some(source), Some(target)) = (&self.source_attachment, &self.target_attachment)
        else {
            return Some(
                "connected route contract requires source and target attachments".to_owned(),
            );
        };
        let cells = self.planned_cells();
        if !cells.contains(&(source.point.x, source.point.y)) {
            return Some("source attachment is not on the planned route".to_owned());
        }
        if !cells.contains(&(target.point.x, target.point.y)) {
            return Some("target attachment is not on the planned route".to_owned());
        }
        let Some(arrow) = self.arrow_attachment else {
            return Some("connected route contract requires arrow attachment".to_owned());
        };
        if !cells.contains(&(arrow.x, arrow.y)) {
            return Some("arrow attachment is not on the planned route".to_owned());
        }

        for attachment in [source, target] {
            let (Some(boundary_id), Some(side)) = (
                attachment.boundary_id.as_deref(),
                attachment.side.as_deref(),
            ) else {
                return Some(format!(
                    "{} attachment is missing boundary role",
                    attachment.role
                ));
            };
            if !self.boundary_claims.iter().any(|claim| {
                claim.boundary_id == boundary_id
                    && claim.side == side
                    && claim.x == attachment.point.x
                    && claim.y == attachment.point.y
            }) {
                return Some(format!(
                    "{} attachment has no matching physical boundary claim",
                    attachment.role
                ));
            }
        }

        let Some(start) = cells.iter().next().copied() else {
            return Some("connected route contract has no planned cells".to_owned());
        };
        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::from([start]);
        while let Some((x, y)) = queue.pop_front() {
            if !visited.insert((x, y)) {
                continue;
            }
            for neighbor in [
                (x.saturating_sub(1), y),
                (x.saturating_add(1), y),
                (x, y.saturating_sub(1)),
                (x, y.saturating_add(1)),
            ] {
                if cells.contains(&neighbor) && !visited.contains(&neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }
        if visited.len() != cells.len() {
            return Some("connected route contract contains a disconnected primitive".to_owned());
        }
        None
    }

    pub(crate) fn planned_cells(&self) -> BTreeSet<(usize, usize)> {
        let mut cells = BTreeSet::new();
        for segment in &self.segments {
            match segment.axis {
                FallbackAxis::Horizontal => {
                    let (start, end) = if segment.from.x <= segment.to.x {
                        (segment.from.x, segment.to.x)
                    } else {
                        (segment.to.x, segment.from.x)
                    };
                    for x in start..=end {
                        cells.insert((x, segment.from.y));
                    }
                }
                FallbackAxis::Vertical => {
                    let (start, end) = if segment.from.y <= segment.to.y {
                        (segment.from.y, segment.to.y)
                    } else {
                        (segment.to.y, segment.from.y)
                    };
                    for y in start..=end {
                        cells.insert((segment.from.x, y));
                    }
                }
            }
        }
        for corner in &self.corners {
            cells.insert((corner.point.x, corner.point.y));
        }
        for paint in &self.paints {
            cells.insert((paint.point.x, paint.point.y));
        }
        for claim in &self.boundary_claims {
            cells.insert((claim.x, claim.y));
        }
        if let Some(source) = &self.source_attachment {
            cells.insert((source.point.x, source.point.y));
        }
        if let Some(target) = &self.target_attachment {
            cells.insert((target.point.x, target.point.y));
        }
        if let Some(arrow) = self.arrow_attachment {
            cells.insert((arrow.x, arrow.y));
        }
        cells
    }

    pub(crate) fn claims_cell(&self, x: usize, y: usize) -> bool {
        self.corners
            .iter()
            .any(|corner| corner.point.x == x && corner.point.y == y)
            || self
                .paints
                .iter()
                .any(|paint| paint.point.x == x && paint.point.y == y)
            || self
                .boundary_claims
                .iter()
                .any(|claim| claim.x == x && claim.y == y)
            || self.segments.iter().any(|segment| match segment.axis {
                FallbackAxis::Horizontal => {
                    segment.from.y == y
                        && x >= segment.from.x.min(segment.to.x)
                        && x <= segment.from.x.max(segment.to.x)
                }
                FallbackAxis::Vertical => {
                    segment.from.x == x
                        && y >= segment.from.y.min(segment.to.y)
                        && y <= segment.from.y.max(segment.to.y)
                }
            })
    }

    pub(crate) fn claims_boundary(
        &self,
        boundary_id: &str,
        side: &str,
        x: usize,
        y: usize,
    ) -> bool {
        self.boundary_claims.iter().any(|claim| {
            claim.boundary_id == boundary_id && claim.side == side && claim.x == x && claim.y == y
        })
    }

    pub(crate) fn trace_on(&self, canvas: &Canvas) -> FallbackRouteTrace {
        let mut coordinates = BTreeSet::new();
        for segment in &self.segments {
            match segment.axis {
                FallbackAxis::Horizontal => {
                    let (start, end) = if segment.from.x <= segment.to.x {
                        (segment.from.x, segment.to.x)
                    } else {
                        (segment.to.x, segment.from.x)
                    };
                    for x in start..=end {
                        coordinates.insert((x, segment.from.y));
                    }
                }
                FallbackAxis::Vertical => {
                    let (start, end) = if segment.from.y <= segment.to.y {
                        (segment.from.y, segment.to.y)
                    } else {
                        (segment.to.y, segment.from.y)
                    };
                    for y in start..=end {
                        coordinates.insert((segment.from.x, y));
                    }
                }
            }
        }
        for corner in &self.corners {
            coordinates.insert((corner.point.x, corner.point.y));
        }
        for paint in &self.paints {
            coordinates.insert((paint.point.x, paint.point.y));
        }
        for claim in &self.boundary_claims {
            coordinates.insert((claim.x, claim.y));
        }
        if let Some(source) = &self.source_attachment {
            coordinates.insert((source.point.x, source.point.y));
        }
        if let Some(target) = &self.target_attachment {
            coordinates.insert((target.point.x, target.point.y));
        }
        if let Some(arrow) = self.arrow_attachment {
            coordinates.insert((arrow.x, arrow.y));
        }

        let shared_coordinates: BTreeSet<_> = self
            .shared_cells
            .iter()
            .map(|point| (point.x, point.y))
            .collect();

        let mut mismatches = Vec::new();
        for claim in &self.boundary_claims {
            let glyph = canvas.get(claim.x, claim.y);
            let is_portal_seam = glyph != claim.expected_glyph
                && canvas.get_meta(claim.x, claim.y).is_some_and(|meta| {
                    meta.owner_kind == super::semantic::CellOwnerKind::PortalOpening
                        && boundary_seam_glyph(claim.expected_glyph, glyph)
                });
            if glyph != claim.expected_glyph && !is_portal_seam {
                mismatches.push(format!(
                    "boundary {} {} claim at ({},{}) expected {:?}, got {:?}",
                    claim.boundary_id, claim.side, claim.x, claim.y, claim.expected_glyph, glyph
                ));
            }
        }

        let boundary_coordinates: BTreeSet<_> = self
            .boundary_claims
            .iter()
            .map(|claim| (claim.x, claim.y))
            .collect();
        let contract_route = !self.covered_edge_ids.is_empty()
            || self.source_attachment.is_some()
            || self.target_attachment.is_some()
            || self.arrow_attachment.is_some();
        let mut cells = Vec::new();
        for (x, y) in coordinates {
            let Some(meta) = canvas.get_meta(x, y) else {
                mismatches.push(format!("planned route cell is out of canvas: ({x},{y})"));
                continue;
            };
            let glyph = canvas.get(x, y);
            if glyph == ' ' {
                mismatches.push(format!("planned route cell became empty: ({x},{y})"));
            }
            if let Some(paint) = self
                .paints
                .iter()
                .find(|paint| paint.point.x == x && paint.point.y == y)
            {
                let is_portal_seam = glyph != paint.glyph
                    && boundary_coordinates.contains(&(x, y))
                    && meta.owner_kind == super::semantic::CellOwnerKind::PortalOpening
                    && boundary_seam_glyph(paint.glyph, glyph);
                if glyph != paint.glyph && !is_portal_seam {
                    mismatches.push(format!(
                        "planned route paint at ({x},{y}) expected {:?}, got {:?}",
                        paint.glyph, glyph
                    ));
                }
            }
            let owned_by_scene = meta.owner_id.as_deref() == Some(self.owner_id.as_str());
            let owned_by_covered_edge = meta
                .owner_id
                .as_deref()
                .is_some_and(|owner_id| self.covered_edge_ids.iter().any(|id| id == owner_id));
            if contract_route
                && !boundary_coordinates.contains(&(x, y))
                && !shared_coordinates.contains(&(x, y))
                && !owned_by_scene
                && !owned_by_covered_edge
            {
                mismatches.push(format!(
                    "planned route cell at ({x},{y}) owned by {:?}, expected scene {:?} or one of {:?}",
                    meta.owner_id, self.owner_id, self.covered_edge_ids
                ));
            }
            cells.push(FallbackCellTrace {
                x,
                y,
                glyph,
                owner_id: meta.owner_id.clone(),
                owner_kind: format!("{:?}", meta.owner_kind),
                write_stage: canvas.write_stage_at(x, y).unwrap_or("unknown").to_owned(),
            });
        }

        FallbackRouteTrace {
            owner_id: self.owner_id.clone(),
            strategy: self.strategy.clone(),
            planned_segments: self.segments.clone(),
            planned_corners: self.corners.clone(),
            paints: self.paints.clone(),
            boundary_claims: self.boundary_claims.clone(),
            source_attachment: self.source_attachment.clone(),
            target_attachment: self.target_attachment.clone(),
            arrow_attachment: self.arrow_attachment,
            covered_edge_ids: self.covered_edge_ids.clone(),
            shared_cells: self.shared_cells.clone(),
            entry_decisions: self.entry_decisions.clone(),
            contract_digest: self.contract_digest.clone(),
            cells,
            mismatches,
        }
    }
}

/// A final portal projection may compose the route shaft with the enclosing
/// border. The route plan still owns the crossing and records its shaft glyph;
/// these are the only alternate glyphs accepted at a claimed boundary cell.
/// Requiring `PortalOpening` ownership at the call site prevents a generic
/// junction or overwritten route from being silently treated as equivalent.
fn boundary_seam_glyph(expected: char, actual: char) -> bool {
    matches!(
        (expected, actual),
        ('|', '+')
            | ('│', '┬' | '┴' | '┯' | '┷' | '╤' | '╧')
            | ('┃', '┳' | '┻' | '┰' | '┸')
            | ('║', '╥' | '╨')
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FallbackCellTrace {
    pub x: usize,
    pub y: usize,
    pub glyph: char,
    pub owner_id: Option<String>,
    pub owner_kind: String,
    pub write_stage: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FallbackRouteTrace {
    pub owner_id: String,
    pub strategy: String,
    pub planned_segments: Vec<FallbackSegment>,
    pub planned_corners: Vec<FallbackCorner>,
    pub paints: Vec<FallbackPaint>,
    pub boundary_claims: Vec<FallbackBoundaryClaim>,
    pub source_attachment: Option<FallbackAttachment>,
    pub target_attachment: Option<FallbackAttachment>,
    pub arrow_attachment: Option<FallbackPoint>,
    pub covered_edge_ids: Vec<String>,
    pub shared_cells: Vec<FallbackPoint>,
    pub entry_decisions: Vec<PortalEntryDecision>,
    pub contract_digest: Option<String>,
    pub cells: Vec<FallbackCellTrace>,
    pub mismatches: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FallbackRouteRejection {
    pub owner_id: String,
    pub strategy: String,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::{FallbackRoutePlan, PortalEntryDecision};

    fn complete_plan() -> FallbackRoutePlan {
        let mut plan = FallbackRoutePlan::new("edge:3:S->T", "test-connected");
        plan.set_source_attachment("source-group", "top", 8, 10);
        plan.set_target_attachment("target-group", "bottom", 11, 2);
        plan.set_arrow_attachment(8, 0);
        plan.claim_boundary("source-group", "top", 8, 10, '|');
        plan.claim_boundary("target-group", "bottom", 11, 2, '|');
        plan.push_vertical(8, 12, 6, '|');
        plan.push_corner(8, 6, '+');
        plan.push_horizontal(6, 8, 11, '-');
        plan.push_corner(11, 6, '+');
        plan.push_vertical(11, 6, 2, '|');
        plan.push_corner(11, 1, '+');
        plan.push_horizontal(1, 11, 8, '-');
        plan.push_corner(8, 1, '+');
        plan.push_vertical(8, 1, 0, '|');
        plan
    }

    #[test]
    fn connected_route_contract_accepts_complete_path() {
        let plan = complete_plan();

        assert_eq!(plan.validation_error(20, 20), None);
    }

    #[test]
    fn connected_route_contract_rejects_disconnected_primitive() {
        let mut plan = complete_plan();
        plan.segments[1].from.y = 4;
        plan.segments[1].to.y = 4;

        let error = plan.validation_error(20, 20).expect("disconnected plan");
        assert!(error.contains("disconnected"));
    }

    #[test]
    fn connected_route_contract_rejects_missing_boundary_claim() {
        let mut plan = complete_plan();
        plan.boundary_claims
            .retain(|claim| claim.boundary_id != "target-group");

        let error = plan.validation_error(20, 20).expect("missing claim");
        assert!(error.contains("target attachment"));
    }

    fn decision_plan(with_claim: bool) -> FallbackRoutePlan {
        let mut plan = FallbackRoutePlan::new("scene:test", "test-decision");
        plan.set_scene_coverage(["edge:1:S->T".to_owned()]);
        plan.set_target_entry_decision(PortalEntryDecision {
            edge_id: "edge:1:S->T".to_owned(),
            owner_id: "scene:test".to_owned(),
            target_node_id: "T".to_owned(),
            boundary_id: "target-group".to_owned(),
            side: "bottom".to_owned(),
            portal_x: 11,
            portal_y: 6,
            arrow_x: 11,
            arrow_y: 4,
        });
        if with_claim {
            plan.claim_boundary("target-group", "bottom", 11, 6, '|');
        }
        plan.push_paint(11, 4, '^');
        plan
    }

    #[test]
    fn target_entry_decision_requires_matching_claim() {
        let error = decision_plan(false)
            .validation_error(20, 20)
            .expect("missing target-entry claim");
        assert!(error.contains("target-entry decision has no matching boundary claim"));
    }

    #[test]
    fn target_entry_decision_accepts_matching_claim_and_arrow() {
        assert_eq!(decision_plan(true).validation_error(20, 20), None);
    }
}
