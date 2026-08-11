//! Public render result owned separately from the render pipeline.

use super::critic::CriticReport;
use super::semantic::SemanticFrame;
use super::trace::PortalTrace;

/// Detailed render output including semantic and critic information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOutcome {
    pub output: String,
    pub semantic_frame: SemanticFrame,
    pub display_semantic_frame: SemanticFrame,
    pub critic_report: CriticReport,
    pub warnings: Vec<String>,
    pub optimized: bool,
    pub repair_passes: usize,
    pub layout_attempts: usize,
    pub layout_repairs_applied: usize,
    /// Diagnostic-only portal ownership and write-stage trace for QA packets.
    pub portal_trace: PortalTrace,
}
