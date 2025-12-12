# npm dependency tree JSON (from `npm ls --all --json`) → Mermaid flowchart.
#
# Usage:
#   npm ls --all --json | jq -r -f examples/jq/npm_ls_to_mermaid.jq

def safe_id:
  gsub("[^A-Za-z0-9_]"; "_")
  | gsub("_+"; "_")
  | sub("^_"; "")
  | sub("_$"; "")
  | (if . == "" then "pkg" else . end);

def pkg_id($name): ("pkg_" + ($name | safe_id));

def edges($parent; $deps):
  ($deps // {})
  | to_entries
  | map(
      .key as $child
      | ("  " + pkg_id($parent) + " --> " + pkg_id($child)),
        (edges($child; .value.dependencies))
    )
  | .[];

("flowchart LR"),
(
  .name? as $root
  | (if ($root|type)=="string" and $root != "" then
      ("  " + pkg_id($root) + "[\"" + $root + "\"]"),
      (edges($root; .dependencies))
    else
      empty
    end)
)

