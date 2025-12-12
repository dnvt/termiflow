# Docker Compose JSON (from `docker compose config --format json`) → Mermaid flowchart.
#
# Usage:
#   docker compose config --format json | jq -r -f examples/jq/compose_json_to_mermaid.jq

def safe_id:
  gsub("[^A-Za-z0-9_]"; "_")
  | gsub("_+"; "_")
  | sub("^_"; "")
  | sub("_$"; "")
  | (if . == "" then "svc" else . end);

def svc_id($name): ("svc_" + ($name | safe_id));

("flowchart TD"),
(
  (.services // {})
  | keys
  | sort
  | map("  " + svc_id(.) + "[\"" + . + "\"]")
  | .[]
),
(
  (.services // {})
  | to_entries
  | map(
      .key as $svc
      | (.value.depends_on // [])
      | (if type == "array" then . else (keys) end)
      | map(select(type=="string"))
      | map("  " + svc_id(.) + " --> " + svc_id($svc))
    )
  | .[]?
  | .[]?
)

