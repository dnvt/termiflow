# TermiFlow Docs

Docs here stay short and user-focused. For plans/specs, see `../planning/`.

## Start Here
- **Project overview & usage:** [`../README.md`](../README.md)

## Quick Usage
```bash
# Render a diagram to stdout
termiflow --print diagram.md

# Choose a style (9 available)
termiflow --print --style rounded diagram.md

# Composite styling - mix and match
termiflow --print --style "corner:dots,border:heavy" diagram.md

# Strict mode (fail on warnings)
termiflow --print --strict diagram.md
```

## Available Styles
`ascii`, `unicode`, `double`, `rounded`, `heavy`, `dots`, `plus`, `stars`, `blocks`

## Breadcrumbs
- Planning & technical specs: `../planning/`
- Tests & fixtures: `../tests/`
- Source code: `../src/`
