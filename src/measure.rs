//! Node measurement helpers (label truncation/wrapping and box height).
//!
//! This stays opt-in: default behavior remains single-line labels with fixed
//! `BOX_HEIGHT` unless `Config.wrap_labels` is enabled.

use crate::config::Config;
use crate::graph::{Graph, NodeShape};
use crate::style::{box_width, display_width, truncate_label, BOX_HEIGHT, BOX_MIN_WIDTH};

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

    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut width = 0usize;

    for c in word.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
        if width + cw > max_width && !current.is_empty() {
            out.push(std::mem::take(&mut current));
            width = 0;
        }
        current.push(c);
        width += cw;
        if width >= max_width {
            out.push(std::mem::take(&mut current));
            width = 0;
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
    let base = truncate_label(&lines[last_idx], budget);
    lines[last_idx] = format!("{}{}", base, suffix);
    lines
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

/// Prepare a parsed graph for layout/render by ensuring node dimensions exist and
/// precomputing label lines + box height.
pub fn measure_graph(graph: &mut Graph, config: &Config) {
    for node in graph.nodes.iter_mut() {
        if node.width == 0 {
            node.width = box_width(&node.label).max(BOX_MIN_WIDTH);
        }

        // For boxlike nodes, the render path uses `width - 4` as the label area width.
        // Keep wrapping within that (and within the user-configured limit).
        let max_line_width = config
            .max_label_width
            .min(node.width.saturating_sub(4));

        if config.wrap_labels && supports_multiline(node.shape) {
            node.label_lines =
                wrapped_label_lines(&node.label, max_line_width, config.max_label_lines);

            // Shrink overly-wide boxes (e.g., labels with `<br>` markers) to the actual visible
            // content, without ever expanding the existing width (keeps layout stable).
            let visible_width = node
                .label_lines
                .iter()
                .map(|l| display_width(l))
                .max()
                .unwrap_or(0);
            let desired_width = (visible_width + 4).max(BOX_MIN_WIDTH);
            node.width = node.width.min(desired_width);

            node.height = (node.label_lines.len() + 2).max(BOX_HEIGHT);
        } else {
            node.label_lines = single_line_label(&node.label, max_line_width);
            node.height = BOX_HEIGHT;
        }
    }
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

        let mut cfg = Config::default();
        cfg.wrap_labels = true;
        cfg.max_label_lines = 3;
        cfg.max_label_width = 8;

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
    fn wrap_shrinks_inflated_width() {
        let mut g = Graph::new();
        let mut n = Node::new("A", "line one<br>line two");
        n.width = 60;
        g.nodes.push(n);

        let mut cfg = Config::default();
        cfg.wrap_labels = true;
        cfg.max_label_lines = 10;
        cfg.max_label_width = 20;

        measure_graph(&mut g, &cfg);
        assert!(g.nodes[0].label_lines.len() >= 2);
        assert!(g.nodes[0].width < 60);
        assert!(g.nodes[0].width >= BOX_MIN_WIDTH);
    }
}
