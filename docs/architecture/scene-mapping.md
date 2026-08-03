# Private scene pilot contract

`Scene` and `SceneRecorder` are crate-private compatibility seams. Canvas is
still the only production glyph authority; the recorder observes primitive
ordering, required fields, and ownership conflicts without changing output.

## Graph-to-scene field matrix

| Legacy source/helper | Scene fields | Ownership/lowering | Current status |
| --- | --- | --- | --- |
| precomputed route segment | `(x, y)`, glyph, edge owner, role, z | `edge_owned` → Canvas overlap resolver | observed for route markers; shaft lowering remains legacy |
| route turn/corner | endpoint coordinate, corner glyph, edge owner, role | Canvas route resolver | deferred; geometry remains route-owned |
| route endpoint/arrowhead | endpoint, arrow glyph, edge owner, `ArrowTip`, z | `owned` → Canvas protected endpoint | observed |
| bidirectional start marker | start coordinate, reverse arrow, edge owner | `owned` → Canvas | observed |
| source junction | junction coordinate, inferred edge intent, later explicit owner | `edge_inferred` then `owned` | observed |
| edge label | label cells, edge owner, text role | Canvas label helper; semantic restamp | deferred to legacy label placement |
| portal opening | boundary cell, subgraph owner, portal role | Canvas portal projection/restoration | deferred to legacy portal contract |
| cycle/back edge | ordered route segments, cycle owner, back glyph family | Canvas cycle router | deferred to legacy cycle router |
| bounded repair | affected cells, critic reason, repair count | Canvas repair + semantic refresh | deferred; compared by differential outcome |

Unresolved/deferred rows are explicit so a future scene consumer cannot mistake
an observed marker for full route, label, portal, cycle, or repair semantics.

## Layer and collision matrix

The total layer order is reservation → topology → semantic cells → glyph
projection → terminal transport. Within glyph projection, the private pilot
uses this collision policy:

| Existing \ incoming | Node/subgraph | Portal | Edge/cycle | Label | Junction | Arrowhead |
| --- | --- | --- | --- | --- | --- | --- |
| Node/subgraph | preserve owner; reject explicit same-layer owner | portal only at declared opening | Canvas may defer route around border | label placement must avoid node | junction only at port | arrowhead never overwrites |
| Portal | preserve protected opening | merge same owner | defer to Canvas portal/route rule | avoid textual cell | preserve portal metadata | preserve arrow |
| Edge/cycle | Canvas overlap resolver | portal stamp after topology | merge parallel/perpendicular glyphs | label helper owns text | junction merge | arrowhead protected |
| Label | label wins in its reserved cell | reject overlap | label placement rejects route collision | same owner is idempotent | reject | reject |
| Junction | junction role survives repair | preserve portal if declared | merge arms; owner is explicit only when safe | reject text overwrite | idempotent | arrowhead wins |
| Arrowhead | protected | protected | protected | protected | protected | same glyph idempotent |

Same-layer explicit owner conflicts are recorder rejections. Edge/cycle
overlaps are intentionally deferred to Canvas because that is the established
collision authority. Out-of-bounds primitives are recorded as rejected and
never count as successful scene publication.

## Differential evidence

The scene pilot compares, for the same immutable input:

- visible output bytes;
- raw semantic frame;
- cropped/padded display semantic frame;
- critic findings and score;
- warnings;
- render and layout repair counts.

Representative coverage includes ordinary flow, empty scene primitives,
clipped primitives, cycles, cross-subgraph routes, and malformed-route fallback.
The production renderer continues to use the legacy lowering for every row
marked deferred.
