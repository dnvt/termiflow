# /render - Quick Diagram Rendering

Render a Mermaid diagram to verify changes or test syntax.

## Arguments
- `$ARGUMENTS` - Required: Either a file path OR inline Mermaid syntax

## Instructions

### If argument is a file path:
```bash
cargo run --bin tw -- "$ARGUMENTS"
```

### If argument is inline Mermaid:
```bash
echo '$ARGUMENTS' | cargo run --bin tw --
```

### Style Variations
After the default render, show at least one alternative style:
```bash
echo '[diagram]' | cargo run --bin tw -- --style ascii
```

### Direction Test
If the diagram uses a non-TD direction, also render with explicit direction flag to verify consistency.

### Report
Show the rendered output and note:
- Dimensions (approximate width × height in characters)
- Any warnings emitted to stderr
- Style used (default is unicode)

### Common Test Diagrams

**Simple flow:**
```
graph TD
A --> B --> C
```

**Branching:**
```
graph TD
A --> B
A --> C
B --> D
C --> D
```

**Subgraph:**
```
graph TD
subgraph Group
A --> B
end
C --> A
```

**All shapes:**
```
graph LR
A[Rect] --> B(Round) --> C{Diamond}
C --> D((Circle)) --> E([Stadium])
E --> F{{Hex}} --> G[(DB)]
G --> H[[Sub]] --> I>Asym]
```
