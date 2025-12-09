//! Mermaid-Lite parser with two-pass parsing
//!
//! Implements SPEC §1: Two-pass parsing with lenient/strict modes
//!
//! Pass 1: Collect all node identifiers from definitions and edges
//! Pass 2: Build graph with validation and auto-create missing nodes

use anyhow::{bail, Result};
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::{HashMap, HashSet};

use crate::graph::{Direction, Edge, Graph, Node, NodeShape};

lazy_static! {
    // SPEC §1.1: Supported syntax patterns
    static ref RE_DIRECTION: Regex = Regex::new(r"graph\s+(TD|LR|RL|TB|BT)").unwrap();

    // Node shape regexes - order matters! More specific patterns first
    // Database: ID[(label)]
    static ref RE_NODE_DB: Regex = Regex::new(r"([a-zA-Z0-9_]+)\[\(([^\)]*)\)\]").unwrap();
    // Subroutine: ID[[label]]
    static ref RE_NODE_SUBROUTINE: Regex = Regex::new(r"([a-zA-Z0-9_]+)\[\[([^\]]*)\]\]").unwrap();
    // Stadium: ID([label])
    static ref RE_NODE_STADIUM: Regex = Regex::new(r"([a-zA-Z0-9_]+)\(\[([^\]]*)\]\)").unwrap();
    // Circle: ID((label))
    static ref RE_NODE_CIRCLE: Regex = Regex::new(r"([a-zA-Z0-9_]+)\(\(([^\)]*)\)\)").unwrap();
    // Hexagon: ID{{label}}
    static ref RE_NODE_HEXAGON: Regex = Regex::new(r"([a-zA-Z0-9_]+)\{\{([^\}]*)\}\}").unwrap();
    // Diamond: ID{label}
    static ref RE_NODE_DIAMOND: Regex = Regex::new(r"([a-zA-Z0-9_]+)\{([^\}]*)\}").unwrap();
    // Rounded: ID(label)
    static ref RE_NODE_ROUNDED: Regex = Regex::new(r"([a-zA-Z0-9_]+)\(([^\(\)]*)\)").unwrap();
    // Asymmetric/Flag: ID>label]
    static ref RE_NODE_ASYMMETRIC: Regex = Regex::new(r"([a-zA-Z0-9_]+)>([^\]]*)\]").unwrap();
    // Parallelogram: ID[/label/]
    static ref RE_NODE_PARALLELOGRAM: Regex = Regex::new(r"([a-zA-Z0-9_]+)\[/([^/]*)(/|\\)\]").unwrap();
    // Trapezoid: ID[/label\] or ID[\label/]
    static ref RE_NODE_TRAPEZOID: Regex = Regex::new(r"([a-zA-Z0-9_]+)\[(/|\\)([^/\\]*)(\\|/)\]").unwrap();
    // Rectangle: ID[label] - default, must be last
    static ref RE_NODE: Regex = Regex::new(r"([a-zA-Z0-9_]+)\[([^\[\]]*)\]").unwrap();

    // Edge regex - handles optional shape syntax after node IDs
    static ref RE_EDGE: Regex = Regex::new(r"([a-zA-Z0-9_]+)(?:\[[^\]]*\]|\([^\)]*\)|\{[^\}]*\}|>[^\]]*\])?\s*--+>\s*([a-zA-Z0-9_]+)").unwrap();
    // Edge with pipe-style label: A -->|label| B
    static ref RE_EDGE_WITH_LABEL: Regex = Regex::new(
        r"([a-zA-Z0-9_]+)(?:\[[^\]]*\]|\([^\)]*\)|\{[^\}]*\}|>[^\]]*\])?\s*--+>\s*\|([^|]+)\|\s*([a-zA-Z0-9_]+)"
    ).unwrap();
    // Edge with text-style label: A -- label --> B
    static ref RE_EDGE_TEXT_LABEL: Regex = Regex::new(
        r"([a-zA-Z0-9_]+)(?:\[[^\]]*\]|\([^\)]*\)|\{[^\}]*\}|>[^\]]*\])?\s*--\s+([^-]+?)\s+--+>\s*([a-zA-Z0-9_]+)"
    ).unwrap();
    static ref RE_CLICK: Regex = Regex::new(r#"click\s+(\w+)\s+["']([^"']+)["']"#).unwrap();
    static ref RE_CONFIG: Regex = Regex::new(r"%%\s*termiflow:\s*(\w+)=([^\s]+)").unwrap();
    static ref RE_COMMENT: Regex = Regex::new(r"^\s*%%").unwrap();
    static ref RE_DIAGRAM_TYPE: Regex = Regex::new(
        r"^(flowchart|sequenceDiagram|classDiagram|stateDiagram-v2|stateDiagram|erDiagram|journey|gantt|pie|requirementDiagram|timeline|mindmap|gitGraph|block|quadrantChart)\b"
    ).unwrap();

    // SPEC §1.2: Unsupported syntax patterns
    static ref RE_NESTED_BRACKET: Regex = Regex::new(r"\[[^\]]*\[").unwrap();
    static ref RE_PIPE_IN_LABEL: Regex = Regex::new(r"\[[^\]]*\|[^\]]*\]").unwrap();
    static ref RE_SUBGRAPH: Regex = Regex::new(r"^\s*subgraph\s").unwrap();
    static ref RE_STYLE: Regex = Regex::new(r"^\s*style\s+\w+").unwrap();
    static ref RE_CLASSDEF: Regex = Regex::new(r"^\s*classDef\s").unwrap();
}

/// Configuration parsed from in-file directives
#[derive(Debug, Default)]
pub struct ParseConfig {
    pub style: Option<String>,
    pub max_label: Option<usize>,
}

/// Parse result containing graph and any in-file configuration
#[derive(Debug)]
pub struct ParseResult {
    pub graph: Graph,
    pub config: ParseConfig,
}

/// Find the first meaningful line (non-blank, non-config comment) and its index
fn first_meaningful_line<'a>(lines: &[&'a str]) -> Option<(usize, &'a str)> {
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if RE_CONFIG.is_match(trimmed) || RE_COMMENT.is_match(trimmed) {
            continue;
        }
        return Some((i, trimmed));
    }
    None
}

/// Collect node definitions matching a specific shape regex.
///
/// Extracts node IDs and labels from regex captures, avoiding duplicates.
/// First shape to match a node ID wins (order of calls matters).
#[allow(clippy::too_many_arguments)]
fn collect_shape_nodes(
    trimmed: &str,
    regex: &Regex,
    shape: NodeShape,
    node_labels: &mut HashMap<String, String>,
    node_shapes: &mut HashMap<String, NodeShape>,
    known_ids: &mut HashSet<String>,
    ordered_ids: &mut Vec<String>,
    node_first_ref: &mut HashMap<String, usize>,
    line_num: usize,
) {
    for caps in regex.captures_iter(trimmed) {
        let id = caps[1].to_string();
        let label = caps[2].to_string();
        // Don't overwrite if already defined (earlier shapes have priority)
        if !node_labels.contains_key(&id) {
            if known_ids.insert(id.clone()) {
                ordered_ids.push(id.clone());
            }
            node_labels.insert(id.clone(), label);
            node_shapes.insert(id.clone(), shape);
        }
        node_first_ref.entry(id).or_insert(line_num);
    }
}

/// Parse Mermaid content into a Graph
///
/// Uses two-pass parsing:
/// - Pass 1: Collect all node identifiers
/// - Pass 2: Build graph with validation
///
/// # Arguments
/// * `input` - Mermaid flowchart content
/// * `strict` - If true, exit on any warning (except INFORMATIONAL)
pub fn parse(input: &str, strict: bool) -> Result<ParseResult> {
    // FATAL: Empty file
    if input.trim().is_empty() {
        bail!("termiflow: error: Empty file (no nodes)");
    }

    let lines: Vec<&str> = input.lines().collect();
    let mut graph = Graph::new();
    let mut config = ParseConfig::default();
    let mut direction_found = false;
    let mut direction_line = 0;
    let mut warnings: Vec<String> = Vec::new();

    // Track lines that already emitted warnings to avoid double-reporting
    let mut unsupported_lines: HashSet<usize> = HashSet::new();
    let mut malformed_lines: HashSet<usize> = HashSet::new();

    // Early diagram type detection: first meaningful line must be flowchart ("graph ...")
    if let Some((line_num, first_content)) = first_meaningful_line(&lines) {
        if let Some(caps) = RE_DIAGRAM_TYPE.captures(first_content) {
            let keyword = &caps[1];
            bail!(
                "termiflow: error: line {}: diagram type not supported (found: '{}') — only flowchart `graph TD/LR/TB/BT` is supported",
                line_num + 1,
                keyword
            );
        }
    }

    // Pre-scan for direction (must appear before nodes/edges)
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Skip blank lines
        if trimmed.is_empty() {
            continue;
        }

        // Check for termiflow config (allowed anywhere) - must check BEFORE comments
        if let Some(caps) = RE_CONFIG.captures(trimmed) {
            parse_config_directive(&caps, &mut config);
            continue;
        }

        // Skip regular comments (after checking for config)
        if RE_COMMENT.is_match(trimmed) {
            continue;
        }

        // Check for direction
        if let Some(caps) = RE_DIRECTION.captures(trimmed) {
            if direction_found {
                // SPEC §1.6: Multiple graph lines - first wins
                warnings.push(format!(
                    "termiflow: warning: line {}: Multiple graph directions, using first (line {})",
                    i + 1,
                    direction_line + 1
                ));
                if strict {
                    bail!("{}", warnings.last().unwrap());
                }
            } else {
                graph.direction = match &caps[1] {
                    "TD" | "TB" => Direction::TD,
                    "LR" => Direction::LR,
                    "RL" => Direction::RL,
                    "BT" => Direction::BT,
                    _ => Direction::TD,
                };
                direction_found = true;
                direction_line = i;
            }
            continue;
        }

        // Content before direction
        if !direction_found {
            // Check if this is actual content (node/edge)
            if RE_NODE.is_match(trimmed) || RE_EDGE.is_match(trimmed) {
                warnings.push(format!(
                    "termiflow: warning: line {}: Content before graph direction",
                    i + 1
                ));
                if strict {
                    bail!("{}", warnings.last().unwrap());
                }
            }
        }
    }

    // FATAL: No direction found
    if !direction_found {
        bail!("termiflow: error: No graph direction found (expected 'graph TD/LR/TB/BT')");
    }

    // PASS 1: Collect all node identifiers
    let mut ordered_ids: Vec<String> = Vec::new();
    let mut known_ids: HashSet<String> = HashSet::new();
    let mut node_labels: HashMap<String, String> = HashMap::new();
    let mut node_shapes: HashMap<String, NodeShape> = HashMap::new();
    let mut node_first_ref: HashMap<String, usize> = HashMap::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Skip non-content lines (check config before comment since %% matches both)
        if trimmed.is_empty()
            || RE_CONFIG.is_match(trimmed)
            || RE_COMMENT.is_match(trimmed)
            || RE_DIRECTION.is_match(trimmed)
        {
            continue;
        }

        // Check for unsupported syntax (SPEC §1.2)
        if let Some(warning) = check_unsupported_syntax(trimmed, i + 1) {
            warnings.push(warning.clone());
            unsupported_lines.insert(i);
            if strict {
                bail!("{}", warning);
            }
            continue;
        }

        // Malformed syntax: warn and skip parsing this line
        if let Some(warning) = check_malformed(trimmed, i + 1) {
            warnings.push(warning.clone());
            malformed_lines.insert(i);
            if strict {
                bail!("{}", warning);
            }
            continue;
        }

        // Collect node definitions with shapes - order matters! More specific first
        // Shape regexes ordered from most specific to least (Rectangle must be last)
        let shape_patterns: &[(&Regex, NodeShape)] = &[
            (&RE_NODE_DB, NodeShape::Database),           // ID[(label)]
            (&RE_NODE_SUBROUTINE, NodeShape::Subroutine), // ID[[label]]
            (&RE_NODE_STADIUM, NodeShape::Stadium),       // ID([label])
            (&RE_NODE_CIRCLE, NodeShape::Circle),         // ID((label))
            (&RE_NODE_HEXAGON, NodeShape::Hexagon),       // ID{{label}}
            (&RE_NODE_DIAMOND, NodeShape::Diamond),       // ID{label}
            (&RE_NODE_ROUNDED, NodeShape::Rounded),       // ID(label)
            (&RE_NODE_ASYMMETRIC, NodeShape::Asymmetric), // ID>label]
            (&RE_NODE, NodeShape::Rectangle),             // ID[label] - default, must be last
        ];

        for (regex, shape) in shape_patterns {
            collect_shape_nodes(
                trimmed,
                regex,
                *shape,
                &mut node_labels,
                &mut node_shapes,
                &mut known_ids,
                &mut ordered_ids,
                &mut node_first_ref,
                i + 1,
            );
        }

        // Note: labeled edges are parsed below and labels are preserved

        // Collect edge endpoints (handle chains like A --> B --> C), including labeled variants
        let mut start_pos = 0;
        while start_pos < trimmed.len() {
            let remaining = &trimmed[start_pos..];

            if let Some(caps) = RE_EDGE_WITH_LABEL.captures(remaining) {
                let from = caps[1].to_string();
                let to = caps[3].to_string();
                if known_ids.insert(from.clone()) {
                    ordered_ids.push(from.clone());
                }
                if known_ids.insert(to.clone()) {
                    ordered_ids.push(to.clone());
                }
                node_first_ref.entry(from).or_insert(i + 1);
                node_first_ref.entry(to).or_insert(i + 1);
                start_pos += caps.get(3).unwrap().start(); // Advance to 'to' node for chain parsing
                continue;
            }

            if let Some(caps) = RE_EDGE_TEXT_LABEL.captures(remaining) {
                let from = caps[1].to_string();
                let to = caps[3].to_string();
                if known_ids.insert(from.clone()) {
                    ordered_ids.push(from.clone());
                }
                if known_ids.insert(to.clone()) {
                    ordered_ids.push(to.clone());
                }
                node_first_ref.entry(from).or_insert(i + 1);
                node_first_ref.entry(to).or_insert(i + 1);
                start_pos += caps.get(3).unwrap().start(); // Advance to 'to' node for chain parsing
                continue;
            }

            if let Some(caps) = RE_EDGE.captures(remaining) {
                let from = caps[1].to_string();
                let to = caps[2].to_string();
                if known_ids.insert(from.clone()) {
                    ordered_ids.push(from.clone());
                }
                if known_ids.insert(to.clone()) {
                    ordered_ids.push(to.clone());
                }
                node_first_ref.entry(from).or_insert(i + 1);
                node_first_ref.entry(to).or_insert(i + 1);
                start_pos += caps.get(2).unwrap().start(); // Advance to 'to' node for chain parsing
                continue;
            }

            break;
        }
    }

    // PASS 2: Build graph with auto-create for missing labels
    let mut click_targets: HashMap<String, String> = HashMap::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Skip non-content lines and already-warned unsupported syntax
        // (check config before comment since %% matches both)
        if trimmed.is_empty()
            || RE_CONFIG.is_match(trimmed)
            || RE_COMMENT.is_match(trimmed)
            || RE_DIRECTION.is_match(trimmed)
            || unsupported_lines.contains(&i)
            || malformed_lines.contains(&i)
        {
            continue;
        }

        // Collect click targets
        for caps in RE_CLICK.captures_iter(trimmed) {
            let id = caps[1].to_string();
            let target = caps[2].to_string();
            click_targets.insert(id, target);
        }

        // Collect edges (handle chains like A --> B --> C), preserving labels
        let mut start_pos = 0;
        while start_pos < trimmed.len() {
            let remaining = &trimmed[start_pos..];

            // Try labeled edges first (pipe style: -->|label|)
            if let Some(caps) = RE_EDGE_WITH_LABEL.captures(remaining) {
                let from = caps[1].to_string();
                let label = caps[2].trim().to_string();
                let to = caps[3].to_string();
                graph.edges.push(Edge {
                    from,
                    to,
                    label: Some(label),
                    is_back_edge: false,
                });
                start_pos += caps.get(3).unwrap().start();
                continue;
            }

            // Try text-style labels (-- label -->)
            if let Some(caps) = RE_EDGE_TEXT_LABEL.captures(remaining) {
                let from = caps[1].to_string();
                let label = caps[2].trim().to_string();
                let to = caps[3].to_string();
                graph.edges.push(Edge {
                    from,
                    to,
                    label: Some(label),
                    is_back_edge: false,
                });
                start_pos += caps.get(3).unwrap().start();
                continue;
            }

            // Try unlabeled edges
            if let Some(caps) = RE_EDGE.captures(remaining) {
                let from = caps[1].to_string();
                let to = caps[2].to_string();
                graph.edges.push(Edge {
                    from,
                    to,
                    label: None,
                    is_back_edge: false,
                });
                start_pos += caps.get(2).unwrap().start();
                continue;
            }

            break;
        }
    }

    // Create nodes (auto-create missing labels with INFORMATIONAL warning)
    for id in &ordered_ids {
        let label = if let Some(l) = node_labels.get(id) {
            l.clone()
        } else {
            // SPEC §1.5: Auto-create warning (INFORMATIONAL - never fatal)
            let line_num = node_first_ref.get(id).unwrap_or(&0);
            let warning = format!(
                "termiflow: warning: line {}: Node '{}' referenced but never defined, using ID as label",
                line_num, id
            );
            warnings.push(warning);
            id.clone()
        };

        // Get shape from detected shapes, default to Rectangle for undefined nodes
        let shape = node_shapes.get(id).copied().unwrap_or(NodeShape::Rectangle);

        graph.nodes.push(Node {
            id: id.clone(),
            label,
            shape,
            click_target: click_targets.get(id).cloned(),
            x: 0,
            y: 0,
            width: 0,
            rank: 0,
        });
    }

    // Store warnings in graph
    graph.warnings = warnings;

    // FATAL: No nodes after parsing
    if graph.nodes.is_empty() {
        bail!("termiflow: error: Empty file (no nodes)");
    }

    Ok(ParseResult { graph, config })
}

/// Parse termiflow config directive
fn parse_config_directive(caps: &regex::Captures, config: &mut ParseConfig) {
    let key = &caps[1];
    let value = &caps[2];

    match key {
        "style" => config.style = Some(value.to_string()),
        "max_label" | "maxlabel" => {
            if let Ok(n) = value.parse::<usize>() {
                config.max_label = Some(n);
            }
        }
        _ => {} // Ignore unknown config keys
    }
}

/// Check for unsupported Mermaid syntax (SPEC §1.2)
/// Returns warning message if unsupported syntax found
fn check_unsupported_syntax(line: &str, line_num: usize) -> Option<String> {
    // Check for nested brackets, but allow subroutine [[label]] and database [(label)]
    if RE_NESTED_BRACKET.is_match(line)
        && !RE_NODE_SUBROUTINE.is_match(line)
        && !RE_NODE_DB.is_match(line)
    {
        return Some(format!(
            "termiflow: warning: line {}: Nested brackets not supported in node labels",
            line_num
        ));
    }

    if RE_PIPE_IN_LABEL.is_match(line) {
        return Some(format!(
            "termiflow: warning: line {}: Pipe character not supported in node labels (reserved for edge labels)",
            line_num
        ));
    }

    if RE_SUBGRAPH.is_match(line) {
        return Some(format!(
            "termiflow: warning: line {}: Subgraphs not supported in v1",
            line_num
        ));
    }

    if RE_STYLE.is_match(line) {
        return Some(format!(
            "termiflow: warning: line {}: Mermaid styling not supported (use termiflow: directive)",
            line_num
        ));
    }

    if RE_CLASSDEF.is_match(line) {
        return Some(format!(
            "termiflow: warning: line {}: Mermaid classes not supported",
            line_num
        ));
    }

    None
}

/// Detect malformed but supported-looking syntax (not matching expected regexes)
/// Returns warning message if malformed syntax found
fn check_malformed(line: &str, line_num: usize) -> Option<String> {
    // Edge arrow present but doesn't match any supported edge syntax
    if line.contains("-->")
        && !RE_EDGE.is_match(line)
        && !RE_EDGE_WITH_LABEL.is_match(line)
        && !RE_EDGE_TEXT_LABEL.is_match(line)
    {
        return Some(format!(
            "termiflow: warning: line {}: Malformed edge '{}'",
            line_num, line
        ));
    }

    // Node-like brackets/braces/parens but not a valid node pattern
    // Check all shape patterns
    let has_node_delimiter =
        line.contains('[') || line.contains('{') || line.contains('(') || line.contains('>');
    let matches_any_shape = RE_NODE_DB.is_match(line)
        || RE_NODE_SUBROUTINE.is_match(line)
        || RE_NODE_STADIUM.is_match(line)
        || RE_NODE_CIRCLE.is_match(line)
        || RE_NODE_HEXAGON.is_match(line)
        || RE_NODE_DIAMOND.is_match(line)
        || RE_NODE_ROUNDED.is_match(line)
        || RE_NODE_ASYMMETRIC.is_match(line)
        || RE_NODE_PARALLELOGRAM.is_match(line)
        || RE_NODE_TRAPEZOID.is_match(line)
        || RE_NODE.is_match(line);

    if has_node_delimiter
        && !matches_any_shape
        && !RE_EDGE.is_match(line)
        && !RE_EDGE_WITH_LABEL.is_match(line)
        && !RE_EDGE_TEXT_LABEL.is_match(line)
    {
        return Some(format!(
            "termiflow: warning: line {}: Malformed node '{}'",
            line_num, line
        ));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // === FATAL ERROR TESTS ===

    #[test]
    fn test_empty_input_fails() {
        let result = parse("", false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Empty file"));
    }

    #[test]
    fn test_whitespace_only_fails() {
        let result = parse("   \n\n   ", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_no_direction_fails() {
        let result = parse("A[Node] --> B[Other]", false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No graph direction"));
    }

    #[test]
    fn test_unsupported_diagram_type_sequence() {
        let result = parse("sequenceDiagram\nA->>B: hi", false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("diagram type not supported"));
    }

    // === DIRECTION PARSING ===

    #[test]
    fn test_direction_td() {
        let result = parse("graph TD\nA[Node]", false).unwrap();
        assert!(matches!(result.graph.direction, Direction::TD));
    }

    #[test]
    fn test_direction_tb() {
        let result = parse("graph TB\nA[Node]", false).unwrap();
        assert!(matches!(result.graph.direction, Direction::TD));
    }

    #[test]
    fn test_direction_lr() {
        let result = parse("graph LR\nA[Node]", false).unwrap();
        assert!(matches!(result.graph.direction, Direction::LR));
    }

    #[test]
    fn test_direction_bt() {
        let result = parse("graph BT\nA[Node]", false).unwrap();
        assert!(matches!(result.graph.direction, Direction::BT));
    }

    // === NODE PARSING ===

    #[test]
    fn test_single_node() {
        let result = parse("graph TD\nA[Gateway]", false).unwrap();
        assert_eq!(result.graph.nodes.len(), 1);
        assert_eq!(result.graph.nodes[0].id, "A");
        assert_eq!(result.graph.nodes[0].label, "Gateway");
    }

    #[test]
    fn test_multiple_nodes() {
        let input = "graph TD\nA[First]\nB[Second]\nC[Third]";
        let result = parse(input, false).unwrap();
        assert_eq!(result.graph.nodes.len(), 3);
    }

    #[test]
    fn test_database_node() {
        let result = parse("graph TD\nDB[(Database)]", false).unwrap();
        assert_eq!(result.graph.nodes.len(), 1);
        assert_eq!(result.graph.nodes[0].label, "Database");
    }

    #[test]
    fn test_node_with_spaces_in_label() {
        let result = parse("graph TD\nA[My Long Label]", false).unwrap();
        assert_eq!(result.graph.nodes[0].label, "My Long Label");
    }

    // === EDGE PARSING ===

    #[test]
    fn test_single_edge() {
        let input = "graph TD\nA[Start] --> B[End]";
        let result = parse(input, false).unwrap();
        assert_eq!(result.graph.edges.len(), 1);
        assert_eq!(result.graph.edges[0].from, "A");
        assert_eq!(result.graph.edges[0].to, "B");
    }

    #[test]
    fn test_multiple_edges() {
        let input = "graph TD\nA[A] --> B[B]\nB --> C[C]\nA --> C";
        let result = parse(input, false).unwrap();
        assert_eq!(result.graph.edges.len(), 3);
    }

    #[test]
    fn test_edge_with_long_arrow() {
        let input = "graph TD\nA[A] ---> B[B]";
        let result = parse(input, false).unwrap();
        assert_eq!(result.graph.edges.len(), 1);
    }

    // === FORWARD REFERENCE (TWO-PASS) ===

    #[test]
    fn test_forward_reference() {
        // B is referenced before defined
        let input = "graph TD\nA[Start] --> B\nB[End]";
        let result = parse(input, false).unwrap();
        assert_eq!(result.graph.nodes.len(), 2);
        // B should have its label from definition
        let b_node = result.graph.nodes.iter().find(|n| n.id == "B").unwrap();
        assert_eq!(b_node.label, "End");
    }

    #[test]
    fn test_undefined_node_auto_create() {
        // C is never defined, should auto-create with ID as label
        let input = "graph TD\nA[Start] --> B[Middle] --> C";
        let result = parse(input, false).unwrap();
        assert_eq!(result.graph.nodes.len(), 3);
        let c_node = result.graph.nodes.iter().find(|n| n.id == "C").unwrap();
        assert_eq!(c_node.label, "C");
        // Should have warning about auto-create
        assert!(result
            .graph
            .warnings
            .iter()
            .any(|w| w.contains("'C' referenced but never defined")));
    }

    // === CLICK TARGETS ===

    #[test]
    fn test_click_target() {
        let input = r#"graph TD
A[Gateway]
click A "gateway.md""#;
        let result = parse(input, false).unwrap();
        assert_eq!(
            result.graph.nodes[0].click_target,
            Some("gateway.md".to_string())
        );
    }

    #[test]
    fn test_click_target_single_quotes() {
        let input = "graph TD\nA[Node]\nclick A 'file.md'";
        let result = parse(input, false).unwrap();
        assert_eq!(
            result.graph.nodes[0].click_target,
            Some("file.md".to_string())
        );
    }

    // === CONFIG DIRECTIVES ===

    #[test]
    fn test_config_style() {
        let input = "graph TD\n%% termiflow: style=unicode\nA[Node]";
        let result = parse(input, false).unwrap();
        assert_eq!(result.config.style, Some("unicode".to_string()));
    }

    #[test]
    fn test_config_max_label() {
        let input = "graph TD\n%% termiflow: max_label=30\nA[Node]";
        let result = parse(input, false).unwrap();
        assert_eq!(result.config.max_label, Some(30));
    }

    // === COMMENTS ===

    #[test]
    fn test_comments_ignored() {
        let input = "graph TD\n%% This is a comment\nA[Node]\n%% Another comment";
        let result = parse(input, false).unwrap();
        assert_eq!(result.graph.nodes.len(), 1);
    }

    // === STRICT MODE ===

    #[test]
    fn test_strict_mode_unsupported_syntax() {
        let input = "graph TD\nsubgraph X\nA[Node]";
        // Lenient mode: should warn but parse
        let lenient = parse(input, false).unwrap();
        assert!(!lenient.graph.warnings.is_empty());

        // Strict mode: should fail
        let strict = parse(input, true);
        assert!(strict.is_err());
    }

    #[test]
    fn test_strict_mode_allows_auto_create() {
        // Auto-create warnings are INFORMATIONAL, not affected by strict
        let input = "graph TD\nA[Start] --> B";
        let result = parse(input, true).unwrap();
        assert_eq!(result.graph.nodes.len(), 2);
        // Warning should still be present
        assert!(result
            .graph
            .warnings
            .iter()
            .any(|w| w.contains("'B' referenced")));
    }

    // === UNSUPPORTED SYNTAX DETECTION ===

    #[test]
    fn test_subgraph_unsupported() {
        let input = "graph TD\nsubgraph X\nA[Node]";
        let result = parse(input, false).unwrap();
        assert!(result
            .graph
            .warnings
            .iter()
            .any(|w| w.contains("Subgraphs not supported")));
    }

    #[test]
    fn test_edge_label_pipe_style() {
        // Pipe-style edge labels should be parsed and preserved
        let input = "graph TD\nA[Start] -->|validate| B[Process]";
        let result = parse(input, false).unwrap();
        assert_eq!(result.graph.edges.len(), 1);
        assert_eq!(result.graph.edges[0].from, "A");
        assert_eq!(result.graph.edges[0].to, "B");
        assert_eq!(result.graph.edges[0].label, Some("validate".to_string()));
    }

    #[test]
    fn test_edge_label_text_style() {
        // Text-style edge labels should be parsed and preserved
        let input = "graph TD\nA[Start] -- process --> B[End]";
        let result = parse(input, false).unwrap();
        assert_eq!(result.graph.edges.len(), 1);
        assert_eq!(result.graph.edges[0].from, "A");
        assert_eq!(result.graph.edges[0].to, "B");
        assert_eq!(result.graph.edges[0].label, Some("process".to_string()));
    }

    #[test]
    fn test_edge_label_multiple() {
        // Multiple labeled edges should preserve all labels
        let input = "graph TD\nA[Start] -->|yes| B[Success]\nA -->|no| C[Retry]";
        let result = parse(input, false).unwrap();
        assert_eq!(result.graph.edges.len(), 2);
        assert_eq!(result.graph.edges[0].label, Some("yes".to_string()));
        assert_eq!(result.graph.edges[1].label, Some("no".to_string()));
    }

    #[test]
    fn test_edge_label_mixed_with_unlabeled() {
        // Both labeled and unlabeled edges should be parsed
        let input = "graph TD\nA --> B\nB -->|done| C";
        let result = parse(input, false).unwrap();
        assert_eq!(result.graph.edges.len(), 2);
        assert_eq!(result.graph.edges[0].from, "A");
        assert_eq!(result.graph.edges[0].to, "B");
        assert!(result.graph.edges[0].label.is_none()); // Unlabeled
        assert_eq!(result.graph.edges[1].from, "B");
        assert_eq!(result.graph.edges[1].to, "C");
        assert_eq!(result.graph.edges[1].label, Some("done".to_string()));
    }

    #[test]
    fn test_style_unsupported() {
        let input = "graph TD\nA[Node]\nstyle A fill:#f00";
        let result = parse(input, false).unwrap();
        assert!(result
            .graph
            .warnings
            .iter()
            .any(|w| w.contains("Mermaid styling not supported")));
    }

    // === MULTIPLE GRAPH DIRECTIONS ===

    #[test]
    fn test_multiple_directions_warns() {
        let input = "graph TD\nA[A]\ngraph LR\nB[B]";
        let result = parse(input, false).unwrap();
        // Should use first direction (TD)
        assert!(matches!(result.graph.direction, Direction::TD));
        // Should have warning
        assert!(result
            .graph
            .warnings
            .iter()
            .any(|w| w.contains("Multiple graph directions")));
    }

    // === EDGE CHAIN TESTS ===

    #[test]
    fn test_edge_chain_simple() {
        let input = "graph TD\nA --> B --> C --> D";
        let result = parse(input, false).unwrap();
        assert_eq!(result.graph.edges.len(), 3);
        assert_eq!(result.graph.edges[0].from, "A");
        assert_eq!(result.graph.edges[0].to, "B");
        assert_eq!(result.graph.edges[1].from, "B");
        assert_eq!(result.graph.edges[1].to, "C");
        assert_eq!(result.graph.edges[2].from, "C");
        assert_eq!(result.graph.edges[2].to, "D");
    }

    #[test]
    fn test_edge_chain_with_inline_labels() {
        // Test chains where nodes have labels defined inline
        let input = "graph TD\nA[Start] --> B[Middle] --> C[End]";
        let result = parse(input, false).unwrap();
        assert_eq!(result.graph.edges.len(), 2);
        assert_eq!(result.graph.nodes.len(), 3);
        // Verify labels were captured
        let b_node = result.graph.nodes.iter().find(|n| n.id == "B").unwrap();
        assert_eq!(b_node.label, "Middle");
    }

    #[test]
    fn test_edge_chain_mixed_definitions() {
        // Mix of inline and separate definitions
        let input = "graph TD\nA --> B[Process] --> C\nC[Output]";
        let result = parse(input, false).unwrap();
        assert_eq!(result.graph.edges.len(), 2);
        let c_node = result.graph.nodes.iter().find(|n| n.id == "C").unwrap();
        assert_eq!(c_node.label, "Output");
    }

    // === NODE SHAPE TESTS ===

    #[test]
    fn test_node_shape_rectangle() {
        let result = parse("graph TD\nA[Rectangle]", false).unwrap();
        assert_eq!(result.graph.nodes[0].shape, NodeShape::Rectangle);
        assert_eq!(result.graph.nodes[0].label, "Rectangle");
    }

    #[test]
    fn test_node_shape_rounded() {
        let result = parse("graph TD\nA(Rounded)", false).unwrap();
        assert_eq!(result.graph.nodes[0].shape, NodeShape::Rounded);
        assert_eq!(result.graph.nodes[0].label, "Rounded");
    }

    #[test]
    fn test_node_shape_diamond() {
        let result = parse("graph TD\nA{Decision}", false).unwrap();
        assert_eq!(result.graph.nodes[0].shape, NodeShape::Diamond);
        assert_eq!(result.graph.nodes[0].label, "Decision");
    }

    #[test]
    fn test_node_shape_circle() {
        let result = parse("graph TD\nA((Circle))", false).unwrap();
        assert_eq!(result.graph.nodes[0].shape, NodeShape::Circle);
        assert_eq!(result.graph.nodes[0].label, "Circle");
    }

    #[test]
    fn test_node_shape_stadium() {
        let result = parse("graph TD\nA([Stadium])", false).unwrap();
        assert_eq!(result.graph.nodes[0].shape, NodeShape::Stadium);
        assert_eq!(result.graph.nodes[0].label, "Stadium");
    }

    #[test]
    fn test_node_shape_hexagon() {
        let result = parse("graph TD\nA{{Hexagon}}", false).unwrap();
        assert_eq!(result.graph.nodes[0].shape, NodeShape::Hexagon);
        assert_eq!(result.graph.nodes[0].label, "Hexagon");
    }

    #[test]
    fn test_node_shape_database() {
        let result = parse("graph TD\nDB[(Database)]", false).unwrap();
        assert_eq!(result.graph.nodes[0].shape, NodeShape::Database);
        assert_eq!(result.graph.nodes[0].label, "Database");
    }

    #[test]
    fn test_node_shape_subroutine() {
        let result = parse("graph TD\nA[[Subroutine]]", false).unwrap();
        assert_eq!(result.graph.nodes[0].shape, NodeShape::Subroutine);
        assert_eq!(result.graph.nodes[0].label, "Subroutine");
    }

    #[test]
    fn test_node_shape_asymmetric() {
        let result = parse("graph TD\nA>Flag]", false).unwrap();
        assert_eq!(result.graph.nodes[0].shape, NodeShape::Asymmetric);
        assert_eq!(result.graph.nodes[0].label, "Flag");
    }

    #[test]
    fn test_node_shapes_mixed() {
        let input = "graph TD\nA[Rectangle]\nB(Rounded)\nC{Diamond}\nD[(Database)]";
        let result = parse(input, false).unwrap();
        assert_eq!(result.graph.nodes.len(), 4);

        let a = result.graph.nodes.iter().find(|n| n.id == "A").unwrap();
        assert_eq!(a.shape, NodeShape::Rectangle);

        let b = result.graph.nodes.iter().find(|n| n.id == "B").unwrap();
        assert_eq!(b.shape, NodeShape::Rounded);

        let c = result.graph.nodes.iter().find(|n| n.id == "C").unwrap();
        assert_eq!(c.shape, NodeShape::Diamond);

        let d = result.graph.nodes.iter().find(|n| n.id == "D").unwrap();
        assert_eq!(d.shape, NodeShape::Database);
    }

    #[test]
    fn test_node_shapes_with_edges() {
        let input = "graph TD\nA{Decision} --> B((Success))\nA --> C[Failure]";
        let result = parse(input, false).unwrap();
        assert_eq!(result.graph.nodes.len(), 3);
        assert_eq!(result.graph.edges.len(), 2);

        let a = result.graph.nodes.iter().find(|n| n.id == "A").unwrap();
        assert_eq!(a.shape, NodeShape::Diamond);

        let b = result.graph.nodes.iter().find(|n| n.id == "B").unwrap();
        assert_eq!(b.shape, NodeShape::Circle);

        let c = result.graph.nodes.iter().find(|n| n.id == "C").unwrap();
        assert_eq!(c.shape, NodeShape::Rectangle);
    }

    #[test]
    fn test_undefined_node_default_rectangle() {
        // Undefined nodes (from edges) should default to Rectangle
        let input = "graph TD\nA --> B";
        let result = parse(input, false).unwrap();
        assert_eq!(result.graph.nodes[0].shape, NodeShape::Rectangle);
        assert_eq!(result.graph.nodes[1].shape, NodeShape::Rectangle);
    }
}
