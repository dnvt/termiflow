graph LR
subgraph G1 [Group 1]
    A[A1] --> A2[A2]
end
subgraph G2 [Group 2]
    B[B1] --> B2[B2]
end
subgraph G3 [Group 3]
    C[C1] --> C2[C2]
end
A2 --> B
B2 --> C
