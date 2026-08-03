# Architecture facts

[`facts.json`](facts.json) is the portable source for maintained architecture
diagrams. It records ownership layers, public/private boundaries, data flow,
QA/release contracts, capabilities, diagnostics, and provenance.

Generated Mermaid files live in [`generated/`](generated/). Do not edit them by
hand; regenerate and validate them with:

```sh
scripts/check_architecture_facts.sh
scripts/architecture_facts_contract.sh
```

The checker rejects stale facts digests, missing source owners, machine-local
paths, unsupported silent degradation, and missing generated projections.
