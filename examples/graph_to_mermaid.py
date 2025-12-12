#!/usr/bin/env python3

import json
import re
import sys
from typing import Any, Dict, List, Optional, Tuple


def _read_json(path: Optional[str]) -> Any:
    if path is None or path == "-":
        return json.load(sys.stdin)
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def _safe_id(raw: str) -> str:
    raw = raw.strip()
    if not raw:
        return "node"
    cooked = re.sub(r"[^A-Za-z0-9_]", "_", raw)
    cooked = re.sub(r"_+", "_", cooked).strip("_")
    return cooked or "node"


def _fmt_node(node_id: str, label: Optional[str]) -> str:
    if label is None or label == node_id:
        return f"  {node_id}"
    escaped = label.replace('"', '\\"')
    return f'  {node_id}["{escaped}"]'


def _fmt_edge(src: str, dst: str, label: Optional[str]) -> str:
    if label:
        lbl = label.strip()
        return f"  {src} -->|{lbl}| {dst}"
    return f"  {src} --> {dst}"


def _partition_edges_by_subgraph(
    edges: List[Dict[str, Any]], node_to_sg: Dict[str, str]
) -> Tuple[List[Dict[str, Any]], Dict[str, List[Dict[str, Any]]]]:
    global_edges: List[Dict[str, Any]] = []
    local_edges: Dict[str, List[Dict[str, Any]]] = {}
    for e in edges:
        src = e.get("from")
        dst = e.get("to")
        if not isinstance(src, str) or not isinstance(dst, str):
            continue
        s1 = node_to_sg.get(src)
        s2 = node_to_sg.get(dst)
        if s1 and s1 == s2:
            local_edges.setdefault(s1, []).append(e)
        else:
            global_edges.append(e)
    return global_edges, local_edges


def main() -> int:
    data = _read_json(sys.argv[1] if len(sys.argv) > 1 else None)
    if not isinstance(data, dict):
        print("expected a JSON object", file=sys.stderr)
        return 2

    direction = data.get("direction", "TD")
    if not isinstance(direction, str) or direction not in {"TD", "LR", "RL", "TB", "BT"}:
        direction = "TD"

    raw_nodes = data.get("nodes", [])
    raw_edges = data.get("edges", [])
    raw_subgraphs = data.get("subgraphs", [])

    nodes: List[Dict[str, Any]] = raw_nodes if isinstance(raw_nodes, list) else []
    edges: List[Dict[str, Any]] = raw_edges if isinstance(raw_edges, list) else []
    subgraphs: List[Dict[str, Any]] = raw_subgraphs if isinstance(raw_subgraphs, list) else []

    # Normalize node ids and build maps.
    id_map: Dict[str, str] = {}
    node_labels: Dict[str, Optional[str]] = {}

    for n in nodes:
        if not isinstance(n, dict):
            continue
        raw_id = n.get("id")
        if not isinstance(raw_id, str):
            continue
        nid = _safe_id(raw_id)
        id_map[raw_id] = nid
        node_labels[nid] = n.get("label") if isinstance(n.get("label"), str) else raw_id

    # Ensure edge endpoints exist as nodes (auto-create).
    for e in edges:
        if not isinstance(e, dict):
            continue
        for k in ("from", "to"):
            raw = e.get(k)
            if not isinstance(raw, str):
                continue
            nid = id_map.get(raw) or _safe_id(raw)
            id_map.setdefault(raw, nid)
            node_labels.setdefault(nid, raw)

    # Normalize subgraphs and membership.
    sg_nodes: Dict[str, List[str]] = {}
    sg_titles: Dict[str, Optional[str]] = {}
    node_to_sg: Dict[str, str] = {}
    for sg in subgraphs:
        if not isinstance(sg, dict):
            continue
        raw_id = sg.get("id")
        if not isinstance(raw_id, str):
            continue
        sg_id = _safe_id(raw_id)
        sg_titles[sg_id] = sg.get("title") if isinstance(sg.get("title"), str) else raw_id
        members = sg.get("nodes", [])
        if not isinstance(members, list):
            continue
        normalized: List[str] = []
        for raw in members:
            if not isinstance(raw, str):
                continue
            nid = id_map.get(raw) or _safe_id(raw)
            id_map.setdefault(raw, nid)
            node_labels.setdefault(nid, raw)
            normalized.append(nid)
            node_to_sg[nid] = sg_id
        sg_nodes[sg_id] = normalized

    # Normalize edges in-place.
    norm_edges: List[Dict[str, Any]] = []
    for e in edges:
        if not isinstance(e, dict):
            continue
        raw_from = e.get("from")
        raw_to = e.get("to")
        if not isinstance(raw_from, str) or not isinstance(raw_to, str):
            continue
        src = id_map.get(raw_from, _safe_id(raw_from))
        dst = id_map.get(raw_to, _safe_id(raw_to))
        lbl = e.get("label") if isinstance(e.get("label"), str) else None
        norm_edges.append({"from": src, "to": dst, "label": lbl})

    global_edges, local_edges = _partition_edges_by_subgraph(norm_edges, node_to_sg)

    print(f"flowchart {direction}")

    # Emit subgraphs first (with node declarations and internal edges).
    for sg_id in sorted(sg_nodes.keys()):
        title = sg_titles.get(sg_id)
        if title:
            print(f"  subgraph {sg_id} [{title}]")
        else:
            print(f"  subgraph {sg_id}")
        for nid in sg_nodes[sg_id]:
            print(_fmt_node(nid, node_labels.get(nid)))
        for e in local_edges.get(sg_id, []):
            print(_fmt_edge(e["from"], e["to"], e.get("label")))
        print("  end")

    # Emit remaining node declarations (outside subgraphs).
    emitted = set()
    for members in sg_nodes.values():
        emitted.update(members)
    for nid in sorted(node_labels.keys()):
        if nid in emitted:
            continue
        print(_fmt_node(nid, node_labels.get(nid)))

    # Emit cross-subgraph/global edges.
    for e in global_edges:
        print(_fmt_edge(e["from"], e["to"], e.get("label")))

    return 0


if __name__ == "__main__":
    raise SystemExit(main())

