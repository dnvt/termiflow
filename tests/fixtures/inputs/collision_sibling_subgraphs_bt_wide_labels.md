graph BT
subgraph Left [Left Network]
    A[Alpha Source] --> B[Beta Processor]
end
subgraph Right [Right Network]
    C[Gamma Processor] --> D[Delta Sink]
end
A --> C
B --> D
