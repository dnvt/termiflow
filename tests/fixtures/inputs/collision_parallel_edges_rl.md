graph RL
subgraph SG1 [Source]
    A[A1]
    B[A2]
    C[A3]
end
subgraph SG2 [Target]
    D[B1]
    E[B2]
    F[B3]
end
A --> D
B --> E
C --> F
