# Pipelines (Generate Mermaid → `termiflow`)

TermiFlow (`termiflow`, recommended alias `tw`) renders Mermaid flowcharts from either files or stdin.

## Basic Pipe

```bash
cat <<'EOF' | tw
flowchart TD
  A[Source] --> B[TermiFlow]
EOF
```

## Demo: JSON Graph → Mermaid → TermiFlow

This is fully local (no external tools beyond Python 3).

```bash
python3 examples/graph_to_mermaid.py examples/inputs/microservices_graph.json | tw
```

## Terraform Plan → Mermaid → TermiFlow

If you have Terraform and `jq` installed:

```bash
terraform plan -out tfplan.bin
terraform show -json tfplan.bin \
  | jq -r -f examples/jq/tfplan_to_mermaid.jq \
  | tw
```

## Docker Compose → Mermaid → TermiFlow

If you have Docker Compose and `jq` installed:

```bash
docker compose config --format json \
  | jq -r -f examples/jq/compose_json_to_mermaid.jq \
  | tw
```

## npm Dependencies → Mermaid → TermiFlow

If you have `npm` and `jq` installed:

```bash
npm ls --all --json \
  | jq -r -f examples/jq/npm_ls_to_mermaid.jq \
  | tw
```
