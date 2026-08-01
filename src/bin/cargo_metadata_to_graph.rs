use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};

use anyhow::{Context, Result};
use clap::Parser;
use serde_json::{json, Value};

#[derive(Debug, Parser)]
#[command(
    name = "cargo-metadata-to-graph",
    about = "Convert Cargo metadata JSON to TermiFlow graph JSON"
)]
struct Args {
    /// Mermaid flowchart direction.
    #[arg(long, default_value = "LR", value_parser = ["TD", "TB", "LR", "RL", "BT"])]
    direction: String,
    /// Omit the workspace subgraph wrapper.
    #[arg(long)]
    no_subgraph: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .context("read Cargo metadata from stdin")?;
    let data: Value = serde_json::from_str(&input).context("parse Cargo metadata JSON")?;
    let object = data
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("expected Cargo metadata JSON object"))?;

    let workspace_members: BTreeSet<String> = object["workspace_members"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect();
    let mut packages = BTreeMap::new();
    for package in object["packages"].as_array().unwrap_or(&Vec::new()) {
        if let (Some(id), Some(package)) = (package["id"].as_str(), package.as_object()) {
            packages.insert(id.to_owned(), package.clone());
        }
    }
    let package_name = |id: &str| -> String {
        packages
            .get(id)
            .and_then(|package| package.get("name"))
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_owned()
    };

    let mut node_ids = Vec::new();
    let mut nodes = Vec::new();
    for id in &workspace_members {
        let name = package_name(id);
        let version = packages
            .get(id)
            .and_then(|package| package.get("version"))
            .and_then(Value::as_str);
        let label = version
            .map(|version| format!("{name} {version}"))
            .unwrap_or_else(|| name.clone());
        node_ids.push(name.clone());
        nodes.push(json!({"id": name, "label": label}));
    }
    let workspace_names: BTreeSet<String> = node_ids.iter().cloned().collect();
    let mut edges = BTreeSet::new();
    if let Some(resolve) = object.get("resolve").and_then(Value::as_object) {
        if let Some(resolve_nodes) = resolve.get("nodes").and_then(Value::as_array) {
            for node in resolve_nodes {
                let Some(id) = node["id"].as_str() else {
                    continue;
                };
                if !workspace_members.contains(id) {
                    continue;
                }
                let source = package_name(id);
                for dependency in node["dependencies"]
                    .as_array()
                    .unwrap_or(&Vec::new())
                    .iter()
                    .filter_map(Value::as_str)
                {
                    if workspace_members.contains(dependency) {
                        let target = package_name(dependency);
                        if source != target
                            && workspace_names.contains(&source)
                            && workspace_names.contains(&target)
                        {
                            edges.insert((source.clone(), target));
                        }
                    }
                }
            }
        }
    } else {
        for id in &workspace_members {
            let source = package_name(id);
            for dependency in packages
                .get(id)
                .and_then(|package| package.get("dependencies"))
                .and_then(Value::as_array)
                .unwrap_or(&Vec::new())
            {
                let name = dependency["name"].as_str().unwrap_or_default();
                if workspace_names.contains(name) && source != name {
                    edges.insert((source.clone(), name.to_owned()));
                }
            }
        }
    }
    let edge_values: Vec<Value> = edges
        .into_iter()
        .map(|(from, to)| json!({"from": from, "to": to}))
        .collect();
    let mut output = json!({"direction": args.direction, "nodes": nodes, "edges": edge_values});
    if !args.no_subgraph {
        output["subgraphs"] = json!([{"id": "workspace", "title": "Workspace", "nodes": node_ids}]);
    }
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
