#!/usr/bin/env python3

"""
Cargo metadata JSON -> TermiFlow "graph JSON" (for examples/graph_to_mermaid.py).

Usage:
  cargo metadata --format-version 1 | python3 examples/cargo_metadata_to_graph.py | python3 examples/graph_to_mermaid.py | tw

Notes:
  - Emits only workspace-member crates by default.
  - Edges are "depends on": crate --> dependency.
"""

from __future__ import annotations

import argparse
import json
import sys
from typing import Any, Dict, List, Optional, Set, Tuple


def read_json() -> Any:
    return json.load(sys.stdin)


def main() -> int:
    ap = argparse.ArgumentParser(add_help=True)
    ap.add_argument(
        "--direction",
        default="LR",
        choices=["TD", "TB", "LR", "RL", "BT"],
        help="Mermaid flowchart direction (default: LR)",
    )
    ap.add_argument(
        "--no-subgraph",
        action="store_true",
        help="Do not wrap workspace members in a single subgraph",
    )
    args = ap.parse_args()

    data = read_json()
    if not isinstance(data, dict):
        print("expected cargo metadata JSON object", file=sys.stderr)
        return 2

    workspace_members: Set[str] = set()
    for mid in data.get("workspace_members", []) or []:
        if isinstance(mid, str):
            workspace_members.add(mid)

    packages: Dict[str, Dict[str, Any]] = {}
    for pkg in data.get("packages", []) or []:
        if isinstance(pkg, dict) and isinstance(pkg.get("id"), str):
            packages[pkg["id"]] = pkg

    def pkg_name(pid: str) -> str:
        pkg = packages.get(pid)
        if pkg and isinstance(pkg.get("name"), str):
            return pkg["name"]
        return pid

    # Nodes: workspace members only.
    node_ids: List[str] = []
    nodes: List[Dict[str, str]] = []
    for pid in sorted(workspace_members):
        name = pkg_name(pid)
        label = name
        pkg = packages.get(pid)
        if pkg and isinstance(pkg.get("version"), str):
            label = f"{name} {pkg['version']}"
        node_ids.append(name)
        nodes.append({"id": name, "label": label})

    ws_names: Set[str] = set(node_ids)

    # Edges: prefer `resolve.nodes` if present (fully resolved).
    edges: Set[Tuple[str, str]] = set()
    resolve = data.get("resolve")
    if isinstance(resolve, dict) and isinstance(resolve.get("nodes"), list):
        for n in resolve["nodes"]:
            if not isinstance(n, dict):
                continue
            pid = n.get("id")
            if not isinstance(pid, str) or pid not in workspace_members:
                continue
            src = pkg_name(pid)
            deps = n.get("dependencies", []) or []
            if not isinstance(deps, list):
                continue
            for dep_id in deps:
                if not isinstance(dep_id, str) or dep_id not in workspace_members:
                    continue
                dst = pkg_name(dep_id)
                if src in ws_names and dst in ws_names and src != dst:
                    edges.add((src, dst))
    else:
        # Fallback: declared dependencies from package manifests.
        for pid in workspace_members:
            pkg = packages.get(pid) or {}
            src = pkg_name(pid)
            for dep in pkg.get("dependencies", []) or []:
                if not isinstance(dep, dict):
                    continue
                dep_name = dep.get("name")
                if isinstance(dep_name, str) and dep_name in ws_names and dep_name != src:
                    edges.add((src, dep_name))

    out: Dict[str, Any] = {
        "direction": args.direction,
        "nodes": nodes,
        "edges": [{"from": a, "to": b} for (a, b) in sorted(edges)],
    }
    if not args.no_subgraph:
        out["subgraphs"] = [
            {"id": "workspace", "title": "Workspace", "nodes": sorted(node_ids)}
        ]

    json.dump(out, sys.stdout, indent=2)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

