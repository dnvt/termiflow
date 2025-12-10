# Areas for Improvement / Future Work

## Completed ✅

* **Subgraphs**: Single-level grouping with edge-aware borders (enabled by default)
* **Edge Labels**: Pipe syntax `-->|label|` and text syntax `-- label -->`
* **Node Shapes**: 9 shapes including rectangle, rounded, diamond, circle, stadium, hexagon, database, subroutine, asymmetric
* **Debugging Tools**: `--debug-layout` flag implemented

## Remaining Work

* **TUI Implementation**: The main feature of the project, the interactive TUI, is not yet implemented. This will be the next major step to realize the project's full vision.
* **Sequence Diagrams**: New diagram type requiring architecture refactor (see ROADMAP.md H3)
* **Nested Subgraphs**: Currently single-level only; nested support would require significant refactoring
* **LR/RL Orientation Polish**: Horizontal layouts need edge routing improvements (see ROADMAP.md H5)
* **Mermaid Styling**: `classDef` and `:::` class syntax not yet supported
