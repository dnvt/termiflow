graph BT
    A1[A1] --> M1[Merge 1]
    A2[A2] --> M1
    B1[B1] --> M2[Merge 2]
    B2[B2] --> M2
    M1 --> F[Final]
    M2 --> F
