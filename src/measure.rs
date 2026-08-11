//! Node measurement helpers (label truncation/wrapping and box height).
//!
//! This stays opt-in: default behavior remains single-line labels with fixed
//! `BOX_HEIGHT` unless `Config.wrap_labels` is enabled.

use crate::config::Config;
use crate::graph::{Direction, EdgeKind, Graph, NodeShape};
use crate::render::bt_parallel_identity::target_port_counts as bt_parallel_target_port_counts;
use crate::render::dedicated_fan_in::{minimum_port_height, target_port_counts};
use crate::render::dual_junction::target_port_counts as dual_junction_target_port_counts;
use crate::render::fan_in_identity::{
    minimum_port_span as identity_minimum_port_span,
    target_port_counts as identity_target_port_counts,
};
use crate::render::sibling_subgraph_fan_in_identity::target_port_counts as sibling_subgraph_target_port_counts;
use crate::render::sibling_target_entry_identity::horizontal_target_port_counts;
use crate::render::subgraph_fan_in_identity::{
    minimum_target_span as subgraph_minimum_target_span,
    target_port_counts as subgraph_target_port_counts,
};
use crate::render::vertical_fan_in::{
    minimum_port_width, nonterminal_target_port_counts,
    target_port_counts as vertical_target_port_counts,
};
use crate::render::wide_terminal_fan_in::{
    minimum_port_height as wide_minimum_port_height, minimum_port_width as wide_minimum_port_width,
    target_port_counts as wide_target_port_counts,
};
use crate::style::{
    box_width, display_width, split_text_to_width_chunks, truncate_label, truncate_to_width,
    BOX_HEIGHT, BOX_MIN_WIDTH, BOX_PADDING,
};

fn supports_multiline(shape: NodeShape) -> bool {
    matches!(
        shape,
        NodeShape::Rectangle
            | NodeShape::Rounded
            | NodeShape::Stadium
            | NodeShape::Hexagon
            | NodeShape::Database
            | NodeShape::Subroutine
            | NodeShape::Asymmetric
            | NodeShape::Parallelogram
            | NodeShape::ParallelogramAlt
            | NodeShape::Trapezoid
            | NodeShape::TrapezoidAlt
    )
}

fn normalize_breaks(label: &str) -> String {
    label
        .replace("\r\n", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n")
        .replace("\\n", "\n")
}

fn split_long_word(word: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![String::new()];
    }

    if display_width(word) <= max_width {
        return vec![word.to_string()];
    }

    // Prefer splitting long "code-ish" tokens on common delimiters so wrapping
    // doesn't produce awkward mid-word breaks (e.g. `route_convergent_edg` / `es`).
    //
    // Delimiters are kept with the left chunk (e.g. `route_convergent_` + `edges`,
    // `Canvas::` + `set_edge_char`) to avoid lines starting with punctuation.
    let mut parts: Vec<String> = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < word.len() {
        if word[i..].starts_with("::") {
            let end = i + 2;
            if end > start {
                parts.push(word[start..end].to_string());
            }
            start = end;
            i = end;
            continue;
        }

        let ch = word[i..].chars().next().unwrap();
        let len = ch.len_utf8();
        if matches!(ch, '_' | '-' | '.' | '/') {
            let end = i + len;
            if end > start {
                parts.push(word[start..end].to_string());
            }
            start = end;
            i = end;
            continue;
        }

        i += len;
    }
    if start < word.len() {
        parts.push(word[start..].to_string());
    }

    if parts.len() > 1 {
        let mut out: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut width = 0usize;

        for part in parts {
            let part_width = display_width(&part);
            if part_width > max_width {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                    width = 0;
                }
                // Fall back to hard splitting for an overlong segment.
                out.extend(split_long_word(&part, max_width));
                continue;
            }

            if width + part_width <= max_width {
                current.push_str(&part);
                width += part_width;
            } else {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
                current.push_str(&part);
                width = part_width;
            }
        }

        if !current.is_empty() {
            out.push(current);
        }
        if !out.is_empty() {
            return out;
        }
    }

    split_text_to_width_chunks(word, max_width)
}

fn wrap_line_to_width(line: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![String::new()];
    }

    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for word in line.split_whitespace() {
        let word_width = display_width(word);
        if current.is_empty() {
            if word_width <= max_width {
                current.push_str(word);
                current_width = word_width;
            } else {
                for chunk in split_long_word(word, max_width) {
                    out.push(chunk);
                }
            }
            continue;
        }

        let needs_space = 1usize;
        if current_width + needs_space + word_width <= max_width {
            current.push(' ');
            current.push_str(word);
            current_width += needs_space + word_width;
        } else {
            out.push(std::mem::take(&mut current));
            current_width = 0;

            if word_width <= max_width {
                current.push_str(word);
                current_width = word_width;
            } else {
                for chunk in split_long_word(word, max_width) {
                    out.push(chunk);
                }
            }
        }
    }

    if !current.is_empty() {
        out.push(current);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn apply_max_lines(mut lines: Vec<String>, max_lines: usize, max_width: usize) -> Vec<String> {
    if max_lines == 0 {
        return vec![String::new()];
    }
    if lines.len() <= max_lines {
        return lines;
    }

    lines.truncate(max_lines);
    if max_width == 0 {
        return lines;
    }

    let last_idx = lines.len().saturating_sub(1);
    let suffix = "...";
    let suffix_width = display_width(suffix);
    if max_width <= suffix_width {
        lines[last_idx] = suffix.to_string();
        return lines;
    }

    let budget = max_width.saturating_sub(suffix_width);
    let base = truncate_label_hard(&lines[last_idx], budget);
    lines[last_idx] = format!("{base}{suffix}");
    lines
}

fn truncate_label_hard(label: &str, max_width: usize) -> String {
    truncate_to_width(label, max_width)
}

fn single_line_label(label: &str, max_width: usize) -> Vec<String> {
    let collapsed = normalize_breaks(label)
        .replace('\n', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    vec![truncate_label(&collapsed, max_width)]
}

fn wrapped_label_lines(label: &str, max_width: usize, max_lines: usize) -> Vec<String> {
    let normalized = normalize_breaks(label);
    let mut out: Vec<String> = Vec::new();

    for raw in normalized.split('\n') {
        let raw = raw.trim();
        if raw.is_empty() {
            out.push(String::new());
            continue;
        }
        out.extend(wrap_line_to_width(raw, max_width));
    }

    if out.is_empty() {
        out.push(String::new());
    }
    apply_max_lines(out, max_lines, max_width)
}

fn box_width_for_content_width(content_width: usize) -> usize {
    (content_width + BOX_PADDING * 2 + 2).max(BOX_MIN_WIDTH)
}

/// Prepare a parsed graph for layout/render by ensuring node dimensions exist and
/// precomputing label lines + box height.
pub fn measure_graph(graph: &mut Graph, config: &Config) {
    for node in graph.nodes.iter_mut() {
        let default_width = box_width(&node.label).max(BOX_MIN_WIDTH);
        let width_is_default = node.width == 0 || node.width == default_width;

        // If the node already has an explicit width (e.g., constructed in tests), honor it
        // as an additional cap for wrapping/truncation.
        let width_cap = if width_is_default {
            config.max_label_width
        } else {
            config.max_label_width.min(node.width.saturating_sub(4))
        };

        if config.wrap_labels && supports_multiline(node.shape) {
            node.label_lines = wrapped_label_lines(&node.label, width_cap, config.max_label_lines);
            let visible_width = node
                .label_lines
                .iter()
                .map(|l| display_width(l))
                .max()
                .unwrap_or(0);
            if width_is_default {
                node.width = box_width_for_content_width(visible_width);
            }

            node.height = (node.label_lines.len() + 2).max(BOX_HEIGHT);
        } else {
            node.label_lines = single_line_label(&node.label, width_cap);
            let visible_width = node
                .label_lines
                .first()
                .map(|l| display_width(l))
                .unwrap_or(0);
            if width_is_default {
                node.width = box_width_for_content_width(visible_width);
            }
            node.height = BOX_HEIGHT;
        }
    }

    // Horizontal dense crossing scenes need two independent side ports per
    // node. A three-row box exposes only one interior side cell, so a pair of
    // routes would be forced to share the same arrow attachment. Increase the
    // measured box height only for the topology family that the dense scene
    // lowerer can prove safe; ordinary diagrams keep their compact boxes.
    if dense_horizontal_crossing_candidate(graph) {
        for node in &mut graph.nodes {
            // Two source side ports and two target side ports need disjoint
            // interior rows. A five-row box only exposes two interior rows,
            // forcing source and target attachments onto the same corridor;
            // Nine rows leave a full blank separator on both sides of the
            // inner target ports in the raw frame.
            node.height = node.height.max(9);
        }
    }

    // Some horizontal fan-in families need a separate target-side row for
    // every incoming edge. Keep this sizing decision coupled to the render
    // policy so routing never asks a compact box for ports it cannot expose.
    for (target_id, port_count) in target_port_counts(graph) {
        if let Some(node) = graph.nodes.iter_mut().find(|node| node.id == target_id) {
            node.height = node.height.max(minimum_port_height(port_count));
        }
    }

    // The bounded vertical fan-in scene attaches arrows to distinct interior
    // columns on the target boundary. Reserve that width before layout so the
    // target projection and route lowerer share one capacity contract.
    let mut vertical_target_ports = vertical_target_port_counts(graph);
    vertical_target_ports.extend(nonterminal_target_port_counts(graph));
    vertical_target_ports.extend(dual_junction_target_port_counts(graph));
    for (target_id, port_count) in vertical_target_ports {
        if let Some(node) = graph.nodes.iter_mut().find(|node| node.id == target_id) {
            node.width = node.width.max(minimum_port_width(port_count));
        }
    }

    // Ordinary identity fan-in uses the same centered, separated port
    // contract in the render lowerer.  Reserve that capacity before layout
    // so the lowerer never has to invent ports after node placement.
    for (target_id, port_count) in identity_target_port_counts(graph) {
        if let Some(node) = graph.nodes.iter_mut().find(|node| node.id == target_id) {
            if matches!(graph.direction, Direction::LR | Direction::RL) {
                node.height = node.height.max(identity_minimum_port_span(port_count));
            } else {
                node.width = node.width.max(identity_minimum_port_span(port_count));
            }
        }
    }

    // Strict boundary-owned subgraph fan-in keeps the source portals distinct
    // and now exposes one target-side port per incoming edge.  This capacity
    // contract is intentionally separate from ordinary subgraph-free fan-in.
    for (target_id, port_count) in subgraph_target_port_counts(graph) {
        if let Some(node) = graph.nodes.iter_mut().find(|node| node.id == target_id) {
            if matches!(graph.direction, Direction::LR | Direction::RL) {
                node.height = node.height.max(subgraph_minimum_target_span(port_count));
            } else {
                node.width = node.width.max(subgraph_minimum_target_span(port_count));
            }
        }
    }

    // The bounded sibling-subgraph scene owns two external target entries but
    // is not eligible for the strict single-subgraph capacity policy above.
    // Reserve the same centered target span before layout so LR/RL can expose
    // two interior side rows instead of collapsing both edges onto the one
    // row available in a default three-row box.
    for (target_id, port_count) in sibling_subgraph_target_port_counts(graph) {
        if let Some(node) = graph.nodes.iter_mut().find(|node| node.id == target_id) {
            let span = identity_minimum_port_span(port_count);
            // BT can choose a horizontal side for a diagonal placement, as
            // can LR/RL.  TD/TB use the top/bottom columns and stay compact
            // vertically unless the physical scene later proves otherwise.
            if matches!(
                graph.direction,
                Direction::BT | Direction::LR | Direction::RL
            ) {
                node.height = node.height.max(span);
            } else {
                node.width = node.width.max(span);
            }
        }
    }
    // The exact LR/RL mixed sibling-target scene owns one internal and one
    // cross-subgraph arrival at D.  Reserve the same two horizontal entry
    // rows that its scene lowerer consumes; otherwise a default three-row
    // target would collapse both arrivals back onto the center row.
    for (target_id, port_count) in horizontal_target_port_counts(graph) {
        if let Some(node) = graph.nodes.iter_mut().find(|node| node.id == target_id) {
            node.height = node.height.max(identity_minimum_port_span(port_count));
        }
    }
    // The bounded BT parallel subgraph scene owns two internal target entries.
    // Reserve that capacity from the same topology selector used by the scene
    // lowerer so measurement cannot collapse the entries back to one center.
    for (target_id, port_count) in bt_parallel_target_port_counts(graph) {
        if let Some(node) = graph.nodes.iter_mut().find(|node| node.id == target_id) {
            node.width = node.width.max(identity_minimum_port_span(port_count));
        }
    }
    // The proof-gated wide terminal fan-in scene needs one separated interior
    // target column per incoming edge. Keep its capacity contract separate
    // from the narrow two/three-source experiment.
    for (target_id, port_count) in wide_target_port_counts(graph) {
        if let Some(node) = graph.nodes.iter_mut().find(|node| node.id == target_id) {
            if matches!(graph.direction, Direction::LR | Direction::RL) {
                node.height = node.height.max(wide_minimum_port_height(port_count));
            } else {
                node.width = node.width.max(wide_minimum_port_width(port_count));
            }
        }
    }
}

fn dense_horizontal_crossing_candidate(graph: &Graph) -> bool {
    if !matches!(graph.direction, Direction::LR | Direction::RL)
        || !graph.subgraphs.is_empty()
        || graph
            .nodes
            .iter()
            .any(|node| node.shape != NodeShape::Rectangle)
        || graph
            .edges
            .iter()
            .any(|edge| edge.is_back_edge || edge.kind != EdgeKind::Arrow || edge.label.is_some())
    {
        return false;
    }

    let sources: Vec<&str> = graph
        .nodes
        .iter()
        .filter(|node| {
            graph
                .edges
                .iter()
                .filter(|edge| edge.from == node.id)
                .count()
                == 2
        })
        .map(|node| node.id.as_str())
        .collect();
    let targets: Vec<&str> = graph
        .nodes
        .iter()
        .filter(|node| graph.edges.iter().filter(|edge| edge.to == node.id).count() == 2)
        .map(|node| node.id.as_str())
        .collect();
    if sources.len() < 3 || targets.len() < 3 {
        return false;
    }

    sources.iter().enumerate().any(|(first, a)| {
        sources
            .iter()
            .enumerate()
            .skip(first + 1)
            .any(|(second, b)| {
                sources.iter().enumerate().skip(second + 1).any(|(_, c)| {
                    let source_set = [*a, *b, *c];
                    let relation: Vec<(&str, &str)> = graph
                        .edges
                        .iter()
                        .filter(|edge| source_set.contains(&edge.from.as_str()))
                        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
                        .collect();
                    let target_set: std::collections::HashSet<&str> =
                        relation.iter().map(|(_, target)| *target).collect();
                    target_set.len() == 3
                        && relation.len() == 6
                        && source_set.iter().all(|source| {
                            relation.iter().filter(|(from, _)| from == source).count() == 2
                        })
                        && target_set.iter().all(|target| {
                            relation.iter().filter(|(_, to)| to == target).count() == 2
                        })
                })
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Graph, Node};

    #[test]
    fn measure_wrap_increases_height() {
        let mut g = Graph::new();
        let mut n = Node::new("A", "hello world from termiflow");
        n.width = 12; // inner width = 8
        g.nodes.push(n);

        let cfg = Config {
            wrap_labels: true,
            max_label_lines: 3,
            max_label_width: 8,
            ..Default::default()
        };

        measure_graph(&mut g, &cfg);
        assert_eq!(g.nodes[0].label_lines.len(), 3);
        assert_eq!(g.nodes[0].height, 5);
    }

    #[test]
    fn measure_default_is_single_line_fixed_height() {
        let mut g = Graph::new();
        g.nodes.push(Node::new("A", "one two three four five"));

        let cfg = Config::default();
        measure_graph(&mut g, &cfg);

        assert_eq!(g.nodes[0].height, BOX_HEIGHT);
        assert_eq!(g.nodes[0].label_lines.len(), 1);
    }

    #[test]
    fn explicit_width_is_honored() {
        let mut g = Graph::new();
        let mut n = Node::new("A", "line one<br>line two");
        n.width = 60;
        g.nodes.push(n);

        let cfg = Config {
            wrap_labels: true,
            max_label_lines: 10,
            max_label_width: 20,
            ..Default::default()
        };

        measure_graph(&mut g, &cfg);
        assert!(g.nodes[0].label_lines.len() >= 2);
        assert_eq!(g.nodes[0].width, 60);
        assert!(g.nodes[0].width >= BOX_MIN_WIDTH);
    }

    #[test]
    fn wrap_can_shrink_default_width_for_manual_line_breaks() {
        let mut g = Graph::new();
        g.nodes.push(Node::new("A", "short line<br>tiny"));

        let cfg = Config {
            wrap_labels: true,
            max_label_lines: 10,
            max_label_width: 80,
            ..Default::default()
        };

        measure_graph(&mut g, &cfg);
        assert!(g.nodes[0].label_lines.len() >= 2);
        // Default width for the raw label is based on its full string; in wrap mode we
        // size to the widest visible line.
        assert!(g.nodes[0].width < box_width("short line<br>tiny").max(BOX_MIN_WIDTH));
        assert!(g.nodes[0].width >= BOX_MIN_WIDTH);
    }

    #[test]
    fn max_label_width_controls_box_width() {
        let mut g = Graph::new();
        g.nodes.push(Node::new("A", "this is a longer label"));

        let cfg = Config {
            max_label_width: 10,
            ..Default::default()
        };

        measure_graph(&mut g, &cfg);
        let w10 = g.nodes[0].width;

        let mut g2 = Graph::new();
        g2.nodes.push(Node::new("A", "this is a longer label"));
        let cfg2 = Config {
            max_label_width: 20,
            ..Default::default()
        };
        measure_graph(&mut g2, &cfg2);
        let w20 = g2.nodes[0].width;

        assert!(w20 > w10);
    }

    #[test]
    fn wrap_uses_single_ellipsis_when_truncated_by_max_lines() {
        let mut g = Graph::new();
        g.nodes.push(Node::new(
            "A",
            "one two three four five six seven eight nine",
        ));

        let cfg = Config {
            wrap_labels: true,
            max_label_width: 6,
            max_label_lines: 2,
            ..Default::default()
        };

        measure_graph(&mut g, &cfg);
        assert_eq!(g.nodes[0].label_lines.len(), 2);
        assert!(g.nodes[0].label_lines[1].ends_with("..."));
        assert!(!g.nodes[0].label_lines[1].ends_with("......"));
    }

    #[test]
    fn split_long_word_preserves_emoji_graphemes() {
        let family = "👨‍👩‍👧‍👦";
        assert_eq!(
            split_long_word(&format!("{family}{family}"), display_width(family)),
            vec![family.to_string(), family.to_string()]
        );
    }

    #[test]
    fn wrapped_label_lines_preserve_combining_graphemes() {
        let accented = "e\u{301}";
        assert_eq!(
            wrapped_label_lines(
                &format!("{accented}{accented}{accented}"),
                display_width(accented),
                8
            ),
            vec![
                accented.to_string(),
                accented.to_string(),
                accented.to_string()
            ]
        );
    }
}
