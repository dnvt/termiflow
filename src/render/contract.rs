//! Code-facing render-layer contract.
//!
//! The current renderer is still incremental and pragmatic, but these layer
//! boundaries describe the canonical responsibilities each module group owns.

/// Canonical render layers from layout reservation through terminal transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderLayer {
    Reservation,
    Topology,
    SemanticCells,
    GlyphProjection,
    TerminalTransport,
}

impl RenderLayer {
    pub fn label(self) -> &'static str {
        match self {
            RenderLayer::Reservation => "reservation",
            RenderLayer::Topology => "topology",
            RenderLayer::SemanticCells => "semantic-cells",
            RenderLayer::GlyphProjection => "glyph-projection",
            RenderLayer::TerminalTransport => "terminal-transport",
        }
    }
}

/// Current code-level contract for one render layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderLayerSpec {
    pub layer: RenderLayer,
    pub canonical_state: &'static str,
    pub primary_modules: &'static [&'static str],
    pub responsibilities: &'static [&'static str],
}

pub struct RenderLayerContract;

const RENDER_LAYER_CONTRACT: [RenderLayerSpec; 5] = [
    RenderLayerSpec {
        layer: RenderLayer::Reservation,
        canonical_state: "layout reservations, bounds, keepouts, and portal slots",
        primary_modules: &["layout", "portals", "spacing", "graph"],
        responsibilities: &[
            "allocate ranks and sibling corridors",
            "reserve subgraph envelopes and title bands",
            "define portal slots before glyph drawing begins",
        ],
    },
    RenderLayerSpec {
        layer: RenderLayer::Topology,
        canonical_state: "edge routes, boundary crossings, and route-owned segments",
        primary_modules: &["geom", "render::edge", "render::cycle", "graph"],
        responsibilities: &[
            "capture route segments before overlap resolution",
            "express boundary exits and entries independently of glyphs",
            "keep cheap deterministic routing separate from final characters",
        ],
    },
    RenderLayerSpec {
        layer: RenderLayer::SemanticCells,
        canonical_state: "cell ownership, role, and z-order",
        primary_modules: &["render::canvas", "render::semantic", "render::provenance"],
        responsibilities: &[
            "track owner-kind and role metadata per cell",
            "stamp portal, node, edge, and label ownership",
            "provide the critic and repair passes with non-glyph state",
        ],
    },
    RenderLayerSpec {
        layer: RenderLayer::GlyphProjection,
        canonical_state: "resolved visible diagram glyphs",
        primary_modules: &[
            "render::mod",
            "render::shapes",
            "render::repair",
            "render::topology",
        ],
        responsibilities: &[
            "project semantic state into ASCII or Unicode glyphs",
            "apply overlap resolution and bounded repair",
            "keep border contracts and junction contracts visible in the final frame",
        ],
    },
    RenderLayerSpec {
        layer: RenderLayer::TerminalTransport,
        canonical_state: "terminal cells, viewport slices, and presenter diffs",
        primary_modules: &["tui::frame", "tui::live", "tui::presenter"],
        responsibilities: &[
            "slice rendered lines by display columns",
            "preserve wide glyph spans and combining marks in the retained frame",
            "emit synchronized terminal updates when supported",
        ],
    },
];

impl RenderLayerContract {
    pub fn current() -> &'static [RenderLayerSpec] {
        &RENDER_LAYER_CONTRACT
    }
}

pub fn current_render_layer_contract() -> &'static [RenderLayerSpec] {
    RenderLayerContract::current()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_layer_contract_lists_expected_layers_in_order() {
        let labels: Vec<_> = current_render_layer_contract()
            .iter()
            .map(|spec| spec.layer.label())
            .collect();

        assert_eq!(
            labels,
            vec![
                "reservation",
                "topology",
                "semantic-cells",
                "glyph-projection",
                "terminal-transport"
            ]
        );
    }

    #[test]
    fn render_layer_contract_has_modules_and_responsibilities() {
        for spec in current_render_layer_contract() {
            assert!(!spec.primary_modules.is_empty());
            assert!(!spec.responsibilities.is_empty());
            assert!(!spec.canonical_state.is_empty());
        }
    }
}
