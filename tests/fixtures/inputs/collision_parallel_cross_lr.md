graph LR
subgraph SG1 [Source]
    A[A1]
    B[A2]
end
subgraph SG2 [Target]
    C[B1]
    D[B2]
end
A --> D
B --> C
