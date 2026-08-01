use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use serde_json::Value;

#[derive(Debug, Parser)]
#[command(
    name = "graph-to-mermaid",
    about = "Convert TermiFlow graph JSON to Mermaid flowchart text"
)]
struct Args {
    /// Optional graph JSON path; stdin is used when omitted.
    input: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let input = match args.input {
        Some(path) => fs::read_to_string(&path)
            .with_context(|| format!("read graph JSON {}", path.display()))?,
        None => {
            let mut input = String::new();
            io::stdin()
                .read_to_string(&mut input)
                .context("read graph JSON from stdin")?;
            input
        }
    };
    let data: Value = serde_json::from_str(&input).context("parse graph JSON")?;
    let direction = match data["direction"].as_str() {
        Some(value) if ["TD", "TB", "LR", "RL", "BT"].contains(&value) => value,
        _ => "TD",
    };
    let raw_nodes = data["nodes"].as_array().cloned().unwrap_or_default();
    let raw_edges = data["edges"].as_array().cloned().unwrap_or_default();
    let raw_subgraphs = data["subgraphs"].as_array().cloned().unwrap_or_default();

    let mut id_map = HashMap::new();
    let mut labels = BTreeMap::new();
    for node in &raw_nodes {
        let Some(raw_id) = node["id"].as_str() else {
            continue;
        };
        let id = safe_id(raw_id);
        id_map.insert(raw_id.to_owned(), id.clone());
        labels.insert(id, node["label"].as_str().unwrap_or(raw_id).to_owned());
    }
    for edge in &raw_edges {
        for key in ["from", "to"] {
            if let Some(raw) = edge[key].as_str() {
                let id = id_map
                    .entry(raw.to_owned())
                    .or_insert_with(|| safe_id(raw))
                    .clone();
                labels.entry(id).or_insert_with(|| raw.to_owned());
            }
        }
    }

    let mut subgraphs: BTreeMap<String, (Option<String>, Vec<String>)> = BTreeMap::new();
    let mut node_to_subgraph = HashMap::new();
    for subgraph in raw_subgraphs {
        let Some(raw_id) = subgraph["id"].as_str() else {
            continue;
        };
        let id = safe_id(raw_id);
        let title = subgraph["title"]
            .as_str()
            .map(ToOwned::to_owned)
            .or_else(|| Some(raw_id.to_owned()));
        let mut members = Vec::new();
        for raw in subgraph["nodes"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .filter_map(Value::as_str)
        {
            let node_id = id_map
                .entry(raw.to_owned())
                .or_insert_with(|| safe_id(raw))
                .clone();
            labels
                .entry(node_id.clone())
                .or_insert_with(|| raw.to_owned());
            members.push(node_id.clone());
            node_to_subgraph.insert(node_id, id.clone());
        }
        subgraphs.insert(id, (title, members));
    }

    let mut edges = Vec::new();
    for edge in raw_edges {
        let (Some(raw_from), Some(raw_to)) = (edge["from"].as_str(), edge["to"].as_str()) else {
            continue;
        };
        let from = id_map
            .get(raw_from)
            .cloned()
            .unwrap_or_else(|| safe_id(raw_from));
        let to = id_map
            .get(raw_to)
            .cloned()
            .unwrap_or_else(|| safe_id(raw_to));
        edges.push((from, to, edge["label"].as_str().map(ToOwned::to_owned)));
    }

    let mut output = vec![format!("flowchart {direction}")];
    let mut emitted = std::collections::BTreeSet::new();
    for (subgraph, (title, members)) in &subgraphs {
        match title.as_deref() {
            Some(title) if !title.is_empty() => {
                output.push(format!("  subgraph {subgraph} [{title}]"))
            }
            _ => output.push(format!("  subgraph {subgraph}")),
        }
        for node in members {
            output.push(format_node(node, labels.get(node).map(String::as_str)));
            emitted.insert(node.clone());
        }
        for (from, to, label) in edges.iter().filter(|(from, to, _)| {
            node_to_subgraph.get(from) == Some(subgraph)
                && node_to_subgraph.get(to) == Some(subgraph)
        }) {
            output.push(format_edge(from, to, label.as_deref()));
        }
        output.push("  end".to_owned());
    }
    for (id, label) in &labels {
        if !emitted.contains(id) {
            output.push(format_node(id, Some(label)));
        }
    }
    for (from, to, label) in edges
        .iter()
        .filter(|(from, to, _)| node_to_subgraph.get(from) != node_to_subgraph.get(to))
    {
        output.push(format_edge(from, to, label.as_deref()));
    }
    println!("{}", output.join("\n"));
    Ok(())
}

fn safe_id(raw: &str) -> String {
    let mut output = String::new();
    let mut previous_separator = false;
    for character in raw.trim().chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            output.push(character);
            previous_separator = false;
        } else if !previous_separator {
            output.push('_');
            previous_separator = true;
        }
    }
    while output.ends_with('_') {
        output.pop();
    }
    if output.is_empty() {
        "node".to_owned()
    } else {
        output
    }
}

fn format_node(id: &str, label: Option<&str>) -> String {
    match label.filter(|label| *label != id) {
        Some(label) => format!("  {id}[\"{}\"]", label.replace('"', "\\\"")),
        None => format!("  {id}"),
    }
}

fn format_edge(from: &str, to: &str, label: Option<&str>) -> String {
    match label.filter(|label| !label.trim().is_empty()) {
        Some(label) => format!("  {from} -->|{label}| {to}"),
        None => format!("  {from} --> {to}"),
    }
}
