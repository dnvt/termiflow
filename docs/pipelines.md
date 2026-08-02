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

This is fully local and uses the Rust binaries shipped by this repository.

```bash
cargo run --locked --quiet --bin graph-to-mermaid -- examples/inputs/microservices_graph.json \
  | tw --wrap --max-lines 3
```

## Cargo Workspace → Mermaid → TermiFlow

Fully local (requires Rust + Cargo).

```bash
cargo metadata --locked --format-version 1 \
  | cargo run --locked --quiet --bin cargo-metadata-to-graph -- --direction LR \
  | cargo run --locked --quiet --bin graph-to-mermaid \
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
