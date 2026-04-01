# /maestro:debug - Complete Guide

**Purpose**: Provide an evidence-first debugging workflow for local, production,
and performance issues.

---

## Modes

### Local

Use for:

- reproducing failing tests
- local logs and traces
- CPU or memory profiling

### Prod

Use for:

- observability signals
- Prometheus, Grafana, Loki, or tracing data
- runtime-only failures

### Perf

Use for:

- hot path analysis
- benchmark regressions
- resource churn or latency issues

### Deep

Use when the issue spans crates or the cause is unclear.

Recommended deep streams:

- data-path tracing
- recent changes review
- error propagation analysis

---

## Debug Sequence

1. reproduce or capture the failure
2. gather logs, tests, traces, and metrics
3. isolate the likely boundary or transformation
4. verify the fix with the exact failing path
5. run broader checks to guard against regressions

---

## Report Shape

Include:

- symptom
- evidence gathered
- root cause
- fix applied or recommended
- verification performed
- residual risk
