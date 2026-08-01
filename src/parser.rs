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

use crate::graph::{Direction, Edge, EdgeKind, Graph, Node, NodeShape, Subgraph};
use crate::spacing::SpacingMode;

lazy_static! {
    // SPEC §1.1: Supported syntax patterns
    // Accept both Mermaid flowchart headers:
    // - `graph TD` (legacy)
    // - `flowchart TD` (common generator output)
    static ref RE_DIRECTION: Regex =
        Regex::new(r"^(?:graph|flowchart)\s+(TD|LR|RL|TB|BT)\b").unwrap();

    // Node shape regexes - order matters! More specific patterns first
    // Database: ID[(label)]
    static ref RE_NODE_DB: Regex = Regex::new(r"([a-zA-Z0-9_]+)\[\(([^\)]*)\)\]").unwrap();
    // Subroutine: ID[[label]]
    static ref RE_NODE_SUBROUTINE: Regex = Regex::new(r"([a-zA-Z0-9_]+)\[\[([^\]]*)\]\]").unwrap();
    // Stadium: ID([label])
    static ref RE_NODE_STADIUM: Regex = Regex::new(r"([a-zA-Z0-9_]+)\(\[([^\]]*)\]\)").unwrap();
    // Double circle: ID(((label))) — must come before Circle to avoid ambiguous match
    static ref RE_NODE_DOUBLE_CIRCLE: Regex = Regex::new(r"([a-zA-Z0-9_]+)\(\(\(([^)]*)\)\)\)").unwrap();
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
    // Parallelogram: ID[/label/]  (lean right — both slashes forward)
    static ref RE_NODE_PARALLELOGRAM: Regex = Regex::new(r"([a-zA-Z0-9_]+)\[/([^/\\]*)/\]").unwrap();
    // ParallelogramAlt: ID[\label\]  (lean left — both slashes backward)
    static ref RE_NODE_PARALLELOGRAM_ALT: Regex = Regex::new(r"([a-zA-Z0-9_]+)\[\\([^/\\]*)\\\]").unwrap();
    // Trapezoid: ID[/label\]  (wider top)
    static ref RE_NODE_TRAPEZOID: Regex = Regex::new(r"([a-zA-Z0-9_]+)\[/([^/\\]*)\\\]").unwrap();
    // TrapezoidAlt: ID[\label/]  (wider bottom)
    static ref RE_NODE_TRAPEZOID_ALT: Regex = Regex::new(r"([a-zA-Z0-9_]+)\[\\([^/\\]*)\/\]").unwrap();
    // Rectangle: ID[label] - default, must be last
    static ref RE_NODE: Regex = Regex::new(r"([a-zA-Z0-9_]+)\[([^\[\]]*)\]").unwrap();
    // Bare node reference: `A`
    static ref RE_NODE_REF: Regex = Regex::new(r"^([a-zA-Z0-9_]+)$").unwrap();

    // Grouped edge (& syntax): A & B --> C & D  or  A & B -->|label| C & D
    // Group IDs are plain identifiers only (no inline shape syntax in & groups).
    static ref RE_EDGE_GROUP_LABEL: Regex = Regex::new(
        r"([a-zA-Z0-9_]+(?:\s*&\s*[a-zA-Z0-9_]+)*)\s*--+>\s*\|([^|]+)\|\s*([a-zA-Z0-9_]+(?:\s*&\s*[a-zA-Z0-9_]+)*)"
    ).unwrap();
    static ref RE_EDGE_GROUP_PLAIN: Regex = Regex::new(
        r"([a-zA-Z0-9_]+(?:\s*&\s*[a-zA-Z0-9_]+)*)\s*--+>\s*([a-zA-Z0-9_]+(?:\s*&\s*[a-zA-Z0-9_]+)*)"
    ).unwrap();

    // Edge regex - handles optional shape syntax after node IDs
    static ref RE_EDGE: Regex = Regex::new(r"([a-zA-Z0-9_]+)(?:\[[^\]]*\]|\(\(\([^\)]*\)\)\)|\(\([^\)]*\)\)|\([^\)]*\)|\{[^\}]*\}|>[^\]]*\])?\s*--+>\s*([a-zA-Z0-9_]+)").unwrap();
    // Edge with pipe-style label: A -->|label| B
    static ref RE_EDGE_WITH_LABEL: Regex = Regex::new(
        r"([a-zA-Z0-9_]+)(?:\[[^\]]*\]|\(\(\([^\)]*\)\)\)|\(\([^\)]*\)\)|\([^\)]*\)|\{[^\}]*\}|>[^\]]*\])?\s*--+>\s*\|([^|]+)\|\s*([a-zA-Z0-9_]+)"
    ).unwrap();
    // Edge with text-style label: A -- label --> B
    static ref RE_EDGE_TEXT_LABEL: Regex = Regex::new(
        r"([a-zA-Z0-9_]+)(?:\[[^\]]*\]|\(\(\([^\)]*\)\)\)|\(\([^\)]*\)\)|\([^\)]*\)|\{[^\}]*\}|>[^\]]*\])?\s*--\s+([^-]+?)\s+--+>\s*([a-zA-Z0-9_]+)"
    ).unwrap();

    // Open link: A --- B (no arrowhead)
    static ref RE_EDGE_OPEN: Regex = Regex::new(r"([a-zA-Z0-9_]+)(?:\[[^\]]*\]|\(\(\([^\)]*\)\)\)|\(\([^\)]*\)\)|\([^\)]*\)|\{[^\}]*\}|>[^\]]*\])?\s*---\s*([a-zA-Z0-9_]+)").unwrap();
    // Open link with pipe label: A ---|label| B
    static ref RE_EDGE_OPEN_WITH_LABEL: Regex = Regex::new(
        r"([a-zA-Z0-9_]+)(?:\[[^\]]*\]|\(\(\([^\)]*\)\)\)|\(\([^\)]*\)\)|\([^\)]*\)|\{[^\}]*\}|>[^\]]*\])?\s*---\s*\|([^|]+)\|\s*([a-zA-Z0-9_]+)"
    ).unwrap();

    // Thick edge: A ==> B
    static ref RE_EDGE_THICK: Regex = Regex::new(r"([a-zA-Z0-9_]+)(?:\[[^\]]*\]|\(\(\([^\)]*\)\)\)|\(\([^\)]*\)\)|\([^\)]*\)|\{[^\}]*\}|>[^\]]*\])?\s*==+>\s*([a-zA-Z0-9_]+)").unwrap();
    // Thick edge with pipe label: A ==>|label| B
    static ref RE_EDGE_THICK_WITH_LABEL: Regex = Regex::new(
        r"([a-zA-Z0-9_]+)(?:\[[^\]]*\]|\(\(\([^\)]*\)\)\)|\(\([^\)]*\)\)|\([^\)]*\)|\{[^\}]*\}|>[^\]]*\])?\s*==+>\s*\|([^|]+)\|\s*([a-zA-Z0-9_]+)"
    ).unwrap();

    // Dotted edge: A -.-> B
    static ref RE_EDGE_DOTTED: Regex = Regex::new(r"([a-zA-Z0-9_]+)(?:\[[^\]]*\]|\(\(\([^\)]*\)\)\)|\(\([^\)]*\)\)|\([^\)]*\)|\{[^\}]*\}|>[^\]]*\])?\s*-\.->\s*([a-zA-Z0-9_]+)").unwrap();
    // Dotted edge with pipe label: A -.->|label| B
    static ref RE_EDGE_DOTTED_WITH_LABEL: Regex = Regex::new(
        r"([a-zA-Z0-9_]+)(?:\[[^\]]*\]|\(\(\([^\)]*\)\)\)|\(\([^\)]*\)\)|\([^\)]*\)|\{[^\}]*\}|>[^\]]*\])?\s*-\.->\s*\|([^|]+)\|\s*([a-zA-Z0-9_]+)"
    ).unwrap();

    // Bidirectional edge: A <--> B
    static ref RE_EDGE_BIDIR: Regex = Regex::new(r"([a-zA-Z0-9_]+)(?:\[[^\]]*\]|\(\(\([^\)]*\)\)\)|\(\([^\)]*\)\)|\([^\)]*\)|\{[^\}]*\}|>[^\]]*\])?\s*<--+>\s*([a-zA-Z0-9_]+)").unwrap();
    // Bidirectional edge with pipe label: A <-->|label| B
    static ref RE_EDGE_BIDIR_WITH_LABEL: Regex = Regex::new(
        r"([a-zA-Z0-9_]+)(?:\[[^\]]*\]|\(\(\([^\)]*\)\)\)|\(\([^\)]*\)\)|\([^\)]*\)|\{[^\}]*\}|>[^\]]*\])?\s*<--+>\s*\|([^|]+)\|\s*([a-zA-Z0-9_]+)"
    ).unwrap();

    // Circle-end edge: A --o B  (shaft with circle end marker, no arrowhead)
    static ref RE_EDGE_CIRCLE: Regex = Regex::new(r"([a-zA-Z0-9_]+)(?:\[[^\]]*\]|\(\(\([^\)]*\)\)\)|\(\([^\)]*\)\)|\([^\)]*\)|\{[^\}]*\}|>[^\]]*\])?\s*--+o\s*([a-zA-Z0-9_]+)").unwrap();
    // Circle-end edge with pipe label: A --o|label| B
    static ref RE_EDGE_CIRCLE_WITH_LABEL: Regex = Regex::new(
        r"([a-zA-Z0-9_]+)(?:\[[^\]]*\]|\(\(\([^\)]*\)\)\)|\(\([^\)]*\)\)|\([^\)]*\)|\{[^\}]*\}|>[^\]]*\])?\s*--+o\s*\|([^|]+)\|\s*([a-zA-Z0-9_]+)"
    ).unwrap();

    // Cross-end edge: A --x B  (shaft with cross end marker, no arrowhead)
    static ref RE_EDGE_CROSS: Regex = Regex::new(r"([a-zA-Z0-9_]+)(?:\[[^\]]*\]|\(\(\([^\)]*\)\)\)|\(\([^\)]*\)\)|\([^\)]*\)|\{[^\}]*\}|>[^\]]*\])?\s*--+x\s*([a-zA-Z0-9_]+)").unwrap();
    // Cross-end edge with pipe label: A --x|label| B
    static ref RE_EDGE_CROSS_WITH_LABEL: Regex = Regex::new(
        r"([a-zA-Z0-9_]+)(?:\[[^\]]*\]|\(\(\([^\)]*\)\)\)|\(\([^\)]*\)\)|\([^\)]*\)|\{[^\}]*\}|>[^\]]*\])?\s*--+x\s*\|([^|]+)\|\s*([a-zA-Z0-9_]+)"
    ).unwrap();
    static ref RE_CLICK: Regex = Regex::new(r#"click\s+(\w+)\s+["']([^"']+)["']"#).unwrap();
    static ref RE_CONFIG: Regex = Regex::new(r"%%\s*termiflow:\s*(\w+)=([^\s]+)").unwrap();
    static ref RE_COMMENT: Regex = Regex::new(r"^\s*%%").unwrap();
    // Mermaid keywords that identify non-flowchart diagram types. Flowcharts are handled
    // via `RE_DIRECTION` above.
    static ref RE_DIAGRAM_TYPE: Regex = Regex::new(
        r"^(sequenceDiagram|classDiagram|stateDiagram-v2|stateDiagram|erDiagram|journey|gantt|pie|requirementDiagram|timeline|mindmap|gitGraph|block|quadrantChart)\b",
    )
    .unwrap();

    // SPEC §1.2: Unsupported syntax patterns
    static ref RE_NESTED_BRACKET: Regex = Regex::new(r"\[[^\]]*\[").unwrap();
    static ref RE_PIPE_IN_LABEL: Regex = Regex::new(r"\[[^\]]*\|[^\]]*\]").unwrap();
    static ref RE_STYLE: Regex = Regex::new(r"^\s*style\s+\w+").unwrap();
    static ref RE_CLASSDEF: Regex = Regex::new(r"^\s*classDef\s").unwrap();
    /// Mermaid class application suffix: `:::className` or `:::class1,class2`
    /// These are stripped before edge/node regex matching to prevent false positives.
    static ref RE_CLASS_SUFFIX: Regex = Regex::new(r":::[a-zA-Z0-9_,]+").unwrap();

    // Subgraph patterns (single-level supported; nested warns/ignored)
    // subgraph ID [title] or subgraph ID ["title"]
    static ref RE_SUBGRAPH_BRACKET: Regex = Regex::new(r"^\s*subgraph\s+(\w+)\s*\[([^\]]*)\]").unwrap();
    // subgraph title (title becomes sanitized ID)
    static ref RE_SUBGRAPH_PLAIN: Regex = Regex::new(r"^\s*subgraph\s+(.+)$").unwrap();
    // end keyword closes subgraph
    static ref RE_SUBGRAPH_END: Regex = Regex::new(r"^\s*end\s*$").unwrap();
}

/// Configuration parsed from in-file directives
#[derive(Debug, Default)]
pub struct ParseConfig {
    pub style: Option<String>,
    pub max_label: Option<usize>,
    /// Maximum edge label width before truncation.
    pub max_edge_label: Option<usize>,
    /// Enable multiline label wrapping (experimental; default off).
    pub wrap_labels: Option<bool>,
    /// Maximum number of label lines when wrapping is enabled.
    pub max_label_lines: Option<usize>,
    /// Spacing preset (compact/default/spacious).
    pub spacing_mode: Option<SpacingMode>,
    /// Enable the bounded render repair loop.
    pub optimize_render: Option<bool>,
    /// Maximum number of repair passes when render optimization is enabled.
    pub render_repair_passes: Option<usize>,
    /// Maximum number of layout candidate passes when render optimization is enabled.
    pub layout_repair_passes: Option<usize>,
    /// Emit critic findings for the rendered frame.
    pub debug_critic: Option<bool>,
}

/// Parse result containing graph and any in-file configuration
#[derive(Debug)]
pub struct ParseResult {
    pub graph: Graph,
    pub config: ParseConfig,
}

#[derive(Debug, Default)]
struct SubgraphParseData {
    title: Option<String>,
    parent_id: Option<String>,
    child_ids: Vec<String>,
    node_ids: Vec<String>,
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

/// Sanitize a subgraph title into a valid ID
/// "My Subgraph" -> "my_subgraph"
fn sanitize_subgraph_id(title: &str) -> String {
    title
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

/// Decode Mermaid label text: HTML entities and inline markup tags.
///
/// Mermaid renders labels as HTML so it accepts `&amp;`, `&lt;`, `&gt;` etc.
/// and simple inline tags like `<b>`, `<i>`, `<s>`, `<u>`.  TermiFlow
/// renders plain-text, so we decode entities to their literal characters
/// and strip the structural tags while preserving the tag content.
pub(crate) fn decode_mermaid_label(s: &str) -> String {
    // Decode named HTML entities (most common in Mermaid diagrams)
    let s = s
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ");

    // Strip simple paired inline tags that Mermaid allows in labels.
    // We keep the inner text; only the tag delimiters are removed.
    let mut result = s;
    for tag in &["b", "i", "s", "u", "em", "strong", "code"] {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        result = result.replace(&open, "").replace(&close, "");
    }
    result
}

fn associate_node_with_subgraph_data(
    node_to_subgraph: &mut HashMap<String, String>,
    subgraph_data: &mut HashMap<String, SubgraphParseData>,
    node_id: &str,
    subgraph_id: &str,
) {
    if let Some(previous_id) = node_to_subgraph
        .insert(node_id.to_string(), subgraph_id.to_string())
        .filter(|previous_id| previous_id != subgraph_id)
    {
        if let Some(previous) = subgraph_data.get_mut(&previous_id) {
            previous.node_ids.retain(|existing| existing != node_id);
        }
    }

    if let Some(current) = subgraph_data.get_mut(subgraph_id) {
        if !current.node_ids.iter().any(|existing| existing == node_id) {
            current.node_ids.push(node_id.to_string());
        }
    }
}

/// Split an `&`-separated group string into individual trimmed IDs.
/// `"A & B & C"` → `["A", "B", "C"]`
fn split_group(s: &str) -> Vec<String> {
    s.split('&')
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect()
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
    explicit_line_nodes: &mut Vec<String>,
    line_num: usize,
) {
    for caps in regex.captures_iter(trimmed) {
        let id = caps[1].to_string();
        let label = decode_mermaid_label(&caps[2]);
        if !explicit_line_nodes.contains(&id) {
            explicit_line_nodes.push(id.clone());
        }
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
                "termiflow: error: line {}: diagram type not supported (found: '{}') — only flowchart `graph|flowchart TD/LR/TB/BT` is supported",
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

    // Subgraph tracking
    let mut subgraph_stack: Vec<String> = Vec::new();
    let mut subgraph_data: HashMap<String, SubgraphParseData> = HashMap::new();
    let mut subgraph_order: Vec<String> = Vec::new(); // Preserve declaration order
                                                      // node_id -> subgraph_id
    let mut node_to_subgraph: HashMap<String, String> = HashMap::new();

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

        // Handle subgraph start: `subgraph ID [title]` or `subgraph title`
        if let Some(caps) = RE_SUBGRAPH_BRACKET.captures(trimmed) {
            let id = caps[1].to_string();
            let title = caps[2].trim();
            let title = if title.is_empty() {
                None
            } else {
                Some(decode_mermaid_label(title.trim_matches('"')))
            };

            let parent_id = subgraph_stack.last().cloned();
            subgraph_data.insert(
                id.clone(),
                SubgraphParseData {
                    title,
                    parent_id: parent_id.clone(),
                    child_ids: Vec::new(),
                    node_ids: Vec::new(),
                },
            );
            if let Some(parent_id) = parent_id {
                if let Some(parent) = subgraph_data.get_mut(&parent_id) {
                    if !parent.child_ids.iter().any(|existing| existing == &id) {
                        parent.child_ids.push(id.clone());
                    }
                }
            }
            subgraph_order.push(id.clone());
            subgraph_stack.push(id);
            continue;
        }

        if RE_SUBGRAPH_PLAIN.is_match(trimmed) && !RE_SUBGRAPH_BRACKET.is_match(trimmed) {
            if let Some(caps) = RE_SUBGRAPH_PLAIN.captures(trimmed) {
                let title = decode_mermaid_label(caps[1].trim());
                let id = sanitize_subgraph_id(&title);

                let parent_id = subgraph_stack.last().cloned();
                subgraph_data.insert(
                    id.clone(),
                    SubgraphParseData {
                        title: Some(title),
                        parent_id: parent_id.clone(),
                        child_ids: Vec::new(),
                        node_ids: Vec::new(),
                    },
                );
                if let Some(parent_id) = parent_id {
                    if let Some(parent) = subgraph_data.get_mut(&parent_id) {
                        if !parent.child_ids.iter().any(|existing| existing == &id) {
                            parent.child_ids.push(id.clone());
                        }
                    }
                }
                subgraph_order.push(id.clone());
                subgraph_stack.push(id);
                continue;
            }
        }

        // Handle subgraph end: `end`
        if RE_SUBGRAPH_END.is_match(trimmed) {
            let _ = subgraph_stack.pop();
            continue;
        }

        // Warn about class application suffixes (:::className) — we strip them for parsing
        // but class assignments themselves are ignored. Emit warning without skipping the line.
        if RE_CLASS_SUFFIX.is_match(trimmed) {
            let warning = format!(
                "termiflow: warning: line {}: Mermaid class application (:::) not supported, class assignment ignored",
                i + 1
            );
            warnings.push(warning.clone());
            if strict {
                bail!("{}", warning);
            }
            // NOTE: no `continue` — we still parse nodes/edges from this line
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

        // Track nodes discovered on this line (for subgraph membership)
        let nodes_before = ordered_ids.len();
        let mut explicit_line_nodes = Vec::new();

        // Collect node definitions with shapes - order matters! More specific first
        // Shape regexes ordered from most specific to least (Rectangle must be last)
        let shape_patterns: &[(&Regex, NodeShape)] = &[
            (&RE_NODE_DB, NodeShape::Database),                 // ID[(label)]
            (&RE_NODE_SUBROUTINE, NodeShape::Subroutine),       // ID[[label]]
            (&RE_NODE_STADIUM, NodeShape::Stadium),             // ID([label])
            (&RE_NODE_DOUBLE_CIRCLE, NodeShape::DoubleCircle),  // ID(((label)))
            (&RE_NODE_CIRCLE, NodeShape::Circle),               // ID((label))
            (&RE_NODE_HEXAGON, NodeShape::Hexagon),             // ID{{label}}
            (&RE_NODE_DIAMOND, NodeShape::Diamond),             // ID{label}
            (&RE_NODE_ROUNDED, NodeShape::Rounded),             // ID(label)
            (&RE_NODE_ASYMMETRIC, NodeShape::Asymmetric),       // ID>label]
            (&RE_NODE_PARALLELOGRAM, NodeShape::Parallelogram), // ID[/label/]
            (&RE_NODE_PARALLELOGRAM_ALT, NodeShape::ParallelogramAlt), // ID[\label\]
            (&RE_NODE_TRAPEZOID, NodeShape::Trapezoid),         // ID[/label\]
            (&RE_NODE_TRAPEZOID_ALT, NodeShape::TrapezoidAlt),  // ID[\label/]
            (&RE_NODE, NodeShape::Rectangle),                   // ID[label] - default, must be last
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
                &mut explicit_line_nodes,
                i + 1,
            );
        }

        // Bare node references should count as explicit line membership too. This matters
        // inside subgraphs when the node was already introduced on an earlier edge line.
        if explicit_line_nodes.is_empty() {
            if let Some(caps) = RE_NODE_REF.captures(trimmed) {
                let id = caps[1].to_string();
                if known_ids.insert(id.clone()) {
                    ordered_ids.push(id.clone());
                }
                node_first_ref.entry(id.clone()).or_insert(i + 1);
                explicit_line_nodes.push(id);
            }
        }

        // Note: labeled edges are parsed below and labels are preserved

        // Pre-register all IDs from &-grouped edge lines before the chain loop.
        // The chain loop only finds one ID per side (misses the left ID in "A & B --> C").
        if trimmed.contains('&') {
            let register = |id: &str,
                            known: &mut HashSet<String>,
                            ordered: &mut Vec<String>,
                            refs: &mut HashMap<String, usize>| {
                if known.insert(id.to_string()) {
                    ordered.push(id.to_string());
                }
                refs.entry(id.to_string()).or_insert(i + 1);
            };
            if let Some(caps) = RE_EDGE_GROUP_LABEL.captures(trimmed) {
                for id in split_group(&caps[1])
                    .iter()
                    .chain(split_group(&caps[3]).iter())
                {
                    register(id, &mut known_ids, &mut ordered_ids, &mut node_first_ref);
                }
            } else if let Some(caps) = RE_EDGE_GROUP_PLAIN.captures(trimmed) {
                for id in split_group(&caps[1])
                    .iter()
                    .chain(split_group(&caps[2]).iter())
                {
                    register(id, &mut known_ids, &mut ordered_ids, &mut node_first_ref);
                }
            }
        }

        // Collect edge endpoints (handle chains like A --> B --> C), including labeled variants
        // Strip class-application suffixes (:::className) before matching to prevent false
        // node registrations like `A[X]:::cls --> B` being parsed as `cls --> B`.
        let trimmed_no_class: std::borrow::Cow<str> = if RE_CLASS_SUFFIX.is_match(trimmed) {
            RE_CLASS_SUFFIX.replace_all(trimmed, "")
        } else {
            std::borrow::Cow::Borrowed(trimmed)
        };
        let trimmed = trimmed_no_class.as_ref();
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

            // Open links (---), thick (==>), dotted (-.->) bidirectional (<-->), circle/cross ends (--o/--x)
            let mut matched_new_kind = false;
            for re in &[
                &*RE_EDGE_OPEN_WITH_LABEL,
                &*RE_EDGE_OPEN,
                &*RE_EDGE_THICK_WITH_LABEL,
                &*RE_EDGE_THICK,
                &*RE_EDGE_DOTTED_WITH_LABEL,
                &*RE_EDGE_DOTTED,
                &*RE_EDGE_BIDIR_WITH_LABEL,
                &*RE_EDGE_BIDIR,
                &*RE_EDGE_CIRCLE_WITH_LABEL,
                &*RE_EDGE_CIRCLE,
                &*RE_EDGE_CROSS_WITH_LABEL,
                &*RE_EDGE_CROSS,
            ] {
                if let Some(caps) = re.captures(remaining) {
                    let from = caps[1].to_string();
                    let cap_count = caps.len();
                    let to = caps[cap_count - 1].to_string(); // last group is always 'to'
                    if known_ids.insert(from.clone()) {
                        ordered_ids.push(from.clone());
                    }
                    if known_ids.insert(to.clone()) {
                        ordered_ids.push(to.clone());
                    }
                    node_first_ref.entry(from).or_insert(i + 1);
                    node_first_ref.entry(to).or_insert(i + 1);
                    start_pos += caps.get(cap_count - 1).unwrap().start();
                    matched_new_kind = true;
                    break;
                }
            }
            if matched_new_kind {
                continue;
            }

            break;
        }

        // Associate newly discovered nodes with current subgraph
        if let Some(sg_id) = subgraph_stack.last() {
            for node_id in &explicit_line_nodes {
                associate_node_with_subgraph_data(
                    &mut node_to_subgraph,
                    &mut subgraph_data,
                    node_id,
                    sg_id,
                );
            }
            for node_id in &ordered_ids[nodes_before..] {
                if !explicit_line_nodes.contains(node_id) && !node_to_subgraph.contains_key(node_id)
                {
                    associate_node_with_subgraph_data(
                        &mut node_to_subgraph,
                        &mut subgraph_data,
                        node_id,
                        sg_id,
                    );
                }
            }
        }
    }

    // Warn about unclosed subgraph
    if !subgraph_stack.is_empty() {
        warnings.push("termiflow: warning: Unclosed subgraph at end of file".to_string());
        if strict {
            bail!("{}", warnings.last().unwrap());
        }
    }

    // Build Subgraph objects from collected data
    for sg_id in &subgraph_order {
        if let Some(data) = subgraph_data.remove(sg_id) {
            let mut subgraph = Subgraph::new(sg_id.clone(), data.title);
            subgraph.parent_id = data.parent_id;
            for child_id in data.child_ids {
                subgraph.add_child(child_id);
            }
            for node_id in data.node_ids {
                subgraph.add_node(node_id);
            }
            graph.add_subgraph(subgraph);
        }
    }

    // Copy node-to-subgraph mapping
    graph.node_subgraph = node_to_subgraph;

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

        // Expand &-grouped edges before the chain loop.
        // "A & B --> C & D" → (A→C, A→D, B→C, B→D), then skip normal loop for this line.
        if trimmed.contains('&') {
            let mut matched_group = false;
            if let Some(caps) = RE_EDGE_GROUP_LABEL.captures(trimmed) {
                let srcs = split_group(&caps[1]);
                let label = decode_mermaid_label(caps[2].trim());
                let dsts = split_group(&caps[3]);
                if srcs.len() + dsts.len() > 2 {
                    // At least one side has multiple nodes
                    for src in &srcs {
                        for dst in &dsts {
                            graph.edges.push(Edge {
                                from: src.clone(),
                                to: dst.clone(),
                                label: Some(label.clone()),
                                is_back_edge: false,
                                kind: EdgeKind::Arrow,
                            });
                        }
                    }
                    matched_group = true;
                }
            } else if let Some(caps) = RE_EDGE_GROUP_PLAIN.captures(trimmed) {
                let srcs = split_group(&caps[1]);
                let dsts = split_group(&caps[2]);
                if srcs.len() + dsts.len() > 2 {
                    for src in &srcs {
                        for dst in &dsts {
                            graph.edges.push(Edge {
                                from: src.clone(),
                                to: dst.clone(),
                                label: None,
                                is_back_edge: false,
                                kind: EdgeKind::Arrow,
                            });
                        }
                    }
                    matched_group = true;
                }
            }
            if matched_group {
                continue; // skip chain-loop for this line — all edges already pushed
            }
        }

        // Collect edges (handle chains like A --> B --> C), preserving labels
        // Strip class-application suffixes (:::className) before matching so that
        // `A[X]:::cls --> B` is correctly parsed as edge A→B, not as `cls --> B`.
        let trimmed_no_class: std::borrow::Cow<str> = if RE_CLASS_SUFFIX.is_match(trimmed) {
            RE_CLASS_SUFFIX.replace_all(trimmed, "")
        } else {
            std::borrow::Cow::Borrowed(trimmed)
        };
        let trimmed = trimmed_no_class.as_ref();
        let mut start_pos = 0;
        while start_pos < trimmed.len() {
            let remaining = &trimmed[start_pos..];

            // Try labeled edges first (pipe style: -->|label|)
            if let Some(caps) = RE_EDGE_WITH_LABEL.captures(remaining) {
                let from = caps[1].to_string();
                let label = decode_mermaid_label(caps[2].trim());
                let to = caps[3].to_string();
                graph.edges.push(Edge {
                    from,
                    to,
                    label: Some(label),
                    is_back_edge: false,
                    kind: EdgeKind::Arrow,
                });
                start_pos += caps.get(3).unwrap().start();
                continue;
            }

            // Try text-style labels (-- label -->)
            if let Some(caps) = RE_EDGE_TEXT_LABEL.captures(remaining) {
                let from = caps[1].to_string();
                let label = decode_mermaid_label(caps[2].trim());
                let to = caps[3].to_string();
                graph.edges.push(Edge {
                    from,
                    to,
                    label: Some(label),
                    is_back_edge: false,
                    kind: EdgeKind::Arrow,
                });
                start_pos += caps.get(3).unwrap().start();
                continue;
            }

            // Try unlabeled arrows
            if let Some(caps) = RE_EDGE.captures(remaining) {
                let from = caps[1].to_string();
                let to = caps[2].to_string();
                graph.edges.push(Edge {
                    from,
                    to,
                    label: None,
                    is_back_edge: false,
                    kind: EdgeKind::Arrow,
                });
                start_pos += caps.get(2).unwrap().start();
                continue;
            }

            // Open links: A ---|label| B  or  A --- B
            if let Some(caps) = RE_EDGE_OPEN_WITH_LABEL.captures(remaining) {
                let from = caps[1].to_string();
                let label = decode_mermaid_label(caps[2].trim());
                let to = caps[3].to_string();
                graph.edges.push(Edge {
                    from,
                    to,
                    label: Some(label),
                    is_back_edge: false,
                    kind: EdgeKind::Open,
                });
                start_pos += caps.get(3).unwrap().start();
                continue;
            }
            if let Some(caps) = RE_EDGE_OPEN.captures(remaining) {
                let from = caps[1].to_string();
                let to = caps[2].to_string();
                graph.edges.push(Edge {
                    from,
                    to,
                    label: None,
                    is_back_edge: false,
                    kind: EdgeKind::Open,
                });
                start_pos += caps.get(2).unwrap().start();
                continue;
            }

            // Thick edges: A ==>|label| B  or  A ==> B
            if let Some(caps) = RE_EDGE_THICK_WITH_LABEL.captures(remaining) {
                let from = caps[1].to_string();
                let label = decode_mermaid_label(caps[2].trim());
                let to = caps[3].to_string();
                graph.edges.push(Edge {
                    from,
                    to,
                    label: Some(label),
                    is_back_edge: false,
                    kind: EdgeKind::Thick,
                });
                start_pos += caps.get(3).unwrap().start();
                continue;
            }
            if let Some(caps) = RE_EDGE_THICK.captures(remaining) {
                let from = caps[1].to_string();
                let to = caps[2].to_string();
                graph.edges.push(Edge {
                    from,
                    to,
                    label: None,
                    is_back_edge: false,
                    kind: EdgeKind::Thick,
                });
                start_pos += caps.get(2).unwrap().start();
                continue;
            }

            // Dotted edges: A -.->|label| B  or  A -.-> B
            if let Some(caps) = RE_EDGE_DOTTED_WITH_LABEL.captures(remaining) {
                let from = caps[1].to_string();
                let label = decode_mermaid_label(caps[2].trim());
                let to = caps[3].to_string();
                graph.edges.push(Edge {
                    from,
                    to,
                    label: Some(label),
                    is_back_edge: false,
                    kind: EdgeKind::Dotted,
                });
                start_pos += caps.get(3).unwrap().start();
                continue;
            }
            if let Some(caps) = RE_EDGE_DOTTED.captures(remaining) {
                let from = caps[1].to_string();
                let to = caps[2].to_string();
                graph.edges.push(Edge {
                    from,
                    to,
                    label: None,
                    is_back_edge: false,
                    kind: EdgeKind::Dotted,
                });
                start_pos += caps.get(2).unwrap().start();
                continue;
            }

            // Bidirectional edges: A <-->|label| B  or  A <--> B
            if let Some(caps) = RE_EDGE_BIDIR_WITH_LABEL.captures(remaining) {
                let from = caps[1].to_string();
                let label = decode_mermaid_label(caps[2].trim());
                let to = caps[3].to_string();
                graph.edges.push(Edge {
                    from,
                    to,
                    label: Some(label),
                    is_back_edge: false,
                    kind: EdgeKind::Bidirectional,
                });
                start_pos += caps.get(3).unwrap().start();
                continue;
            }
            if let Some(caps) = RE_EDGE_BIDIR.captures(remaining) {
                let from = caps[1].to_string();
                let to = caps[2].to_string();
                graph.edges.push(Edge {
                    from,
                    to,
                    label: None,
                    is_back_edge: false,
                    kind: EdgeKind::Bidirectional,
                });
                start_pos += caps.get(2).unwrap().start();
                continue;
            }

            // Circle-end edges: A --o|label| B  or  A --o B
            if let Some(caps) = RE_EDGE_CIRCLE_WITH_LABEL.captures(remaining) {
                let from = caps[1].to_string();
                let label = decode_mermaid_label(caps[2].trim());
                let to = caps[3].to_string();
                graph.edges.push(Edge {
                    from,
                    to,
                    label: Some(label),
                    is_back_edge: false,
                    kind: EdgeKind::CircleEnd,
                });
                start_pos += caps.get(3).unwrap().start();
                continue;
            }
            if let Some(caps) = RE_EDGE_CIRCLE.captures(remaining) {
                let from = caps[1].to_string();
                let to = caps[2].to_string();
                graph.edges.push(Edge {
                    from,
                    to,
                    label: None,
                    is_back_edge: false,
                    kind: EdgeKind::CircleEnd,
                });
                start_pos += caps.get(2).unwrap().start();
                continue;
            }

            // Cross-end edges: A --x|label| B  or  A --x B
            if let Some(caps) = RE_EDGE_CROSS_WITH_LABEL.captures(remaining) {
                let from = caps[1].to_string();
                let label = decode_mermaid_label(caps[2].trim());
                let to = caps[3].to_string();
                graph.edges.push(Edge {
                    from,
                    to,
                    label: Some(label),
                    is_back_edge: false,
                    kind: EdgeKind::CrossEnd,
                });
                start_pos += caps.get(3).unwrap().start();
                continue;
            }
            if let Some(caps) = RE_EDGE_CROSS.captures(remaining) {
                let from = caps[1].to_string();
                let to = caps[2].to_string();
                graph.edges.push(Edge {
                    from,
                    to,
                    label: None,
                    is_back_edge: false,
                    kind: EdgeKind::CrossEnd,
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

        let mut node = Node::with_shape(id.clone(), label, shape);
        node.click_target = click_targets.get(id).cloned();
        graph.nodes.push(node);
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
        "max_edge_label" | "maxedgelabel" => {
            if let Ok(n) = value.parse::<usize>() {
                config.max_edge_label = Some(n);
            }
        }
        "wrap" | "wrap_labels" => {
            if let Some(b) = parse_bool(value) {
                config.wrap_labels = Some(b);
            }
        }
        "max_lines" | "max_label_lines" => {
            if let Ok(n) = value.parse::<usize>() {
                config.max_label_lines = Some(n);
            }
        }
        "spacing" | "spacing_mode" => {
            if let Ok(mode) = value.parse::<SpacingMode>() {
                config.spacing_mode = Some(mode);
            }
        }
        "optimize_render" | "optimize" => {
            if let Some(b) = parse_bool(value) {
                config.optimize_render = Some(b);
            }
        }
        "render_repair_passes" | "repair_passes" => {
            if let Ok(n) = value.parse::<usize>() {
                config.render_repair_passes = Some(n.max(1));
            }
        }
        "layout_repair_passes" | "layout_passes" => {
            if let Ok(n) = value.parse::<usize>() {
                config.layout_repair_passes = Some(n.max(1));
            }
        }
        "debug_critic" | "critic_debug" => {
            if let Some(b) = parse_bool(value) {
                config.debug_critic = Some(b);
            }
        }
        _ => {} // Ignore unknown config keys
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
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

    // Note: Subgraphs are now supported - handled in main parse loop

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
    let is_known_edge = RE_EDGE.is_match(line)
        || RE_EDGE_WITH_LABEL.is_match(line)
        || RE_EDGE_TEXT_LABEL.is_match(line)
        || RE_EDGE_OPEN.is_match(line)
        || RE_EDGE_OPEN_WITH_LABEL.is_match(line)
        || RE_EDGE_THICK.is_match(line)
        || RE_EDGE_THICK_WITH_LABEL.is_match(line)
        || RE_EDGE_DOTTED.is_match(line)
        || RE_EDGE_DOTTED_WITH_LABEL.is_match(line)
        || RE_EDGE_BIDIR.is_match(line)
        || RE_EDGE_BIDIR_WITH_LABEL.is_match(line)
        || RE_EDGE_CIRCLE.is_match(line)
        || RE_EDGE_CIRCLE_WITH_LABEL.is_match(line)
        || RE_EDGE_CROSS.is_match(line)
        || RE_EDGE_CROSS_WITH_LABEL.is_match(line);

    // Node-like brackets/braces/parens but not a valid node pattern
    let has_node_delimiter =
        line.contains('[') || line.contains('{') || line.contains('(') || line.contains('>');
    let matches_any_shape = RE_NODE_DB.is_match(line)
        || RE_NODE_SUBROUTINE.is_match(line)
        || RE_NODE_STADIUM.is_match(line)
        || RE_NODE_DOUBLE_CIRCLE.is_match(line)
        || RE_NODE_CIRCLE.is_match(line)
        || RE_NODE_HEXAGON.is_match(line)
        || RE_NODE_DIAMOND.is_match(line)
        || RE_NODE_ROUNDED.is_match(line)
        || RE_NODE_ASYMMETRIC.is_match(line)
        || RE_NODE_PARALLELOGRAM.is_match(line)
        || RE_NODE_PARALLELOGRAM_ALT.is_match(line)
        || RE_NODE_TRAPEZOID.is_match(line)
        || RE_NODE_TRAPEZOID_ALT.is_match(line)
        || RE_NODE.is_match(line);

    // Arrow indicator present but matches no supported edge syntax → malformed edge.
    // Skip this check when the line also contains a valid shape (e.g. `A(((x))) --> B`);
    // those are handled by the Pass 2 chain loop even if the shape suffix defeats RE_EDGE.
    if (line.contains("-->") || line.contains("==>") || line.contains("-.->"))
        && !is_known_edge
        && !matches_any_shape
    {
        return Some(format!(
            "termiflow: warning: line {}: Malformed edge '{}'",
            line_num, line
        ));
    }

    if has_node_delimiter && !matches_any_shape && !is_known_edge {
        return Some(format!(
            "termiflow: warning: line {}: Malformed node '{}'",
            line_num, line
        ));
    }

    None
}

#[cfg(test)]
mod tests;
