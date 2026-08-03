# TermiFlow behavior and API contract ledger

Status: A0 baseline, observed at `fc4e888c72349c3caf4ba55008e8a51c81dbdaab`

This ledger freezes the behavior that the modular-renderer work must preserve
unless a later slice records a deliberate, reviewed rendering change. It is a
repository-owned contract for the architecture audit; the dated baseline
manifest beside it binds the observations to source, toolchain, inputs, and
packet digests.

## Invariants for the larger program

The quality program is a closed learning loop, not a one-off snapshot refresh:

```text
canonical Mermaid schema/source
  -> deterministic golden candidates
  -> immutable one-frame AI/human-eye inspection
  -> human-eye ledger of every visible flaw, including one-cell/one-row and tiny-detail issues
  -> falsifiable owner-layer hypothesis and falsifier
  -> smallest regression fixture or independent oracle
  -> localized renderer/flow fix
  -> direction/style/mode homolog and holdout verification
  -> fresh hash-bound review and explicit golden approval
  -> lesson promotion into code, fixtures, oracles, taxonomy, or reusable skill/script rules
```

Every future review record must identify the source schema, generated candidate,
frame hash, exact coordinates or region, visual dimension, severity, affected
homologs, hypothesis, expected observation, falsifier, targeted command, owner
layer, and negative result. Machine structural pre-screening is evidence for
triage only; it never substitutes for sequential one-frame human-eye review.

The maintenance contract is also absolute-latest: at every dated refresh,
update Rust and every direct, development, build, target/feature, and reachable
transitive dependency to the newest published releases, including majors.
Refresh `Cargo.toml`/`Cargo.lock`, adapt code and benchmarks, and rerun MSRV,
CI, security, package, publish, documentation, and visual gates. Any entry that
cannot move must have a dated, evidence-backed owner/reason/revisit record; a
compatible range or a previous observation is not a waiver.

## Private QA state and identity transitions

The private persistence contract is deliberately stricter than the
backward-readable public packet shape. `run_spec.v1` owns the requested work;
`run_identity.v2` owns the final policy-bound identity; and `run_state.v2` owns
recovery evidence. A pending policy is represented by `policy_pending: true`,
never by a zero digest or a provisional final run ID.

The effective policy record is intentionally `termiflow.effective_policy.v1`:
this is the first persisted policy contract and no earlier packet carried a
comparable effective-policy schema. Its strict nested shape, canonical digest,
unknown-field rejection, and legacy `legacy-uncomparable` handling are the
version-one contract; a future incompatible policy-shape change must increment
this schema rather than silently widening it.

| State | Required evidence | Allowed next states | Final-path rule |
| --- | --- | --- | --- |
| `planned` | valid run spec, owner, requested final | `claimed`, `failed`, `recovery-required` | final absent |
| `claimed` | private stage and owner claim | `writing`, `failed`, `recovery-required` | final absent |
| `writing` | pending or final policy context, private stage | `ready`, `failed`, `recovery-required` | final absent |
| `ready` | complete marker, manifest, policy set, packet digest, final run identity | `published`, `failed`, `recovery-required` | final absent |
| `published` | complete final packet and matching published state | `published` (equal replay only) | final complete |
| `failed` | transition reason and preserved owner/stage evidence | `recovery-required` | final absent unless claim already recorded |
| `recovery-required` | reason, owner, stage/final claim and repair route | `published` only through repair, or manual action | never republish an already-claimed final |

Every state carries `run_spec_id`, owner `pid`/`host`/process-start token,
creation and last-transition timestamps, transition reason, intended final,
private stage, candidate packet digest, `policy_pending` or policy-set digest,
and publication-guard identity. A successful directory claim is irreversible;
post-claim state or guard failures preserve the final and route through
`repair_published_state(final)`.

## Packet and receipt identity ownership

| Artifact | Owns | Retry/reconciliation |
| --- | --- | --- |
| `run_spec.json` | requested role, source/workload, final path, policy context | discoverable before policy collection; never a success signal |
| `identity.json` | final `run_id`, `run_spec_id`, source/workload, policy-set digest | must match every manifest row and run state |
| `COMPLETE.json`/`PACKET.sha256` | complete packet and deterministic packet digest | final publication requires both; bytes are immutable after claim |
| holdout receipt | queue/spec, final run identity, policy digest, packet/manifest/complete digests | absent receipt may be reconstructed only from a complete matching packet; equal bytes replay, different/malformed bytes conflict or require recovery |

The exact field inventories and target capability claims are machine-readable in
`docs/architecture/effective-policy-matrix.json` and
`docs/architecture/persistence-capability-matrix.json`; the CI contract check
must fail if either inventory drifts from code or the subprocess interruption
test inventory.

## Source and public-surface inventory

The public facade is `src/lib.rs`. Public module visibility is itself part of
the compatibility surface until downstream usage is measured:

| Surface | Current contract | A0 evidence |
| --- | --- | --- |
| High-level rendering | `render`, `render_with_feedback`, `render_json`, `layout_and_render_with_feedback` preserve the current `Result` and output shape | `tests/api_contract.rs` |
| Options/configuration | `RenderOptions`, `Config`, `ConfigBuilder`, `ParseConfig` remain constructible and field-compatible | `tests/api_contract.rs`, `src/config.rs` |
| Parsing | Mermaid flowcharts and the lightweight JSON graph schema; parse warnings remain observable and strict mode can turn them into errors | `src/parser.rs`, `src/json_input.rs`, `docs/reference.md` |
| Semantic graph | `Graph`, `Node`, `Edge`, `EdgeKind`, subgraphs, directions, and public graph helpers remain available | `src/graph.rs` |
| Layout/measurement | Public coarse layout, geometry, scaling, spacing, orientation, crossing, and measure modules remain available; derived-state ownership is not narrowed in A0 | `src/lib.rs` |
| Rendering contract | Canvas, layer contract, traces, critic reports, semantic/topology helpers, and render entry points remain available | `src/render/mod.rs`, `src/render/contract.rs` |
| Terminal/TUI | Frame, diff, presenter, live/watch support remains available with current terminal caveats | `src/tui/`, `README.md`, `docs/reference.md` |
| CLI binaries | `termiflow` and `tw` share the CLI implementation; print, JSON, watch, TUI, audit, strict, and repair flags remain behaviorally compatible | `src/bin/common/mod.rs`, `docs/reference.md` |
| Package shape | Current published package file list and release identity remain stable unless a release decision says otherwise | `cargo package --locked --list`, release preflight |

`pub(crate)` modules (`indexed_graph`, `layout_snapshot`, and `route_plan`) are
internal compatibility seams, not public downstream commitments. Private
modules and implementation helpers may move only after their callers and
derived-state ownership are covered by the staged roadmap.

## Input and configuration contract

The effective configuration order is:

```text
defaults < platform config file < in-file %% termiflow: directives < CLI/API overrides
```

`Config::from_parse_config` owns the file/directive merge and
`ConfigBuilder::build` applies the highest-priority API/CLI overrides. The
following fields are covered by the precedence rule: base/composite style,
label and edge-label width, wrapping and maximum lines, crop, pad, strictness,
spacing, render optimization, render repair passes, layout repair passes, and
critic output. If a field is not supported by a lower tier, the ledger must say
so rather than implying that all tiers are symmetrical.

Config files are read from the platform-specific application configuration
location documented in `docs/reference.md`. Tests must not depend on a user
machine's config file; isolated tests use explicit overrides or a temporary
configuration boundary.

Accepted source modes are:

- Mermaid `graph`/`flowchart` flowcharts with directions `TD`, `TB`, `LR`,
  `RL`, and `BT`.
- TermiFlow's lightweight JSON graph schema through `--from-json` and
  `parse_json_graph`.
- stdin, file input, normal print output, primary-screen `--watch`, partial
  alternate-screen `--tui`, and maintainer visual-audit output.

The renderer is intentionally flowchart-only. It does not claim sequence,
state, class, ER, Gantt, or other Mermaid diagram types.

## Parser diagnostics and unsupported syntax

The parser may preserve a graph while emitting warnings for unsupported or
malformed constructs. Strict mode converts parse warnings into an error. Warning
text and source-line association are compatibility-sensitive until replaced by
structured diagnostics with an equivalence suite.

Currently documented gaps include Mermaid `style`, `classDef`, and `:::`
classes (the suffix is stripped with a warning and no style is applied), edge
IDs, `@{}` shape syntax, Markdown-aware labels/strings, and non-flowchart
diagram types. `click ID "file.md"` is parsed as an informational target; it
does not create a navigation side effect. A bare `end` closes the active
subgraph.

## Environment compatibility inventory

Environment reads are currently distributed through library and CLI seams. A1
must either centralize them at a documented boundary or explicitly preserve
their library behavior before changing ownership.

| Variable | Current effect | Scope |
| --- | --- | --- |
| `TERMIFLOW_DISABLE_PORTALS` | disables subgraph-border portal carving | layout/render |
| `TERMIFLOW_DEBUG_TIMING` | emits timing/routing diagnostics | layout/render/CLI |
| `TERMIFLOW_DEBUG_ROUTES` | dumps precomputed route segments | CLI |
| `TERMIFLOW_DEBUG_CROSSING` | emits crossing-minimizer diagnostics | crossing |
| `TERMIFLOW_DEBUG_CRITIC` | enables critic findings without the CLI flag | render/CLI |
| `TERMIFLOW_OPTIMIZE_RENDER` | enables render optimization without the CLI flag | render |
| `TERMIFLOW_RENDER_REPAIR_PASSES` | overrides the bounded render-repair budget | render |
| `TERMIFLOW_LAYOUT_REPAIR_PASSES` | overrides the layout-repair budget | layout/render |
| `TERMIFLOW_RELEASE_BOUNDARY` | selects the release-boundary receipt for the release preflight script | release script |

Presence is currently the compatibility trigger for debug/boolean variables;
numeric variables are parsed at their consuming boundary. Invalid numeric
values retain the existing fallback behavior and must be covered before a
configuration resolver is extracted.

## QA, packet, and review persistence contract

The visual QA flow produces immutable, hash-bound packets outside the golden
directory. A packet is valid only when its manifest, evidence rows, identity,
summary, and completion marker agree; strict validation must pass against the
checked-in quality baseline. Refactor-only changes require exact frame and
quality-baseline equality. Intentional visual drift requires a separate
rendering-change decision and explicit golden approval.

The current workflow is a documented single-writer model for JSONL review
decisions. A single writer may append or publish one decision at a time; safe
retry and conflicting replay semantics are not yet a public concurrency
guarantee. A2 must either retain this explicit rule or add locking/transactional
publication before presenting concurrent writes as supported. Any future
decision identity must include source/packet identity, fixture, style, mode,
and review target so a conflicting replay fails closed.

## Architecture ownership facts before refactoring

The current render contract describes five ordered layers:

1. reservation: ranks, bounds, keepouts, and portal slots;
2. topology: route segments and boundary crossings;
3. semantic cells: cell ownership, role, and z-order;
4. glyph projection: visible ASCII/Unicode characters and bounded repair;
5. terminal transport: viewport slices, frame retention, and presenter diffs.

The current `Graph` carries both semantic input and derived geometry/routes.
That is a known migration target, not permission to remove fields in A0. Future
IR work must exclude coordinates, ranks, bounds, back-edge flags, and route maps
from immutable source semantics; the compatibility `Graph` remains the current
layout/render adapter until Mermaid/JSON equivalence is proven.

## Baseline and change protocol

The companion `baseline-manifest.json` records the clean source commit,
toolchain, manifest/lock digests, fixture inputs, quality baseline, and the
948-row visual packet. Reproduce it in a fresh process with:

```sh
scripts/visual_audit.sh \
  --out /tmp/termiflow-architecture-a0-visual \
  --styles ascii,unicode \
  --modes default,optimized
scripts/visual_validate.sh \
  --packet /tmp/termiflow-architecture-a0-visual \
  --baseline tests/fixtures/quality_baseline.json \
  --strict-quality
```

Before accepting any later architecture slice, record the changed source
commit, worktree state, toolchain/MSRV/target, package and dependency state,
full test gates, packet/manifest/identity hashes, and any exact output drift.
The A0 ledger is complete only when every changed public symbol or documented
behavior can be classified as preserved, intentionally changed with a decision,
or an explicitly unknown external-consumer risk.
