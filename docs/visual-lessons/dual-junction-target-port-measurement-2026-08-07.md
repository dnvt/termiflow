# Visual lesson: dual-junction target ports must be measured with the route policy

Date: 2026-08-07
Hypothesis: an exact five-node, four-edge, two-in/two-out junction should expose one visible target-side arrow port per semantic edge in every direction, style, and optimization mode.

## Failure observed

The first H17 holdout implementation emitted four arrowheads according to the machine prescreen, but the second horizontal incoming edge in RL could land below a compact three-row target. To a human eye that read as a floating or detached arrow rather than a connected target-port route. This is a measurement/routing contract failure, not an arrow-count failure.

The important review rule is therefore:

> A route is not visually correct merely because its arrow glyph exists. Its target port must be inside the measured node boundary, connected to the source route, and legible as the declared relationship.

## Correction

The exact dual-junction structural policy now lives in `src/render/dual_junction.rs` and is shared by:

- horizontal and vertical route selection;
- target measurement and port capacity;
- independent raw-frame oracles;
- route-clarity findings.

The horizontal dedicated-fan-in measurement policy delegates to the same dual-junction predicate. The target grows to the minimum five-row body required for two separated side ports. The lower horizontal route now turns into the target-side port instead of stopping outside the box.

## Acceptance evidence

- Targeted holdout: 16/16 fresh perceptual decisions passed across TD, BT, LR, and RL × ASCII, Unicode, default, and optimized.
- Holdout manifest SHA-256: `b8b711a710f98a1d6841b4f4d0302c77071d1244ae8780a8692216d89bc67906`.
- Holdout decisions SHA-256: `d06c1a9c7fc1ff11e472377a9e614581c8d830ea3cfee3741c53cd86074f9a09`.
- Full corpus: all 237 existing input fixtures × four successful style/mode rows = 936/936 reviewed; 12 typed expected errors separately validated; 948/948 packet rows structurally validated.
- Full packet manifest SHA-256: `1e001f863f5d490661db915f61ea0140c7ce193f059a72aa9ba82919193f7a17`.
- Full visual decisions SHA-256: `65e54074e95a2ab98afe4b3f16dba2603be25a16a4e7a6ad932e30839eb3f04c`.
- Full expected-error ledger SHA-256: `3f993ba19be231667b2470b0f36fa81d15f75b2a902b932f2e881d2c7477ee95`.
- Full-source run identity SHA-256: `79d680817972acfb05403a7a113443d1a31917f1ce6c699142dd58bad9c83efe`.

The full visual decision distribution remains explicit: 555 pass and 381 watch. Existing watch decisions were carried forward only when their frame hash matched; changed junction frames received fresh decisions. No prior watch was silently converted or deleted.

## Future self-improvement check

For any new multi-edge lowerer, compare the measurement contract and route contract before reviewing glyph counts. Add a targeted holdout for every direction and both rendering modes, then rerun the complete 237/936/12/948 corpus after each source epoch. A future reviewer should specifically look for arrows that are present but appear detached, terminate on a border corner, share a target row, or require reconstructing a route through a node label.
