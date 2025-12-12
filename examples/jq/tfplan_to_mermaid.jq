# Terraform plan JSON (from `terraform show -json`) → Mermaid flowchart.
#
# Usage:
#   terraform show -json tfplan.bin | jq -r -f examples/jq/tfplan_to_mermaid.jq
#
# Notes:
# - This is a dependency sketcher, not a full semantic diagrammer.
# - Addresses contain characters Mermaid IDs don't like; we generate stable ids.

def safe_id:
  gsub("[^A-Za-z0-9_]"; "_")
  | gsub("_+"; "_")
  | sub("^_"; "")
  | sub("_$"; "")
  | (if . == "" then "node" else . end);

def node_id($addr): ("n_" + ($addr | safe_id));

def node_decl($addr):
  "  " + node_id($addr) + "[\"" + ($addr|tostring) + "\"]";

def edge_decl($a; $b):
  "  " + node_id($a) + " --> " + node_id($b);

def try_deps:
  (.change.before.depends_on? // .change.after.depends_on? // []);

("flowchart LR"),
(
  (.resource_changes? // [])
  | map(select(.address?))
  | map(.address)
  | unique
  | map(node_decl(.))
  | .[]
),
(
  (.resource_changes? // [])
  | map(select(.address?))
  | map({addr: .address, deps: (try_deps | map(select(type=="string")))})
  | map(. as $r | ($r.deps[]? | edge_decl(. ; $r.addr)))
  | .[]
)

