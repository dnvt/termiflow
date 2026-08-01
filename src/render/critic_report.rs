//! Stable critic finding types and human-facing aggregation.

use serde::Serialize;

/// Stable code for a critic finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FindingCode {
    EmptyRenderedFrame,
    JunctionTopologyMismatch,
    RouteTopologyMismatch,
    RouteSymmetryImbalance,
    BranchSpacingImbalance,
    BranchCrowding,
    UnusedPortalOpening,
    ArrowWithoutVisibleShaft,
    ChainTooCrampedLR,
    ArrowTouchesNodeBorder,
    ArrowTouchesSubgraphBorder,
    RouteCrossesNodeInterior,
    SubgraphTitleCorrupted,
    CrowdedEdgeLabel,
    CanvasClipped,
    EdgeLabelCollidesWithNode,
}

/// Severity level for a critic finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FindingSeverity {
    Info,
    Warning,
    Error,
}

/// Single critic finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CriticFinding {
    pub code: FindingCode,
    pub severity: FindingSeverity,
    pub penalty: i32,
    pub message: String,
    pub cells: Vec<(usize, usize)>,
    pub owner_ids: Vec<String>,
}

/// Aggregate critic report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CriticReport {
    pub score: i32,
    pub findings: Vec<CriticFinding>,
    pub notes: Vec<String>,
}

/// High-level quality verdict derived from a critic report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AuditVerdict {
    Clean,
    NeedsReview,
    Broken,
}

/// Human-facing audit summary for a rendered diagram.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditSummary {
    pub verdict: AuditVerdict,
    pub score: i32,
    pub info_count: usize,
    pub warning_count: usize,
    pub error_count: usize,
    pub highlights: Vec<String>,
}

impl AuditSummary {
    pub fn is_clean(&self) -> bool {
        self.verdict == AuditVerdict::Clean
    }
}

impl CriticReport {
    pub fn audit_summary(&self) -> AuditSummary {
        let info_count = self
            .findings
            .iter()
            .filter(|finding| finding.severity == FindingSeverity::Info)
            .count();
        let warning_count = self
            .findings
            .iter()
            .filter(|finding| finding.severity == FindingSeverity::Warning)
            .count();
        let error_count = self
            .findings
            .iter()
            .filter(|finding| finding.severity == FindingSeverity::Error)
            .count();

        let verdict = if error_count > 0 {
            AuditVerdict::Broken
        } else if self.findings.is_empty() {
            AuditVerdict::Clean
        } else {
            AuditVerdict::NeedsReview
        };

        let mut ordered: Vec<&CriticFinding> = self.findings.iter().collect();
        ordered.sort_by_key(|finding| (severity_rank(finding.severity), finding.penalty));
        ordered.reverse();
        let highlights = ordered
            .into_iter()
            .take(5)
            .map(|finding| {
                format!(
                    "{:?} {:?}: {}",
                    finding.severity, finding.code, finding.message
                )
            })
            .collect();

        AuditSummary {
            verdict,
            score: self.score,
            info_count,
            warning_count,
            error_count,
            highlights,
        }
    }
}

fn severity_rank(severity: FindingSeverity) -> u8 {
    match severity {
        FindingSeverity::Info => 0,
        FindingSeverity::Warning => 1,
        FindingSeverity::Error => 2,
    }
}
