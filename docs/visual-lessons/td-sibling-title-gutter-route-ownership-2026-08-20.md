# Visual lesson: TD sibling title gutters must preserve owned route rails

Status: Promoted local rule; sibling-chain ownership remains a P2 visual watch

## Observation

The TD sibling-chain lane correctly moved to the right-side title gutter, but
the final title-redraw pass replaced the route at an interior padded cell.
The result was a one-cell visual gap even though the fallback plan and portal
attachment were correct. After preserving route-owned cells anywhere in the
non-visible title gutter, the rail remains continuous and the title text stays
unchanged. The full triple-sibling frame is still visually dense at repeated
`+-|`/title-gutter pierces, so connectivity is not treated as perceptual
approval.

## Hypothesis and fix

Title restoration must protect every route-owned padding cell outside the
visible title span, not only the first and last wrapper cell. The renderer now
shares the padded title-span contract between title redraw and the critic;
visible title characters remain protected while a topology-owned portal rail
may pass through its declared gutter.

## Falsifier and regression

The rule is falsified if any TD sibling homolog loses a rail at a padded title
cell, overwrites a visible title character, produces a geometry mismatch, or
recreates duplicated wall pipes in ASCII or Unicode. The focused regression is:

```text
cargo test --locked --all-features --test subgraph_boundary_arrows stacked_td_sibling_corridors_trace_every_cross_boundary_edge -- --nocapture
cargo test --locked --all-features --test render_options_api nested_subgraphs::render_with_feedback_gives_stacked_td_siblings_two_connector_rows -- --nocapture
```

The complete canonical and authored corpus packets must continue to review
all style/mode rows, with `collision_sibling_triple_td` and `subgraph_chain_td`
retained as P2 human-eye watches until boundary ownership is immediately
legible across their homologs.
