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
python3 examples/graph_to_mermaid.py examples/inputs/microservices_graph.json \
  | tw --wrap --max-lines 3
```

## Demo: JSON Graph → TermiFlow (No Mermaid)

If your source already emits TermiFlow's JSON graph schema, you can render directly:

```bash
cat examples/inputs/microservices_graph.json | tw --from-json --wrap --max-lines 3
```

## Cargo Workspace → Mermaid → TermiFlow

Fully local (requires Rust + Cargo).

```bash
cargo metadata --format-version 1 \
  | python3 examples/cargo_metadata_to_graph.py --direction LR \
  | python3 examples/graph_to_mermaid.py \
  | tw --wrap --max-lines 3
```

## Terraform Plan → Mermaid → TermiFlow

If you have Terraform and `jq` installed:

```bash
terraform plan -out tfplan.bin
terraform show -json tfplan.bin \
  | jq -r -f examples/jq/tfplan_to_mermaid.jq \
  | tw --wrap --max-lines 3
```

## Docker Compose → Mermaid → TermiFlow

If you have Docker Compose and `jq` installed:

```bash
docker compose config --format json \
  | jq -r -f examples/jq/compose_json_to_mermaid.jq \
  | tw --wrap --max-lines 3
```

## npm Dependencies → Mermaid → TermiFlow

If you have `npm` and `jq` installed:

```bash
npm ls --all --json \
  | jq -r -f examples/jq/npm_ls_to_mermaid.jq \
  | tw --wrap --max-lines 3
```
